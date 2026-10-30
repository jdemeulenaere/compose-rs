//! `NavigationEventInput`, as AndroidX's abstract class: where gestures come from. Its state here,
//! what it hears in [`NavigationEventInputCallbacks`].

use std::cell::{Cell, RefCell};
use std::fmt;
use std::ops::Deref;
use std::rc::{Rc, Weak};

use crate::dispatcher::{DispatcherInner, NavigationEventDispatcher};
use crate::event::{NavigationEvent, NavigationEventHistory, NavigationEventTransitionState};

use NavigationEventTransitionState as State;

/// What an input hears, as the methods of AndroidX's `NavigationEventInput` to override.
#[allow(unused_variables)]
pub trait NavigationEventInputCallbacks {
    fn on_added(&self, dispatcher: &NavigationEventDispatcher) {}
    fn on_removed(&self) {}
    fn on_has_enabled_handlers_changed(&self, has_enabled_handlers: bool) {}
    fn on_has_enabled_back_handlers_changed(&self, has_enabled_back_handlers: bool) {}
    fn on_has_enabled_forward_handlers_changed(&self, has_enabled_forward_handlers: bool) {}
    fn on_history_changed(&self, history: &NavigationEventHistory) {}
}

struct NoCallbacks;

impl NavigationEventInputCallbacks for NoCallbacks {}

struct InputInner {
    current_dispatcher: RefCell<Weak<DispatcherInner>>,
    is_predictive_back_in_progress: Cell<bool>,
    is_predictive_forward_in_progress: Cell<bool>,
    has_enabled_handlers: Cell<bool>,
    has_enabled_back_handlers: Cell<bool>,
    has_enabled_forward_handlers: Cell<bool>,
    callbacks: Box<dyn NavigationEventInputCallbacks>,
}

/// Where gestures come from, as AndroidX's `NavigationEventInput`: a handle, compared by identity.
/// Its `dispatch_on_*`, protected in AndroidX's, are what a platform calls.
#[derive(Clone)]
pub struct NavigationEventInput {
    inner: Rc<InputInner>,
}

impl PartialEq for NavigationEventInput {
    fn eq(&self, other: &NavigationEventInput) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl fmt::Debug for NavigationEventInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "NavigationEventInput@{:x}",
            Rc::as_ptr(&self.inner) as *const () as usize
        )
    }
}

impl NavigationEventInput {
    pub fn new(callbacks: impl NavigationEventInputCallbacks + 'static) -> NavigationEventInput {
        NavigationEventInput {
            inner: Rc::new(InputInner {
                current_dispatcher: RefCell::new(Weak::new()),
                is_predictive_back_in_progress: Cell::new(false),
                is_predictive_forward_in_progress: Cell::new(false),
                has_enabled_handlers: Cell::new(false),
                has_enabled_back_handlers: Cell::new(false),
                has_enabled_forward_handlers: Cell::new(false),
                callbacks: Box::new(callbacks),
            }),
        }
    }

    pub fn has_enabled_handlers(&self) -> bool {
        self.inner.has_enabled_handlers.get()
    }

    pub fn has_enabled_back_handlers(&self) -> bool {
        self.inner.has_enabled_back_handlers.get()
    }

    pub fn has_enabled_forward_handlers(&self) -> bool {
        self.inner.has_enabled_forward_handlers.get()
    }

    fn dispatcher(&self) -> Option<NavigationEventDispatcher> {
        self.inner.current_dispatcher.borrow().upgrade().map(NavigationEventDispatcher::from_inner)
    }

    pub(crate) fn do_on_added(&self, dispatcher: &NavigationEventDispatcher) {
        assert!(
            self.dispatcher().is_none(),
            "Input '{self:?}' is already added to dispatcher {:?}.",
            self.dispatcher()
        );
        *self.inner.current_dispatcher.borrow_mut() = dispatcher.downgrade();
        self.inner.callbacks.on_added(dispatcher);
    }

    pub(crate) fn do_on_removed(&self) {
        *self.inner.current_dispatcher.borrow_mut() = Weak::new();
        self.inner.is_predictive_back_in_progress.set(false);
        self.inner.is_predictive_forward_in_progress.set(false);
        self.inner.callbacks.on_removed();
    }

    pub(crate) fn do_on_has_enabled_handlers_changed(&self, value: bool) {
        self.inner.has_enabled_handlers.set(value);
        self.inner.callbacks.on_has_enabled_handlers_changed(value);
    }

    pub(crate) fn do_on_has_enabled_back_handlers_changed(&self, value: bool) {
        self.inner.has_enabled_back_handlers.set(value);
        self.inner.callbacks.on_has_enabled_back_handlers_changed(value);
    }

    pub(crate) fn do_on_has_enabled_forward_handlers_changed(&self, value: bool) {
        self.inner.has_enabled_forward_handlers.set(value);
        self.inner.callbacks.on_has_enabled_forward_handlers_changed(value);
    }

    pub(crate) fn do_on_history_changed(&self, history: &NavigationEventHistory) {
        self.inner.callbacks.on_history_changed(history);
    }

    fn started(&self, flag: &Cell<bool>, direction: i32, event: NavigationEvent) {
        let Some(dispatcher) = self.dispatcher() else { return };
        // A gesture under way isn't started again.
        if !flag.get() {
            dispatcher.dispatch_on_started(self, direction, Some(event));
            flag.set(true);
        }
    }

    fn progressed(&self, flag: &Cell<bool>, direction: i32, event: NavigationEvent) {
        let Some(dispatcher) = self.dispatcher() else { return };
        if flag.get() {
            dispatcher.dispatch_on_progressed(self, direction, event);
        }
    }

    fn ended(&self, flag: &Cell<bool>, direction: i32, completed: bool) {
        let Some(dispatcher) = self.dispatcher() else { return };
        if !flag.get() {
            // Not predictive: a start for the processor's "start, then an end".
            dispatcher.dispatch_on_started(self, direction, None);
        }
        if completed {
            dispatcher.dispatch_on_completed(self, direction);
        } else {
            dispatcher.dispatch_on_cancelled(self, direction);
        }
        flag.set(false);
    }

    /// A back gesture starting, as `dispatchOnBackStarted`.
    pub fn dispatch_on_back_started(&self, event: NavigationEvent) {
        let flag = &self.inner.is_predictive_back_in_progress;
        self.started(flag, State::TRANSITIONING_BACK, event);
    }

    /// Its progress, as `dispatchOnBackProgressed`.
    pub fn dispatch_on_back_progressed(&self, event: NavigationEvent) {
        let flag = &self.inner.is_predictive_back_in_progress;
        self.progressed(flag, State::TRANSITIONING_BACK, event);
    }

    /// Its end, or a press with no gesture, as `dispatchOnBackCancelled`.
    pub fn dispatch_on_back_cancelled(&self) {
        let flag = &self.inner.is_predictive_back_in_progress;
        self.ended(flag, State::TRANSITIONING_BACK, false);
    }

    /// Its end, or a press with no gesture, as `dispatchOnBackCompleted`.
    pub fn dispatch_on_back_completed(&self) {
        let flag = &self.inner.is_predictive_back_in_progress;
        self.ended(flag, State::TRANSITIONING_BACK, true);
    }

    /// A forward gesture starting, as `dispatchOnForwardStarted`.
    pub fn dispatch_on_forward_started(&self, event: NavigationEvent) {
        let flag = &self.inner.is_predictive_forward_in_progress;
        self.started(flag, State::TRANSITIONING_FORWARD, event);
    }

    /// Its progress, as `dispatchOnForwardProgressed`.
    pub fn dispatch_on_forward_progressed(&self, event: NavigationEvent) {
        let flag = &self.inner.is_predictive_forward_in_progress;
        self.progressed(flag, State::TRANSITIONING_FORWARD, event);
    }

    /// Its end, or a press with no gesture, as `dispatchOnForwardCancelled`.
    pub fn dispatch_on_forward_cancelled(&self) {
        let flag = &self.inner.is_predictive_forward_in_progress;
        self.ended(flag, State::TRANSITIONING_FORWARD, false);
    }

    /// Its end, or a press with no gesture, as `dispatchOnForwardCompleted`.
    pub fn dispatch_on_forward_completed(&self) {
        let flag = &self.inner.is_predictive_forward_in_progress;
        self.ended(flag, State::TRANSITIONING_FORWARD, true);
    }
}

/// An input to call by hand, as AndroidX's `DirectNavigationEventInput`.
#[derive(Clone, PartialEq, Debug)]
pub struct DirectNavigationEventInput {
    input: NavigationEventInput,
}

impl Default for DirectNavigationEventInput {
    fn default() -> DirectNavigationEventInput {
        DirectNavigationEventInput::new()
    }
}

impl Deref for DirectNavigationEventInput {
    type Target = NavigationEventInput;
    fn deref(&self) -> &NavigationEventInput {
        &self.input
    }
}

impl DirectNavigationEventInput {
    pub fn new() -> DirectNavigationEventInput {
        DirectNavigationEventInput::with_callbacks(NoCallbacks)
    }

    /// One that hears what an input does, as a subclass of AndroidX's would.
    pub fn with_callbacks(
        callbacks: impl NavigationEventInputCallbacks + 'static,
    ) -> DirectNavigationEventInput {
        DirectNavigationEventInput { input: NavigationEventInput::new(callbacks) }
    }

    pub fn back_started(&self, event: NavigationEvent) {
        self.dispatch_on_back_started(event);
    }

    pub fn back_progressed(&self, event: NavigationEvent) {
        self.dispatch_on_back_progressed(event);
    }

    pub fn back_cancelled(&self) {
        self.dispatch_on_back_cancelled();
    }

    pub fn back_completed(&self) {
        self.dispatch_on_back_completed();
    }

    pub fn forward_started(&self, event: NavigationEvent) {
        self.dispatch_on_forward_started(event);
    }

    pub fn forward_progressed(&self, event: NavigationEvent) {
        self.dispatch_on_forward_progressed(event);
    }

    pub fn forward_cancelled(&self) {
        self.dispatch_on_forward_cancelled();
    }

    pub fn forward_completed(&self) {
        self.dispatch_on_forward_completed();
    }
}
