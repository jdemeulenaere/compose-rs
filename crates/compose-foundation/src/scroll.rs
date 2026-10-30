//! Scrolling, as Compose's `Modifier.verticalScroll(state)` and `horizontalScroll(state)`: the
//! content measured without a bound along the axis, clipped, and moved by the scroll offset,
//! which the wheel and drags change.
//!
//! The offset is state: the node reads it while measuring (observed, so a write by the
//! application scrolls), and writes it when the wheel or a drag scrolls.

use std::cell::Cell;
use std::rc::Rc;
use std::task::Poll;

use compose_animation::AnimationSpec;
use compose_runtime::{
    ComposeArg, CoroutineScope, Job, MutableState, ObjectHandle, Owned, OwnedState, composable,
    mutable_state_of,
};
use compose_ui::{
    Constraints, DragInteraction, DragStart, DrawModifierNode, DrawScope, EventContext, IntSize,
    LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, MutableInteractionSource, NestedScrollConnection, NestedScrollSource,
    Offset, Outline, PointerEvent, PointerEventPass, PointerEventType, PointerId,
    PointerInputModifierNode, Rect, platform::NestedScrollChain,
};

use crate::gestures::{
    DefaultFlingBehavior, DragSession, FlingBehavior, LocalBoxFuture, MouseWheel, ScrollBlock,
    ScrollCore, ScrollScope, ScrollableState, ScrollableStateExt,
};
use crate::mutator::{MutatePriority, MutationInterrupted};

/// How far content is scrolled, in pixels, and how far it can be, as Compose's `ScrollState`:
/// state, so reading it in composition recomposes on scrolls. A [`ScrollableState`]: it
/// scrolls in `scroll` blocks ([`scroll_to`](Self::scroll_to),
/// [`animate_scroll_to`](Self::animate_scroll_to), drags and flings) or at once
/// (`dispatch_raw_delta`), within `[0, max_value]`.
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
    /// The content's length along the axis, as last laid out.
    content_size: OwnedState<i32>,
    /// The fraction of a pixel scrolled but not shown, as Compose's `accumulator`.
    accumulator: Cell<f32>,
    /// Its scrolls' mutex and flags.
    core: ScrollCore,
}

impl ScrollState {
    #[track_caller]
    pub fn new(initial: i32) -> Owned<ScrollState> {
        let states = ScrollStates {
            value: mutable_state_of(initial.max(0)),
            max: mutable_state_of(i32::MAX),
            viewport: mutable_state_of(0),
            content_size: mutable_state_of(0),
            accumulator: Cell::new(0.0),
            core: ScrollCore::new(),
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

    /// The largest offset, known once laid out (`i32::MAX` before), as `maxValue`.
    pub fn max_value(&self) -> i32 {
        self.read(self.max, 0)
    }

    /// Sets the largest offset (the layout's), the value coerced into it at once, as Compose's
    /// `maxValue` setter.
    fn set_max_value(&self, max: i32) {
        if self.max.try_get() != Some(max) {
            self.write(self.max, max);
        }
        if self.value.try_get().is_some_and(|value| value > max) {
            self.write(self.value, max);
        }
    }

    /// The viewport's length along the scroll's axis, in pixels, as `viewportSize` (a state
    /// read; 0 until laid out).
    pub fn viewport_size(&self) -> i32 {
        self.read(self.viewport, 0)
    }

    /// The interaction source its container reports its drags to, as `interactionSource`.
    pub fn interaction_source(&self) -> MutableInteractionSource {
        self.object.try_get().map_or_else(MutableInteractionSource::new, |states| {
            states.core.interaction_source.clone()
        })
    }

    /// Consumes `delta` pixels within `[0, max_value]`, the fraction of a pixel carried to the
    /// next, as Compose's `ScrollState`'s consumer; returns the pixels used.
    fn consume(&self, delta: f32) -> f32 {
        let Some(states) = self.object.try_get() else { return 0.0 };
        let value = self.value();
        let absolute = value as f32 + delta + states.accumulator.get();
        let new_value = absolute.clamp(0.0, self.max_value().max(0) as f32);
        let changed = absolute != new_value;
        let consumed = new_value - value as f32;
        let consumed_int = consumed.round() as i32;
        if consumed_int != 0 {
            // The layout read the value: the write measures it again.
            self.write(self.value, value + consumed_int);
        }
        states.accumulator.set(consumed - consumed_int as f32);
        // Without a floating-point rounding error when nothing was cut.
        if changed { consumed } else { delta }
    }

    /// Scrolls to `value` (within `[0, max_value]`) in a scroll of the default priority, as
    /// `scrollTo`; returns the pixels it moved.
    pub async fn scroll_to(&self, value: i32) -> Result<f32, MutationInterrupted> {
        self.scroll_by((value - self.value()) as f32).await
    }

    /// Scrolls to `value` along `spec` (Compose's default: `spring()`), as `animateScrollTo`.
    pub async fn animate_scroll_to(
        &self,
        value: i32,
        spec: impl Into<AnimationSpec>,
    ) -> Result<(), MutationInterrupted> {
        self.animate_scroll_by((value - self.value()) as f32, spec).await.map(|_| ())
    }
}

impl ScrollableState for ScrollState {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        let Some(states) = self.object.get() else { return Box::pin(async { Ok(()) }) };
        let state = *self;
        Box::pin(async move {
            states.core.scroll(priority, Rc::new(move |delta| state.consume(delta)), block).await
        })
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.consume(delta)
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.object.try_get().is_some_and(|states| states.core.is_scroll_in_progress())
    }

    /// Whether `value < max_value`, as `canScrollForward`.
    fn can_scroll_forward(&self) -> bool {
        self.value() < self.max_value()
    }

    /// Whether `value > 0`, as `canScrollBackward`.
    fn can_scroll_backward(&self) -> bool {
        self.value() > 0
    }

    fn last_scrolled_forward(&self) -> bool {
        self.object.try_get().is_some_and(|states| states.core.last_scrolled_forward())
    }

    fn last_scrolled_backward(&self) -> bool {
        self.object.try_get().is_some_and(|states| states.core.last_scrolled_backward())
    }

    fn internal_interaction_source(&self) -> Option<MutableInteractionSource> {
        self.object.try_get().map(|states| states.core.interaction_source.clone())
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
    /// The same with Compose's optional parameters, as `Modifier.verticalScroll(state, enabled,
    /// flingBehavior, reverseScrolling)`: `enabled = false` ignores drags and the wheel (the
    /// state still scrolls), `fling_behavior` (`None`: the default) is what a release does, and
    /// `reverse_scrolling` starts the content at the bottom, scrolled from there.
    fn vertical_scroll_with(
        self,
        state: ScrollState,
        enabled: bool,
        fling_behavior: Option<Rc<dyn FlingBehavior>>,
        reverse_scrolling: bool,
    ) -> Modifier;
    /// As `Modifier.horizontalScroll(state, enabled, flingBehavior, reverseScrolling)`.
    fn horizontal_scroll_with(
        self,
        state: ScrollState,
        enabled: bool,
        fling_behavior: Option<Rc<dyn FlingBehavior>>,
        reverse_scrolling: bool,
    ) -> Modifier;
}

impl ScrollModifiers for Modifier {
    fn vertical_scroll(self, state: ScrollState) -> Modifier {
        self.vertical_scroll_with(state, true, None, false)
    }

    fn horizontal_scroll(self, state: ScrollState) -> Modifier {
        self.horizontal_scroll_with(state, true, None, false)
    }

    fn vertical_scroll_with(
        self,
        state: ScrollState,
        enabled: bool,
        fling_behavior: Option<Rc<dyn FlingBehavior>>,
        reverse_scrolling: bool,
    ) -> Modifier {
        // Its content a group for focus searches, as `ScrollableNode`'s target: outside what's
        // scrolled, so that its rectangle is the viewport's.
        self.focus_group().then_custom(ScrollElement {
            state,
            vertical: true,
            enabled,
            fling_behavior,
            reverse_scrolling,
        })
    }

    fn horizontal_scroll_with(
        self,
        state: ScrollState,
        enabled: bool,
        fling_behavior: Option<Rc<dyn FlingBehavior>>,
        reverse_scrolling: bool,
    ) -> Modifier {
        self.focus_group().then_custom(ScrollElement {
            state,
            vertical: false,
            enabled,
            fling_behavior,
            reverse_scrolling,
        })
    }
}

struct ScrollElement {
    state: ScrollState,
    vertical: bool,
    enabled: bool,
    fling_behavior: Option<Rc<dyn FlingBehavior>>,
    reverse_scrolling: bool,
}

/// Whether two optional fling behaviors are the same (by identity).
pub(crate) fn same_fling(
    a: &Option<Rc<dyn FlingBehavior>>,
    b: &Option<Rc<dyn FlingBehavior>>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}

impl ModifierNodeElement for ScrollElement {
    type Node = ScrollNode;
    fn create(&self) -> ScrollNode {
        ScrollNode {
            state: self.state,
            dyn_state: Rc::new(self.state),
            vertical: self.vertical,
            enabled: self.enabled,
            fling_behavior: self.fling_behavior.clone(),
            reverse_scrolling: self.reverse_scrolling,
            drag: DragScroller::default(),
            laid_out: 0,
            rtl: false,
        }
    }

    fn update(&self, node: &mut ScrollNode) {
        // A fling scrolls the state it started with: a new state stops it (the old one's
        // owner may be going, D240).
        if node.state != self.state {
            node.drag.cancel();
            node.drag.stop_fling();
            node.dyn_state = Rc::new(self.state);
        }
        if node.enabled && !self.enabled {
            node.drag.cancel();
        }
        node.state = self.state;
        node.vertical = self.vertical;
        node.enabled = self.enabled;
        node.fling_behavior = self.fling_behavior.clone();
        node.reverse_scrolling = self.reverse_scrolling;
    }

    fn same(&self, other: &Self) -> bool {
        other.vertical == self.vertical
            && other.state.same(&self.state)
            && other.enabled == self.enabled
            && other.reverse_scrolling == self.reverse_scrolling
            && same_fling(&other.fling_behavior, &self.fling_behavior)
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
        tracker.press(event_time(event), along);
        Drag { pointer: event.pointer, start: position, last: position, scrolling: false, tracker }
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
}

/// When an event happened (its time, else now), for velocities.
pub(crate) fn event_time(event: &PointerEvent) -> u64 {
    if event.time_nanos != 0 { event.time_nanos } else { compose_runtime::platform::now_nanos() }
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
    // With nothing left too, as Compose's `performScroll`: the direction noted is none.
    let used = scroll_by(left);
    let after = along(chain.post_scroll(axis(-used), axis(-(left - used)), source));
    used - before - after
}

/// The distance a pointer moves before a drag scrolls, as Compose's touch slop (8 dp), in
/// pixels.
pub(crate) fn touch_slop(context: &EventContext<'_>) -> f32 {
    context.view_configuration().touch_slop
}

struct ScrollNode {
    state: ScrollState,
    /// The state as a scrollable state, for drags and flings.
    dyn_state: Rc<dyn ScrollableState>,
    vertical: bool,
    enabled: bool,
    fling_behavior: Option<Rc<dyn FlingBehavior>>,
    reverse_scrolling: bool,
    drag: DragScroller,
    /// The offset the last layout used.
    laid_out: i32,
    /// Horizontal in a right-to-left layout: its content starts at the right.
    rtl: bool,
}

impl ScrollNode {
    /// Whether the content runs against the pointer: scrolled from its end
    /// (`reverse_scrolling`), or a row right to left (both: neither).
    fn mirrored(&self, rtl: bool) -> bool {
        self.reverse_scrolling ^ (!self.vertical && rtl)
    }
}

impl compose_ui::SemanticsModifierNode for ScrollNode {
    /// Its offset and the largest, along its axis, as `Modifier.verticalScroll`'s semantics:
    /// accessibility services scroll it a page at a time (D478).
    fn apply_semantics(&self, config: &mut compose_ui::SemanticsConfiguration) {
        let range = compose_ui::ScrollAxisRange {
            value: self.state.value() as f32,
            max_value: self.state.max_value() as f32,
            reverse_scrolling: self.reverse_scrolling,
        };
        if self.vertical {
            config.set_vertical_scroll_axis_range(range);
        } else {
            config.set_horizontal_scroll_axis_range(range);
        }
        // Not offered when disabled, as Compose's scrollable (the range stays).
        if !self.enabled {
            return;
        }
        let (state, vertical) = (self.state, self.vertical);
        config.scroll_by(None, move |x, y| {
            // Towards the content's end (its value growing), as Compose's (D491); handled even
            // where nothing moves (an edge), as Compose's action returns true (D877).
            let delta = if vertical { y } else { x };
            if delta != 0.0 {
                state.dispatch_raw_delta(delta);
            }
            true
        });
    }
}

impl ModifierNode for ScrollNode {
    fn as_semantics(&mut self) -> Option<&mut dyn compose_ui::SemanticsModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, _context: &mut compose_ui::ReleaseContext<'_>) {
        self.drag.cancel();
        self.drag.detach();
    }
    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.drag.cancel();
        self.drag.detach();
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
        let mirrored = self.mirrored(self.rtl);
        let (start, end, length) = scroll_axis(rect, viewport, self.vertical, mirrored);
        // Where it will be once the scrolling asked for (not yet laid out) is.
        let pending = (self.state.value() - self.laid_out) as f32;
        let delta = bring_into_view_delta(start - pending, end - pending, length);
        let used = if delta == 0.0 { 0.0 } else { self.state.dispatch_raw_delta(delta) };
        Some(scrolled_rect(rect, used, self.vertical, mirrored))
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
    // As Compose's `ScrollingLayoutNode`: the content's, unbounded along the scroll.
    fn min_intrinsic_width(
        &mut self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        scope.min_intrinsic_width(
            content,
            if self.vertical { Constraints::INFINITY } else { height },
        )
    }

    fn min_intrinsic_height(
        &mut self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        scope.min_intrinsic_height(
            content,
            if self.vertical { width } else { Constraints::INFINITY },
        )
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        scope.max_intrinsic_width(
            content,
            if self.vertical { Constraints::INFINITY } else { height },
        )
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        scope.max_intrinsic_height(
            content,
            if self.vertical { width } else { Constraints::INFINITY },
        )
    }

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
        // Before placing: the value is coerced into the new range at once, as Compose's.
        self.state.set_max_value(max);
        let viewport = if self.vertical { height } else { width };
        if self.state.viewport_size() != viewport {
            self.state.write(self.state.viewport, viewport);
        }
        if let Some(states) = self.state.object.try_get() {
            let content = if self.vertical { placeable.height } else { placeable.width };
            if states.content_size.try_get() != Some(content) {
                let _ = states.content_size.try_set(content);
            }
        }
        let value = self.state.value().clamp(0, max);
        self.laid_out = value;
        // Scrolled from the end when reversed, as Compose's `ScrollNode`.
        let offset = if self.reverse_scrolling { value - max } else { -value };
        // Relatively: a horizontal scroll's content starts at the right in a right-to-left
        // layout, and scrolls toward the left.
        if self.vertical {
            scope.place_relative(&placeable, 0, offset);
        } else {
            scope.place_relative(&placeable, offset, 0);
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
    /// It waits for a touch's drag past a press, so a clickable inside delays its press (not a
    /// mouse's, as Compose's scrollables, which a mouse doesn't drag).
    fn waits_for_gesture(&self, press: &PointerEvent) -> bool {
        press.pointer_type != compose_ui::PointerType::Mouse
    }

    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if !self.enabled {
            return;
        }
        let state = self.state;
        let reverse = self.mirrored(context.layout_direction() == compose_ui::LayoutDirection::Rtl);
        let driver = ScrollDriver {
            state: &self.dyn_state,
            fling_behavior: self.fling_behavior.as_ref(),
            interaction_source: None,
            state_source: true,
            set_outer: None,
        };
        self.drag.on_pointer_event(
            event,
            pass,
            bounds,
            self.vertical,
            reverse,
            context,
            &driver,
            // As the nested-scroll parent of the scrollables inside: what they leave scrolls it,
            // unless a scroll of its own goes on, as Compose's `performRawScroll`.
            move |_: &NestedScrollChain, _: NestedScrollSource| {
                Rc::new(move |delta| {
                    if state.is_scroll_in_progress() {
                        0.0
                    } else {
                        state.dispatch_raw_delta(delta)
                    }
                })
            },
        );
    }

    fn on_cancel_pointer_input(&mut self) {
        self.drag.cancel();
    }
}

/// What a scrollable node's drags and flings drive: its state (a drag holds its scroll, a fling
/// runs in one), its fling behavior (`None`: the default), the interaction source told of drags
/// (`None`: the state's own, if any), and for a lazy list, where its scope's deltas learn the
/// nested-scroll parents around (D479).
pub(crate) struct ScrollDriver<'a> {
    pub state: &'a Rc<dyn ScrollableState>,
    pub fling_behavior: Option<&'a Rc<dyn FlingBehavior>>,
    pub interaction_source: Option<&'a MutableInteractionSource>,
    /// Without one, the drags go to the state's own source (`vertical_scroll`'s and the lazy
    /// layouts', which give Compose's `scrollable` their state's `internalInteractionSource`);
    /// `Modifier.scrollable` without one reports nothing, as Compose's.
    pub state_source: bool,
    pub set_outer: Option<&'a OuterSetter>,
}

/// Tells a lazy list's scope the nested-scroll parents its next deltas come through (`None`
/// after them).
pub(crate) type OuterSetter = Rc<dyn Fn(Option<(NestedScrollChain, NestedScrollSource)>)>;

/// Scrolls `delta` through `scope`, the parents named to `set_outer` around it.
fn scope_scroll(
    scope: &ScrollScope,
    set_outer: Option<&OuterSetter>,
    chain: &NestedScrollChain,
    source: NestedScrollSource,
    delta: f32,
) -> f32 {
    match set_outer {
        Some(set_outer) => {
            set_outer((!chain.is_empty()).then(|| (chain.clone(), source)));
            let used = scope.scroll_by(delta);
            set_outer(None);
            used
        }
        None => scope.scroll_by(delta),
    }
}

/// A pointer's `delta`, as Compose's `performScroll`: through the connections around, then `scope`
/// (the parents named to `set_outer`). A horizontal scroll in a right-to-left layout goes the
/// other way, as Compose's `reverseDirection`. Returns what all of them used.
fn scroll_through(
    scope: &ScrollScope,
    set_outer: Option<&OuterSetter>,
    chain: &NestedScrollChain,
    vertical: bool,
    reverse: bool,
    delta: f32,
) -> f32 {
    let mut own = |delta: f32| {
        let delta = if reverse { -delta } else { delta };
        let used = scope_scroll(scope, set_outer, chain, NestedScrollSource::UserInput, delta);
        if reverse { -used } else { used }
    };
    dispatch_scroll(chain, vertical, delta, NestedScrollSource::UserInput, &mut own)
}

/// What a scrollable node's flings and wheel scroll, owned for tasks to outlive an event: a
/// [`ScrollDriver`]'s values, its axis and way, the connections around, as Compose's `ScrollingLogic`.
#[derive(Clone)]
pub(crate) struct ScrollTarget {
    pub state: Rc<dyn ScrollableState>,
    pub fling_behavior: Option<Rc<dyn FlingBehavior>>,
    pub set_outer: Option<OuterSetter>,
    pub chain: NestedScrollChain,
    pub vertical: bool,
    pub reverse: bool,
    /// Pixels per dp: the default fling's spline and the wheel's animation are in dp.
    pub density: f32,
    /// A fling of this node goes on, as `isFlinging`.
    flinging: Rc<Cell<bool>>,
    /// The node left, as `isScrollableNodeAttached` false: its flings, its own or a child's
    /// velocity's, end at their next delta.
    detached: Rc<Cell<bool>>,
}

/// Clears `is_flinging` however a fling ends.
struct Flinging(Rc<Cell<bool>>);

impl Drop for Flinging {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl ScrollTarget {
    fn axis(&self, value: f32) -> Offset {
        if self.vertical { Offset::new(0.0, value) } else { Offset::new(value, 0.0) }
    }

    fn along(&self, offset: Offset) -> f32 {
        if self.vertical { offset.y } else { offset.x }
    }

    /// A delta of the user's in `scope`, as `scrollBy(offset, UserInput)`.
    pub(crate) fn user_scroll(&self, scope: &ScrollScope, delta: f32) -> f32 {
        scroll_through(
            scope,
            self.set_outer.as_ref(),
            &self.chain,
            self.vertical,
            self.reverse,
            delta,
        )
    }

    /// Flings at `velocity` (towards the end, before a reversal) in a scroll of its own, as
    /// `doFlingAnimation`; returns the velocity left, or `Err` if another scroll interrupted it,
    /// or the node left under a custom behavior, as Compose's `FlingCancellationException`, which
    /// only the default behavior catches.
    async fn do_fling(&self, velocity: f32) -> Result<f32, MutationInterrupted> {
        let behavior: Rc<dyn FlingBehavior> = match &self.fling_behavior {
            Some(behavior) => behavior.clone(),
            None => Rc::new(DefaultFlingBehavior::new(self.density)),
        };
        self.flinging.set(true);
        let _flinging = Flinging(self.flinging.clone());
        let left = Rc::new(Cell::new(Some(velocity)));
        let into = left.clone();
        let target = self.clone();
        self.state
            .scroll(MutatePriority::Default, move |scope| async move {
                let reverse = target.reverse;
                let cancelled = Rc::new(Cell::new(false));
                let saw_cancelled = cancelled.clone();
                let flung = target.clone();
                // The fling's deltas, the state's way (as its velocity): through the connections
                // around, then the scroll's scope.
                let nested = ScrollScope::of_fling(
                    move |state_delta: f32| {
                        if state_delta != 0.0 && flung.detached.get() {
                            cancelled.set(true);
                            return 0.0;
                        }
                        let delta = if reverse { -state_delta } else { state_delta };
                        let mut own = |delta: f32| {
                            let delta = if reverse { -delta } else { delta };
                            let used = scope_scroll(
                                &scope,
                                flung.set_outer.as_ref(),
                                &flung.chain,
                                NestedScrollSource::SideEffect,
                                delta,
                            );
                            if reverse { -used } else { used }
                        };
                        let used = dispatch_scroll(
                            &flung.chain,
                            flung.vertical,
                            delta,
                            NestedScrollSource::SideEffect,
                            &mut own,
                        );
                        if reverse { -used } else { used }
                    },
                    saw_cancelled.clone(),
                );
                // The velocity the state's way, as Compose's `reverseIfNeeded`.
                let toward_state = if reverse { -velocity } else { velocity };
                let catches = !behavior.should_be_triggered_by_mouse_wheel();
                let mut fling = behavior.perform_fling(nested, toward_state);
                let state_left = std::future::poll_fn(|context| {
                    let polled = fling.as_mut().poll(context);
                    if saw_cancelled.get() && !catches {
                        return Poll::Ready(None);
                    }
                    polled.map(Some)
                })
                .await;
                into.set(state_left.map(|left| if reverse { -left } else { left }));
            })
            .await?;
        left.get().ok_or(MutationInterrupted)
    }

    /// A scroll's end at `velocity`: the connections around take their part first, the content
    /// flings with the rest (the fling behavior, the default's spline if none), then they hear how
    /// it ended, as Compose's `onScrollStopped`.
    pub(crate) fn launch_fling(self, velocity: f32) -> Job {
        CoroutineScope::new().launch(async move {
            let pre = self.chain.pre_fling(self.axis(-velocity)).await;
            let velocity = velocity + self.along(pre);
            // Interrupted, what interrupted it goes on, and no one hears of the fling.
            if let Ok(left) = self.do_fling(velocity).await {
                self.chain.post_fling(self.axis(-(velocity - left)), self.axis(-left)).await;
            }
        })
    }
}

/// The wheel and drags of a scrollable node, as Compose's `scrollable`: `scroll_by` gets the
/// wheel's deltas (positive towards the end) and returns the part it used; a drag holds the
/// state's scroll (`ScrollDriver`) and its deltas go through the scroll's scope; a release
/// flings in a scroll of its own, with the fling behavior.
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
    /// A fling going on: a press stops it.
    fling: Option<Job>,
    /// A fling of this node goes on, as `isFlinging`.
    flinging: Rc<Cell<bool>>,
    /// The node left, for the flings of the time it was attached; a new one when it's reused.
    detached: Rc<Cell<bool>>,
    /// The wheel, once turned.
    wheel: Option<MouseWheel>,
    /// The drag's hold on the state's scroll, while it scrolls.
    session: Option<DragSession>,
    /// Where the drag going on was told (to tell it ended).
    dragged: Option<(MutableInteractionSource, DragStart)>,
    /// The nested-scroll connections around, as the last press or wheel turn found them.
    chain: NestedScrollChain,
    /// The axis, for a cancel's fling dispatch.
    vertical: bool,
}

/// A scrollable as the nested-scroll parent of the scrollables inside it: what they leave
/// scrolls it.
struct ScrollableConnection {
    /// Its scroll for what a child's drag (`UserInput`) leaves, and for what a child's fling
    /// (`SideEffect`) leaves: a lazy list offers its own parents the rest as such (D493).
    scroll: Rc<dyn Fn(f32) -> f32>,
    scroll_side_effect: Rc<dyn Fn(f32) -> f32>,
    vertical: bool,
    reverse: bool,
    /// Its fling with what they leave.
    target: ScrollTarget,
}

impl NestedScrollConnection for ScrollableConnection {
    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        let along = if self.vertical { available.y } else { available.x };
        // The content scrolls the pointer's other way.
        let delta = -along;
        if delta == 0.0 {
            return Offset::ZERO;
        }
        let scroll = if source == NestedScrollSource::UserInput {
            &self.scroll
        } else {
            &self.scroll_side_effect
        };
        let used = if self.reverse { -scroll(-delta) } else { scroll(delta) };
        if self.vertical { Offset::new(0.0, -used) } else { Offset::new(-used, 0.0) }
    }

    /// Flings with what a scrollable inside left, as Compose's `onPostFling`, unless it's flinging
    /// already; returns what it used, along its axis: what goes across is for the parents, as
    /// `doFlingAnimation`. Its fling interrupted, the fling that called it ends too, as Compose's
    /// cancellation goes through the suspending call.
    fn on_post_fling(&self, _: Offset, available: Offset) -> LocalBoxFuture<'_, Offset> {
        Box::pin(async move {
            let along = self.target.along(available);
            let left = if self.target.flinging.get() {
                0.0
            } else {
                match self.target.do_fling(-along).await {
                    Ok(left) => left,
                    Err(MutationInterrupted) => {
                        compose_runtime::platform::cancel_current_task();
                        std::future::pending().await
                    }
                }
            };
            // The content's way is the pointer's other.
            self.target.axis(along + left)
        })
    }
}

impl DragScroller {
    /// The wheel's and the drags' deltas scroll the driver's state in scrolls of the user-input
    /// priority; `flinger` gives a scroll usable later, as the nested-scroll parent of the
    /// scrollables inside.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        vertical: bool,
        reverse: bool,
        context: &EventContext<'_>,
        driver: &ScrollDriver<'_>,
        flinger: impl Fn(&NestedScrollChain, NestedScrollSource) -> Rc<dyn Fn(f32) -> f32>,
    ) {
        let slop = touch_slop(context);
        self.vertical = vertical;
        // The connections around: found on the way in, as a gesture or a wheel turn starts;
        // then this scrollable is one for those inside it.
        if pass == PointerEventPass::Initial
            && matches!(event.kind, PointerEventType::Press | PointerEventType::Scroll)
        {
            self.chain = NestedScrollChain::current();
            let target = self.target(driver, reverse, context.density().density);
            if event.kind == PointerEventType::Scroll {
                match &self.wheel {
                    Some(wheel) => wheel.set_target(target.clone()),
                    None => self.wheel = Some(MouseWheel::new(target.clone())),
                }
            }
            NestedScrollChain::enter(Rc::new(ScrollableConnection {
                scroll: flinger(&self.chain, NestedScrollSource::UserInput),
                scroll_side_effect: flinger(&self.chain, NestedScrollSource::SideEffect),
                vertical,
                reverse,
                target,
            }));
        }
        if event.kind == PointerEventType::Scroll {
            if let Some(wheel) = &self.wheel {
                wheel.on_pointer_event(event, pass, bounds);
            }
            return;
        }
        // A drag's deltas: through the scroll scope of the scroll of the user-input priority it
        // holds while it lasts, so it stops a fling or an animation and the state says it's
        // scrolling.
        let chain = &self.chain;
        let set_outer = driver.set_outer;
        let drag_by = |session: &Option<DragSession>, delta: f32| {
            if let Some(scope) = session.as_ref().and_then(DragSession::scope) {
                scroll_through(&scope, set_outer, chain, vertical, reverse, delta);
            }
        };
        let along_axis = move |offset: Offset| if vertical { offset.y } else { offset.x };
        // Where the pointer is in the window: the deltas don't count the content's own moves (a
        // bar collapsing above it), as Compose's changes compare positions in the same layout.
        let position = context.pointer_position().unwrap_or(event.position);
        match (event.kind, pass) {
            (PointerEventType::Press, PointerEventPass::Initial) => {
                // A mouse doesn't drag a scrollable, as Compose's `CanDragCalculation` (`type !=
                // PointerType.Mouse`, D874): its press stops nothing, and it never takes over a
                // drag. Its wheel scrolls it.
                if event.pointer_type == compose_ui::PointerType::Mouse {
                    return;
                }
                self.pressed.retain(|(pointer, _)| *pointer != event.pointer);
                self.pressed.push((event.pointer, position));
                let flinging = self.fling.take().is_some_and(|fling| {
                    let active = fling.is_active();
                    fling.cancel();
                    active
                });
                // Another finger joining a drag leaves it to the first.
                if self.drag.is_none() {
                    let mut drag = Drag::new(event, position, along_axis(position));
                    // A press during a scroll (a fling, an animated scroll, an app's own) stops
                    // it and drags at once, as Compose's `startDragImmediately`, and is taken:
                    // the item under it isn't clicked (D883).
                    if flinging || driver.state.is_scroll_in_progress() {
                        event.consume();
                        drag.scrolling = true;
                        Self::begin_scroll(&mut self.session, &mut self.dragged, driver);
                    }
                    self.drag = Some(drag);
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
                    drag_by(&self.session, -along);
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
                    Self::begin_scroll(&mut self.session, &mut self.dragged, driver);
                    // What moved past the slop scrolls.
                    drag_by(&self.session, -(along - slop.copysign(along)));
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
                    drag.tracker.press(event_time(event), along_axis(at));
                    return;
                }
                let Some(mut drag) = self.drag.take() else { return };
                if !drag.scrolling {
                    return;
                }
                event.consume();
                self.end_drag(DragInteraction::Stop);
                // A finger or a stylus let go while moving: the content goes on, slowing down
                // (every pointer that drags flings, as Compose's).
                drag.tracker.release(event_time(event));
                // The platform's fling bounds (`ViewConfiguration`, D907).
                let configuration = context.view_configuration();
                let maximum = configuration.maximum_fling_velocity;
                // The pointer's velocity; the content scrolls the other way.
                let velocity = (-drag.tracker.velocity()).clamp(-maximum, maximum);
                let density = context.density().density;
                self.end(velocity, density, reverse, driver);
            }
            _ => {}
        }
    }

    /// The drag holds the state's scroll (a fling or an animated scroll stops), and says so to the
    /// interaction source.
    fn begin_scroll(
        session: &mut Option<DragSession>,
        dragged: &mut Option<(MutableInteractionSource, DragStart)>,
        driver: &ScrollDriver<'_>,
    ) {
        *session = Some(DragSession::start(driver.state.clone()));
        let source = driver
            .interaction_source
            .cloned()
            .or_else(|| driver.state_source.then(|| driver.state.internal_interaction_source())?);
        *dragged = source.map(|source| {
            let start = DragStart::new();
            source.try_emit(DragInteraction::Start(start));
            (source, start)
        });
    }

    /// The drag's hold on the state's scroll ends, and the interaction source hears how:
    /// `ended` is `DragInteraction::Stop` for a release, `Cancel` for a gesture cancelled or a
    /// node gone, as Compose's `processDragStop` and `processDragCancel`.
    fn end_drag(&mut self, ended: fn(DragStart) -> DragInteraction) {
        if let Some(session) = self.session.take() {
            session.end();
        }
        if let Some((source, start)) = self.dragged.take() {
            source.try_emit(ended(start));
        }
    }

    /// What this node's flings and wheel scroll, as of this event.
    fn target(&self, driver: &ScrollDriver<'_>, reverse: bool, density: f32) -> ScrollTarget {
        ScrollTarget {
            state: driver.state.clone(),
            fling_behavior: driver.fling_behavior.cloned(),
            set_outer: driver.set_outer.cloned(),
            chain: self.chain.clone(),
            vertical: self.vertical,
            reverse,
            density,
            flinging: self.flinging.clone(),
            detached: self.detached.clone(),
        }
    }

    /// A drag's end at `velocity` (towards the end, before a reversal).
    fn end(&mut self, velocity: f32, density: f32, reverse: bool, driver: &ScrollDriver<'_>) {
        self.fling = Some(self.target(driver, reverse, density).launch_fling(velocity));
    }

    /// The gesture is cancelled: a drag going on stops without velocity, as Compose's
    /// `onDragStopped(Zero)` (the connections around still hear it end).
    pub(crate) fn cancel(&mut self) {
        self.pressed.clear();
        self.end_drag(DragInteraction::Cancel);
        if self.drag.take().is_some_and(|drag| drag.scrolling) {
            let chain = self.chain.clone();
            CoroutineScope::new().launch(async move {
                chain.pre_fling(Offset::ZERO).await;
                chain.post_fling(Offset::ZERO, Offset::ZERO).await;
            });
        }
    }

    /// Another state takes over: what the old one's doing stops now (its owner may be going, D240),
    /// and a fling with what a child left, in the child's task, at its next delta.
    pub(crate) fn stop_fling(&mut self) {
        self.detached.set(true);
        self.detached = Rc::default();
        if let Some(fling) = self.fling.take() {
            fling.cancel();
        }
        if let Some(wheel) = self.wheel.take() {
            wheel.stop();
            wheel.cancel_fling();
        }
    }

    /// The node leaves, as `isScrollableNodeAttached` false: its flings end at their next delta,
    /// the default's leaving its velocity there to the parents, as Compose's, and a child's velocity
    /// moves nothing; its wheel stops with its coroutines. Reused, it starts over.
    pub(crate) fn detach(&mut self) {
        self.detached.set(true);
        self.detached = Rc::default();
        self.fling = None;
        if let Some(wheel) = self.wheel.take() {
            wheel.stop();
        }
    }
}
