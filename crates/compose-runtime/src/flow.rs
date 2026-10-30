//! Flows, as Compose's `snapshotFlow` and `collectAsState` and kotlinx's `StateFlow`
//! (docs/parity/design/snapshots.md §3.12). The stream trait is the ecosystem's,
//! `futures_core::Stream`, so other libraries' streams collect as state; compose-rs adds no
//! combinators but [`next`].

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

pub use futures_core::Stream;

use crate::compose_arg::ComposeArg;
use crate::effects::{produce_state, produce_state_with};
use crate::snapshots::{ChangeSet, ObserverHandle, Snapshot, StateObjectId};
use crate::state::{MutationPolicy, State};

/// The stream's next item, `None` when it's over.
pub fn next<S: Stream + Unpin>(stream: &mut S) -> Next<'_, S> {
    Next { stream }
}

/// The future of [`next`].
pub struct Next<'a, S> {
    stream: &'a mut S,
}

impl<S: Stream + Unpin> Future for Next<'_, S> {
    type Output = Option<S::Item>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<S::Item>> {
        Pin::new(&mut *self.stream).poll_next(context)
    }
}

/// What a flow's manager tells it: what it read changed.
#[derive(Default)]
struct Watcher {
    changed: Cell<bool>,
    waker: RefCell<Option<Waker>>,
    /// What its block read in its last run.
    reads: RefCell<HashSet<StateObjectId>>,
}

impl Watcher {
    fn signal(&self) {
        self.changed.set(true);
        let waker = self.waker.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

#[derive(Default)]
struct ManagerInner {
    /// Its apply observer, registered at the first flow's run, until it's disposed.
    registration: RefCell<Option<ObserverHandle>>,
    watchers: RefCell<HashMap<StateObjectId, Vec<Rc<Watcher>>>>,
}

impl ManagerInner {
    fn register(self: &Rc<ManagerInner>) {
        if self.registration.borrow().is_some() {
            return;
        }
        let inner: Weak<ManagerInner> = Rc::downgrade(self);
        let registration = Snapshot::register_apply_observer(move |changes: &ChangeSet, _| {
            if let Some(inner) = inner.upgrade() {
                inner.changed(changes);
            }
        });
        *self.registration.borrow_mut() = Some(registration);
    }

    /// Tells the flows watching what changed, outside the borrow: a waker is someone else's code.
    fn changed(&self, changes: &ChangeSet) {
        let mut told: Vec<Rc<Watcher>> = Vec::new();
        {
            let watchers = self.watchers.borrow();
            for state in changes.iter() {
                if let Some(watching) = watchers.get(&state) {
                    told.extend(watching.iter().cloned());
                }
            }
        }
        for watcher in told {
            watcher.signal();
        }
    }

    /// `watcher` now reads `reads`.
    fn watch(&self, watcher: &Rc<Watcher>, reads: HashSet<StateObjectId>) {
        let old = watcher.reads.replace(reads);
        let reads = watcher.reads.borrow();
        let mut watchers = self.watchers.borrow_mut();
        for state in old.difference(&reads) {
            remove_watcher(&mut watchers, *state, watcher);
        }
        for state in reads.difference(&old) {
            watchers.entry(*state).or_default().push(watcher.clone());
        }
    }

    fn unwatch(&self, watcher: &Rc<Watcher>) {
        let reads = watcher.reads.take();
        let mut watchers = self.watchers.borrow_mut();
        for state in reads {
            remove_watcher(&mut watchers, state, watcher);
        }
    }
}

fn remove_watcher(
    watchers: &mut HashMap<StateObjectId, Vec<Rc<Watcher>>>,
    state: StateObjectId,
    watcher: &Rc<Watcher>,
) {
    if let Some(watching) = watchers.get_mut(&state) {
        watching.retain(|other| !Rc::ptr_eq(other, watcher));
        if watching.is_empty() {
            watchers.remove(&state);
        }
    }
}

/// One apply observer shared by the flows made with it, as Compose's `SnapshotFlowManager`: many
/// flows, each watching few states of many, cost one lookup per state changed.
#[derive(Clone, Default)]
pub struct SnapshotFlowManager {
    inner: Rc<ManagerInner>,
}

impl SnapshotFlowManager {
    pub fn new() -> SnapshotFlowManager {
        SnapshotFlowManager::default()
    }

    /// Unregisters its apply observer, as `dispose()`: its flows hear nothing more. Dropping the
    /// last handle and the flows made with it does the same.
    pub fn dispose(&self) {
        let registration = self.inner.registration.borrow_mut().take();
        drop(registration);
    }

    /// How many states its flows watch. For tests.
    pub fn watched_states(&self) -> usize {
        self.inner.watchers.borrow().len()
    }
}

/// The stream of [`snapshot_flow`]. `!Send`, as state is.
pub struct SnapshotFlow<T> {
    block: Box<dyn Fn() -> T>,
    last: Option<T>,
    watcher: Rc<Watcher>,
    manager: Rc<ManagerInner>,
}

// No pinned projection: the fields move freely.
impl<T> Unpin for SnapshotFlow<T> {}

impl<T> SnapshotFlow<T> {
    /// Runs the block, watching what it reads.
    fn run(&mut self) -> T {
        self.manager.register();
        let reads: Rc<RefCell<HashSet<StateObjectId>>> = Rc::default();
        let log = reads.clone();
        // As Compose's: a read-only snapshot, so what the block reads is consistent, whatever it
        // runs (D775).
        let snapshot = Snapshot::take_snapshot(Some(Rc::new(move |state| {
            log.borrow_mut().insert(state);
        })));
        let value = snapshot.enter(&self.block);
        drop(snapshot);
        self.manager.watch(&self.watcher, reads.take());
        value
    }
}

impl<T: Clone + PartialEq> Stream for SnapshotFlow<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        let this = &mut *self;
        if this.last.is_none() {
            let value = this.run();
            this.last = Some(value.clone());
            return Poll::Ready(Some(value));
        }
        while this.watcher.changed.replace(false) {
            let value = this.run();
            if this.last.as_ref() != Some(&value) {
                this.last = Some(value.clone());
                return Poll::Ready(Some(value));
            }
        }
        *this.watcher.waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    }
}

impl<T> Drop for SnapshotFlow<T> {
    fn drop(&mut self) {
        self.manager.unwatch(&self.watcher);
    }
}

/// A stream of what `block` gives: its result at the first poll, then a different result each time
/// a change applied to what it read gives one, coalescing the changes between two polls, as
/// Compose's `snapshotFlow(block)`.
pub fn snapshot_flow<T: Clone + PartialEq + 'static>(
    block: impl Fn() -> T + 'static,
) -> SnapshotFlow<T> {
    snapshot_flow_with(&SnapshotFlowManager::new(), block)
}

/// Like [`snapshot_flow`], sharing `manager`'s apply observer with its other flows, as
/// `snapshotFlow(manager, block)`.
pub fn snapshot_flow_with<T: Clone + PartialEq + 'static>(
    manager: &SnapshotFlowManager,
    block: impl Fn() -> T + 'static,
) -> SnapshotFlow<T> {
    SnapshotFlow {
        block: Box::new(block),
        last: None,
        watcher: Rc::default(),
        manager: manager.inner.clone(),
    }
}

/// State following a stream, as Compose's `Flow.collectAsState(initial)`: `initial`, then each
/// item of the stream `stream` makes. Rust's streams have no identity to key on, so `key` says when
/// it's another one: `stream` runs again then (the state keeps its value until the new one gives
/// one), and collecting ends when this leaves the composition.
pub fn collect_as_state<K, S>(
    key: K,
    initial: S::Item,
    stream: impl FnOnce() -> S + 'static,
) -> State<S::Item>
where
    K: ComposeArg,
    S: Stream + 'static,
    S::Item: PartialEq + 'static,
{
    produce_state(initial, key, move |scope| async move {
        let mut stream = Box::pin(stream());
        while let Some(value) = next(&mut stream).await {
            scope.set(value);
        }
    })
}

/// Like [`collect_as_state`], with the state's mutation policy, as
/// `collectAsState(initial, mutationPolicy = …)`.
pub fn collect_as_state_with<K, S, P>(
    key: K,
    initial: S::Item,
    policy: P,
    stream: impl FnOnce() -> S + 'static,
) -> State<S::Item>
where
    K: ComposeArg,
    S: Stream + 'static,
    S::Item: 'static,
    P: MutationPolicy<S::Item>,
{
    produce_state_with(initial, key, policy, move |scope| async move {
        let mut stream = Box::pin(stream());
        while let Some(value) = next(&mut stream).await {
            scope.set(value);
        }
    })
}

struct FlowShared<T> {
    value: T,
    /// Bumped by each change: what a subscription compares.
    version: u64,
    wakers: Vec<Waker>,
}

/// A value that changes, and tells who collects it, as kotlinx's `MutableStateFlow`. A `Clone`
/// handle, `Send + Sync` for `T: Send`: background threads set it, and the UI thread's
/// [`collect_as_state`](StateFlow::collect_as_state) follows.
pub struct MutableStateFlow<T> {
    shared: Arc<Mutex<FlowShared<T>>>,
}

impl<T> Clone for MutableStateFlow<T> {
    fn clone(&self) -> Self {
        MutableStateFlow { shared: self.shared.clone() }
    }
}

/// The read-only view of a [`MutableStateFlow`], as kotlinx's `StateFlow`.
pub struct StateFlow<T> {
    shared: Arc<Mutex<FlowShared<T>>>,
}

impl<T> Clone for StateFlow<T> {
    fn clone(&self) -> Self {
        StateFlow { shared: self.shared.clone() }
    }
}

fn lock<T>(shared: &Mutex<FlowShared<T>>) -> std::sync::MutexGuard<'_, FlowShared<T>> {
    shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl<T: Clone + PartialEq + 'static> MutableStateFlow<T> {
    pub fn new(value: T) -> MutableStateFlow<T> {
        let shared = FlowShared { value, version: 0, wakers: Vec::new() };
        MutableStateFlow { shared: Arc::new(Mutex::new(shared)) }
    }

    /// The value now.
    pub fn value(&self) -> T {
        lock(&self.shared).value.clone()
    }

    /// Sets the value, as `value = …`: an equal one changes nothing, as a `StateFlow` compares.
    pub fn set_value(&self, value: T) {
        let shared = lock(&self.shared);
        if shared.value != value {
            self.change(shared, value);
        }
    }

    /// Sets the value if it's `expect`, as `compareAndSet`. Returns whether it was.
    pub fn compare_and_set(&self, expect: T, update: T) -> bool {
        let shared = lock(&self.shared);
        if shared.value != expect {
            return false;
        }
        if expect != update {
            self.change(shared, update);
        }
        true
    }

    fn change(&self, mut shared: std::sync::MutexGuard<'_, FlowShared<T>>, value: T) {
        let old = std::mem::replace(&mut shared.value, value);
        shared.version += 1;
        let wakers = std::mem::take(&mut shared.wakers);
        drop(shared);
        drop(old);
        for waker in wakers {
            waker.wake();
        }
    }

    /// Its read-only view.
    pub fn as_state_flow(&self) -> StateFlow<T> {
        StateFlow { shared: self.shared.clone() }
    }

    /// A stream of its value, as [`StateFlow::subscribe`].
    pub fn subscribe(&self) -> StateFlowSubscription<T> {
        self.as_state_flow().subscribe()
    }
}

impl<T: Clone + PartialEq + 'static> StateFlow<T> {
    /// The value now.
    pub fn value(&self) -> T {
        lock(&self.shared).value.clone()
    }

    /// A stream of its value: the value now, then the latest after each change, coalescing, as
    /// collecting a `StateFlow`.
    pub fn subscribe(&self) -> StateFlowSubscription<T> {
        StateFlowSubscription { shared: self.shared.clone(), seen: None }
    }

    /// State following it, as `StateFlow.collectAsState()`: its value now, then its changes.
    /// Another flow here starts following that one; the state keeps its value until then.
    pub fn collect_as_state(&self) -> State<T> {
        let flow = self.clone();
        collect_as_state(self.key(), self.value(), move || flow.subscribe())
    }

    /// Like [`collect_as_state`](Self::collect_as_state), with the state's mutation policy, as
    /// `collectAsState(mutationPolicy = …)`.
    pub fn collect_as_state_with<P: MutationPolicy<T>>(&self, policy: P) -> State<T> {
        let flow = self.clone();
        collect_as_state_with(self.key(), self.value(), policy, move || flow.subscribe())
    }

    /// Its identity, to key the collection on.
    fn key(&self) -> usize {
        Arc::as_ptr(&self.shared) as *const () as usize
    }
}

/// The stream of [`StateFlow::subscribe`].
pub struct StateFlowSubscription<T> {
    shared: Arc<Mutex<FlowShared<T>>>,
    seen: Option<u64>,
}

impl<T> Unpin for StateFlowSubscription<T> {}

impl<T: Clone> Stream for StateFlowSubscription<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        let this = &mut *self;
        let mut shared = lock(&this.shared);
        if this.seen == Some(shared.version) {
            let waker = context.waker();
            if !shared.wakers.iter().any(|other| other.will_wake(waker)) {
                shared.wakers.push(waker.clone());
            }
            return Poll::Pending;
        }
        this.seen = Some(shared.version);
        Poll::Ready(Some(shared.value.clone()))
    }
}
