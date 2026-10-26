//! The tab rows, as Compose's Material 3 at androidx-main's tip: `PrimaryTabRow` and
//! `SecondaryTabRow` (each tab an equal share of the width), `PrimaryScrollableTabRow` and
//! `SecondaryScrollableTabRow` (each tab as wide as it needs, scrolling), their `indicator` slot
//! placed through a `TabIndicatorScope` (`tab_indicator_offset` follows the selected tab,
//! animated), their `divider` slot; and the tabs: `Tab`, `ContentTab` (Compose's `Tab(content)`
//! overload) and `LeadingIconTab`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::{Animatable, AnimationSpec};
use compose_foundation::layout::{Arrangement, Box, Column, ColumnScope, Row, Spacer};
use compose_foundation::{ScrollModifiers, ScrollState};
use compose_runtime::{
    Callback, CompositionLocalProvider, CoroutineScope, Owned, composable, remember,
};
use compose_ui::{
    Alignment, ClickableOptions, Color, Constraints, CornerSize, Dp, Layout, LayoutDirection,
    Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, MutableInteractionSource, Role, Shape, SizeSpec, TextAlign, UnitExt,
};

use crate::components::Surface;
use crate::motion;
use crate::scaffold::HorizontalDivider;
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// As Compose's `TabRowDefaults`.
pub struct TabRowDefaults;

impl TabRowDefaults {
    /// A scrollable row's narrowest tab, as `ScrollableTabRowMinTabWidth`.
    pub const SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH: Dp = Dp(90.0);
    /// Before the first scrollable tab and after the last, as
    /// `ScrollableTabRowEdgeStartPadding`.
    pub const SCROLLABLE_TAB_ROW_EDGE_START_PADDING: Dp = Dp(52.0);
    /// The indicators' height, as `PrimaryNavigationTabTokens.ActiveIndicatorHeight`.
    pub const INDICATOR_HEIGHT: Dp = Dp(3.0);
    /// A primary indicator's width when not following the content, as `PrimaryIndicator`'s
    /// default.
    pub const PRIMARY_INDICATOR_WIDTH: Dp = Dp(24.0);

    /// A primary row's color, as `primaryContainerColor` (the surface).
    pub fn primary_container_color() -> Color {
        MaterialTheme::color_scheme().surface
    }

    /// A secondary row's color, as `secondaryContainerColor` (the surface).
    pub fn secondary_container_color() -> Color {
        MaterialTheme::color_scheme().surface
    }

    /// A primary row's tabs' color, as `primaryContentColor` (the primary color).
    pub fn primary_content_color() -> Color {
        MaterialTheme::color_scheme().primary
    }

    /// A secondary row's tabs' color, as `secondaryContentColor` (on the surface).
    pub fn secondary_content_color() -> Color {
        MaterialTheme::color_scheme().on_surface
    }

    /// The indicators' color, as `PrimaryNavigationTabTokens.ActiveIndicatorColor` (the
    /// primary color).
    pub fn indicator_color() -> Color {
        MaterialTheme::color_scheme().primary
    }

    /// A primary indicator's shape, as `PrimaryNavigationTabTokens.ActiveIndicatorShape`:
    /// 3 dp corners.
    pub fn primary_indicator_shape() -> Shape {
        Shape::Rounded([CornerSize::Dp(Dp(3.0)); 4])
    }

    /// The primary rows' indicator, as `TabRowDefaults.PrimaryIndicator`: a `width` by `height`
    /// pill (`width` `Dp::UNSPECIFIED`: as wide as `modifier` makes it, the content's width
    /// under `tab_indicator_offset(…, true)`).
    pub fn primary_indicator(
        modifier: Modifier,
        width: Dp,
        height: Dp,
        color: Color,
        shape: Shape,
    ) {
        let modifier = modifier.size_in(SizeSpec::exact(Dp::UNSPECIFIED, height, true));
        let modifier = if width.is_specified() {
            modifier.size_in(SizeSpec::exact(width, Dp::UNSPECIFIED, true))
        } else {
            modifier
        };
        Spacer(modifier.background_with(color, shape));
    }

    /// The secondary rows' indicator, as `TabRowDefaults.SecondaryIndicator`: a line as wide
    /// as allowed, `height` high.
    pub fn secondary_indicator(modifier: Modifier, height: Dp, color: Color) {
        Box(
            modifier.fill_max_width().height(height).background(color),
            Alignment::TOP_START,
            false,
            compose_runtime::callback(|_| {}),
        );
    }
}

/// Where a tab is in its row, as Compose's `TabPosition`: its left (from the row's start,
/// edge padding included), its width, and its content's width (its text's, at least 24 dp).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TabPosition {
    pub left: Dp,
    pub width: Dp,
    pub content_width: Dp,
}

impl TabPosition {
    pub fn right(&self) -> Dp {
        self.left + self.width
    }
}

/// What a row's `indicator` is placed with, as Compose's `TabIndicatorScope`: the tabs'
/// positions, as the row last laid them out. Use it through `TabIndicatorModifiers`.
#[derive(Clone)]
pub struct TabIndicatorScope {
    positions: Rc<RefCell<Vec<TabPosition>>>,
}

impl compose_runtime::ComposeArg for TabIndicatorScope {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.positions, &other.positions)
    }
}

impl TabIndicatorScope {
    /// The tabs' positions, as the row last laid them out.
    pub fn tab_positions(&self) -> Vec<TabPosition> {
        self.positions.borrow().clone()
    }
}

/// The modifiers of a row's indicator, as `TabIndicatorScope`'s (they take the scope, as
/// scoped modifiers do, D19).
pub trait TabIndicatorModifiers {
    /// Under the selected tab, as `Modifier.tabIndicatorOffset(selectedTabIndex,
    /// matchContentSize)`: as wide as the tab (or its content), moving and resizing to a new
    /// selection with the standard motion scheme's spatial spring.
    fn tab_indicator_offset(
        self,
        scope: &TabIndicatorScope,
        selected_tab_index: usize,
        match_content_size: bool,
    ) -> Modifier;

    /// Measures and places the indicator with the tabs' positions, as
    /// `Modifier.tabIndicatorLayout`.
    fn tab_indicator_layout(
        self,
        scope: &TabIndicatorScope,
        measure: impl Fn(
            &mut MeasureScope<'_>,
            Measurable,
            Constraints,
            &[TabPosition],
        ) -> MeasureResult
        + 'static,
    ) -> Modifier;
}

impl TabIndicatorModifiers for Modifier {
    fn tab_indicator_offset(
        self,
        scope: &TabIndicatorScope,
        selected_tab_index: usize,
        match_content_size: bool,
    ) -> Modifier {
        self.then_custom(TabIndicatorOffset {
            positions: scope.positions.clone(),
            selected_tab_index,
            match_content_size,
        })
    }

    fn tab_indicator_layout(
        self,
        scope: &TabIndicatorScope,
        measure: impl Fn(
            &mut MeasureScope<'_>,
            Measurable,
            Constraints,
            &[TabPosition],
        ) -> MeasureResult
        + 'static,
    ) -> Modifier {
        let positions = scope.positions.clone();
        self.layout(move |scope, measurable, constraints| {
            let positions = positions.borrow().clone();
            measure(scope, measurable, constraints, &positions)
        })
    }
}

struct TabIndicatorOffset {
    positions: Rc<RefCell<Vec<TabPosition>>>,
    selected_tab_index: usize,
    match_content_size: bool,
}

impl ModifierNodeElement for TabIndicatorOffset {
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        std::boxed::Box::new(TabIndicatorOffsetNode {
            positions: self.positions.clone(),
            selected_tab_index: self.selected_tab_index,
            match_content_size: self.match_content_size,
            offset: None,
            width: None,
            initial: None,
            scope: CoroutineScope::new(),
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<TabIndicatorOffsetNode>() {
            node.positions = self.positions.clone();
            node.selected_tab_index = self.selected_tab_index;
            node.match_content_size = self.match_content_size;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<TabIndicatorOffset>().is_some_and(|other| {
            Rc::ptr_eq(&other.positions, &self.positions)
                && other.selected_tab_index == self.selected_tab_index
                && other.match_content_size == self.match_content_size
        })
    }

    fn name(&self) -> &'static str {
        "tab_indicator_offset"
    }
}

/// As Compose's `TabIndicatorOffsetNode`: the selected tab's left and width (dp), animated
/// from the first ones it measured with.
struct TabIndicatorOffsetNode {
    positions: Rc<RefCell<Vec<TabPosition>>>,
    selected_tab_index: usize,
    match_content_size: bool,
    offset: Option<Animatable<f32>>,
    width: Option<Animatable<f32>>,
    /// The first left and width, before any animation.
    initial: Option<(f32, f32)>,
    /// Where its animations run, cancelled when it goes (Compose's node's `coroutineScope`).
    scope: Owned<CoroutineScope>,
}

impl Drop for TabIndicatorOffsetNode {
    fn drop(&mut self) {
        self.scope.cancel();
    }
}

impl ModifierNode for TabIndicatorOffsetNode {
    fn as_layout(&mut self) -> Option<&mut dyn compose_ui::LayoutModifierNode> {
        Some(self)
    }
}

impl TabIndicatorOffsetNode {
    /// The animated value going to `target` (dp), created at `initial` the first time.
    fn follow(&self, animatable: &Option<Animatable<f32>>, target: f32) -> Option<f32> {
        let animatable = animatable.as_ref()?;
        if animatable.target_value() != target {
            let animatable = animatable.clone();
            self.scope.launch(async move {
                animatable.animate_to(target, AnimationSpec::Spring(motion::DEFAULT_SPATIAL)).await;
            });
        }
        Some(animatable.value())
    }
}

impl compose_ui::LayoutModifierNode for TabIndicatorOffsetNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let Some(position) = self.positions.borrow().get(self.selected_tab_index).copied() else {
            return scope.layout(0, 0);
        };
        let target_width =
            if self.match_content_size { position.content_width.0 } else { position.width.0 };
        let target_offset = position.left.0;
        match self.initial {
            None => self.initial = Some((target_offset, target_width)),
            Some((initial_offset, initial_width)) => {
                self.width.get_or_insert_with(|| Animatable::new(initial_width));
                self.offset.get_or_insert_with(|| Animatable::new(initial_offset));
            }
        }
        // Read while measuring: each frame of the animation measures it again.
        let width = self.follow(&self.width, target_width).unwrap_or(target_width);
        let offset = self.follow(&self.offset, target_offset).unwrap_or(target_offset);
        let density = scope.density();
        let width_px = density.round_to_px(Dp(width)).max(0);
        let placeable = scope.measure(
            content,
            Constraints::new(width_px, width_px, constraints.min_height, constraints.max_height),
        );
        let offset_px = density.round_to_px(Dp(offset));
        let offset_px =
            if scope.layout_direction() == LayoutDirection::Ltr { offset_px } else { -offset_px };
        // Its size kept within its constraints, its content centered in it when they differ,
        // as Compose does with a layout that measures outside its constraints.
        let size = constraints.constrain_width(placeable.width);
        let height = constraints.constrain_height(placeable.height);
        let centered = (size - placeable.width) / 2;
        scope.place(&placeable, offset_px + centered, (height - placeable.height) / 2);
        scope.layout(size, height)
    }
}

/// `PrimaryTabRow`'s and `PrimaryScrollableTabRow`'s default indicator: a primary indicator as
/// wide as the selected tab's content.
fn primary_default_indicator(selected_tab_index: usize) -> Callback<dyn Fn(TabIndicatorScope)> {
    compose_runtime::callback(move |scope: TabIndicatorScope| {
        TabRowDefaults::primary_indicator(
            Modifier.tab_indicator_offset(&scope, selected_tab_index, true),
            Dp::UNSPECIFIED,
            TabRowDefaults::INDICATOR_HEIGHT,
            TabRowDefaults::indicator_color(),
            TabRowDefaults::primary_indicator_shape(),
        );
    })
}

/// The secondary rows' default indicator: a secondary indicator as wide as the selected tab.
fn secondary_default_indicator(selected_tab_index: usize) -> Callback<dyn Fn(TabIndicatorScope)> {
    compose_runtime::callback(move |scope: TabIndicatorScope| {
        TabRowDefaults::secondary_indicator(
            Modifier.tab_indicator_offset(&scope, selected_tab_index, false),
            TabRowDefaults::INDICATOR_HEIGHT,
            TabRowDefaults::indicator_color(),
        );
    })
}

/// The tab rows' default divider, as `{ HorizontalDivider() }`: one callback per thread.
#[doc(hidden)]
pub fn default_tab_divider() -> Callback {
    thread_local! {
        static DIVIDER: Callback = compose_runtime::callback(|| {
            HorizontalDivider(Modifier, Dp(1.0), MaterialTheme::color_scheme().outline_variant);
        });
    }
    DIVIDER.with(Clone::clone)
}

/// A row of tabs, as Compose's `PrimaryTabRow`: each tab an equal share of the width, as tall
/// as the tallest, `divider` along the bottom, and `indicator` (by default a 3 dp pill as wide
/// as the selected tab's content, `None`) placed through its `TabIndicatorScope`. In
/// semantics, a selectable group.
#[composable]
#[allow(non_snake_case)]
pub fn PrimaryTabRow(
    selected_tab_index: usize,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TabRowDefaults::primary_container_color())] container_color: Color,
    #[default(crate::TabRowDefaults::primary_content_color())] content_color: Color,
    #[default(None)] indicator: Option<impl Fn(TabIndicatorScope) + 'static>,
    #[default(crate::default_tab_divider())] divider: impl Fn() + 'static,
    tabs: impl Fn() + 'static,
) {
    let indicator = indicator.unwrap_or_else(|| primary_default_indicator(selected_tab_index));
    tab_row(modifier, container_color, content_color, indicator, divider, tabs);
}

/// A row of tabs, as Compose's `SecondaryTabRow`: `PrimaryTabRow`'s layout, its tabs on the
/// surface by default, its indicator (`None`) a 3 dp line as wide as the selected tab.
#[composable]
#[allow(non_snake_case)]
pub fn SecondaryTabRow(
    selected_tab_index: usize,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TabRowDefaults::secondary_container_color())] container_color: Color,
    #[default(crate::TabRowDefaults::secondary_content_color())] content_color: Color,
    #[default(None)] indicator: Option<impl Fn(TabIndicatorScope) + 'static>,
    #[default(crate::default_tab_divider())] divider: impl Fn() + 'static,
    tabs: impl Fn() + 'static,
) {
    let indicator = indicator.unwrap_or_else(|| secondary_default_indicator(selected_tab_index));
    tab_row(modifier, container_color, content_color, indicator, divider, tabs);
}

/// The fixed rows' body, as Compose's `TabRowImpl`.
fn tab_row(
    modifier: Modifier,
    container_color: Color,
    content_color: Color,
    indicator: Callback<dyn Fn(TabIndicatorScope)>,
    divider: Callback,
    tabs: Callback,
) {
    let scope = remember(|| TabIndicatorScope { positions: Rc::new(RefCell::new(Vec::new())) });
    let policy = {
        let positions = scope.positions.clone();
        remember(move || Rc::new(FixedTabRowLayout { positions }) as Rc<dyn MeasurePolicy>)
    };
    Surface(
        modifier.selectable_group(),
        compose_ui::RECTANGLE_SHAPE,
        container_color,
        content_color,
        Dp(0.0),
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            let (scope, indicator, divider, tabs) =
                (scope.clone(), indicator.clone(), divider.clone(), tabs.clone());
            Layout(
                Modifier.fill_max_width(),
                policy.clone(),
                compose_runtime::callback(move || {
                    tabs();
                    let divider = divider.clone();
                    Box(
                        Modifier,
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| divider()),
                    );
                    let (scope, indicator) = (scope.clone(), indicator.clone());
                    Box(
                        Modifier,
                        Alignment::TOP_START,
                        true,
                        compose_runtime::callback(move |_| indicator(scope.clone())),
                    );
                }),
            );
        }),
    );
}

/// The padding around a tab's text, as `HorizontalTextPadding`.
const HORIZONTAL_TEXT_PADDING: Dp = Dp(16.0);
/// The narrowest indicator under a tab's content.
const MIN_CONTENT_INDICATOR_WIDTH: Dp = Dp(24.0);

/// As Compose's `TabRowImpl`'s measure policy: the tabs, then the divider and the indicator
/// (each in a box of its own).
struct FixedTabRowLayout {
    positions: Rc<RefCell<Vec<TabPosition>>>,
}

impl MeasurePolicy for FixedTabRowLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [tabs @ .., divider, indicator] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let density = scope.density();
        let row_width = constraints.max_width;
        let count = tabs.len() as i32;
        let tab_width = if count > 0 { row_width / count } else { 0 };
        let row_height =
            tabs.iter().map(|&tab| scope.max_intrinsic_height(tab, tab_width)).max().unwrap_or(0);
        let to_dp = |px: i32| Dp(px as f32 / density.density);
        let positions: Vec<TabPosition> = tabs
            .iter()
            .enumerate()
            .map(|(index, &tab)| {
                let content = scope.max_intrinsic_width(tab, row_height).min(tab_width);
                let content_width = Dp((to_dp(content).0 - 2.0 * HORIZONTAL_TEXT_PADDING.0)
                    .max(MIN_CONTENT_INDICATOR_WIDTH.0));
                TabPosition {
                    left: to_dp(tab_width * index as i32),
                    width: to_dp(tab_width),
                    content_width,
                }
            })
            .collect();
        *self.positions.borrow_mut() = positions;
        let tab_constraints = Constraints::new(tab_width, tab_width, row_height, row_height);
        let placeables: Vec<_> =
            tabs.iter().map(|&tab| scope.measure(tab, tab_constraints)).collect();
        let divider = scope.measure(
            *divider,
            Constraints::new(
                constraints.min_width,
                constraints.max_width,
                0,
                constraints.max_height,
            ),
        );
        let indicator =
            scope.measure(*indicator, Constraints::new(tab_width, tab_width, 0, row_height));
        for (index, tab) in placeables.iter().enumerate() {
            scope.place_relative(tab, index as i32 * tab_width, 0);
        }
        scope.place_relative(&divider, 0, row_height - divider.height);
        scope.place_relative(&indicator, 0, row_height - indicator.height);
        scope.layout(row_width, row_height)
    }

    fn debug_name(&self) -> &'static str {
        "TabRow"
    }
}

/// A row of tabs that scrolls, as Compose's `PrimaryScrollableTabRow`: each tab as wide as it
/// needs (at least `min_tab_width`), `edge_padding` before the first and after the last; the
/// selected tab scrolled towards the middle (with the standard spatial spring) when it changes,
/// and when first laid out; `indicator` by default a pill as wide as the selected tab's content.
#[composable]
#[allow(non_snake_case)]
pub fn PrimaryScrollableTabRow(
    selected_tab_index: usize,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_scroll_state(0))] scroll_state: ScrollState,
    #[default(crate::TabRowDefaults::primary_container_color())] container_color: Color,
    #[default(crate::TabRowDefaults::primary_content_color())] content_color: Color,
    #[default(crate::TabRowDefaults::SCROLLABLE_TAB_ROW_EDGE_START_PADDING)] edge_padding: Dp,
    #[default(None)] indicator: Option<impl Fn(TabIndicatorScope) + 'static>,
    #[default(crate::default_tab_divider())] divider: impl Fn() + 'static,
    #[default(crate::TabRowDefaults::SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH)] min_tab_width: Dp,
    tabs: impl Fn() + 'static,
) {
    let indicator = indicator.unwrap_or_else(|| primary_default_indicator(selected_tab_index));
    scrollable_tab_row(
        selected_tab_index,
        modifier,
        scroll_state,
        container_color,
        content_color,
        edge_padding,
        indicator,
        divider,
        min_tab_width,
        tabs,
    );
}

/// A row of tabs that scrolls, as Compose's `SecondaryScrollableTabRow`:
/// `PrimaryScrollableTabRow`'s layout with the secondary colors and indicator.
#[composable]
#[allow(non_snake_case)]
pub fn SecondaryScrollableTabRow(
    selected_tab_index: usize,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::remember_scroll_state(0))] scroll_state: ScrollState,
    #[default(crate::TabRowDefaults::secondary_container_color())] container_color: Color,
    #[default(crate::TabRowDefaults::secondary_content_color())] content_color: Color,
    #[default(crate::TabRowDefaults::SCROLLABLE_TAB_ROW_EDGE_START_PADDING)] edge_padding: Dp,
    #[default(None)] indicator: Option<impl Fn(TabIndicatorScope) + 'static>,
    #[default(crate::default_tab_divider())] divider: impl Fn() + 'static,
    #[default(crate::TabRowDefaults::SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH)] min_tab_width: Dp,
    tabs: impl Fn() + 'static,
) {
    let indicator = indicator.unwrap_or_else(|| secondary_default_indicator(selected_tab_index));
    scrollable_tab_row(
        selected_tab_index,
        modifier,
        scroll_state,
        container_color,
        content_color,
        edge_padding,
        indicator,
        divider,
        min_tab_width,
        tabs,
    );
}

/// The scrollable rows' body, as Compose's `ScrollableTabRowImpl`.
#[allow(clippy::too_many_arguments)]
fn scrollable_tab_row(
    selected_tab_index: usize,
    modifier: Modifier,
    scroll_state: ScrollState,
    container_color: Color,
    content_color: Color,
    edge_padding: Dp,
    indicator: Callback<dyn Fn(TabIndicatorScope)>,
    divider: Callback,
    min_tab_width: Dp,
    tabs: Callback,
) {
    let scope = remember(|| TabIndicatorScope { positions: Rc::new(RefCell::new(Vec::new())) });
    let coroutines = compose_runtime::remember_coroutine_scope();
    let last_selected = remember(|| Rc::new(Cell::new(None::<usize>)));
    let policy = {
        let positions = scope.positions.clone();
        compose_runtime::remember_keyed(
            (selected_tab_index, edge_padding, min_tab_width, scroll_state),
            move || {
                Rc::new(ScrollableTabRowLayout {
                    positions,
                    selected_tab_index,
                    edge_padding,
                    min_tab_width,
                    scroll_state,
                    coroutines,
                    last_selected,
                }) as Rc<dyn MeasurePolicy>
            },
        )
    };
    Surface(
        modifier,
        compose_ui::RECTANGLE_SHAPE,
        container_color,
        content_color,
        Dp(0.0),
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            let (scope, indicator, divider, tabs, policy) =
                (scope.clone(), indicator.clone(), divider.clone(), tabs.clone(), policy.clone());
            Box(
                Modifier,
                Alignment::BOTTOM_START,
                false,
                compose_runtime::callback(move |_| {
                    divider();
                    let (scope, indicator, tabs) = (scope.clone(), indicator.clone(), tabs.clone());
                    Layout(
                        Modifier
                            .fill_max_width()
                            .wrap_content_size_with(Alignment::CENTER_START, false)
                            .horizontal_scroll(scroll_state)
                            .selectable_group()
                            .clip_to_bounds(),
                        policy.clone(),
                        compose_runtime::callback(move || {
                            tabs();
                            let (scope, indicator) = (scope.clone(), indicator.clone());
                            Box(
                                Modifier,
                                Alignment::TOP_START,
                                true,
                                compose_runtime::callback(move |_| indicator(scope.clone())),
                            );
                        }),
                    );
                }),
            );
        }),
    );
}

/// As Compose's `ScrollableTabRowImpl`'s measure policy and `ScrollableTabData`.
struct ScrollableTabRowLayout {
    positions: Rc<RefCell<Vec<TabPosition>>>,
    selected_tab_index: usize,
    edge_padding: Dp,
    min_tab_width: Dp,
    scroll_state: ScrollState,
    coroutines: CoroutineScope,
    /// The selection the row last scrolled to (none before the first layout).
    last_selected: Rc<Cell<Option<usize>>>,
}

impl MeasurePolicy for ScrollableTabRowLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [tabs @ .., indicator] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let density = scope.density();
        let padding = density.round_to_px(self.edge_padding);
        let height = tabs
            .iter()
            .map(|&tab| scope.max_intrinsic_height(tab, Constraints::INFINITY))
            .max()
            .unwrap_or(0);
        // The contents' widths first: an intrinsic probe measures a node, so it comes before the
        // measurement it would otherwise replace.
        let contents: Vec<i32> =
            tabs.iter().map(|&tab| scope.max_intrinsic_width(tab, Constraints::INFINITY)).collect();
        let min_tab = density.round_to_px(self.min_tab_width);
        let tab_constraints =
            Constraints::new(min_tab, constraints.max_width.max(min_tab), height, height);
        let placeables: Vec<_> =
            tabs.iter().map(|&tab| scope.measure(tab, tab_constraints)).collect();
        let to_dp = |px: i32| Dp(px as f32 / density.density);
        let mut left = self.edge_padding;
        let mut layout_width = padding * 2;
        let positions: Vec<TabPosition> = contents
            .iter()
            .zip(&placeables)
            .map(|(&content, placeable)| {
                let width = Dp(self.min_tab_width.0.max(to_dp(placeable.width).0));
                layout_width += density.round_to_px(width);
                let content_width = Dp((to_dp(content).0 - 2.0 * HORIZONTAL_TEXT_PADDING.0)
                    .max(MIN_CONTENT_INDICATOR_WIDTH.0));
                let position = TabPosition { left, width, content_width };
                left = left + width;
                position
            })
            .collect();
        let selected = positions.get(self.selected_tab_index).copied();
        // The indicator places itself from the positions.
        *self.positions.borrow_mut() = positions.clone();
        let indicator = scope.measure(
            *indicator,
            Constraints::new(
                0,
                selected.map_or(0, |it| density.round_to_px(it.content_width)),
                0,
                height,
            ),
        );
        let mut x = self.edge_padding;
        for (placeable, position) in placeables.iter().zip(&positions) {
            scope.place_relative(placeable, density.round_to_px(x), 0);
            x = x + position.width;
        }
        if let Some(selected) = selected {
            let offset = ((density.round_to_px(selected.width) - indicator.width) / 2).max(0);
            scope.place_relative(&indicator, offset, height - indicator.height);
        }
        // Scrolls to a new selection (or the first one), as `ScrollableTabData.onLaidOut`: once
        // laid out (the scroll's maximum is the scroll node's, set after this measure, where
        // Compose's placement runs after it).
        if self.last_selected.get() != Some(self.selected_tab_index) {
            self.last_selected.set(Some(self.selected_tab_index));
            if let (Some(selected), Some(last)) = (selected, positions.last()) {
                let total = density.round_to_px(last.right()) + padding;
                let tab_offset = density.round_to_px(selected.left);
                let tab_width = density.round_to_px(selected.width);
                let scroll = self.scroll_state;
                self.coroutines.launch(async move {
                    compose_runtime::with_frame_nanos(|_| ()).await;
                    let visible = total - scroll.max_value();
                    let centered = tab_offset - (visible / 2 - tab_width / 2);
                    let target = centered.clamp(0, (total - visible).max(0));
                    if scroll.value() != target {
                        animate_scroll_to(scroll, target).await;
                    }
                });
            }
        }
        scope.layout(layout_width, height)
    }

    fn debug_name(&self) -> &'static str {
        "ScrollableTabRow"
    }
}

/// Scrolls `scroll` to `target` with the standard spatial spring, as `animateScrollTo`.
async fn animate_scroll_to(scroll: ScrollState, target: i32) {
    let position = Animatable::new(scroll.value() as f32);
    let follow = position.clone();
    let mover = async move {
        position.animate_to(target as f32, AnimationSpec::Spring(motion::DEFAULT_SPATIAL)).await;
    };
    // Each frame of the animation scrolls to its value.
    let stepper = async move {
        loop {
            compose_runtime::with_frame_nanos(|_| ()).await;
            scroll.scroll_to(follow.value().round() as i32);
            if !follow.is_running() {
                break;
            }
        }
    };
    futures_join(mover, stepper).await;
}

/// Runs both futures to completion.
async fn futures_join(
    first: impl std::future::Future<Output = ()>,
    second: impl std::future::Future<Output = ()>,
) {
    let mut first = std::boxed::Box::pin(first);
    let mut second = std::boxed::Box::pin(second);
    let (mut first_done, mut second_done) = (false, false);
    std::future::poll_fn(move |context| {
        if !first_done {
            first_done = first.as_mut().poll(context).is_ready();
        }
        if !second_done {
            second_done = second.as_mut().poll(context).is_ready();
        }
        if first_done && second_done {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
}

/// A tab's height with its text or its icon, as `SmallTabHeight`, and with both, as
/// `LargeTabHeight`.
const SMALL_TAB_HEIGHT: Dp = Dp(48.0);
const LARGE_TAB_HEIGHT: Dp = Dp(72.0);

/// A tab's content color, following its selection, as Compose's `TabTransition`: towards the
/// selected color with the standard default-effects spring, away from it with the fast one.
fn tab_color(selected: bool, selected_color: Color, unselected_color: Color) -> Color {
    let (target, spec) = if selected {
        (selected_color, motion::DEFAULT_EFFECTS)
    } else {
        (unselected_color, motion::FAST_EFFECTS)
    };
    compose_animation::animate_color_as_state(target, AnimationSpec::Spring(spec), "TabColor", None)
        .get()
}

/// A tab's click: selectable, a tab's role, the state layer in the selected color.
fn tab_modifier(
    modifier: Modifier,
    selected: bool,
    on_click: Callback,
    enabled: bool,
    selected_content_color: Color,
    interaction_source: Option<MutableInteractionSource>,
) -> Modifier {
    modifier.selectable_with(
        selected,
        ClickableOptions {
            enabled,
            role: Some(Role::Tab),
            indication: selected_content_color.into(),
            interaction_source,
        },
        on_click,
    )
}

/// A tab, as Compose's `Tab(selected, onClick, …, text, icon, …)`: its text (title small,
/// centered, 16 dp from its sides) and icon centered, 48 dp high (72 dp with both); its content
/// in `selected_content_color` when selected, else `unselected_content_color` (by default the
/// same), animated; clicking it selects it.
#[composable]
#[allow(non_snake_case)]
pub fn Tab(
    selected: bool,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] text: Option<impl Fn() + 'static>,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(crate::LOCAL_CONTENT_COLOR.current())] selected_content_color: Color,
    #[default(crate::Color::UNSPECIFIED)] unselected_content_color: Color,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let style = centered(MaterialTheme::typography().title_small);
    let height = if text.is_some() && icon.is_some() { LARGE_TAB_HEIGHT } else { SMALL_TAB_HEIGHT };
    let content = compose_runtime::callback(move |_: ColumnScope| {
        let (text, icon) = (text.clone(), icon.clone());
        // At least the tab's height, the text and icon centered, as `TabBaselineLayout` (which
        // places a text with an icon by their baselines: compose-rs centers them).
        Column(
            Modifier.size_in(SizeSpec::bounds(
                Dp::UNSPECIFIED,
                height,
                Dp::UNSPECIFIED,
                Dp::UNSPECIFIED,
            )),
            Arrangement::CENTER.into(),
            Alignment::CENTER_HORIZONTALLY,
            compose_runtime::callback(move |_| {
                if let Some(icon) = icon.clone() {
                    icon();
                }
                if let Some(text) = text.clone() {
                    CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), || {
                        let text = text.clone();
                        Box(
                            Modifier.padding((HORIZONTAL_TEXT_PADDING, 0.dp())),
                            Alignment::CENTER,
                            false,
                            compose_runtime::callback(move |_| text()),
                        );
                    });
                }
            }),
        );
    });
    ContentTab(
        selected,
        on_click,
        modifier,
        enabled,
        selected_content_color,
        unselected_content_color,
        interaction_source,
        content,
    );
}

/// The style centered, as `textAlign = TextAlign.Center`.
fn centered(style: compose_ui::TextStyle) -> compose_ui::TextStyle {
    compose_ui::TextStyle { text_align: Some(TextAlign::Center), ..style }
}

/// A tab of any content, as Compose's `Tab(selected, onClick, …, content)`: `content` in a
/// column centered in the tab, in the selected or unselected color, animated.
#[composable]
#[allow(non_snake_case)]
pub fn ContentTab(
    selected: bool,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::LOCAL_CONTENT_COLOR.current())] selected_content_color: Color,
    #[default(crate::Color::UNSPECIFIED)] unselected_content_color: Color,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(ColumnScope) + 'static,
) {
    let unselected = unselected_content_color.or(selected_content_color);
    let color = tab_color(selected, selected_content_color, unselected);
    let modifier = tab_modifier(
        modifier,
        selected,
        on_click,
        enabled,
        selected_content_color,
        interaction_source,
    )
    .fill_max_width();
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        Column(
            modifier.clone(),
            Arrangement::CENTER.into(),
            Alignment::CENTER_HORIZONTALLY,
            content.clone(),
        );
    });
}

/// A tab with its icon before its text, as Compose's `LeadingIconTab`: 48 dp high, the icon
/// 8 dp before the text (title small), both centered, 16 dp from the tab's sides.
#[composable]
#[allow(non_snake_case)]
pub fn LeadingIconTab(
    selected: bool,
    on_click: impl Fn() + 'static,
    text: impl Fn() + 'static,
    icon: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::LOCAL_CONTENT_COLOR.current())] selected_content_color: Color,
    #[default(crate::Color::UNSPECIFIED)] unselected_content_color: Color,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let unselected = unselected_content_color.or(selected_content_color);
    let color = tab_color(selected, selected_content_color, unselected);
    let style = centered(MaterialTheme::typography().title_small);
    let modifier = tab_modifier(
        modifier.height(SMALL_TAB_HEIGHT),
        selected,
        on_click,
        enabled,
        selected_content_color,
        interaction_source,
    )
    .padding((HORIZONTAL_TEXT_PADDING, 0.dp()))
    .fill_max_width();
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        let (text, icon) = (text.clone(), icon.clone());
        Row(
            modifier.clone(),
            Arrangement::CENTER.into(),
            Alignment::CENTER_VERTICALLY,
            compose_runtime::callback(move |_| {
                icon();
                Spacer(Modifier.size_in(SizeSpec::exact(8.dp(), Dp::UNSPECIFIED, true)));
                CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), &*text);
            }),
        );
    });
}
