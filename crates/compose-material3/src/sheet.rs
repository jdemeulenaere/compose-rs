//! `ModalBottomSheet`, as Compose's Material 3: a sheet sliding up from the bottom of the
//! window over a scrim, dragged between hidden, partially expanded and expanded, with its
//! `SheetState`.

use compose_runtime::{ObjectHandle, Owned};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::{AnimationSpec, Spring};
use compose_foundation::MutatePriority;
use compose_foundation::gestures::{
    AnchoredDraggableModifier, AnchoredDraggableOptions, AnchoredDraggableState, DraggableAnchors,
    FlingBehavior, LocalBoxFuture, Orientation, ScrollScope, remember_anchored_fling_behavior,
};
use compose_foundation::layout::{
    Box, Column, ColumnScope, WindowInsets, WindowInsetsModifiers, WindowInsetsProvider,
    WindowInsetsSides,
};
use compose_runtime::{ComposeArg, LaunchedEffect, composable, remember, remember_coroutine_scope};
use compose_ui::{
    Alignment, Color, Constraints, Dp, IntOffset, IntSize, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, PopupPositionProvider, PopupProperties,
    Rect, Shape, UnitExt,
};
use compose_ui::{NestedScrollConnection, NestedScrollSource, Offset};

use crate::motion::MotionScheme;
use crate::theme::MaterialTheme;

/// Where a sheet rests, as Compose's `SheetValue`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetValue {
    Hidden,
    Expanded,
    PartiallyExpanded,
}

impl ComposeArg for SheetValue {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SheetValue {
    /// Every value, as `rememberBottomSheetState`'s default `enabledValues`.
    pub const ALL: [SheetValue; 3] =
        [SheetValue::Hidden, SheetValue::PartiallyExpanded, SheetValue::Expanded];
}

impl SheetValue {
    fn from_key(key: u8) -> SheetValue {
        match key {
            0 => SheetValue::Hidden,
            1 => SheetValue::Expanded,
            _ => SheetValue::PartiallyExpanded,
        }
    }
}

/// A fling behavior as a key, by identity.
#[derive(Clone)]
pub(crate) struct FlingKey(pub(crate) Rc<dyn FlingBehavior>);

impl ComposeArg for FlingKey {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

pub(crate) struct SheetInner {
    pub(crate) anchored: AnchoredDraggableState<SheetValue>,
    pub(crate) skip_partially_expanded: bool,
    pub(crate) skip_hidden_state: bool,
    /// How far a drag must go to settle at the next anchor, in pixels, as
    /// `positionalThreshold`.
    pub(crate) positional_threshold: Box<dyn Fn() -> f32>,
    confirm_value_change: Rc<dyn Fn(SheetValue) -> bool>,
    /// What showing and expanding animate with, and hiding and partially expanding, as
    /// `showMotionSpec` and `hideMotionSpec`: at once until a sheet sets its motion.
    show_spec: RefCell<AnimationSpec>,
    hide_spec: RefCell<AnimationSpec>,
}

/// The sheet's offset from the window's top, as Compose's `SheetWindowInsets`: what it consumes
/// for its content, read as the sheet moves.
struct SheetWindowInsets(SheetState);

impl WindowInsetsProvider for SheetWindowInsets {
    fn get_left(&self, _: &compose_ui::Density, _: compose_ui::LayoutDirection) -> i32 {
        0
    }

    fn get_top(&self, _: &compose_ui::Density) -> i32 {
        let offset = self.0.require_offset();
        if offset.is_nan() { 0 } else { (offset as i32).max(0) }
    }

    fn get_right(&self, _: &compose_ui::Density, _: compose_ui::LayoutDirection) -> i32 {
        0
    }

    fn get_bottom(&self, _: &compose_ui::Density) -> i32 {
        0
    }

    fn same(&self, other: &dyn std::any::Any) -> bool {
        other.downcast_ref::<SheetWindowInsets>().is_some_and(|other| other.0 == self.0)
    }
}

impl std::fmt::Display for SheetWindowInsets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SheetWindowInsets")
    }
}

/// A bottom sheet's state, as Compose's `SheetState`: where it rests and where it's going,
/// moved by `show`, `hide`, `expand` and `partial_expand` (animated) and by drags.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_bottom_sheet_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SheetState {
    object: ObjectHandle<SheetInner>,
}

impl SheetState {
    /// Its object, or `None` once its owner went (D240): then it does nothing, its reads
    /// giving the defaults.
    #[track_caller]
    pub(crate) fn inner(&self) -> Option<Rc<SheetInner>> {
        self.object.get()
    }

    /// A state resting at `initial_value`, settling only at `enabled_values` (which must hold
    /// `Expanded` and `initial_value`), as `SheetState(enabledValues, positionalThreshold,
    /// velocityThreshold, initialValue, confirmValueChange)`: partially expanded at half its
    /// content's height (at most half the window's); drags settle past `positional_threshold`
    /// pixels or faster than `velocity_threshold` pixels a second; `confirm_value_change` can
    /// refuse a value.
    #[track_caller]
    pub fn new(
        enabled_values: &[SheetValue],
        positional_threshold: impl Fn() -> f32 + 'static,
        velocity_threshold: impl Fn() -> f32 + 'static,
        initial_value: SheetValue,
        confirm_value_change: impl Fn(SheetValue) -> bool + 'static,
    ) -> Owned<SheetState> {
        Self::create(
            enabled_values,
            Box::new(positional_threshold),
            Box::new(velocity_threshold),
            initial_value,
            Rc::new(confirm_value_change),
        )
    }

    #[track_caller]
    fn create(
        enabled_values: &[SheetValue],
        positional_threshold: Box<dyn Fn() -> f32>,
        _velocity_threshold: Box<dyn Fn() -> f32>,
        initial_value: SheetValue,
        confirm_value_change: Rc<dyn Fn(SheetValue) -> bool>,
    ) -> Owned<SheetState> {
        assert!(
            enabled_values.contains(&SheetValue::Expanded),
            "Expanded must be one of the enabled values."
        );
        assert!(
            enabled_values.contains(&initial_value),
            "The initial value must be one of the enabled values."
        );
        let confirm = confirm_value_change.clone();
        Owned::new(
            SheetInner {
                anchored: AnchoredDraggableState::with_confirm(initial_value, move |value| {
                    confirm(*value)
                }),
                skip_partially_expanded: !enabled_values.contains(&SheetValue::PartiallyExpanded),
                skip_hidden_state: !enabled_values.contains(&SheetValue::Hidden),
                positional_threshold,
                confirm_value_change,
                show_spec: RefCell::new(compose_animation::snap(0).into()),
                hide_spec: RefCell::new(compose_animation::snap(0).into()),
            },
            |object| SheetState { object },
        )
    }

    /// Where it last settled (a state read), as `currentValue` (the anchored state's settled
    /// value).
    pub fn current_value(&self) -> SheetValue {
        self.inner().map_or(SheetValue::Hidden, |inner| inner.anchored.settled_value())
    }

    /// Where it's going (a state read), as `targetValue`.
    pub fn target_value(&self) -> SheetValue {
        self.inner().map_or(SheetValue::Hidden, |inner| inner.anchored.target_value())
    }

    /// Whether it's showing, as `isVisible`: not hidden, or dragged more than half way out of
    /// hiding (the anchored state's current value).
    pub fn is_visible(&self) -> bool {
        self.inner().is_some_and(|inner| inner.anchored.current_value() != SheetValue::Hidden)
    }

    /// Whether it's animating to a value (a state read), as `isAnimationRunning`.
    pub fn is_animation_running(&self) -> bool {
        self.inner().is_some_and(|inner| inner.anchored.is_animation_running())
    }

    /// Whether it can rest expanded, as `hasExpandedState`.
    pub fn has_expanded_state(&self) -> bool {
        self.inner()
            .is_some_and(|inner| inner.anchored.anchors().has_position_for(&SheetValue::Expanded))
    }

    /// Whether it can rest half way, as `hasPartiallyExpandedState`.
    pub fn has_partially_expanded_state(&self) -> bool {
        self.inner().is_some_and(|inner| {
            inner.anchored.anchors().has_position_for(&SheetValue::PartiallyExpanded)
        })
    }

    /// Its top's offset from the top of the window, in pixels (a state read; NaN before it's
    /// laid out), as `requireOffset()`.
    pub fn require_offset(&self) -> f32 {
        self.inner().map_or(f32::NAN, |inner| inner.anchored.offset())
    }

    /// Whether `confirm_value_change` lets it go to `value`.
    pub(crate) fn confirms(&self, value: SheetValue) -> bool {
        self.inner().is_some_and(|inner| (inner.confirm_value_change)(value))
    }

    /// Animates to `value`, if `confirm_value_change` lets it, with the show or hide motion.
    async fn animate_to(&self, value: SheetValue, show: bool) {
        if let Some(inner) = self.inner()
            && (inner.confirm_value_change)(value)
        {
            let spec = if show { &inner.show_spec } else { &inner.hide_spec }.borrow().clone();
            // The target new anchors gave it, if not this one: there at once, as Compose's.
            let resolved = Rc::new(Cell::new(value));
            let noted = resolved.clone();
            let anchored = inner.anchored.clone();
            let _ = inner
                .anchored
                .anchored_drag_to(value, MutatePriority::Default, move |scope, anchors, latest| {
                    noted.set(latest);
                    let (spec, anchored) = (spec.clone(), anchored.clone());
                    async move {
                        let target = anchors.position_of(&latest);
                        let offset = anchored.offset();
                        let previous = if offset.is_nan() { 0.0 } else { offset };
                        if !target.is_nan() && previous != target {
                            compose_animation::animate(
                                previous,
                                target,
                                anchored.last_velocity(),
                                spec,
                                |value, velocity| scope.drag_to(value, velocity),
                            )
                            .await;
                        }
                    }
                })
                .await;
            let resolved = resolved.get();
            if resolved != value && inner.anchored.anchors().has_position_for(&resolved) {
                let _ = inner.anchored.snap_to(resolved).await;
            }
        }
    }

    /// Shows it, half way if it can rest there, as `show()`.
    pub async fn show(&self) {
        let value = if self.has_partially_expanded_state() {
            SheetValue::PartiallyExpanded
        } else {
            SheetValue::Expanded
        };
        self.animate_to(value, true).await;
    }

    /// Hides it, as `hide()`. Panics if it can't rest hidden, as Compose's.
    pub async fn hide(&self) {
        if let Some(inner) = self.inner() {
            assert!(
                !inner.skip_hidden_state,
                "Attempted to animate to hidden when skipHiddenState was enabled. Set \
                 skipHiddenState to false to use this function."
            );
        }
        self.animate_to(SheetValue::Hidden, false).await;
    }

    /// Expands it, as `expand()`.
    pub async fn expand(&self) {
        self.animate_to(SheetValue::Expanded, true).await;
    }

    /// Takes it half way, as `partialExpand()`. Panics if it can't rest there, as Compose's.
    pub async fn partial_expand(&self) {
        if let Some(inner) = self.inner() {
            assert!(
                !inner.skip_partially_expanded,
                "Attempted to animate to partial expanded when skipPartiallyExpanded was \
                 enabled. Set skipPartiallyExpanded to false to use this function."
            );
        }
        self.animate_to(SheetValue::PartiallyExpanded, false).await;
    }

    /// At `value` at once, as `snapTo`.
    pub async fn snap_to(&self, value: SheetValue) {
        if let Some(inner) = self.inner() {
            let _ = inner.anchored.snap_to(value).await;
        }
    }

    /// Flings in a drag of its state, as `anchoredDrag(flingBehavior, initialVelocity)`: returns
    /// what the behavior left.
    pub(crate) async fn fling(&self, behavior: &Rc<dyn FlingBehavior>, velocity: f32) -> f32 {
        let Some(inner) = self.inner() else { return 0.0 };
        let left = Rc::new(Cell::new(0.0));
        let noted = left.clone();
        let anchored = inner.anchored.clone();
        let _ = inner
            .anchored
            .anchored_drag(MutatePriority::Default, move |drag, _| {
                let (behavior, noted, anchored) =
                    (behavior.clone(), noted.clone(), anchored.clone());
                async move {
                    let scope = ScrollScope::new(move |pixels| {
                        let offset = anchored.offset();
                        let (min, max) =
                            (anchored.anchors().min_position(), anchored.anchors().max_position());
                        let start = if offset.is_nan() { 0.0 } else { offset };
                        let next = (start + pixels).clamp(min, max);
                        drag.drag_to(next, 0.0);
                        next - start
                    });
                    noted.set(behavior.perform_fling(scope, velocity).await);
                }
            })
            .await;
        left.get()
    }

    /// New anchors and where they take it, in a measurement, as `draggableAnchors`.
    pub(crate) fn update_anchors(&self, anchors: DraggableAnchors<SheetValue>, target: SheetValue) {
        if let Some(inner) = self.inner() {
            compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                inner.anchored.update_anchors_to(anchors, target);
            });
        }
    }

    /// The state its gestures drive; a state of its own once its owner went (D240).
    pub(crate) fn anchored_state(&self) -> AnchoredDraggableState<SheetValue> {
        self.inner().map_or_else(
            || AnchoredDraggableState::new(SheetValue::Hidden),
            |inner| inner.anchored.clone(),
        )
    }

    /// A fling settling by its positional threshold and the theme's spatial motion, as both sheets'
    /// `AnchoredDraggableDefaults.flingBehavior`.
    pub(crate) fn remember_settling(&self) -> Rc<dyn FlingBehavior> {
        let state = *self;
        let threshold = compose_runtime::remember_keyed(state, move || {
            Rc::new(move |_: f32| state.inner().map_or(0.0, |inner| (inner.positional_threshold)()))
                as Rc<dyn Fn(f32) -> f32>
        });
        remember_anchored_fling_behavior(
            self.anchored_state(),
            threshold,
            MaterialTheme::motion_scheme().default_spatial_spec(),
        )
    }

    /// Sets what it animates with, as Compose's sheets do from the motion scheme: showing and
    /// expanding with the default spatial spec, hiding with the fast effects one, drags settling
    /// with the default spatial one.
    pub(crate) fn set_motion(&self, motion: &MotionScheme) {
        if let Some(inner) = self.inner() {
            *inner.show_spec.borrow_mut() = motion.default_spatial_spec();
            *inner.hide_spec.borrow_mut() = motion.fast_effects_spec();
        }
    }
}

impl ComposeArg for SheetState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SheetState {
    /// Saves where it last settled, as Compose's `SheetState.Saver(enabledValues,
    /// positionalThreshold, velocityThreshold, confirmValueChange)`, restoring a state made
    /// with the same arguments.
    pub fn saver(
        enabled_values: &[SheetValue],
        positional_threshold: impl Fn() -> f32 + 'static,
        velocity_threshold: impl Fn() -> f32 + 'static,
        confirm_value_change: impl Fn(SheetValue) -> bool + 'static,
    ) -> impl compose_runtime::Saver<Owned<SheetState>> {
        Self::saver_with(
            enabled_values.to_vec(),
            Rc::new(positional_threshold),
            Rc::new(velocity_threshold),
            Rc::new(confirm_value_change),
        )
    }

    fn saver_with(
        enabled_values: Vec<SheetValue>,
        positional_threshold: Rc<dyn Fn() -> f32>,
        velocity_threshold: Rc<dyn Fn() -> f32>,
        confirm_value_change: Rc<dyn Fn(SheetValue) -> bool>,
    ) -> impl compose_runtime::Saver<Owned<SheetState>> {
        compose_runtime::saver(
            |state: &Owned<SheetState>| {
                state.inner()?;
                Some(compose_runtime::SavedValue::Int(state.current_value() as i64))
            },
            move |saved| match saved {
                compose_runtime::SavedValue::Int(key) if (0..=2).contains(key) => {
                    let value = SheetValue::from_key(*key as u8);
                    if !enabled_values.contains(&value) {
                        return None;
                    }
                    let (positional, velocity) =
                        (positional_threshold.clone(), velocity_threshold.clone());
                    Some(SheetState::create(
                        &enabled_values,
                        Box::new(move || positional()),
                        Box::new(move || velocity()),
                        value,
                        confirm_value_change.clone(),
                    ))
                }
                _ => None,
            },
        )
    }
}

/// A threshold in pixels, read when a drag settles.
type Threshold = Rc<dyn Fn() -> f32>;

/// The thresholds of a state remembered at `density`, as `rememberSheetState`'s.
fn thresholds(density: compose_ui::Density) -> (Threshold, Threshold) {
    (
        Rc::new(move || density.to_px(BottomSheetDefaults::POSITIONAL_THRESHOLD)),
        Rc::new(move || density.to_px(BottomSheetDefaults::VELOCITY_THRESHOLD)),
    )
}

/// A sheet state remembered here, as Compose's `rememberSheetState`: saved across the host's
/// recreation (a `rememberSaveable`).
#[track_caller]
fn remember_sheet_state(
    enabled_values: &[SheetValue],
    confirm_value_change: Rc<dyn Fn(SheetValue) -> bool>,
    initial_value: SheetValue,
) -> SheetState {
    let density = compose_ui::LOCAL_DENSITY.current();
    let (positional, velocity) = thresholds(density);
    let values = enabled_values.to_vec();
    compose_runtime::remember_saveable_with(
        SheetState::saver_with(
            values.clone(),
            positional.clone(),
            velocity.clone(),
            confirm_value_change.clone(),
        ),
        move || {
            let (positional, velocity) = (positional.clone(), velocity.clone());
            SheetState::create(
                &values,
                Box::new(move || positional()),
                Box::new(move || velocity()),
                initial_value,
                confirm_value_change.clone(),
            )
        },
    )
}

/// A sheet's state remembered here, at `initial_value`, as `rememberBottomSheetState(
/// initialValue, enabledValues, confirmValueChange)`: saved across the host's recreation (a
/// `rememberSaveable`, as Compose's). `confirm_value_change` is the first composition's.
#[composable(non_restartable)]
pub fn remember_bottom_sheet_state(
    initial_value: SheetValue,
    #[default(&crate::SheetValue::ALL)] enabled_values: &[SheetValue],
    #[default(|_| true)] confirm_value_change: impl Fn(SheetValue) -> bool + 'static,
) -> SheetState {
    let confirm: Rc<dyn Fn(SheetValue) -> bool> = Rc::new(move |value| confirm_value_change(value));
    remember_sheet_state(enabled_values, confirm, initial_value)
}

/// What a modal sheet's back and outside clicks do, as Compose's `ModalBottomSheetProperties`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ModalBottomSheetProperties {
    /// Back (Escape) dismisses it, as `shouldDismissOnBackPress`.
    pub should_dismiss_on_back_press: bool,
    /// A click on the scrim dismisses it, as `shouldDismissOnClickOutside`.
    pub should_dismiss_on_click_outside: bool,
}

impl ModalBottomSheetProperties {
    /// As `ModalBottomSheetProperties()`: both dismiss.
    pub const DEFAULT: ModalBottomSheetProperties = ModalBottomSheetProperties {
        should_dismiss_on_back_press: true,
        should_dismiss_on_click_outside: true,
    };
}

impl Default for ModalBottomSheetProperties {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ComposeArg for ModalBottomSheetProperties {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `ModalBottomSheetDefaults`.
pub struct ModalBottomSheetDefaults;

impl ModalBottomSheetDefaults {
    /// As `properties`.
    pub const PROPERTIES: ModalBottomSheetProperties = ModalBottomSheetProperties::DEFAULT;
}

/// As Compose's `BottomSheetDefaults`.
pub struct BottomSheetDefaults;

impl BottomSheetDefaults {
    /// The widest a sheet gets, as `SheetMaxWidth`.
    pub const SHEET_MAX_WIDTH: Dp = Dp(640.0);
    /// A standard sheet's shadow, as `Elevation` (`ElevationTokens.Level1`).
    pub const ELEVATION: Dp = Dp(1.0);
    /// As `SheetPeekHeight`.
    pub const SHEET_PEEK_HEIGHT: Dp = Dp(56.0);
    /// How far a drag must go to settle at the next anchor, as Compose's internal
    /// `PositionalThreshold` (what `remember_bottom_sheet_state` gives its state).
    #[doc(hidden)]
    pub const POSITIONAL_THRESHOLD: Dp = Dp(56.0);
    /// A release faster than this settles in its direction, as Compose's internal
    /// `VelocityThreshold` (125 dp/s).
    #[doc(hidden)]
    pub const VELOCITY_THRESHOLD: Dp = Dp(125.0);
    /// Near the hidden anchor, flings down slow (quadratically) within this, as Compose's
    /// internal `BoundaryDampeningZone`.
    const BOUNDARY_DAMPENING_ZONE: Dp = Dp(125.0);

    /// Extra-large top corners, as `ExpandedShape`.
    pub fn expanded_shape() -> Shape {
        let corner = match MaterialTheme::shapes().extra_large.corners() {
            Some(corners) => corners[0],
            None => compose_ui::CornerSize::Dp(Dp(0.0)),
        };
        let none = compose_ui::CornerSize::Dp(Dp(0.0));
        Shape::Rounded([corner, corner, none, none])
    }

    /// The low surface container, as `ContainerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface_container_low
    }

    /// 32 % of the scrim color, as `ScrimColor`.
    pub fn scrim_color() -> Color {
        MaterialTheme::color_scheme().scrim.with_alpha(0.32)
    }

    /// As `modalWindowInsets`: what's safe to draw on, at the top and the bottom. The sheet's
    /// offset from the window's top is consumed, so the top counts only as it nears the window's.
    pub fn modal_window_insets() -> WindowInsets {
        WindowInsets::safe_drawing().only(WindowInsetsSides::BOTTOM + WindowInsetsSides::TOP)
    }

    /// As `standardWindowInsets`: what's safe to draw on, at the bottom.
    pub fn standard_window_insets() -> WindowInsets {
        WindowInsets::safe_drawing().only(WindowInsetsSides::BOTTOM)
    }

    /// The drag handle, as `DragHandle()`: a 32 by 4 dp pill in the on-surface-variant color
    /// at 40 %, 22 dp above and below.
    pub fn drag_handle() {
        BottomSheetDefaults::drag_handle_with(
            Modifier,
            32.dp(),
            4.dp(),
            MaterialTheme::shapes().extra_large.clone(),
            MaterialTheme::color_scheme().on_surface_variant.with_alpha(0.4),
        );
    }

    /// The drag handle with `DragHandle`'s parameters.
    pub fn drag_handle_with(modifier: Modifier, width: Dp, height: Dp, shape: Shape, color: Color) {
        crate::components::Surface(
            modifier.padding((Dp(0.0), 22.dp())).semantics(|config| {
                config.set_content_description("Drag handle");
            }),
            shape,
            color,
            Color::UNSPECIFIED,
            Dp(0.0),
            Dp(0.0),
            None,
            compose_runtime::callback(move || {
                compose_foundation::layout::Spacer(Modifier.size((width, height)));
            }),
        );
    }
}

/// The sheet's part in its content's scrolls.
pub(crate) struct SheetConnection {
    pub(crate) state: SheetState,
    pub(crate) fling: Rc<dyn FlingBehavior>,
}

impl NestedScrollConnection for SheetConnection {
    /// Scrolling the content up drags the sheet up first, as far as it goes.
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        if available.y < 0.0
            && source == NestedScrollSource::UserInput
            && let Some(inner) = self.state.inner()
        {
            Offset::new(0.0, inner.anchored.dispatch_raw_delta(available.y))
        } else {
            Offset::ZERO
        }
    }

    /// What the content leaves scrolling down (it's at its top) drags the sheet down.
    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        if source == NestedScrollSource::UserInput
            && available.y != 0.0
            && let Some(inner) = self.state.inner()
        {
            Offset::new(0.0, inner.anchored.dispatch_raw_delta(available.y))
        } else {
            Offset::ZERO
        }
    }

    /// A fling up while the sheet isn't at its top flings the sheet instead.
    fn on_pre_fling(&self, available: Offset) -> compose_ui::LocalBoxFuture<'_, Offset> {
        Box::pin(async move {
            let Some(inner) = self.state.inner() else { return Offset::ZERO };
            if available.y < 0.0
                && inner.anchored.offset() > inner.anchored.anchors().min_position()
            {
                self.state.fling(&self.fling, available.y).await;
                // It goes to the anchor with a settling animation: all consumed, for the best UX.
                available
            } else {
                Offset::ZERO
            }
        })
    }

    /// After the content's fling (or a drag let go), the sheet flings.
    fn on_post_fling(
        &self,
        consumed: Offset,
        available: Offset,
    ) -> compose_ui::LocalBoxFuture<'_, Offset> {
        Box::pin(async move {
            let left = self.state.fling(&self.fling, available.y).await;
            Offset::new(consumed.x, left)
        })
    }
}

/// A modal sheet's fling, as `modalBottomSheetFlingBehavior`: slowed near the hidden anchor, and
/// the caller told if it ended hidden.
struct ModalSheetFling {
    state: SheetState,
    settle: Rc<dyn FlingBehavior>,
    density: compose_ui::Density,
    max_velocity: f32,
    on_dismiss: Rc<RefCell<Option<compose_runtime::Callback>>>,
}

/// Tells the caller however the fling ends, as its `finally`.
struct DismissIfHidden<'a>(&'a ModalSheetFling);

impl Drop for DismissIfHidden<'_> {
    fn drop(&mut self) {
        if !self.0.state.is_visible() {
            let on_dismiss = self.0.on_dismiss.borrow().clone();
            if let Some(on_dismiss) = on_dismiss {
                on_dismiss();
            }
        }
    }
}

impl FlingBehavior for ModalSheetFling {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        Box::pin(async move {
            let mut velocity = initial_velocity.clamp(-self.max_velocity, self.max_velocity);
            if velocity > 0.0
                && let Some(inner) = self.state.inner()
                && inner.anchored.anchors().has_position_for(&SheetValue::Hidden)
            {
                let hidden = inner.anchored.anchors().position_of(&SheetValue::Hidden);
                let distance = (hidden - inner.anchored.offset()).max(0.0);
                let zone = self.density.to_px(BottomSheetDefaults::BOUNDARY_DAMPENING_ZONE);
                if distance < zone {
                    let factor = distance / zone;
                    velocity *= factor * factor;
                    // A fling fast enough to settle stays so.
                    let floor = self.density.to_px(BottomSheetDefaults::VELOCITY_THRESHOLD);
                    if initial_velocity >= floor {
                        velocity = velocity.max(floor);
                    }
                }
            }
            let _dismiss = DismissIfHidden(self);
            self.settle.perform_fling(scope, velocity).await
        })
    }
}

/// The window's top start, for the sheet's overlay.
struct AtWindowTop;

impl PopupPositionProvider for AtWindowTop {
    fn position(&self, _: Rect, _: IntSize, _: LayoutDirection, _: IntSize) -> IntOffset {
        IntOffset::ZERO
    }
}

/// A modal bottom sheet, as Compose's `ModalBottomSheet`: over the whole window, a scrim (a
/// click on it hides the sheet, with `properties`) and the sheet at the bottom (up to 640 dp
/// wide, centered), its drag handle and content in a column. It shows as it enters (half way
/// if it can rest there: by default at half its height, at most half the window's), and drags
/// between its anchors (with `sheet_gestures_enabled`); a click on the drag handle expands it,
/// or dismisses it once expanded. Once hidden by the scrim, a drag, the drag handle or back
/// (Escape, with `properties`), `on_dismiss_request` is called (stop calling
/// `ModalBottomSheet` then).
#[composable]
pub fn ModalBottomSheet(
    on_dismiss_request: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_bottom_sheet_state(
        crate::SheetValue::Hidden,
        &crate::SheetValue::ALL,
        crate::always_confirm(),
    ))]
    sheet_state: SheetState,
    #[default(crate::BottomSheetDefaults::SHEET_MAX_WIDTH)] sheet_max_width: Dp,
    #[default(true)] sheet_gestures_enabled: bool,
    #[default(crate::BottomSheetDefaults::expanded_shape())] shape: Shape,
    #[default(crate::BottomSheetDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::Dp(0.0))] tonal_elevation: Dp,
    #[default(crate::BottomSheetDefaults::scrim_color())] scrim_color: Color,
    #[default(crate::default_drag_handle())] drag_handle: Option<impl Fn() + 'static>,
    #[default(crate::BottomSheetDefaults::modal_window_insets())]
    content_window_insets: WindowInsets,
    #[default(crate::ModalBottomSheetDefaults::PROPERTIES)] properties: ModalBottomSheetProperties,
    content: impl Fn(ColumnScope) + 'static,
) {
    let scope = remember_coroutine_scope();
    let density = compose_ui::LOCAL_DENSITY.current();
    // The motion scheme's springs, as Compose's `BottomSheet` sets them.
    {
        let state = sheet_state;
        let motion = MaterialTheme::motion_scheme();
        compose_runtime::SideEffect(move || state.set_motion(&motion));
    }
    {
        // Shown on entering once laid out, if it can rest expanded (an empty sheet can't),
        // unless it's already shown (a state restored expanded), as Compose's with
        // `isModalBottomSheetStateRestorationFixEnabled` (on by default).
        let state = sheet_state;
        LaunchedEffect(sheet_state, async move {
            for _ in 0..8 {
                if !state.require_offset().is_nan() {
                    break;
                }
                compose_runtime::with_frame_nanos(|_| ()).await;
            }
            if state.has_expanded_state() && state.target_value() == SheetValue::Hidden {
                state.show().await;
            }
        });
    }
    // Only the modal sheet's offset is consumed, as Compose's: a `BottomSheet` in place has its
    // provided insets only.
    let parts = sheet_parts(
        sheet_state,
        on_dismiss_request,
        modifier.consume_window_insets(WindowInsets::from_provider(SheetWindowInsets(sheet_state))),
        sheet_gestures_enabled,
        scope,
        density,
    );
    let policy = {
        let state = sheet_state;
        compose_runtime::remember_keyed((sheet_state, sheet_max_width), move || {
            Rc::new(SheetLayout { state, max_width: sheet_max_width }) as Rc<dyn MeasurePolicy>
        })
    };
    // The scrim fades in while the sheet is going anywhere but hidden, on the default effects
    // spring.
    let scrim_alpha = compose_animation::animate_float_as_state(
        if sheet_state.target_value() != SheetValue::Hidden { 1.0 } else { 0.0 },
        MaterialTheme::motion_scheme().default_effects_spec(),
        Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
        "ScrimAlpha",
        None,
    );
    let provider = remember(|| Rc::new(AtWindowTop) as Rc<dyn PopupPositionProvider>);
    let popup_properties = PopupProperties {
        focusable: true,
        dismiss_on_back_press: properties.should_dismiss_on_back_press,
        dismiss_on_click_outside: false,
        fills_window: true,
        pass_through_outside: false,
    };
    let SheetParts { dismiss, back, modifier: sheet_modifier, handle_click, handle_semantics } =
        parts;
    compose_ui::PopupWithPositionProvider(
        provider,
        Some(back),
        popup_properties,
        compose_runtime::callback(move || {
            let (dismiss, content, drag_handle, handle_click) =
                (dismiss.clone(), content.clone(), drag_handle.clone(), handle_click.clone());
            let handle_semantics = handle_semantics.clone();
            let sheet_modifier = sheet_modifier.clone();
            let shape = shape.clone();
            Layout(
                // Read in order: the sheet, then the scrim's close (`Scrim`'s traversal index).
                Modifier.semantics(|config| config.set_is_traversal_group(true)),
                policy.clone(),
                compose_runtime::callback(move || {
                    Box(
                        Modifier
                            .fill_max_size()
                            .draw_behind(move |scope| {
                                let alpha = scrim_color.alpha() as f32 / 255.0 * scrim_alpha.get();
                                scope.draw_rect(
                                    scope.bounds(),
                                    compose_ui::Paint::fill(scrim_color.with_alpha(alpha)),
                                );
                            })
                            .then_custom(crate::drawer::ScrimElement {
                                close: dismiss.clone(),
                                enabled: properties.should_dismiss_on_click_outside,
                                description: Some("Close sheet".into()),
                                traversal_index: Some(1.0),
                            }),
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(|_| {}),
                    );
                    sheet_surface(
                        sheet_state,
                        sheet_modifier.clone(),
                        shape.clone(),
                        container_color,
                        content_color,
                        tonal_elevation,
                        Dp(0.0),
                        content_window_insets,
                        drag_handle.clone(),
                        handle_click.clone(),
                        handle_semantics.clone(),
                        content.clone(),
                    );
                }),
            );
        }),
    );
}

/// A bottom sheet in place, as Compose's `BottomSheet`: no window and no scrim, at the bottom of
/// the space it's given (up to `max_width`, centered), its drag handle and content in a column.
/// It stays where its `state` says; `state.show()` shows it. Drags move it between its anchors
/// (with `gestures_enabled`); back (with `back_handler_enabled`, while it's visible) takes an
/// expanded sheet half way if it can rest there, else hides it and calls `on_dismiss_request`.
#[composable]
#[allow(clippy::too_many_arguments)]
pub fn BottomSheet(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_bottom_sheet_state(
        crate::SheetValue::Hidden,
        &crate::SheetValue::ALL,
        crate::always_confirm(),
    ))]
    state: SheetState,
    #[default(|| {})] on_dismiss_request: impl Fn() + 'static,
    #[default(crate::BottomSheetDefaults::SHEET_MAX_WIDTH)] max_width: Dp,
    #[default(true)] gestures_enabled: bool,
    #[default(true)] back_handler_enabled: bool,
    #[default(crate::default_drag_handle())] drag_handle: Option<impl Fn() + 'static>,
    #[default(crate::BottomSheetDefaults::standard_window_insets())]
    content_window_insets: WindowInsets,
    #[default(crate::BottomSheetDefaults::expanded_shape())] shape: Shape,
    #[default(crate::BottomSheetDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::BottomSheetDefaults::ELEVATION)] tonal_elevation: Dp,
    #[default(crate::Dp(0.0))] shadow_elevation: Dp,
    content: impl Fn(ColumnScope) + 'static,
) {
    let scope = remember_coroutine_scope();
    let density = compose_ui::LOCAL_DENSITY.current();
    let motion = MaterialTheme::motion_scheme();
    compose_runtime::SideEffect(move || state.set_motion(&motion));
    let SheetParts { back, modifier: sheet_modifier, handle_click, handle_semantics, .. } =
        sheet_parts(state, on_dismiss_request, modifier, gestures_enabled, scope, density);
    compose_ui::BackHandler(back_handler_enabled && state.is_visible(), back);
    let policy = compose_runtime::remember_keyed((state, max_width), move || {
        Rc::new(SheetLayout { state, max_width }) as Rc<dyn MeasurePolicy>
    });
    Layout(
        Modifier,
        policy,
        compose_runtime::callback(move || {
            sheet_surface(
                state,
                sheet_modifier.clone(),
                shape.clone(),
                container_color,
                content_color,
                tonal_elevation,
                shadow_elevation,
                content_window_insets,
                drag_handle.clone(),
                handle_click.clone(),
                handle_semantics.clone(),
                content.clone(),
            );
        }),
    );
}

/// What a modal and a standalone sheet share, as Compose's `BottomSheetImpl`.
struct SheetParts {
    /// Hides it (if `confirm_value_change` lets it), then tells the caller.
    dismiss: compose_runtime::Callback,
    /// Back: an expanded sheet that can rest half way goes there, else as `dismiss`.
    back: compose_runtime::Callback,
    /// The sheet's own modifier: its drags and nested scroll, its pane.
    modifier: Modifier,
    /// A click on the drag handle.
    handle_click: compose_runtime::Callback,
    /// The drag handle's actions.
    handle_semantics: Rc<dyn Fn(&mut compose_ui::SemanticsConfiguration)>,
}

fn sheet_parts(
    sheet_state: SheetState,
    on_dismiss_request: compose_runtime::Callback,
    modifier: Modifier,
    sheet_gestures_enabled: bool,
    scope: compose_runtime::CoroutineScope,
    density: compose_ui::Density,
) -> SheetParts {
    // Hides the sheet, then tells the caller.
    // The scrim hides the sheet (if `confirm_value_change` lets it), then tells the caller, as
    // Compose's `animateToDismiss`.
    let dismiss = {
        let (state, scope, on_dismiss) = (sheet_state, scope, on_dismiss_request.clone());
        compose_runtime::callback(move || {
            if !state.confirms(SheetValue::Hidden) {
                return;
            }
            let (state, on_dismiss) = (state, on_dismiss.clone());
            scope.launch(async move {
                state.hide().await;
                if !state.is_visible() {
                    on_dismiss();
                }
            });
        })
    };
    // Back (Escape) takes an expanded sheet that can rest half way there, else hides it and
    // tells the caller, as Compose's sheet dialog's `onDismissRequest`.
    let back = {
        let (state, scope, on_dismiss) = (sheet_state, scope, on_dismiss_request.clone());
        compose_runtime::callback(move || {
            let (state, on_dismiss) = (state, on_dismiss.clone());
            if state.current_value() == SheetValue::Expanded && state.has_partially_expanded_state()
            {
                scope.launch(async move { state.partial_expand().await });
            } else {
                scope.launch(async move {
                    state.hide().await;
                    if !state.is_visible() {
                        on_dismiss();
                    }
                });
            }
        })
    };
    let anchored_state = sheet_state.anchored_state();
    let settle = sheet_state.remember_settling();
    // The newest `on_dismiss_request`: a fling outlives its composition.
    let latest_dismiss = compose_runtime::remember(|| Rc::new(RefCell::new(None)));
    *latest_dismiss.borrow_mut() = Some(on_dismiss_request.clone());
    let max_velocity = compose_ui::LOCAL_VIEW_CONFIGURATION.current().maximum_fling_velocity;
    let fling: Rc<dyn FlingBehavior> = compose_runtime::remember_keyed(
        (FlingKey(settle.clone()), sheet_state, density, max_velocity.to_bits()),
        move || {
            Rc::new(ModalSheetFling {
                state: sheet_state,
                settle,
                density,
                max_velocity,
                on_dismiss: latest_dismiss,
            }) as Rc<dyn FlingBehavior>
        },
    );
    // The content's scrolls drag the sheet within its anchors first (up) or after it (down),
    // and its flings settle it, as Compose's `ConsumeSwipeWithinBottomSheetBoundsNestedScroll
    // Connection`.
    let connection = {
        let (state, fling) = (sheet_state, fling.clone());
        compose_runtime::remember_keyed((sheet_state, FlingKey(fling.clone())), move || {
            Rc::new(SheetConnection { state, fling }) as Rc<dyn NestedScrollConnection>
        })
    };
    // Dragged once shown, and part of its content's scrolls, with its gestures.
    let gestures = AnchoredDraggableOptions {
        enabled: sheet_gestures_enabled && sheet_state.current_value() != SheetValue::Hidden,
        fling_behavior: Some(fling),
        ..AnchoredDraggableOptions::default()
    };
    let sheet_modifier =
        if sheet_gestures_enabled { modifier.nested_scroll(connection) } else { modifier }
            .anchored_draggable(anchored_state, Orientation::Vertical, gestures)
            .semantics(|config| {
                config.set_pane_title("Bottom Sheet");
                config.set_traversal_index(0.0);
            });
    // The drag handle's actions, by the sheet's value, while its gestures are on, as Compose's.
    let handle_semantics = {
        let (state, scope, dismiss) = (sheet_state, scope, dismiss.clone());
        move |config: &mut compose_ui::SemanticsConfiguration| {
            if !sheet_gestures_enabled {
                return;
            }
            if let Some(inner) = state.inner() {
                // New anchors change what it offers.
                inner.anchored.anchors();
            }
            let on_dismiss = dismiss.clone();
            config.dismiss(Some("Dismiss bottom sheet".into()), move || {
                on_dismiss();
                true
            });
            if state.current_value() == SheetValue::PartiallyExpanded {
                config.expand(Some("Expand bottom sheet".into()), move || {
                    if state.confirms(SheetValue::Expanded) {
                        scope.launch(async move { state.expand().await });
                    }
                    true
                });
            } else if state.has_partially_expanded_state() {
                config.collapse(Some("Collapse bottom sheet".into()), move || {
                    if state.confirms(SheetValue::PartiallyExpanded) {
                        scope.launch(async move { state.partial_expand().await });
                    }
                    true
                });
            }
        }
    };
    let handle_semantics: Rc<dyn Fn(&mut compose_ui::SemanticsConfiguration)> =
        Rc::new(handle_semantics);
    // A click on the drag handle expands the sheet, dismisses it once expanded, as Compose's.
    let handle_click = {
        let (state, scope, dismiss) = (sheet_state, scope, dismiss.clone());
        compose_runtime::callback(move || match state.current_value() {
            SheetValue::Expanded => dismiss(),
            SheetValue::PartiallyExpanded => {
                scope.launch(async move { state.expand().await });
            }
            SheetValue::Hidden => {
                scope.launch(async move { state.show().await });
            }
        })
    };
    SheetParts { dismiss, back, modifier: sheet_modifier, handle_click, handle_semantics }
}

/// A drag handle's actions, read when the semantics are (they depend on the anchors, which the
/// layout finds after the composition), merging what's in it, as Compose's lambda.
pub(crate) struct HandleSemantics(pub(crate) Rc<dyn Fn(&mut compose_ui::SemanticsConfiguration)>);

pub(crate) struct HandleSemanticsNode(Rc<dyn Fn(&mut compose_ui::SemanticsConfiguration)>);

impl compose_ui::ModifierNodeElement for HandleSemantics {
    type Node = HandleSemanticsNode;
    fn create(&self) -> HandleSemanticsNode {
        HandleSemanticsNode(self.0.clone())
    }
    fn update(&self, node: &mut HandleSemanticsNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    fn name(&self) -> &'static str {
        "sheet_drag_handle_semantics"
    }
}

impl compose_ui::ModifierNode for HandleSemanticsNode {
    fn as_semantics(&mut self) -> Option<&mut dyn compose_ui::SemanticsModifierNode> {
        Some(self)
    }
}

impl compose_ui::SemanticsModifierNode for HandleSemanticsNode {
    fn apply_semantics(&self, config: &mut compose_ui::SemanticsConfiguration) {
        config.is_merging_semantics_of_descendants = true;
        (self.0)(config);
    }
}

/// The sheet: a surface of `shape`, its drag handle (clickable, with its actions) and content in
/// a column.
#[allow(clippy::too_many_arguments)]
fn sheet_surface(
    state: SheetState,
    modifier: Modifier,
    shape: Shape,
    container_color: Color,
    content_color: Color,
    tonal_elevation: Dp,
    shadow_elevation: Dp,
    content_window_insets: WindowInsets,
    drag_handle: Option<compose_runtime::Callback>,
    handle_click: compose_runtime::Callback,
    handle_semantics: Rc<dyn Fn(&mut compose_ui::SemanticsConfiguration)>,
    content: compose_runtime::Callback<dyn Fn(ColumnScope)>,
) {
    // How far the state's past its top anchor, as a spring overshoots.
    let overflow_scale = move |height: f32| {
        let overflow = state.inner().map_or(0.0, |inner| {
            let (offset, top) = (inner.anchored.offset(), inner.anchored.anchors().min_position());
            if offset < top { top - offset } else { 0.0 }
        });
        if overflow > 0.0 && height > 0.0 { (height + overflow) / height } else { 1.0 }
    };
    let at_top = compose_ui::TransformOrigin { pivot_fraction_x: 0.5, pivot_fraction_y: 0.0 };
    crate::components::Surface(
        // Scaled up so no gap shows below, as `verticalScaleUp`.
        modifier.fill_max_width().graphics_layer(move |layer| {
            layer.scale_y = overflow_scale(layer.size.height);
            layer.transform_origin = at_top;
        }),
        shape,
        container_color,
        content_color,
        tonal_elevation,
        shadow_elevation,
        None,
        compose_runtime::callback(move || {
            let (drag_handle, handle_click) = (drag_handle.clone(), handle_click.clone());
            let handle_semantics = handle_semantics.clone();
            let content = content.clone();
            Column(
                // And the content back, as `verticalScaleDown`.
                Modifier
                    .fill_max_width()
                    .graphics_layer(move |layer| {
                        layer.scale_y = 1.0 / overflow_scale(layer.size.height);
                        layer.transform_origin = at_top;
                    })
                    .window_insets_padding(content_window_insets.clone()),
                compose_foundation::layout::Arrangement::TOP,
                Alignment::START,
                compose_runtime::callback(move |column: ColumnScope| {
                    if let Some(drag_handle) = drag_handle.clone() {
                        let click = handle_click.clone();
                        let semantics = handle_semantics.clone();
                        Box(
                            Modifier
                                .align(column, Alignment::CENTER_HORIZONTALLY)
                                .clickable(click)
                                .then_custom(HandleSemantics(semantics)),
                            Alignment::TOP_START,
                            false,
                            compose_runtime::callback(move |_| drag_handle()),
                        );
                    }
                    content(column);
                }),
            );
        }),
    );
}

/// The scrim over the window, and the sheet at the bottom: as wide as allowed up to its
/// maximum, centered, its top at the state's offset; the anchors from its height.
struct SheetLayout {
    state: SheetState,
    max_width: Dp,
}

impl MeasurePolicy for SheetLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let (scrim, sheet) = match measurables {
            [scrim, sheet] => (Some(scrim), sheet),
            [sheet] => (None, sheet),
            _ => return scope.layout(constraints.min_width, constraints.min_height),
        };
        let (width, height) = (constraints.max_width, constraints.max_height);
        let scrim = scrim.map(|scrim| scope.measure(*scrim, Constraints::fixed(width, height)));
        let sheet_width = width.min(scope.density().round_to_px(self.max_width));
        let sheet = scope.measure(*sheet, Constraints::new(0, sheet_width, 0, height));
        let full = height as f32;
        let sheet_height = sheet.height as f32;
        let Some(inner) = self.state.inner() else {
            if let Some(scrim) = &scrim {
                scope.place(scrim, 0, 0);
            }
            return scope.layout(width, height);
        };
        // Hidden below the window; half way when it can rest there (Compose's
        // `isPartiallyExpandedAnchorAvailable` and `calculatePartiallyExpandedOffset`); expanded
        // at its height.
        let mut anchors = Vec::with_capacity(3);
        anchors.push((SheetValue::Hidden, full));
        // Half the sheet's height, at most half the window's (Compose's flag
        // `isBottomSheetPartiallyExpandedDeterministicEnabled`, on by default).
        if !inner.skip_partially_expanded && sheet_height != 0.0 {
            let visible = (full / 2.0).min(sheet_height / 2.0);
            anchors.push((SheetValue::PartiallyExpanded, full - visible));
        }
        if sheet_height != 0.0 {
            anchors.push((SheetValue::Expanded, (full - sheet_height).max(0.0)));
        }
        let new_anchors = DraggableAnchors::new(|config| {
            for (value, at) in &anchors {
                config.at(*value, *at);
            }
        });
        // Where it goes when its anchor went, as Compose's `draggableAnchors` new target.
        let retarget = compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            let has = |value: SheetValue| new_anchors.has_position_for(&value);
            match self.state.target_value() {
                SheetValue::Hidden => SheetValue::Hidden,
                SheetValue::PartiallyExpanded if has(SheetValue::PartiallyExpanded) => {
                    SheetValue::PartiallyExpanded
                }
                SheetValue::PartiallyExpanded | SheetValue::Expanded
                    if has(SheetValue::Expanded) =>
                {
                    SheetValue::Expanded
                }
                _ => SheetValue::Hidden,
            }
        });
        self.state.update_anchors(new_anchors, retarget);
        // Read while measuring: a frame of the slide places the sheet again only.
        let offset = inner.anchored.offset();
        let top = if offset.is_nan() { full } else { offset };
        if let Some(scrim) = &scrim {
            scope.place(scrim, 0, 0);
        }
        scope.place(&sheet, (width - sheet.width) / 2, top.round() as i32);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "ModalBottomSheet"
    }
}
