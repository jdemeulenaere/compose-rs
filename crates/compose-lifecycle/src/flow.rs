//! Lifecycle-aware streams and state collection, as AndroidX's `Lifecycle.eventFlow`,
//! `Flow.flowWithLifecycle` (`lifecycle-runtime`) and `collectAsStateWithLifecycle`
//! (`lifecycle-runtime-compose`), D2150, D2160.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use compose_runtime::flow::{MutableStateFlow, StateFlow, StateFlowSubscription, Stream, next};
use compose_runtime::{ComposeArg, LaunchedEffect, State, mutable_state_of, remember};

use crate::composition::LOCAL_LIFECYCLE_OWNER;
use crate::coroutines::removing_itself;
use crate::lifecycle::{
    Lifecycle, LifecycleEvent, LifecycleObserver, LifecycleOwner, LifecycleState,
};

#[derive(Default)]
struct EventQueue {
    events: RefCell<VecDeque<LifecycleEvent>>,
    closed: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

/// The stream of [`Lifecycle::event_flow`]: each [`LifecycleEvent`] dispatched to its observer,
/// ending after [`LifecycleEvent::OnDestroy`], and removing its observer when dropped.
pub struct EventFlow {
    lifecycle: Lifecycle,
    started: bool,
    observer: Option<Rc<dyn LifecycleObserver>>,
    queue: Rc<EventQueue>,
}

impl Unpin for EventFlow {}

impl Lifecycle {
    /// A stream of the [`LifecycleEvent`]s this lifecycle dispatches, as `Lifecycle.eventFlow`:
    /// brings its observer up to the current state at the first poll, ends after
    /// [`LifecycleEvent::OnDestroy`] (at once if already [`LifecycleState::Destroyed`]), and
    /// removes its observer when dropped.
    pub fn event_flow(&self) -> EventFlow {
        EventFlow { lifecycle: self.clone(), started: false, observer: None, queue: Rc::default() }
    }
}

fn replace_waker(slot: &RefCell<Option<Waker>>, waker: &Waker) {
    if slot.borrow().as_ref().is_some_and(|current| current.will_wake(waker)) {
        return;
    }
    let new_waker = waker.clone();
    let previous = slot.borrow_mut().replace(new_waker);
    drop(previous);
}

impl Stream for EventFlow {
    type Item = LifecycleEvent;

    fn poll_next(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<LifecycleEvent>> {
        let this = &mut *self;
        if !this.started {
            this.started = true;
            if this.lifecycle.current_state() == LifecycleState::Destroyed {
                this.queue.closed.set(true);
            } else {
                let queue = this.queue.clone();
                let observer = removing_itself(move |_, event, remove| {
                    queue.events.borrow_mut().push_back(event);
                    if event == LifecycleEvent::OnDestroy {
                        queue.closed.set(true);
                        remove();
                    }
                    let waker = queue.waker.borrow_mut().take();
                    if let Some(waker) = waker {
                        waker.wake();
                    }
                });
                this.observer = Some(observer.clone());
                this.lifecycle.add_observer(observer);
                if this.queue.closed.get() {
                    this.observer = None;
                }
            }
        }
        if let Some(event) = this.queue.events.borrow_mut().pop_front() {
            return Poll::Ready(Some(event));
        }
        if this.queue.closed.get() {
            this.observer = None;
            return Poll::Ready(None);
        }
        replace_waker(&this.queue.waker, context.waker());
        Poll::Pending
    }
}

impl Drop for EventFlow {
    fn drop(&mut self) {
        if let Some(observer) = self.observer.take() {
            self.lifecycle.remove_observer(&observer);
        }
        let waker = self.queue.waker.borrow_mut().take();
        drop(waker);
    }
}

/// What a lifecycle can collect anew each time it reaches its target state, as Kotlin's cold or
/// hot `Flow<T>`: a `|| stream` closure making a [`Stream`], or a [`StateFlow`] /
/// [`MutableStateFlow`].
pub trait Flow: 'static {
    type Item: 'static;
    type Stream: Stream<Item = Self::Item> + 'static;

    /// Starts one collection's stream.
    fn subscribe_flow(&self) -> Self::Stream;
}

impl<S, F> Flow for F
where
    F: Fn() -> S + 'static,
    S: Stream + 'static,
    S::Item: 'static,
{
    type Item = S::Item;
    type Stream = S;

    fn subscribe_flow(&self) -> S {
        self()
    }
}

impl<T: Clone + PartialEq + 'static> Flow for StateFlow<T> {
    type Item = T;
    type Stream = StateFlowSubscription<T>;

    fn subscribe_flow(&self) -> StateFlowSubscription<T> {
        self.subscribe()
    }
}

impl<T: Clone + PartialEq + 'static> Flow for MutableStateFlow<T> {
    type Item = T;
    type Stream = StateFlowSubscription<T>;

    fn subscribe_flow(&self) -> StateFlowSubscription<T> {
        self.subscribe()
    }
}

struct FlowInner<F: Flow> {
    flow: F,
    active_stream: RefCell<Option<Pin<Box<F::Stream>>>>,
    generation: Cell<u64>,
    active: Cell<bool>,
    destroyed: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

/// The stream of [`flow_with_lifecycle`] and [`flow_with_lifecycle_with`].
pub struct FlowWithLifecycle<F: Flow> {
    lifecycle: Lifecycle,
    min_active_state: LifecycleState,
    started: bool,
    observer: Option<Rc<dyn LifecycleObserver>>,
    inner: Rc<FlowInner<F>>,
}

impl<F: Flow> Unpin for FlowWithLifecycle<F> {}

/// Collects `flow` while `lifecycle` is at least [`LifecycleState::Started`], cancelling upstream
/// collection when it drops below and starting again when it returns, until `lifecycle` is
/// destroyed, as `Flow.flowWithLifecycle(lifecycle)`.
pub fn flow_with_lifecycle<F: Flow>(
    flow: F,
    lifecycle: impl LifecycleOwner,
) -> FlowWithLifecycle<F> {
    flow_with_lifecycle_with(flow, lifecycle, LifecycleState::Started)
}

/// Like [`flow_with_lifecycle`], collecting while `lifecycle` is at least `min_active_state`, as
/// `Flow.flowWithLifecycle(lifecycle, minActiveState)`. Panics for
/// [`LifecycleState::Initialized`], as AndroidX throws.
pub fn flow_with_lifecycle_with<F: Flow>(
    flow: F,
    lifecycle: impl LifecycleOwner,
    min_active_state: LifecycleState,
) -> FlowWithLifecycle<F> {
    assert!(
        min_active_state != LifecycleState::Initialized,
        "repeatOnLifecycle cannot start work with the INITIALIZED lifecycle state."
    );
    FlowWithLifecycle {
        lifecycle: lifecycle.lifecycle(),
        min_active_state,
        started: false,
        observer: None,
        inner: Rc::new(FlowInner {
            flow,
            active_stream: RefCell::new(None),
            generation: Cell::new(0),
            active: Cell::new(false),
            destroyed: Cell::new(false),
            waker: RefCell::new(None),
        }),
    }
}

/// Method syntax for [`flow_with_lifecycle`] and [`flow_with_lifecycle_with`], as Kotlin's
/// `Flow<T>.flowWithLifecycle(...)` extension.
pub trait FlowWithLifecycleExt: Flow + Sized {
    /// Collects `self` while `lifecycle` is at least [`LifecycleState::Started`], as
    /// `flow.flowWithLifecycle(lifecycle)`.
    fn flow_with_lifecycle(self, lifecycle: impl LifecycleOwner) -> FlowWithLifecycle<Self> {
        flow_with_lifecycle(self, lifecycle)
    }

    /// Collects `self` while `lifecycle` is at least `min_active_state`, as
    /// `flow.flowWithLifecycle(lifecycle, minActiveState)`.
    fn flow_with_lifecycle_with(
        self,
        lifecycle: impl LifecycleOwner,
        min_active_state: LifecycleState,
    ) -> FlowWithLifecycle<Self> {
        flow_with_lifecycle_with(self, lifecycle, min_active_state)
    }
}

impl<F: Flow> FlowWithLifecycleExt for F {}

impl<F: Flow> Stream for FlowWithLifecycle<F> {
    type Item = F::Item;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<F::Item>> {
        let this = &mut *self;
        if !this.started {
            this.started = true;
            if this.lifecycle.current_state() == LifecycleState::Destroyed {
                this.inner.destroyed.set(true);
                return Poll::Ready(None);
            }
            let up = LifecycleEvent::up_to(this.min_active_state);
            let down = LifecycleEvent::down_from(this.min_active_state);
            let inner = this.inner.clone();
            let observer = removing_itself(move |_, event, remove| {
                if Some(event) == up {
                    let generation = inner.generation.get().wrapping_add(1);
                    inner.generation.set(generation);
                    inner.active.set(true);
                    let stream = Box::pin(inner.flow.subscribe_flow());
                    let alive = inner.generation.get() == generation && inner.active.get();
                    let previous = if alive {
                        inner.active_stream.borrow_mut().replace(stream)
                    } else {
                        Some(stream)
                    };
                    drop(previous);
                    if alive {
                        let waker = inner.waker.borrow_mut().take();
                        if let Some(waker) = waker {
                            waker.wake();
                        }
                    }
                    return;
                }
                if Some(event) == down {
                    inner.generation.set(inner.generation.get().wrapping_add(1));
                    inner.active.set(false);
                    let previous = inner.active_stream.borrow_mut().take();
                    drop(previous);
                }
                if event == LifecycleEvent::OnDestroy {
                    inner.generation.set(inner.generation.get().wrapping_add(1));
                    inner.active.set(false);
                    inner.destroyed.set(true);
                    remove();
                    let previous = inner.active_stream.borrow_mut().take();
                    drop(previous);
                    let waker = inner.waker.borrow_mut().take();
                    if let Some(waker) = waker {
                        waker.wake();
                    }
                }
            });
            this.observer = Some(observer.clone());
            this.lifecycle.add_observer(observer);
            if this.inner.destroyed.get() {
                this.observer = None;
            }
        }

        loop {
            if this.inner.destroyed.get() {
                this.observer = None;
                return Poll::Ready(None);
            }
            let generation = this.inner.generation.get();
            let Some(mut stream) = this.inner.active_stream.borrow_mut().take() else {
                break;
            };
            match stream.as_mut().poll_next(context) {
                Poll::Ready(Some(item)) => {
                    if this.inner.generation.get() == generation && this.inner.active.get() {
                        let previous = this.inner.active_stream.borrow_mut().replace(stream);
                        drop(previous);
                        return Poll::Ready(Some(item));
                    }
                    drop(stream);
                }
                Poll::Ready(None) => {
                    drop(stream);
                    if this.inner.generation.get() != generation {
                        continue;
                    }
                    break;
                }
                Poll::Pending => {
                    if this.inner.generation.get() == generation && this.inner.active.get() {
                        let previous = this.inner.active_stream.borrow_mut().replace(stream);
                        drop(previous);
                        break;
                    }
                    drop(stream);
                    if this.inner.generation.get() != generation {
                        continue;
                    }
                    break;
                }
            }
        }

        if this.inner.destroyed.get() {
            this.observer = None;
            return Poll::Ready(None);
        }
        replace_waker(&this.inner.waker, context.waker());
        Poll::Pending
    }
}

impl<F: Flow> Drop for FlowWithLifecycle<F> {
    fn drop(&mut self) {
        if let Some(observer) = self.observer.take() {
            self.lifecycle.remove_observer(&observer);
        }
        self.inner.generation.set(self.inner.generation.get().wrapping_add(1));
        self.inner.active.set(false);
        let previous = self.inner.active_stream.borrow_mut().take();
        let waker = self.inner.waker.borrow_mut().take();
        drop(previous);
        drop(waker);
    }
}

/// Lifecycle-aware state collection for [`StateFlow`] and [`MutableStateFlow`], as AndroidX's
/// `StateFlow.collectAsStateWithLifecycle`.
pub trait StateFlowLifecycleExt<T: Clone + PartialEq + 'static> {
    /// State starting at the flow's current value and following its emissions while the local
    /// [`LOCAL_LIFECYCLE_OWNER`] is at least [`LifecycleState::Started`], as
    /// `stateFlow.collectAsStateWithLifecycle()`.
    fn collect_as_state_with_lifecycle(&self) -> State<T>;

    /// Like [`collect_as_state_with_lifecycle`](Self::collect_as_state_with_lifecycle), following
    /// `lifecycle` at `min_active_state`, as
    /// `stateFlow.collectAsStateWithLifecycle(lifecycle, minActiveState)`.
    fn collect_as_state_with_lifecycle_with(
        &self,
        lifecycle: impl LifecycleOwner,
        min_active_state: LifecycleState,
    ) -> State<T>;
}

impl<T: Clone + PartialEq + 'static> StateFlowLifecycleExt<T> for StateFlow<T> {
    fn collect_as_state_with_lifecycle(&self) -> State<T> {
        self.collect_as_state_with_lifecycle_with(
            LOCAL_LIFECYCLE_OWNER.current(),
            LifecycleState::Started,
        )
    }

    fn collect_as_state_with_lifecycle_with(
        &self,
        lifecycle: impl LifecycleOwner,
        min_active_state: LifecycleState,
    ) -> State<T> {
        let (key, now, collected) = (self.clone(), self.clone(), self.clone());
        collect_into(key, move || now.value(), lifecycle.lifecycle(), min_active_state, collected)
    }
}

impl<T: Clone + PartialEq + 'static> StateFlowLifecycleExt<T> for MutableStateFlow<T> {
    fn collect_as_state_with_lifecycle(&self) -> State<T> {
        self.as_state_flow().collect_as_state_with_lifecycle()
    }

    fn collect_as_state_with_lifecycle_with(
        &self,
        lifecycle: impl LifecycleOwner,
        min_active_state: LifecycleState,
    ) -> State<T> {
        self.as_state_flow().collect_as_state_with_lifecycle_with(lifecycle, min_active_state)
    }
}

/// State starting at `initial` and following `flow` while [`LOCAL_LIFECYCLE_OWNER`] is at least
/// [`LifecycleState::Started`], as `Flow.collectAsStateWithLifecycle(initialValue)`. `key` says
/// when `flow` is another stream, as [`compose_runtime::collect_as_state`].
pub fn collect_as_state_with_lifecycle<K, F>(key: K, initial: F::Item, flow: F) -> State<F::Item>
where
    K: ComposeArg,
    F: Flow,
    F::Item: PartialEq + 'static,
{
    collect_as_state_with_lifecycle_with(
        key,
        initial,
        LOCAL_LIFECYCLE_OWNER.current(),
        LifecycleState::Started,
        flow,
    )
}

/// Like [`collect_as_state_with_lifecycle`], following `lifecycle` at `min_active_state`, as
/// `Flow.collectAsStateWithLifecycle(initialValue, lifecycle, minActiveState)`. Panics for
/// [`LifecycleState::Initialized`], as AndroidX throws.
pub fn collect_as_state_with_lifecycle_with<K, F>(
    key: K,
    initial: F::Item,
    lifecycle: impl LifecycleOwner,
    min_active_state: LifecycleState,
    flow: F,
) -> State<F::Item>
where
    K: ComposeArg,
    F: Flow,
    F::Item: PartialEq + 'static,
{
    collect_into(key, move || initial, lifecycle.lifecycle(), min_active_state, flow)
}

fn collect_into<K, F>(
    key: K,
    initial: impl FnOnce() -> F::Item,
    lifecycle: Lifecycle,
    min_active_state: LifecycleState,
    flow: F,
) -> State<F::Item>
where
    K: ComposeArg,
    F: Flow,
    F::Item: PartialEq + 'static,
{
    assert!(
        min_active_state != LifecycleState::Initialized,
        "repeatOnLifecycle cannot start work with the INITIALIZED lifecycle state."
    );
    let state = remember(|| mutable_state_of(initial()));
    let observed = lifecycle.clone();
    LaunchedEffect((key, lifecycle, min_active_state), async move {
        let mut stream = flow_with_lifecycle_with(flow, observed, min_active_state);
        while let Some(value) = next(&mut stream).await {
            state.set(value);
        }
    });
    state.as_state()
}
