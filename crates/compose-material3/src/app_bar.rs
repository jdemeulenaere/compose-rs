//! App bars, as Compose's Material 3: `TopAppBar`, `CenterAlignedTopAppBar`, `MediumTopAppBar`
//! and `LargeTopAppBar`, their flexible namesakes and `TwoRowsTopAppBar`, with their
//! `TopAppBarState` and `TopAppBarScrollBehavior`s (pinned, enter-always, exit-until-collapsed), and
//! `BottomAppBar` and `FlexibleBottomAppBar`, with `BottomAppBarState` and exit-always.

use compose_runtime::{ObjectHandle, Owned};
use std::rc::Rc;

use compose_animation::{
    AnimationSpec, AnimationState, DecayAnimationSpec, Easing, FAST_OUT_LINEAR_IN_EASING,
    animate_color_as_state,
};
use compose_foundation::layout::{
    Arrangement, Box, Column, HorizontalArrangement, Row, RowScope, WindowInsets,
    WindowInsetsModifiers, WindowInsetsSides,
};
use compose_runtime::{
    ComposeArg, CompositionLocalProvider, CoroutineScope, OwnedState, composable, derived_state_of,
    mutable_state_of,
};
use compose_ui::{
    Alignment, Color, Constraints, Dp, HorizontalAlignment, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, Offset, PaddingValues, TextStyle,
    UnitExt,
};

use compose_foundation::gestures::{
    DraggableModifier, DraggableOptions, Orientation, ScrollableState, remember_draggable_state,
};
use compose_runtime::Callback;

use crate::motion::MotionScheme;
use crate::theme::{LOCAL_CONTENT_COLOR, MaterialTheme, provide_content_color_text_style};
use compose_ui::{NestedScrollConnection, NestedScrollSource};

// ---- State and scroll behaviors -------------------------------------------------------------

struct TopAppBarStateInner {
    height_offset_limit: OwnedState<f32>,
    height_offset: OwnedState<f32>,
    content_offset: OwnedState<f32>,
    /// As `isScrollingContentAtStart`: `None`, it's taken to be.
    is_at_start: std::cell::RefCell<Option<Rc<dyn Fn() -> bool>>>,
}

impl TopAppBarStateInner {
    fn new(limit: f32, height_offset: f32, content_offset: f32) -> TopAppBarStateInner {
        TopAppBarStateInner {
            height_offset_limit: mutable_state_of(limit),
            // Not kept in the limit yet, as Compose's: only what's set later is.
            height_offset: mutable_state_of(height_offset),
            content_offset: mutable_state_of(content_offset),
            is_at_start: Default::default(),
        }
    }
}

/// How far a top app bar has collapsed and how far the content under it has scrolled, as
/// Compose's `TopAppBarState` (state reads and writes, in pixels).
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_top_app_bar_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TopAppBarState {
    object: ObjectHandle<TopAppBarStateInner>,
}

impl TopAppBarState {
    /// Its object, or `None` once its owner went (D240): then it does nothing, its reads
    /// giving the defaults.
    #[track_caller]
    fn inner(&self) -> Option<Rc<TopAppBarStateInner>> {
        self.object.get()
    }

    #[track_caller]
    pub fn new(
        initial_height_offset_limit: f32,
        initial_height_offset: f32,
        initial_content_offset: f32,
    ) -> Owned<TopAppBarState> {
        Owned::new(
            TopAppBarStateInner::new(
                initial_height_offset_limit,
                initial_height_offset,
                initial_content_offset,
            ),
            |object| TopAppBarState { object },
        )
    }

    /// The most the bar collapses (negative), as `heightOffsetLimit`: the bar sets it.
    pub fn height_offset_limit(&self) -> f32 {
        self.inner().map_or(-f32::MAX, |inner| inner.height_offset_limit.get())
    }

    pub fn set_height_offset_limit(&self, limit: f32) {
        if let Some(inner) = self.inner() {
            inner.height_offset_limit.set(limit);
        }
    }

    /// How far it's collapsed, between the limit and 0, as `heightOffset`.
    pub fn height_offset(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.height_offset.get())
    }

    /// Collapses it to `offset`, kept between the limit and 0.
    pub fn set_height_offset(&self, offset: f32) {
        let limit = self.height_offset_limit().min(0.0);
        let offset = offset.clamp(limit, 0.0);
        if let Some(inner) = self.inner() {
            inner.height_offset.set(offset);
        }
    }

    /// How far the content has scrolled (negative: into it), as `contentOffset`.
    pub fn content_offset(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.content_offset.get())
    }

    pub fn set_content_offset(&self, offset: f32) {
        if let Some(inner) = self.inner() {
            inner.content_offset.set(offset);
        }
    }

    /// 0 expanded to 1 collapsed, as `collapsedFraction`.
    pub fn collapsed_fraction(&self) -> f32 {
        let limit = self.height_offset_limit();
        if limit != 0.0 { self.height_offset() / limit } else { 0.0 }
    }

    /// How much content has scrolled under the bar, 0 to 1, as `overlappedFraction`: all of it, for
    /// a scrollable known not to be at its start though nothing's been seen to scroll.
    pub fn overlapped_fraction(&self) -> f32 {
        let at_start = self
            .inner()
            .and_then(|inner| inner.is_at_start.borrow().clone())
            .is_none_or(|is_at_start| is_at_start());
        let limit = self.height_offset_limit();
        if !at_start && self.content_offset() == 0.0 {
            1.0
        } else if limit != 0.0 {
            1.0 - (limit + self.content_offset().abs()).clamp(limit, 0.0) / limit
        } else {
            0.0
        }
    }

    fn set_is_at_start(&self, is_at_start: Option<Rc<dyn Fn() -> bool>>) {
        if let Some(inner) = self.inner() {
            inner.is_at_start.replace(is_at_start);
        }
    }
}

/// Settles a collapse left between its ends, as `settleAppBar`: on with what's left of the fling,
/// on `fling`, then to the nearer end, on `snap`. Gives back the velocity left, along the axis.
async fn settle_app_bar(
    state: TopAppBarState,
    velocity: f32,
    fling: Option<DecayAnimationSpec>,
    snap: Option<AnimationSpec>,
) -> Offset {
    let fraction = state.collapsed_fraction();
    if fraction < 0.01 || fraction == 1.0 {
        return Offset::ZERO;
    }
    let remaining = std::cell::Cell::new(velocity);
    if let Some(fling) = fling
        && velocity.abs() > 1.0
    {
        let last = std::cell::Cell::new(0.0f32);
        AnimationState::with_velocity(0.0f32, velocity)
            .animate_decay(&fling, false, |scope| {
                let delta = scope.value() - last.get();
                let before = state.height_offset();
                state.set_height_offset(before + delta);
                let consumed = (before - state.height_offset()).abs();
                last.set(scope.value());
                remaining.set(scope.velocity());
                // Rounding errors aside, what's not taken ends it.
                if (delta - consumed).abs() > 0.5 {
                    scope.cancel_animation();
                }
            })
            .await;
    }
    if let Some(snap) = snap {
        let (offset, limit) = (state.height_offset(), state.height_offset_limit());
        if offset < 0.0 && offset > limit {
            let target = if state.collapsed_fraction() < 0.5 { 0.0 } else { limit };
            AnimationState::new(offset)
                .animate_to(target, snap, false, |scope| state.set_height_offset(scope.value()))
                .await;
        }
    }
    Offset::new(0.0, remaining.get())
}

impl TopAppBarState {
    /// Saves its height offset limit, height offset and content offset, as Compose's
    /// `TopAppBarState.Saver`.
    pub fn saver() -> impl compose_runtime::Saver<Owned<TopAppBarState>> {
        use compose_runtime::Saveable;
        compose_runtime::saver(
            |state: &Owned<TopAppBarState>| {
                state.inner()?;
                Some(
                    (state.height_offset_limit(), state.height_offset(), state.content_offset())
                        .save(),
                )
            },
            |saved| {
                <(f32, f32, f32)>::restore(saved)
                    .map(|(limit, height, content)| TopAppBarState::new(limit, height, content))
            },
        )
    }
}

impl ComposeArg for TopAppBarState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A top app bar state remembered here, as `rememberTopAppBarState(initialHeightOffsetLimit,
/// initialHeightOffset, initialContentOffset)`: by default no limit until a bar sets it,
/// expanded, the content at its start.
/// Saved across the host's recreation (a `rememberSaveable`, as Compose's), and kept by a
/// `SaveableStateProvider`: a bar collapsed on a screen is collapsed when it shows again.
#[composable(non_restartable)]
pub fn remember_top_app_bar_state(
    #[default(-f32::MAX)] initial_height_offset_limit: f32,
    #[default(0.0)] initial_height_offset: f32,
    #[default(0.0)] initial_content_offset: f32,
) -> TopAppBarState {
    compose_runtime::remember_saveable_with(TopAppBarState::saver(), || {
        TopAppBarState::new(
            initial_height_offset_limit,
            initial_height_offset,
            initial_content_offset,
        )
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScrollKind {
    Pinned,
    EnterAlways,
    ExitUntilCollapsed,
    /// A bottom bar's.
    ExitAlways,
}

/// How a top app bar follows the content's scrolling, as Compose's `TopAppBarScrollBehavior`:
/// give its `nested_scroll_connection()` to `Modifier.nested_scroll` around the content (the
/// `Scaffold`), and the behavior to the bar.
#[derive(Clone)]
pub struct TopAppBarScrollBehavior {
    state: TopAppBarState,
    kind: ScrollKind,
    connection: Rc<dyn NestedScrollConnection>,
    snap: Option<AnimationSpec>,
    fling: Option<DecayAnimationSpec>,
    can_scroll: LatestCanScroll,
}

/// The latest `can_scroll` a behavior was given: a closure written inline is a new one each
/// composition, and shouldn't be a new behavior and connection.
type LatestCanScroll = Rc<std::cell::RefCell<Option<Callback<dyn Fn() -> bool>>>>;

impl TopAppBarScrollBehavior {
    /// The bar stays; its color follows the content scrolled under it, as
    /// `pinnedScrollBehavior(state)`.
    pub fn pinned(state: TopAppBarState) -> TopAppBarScrollBehavior {
        TopAppBarScrollBehavior::new(ScrollKind::Pinned, state)
    }

    /// The bar collapses as the content scrolls forward and comes back as soon as it scrolls
    /// back, as `enterAlwaysScrollBehavior(state)`.
    pub fn enter_always(state: TopAppBarState) -> TopAppBarScrollBehavior {
        TopAppBarScrollBehavior::new(ScrollKind::EnterAlways, state)
    }

    /// The bar collapses as the content scrolls forward, and expands once the content is back at
    /// its start, as `exitUntilCollapsedScrollBehavior(state)`.
    pub fn exit_until_collapsed(state: TopAppBarState) -> TopAppBarScrollBehavior {
        TopAppBarScrollBehavior::new(ScrollKind::ExitUntilCollapsed, state)
    }

    fn new(kind: ScrollKind, state: TopAppBarState) -> Self {
        // These aren't composables: the standard scheme's, whatever the theme's
        // (`with_snap_animation_spec` says otherwise; `TopAppBarDefaults`' read the theme).
        let snap = Some(MotionScheme::standard().default_effects_spec());
        TopAppBarScrollBehavior::with(kind, state, snap, None, Default::default())
    }

    fn with(
        kind: ScrollKind,
        state: TopAppBarState,
        snap: Option<AnimationSpec>,
        fling: Option<DecayAnimationSpec>,
        can_scroll: LatestCanScroll,
    ) -> Self {
        let (snap, fling) = if kind == ScrollKind::Pinned { (None, None) } else { (snap, fling) };
        let connection = Rc::new(ScrollConnection {
            kind,
            state,
            snap: snap.clone(),
            fling: fling.clone(),
            can_scroll: can_scroll.clone(),
        });
        TopAppBarScrollBehavior { state, kind, connection, snap, fling, can_scroll }
    }

    /// How the bar snaps to expanded or collapsed when a scroll leaves it between them, as the
    /// behaviors' `snapAnimationSpec` (from a raw constructor, the standard scheme's default
    /// effects whatever the theme's); `None` leaves it there. Each of these makes a new connection:
    /// keep it (`remember`), or use `TopAppBarDefaults`' `_with` forms, which do.
    pub fn with_snap_animation_spec(self, snap: Option<AnimationSpec>) -> TopAppBarScrollBehavior {
        let TopAppBarScrollBehavior { state, kind, fling, can_scroll, .. } = self;
        TopAppBarScrollBehavior::with(kind, state, snap, fling, can_scroll)
    }

    /// What's left of a fling carries the bar on, on this, as `flingAnimationSpec`: from a raw
    /// constructor, none.
    pub fn with_fling_animation_spec(
        self,
        fling: Option<DecayAnimationSpec>,
    ) -> TopAppBarScrollBehavior {
        let TopAppBarScrollBehavior { state, kind, snap, can_scroll, .. } = self;
        TopAppBarScrollBehavior::with(kind, state, snap, fling, can_scroll)
    }

    /// Whether scrolls are handled now, as `canScroll`: by default, always.
    pub fn with_can_scroll(
        self,
        can_scroll: impl Fn() -> bool + 'static,
    ) -> TopAppBarScrollBehavior {
        let TopAppBarScrollBehavior { state, kind, snap, fling, .. } = self;
        let can_scroll = Rc::new(std::cell::RefCell::new(Some(Callback::new(can_scroll))));
        TopAppBarScrollBehavior::with(kind, state, snap, fling, can_scroll)
    }

    /// Its state, as `state`.
    pub fn state(&self) -> TopAppBarState {
        self.state
    }

    /// Whether the bar never collapses, as `isPinned`.
    pub fn is_pinned(&self) -> bool {
        self.kind == ScrollKind::Pinned
    }

    /// What `Modifier.nested_scroll` takes, as `nestedScrollConnection`.
    pub fn nested_scroll_connection(&self) -> Rc<dyn NestedScrollConnection> {
        self.connection.clone()
    }

    /// As `snapAnimationSpec`.
    pub fn snap_animation_spec(&self) -> Option<AnimationSpec> {
        self.snap.clone()
    }

    /// As `flingAnimationSpec`.
    pub fn fling_animation_spec(&self) -> Option<DecayAnimationSpec> {
        self.fling.clone()
    }
}

impl ComposeArg for TopAppBarScrollBehavior {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.connection, &other.connection)
    }
}

/// The behaviors' nested scrolling, as Compose's.
struct ScrollConnection {
    kind: ScrollKind,
    state: TopAppBarState,
    snap: Option<AnimationSpec>,
    fling: Option<DecayAnimationSpec>,
    can_scroll: LatestCanScroll,
}

impl ScrollConnection {
    fn can_scroll(&self) -> bool {
        // Not borrowed while it runs: it's the app's, and may compose or scroll anything.
        let can_scroll = self.can_scroll.borrow().clone();
        can_scroll.is_none_or(|can_scroll| can_scroll())
    }
}

impl NestedScrollConnection for ScrollConnection {
    fn on_pre_scroll(&self, available: Offset, _source: NestedScrollSource) -> Offset {
        let state = &self.state;
        let collapses = match self.kind {
            ScrollKind::Pinned | ScrollKind::ExitAlways => false,
            ScrollKind::EnterAlways => true,
            // Not when scrolling back: the content goes first.
            ScrollKind::ExitUntilCollapsed => available.y <= 0.0,
        };
        if !collapses || !self.can_scroll() {
            return Offset::ZERO;
        }
        let before = state.height_offset();
        state.set_height_offset(before + available.y);
        // Moving the bar takes the whole vertical delta, as Compose's.
        if state.height_offset() != before { Offset::new(0.0, available.y) } else { Offset::ZERO }
    }

    fn on_post_scroll(
        &self,
        consumed: Offset,
        available: Offset,
        _source: NestedScrollSource,
    ) -> Offset {
        if !self.can_scroll() {
            return Offset::ZERO;
        }
        let state = &self.state;
        state.set_content_offset(state.content_offset() + consumed.y);
        match self.kind {
            ScrollKind::Pinned | ScrollKind::EnterAlways => {}
            ScrollKind::ExitAlways => state.set_height_offset(state.height_offset() + consumed.y),
            ScrollKind::ExitUntilCollapsed => {
                if available.y < 0.0 || consumed.y < 0.0 {
                    let before = state.height_offset();
                    state.set_height_offset(before + consumed.y);
                    return Offset::new(0.0, state.height_offset() - before);
                }
                if available.y > 0.0 {
                    // What the pre-scroll left and the content didn't take expands the bar.
                    let before = state.height_offset();
                    state.set_height_offset(before + available.y);
                    return Offset::new(0.0, state.height_offset() - before);
                }
            }
        }
        Offset::ZERO
    }

    fn on_post_fling(
        &self,
        _consumed: Offset,
        available: Offset,
    ) -> compose_ui::LocalBoxFuture<'_, Offset> {
        let state = self.state;
        // All the way back: no rounding errors left.
        let at_an_end = match self.kind {
            ScrollKind::ExitAlways => {
                state.height_offset() == 0.0 || state.height_offset() == state.height_offset_limit()
            }
            ScrollKind::Pinned | ScrollKind::EnterAlways | ScrollKind::ExitUntilCollapsed => true,
        };
        if available.y > 0.0 && at_an_end {
            state.set_content_offset(0.0);
        }
        if self.kind == ScrollKind::Pinned {
            return Box::pin(async { Offset::ZERO });
        }
        let (fling, snap) = (self.fling.clone(), self.snap.clone());
        Box::pin(async move { settle_app_bar(state, available.y, fling, snap).await })
    }
}

/// Dragging the bar itself moves it, and letting go settles it, as the bars' `draggable`.
fn bar_drag(scroll_behavior: &Option<TopAppBarScrollBehavior>, sign: f32) -> Modifier {
    let Some(behavior) = scroll_behavior.clone().filter(|behavior| !behavior.is_pinned()) else {
        return Modifier;
    };
    let state = behavior.state;
    let drag = remember_draggable_state!(move |delta: f32| {
        state.set_height_offset(state.height_offset() + sign * delta);
    });
    let (fling, snap) = (behavior.fling.clone(), behavior.snap.clone());
    let on_stopped: compose_foundation::gestures::OnDragStopped =
        Rc::new(move |scope: CoroutineScope, velocity: f32| {
            let (fling, snap) = (fling.clone(), snap.clone());
            scope.launch(async move {
                settle_app_bar(state, velocity, fling, snap).await;
            });
        });
    Modifier.draggable_with(
        drag,
        Orientation::Vertical,
        DraggableOptions { on_drag_stopped: Some(on_stopped), ..DraggableOptions::default() },
    )
}

// ---- Top app bars ----------------------------------------------------------------------------

/// A top app bar's colors, as Compose's `TopAppBarColors`: the container turns
/// `scrolled_container_color` as content scrolls under it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TopAppBarColors {
    pub container_color: Color,
    pub scrolled_container_color: Color,
    pub navigation_icon_content_color: Color,
    pub title_content_color: Color,
    pub action_icon_content_color: Color,
    pub subtitle_content_color: Color,
}

impl TopAppBarColors {
    /// The container's color, `fraction` of the way to the scrolled one (eased), as
    /// `containerColor(colorTransitionFraction)`.
    pub fn container_color(&self, fraction: f32) -> Color {
        Color::lerp(
            self.container_color,
            self.scrolled_container_color,
            FAST_OUT_LINEAR_IN_EASING.transform(fraction.clamp(0.0, 1.0)),
        )
    }
}

impl ComposeArg for TopAppBarColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `TopAppBarDefaults`.
pub struct TopAppBarDefaults;

impl TopAppBarDefaults {
    /// As `TopAppBarExpandedHeight`.
    pub const TOP_APP_BAR_EXPANDED_HEIGHT: Dp = Dp(64.0);
    /// As `MediumAppBarCollapsedHeight`.
    pub const MEDIUM_APP_BAR_COLLAPSED_HEIGHT: Dp = Dp(64.0);
    /// As `MediumAppBarExpandedHeight`.
    pub const MEDIUM_APP_BAR_EXPANDED_HEIGHT: Dp = Dp(112.0);
    /// As `LargeAppBarCollapsedHeight`.
    pub const LARGE_APP_BAR_COLLAPSED_HEIGHT: Dp = Dp(64.0);
    /// As `LargeAppBarExpandedHeight`.
    pub const LARGE_APP_BAR_EXPANDED_HEIGHT: Dp = Dp(152.0);
    /// As `MediumFlexibleAppBarWithoutSubtitleExpandedHeight`.
    pub const MEDIUM_FLEXIBLE_APP_BAR_WITHOUT_SUBTITLE_EXPANDED_HEIGHT: Dp = Dp(112.0);
    /// As `MediumFlexibleAppBarWithSubtitleExpandedHeight`.
    pub const MEDIUM_FLEXIBLE_APP_BAR_WITH_SUBTITLE_EXPANDED_HEIGHT: Dp = Dp(136.0);
    /// As `LargeFlexibleAppBarWithoutSubtitleExpandedHeight`.
    pub const LARGE_FLEXIBLE_APP_BAR_WITHOUT_SUBTITLE_EXPANDED_HEIGHT: Dp = Dp(120.0);
    /// As `LargeFlexibleAppBarWithSubtitleExpandedHeight`.
    pub const LARGE_FLEXIBLE_APP_BAR_WITH_SUBTITLE_EXPANDED_HEIGHT: Dp = Dp(152.0);
    /// As `ContentPadding`: none.
    pub const CONTENT_PADDING: PaddingValues = PaddingValues::ZERO;

    pub fn top_app_bar_colors() -> TopAppBarColors {
        let scheme = MaterialTheme::color_scheme();
        TopAppBarColors {
            container_color: scheme.surface,
            scrolled_container_color: scheme.surface_container,
            navigation_icon_content_color: scheme.on_surface,
            title_content_color: scheme.on_surface,
            action_icon_content_color: scheme.on_surface_variant,
            subtitle_content_color: scheme.on_surface_variant,
        }
    }

    /// As `centerAlignedTopAppBarColors()`.
    pub fn center_aligned_top_app_bar_colors() -> TopAppBarColors {
        TopAppBarDefaults::top_app_bar_colors()
    }

    /// As `mediumTopAppBarColors()`.
    pub fn medium_top_app_bar_colors() -> TopAppBarColors {
        TopAppBarDefaults::top_app_bar_colors()
    }

    /// As `largeTopAppBarColors()`.
    pub fn large_top_app_bar_colors() -> TopAppBarColors {
        TopAppBarDefaults::top_app_bar_colors()
    }

    /// The insets a bar keeps its content clear of, as `windowInsets`: the system bars and a
    /// cutout, at the top and the sides.
    pub fn window_insets() -> WindowInsets {
        crate::scaffold::system_bars_for_visual_components()
            .only(WindowInsetsSides::HORIZONTAL + WindowInsetsSides::TOP)
    }

    /// How a bar snaps when a scroll leaves it between expanded and collapsed, as
    /// `snapAnimationSpec`: the theme's default effects spec.
    pub fn snap_animation_spec() -> AnimationSpec {
        MaterialTheme::motion_scheme().default_effects_spec()
    }

    /// What's left of a fling carries a bar on, on this, as `flingAnimationSpec`.
    pub fn fling_animation_spec() -> DecayAnimationSpec {
        compose_animation::remember_spline_based_decay()
    }

    /// A pinned behavior with its state remembered here, as `pinnedScrollBehavior()`.
    pub fn pinned_scroll_behavior() -> TopAppBarScrollBehavior {
        remembered_behavior(ScrollKind::Pinned, None)
    }

    /// As `pinnedScrollBehavior(state, canScroll)`.
    pub fn pinned_scroll_behavior_with(
        can_scroll: Option<Callback<dyn Fn() -> bool>>,
    ) -> TopAppBarScrollBehavior {
        remembered_behavior(ScrollKind::Pinned, can_scroll)
    }

    /// As `pinnedScrollBehavior(scrollableState)`: the color goes by where `scrollable` is.
    pub fn pinned_scroll_behavior_for(
        scrollable: &(impl ScrollableState + ComposeArg + Clone + 'static),
    ) -> TopAppBarScrollBehavior {
        TopAppBarDefaults::pinned_scroll_behavior_for_with(scrollable, None)
    }

    /// As `pinnedScrollBehavior(scrollableState, state, canScroll)`.
    pub fn pinned_scroll_behavior_for_with(
        scrollable: &(impl ScrollableState + ComposeArg + Clone + 'static),
        can_scroll: Option<Callback<dyn Fn() -> bool>>,
    ) -> TopAppBarScrollBehavior {
        behavior_reading(remembered_behavior(ScrollKind::Pinned, can_scroll), scrollable)
    }

    /// As `enterAlwaysScrollBehavior()`.
    pub fn enter_always_scroll_behavior() -> TopAppBarScrollBehavior {
        remembered_behavior(ScrollKind::EnterAlways, None)
    }

    /// As `enterAlwaysScrollBehavior(scrollableState)`: the color goes by where `scrollable` is, a
    /// reversed one too.
    pub fn enter_always_scroll_behavior_for(
        scrollable: &(impl ScrollableState + ComposeArg + Clone + 'static),
    ) -> TopAppBarScrollBehavior {
        behavior_reading(remembered_behavior(ScrollKind::EnterAlways, None), scrollable)
    }

    /// As `enterAlwaysScrollBehavior(scrollableState, state, canScroll, snapAnimationSpec,
    /// flingAnimationSpec)`: `None`s leave the bar where a scroll does.
    pub fn enter_always_scroll_behavior_for_with(
        scrollable: &(impl ScrollableState + ComposeArg + Clone + 'static),
        can_scroll: Option<Callback<dyn Fn() -> bool>>,
        snap: Option<AnimationSpec>,
        fling: Option<DecayAnimationSpec>,
    ) -> TopAppBarScrollBehavior {
        behavior_reading(
            remembered_behavior_with(ScrollKind::EnterAlways, can_scroll, snap, fling),
            scrollable,
        )
    }

    /// As `enterAlwaysScrollBehavior(canScroll, snapAnimationSpec, flingAnimationSpec)`: `None`s
    /// leave the bar where a scroll does.
    pub fn enter_always_scroll_behavior_with(
        can_scroll: Option<Callback<dyn Fn() -> bool>>,
        snap: Option<AnimationSpec>,
        fling: Option<DecayAnimationSpec>,
    ) -> TopAppBarScrollBehavior {
        remembered_behavior_with(ScrollKind::EnterAlways, can_scroll, snap, fling)
    }

    /// As `exitUntilCollapsedScrollBehavior()`.
    pub fn exit_until_collapsed_scroll_behavior() -> TopAppBarScrollBehavior {
        remembered_behavior(ScrollKind::ExitUntilCollapsed, None)
    }

    /// As `exitUntilCollapsedScrollBehavior(canScroll, snapAnimationSpec, flingAnimationSpec)`:
    /// `None`s leave the bar where a scroll does.
    pub fn exit_until_collapsed_scroll_behavior_with(
        can_scroll: Option<Callback<dyn Fn() -> bool>>,
        snap: Option<AnimationSpec>,
        fling: Option<DecayAnimationSpec>,
    ) -> TopAppBarScrollBehavior {
        remembered_behavior_with(ScrollKind::ExitUntilCollapsed, can_scroll, snap, fling)
    }
}

fn remembered_behavior(
    kind: ScrollKind,
    can_scroll: Option<Callback<dyn Fn() -> bool>>,
) -> TopAppBarScrollBehavior {
    remembered_behavior_with(
        kind,
        can_scroll,
        Some(TopAppBarDefaults::snap_animation_spec()),
        Some(TopAppBarDefaults::fling_animation_spec()),
    )
}

/// `behavior`, its color going by where `scrollable` is, as the overloads taking one.
fn behavior_reading(
    behavior: TopAppBarScrollBehavior,
    scrollable: &(impl ScrollableState + ComposeArg + Clone + 'static),
) -> TopAppBarScrollBehavior {
    let (state, scrollable) = (behavior.state, scrollable.clone());
    compose_runtime::remember_keyed(scrollable.clone(), move || {
        state.set_is_at_start(scrollable.scroll_indicator_state().map(|indicator| {
            let is_at_start: Rc<dyn Fn() -> bool> = Rc::new(move || indicator.scroll_offset() == 0);
            is_at_start
        }));
    });
    behavior
}

/// Kept until its specs change, as Compose's `remember(state, canScroll, snapAnimationSpec, ...)`,
/// but for `can_scroll`, of which it keeps the latest: Compose's lambdas stay the same object over
/// recompositions by what they capture, and a closure written inline here doesn't.
fn remembered_behavior_with(
    kind: ScrollKind,
    can_scroll: Option<Callback<dyn Fn() -> bool>>,
    snap: Option<AnimationSpec>,
    fling: Option<DecayAnimationSpec>,
) -> TopAppBarScrollBehavior {
    let state = remember_top_app_bar_state!();
    let latest: LatestCanScroll = compose_runtime::remember(LatestCanScroll::default);
    latest.replace(can_scroll);
    compose_runtime::remember_keyed((snap.clone(), fling.clone()), move || {
        TopAppBarScrollBehavior::with(kind, state, snap, fling, latest)
    })
}

/// The padding around the navigation icon and the actions, as `TopAppBarHorizontalPadding`.
const HORIZONTAL_PADDING: Dp = Dp(4.0);
/// The title's start without a navigation icon (with its own padding, 16 dp in), as
/// `TopAppBarTitleInset`.
const TITLE_INSET: Dp = Dp(12.0);

/// Unspecified or infinite, the default, as Compose's bars.
fn height_or(height: Dp, default: Dp) -> Dp {
    if height.is_specified() && height.0.is_finite() { height } else { default }
}

/// A small top app bar, as Compose's `TopAppBar`: `expanded_height` (64 dp) high (less as its
/// `scroll_behavior` collapses it), the navigation icon at the start, the title after it in
/// the title-large style, with `subtitle` under it in label medium, aligned by
/// `title_horizontal_alignment`, the actions at the end; the container turns the scrolled color as
/// content scrolls under it.
#[composable]
pub fn TopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::TopAppBarDefaults::TOP_APP_BAR_EXPANDED_HEIGHT)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(crate::TopAppBarDefaults::top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
    #[default(crate::TopAppBarDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] subtitle: Option<impl Fn() + 'static>,
    #[default(crate::Alignment::START)] title_horizontal_alignment: HorizontalAlignment,
) {
    let typography = MaterialTheme::typography();
    single_row_top_app_bar(SingleRow {
        title,
        title_style: typography.title_large.clone(),
        subtitle,
        subtitle_style: typography.label_medium.clone(),
        title_horizontal_alignment,
        modifier,
        navigation_icon,
        actions,
        expanded_height: height_or(expanded_height, TopAppBarDefaults::TOP_APP_BAR_EXPANDED_HEIGHT),
        content_padding,
        window_insets,
        colors,
        scroll_behavior,
    });
}

/// A top app bar with its title centered, as Compose's `CenterAlignedTopAppBar`.
#[composable]
pub fn CenterAlignedTopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::TopAppBarDefaults::TOP_APP_BAR_EXPANDED_HEIGHT)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(crate::TopAppBarDefaults::center_aligned_top_app_bar_colors())]
    colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
    #[default(crate::TopAppBarDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
) {
    single_row_top_app_bar(SingleRow {
        title,
        title_style: MaterialTheme::typography().title_large.clone(),
        subtitle: None,
        subtitle_style: TextStyle::DEFAULT,
        title_horizontal_alignment: Alignment::CENTER_HORIZONTALLY,
        modifier,
        navigation_icon,
        actions,
        expanded_height: height_or(expanded_height, TopAppBarDefaults::TOP_APP_BAR_EXPANDED_HEIGHT),
        content_padding,
        window_insets,
        colors,
        scroll_behavior,
    });
}

/// A medium top app bar, as Compose's `MediumTopAppBar`: a row of the navigation icon, the
/// small title (title-large, shown once collapsed) and the actions, over the title in the
/// headline-small style; 112 dp high, collapsing to 64 dp.
#[composable]
pub fn MediumTopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::TopAppBarDefaults::MEDIUM_APP_BAR_COLLAPSED_HEIGHT)] collapsed_height: Dp,
    #[default(crate::TopAppBarDefaults::MEDIUM_APP_BAR_EXPANDED_HEIGHT)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(crate::TopAppBarDefaults::medium_top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    let typography = MaterialTheme::typography();
    two_rows_top_app_bar(TwoRows {
        title: title.clone(),
        title_style: typography.headline_small.clone(),
        title_bottom_padding: MEDIUM_TITLE_BOTTOM_PADDING,
        small_title: title,
        small_title_style: typography.title_large.clone(),
        subtitle: None,
        subtitle_style: TextStyle::DEFAULT,
        small_subtitle: None,
        small_subtitle_style: TextStyle::DEFAULT,
        title_horizontal_alignment: Alignment::START,
        modifier,
        navigation_icon,
        actions,
        collapsed_height: height_or(
            collapsed_height,
            TopAppBarDefaults::MEDIUM_APP_BAR_COLLAPSED_HEIGHT,
        ),
        expanded_height: height_or(
            expanded_height,
            TopAppBarDefaults::MEDIUM_APP_BAR_EXPANDED_HEIGHT,
        ),
        window_insets,
        colors,
        scroll_behavior,
    });
}

/// A large top app bar, as Compose's `LargeTopAppBar`: as the medium one, its title in the
/// headline-medium style; 152 dp high, collapsing to 64 dp.
#[composable]
pub fn LargeTopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::TopAppBarDefaults::LARGE_APP_BAR_COLLAPSED_HEIGHT)] collapsed_height: Dp,
    #[default(crate::TopAppBarDefaults::LARGE_APP_BAR_EXPANDED_HEIGHT)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(crate::TopAppBarDefaults::large_top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    let typography = MaterialTheme::typography();
    two_rows_top_app_bar(TwoRows {
        title: title.clone(),
        title_style: typography.headline_medium.clone(),
        title_bottom_padding: LARGE_TITLE_BOTTOM_PADDING,
        small_title: title,
        small_title_style: typography.title_large.clone(),
        subtitle: None,
        subtitle_style: TextStyle::DEFAULT,
        small_subtitle: None,
        small_subtitle_style: TextStyle::DEFAULT,
        title_horizontal_alignment: Alignment::START,
        modifier,
        navigation_icon,
        actions,
        collapsed_height: height_or(
            collapsed_height,
            TopAppBarDefaults::LARGE_APP_BAR_COLLAPSED_HEIGHT,
        ),
        expanded_height: height_or(
            expanded_height,
            TopAppBarDefaults::LARGE_APP_BAR_EXPANDED_HEIGHT,
        ),
        window_insets,
        colors,
        scroll_behavior,
    });
}

/// The medium flexible bar, as Compose's `MediumFlexibleTopAppBar`: its title in headline medium,
/// `subtitle` under it in label large; 112 dp high, or 136 dp with a subtitle, collapsing to 64 dp.
/// By default `expanded_height` goes by the subtitle.
#[composable]
pub fn MediumFlexibleTopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] subtitle: Option<impl Fn() + 'static>,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::Alignment::START)] title_horizontal_alignment: HorizontalAlignment,
    #[default(crate::TopAppBarDefaults::MEDIUM_APP_BAR_COLLAPSED_HEIGHT)] collapsed_height: Dp,
    #[default(crate::Dp::UNSPECIFIED)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(crate::TopAppBarDefaults::top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    let typography = MaterialTheme::typography();
    let default_expanded = if subtitle.is_some() {
        TopAppBarDefaults::MEDIUM_FLEXIBLE_APP_BAR_WITH_SUBTITLE_EXPANDED_HEIGHT
    } else {
        TopAppBarDefaults::MEDIUM_FLEXIBLE_APP_BAR_WITHOUT_SUBTITLE_EXPANDED_HEIGHT
    };
    let subtitle = subtitle.unwrap_or_else(|| compose_runtime::callback(|| {}));
    two_rows_top_app_bar(TwoRows {
        title: title.clone(),
        title_style: typography.headline_medium.clone(),
        title_bottom_padding: MEDIUM_TITLE_BOTTOM_PADDING,
        small_title: title,
        small_title_style: typography.title_large.clone(),
        subtitle: Some(subtitle.clone()),
        subtitle_style: typography.label_large.clone(),
        small_subtitle: Some(subtitle),
        small_subtitle_style: typography.label_medium.clone(),
        title_horizontal_alignment,
        modifier,
        navigation_icon,
        actions,
        collapsed_height: height_or(
            collapsed_height,
            TopAppBarDefaults::MEDIUM_APP_BAR_COLLAPSED_HEIGHT,
        ),
        expanded_height: height_or(expanded_height, default_expanded),
        window_insets,
        colors,
        scroll_behavior,
    });
}

/// The large flexible bar, as Compose's `LargeFlexibleTopAppBar`: its title in display small,
/// `subtitle` under it in title medium; 120 dp high, or 152 dp with a subtitle, collapsing to 64 dp.
/// By default `expanded_height` goes by the subtitle.
#[composable]
pub fn LargeFlexibleTopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] subtitle: Option<impl Fn() + 'static>,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::Alignment::START)] title_horizontal_alignment: HorizontalAlignment,
    #[default(crate::TopAppBarDefaults::LARGE_APP_BAR_COLLAPSED_HEIGHT)] collapsed_height: Dp,
    #[default(crate::Dp::UNSPECIFIED)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(crate::TopAppBarDefaults::top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    let typography = MaterialTheme::typography();
    let default_expanded = if subtitle.is_some() {
        TopAppBarDefaults::LARGE_FLEXIBLE_APP_BAR_WITH_SUBTITLE_EXPANDED_HEIGHT
    } else {
        TopAppBarDefaults::LARGE_FLEXIBLE_APP_BAR_WITHOUT_SUBTITLE_EXPANDED_HEIGHT
    };
    let subtitle = subtitle.unwrap_or_else(|| compose_runtime::callback(|| {}));
    two_rows_top_app_bar(TwoRows {
        title: title.clone(),
        title_style: typography.display_small.clone(),
        title_bottom_padding: LARGE_TITLE_BOTTOM_PADDING,
        small_title: title,
        small_title_style: typography.title_large.clone(),
        subtitle: Some(subtitle.clone()),
        subtitle_style: typography.title_medium.clone(),
        small_subtitle: Some(subtitle),
        small_subtitle_style: typography.label_medium.clone(),
        title_horizontal_alignment,
        modifier,
        navigation_icon,
        actions,
        collapsed_height: height_or(
            collapsed_height,
            TopAppBarDefaults::LARGE_APP_BAR_COLLAPSED_HEIGHT,
        ),
        expanded_height: height_or(expanded_height, default_expanded),
        window_insets,
        colors,
        scroll_behavior,
    });
}

/// A bar of two rows of your own, as Compose's public `TwoRowsTopAppBar`: `title` and `subtitle`
/// are told whether they're the expanded row's (`true`) or the collapsed row's. By default it's as
/// high as the medium flexible bar.
#[composable]
pub fn TwoRowsTopAppBar(
    title: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] subtitle: Option<impl Fn(bool) + 'static>,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::Alignment::START)] title_horizontal_alignment: HorizontalAlignment,
    #[default(crate::Dp::UNSPECIFIED)] collapsed_height: Dp,
    #[default(crate::Dp::UNSPECIFIED)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(crate::TopAppBarDefaults::top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    let typography = MaterialTheme::typography();
    let default_expanded = if subtitle.is_some() {
        TopAppBarDefaults::MEDIUM_FLEXIBLE_APP_BAR_WITH_SUBTITLE_EXPANDED_HEIGHT
    } else {
        TopAppBarDefaults::MEDIUM_FLEXIBLE_APP_BAR_WITHOUT_SUBTITLE_EXPANDED_HEIGHT
    };
    let told = |slot: Option<Callback<dyn Fn(bool)>>, expanded: bool| {
        compose_runtime::callback(move || {
            if let Some(slot) = &slot {
                slot(expanded);
            }
        })
    };
    let (big_title, small_title) = {
        let (first, second) = (title.clone(), title);
        (
            compose_runtime::callback(move || first(true)),
            compose_runtime::callback(move || second(false)),
        )
    };
    two_rows_top_app_bar(TwoRows {
        title: big_title,
        title_style: typography.headline_medium.clone(),
        title_bottom_padding: Dp(0.0),
        small_title,
        small_title_style: typography.title_large.clone(),
        subtitle: Some(told(subtitle.clone(), true)),
        subtitle_style: typography.label_large.clone(),
        small_subtitle: Some(told(subtitle, false)),
        small_subtitle_style: typography.label_medium.clone(),
        title_horizontal_alignment,
        modifier,
        navigation_icon,
        actions,
        collapsed_height: height_or(
            collapsed_height,
            TopAppBarDefaults::MEDIUM_APP_BAR_COLLAPSED_HEIGHT,
        ),
        expanded_height: height_or(expanded_height, default_expanded),
        window_insets,
        colors,
        scroll_behavior,
    });
}

/// As `MediumTitleBottomPadding` and `LargeTitleBottomPadding`.
const MEDIUM_TITLE_BOTTOM_PADDING: Dp = Dp(24.0);
const LARGE_TITLE_BOTTOM_PADDING: Dp = Dp(28.0);

/// As `adjustHeightOffsetLimit`: what collapses is what it's measured at.
fn adjust_height_offset_limit(state: Option<TopAppBarState>) -> Modifier {
    let Some(state) = state else { return Modifier };
    compose_runtime::remember_keyed(state, move || {
        Modifier.on_size_changed(move |size: compose_ui::IntSize| {
            state.set_height_offset_limit(state.height_offset() - size.height as f32);
        })
    })
}

struct SingleRow {
    title: Callback,
    title_style: TextStyle,
    subtitle: Option<Callback>,
    subtitle_style: TextStyle,
    title_horizontal_alignment: HorizontalAlignment,
    modifier: Modifier,
    navigation_icon: Option<Callback>,
    actions: Callback<dyn Fn(RowScope)>,
    expanded_height: Dp,
    content_padding: PaddingValues,
    window_insets: WindowInsets,
    colors: TopAppBarColors,
    scroll_behavior: Option<TopAppBarScrollBehavior>,
}

/// As Compose's `SingleRowTopAppBar`.
fn single_row_top_app_bar(bar: SingleRow) {
    let SingleRow {
        title,
        title_style,
        subtitle,
        subtitle_style,
        title_horizontal_alignment,
        modifier,
        navigation_icon,
        actions,
        expanded_height,
        content_padding,
        window_insets,
        colors,
        scroll_behavior,
    } = bar;
    let state = scroll_behavior.as_ref().map(TopAppBarScrollBehavior::state);
    let drag = bar_drag(&scroll_behavior, 1.0);
    // The container turns the scrolled color once content is under the bar, animated, as
    // Compose's (the threshold read in a derived state: crossing it composes again only).
    let target_color = compose_runtime::remember_keyed((colors, state), move || {
        Rc::new(derived_state_of(move || {
            let overlapped = state.as_ref().map_or(0.0, TopAppBarState::overlapped_fraction);
            colors.container_color(if overlapped > 0.01 { 1.0 } else { 0.0 })
        }))
    });
    let container_color = animate_color_as_state!(
        target_color.get(),
        MaterialTheme::motion_scheme().default_effects_spec()
    );
    let policy = compose_runtime::remember_keyed(
        (state, expanded_height, title_horizontal_alignment.bias.to_bits(), content_padding),
        move || {
            Rc::new(TopAppBarLayout {
                height: expanded_height,
                state,
                title_horizontal_alignment,
                title_at_bottom: false,
                title_bottom_padding: Dp(0.0),
                content_padding,
            }) as Rc<dyn MeasurePolicy>
        },
    );
    let actions = actions_row(actions);
    let adjust = adjust_height_offset_limit(state);
    // Read while drawing: the transition draws again only.
    let container = Modifier.draw_behind(move |scope| {
        let color = container_color.get();
        if color != Color::UNSPECIFIED {
            scope.draw_rect(scope.bounds(), compose_ui::Paint::fill(color));
        }
    });
    // The row clips with a layer, as Compose's: a frame drawing what's around it again keeps the
    // bar's drawing (D251, D522). Pointers stop here, as `pointerInput(Unit) {}`.
    Box(
        modifier
            .then(drag)
            .then(container)
            .semantics(|config| config.set_is_traversal_group(true))
            .then_custom(crate::components::BlockPointer),
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| {
            top_app_bar_row(
                Modifier
                    .window_insets_padding(window_insets.clone())
                    // After the padding: no title over the inset area.
                    .clip_to_bounds()
                    .then(adjust.clone()),
                policy.clone(),
                TitleSlot {
                    title: title.clone(),
                    style: title_style.clone(),
                    subtitle: subtitle.clone(),
                    subtitle_style: subtitle_style.clone(),
                    alignment: title_horizontal_alignment,
                    // As `titleAlpha = { 1f }`: Compose's title has its layer here too.
                    alpha: Some(Modifier.graphics_layer(|layer| layer.alpha = 1.0)),
                    hide_semantics: false,
                },
                navigation_icon.clone(),
                Some(actions.clone()),
                colors,
            );
        }),
    );
}

struct TwoRows {
    title: Callback,
    title_style: TextStyle,
    title_bottom_padding: Dp,
    small_title: Callback,
    small_title_style: TextStyle,
    subtitle: Option<Callback>,
    subtitle_style: TextStyle,
    small_subtitle: Option<Callback>,
    small_subtitle_style: TextStyle,
    title_horizontal_alignment: HorizontalAlignment,
    modifier: Modifier,
    navigation_icon: Option<Callback>,
    actions: Callback<dyn Fn(RowScope)>,
    collapsed_height: Dp,
    expanded_height: Dp,
    window_insets: WindowInsets,
    colors: TopAppBarColors,
    scroll_behavior: Option<TopAppBarScrollBehavior>,
}

/// The medium and large bars, as Compose's `TwoRowsTopAppBar`: a pinned row with the small
/// title fading in as the bar collapses, over a row with the title fading out, shrinking.
fn two_rows_top_app_bar(bar: TwoRows) {
    let TwoRows {
        title,
        title_style,
        title_bottom_padding,
        small_title,
        small_title_style,
        subtitle,
        subtitle_style,
        small_subtitle,
        small_subtitle_style,
        title_horizontal_alignment,
        modifier,
        navigation_icon,
        actions,
        collapsed_height,
        expanded_height,
        window_insets,
        colors,
        scroll_behavior,
    } = bar;
    assert!(
        expanded_height.0 >= collapsed_height.0,
        "The expandedHeight is expected to be greater or equal to the collapsedHeight"
    );
    let state = scroll_behavior.as_ref().map(TopAppBarScrollBehavior::state);
    let drag = bar_drag(&scroll_behavior, 1.0);
    let collapsed = { move || state.as_ref().map_or(0.0, TopAppBarState::collapsed_fraction) };
    // Which row's title screen readers see: crossing half way composes again only.
    let top_shown = {
        compose_runtime::remember_keyed(state, move || {
            Rc::new(derived_state_of(move || collapsed() >= 0.5))
        })
    };
    let top_shown = top_shown.get();
    let policies = compose_runtime::remember_keyed(
        (
            state,
            collapsed_height,
            expanded_height,
            title_bottom_padding,
            title_horizontal_alignment.bias.to_bits(),
        ),
        move || {
            let top: Rc<dyn MeasurePolicy> = Rc::new(TopAppBarLayout {
                height: collapsed_height,
                state: None,
                title_horizontal_alignment,
                title_at_bottom: false,
                title_bottom_padding: Dp(0.0),
                content_padding: TopAppBarDefaults::CONTENT_PADDING,
            });
            let bottom: Rc<dyn MeasurePolicy> = Rc::new(TopAppBarLayout {
                height: Dp(expanded_height.0 - collapsed_height.0),
                state,
                title_horizontal_alignment,
                title_at_bottom: true,
                title_bottom_padding,
                content_padding: TopAppBarDefaults::CONTENT_PADDING,
            });
            (top, bottom)
        },
    );
    let adjust = adjust_height_offset_limit(state);
    // The titles' alphas and the container's color, read while drawing.
    let (top_alpha, bottom_alpha, container) = {
        let (first, second, third) = (collapsed, collapsed, collapsed);
        compose_runtime::remember_keyed((state, colors), move || {
            (
                Modifier.graphics_layer(move |layer| {
                    layer.alpha = TOP_TITLE_ALPHA_EASING.transform(first());
                }),
                Modifier.graphics_layer(move |layer| layer.alpha = 1.0 - second()),
                Modifier.draw_behind(move |scope| {
                    let color = colors.container_color(third());
                    scope.draw_rect(scope.bounds(), compose_ui::Paint::fill(color));
                }),
            )
        })
    };
    let actions = actions_row(actions);
    // Its rows clip with layers, as the small bar's (D251, D522).
    Box(
        modifier
            .then(drag)
            .then(container)
            .semantics(|config| config.set_is_traversal_group(true))
            .then_custom(crate::components::BlockPointer),
        Alignment::TOP_START,
        false,
        compose_runtime::callback(move |_| {
            let (title, small_title, navigation_icon, actions) =
                (title.clone(), small_title.clone(), navigation_icon.clone(), actions.clone());
            let (subtitle, small_subtitle) = (subtitle.clone(), small_subtitle.clone());
            let (policies, top_alpha, bottom_alpha) =
                (policies.clone(), top_alpha.clone(), bottom_alpha.clone());
            let (title_style, small_title_style) = (title_style.clone(), small_title_style.clone());
            let (subtitle_style, small_subtitle_style) =
                (subtitle_style.clone(), small_subtitle_style.clone());
            let window_insets = window_insets.clone();
            let adjust = adjust.clone();
            Column(
                Modifier,
                Arrangement::TOP,
                Alignment::START,
                compose_runtime::callback(move |_| {
                    top_app_bar_row(
                        Modifier.window_insets_padding(window_insets.clone()).clip_to_bounds(),
                        policies.0.clone(),
                        TitleSlot {
                            title: small_title.clone(),
                            style: small_title_style.clone(),
                            subtitle: small_subtitle.clone(),
                            subtitle_style: small_subtitle_style.clone(),
                            alignment: title_horizontal_alignment,
                            alpha: Some(top_alpha.clone()),
                            hide_semantics: !top_shown,
                        },
                        navigation_icon.clone(),
                        Some(actions.clone()),
                        colors,
                    );
                    top_app_bar_row(
                        Modifier
                            .window_insets_padding(
                                window_insets.clone().only(WindowInsetsSides::HORIZONTAL),
                            )
                            .clip_to_bounds()
                            .then(adjust.clone()),
                        policies.1.clone(),
                        TitleSlot {
                            title: title.clone(),
                            style: title_style.clone(),
                            subtitle: subtitle.clone(),
                            subtitle_style: subtitle_style.clone(),
                            alignment: title_horizontal_alignment,
                            alpha: Some(bottom_alpha.clone()),
                            hide_semantics: top_shown,
                        },
                        None,
                        None,
                        colors,
                    );
                }),
            );
        }),
    );
}

/// The small title's fade, as Compose's `TopTitleAlphaEasing`.
const TOP_TITLE_ALPHA_EASING: Easing = Easing::cubic_bezier(0.8, 0.0, 0.8, 0.15);

/// A row's title, as `TopAppBarLayout` takes it.
#[derive(Clone)]
struct TitleSlot {
    title: Callback,
    style: TextStyle,
    subtitle: Option<Callback>,
    subtitle_style: TextStyle,
    alignment: HorizontalAlignment,
    alpha: Option<Modifier>,
    hide_semantics: bool,
}

/// A row of a top app bar, as Compose's `TopAppBarLayout`: the navigation icon, the title and
/// the actions, laid out by `policy`.
fn top_app_bar_row(
    modifier: Modifier,
    policy: Rc<dyn MeasurePolicy>,
    title: TitleSlot,
    navigation_icon: Option<Callback>,
    actions: Option<Callback>,
    colors: TopAppBarColors,
) {
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            bar_slot(
                Modifier.padding((HORIZONTAL_PADDING, Dp(0.0), Dp(0.0), Dp(0.0))),
                colors.navigation_icon_content_color,
                None,
                navigation_icon.clone(),
            );
            let mut title_modifier = Modifier.padding((HORIZONTAL_PADDING, 0.dp()));
            if title.hide_semantics {
                title_modifier = title_modifier.clear_and_set_semantics(|_| {});
            } else if title.subtitle.is_some() {
                // Without a subtitle, a box of nothing more, as Compose's.
                title_modifier =
                    title_modifier.semantics(|config| config.set_is_traversal_group(true));
            }
            if let Some(alpha) = &title.alpha {
                title_modifier = title_modifier.then(alpha.clone());
            }
            match title.subtitle.clone() {
                None => bar_slot(
                    title_modifier,
                    colors.title_content_color,
                    Some(title.style.clone()),
                    Some(title.title.clone()),
                ),
                Some(subtitle) => {
                    let (first, first_style, second_style) =
                        (title.title.clone(), title.style.clone(), title.subtitle_style.clone());
                    Column(
                        title_modifier,
                        Arrangement::TOP,
                        title.alignment,
                        compose_runtime::callback(move |_| {
                            provide_content_color_text_style(
                                colors.title_content_color,
                                first_style.clone(),
                                &*first,
                            );
                            provide_content_color_text_style(
                                colors.subtitle_content_color,
                                second_style.clone(),
                                &*subtitle,
                            );
                        }),
                    );
                }
            }
            bar_slot(
                Modifier.padding((Dp(0.0), Dp(0.0), HORIZONTAL_PADDING, Dp(0.0))),
                colors.action_icon_content_color,
                None,
                actions.clone(),
            );
        }),
    )
}

/// A slot of a bar's row: `content` in a box of `modifier`, in `color` (and `style`).
fn bar_slot(modifier: Modifier, color: Color, style: Option<TextStyle>, content: Option<Callback>) {
    let body = move || {
        let content = content.clone();
        Box(
            modifier.clone(),
            Alignment::TOP_START,
            false,
            compose_runtime::callback(move |_| {
                if let Some(content) = &content {
                    content();
                }
            }),
        );
    };
    match style {
        Some(style) => provide_content_color_text_style(color, style, body),
        None => CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), body),
    }
}

/// The navigation icon, the title and the actions of a bar's row, as Compose's
/// `TopAppBarMeasurePolicy`: `height`, or more for the title, plus the paddings and the state's
/// (negative) height offset, as wide as allowed; the icons centered down it at either end; the
/// title aligned over the whole width but clear of them, centered down it, or at its bottom, its
/// last baseline (a subtitle's where there's one) `title_bottom_padding` up. The line's read only
/// where it says something: a bar's title centered asks nothing of it.
struct TopAppBarLayout {
    height: Dp,
    state: Option<TopAppBarState>,
    title_horizontal_alignment: HorizontalAlignment,
    title_at_bottom: bool,
    title_bottom_padding: Dp,
    content_padding: PaddingValues,
}

impl MeasurePolicy for TopAppBarLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [navigation, title, actions] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let density = scope.density();
        let mut loose = constraints;
        loose.min_width = 0;
        let navigation = scope.measure(*navigation, loose);
        let actions = scope.measure(*actions, loose);
        let padding = self.content_padding;
        let start_padding = density.round_to_px(padding.start);
        let end_padding = density.round_to_px(padding.end);
        let title_start = density.round_to_px(TITLE_INSET).max(navigation.width);
        let width = constraints.max_width;
        let max_title_width = if width == Constraints::INFINITY {
            width
        } else {
            (width - title_start - actions.width - start_padding - end_padding).max(0)
        };
        let mut title_constraints = loose;
        title_constraints.max_width = max_title_width;
        let title = scope.measure(*title, title_constraints);
        // Read while measuring: collapsing measures the bar again only.
        let offset = self.state.as_ref().map_or(0.0, TopAppBarState::height_offset);
        let height_offset =
            if offset.is_nan() { 0 } else { compose_ui::platform::round_to_int(offset) };
        let top_padding = density.round_to_px(padding.top);
        let bottom_padding = density.round_to_px(padding.bottom);
        let max_height =
            density.round_to_px(self.height).max(title.height) + top_padding + bottom_padding;
        let height = if constraints.max_height == Constraints::INFINITY {
            max_height
        } else {
            (max_height + height_offset).max(0)
        };
        let content_height = height + top_padding - bottom_padding;
        scope.place_relative(&navigation, start_padding, (content_height - navigation.height) / 2);
        // Over the whole width, so a center is the bar's, not what's between the icons; then clear
        // of them. Left to right: placed relative.
        let mut x = self.title_horizontal_alignment.align(title.width, width, LayoutDirection::Ltr);
        if x < title_start {
            x += start_padding + (title_start - x);
        } else if x + title.width > width - actions.width {
            x += start_padding + ((width - actions.width) - (x + title.width));
        }
        let y = if !self.title_at_bottom {
            (content_height - title.height) / 2
        } else if self.title_bottom_padding.0 == 0.0 {
            content_height - title.height
        } else {
            let baseline = match scope.alignment_line(&title, compose_ui::LAST_BASELINE) {
                compose_ui::AlignmentLine::UNSPECIFIED => 0,
                baseline => baseline,
            };
            let from_bottom =
                density.round_to_px(self.title_bottom_padding) - (title.height - baseline);
            // Less when there's no room for it.
            let with_padding = from_bottom + title.height;
            let adjusted = if with_padding > max_height {
                from_bottom - (with_padding - max_height)
            } else {
                from_bottom
            };
            content_height - title.height - adjusted.max(0)
        };
        scope.place_relative(&title, x, y);
        scope.place_relative(
            &actions,
            width - actions.width - end_padding,
            (content_height - actions.height) / 2,
        );
        scope.layout(width, height)
    }

    // As Compose's `TopAppBarLayout`: its slots side by side, and at least its height.
    fn min_intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        measurables.iter().map(|&slot| scope.min_intrinsic_width(slot, height)).sum()
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        let own = scope.density().round_to_px(self.height);
        measurables.iter().map(|&slot| scope.min_intrinsic_height(slot, width)).fold(own, i32::max)
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        measurables.iter().map(|&slot| scope.max_intrinsic_width(slot, height)).sum()
    }

    fn max_intrinsic_height(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        let own = scope.density().round_to_px(self.height);
        measurables.iter().map(|&slot| scope.max_intrinsic_height(slot, width)).fold(own, i32::max)
    }

    fn debug_name(&self) -> &'static str {
        "TopAppBarLayout"
    }
}

// ---- Bottom app bar --------------------------------------------------------------------------

/// How far a bottom app bar has collapsed, as Compose's `BottomAppBarState`: what a
/// [`TopAppBarState`] holds, the same handle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BottomAppBarState(TopAppBarState);

impl BottomAppBarState {
    /// As `BottomAppBarState(initialHeightOffsetLimit, initialHeightOffset, initialContentOffset)`.
    #[track_caller]
    pub fn new(
        initial_height_offset_limit: f32,
        initial_height_offset: f32,
        initial_content_offset: f32,
    ) -> Owned<BottomAppBarState> {
        Owned::new(
            TopAppBarStateInner::new(
                initial_height_offset_limit,
                initial_height_offset,
                initial_content_offset,
            ),
            |object| BottomAppBarState(TopAppBarState { object }),
        )
    }

    /// Saves what's saved of a top app bar's, as `BottomAppBarState.Saver`.
    pub fn saver() -> impl compose_runtime::Saver<Owned<BottomAppBarState>> {
        use compose_runtime::Saveable;
        compose_runtime::saver(
            |state: &Owned<BottomAppBarState>| {
                state.0.inner()?;
                Some(
                    (state.height_offset_limit(), state.height_offset(), state.content_offset())
                        .save(),
                )
            },
            |saved| {
                <(f32, f32, f32)>::restore(saved)
                    .map(|(limit, height, content)| BottomAppBarState::new(limit, height, content))
            },
        )
    }

    /// The most it collapses (negative), as `heightOffsetLimit`: the bar sets it.
    pub fn height_offset_limit(&self) -> f32 {
        self.0.height_offset_limit()
    }

    pub fn set_height_offset_limit(&self, limit: f32) {
        self.0.set_height_offset_limit(limit);
    }

    /// How far it's collapsed, between the limit and 0, as `heightOffset`.
    pub fn height_offset(&self) -> f32 {
        self.0.height_offset()
    }

    pub fn set_height_offset(&self, offset: f32) {
        self.0.set_height_offset(offset);
    }

    /// How far the content has scrolled, as `contentOffset`.
    pub fn content_offset(&self) -> f32 {
        self.0.content_offset()
    }

    pub fn set_content_offset(&self, offset: f32) {
        self.0.set_content_offset(offset);
    }

    /// 0 expanded to 1 collapsed, as `collapsedFraction`.
    pub fn collapsed_fraction(&self) -> f32 {
        self.0.collapsed_fraction()
    }
}

impl ComposeArg for BottomAppBarState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A bottom app bar state remembered here and saved, as `rememberBottomAppBarState`.
#[composable(non_restartable)]
pub fn remember_bottom_app_bar_state(
    #[default(-f32::MAX)] initial_height_offset_limit: f32,
    #[default(0.0)] initial_height_offset: f32,
    #[default(0.0)] initial_content_offset: f32,
) -> BottomAppBarState {
    compose_runtime::remember_saveable_with(BottomAppBarState::saver(), || {
        BottomAppBarState::new(
            initial_height_offset_limit,
            initial_height_offset,
            initial_content_offset,
        )
    })
}

/// How a bottom app bar follows the content's scrolling, as Compose's
/// `BottomAppBarScrollBehavior`: give its `nested_scroll_connection()` to `Modifier.nested_scroll`
/// around the content, and the behavior to the bar.
#[derive(Clone)]
pub struct BottomAppBarScrollBehavior(TopAppBarScrollBehavior);

impl BottomAppBarScrollBehavior {
    /// Its state, as `state`.
    pub fn state(&self) -> BottomAppBarState {
        BottomAppBarState(self.0.state)
    }

    /// As `isPinned`: no.
    pub fn is_pinned(&self) -> bool {
        false
    }

    /// As `snapAnimationSpec`.
    pub fn snap_animation_spec(&self) -> Option<AnimationSpec> {
        self.0.snap.clone()
    }

    /// As `flingAnimationSpec`.
    pub fn fling_animation_spec(&self) -> Option<DecayAnimationSpec> {
        self.0.fling.clone()
    }

    /// What `Modifier.nested_scroll` takes, as `nestedScrollConnection`.
    pub fn nested_scroll_connection(&self) -> Rc<dyn NestedScrollConnection> {
        self.0.nested_scroll_connection()
    }
}

impl ComposeArg for BottomAppBarScrollBehavior {
    fn same(&self, other: &Self) -> bool {
        self.0.same(&other.0)
    }
}

/// As Compose's `BottomAppBarDefaults`.
pub struct BottomAppBarDefaults;

impl BottomAppBarDefaults {
    /// Its tonal elevation, as `ContainerElevation`.
    pub const CONTAINER_ELEVATION: Dp = Dp(0.0);
    /// As `ContentPadding`: 4 dp at the sides and the top.
    pub const CONTENT_PADDING: PaddingValues =
        PaddingValues::new(Dp(4.0), Dp(4.0), Dp(4.0), Dp(0.0));
    /// As `FlexibleContentPadding`: 16 dp at the sides.
    pub const FLEXIBLE_CONTENT_PADDING: PaddingValues =
        PaddingValues::new(Dp(16.0), Dp(0.0), Dp(16.0), Dp(0.0));
    /// As `FlexibleBottomAppBarHeight`.
    pub const FLEXIBLE_BOTTOM_APP_BAR_HEIGHT: Dp = Dp(64.0);

    /// As `FlexibleHorizontalArrangement`.
    pub fn flexible_horizontal_arrangement() -> HorizontalArrangement {
        Arrangement::SPACE_BETWEEN.into()
    }

    /// As `FlexibleFixedHorizontalArrangement`: 32 dp apart, centered.
    pub fn flexible_fixed_horizontal_arrangement() -> HorizontalArrangement {
        Arrangement::spaced_by((Dp(32.0), Alignment::CENTER_HORIZONTALLY))
    }

    /// As `containerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface_container
    }

    /// A floating action button's color in the bar, as `bottomAppBarFabColor`.
    pub fn bottom_app_bar_fab_color() -> Color {
        MaterialTheme::color_scheme().secondary_container
    }

    /// As `windowInsets`: the system bars and a cutout, at the bottom and the sides.
    pub fn window_insets() -> WindowInsets {
        crate::scaffold::system_bars_for_visual_components()
            .only(WindowInsetsSides::HORIZONTAL + WindowInsetsSides::BOTTOM)
    }

    /// As `snapAnimationSpec`: the theme's fast spatial spec.
    pub fn snap_animation_spec() -> AnimationSpec {
        MaterialTheme::motion_scheme().fast_spatial_spec()
    }

    /// As `flingAnimationSpec`.
    pub fn fling_animation_spec() -> DecayAnimationSpec {
        compose_animation::remember_spline_based_decay()
    }

    /// The bar leaves as the content scrolls forward, back as it scrolls back, as
    /// `exitAlwaysScrollBehavior()`.
    pub fn exit_always_scroll_behavior() -> BottomAppBarScrollBehavior {
        Self::exit_always_scroll_behavior_with(
            remember_bottom_app_bar_state!(),
            None,
            Some(Self::snap_animation_spec()),
            Some(Self::fling_animation_spec()),
        )
    }

    /// As `exitAlwaysScrollBehavior(state, canScroll, snapAnimationSpec, flingAnimationSpec)`.
    pub fn exit_always_scroll_behavior_with(
        state: BottomAppBarState,
        can_scroll: Option<Callback<dyn Fn() -> bool>>,
        snap: Option<AnimationSpec>,
        fling: Option<DecayAnimationSpec>,
    ) -> BottomAppBarScrollBehavior {
        let latest: LatestCanScroll = compose_runtime::remember(LatestCanScroll::default);
        latest.replace(can_scroll);
        compose_runtime::remember_keyed((state, snap.clone(), fling.clone()), move || {
            BottomAppBarScrollBehavior(TopAppBarScrollBehavior::with(
                ScrollKind::ExitAlways,
                state.0,
                snap,
                fling,
                latest,
            ))
        })
    }
}

/// As Compose's `BottomAppBarLayout`: a surface collapsing with the behavior, `content` in a row.
#[allow(clippy::too_many_arguments)]
fn bottom_app_bar_layout(
    container_height: Dp,
    horizontal_arrangement: HorizontalArrangement,
    modifier: Modifier,
    container_color: Color,
    content_color: Color,
    tonal_elevation: Dp,
    content_padding: PaddingValues,
    window_insets: WindowInsets,
    scroll_behavior: Option<BottomAppBarScrollBehavior>,
    content: Callback<dyn Fn(RowScope)>,
) {
    let behavior = scroll_behavior.map(|behavior| behavior.0);
    let state = behavior.as_ref().map(TopAppBarScrollBehavior::state);
    let drag = bar_drag(&behavior, -1.0);
    // Collapses the whole height, read while measuring: a frame of it measures this only.
    let collapsing = compose_runtime::remember_keyed(state, move || {
        Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let mut height = placeable.height as f32;
            if let Some(state) = state {
                state.set_height_offset_limit(-(placeable.height as f32));
                height = (height + state.height_offset()).max(0.0);
            }
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, height.round() as i32)
        })
    });
    crate::components::surface(
        modifier.then(collapsing).then(drag),
        compose_ui::RECTANGLE_SHAPE,
        container_color,
        content_color,
        tonal_elevation,
        Dp(0.0),
        None,
        None,
        compose_runtime::callback(move || {
            Row(
                Modifier
                    .fill_max_width()
                    .window_insets_padding(window_insets.clone())
                    .height(container_height)
                    .padding(content_padding),
                horizontal_arrangement.clone(),
                Alignment::CENTER_VERTICALLY,
                content.clone(),
            );
        }),
    );
}

/// A bottom app bar, as Compose's `BottomAppBar(actions, …)`: 80 dp high, the actions in a row
/// at the start, the floating action button at the end (12 dp from the top, 16 dp from the end).
#[composable]
pub fn BottomAppBar(
    actions: impl Fn(RowScope) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] floating_action_button: Option<impl Fn() + 'static>,
    #[default(crate::BottomAppBarDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::BottomAppBarDefaults::CONTAINER_ELEVATION)] tonal_elevation: Dp,
    #[default(crate::BottomAppBarDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(crate::BottomAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(None)] scroll_behavior: Option<BottomAppBarScrollBehavior>,
) {
    bottom_app_bar_layout(
        BOTTOM_APP_BAR_HEIGHT,
        Arrangement::START,
        modifier,
        container_color,
        content_color,
        tonal_elevation,
        content_padding,
        window_insets,
        scroll_behavior,
        compose_runtime::callback(move |row: RowScope| {
            Row(
                Modifier.fill_max_height().weight(row, 1.0),
                Arrangement::START,
                Alignment::CENTER_VERTICALLY,
                actions.clone(),
            );
            if let Some(fab) = floating_action_button.clone() {
                Box(
                    // `FABVerticalPadding` and `FABHorizontalPadding`: 12 and 16 dp from
                    // the bar's edges, its 4 dp content padding included.
                    Modifier.fill_max_height().padding((Dp(0.0), 8.dp(), 12.dp(), Dp(0.0))),
                    Alignment::TOP_START,
                    false,
                    compose_runtime::callback(move |_| fab()),
                );
            }
        }),
    );
}

/// As `BottomAppBarTokens.ContainerHeight`.
const BOTTOM_APP_BAR_HEIGHT: Dp = Dp(80.0);

/// A bottom app bar with `content` in its row, as Compose's `BottomAppBar(content)`.
#[composable]
pub fn ContentBottomAppBar(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::BottomAppBarDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::BottomAppBarDefaults::CONTAINER_ELEVATION)] tonal_elevation: Dp,
    #[default(crate::BottomAppBarDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(crate::BottomAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(None)] scroll_behavior: Option<BottomAppBarScrollBehavior>,
    content: impl Fn(RowScope) + 'static,
) {
    bottom_app_bar_layout(
        BOTTOM_APP_BAR_HEIGHT,
        Arrangement::START,
        modifier,
        container_color,
        content_color,
        tonal_elevation,
        content_padding,
        window_insets,
        scroll_behavior,
        content,
    );
}

/// The flexible bottom app bar, as Compose's `FlexibleBottomAppBar`: `expanded_height` (64 dp) high,
/// `content` spaced by `horizontal_arrangement`.
#[composable]
pub fn FlexibleBottomAppBar(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::BottomAppBarDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::BottomAppBarDefaults::FLEXIBLE_CONTENT_PADDING)]
    content_padding: PaddingValues,
    #[default(crate::BottomAppBarDefaults::flexible_horizontal_arrangement())]
    horizontal_arrangement: HorizontalArrangement,
    #[default(crate::BottomAppBarDefaults::FLEXIBLE_BOTTOM_APP_BAR_HEIGHT)] expanded_height: Dp,
    #[default(crate::BottomAppBarDefaults::window_insets())] window_insets: WindowInsets,
    #[default(None)] scroll_behavior: Option<BottomAppBarScrollBehavior>,
    content: impl Fn(RowScope) + 'static,
) {
    let height = if expanded_height.is_specified()
        && expanded_height.0.is_finite()
        && expanded_height.0 > 0.0
    {
        expanded_height
    } else {
        BottomAppBarDefaults::FLEXIBLE_BOTTOM_APP_BAR_HEIGHT
    };
    bottom_app_bar_layout(
        height,
        horizontal_arrangement,
        modifier,
        container_color,
        content_color,
        Dp(0.0),
        content_padding,
        window_insets,
        scroll_behavior,
        content,
    );
}

/// The actions in a row at the end, as one callback (kept while `actions` is the same).
fn actions_row(actions: compose_runtime::Callback<dyn Fn(RowScope)>) -> compose_runtime::Callback {
    compose_runtime::remember_keyed(actions.clone(), move || {
        compose_runtime::callback(move || {
            Row(Modifier, Arrangement::END, Alignment::CENTER_VERTICALLY, actions.clone());
        })
    })
}
