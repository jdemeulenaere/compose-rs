//! `TabRow`, `ScrollableTabRow` and `Tab`, as Compose's Material 3: tabs sharing a row's width
//! (or scrolling), the selected one underlined by an indicator that slides to it.

use std::cell::RefCell;
use std::rc::Rc;

use compose_animation::{Animatable, FAST_OUT_SLOW_IN_EASING, tween};
use compose_foundation::layout::{Arrangement, Box, Column};
use compose_foundation::{ScrollModifiers, ScrollState};
use compose_runtime::{CompositionLocalProvider, LaunchedEffect, composable, remember};
use compose_ui::{
    Alignment, Color, Constraints, Dp, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, Offset, Paint, Rect, Role, SizeSpec, UnitExt,
};

use crate::components::Surface;
use crate::scaffold::HorizontalDivider;
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// As Compose's `TabRowDefaults`.
pub struct TabRowDefaults;

impl TabRowDefaults {
    /// The indicator's height, as `TabRowDefaults.Indicator`'s.
    pub const INDICATOR_HEIGHT: Dp = Dp(3.0);
    /// Before the first scrollable tab and after the last, as
    /// `ScrollableTabRowEdgeStartPadding`.
    pub const SCROLLABLE_TAB_ROW_EDGE_START_PADDING: Dp = Dp(52.0);

    /// The row's color, as `TabRowDefaults.containerColor` (the surface).
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface
    }

    /// The tabs' color, as `TabRowDefaults.contentColor` (the primary color).
    pub fn content_color() -> Color {
        MaterialTheme::color_scheme().primary
    }
}
/// A row of tabs, as Compose's `TabRow`: each tab an equal share of its width, a divider along
/// its bottom, and a 3 dp indicator in the content color under the selected tab, sliding to a
/// newly selected one (250 ms).
#[composable]
#[allow(non_snake_case)]
pub fn TabRow(
    selected_tab_index: usize,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TabRowDefaults::container_color())] container_color: Color,
    #[default(crate::TabRowDefaults::content_color())] content_color: Color,
    tabs: impl Fn() + 'static,
) {
    tab_row(selected_tab_index, modifier, container_color, content_color, None, tabs);
}

/// A row of tabs that scrolls, as Compose's `ScrollableTabRow`: each tab as wide as it needs
/// (at least 90 dp), `edge_padding` (52 dp) before the first and after the last; the selected
/// tab scrolled to the middle (as far as it goes) when the selection changes, over 250 ms.
#[composable]
#[allow(non_snake_case)]
pub fn ScrollableTabRow(
    selected_tab_index: usize,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TabRowDefaults::container_color())] container_color: Color,
    #[default(crate::TabRowDefaults::content_color())] content_color: Color,
    #[default(crate::TabRowDefaults::SCROLLABLE_TAB_ROW_EDGE_START_PADDING)] edge_padding: Dp,
    tabs: impl Fn() + 'static,
) {
    tab_row(selected_tab_index, modifier, container_color, content_color, Some(edge_padding), tabs);
}

/// The tab rows' body: fixed (`edge_padding` `None`) or scrollable.
fn tab_row(
    selected_tab_index: usize,
    modifier: Modifier,
    container_color: Color,
    content_color: Color,
    edge_padding: Option<Dp>,
    tabs: compose_runtime::Callback,
) {
    // Each tab's left and width, as the last layout placed them. Not state: the row's layout
    // writes them and the same node's drawing reads them, so measuring the row draws it again
    // (a reader in another repaint boundary would draw stale positions, docs/06).
    let positions = remember(|| Rc::new(RefCell::new(Vec::<(f32, f32)>::new())));
    // The indicator while it slides: left in `x`, width in `y`.
    let indicator = remember(|| Animatable::new(Offset::new(f32::NAN, 0.0)));
    // The tab it rests under (where it's drawn, as laid out, when not sliding), and whether
    // it's at rest (else it's where the animation left it).
    let resting_at = remember(|| Rc::new(std::cell::Cell::new(None::<usize>)));
    let at_rest = remember(|| Rc::new(std::cell::Cell::new(true)));
    let scroll = remember(|| ScrollState::new(0));
    // Scrolls to the selected tab; cancelled with the row.
    let scroll_scope = compose_runtime::remember_coroutine_scope();
    let scroll_job = remember(|| Rc::new(RefCell::new(None::<compose_runtime::Job>)));
    {
        let (positions, indicator, resting_at, at_rest) =
            (positions.clone(), indicator.clone(), resting_at.clone(), at_rest.clone());
        let (scroll, scroll_scope, scroll_job) = (scroll, scroll_scope, scroll_job.clone());
        let density = compose_ui::LOCAL_DENSITY.current();
        LaunchedEffect(selected_tab_index, async move {
            // After the layout that placed the tabs.
            compose_runtime::with_frame_nanos(|_| ()).await;
            let position = |index: usize| positions.borrow().get(index).copied();
            let Some((left, width)) = position(selected_tab_index) else { return };
            if let Some(edge) = edge_padding {
                // The selected tab to the middle, as far as the row scrolls, as Compose's
                // `ScrollableTabData`.
                let last = positions.borrow().last().map_or(0.0, |&(left, width)| left + width);
                let total = last + density.to_px(edge);
                let visible = total - scroll.max_value() as f32;
                let centered = left - (visible / 2.0 - width / 2.0);
                let target = centered.clamp(0.0, (total - visible).max(0.0)).round() as i32;
                let job = scroll_scope.launch(async move {
                    scroll_with_tween(&scroll, target).await;
                });
                // A new selection's scroll replaces the last one's.
                if let Some(last) = scroll_job.replace(Some(job)) {
                    last.cancel();
                }
            }
            let target = Offset::new(left, width);
            match resting_at.get() {
                Some(from) if from != selected_tab_index => {
                    // From where it's drawn: under its tab as now laid out, or mid-slide.
                    if at_rest.get()
                        && let Some((left, width)) = position(from)
                    {
                        indicator.snap_to(Offset::new(left, width));
                    }
                    at_rest.set(false);
                    indicator.animate_to(target, tween(250).easing(FAST_OUT_SLOW_IN_EASING)).await;
                }
                _ => indicator.snap_to(target),
            }
            resting_at.set(Some(selected_tab_index));
            at_rest.set(true);
        });
    }
    let policy = {
        let positions = positions.clone();
        compose_runtime::remember_keyed(edge_padding, move || {
            Rc::new(TabRowLayout { positions, edge_padding }) as Rc<dyn MeasurePolicy>
        })
    };
    let draw_positions = positions.clone();
    let (draw_resting, draw_rest) = (resting_at.clone(), at_rest.clone());
    Surface(
        modifier,
        compose_ui::RECTANGLE_SHAPE,
        container_color,
        content_color,
        Dp(0.0),
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            let (draw_positions, indicator, tabs) =
                (draw_positions.clone(), indicator.clone(), tabs.clone());
            let (draw_resting, draw_rest) = (draw_resting.clone(), draw_rest.clone());
            let row = Modifier.draw_with_content(move |scope| {
                scope.draw_content();
                // Sliding, where the animation is; at rest, under its tab as laid out (until
                // a new selection's slide starts: the old one's).
                let (left, width) = if !draw_rest.get() || indicator.is_running() {
                    let value = indicator.value();
                    (value.x, value.y)
                } else {
                    let index = draw_resting.get().unwrap_or(selected_tab_index);
                    match draw_positions.borrow().get(index) {
                        Some(&position) => position,
                        None => return,
                    }
                };
                let height = scope.density().to_px(TabRowDefaults::INDICATOR_HEIGHT);
                let bottom = scope.size().height;
                scope.draw_rect(
                    Rect::new(left, bottom - height, left + width, bottom),
                    Paint::fill(content_color),
                );
            });
            let row = match edge_padding {
                None => Modifier.fill_max_width().then(row),
                Some(_) => Modifier.fill_max_width().horizontal_scroll(scroll).then(row),
            };
            Layout(
                row,
                policy.clone(),
                compose_runtime::callback(move || {
                    tabs();
                    HorizontalDivider(
                        Modifier,
                        Dp(1.0),
                        MaterialTheme::color_scheme().outline_variant,
                    );
                }),
            );
        }),
    );
}

/// Scrolls `scroll` to `target` over 250 ms, eased, as Compose's `ScrollableTabRowScrollSpec`.
async fn scroll_with_tween(scroll: &ScrollState, target: i32) {
    let start = scroll.value();
    if start == target {
        return;
    }
    let began = compose_runtime::with_frame_nanos(|time| time).await;
    loop {
        let now = compose_runtime::with_frame_nanos(|time| time).await;
        let fraction = ((now - began) as f32 / 250_000_000.0).min(1.0);
        let eased = FAST_OUT_SLOW_IN_EASING.transform(fraction);
        scroll.scroll_to(start + ((target - start) as f32 * eased).round() as i32);
        if fraction >= 1.0 {
            break;
        }
    }
}

/// Tabs, then the divider. Fixed: each tab an equal share of the width, as tall as the
/// tallest. Scrollable: each tab as wide as it needs (at least 90 dp), after the edge padding,
/// the row as wide as they are (at least as wide as allowed). The divider along the bottom.
struct TabRowLayout {
    positions: Rc<RefCell<Vec<(f32, f32)>>>,
    edge_padding: Option<Dp>,
}

impl MeasurePolicy for TabRowLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let Some((&divider, tabs)) = measurables.split_last() else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let density = scope.density();
        let (placeables, edge, height) = match self.edge_padding {
            None => {
                let width = constraints.max_width;
                let tab_width = if tabs.is_empty() { 0 } else { width / tabs.len() as i32 };
                // Every tab as tall as the tallest, as Compose's (`maxIntrinsicHeight`).
                let height = constraints.constrain_height(
                    tabs.iter()
                        .map(|&tab| scope.max_intrinsic_height(tab, tab_width))
                        .max()
                        .unwrap_or(0),
                );
                let placeables: Vec<_> = tabs
                    .iter()
                    .map(|&tab| {
                        scope.measure(tab, Constraints::new(tab_width, tab_width, height, height))
                    })
                    .collect();
                (placeables, 0, height)
            }
            Some(edge) => {
                let minimum = density.round_to_px(Dp(90.0));
                let widths: Vec<i32> = tabs
                    .iter()
                    .map(|&tab| scope.max_intrinsic_width(tab, Constraints::INFINITY).max(minimum))
                    .collect();
                let height = tabs
                    .iter()
                    .zip(&widths)
                    .map(|(&tab, &width)| scope.max_intrinsic_height(tab, width))
                    .max()
                    .unwrap_or(0);
                let placeables: Vec<_> = tabs
                    .iter()
                    .zip(&widths)
                    .map(|(&tab, &width)| {
                        scope.measure(tab, Constraints::new(width, width, height, height))
                    })
                    .collect();
                (placeables, density.round_to_px(edge), height)
            }
        };
        let content: i32 = placeables.iter().map(|tab| tab.width).sum();
        let width = if self.edge_padding.is_some() {
            (content + 2 * edge).max(constraints.min_width)
        } else {
            constraints.max_width
        };
        let mut positions = self.positions.borrow_mut();
        positions.clear();
        let mut x = edge;
        for tab in &placeables {
            scope.place_relative(tab, x, 0);
            // Where it ends up, mirrored in a right-to-left row.
            let left = if scope.layout_direction() == compose_ui::LayoutDirection::Rtl {
                width - x - tab.width
            } else {
                x
            };
            positions.push((left as f32, tab.width as f32));
            x += tab.width;
        }
        let divider = scope.measure(divider, Constraints::new(width, width, 0, height));
        scope.place_relative(&divider, 0, height - divider.height);
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "TabRow"
    }
}

/// A tab, as Compose's `Tab`: its text (title-small) and icon centered, 48 dp high (72 dp
/// with both), in `selected_content_color` when selected; clicking it selects it.
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
) {
    // As Compose's: unselected tabs in the selected color unless told otherwise.
    let unselected = unselected_content_color.or(selected_content_color);
    let color = if selected { selected_content_color } else { unselected };
    let color = if enabled { color } else { color.with_alpha(0.38) };
    let height = if text.is_some() && icon.is_some() { 72.dp() } else { 48.dp() };
    let typography = MaterialTheme::typography();
    let modifier = modifier
        .fill_max_width()
        .size_in(SizeSpec::bounds(Dp::UNSPECIFIED, height, Dp::UNSPECIFIED, Dp::UNSPECIFIED))
        .clickable_indicated(enabled, Some(Role::Tab), selected_content_color, on_click)
        .semantics(false, move |config| config.selected = Some(selected));
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(typography.title_small), || {
            let (text, icon) = (text.clone(), icon.clone());
            Column(
                modifier.clone(),
                Arrangement::Center,
                Alignment::CENTER_HORIZONTALLY,
                compose_runtime::callback(move |_| {
                    if let Some(icon) = icon.clone() {
                        icon();
                    }
                    if let Some(text) = text.clone() {
                        Box(
                            Modifier.padding_horizontal_vertical(16.dp(), 0.dp()),
                            Alignment::CENTER,
                            false,
                            compose_runtime::callback(move |_| text()),
                        );
                    }
                }),
            );
        });
    });
}
