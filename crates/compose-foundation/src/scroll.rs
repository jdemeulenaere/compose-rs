//! Scrolling, as Compose's `Modifier.verticalScroll(state)` and `horizontalScroll(state)`: the
//! content measured without a bound along the axis, clipped, and moved by the scroll offset,
//! which the wheel and drags change.
//!
//! The offset is state: the node reads it while measuring (observed, so a write by the
//! application scrolls), and writes it when the wheel or a drag scrolls.

use std::rc::Rc;

use compose_runtime::{
    ComposeArg, CoroutineScope, Job, MutableState, ObjectHandle, Owned, OwnedState, composable,
    mutable_state_of, with_frame_nanos,
};
use compose_ui::{
    Constraints, DrawModifierNode, DrawScope, EventContext, IntSize, LayoutModifierNode,
    Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement,
    NestedScrollConnection, NestedScrollSource, Offset, Outline, PointerEvent, PointerEventPass,
    PointerEventType, PointerId, PointerInputModifierNode, Rect, platform::NestedScrollChain,
};

/// How far content is scrolled, in pixels, and how far it can be, as Compose's `ScrollState`:
/// state, so reading it in composition recomposes on scrolls, and setting it scrolls.
///
/// A `Copy` handle of its states (D240): [`new`](Self::new) returns its owner, and
/// [`remember_scroll_state`] keeps that in the composition. Once its owner went, it reads 0
/// and ignores writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScrollState {
    value: MutableState<i32>,
    max: MutableState<i32>,
    /// The viewport's length along the axis, as last laid out (for scrollbars).
    viewport: MutableState<i32>,
    /// Their owner, to say when they're used after it went.
    object: ObjectHandle<ScrollStates>,
}

/// What a `ScrollState`'s owner keeps.
struct ScrollStates {
    value: OwnedState<i32>,
    max: OwnedState<i32>,
    viewport: OwnedState<i32>,
}

impl ScrollState {
    #[track_caller]
    pub fn new(initial: i32) -> Owned<ScrollState> {
        let states = ScrollStates {
            value: mutable_state_of(initial.max(0)),
            max: mutable_state_of(i32::MAX),
            viewport: mutable_state_of(0),
        };
        let (value, max, viewport) =
            (states.value.handle(), states.max.handle(), states.viewport.handle());
        Owned::new(states, move |object| ScrollState { value, max, viewport, object })
    }

    /// `state`'s value, or `default` once the owner went (D240).
    fn read(&self, state: MutableState<i32>, default: i32) -> i32 {
        state.try_get().unwrap_or_else(|| {
            self.object.warn_if_freed();
            default
        })
    }

    /// Writes `state`, unless the owner went (D240).
    fn write(&self, state: MutableState<i32>, value: i32) {
        if !state.try_set(value) {
            self.object.warn_if_freed();
        }
    }

    /// The scroll offset, in pixels.
    pub fn value(&self) -> i32 {
        self.read(self.value, 0)
    }

    /// The largest offset, known once laid out.
    pub fn max_value(&self) -> i32 {
        self.read(self.max, 0)
    }

    /// The viewport's length along the scroll's axis, in pixels, as `viewportSize` (a state
    /// read; 0 until laid out).
    pub fn viewport_size(&self) -> i32 {
        self.read(self.viewport, 0)
    }

    /// Scrolls to `value` (clamped to the content at the next layout), as `scrollTo`, without
    /// animation.
    pub fn scroll_to(&self, value: i32) {
        self.write(self.value, value.max(0));
    }
}

impl ComposeArg for ScrollState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A scroll state kept across recompositions and saved across the host's recreation, as
/// `rememberScrollState()` (a `rememberSaveable` of `ScrollState.Saver`).
#[composable(non_restartable)]
pub fn remember_scroll_state(#[default(0)] initial: i32) -> ScrollState {
    let saver = compose_runtime::saver(
        |state: &Owned<ScrollState>| {
            Some(compose_runtime::SavedValue::Int(i64::from(state.value())))
        },
        |saved| match saved {
            compose_runtime::SavedValue::Int(value) => Some(ScrollState::new(*value as i32)),
            _ => None,
        },
    );
    compose_runtime::remember_saveable_with(saver, || ScrollState::new(initial))
}

/// The scrolling modifiers, on `Modifier`.
pub trait ScrollModifiers {
    /// Scrolls the content vertically, as `Modifier.verticalScroll(state)`.
    fn vertical_scroll(self, state: ScrollState) -> Modifier;
    /// Scrolls the content horizontally, as `Modifier.horizontalScroll(state)`.
    fn horizontal_scroll(self, state: ScrollState) -> Modifier;
}

impl ScrollModifiers for Modifier {
    fn vertical_scroll(self, state: ScrollState) -> Modifier {
        self.then_custom(ScrollElement { state, vertical: true })
    }

    fn horizontal_scroll(self, state: ScrollState) -> Modifier {
        self.then_custom(ScrollElement { state, vertical: false })
    }
}

struct ScrollElement {
    state: ScrollState,
    vertical: bool,
}

impl ModifierNodeElement for ScrollElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(ScrollNode {
            state: self.state,
            vertical: self.vertical,
            drag: DragScroller::default(),
            fraction: 0.0,
            laid_out: 0,
            rtl: false,
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<ScrollNode>() {
            // A fling scrolls the state it started with: a new state stops it (the old one's
            // owner may be going, D240).
            if node.state != self.state {
                node.drag.stop_fling();
            }
            node.state = self.state;
            node.vertical = self.vertical;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other
            .downcast_ref::<ScrollElement>()
            .is_some_and(|other| other.vertical == self.vertical && other.state.same(&self.state))
    }

    fn name(&self) -> &'static str {
        "scroll"
    }
}

/// A drag in progress: the pointer, where it last was, and whether it moved past the touch slop
/// (then it scrolls, and clicks under it are cancelled).
impl Drag {
    fn new(event: &PointerEvent, position: Offset, along: f32) -> Drag {
        let mut tracker = crate::fling::VelocityTracker::default();
        tracker.add(event_time(event), along);
        Drag {
            pointer: event.pointer,
            start: position,
            last: position,
            scrolling: false,
            tracker,
            touch: event.pointer_type == compose_ui::PointerType::Touch,
        }
    }
}

struct Drag {
    pointer: PointerId,
    /// Where it was pressed.
    start: Offset,
    last: Offset,
    scrolling: bool,
    /// Its speed, for the fling at its release.
    tracker: crate::fling::VelocityTracker,
    touch: bool,
}

/// When an event happened (its time, else now), for velocities.
fn event_time(event: &PointerEvent) -> u64 {
    if event.time_nanos != 0 { event.time_nanos } else { compose_runtime::platform::now_nanos() }
}

/// The fastest a fling starts, as Android's `ViewConfiguration.getScaledMaximumFlingVelocity`
/// (8,000 dp/s), and the slowest that flings (50 dp/s).
const MAXIMUM_FLING_DP: f32 = 8000.0;
const MINIMUM_FLING_DP: f32 = 50.0;

/// Scrolls with `scroll_by` (pixels towards the end; returns those used) along a fling starting
/// at `velocity` (pixels per second), frame by frame, until its distance is covered or an edge
/// stops it, as Compose's default fling behaviour.
///
/// `on_end` gets the velocity left when it ends (none once its distance is covered; what an edge
/// stopped).
fn fling(
    velocity: f32,
    density: f32,
    scroll_by: Rc<dyn Fn(f32) -> f32>,
    on_end: impl FnOnce(f32) + 'static,
) -> Job {
    let course = crate::fling::Fling::new(velocity, density);
    CoroutineScope::new().launch(async move {
        let start = with_frame_nanos(|time| time).await;
        let mut covered = 0.0f32;
        loop {
            let now = with_frame_nanos(|time| time).await;
            let fraction = if course.duration_nanos == 0 {
                1.0
            } else {
                now.saturating_sub(start) as f32 / course.duration_nanos as f32
            };
            let target = course.distance_at(fraction);
            let delta = target - covered;
            covered = target;
            let used = if delta == 0.0 { 0.0 } else { scroll_by(delta) };
            // An edge took less than asked: the fling ends there.
            if fraction >= 1.0 {
                on_end(0.0);
                break;
            }
            if (delta - used).abs() > 0.5 {
                // Its velocity there: the spline's slope (it slows down fast), as Compose's
                // `velocityLeft`.
                let step = 0.01f32.min(fraction);
                let seconds = course.duration_nanos as f32 / 1e9;
                let slope = (course.distance_at(fraction) - course.distance_at(fraction - step))
                    / (step * seconds).max(f32::EPSILON);
                on_end(slope);
                break;
            }
        }
    })
}

/// Scrolls by `delta` (pixels towards the end, before a right-to-left reversal) through the
/// nested-scroll `chain`, as Compose's scrollables with their `NestedScrollDispatcher`: the
/// connections around first, then `scroll_by`, then the connections again with what's left;
/// returns what all of them used.
pub(crate) fn dispatch_scroll(
    chain: &NestedScrollChain,
    vertical: bool,
    delta: f32,
    source: NestedScrollSource,
    scroll_by: &mut dyn FnMut(f32) -> f32,
) -> f32 {
    if chain.is_empty() {
        return scroll_by(delta);
    }
    // The connections' offsets are the pointer's way: the content's scroll's opposite.
    let axis =
        |value: f32| if vertical { Offset::new(0.0, value) } else { Offset::new(value, 0.0) };
    let along = |offset: Offset| if vertical { offset.y } else { offset.x };
    let before = along(chain.pre_scroll(axis(-delta), source));
    let left = delta + before;
    let used = if left == 0.0 { 0.0 } else { scroll_by(left) };
    let after = along(chain.post_scroll(axis(-used), axis(-(left - used)), source));
    used - before - after
}

/// The distance a pointer moves before a drag scrolls, as Compose's touch slop (8 dp), in
/// pixels.
pub(crate) fn touch_slop(context: &EventContext<'_>) -> f32 {
    context.density().to_px(compose_ui::Dp(8.0))
}

struct ScrollNode {
    state: ScrollState,
    vertical: bool,
    drag: DragScroller,
    /// The fraction of a pixel scrolled but not yet shown.
    fraction: f32,
    /// The offset the last layout used.
    laid_out: i32,
    /// Horizontal in a right-to-left layout: its content starts at the right.
    rtl: bool,
}

impl compose_ui::SemanticsModifierNode for ScrollNode {
    /// Its offset and the largest, along its axis, as `Modifier.verticalScroll`'s semantics:
    /// accessibility services scroll it a page at a time (D478).
    fn apply_semantics(&self, config: &mut compose_ui::SemanticsConfiguration) {
        let range = compose_ui::ScrollAxisRange {
            value: self.state.value() as f32,
            max_value: self.state.max_value() as f32,
            reverse_scrolling: false,
        };
        if self.vertical {
            config.vertical_scroll_axis_range = Some(range);
        } else {
            config.horizontal_scroll_axis_range = Some(range);
        }
    }

    fn perform_action(
        &mut self,
        action: compose_ui::SemanticsAction,
        _context: &mut compose_ui::EventContext<'_>,
    ) -> bool {
        let compose_ui::SemanticsAction::ScrollBy { x, y } = action else { return false };
        let delta = if self.vertical { y } else { x };
        delta != 0.0 && scroll_state_by(&self.state, delta, &mut self.fraction) != 0.0
    }
}

impl ModifierNode for ScrollNode {
    fn as_semantics(&mut self) -> Option<&mut dyn compose_ui::SemanticsModifierNode> {
        Some(self)
    }
    fn on_detach(&mut self, _context: &mut compose_ui::DetachContext<'_>) {
        self.drag.stop_fling();
    }
    fn on_reset(&mut self) {
        self.drag.cancel();
        self.drag.stop_fling();
    }

    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn hit_clip(&self) -> Option<compose_ui::Shape> {
        Some(compose_ui::Shape::Rectangle)
    }
    /// Scrolls the least that shows `rect` (its start, if it's longer than the viewport).
    fn bring_into_view(&mut self, rect: Rect, viewport: IntSize) -> Option<Rect> {
        let (start, end, length) = scroll_axis(rect, viewport, self.vertical, self.rtl);
        // Where it will be once the scrolling asked for (not yet laid out) is.
        let pending = (self.state.value() - self.laid_out) as f32;
        let delta = bring_into_view_delta(start - pending, end - pending, length);
        let used = scroll_state_by(&self.state, delta, &mut self.fraction);
        Some(scrolled_rect(rect, used, self.vertical, self.rtl))
    }
}

/// `rect`'s start and end along a scroll's axis, from where its content starts (the far end
/// when `reversed`: the right of a right-to-left horizontal scroll, the bottom of a reversed
/// lazy column), and the viewport's length.
pub(crate) fn scroll_axis(
    rect: Rect,
    viewport: IntSize,
    vertical: bool,
    reversed: bool,
) -> (f32, f32, f32) {
    let (start, end, length) = if vertical {
        (rect.top, rect.bottom, viewport.height as f32)
    } else {
        (rect.left, rect.right, viewport.width as f32)
    };
    if reversed { (length - end, length - start, length) } else { (start, end, length) }
}

/// `rect` once its scroll moved `used` pixels forward.
pub(crate) fn scrolled_rect(rect: Rect, used: f32, vertical: bool, reversed: bool) -> Rect {
    let used = if reversed { -used } else { used };
    if vertical {
        Rect::new(rect.left, rect.top - used, rect.right, rect.bottom - used)
    } else {
        Rect::new(rect.left - used, rect.top, rect.right - used, rect.bottom)
    }
}

/// How far to scroll so `[start, end)` shows in `[0, length)`: none if it does, else the least,
/// its start first if it's longer.
pub(crate) fn bring_into_view_delta(start: f32, end: f32, length: f32) -> f32 {
    if start >= 0.0 && end <= length {
        0.0
    } else if start < 0.0 || end - start > length {
        start
    } else {
        end - length
    }
}

impl LayoutModifierNode for ScrollNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let content_constraints = if self.vertical {
            Constraints::new(constraints.min_width, constraints.max_width, 0, Constraints::INFINITY)
        } else {
            Constraints::new(
                0,
                Constraints::INFINITY,
                constraints.min_height,
                constraints.max_height,
            )
        };
        let placeable = scope.measure(content, content_constraints);
        self.rtl = !self.vertical && scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let width = constraints.constrain_width(placeable.width);
        let height = constraints.constrain_height(placeable.height);
        let max =
            if self.vertical { placeable.height - height } else { placeable.width - width }.max(0);
        if self.state.max_value() != max {
            self.state.write(self.state.max, max);
        }
        let viewport = if self.vertical { height } else { width };
        if self.state.viewport_size() != viewport {
            self.state.write(self.state.viewport, viewport);
        }
        let value = self.state.value().min(max);
        self.laid_out = value;
        if value != self.state.value() {
            self.state.write(self.state.value, value);
        }
        // Relatively: a horizontal scroll's content starts at the right in a right-to-left
        // layout, and scrolls toward the left.
        if self.vertical {
            scope.place_relative(&placeable, 0, -value);
        } else {
            scope.place_relative(&placeable, -value, 0);
        }
        scope.layout(width, height)
    }
}

impl DrawModifierNode for ScrollNode {
    /// Clipped to its bounds.
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        scope.save();
        let size = scope.size();
        scope.clip_outline(Outline::Rectangle(Rect::new(0.0, 0.0, size.width, size.height)));
    }

    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        scope.restore();
    }
}

impl PointerInputModifierNode for ScrollNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        let (vertical, state) = (self.vertical, self.state);
        let fraction = &mut self.fraction;
        let reverse = !vertical && context.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let flung = state;
        self.drag.on_pointer_event(
            event,
            pass,
            vertical,
            reverse,
            context,
            |delta| scroll_state_by(&state, delta, fraction),
            move || {
                let fraction = std::cell::Cell::new(0.0f32);
                Rc::new(move |delta| {
                    let mut left = fraction.get();
                    let used = scroll_state_by(&flung, delta, &mut left);
                    fraction.set(left);
                    used
                })
            },
        );
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag.cancel();
    }
}

/// Scrolls `state` by `delta` pixels, within its range, carrying the fraction of a pixel left
/// over to the next delta (slow drags and fine wheels add up); returns the pixels used.
fn scroll_state_by(state: &ScrollState, delta: f32, fraction: &mut f32) -> f32 {
    let old = state.value();
    let max = state.max_value().max(0);
    let start = old as f32 + *fraction;
    let clamped = (start + delta).clamp(0.0, max as f32);
    let new = clamped.round();
    *fraction = clamped - new;
    if new as i32 != old {
        // The layout read the value: the write measures it again.
        state.write(state.value, new as i32);
    }
    clamped - start
}

/// The wheel and drags of a scrollable node, as Compose's `scrollable`: `scroll_by` gets the
/// deltas (positive towards the end) and returns the part it used.
///
/// A drag is taken by the innermost scrollable once past the touch slop (its Main pass, which
/// runs from the target outward: an outer one sees it consumed), then in the Initial pass, so
/// what's under it sees it consumed (a click there is cancelled). Every delta goes through the
/// nested-scroll connections around (D188), among them the scrollables around, each a
/// nested-scroll parent of those inside it (its post-scroll scrolls it), as Compose's
/// `ScrollingLogic`: what the innermost can't use scrolls the ones around it.
#[derive(Default)]
pub(crate) struct DragScroller {
    drag: Option<Drag>,
    /// The pointers down on the node, with where they last were in the window: the drag
    /// follows the first, then another when it lifts, as Compose's.
    pressed: Vec<(PointerId, Offset)>,
    /// A fling going on: a press stops it (and does nothing else).
    fling: Option<Job>,
    /// The nested-scroll connections around, as the last press or wheel turn found them.
    chain: NestedScrollChain,
    /// The axis, for a cancel's fling dispatch.
    vertical: bool,
}

/// A scrollable as the nested-scroll parent of the scrollables inside it: what they leave
/// scrolls it.
struct ScrollableConnection {
    scroll: Rc<dyn Fn(f32) -> f32>,
    vertical: bool,
    reverse: bool,
}

impl NestedScrollConnection for ScrollableConnection {
    fn on_post_scroll(&self, _: Offset, available: Offset, _: NestedScrollSource) -> Offset {
        let along = if self.vertical { available.y } else { available.x };
        // The content scrolls the pointer's other way.
        let delta = -along;
        if delta == 0.0 {
            return Offset::ZERO;
        }
        let used = if self.reverse { -(self.scroll)(-delta) } else { (self.scroll)(delta) };
        if self.vertical { Offset::new(0.0, -used) } else { Offset::new(-used, 0.0) }
    }
}

impl DragScroller {
    /// Whether a nested-scroll parent was around the last press or wheel turn: it takes what
    /// this scrollable leaves of a delta.
    pub(crate) fn has_outer(&self) -> bool {
        !self.chain.is_empty()
    }

    /// `scroll_by` scrolls during the event; `flinger` gives a scroll usable later, by a
    /// fling's task (a touch let go while moving flings, as on Android) or as the nested-scroll
    /// parent of the scrollables inside.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        vertical: bool,
        reverse: bool,
        context: &EventContext<'_>,
        mut scroll_by: impl FnMut(f32) -> f32,
        flinger: impl Fn() -> Rc<dyn Fn(f32) -> f32>,
    ) {
        let slop = touch_slop(context);
        self.vertical = vertical;
        // The connections around: found on the way in, as a gesture or a wheel turn starts;
        // then this scrollable is one for those inside it.
        if pass == PointerEventPass::Initial
            && matches!(event.kind, PointerEventType::Press | PointerEventType::Scroll)
        {
            self.chain = NestedScrollChain::current();
            NestedScrollChain::enter(Rc::new(ScrollableConnection {
                scroll: flinger(),
                vertical,
                reverse,
            }));
        }
        // A horizontal scroll in a right-to-left layout goes the other way, as Compose's
        // `reverseDirection`.
        let mut reversed =
            move |delta: f32| if reverse { -scroll_by(-delta) } else { scroll_by(delta) };
        let chain = self.chain.clone();
        // Through the connections around.
        let mut scroll_by = move |delta: f32| {
            dispatch_scroll(&chain, vertical, delta, NestedScrollSource::UserInput, &mut reversed)
        };
        let along_axis = move |offset: Offset| if vertical { offset.y } else { offset.x };
        // Where the pointer is in the window: the deltas don't count the content's own moves (a
        // bar collapsing above it), as Compose's changes compare positions in the same layout.
        let position = context.pointer_position().unwrap_or(event.position);
        match (event.kind, pass) {
            (PointerEventType::Scroll, PointerEventPass::Main) if !event.consumed => {
                let delta = if vertical { event.scroll_delta.y } else { event.scroll_delta.x };
                if delta == 0.0 {
                    return;
                }
                // The scrollables and connections around had what's left along the axis.
                scroll_by(delta);
                if vertical {
                    event.scroll_delta.y = 0.0;
                } else {
                    event.scroll_delta.x = 0.0;
                }
                if event.scroll_delta.x.abs() < 0.01 && event.scroll_delta.y.abs() < 0.01 {
                    event.consume();
                }
            }
            (PointerEventType::Press, PointerEventPass::Initial) => {
                // A press during a fling stops it, and is taken: the item under it isn't
                // clicked, as Compose's.
                if let Some(fling) = self.fling.take()
                    && fling.is_active()
                {
                    fling.cancel();
                    event.consume();
                }
                self.pressed.retain(|(pointer, _)| *pointer != event.pointer);
                self.pressed.push((event.pointer, position));
                // Another finger joining a drag leaves it to the first.
                if self.drag.is_none() {
                    self.drag = Some(Drag::new(event, position, along_axis(position)));
                }
            }
            (PointerEventType::Move, PointerEventPass::Initial) => {
                if let Some(pressed) =
                    self.pressed.iter_mut().find(|(pointer, _)| *pointer == event.pointer)
                {
                    pressed.1 = position;
                }
                let Some(drag) = &mut self.drag else { return };
                if drag.pointer != event.pointer || !event.pressed {
                    return;
                }
                let moved = position - drag.last;
                let along = if vertical { moved.y } else { moved.x };
                drag.tracker.add(event_time(event), along_axis(position));
                if drag.scrolling {
                    drag.last = position;
                    event.consume();
                    // Dragging content up scrolls down.
                    scroll_by(-along);
                }
            }
            (PointerEventType::Move, PointerEventPass::Main) => {
                let Some(drag) = &mut self.drag else { return };
                if drag.scrolling || drag.pointer != event.pointer || !event.pressed {
                    return;
                }
                drag.last = position;
                if event.consumed {
                    // An inner scrollable or handler took it.
                    return;
                }
                // How far it is from the press (a tremor going back and forth isn't a drag).
                let moved = position - drag.start;
                let along = if vertical { moved.y } else { moved.x };
                if along.abs() > slop {
                    drag.scrolling = true;
                    event.consume();
                    // What moved past the slop scrolls.
                    scroll_by(-(along - slop.copysign(along)));
                }
            }
            (PointerEventType::Release, PointerEventPass::Initial) => {
                self.pressed.retain(|(pointer, _)| *pointer != event.pointer);
                let Some(drag) = &mut self.drag else { return };
                if drag.pointer != event.pointer {
                    return;
                }
                // The drag goes on with a finger still down.
                if let Some(&(pointer, at)) = self.pressed.last() {
                    drag.pointer = pointer;
                    drag.start = at - (drag.last - drag.start);
                    drag.last = at;
                    drag.tracker = crate::fling::VelocityTracker::default();
                    drag.tracker.add(event_time(event), along_axis(at));
                    return;
                }
                let Some(mut drag) = self.drag.take() else { return };
                if !drag.scrolling {
                    return;
                }
                event.consume();
                // A touch let go while moving: the content goes on, slowing down.
                drag.tracker.add(event_time(event), along_axis(position));
                let density = context.density().density;
                let maximum = MAXIMUM_FLING_DP * density;
                // The finger's velocity; the content scrolls the other way.
                let velocity = if drag.touch {
                    (-drag.tracker.velocity()).clamp(-maximum, maximum)
                } else {
                    0.0
                };
                self.end(velocity, density, reverse, flinger);
            }
            _ => {}
        }
    }

    /// A drag's end at `velocity` (towards the end): the connections around take their part
    /// first, the content flings with the rest, then they hear how it ended.
    fn end(
        &mut self,
        velocity: f32,
        density: f32,
        reverse: bool,
        flinger: impl Fn() -> Rc<dyn Fn(f32) -> f32>,
    ) {
        let vertical = self.vertical;
        let chain = self.chain.clone();
        let axis = move |value: f32| {
            if vertical { Offset::new(0.0, value) } else { Offset::new(value, 0.0) }
        };
        let along_axis = move |offset: Offset| if vertical { offset.y } else { offset.x };
        let velocity = velocity + along_axis(chain.pre_fling(axis(-velocity)));
        if velocity.abs() > MINIMUM_FLING_DP * density {
            let scroll = flinger();
            let flung = chain.clone();
            let scroll: Rc<dyn Fn(f32) -> f32> = Rc::new(move |delta: f32| {
                let mut reversed =
                    |delta: f32| if reverse { -scroll(-delta) } else { scroll(delta) };
                dispatch_scroll(
                    &flung,
                    vertical,
                    delta,
                    NestedScrollSource::SideEffect,
                    &mut reversed,
                )
            });
            let on_end = move |left: f32| {
                chain.post_fling(axis(-(velocity - left)), axis(-left));
            };
            self.fling = Some(fling(velocity, density, scroll, on_end));
        } else {
            chain.post_fling(Offset::ZERO, axis(-velocity));
        }
    }

    /// The gesture is cancelled: a drag going on stops without velocity, as Compose's
    /// `onDragStopped(Zero)` (the connections around still hear it end).
    pub(crate) fn cancel(&mut self) {
        self.pressed.clear();
        if self.drag.take().is_some_and(|drag| drag.scrolling) {
            let chain = self.chain.clone();
            chain.pre_fling(Offset::ZERO);
            chain.post_fling(Offset::ZERO, Offset::ZERO);
        }
    }

    /// The node leaves: its fling stops.
    pub(crate) fn stop_fling(&mut self) {
        if let Some(fling) = self.fling.take() {
            fling.cancel();
        }
    }
}
