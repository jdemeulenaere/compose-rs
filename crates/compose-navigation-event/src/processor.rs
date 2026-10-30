//! `NavigationEventProcessor`, as AndroidX's: what decides which handler takes which gesture, shared
//! by a tree of dispatchers. Nothing is borrowed while a handler or an input is called: they add and
//! remove others.

use std::cell::{Cell, RefCell};

use compose_runtime::{OwnedState, mutable_state_of};

use crate::dispatcher::{
    NavigationEventDispatcher, OnBackCompletedFallback, OnForwardCompletedFallback,
};
use crate::event::{
    NavigationEvent, NavigationEventHistory, NavigationEventInfo, NavigationEventTransitionState,
};
use crate::handler::NavigationEventHandler;
use crate::input::NavigationEventInput;

use NavigationEventTransitionState as State;

/// Whether some handler's enabled, of one group.
#[derive(Clone, Copy, Default, PartialEq)]
struct Enabled {
    any: bool,
    back: bool,
    forward: bool,
}

pub(crate) struct NavigationEventProcessor {
    transition_state: OwnedState<NavigationEventTransitionState>,
    history: OwnedState<NavigationEventHistory>,
    /// Those of dialogs, by how many they're in, deepest first, the last added first in each.
    overlay_handlers: RefCell<Vec<(u32, NavigationEventHandler)>>,
    default_handlers: RefCell<Vec<NavigationEventHandler>>,
    in_progress_handler: RefCell<Option<NavigationEventHandler>>,
    in_progress_direction: Cell<i32>,
    in_progress_input: RefCell<Option<NavigationEventInput>>,
    unspecified_inputs: RefCell<Vec<NavigationEventInput>>,
    default_inputs: RefCell<Vec<NavigationEventInput>>,
    overlay_inputs: RefCell<Vec<NavigationEventInput>>,
    enabled_overlay: Cell<Enabled>,
    enabled_default: Cell<Enabled>,
    enabled_all: Cell<Enabled>,
}

fn enabled_of<'a>(handlers: impl Iterator<Item = &'a NavigationEventHandler>) -> Enabled {
    let mut enabled = Enabled::default();
    for handler in handlers {
        enabled.back |= handler.is_back_enabled();
        enabled.forward |= handler.is_forward_enabled();
    }
    enabled.any = enabled.back || enabled.forward;
    enabled
}

fn notify(inputs: &RefCell<Vec<NavigationEventInput>>, old: Enabled, new: Enabled) {
    if old == new {
        return;
    }
    let inputs = inputs.borrow().clone();
    for input in inputs {
        if old.any != new.any {
            input.do_on_has_enabled_handlers_changed(new.any);
        }
        if old.back != new.back {
            input.do_on_has_enabled_back_handlers_changed(new.back);
        }
        if old.forward != new.forward {
            input.do_on_has_enabled_forward_handlers_changed(new.forward);
        }
    }
}

impl NavigationEventProcessor {
    pub(crate) fn new() -> NavigationEventProcessor {
        NavigationEventProcessor {
            transition_state: mutable_state_of(State::Idle),
            history: mutable_state_of(NavigationEventHistory::default()),
            overlay_handlers: RefCell::new(Vec::new()),
            default_handlers: RefCell::new(Vec::new()),
            in_progress_handler: RefCell::new(None),
            in_progress_direction: Cell::new(State::TRANSITIONING_UNKNOWN),
            in_progress_input: RefCell::new(None),
            unspecified_inputs: RefCell::new(Vec::new()),
            default_inputs: RefCell::new(Vec::new()),
            overlay_inputs: RefCell::new(Vec::new()),
            enabled_overlay: Cell::new(Enabled::default()),
            enabled_default: Cell::new(Enabled::default()),
            enabled_all: Cell::new(Enabled::default()),
        }
    }

    pub(crate) fn transition_state(&self) -> NavigationEventTransitionState {
        self.transition_state.get()
    }

    pub(crate) fn history(&self) -> NavigationEventHistory {
        self.history.get()
    }

    pub(crate) fn refresh_enabled_handlers(&self) {
        let overlay = {
            let handlers = self.overlay_handlers.borrow();
            enabled_of(handlers.iter().map(|(_, handler)| handler))
        };
        let default = enabled_of(self.default_handlers.borrow().iter());
        let all = Enabled {
            any: overlay.any || default.any,
            back: overlay.back || default.back,
            forward: overlay.forward || default.forward,
        };
        let old_overlay = self.enabled_overlay.replace(overlay);
        let old_default = self.enabled_default.replace(default);
        let old_all = self.enabled_all.replace(all);
        notify(&self.overlay_inputs, old_overlay, overlay);
        notify(&self.default_inputs, old_default, default);
        notify(&self.unspecified_inputs, old_all, all);
        let active = self.active_handler();
        self.update_enabled_handler_info(active.as_ref());
    }

    fn active_handler(&self) -> Option<NavigationEventHandler> {
        let in_progress = self.in_progress_handler.borrow().clone();
        in_progress.or_else(|| self.resolve_enabled_handler(State::TRANSITIONING_UNKNOWN))
    }

    /// `handler`'s info changed: the history follows, if it's the one active, as
    /// `updateEnabledHandlerInfo`.
    pub(crate) fn update_enabled_handler_info(&self, handler: Option<&NavigationEventHandler>) {
        let active = self.active_handler();
        if active.as_ref() != handler {
            return;
        }
        let new_history = match &active {
            // None left: not stuck on the last one's.
            None => NavigationEventHistory::default(),
            Some(active) => NavigationEventHistory::new(
                active.current_info(),
                self.resolve_combined_back_info(),
                active.forward_info(),
            ),
        };
        if self.history.get() == new_history {
            return;
        }
        self.history.set(new_history.clone());
        for inputs in [&self.overlay_inputs, &self.default_inputs, &self.unspecified_inputs] {
            let inputs = inputs.borrow().clone();
            for input in inputs {
                input.do_on_history_changed(&new_history);
            }
        }
    }

    pub(crate) fn add_handler(
        &self,
        dispatcher: &NavigationEventDispatcher,
        handler: &NavigationEventHandler,
        layer: u32,
    ) {
        assert!(
            !handler.is_registered(),
            "Handler '{handler:?}' is already registered with a dispatcher"
        );
        if layer == 0 {
            self.default_handlers.borrow_mut().insert(0, handler.clone());
        } else {
            let mut handlers = self.overlay_handlers.borrow_mut();
            let index =
                handlers.iter().position(|(other, _)| *other <= layer).unwrap_or(handlers.len());
            handlers.insert(index, (layer, handler.clone()));
        }
        handler.set_dispatcher(dispatcher.downgrade());
        self.refresh_enabled_handlers();
    }

    pub(crate) fn remove_handler(&self, handler: &NavigationEventHandler) {
        if self.in_progress_handler.borrow().as_ref() == Some(handler) {
            match self.in_progress_direction.get() {
                State::TRANSITIONING_BACK => handler.do_on_back_cancelled(),
                State::TRANSITIONING_FORWARD => handler.do_on_forward_cancelled(),
                _ => {}
            }
            self.clear_in_progress();
        }
        self.overlay_handlers.borrow_mut().retain(|(_, other)| other != handler);
        self.default_handlers.borrow_mut().retain(|other| other != handler);
        handler.set_dispatcher(std::rc::Weak::new());
        self.refresh_enabled_handlers();
    }

    fn clear_in_progress(&self) {
        *self.in_progress_handler.borrow_mut() = None;
        self.in_progress_direction.set(State::TRANSITIONING_UNKNOWN);
        *self.in_progress_input.borrow_mut() = None;
    }

    pub(crate) fn add_input(
        &self,
        dispatcher: &NavigationEventDispatcher,
        input: &NavigationEventInput,
        priority: i32,
    ) {
        let (inputs, enabled) = match priority {
            NavigationEventDispatcher::PRIORITY_OVERLAY => {
                (&self.overlay_inputs, self.enabled_overlay.get())
            }
            NavigationEventDispatcher::PRIORITY_DEFAULT => {
                (&self.default_inputs, self.enabled_default.get())
            }
            _ => (&self.unspecified_inputs, self.enabled_all.get()),
        };
        inputs.borrow_mut().push(input.clone());
        input.do_on_added(dispatcher);
        // What it would have missed.
        input.do_on_history_changed(&self.history.get());
        input.do_on_has_enabled_handlers_changed(enabled.any);
        input.do_on_has_enabled_back_handlers_changed(enabled.back);
        input.do_on_has_enabled_forward_handlers_changed(enabled.forward);
    }

    pub(crate) fn remove_input(&self, input: &NavigationEventInput) {
        self.overlay_inputs.borrow_mut().retain(|other| other != input);
        self.default_inputs.borrow_mut().retain(|other| other != input);
        self.unspecified_inputs.borrow_mut().retain(|other| other != input);
        input.do_on_removed();
    }

    pub(crate) fn dispatch_on_started(
        &self,
        input: &NavigationEventInput,
        direction: i32,
        event: Option<NavigationEvent>,
    ) {
        if self.in_progress_direction.get() != State::TRANSITIONING_UNKNOWN {
            return;
        }
        let handler = self.resolve_enabled_handler(direction);
        // Before it's called: it may remove itself.
        *self.in_progress_handler.borrow_mut() = handler.clone();
        self.in_progress_direction.set(direction);
        *self.in_progress_input.borrow_mut() = Some(input.clone());
        // With an event, a gesture. Without, a press: no start, straight to its end.
        if let Some(event) = event {
            if let Some(handler) = handler {
                match direction {
                    State::TRANSITIONING_BACK => handler.do_on_back_started(&event),
                    State::TRANSITIONING_FORWARD => handler.do_on_forward_started(&event),
                    _ => {}
                }
            }
            self.transition_state.set(State::InProgress { latest_event: event, direction });
        }
    }

    /// Whether a back's end would go to a handler: the one in a gesture, enabled or not, or the
    /// first enabled.
    pub(crate) fn takes_back(&self) -> bool {
        self.in_progress_handler.borrow().is_some()
            || self.resolve_enabled_handler(State::TRANSITIONING_BACK).is_some()
    }

    fn is_in_progress(&self, input: &NavigationEventInput, direction: i32) -> bool {
        self.in_progress_input.borrow().as_ref() == Some(input)
            && self.in_progress_direction.get() == direction
    }

    fn in_progress_or_enabled(&self, direction: i32) -> Option<NavigationEventHandler> {
        let in_progress = self.in_progress_handler.borrow().clone();
        in_progress.or_else(|| self.resolve_enabled_handler(direction))
    }

    pub(crate) fn dispatch_on_progressed(
        &self,
        input: &NavigationEventInput,
        direction: i32,
        event: NavigationEvent,
    ) {
        if !self.is_in_progress(input, direction) {
            return;
        }
        if let Some(handler) = self.in_progress_or_enabled(direction) {
            match direction {
                State::TRANSITIONING_BACK => handler.do_on_back_progressed(&event),
                State::TRANSITIONING_FORWARD => handler.do_on_forward_progressed(&event),
                _ => {}
            }
        }
        self.transition_state.set(State::InProgress { latest_event: event, direction });
    }

    pub(crate) fn dispatch_on_completed(
        &self,
        input: &NavigationEventInput,
        direction: i32,
        on_back_fallback: Option<OnBackCompletedFallback>,
        on_forward_fallback: Option<OnForwardCompletedFallback>,
    ) {
        if !self.is_in_progress(input, direction) {
            return;
        }
        let handler = self.in_progress_or_enabled(direction);
        self.clear_in_progress();
        match (direction, handler) {
            (State::TRANSITIONING_BACK, Some(handler)) => handler.do_on_back_completed(),
            (State::TRANSITIONING_BACK, None) => {
                if let Some(fallback) = on_back_fallback {
                    fallback();
                }
            }
            (State::TRANSITIONING_FORWARD, Some(handler)) => handler.do_on_forward_completed(),
            (State::TRANSITIONING_FORWARD, None) => {
                if let Some(fallback) = on_forward_fallback {
                    fallback();
                }
            }
            _ => {}
        }
        self.transition_state.set(State::Idle);
    }

    pub(crate) fn dispatch_on_cancelled(&self, input: &NavigationEventInput, direction: i32) {
        if !self.is_in_progress(input, direction) {
            return;
        }
        let handler = self.in_progress_or_enabled(direction);
        self.clear_in_progress();
        if let Some(handler) = handler {
            match direction {
                State::TRANSITIONING_BACK => handler.do_on_back_cancelled(),
                State::TRANSITIONING_FORWARD => handler.do_on_forward_cancelled(),
                _ => {}
            }
        }
        self.transition_state.set(State::Idle);
    }

    fn resolve_enabled_handler(&self, direction: i32) -> Option<NavigationEventHandler> {
        let takes = |handler: &NavigationEventHandler| match direction {
            State::TRANSITIONING_BACK => handler.is_back_enabled(),
            State::TRANSITIONING_FORWARD => handler.is_forward_enabled(),
            _ => handler.is_back_enabled() || handler.is_forward_enabled(),
        };
        let overlay = self.overlay_handlers.borrow();
        if let Some((_, handler)) = overlay.iter().find(|(_, handler)| takes(handler)) {
            return Some(handler.clone());
        }
        self.default_handlers.borrow().iter().find(|handler| takes(handler)).cloned()
    }

    fn resolve_combined_back_info(&self) -> Vec<NavigationEventInfo> {
        let mut combined = Vec::new();
        for (_, handler) in self.overlay_handlers.borrow().iter() {
            if handler.is_back_enabled() {
                combined.extend(handler.back_info());
            }
        }
        for handler in self.default_handlers.borrow().iter() {
            if handler.is_back_enabled() {
                combined.extend(handler.back_info());
            }
        }
        combined
    }
}
