//! `NavigationEventDispatcher`, as AndroidX's: a tree of them shares one processor.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::{Rc, Weak};

use compose_runtime::ComposeArg;

use crate::event::{NavigationEvent, NavigationEventHistory, NavigationEventTransitionState};
use crate::handler::NavigationEventHandler;
use crate::input::NavigationEventInput;
use crate::processor::NavigationEventProcessor;

/// What runs when a back gesture completes with no handler enabled, as `OnBackCompletedFallback`.
pub type OnBackCompletedFallback = Rc<dyn Fn()>;
/// The same for a forward gesture, as `OnForwardCompletedFallback`.
pub type OnForwardCompletedFallback = Rc<dyn Fn()>;

pub(crate) struct DispatcherInner {
    parent: RefCell<Option<NavigationEventDispatcher>>,
    local_back_fallback: RefCell<Option<OnBackCompletedFallback>>,
    local_forward_fallback: RefCell<Option<OnForwardCompletedFallback>>,
    is_disposed: Cell<bool>,
    is_enabled: Cell<bool>,
    shared_processor: Rc<NavigationEventProcessor>,
    /// Held by whoever made them (a parent keeping them would be a cycle).
    child_dispatchers: RefCell<Vec<Weak<DispatcherInner>>>,
    handlers: RefCell<Vec<NavigationEventHandler>>,
    inputs: RefCell<Vec<NavigationEventInput>>,
}

/// Where inputs send gestures and handlers take them, as AndroidX's `NavigationEventDispatcher`: a
/// handle, compared by identity, also what `NavigationEventDispatcherOwner` gives. Children share
/// the parent's handlers and inputs by one processor.
#[derive(Clone)]
pub struct NavigationEventDispatcher {
    inner: Rc<DispatcherInner>,
}

impl PartialEq for NavigationEventDispatcher {
    fn eq(&self, other: &NavigationEventDispatcher) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl ComposeArg for NavigationEventDispatcher {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl fmt::Debug for NavigationEventDispatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "NavigationEventDispatcher@{:x}",
            Rc::as_ptr(&self.inner) as *const () as usize
        )
    }
}

impl Default for NavigationEventDispatcher {
    fn default() -> NavigationEventDispatcher {
        NavigationEventDispatcher::new()
    }
}

impl NavigationEventDispatcher {
    /// Handlers take the gestures first, overlays above the others, the last added of each first.
    pub const PRIORITY_OVERLAY: i32 = 0;
    pub const PRIORITY_DEFAULT: i32 = 1;

    fn make(
        parent: Option<NavigationEventDispatcher>,
        back: Option<OnBackCompletedFallback>,
        forward: Option<OnForwardCompletedFallback>,
    ) -> NavigationEventDispatcher {
        let shared_processor = parent.as_ref().map_or_else(
            || Rc::new(NavigationEventProcessor::new()),
            |p| p.inner.shared_processor.clone(),
        );
        let dispatcher = NavigationEventDispatcher {
            inner: Rc::new(DispatcherInner {
                parent: RefCell::new(parent.clone()),
                local_back_fallback: RefCell::new(back),
                local_forward_fallback: RefCell::new(forward),
                is_disposed: Cell::new(false),
                is_enabled: Cell::new(true),
                shared_processor,
                child_dispatchers: RefCell::new(Vec::new()),
                handlers: RefCell::new(Vec::new()),
                inputs: RefCell::new(Vec::new()),
            }),
        };
        if let Some(parent) = parent {
            parent.inner.child_dispatchers.borrow_mut().push(dispatcher.downgrade());
        }
        dispatcher
    }

    /// A root, as `NavigationEventDispatcher()`.
    pub fn new() -> NavigationEventDispatcher {
        NavigationEventDispatcher::make(None, None, None)
    }

    /// A root with a fallback, as `NavigationEventDispatcher(onBackCompletedFallback)`.
    pub fn with_back_fallback(on_back: impl Fn() + 'static) -> NavigationEventDispatcher {
        NavigationEventDispatcher::with_fallbacks(Rc::new(on_back), None)
    }

    /// A root with both fallbacks, as `NavigationEventDispatcher(onBackCompletedFallback,
    /// onForwardCompletedFallback)`.
    pub fn with_fallbacks(
        on_back: OnBackCompletedFallback,
        on_forward: Option<OnForwardCompletedFallback>,
    ) -> NavigationEventDispatcher {
        NavigationEventDispatcher::make(None, Some(on_back), on_forward)
    }

    /// A child, as `NavigationEventDispatcher(parent)`.
    pub fn with_parent(parent: &NavigationEventDispatcher) -> NavigationEventDispatcher {
        NavigationEventDispatcher::make(Some(parent.clone()), None, None)
    }

    pub(crate) fn from_inner(inner: Rc<DispatcherInner>) -> NavigationEventDispatcher {
        NavigationEventDispatcher { inner }
    }

    pub(crate) fn downgrade(&self) -> Weak<DispatcherInner> {
        Rc::downgrade(&self.inner)
    }

    pub(crate) fn shared_processor(&self) -> &NavigationEventProcessor {
        &self.inner.shared_processor
    }

    fn parent(&self) -> Option<NavigationEventDispatcher> {
        self.inner.parent.borrow().clone()
    }

    pub(crate) fn is_disposed(&self) -> bool {
        self.parent().is_some_and(|parent| parent.is_disposed()) || self.inner.is_disposed.get()
    }

    #[track_caller]
    fn check_invariants(&self) {
        assert!(
            !self.is_disposed(),
            "This NavigationEventDispatcher has already been disposed and cannot be used."
        );
    }

    /// Whether it dispatches: not while its parent doesn't, as its getter.
    pub fn is_enabled(&self) -> bool {
        self.parent().is_none_or(|parent| parent.is_enabled()) && self.inner.is_enabled.get()
    }

    #[track_caller]
    pub fn set_enabled(&self, enabled: bool) {
        self.check_invariants();
        if self.inner.is_enabled.replace(enabled) != enabled {
            self.inner.shared_processor.refresh_enabled_handlers();
        }
    }

    /// The fallback for a back gesture here, its own or its parent's.
    pub fn on_back_completed_fallback(&self) -> Option<OnBackCompletedFallback> {
        let local = self.inner.local_back_fallback.borrow().clone();
        local.or_else(|| self.parent().and_then(|parent| parent.on_back_completed_fallback()))
    }

    /// The fallback for a forward gesture here, its own or its parent's.
    pub fn on_forward_completed_fallback(&self) -> Option<OnForwardCompletedFallback> {
        let local = self.inner.local_forward_fallback.borrow().clone();
        local.or_else(|| self.parent().and_then(|parent| parent.on_forward_completed_fallback()))
    }

    /// Whether a gesture's under way, shared by the tree, as `transitionState` (a state read).
    pub fn transition_state(&self) -> NavigationEventTransitionState {
        self.inner.shared_processor.transition_state()
    }

    /// The places its enabled handlers tell of, shared by the tree, as `history` (a state read).
    pub fn history(&self) -> NavigationEventHistory {
        self.inner.shared_processor.history()
    }

    /// Adds `handler` at [`PRIORITY_DEFAULT`](Self::PRIORITY_DEFAULT).
    #[track_caller]
    pub fn add_handler(&self, handler: &NavigationEventHandler) {
        self.add_handler_with_priority(handler, NavigationEventDispatcher::PRIORITY_DEFAULT);
    }

    /// Adds `handler`, as `addHandler(handler, priority)`: again, it does nothing; to another
    /// dispatcher, or with another priority than the two, it panics.
    #[track_caller]
    pub fn add_handler_with_priority(&self, handler: &NavigationEventHandler, priority: i32) {
        let layer = match priority {
            NavigationEventDispatcher::PRIORITY_OVERLAY => 1,
            NavigationEventDispatcher::PRIORITY_DEFAULT => 0,
            _ => panic!("Unsupported priority value: {priority}"),
        };
        self.add_handler_in_layer(handler, layer);
    }

    /// Adds `handler` under `layer` dialogs: 0 is the default priority, more an overlay, deeper
    /// layers first. A dialog here isn't a window with a dispatcher of its own (D845).
    #[doc(hidden)]
    #[track_caller]
    pub fn add_handler_in_layer(&self, handler: &NavigationEventHandler, layer: u32) {
        self.check_invariants();
        if self.inner.handlers.borrow().contains(handler) {
            return;
        }
        self.inner.handlers.borrow_mut().push(handler.clone());
        self.inner.shared_processor.add_handler(self, handler, layer);
    }

    pub(crate) fn remove_handler(&self, handler: &NavigationEventHandler) {
        let index = self.inner.handlers.borrow().iter().position(|other| other == handler);
        if let Some(index) = index {
            self.inner.handlers.borrow_mut().remove(index);
            self.inner.shared_processor.remove_handler(handler);
        }
    }

    /// Adds `input`, told of all the handlers, as `addInput(input)`.
    #[track_caller]
    pub fn add_input(&self, input: &NavigationEventInput) {
        self.check_invariants();
        self.add_input_at(input, -1);
    }

    /// Adds `input`, told of those of one priority, as `addInput(input, priority)`.
    #[track_caller]
    pub fn add_input_with_priority(&self, input: &NavigationEventInput, priority: i32) {
        self.check_invariants();
        assert!(
            priority == NavigationEventDispatcher::PRIORITY_DEFAULT
                || priority == NavigationEventDispatcher::PRIORITY_OVERLAY,
            "Unsupported priority value: {priority}"
        );
        self.add_input_at(input, priority);
    }

    fn add_input_at(&self, input: &NavigationEventInput, priority: i32) {
        if self.inner.inputs.borrow().contains(input) {
            return;
        }
        self.inner.inputs.borrow_mut().push(input.clone());
        self.inner.shared_processor.add_input(self, input, priority);
    }

    /// Takes `input` off, as `removeInput`.
    #[track_caller]
    pub fn remove_input(&self, input: &NavigationEventInput) {
        self.check_invariants();
        let index = self.inner.inputs.borrow().iter().position(|other| other == input);
        if let Some(index) = index {
            self.inner.inputs.borrow_mut().remove(index);
            self.inner.shared_processor.remove_input(input);
        }
    }

    pub(crate) fn dispatch_on_started(
        &self,
        input: &NavigationEventInput,
        direction: i32,
        event: Option<NavigationEvent>,
    ) {
        self.check_invariants();
        if self.is_enabled() {
            self.inner.shared_processor.dispatch_on_started(input, direction, event);
        }
    }

    pub(crate) fn dispatch_on_progressed(
        &self,
        input: &NavigationEventInput,
        direction: i32,
        event: NavigationEvent,
    ) {
        self.check_invariants();
        if self.is_enabled() {
            self.inner.shared_processor.dispatch_on_progressed(input, direction, event);
        }
    }

    pub(crate) fn dispatch_on_completed(&self, input: &NavigationEventInput, direction: i32) {
        self.check_invariants();
        if self.is_enabled() {
            self.inner.shared_processor.dispatch_on_completed(
                input,
                direction,
                self.on_back_completed_fallback(),
                self.on_forward_completed_fallback(),
            );
        }
    }

    pub(crate) fn dispatch_on_cancelled(&self, input: &NavigationEventInput, direction: i32) {
        self.check_invariants();
        if self.is_enabled() {
            self.inner.shared_processor.dispatch_on_cancelled(input, direction);
        }
    }

    /// Ends it and its children, as `dispose`: their inputs and handlers are removed. Using it after
    /// that panics.
    #[track_caller]
    pub fn dispose(&self) {
        self.check_invariants();
        self.inner.is_disposed.set(true);
        let mut queue = vec![self.clone()];
        while let Some(current) = queue.pop() {
            current.inner.is_disposed.set(true);
            let children = std::mem::take(&mut *current.inner.child_dispatchers.borrow_mut());
            queue.extend(
                children
                    .iter()
                    .filter_map(Weak::upgrade)
                    .map(|inner| NavigationEventDispatcher { inner }),
            );
            let inputs = std::mem::take(&mut *current.inner.inputs.borrow_mut());
            for input in &inputs {
                current.inner.shared_processor.remove_input(input);
            }
            let handlers = std::mem::take(&mut *current.inner.handlers.borrow_mut());
            for handler in &handlers {
                current.inner.shared_processor.remove_handler(handler);
            }
            let parent = current.inner.parent.borrow_mut().take();
            if let Some(parent) = parent {
                let this = Rc::as_ptr(&current.inner);
                parent
                    .inner
                    .child_dispatchers
                    .borrow_mut()
                    .retain(|child| !std::ptr::eq(child.as_ptr(), this));
            }
            // Its own fallbacks go with it, as its parent's did.
            current.inner.local_back_fallback.borrow_mut().take();
            current.inner.local_forward_fallback.borrow_mut().take();
        }
    }
}
