//! `RangeSlider`, as Compose's: two thumbs on a track, each focusable with its own keys and
//! semantics, the gesture taken by the nearer one (D1035).

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::Box;
use compose_runtime::{Callback, ObjectHandle, Owned, OwnedState, composable, mutable_state_of};
use compose_ui::{
    Alignment, Constraints, Dp, DragInteraction, DragStart, EventContext, Focus, FocusInteraction,
    FocusTargetModifierNode, IntSize, Key, KeyEvent, KeyEventType, KeyInputModifierNode, Layout,
    Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, MutableInteractionSource, Offset, PointerEvent, PointerEventPass,
    PointerEventType, PointerId, PointerInputModifierNode, Press, PressInteraction,
    SemanticsConfiguration, SemanticsModifierNode, SizeSpec,
};

use crate::components::{increase_semantics_bounds, minimum_interactive_component_size};
use crate::slider::{
    DrawSliderStopIndicator, DrawSliderTick, INSET_FOCUS_RING_PADDING, SliderColors,
    SliderDefaults, THUMB_TRACK_GAP_SIZE, THUMB_WIDTH, TRACK_HEIGHT, TRACK_INSIDE_CORNER_SIZE,
    TrackDrawing, draw_track, format_for_semantics, fraction, nearest_stop, snap_to_tick,
    thumb_focus_ring, tick_fractions, uses_inset_focus_ring,
};

struct RangeInner {
    start_value: OwnedState<f32>,
    end_value: OwnedState<f32>,
    steps: u32,
    track_range: (f32, f32),
    is_start_focused: OwnedState<bool>,
    is_end_focused: OwnedState<bool>,
    is_dragging: Cell<bool>,
    /// The thumbs' sizes, their focus padding apart, the slider's width and the track's corner, as
    /// the last layout measured them.
    start_thumb: Cell<(f32, f32)>,
    end_thumb: Cell<(f32, f32)>,
    total_width: Cell<i32>,
    track_corner: Cell<i32>,
    /// Where the thumbs are, and between where they move, in pixels, as `rawOffsetStart` and its
    /// siblings: a drag keeps moving these past where the values stop.
    raw_start: Cell<f32>,
    raw_end: Cell<f32>,
    min_px: Cell<f32>,
    max_px: Cell<f32>,
}

/// A range slider's state, as Compose's `RangeSliderState`: its start and end values in
/// `track_range`, snapped to `steps` stops between its ends.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_range_slider_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RangeSliderState {
    object: ObjectHandle<RangeInner>,
}

impl compose_runtime::ComposeArg for RangeSliderState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl RangeSliderState {
    /// As `RangeSliderState(startValue, endValue, steps, trackRange)`: the values kept in the
    /// range and in order.
    #[track_caller]
    pub fn new(
        start_value: f32,
        end_value: f32,
        steps: u32,
        track_range: (f32, f32),
    ) -> Owned<RangeSliderState> {
        let start = start_value.clamp(track_range.0, track_range.1);
        let end = end_value.clamp(track_range.0, track_range.1);
        Owned::new(
            RangeInner {
                start_value: mutable_state_of(start.min(end)),
                end_value: mutable_state_of(start.max(end)),
                steps,
                track_range,
                is_start_focused: mutable_state_of(false),
                is_end_focused: mutable_state_of(false),
                is_dragging: Cell::new(false),
                start_thumb: Cell::new((0.0, 0.0)),
                end_thumb: Cell::new((0.0, 0.0)),
                total_width: Cell::new(0),
                track_corner: Cell::new(0),
                raw_start: Cell::new(0.0),
                raw_end: Cell::new(0.0),
                min_px: Cell::new(0.0),
                max_px: Cell::new(0.0),
            },
            |object| RangeSliderState { object },
        )
    }

    #[track_caller]
    fn inner(&self) -> Option<Rc<RangeInner>> {
        self.object.get()
    }

    /// The range's start (a state read), as `startValue`.
    pub fn start_value(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.start_value.get())
    }

    /// Sets the range's start, kept up to its end and snapped to a stop.
    pub fn set_start_value(&self, value: f32) {
        let Some(inner) = self.inner() else { return };
        let (start, end) = inner.track_range;
        let value = value.clamp(start, inner.end_value.get().max(start));
        inner.start_value.set(snap_to_tick(value, inner.steps, start, end));
    }

    /// The range's end (a state read), as `endValue`.
    pub fn end_value(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.end_value.get())
    }

    /// Sets the range's end, kept from its start and snapped to a stop.
    pub fn set_end_value(&self, value: f32) {
        let Some(inner) = self.inner() else { return };
        let (start, end) = inner.track_range;
        let value = value.clamp(inner.start_value.get().min(end), end);
        inner.end_value.set(snap_to_tick(value, inner.steps, start, end));
    }

    /// The stops between the ends, as `steps`.
    pub fn steps(&self) -> u32 {
        self.inner().map_or(0, |inner| inner.steps)
    }

    /// The values' range, as `trackRange`.
    pub fn track_range(&self) -> (f32, f32) {
        self.inner().map_or((0.0, 1.0), |inner| inner.track_range)
    }

    fn start_fraction(&self) -> f32 {
        let (start, end) = self.track_range();
        fraction(start, end, self.start_value())
    }

    fn end_fraction(&self) -> f32 {
        let (start, end) = self.track_range();
        fraction(start, end, self.end_value())
    }

    /// As `startSteps`: the stops before the end thumb.
    fn start_steps(&self) -> u32 {
        (self.steps() as f32 * self.end_fraction()).floor() as u32
    }

    /// As `endSteps`: the stops after the start thumb.
    fn end_steps(&self) -> u32 {
        (self.steps() as f32 * (1.0 - self.start_fraction())).floor() as u32
    }

    fn scale_to_user(&self, min: f32, max: f32, offset: f32) -> f32 {
        let (start, end) = self.track_range();
        start + (end - start) * fraction(min, max, offset)
    }

    fn scale_to_offset(&self, min: f32, max: f32, value: f32) -> f32 {
        min + (max - min) * {
            let (start, end) = self.track_range();
            fraction(start, end, value)
        }
    }

    /// As `updateMinMaxPx`: from the last layout, unless a drag is under way.
    fn update_min_max(&self) {
        let Some(inner) = self.inner() else { return };
        let max = (inner.total_width.get() as f32 - inner.end_thumb.get().0 / 2.0).max(0.0);
        let min = (inner.start_thumb.get().0 / 2.0).min(max);
        if !inner.is_dragging.get() {
            inner.min_px.set(min);
            inner.max_px.set(max);
            inner.raw_start.set(self.scale_to_offset(min, max, self.start_value()));
            inner.raw_end.set(self.scale_to_offset(min, max, self.end_value()));
        }
    }

    /// As `onDrag`: `offset` further for one thumb; the new range, if it's another.
    fn on_drag(&self, is_start: bool, offset: f32) -> Option<(f32, f32)> {
        let inner = self.inner()?;
        let (min, max) = (inner.min_px.get(), inner.max_px.get());
        let (start_value, end_value) = (self.start_value(), self.end_value());
        let (track_start, track_end) = inner.track_range;
        let snap = |value: f32| snap_to_tick(value, inner.steps, track_start, track_end);
        let range = if is_start {
            inner.raw_start.set(inner.raw_start.get() + offset);
            inner.raw_end.set(self.scale_to_offset(min, max, end_value));
            let at = inner.raw_start.get().clamp(min.min(inner.raw_end.get()), inner.raw_end.get());
            (snap(self.scale_to_user(min, max, at)).min(end_value), end_value)
        } else {
            inner.raw_end.set(inner.raw_end.get() + offset);
            inner.raw_start.set(self.scale_to_offset(min, max, start_value));
            let at =
                inner.raw_end.get().clamp(inner.raw_start.get(), max.max(inner.raw_start.get()));
            (start_value, snap(self.scale_to_user(min, max, at)).max(start_value))
        };
        (range != (start_value, end_value)).then_some(range)
    }

    /// Saves its values, as `RangeSliderState.Saver(steps, trackRange)`.
    pub fn saver(
        steps: u32,
        track_range: (f32, f32),
    ) -> impl compose_runtime::Saver<Owned<RangeSliderState>> {
        compose_runtime::saver(
            |state: &Owned<RangeSliderState>| {
                state.inner()?;
                Some(compose_runtime::SavedValue::List(vec![
                    compose_runtime::SavedValue::Float(state.start_value() as f64),
                    compose_runtime::SavedValue::Float(state.end_value() as f64),
                ]))
            },
            move |saved| match saved {
                compose_runtime::SavedValue::List(values) => match values.as_slice() {
                    [
                        compose_runtime::SavedValue::Float(start),
                        compose_runtime::SavedValue::Float(end),
                    ] => {
                        Some(RangeSliderState::new(*start as f32, *end as f32, steps, track_range))
                    }
                    _ => None,
                },
                _ => None,
            },
        )
    }
}

/// A range slider state remembered here, as `rememberRangeSliderState`: saved across the host's
/// recreation.
#[composable(non_restartable)]
pub fn remember_range_slider_state(
    #[default(0.0)] start_value: f32,
    #[default(1.0)] end_value: f32,
    #[default(0)] steps: u32,
    #[default((0.0, 1.0))] track_range: (f32, f32),
) -> RangeSliderState {
    compose_runtime::remember_saveable_with(
        RangeSliderState::saver(steps, track_range),
        move || RangeSliderState::new(start_value, end_value, steps, track_range),
    )
}

impl SliderDefaults {
    /// A range slider's track, as `SliderDefaults.Track(rangeSliderState, modifier, enabled,
    /// colors)`: the active part between the thumbs, a gap around each, the inactive parts outside
    /// with a stop dot at each end.
    pub fn range_track(
        state: RangeSliderState,
        modifier: Modifier,
        enabled: bool,
        colors: SliderColors,
    ) {
        SliderDefaults::range_track_with(
            state,
            modifier,
            enabled,
            colors,
            Some(SliderDefaults::default_draw_stop_indicator(enabled, colors)),
            SliderDefaults::default_draw_tick(),
            THUMB_TRACK_GAP_SIZE,
            TRACK_INSIDE_CORNER_SIZE,
            Dp::UNSPECIFIED,
        );
    }

    /// As `SliderDefaults.Track` for a range slider with all its parameters; an unspecified
    /// `track_corner_size` is half the track's height.
    #[allow(clippy::too_many_arguments)]
    pub fn range_track_with(
        state: RangeSliderState,
        modifier: Modifier,
        enabled: bool,
        colors: SliderColors,
        draw_stop_indicator: Option<DrawSliderStopIndicator>,
        draw_tick: DrawSliderTick,
        thumb_track_gap_size: Dp,
        track_inside_corner_size: Dp,
        track_corner_size: Dp,
    ) {
        let inset_ring = uses_inset_focus_ring();
        let focus_padding = |focused: bool| {
            if inset_ring && focused { INSET_FOCUS_RING_PADDING } else { Dp(0.0) }
        };
        let start_gap = thumb_track_gap_size
            + focus_padding(state.inner().is_some_and(|inner| inner.is_start_focused.get()));
        let end_gap = thumb_track_gap_size
            + focus_padding(state.inner().is_some_and(|inner| inner.is_end_focused.get()));
        let modifier = modifier.fill_max_width().height(TRACK_HEIGHT).layout(
            move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                if let Some(inner) = state.inner() {
                    inner.track_corner.set(placeable.height / 2);
                }
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            },
        );
        compose_foundation::layout::Canvas(modifier, move |scope| {
            let Some(inner) = state.inner() else { return };
            let density = scope.density();
            draw_track(
                scope,
                &TrackDrawing {
                    ticks: &tick_fractions(state.steps()),
                    active_range: (state.start_fraction(), state.end_fraction()),
                    enabled,
                    colors: &colors,
                    start_thumb: inner.start_thumb.get(),
                    end_thumb: inner.end_thumb.get(),
                    start_gap,
                    end_gap,
                    inside_corner: density.to_px(track_inside_corner_size),
                    corner: if track_corner_size.is_specified() {
                        density.to_px(track_corner_size)
                    } else {
                        scope.size().height / 2.0
                    },
                    draw_stop_indicator: draw_stop_indicator.as_ref(),
                    draw_tick: &draw_tick,
                    is_range: true,
                    corner_shrinking: false,
                    vertical: false,
                    is_centered: false,
                },
            );
        });
    }
}

/// A slider with two thumbs picking a range, as Compose's `RangeSlider(state, onValueChange, …)`:
/// each change goes to `on_value_change` as (start, end) (set the state's values there: it shows
/// them), and `on_value_change_finished` hears the end of a gesture. A gesture takes the nearer
/// thumb; two at about the same place, the one it moves towards. Each thumb is focusable, with its
/// own keys, and a node of its own for accessibility.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn RangeSlider(
    state: RangeSliderState,
    on_value_change: impl Fn((f32, f32)) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] on_value_change_finished: Option<impl Fn() + 'static>,
    #[default(crate::SliderDefaults::colors())] colors: SliderColors,
    #[default(None)] start_thumb_interaction_source: Option<MutableInteractionSource>,
    #[default(None)] end_thumb_interaction_source: Option<MutableInteractionSource>,
    #[default(None)] start_thumb: Option<impl Fn(RangeSliderState) + 'static>,
    #[default(None)] end_thumb: Option<impl Fn(RangeSliderState) + 'static>,
    #[default(None)] track: Option<impl Fn(RangeSliderState) + 'static>,
) {
    let start_source =
        crate::components::remembered_interaction_source(start_thumb_interaction_source);
    let end_source = crate::components::remembered_interaction_source(end_thumb_interaction_source);
    let (start_focused, end_focused) = (start_source.is_focused(), end_source.is_focused());
    if let Some(inner) = state.inner() {
        inner.is_start_focused.set(start_focused);
        inner.is_end_focused.set(end_focused);
    }
    let inset_ring = uses_inset_focus_ring();
    let padding_of =
        |focused: bool| if inset_ring && focused { INSET_FOCUS_RING_PADDING } else { Dp(0.0) };
    let (start_padding, end_padding) = (padding_of(start_focused), padding_of(end_focused));
    let finished: Option<Callback> = on_value_change_finished.map(compose_runtime::callback);
    let on_value_change: Callback<dyn Fn((f32, f32))> = compose_runtime::callback(on_value_change);
    let thumb_modifier = |is_start: bool, source: &MutableInteractionSource, padding: Dp| {
        let modifier = if inset_ring {
            Modifier.indication(source.clone(), thumb_focus_ring()).padding(padding)
        } else {
            Modifier
        };
        increase_semantics_bounds(modifier.wrap_content_width(), true).then_custom(RangeThumb {
            state,
            is_start,
            on_value_change: on_value_change.clone(),
            on_value_change_finished: finished.clone(),
            enabled,
            source: source.clone(),
        })
    };
    let start_modifier = thumb_modifier(true, &start_source, start_padding);
    let end_modifier = thumb_modifier(false, &end_source, end_padding);
    let modifier = minimum_interactive_component_size(modifier).size_in(SizeSpec {
        enforce_incoming: false,
        ..SizeSpec::bounds(THUMB_WIDTH, TRACK_HEIGHT, Dp::UNSPECIFIED, Dp::UNSPECIFIED)
    });
    // Gone with `enabled`, as `rangeSliderPressDragModifier`: its gesture is cancelled.
    let modifier = if enabled {
        modifier.then_custom(RangePressDrag {
            state,
            start_source: start_source.clone(),
            end_source: end_source.clone(),
            on_value_change: on_value_change.clone(),
            on_value_change_finished: finished.clone(),
        })
    } else {
        modifier
    };
    let policy = compose_runtime::remember_keyed((state, start_padding, end_padding), move || {
        Rc::new(RangeSliderLayout { state, start_padding, end_padding }) as Rc<dyn MeasurePolicy>
    });
    let default_thumb = move |source: MutableInteractionSource| {
        compose_runtime::callback(move |_: RangeSliderState| {
            SliderDefaults::thumb(
                source.clone(),
                Modifier,
                colors,
                enabled,
                SliderDefaults::THUMB_SIZE,
            );
        })
    };
    let start_thumb: Callback<dyn Fn(RangeSliderState)> = match start_thumb {
        Some(thumb) => compose_runtime::callback(thumb),
        None => default_thumb(start_source),
    };
    let end_thumb: Callback<dyn Fn(RangeSliderState)> = match end_thumb {
        Some(thumb) => compose_runtime::callback(thumb),
        None => default_thumb(end_source),
    };
    let track: Callback<dyn Fn(RangeSliderState)> = match track {
        Some(track) => compose_runtime::callback(track),
        None => compose_runtime::callback(move |state: RangeSliderState| {
            SliderDefaults::range_track(state, Modifier, enabled, colors);
        }),
    };
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            let (start_thumb, end_thumb, track) =
                (start_thumb.clone(), end_thumb.clone(), track.clone());
            Box(
                start_modifier.clone(),
                Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| start_thumb(state)),
            );
            Box(
                end_modifier.clone(),
                Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| end_thumb(state)),
            );
            Box(
                Modifier,
                Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| track(state)),
            );
        }),
    );
}

/// As `RangeSliderImpl`'s measure policy: the thumbs, the track as wide as what half of each leaves,
/// half the start thumb in, each thumb at its value (at the stops, past the track's corners).
struct RangeSliderLayout {
    state: RangeSliderState,
    start_padding: Dp,
    end_padding: Dp,
}

impl MeasurePolicy for RangeSliderLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [start_thumb, end_thumb, track] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let density = scope.density();
        let start_padding = density.round_to_px(self.start_padding);
        let end_padding = density.round_to_px(self.end_padding);
        let start_thumb = scope.measure(*start_thumb, constraints);
        let end_thumb = scope.measure(*end_thumb, constraints);
        let start_core = start_thumb.width - 2 * start_padding;
        let end_core = end_thumb.width - 2 * end_padding;
        let thumbs = (start_core + end_core) / 2;
        let track = scope.measure(
            *track,
            Constraints::new(
                (constraints.min_width - thumbs).max(0),
                if constraints.max_width == Constraints::INFINITY {
                    Constraints::INFINITY
                } else {
                    (constraints.max_width - thumbs).max(0)
                },
                0,
                constraints.max_height,
            ),
        );
        let width = track.width + thumbs;
        let height = track
            .height
            .max(start_thumb.height - 2 * start_padding)
            .max(end_thumb.height - 2 * end_padding);
        let Some(inner) = self.state.inner() else { return scope.layout(width, height) };
        inner.start_thumb.set((start_core as f32, (start_thumb.height - 2 * start_padding) as f32));
        inner.end_thumb.set((end_core as f32, (end_thumb.height - 2 * end_padding) as f32));
        inner.total_width.set(width);
        self.state.update_min_max();
        let corner = inner.track_corner.get();
        // Read while measuring: new values place the thumbs again.
        let ticks = tick_fractions(inner.steps);
        // With different widths, the end thumb's centered by this much more.
        let end_correction = ((start_core - end_core) / 2) as f32;
        let along = |value: f32, correction: f32| {
            let on_end = ticks.first() == Some(&value) || ticks.last() == Some(&value);
            if !ticks.is_empty() && !on_end {
                (((track.width - corner * 2) as f32 * value + correction).round() as i32) + corner
            } else {
                (track.width as f32 * value + correction).round() as i32
            }
        };
        let start_x = along(self.state.start_fraction(), 0.0);
        let end_x = along(self.state.end_fraction(), end_correction);
        scope.place_relative(
            &track,
            start_thumb.width / 2 - start_padding,
            (height - track.height) / 2,
        );
        scope.place_relative(
            &start_thumb,
            start_x - start_padding,
            (height - start_thumb.height) / 2,
        );
        scope.place_relative(&end_thumb, end_x - end_padding, (height - end_thumb.height) / 2);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "RangeSlider"
    }
}

/// A thumb's keys, focus and semantics, as `rangeSliderOnKeyEvents`, `focusable` and
/// `rangeSliderStartThumbSemantics`.
struct RangeThumb {
    state: RangeSliderState,
    is_start: bool,
    on_value_change: Callback<dyn Fn((f32, f32))>,
    on_value_change_finished: Option<Callback>,
    enabled: bool,
    source: MutableInteractionSource,
}

struct RangeThumbNode {
    state: RangeSliderState,
    is_start: bool,
    on_value_change: Callback<dyn Fn((f32, f32))>,
    on_value_change_finished: Option<Callback>,
    enabled: bool,
    source: MutableInteractionSource,
    focus: Option<Focus>,
    focused: bool,
}

impl ModifierNodeElement for RangeThumb {
    type Node = RangeThumbNode;
    fn create(&self) -> RangeThumbNode {
        RangeThumbNode {
            state: self.state,
            is_start: self.is_start,
            on_value_change: self.on_value_change.clone(),
            on_value_change_finished: self.on_value_change_finished.clone(),
            enabled: self.enabled,
            source: self.source.clone(),
            focus: None,
            focused: false,
        }
    }

    fn update(&self, node: &mut RangeThumbNode) {
        node.state = self.state;
        node.is_start = self.is_start;
        node.on_value_change = self.on_value_change.clone();
        node.on_value_change_finished = self.on_value_change_finished.clone();
        node.enabled = self.enabled;
        if node.source != self.source {
            node.emit_focus(false);
            node.source = self.source.clone();
        }
    }

    fn same(&self, other: &Self) -> bool {
        other.state == self.state
            && other.is_start == self.is_start
            && other.on_value_change.ptr_eq(&self.on_value_change)
            && match (&other.on_value_change_finished, &self.on_value_change_finished) {
                (Some(a), Some(b)) => a.ptr_eq(b),
                (None, None) => true,
                _ => false,
            }
            && other.enabled == self.enabled
            && other.source == self.source
    }

    fn name(&self) -> &'static str {
        "range_slider_thumb"
    }
}

impl RangeThumbNode {
    fn emit_focus(&mut self, focused: bool) {
        if focused {
            let focus = Focus::new();
            self.source.try_emit(FocusInteraction::Focus(focus));
            self.focus = Some(focus);
        } else if let Some(focus) = self.focus.take() {
            self.source.try_emit(FocusInteraction::Unfocus(focus));
        }
    }

    /// A new value for this thumb, the other's as it is.
    fn range_with(&self, value: f32) -> (f32, f32) {
        if self.is_start {
            (value, self.state.end_value())
        } else {
            (self.state.start_value(), value)
        }
    }
}

impl ModifierNode for RangeThumbNode {
    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.focused = false;
    }
    fn on_detach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.emit_focus(false);
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl FocusTargetModifierNode for RangeThumbNode {
    fn focusability(&self) -> compose_ui::Focusability {
        if self.enabled {
            compose_ui::Focusability::ALWAYS
        } else {
            compose_ui::Focusability::NEVER
        }
    }
    fn on_focus_changed(
        &mut self,
        _previous: compose_ui::FocusState,
        current: compose_ui::FocusState,
        _context: &mut EventContext<'_>,
    ) {
        let focused = current.is_focused();
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        self.emit_focus(focused);
    }
}

impl KeyInputModifierNode for RangeThumbNode {
    /// Left and Right a step (a hundredth of the range without steps), Page Up and Down a tenth of
    /// the steps (1 to 10), Home and End to the ends this thumb can reach: the other thumb's its
    /// limit; the change finishes when the key's released.
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        if !self.enabled {
            return false;
        }
        let handled = matches!(
            event.key,
            Key::DirectionRight
                | Key::DirectionLeft
                | Key::PageUp
                | Key::PageDown
                | Key::MoveHome
                | Key::MoveEnd
        );
        if !handled {
            return false;
        }
        if event.kind == KeyEventType::KeyUp {
            if let Some(callback) = &self.on_value_change_finished {
                context.call(callback);
            }
            return true;
        }
        let (start, end) = self.state.track_range();
        let (value_start, value_end) = (self.state.start_value(), self.state.end_value());
        let steps = self.state.steps();
        let actual_steps = if steps > 0 { steps + 1 } else { 100 };
        let delta = (end - start).abs() / actual_steps as f32;
        let page = (actual_steps / 10).clamp(1, 10) as f32;
        let sign =
            if context.layout_direction() == compose_ui::LayoutDirection::Rtl { -1.0 } else { 1.0 };
        let (value, low, high) = if self.is_start {
            (value_start, start, value_end)
        } else {
            (value_end, value_start, end)
        };
        let target = match event.key {
            Key::DirectionRight => value + sign * delta,
            Key::DirectionLeft => value - sign * delta,
            Key::PageUp => value + page * delta,
            Key::PageDown => value - page * delta,
            Key::MoveHome => low,
            Key::MoveEnd => high,
            _ => return false,
        };
        let range = self.range_with(target.clamp(low, high));
        let callback = self.on_value_change.clone();
        context.run(move || callback(range));
        true
    }
}

impl SemanticsModifierNode for RangeThumbNode {
    /// A node of its own, named, in the range this thumb can reach, as
    /// `rangeSliderStartThumbSemantics` and its sibling.
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        let state = self.state;
        let is_start = self.is_start;
        let (track_start, track_end) = state.track_range();
        let (range, steps, value) = if is_start {
            ((track_start, state.end_value()), state.start_steps(), state.start_value())
        } else {
            ((state.start_value(), track_end), state.end_steps(), state.end_value())
        };
        config.is_merging_semantics_of_descendants = true;
        config.set_content_description(if is_start { "Range start" } else { "Range end" });
        if self.enabled {
            config.set_focused(self.focused);
            config.request_focus_of_node();
        } else {
            config.disabled();
        }
        if track_start < 0.0 {
            config.set_state_description(format_for_semantics(value));
        }
        config.set_progress_bar_range_info(compose_ui::ProgressBarRangeInfo {
            current: value.clamp(range.0, range.1.max(range.0)),
            range,
            steps,
        });
        let on_value_change = self.on_value_change.clone();
        let finished = self.on_value_change_finished.clone();
        config.set_progress(None, move |target| {
            let (range, steps, value) = if is_start {
                ((track_start, state.end_value()), state.start_steps(), state.start_value())
            } else {
                ((state.start_value(), track_end), state.end_steps(), state.end_value())
            };
            let resolved = nearest_stop(range, steps, target);
            if resolved == value {
                return false;
            }
            let new_range = if is_start {
                (resolved, state.end_value())
            } else {
                (state.start_value(), resolved)
            };
            on_value_change(new_range);
            if let Some(finished) = &finished {
                finished();
            }
            true
        });
    }
}

/// The slider's gesture, as `rangeSliderPressDragModifier`.
struct RangePressDrag {
    state: RangeSliderState,
    start_source: MutableInteractionSource,
    end_source: MutableInteractionSource,
    on_value_change: Callback<dyn Fn((f32, f32))>,
    on_value_change_finished: Option<Callback>,
}

/// Where a gesture is.
enum Gesture {
    Idle,
    /// Down, not yet past the slop, its press on the thumb it would take.
    Down {
        pointer: PointerId,
        last: Offset,
        x: f32,
        is_start: bool,
        press: Press,
        over: Offset,
    },
    /// Dragging a thumb.
    Dragging {
        pointer: PointerId,
        last: Offset,
        is_start: bool,
        start: DragStart,
    },
    /// Not ours (it went across): nothing until it's up.
    Waiting {
        pointer: PointerId,
    },
}

struct RangePressDragNode {
    state: RangeSliderState,
    start_source: MutableInteractionSource,
    end_source: MutableInteractionSource,
    on_value_change: Callback<dyn Fn((f32, f32))>,
    on_value_change_finished: Option<Callback>,
    gesture: Gesture,
    /// Other fingers down during a gesture, and where.
    others: Vec<(compose_ui::PointerId, Offset)>,
}

impl ModifierNodeElement for RangePressDrag {
    type Node = RangePressDragNode;
    fn create(&self) -> RangePressDragNode {
        RangePressDragNode {
            state: self.state,
            start_source: self.start_source.clone(),
            end_source: self.end_source.clone(),
            on_value_change: self.on_value_change.clone(),
            on_value_change_finished: self.on_value_change_finished.clone(),
            gesture: Gesture::Idle,
            others: Vec::new(),
        }
    }

    fn update(&self, node: &mut RangePressDragNode) {
        // New sources or a new state start the gesture over, as `pointerInput`'s keys.
        if node.state != self.state
            || node.start_source != self.start_source
            || node.end_source != self.end_source
        {
            node.cancel();
        }
        node.state = self.state;
        node.start_source = self.start_source.clone();
        node.end_source = self.end_source.clone();
        node.on_value_change = self.on_value_change.clone();
        node.on_value_change_finished = self.on_value_change_finished.clone();
    }

    fn same(&self, other: &Self) -> bool {
        other.state == self.state
            && other.start_source == self.start_source
            && other.end_source == self.end_source
            && other.on_value_change.ptr_eq(&self.on_value_change)
            && match (&other.on_value_change_finished, &self.on_value_change_finished) {
                (Some(a), Some(b)) => a.ptr_eq(b),
                (None, None) => true,
                _ => false,
            }
    }

    fn name(&self) -> &'static str {
        "range_slider_press_drag"
    }
}

impl RangePressDragNode {
    fn source(&self, is_start: bool) -> &MutableInteractionSource {
        if is_start { &self.start_source } else { &self.end_source }
    }

    fn set_dragging(&self, dragging: bool) {
        if let Some(inner) = self.state.inner() {
            inner.is_dragging.set(dragging);
        }
    }

    /// The new range, if it's another, goes to `on_value_change`.
    fn drag(&self, is_start: bool, offset: f32, context: &mut EventContext<'_>) {
        if let Some(range) = self.state.on_drag(is_start, offset) {
            let callback = self.on_value_change.clone();
            context.run(move || callback(range));
        }
    }

    fn finish(&self, context: &mut EventContext<'_>) {
        if let Some(callback) = &self.on_value_change_finished {
            context.call(callback);
        }
    }

    /// As the gesture's `finally`: where there's no event context, a press is cancelled, and a drag
    /// too, its change finishing in a task.
    fn cancel(&mut self) {
        self.others.clear();
        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Down { is_start, press, .. } => {
                self.source(is_start).try_emit(PressInteraction::Cancel(press));
            }
            Gesture::Dragging { is_start, start, .. } => {
                self.source(is_start).try_emit(DragInteraction::Cancel(start));
                self.set_dragging(false);
                if let Some(callback) = self.on_value_change_finished.clone() {
                    compose_runtime::platform::launch(async move { callback() });
                }
            }
            Gesture::Idle | Gesture::Waiting { .. } => {}
        }
    }
}

impl ModifierNode for RangePressDragNode {
    fn on_detach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.cancel();
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for RangePressDragNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Main {
            return;
        }
        let rtl = context.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let released = event.kind == PointerEventType::Release;
        let moved = released || event.kind == PointerEventType::Move;
        // Other fingers down, for when the one dragging lifts.
        if let Gesture::Down { pointer, .. } | Gesture::Dragging { pointer, .. } = self.gesture
            && pointer != event.pointer
        {
            self.others.retain(|(other, _)| *other != event.pointer);
            if event.pressed {
                self.others.push((event.pointer, event.position));
            }
            return;
        }
        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Idle => {
                if event.kind != PointerEventType::Press {
                    return;
                }
                self.state.update_min_max();
                let Some(inner) = self.state.inner() else { return };
                let x = if rtl { bounds.width as f32 - event.position.x } else { event.position.x };
                let (raw_start, raw_end) = (inner.raw_start.get(), inner.raw_end.get());
                let (to_start, to_end) = ((raw_start - x).abs(), (raw_end - x).abs());
                // The nearer; as near, the start if it's past the press.
                let is_start = if to_start != to_end { to_start < to_end } else { raw_start > x };
                let press = Press::new(event.position);
                self.source(is_start).try_emit(PressInteraction::Press(press));
                self.gesture = Gesture::Down {
                    pointer: event.pointer,
                    last: event.position,
                    x,
                    is_start,
                    press,
                    over: Offset::ZERO,
                };
            }
            Gesture::Down { pointer, last, x, is_start, press, over } => {
                if event.pointer != pointer || !moved {
                    self.gesture = Gesture::Down { pointer, last, x, is_start, press, over };
                    return;
                }
                let over = over + (event.position - last);
                let slop = context.view_configuration().touch_slop;
                if !released && over.x * over.x + over.y * over.y < slop * slop {
                    self.gesture =
                        Gesture::Down { pointer, last: event.position, x, is_start, press, over };
                    return;
                }
                let Some(inner) = self.state.inner() else { return };
                if !released && over.x.abs() > over.y.abs() {
                    // A drag: the press is over, the event taken so a horizontal scroll around
                    // doesn't start too.
                    event.consume();
                    self.source(is_start).try_emit(PressInteraction::Cancel(press));
                    let (raw_start, raw_end) = (inner.raw_start.get(), inner.raw_end.get());
                    // Thumbs on top of each other: the one it moves towards.
                    let is_start = if (raw_end - x).abs() < slop && (raw_start - x).abs() < slop {
                        if rtl { over.x >= 0.0 } else { over.x < 0.0 }
                    } else {
                        is_start
                    };
                    let start = DragStart::new();
                    self.source(is_start).try_emit(DragInteraction::Start(start));
                    let raw = if is_start { raw_start } else { raw_end };
                    let offset = x - raw + if rtl { -over.x } else { over.x };
                    self.drag(is_start, offset, context);
                    self.set_dragging(true);
                    self.gesture =
                        Gesture::Dragging { pointer, last: event.position, is_start, start };
                } else if released {
                    self.source(is_start).try_emit(PressInteraction::Release(press));
                    let raw = if is_start { inner.raw_start.get() } else { inner.raw_end.get() };
                    self.drag(is_start, x - raw, context);
                    self.finish(context);
                } else {
                    // Across it, a scroll's: the press is over.
                    self.source(is_start).try_emit(PressInteraction::Cancel(press));
                    self.gesture = Gesture::Waiting { pointer };
                }
            }
            Gesture::Dragging { pointer, last, is_start, start } => {
                if event.pointer != pointer || !moved {
                    self.gesture = Gesture::Dragging { pointer, last, is_start, start };
                    return;
                }
                if event.consumed {
                    // Something else took it, as `horizontalDrag` ends.
                    self.set_dragging(false);
                    self.finish(context);
                    self.source(is_start).try_emit(DragInteraction::Cancel(start));
                    return;
                }
                if released {
                    // Its last move isn't a drag's, as `drag`: an up ends it first. Another finger
                    // still down takes over, as `awaitDragOrUp`.
                    if let Some((other, at)) = self.others.first().copied() {
                        self.gesture =
                            Gesture::Dragging { pointer: other, last: at, is_start, start };
                        self.others.remove(0);
                        return;
                    }
                    self.set_dragging(false);
                    self.finish(context);
                    self.source(is_start).try_emit(DragInteraction::Stop(start));
                    return;
                }
                let delta = event.position.x - last.x;
                self.drag(is_start, if rtl { -delta } else { delta }, context);
                event.consume();
                self.gesture = Gesture::Dragging { pointer, last: event.position, is_start, start };
            }
            Gesture::Waiting { pointer } => {
                if !(event.pointer == pointer && released) {
                    self.gesture = Gesture::Waiting { pointer };
                }
            }
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.cancel();
    }
}
