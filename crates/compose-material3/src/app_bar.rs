//! App bars, as Compose's Material 3: `TopAppBar`, `CenterAlignedTopAppBar`, `MediumTopAppBar`
//! and `LargeTopAppBar` with their `TopAppBarState` and `TopAppBarScrollBehavior`s (pinned,
//! enter-always, exit-until-collapsed), and `BottomAppBar`.

use compose_runtime::{ObjectHandle, Owned};
use std::rc::Rc;

use compose_animation::{Animatable, AnimationSpec, Easing, FAST_OUT_LINEAR_IN_EASING, Spring};
use compose_foundation::layout::{Arrangement, Box, Column, Row, RowScope};
use compose_runtime::{
    ComposeArg, CompositionLocalProvider, CoroutineScope, OwnedState, composable, derived_state_of,
    mutable_state_of, remember, remember_coroutine_scope,
};
use compose_ui::{
    Alignment, Color, Constraints, Dp, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, Offset, PaddingValues, TextStyle, UnitExt,
};

use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};
use compose_ui::{NestedScrollConnection, NestedScrollSource};

// ---- State and scroll behaviors -------------------------------------------------------------

struct TopAppBarStateInner {
    height_offset_limit: OwnedState<f32>,
    /// Animated when it settles; snapped as the content scrolls.
    height_offset: Animatable<f32>,
    content_offset: OwnedState<f32>,
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
    #[track_caller]
    fn inner(&self) -> Rc<TopAppBarStateInner> {
        self.object.get()
    }

    #[track_caller]
    pub fn new(
        initial_height_offset_limit: f32,
        initial_height_offset: f32,
        initial_content_offset: f32,
    ) -> Owned<TopAppBarState> {
        Owned::new(
            TopAppBarStateInner {
                height_offset_limit: mutable_state_of(initial_height_offset_limit),
                height_offset: Animatable::new(
                    initial_height_offset.clamp(initial_height_offset_limit.min(0.0), 0.0),
                ),
                content_offset: mutable_state_of(initial_content_offset),
            },
            |object| TopAppBarState { object },
        )
    }

    /// The most the bar collapses (negative), as `heightOffsetLimit`: the bar sets it.
    pub fn height_offset_limit(&self) -> f32 {
        self.inner().height_offset_limit.get()
    }

    pub fn set_height_offset_limit(&self, limit: f32) {
        self.inner().height_offset_limit.set(limit);
    }

    /// How far it's collapsed, between the limit and 0, as `heightOffset`.
    pub fn height_offset(&self) -> f32 {
        self.inner().height_offset.value()
    }

    /// Collapses it to `offset`, kept between the limit and 0.
    pub fn set_height_offset(&self, offset: f32) {
        let limit = self.height_offset_limit().min(0.0);
        let offset = offset.clamp(limit, 0.0);
        if offset != self.inner().height_offset.value() || self.inner().height_offset.is_running() {
            self.inner().height_offset.snap_to(offset);
        }
    }

    /// How far the content has scrolled (negative: into it), as `contentOffset`.
    pub fn content_offset(&self) -> f32 {
        self.inner().content_offset.get()
    }

    pub fn set_content_offset(&self, offset: f32) {
        self.inner().content_offset.set(offset);
    }

    /// 0 expanded to 1 collapsed, as `collapsedFraction`.
    pub fn collapsed_fraction(&self) -> f32 {
        let limit = self.height_offset_limit();
        if limit != 0.0 { self.height_offset() / limit } else { 0.0 }
    }

    /// How much content has scrolled under the bar, 0 to 1, as `overlappedFraction`.
    pub fn overlapped_fraction(&self) -> f32 {
        let limit = self.height_offset_limit();
        if limit != 0.0 {
            1.0 - (limit - self.content_offset()).clamp(limit, 0.0) / limit
        } else {
            0.0
        }
    }

    /// Settles a collapse left halfway: to the nearer of expanded and collapsed, on a spring,
    /// as Compose's `settleAppBar` (without the fling's decay first).
    async fn settle(&self) {
        let fraction = self.collapsed_fraction();
        if !(0.01..1.0).contains(&fraction) {
            return;
        }
        let offset = self.height_offset();
        let limit = self.height_offset_limit();
        if offset < 0.0 && offset > limit {
            let target = if fraction < 0.5 { 0.0 } else { limit };
            let spring = compose_animation::spring_with(1.0, Spring::STIFFNESS_MEDIUM_LOW);
            self.inner().height_offset.animate_to(target, spring).await;
        }
    }
}

impl ComposeArg for TopAppBarState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A top app bar state remembered here, as `rememberTopAppBarState()`: no limit until a bar
/// sets it, expanded, the content at its start.
pub fn remember_top_app_bar_state() -> TopAppBarState {
    remember(|| TopAppBarState::new(-f32::MAX, 0.0, 0.0))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScrollKind {
    Pinned,
    EnterAlways,
    ExitUntilCollapsed,
}

/// How a top app bar follows the content's scrolling, as Compose's `TopAppBarScrollBehavior`:
/// give its `nested_scroll_connection()` to `Modifier.nested_scroll` around the content (the
/// `Scaffold`), and the behavior to the bar.
#[derive(Clone)]
pub struct TopAppBarScrollBehavior {
    state: TopAppBarState,
    kind: ScrollKind,
    connection: Rc<dyn NestedScrollConnection>,
}

impl TopAppBarScrollBehavior {
    /// The bar stays; its color follows the content scrolled under it, as
    /// `pinnedScrollBehavior(state)`.
    pub fn pinned(state: TopAppBarState, scope: CoroutineScope) -> TopAppBarScrollBehavior {
        TopAppBarScrollBehavior::new(ScrollKind::Pinned, state, scope)
    }

    /// The bar collapses as the content scrolls forward and comes back as soon as it scrolls
    /// back, as `enterAlwaysScrollBehavior(state)`.
    pub fn enter_always(state: TopAppBarState, scope: CoroutineScope) -> TopAppBarScrollBehavior {
        TopAppBarScrollBehavior::new(ScrollKind::EnterAlways, state, scope)
    }

    /// The bar collapses as the content scrolls forward, and expands once the content is back at
    /// its start, as `exitUntilCollapsedScrollBehavior(state)`.
    pub fn exit_until_collapsed(
        state: TopAppBarState,
        scope: CoroutineScope,
    ) -> TopAppBarScrollBehavior {
        TopAppBarScrollBehavior::new(ScrollKind::ExitUntilCollapsed, state, scope)
    }

    fn new(kind: ScrollKind, state: TopAppBarState, scope: CoroutineScope) -> Self {
        let connection = Rc::new(ScrollConnection { kind, state, scope });
        TopAppBarScrollBehavior { state, kind, connection }
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
    scope: CoroutineScope,
}

impl NestedScrollConnection for ScrollConnection {
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        let state = &self.state;
        // A new drag or wheel turn stops a settle going on, as Compose's (its fling's
        // coroutine is cancelled).
        if source == NestedScrollSource::UserInput && state.inner().height_offset.is_running() {
            state.inner().height_offset.stop();
        }
        let collapses = match self.kind {
            ScrollKind::Pinned => false,
            ScrollKind::EnterAlways => true,
            // Not when scrolling back: the content goes first.
            ScrollKind::ExitUntilCollapsed => available.y <= 0.0,
        };
        if !collapses {
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
        let state = &self.state;
        match self.kind {
            ScrollKind::Pinned => {
                if consumed.y == 0.0 && available.y > 0.0 {
                    state.set_content_offset(0.0);
                } else {
                    state.set_content_offset(state.content_offset() + consumed.y);
                }
                Offset::ZERO
            }
            ScrollKind::EnterAlways => {
                state.set_content_offset(state.content_offset() + consumed.y);
                let at_end = state.height_offset() == 0.0
                    || state.height_offset() == state.height_offset_limit();
                if at_end && consumed.y == 0.0 && available.y > 0.0 {
                    // Back at the start: no rounding errors left.
                    state.set_content_offset(0.0);
                }
                state.set_height_offset(state.height_offset() + consumed.y);
                Offset::ZERO
            }
            ScrollKind::ExitUntilCollapsed => {
                state.set_content_offset(state.content_offset() + consumed.y);
                if available.y < 0.0 || consumed.y < 0.0 {
                    let before = state.height_offset();
                    state.set_height_offset(before + consumed.y);
                    return Offset::new(0.0, state.height_offset() - before);
                }
                if consumed.y == 0.0 && available.y > 0.0 {
                    state.set_content_offset(0.0);
                }
                if available.y > 0.0 {
                    // What the content left (it's at its start) expands the bar.
                    let before = state.height_offset();
                    state.set_height_offset(before + available.y);
                    return Offset::new(0.0, state.height_offset() - before);
                }
                Offset::ZERO
            }
        }
    }

    fn on_post_fling(&self, _consumed: Offset, _available: Offset) -> Offset {
        if self.kind != ScrollKind::Pinned {
            let state = self.state;
            self.scope.launch(async move { state.settle().await });
        }
        Offset::ZERO
    }
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
}

impl TopAppBarColors {
    /// The container's color, `fraction` of the way to the scrolled one (eased), as
    /// `containerColor(colorTransitionFraction)`.
    pub fn container_color(&self, fraction: f32) -> Color {
        crate::selection_blend(
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

    pub fn top_app_bar_colors() -> TopAppBarColors {
        let scheme = MaterialTheme::color_scheme();
        TopAppBarColors {
            container_color: scheme.surface,
            scrolled_container_color: scheme.surface_container,
            navigation_icon_content_color: scheme.on_surface,
            title_content_color: scheme.on_surface,
            action_icon_content_color: scheme.on_surface_variant,
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

    /// The insets a bar keeps its content clear of, as `windowInsets`: none until hosts report
    /// the window's (padding stands in for `WindowInsets`).
    pub fn window_insets() -> PaddingValues {
        PaddingValues::ZERO
    }

    /// A pinned behavior with its state remembered here, as `pinnedScrollBehavior()`.
    pub fn pinned_scroll_behavior() -> TopAppBarScrollBehavior {
        remembered_behavior(TopAppBarScrollBehavior::pinned)
    }

    /// As `enterAlwaysScrollBehavior()`.
    pub fn enter_always_scroll_behavior() -> TopAppBarScrollBehavior {
        remembered_behavior(TopAppBarScrollBehavior::enter_always)
    }

    /// As `exitUntilCollapsedScrollBehavior()`.
    pub fn exit_until_collapsed_scroll_behavior() -> TopAppBarScrollBehavior {
        remembered_behavior(TopAppBarScrollBehavior::exit_until_collapsed)
    }
}

fn remembered_behavior(
    make: fn(TopAppBarState, CoroutineScope) -> TopAppBarScrollBehavior,
) -> TopAppBarScrollBehavior {
    let state = remember_top_app_bar_state();
    let scope = remember_coroutine_scope();
    remember(move || make(state, scope))
}

/// The padding around the navigation icon and the actions, as `TopAppBarHorizontalPadding`.
const HORIZONTAL_PADDING: Dp = Dp(4.0);
/// The title's start without a navigation icon (with its own padding, 16 dp in), as
/// `TopAppBarTitleInset`.
const TITLE_INSET: Dp = Dp(12.0);

/// A small top app bar, as Compose's `TopAppBar`: `expanded_height` (64 dp) high (less as its
/// `scroll_behavior` collapses it), the navigation icon at the start, the title after it in
/// the title-large style, the actions at the end; the container turns the scrolled color as
/// content scrolls under it.
#[composable]
pub fn TopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::TopAppBarDefaults::TOP_APP_BAR_EXPANDED_HEIGHT)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: PaddingValues,
    #[default(crate::TopAppBarDefaults::top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    single_row_top_app_bar(
        title,
        modifier,
        navigation_icon,
        actions,
        expanded_height,
        window_insets,
        colors,
        scroll_behavior,
        false,
    );
}

/// A top app bar with its title centered, as Compose's `CenterAlignedTopAppBar`.
#[composable]
pub fn CenterAlignedTopAppBar(
    title: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] navigation_icon: Option<impl Fn() + 'static>,
    #[default(|_| {})] actions: impl Fn(RowScope) + 'static,
    #[default(crate::TopAppBarDefaults::TOP_APP_BAR_EXPANDED_HEIGHT)] expanded_height: Dp,
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: PaddingValues,
    #[default(crate::TopAppBarDefaults::center_aligned_top_app_bar_colors())]
    colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    single_row_top_app_bar(
        title,
        modifier,
        navigation_icon,
        actions,
        expanded_height,
        window_insets,
        colors,
        scroll_behavior,
        true,
    );
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
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: PaddingValues,
    #[default(crate::TopAppBarDefaults::medium_top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    let typography = MaterialTheme::typography();
    two_rows_top_app_bar(TwoRows {
        title,
        title_style: typography.headline_small,
        small_title_style: typography.title_large,
        title_bottom_padding: Dp(24.0),
        modifier,
        navigation_icon,
        actions,
        collapsed_height,
        expanded_height,
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
    #[default(crate::TopAppBarDefaults::window_insets())] window_insets: PaddingValues,
    #[default(crate::TopAppBarDefaults::large_top_app_bar_colors())] colors: TopAppBarColors,
    #[default(None)] scroll_behavior: Option<TopAppBarScrollBehavior>,
) {
    let typography = MaterialTheme::typography();
    two_rows_top_app_bar(TwoRows {
        title,
        title_style: typography.headline_medium,
        small_title_style: typography.title_large,
        title_bottom_padding: Dp(28.0),
        modifier,
        navigation_icon,
        actions,
        collapsed_height,
        expanded_height,
        window_insets,
        colors,
        scroll_behavior,
    });
}

/// Sets the state's limit to `limit` pixels after the pass, as Compose's bars' `SideEffect`.
fn set_limit(scroll_behavior: &Option<TopAppBarScrollBehavior>, limit: f32) {
    if let Some(state) = scroll_behavior.as_ref().map(TopAppBarScrollBehavior::state) {
        compose_runtime::SideEffect(move || {
            if state.height_offset_limit() != limit {
                state.set_height_offset_limit(limit);
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn single_row_top_app_bar(
    title: compose_runtime::Callback,
    modifier: Modifier,
    navigation_icon: Option<compose_runtime::Callback>,
    actions: compose_runtime::Callback<dyn Fn(RowScope)>,
    expanded_height: Dp,
    window_insets: PaddingValues,
    colors: TopAppBarColors,
    scroll_behavior: Option<TopAppBarScrollBehavior>,
    centered: bool,
) {
    let density = compose_ui::LOCAL_DENSITY.current();
    set_limit(&scroll_behavior, -density.to_px(expanded_height));
    let state = scroll_behavior.as_ref().map(TopAppBarScrollBehavior::state);
    // The container turns the scrolled color once content is under the bar, animated, as
    // Compose's (the threshold read in a derived state: crossing it composes again only).
    let overlapped = {
        compose_runtime::remember_keyed(state, move || {
            Rc::new(derived_state_of(move || {
                state.as_ref().is_some_and(|state| state.overlapped_fraction() > 0.01)
            }))
        })
    };
    let fraction = compose_animation::animate_float_as_state(
        if overlapped.get() { 1.0 } else { 0.0 },
        AnimationSpec::Spring(compose_animation::spring_with(1.0, Spring::STIFFNESS_MEDIUM_LOW)),
        Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
        "TopAppBarColor",
        None,
    );
    let policy = compose_runtime::remember_keyed((state, expanded_height, centered), {
        move || {
            Rc::new(TopAppBarLayout {
                height: expanded_height,
                state,
                centered,
                title_bottom_padding: None,
            }) as Rc<dyn MeasurePolicy>
        }
    });
    let title_style = MaterialTheme::typography().title_large;
    let actions = actions_row(actions);
    // Read while drawing: the transition draws again only.
    let container = Modifier.draw_behind(move |scope| {
        let color = colors.container_color(fraction.get());
        scope.draw_rect(scope.bounds(), compose_ui::Paint::fill(color));
    });
    crate::components::surface(
        modifier.then(container),
        compose_ui::RECTANGLE_SHAPE,
        Color::TRANSPARENT,
        colors.title_content_color,
        Dp(0.0),
        Dp(0.0),
        None,
        None,
        compose_runtime::callback(move || {
            top_app_bar_row(
                Modifier.padding_values(window_insets),
                policy.clone(),
                title.clone(),
                title_style,
                None,
                false,
                navigation_icon.clone(),
                Some(actions.clone()),
                colors,
            );
        }),
    );
}

struct TwoRows {
    title: compose_runtime::Callback,
    title_style: TextStyle,
    small_title_style: TextStyle,
    title_bottom_padding: Dp,
    modifier: Modifier,
    navigation_icon: Option<compose_runtime::Callback>,
    actions: compose_runtime::Callback<dyn Fn(RowScope)>,
    collapsed_height: Dp,
    expanded_height: Dp,
    window_insets: PaddingValues,
    colors: TopAppBarColors,
    scroll_behavior: Option<TopAppBarScrollBehavior>,
}

/// The medium and large bars, as Compose's `TwoRowsTopAppBar`: a pinned row with the small
/// title fading in as the bar collapses, over a row with the title fading out, shrinking.
fn two_rows_top_app_bar(bar: TwoRows) {
    let TwoRows { colors, collapsed_height, expanded_height, .. } = bar;
    let density = compose_ui::LOCAL_DENSITY.current();
    set_limit(
        &bar.scroll_behavior,
        density.to_px(collapsed_height) - density.to_px(expanded_height),
    );
    let state = bar.scroll_behavior.as_ref().map(TopAppBarScrollBehavior::state);
    let collapsed = { move || state.as_ref().map_or(0.0, TopAppBarState::collapsed_fraction) };
    // Which row's title screen readers see: crossing half way composes again only.
    let top_shown = {
        compose_runtime::remember_keyed(state, move || {
            Rc::new(derived_state_of(move || collapsed() >= 0.5))
        })
    };
    let top_shown = top_shown.get();
    let policies = compose_runtime::remember_keyed(
        (state, collapsed_height, expanded_height, bar.title_bottom_padding),
        {
            let bottom_padding = bar.title_bottom_padding;
            move || {
                let top: Rc<dyn MeasurePolicy> = Rc::new(TopAppBarLayout {
                    height: collapsed_height,
                    state: None,
                    centered: false,
                    title_bottom_padding: None,
                });
                let bottom: Rc<dyn MeasurePolicy> = Rc::new(TopAppBarLayout {
                    height: Dp(expanded_height.0 - collapsed_height.0),
                    state,
                    centered: false,
                    title_bottom_padding: Some(bottom_padding),
                });
                (top, bottom)
            }
        },
    );
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
    let TwoRows { title, title_style, small_title_style, navigation_icon, actions, .. } = bar;
    let actions = actions_row(actions);
    let window_insets = bar.window_insets;
    crate::components::surface(
        bar.modifier.then(container),
        compose_ui::RECTANGLE_SHAPE,
        Color::TRANSPARENT,
        colors.title_content_color,
        Dp(0.0),
        Dp(0.0),
        None,
        None,
        compose_runtime::callback(move || {
            let (title, navigation_icon, actions) =
                (title.clone(), navigation_icon.clone(), actions.clone());
            let (policies, top_alpha, bottom_alpha) =
                (policies.clone(), top_alpha.clone(), bottom_alpha.clone());
            Column(
                Modifier,
                Arrangement::TOP,
                Alignment::START,
                compose_runtime::callback(move |_| {
                    top_app_bar_row(
                        Modifier.padding_values(window_insets).clip_to_bounds(),
                        policies.0.clone(),
                        title.clone(),
                        small_title_style,
                        Some(top_alpha.clone()),
                        !top_shown,
                        navigation_icon.clone(),
                        Some(actions.clone()),
                        colors,
                    );
                    top_app_bar_row(
                        Modifier
                            .padding_values(PaddingValues::new(
                                window_insets.start,
                                Dp(0.0),
                                window_insets.end,
                                Dp(0.0),
                            ))
                            .clip_to_bounds(),
                        policies.1.clone(),
                        title.clone(),
                        title_style,
                        Some(bottom_alpha.clone()),
                        top_shown,
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
const TOP_TITLE_ALPHA_EASING: Easing = Easing::CubicBezier(0.8, 0.0, 0.8, 0.15);

/// A row of a top app bar, as Compose's `TopAppBarLayout`: the navigation icon, the title and
/// the actions, laid out by `policy`.
#[allow(clippy::too_many_arguments)]
fn top_app_bar_row(
    modifier: Modifier,
    policy: Rc<dyn MeasurePolicy>,
    title: compose_runtime::Callback,
    title_style: TextStyle,
    title_alpha: Option<Modifier>,
    hide_title_semantics: bool,
    navigation_icon: Option<compose_runtime::Callback>,
    actions: Option<compose_runtime::Callback>,
    colors: TopAppBarColors,
) {
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            bar_slot(
                Modifier.padding_sides(HORIZONTAL_PADDING, Dp(0.0), Dp(0.0), Dp(0.0)),
                colors.navigation_icon_content_color,
                None,
                navigation_icon.clone(),
            );
            let mut title_modifier =
                Modifier.padding_horizontal_vertical(HORIZONTAL_PADDING, 0.dp());
            if hide_title_semantics {
                title_modifier = title_modifier.clear_and_set_semantics(|_| {});
            }
            if let Some(alpha) = &title_alpha {
                title_modifier = title_modifier.then(alpha.clone());
            }
            bar_slot(
                title_modifier,
                colors.title_content_color,
                Some(title_style),
                Some(title.clone()),
            );
            bar_slot(
                Modifier.padding_sides(Dp(0.0), Dp(0.0), HORIZONTAL_PADDING, Dp(0.0)),
                colors.action_icon_content_color,
                None,
                actions.clone(),
            );
        }),
    )
}

/// A slot of a bar's row: `content` in a box of `modifier`, in `color` (and `style`).
fn bar_slot(
    modifier: Modifier,
    color: Color,
    style: Option<TextStyle>,
    content: Option<compose_runtime::Callback>,
) {
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
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || match style {
        Some(style) => CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), &body),
        None => body(),
    });
}

/// The navigation icon, the title and the actions of a bar's row, as Compose's
/// `TopAppBarLayout`: `height` plus the state's (negative) height offset high, as wide as
/// allowed; the icons centered down it at either end; the title after the navigation icon (at
/// least 12 dp in), or centered between them, or (with a bottom padding) its baseline that far
/// above the bottom.
struct TopAppBarLayout {
    height: Dp,
    state: Option<TopAppBarState>,
    centered: bool,
    title_bottom_padding: Option<Dp>,
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
        let loose = Constraints::new(0, constraints.max_width, 0, constraints.max_height);
        let navigation = scope.measure(*navigation, loose);
        let actions = scope.measure(*actions, loose);
        let width = constraints.max_width;
        let title_width = if width == Constraints::INFINITY {
            width
        } else {
            (width - navigation.width - actions.width).max(0)
        };
        let title =
            scope.measure(*title, Constraints::new(0, title_width, 0, constraints.max_height));
        // Read while measuring: collapsing measures the bar again only.
        let offset = self.state.as_ref().map_or(0.0, TopAppBarState::height_offset);
        let height = (density.to_px(self.height) + offset).round().max(0.0) as i32;
        scope.place_relative(&navigation, 0, (height - navigation.height) / 2);
        let x = if self.centered {
            let mut x = (width - title.width) / 2;
            if x < navigation.width {
                // A long title next to a wide navigation icon: more of the title shows.
                x += navigation.width - x;
            } else if x + title.width > width - actions.width {
                x += (width - actions.width) - (x + title.width);
            }
            x
        } else {
            density.round_to_px(TITLE_INSET).max(navigation.width)
        };
        let y = match self.title_bottom_padding {
            None => (height - title.height) / 2,
            Some(padding) => {
                // Its baseline `padding` above the bottom (a line's baseline taken at 80 % of
                // its height, as the text measurer puts it).
                let baseline = (title.height as f32 * 0.8).round() as i32;
                let padding = density.round_to_px(padding);
                height - title.height - (padding - title.height + baseline).max(0)
            }
        };
        scope.place_relative(&title, x, y);
        scope.place_relative(&actions, width - actions.width, (height - actions.height) / 2);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "TopAppBarLayout"
    }
}

// ---- Bottom app bar --------------------------------------------------------------------------

/// As Compose's `BottomAppBarDefaults`.
pub struct BottomAppBarDefaults;

impl BottomAppBarDefaults {
    /// Its tonal elevation, as `ContainerElevation`.
    pub const CONTAINER_ELEVATION: Dp = Dp(3.0);
    /// As `ContentPadding`: 4 dp at the sides and the top.
    pub const CONTENT_PADDING: PaddingValues =
        PaddingValues::new(Dp(4.0), Dp(4.0), Dp(4.0), Dp(0.0));

    /// As `containerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface_container
    }

    /// A floating action button's color in the bar, as `bottomAppBarFabColor`.
    pub fn bottom_app_bar_fab_color() -> Color {
        MaterialTheme::color_scheme().secondary_container
    }

    /// As `windowInsets`: none until hosts report them.
    pub fn window_insets() -> PaddingValues {
        PaddingValues::ZERO
    }
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
    #[default(crate::BottomAppBarDefaults::window_insets())] window_insets: PaddingValues,
) {
    crate::components::surface(
        modifier,
        compose_ui::RECTANGLE_SHAPE,
        container_color,
        content_color,
        tonal_elevation,
        Dp(0.0),
        None,
        None,
        compose_runtime::callback(move || {
            let (actions, fab) = (actions.clone(), floating_action_button.clone());
            Row(
                Modifier
                    .fill_max_width()
                    .padding_values(window_insets)
                    .height(80.dp())
                    .padding_values(content_padding),
                Arrangement::Start,
                Alignment::CENTER_VERTICALLY,
                compose_runtime::callback(move |row: RowScope| {
                    Row(
                        Modifier.fill_max_height().weight(row, 1.0),
                        Arrangement::Start,
                        Alignment::CENTER_VERTICALLY,
                        actions.clone(),
                    );
                    if let Some(fab) = fab.clone() {
                        Box(
                            // `FABVerticalPadding` and `FABHorizontalPadding`: 12 and 16 dp from
                            // the bar's edges, its 4 dp content padding included.
                            Modifier.fill_max_height().padding_sides(
                                Dp(0.0),
                                8.dp(),
                                12.dp(),
                                Dp(0.0),
                            ),
                            Alignment::TOP_START,
                            false,
                            compose_runtime::callback(move |_| fab()),
                        );
                    }
                }),
            );
        }),
    );
}

/// The actions in a row at the end, as one callback (kept while `actions` is the same).
fn actions_row(actions: compose_runtime::Callback<dyn Fn(RowScope)>) -> compose_runtime::Callback {
    compose_runtime::remember_keyed(actions.clone(), move || {
        compose_runtime::callback(move || {
            Row(Modifier, Arrangement::End, Alignment::CENTER_VERTICALLY, actions.clone());
        })
    })
}
