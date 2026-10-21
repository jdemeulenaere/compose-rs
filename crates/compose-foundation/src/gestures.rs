//! `Modifier.draggable`, as Compose's: drags along one axis reported as deltas to a
//! `DraggableState`.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{ComposeArg, composable};
use compose_ui::{
    Axis, EventContext, IntSize, Modifier, ModifierNode, ModifierNodeElement, Offset, PointerEvent,
    PointerEventPass, PointerEventType, PointerInputDriver, PointerInputModifierNode,
    PointerInputScope,
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
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        let callbacks = Rc::new(RefCell::new(self.callbacks.clone()));
        std::boxed::Box::new(DraggableNode {
            orientation: self.orientation,
            driver: drag_driver(self.orientation, callbacks.clone()),
            callbacks,
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<DraggableNode>() {
            let restart = node.orientation != self.orientation
                || !node.callbacks.borrow().state.same(&self.callbacks.state);
            *node.callbacks.borrow_mut() = self.callbacks.clone();
            if restart {
                node.orientation = self.orientation;
                node.driver.stop();
                node.driver = drag_driver(self.orientation, node.callbacks.clone());
            }
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<DraggableElement>().is_some_and(|other| {
            let (mine, theirs) = (&self.callbacks, &other.callbacks);
            other.orientation == self.orientation
                && theirs.state.same(&mine.state)
                && theirs.reverse_direction == mine.reverse_direction
                && Rc::ptr_eq(&theirs.on_drag_started, &mine.on_drag_started)
                && Rc::ptr_eq(&theirs.on_drag_stopped, &mine.on_drag_stopped)
        })
    }
    fn name(&self) -> &'static str {
        "draggable"
    }
}

impl ModifierNode for DraggableNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn on_detach(&mut self, _context: &mut compose_ui::DetachContext<'_>) {
        self.driver.stop();
    }
}

impl PointerInputModifierNode for DraggableNode {
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
            compose_ui::pointer_input::run_soon(move || stopped(0.0));
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
                let mut last = start.position;
                let mut tracker = VelocityTracker::default();
                tracker.add(start.uptime_millis, along(start.position));
                let velocity = loop {
                    let change = scope.await_pointer_event(PointerEventPass::Main).await;
                    if change.id != down.id {
                        continue;
                    }
                    match change.kind {
                        PointerEventType::Move if !change.is_consumed() => {
                            change.consume();
                            delta(&callbacks, along(change.position - last));
                            last = change.position;
                            tracker.add(change.uptime_millis, along(change.position));
                        }
                        PointerEventType::Release => {
                            tracker.add(change.uptime_millis, along(change.position));
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

/// The pointer's speed along the drag's axis at its release, in pixels per second, from its
/// moves in the last 100 ms, as Compose's `VelocityTracker` (a straight line through them,
/// here).
#[derive(Default)]
struct VelocityTracker {
    samples: std::collections::VecDeque<(u64, f32)>,
}

impl VelocityTracker {
    /// How far back the moves count, as Compose's `HorizonMilliseconds`.
    const HORIZON_MILLIS: u64 = 100;

    fn add(&mut self, millis: u64, position: f32) {
        self.samples.push_back((millis, position));
        while self.samples.front().is_some_and(|(at, _)| millis - at > Self::HORIZON_MILLIS) {
            self.samples.pop_front();
        }
    }

    fn velocity(&self) -> f32 {
        // Least squares over the window: the slope of position over time.
        let count = self.samples.len() as f32;
        if self.samples.len() < 2 {
            return 0.0;
        }
        let first = self.samples[0].0;
        let times: Vec<f32> =
            self.samples.iter().map(|(at, _)| (at - first) as f32 / 1000.0).collect();
        let mean_time = times.iter().sum::<f32>() / count;
        let mean_position = self.samples.iter().map(|(_, at)| at).sum::<f32>() / count;
        let (mut covariance, mut variance) = (0.0, 0.0);
        for (time, (_, position)) in times.iter().zip(&self.samples) {
            covariance += (time - mean_time) * (position - mean_position);
            variance += (time - mean_time) * (time - mean_time);
        }
        if variance <= f32::EPSILON { 0.0 } else { covariance / variance }
    }
}
