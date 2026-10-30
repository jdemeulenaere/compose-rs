//! `Lifecycle` and `LifecycleRegistry`, as AndroidX's: its observers moved up in the order they
//! were added and down in the opposite one, an observer never ahead of the one before it.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use compose_runtime::{ComposeArg, CoroutineScope, Owned, OwnedState, State, mutable_state_of};

/// Where a lifecycle is, as AndroidX's `Lifecycle.State`, in its order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum LifecycleState {
    Destroyed,
    Initialized,
    Created,
    Started,
    Resumed,
}

impl LifecycleState {
    /// Whether this state is `state` or further on, as `isAtLeast`.
    pub fn is_at_least(self, state: LifecycleState) -> bool {
        self >= state
    }
}

impl ComposeArg for LifecycleState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// What a lifecycle tells its observers, as AndroidX's `Lifecycle.Event`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LifecycleEvent {
    OnCreate,
    OnStart,
    OnResume,
    OnPause,
    OnStop,
    OnDestroy,
    OnAny,
}

impl ComposeArg for LifecycleEvent {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl LifecycleEvent {
    /// The event leaving `state` downwards, as `downFrom`.
    pub fn down_from(state: LifecycleState) -> Option<LifecycleEvent> {
        match state {
            LifecycleState::Created => Some(LifecycleEvent::OnDestroy),
            LifecycleState::Started => Some(LifecycleEvent::OnStop),
            LifecycleState::Resumed => Some(LifecycleEvent::OnPause),
            _ => None,
        }
    }

    /// The event reaching `state` downwards, as `downTo`.
    pub fn down_to(state: LifecycleState) -> Option<LifecycleEvent> {
        match state {
            LifecycleState::Destroyed => Some(LifecycleEvent::OnDestroy),
            LifecycleState::Created => Some(LifecycleEvent::OnStop),
            LifecycleState::Started => Some(LifecycleEvent::OnPause),
            _ => None,
        }
    }

    /// The event leaving `state` upwards, as `upFrom`.
    pub fn up_from(state: LifecycleState) -> Option<LifecycleEvent> {
        match state {
            LifecycleState::Initialized => Some(LifecycleEvent::OnCreate),
            LifecycleState::Created => Some(LifecycleEvent::OnStart),
            LifecycleState::Started => Some(LifecycleEvent::OnResume),
            _ => None,
        }
    }

    /// The event reaching `state` upwards, as `upTo`.
    pub fn up_to(state: LifecycleState) -> Option<LifecycleEvent> {
        match state {
            LifecycleState::Created => Some(LifecycleEvent::OnCreate),
            LifecycleState::Started => Some(LifecycleEvent::OnStart),
            LifecycleState::Resumed => Some(LifecycleEvent::OnResume),
            _ => None,
        }
    }

    /// The state this event leads to, as `targetState`. Panics for `OnAny`, as AndroidX throws.
    pub fn target_state(self) -> LifecycleState {
        match self {
            LifecycleEvent::OnCreate | LifecycleEvent::OnStop => LifecycleState::Created,
            LifecycleEvent::OnStart | LifecycleEvent::OnPause => LifecycleState::Started,
            LifecycleEvent::OnResume => LifecycleState::Resumed,
            LifecycleEvent::OnDestroy => LifecycleState::Destroyed,
            LifecycleEvent::OnAny => panic!("ON_ANY doesn't have a target state"),
        }
    }
}

/// What hears a lifecycle's events, as AndroidX's `LifecycleEventObserver`. A closure is one.
/// Added and removed by its `Rc`, whose identity is the observer's.
pub trait LifecycleObserver {
    fn on_state_changed(&self, source: &Lifecycle, event: LifecycleEvent);
}

impl<F: Fn(&Lifecycle, LifecycleEvent)> LifecycleObserver for F {
    fn on_state_changed(&self, source: &Lifecycle, event: LifecycleEvent) {
        self(source, event);
    }
}

/// An observer by event, each doing nothing unless overridden, as AndroidX's
/// `DefaultLifecycleObserver`. [`DefaultLifecycleObserverAdapter`] makes it a [`LifecycleObserver`].
pub trait DefaultLifecycleObserver {
    fn on_create(&self, _owner: &Lifecycle) {}
    fn on_start(&self, _owner: &Lifecycle) {}
    fn on_resume(&self, _owner: &Lifecycle) {}
    fn on_pause(&self, _owner: &Lifecycle) {}
    fn on_stop(&self, _owner: &Lifecycle) {}
    fn on_destroy(&self, _owner: &Lifecycle) {}
}

/// A [`DefaultLifecycleObserver`] as a [`LifecycleObserver`], as AndroidX's
/// `DefaultLifecycleObserverAdapter`: the event's method, then `event_observer`'s `on_state_changed`
/// if it has one (an object implementing both, AndroidX's `Lifecycling`).
pub struct DefaultLifecycleObserverAdapter {
    observer: Rc<dyn DefaultLifecycleObserver>,
    event_observer: Option<Rc<dyn LifecycleObserver>>,
}

impl DefaultLifecycleObserverAdapter {
    pub fn new(
        observer: Rc<dyn DefaultLifecycleObserver>,
        event_observer: Option<Rc<dyn LifecycleObserver>>,
    ) -> DefaultLifecycleObserverAdapter {
        DefaultLifecycleObserverAdapter { observer, event_observer }
    }
}

impl LifecycleObserver for DefaultLifecycleObserverAdapter {
    fn on_state_changed(&self, source: &Lifecycle, event: LifecycleEvent) {
        match event {
            LifecycleEvent::OnCreate => self.observer.on_create(source),
            LifecycleEvent::OnStart => self.observer.on_start(source),
            LifecycleEvent::OnResume => self.observer.on_resume(source),
            LifecycleEvent::OnPause => self.observer.on_pause(source),
            LifecycleEvent::OnStop => self.observer.on_stop(source),
            LifecycleEvent::OnDestroy => self.observer.on_destroy(source),
            LifecycleEvent::OnAny => panic!("ON_ANY must not been send by anybody"),
        }
        if let Some(event_observer) = &self.event_observer {
            event_observer.on_state_changed(source, event);
        }
    }
}

/// What has a lifecycle, as AndroidX's `LifecycleOwner`.
pub trait LifecycleOwner {
    fn lifecycle(&self) -> Lifecycle;
}

/// A lifecycle, as AndroidX's `Lifecycle`: a handle (clones are the same one), compared by
/// identity. It's also its own [`LifecycleOwner`].
#[derive(Clone)]
pub struct Lifecycle {
    inner: Rc<Inner>,
}

/// An observer and where it is, as `ObserverWithState`.
struct Entry {
    /// Its order of addition.
    order: u64,
    observer: Rc<dyn LifecycleObserver>,
    state: Cell<LifecycleState>,
}

struct Inner {
    state: Cell<LifecycleState>,
    /// In the order they were added, as `FastSafeIterableMap`: the passes walk by `Entry::order`,
    /// so an entry removed or added during a dispatch is seen as AndroidX's iterators see it.
    observers: RefCell<Vec<Rc<Entry>>>,
    next_order: Cell<u64>,
    adding_observer_counter: Cell<u32>,
    handling_event: Cell<bool>,
    new_event_occurred: Cell<bool>,
    parent_states: RefCell<Vec<LifecycleState>>,
    /// The state as a state, set once the observers are told, as `currentStateFlow`.
    observed_state: OwnedState<LifecycleState>,
    pub(crate) scope: OnceCell<Owned<CoroutineScope>>,
}

fn same_observer(a: &Rc<dyn LifecycleObserver>, b: &Rc<dyn LifecycleObserver>) -> bool {
    std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b))
}

impl Lifecycle {
    fn new() -> Lifecycle {
        Lifecycle {
            inner: Rc::new(Inner {
                state: Cell::new(LifecycleState::Initialized),
                observers: RefCell::new(Vec::new()),
                next_order: Cell::new(0),
                adding_observer_counter: Cell::new(0),
                handling_event: Cell::new(false),
                new_event_occurred: Cell::new(false),
                parent_states: RefCell::new(Vec::new()),
                observed_state: mutable_state_of(LifecycleState::Initialized),
                scope: OnceCell::new(),
            }),
        }
    }

    /// Where it is, as `currentState` (no state read: [`as_state`](Self::as_state) is one).
    pub fn current_state(&self) -> LifecycleState {
        self.inner.state.get()
    }

    /// Its state as a state, set once the observers were told of a change, as `currentStateFlow`
    /// and `currentStateAsState()`.
    pub fn as_state(&self) -> State<LifecycleState> {
        self.inner.observed_state.as_state()
    }

    /// Adds an observer, as `addObserver`: it hears the events up to where the lifecycle is, in
    /// order. Added already: nothing.
    pub fn add_observer(&self, observer: Rc<dyn LifecycleObserver>) {
        let inner = &self.inner;
        let initial = if inner.state.get() == LifecycleState::Destroyed {
            LifecycleState::Destroyed
        } else {
            LifecycleState::Initialized
        };
        if inner.observers.borrow().iter().any(|entry| same_observer(&entry.observer, &observer)) {
            return;
        }
        let order = inner.next_order.get();
        inner.next_order.set(order + 1);
        let entry = Rc::new(Entry { order, observer, state: Cell::new(initial) });
        inner.observers.borrow_mut().push(entry.clone());
        let is_reentrance = inner.adding_observer_counter.get() != 0 || inner.handling_event.get();
        let mut target = self.target_state(order);
        inner.adding_observer_counter.set(inner.adding_observer_counter.get() + 1);
        while entry.state.get() < target && self.contains(order) {
            inner.parent_states.borrow_mut().push(entry.state.get());
            let event = LifecycleEvent::up_from(entry.state.get())
                .unwrap_or_else(|| panic!("no event up from {:?}", entry.state.get()));
            self.dispatch(&entry, event);
            inner.parent_states.borrow_mut().pop();
            // The state or a sibling's may have changed during the dispatch.
            target = self.target_state(order);
        }
        if !is_reentrance {
            self.sync();
        }
        inner.adding_observer_counter.set(inner.adding_observer_counter.get() - 1);
    }

    /// Adds a closure as an observer and gives it back to remove it, as `addObserver { … }`.
    pub fn add_observer_fn(
        &self,
        on_state_changed: impl Fn(&Lifecycle, LifecycleEvent) + 'static,
    ) -> Rc<dyn LifecycleObserver> {
        let observer: Rc<dyn LifecycleObserver> = Rc::new(on_state_changed);
        self.add_observer(observer.clone());
        observer
    }

    /// Removes an observer, as `removeObserver`: it hears no more, and no events to bring it down.
    pub fn remove_observer(&self, observer: &Rc<dyn LifecycleObserver>) {
        self.inner.observers.borrow_mut().retain(|entry| !same_observer(&entry.observer, observer));
    }

    fn contains(&self, order: u64) -> bool {
        self.inner.observers.borrow().iter().any(|entry| entry.order == order)
    }

    /// As `calculateTargetState`: not beyond the lifecycle, the observer added before this one, or
    /// the observer being dispatched to.
    fn target_state(&self, order: u64) -> LifecycleState {
        let inner = &self.inner;
        let sibling = inner
            .observers
            .borrow()
            .iter()
            .rev()
            .find(|entry| entry.order < order)
            .map_or(LifecycleState::Resumed, |entry| entry.state.get());
        let parent =
            inner.parent_states.borrow().last().copied().unwrap_or(LifecycleState::Resumed);
        inner.state.get().min(sibling).min(parent)
    }

    fn dispatch(&self, entry: &Entry, event: LifecycleEvent) {
        let new_state = event.target_state();
        entry.state.set(entry.state.get().min(new_state));
        entry.observer.on_state_changed(self, event);
        entry.state.set(new_state);
    }

    fn move_to_state(&self, next: LifecycleState) {
        let inner = &self.inner;
        let current = inner.state.get();
        if current == next {
            return;
        }
        if current == LifecycleState::Initialized && next == LifecycleState::Destroyed {
            panic!("State must be at least 'CREATED' to be moved to 'DESTROYED'");
        }
        if current == LifecycleState::Destroyed {
            panic!("State is 'DESTROYED' and cannot be moved to `{next:?}`");
        }
        inner.state.set(next);
        if inner.handling_event.get() || inner.adding_observer_counter.get() != 0 {
            // A re-entrant call: the active loop notices and starts over.
            inner.new_event_occurred.set(true);
            return;
        }
        inner.handling_event.set(true);
        self.sync();
        inner.handling_event.set(false);
        if inner.state.get() == LifecycleState::Destroyed {
            inner.observers.borrow_mut().clear();
        }
    }

    fn is_synced(&self) -> bool {
        let observers = self.inner.observers.borrow();
        match (observers.first(), observers.last()) {
            (Some(eldest), Some(newest)) => {
                eldest.state.get() == newest.state.get()
                    && self.inner.state.get() == newest.state.get()
            }
            _ => true,
        }
    }

    fn sync(&self) {
        let inner = &self.inner;
        while !self.is_synced() {
            inner.new_event_occurred.set(false);
            let eldest = inner.observers.borrow().first().map(|entry| entry.state.get());
            if eldest.is_some_and(|eldest| inner.state.get() < eldest) {
                self.backward_pass();
            }
            let newest = inner.observers.borrow().last().map(|entry| entry.state.get());
            if !inner.new_event_occurred.get()
                && newest.is_some_and(|newest| inner.state.get() > newest)
            {
                self.forward_pass();
            }
        }
        inner.new_event_occurred.set(false);
        inner.observed_state.set(inner.state.get());
    }

    /// Up, eldest first, those added meanwhile too (`forEachWithAdditions`).
    fn forward_pass(&self) {
        let inner = &self.inner;
        let mut after = None;
        loop {
            let next = inner
                .observers
                .borrow()
                .iter()
                .find(|entry| after.is_none_or(|after| entry.order > after))
                .cloned();
            let Some(entry) = next else { break };
            after = Some(entry.order);
            while entry.state.get() < inner.state.get()
                && !inner.new_event_occurred.get()
                && self.contains(entry.order)
            {
                inner.parent_states.borrow_mut().push(entry.state.get());
                let event = LifecycleEvent::up_from(entry.state.get())
                    .unwrap_or_else(|| panic!("no event up from {:?}", entry.state.get()));
                self.dispatch(&entry, event);
                inner.parent_states.borrow_mut().pop();
            }
        }
    }

    /// Down, newest first, not those added meanwhile (`forEachReversed`).
    fn backward_pass(&self) {
        let inner = &self.inner;
        let mut before = inner.next_order.get();
        loop {
            let next =
                inner.observers.borrow().iter().rev().find(|entry| entry.order < before).cloned();
            let Some(entry) = next else { break };
            before = entry.order;
            while entry.state.get() > inner.state.get()
                && !inner.new_event_occurred.get()
                && self.contains(entry.order)
            {
                let event = LifecycleEvent::down_from(entry.state.get())
                    .unwrap_or_else(|| panic!("no event down from {:?}", entry.state.get()));
                inner.parent_states.borrow_mut().push(event.target_state());
                self.dispatch(&entry, event);
                inner.parent_states.borrow_mut().pop();
            }
        }
    }

    pub(crate) fn scope_cell(&self) -> &OnceCell<Owned<CoroutineScope>> {
        &self.inner.scope
    }
}

impl LifecycleOwner for Lifecycle {
    fn lifecycle(&self) -> Lifecycle {
        self.clone()
    }
}

impl PartialEq for Lifecycle {
    fn eq(&self, other: &Lifecycle) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl ComposeArg for Lifecycle {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Debug for Lifecycle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Lifecycle({:?})", self.current_state())
    }
}

/// What moves a lifecycle, as AndroidX's `LifecycleRegistry`. Reads as its [`Lifecycle`] too.
#[derive(Clone)]
pub struct LifecycleRegistry {
    lifecycle: Lifecycle,
}

impl Default for LifecycleRegistry {
    fn default() -> LifecycleRegistry {
        LifecycleRegistry::new()
    }
}

impl LifecycleRegistry {
    /// A registry at `Initialized`, as `LifecycleRegistry(provider)`.
    pub fn new() -> LifecycleRegistry {
        LifecycleRegistry { lifecycle: Lifecycle::new() }
    }

    /// Moves to `state`, telling the observers, as `currentState = state`. Panics moving from
    /// `Initialized` to `Destroyed` or away from `Destroyed`, as AndroidX throws.
    pub fn set_current_state(&self, state: LifecycleState) {
        self.lifecycle.move_to_state(state);
    }

    /// Moves to where `event` leads, as `handleLifecycleEvent`.
    pub fn handle_lifecycle_event(&self, event: LifecycleEvent) {
        self.lifecycle.move_to_state(event.target_state());
    }

    /// How many observers it has, as `observerCount`.
    pub fn observer_count(&self) -> usize {
        self.lifecycle.inner.observers.borrow().len()
    }
}

impl std::ops::Deref for LifecycleRegistry {
    type Target = Lifecycle;
    fn deref(&self) -> &Lifecycle {
        &self.lifecycle
    }
}

impl LifecycleOwner for LifecycleRegistry {
    fn lifecycle(&self) -> Lifecycle {
        self.lifecycle.clone()
    }
}
