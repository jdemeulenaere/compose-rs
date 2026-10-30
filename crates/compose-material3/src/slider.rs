//! `Slider`, as Compose's Material 3 at androidx-main's tip: a `SliderState` (value, steps,
//! range) changed by presses, drags, the keys and accessibility, drawn as a 4 by 44 dp handle
//! (narrowing to 2 dp while pressed, dragged or focused) on a 16 dp track split around it,
//! with a stop dot at the track's end and the steps' ticks. The `thumb` and `track` are slots.

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::{Box, Spacer};
use compose_runtime::{Callback, ObjectHandle, Owned, OwnedState, composable, mutable_state_of};
use compose_ui::{
    Alignment, Color, Constraints, Dp, DpSize, DragInteraction, DragStart, DrawScope, EventContext,
    Focus, FocusInteraction, FocusTargetModifierNode, IntSize, Key, KeyEvent, KeyEventType,
    KeyInputModifierNode, Layout, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier,
    ModifierNode, ModifierNodeElement, MutableInteractionSource, Offset, Paint, PointerEvent,
    PointerEventPass, PointerEventType, PointerIcon, PointerId, PointerInputModifierNode, Press,
    PressInteraction, Rect, SemanticsConfiguration, SemanticsModifierNode, SizeSpec, UnitExt,
};

use crate::components::minimum_interactive_component_size;
use crate::theme::MaterialTheme;

// ---- State -------------------------------------------------------------------------------------

struct SliderInner {
    value: OwnedState<f32>,
    steps: u32,
    track_range: (f32, f32),
    should_auto_snap: Cell<bool>,
    is_dragging: OwnedState<bool>,
    /// Whether it holds the focus, as `isFocused`: what's padded for an inset focus ring.
    is_focused: OwnedState<bool>,
    /// The thumb's size, its focus padding apart, and the slider's width, as the last layout
    /// measured them.
    thumb_size: Cell<IntSize>,
    total_width: Cell<i32>,
    /// The track's corner, as its `CornerSizeAlignmentLine` (the default track sets it).
    track_corner: Cell<i32>,
}

/// A slider's state, as Compose's `SliderState`: its value in `track_range`, snapped to `steps`
/// stops between the ends (with `should_auto_snap`), and whether it's being dragged.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and [`remember_slider_state`]
/// keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SliderState {
    object: ObjectHandle<SliderInner>,
}

impl compose_runtime::ComposeArg for SliderState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SliderState {
    /// As `SliderState(value, steps, trackRange)`.
    #[track_caller]
    pub fn new(value: f32, steps: u32, track_range: (f32, f32)) -> Owned<SliderState> {
        // As Compose's: the first value is kept as given (only the setter coerces and snaps).
        let inner = SliderInner {
            value: mutable_state_of(value),
            steps,
            track_range,
            should_auto_snap: Cell::new(true),
            is_dragging: mutable_state_of(false),
            is_focused: mutable_state_of(false),
            thumb_size: Cell::new(IntSize::ZERO),
            total_width: Cell::new(0),
            track_corner: Cell::new(0),
        };
        Owned::new(inner, |object| SliderState { object })
    }

    #[track_caller]
    fn inner(&self) -> Option<Rc<SliderInner>> {
        self.object.get()
    }

    /// The value (a state read), as `value`.
    pub fn value(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.value.get())
    }

    /// Sets the value, snapped to the nearest stop (in the range) unless `should_auto_snap` is
    /// off, as `value`'s setter.
    pub fn set_value(&self, value: f32) {
        let Some(inner) = self.inner() else { return };
        let value = if inner.should_auto_snap.get() { self.snapped(value) } else { value };
        if inner.value.get() != value {
            inner.value.set(value);
        }
    }

    /// The stops between the ends, as `steps`.
    pub fn steps(&self) -> u32 {
        self.inner().map_or(0, |inner| inner.steps)
    }

    /// The values' range, as `trackRange`.
    pub fn track_range(&self) -> (f32, f32) {
        self.inner().map_or((0.0, 1.0), |inner| inner.track_range)
    }

    /// Whether a value set is snapped to a stop, as `shouldAutoSnap` (on by default).
    pub fn should_auto_snap(&self) -> bool {
        self.inner().is_some_and(|inner| inner.should_auto_snap.get())
    }

    pub fn set_should_auto_snap(&self, snap: bool) {
        if let Some(inner) = self.inner() {
            inner.should_auto_snap.set(snap);
        }
    }

    /// The value as a fraction of the range, 0 to 1 (a state read), as
    /// `coercedValueAsFraction`.
    pub fn coerced_value_as_fraction(&self) -> f32 {
        let (start, end) = self.track_range();
        fraction(start, end, self.value().clamp(start.min(end), start.max(end)))
    }

    /// Whether it's being dragged (a state read), as `isDragging`.
    pub fn is_dragging(&self) -> bool {
        self.inner().is_some_and(|inner| inner.is_dragging.get())
    }

    /// The stops as fractions of the track (none without steps), as `tickFractions`.
    fn tick_fractions(&self) -> Vec<f32> {
        tick_fractions(self.steps())
    }

    /// `value` in the range, at the nearest stop, as `calculateSnappedValue`.
    fn snapped(&self, value: f32) -> f32 {
        let (start, end) = self.track_range();
        let value = value.clamp(start.min(end), start.max(end));
        snap_to_tick(value, self.steps(), start, end)
    }

    /// Saves its value, as `SliderState.Saver(steps, trackRange)`.
    pub fn saver(
        steps: u32,
        track_range: (f32, f32),
    ) -> impl compose_runtime::Saver<Owned<SliderState>> {
        compose_runtime::saver(
            |state: &Owned<SliderState>| {
                state.inner()?;
                Some(compose_runtime::SavedValue::Float(state.value() as f64))
            },
            move |saved| match saved {
                compose_runtime::SavedValue::Float(value) => {
                    Some(SliderState::new(*value as f32, steps, track_range))
                }
                _ => None,
            },
        )
    }
}

/// A slider state remembered here, as `rememberSliderState(value, steps, trackRange)`: saved
/// across the host's recreation (a `rememberSaveable`, as Compose's).
#[composable(non_restartable)]
pub fn remember_slider_state(
    #[default(0.0)] value: f32,
    #[default(0)] steps: u32,
    #[default((0.0, 1.0))] track_range: (f32, f32),
) -> SliderState {
    compose_runtime::remember_saveable_with(SliderState::saver(steps, track_range), move || {
        SliderState::new(value, steps, track_range)
    })
}

/// The stops as fractions, as `stepsToTickFractions`: none without steps, else the ends and
/// `steps` between.
fn tick_fractions(steps: u32) -> Vec<f32> {
    if steps == 0 {
        return Vec::new();
    }
    let count = steps + 2;
    (0..count).map(|index| index as f32 / (count - 1) as f32).collect()
}

/// The nearest stop's value to `value`, as `snapValueToTick`.
fn snap_to_tick(value: f32, steps: u32, start: f32, end: f32) -> f32 {
    if steps == 0 {
        return value;
    }
    let total = (steps + 1) as f64;
    (0..=steps + 1)
        .map(|index| (start as f64 + (end as f64 - start as f64) * index as f64 / total) as f32)
        .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()))
        .unwrap_or(value)
}

/// `position`'s fraction between `a` and `b`, 0 to 1, as `calcFraction`.
fn fraction(a: f32, b: f32, position: f32) -> f32 {
    (if b - a == 0.0 { 0.0 } else { (position - a) / (b - a) }).clamp(0.0, 1.0)
}

// ---- Colors and defaults ---------------------------------------------------------------------

/// A slider's colors, as Compose's `SliderColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SliderColors {
    pub thumb_color: Color,
    pub active_track_color: Color,
    pub active_tick_color: Color,
    pub inactive_track_color: Color,
    pub inactive_tick_color: Color,
    pub disabled_thumb_color: Color,
    pub disabled_active_track_color: Color,
    pub disabled_active_tick_color: Color,
    pub disabled_inactive_track_color: Color,
    pub disabled_inactive_tick_color: Color,
}

impl compose_runtime::ComposeArg for SliderColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SliderColors {
    pub fn thumb_color(&self, enabled: bool) -> Color {
        if enabled { self.thumb_color } else { self.disabled_thumb_color }
    }

    pub fn track_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_track_color,
            (true, false) => self.inactive_track_color,
            (false, true) => self.disabled_active_track_color,
            (false, false) => self.disabled_inactive_track_color,
        }
    }

    pub fn tick_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_tick_color,
            (true, false) => self.inactive_tick_color,
            (false, true) => self.disabled_active_tick_color,
            (false, false) => self.disabled_inactive_tick_color,
        }
    }
}

/// As Compose's `SliderDefaults`.
pub struct SliderDefaults;

/// The track's height, as `TrackHeight` (`SliderTokens.InactiveTrackHeight`).
const TRACK_HEIGHT: Dp = Dp(16.0);
/// Around the handle and added to its gaps while an inset focus ring shows, as
/// `insetFocusRingPadding`.
const INSET_FOCUS_RING_PADDING: Dp = Dp(4.0);
/// The handle's width and height, as `ThumbWidth` and `ThumbHeight`.
const THUMB_WIDTH: Dp = Dp(4.0);
const THUMB_HEIGHT: Dp = Dp(44.0);
/// Between the handle and the track, as `ThumbTrackGapSize`
/// (`SliderTokens.ActiveHandleLeadingSpace`).
const THUMB_TRACK_GAP_SIZE: Dp = Dp(6.0);
/// The track's corners next to the handle, as `TrackInsideCornerSize`.
const TRACK_INSIDE_CORNER_SIZE: Dp = Dp(2.0);

impl SliderDefaults {
    /// The handle's size, as `ThumbSize`.
    pub const THUMB_SIZE: DpSize = DpSize::new(THUMB_WIDTH, THUMB_HEIGHT);
    /// The stop dot at the track's end, as `TrackStopIndicatorSize`.
    pub const TRACK_STOP_INDICATOR_SIZE: Dp = Dp(4.0);
    /// The steps' ticks, as `TickSize`.
    pub const TICK_SIZE: Dp = Dp(4.0);

    /// From the theme, as `SliderDefaults.colors()` (`SliderTokens`): the handle and the active
    /// track in the primary color, the inactive track in the secondary container, the ticks in
    /// the other track's color; disabled, on-surface at 38 % (the handle composited over the
    /// surface) and 12 % for the inactive track.
    pub fn colors() -> SliderColors {
        let scheme = MaterialTheme::color_scheme();
        SliderColors {
            thumb_color: scheme.primary,
            active_track_color: scheme.primary,
            active_tick_color: scheme.secondary_container,
            inactive_track_color: scheme.secondary_container,
            inactive_tick_color: scheme.primary,
            disabled_thumb_color: scheme.on_surface.with_alpha(0.38).composite_over(scheme.surface),
            disabled_active_track_color: scheme.on_surface.with_alpha(0.38),
            disabled_active_tick_color: scheme.on_surface.with_alpha(0.12),
            disabled_inactive_track_color: scheme.on_surface.with_alpha(0.12),
            disabled_inactive_tick_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// The handle, as `SliderDefaults.Thumb`: a `thumb_size` pill in the thumb color, half as
    /// wide while `interaction_source` has a press, a drag or a focus under way, a hand over it
    /// for a mouse.
    pub fn thumb(
        interaction_source: MutableInteractionSource,
        modifier: Modifier,
        colors: SliderColors,
        enabled: bool,
        thumb_size: DpSize,
    ) {
        let interacting = interaction_source.is_pressed()
            || interaction_source.is_dragged()
            || interaction_source.is_focused();
        let width = if interacting { thumb_size.width / 2.0 } else { thumb_size.width };
        let color = colors.thumb_color(enabled);
        Box(
            modifier
                .size(thumb_size)
                .hoverable(interaction_source)
                .pointer_hover_icon(PointerIcon::Hand),
            Alignment::CENTER,
            false,
            compose_runtime::callback(move |_| {
                Spacer(
                    Modifier
                        .size((width, thumb_size.height))
                        .background_with(color, compose_foundation::CIRCLE_SHAPE),
                );
            }),
        );
    }

    /// The track, as `SliderDefaults.Track(sliderState, modifier, enabled, colors)`: 16 dp high
    /// and as wide as allowed, the active part up to 6 dp before the handle, the inactive part
    /// from 6 dp after it to the end (2 dp corners by the handle), a stop dot at the end and the
    /// steps' ticks (none in the gap around the handle). [`track_with`](Self::track_with) takes
    /// the rest of Compose's parameters.
    pub fn track(
        slider_state: SliderState,
        modifier: Modifier,
        enabled: bool,
        colors: SliderColors,
    ) {
        SliderDefaults::track_with(
            slider_state,
            modifier,
            enabled,
            colors,
            Some(SliderDefaults::default_draw_stop_indicator(enabled, colors)),
            SliderDefaults::default_draw_tick(),
            THUMB_TRACK_GAP_SIZE,
            TRACK_INSIDE_CORNER_SIZE,
            None,
        );
    }

    /// The track with all of Compose's parameters, as `SliderDefaults.Track`'s two overloads:
    /// `draw_stop_indicator` draws the dot at the end (`None` for none; the default is
    /// [`default_draw_stop_indicator`](Self::default_draw_stop_indicator)), `draw_tick` each
    /// step's tick (the default is [`default_draw_tick`](Self::default_draw_tick)),
    /// `thumb_track_gap_size` the gap on each side of the handle (none at 0),
    /// `track_inside_corner_size` the corners by the handle. `track_corner_size` picks the
    /// overload: `None` for the one without it (corners half the track's height); `Some` for
    /// the one with it (`Dp::UNSPECIFIED` for half the height), whose parts keep drawing as they
    /// get shorter than their corners.
    #[allow(clippy::too_many_arguments)]
    pub fn track_with(
        slider_state: SliderState,
        modifier: Modifier,
        enabled: bool,
        colors: SliderColors,
        draw_stop_indicator: Option<DrawSliderStopIndicator>,
        draw_tick: DrawSliderTick,
        thumb_track_gap_size: Dp,
        track_inside_corner_size: Dp,
        track_corner_size: Option<Dp>,
    ) {
        let corner_size = track_corner_size.unwrap_or(Dp::UNSPECIFIED);
        let modifier = modifier.fill_max_width().height(TRACK_HEIGHT).layout(
            move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                // As the track's `CornerSizeAlignmentLine`: the slider places the handle at the
                // stops with it.
                let corner = if corner_size.is_specified() {
                    scope.density().round_to_px(corner_size)
                } else {
                    placeable.height / 2
                };
                if let Some(inner) = slider_state.inner() {
                    inner.track_corner.set(corner);
                }
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            },
        );
        let focus_padding = if uses_inset_focus_ring()
            && slider_state.inner().is_some_and(|inner| inner.is_focused.get())
        {
            INSET_FOCUS_RING_PADDING
        } else {
            Dp(0.0)
        };
        let style = TrackStyle {
            draw_stop_indicator,
            draw_tick,
            thumb_track_gap_size: thumb_track_gap_size + focus_padding,
            track_inside_corner_size,
            track_corner_size: corner_size,
            corner_shrinking: track_corner_size.is_some(),
        };
        compose_foundation::layout::Canvas(modifier, move |scope| {
            draw_track(scope, slider_state, enabled, &colors, &style);
        });
    }

    /// The track's default stop dot: a `TRACK_STOP_INDICATOR_SIZE` dot in the active track's
    /// color, as `Track`'s default `drawStopIndicator`.
    pub fn default_draw_stop_indicator(
        enabled: bool,
        colors: SliderColors,
    ) -> DrawSliderStopIndicator {
        DrawSliderStopIndicator::new(move |scope, offset| {
            SliderDefaults::draw_stop_indicator(
                scope,
                offset,
                SliderDefaults::TRACK_STOP_INDICATOR_SIZE,
                colors.track_color(enabled, true),
            );
        })
    }

    /// The track's default tick: a `TICK_SIZE` dot, as `Track`'s default `drawTick`.
    pub fn default_draw_tick() -> DrawSliderTick {
        DrawSliderTick::new(|scope, offset, color| {
            SliderDefaults::draw_stop_indicator(scope, offset, SliderDefaults::TICK_SIZE, color);
        })
    }

    /// A stop dot or tick: a circle `size` wide at `offset`, as `drawStopIndicator`.
    pub fn draw_stop_indicator(scope: &mut DrawScope<'_>, offset: Offset, size: Dp, color: Color) {
        let radius = scope.density().to_px(size) / 2.0;
        scope.draw_circle(offset, radius, Paint::fill(color));
    }
}

/// Draws a slider track's stop dot at an offset, as `SliderDefaults.Track`'s
/// `drawStopIndicator: DrawScope.(Offset) -> Unit`.
#[derive(Clone)]
pub struct DrawSliderStopIndicator(Rc<StopIndicatorFn>);

type StopIndicatorFn = dyn Fn(&mut DrawScope<'_>, Offset);

impl DrawSliderStopIndicator {
    pub fn new(draw: impl Fn(&mut DrawScope<'_>, Offset) + 'static) -> DrawSliderStopIndicator {
        DrawSliderStopIndicator(Rc::new(draw))
    }

    /// Draws the dot at `offset`.
    pub fn draw(&self, scope: &mut DrawScope<'_>, offset: Offset) {
        (self.0)(scope, offset);
    }
}

/// Draws a slider track's tick at an offset in a color (the active or the inactive tick
/// color), as `SliderDefaults.Track`'s `drawTick: DrawScope.(Offset, Color) -> Unit`.
#[derive(Clone)]
pub struct DrawSliderTick(Rc<TickFn>);

type TickFn = dyn Fn(&mut DrawScope<'_>, Offset, Color);

impl DrawSliderTick {
    pub fn new(draw: impl Fn(&mut DrawScope<'_>, Offset, Color) + 'static) -> DrawSliderTick {
        DrawSliderTick(Rc::new(draw))
    }

    /// Draws a tick at `offset` in `color`.
    pub fn draw(&self, scope: &mut DrawScope<'_>, offset: Offset, color: Color) {
        (self.0)(scope, offset, color);
    }
}

/// Whether the theme shows a focus as an inset ring.
fn uses_inset_focus_ring() -> bool {
    matches!(
        crate::ripple::LOCAL_RIPPLE_THEME_CONFIGURATION.current().focus,
        crate::ripple::RippleThemeFocus::InsetRing { .. }
    )
}

/// What `SliderDefaults::track_with` draws with, past the colors.
struct TrackStyle {
    draw_stop_indicator: Option<DrawSliderStopIndicator>,
    draw_tick: DrawSliderTick,
    thumb_track_gap_size: Dp,
    track_inside_corner_size: Dp,
    track_corner_size: Dp,
    corner_shrinking: bool,
}

/// A part of the track between `left` and `right`, its start's corners `start_radius` and its
/// end's `end_radius` (left and right in a left-to-right track), as `drawTrackPath`.
fn draw_track_part(
    scope: &mut DrawScope<'_>,
    left: f32,
    right: f32,
    color: Color,
    left_radius: f32,
    right_radius: f32,
) {
    let height = scope.size().height;
    scope.draw_round_rect(
        Rect::new(left, 0.0, right, height),
        [left_radius, right_radius, right_radius, left_radius],
        Paint::fill(color),
    );
}

/// The track's drawing, as Compose's `drawTrack` for a slider (not centered, not a range).
fn draw_track(
    scope: &mut DrawScope<'_>,
    state: SliderState,
    enabled: bool,
    colors: &SliderColors,
    style: &TrackStyle,
) {
    let size = scope.size();
    let density = scope.density();
    let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
    let corner = if style.track_corner_size.is_specified() {
        density.to_px(style.track_corner_size)
    } else {
        size.height / 2.0
    };
    let inside = density.to_px(style.track_inside_corner_size);
    let ticks = state.tick_fractions();
    let value = state.coerced_value_as_fraction();
    let on_end = ticks.first() == Some(&value) || ticks.last() == Some(&value);
    let width = size.width;
    let value_end = if !ticks.is_empty() && !on_end {
        (width - corner * 2.0) * value + corner
    } else {
        width * value
    };
    let thumb_width = state.inner().map_or(0, |inner| inner.thumb_size.get().width) as f32;
    let end_gap = if style.thumb_track_gap_size.0 > 0.0 {
        thumb_width / 2.0 + density.to_px(style.thumb_track_gap_size)
    } else {
        0.0
    };
    // Without shrinking corners (or with ticks), a part shorter than its corner isn't drawn.
    let keep_corners = !style.corner_shrinking || !ticks.is_empty();
    let center_y = size.height / 2.0;
    let x = |position: f32| if rtl { width - position } else { position };
    // The inactive track, after the handle.
    if value_end < width - end_gap - if keep_corners { corner } else { 0.0 } {
        let start = value_end + end_gap;
        let color = colors.track_color(enabled, false);
        if rtl {
            draw_track_part(scope, 0.0, width - start, color, corner, inside);
        } else {
            draw_track_part(scope, start, width, color, inside, corner);
        }
        if let Some(draw) = &style.draw_stop_indicator {
            draw.draw(scope, Offset::new(x(width - corner), center_y));
        }
    }
    // The active track, before the handle.
    let active_end = value_end - end_gap;
    let start_radius = if rtl { inside } else { corner };
    let threshold = if keep_corners { start_radius } else { 0.0 };
    if active_end > threshold {
        let color = colors.track_color(enabled, true);
        if rtl {
            draw_track_part(scope, width - active_end, width, color, inside, corner);
        } else {
            draw_track_part(scope, 0.0, active_end, color, corner, inside);
        }
    }
    // The ticks, but at the stop dot and in the gap around the handle.
    let (start, end) = (corner, width - corner);
    for (index, tick) in ticks.iter().enumerate() {
        if style.draw_stop_indicator.is_some() && index == ticks.len() - 1 {
            continue;
        }
        let at = start + (end - start) * tick;
        if (value_end - end_gap..=value_end + end_gap).contains(&at) {
            continue;
        }
        let active = (0.0..=active_end).contains(&at);
        let color = colors.tick_color(enabled, active);
        style.draw_tick.draw(scope, Offset::new(x(at), center_y), color);
    }
}

// ---- The slider --------------------------------------------------------------------------------

/// A slider, as Compose's `Slider(state, onValueChange, …)`: its `state`'s value picked by
/// pressing or dragging along the track (snapped to its steps), with the arrow keys when focused,
/// or by accessibility; each change goes to `on_value_change` (set the state's value there:
/// the slider shows the state's value), and `on_value_change_finished` hears the end of a
/// gesture. At least 48 dp for touch. `thumb` and `track` draw it: `None` for Compose's defaults
/// (`SliderDefaults::thumb` and `track` in `colors`). A drag follows the pointer past the
/// touch slop (the value lags the pointer by it, as Compose's).
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn Slider(
    state: SliderState,
    on_value_change: impl Fn(f32) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] on_value_change_finished: Option<impl Fn() + 'static>,
    #[default(crate::SliderDefaults::colors())] colors: SliderColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    #[default(None)] thumb: Option<impl Fn(SliderState) + 'static>,
    #[default(None)] track: Option<impl Fn(SliderState) + 'static>,
) {
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let input = SliderInput {
        state,
        on_value_change,
        on_value_change_finished,
        enabled,
        interaction_source: interaction_source.clone(),
    };
    // As Compose's: the semantics 10 dp wider on each side (`IncreaseHorizontalSemanticsBounds`),
    // the input inside.
    let modifier = minimum_interactive_component_size(modifier).size_in(SizeSpec {
        enforce_incoming: false,
        ..SizeSpec::bounds(THUMB_WIDTH, TRACK_HEIGHT, Dp::UNSPECIFIED, Dp::UNSPECIFIED)
    });
    let modifier = crate::components::increase_semantics_bounds(modifier, true).then_custom(input);
    let is_focused = interaction_source.is_focused();
    if let Some(inner) = state.inner() {
        inner.is_focused.set(is_focused);
    }
    // The focus as a ring around the thumb, where the theme shows it so, as Compose's: the thumb
    // padded while it shows, the track and the value as if it weren't.
    let inset_ring = uses_inset_focus_ring();
    let focus_padding = if inset_ring && is_focused { INSET_FOCUS_RING_PADDING } else { Dp(0.0) };
    let thumb_modifier = Modifier.wrap_content_width();
    let thumb_modifier = if inset_ring {
        thumb_modifier
            .indication(
                interaction_source.clone(),
                crate::ripple::ripple_with(crate::ripple::RippleOptions {
                    focus_ring_shape: Some(compose_foundation::CIRCLE_SHAPE),
                    enable_press_indication: false,
                    enable_focus_indication: true,
                    enable_drag_indication: false,
                    enable_hover_indication: false,
                    ..crate::ripple::RippleOptions::default()
                }),
            )
            .padding(focus_padding)
    } else {
        thumb_modifier
    };
    let policy = compose_runtime::remember_keyed((state, focus_padding), move || {
        Rc::new(SliderLayout { state, focus_padding }) as Rc<dyn MeasurePolicy>
    });
    let thumb = thumb.unwrap_or_else(|| {
        let source = interaction_source.clone();
        compose_runtime::callback(move |_: SliderState| {
            SliderDefaults::thumb(
                source.clone(),
                Modifier,
                colors,
                enabled,
                SliderDefaults::THUMB_SIZE,
            );
        })
    });
    let track = track.unwrap_or_else(|| {
        compose_runtime::callback(move |state: SliderState| {
            SliderDefaults::track(state, Modifier, enabled, colors);
        })
    });
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            let (thumb, track) = (thumb.clone(), track.clone());
            Box(
                thumb_modifier.clone(),
                Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| thumb(state)),
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

/// As Compose's `SliderImpl`'s measure policy: the thumb, then the track as wide as what's
/// left; the track half a thumb in, the thumb at the value (at the stops, past the track's
/// corners), both centered vertically.
struct SliderLayout {
    state: SliderState,
    /// What the thumb's padded by for an inset focus ring: laid out as if it weren't.
    focus_padding: Dp,
}

impl MeasurePolicy for SliderLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [thumb, track] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let padding = scope.density().round_to_px(self.focus_padding);
        // With the slider's constraints, as Compose's (its minimum 4 by 16 dp).
        let thumb = scope.measure(*thumb, constraints);
        let (thumb_width, thumb_height) = (thumb.width - 2 * padding, thumb.height - 2 * padding);
        let track = scope.measure(
            *track,
            Constraints::new(
                (constraints.min_width - thumb_width).max(0),
                if constraints.max_width == Constraints::INFINITY {
                    Constraints::INFINITY
                } else {
                    (constraints.max_width - thumb_width).max(0)
                },
                0,
                constraints.max_height,
            ),
        );
        let width = thumb_width + track.width;
        let height = track.height.max(thumb_height);
        let Some(inner) = self.state.inner() else { return scope.layout(width, height) };
        inner.thumb_size.set(IntSize::new(thumb_width, thumb_height));
        inner.total_width.set(width);
        let corner = inner.track_corner.get();
        // Read while measuring: a new value places the thumb again.
        let value = self.state.coerced_value_as_fraction();
        let ticks = self.state.tick_fractions();
        let on_end = ticks.first() == Some(&value) || ticks.last() == Some(&value);
        let offset = if !ticks.is_empty() && !on_end {
            ((track.width - corner * 2) as f32 * value).round() as i32 + corner
        } else {
            (track.width as f32 * value).round() as i32
        };
        let track_x = thumb_width / 2;
        scope.place_relative(&track, track_x, (height - track.height) / 2);
        scope.place_relative(
            &thumb,
            offset + track_x - thumb.width / 2,
            (height - thumb.height) / 2,
        );
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "Slider"
    }
}

/// The slider's input and semantics: presses, drags, keys, focus, accessibility.
struct SliderInput {
    state: SliderState,
    on_value_change: Callback<dyn Fn(f32)>,
    on_value_change_finished: Option<Callback<dyn Fn()>>,
    enabled: bool,
    interaction_source: MutableInteractionSource,
}

struct SliderInputNode {
    state: SliderState,
    on_value_change: Callback<dyn Fn(f32)>,
    on_value_change_finished: Option<Callback<dyn Fn()>>,
    enabled: bool,
    interaction_source: MutableInteractionSource,
    /// The pointer of the gesture under way and where it went down: other fingers are ignored,
    /// as `detectTapGestures` and `draggable` follow their first.
    down: Option<(PointerId, Offset)>,
    /// The press's interaction, until it becomes a tap or a drag, leaves the slider, or another
    /// handler takes it.
    press: Option<Press>,
    /// The drag's interaction, while dragging.
    drag: Option<DragStart>,
    /// The focus's interaction, while focused.
    focus: Option<Focus>,
    /// Dragging: the pointer's lead over the value's position (the touch slop, in the drag's
    /// direction), as Compose's drag that starts past the slop.
    drag_lag: Option<f32>,
    focused: bool,
}

impl ModifierNodeElement for SliderInput {
    type Node = SliderInputNode;
    fn create(&self) -> SliderInputNode {
        SliderInputNode {
            state: self.state,
            on_value_change: self.on_value_change.clone(),
            on_value_change_finished: self.on_value_change_finished.clone(),
            enabled: self.enabled,
            interaction_source: self.interaction_source.clone(),
            down: None,
            press: None,
            drag: None,
            focus: None,
            drag_lag: None,
            focused: false,
        }
    }

    fn update(&self, node: &mut SliderInputNode) {
        node.state = self.state;
        node.on_value_change = self.on_value_change.clone();
        node.on_value_change_finished = self.on_value_change_finished.clone();
        if node.enabled && !self.enabled {
            // As `sliderTapModifier` dropping off and `DragGestureNode.update` when disabled: a
            // drag under way is cancelled, which finishes the value's change (`processDragCancel`
            // to `onDragStopped`).
            let dragging = node.drag_lag.is_some();
            node.end_gesture();
            if dragging {
                node.finish_later();
            }
        }
        node.enabled = self.enabled;
        if node.interaction_source != self.interaction_source {
            // What's under way ends on the old source, as `DragGestureNode.update` and
            // `FocusableNode.update` dispose theirs. The new one hears nothing until the next
            // change, and a drag goes on, unreported.
            node.end_interactions();
            node.interaction_source = self.interaction_source.clone();
        }
    }

    fn same(&self, other: &Self) -> bool {
        other.state == self.state
            && other.on_value_change.ptr_eq(&self.on_value_change)
            && match (&other.on_value_change_finished, &self.on_value_change_finished) {
                (Some(a), Some(b)) => a.ptr_eq(b),
                (None, None) => true,
                _ => false,
            }
            && other.enabled == self.enabled
            && other.interaction_source == self.interaction_source
    }

    fn name(&self) -> &'static str {
        "slider"
    }
}

impl SliderInputNode {
    /// A press's start, as `sliderTapModifier`'s `PressInteraction.Press`.
    fn start_press(&mut self, position: Offset) {
        let press = Press::new(position);
        self.interaction_source.try_emit(PressInteraction::Press(press));
        self.press = Some(press);
    }

    /// The press's end: `released`, a release, else (a drag or another handler took it, or it
    /// left the slider) a cancellation, as `tryAwaitRelease`.
    fn end_press(&mut self, released: bool) {
        if let Some(press) = self.press.take() {
            self.interaction_source.try_emit(if released {
                PressInteraction::Release(press)
            } else {
                PressInteraction::Cancel(press)
            });
        }
    }

    /// A drag's start, as `draggable`'s `DragInteraction.Start`.
    fn start_drag(&mut self) {
        let start = DragStart::new();
        self.interaction_source.try_emit(DragInteraction::Start(start));
        self.drag = Some(start);
    }

    /// The drag's end: `stopped`, a stop, else a cancellation, as `draggable`'s.
    fn end_drag(&mut self, stopped: bool) {
        if let Some(start) = self.drag.take() {
            self.interaction_source.try_emit(if stopped {
                DragInteraction::Stop(start)
            } else {
                DragInteraction::Cancel(start)
            });
        }
    }

    /// A focus gained or lost, as `focusable`'s `FocusInteraction`.
    fn emit_focus(&mut self, focused: bool) {
        if focused {
            let focus = Focus::new();
            self.interaction_source.try_emit(FocusInteraction::Focus(focus));
            self.focus = Some(focus);
        } else if let Some(focus) = self.focus.take() {
            self.interaction_source.try_emit(FocusInteraction::Unfocus(focus));
        }
    }

    /// Ends what's under way on the source, so it doesn't stay pressed, dragged or focused.
    fn end_interactions(&mut self) {
        self.end_press(false);
        self.end_drag(false);
        self.emit_focus(false);
    }

    /// The gesture is over without its release (the slider disabled or gone): its interactions
    /// end, and nothing of it is left for a later event.
    fn end_gesture(&mut self) {
        self.end_interactions();
        self.down = None;
        self.drag_lag = None;
        self.set_dragging(false);
    }

    /// The value at `x` (pixels from the slider's start), as `dispatchRawDelta`: mapped from
    /// half a thumb in to half a thumb before the end, snapped.
    fn value_at_offset(&self, x: f32) -> f32 {
        let Some(inner) = self.state.inner() else { return 0.0 };
        let total = inner.total_width.get() as f32;
        let thumb = inner.thumb_size.get().width as f32;
        let max = (total - thumb / 2.0).max(0.0);
        let min = (thumb / 2.0).min(max);
        let (start, end) = self.state.track_range();
        let value = start + (end - start) * fraction(min, max, x.clamp(min, max));
        self.state.snapped(value)
    }

    /// Asks for `value` (through `on_value_change`), if it isn't the state's, as
    /// `dispatchRawDeltaInternal`: a drag's or a tap's.
    fn change_to(&mut self, value: f32, context: &mut EventContext<'_>) {
        if value != self.state.value() {
            self.ask(value, context);
        }
    }

    /// Asks for `value` (through `on_value_change`), new or not: a key's, as Compose's
    /// `slideOnKeyEvents` calls `onValueChange` on every key down, at the ends too.
    fn ask(&mut self, value: f32, context: &mut EventContext<'_>) {
        let callback = self.on_value_change.clone();
        context.run(move || callback(value));
    }

    fn finish(&self, context: &mut EventContext<'_>) {
        if let Some(callback) = &self.on_value_change_finished {
            context.call(callback);
        }
    }

    /// `on_value_change_finished` in a task: where there's no event context, in an update or the
    /// tree in the middle of a cancellation.
    fn finish_later(&self) {
        if let Some(callback) = self.on_value_change_finished.clone() {
            compose_runtime::platform::launch(async move { callback() });
        }
    }

    fn set_dragging(&self, dragging: bool) {
        if let Some(inner) = self.state.inner()
            && inner.is_dragging.get() != dragging
        {
            inner.is_dragging.set(dragging);
        }
    }

    /// Whether `event` is outside `bounds`, extended to the minimum touch target around a smaller
    /// slider, as `isOutOfBounds(size, extendedTouchPadding)`.
    fn out_of_bounds(event: &PointerEvent, bounds: IntSize, context: &EventContext<'_>) -> bool {
        let minimum = context.view_configuration().minimum_touch_target_size;
        let density = context.density();
        let horizontal = ((density.to_px(minimum.width) - bounds.width as f32) / 2.0).max(0.0);
        let vertical = ((density.to_px(minimum.height) - bounds.height as f32) / 2.0).max(0.0);
        let position = event.position;
        position.x < -horizontal
            || position.y < -vertical
            || position.x > bounds.width as f32 + horizontal
            || position.y > bounds.height as f32 + vertical
    }

    /// The pointer's offset from the slider's start (from the right in a right-to-left layout).
    fn start_offset(&self, x: f32, width: i32, context: &EventContext<'_>) -> f32 {
        if context.layout_direction() == compose_ui::LayoutDirection::Rtl {
            width as f32 - x
        } else {
            x
        }
    }
}

impl ModifierNode for SliderInputNode {
    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        // `on_detach`, which always follows, ends the gesture.
        self.focused = false;
    }
    fn on_detach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.end_gesture();
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
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

impl PointerInputModifierNode for SliderInputNode {
    /// A tap moves the value to where it pressed; a horizontal drag past the touch slop follows
    /// the pointer (less the slop); a vertical one is left to a scrolling parent, as Compose's.
    /// The first finger only.
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Main || !self.enabled {
            return;
        }
        if self.down.is_some_and(|(pointer, _)| pointer != event.pointer) {
            return;
        }
        let x = self.start_offset(event.position.x, bounds.width, context);
        let slop = context.density().to_px(8.dp());
        match event.kind {
            PointerEventType::Press if self.down.is_none() && !event.consumed => {
                // As `detectTapGestures`, the down is consumed: a clickable around doesn't react.
                event.consume();
                self.down = Some((event.pointer, event.position));
                self.start_press(event.position);
                self.drag_lag = None;
            }
            PointerEventType::Move if self.drag_lag.is_some() => {
                event.consume();
                let value = self.value_at_offset(x - self.drag_lag.unwrap_or(0.0));
                self.change_to(value, context);
            }
            PointerEventType::Move => {
                let Some((_, start)) = self.down else { return };
                if event.consumed {
                    // A scrolling parent (or another handler) took it.
                    self.down = None;
                    self.end_press(false);
                    return;
                }
                let moved = event.position.x - start.x;
                if moved.abs() > slop {
                    // The lead is in the slider's direction: mirrored right to left.
                    let lead = slop * moved.signum();
                    let lead = if context.layout_direction() == compose_ui::LayoutDirection::Rtl {
                        -lead
                    } else {
                        lead
                    };
                    self.drag_lag = Some(lead);
                    // The drag starts, then takes the press from the tap detector: in that order,
                    // as `slider_interactionSource_resetWhenDisposed` hears them.
                    self.start_drag();
                    self.end_press(false);
                    self.set_dragging(true);
                    event.consume();
                    let value = self.value_at_offset(x - lead);
                    self.change_to(value, context);
                } else if Self::out_of_bounds(event, bounds, context) {
                    // Out of bounds, a press is cancelled and there'll be no tap, as
                    // `waitForUpOrCancellation`; a drag can still start.
                    self.end_press(false);
                }
            }
            PointerEventType::Release => {
                let Some((_, start)) = self.down.take() else { return };
                if self.drag_lag.take().is_some() {
                    event.consume();
                    self.end_drag(true);
                    self.set_dragging(false);
                    self.finish(context);
                } else if self.press.is_some() {
                    // A release something else consumed is a cancellation.
                    let released = !event.consumed;
                    self.end_press(released);
                    if released {
                        event.consume();
                        let x = self.start_offset(start.x, bounds.width, context);
                        let value = self.value_at_offset(x);
                        self.change_to(value, context);
                        self.finish(context);
                    }
                }
            }
            _ => {}
        }
    }

    /// The gesture ended without its events: a drag ends as `processDragCancel` does, through
    /// `onDragStopped` and so `on_value_change_finished`.
    fn on_cancel_pointer_input(&mut self) {
        self.down = None;
        self.end_press(false);
        let dragging = self.drag_lag.take().is_some();
        self.end_drag(false);
        if dragging {
            self.set_dragging(false);
            self.finish_later();
        }
    }
}

impl FocusTargetModifierNode for SliderInputNode {
    fn can_focus(&self) -> bool {
        self.enabled
    }
    fn on_focus_changed(&mut self, focused: bool, _context: &mut EventContext<'_>) {
        self.focused = focused;
        self.emit_focus(focused);
    }
}

impl KeyInputModifierNode for SliderInputNode {
    /// Left and Right move a step (a hundredth of the range without steps), Page Up and Down
    /// a tenth of the steps (1 to 10 of them), Home and End to the ends; the change finishes
    /// when the key is released, as Compose's `slideOnKeyEvents`. Right is towards the start in
    /// a right-to-left layout.
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
            self.finish(context);
            return true;
        }
        let steps = self.state.steps();
        let (start, end) = self.state.track_range();
        let actual_steps = if steps > 0 { steps + 1 } else { 100 };
        let delta = (end - start).abs() / actual_steps as f32;
        let page = (actual_steps / 10).clamp(1, 10) as f32;
        let sign =
            if context.layout_direction() == compose_ui::LayoutDirection::Rtl { -1.0 } else { 1.0 };
        let value = self.state.value();
        let target = match event.key {
            Key::MoveHome => start,
            Key::MoveEnd => end,
            Key::DirectionRight => value + sign * delta,
            Key::DirectionLeft => value - sign * delta,
            Key::PageUp => value + page * delta,
            Key::PageDown => value - page * delta,
            _ => return false,
        };
        self.ask(target.clamp(start.min(end), start.max(end)), context);
        true
    }
}

impl SemanticsModifierNode for SliderInputNode {
    /// Its own node, its value in its range (and its steps), `SetProgress`, as
    /// `sliderSemantics`; disabled when it is.
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        let (start, end) = self.state.track_range();
        if self.enabled {
            config.set_focused(self.focused);
            config.request_focus_of_node();
        } else {
            config.disabled();
        }
        if start < 0.0 {
            // As Compose's: the value, two decimals at most, for a range below zero.
            let value = (self.state.value() * 100.0).round() / 100.0;
            // Kotlin's float formatting: a whole number keeps its ".0".
            let text =
                if value.fract() == 0.0 { format!("{value:.1}") } else { format!("{value}") };
            config.set_state_description(text);
        }
        config.set_progress_bar_range_info(compose_ui::ProgressBarRangeInfo {
            current: self.state.value().clamp(start.min(end), start.max(end)),
            range: (start, end),
            steps: self.state.steps(),
        });
        // Moves to the value asked (in the range, at the nearest stop), as Compose's
        // `setProgress`; no change from the state's value is no action. Whether enabled or not:
        // refusing a disabled node's actions is the bridges' (D1255).
        let state = self.state;
        let on_value_change = self.on_value_change.clone();
        let finished = self.on_value_change_finished.clone();
        config.set_progress(None, move |target| {
            let value = nearest_stop(state, target);
            if value == state.value() {
                return false;
            }
            on_value_change(value);
            if let Some(finished) = &finished {
                finished();
            }
            true
        });
    }
}

/// `target` in the state's range at its nearest stop, as `sliderSemantics`' search: from a
/// distance of the target itself (`var distance: Float = newValue`: below zero no stop is that
/// near, and the target stays as is), the last of two stops as near (`<=`).
fn nearest_stop(state: SliderState, target: f32) -> f32 {
    let (start, end) = state.track_range();
    let target = target.clamp(start.min(end), start.max(end));
    let steps = state.steps();
    if steps == 0 {
        return target;
    }
    let mut value = target;
    let mut distance = target;
    for index in 0..=steps + 1 {
        let stop = start + (end - start) * (index as f32 / (steps + 1) as f32);
        if (stop - target).abs() <= distance {
            distance = (stop - target).abs();
            value = stop;
        }
    }
    value
}
