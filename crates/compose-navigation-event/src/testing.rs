//! What tests use, as AndroidX's `navigationevent-testing`.

use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::rc::Rc;

use crate::dispatcher::NavigationEventDispatcher;
use crate::event::{NavigationEvent, NavigationEventInfo};
use crate::handler::{NavigationEventCallbacks, NavigationEventHandler};

type OnEvent = Box<dyn Fn(&TestNavigationEventHandler, &NavigationEvent)>;
type OnEnd = Box<dyn Fn(&TestNavigationEventHandler)>;

#[derive(Default)]
struct Calls {
    back_started: RefCell<Vec<NavigationEvent>>,
    back_progressed: RefCell<Vec<NavigationEvent>>,
    back_completed: Cell<u32>,
    back_cancelled: Cell<u32>,
    forward_started: RefCell<Vec<NavigationEvent>>,
    forward_progressed: RefCell<Vec<NavigationEvent>>,
    forward_completed: Cell<u32>,
    forward_cancelled: Cell<u32>,
}

#[derive(Default)]
struct TestCallbacks {
    calls: Rc<Calls>,
    on_back_started: Option<OnEvent>,
    on_back_cancelled: Option<OnEnd>,
    on_forward_started: Option<OnEvent>,
    on_forward_cancelled: Option<OnEnd>,
}

impl TestCallbacks {
    fn wrap(&self, handler: &NavigationEventHandler) -> TestNavigationEventHandler {
        TestNavigationEventHandler { handler: handler.clone(), calls: self.calls.clone() }
    }
}

impl NavigationEventCallbacks for TestCallbacks {
    fn on_back_started(&self, handler: &NavigationEventHandler, event: &NavigationEvent) {
        self.calls.back_started.borrow_mut().push(*event);
        if let Some(on_event) = &self.on_back_started {
            on_event(&self.wrap(handler), event);
        }
    }

    fn on_back_progressed(&self, _: &NavigationEventHandler, event: &NavigationEvent) {
        self.calls.back_progressed.borrow_mut().push(*event);
    }

    fn on_back_completed(&self, _: &NavigationEventHandler) {
        self.calls.back_completed.set(self.calls.back_completed.get() + 1);
    }

    fn on_back_cancelled(&self, handler: &NavigationEventHandler) {
        self.calls.back_cancelled.set(self.calls.back_cancelled.get() + 1);
        if let Some(on_end) = &self.on_back_cancelled {
            on_end(&self.wrap(handler));
        }
    }

    fn on_forward_started(&self, handler: &NavigationEventHandler, event: &NavigationEvent) {
        self.calls.forward_started.borrow_mut().push(*event);
        if let Some(on_event) = &self.on_forward_started {
            on_event(&self.wrap(handler), event);
        }
    }

    fn on_forward_progressed(&self, _: &NavigationEventHandler, event: &NavigationEvent) {
        self.calls.forward_progressed.borrow_mut().push(*event);
    }

    fn on_forward_completed(&self, _: &NavigationEventHandler) {
        self.calls.forward_completed.set(self.calls.forward_completed.get() + 1);
    }

    fn on_forward_cancelled(&self, handler: &NavigationEventHandler) {
        self.calls.forward_cancelled.set(self.calls.forward_cancelled.get() + 1);
        if let Some(on_end) = &self.on_forward_cancelled {
            on_end(&self.wrap(handler));
        }
    }
}

/// A handler counting what it gets, as AndroidX's `TestNavigationEventHandler`.
#[derive(Clone)]
pub struct TestNavigationEventHandler {
    handler: NavigationEventHandler,
    calls: Rc<Calls>,
}

impl Deref for TestNavigationEventHandler {
    type Target = NavigationEventHandler;
    fn deref(&self) -> &NavigationEventHandler {
        &self.handler
    }
}

/// How a [`TestNavigationEventHandler`] is made.
#[derive(Default)]
struct TestBuilder {
    callbacks: TestCallbacks,
}

impl Default for TestNavigationEventHandler {
    fn default() -> TestNavigationEventHandler {
        TestNavigationEventHandler::new()
    }
}

impl TestNavigationEventHandler {
    /// With no info, enabled both ways.
    pub fn new() -> TestNavigationEventHandler {
        TestNavigationEventHandler::enabled(true, true)
    }

    fn make(info: NavigationEventInfo, back: bool, forward: bool, builder: TestBuilder) -> Self {
        let calls = builder.callbacks.calls.clone();
        let handler = NavigationEventHandler::new(info, back, forward, builder.callbacks);
        TestNavigationEventHandler { handler, calls }
    }

    /// With no info, enabled as said.
    pub fn enabled(is_back_enabled: bool, is_forward_enabled: bool) -> TestNavigationEventHandler {
        let info = NavigationEventInfo::none();
        TestNavigationEventHandler::make(
            info,
            is_back_enabled,
            is_forward_enabled,
            TestBuilder::default(),
        )
    }

    /// With info, enabled both ways.
    pub fn with_info(
        current_info: NavigationEventInfo,
        back_info: Vec<NavigationEventInfo>,
        forward_info: Vec<NavigationEventInfo>,
    ) -> TestNavigationEventHandler {
        let handler = TestNavigationEventHandler::make(
            current_info.clone(),
            true,
            true,
            TestBuilder::default(),
        );
        handler.set_info(current_info, back_info, forward_info);
        handler
    }

    /// Running `on_back_started` on that event, as its parameter.
    pub fn on_back_started(
        on_back_started: impl Fn(&TestNavigationEventHandler, &NavigationEvent) + 'static,
    ) -> TestNavigationEventHandler {
        let callbacks = TestCallbacks {
            on_back_started: Some(Box::new(on_back_started)),
            ..TestCallbacks::default()
        };
        let info = NavigationEventInfo::none();
        TestNavigationEventHandler::make(info, true, true, TestBuilder { callbacks })
    }

    /// Running `on_back_cancelled` on that event, as its parameter.
    pub fn on_back_cancelled(
        on_back_cancelled: impl Fn(&TestNavigationEventHandler) + 'static,
    ) -> TestNavigationEventHandler {
        let callbacks = TestCallbacks {
            on_back_cancelled: Some(Box::new(on_back_cancelled)),
            ..TestCallbacks::default()
        };
        let info = NavigationEventInfo::none();
        TestNavigationEventHandler::make(info, true, true, TestBuilder { callbacks })
    }

    /// Running `on_forward_started` on that event, as its parameter.
    pub fn on_forward_started(
        on_forward_started: impl Fn(&TestNavigationEventHandler, &NavigationEvent) + 'static,
    ) -> TestNavigationEventHandler {
        let callbacks = TestCallbacks {
            on_forward_started: Some(Box::new(on_forward_started)),
            ..TestCallbacks::default()
        };
        let info = NavigationEventInfo::none();
        TestNavigationEventHandler::make(info, true, true, TestBuilder { callbacks })
    }

    /// Running `on_forward_cancelled` on that event, as its parameter.
    pub fn on_forward_cancelled(
        on_forward_cancelled: impl Fn(&TestNavigationEventHandler) + 'static,
    ) -> TestNavigationEventHandler {
        let callbacks = TestCallbacks {
            on_forward_cancelled: Some(Box::new(on_forward_cancelled)),
            ..TestCallbacks::default()
        };
        let info = NavigationEventInfo::none();
        TestNavigationEventHandler::make(info, true, true, TestBuilder { callbacks })
    }

    pub fn on_back_started_invocations(&self) -> u32 {
        self.calls.back_started.borrow().len() as u32
    }

    pub fn on_back_progressed_invocations(&self) -> u32 {
        self.calls.back_progressed.borrow().len() as u32
    }

    pub fn on_back_completed_invocations(&self) -> u32 {
        self.calls.back_completed.get()
    }

    pub fn on_back_cancelled_invocations(&self) -> u32 {
        self.calls.back_cancelled.get()
    }

    pub fn on_forward_started_invocations(&self) -> u32 {
        self.calls.forward_started.borrow().len() as u32
    }

    pub fn on_forward_progressed_invocations(&self) -> u32 {
        self.calls.forward_progressed.borrow().len() as u32
    }

    pub fn on_forward_completed_invocations(&self) -> u32 {
        self.calls.forward_completed.get()
    }

    pub fn on_forward_cancelled_invocations(&self) -> u32 {
        self.calls.forward_cancelled.get()
    }
}

/// A root dispatcher counting its fallbacks, as AndroidX's `TestNavigationEventDispatcherOwner`.
#[derive(Clone)]
pub struct TestNavigationEventDispatcherOwner {
    dispatcher: NavigationEventDispatcher,
    back_fallbacks: Rc<Cell<u32>>,
    forward_fallbacks: Rc<Cell<u32>>,
}

impl Default for TestNavigationEventDispatcherOwner {
    fn default() -> TestNavigationEventDispatcherOwner {
        TestNavigationEventDispatcherOwner::new()
    }
}

impl TestNavigationEventDispatcherOwner {
    pub fn new() -> TestNavigationEventDispatcherOwner {
        let (back_fallbacks, forward_fallbacks) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (back, forward) = (back_fallbacks.clone(), forward_fallbacks.clone());
        let dispatcher = NavigationEventDispatcher::with_fallbacks(
            Rc::new(move || back.set(back.get() + 1)),
            Some(Rc::new(move || forward.set(forward.get() + 1))),
        );
        TestNavigationEventDispatcherOwner { dispatcher, back_fallbacks, forward_fallbacks }
    }

    pub fn navigation_event_dispatcher(&self) -> NavigationEventDispatcher {
        self.dispatcher.clone()
    }

    pub fn on_back_completed_fallback_invocations(&self) -> u32 {
        self.back_fallbacks.get()
    }

    pub fn on_forward_completed_fallback_invocations(&self) -> u32 {
        self.forward_fallbacks.get()
    }
}
