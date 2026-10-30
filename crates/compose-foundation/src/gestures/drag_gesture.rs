//! The drag detection of `Modifier.draggable` and `Modifier.draggable2D`, as Compose's
//! `DragGestureNode`: a state machine over the pointer events of each pass, whose drags go through
//! a queue to a task of the node's, which holds the state's drag while they last.
//!
//! Compose's event holds a change per pointer; here an event is one pointer's, so the pointers down
//! are kept here, and what Compose reads of the others in its event is read from them (D1458).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::{Poll, Waker};

use compose_runtime::{ComposeArg, CoroutineScope, Job};
use compose_ui::{
    DragInteraction, DragStart, EventContext, IntSize, ModifierNode, MutableInteractionSource,
    NodeContext, Offset, PointerEvent, PointerEventPass, PointerEventType, PointerId,
    PointerInputModifierNode, PointerType, TraversableNode, TraverseKey, Velocity, VelocityTracker,
};

use super::Orientation;
use super::scrollable::LocalBoxFuture;
use crate::mutator::MutationInterrupted;

/// What finds the drag gestures around a node, as `traverseAncestorGestures`.
pub(crate) static DRAG_GESTURES: TraverseKey = TraverseKey::new("drag gesture");

/// A gesture past the slop along an axis, past `HorizontalAngleUpperBounds` for the vertical.
const HORIZONTAL_ANGLE_UPPER_BOUND: f32 = 30.0;
const VERTICAL_ANGLE_UPPER_BOUND: f32 = 90.0;

/// A mouse's slop against a finger's, as `mouseToTouchSlopRatio`.
const MOUSE_TO_TOUCH_SLOP_RATIO: f32 = 0.125 / 18.0;

/// What a drag sends its task, as `DragEvent`.
enum DragEvent {
    Started(Offset),
    Delta(Offset),
    Stopped(Velocity),
    Cancelled,
}

/// How the deltas of a drag ended.
#[derive(Clone, Copy)]
enum Ended {
    Stopped(Velocity),
    Cancelled,
}

/// The node's unlimited channel of drag events.
#[derive(Default)]
struct Queue {
    events: RefCell<VecDeque<DragEvent>>,
    waker: RefCell<Option<Waker>>,
}

impl Queue {
    fn send(&self, event: DragEvent) {
        self.events.borrow_mut().push_back(event);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }

    async fn receive(&self) -> DragEvent {
        std::future::poll_fn(|context| match self.events.borrow_mut().pop_front() {
            Some(event) => Poll::Ready(event),
            None => {
                *self.waker.borrow_mut() = Some(context.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}

/// The deltas of a drag going on, for its `drag`, as Compose's `forEachDelta`.
pub(crate) struct Deltas {
    queue: Rc<Queue>,
    ended: Rc<Cell<Option<Ended>>>,
}

impl Deltas {
    /// The next delta, or `None` once the drag ended.
    pub(crate) async fn next(&self) -> Option<Offset> {
        loop {
            match self.queue.receive().await {
                DragEvent::Delta(delta) => return Some(delta),
                DragEvent::Stopped(velocity) => {
                    self.ended.set(Some(Ended::Stopped(velocity)));
                    return None;
                }
                DragEvent::Cancelled => {
                    self.ended.set(Some(Ended::Cancelled));
                    return None;
                }
                DragEvent::Started(_) => {}
            }
        }
    }
}

/// What a kind of drag does with one, as `DragGestureNode`'s abstract members.
pub(crate) trait DragBehavior: 'static {
    /// The behavior itself, for its element to give it new inputs.
    fn as_any(&self) -> &dyn std::any::Any;
    /// Holds the state's drag, its deltas applied, until they end.
    fn drag(
        self: Rc<Self>,
        deltas: Deltas,
    ) -> LocalBoxFuture<'static, Result<(), MutationInterrupted>>;
    fn on_drag_started(&self, scope: CoroutineScope, position: Offset);
    fn on_drag_stopped(&self, scope: CoroutineScope, velocity: Velocity);
    fn start_drag_immediately(&self) -> bool;
}

/// `GestureState`, for the gestures inside to read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GestureState {
    Idle,
    Waiting,
    Recognized,
}

/// The interaction source of a drag, and the start of the one going on.
#[derive(Default)]
struct Interactions {
    source: Option<MutableInteractionSource>,
    start: Option<DragStart>,
}

impl Interactions {
    fn start(&mut self) {
        self.end(DragInteraction::Cancel);
        if let Some(source) = &self.source {
            let start = DragStart::new();
            source.try_emit(DragInteraction::Start(start));
            self.start = Some(start);
        }
    }

    fn end(&mut self, ended: fn(DragStart) -> DragInteraction) {
        if let Some(start) = self.start.take()
            && let Some(source) = &self.source
        {
            source.try_emit(ended(start));
        }
    }
}

/// `TouchSlopDetector`: how far a pointer went, along `orientation` (`None`: any way).
#[derive(Default)]
struct TouchSlopDetector {
    total: Offset,
}

impl TouchSlopDetector {
    /// What's past the slop, once it's crossed.
    fn post_slop_offset(
        &mut self,
        change: Offset,
        slop: f32,
        orientation: Option<Orientation>,
    ) -> Option<Offset> {
        self.total = self.total + change;
        let total = self.total;
        match orientation {
            None => {
                let distance = total.x.hypot(total.y);
                let past = 1.0 - slop / distance;
                (distance > 0.0 && distance >= slop)
                    .then(|| Offset::new(total.x * past, total.y * past))
            }
            Some(Orientation::Horizontal) => (total.x != 0.0 && total.x.abs() >= slop)
                .then(|| Offset::new(total.x - slop.copysign(total.x), total.y)),
            Some(Orientation::Vertical) => (total.y != 0.0 && total.y.abs() >= slop)
                .then(|| Offset::new(total.x, total.y - slop.copysign(total.y))),
        }
    }
}

/// Where the detection is, as `DragDetectionState`. A press's position is in the node.
#[derive(Clone, Copy)]
enum Detection {
    AwaitDown { await_slop: Option<bool>, consumed_on_initial: bool, seen_initial: bool },
    AwaitTouchSlop { down: Offset, pointer: PointerId, verify_in_final: bool },
    AwaitGesturePickup { down: Offset, pointer: PointerId },
    Dragging { pointer: PointerId, consumed_on_initial: bool, started_immediately: bool },
}

const AWAIT_DOWN: Detection =
    Detection::AwaitDown { await_slop: None, consumed_on_initial: false, seen_initial: false };

/// The node of a drag, as `DragGestureNode`, for `draggable` and `draggable2D` by `behavior`.
pub(crate) struct DragGestureNode {
    pub(crate) behavior: Rc<dyn DragBehavior>,
    enabled: bool,
    orientation: Option<Orientation>,
    interactions: Rc<RefCell<Interactions>>,
    detection: Detection,
    /// The gesture accumulated so far, for its angle.
    accumulator: Offset,
    slop: TouchSlopDetector,
    /// The pointers down, in order.
    pressed: Vec<PointerId>,
    /// Where each pointer was in the window at its last event: the deltas don't count the node's
    /// own moves (a sheet it drags), as Compose's changes compare positions in the same layout.
    last_in_window: Vec<(PointerId, Offset)>,
    trackers: Vec<(PointerId, VelocityTracker)>,
    queue: Rc<Queue>,
    task: Option<Job>,
    /// Whether it has seen pointer events since the last gesture, as
    /// `isListeningForPointerInputEvents`.
    listening_to_pointers: bool,
}

impl DragGestureNode {
    pub(crate) fn new(
        behavior: Rc<dyn DragBehavior>,
        enabled: bool,
        orientation: Option<Orientation>,
        interaction_source: Option<MutableInteractionSource>,
    ) -> DragGestureNode {
        DragGestureNode {
            behavior,
            enabled,
            orientation,
            interactions: Rc::new(RefCell::new(Interactions {
                source: interaction_source,
                start: None,
            })),
            detection: AWAIT_DOWN,
            accumulator: Offset::ZERO,
            slop: TouchSlopDetector::default(),
            pressed: Vec::new(),
            last_in_window: Vec::new(),
            trackers: Vec::new(),
            queue: Rc::default(),
            task: None,
            listening_to_pointers: false,
        }
    }

    /// The new inputs, as `update`: `reset` when what's new ends a drag going on.
    pub(crate) fn update(
        &mut self,
        enabled: bool,
        orientation: Option<Orientation>,
        interaction_source: Option<MutableInteractionSource>,
        mut reset: bool,
    ) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if !enabled {
                self.interactions.borrow_mut().end(DragInteraction::Cancel);
            }
            reset = true;
        }
        let same_source = {
            let interactions = self.interactions.borrow();
            match (&interactions.source, &interaction_source) {
                (None, None) => true,
                (Some(mine), Some(theirs)) => mine.same(theirs),
                _ => false,
            }
        };
        if !same_source {
            let mut interactions = self.interactions.borrow_mut();
            interactions.end(DragInteraction::Cancel);
            interactions.source = interaction_source;
        }
        if self.orientation != orientation {
            self.orientation = orientation;
            reset = true;
        }
        if reset && self.listening_to_pointers {
            self.reset_detection();
        }
    }

    /// The node leaves, as `onDetach`: its task ends with its scope, without a stop.
    pub(crate) fn detach(&mut self) {
        self.task = None;
        self.queue = Rc::default();
        self.interactions.borrow_mut().end(DragInteraction::Cancel);
        self.detection = AWAIT_DOWN;
        self.accumulator = Offset::ZERO;
        self.pressed.clear();
        self.last_in_window.clear();
        self.trackers.clear();
        self.listening_to_pointers = false;
    }

    /// As `gestureState`.
    pub(crate) fn gesture_state(&self) -> GestureState {
        if !self.enabled {
            return GestureState::Idle;
        }
        match self.detection {
            Detection::AwaitDown { seen_initial: true, .. }
            | Detection::AwaitTouchSlop { .. }
            | Detection::AwaitGesturePickup { .. } => GestureState::Waiting,
            Detection::AwaitDown { .. } => GestureState::Idle,
            Detection::Dragging { .. } => GestureState::Recognized,
        }
    }

    /// The gesture stops being tracked, a drag going on cancelled, as `resetDragDetectionState`.
    fn reset_detection(&mut self) {
        self.move_to_await_down();
        if self.task.is_some() {
            self.queue.send(DragEvent::Cancelled);
        }
        self.trackers.clear();
    }

    fn move_to_await_down(&mut self) {
        self.accumulator = Offset::ZERO;
        self.detection = AWAIT_DOWN;
    }

    /// The task taking the drags, in the node's scope, as `startListeningForEvents`.
    fn start_listening(&mut self, context: &mut NodeContext<'_>) {
        if self.task.is_some() {
            return;
        }
        let scope = context.coroutine_scope();
        let (queue, behavior, interactions) =
            (self.queue.clone(), self.behavior.clone(), self.interactions.clone());
        self.task = Some(scope.launch(async move {
            loop {
                let DragEvent::Started(position) = queue.receive().await else { continue };
                interactions.borrow_mut().start();
                behavior.on_drag_started(scope, position);
                let ended = Rc::new(Cell::new(None));
                let deltas = Deltas { queue: queue.clone(), ended: ended.clone() };
                let held = behavior.clone().drag(deltas).await;
                match (held, ended.get()) {
                    (Ok(()), Some(Ended::Stopped(velocity))) => {
                        interactions.borrow_mut().end(DragInteraction::Stop);
                        behavior.on_drag_stopped(scope, velocity);
                    }
                    // Cancelled, or a drag of a higher priority took the state.
                    _ => {
                        interactions.borrow_mut().end(DragInteraction::Cancel);
                        behavior.on_drag_stopped(scope, Velocity::ZERO);
                    }
                }
            }
        }));
    }

    fn send_drag_start(&mut self, context: &mut NodeContext<'_>, position: Offset) {
        self.start_listening(context);
        self.queue.send(DragEvent::Started(position));
    }

    fn send_drag_delta(&mut self, delta: Offset) {
        self.accumulator = self.accumulator + delta;
        self.queue.send(DragEvent::Delta(delta));
    }

    fn send_drag_stopped(&mut self, pointer: PointerId, maximum: f32) {
        let velocity = self
            .trackers
            .iter()
            .find(|(tracked, _)| *tracked == pointer)
            .map_or(Velocity::ZERO, |(_, tracker)| {
                tracker.calculate_velocity_with(Velocity::new(maximum, maximum))
            });
        // `toValidVelocity`.
        let valid = |value: f32| if value.is_nan() { 0.0 } else { value };
        self.queue.send(DragEvent::Stopped(Velocity::new(valid(velocity.x), valid(velocity.y))));
        self.listening_to_pointers = false;
    }

    /// Another pointer down, than the one going.
    fn other_down(&self, pointer: PointerId) -> Option<PointerId> {
        self.pressed.iter().copied().find(|other| *other != pointer)
    }

    fn process_await_down(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        context: &mut NodeContext<'_>,
    ) {
        // `isAnyChangedToDown`: the first of the pointers down starts it.
        if event.kind != PointerEventType::Press || self.other_down(event.pointer).is_some() {
            return;
        }
        let Detection::AwaitDown { await_slop, consumed_on_initial, seen_initial } = self.detection
        else {
            return;
        };
        let await_slop = await_slop.unwrap_or_else(|| !self.behavior.start_drag_immediately());
        let mut consumed_on_initial = consumed_on_initial;
        let mut seen_initial = seen_initial;
        if pass == PointerEventPass::Initial {
            // Not awaiting the slop, it's taken at once.
            if !await_slop {
                event.consume();
                consumed_on_initial = true;
            }
            // It waits for the slop from the first down it sees.
            seen_initial = true;
        }
        self.detection = Detection::AwaitDown {
            await_slop: Some(await_slop),
            consumed_on_initial,
            seen_initial,
        };
        if pass == PointerEventPass::Main {
            if await_slop {
                self.slop.total = Offset::ZERO;
                self.detection = Detection::AwaitTouchSlop {
                    down: event.position,
                    pointer: event.pointer,
                    verify_in_final: false,
                };
            } else if consumed_on_initial {
                self.send_drag_start(context, event.position);
                self.send_drag_delta(Offset::ZERO);
                self.detection = Detection::Dragging {
                    pointer: event.pointer,
                    consumed_on_initial: false,
                    started_immediately: true,
                };
            }
        }
    }

    fn process_await_touch_slop(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        moved: Offset,
        slop: f32,
        context: &mut NodeContext<'_>,
    ) {
        let Detection::AwaitTouchSlop { down, pointer, verify_in_final } = self.detection else {
            return;
        };
        if pass == PointerEventPass::Initial {
            return;
        }
        // Not the pointer it follows: another's event says nothing of it (Compose would look for
        // another only if this one's gone, which a release of its own says, below).
        if event.pointer != pointer {
            return;
        }
        if pass == PointerEventPass::Main {
            if event.consumed {
                // Someone took the gesture: it may be left to pick up.
                self.detection = Detection::AwaitGesturePickup { down, pointer };
            } else if event.kind == PointerEventType::Release {
                // The pointer lifted, look for another.
                match self.other_down(pointer) {
                    None => self.move_to_await_down(),
                    Some(other) => {
                        self.detection =
                            Detection::AwaitTouchSlop { down, pointer: other, verify_in_final };
                    }
                }
            } else if event.kind == PointerEventType::Move {
                let slop = if event.pointer_type == PointerType::Mouse {
                    slop * MOUSE_TO_TOUCH_SLOP_RATIO
                } else {
                    slop
                };
                match self.slop.post_slop_offset(moved, slop, self.orientation) {
                    Some(post_slop) => {
                        self.accumulator = self.accumulator + moved;
                        // The gesture's angle so far: if it's not this node's way while it's the
                        // way of a gesture around that's waiting, that one goes first.
                        let angle =
                            self.accumulator.y.abs().atan2(self.accumulator.x.abs()).to_degrees();
                        let aligned = self.orientation.is_none_or(|own| aligned(own, angle));
                        if !aligned && parent_aligned(context, angle) {
                            self.detection =
                                Detection::AwaitTouchSlop { down, pointer, verify_in_final: true };
                        } else {
                            event.consume();
                            self.send_drag_start(context, event.position - post_slop);
                            self.send_drag_delta(post_slop);
                            self.detection = Detection::Dragging {
                                pointer,
                                consumed_on_initial: false,
                                started_immediately: false,
                            };
                        }
                    }
                    None => {
                        self.detection =
                            Detection::AwaitTouchSlop { down, pointer, verify_in_final: true };
                        self.accumulator = self.accumulator + moved;
                    }
                }
            }
        }
        // A gesture around took it in the Main pass, after this node, or in the Final pass.
        if pass == PointerEventPass::Final
            && let Detection::AwaitTouchSlop { down, pointer, verify_in_final: true } =
                self.detection
        {
            if event.consumed {
                self.detection = Detection::AwaitGesturePickup { down, pointer };
            } else {
                self.detection =
                    Detection::AwaitTouchSlop { down, pointer, verify_in_final: false };
            }
        }
    }

    fn process_await_pickup(&mut self, event: &PointerEvent, pass: PointerEventPass) {
        let Detection::AwaitGesturePickup { down, pointer } = self.detection else { return };
        if pass != PointerEventPass::Final {
            return;
        }
        if self.pressed.is_empty() {
            self.move_to_await_down();
            return;
        }
        // Compose's event holds every pointer, and one consumed keeps it waiting: here that's the
        // pointer followed, or the event's if that one lifted.
        let pointer = if self.pressed.contains(&pointer) { pointer } else { event.pointer };
        if event.pointer == pointer && !event.consumed {
            // Past the slop most likely, from the same down: picked up at this event.
            self.slop.total = event.position - down;
            self.detection = Detection::AwaitTouchSlop { down, pointer, verify_in_final: false };
        }
    }

    fn process_dragging(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        moved: Offset,
        maximum_velocity: f32,
    ) {
        let Detection::Dragging { pointer, consumed_on_initial, started_immediately } =
            self.detection
        else {
            return;
        };
        if event.pointer != pointer {
            return;
        }
        if pass == PointerEventPass::Initial && started_immediately {
            if !event.consumed {
                event.consume();
                self.detection =
                    Detection::Dragging { pointer, consumed_on_initial: true, started_immediately };
            }
            return;
        }
        if pass != PointerEventPass::Main {
            return;
        }
        let consumed_by_other = event.consumed && !consumed_on_initial;
        self.detection =
            Detection::Dragging { pointer, consumed_on_initial: false, started_immediately };
        if event.kind == PointerEventType::Release {
            match self.other_down(pointer) {
                None => {
                    if consumed_by_other {
                        self.queue.send(DragEvent::Cancelled);
                    } else {
                        self.send_drag_stopped(pointer, maximum_velocity);
                    }
                    self.move_to_await_down();
                }
                Some(other) => {
                    self.detection = Detection::Dragging {
                        pointer: other,
                        consumed_on_initial: false,
                        started_immediately,
                    };
                }
            }
        } else if event.kind == PointerEventType::Move {
            if consumed_by_other {
                self.queue.send(DragEvent::Cancelled);
            } else {
                self.send_drag_delta(moved);
                event.consume();
            }
        }
    }

    /// Runs what's queued right after the dispatch, not at the executor's next turn.
    fn poll_task_after_dispatch(&self, context: &mut EventContext<'_>) {
        if let Some(job) = self.task
            && !self.queue.events.borrow().is_empty()
        {
            context.run(move || {
                compose_runtime::platform::poll_now(job);
            });
        }
    }
}

/// Whether a gesture at `angle` degrees (0: sideways, 90: up or down) goes along `orientation`, as
/// `isDragAngleAlignedWithOrientation`.
pub(crate) fn aligned(orientation: Orientation, angle: f32) -> bool {
    match orientation {
        Orientation::Horizontal => angle <= HORIZONTAL_ANGLE_UPPER_BOUND,
        Orientation::Vertical => {
            angle > HORIZONTAL_ANGLE_UPPER_BOUND && angle <= VERTICAL_ANGLE_UPPER_BOUND
        }
    }
}

/// Whether a drag gesture around is waiting, and a gesture at `angle` is its way. A scrollable is
/// one, as Compose's, whose nodes are `DragGestureNode`s; here they have their own detection.
pub(crate) fn parent_aligned(context: &mut NodeContext<'_>, angle: f32) -> bool {
    let mut found = false;
    context.traverse_ancestors_with_key(&DRAG_GESTURES, |node, _| {
        let around: Option<(Option<Orientation>, GestureState)> =
            if let Some(parent) = node.downcast_ref::<DragGestureNode>() {
                Some((parent.orientation, parent.gesture_state()))
            } else if let Some(parent) = node.downcast_ref::<crate::scroll::ScrollNode>() {
                Some(parent.drag_gesture())
            } else if let Some(parent) = node.downcast_ref::<crate::lazy::LazyScrollNode>() {
                Some(parent.drag_gesture())
            } else {
                node.downcast_ref::<super::scrollable::ScrollableNode>()
                    .map(super::scrollable::ScrollableNode::drag_gesture)
            };
        if let Some((orientation, state)) = around {
            found = state == GestureState::Waiting
                && orientation.is_some_and(|parent| aligned(parent, angle));
        }
        !found
    });
    found
}

impl ModifierNode for DragGestureNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
        Some(self)
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.detach();
    }
    fn on_reset(&mut self, _context: &mut NodeContext<'_>) {
        self.detach();
    }
}

impl TraversableNode for DragGestureNode {
    fn traverse_key(&self) -> &'static TraverseKey {
        &DRAG_GESTURES
    }
}

impl PointerInputModifierNode for DragGestureNode {
    /// It waits for a drag past a press, so a clickable inside delays its press.
    fn waits_for_gesture(&self, _press: &PointerEvent) -> bool {
        self.enabled
    }

    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        self.listening_to_pointers = true;
        if !self.enabled
            || !matches!(
                event.kind,
                PointerEventType::Press | PointerEventType::Move | PointerEventType::Release
            )
        {
            return;
        }
        // A pointer that isn't down isn't dragging (a mouse moving over it).
        if event.kind == PointerEventType::Move && !self.pressed.contains(&event.pointer) {
            return;
        }
        let in_window = context.pointer_position().unwrap_or(event.position);
        let last = self.last_in_window.iter().find(|(pointer, _)| *pointer == event.pointer);
        let moved = if event.kind == PointerEventType::Move {
            last.map_or(Offset::ZERO, |&(_, at)| in_window - at)
        } else {
            Offset::ZERO
        };
        if pass == PointerEventPass::Initial {
            match event.kind {
                PointerEventType::Press => {
                    if !self.pressed.contains(&event.pointer) {
                        self.pressed.push(event.pointer);
                    }
                }
                PointerEventType::Release => self.pressed.retain(|down| *down != event.pointer),
                _ => {}
            }
        }
        let time_millis = (crate::scroll::event_time(event) / 1_000_000) as i64;
        // Each pointer's velocity, from its first event, as `preProcessVelocity`.
        if pass == PointerEventPass::Main {
            if self.trackers.iter().all(|(tracked, _)| *tracked != event.pointer) {
                self.trackers.push((event.pointer, VelocityTracker::new()));
            }
            if let Some((_, tracker)) =
                self.trackers.iter_mut().find(|(tracked, _)| *tracked == event.pointer)
            {
                tracker.add_pointer_input_change(
                    time_millis,
                    in_window,
                    event.kind == PointerEventType::Press,
                    event.kind == PointerEventType::Release,
                );
            }
        }
        let configuration = context.view_configuration();
        match self.detection {
            Detection::AwaitDown { .. } => self.process_await_down(event, pass, context),
            Detection::AwaitTouchSlop { .. } => {
                self.process_await_touch_slop(event, pass, moved, configuration.touch_slop, context)
            }
            Detection::AwaitGesturePickup { .. } => self.process_await_pickup(event, pass),
            Detection::Dragging { .. } => {
                self.process_dragging(event, pass, moved, configuration.maximum_fling_velocity)
            }
        }
        if pass == PointerEventPass::Main && event.kind == PointerEventType::Release {
            self.trackers.retain(|(tracked, _)| *tracked != event.pointer);
        }
        if pass == PointerEventPass::Final {
            self.last_in_window.retain(|(pointer, _)| *pointer != event.pointer);
            if event.kind != PointerEventType::Release {
                self.last_in_window.push((event.pointer, in_window));
            }
        }
        self.poll_task_after_dispatch(context);
    }

    fn on_cancel_pointer_input(&mut self) {
        self.pressed.clear();
        self.last_in_window.clear();
        if self.listening_to_pointers {
            self.reset_detection();
        }
        self.listening_to_pointers = false;
    }
}
