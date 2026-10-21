//! `Modifier.draggable`, as Compose's: drags along one axis reported as deltas to a
//! `DraggableState`.

use std::rc::Rc;

use compose_runtime::{ComposeArg, composable};
use compose_ui::{Modifier, PointerEventPass, PointerEventType};

/// An axis, as Compose's `Orientation`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

impl ComposeArg for Orientation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Where a drag's deltas go, as Compose's `DraggableState`.
#[derive(Clone)]
pub struct DraggableState {
    on_delta: Rc<dyn Fn(f32)>,
}

impl DraggableState {
    /// A state calling `on_delta` with each delta (pixels along the axis), as
    /// `DraggableState(onDelta)`.
    pub fn new(on_delta: impl Fn(f32) + 'static) -> DraggableState {
        DraggableState { on_delta: Rc::new(on_delta) }
    }

    /// Drags by `delta` pixels, as `dispatchRawDelta`.
    pub fn dispatch_raw_delta(&self, delta: f32) {
        (self.on_delta)(delta)
    }
}

impl ComposeArg for DraggableState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.on_delta, &other.on_delta)
    }
}

/// A draggable state remembered at its call site, as `rememberDraggableState(onDelta)`: the
/// newest `on_delta` is called.
#[composable(non_restartable)]
pub fn remember_draggable_state(on_delta: impl Fn(f32) + 'static) -> DraggableState {
    let latest = compose_runtime::remember(|| {
        Rc::new(std::cell::RefCell::new(None::<compose_runtime::Callback<dyn Fn(f32)>>))
    });
    *latest.borrow_mut() = Some(on_delta);
    compose_runtime::remember(move || {
        let latest = latest.clone();
        DraggableState::new(move |delta| {
            let on_delta = latest.borrow().clone();
            if let Some(on_delta) = on_delta {
                on_delta(delta);
            }
        })
    })
}

/// Drags along `orientation` are told to `state` as deltas, as `Modifier.draggable`: a press
/// that moves past the touch slop along the axis starts one (`on_drag_started` with where it
/// was pressed), the moves after it (consumed) are its deltas (negated if
/// `reverse_direction`), and the release ends it (`on_drag_stopped`).
pub trait DraggableModifier {
    #[allow(clippy::too_many_arguments)]
    fn draggable(
        self,
        state: DraggableState,
        orientation: Orientation,
        enabled: bool,
        reverse_direction: bool,
        on_drag_started: impl Fn(compose_ui::Offset) + 'static,
        on_drag_stopped: impl Fn() + 'static,
    ) -> Modifier;
}

impl DraggableModifier for Modifier {
    fn draggable(
        self,
        state: DraggableState,
        orientation: Orientation,
        enabled: bool,
        reverse_direction: bool,
        on_drag_started: impl Fn(compose_ui::Offset) + 'static,
        on_drag_stopped: impl Fn() + 'static,
    ) -> Modifier {
        if !enabled {
            return self;
        }
        let (on_drag_started, on_drag_stopped) =
            (Rc::new(on_drag_started), Rc::new(on_drag_stopped));
        self.pointer_input((state.clone(), orientation, reverse_direction), move |scope| {
            let (state, on_drag_started, on_drag_stopped) =
                (state.clone(), on_drag_started.clone(), on_drag_stopped.clone());
            async move {
                let along = |offset: compose_ui::Offset| match orientation {
                    Orientation::Horizontal => offset.x,
                    Orientation::Vertical => offset.y,
                };
                let sign = if reverse_direction { -1.0 } else { 1.0 };
                loop {
                    let down = scope.await_first_down(false).await;
                    // Past the slop along the axis, or not a drag.
                    let mut last = loop {
                        let change = scope.await_pointer_event(PointerEventPass::Main).await;
                        match change.kind {
                            PointerEventType::Move if !change.is_consumed() => {
                                let moved = along(change.position - down.position);
                                if moved.abs() > scope.touch_slop() {
                                    change.consume();
                                    let (started, position) =
                                        (on_drag_started.clone(), down.position);
                                    scope.run_after_dispatch(move || started(position));
                                    // The slop isn't part of the drag, as Compose's.
                                    let slop = scope.touch_slop() * moved.signum();
                                    let delta = sign * (moved - slop);
                                    let state = state.clone();
                                    scope.run_after_dispatch(move || {
                                        state.dispatch_raw_delta(delta)
                                    });
                                    break Some(change.position);
                                }
                            }
                            PointerEventType::Release | PointerEventType::Move => break None,
                            _ => {}
                        }
                    };
                    while let Some(previous) = last {
                        let change = scope.await_pointer_event(PointerEventPass::Main).await;
                        match change.kind {
                            PointerEventType::Move if !change.is_consumed() => {
                                change.consume();
                                let delta = sign * along(change.position - previous);
                                last = Some(change.position);
                                let state = state.clone();
                                scope.run_after_dispatch(move || state.dispatch_raw_delta(delta));
                            }
                            PointerEventType::Release | PointerEventType::Move => {
                                let stopped = on_drag_stopped.clone();
                                scope.run_after_dispatch(move || stopped());
                                last = None;
                            }
                            _ => {}
                        }
                    }
                }
            }
        })
    }
}
