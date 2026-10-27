//! `ModalBottomSheet`, as Compose's Material 3: a sheet sliding up from the bottom of the
//! window over a scrim, dragged between hidden, partially expanded and expanded, with its
//! `SheetState`.

use compose_runtime::{ObjectHandle, Owned};
use std::cell::Cell;
use std::rc::Rc;

use compose_animation::{AnimationSpec, FAST_OUT_SLOW_IN_EASING, Spring};
use compose_foundation::gestures::{DraggableModifier, DraggableState, Orientation};
use compose_foundation::layout::{Box, Column, ColumnScope};
use compose_runtime::{ComposeArg, LaunchedEffect, composable, remember, remember_coroutine_scope};
use compose_ui::{
    Alignment, Color, Constraints, Dp, IntOffset, IntSize, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, PaddingValues, PopupPositionProvider,
    PopupProperties, Rect, Shape, UnitExt,
};
use compose_ui::{NestedScrollConnection, NestedScrollSource, Offset};

use crate::anchored::Anchored;
use crate::theme::MaterialTheme;

/// Where a sheet rests, as Compose's `SheetValue`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetValue {
    Hidden,
    Expanded,
    PartiallyExpanded,
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

struct SheetInner {
    anchored: Anchored,
    skip_partially_expanded: bool,
    skip_hidden_state: bool,
    /// Half its content high when partially expanded (at most half the window), and that
    /// whatever its height, as Compose's `isBottomSheetPartiallyExpandedDeterministicEnabled`
    /// (the constructor taking the enabled values); else half the window, only if taller
    /// (the legacy states).
    deterministic: bool,
    /// How far a drag must go to settle at the next anchor, in pixels, as
    /// `positionalThreshold`.
    positional_threshold: Box<dyn Fn() -> f32>,
    /// A release faster than this (pixels per second) settles in its direction, as
    /// `velocityThreshold`.
    velocity_threshold: Box<dyn Fn() -> f32>,
    confirm_value_change: Rc<dyn Fn(SheetValue) -> bool>,
    /// What showing and expanding animate with, and hiding and partially expanding, as
    /// `showMotionSpec` and `hideMotionSpec`: at once until a sheet sets its motion.
    show_spec: Cell<AnimationSpec>,
    hide_spec: Cell<AnimationSpec>,
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
    fn inner(&self) -> Option<Rc<SheetInner>> {
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
            true,
        )
    }

    /// The legacy state, as Compose's deprecated `SheetState(skipPartiallyExpanded,
    /// positionalThreshold, velocityThreshold, initialValue, confirmValueChange,
    /// skipHiddenState)`: partially expanded at half the window, only for a sheet taller than
    /// that.
    #[track_caller]
    pub fn new_skipping(
        skip_partially_expanded: bool,
        positional_threshold: impl Fn() -> f32 + 'static,
        velocity_threshold: impl Fn() -> f32 + 'static,
        initial_value: SheetValue,
        confirm_value_change: impl Fn(SheetValue) -> bool + 'static,
        skip_hidden_state: bool,
    ) -> Owned<SheetState> {
        Self::create(
            &enabled_values(skip_partially_expanded, skip_hidden_state),
            Box::new(positional_threshold),
            Box::new(velocity_threshold),
            initial_value,
            Rc::new(confirm_value_change),
            false,
        )
    }

    #[track_caller]
    fn create(
        enabled_values: &[SheetValue],
        positional_threshold: Box<dyn Fn() -> f32>,
        velocity_threshold: Box<dyn Fn() -> f32>,
        initial_value: SheetValue,
        confirm_value_change: Rc<dyn Fn(SheetValue) -> bool>,
        deterministic: bool,
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
                anchored: Anchored::new(
                    initial_value as u8,
                    Box::new(move |key| confirm(SheetValue::from_key(key))),
                    BOTTOM_SHEET_ANIMATION_SPEC,
                ),
                skip_partially_expanded: !enabled_values.contains(&SheetValue::PartiallyExpanded),
                skip_hidden_state: !enabled_values.contains(&SheetValue::Hidden),
                deterministic,
                positional_threshold,
                velocity_threshold,
                confirm_value_change,
                show_spec: Cell::new(compose_animation::snap(0).into()),
                hide_spec: Cell::new(compose_animation::snap(0).into()),
            },
            |object| SheetState { object },
        )
    }

    /// Where it last settled (a state read), as `currentValue` (the anchored state's settled
    /// value).
    pub fn current_value(&self) -> SheetValue {
        self.inner().map_or(SheetValue::Hidden, |inner| {
            SheetValue::from_key(inner.anchored.settled_value())
        })
    }

    /// Where it's going (a state read), as `targetValue`.
    pub fn target_value(&self) -> SheetValue {
        self.inner()
            .map_or(SheetValue::Hidden, |inner| SheetValue::from_key(inner.anchored.target_value()))
    }

    /// Whether it's showing, as `isVisible`: not hidden, or dragged more than half way out of
    /// hiding (the anchored state's current value).
    pub fn is_visible(&self) -> bool {
        self.inner().is_some_and(|inner| {
            SheetValue::from_key(inner.anchored.current_value()) != SheetValue::Hidden
        })
    }

    /// Whether it's animating to a value (a state read), as `isAnimationRunning`.
    pub fn is_animation_running(&self) -> bool {
        self.inner().is_some_and(|inner| inner.anchored.is_animation_running())
    }

    /// Whether it can rest expanded, as `hasExpandedState`.
    pub fn has_expanded_state(&self) -> bool {
        self.inner().is_some_and(|inner| inner.anchored.has_anchor(SheetValue::Expanded as u8))
    }

    /// Whether it can rest half way, as `hasPartiallyExpandedState`.
    pub fn has_partially_expanded_state(&self) -> bool {
        self.inner()
            .is_some_and(|inner| inner.anchored.has_anchor(SheetValue::PartiallyExpanded as u8))
    }

    /// Its top's offset from the top of the window, in pixels (a state read; NaN before it's
    /// laid out), as `requireOffset()`.
    pub fn require_offset(&self) -> f32 {
        self.inner().map_or(f32::NAN, |inner| inner.anchored.offset())
    }

    /// Whether `confirm_value_change` lets it go to `value`.
    fn confirms(&self, value: SheetValue) -> bool {
        self.inner().is_some_and(|inner| (inner.confirm_value_change)(value))
    }

    /// Animates to `value`, if `confirm_value_change` lets it, with the show or hide motion.
    async fn animate_to(&self, value: SheetValue, show: bool) {
        if let Some(inner) = self.inner()
            && (inner.confirm_value_change)(value)
        {
            let spec = if show { inner.show_spec.get() } else { inner.hide_spec.get() };
            inner.anchored.animate_to_with(value as u8, spec).await;
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
    pub fn snap_to(&self, value: SheetValue) {
        if let Some(inner) = self.inner() {
            inner.anchored.snap_to(value as u8);
        }
    }

    /// Sets what it animates with, as Compose's sheets do from the motion scheme: showing and
    /// expanding with the default spatial spring, hiding with the fast effects one, drags
    /// settling with the default spatial one.
    fn set_motion(&self) {
        if let Some(inner) = self.inner() {
            inner.show_spec.set(AnimationSpec::Spring(crate::motion::DEFAULT_SPATIAL));
            inner.hide_spec.set(AnimationSpec::Spring(crate::motion::FAST_EFFECTS));
            inner.anchored.set_spec(AnimationSpec::Spring(crate::motion::DEFAULT_SPATIAL));
        }
    }
}

/// The values of the legacy constructor's flags.
fn enabled_values(skip_partially_expanded: bool, skip_hidden_state: bool) -> Vec<SheetValue> {
    let mut values = vec![SheetValue::Expanded];
    if !skip_partially_expanded {
        values.push(SheetValue::PartiallyExpanded);
    }
    if !skip_hidden_state {
        values.push(SheetValue::Hidden);
    }
    values
}

/// A 300 ms ease, as Compose's `BottomSheetAnimationSpec`: drags settle with it until a sheet
/// sets its motion.
const BOTTOM_SHEET_ANIMATION_SPEC: AnimationSpec =
    AnimationSpec::Tween(compose_animation::TweenSpec {
        duration_millis: 300,
        delay_millis: 0,
        easing: FAST_OUT_SLOW_IN_EASING,
    });

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
            true,
        )
    }

    /// The legacy states' saver, as the deprecated `SheetState.Saver(skipPartiallyExpanded,
    /// positionalThreshold, velocityThreshold, confirmValueChange, skipHiddenState)`.
    pub fn saver_skipping(
        skip_partially_expanded: bool,
        positional_threshold: impl Fn() -> f32 + 'static,
        velocity_threshold: impl Fn() -> f32 + 'static,
        confirm_value_change: impl Fn(SheetValue) -> bool + 'static,
        skip_hidden_state: bool,
    ) -> impl compose_runtime::Saver<Owned<SheetState>> {
        Self::saver_with(
            enabled_values(skip_partially_expanded, skip_hidden_state),
            Rc::new(positional_threshold),
            Rc::new(velocity_threshold),
            Rc::new(confirm_value_change),
            false,
        )
    }

    fn saver_with(
        enabled_values: Vec<SheetValue>,
        positional_threshold: Rc<dyn Fn() -> f32>,
        velocity_threshold: Rc<dyn Fn() -> f32>,
        confirm_value_change: Rc<dyn Fn(SheetValue) -> bool>,
        deterministic: bool,
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
                        deterministic,
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
    deterministic: bool,
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
            deterministic,
        ),
        move || {
            let (positional, velocity) = (positional.clone(), velocity.clone());
            SheetState::create(
                &values,
                Box::new(move || positional()),
                Box::new(move || velocity()),
                initial_value,
                confirm_value_change.clone(),
                deterministic,
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
    remember_sheet_state(enabled_values, confirm, initial_value, true)
}

/// A modal sheet's legacy state remembered here, hidden, as the deprecated
/// `rememberModalBottomSheetState(skipPartiallyExpanded, confirmValueChange)`: partially
/// expanded at half the window, only for a sheet taller than that. Saved across the host's
/// recreation; `confirm_value_change` is the first composition's.
#[composable(non_restartable)]
pub fn remember_modal_bottom_sheet_state(
    #[default(false)] skip_partially_expanded: bool,
    #[default(|_| true)] confirm_value_change: impl Fn(SheetValue) -> bool + 'static,
) -> SheetState {
    let confirm: Rc<dyn Fn(SheetValue) -> bool> = Rc::new(move |value| confirm_value_change(value));
    remember_sheet_state(
        &enabled_values(skip_partially_expanded, false),
        confirm,
        SheetValue::Hidden,
        false,
    )
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

    /// As `windowInsets`: none until hosts report them.
    pub fn window_insets() -> PaddingValues {
        PaddingValues::ZERO
    }

    /// The drag handle, as `DragHandle()`: a 32 by 4 dp pill in the on-surface-variant color
    /// at 40 %, 22 dp above and below.
    pub fn drag_handle() {
        let color = MaterialTheme::color_scheme().on_surface_variant.with_alpha(0.4);
        crate::components::Surface(
            Modifier.padding((Dp(0.0), 22.dp())).semantics(|config| {
                config.set_content_description("Drag handle");
            }),
            MaterialTheme::shapes().extra_large,
            color,
            Color::UNSPECIFIED,
            Dp(0.0),
            Dp(0.0),
            None,
            compose_runtime::callback(|| {
                compose_foundation::layout::Spacer(Modifier.size((32.dp(), 4.dp())));
            }),
        );
    }
}

/// The sheet's part in its content's scrolls.
struct SheetConnection {
    state: SheetState,
    settle: std::cell::RefCell<Rc<dyn Fn(f32)>>,
}

impl NestedScrollConnection for SheetConnection {
    /// Scrolling the content up drags the sheet up first, as far as it goes.
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        if available.y < 0.0
            && source == NestedScrollSource::UserInput
            && let Some(inner) = self.state.inner()
        {
            Offset::new(0.0, inner.anchored.drag_by(available.y))
        } else {
            Offset::ZERO
        }
    }

    /// What the content leaves scrolling down (it's at its top) drags the sheet down.
    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        if source == NestedScrollSource::UserInput
            && let Some(inner) = self.state.inner()
        {
            Offset::new(0.0, inner.anchored.drag_by(available.y))
        } else {
            Offset::ZERO
        }
    }

    /// A fling up while the sheet isn't at its top settles the sheet instead.
    fn on_pre_fling(&self, available: Offset) -> Offset {
        let Some(inner) = self.state.inner() else { return Offset::ZERO };
        let anchored = &inner.anchored;
        if available.y < 0.0 && anchored.offset() > anchored.bounds().0 {
            (self.settle.borrow())(available.y);
            available
        } else {
            Offset::ZERO
        }
    }

    /// After the content's fling (or a drag let go), the sheet settles.
    fn on_post_fling(&self, _: Offset, available: Offset) -> Offset {
        (self.settle.borrow())(available.y);
        available
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
    #[default(crate::BottomSheetDefaults::window_insets())] content_window_insets: PaddingValues,
    #[default(crate::ModalBottomSheetDefaults::PROPERTIES)] properties: ModalBottomSheetProperties,
    content: impl Fn(ColumnScope) + 'static,
) {
    let scope = remember_coroutine_scope();
    let density = compose_ui::LOCAL_DENSITY.current();
    // The motion scheme's springs, as Compose's `BottomSheet` sets them.
    {
        let state = sheet_state;
        compose_runtime::SideEffect(move || state.set_motion());
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
    let settle = {
        let (state, scope, on_dismiss) = (sheet_state, scope, on_dismiss_request);
        let dampening_zone = density.to_px(BottomSheetDefaults::BOUNDARY_DAMPENING_ZONE);
        let velocity_floor = density.to_px(BottomSheetDefaults::VELOCITY_THRESHOLD);
        Rc::new(move |velocity: f32| {
            let (state, on_dismiss) = (state, on_dismiss.clone());
            scope.launch(async move {
                if let Some(inner) = state.inner() {
                    // A fling down near the hidden anchor slows, quadratically, though one
                    // fast enough to settle stays so, as Compose's sheet fling behavior.
                    let mut velocity = velocity;
                    if velocity > 0.0
                        && let Some(hidden) = inner.anchored.position_of(SheetValue::Hidden as u8)
                    {
                        let distance = (hidden - inner.anchored.offset()).max(0.0);
                        if distance < dampening_zone {
                            let factor = distance / dampening_zone;
                            let initial = velocity;
                            velocity *= factor * factor;
                            if initial >= velocity_floor {
                                velocity = velocity.max(velocity_floor);
                            }
                        }
                    }
                    let positional = (inner.positional_threshold)();
                    let threshold = (inner.velocity_threshold)();
                    inner.anchored.settle(velocity, &|_| positional, threshold).await;
                }
                if !state.is_visible() {
                    on_dismiss();
                }
            });
        }) as Rc<dyn Fn(f32)>
    };
    // The content's scrolls drag the sheet within its anchors first (up) or after it (down),
    // and its flings settle it, as Compose's `ConsumeSwipeWithinBottomSheetBoundsNestedScroll
    // Connection`.
    let connection = {
        let (state, settle) = (sheet_state, settle.clone());
        compose_runtime::remember_keyed(sheet_state, move || {
            Rc::new(SheetConnection { state, settle: std::cell::RefCell::new(settle) })
        })
    };
    *connection.settle.borrow_mut() = settle.clone();
    let connection: Rc<dyn NestedScrollConnection> = connection;
    let drag = {
        let state = sheet_state;
        compose_runtime::remember_keyed(sheet_state, move || {
            DraggableState::new(move |delta| {
                if let Some(inner) = state.inner() {
                    inner.anchored.drag_by(delta);
                }
            })
        })
    };
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
        AnimationSpec::Spring(crate::motion::DEFAULT_EFFECTS),
        Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
        "ScrimAlpha",
        None,
    );
    let provider = remember(|| Rc::new(AtWindowTop) as Rc<dyn PopupPositionProvider>);
    let popup_properties = PopupProperties {
        focusable: true,
        dismiss_on_back_press: properties.should_dismiss_on_back_press,
        dismiss_on_click_outside: false,
    };
    // Dragged once shown, and part of its content's scrolls, with its gestures.
    let sheet_modifier = modifier.draggable(
        drag,
        Orientation::Vertical,
        sheet_gestures_enabled && sheet_state.current_value() != SheetValue::Hidden,
        false,
        |_| {},
        {
            let settle = settle.clone();
            move |velocity| settle(velocity)
        },
    );
    let sheet_modifier = if sheet_gestures_enabled {
        sheet_modifier.nested_scroll(connection)
    } else {
        sheet_modifier
    }
    .semantics(|config| config.set_content_description("Bottom sheet"));
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
    compose_ui::PopupWithPositionProvider(
        provider,
        Some(back),
        popup_properties,
        compose_runtime::callback(move || {
            let (dismiss, content, drag_handle, handle_click) =
                (dismiss.clone(), content.clone(), drag_handle.clone(), handle_click.clone());
            let sheet_modifier = sheet_modifier.clone();
            Layout(
                Modifier,
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
                                description: Some("Close sheet"),
                            }),
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(|_| {}),
                    );
                    let (content, drag_handle, handle_click) =
                        (content.clone(), drag_handle.clone(), handle_click.clone());
                    crate::components::Surface(
                        sheet_modifier.clone().fill_max_width(),
                        shape,
                        container_color,
                        content_color,
                        tonal_elevation,
                        Dp(0.0),
                        None,
                        compose_runtime::callback(move || {
                            let (drag_handle, handle_click) =
                                (drag_handle.clone(), handle_click.clone());
                            let content = content.clone();
                            Column(
                                Modifier.fill_max_width().padding(content_window_insets),
                                compose_foundation::layout::Arrangement::TOP,
                                Alignment::START,
                                compose_runtime::callback(move |column: ColumnScope| {
                                    if let Some(drag_handle) = drag_handle.clone() {
                                        let click = handle_click.clone();
                                        Box(
                                            Modifier
                                                .align(column, Alignment::CENTER_HORIZONTALLY)
                                                .clickable(move || click()),
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
        let [scrim, sheet] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let (width, height) = (constraints.max_width, constraints.max_height);
        let scrim = scope.measure(*scrim, Constraints::fixed(width, height));
        let sheet_width = width.min(scope.density().round_to_px(self.max_width));
        let sheet = scope.measure(*sheet, Constraints::new(0, sheet_width, 0, height));
        let full = height as f32;
        let sheet_height = sheet.height as f32;
        let Some(inner) = self.state.inner() else {
            scope.place(&scrim, 0, 0);
            return scope.layout(width, height);
        };
        // Hidden below the window; half way when it can rest there (Compose's
        // `isPartiallyExpandedAnchorAvailable` and `calculatePartiallyExpandedOffset`); expanded
        // at its height.
        let mut anchors = Vec::with_capacity(3);
        anchors.push((SheetValue::Hidden, full));
        if !inner.skip_partially_expanded
            && sheet_height != 0.0
            && (inner.deterministic || sheet_height > full / 2.0)
        {
            let visible =
                if inner.deterministic { (full / 2.0).min(sheet_height / 2.0) } else { full / 2.0 };
            anchors.push((SheetValue::PartiallyExpanded, full - visible));
        }
        if sheet_height != 0.0 {
            anchors.push((SheetValue::Expanded, (full - sheet_height).max(0.0)));
        }
        let has = |value: SheetValue| anchors.iter().any(|(anchor, _)| *anchor == value);
        // Where it goes when its anchor went, as Compose's `draggableAnchors` new target.
        let retarget = match self.state.target_value() {
            SheetValue::Hidden => SheetValue::Hidden,
            SheetValue::PartiallyExpanded if has(SheetValue::PartiallyExpanded) => {
                SheetValue::PartiallyExpanded
            }
            SheetValue::PartiallyExpanded | SheetValue::Expanded if has(SheetValue::Expanded) => {
                SheetValue::Expanded
            }
            _ => SheetValue::Hidden,
        };
        let changed = inner
            .anchored
            .update_anchors(anchors.into_iter().map(|(value, at)| (value as u8, at)).collect());
        // New anchors take it to its target at once unless it's animating, as Compose's
        // `updateAnchors` (`trySnapTo`): where it rests follows the anchors anyway.
        if changed
            && !inner.anchored.is_animation_running()
            && (!inner.anchored.is_resting()
                || !inner.anchored.has_anchor(inner.anchored.settled_value()))
        {
            inner.anchored.snap_to(retarget as u8);
        }
        // Read while measuring: a frame of the slide places the sheet again only.
        let offset = inner.anchored.offset();
        let top = if offset.is_nan() { full } else { offset };
        scope.place(&scrim, 0, 0);
        scope.place(&sheet, (width - sheet.width) / 2, top.round() as i32);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "ModalBottomSheet"
    }
}
