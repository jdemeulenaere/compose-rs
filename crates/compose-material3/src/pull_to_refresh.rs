//! `PullToRefreshBox`, `Modifier::pull_to_refresh` and the indicator, as Compose's Material 3
//! (`pulltorefresh/PullToRefresh.kt`): pulling a scrollable down past a threshold refreshes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::{Animatable, Crossfade, animate_float_as_state};
use compose_foundation::layout::{Box, BoxScope, Canvas};
use compose_runtime::{
    Callback, CoroutineScope, composable, derived_state_of, remember, remember_saveable_with,
};
use compose_ui::{
    Alignment, Color, Delegates, Density, Dp, LocalBoxFuture, Modifier, ModifierNode,
    ModifierNodeElement, NestedScrollConnection, NestedScrollSource, NodeContext, Offset, Paint,
    Path, PathFillType, ProgressBarRangeInfo, Rect, SemanticsConfiguration, SemanticsModifierNode,
    Shape,
};

use crate::theme::MaterialTheme;

const DRAG_MULTIPLIER: f32 = 0.5;
const MAX_PROGRESS_ARC: f32 = 0.8;
const STROKE_WIDTH: Dp = Dp(2.5);
const ARC_RADIUS: Dp = Dp(5.5);
const SPINNER_SIZE: Dp = Dp(16.0);
const SPINNER_CONTAINER_SIZE: Dp = Dp(40.0);
const ARROW_WIDTH: Dp = Dp(10.0);
const ARROW_HEIGHT: Dp = Dp(5.0);
// From `SwipeRefreshLayout`.
const MIN_ALPHA: f32 = 0.3;
const MAX_ALPHA: f32 = 1.0;

/// How far an indicator is pulled, as Compose's `PullToRefreshState`.
pub trait PullToRefreshState {
    /// As `distanceFraction`: 1 at the threshold (a state read).
    fn distance_fraction(&self) -> f32;
    /// As `isAnimating`.
    fn is_animating(&self) -> bool;
    /// As `animateToThreshold`.
    fn animate_to_threshold(&self) -> LocalBoxFuture<'_, ()>;
    /// As `animateToHidden`.
    fn animate_to_hidden(&self) -> LocalBoxFuture<'_, ()>;
    /// As `snapTo`.
    fn snap_to(&self, target_value: f32) -> LocalBoxFuture<'_, ()>;
}

struct PullToRefreshStateImpl {
    anim: Animatable<f32>,
}

impl PullToRefreshState for PullToRefreshStateImpl {
    fn distance_fraction(&self) -> f32 {
        self.anim.value()
    }
    fn is_animating(&self) -> bool {
        self.anim.is_running()
    }
    fn animate_to_threshold(&self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move {
            self.anim.animate_to(1.0, compose_animation::spring()).await;
        })
    }
    fn animate_to_hidden(&self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move {
            self.anim.animate_to(0.0, compose_animation::spring()).await;
        })
    }
    fn snap_to(&self, target_value: f32) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move { self.anim.snap_to(target_value) })
    }
}

/// A new state, as `PullToRefreshState()`.
pub fn pull_to_refresh_state() -> Rc<dyn PullToRefreshState> {
    Rc::new(PullToRefreshStateImpl { anim: Animatable::new(0.0) })
}

/// A state kept at its call site and saved, as `rememberPullToRefreshState`.
pub fn remember_pull_to_refresh_state() -> Rc<dyn PullToRefreshState> {
    let state: Rc<PullToRefreshStateImpl> = remember_saveable_with(
        compose_runtime::saver(
            |state: &Rc<PullToRefreshStateImpl>| {
                Some(compose_runtime::SavedValue::Float(state.anim.value() as f64))
            },
            |saved| match saved {
                compose_runtime::SavedValue::Float(value) => {
                    Some(Rc::new(PullToRefreshStateImpl { anim: Animatable::new(*value as f32) }))
                }
                _ => None,
            },
        ),
        || Rc::new(PullToRefreshStateImpl { anim: Animatable::new(0.0) }),
    );
    state
}

/// What the node keeps, shared with the scrolls it takes part in.
struct PullConnection {
    is_refreshing: Cell<bool>,
    on_refresh: RefCell<Callback>,
    enabled: Cell<bool>,
    state: RefCell<Rc<dyn PullToRefreshState>>,
    threshold: Cell<Dp>,
    density: Cell<Density>,
    scope: Cell<Option<CoroutineScope>>,
    distance_pulled: Cell<f32>,
    vertical_offset: Cell<f32>,
}

impl PullConnection {
    fn state(&self) -> Rc<dyn PullToRefreshState> {
        self.state.borrow().clone()
    }

    fn threshold_px(&self) -> f32 {
        self.density.get().round_to_px(self.threshold.get()) as f32
    }

    fn adjusted_distance_pulled(&self) -> f32 {
        self.distance_pulled.get() * DRAG_MULTIPLIER
    }

    fn consume_available_offset(&self, available: Offset) -> Offset {
        if self.is_refreshing.get() {
            return Offset::ZERO;
        }
        let new_offset = (self.distance_pulled.get() + available.y).max(0.0);
        let consumed = new_offset - self.distance_pulled.get();
        self.distance_pulled.set(new_offset);
        self.vertical_offset.set(self.calculate_vertical_offset());
        Offset::new(0.0, consumed)
    }

    fn calculate_vertical_offset(&self) -> f32 {
        let threshold = self.threshold_px();
        let adjusted = self.adjusted_distance_pulled();
        if adjusted <= threshold {
            return adjusted;
        }
        threshold + threshold * tension_percent((adjusted / threshold).abs() - 1.0)
    }

    async fn on_release(&self, velocity: f32) -> f32 {
        if self.is_refreshing.get() {
            return 0.0;
        }
        let triggered = self.adjusted_distance_pulled() > self.threshold_px();
        let consumed =
            if self.distance_pulled.get() == 0.0 || velocity < 0.0 { 0.0 } else { velocity };
        if triggered {
            let on_refresh = self.on_refresh.borrow().clone();
            on_refresh();
            // A frame for an `is_refreshing` set by `on_refresh` to arrive: its animation to the
            // threshold mustn't run alongside one to hidden, as Compose's. Two here, a frame's
            // tasks running before its recomposition.
            compose_runtime::with_frame_nanos(|_| ()).await;
            compose_runtime::with_frame_nanos(|_| ()).await;
        }
        if !self.is_refreshing.get() {
            self.animate_to_hidden().await;
        }
        self.distance_pulled.set(0.0);
        consumed
    }

    async fn animate_to_threshold(&self) {
        self.state().animate_to_threshold().await;
        let threshold = self.threshold_px();
        self.distance_pulled.set(threshold);
        self.vertical_offset.set(threshold);
    }

    async fn animate_to_hidden(&self) {
        self.state().animate_to_hidden().await;
        self.distance_pulled.set(0.0);
        self.vertical_offset.set(0.0);
    }
}

/// How far past the threshold it's pulled, with tension: 200 % at most, slowing as it goes.
fn tension_percent(overshoot_percent: f32) -> f32 {
    let linear = overshoot_percent.clamp(0.0, 2.0);
    linear - linear * linear / 4.0
}

impl NestedScrollConnection for PullConnection {
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        if self.state().is_animating() || !self.enabled.get() {
            return Offset::ZERO;
        }
        // Swiping up.
        if source == NestedScrollSource::UserInput && available.y < 0.0 {
            return self.consume_available_offset(available);
        }
        Offset::ZERO
    }

    fn on_post_scroll(
        &self,
        _consumed: Offset,
        available: Offset,
        source: NestedScrollSource,
    ) -> Offset {
        if self.state().is_animating() || !self.enabled.get() {
            return Offset::ZERO;
        }
        // Swiping down.
        if source != NestedScrollSource::UserInput {
            return Offset::ZERO;
        }
        let consumed = self.consume_available_offset(available);
        if let Some(scope) = self.scope.get() {
            let (state, fraction) =
                (self.state(), self.vertical_offset.get() / self.threshold_px());
            scope.launch(async move {
                if !state.is_animating() {
                    state.snap_to(fraction).await;
                }
            });
        }
        consumed
    }

    fn on_pre_fling(&self, available: Offset) -> LocalBoxFuture<'_, Offset> {
        Box::pin(async move { Offset::new(0.0, self.on_release(available.y).await) })
    }
}

struct PullToRefreshElement {
    is_refreshing: bool,
    on_refresh: Callback,
    enabled: bool,
    state: Rc<dyn PullToRefreshState>,
    threshold: Dp,
}

struct PullToRefreshNode {
    connection: Rc<PullConnection>,
    delegates: Delegates,
}

impl ModifierNodeElement for PullToRefreshElement {
    type Node = PullToRefreshNode;
    fn create(&self) -> PullToRefreshNode {
        let connection = Rc::new(PullConnection {
            is_refreshing: Cell::new(self.is_refreshing),
            on_refresh: RefCell::new(self.on_refresh.clone()),
            enabled: Cell::new(self.enabled),
            state: RefCell::new(self.state.clone()),
            threshold: Cell::new(self.threshold),
            density: Cell::new(Density::default()),
            scope: Cell::new(None),
            distance_pulled: Cell::new(0.0),
            vertical_offset: Cell::new(0.0),
        });
        let mut delegates = Delegates::new();
        delegates.delegate(compose_ui::nested_scroll_modifier_node(connection.clone()));
        PullToRefreshNode { connection, delegates }
    }
    fn update(&self, node: &mut PullToRefreshNode) {
        let connection = &node.connection;
        connection.on_refresh.replace(self.on_refresh.clone());
        connection.enabled.set(self.enabled);
        connection.state.replace(self.state.clone());
        connection.threshold.set(self.threshold);
        if connection.is_refreshing.replace(self.is_refreshing) != self.is_refreshing
            && let Some(scope) = connection.scope.get()
        {
            let (connection, refreshing) = (connection.clone(), self.is_refreshing);
            scope.launch(async move {
                if refreshing {
                    connection.animate_to_threshold().await;
                } else {
                    connection.animate_to_hidden().await;
                }
            });
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.is_refreshing == other.is_refreshing
            && self.enabled == other.enabled
            && self.on_refresh.ptr_eq(&other.on_refresh)
            && Rc::ptr_eq(&self.state, &other.state)
            && self.threshold == other.threshold
    }
    fn name(&self) -> &'static str {
        "PullToRefreshModifierNode"
    }
}

impl ModifierNode for PullToRefreshNode {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn should_auto_invalidate(&self) -> bool {
        false
    }
    fn on_density_change(&mut self, context: &mut NodeContext<'_>) {
        self.connection.density.set(context.require_density());
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let connection = &self.connection;
        connection.density.set(context.require_density());
        let scope = context.coroutine_scope();
        connection.scope.set(Some(scope));
        let (state, refreshing) = (connection.state(), connection.is_refreshing.get());
        scope.launch(async move {
            state.snap_to(if refreshing { 1.0 } else { 0.0 }).await;
        });
        connection.vertical_offset.set(if refreshing { connection.threshold_px() } else { 0.0 });
    }
}

/// `Modifier::pull_to_refresh`, in a trait as it's of this crate.
pub trait PullToRefreshModifiers {
    /// Pulling what scrolls inside down, past 80 dp, calls `on_refresh`; while `is_refreshing`, the
    /// state stays at the threshold, as `Modifier.pullToRefresh`.
    fn pull_to_refresh(
        self,
        is_refreshing: bool,
        state: Rc<dyn PullToRefreshState>,
        on_refresh: impl Fn() + 'static,
    ) -> Modifier;
    /// As `pull_to_refresh`, with `enabled` and `threshold`.
    fn pull_to_refresh_with(
        self,
        is_refreshing: bool,
        state: Rc<dyn PullToRefreshState>,
        enabled: bool,
        threshold: Dp,
        on_refresh: impl Fn() + 'static,
    ) -> Modifier;
}

impl PullToRefreshModifiers for Modifier {
    fn pull_to_refresh(
        self,
        is_refreshing: bool,
        state: Rc<dyn PullToRefreshState>,
        on_refresh: impl Fn() + 'static,
    ) -> Modifier {
        self.pull_to_refresh_with(
            is_refreshing,
            state,
            true,
            PullToRefreshDefaults::POSITIONAL_THRESHOLD,
            on_refresh,
        )
    }

    fn pull_to_refresh_with(
        self,
        is_refreshing: bool,
        state: Rc<dyn PullToRefreshState>,
        enabled: bool,
        threshold: Dp,
        on_refresh: impl Fn() + 'static,
    ) -> Modifier {
        self.then_custom(PullToRefreshElement {
            is_refreshing,
            on_refresh: compose_runtime::callback(on_refresh),
            enabled,
            state,
            threshold,
        })
    }
}

/// A box that refreshes when what scrolls in it is pulled, with an indicator over it, as Compose's
/// `PullToRefreshBox`. `None` for `state` is a remembered one, and for `indicator`, the default
/// one at the top center.
#[composable]
#[allow(non_snake_case)]
pub fn PullToRefreshBox(
    is_refreshing: bool,
    on_refresh: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] state: Option<Rc<dyn PullToRefreshState>>,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default(None)] indicator: Option<impl Fn(BoxScope) + 'static>,
    #[default(true)] enabled: bool,
    #[default(crate::PullToRefreshDefaults::POSITIONAL_THRESHOLD)] threshold: Dp,
    content: impl Fn(BoxScope) + 'static,
) {
    let remembered = remember_pull_to_refresh_state();
    let state = state.unwrap_or(remembered);
    Box(
        modifier.pull_to_refresh_with(
            is_refreshing,
            state.clone(),
            enabled,
            threshold,
            move || on_refresh(),
        ),
        content_alignment,
        false,
        compose_runtime::callback(move |scope: BoxScope| {
            content(scope);
            match &indicator {
                Some(indicator) => indicator(scope),
                None => PullToRefreshIndicator(
                    state.clone(),
                    is_refreshing,
                    Modifier.align(scope, Alignment::TOP_CENTER),
                    PullToRefreshDefaults::indicator_container_color(),
                    PullToRefreshDefaults::indicator_color(),
                    PullToRefreshDefaults::INDICATOR_MAX_DISTANCE,
                ),
            }
        }),
    );
}

/// As Compose's `PullToRefreshDefaults`; its `IndicatorBox` and `Indicator` are
/// [`PullToRefreshIndicatorBox`] and [`PullToRefreshIndicator`].
pub struct PullToRefreshDefaults;

impl PullToRefreshDefaults {
    /// As `indicatorShape`.
    pub const INDICATOR_SHAPE: Shape = compose_foundation::CIRCLE_SHAPE;
    /// As `PositionalThreshold`.
    pub const POSITIONAL_THRESHOLD: Dp = Dp(80.0);
    /// As `IndicatorMaxDistance`.
    pub const INDICATOR_MAX_DISTANCE: Dp = Self::POSITIONAL_THRESHOLD;
    /// `ElevationTokens.Level2`, as `Elevation`.
    pub const ELEVATION: Dp = Dp(3.0);

    /// As `indicatorContainerColor`.
    pub fn indicator_container_color() -> Color {
        MaterialTheme::color_scheme().surface_container_high
    }

    /// As `indicatorColor`.
    pub fn indicator_color() -> Color {
        MaterialTheme::color_scheme().on_surface_variant
    }
}

/// An indicator's container, sliding in from above by `state`, as `PullToRefreshDefaults.IndicatorBox`.
#[composable]
#[allow(non_snake_case)]
pub fn PullToRefreshIndicatorBox(
    state: Rc<dyn PullToRefreshState>,
    is_refreshing: bool,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::PullToRefreshDefaults::INDICATOR_MAX_DISTANCE)] max_distance: Dp,
    #[default(crate::PullToRefreshDefaults::INDICATOR_SHAPE)] shape: Shape,
    #[default(crate::Color::UNSPECIFIED)] container_color: Color,
    #[default(crate::PullToRefreshDefaults::ELEVATION)] elevation: Dp,
    content: impl Fn(BoxScope) + 'static,
) {
    let layer_shape = shape.clone();
    Box(
        modifier
            .size(SPINNER_CONTAINER_SIZE)
            // Not above where it's placed: it comes from under what's above.
            .draw_with_content(|draw| {
                let bounds = Rect::new(-f32::MAX, 0.0, f32::MAX, f32::MAX);
                draw.save();
                draw.clip_outline(compose_ui::Outline::Rectangle(bounds));
                draw.draw_content();
                draw.restore();
            })
            .graphics_layer(move |layer| {
                let fraction = state.distance_fraction();
                let density = layer.density;
                layer.translation_y =
                    fraction * density.round_to_px(max_distance) as f32 - layer.size.height;
                let shows = fraction > 0.0 || is_refreshing;
                layer.shadow_elevation = if shows { density.to_px(elevation) } else { 0.0 };
                layer.shape = layer_shape.clone();
                layer.clip = true;
            })
            .background_with(container_color, shape),
        Alignment::CENTER,
        false,
        content,
    );
}

/// The default indicator: an arrow and arc pulled in, a spinner while refreshing, as
/// `PullToRefreshDefaults.Indicator`.
#[composable]
#[allow(non_snake_case)]
pub fn PullToRefreshIndicator(
    state: Rc<dyn PullToRefreshState>,
    is_refreshing: bool,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::PullToRefreshDefaults::indicator_container_color())] container_color: Color,
    #[default(crate::PullToRefreshDefaults::indicator_color())] color: Color,
    #[default(crate::PullToRefreshDefaults::INDICATOR_MAX_DISTANCE)] max_distance: Dp,
) {
    let indicator_state = state.clone();
    PullToRefreshIndicatorBox(
        state,
        is_refreshing,
        modifier,
        max_distance,
        PullToRefreshDefaults::INDICATOR_SHAPE,
        container_color,
        PullToRefreshDefaults::ELEVATION,
        compose_runtime::callback(move |_| {
            let state = indicator_state.clone();
            let spec = MaterialTheme::motion_scheme().default_effects_spec();
            Crossfade!(is_refreshing, animation_spec = spec, move |refreshing: bool| {
                if refreshing {
                    crate::IndeterminateCircularProgressIndicator!(
                        modifier = Modifier.size(SPINNER_SIZE),
                        color = color,
                        stroke_width = STROKE_WIDTH
                    );
                } else {
                    circular_arrow_progress_indicator(state.clone(), color);
                }
            });
        }),
    );
}

/// The pulled arc and its arrow's progress bar semantics, read as they're asked for.
struct ArrowSemantics(Rc<dyn PullToRefreshState>);

impl ModifierNodeElement for ArrowSemantics {
    type Node = ArrowSemanticsNode;
    fn create(&self) -> ArrowSemanticsNode {
        ArrowSemanticsNode(self.0.clone())
    }
    fn update(&self, node: &mut ArrowSemanticsNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

struct ArrowSemanticsNode(Rc<dyn PullToRefreshState>);

impl ModifierNode for ArrowSemanticsNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for ArrowSemanticsNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        let progress = self.0.distance_fraction();
        if progress > 0.0 {
            config.set_progress_bar_range_info(ProgressBarRangeInfo {
                current: progress,
                range: (0.0, 1.0),
                steps: 0,
            });
        }
    }
    fn should_clear_descendant_semantics(&self) -> bool {
        true
    }
}

/// As Compose's private `CircularArrowProgressIndicator`.
fn circular_arrow_progress_indicator(state: Rc<dyn PullToRefreshState>, color: Color) {
    let target =
        {
            let state = state.clone();
            remember(|| {
                derived_state_of(move || {
                    if state.distance_fraction() >= 1.0 { MAX_ALPHA } else { MIN_ALPHA }
                })
            })
        };
    let spec = MaterialTheme::motion_scheme().default_effects_spec();
    let alpha = animate_float_as_state!(target.get(), animation_spec = spec);
    let drawn = state.clone();
    Canvas(Modifier.then_custom(ArrowSemantics(state)).size(SPINNER_SIZE), move |draw| {
        let density = draw.density();
        let values = ArrowValues::of(drawn.distance_fraction());
        let color = color.with_alpha(color.alpha() as f32 / 255.0 * alpha.get());
        let stroke = density.to_px(STROKE_WIDTH);
        let bounds = draw.bounds();
        let center = Offset::new(bounds.width() / 2.0, bounds.height() / 2.0);
        draw.rotate(values.rotation, center, |draw| {
            let radius = density.to_px(ARC_RADIUS) + stroke / 2.0;
            let arc = Rect::new(
                center.x - radius,
                center.y - radius,
                center.x + radius,
                center.y + radius,
            );
            draw.draw_arc(
                arc,
                values.start_angle,
                values.end_angle - values.start_angle,
                Paint::stroke(color, stroke),
            );
            draw_arrow(draw, arc, color, &values, stroke, density);
        });
    });
}

struct ArrowValues {
    rotation: f32,
    start_angle: f32,
    end_angle: f32,
    scale: f32,
}

impl ArrowValues {
    fn of(progress: f32) -> ArrowValues {
        // The first 40 % of the progress discarded, the rest over the full range.
        let adjusted = (progress.min(1.0) - 0.4).max(0.0) * 5.0 / 3.0;
        let tension = tension_percent(progress.abs() - 1.0);
        // As `SwipeRefreshLayout`'s.
        let end_trim = adjusted * MAX_PROGRESS_ARC;
        let rotation = (-0.25 + 0.4 * adjusted + tension) * 0.5;
        ArrowValues {
            rotation,
            start_angle: rotation * 360.0,
            end_angle: (rotation + end_trim) * 360.0,
            scale: adjusted.min(1.0),
        }
    }
}

fn draw_arrow(
    draw: &mut compose_ui::DrawScope<'_>,
    bounds: Rect,
    color: Color,
    values: &ArrowValues,
    stroke: f32,
    density: Density,
) {
    let (width, height) = (density.to_px(ARROW_WIDTH), density.to_px(ARROW_HEIGHT));
    let arrow = Path::new();
    arrow.set_fill_type(PathFillType::EvenOdd);
    arrow.move_to(0.0, 0.0);
    arrow.line_to(width * values.scale / 2.0, height * values.scale);
    arrow.line_to(width * values.scale, 0.0);
    let radius = bounds.width().min(bounds.height()) / 2.0;
    let inset = width * values.scale / 2.0;
    let center =
        Offset::new((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0);
    arrow.translate(Offset::new(radius + center.x - inset, center.y - stroke));
    let around = Offset::new(draw.bounds().width() / 2.0, draw.bounds().height() / 2.0);
    draw.rotate(values.end_angle - stroke, around, |draw| {
        draw.draw_path(&arrow, Paint::stroke(color, stroke));
    });
}
