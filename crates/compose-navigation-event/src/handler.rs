//! `NavigationEventHandler`, as AndroidX's abstract class: its state here, what it does in
//! [`NavigationEventCallbacks`].

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::{Rc, Weak};

use crate::dispatcher::{DispatcherInner, NavigationEventDispatcher};
use crate::event::{NavigationEvent, NavigationEventInfo, NavigationEventTransitionState};

use NavigationEventTransitionState as State;

/// What a handler does, as the methods AndroidX's `NavigationEventHandler` has to override. Each
/// is given the handler. A handler without its `on_back_completed` or `on_forward_completed` panics
/// if it gets such an event, as AndroidX's throws.
#[allow(unused_variables)]
pub trait NavigationEventCallbacks {
    fn on_back_started(&self, handler: &NavigationEventHandler, event: &NavigationEvent) {}
    fn on_back_progressed(&self, handler: &NavigationEventHandler, event: &NavigationEvent) {}
    fn on_back_completed(&self, handler: &NavigationEventHandler) {
        panic!(
            "A handler that receives a 'backCompleted' event must override 'onBackCompleted()' to \
             handle the callback."
        )
    }
    fn on_back_cancelled(&self, handler: &NavigationEventHandler) {}
    fn on_forward_started(&self, handler: &NavigationEventHandler, event: &NavigationEvent) {}
    fn on_forward_progressed(&self, handler: &NavigationEventHandler, event: &NavigationEvent) {}
    fn on_forward_completed(&self, handler: &NavigationEventHandler) {
        panic!(
            "A handler that receives a 'forwardCompleted' event must override \
             'onForwardCompleted()' to handle the callback."
        )
    }
    fn on_forward_cancelled(&self, handler: &NavigationEventHandler) {}
}

struct HandlerInner {
    current_info: RefCell<NavigationEventInfo>,
    back_info: RefCell<Vec<NavigationEventInfo>>,
    forward_info: RefCell<Vec<NavigationEventInfo>>,
    transition_state: Cell<State>,
    is_back_enabled: Cell<bool>,
    is_forward_enabled: Cell<bool>,
    dispatcher: RefCell<Weak<DispatcherInner>>,
    callbacks: Box<dyn NavigationEventCallbacks>,
}

/// One that can take gestures, as AndroidX's `NavigationEventHandler`: a handle, compared by
/// identity, added to a dispatcher, and removed with [`remove`](Self::remove).
#[derive(Clone)]
pub struct NavigationEventHandler {
    inner: Rc<HandlerInner>,
}

impl PartialEq for NavigationEventHandler {
    fn eq(&self, other: &NavigationEventHandler) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl fmt::Debug for NavigationEventHandler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "NavigationEventHandler@{:x}",
            Rc::as_ptr(&self.inner) as *const () as usize
        )
    }
}

impl NavigationEventHandler {
    /// What tells it from others, as an effect's key.
    pub fn id(&self) -> usize {
        Rc::as_ptr(&self.inner) as *const () as usize
    }

    pub fn new(
        initial_info: NavigationEventInfo,
        is_back_enabled: bool,
        is_forward_enabled: bool,
        callbacks: impl NavigationEventCallbacks + 'static,
    ) -> NavigationEventHandler {
        NavigationEventHandler {
            inner: Rc::new(HandlerInner {
                current_info: RefCell::new(initial_info),
                back_info: RefCell::new(Vec::new()),
                forward_info: RefCell::new(Vec::new()),
                transition_state: Cell::new(State::Idle),
                is_back_enabled: Cell::new(is_back_enabled),
                is_forward_enabled: Cell::new(is_forward_enabled),
                dispatcher: RefCell::new(Weak::new()),
                callbacks: Box::new(callbacks),
            }),
        }
    }

    pub fn current_info(&self) -> NavigationEventInfo {
        self.inner.current_info.borrow().clone()
    }

    pub fn back_info(&self) -> Vec<NavigationEventInfo> {
        self.inner.back_info.borrow().clone()
    }

    pub fn forward_info(&self) -> Vec<NavigationEventInfo> {
        self.inner.forward_info.borrow().clone()
    }

    pub fn transition_state(&self) -> State {
        self.inner.transition_state.get()
    }

    fn dispatcher(&self) -> Option<NavigationEventDispatcher> {
        self.inner.dispatcher.borrow().upgrade().map(NavigationEventDispatcher::from_inner)
    }

    fn dispatcher_disabled(&self) -> bool {
        self.dispatcher().is_some_and(|dispatcher| !dispatcher.is_enabled())
    }

    /// Whether it takes back gestures: not while its dispatcher is disabled, as its getter.
    pub fn is_back_enabled(&self) -> bool {
        !self.dispatcher_disabled() && self.inner.is_back_enabled.get()
    }

    pub fn set_back_enabled(&self, enabled: bool) {
        if self.inner.is_back_enabled.replace(enabled) != enabled {
            self.refresh();
        }
    }

    /// Whether it takes forward gestures: not while its dispatcher is disabled, as its getter.
    pub fn is_forward_enabled(&self) -> bool {
        !self.dispatcher_disabled() && self.inner.is_forward_enabled.get()
    }

    pub fn set_forward_enabled(&self, enabled: bool) {
        if self.inner.is_forward_enabled.replace(enabled) != enabled {
            self.refresh();
        }
    }

    fn refresh(&self) {
        if let Some(dispatcher) = self.dispatcher() {
            dispatcher.shared_processor().refresh_enabled_handlers();
        }
    }

    /// Takes it off its dispatcher, as `remove`: a gesture it's in is cancelled.
    pub fn remove(&self) {
        if let Some(dispatcher) = self.dispatcher() {
            dispatcher.remove_handler(self);
        }
    }

    /// What it tells of where the navigation is, as `setInfo`.
    pub fn set_info(
        &self,
        current_info: NavigationEventInfo,
        back_info: Vec<NavigationEventInfo>,
        forward_info: Vec<NavigationEventInfo>,
    ) {
        *self.inner.current_info.borrow_mut() = current_info;
        *self.inner.back_info.borrow_mut() = back_info;
        *self.inner.forward_info.borrow_mut() = forward_info;
        if let Some(dispatcher) = self.dispatcher() {
            dispatcher.shared_processor().update_enabled_handler_info(Some(self));
        }
    }

    pub(crate) fn is_registered(&self) -> bool {
        self.inner.dispatcher.borrow().strong_count() > 0
    }

    pub(crate) fn set_dispatcher(&self, dispatcher: Weak<DispatcherInner>) {
        *self.inner.dispatcher.borrow_mut() = dispatcher;
    }

    fn in_progress(&self, event: &NavigationEvent, direction: i32) {
        self.inner.transition_state.set(State::InProgress { latest_event: *event, direction });
    }

    pub(crate) fn do_on_back_started(&self, event: &NavigationEvent) {
        self.in_progress(event, State::TRANSITIONING_BACK);
        self.inner.callbacks.on_back_started(self, event);
    }

    pub(crate) fn do_on_back_progressed(&self, event: &NavigationEvent) {
        self.in_progress(event, State::TRANSITIONING_BACK);
        self.inner.callbacks.on_back_progressed(self, event);
    }

    pub(crate) fn do_on_back_completed(&self) {
        self.inner.transition_state.set(State::Idle);
        self.inner.callbacks.on_back_completed(self);
    }

    pub(crate) fn do_on_back_cancelled(&self) {
        self.inner.transition_state.set(State::Idle);
        self.inner.callbacks.on_back_cancelled(self);
    }

    pub(crate) fn do_on_forward_started(&self, event: &NavigationEvent) {
        self.in_progress(event, State::TRANSITIONING_FORWARD);
        self.inner.callbacks.on_forward_started(self, event);
    }

    pub(crate) fn do_on_forward_progressed(&self, event: &NavigationEvent) {
        self.in_progress(event, State::TRANSITIONING_FORWARD);
        self.inner.callbacks.on_forward_progressed(self, event);
    }

    pub(crate) fn do_on_forward_completed(&self) {
        self.inner.transition_state.set(State::Idle);
        self.inner.callbacks.on_forward_completed(self);
    }

    pub(crate) fn do_on_forward_cancelled(&self) {
        self.inner.transition_state.set(State::Idle);
        self.inner.callbacks.on_forward_cancelled(self);
    }
}
