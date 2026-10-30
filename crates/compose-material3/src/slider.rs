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
    /// Which way it runs, as `orientation` and `reverseVerticalDirection`: `VerticalSlider` sets
    /// them.
    vertical: Cell<bool>,
    reverse_vertical: Cell<bool>,
    /// The thumb's size, its focus padding apart, and the slider's size, as the last layout
    /// measured them.
    thumb_size: Cell<IntSize>,
    total_width: Cell<i32>,
    total_height: Cell<i32>,
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
            vertical: Cell::new(false),
            reverse_vertical: Cell::new(false),
            thumb_size: Cell::new(IntSize::ZERO),
            total_width: Cell::new(0),
            total_height: Cell::new(0),
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

    fn is_vertical(&self) -> bool {
        self.inner().is_some_and(|inner| inner.vertical.get())
    }

    fn is_reverse_vertical(&self) -> bool {
        self.inner().is_some_and(|inner| inner.reverse_vertical.get())
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
pub(crate) fn tick_fractions(steps: u32) -> Vec<f32> {
    if steps == 0 {
        return Vec::new();
    }
    let count = steps + 2;
    (0..count).map(|index| index as f32 / (count - 1) as f32).collect()
}

/// The nearest stop's value to `value`, as `snapValueToTick`.
pub(crate) fn snap_to_tick(value: f32, steps: u32, start: f32, end: f32) -> f32 {
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
pub(crate) fn fraction(a: f32, b: f32, position: f32) -> f32 {
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
pub(crate) const TRACK_HEIGHT: Dp = Dp(16.0);
/// Around the handle and added to its gaps while an inset focus ring shows, as
/// `insetFocusRingPadding`.
pub(crate) const INSET_FOCUS_RING_PADDING: Dp = Dp(4.0);
/// The handle's width and height, as `ThumbWidth` and `ThumbHeight`.
pub(crate) const THUMB_WIDTH: Dp = Dp(4.0);
const THUMB_HEIGHT: Dp = Dp(44.0);
/// Between the handle and the track, as `ThumbTrackGapSize`
/// (`SliderTokens.ActiveHandleLeadingSpace`).
pub(crate) const THUMB_TRACK_GAP_SIZE: Dp = Dp(6.0);
/// The track's corners next to the handle, as `TrackInsideCornerSize`.
pub(crate) const TRACK_INSIDE_CORNER_SIZE: Dp = Dp(2.0);

impl SliderDefaults {
    /// The handle's size, as `ThumbSize`.
    pub const THUMB_SIZE: DpSize = DpSize::new(THUMB_WIDTH, THUMB_HEIGHT);
    /// A vertical slider's handle, as `VerticalThumbSize`.
    pub const VERTICAL_THUMB_SIZE: DpSize = DpSize::new(THUMB_HEIGHT, THUMB_WIDTH);
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
        SliderDefaults::thumb_with(
            interaction_source,
            false,
            modifier,
            colors,
            enabled,
            thumb_size,
        );
    }

    /// As `SliderDefaults.Thumb(interactionSource, isVertical, ...)`: a vertical one half as high.
    pub fn thumb_with(
        interaction_source: MutableInteractionSource,
        is_vertical: bool,
        modifier: Modifier,
        colors: SliderColors,
        enabled: bool,
        thumb_size: DpSize,
    ) {
        let interacting = interaction_source.is_pressed()
            || interaction_source.is_dragged()
            || interaction_source.is_focused();
        let size = match (interacting, is_vertical) {
            (false, _) => thumb_size,
            (true, false) => DpSize::new(thumb_size.width / 2.0, thumb_size.height),
            (true, true) => DpSize::new(thumb_size.width, thumb_size.height / 2.0),
        };
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
                        .size((size.width, size.height))
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
        track_impl(
            slider_state,
            modifier,
            enabled,
            colors,
            draw_stop_indicator,
            draw_tick,
            thumb_track_gap_size,
            track_inside_corner_size,
            track_corner_size.unwrap_or(Dp::UNSPECIFIED),
            track_corner_size.is_some(),
            false,
        );
    }

    /// The track of a slider with its active part from the middle to the handle, as
    /// `SliderDefaults.CenteredTrack(sliderState, modifier, enabled, colors)`: a stop dot at each
    /// end.
    pub fn centered_track(
        slider_state: SliderState,
        modifier: Modifier,
        enabled: bool,
        colors: SliderColors,
    ) {
        SliderDefaults::centered_track_with(
            slider_state,
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

    /// As `SliderDefaults.CenteredTrack` with all its parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn centered_track_with(
        slider_state: SliderState,
        modifier: Modifier,
        enabled: bool,
        colors: SliderColors,
        draw_stop_indicator: Option<DrawSliderStopIndicator>,
        draw_tick: DrawSliderTick,
        thumb_track_gap_size: Dp,
        track_inside_corner_size: Dp,
        track_corner_size: Dp,
    ) {
        track_impl(
            slider_state,
            modifier,
            enabled,
            colors,
            draw_stop_indicator,
            draw_tick,
            thumb_track_gap_size,
            track_inside_corner_size,
            track_corner_size,
            true,
            true,
        );
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

/// As `SliderDefaults`' `TrackImpl`: `enable_corner_shrinking` lets a part shorter than its corner
/// still draw, `is_centered` makes it a centered track.
#[allow(clippy::too_many_arguments)]
fn track_impl(
    slider_state: SliderState,
    modifier: Modifier,
    enabled: bool,
    colors: SliderColors,
    draw_stop_indicator: Option<DrawSliderStopIndicator>,
    draw_tick: DrawSliderTick,
    thumb_track_gap_size: Dp,
    track_inside_corner_size: Dp,
    track_corner_size: Dp,
    enable_corner_shrinking: bool,
    is_centered: bool,
) {
    let vertical = slider_state.is_vertical();
    let modifier = if vertical {
        let modifier = modifier.width(TRACK_HEIGHT).fill_max_height();
        // Drawn from its top: reversed, the whole of it is flipped.
        if slider_state.is_reverse_vertical() {
            modifier.graphics_layer(|layer| layer.scale_y = -1.0)
        } else {
            modifier
        }
    } else {
        modifier.fill_max_width().height(TRACK_HEIGHT)
    };
    let modifier = modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        // As the track's `CornerSizeAlignmentLine`: the slider places the handle at the stops
        // with it.
        let corner = if track_corner_size.is_specified() {
            scope.density().round_to_px(track_corner_size)
        } else if vertical {
            placeable.width / 2
        } else {
            placeable.height / 2
        };
        if let Some(inner) = slider_state.inner() {
            inner.track_corner.set(corner);
        }
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    });
    let focus_padding = if uses_inset_focus_ring()
        && slider_state.inner().is_some_and(|inner| inner.is_focused.get())
    {
        INSET_FOCUS_RING_PADDING
    } else {
        Dp(0.0)
    };
    let gap = thumb_track_gap_size + focus_padding;
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let value = slider_state.coerced_value_as_fraction();
        let density = scope.density();
        let thumb = slider_state.inner().map_or(IntSize::ZERO, |inner| inner.thumb_size.get());
        let thumb = (thumb.width as f32, thumb.height as f32);
        // A centered track's gaps go around the handle on its side of the middle only.
        let (start_thumb, end_thumb) = if is_centered {
            (
                if value > 0.5 { (0.0, 0.0) } else { thumb },
                if value < 0.5 { (0.0, 0.0) } else { thumb },
            )
        } else {
            ((0.0, 0.0), thumb)
        };
        draw_track(
            scope,
            &TrackDrawing {
                ticks: &slider_state.tick_fractions(),
                active_range: (0.0, value),
                enabled,
                colors: &colors,
                start_thumb,
                end_thumb,
                start_gap: if is_centered { gap } else { Dp(0.0) },
                end_gap: gap,
                inside_corner: density.to_px(track_inside_corner_size),
                corner: if track_corner_size.is_specified() {
                    density.to_px(track_corner_size)
                } else if vertical {
                    scope.size().width / 2.0
                } else {
                    scope.size().height / 2.0
                },
                draw_stop_indicator: draw_stop_indicator.as_ref(),
                draw_tick: &draw_tick,
                is_range: false,
                corner_shrinking: enable_corner_shrinking,
                vertical,
                is_centered,
            },
        );
    });
}

/// Whether the theme shows a focus as an inset ring.
pub(crate) fn uses_inset_focus_ring() -> bool {
    matches!(
        crate::ripple::LOCAL_RIPPLE_THEME_CONFIGURATION.current().focus,
        crate::ripple::RippleThemeFocus::InsetRing { .. }
    )
}

/// What `draw_track` draws, as the parameters of Compose's `drawTrack`.
pub(crate) struct TrackDrawing<'a> {
    pub ticks: &'a [f32],
    /// As fractions of the track.
    pub active_range: (f32, f32),
    pub enabled: bool,
    pub colors: &'a SliderColors,
    pub start_thumb: (f32, f32),
    pub end_thumb: (f32, f32),
    pub start_gap: Dp,
    pub end_gap: Dp,
    pub inside_corner: f32,
    pub corner: f32,
    pub draw_stop_indicator: Option<&'a DrawSliderStopIndicator>,
    pub draw_tick: &'a DrawSliderTick,
    pub is_range: bool,
    /// A part shorter than its corner still draws.
    pub corner_shrinking: bool,
    pub vertical: bool,
    pub is_centered: bool,
}

/// A part of the track at `offset` of `length` along it, as `drawTrackPath`: `start_radius` its
/// corners on the left (or top), `end_radius` on the right (or bottom).
fn draw_track_path(
    scope: &mut DrawScope<'_>,
    vertical: bool,
    offset: f32,
    length: f32,
    color: Color,
    start_radius: f32,
    end_radius: f32,
) {
    let size = scope.size();
    let (rect, radii) = if vertical {
        (
            Rect::new(0.0, offset, size.width, offset + length),
            [start_radius, start_radius, end_radius, end_radius],
        )
    } else {
        (
            Rect::new(offset, 0.0, offset + length, size.height),
            [start_radius, end_radius, end_radius, start_radius],
        )
    };
    scope.draw_round_rect(rect, radii, Paint::fill(color));
}

/// The track's drawing, as `SliderDefaults`' `drawTrack`, for a slider, a centered one or a range.
pub(crate) fn draw_track(scope: &mut DrawScope<'_>, track: &TrackDrawing<'_>) {
    let size = scope.size();
    let density = scope.density();
    let vertical = track.vertical;
    let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
    let rtl_horizontal = rtl && !vertical;
    let (corner, inside) = (track.corner, track.inside_corner);
    let slider_end = if vertical { size.height } else { size.width };
    let (colors, enabled) = (track.colors, track.enabled);
    let (inactive_color, active_color) =
        (colors.track_color(enabled, false), colors.track_color(enabled, true));
    let ticks = track.ticks;
    let on_tick_end =
        |fraction: f32| ticks.first() == Some(&fraction) || ticks.last() == Some(&fraction);
    let position = |fraction: f32| {
        if !ticks.is_empty() && !on_tick_end(fraction) {
            (slider_end - corner * 2.0) * fraction + corner
        } else {
            slider_end * fraction
        }
    };
    let (value_start, value_end) = (position(track.active_range.0), position(track.active_range.1));
    let gap_of = |gap: Dp, thumb: (f32, f32)| {
        if gap.0 > 0.0 {
            (if vertical { thumb.1 } else { thumb.0 }) / 2.0 + density.to_px(gap)
        } else {
            0.0
        }
    };
    let start_gap = gap_of(track.start_gap, track.start_thumb);
    let end_gap = gap_of(track.end_gap, track.end_thumb);
    let center_axis = slider_end / 2.0;
    let center_x = size.width / 2.0;
    let center_y = size.height / 2.0;
    let keep_corners = !track.corner_shrinking || !ticks.is_empty();

    // The inactive track before the active one (centered or a range).
    let threshold = start_gap + if keep_corners { corner } else { 0.0 };
    let before_end = if track.is_centered { value_end.min(center_axis) } else { value_start };
    if (track.is_centered || track.is_range) && before_end > threshold {
        let (start_radius, end_radius) =
            if rtl_horizontal { (inside, corner) } else { (corner, inside) };
        let end = before_end - start_gap;
        let offset = if rtl_horizontal { size.width - end } else { 0.0 };
        draw_track_path(scope, vertical, offset, end, inactive_color, start_radius, end_radius);
        if let Some(draw) = track.draw_stop_indicator {
            let at = if vertical {
                Offset::new(center_x, corner)
            } else if rtl {
                Offset::new(size.width - corner, center_y)
            } else {
                Offset::new(corner, center_y)
            };
            draw.draw(scope, at);
        }
    }
    // The inactive track after it.
    let threshold = slider_end - end_gap - if keep_corners { corner } else { 0.0 };
    let after_start = if track.is_centered { value_end.max(center_axis) } else { value_end };
    if after_start < threshold {
        let (start_radius, end_radius) =
            if rtl_horizontal { (corner, inside) } else { (inside, corner) };
        let start = after_start + end_gap;
        let length = slider_end - start;
        let offset = if !vertical && rtl { 0.0 } else { start };
        draw_track_path(scope, vertical, offset, length, inactive_color, start_radius, end_radius);
        if let Some(draw) = track.draw_stop_indicator {
            let at = if vertical {
                Offset::new(center_x, slider_end - corner)
            } else if rtl {
                Offset::new(corner, center_y)
            } else {
                Offset::new(slider_end - corner, center_y)
            };
            draw.draw(scope, at);
        }
    }
    // The active track.
    let plain = !track.is_centered && !track.is_range;
    let active_start = if track.is_centered {
        before_end + if before_end < center_axis { start_gap } else { 0.0 }
    } else if track.is_range {
        value_start + start_gap
    } else {
        0.0
    };
    let active_end = if track.is_centered {
        after_start - if after_start > center_axis { end_gap } else { 0.0 }
    } else {
        value_end - end_gap
    };
    let start_radius = if rtl_horizontal || !plain { inside } else { corner };
    let end_radius = if rtl_horizontal && plain { corner } else { inside };
    let active_length =
        if rtl_horizontal && plain { active_end } else { active_end - active_start };
    let threshold = if keep_corners { start_radius } else { 0.0 };
    if active_length > threshold {
        let offset = if vertical {
            active_start
        } else if rtl {
            size.width - active_end
        } else {
            active_start
        };
        draw_track_path(
            scope,
            vertical,
            offset,
            active_length,
            active_color,
            start_radius,
            end_radius,
        );
    }
    // The ticks, but at the stop dots and in the gaps around the handles and the middle.
    let (start, end) = (corner, slider_end - corner);
    let in_gap = |at: f32, center: f32, gap: f32| (center - gap..=center + gap).contains(&at);
    for (index, tick) in ticks.iter().enumerate() {
        if track.draw_stop_indicator.is_some() {
            let at_start = (track.is_centered || track.is_range) && index == 0;
            if at_start || index == ticks.len() - 1 {
                continue;
            }
        }
        let at = start + (end - start) * tick;
        let beyond = value_end > center_axis;
        let on_center =
            track.is_centered && in_gap(at, center_axis, if beyond { start_gap } else { end_gap });
        let on_start = track.is_range && in_gap(at, value_start, start_gap);
        let end_tick_gap = if track.is_centered && !beyond { start_gap } else { end_gap };
        if on_center || on_start || in_gap(at, value_end, end_tick_gap) {
            continue;
        }
        let offset = if vertical {
            Offset::new(center_x, at)
        } else if rtl {
            Offset::new(size.width - at, center_y)
        } else {
            Offset::new(at, center_y)
        };
        let active = (active_start..=active_end).contains(&at);
        track.draw_tick.draw(scope, offset, colors.tick_color(enabled, active));
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
    if let Some(inner) = state.inner() {
        inner.vertical.set(false);
    }
    let thumb =
        thumb.map(|thumb| compose_runtime::callback(thumb) as Callback<dyn Fn(SliderState)>);
    let track =
        track.map(|track| compose_runtime::callback(track) as Callback<dyn Fn(SliderState)>);
    slider_impl(
        state,
        compose_runtime::callback(on_value_change),
        modifier,
        enabled,
        on_value_change_finished.map(compose_runtime::callback),
        colors,
        interaction_source,
        thumb,
        track,
    );
}

/// A slider along its height, as Compose's `VerticalSlider`: `top_to_bottom`, its start at the top,
/// else at the bottom. It's `Slider` but for its direction: Up and Down for its keys, a thumb half
/// as high while interacted with.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn VerticalSlider(
    state: SliderState,
    on_value_change: impl Fn(f32) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] on_value_change_finished: Option<impl Fn() + 'static>,
    #[default(true)] top_to_bottom: bool,
    #[default(crate::SliderDefaults::colors())] colors: SliderColors,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    #[default(None)] thumb: Option<impl Fn(SliderState) + 'static>,
    #[default(None)] track: Option<impl Fn(SliderState) + 'static>,
) {
    if let Some(inner) = state.inner() {
        inner.vertical.set(true);
        inner.reverse_vertical.set(!top_to_bottom);
    }
    let thumb =
        thumb.map(|thumb| compose_runtime::callback(thumb) as Callback<dyn Fn(SliderState)>);
    let track =
        track.map(|track| compose_runtime::callback(track) as Callback<dyn Fn(SliderState)>);
    slider_impl(
        state,
        compose_runtime::callback(on_value_change),
        modifier,
        enabled,
        on_value_change_finished.map(compose_runtime::callback),
        colors,
        interaction_source,
        thumb,
        track,
    );
}

/// The sliders' body, as `SliderImpl`.
#[allow(clippy::too_many_arguments)]
fn slider_impl(
    state: SliderState,
    on_value_change: Callback<dyn Fn(f32)>,
    modifier: Modifier,
    enabled: bool,
    on_value_change_finished: Option<Callback>,
    colors: SliderColors,
    interaction_source: Option<MutableInteractionSource>,
    thumb: Option<Callback<dyn Fn(SliderState)>>,
    track: Option<Callback<dyn Fn(SliderState)>>,
) {
    let vertical = state.is_vertical();
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let input = SliderInput {
        state,
        on_value_change,
        on_value_change_finished,
        enabled,
        interaction_source: interaction_source.clone(),
    };
    // As Compose's: the semantics 10 dp longer at each end (`IncreaseHorizontalSemanticsBounds`,
    // `IncreaseVerticalSemanticsBounds`), the input inside.
    let (min_width, min_height) =
        if vertical { (TRACK_HEIGHT, THUMB_WIDTH) } else { (THUMB_WIDTH, TRACK_HEIGHT) };
    let modifier = minimum_interactive_component_size(modifier).size_in(SizeSpec {
        enforce_incoming: false,
        ..SizeSpec::bounds(min_width, min_height, Dp::UNSPECIFIED, Dp::UNSPECIFIED)
    });
    let modifier =
        crate::components::increase_semantics_bounds(modifier, !vertical).then_custom(input);
    let is_focused = interaction_source.is_focused();
    if let Some(inner) = state.inner() {
        inner.is_focused.set(is_focused);
    }
    // The focus as a ring around the thumb, where the theme shows it so, as Compose's: the thumb
    // padded while it shows, the track and the value as if it weren't.
    let inset_ring = uses_inset_focus_ring();
    let focus_padding = if inset_ring && is_focused { INSET_FOCUS_RING_PADDING } else { Dp(0.0) };
    let thumb_modifier =
        if vertical { Modifier.wrap_content_height() } else { Modifier.wrap_content_width() };
    let thumb_modifier = if inset_ring {
        thumb_modifier
            .indication(interaction_source.clone(), thumb_focus_ring())
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
            SliderDefaults::thumb_with(
                source.clone(),
                vertical,
                Modifier,
                colors,
                enabled,
                if vertical {
                    SliderDefaults::VERTICAL_THUMB_SIZE
                } else {
                    SliderDefaults::THUMB_SIZE
                },
            );
        })
    });
    let track = track.unwrap_or_else(|| {
        compose_runtime::callback(move |state: SliderState| {
            if vertical {
                // Its overload with corners that shrink, as `VerticalSlider`'s default.
                SliderDefaults::track_with(
                    state,
                    Modifier,
                    enabled,
                    colors,
                    Some(SliderDefaults::default_draw_stop_indicator(enabled, colors)),
                    SliderDefaults::default_draw_tick(),
                    THUMB_TRACK_GAP_SIZE,
                    TRACK_INSIDE_CORNER_SIZE,
                    Some(Dp::UNSPECIFIED),
                );
            } else {
                SliderDefaults::track(state, Modifier, enabled, colors);
            }
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

/// The ripple that's only a thumb's inset focus ring, as `focusRingModifier`.
pub(crate) fn thumb_focus_ring() -> compose_ui::Indication {
    crate::ripple::ripple_with(crate::ripple::RippleOptions {
        focus_ring_shape: Some(compose_foundation::CIRCLE_SHAPE),
        enable_press_indication: false,
        enable_focus_indication: true,
        enable_drag_indication: false,
        enable_hover_indication: false,
        ..crate::ripple::RippleOptions::default()
    })
}

/// As Compose's `SliderImpl`'s measure policy: the thumb, then the track as long as what's left;
/// the track half a thumb in, the thumb at the value (at the stops, past the track's corners), both
/// centered across it.
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
        let vertical = self.state.is_vertical();
        let padding = scope.density().round_to_px(self.focus_padding);
        // With the slider's constraints, as Compose's (its minimum 4 by 16 dp).
        let thumb = scope.measure(*thumb, constraints);
        let (thumb_width, thumb_height) = (thumb.width - 2 * padding, thumb.height - 2 * padding);
        let less = |bound: i32, by: i32| {
            if bound == Constraints::INFINITY { Constraints::INFINITY } else { (bound - by).max(0) }
        };
        let track_constraints = if vertical {
            Constraints::new(
                0,
                constraints.max_width,
                (constraints.min_height - thumb_height).max(0),
                less(constraints.max_height, thumb_height),
            )
        } else {
            Constraints::new(
                (constraints.min_width - thumb_width).max(0),
                less(constraints.max_width, thumb_width),
                0,
                constraints.max_height,
            )
        };
        let track = scope.measure(*track, track_constraints);
        let (width, height) = if vertical {
            (track.width.max(thumb_width), thumb_height + track.height)
        } else {
            (thumb_width + track.width, track.height.max(thumb_height))
        };
        let Some(inner) = self.state.inner() else { return scope.layout(width, height) };
        inner.thumb_size.set(IntSize::new(thumb_width, thumb_height));
        inner.total_width.set(width);
        inner.total_height.set(height);
        let corner = inner.track_corner.get();
        // Read while measuring: a new value places the thumb again.
        let value = self.state.coerced_value_as_fraction();
        let ticks = self.state.tick_fractions();
        let on_end = ticks.first() == Some(&value) || ticks.last() == Some(&value);
        let along = |length: i32| {
            if !ticks.is_empty() && !on_end {
                ((length - corner * 2) as f32 * value).round() as i32 + corner
            } else {
                (length as f32 * value).round() as i32
            }
        };
        if vertical {
            let track_y = thumb_height / 2;
            let offset = along(track.height);
            let offset =
                if self.state.is_reverse_vertical() { track.height - offset } else { offset };
            scope.place_relative(&track, (width - track.width) / 2, track_y);
            scope.place_relative(
                &thumb,
                (width - thumb.width) / 2,
                offset + track_y - thumb.height / 2,
            );
        } else {
            let track_x = thumb_width / 2;
            scope.place_relative(&track, track_x, (height - track.height) / 2);
            scope.place_relative(
                &thumb,
                along(track.width) + track_x - thumb.width / 2,
                (height - thumb.height) / 2,
            );
        }
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
    /// The container around it, pinned while it has the focus.
    pin: compose_ui::FocusPin,
    /// Disabled, it isn't a focus target (D1274).
    is_focus_target: bool,
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
            pin: Default::default(),
            is_focus_target: self.enabled,
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
        let vertical = inner.vertical.get();
        let total =
            if vertical { inner.total_height.get() } else { inner.total_width.get() } as f32;
        let thumb = inner.thumb_size.get();
        let thumb = if vertical { thumb.height } else { thumb.width } as f32;
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

    /// Whether it runs against its axis, as `reverseDirection`: right to left horizontally, or
    /// bottom to top.
    fn reverse_direction(&self, context: &EventContext<'_>) -> bool {
        if self.state.is_vertical() {
            self.state.is_reverse_vertical()
        } else {
            context.layout_direction() == compose_ui::LayoutDirection::Rtl
        }
    }

    /// `position` along the slider's axis.
    fn along(&self, position: Offset) -> f32 {
        if self.state.is_vertical() { position.y } else { position.x }
    }

    /// The pointer's offset from the slider's start: from its far edge, reversed.
    fn start_offset(&self, position: Offset, bounds: IntSize, context: &EventContext<'_>) -> f32 {
        let length = if self.state.is_vertical() { bounds.height } else { bounds.width };
        if self.reverse_direction(context) {
            length as f32 - self.along(position)
        } else {
            self.along(position)
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
        if self.is_focus_target { Some(self) } else { None }
    }
    fn focus_target_wanted(&self) -> Option<bool> {
        Some(self.enabled)
    }
    fn set_is_focus_target(&mut self, is_target: bool) {
        self.is_focus_target = is_target;
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for SliderInputNode {
    /// A tap moves the value to where it pressed; a drag along it past the touch slop follows the
    /// pointer (less the slop); one across it is left to a scrolling parent, as Compose's.
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
        let x = self.start_offset(event.position, bounds, context);
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
                let moved = self.along(event.position) - self.along(start);
                if moved.abs() > slop {
                    // The lead is in the slider's direction: mirrored against its axis.
                    let lead = slop * moved.signum();
                    let lead = if self.reverse_direction(context) { -lead } else { lead };
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
                        let x = self.start_offset(start, bounds, context);
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
        context: &mut EventContext<'_>,
    ) {
        let focused = current.is_focused();
        self.pin.focus_changed(focused, context);
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        self.emit_focus(focused);
        if focused {
            context.bring_into_view();
        }
    }
}

impl KeyInputModifierNode for SliderInputNode {
    /// Left and Right (Up and Down, vertically) move a step (a hundredth of the range without
    /// steps), Page Up and Down a tenth of the steps (1 to 10 of them), Home and End to the ends;
    /// the change finishes when the key is released, as Compose's `slideOnKeyEvents`. Right is
    /// towards the start in a right-to-left layout, Down in a slider going up.
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        if !self.enabled {
            return false;
        }
        let vertical = self.state.is_vertical();
        let handled = match event.key {
            Key::DirectionRight | Key::DirectionLeft => !vertical,
            Key::DirectionUp | Key::DirectionDown => vertical,
            Key::PageUp | Key::PageDown | Key::MoveHome | Key::MoveEnd => true,
            _ => false,
        };
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
        let sign = if self.reverse_direction(context) { -1.0 } else { 1.0 };
        let value = self.state.value();
        let target = match event.key {
            Key::MoveHome => start,
            Key::MoveEnd => end,
            Key::DirectionRight | Key::DirectionDown => value + sign * delta,
            Key::DirectionLeft | Key::DirectionUp => value - sign * delta,
            // A page up is towards the start of a vertical slider, the end of a horizontal one.
            Key::PageUp if vertical => value - page * sign * delta,
            Key::PageDown if vertical => value + page * sign * delta,
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
            config.set_state_description(format_for_semantics(self.state.value()));
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
            let value = nearest_stop(state.track_range(), state.steps(), target);
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

/// As `formatForSemantics`: two decimals at most, for a range below zero; Kotlin's float formatting,
/// a whole number keeping its ".0".
pub(crate) fn format_for_semantics(value: f32) -> String {
    let value = (value * 100.0).round() / 100.0;
    if value.fract() == 0.0 { format!("{value:.1}") } else { format!("{value}") }
}

/// `target` in `range` at its nearest stop, as `sliderSemantics`' search: from a distance of the
/// target itself (`var distance: Float = newValue`: below zero no stop is that near, and the target
/// stays as is), the last of two stops as near (`<=`).
pub(crate) fn nearest_stop(range: (f32, f32), steps: u32, target: f32) -> f32 {
    let (start, end) = range;
    let target = target.clamp(start.min(end), start.max(end));
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
