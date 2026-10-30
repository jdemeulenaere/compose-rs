//! Compose's `foundation.gestures`: `Modifier.draggable` (drags along one axis reported as
//! deltas to a `DraggableState`), and scrolling (`scrollable`: `ScrollableState`,
//! `Modifier.scrollable`, flings).

mod mouse_wheel;
mod scrollable;

pub(crate) use mouse_wheel::MouseWheel;
pub use scrollable::{
    DefaultFlingBehavior, DefaultScrollableState, FlingBehavior, LocalBoxFuture, ScrollBlock,
    ScrollScope, ScrollableDefaults, ScrollableModifier, ScrollableState, ScrollableStateExt,
    remember_scrollable_state, scrollable_state,
};
pub(crate) use scrollable::{DragSession, ScrollCore};

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{ComposeArg, composable};
use compose_ui::{
    Axis, EventContext, IntSize, Modifier, ModifierNode, ModifierNodeElement, Offset, PointerEvent,
    PointerEventPass, PointerEventType, PointerInputModifierNode, PointerInputScope,
    platform::PointerInputDriver,
};

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
/// crossed the slop), with the movement past the slop as its first delta; the moves of its pointer
/// after it (consumed) are its deltas (negated if `reverse_direction`), and the release ends it
/// (`on_drag_stopped` with its velocity along the axis, in pixels per second; also called, with
/// 0, when a parent takes the moves or the gesture is cancelled).
/// A parent that takes the moves before the slop is crossed keeps the gesture. The newest
/// callbacks are called; a new `state` or `orientation` starts the detection over.
pub trait DraggableModifier {
    #[allow(clippy::too_many_arguments)]
    fn draggable(
        self,
        state: DraggableState,
        orientation: Orientation,
        enabled: bool,
        reverse_direction: bool,
        on_drag_started: impl Fn(Offset) + 'static,
        on_drag_stopped: impl Fn(f32) + 'static,
    ) -> Modifier;
}

impl DraggableModifier for Modifier {
    fn draggable(
        self,
        state: DraggableState,
        orientation: Orientation,
        enabled: bool,
        reverse_direction: bool,
        on_drag_started: impl Fn(Offset) + 'static,
        on_drag_stopped: impl Fn(f32) + 'static,
    ) -> Modifier {
        if !enabled {
            return self;
        }
        self.then_custom(DraggableElement {
            orientation,
            callbacks: DragCallbacks {
                state,
                reverse_direction,
                on_drag_started: Rc::new(on_drag_started),
                on_drag_stopped: Rc::new(on_drag_stopped),
            },
        })
    }
}

/// What a drag calls: the node's newest, read when called.
#[derive(Clone)]
struct DragCallbacks {
    state: DraggableState,
    reverse_direction: bool,
    on_drag_started: Rc<dyn Fn(Offset)>,
    on_drag_stopped: Rc<dyn Fn(f32)>,
}

struct DraggableElement {
    orientation: Orientation,
    callbacks: DragCallbacks,
}

struct DraggableNode {
    orientation: Orientation,
    callbacks: Rc<RefCell<DragCallbacks>>,
    driver: PointerInputDriver,
}

impl ModifierNodeElement for DraggableElement {
    type Node = DraggableNode;
    fn create(&self) -> DraggableNode {
        let callbacks = Rc::new(RefCell::new(self.callbacks.clone()));
        DraggableNode {
            orientation: self.orientation,
            driver: drag_driver(self.orientation, callbacks.clone()),
            callbacks,
        }
    }
    fn update(&self, node: &mut DraggableNode) {
        let restart = node.orientation != self.orientation
            || !node.callbacks.borrow().state.same(&self.callbacks.state);
        *node.callbacks.borrow_mut() = self.callbacks.clone();
        if restart {
            node.orientation = self.orientation;
            node.driver.stop();
            node.driver = drag_driver(self.orientation, node.callbacks.clone());
        }
    }
    fn same(&self, other: &Self) -> bool {
        let (mine, theirs) = (&self.callbacks, &other.callbacks);
        other.orientation == self.orientation
            && theirs.state.same(&mine.state)
            && theirs.reverse_direction == mine.reverse_direction
            && Rc::ptr_eq(&theirs.on_drag_started, &mine.on_drag_started)
            && Rc::ptr_eq(&theirs.on_drag_stopped, &mine.on_drag_stopped)
    }
    fn name(&self) -> &'static str {
        "draggable"
    }
}

impl ModifierNode for DraggableNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, _context: &mut compose_ui::ReleaseContext<'_>) {
        self.driver.stop();
    }
    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.driver.stop();
    }
}

impl PointerInputModifierNode for DraggableNode {
    /// It waits for a drag past a press, so a clickable inside delays its press.
    fn waits_for_gesture(&self, _press: &PointerEvent) -> bool {
        true
    }

    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        self.driver.on_pointer_event(event, pass, bounds, context);
    }
    fn on_cancel_pointer_input(&mut self) {
        self.driver.stop();
    }
}

/// Calls `on_drag_stopped` if dropped mid-drag (the gesture cancelled from outside).
struct StopOnDrop {
    callbacks: Rc<RefCell<DragCallbacks>>,
    armed: bool,
}

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let stopped = self.callbacks.borrow().on_drag_stopped.clone();
            compose_ui::platform::run_soon(move || stopped(0.0));
        }
    }
}

/// The drag detection, as Compose's `DragGestureNode`.
fn drag_driver(
    orientation: Orientation,
    callbacks: Rc<RefCell<DragCallbacks>>,
) -> PointerInputDriver {
    PointerInputDriver::new(move |scope: PointerInputScope| {
        let callbacks = callbacks.clone();
        async move {
            let (axis, along): (Axis, fn(Offset) -> f32) = match orientation {
                Orientation::Horizontal => (Axis::X, |offset| offset.x),
                Orientation::Vertical => (Axis::Y, |offset| offset.y),
            };
            let delta = |callbacks: &Rc<RefCell<DragCallbacks>>, amount: f32| {
                let callbacks = callbacks.clone();
                scope.run_after_dispatch(move || {
                    let callbacks = callbacks.borrow().clone();
                    let sign = if callbacks.reverse_direction { -1.0 } else { 1.0 };
                    callbacks.state.dispatch_raw_delta(sign * amount);
                });
            };
            loop {
                let down = scope.await_first_down(false).await;
                let Some((start, over)) =
                    scope.await_touch_slop_or_cancellation(&down, Some(axis)).await
                else {
                    continue;
                };
                let started = callbacks.borrow().on_drag_started.clone();
                // Where the slop was crossed, as Compose's `DragStarted`.
                let position = start.position - over;
                scope.run_after_dispatch(move || started(position));
                let mut guard = StopOnDrop { callbacks: callbacks.clone(), armed: true };
                delta(&callbacks, along(over));
                // Where the pointer is along the drag, from its moves (the node moving under it, a
                // sheet it drags, doesn't count), as Compose's `positionChange()`.
                let mut at = start.position;
                let mut tracker = crate::fling::VelocityTracker::default();
                tracker.press(down.uptime_millis * 1_000_000, along(down.position));
                tracker.add(start.uptime_millis * 1_000_000, along(start.position));
                let velocity = loop {
                    let change = scope.await_pointer_event(PointerEventPass::Main).await;
                    if change.id != down.id {
                        continue;
                    }
                    match change.kind {
                        PointerEventType::Move if !change.is_consumed() => {
                            change.consume();
                            let moved = change.position_change();
                            delta(&callbacks, along(moved));
                            at = at + moved;
                            tracker.add(change.uptime_millis * 1_000_000, along(at));
                        }
                        PointerEventType::Release => {
                            tracker.release(change.uptime_millis * 1_000_000);
                            break tracker.velocity();
                        }
                        PointerEventType::Move => break 0.0,
                        _ => {}
                    }
                };
                guard.armed = false;
                let stopped = callbacks.borrow().on_drag_stopped.clone();
                let sign = if callbacks.borrow().reverse_direction { -1.0 } else { 1.0 };
                scope.run_after_dispatch(move || stopped(sign * velocity));
            }
        }
    })
}
