//! What the tests share: an input recording what it hears, and a panic's text.

use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_navigation_event::*;

#[derive(Default)]
pub struct Heard {
    pub added: Cell<u32>,
    pub removed: Cell<u32>,
    pub enabled: RefCell<Vec<bool>>,
    pub history_changes: Cell<u32>,
    pub history: RefCell<NavigationEventHistory>,
    pub dispatcher: RefCell<Option<NavigationEventDispatcher>>,
    pub order: RefCell<Vec<&'static str>>,
}

struct Recording(Rc<Heard>);

impl NavigationEventInputCallbacks for Recording {
    fn on_added(&self, dispatcher: &NavigationEventDispatcher) {
        self.0.added.set(self.0.added.get() + 1);
        *self.0.dispatcher.borrow_mut() = Some(dispatcher.clone());
        self.0.order.borrow_mut().push("on_added");
    }

    fn on_removed(&self) {
        self.0.removed.set(self.0.removed.get() + 1);
        *self.0.dispatcher.borrow_mut() = None;
        self.0.order.borrow_mut().push("on_removed");
    }

    fn on_has_enabled_handlers_changed(&self, has_enabled_handlers: bool) {
        self.0.enabled.borrow_mut().push(has_enabled_handlers);
        self.0.order.borrow_mut().push("on_has_enabled_handlers_changed");
    }

    fn on_history_changed(&self, history: &NavigationEventHistory) {
        self.0.history_changes.set(self.0.history_changes.get() + 1);
        *self.0.history.borrow_mut() = history.clone();
        self.0.order.borrow_mut().push("on_history_changed");
    }
}

/// As the test files' own `TestNavigationEventInput`.
pub struct TestInput {
    input: NavigationEventInput,
    pub heard: Rc<Heard>,
}

impl Deref for TestInput {
    type Target = NavigationEventInput;
    fn deref(&self) -> &NavigationEventInput {
        &self.input
    }
}

impl TestInput {
    pub fn new() -> TestInput {
        let heard = Rc::new(Heard::default());
        TestInput { input: NavigationEventInput::new(Recording(heard.clone())), heard }
    }

    pub fn back_started(&self) {
        self.dispatch_on_back_started(NavigationEvent::default());
    }

    pub fn back_progressed(&self) {
        self.dispatch_on_back_progressed(NavigationEvent::default());
    }

    pub fn back_completed(&self) {
        self.dispatch_on_back_completed();
    }

    pub fn back_cancelled(&self) {
        self.dispatch_on_back_cancelled();
    }

    pub fn forward_started(&self) {
        self.dispatch_on_forward_started(NavigationEvent::default());
    }

    pub fn forward_progressed(&self) {
        self.dispatch_on_forward_progressed(NavigationEvent::default());
    }

    pub fn forward_completed(&self) {
        self.dispatch_on_forward_completed();
    }

    pub fn forward_cancelled(&self) {
        self.dispatch_on_forward_cancelled();
    }

    pub fn enabled(&self) -> Vec<bool> {
        self.heard.enabled.borrow().clone()
    }
}

/// A dispatcher with a handler, then an input.
pub fn dispatcher_with_handler()
-> (NavigationEventDispatcher, TestNavigationEventHandler, TestInput) {
    let dispatcher = NavigationEventDispatcher::new();
    let handler = TestNavigationEventHandler::new();
    dispatcher.add_handler(&handler);
    let input = TestInput::new();
    dispatcher.add_input(&input);
    (dispatcher, handler, input)
}

/// What `action` panics with.
#[track_caller]
pub fn panic_text(action: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(action)).expect_err("it should have panicked");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(ToString::to_string))
        .unwrap_or_default()
}

/// The info of a home screen and a details screen, as the tests' `HomeScreenInfo`.
#[derive(PartialEq, Debug)]
pub struct Home(pub &'static str);

#[derive(PartialEq, Debug)]
pub struct Details(pub &'static str);

pub fn home(id: &'static str) -> NavigationEventInfo {
    NavigationEventInfo::new(Home(id))
}

pub fn details(id: &'static str) -> NavigationEventInfo {
    NavigationEventInfo::new(Details(id))
}
