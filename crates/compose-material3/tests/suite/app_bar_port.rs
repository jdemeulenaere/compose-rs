//! More of Compose's `AppBarTest`, at density 1 in a 400 by 800 window: what's under `Scaffold`s,
//! the colors over each kind of scrollable, the rows' places with their baselines.
//! `captureToImage().assertContainsColor` reads as the display list holding
//! a fill of the color.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::animate_color_as_state;
use compose_foundation::gestures::ScrollableState as _;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::platform::{FixedFontMetrics, FixedTextMeasurer};
use compose_ui::{Color, Density, IntSize, Modifier, Offset, PaddingValues, UnitExt};

const BAR: &str = "bar";
const CONTENT: &str = "content";
const TITLE: &str = "title";
const SUBTITLE: &str = "subtitle";
const NAVIGATION: &str = "navigation";
const ACTIONS: &str = "actions";

fn no_insets() -> WindowInsets {
    WindowInsets::new(0.dp(), 0.dp(), 0.dp(), 0.dp())
}

fn window(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 800), content)
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag_unmerged(tag).fetch_semantics_snapshot().bounds
}

fn fills(ui: &mut ComposeTestRule) -> Vec<Color> {
    let list = ui.host().display_list();
    list.ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(_, paint) => Some(paint.color),
            _ => None,
        })
        .collect()
}

#[composable]
fn FakeIcon(tag: &'static str) {
    Spacer(Modifier.size(24.dp()).test_tag(tag));
}

// ---- The colors over what scrolls ---------------------------------------------------------------

macro_rules! compose_arg {
    ($($name:ident),*) => {
        $(impl ComposeArg for $name {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Pinned,
    EnterAlways,
}

#[derive(Clone, Copy, PartialEq)]
enum Body {
    Column,
    List,
    Grid,
    TallGrid,
}

compose_arg!(Kind, Body);

const RED_THEN_GREEN: fn() -> TopAppBarColors = || TopAppBarColors {
    container_color: Color::RED,
    scrolled_container_color: Color::GREEN,
    ..TopAppBarDefaults::top_app_bar_colors()
};

/// `Column` with `reverse` and a scroll of `initial`, or a list or grid first at item `initial`,
/// under a bar going red to green.
#[composable]
fn ColorScreen(kind: Kind, body: Body, reverse: bool, initial: i32) {
    MaterialTheme!({
        let scroll = remember_scroll_state!(initial);
        let list = remember_lazy_list_state!(initial as usize, 0);
        let grid = remember_lazy_grid_state!(initial as usize, 0);
        let behavior = match (kind, body) {
            (Kind::Pinned, Body::Column) => TopAppBarDefaults::pinned_scroll_behavior_for(&scroll),
            (Kind::Pinned, Body::List) => TopAppBarDefaults::pinned_scroll_behavior_for(&list),
            (Kind::Pinned, Body::Grid | Body::TallGrid) => {
                TopAppBarDefaults::pinned_scroll_behavior_for(&grid)
            }
            (Kind::EnterAlways, Body::Column) => {
                TopAppBarDefaults::enter_always_scroll_behavior_for(&scroll)
            }
            (Kind::EnterAlways, Body::List) => {
                TopAppBarDefaults::enter_always_scroll_behavior_for(&list)
            }
            (Kind::EnterAlways, Body::Grid | Body::TallGrid) => {
                TopAppBarDefaults::enter_always_scroll_behavior_for(&grid)
            }
        };
        let connection = behavior.nested_scroll_connection();
        Scaffold!(
            modifier = Modifier.fill_max_size(),
            top_bar = {
                TopAppBar!(
                    { Text!("Title") },
                    modifier = Modifier.test_tag(BAR),
                    scroll_behavior = Some(behavior.clone()),
                    colors = RED_THEN_GREEN()
                );
            },
            |padding: PaddingValues| {
                let modifier = Modifier
                    .fill_max_size()
                    .test_tag(CONTENT)
                    .padding(padding)
                    .nested_scroll(connection.clone());
                match body {
                    Body::Column => {
                        Column!(
                            modifier = modifier.vertical_scroll_with(scroll, true, None, reverse),
                            vertical_arrangement = Arrangement::BOTTOM,
                            {
                                for _ in 0..50 {
                                    Spacer(Modifier.fill_max_width().height(80.dp()));
                                }
                            }
                        );
                    }
                    Body::List => {
                        LazyColumn!(
                            modifier = modifier,
                            state = list,
                            reverse_layout = reverse,
                            |scope: LazyListScope| {
                                scope.items(100, |index| {
                                    Text!("{index}", modifier = Modifier.height(20.dp()));
                                });
                            }
                        );
                    }
                    Body::Grid | Body::TallGrid => {
                        // As long in pixels as Compose's is at its density, for a fling to end in it.
                        let (count, item) =
                            if body == Body::TallGrid { (400, 100.dp()) } else { (100, 50.dp()) };
                        LazyVerticalGrid!(
                            GridCells::Adaptive(100.dp()),
                            modifier = modifier,
                            state = grid,
                            reverse_layout = reverse,
                            |scope: LazyGridScope| {
                                scope.items(count, move |_| {
                                    Spacer(Modifier.fill_max_width().height(item));
                                });
                            }
                        );
                    }
                }
            }
        );
    });
}

fn color_screen(kind: Kind, body: Body, reverse: bool, initial: i32) -> ComposeTestRule {
    let mut ui = window(move || ColorScreen(kind, body, reverse, initial));
    ui.wait_for_idle();
    ui
}

fn assert_has(ui: &mut ComposeTestRule, color: Color) {
    let colors = fills(ui);
    assert!(colors.contains(&color), "no {color:?} in {colors:?}");
}

fn swipe_up(ui: &mut ComposeTestRule, from: f32, to: f32) {
    ui.on_node_with_tag(CONTENT).perform_touch_input(|touch| touch.swipe_up_with(from, to, 200));
    ui.wait_for_idle();
}

fn swipe_down(ui: &mut ComposeTestRule, from: f32, to: f32) {
    ui.on_node_with_tag(CONTENT).perform_touch_input(|touch| touch.swipe_down_with(from, to, 200));
    ui.wait_for_idle();
}

/// Red at the start, green once it scrolled.
fn turns_green_on_a_swipe_up(kind: Kind, body: Body) {
    let mut ui = color_screen(kind, body, false, 0);
    assert_has(&mut ui, Color::RED);
    let height = bounds(&mut ui, CONTENT).height();
    swipe_up(&mut ui, height - 200.0, height - 1000.0);
    assert_has(&mut ui, Color::GREEN);
}

/// Red at the start, the bar gone on a swipe up, and back green.
fn enter_always_goes_and_comes_back_green(body: Body) {
    let mut ui = color_screen(Kind::EnterAlways, body, false, 0);
    assert_has(&mut ui, Color::RED);
    let height = bounds(&mut ui, CONTENT).height();
    swipe_up(&mut ui, height - 200.0, height - 1000.0);
    assert_eq!(bounds(&mut ui, BAR).height(), 0.0);
    swipe_down(&mut ui, height - 1000.0, height - 800.0);
    assert_has(&mut ui, Color::GREEN);
}

/// Green from the start, reversed or scrolled already; green while there's more; red at its start,
/// the swipes as far as it takes here, a swipe being worth fewer items than at Compose's density.
fn green_until_its_start(kind: Kind, body: Body, initial: i32) {
    let mut ui = color_screen(kind, body, true, initial);
    assert_has(&mut ui, Color::GREEN);
    swipe_down(&mut ui, 0.0, 500.0);
    assert_has(&mut ui, Color::GREEN);
    for _ in 0..12 {
        let height = bounds(&mut ui, CONTENT).height();
        swipe_down(&mut ui, 500.0, height + 1000.0);
    }
    assert_has(&mut ui, Color::RED);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_scrolledColumn
#[test]
fn pinned_over_a_column() {
    turns_green_on_a_swipe_up(Kind::Pinned, Body::Column);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_scrolledLazyGrid
#[test]
fn pinned_over_a_grid() {
    turns_green_on_a_swipe_up(Kind::Pinned, Body::Grid);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_scrolledLazyColumn_setIsAtStart
#[test]
fn enter_always_over_a_list() {
    enter_always_goes_and_comes_back_green(Body::List);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_scrolledColumn_setIsAtStart
#[test]
fn enter_always_over_a_column() {
    enter_always_goes_and_comes_back_green(Body::Column);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_scrolledLazyGrid
#[test]
fn enter_always_over_a_grid() {
    enter_always_goes_and_comes_back_green(Body::Grid);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_reverseLayout_scrolledColumn
#[test]
fn pinned_over_a_reversed_column() {
    green_until_its_start(Kind::Pinned, Body::Column, 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_reverseLayout_preScrolledColumn
#[test]
fn pinned_over_a_reversed_column_scrolled_already() {
    green_until_its_start(Kind::Pinned, Body::Column, 2000);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_reverseLayout_scrolledLazyColumn
#[test]
fn pinned_over_a_reversed_list() {
    green_until_its_start(Kind::Pinned, Body::List, 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_reverseLayout_preScrolledLazyColumn
#[test]
fn pinned_over_a_reversed_list_scrolled_already() {
    green_until_its_start(Kind::Pinned, Body::List, 30);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_reverseLayout_scrolledLazyGrid
#[test]
fn pinned_over_a_reversed_grid() {
    green_until_its_start(Kind::Pinned, Body::Grid, 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_reverseLayout_preScrolledLazyGrid
#[test]
fn pinned_over_a_reversed_grid_scrolled_already() {
    green_until_its_start(Kind::Pinned, Body::Grid, 30);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_reverseLayout_scrolledColumn_setIsAtStart
#[test]
fn enter_always_over_a_reversed_column() {
    green_until_its_start(Kind::EnterAlways, Body::Column, 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_reverseLayout_preScrolledColumn
#[test]
fn enter_always_over_a_reversed_column_scrolled_already() {
    green_until_its_start(Kind::EnterAlways, Body::Column, 2000);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_reverseLayout_scrolledLazyColumn_setIsAtStart
#[test]
fn enter_always_over_a_reversed_list() {
    green_until_its_start(Kind::EnterAlways, Body::List, 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_reverseLayout_preScrolledLazyColumn
#[test]
fn enter_always_over_a_reversed_list_scrolled_already() {
    green_until_its_start(Kind::EnterAlways, Body::List, 30);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_reverseLayout_preScrolledLazyGrid
#[test]
fn enter_always_over_a_reversed_grid_scrolled_already() {
    green_until_its_start(Kind::EnterAlways, Body::Grid, 30);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_changeColors_reversedLayout_scrolledLazyGrid
#[test]
fn enter_always_over_a_reversed_grid_up_and_down() {
    let mut ui = color_screen(Kind::EnterAlways, Body::TallGrid, true, 0);
    assert_has(&mut ui, Color::GREEN);
    let height = bounds(&mut ui, CONTENT).height();
    ui.on_node_with_tag(CONTENT)
        .perform_touch_input(|touch| touch.swipe_down_with(height - 700.0, height - 200.0, 150));
    ui.wait_for_idle();
    assert_has(&mut ui, Color::GREEN);
    ui.on_node_with_tag(CONTENT)
        // Less than Compose's 500: a fling goes further at density 1, here all the way back.
        .perform_touch_input(|touch| touch.swipe_up_with(height - 200.0, height - 400.0, 100));
    ui.wait_for_idle();
    assert_has(&mut ui, Color::GREEN);
    for _ in 0..24 {
        swipe_down(&mut ui, 500.0, height + 1000.0);
    }
    assert_has(&mut ui, Color::RED);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_preScrolledColumn
#[test]
fn pinned_over_a_column_scrolled_already_is_green_from_the_start() {
    let mut ui = color_screen(Kind::Pinned, Body::Column, false, 2000);
    assert_has(&mut ui, Color::GREEN);
    let height = bounds(&mut ui, CONTENT).height();
    swipe_up(&mut ui, height - 200.0, height - 1000.0);
    assert_has(&mut ui, Color::GREEN);
}

// ---- Scaffolds -----------------------------------------------------------------------------------

/// The list's state and a bar over it of `behavior`.
#[composable]
fn ListUnderBar(
    behavior: TopAppBarScrollBehavior,
    list: LazyListState,
    reverse: bool,
    item_height: f32,
) {
    let connection = behavior.nested_scroll_connection();
    Scaffold!(
        modifier = Modifier.fill_max_size(),
        top_bar = {
            TopAppBar!(
                { Text!("Title") },
                modifier = Modifier.test_tag(BAR),
                scroll_behavior = Some(behavior.clone())
            );
        },
        |padding: PaddingValues| {
            LazyColumn!(
                modifier = Modifier
                    .fill_max_size()
                    .nested_scroll(connection.clone())
                    .padding(padding)
                    .test_tag(CONTENT),
                state = list,
                reverse_layout = reverse,
                |scope: LazyListScope| {
                    scope.items(100, move |index| {
                        Text!(
                            "Item {index}",
                            modifier = Modifier.fill_max_width().height(item_height.dp())
                        );
                    });
                }
            );
        }
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_scrollingAndContentMovement
#[test]
fn enter_always_brings_the_bar_back_with_the_content() {
    let captured = Rc::new(Cell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let list = remember_lazy_list_state!();
            keep.set(Some(list));
            ListUnderBar!(TopAppBarDefaults::enter_always_scroll_behavior(), list, false, 40.0);
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(CONTENT)
        .perform_touch_input(|touch| touch.swipe_up_with(640.0, 100.0, 200));
    ui.wait_for_idle();
    let list = captured.get().unwrap();
    let tracked = list.layout_info().visible_items_info[0].index + 2;
    let text = format!("Item {tracked}");
    let before = ui.on_node_with_text(&text).fetch_semantics_snapshot().bounds.top;
    // Slowly: no fling, the bar's whole height down and the content that far.
    ui.on_node_with_tag(CONTENT)
        .perform_touch_input(|touch| touch.swipe_down_with(140.0, 204.0, 2000));
    ui.wait_for_idle();
    let after = ui.on_node_with_text(&text).fetch_semantics_snapshot().bounds.top;
    assert_eq!(after, before + 64.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_reverseLayout_scrollingAndLazyColumnMovement
#[test]
fn enter_always_over_a_reversed_list_keeps_the_list_under_the_bar() {
    let captured = Rc::new(Cell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let list = remember_lazy_list_state!();
            keep.set(Some(list));
            ListUnderBar!(
                TopAppBarDefaults::enter_always_scroll_behavior_for(&list),
                list,
                true,
                48.0
            );
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(CONTENT)
        .perform_touch_input(|touch| touch.swipe_down_with(40.0, 600.0, 200));
    ui.wait_for_idle();
    ui.on_node_with_tag(CONTENT).perform_touch_input(|touch| {
        touch.down(Offset::new(200.0, 368.0));
        touch.move_to(Offset::new(200.0, 368.0 - 64.0 + 50.0));
    });
    ui.wait_for_idle();
    let last = captured.get().unwrap().layout_info().visible_items_info.last().unwrap().index;
    let bar = bounds(&mut ui, BAR).bottom;
    assert_eq!(bounds(&mut ui, CONTENT).top, bar);
    assert!(bar < 64.0, "the drag collapsed the bar: {bar}");
    let item = ui.on_node_with_text(&format!("Item {last}")).fetch_semantics_snapshot().bounds;
    assert!(item.top <= bar && item.bottom > bar, "{item:?} under {bar}");
}

fn allows_a_horizontal_scroll(make: fn() -> TopAppBarScrollBehavior) {
    let captured = Rc::new(Cell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = make();
            let list = remember_lazy_list_state!();
            keep.set(Some(list));
            Scaffold!(
                modifier = Modifier.nested_scroll(behavior.nested_scroll_connection()),
                top_bar = {
                    TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior.clone()));
                },
                |padding: PaddingValues| {
                    LazyRow!(
                        modifier = Modifier.fill_max_size().padding(padding).test_tag(CONTENT),
                        state = list,
                        |scope: LazyListScope| {
                            scope.items(4, |_| Spacer(Modifier.fill_max_height().width(400.dp())));
                        }
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(CONTENT).perform_touch_input(|touch| touch.swipe_left());
    ui.wait_for_idle();
    assert!(captured.get().unwrap().first_visible_item_index() >= 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_enterAlways_allowHorizontalScroll
#[test]
fn enter_always_allows_a_horizontal_scroll() {
    allows_a_horizontal_scroll(TopAppBarDefaults::enter_always_scroll_behavior);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_exitUntilCollapsed_allowHorizontalScroll
#[test]
fn exit_until_collapsed_allows_a_horizontal_scroll() {
    allows_a_horizontal_scroll(TopAppBarDefaults::exit_until_collapsed_scroll_behavior);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_allowHorizontalScroll
#[test]
fn pinned_allows_a_horizontal_scroll() {
    allows_a_horizontal_scroll(TopAppBarDefaults::pinned_scroll_behavior);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_exitAlways_allowHorizontalScroll
#[test]
fn exit_always_allows_a_horizontal_scroll() {
    let captured = Rc::new(Cell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = BottomAppBarDefaults::exit_always_scroll_behavior();
            let list = remember_lazy_list_state!();
            keep.set(Some(list));
            Scaffold!(
                modifier = Modifier.nested_scroll(behavior.nested_scroll_connection()),
                bottom_bar = {
                    BottomAppBar!(|_| {}, scroll_behavior = Some(behavior.clone()));
                },
                |padding: PaddingValues| {
                    LazyRow!(
                        modifier = Modifier.fill_max_size().padding(padding).test_tag(CONTENT),
                        state = list,
                        |scope: LazyListScope| {
                            scope.items(4, |_| Spacer(Modifier.fill_max_height().width(400.dp())));
                        }
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(CONTENT).perform_touch_input(|touch| touch.swipe_left());
    ui.wait_for_idle();
    assert!(captured.get().unwrap().first_visible_item_index() >= 1);
}

/// A scaffold with an exit-always bottom bar and an overlaying action button.
fn fab_screen(captured: Rc<RefCell<Option<BottomAppBarScrollBehavior>>>) -> ComposeTestRule {
    window(move || {
        let keep = captured.clone();
        MaterialTheme!({
            let behavior = BottomAppBarDefaults::exit_always_scroll_behavior();
            keep.replace(Some(behavior.clone()));
            Scaffold!(
                modifier = Modifier.nested_scroll(behavior.nested_scroll_connection()),
                content_window_insets = no_insets(),
                bottom_bar = {
                    BottomAppBar!(
                        |_| {},
                        modifier = Modifier.test_tag(BAR),
                        window_insets = no_insets(),
                        scroll_behavior = Some(behavior.clone())
                    );
                },
                floating_action_button = {
                    FloatingActionButton!(
                        || {},
                        modifier = Modifier.test_tag("fab").offset(0.dp(), 4.dp()),
                        {}
                    );
                },
                floating_action_button_position = FabPosition::EndOverlay,
                |_: PaddingValues| {}
            );
        });
    })
}

fn assert_fab_in_place(ui: &mut ComposeTestRule, bar_height: f32) {
    assert_eq!(bounds(ui, BAR).height(), bar_height);
    let fab = bounds(ui, "fab");
    assert_eq!(fab.left, 400.0 - 16.0 - fab.width());
    assert_eq!(fab.top, 800.0 - 12.0 - fab.height());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_exitAlways_scaffoldWithFAB_default_positioning
#[test]
fn an_overlaying_button_is_over_the_bar() {
    let mut ui = fab_screen(Rc::default());
    ui.wait_for_idle();
    assert_fab_in_place(&mut ui, 80.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_exitAlways_scaffoldWithFAB_scrolled_positioning
#[test]
fn an_overlaying_button_stays_as_the_bar_goes() {
    let captured = Rc::new(RefCell::new(None));
    let mut ui = fab_screen(captured.clone());
    ui.wait_for_idle();
    let behavior = captured.borrow().clone().unwrap();
    behavior.state().set_height_offset(-20.0);
    behavior.state().set_content_offset(-20.0);
    ui.wait_for_idle();
    assert_fab_in_place(&mut ui, 60.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_FABShown_whenActionsOverflowRow
#[test]
fn a_button_shows_when_the_actions_overflow() {
    let mut ui = window(|| {
        MaterialTheme!({
            BottomAppBar!(
                |_| {
                    for _ in 0..12 {
                        Spacer(Modifier.size(48.dp()));
                    }
                },
                floating_action_button = {
                    FloatingActionButton!(|| {}, modifier = Modifier.test_tag("fab"), {});
                }
            );
        });
    });
    ui.wait_for_idle();
    let fab = bounds(&mut ui, "fab");
    assert!(fab.width() > 0.0 && fab.right <= 400.0, "{fab:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBarWithFAB_default_positioning
#[test]
fn a_bars_button_is_16_from_the_end_and_12_from_the_top() {
    let mut ui = window(|| {
        MaterialTheme!({
            BottomAppBar!(
                |_| {},
                modifier = Modifier.test_tag(BAR),
                floating_action_button = {
                    FloatingActionButton!(|| {}, modifier = Modifier.test_tag("fab"), {});
                }
            );
        });
    });
    ui.wait_for_idle();
    let (bar, fab) = (bounds(&mut ui, BAR), bounds(&mut ui, "fab"));
    assert_eq!(fab.left, bar.width() - 16.0 - fab.width());
    assert_eq!(fab.top, 12.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_widthExpandsToScreen
#[test]
fn a_bottom_bar_is_as_wide_as_the_window_and_80_high() {
    let mut ui = window(|| {
        MaterialTheme!({
            ContentBottomAppBar!(
                modifier = Modifier.test_tag(BAR),
                window_insets = no_insets(),
                content = |_| {}
            );
        });
    });
    let bar = bounds(&mut ui, BAR);
    assert_eq!((bar.width(), bar.height()), (400.0, 80.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBarWithFAB_heightIsFromSpec
#[test]
fn a_bottom_bar_with_a_button_is_80_high() {
    let mut ui = window(|| {
        MaterialTheme!({
            BottomAppBar!(
                |_| {},
                modifier = Modifier.test_tag(BAR),
                floating_action_button = {
                    FloatingActionButton!(|| {}, {});
                },
                window_insets = no_insets()
            );
        });
    });
    let bar = bounds(&mut ui, BAR);
    assert_eq!((bar.width(), bar.height()), (400.0, 80.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_default_positioning_respectsContentPadding
#[test]
fn a_bottom_bars_content_is_in_its_padding() {
    let mut ui = window(|| {
        MaterialTheme!({
            ContentBottomAppBar!(
                modifier = Modifier.test_tag(BAR),
                window_insets = no_insets(),
                content_padding =
                    PaddingValues { top: 5.dp(), start: 3.dp(), ..PaddingValues::ZERO },
                content = |_| FakeIcon("icon")
            );
        });
    });
    ui.wait_for_idle();
    let (bar, icon) = (bounds(&mut ui, BAR), bounds(&mut ui, "icon"));
    assert_eq!(icon.left, 3.0);
    // Centered in what the padding leaves, rounded up as Compose's alignment.
    assert_eq!(icon.top, bar.top + 5.0 + 26.0);
}

// ---- Small bars ----------------------------------------------------------------------------------

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_transparentContainerColor
#[test]
fn a_transparent_bar_shows_whats_behind() {
    let mut ui = window(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.fill_max_width().background(Color::RED), {
                TopAppBar!(
                    { Text!("Title") },
                    colors = TopAppBarColors {
                        container_color: Color::TRANSPARENT,
                        ..TopAppBarDefaults::top_app_bar_colors()
                    },
                    scroll_behavior = Some(TopAppBarDefaults::pinned_scroll_behavior())
                );
            });
        });
    });
    ui.wait_for_idle();
    let colors = fills(&mut ui);
    assert!(colors.contains(&Color::RED) && !colors.contains(&Color::TRANSPARENT), "{colors:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_expandsToScreen
#[test]
fn a_small_bar_is_as_wide_as_the_window() {
    let mut ui = window(|| {
        MaterialTheme!({
            TopAppBar!({ Text!("Title") }, modifier = Modifier.test_tag(BAR));
        });
    });
    let bar = bounds(&mut ui, BAR);
    assert_eq!((bar.width(), bar.height()), (400.0, 64.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_expandsToScreen
#[test]
fn a_centered_bar_is_as_wide_as_the_window() {
    let mut ui = window(|| {
        MaterialTheme!({
            CenterAlignedTopAppBar!({ Text!("Title") }, modifier = Modifier.test_tag(BAR));
        });
    });
    let bar = bounds(&mut ui, BAR);
    assert_eq!((bar.width(), bar.height()), (400.0, 64.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumTopAppBar_expandsToScreen
#[test]
fn a_medium_bar_is_as_wide_as_the_window() {
    let mut ui = window(|| {
        MaterialTheme!({
            MediumTopAppBar!({ Text!("Title") }, modifier = Modifier.test_tag(BAR));
        });
    });
    let bar = bounds(&mut ui, BAR);
    assert_eq!((bar.width(), bar.height()), (400.0, 112.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_expandsToScreen
#[test]
fn a_large_bar_is_as_wide_as_the_window() {
    let mut ui = window(|| {
        MaterialTheme!({
            LargeTopAppBar!({ Text!("Title") }, modifier = Modifier.test_tag(BAR));
        });
    });
    let bar = bounds(&mut ui, BAR);
    assert_eq!((bar.width(), bar.height()), (400.0, 152.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_customHeight
#[test]
fn a_small_bars_height_is_its_limit() {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::enter_always_scroll_behavior();
            keep.replace(Some(behavior.clone()));
            TopAppBar!(
                { Text!("Title") },
                modifier = Modifier.test_tag(BAR),
                expanded_height = 50.dp(),
                window_insets = no_insets(),
                scroll_behavior = Some(behavior)
            );
        });
    });
    ui.wait_for_idle();
    let behavior: TopAppBarScrollBehavior = captured.borrow().clone().unwrap();
    assert_eq!(behavior.state().height_offset_limit(), -50.0);
    assert_eq!(bounds(&mut ui, BAR).height(), 50.0);
}

/// A small bar of each kind, its icons fake.
fn small_positions(centered: bool, navigation: bool) -> ComposeTestRule {
    thread_local! { static CASE: Cell<(bool, bool)> = const { Cell::new((false, false)) }; }
    CASE.with(|case| case.set((centered, navigation)));
    let mut ui = window(|| {
        let (centered, navigation) = CASE.with(Cell::get);
        MaterialTheme!({
            Box!(modifier = Modifier.test_tag(BAR), {
                let icon = navigation.then_some(|| FakeIcon(NAVIGATION));
                if centered {
                    CenterAlignedTopAppBar!(
                        { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
                        navigation_icon = icon,
                        actions = |_| FakeIcon(ACTIONS),
                        window_insets = no_insets()
                    );
                } else {
                    TopAppBar!(
                        { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
                        navigation_icon = icon,
                        actions = |_| FakeIcon(ACTIONS),
                        window_insets = no_insets()
                    );
                }
            });
        });
    });
    ui.wait_for_idle();
    ui
}

fn assert_actions_at_the_end(ui: &mut ComposeTestRule) {
    let bar = bounds(ui, BAR);
    let actions = bounds(ui, ACTIONS);
    assert_eq!(actions.left, bar.width() - 4.0 - 24.0);
    assert_eq!(actions.top, 20.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_default_positioning
#[test]
fn a_small_bars_rows_are_in_place() {
    let mut ui = small_positions(false, true);
    let navigation = bounds(&mut ui, NAVIGATION);
    assert_eq!((navigation.left, navigation.top), (4.0, 20.0));
    let title = bounds(&mut ui, TITLE);
    assert_eq!(title.left, 4.0 + 24.0 + 4.0 + 12.0 - 12.0);
    assert_eq!(title.top, (64.0 - title.height()) / 2.0);
    assert_actions_at_the_end(&mut ui);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_noNavigationIcon_positioning
#[test]
fn a_small_bars_title_without_an_icon_is_16_in() {
    let mut ui = small_positions(false, false);
    let title = bounds(&mut ui, TITLE);
    assert_eq!(title.left, 16.0);
    assert_eq!(title.top, (64.0 - title.height()) / 2.0);
    assert_actions_at_the_end(&mut ui);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_default_positioning
#[test]
fn a_centered_bars_rows_are_in_place() {
    let mut ui = small_positions(true, true);
    let navigation = bounds(&mut ui, NAVIGATION);
    assert_eq!((navigation.left, navigation.top), (4.0, 20.0));
    let title = bounds(&mut ui, TITLE);
    assert_eq!(title.left, ((400.0 - title.width()) / 2.0).round());
    assert_eq!(title.top, (64.0 - title.height()) / 2.0);
    assert_actions_at_the_end(&mut ui);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_noNavigationIcon_positioning
#[test]
fn a_centered_bars_title_without_an_icon_is_centered() {
    let mut ui = small_positions(true, false);
    let title = bounds(&mut ui, TITLE);
    assert_eq!(title.left, ((400.0 - title.width()) / 2.0).round());
    assert_actions_at_the_end(&mut ui);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_measureWithNonZeroMinWidth
#[test]
fn a_centered_bar_with_a_minimum_width_measures_its_title() {
    let mut ui = window(|| {
        MaterialTheme!({
            CenterAlignedTopAppBar!(
                { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
                modifier = Modifier.width_in(100.dp(), 400.dp()).test_tag(BAR)
            );
        });
    });
    ui.wait_for_idle();
    assert!(bounds(&mut ui, TITLE).width() > 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_scrolledPositioning
#[test]
fn a_small_bar_collapses_by_its_offset() {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::enter_always_scroll_behavior();
            keep.replace(Some(behavior.clone()));
            TopAppBar!(
                { Text!("Title") },
                modifier = Modifier.test_tag(BAR),
                window_insets = no_insets(),
                scroll_behavior = Some(behavior)
            );
        });
    });
    ui.wait_for_idle();
    let behavior: TopAppBarScrollBehavior = captured.borrow().clone().unwrap();
    behavior.state().set_height_offset(-20.0);
    behavior.state().set_content_offset(-20.0);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 44.0);
    // As Kotlin's `roundToInt`, halves up, not away from zero.
    behavior.state().set_height_offset(-1.5);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 63.0);
}

/// What a slot reads, last composed.
#[derive(Default, Clone)]
struct Seen {
    title_style: Rc<RefCell<Option<compose_ui::TextStyle>>>,
    subtitle_style: Rc<RefCell<Option<compose_ui::TextStyle>>>,
    title_color: Rc<Cell<Option<Color>>>,
    navigation_color: Rc<Cell<Option<Color>>>,
    action_color: Rc<Cell<Option<Color>>>,
}

fn small_bar_seeing(centered: bool, seen: Seen) -> ComposeTestRule {
    window(move || {
        let seen = seen.clone();
        MaterialTheme!({
            let (title, subtitle, navigation, actions) =
                (seen.clone(), seen.clone(), seen.clone(), seen.clone());
            if centered {
                CenterAlignedTopAppBar!(
                    move || {
                        title.title_style.replace(Some(LOCAL_TEXT_STYLE.current()));
                        title.title_color.set(Some(LOCAL_CONTENT_COLOR.current()));
                    },
                    navigation_icon = Some(move || navigation
                        .navigation_color
                        .set(Some(LOCAL_CONTENT_COLOR.current()))),
                    actions =
                        move |_| actions.action_color.set(Some(LOCAL_CONTENT_COLOR.current()))
                );
            } else {
                TopAppBar!(
                    move || {
                        title.title_style.replace(Some(LOCAL_TEXT_STYLE.current()));
                        title.title_color.set(Some(LOCAL_CONTENT_COLOR.current()));
                    },
                    navigation_icon = Some(move || navigation
                        .navigation_color
                        .set(Some(LOCAL_CONTENT_COLOR.current()))),
                    actions =
                        move |_| actions.action_color.set(Some(LOCAL_CONTENT_COLOR.current())),
                    subtitle = Some(move || {
                        subtitle.subtitle_style.replace(Some(LOCAL_TEXT_STYLE.current()));
                    })
                );
            }
        });
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_titleDefaultStyle
#[test]
fn a_small_bars_title_is_in_title_large() {
    let seen = Seen::default();
    let mut ui = small_bar_seeing(false, seen.clone());
    ui.wait_for_idle();
    let typography = Typography::default();
    let style = seen.title_style.borrow().clone().unwrap();
    assert_eq!(style, LOCAL_TEXT_STYLE_DEFAULT().merge(&typography.title_large));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_subtitleDefaultStyle
#[test]
fn a_small_bars_subtitle_is_in_label_medium() {
    let seen = Seen::default();
    let mut ui = small_bar_seeing(false, seen.clone());
    ui.wait_for_idle();
    let typography = Typography::default();
    let style = seen.subtitle_style.borrow().clone().unwrap();
    assert_eq!(style, LOCAL_TEXT_STYLE_DEFAULT().merge(&typography.label_medium));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_titleDefaultStyle
#[test]
fn a_centered_bars_title_is_in_title_large() {
    let seen = Seen::default();
    let mut ui = small_bar_seeing(true, seen.clone());
    ui.wait_for_idle();
    let typography = Typography::default();
    let style = seen.title_style.borrow().clone().unwrap();
    assert_eq!(style, LOCAL_TEXT_STYLE_DEFAULT().merge(&typography.title_large));
}

#[allow(non_snake_case)]
fn LOCAL_TEXT_STYLE_DEFAULT() -> compose_ui::TextStyle {
    compose_ui::TextStyle::DEFAULT
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_contentColor
#[test]
fn a_small_bars_slots_take_their_colors() {
    let seen = Seen::default();
    let mut ui = small_bar_seeing(false, seen.clone());
    ui.wait_for_idle();
    let scheme = light_color_scheme();
    assert_eq!(seen.navigation_color.get(), Some(scheme.on_surface));
    assert_eq!(seen.title_color.get(), Some(scheme.on_surface));
    assert_eq!(seen.action_color.get(), Some(scheme.on_surface_variant));
    assert_has(&mut ui, scheme.surface);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_contentColor
#[test]
fn a_centered_bars_slots_take_their_colors() {
    let seen = Seen::default();
    let mut ui = small_bar_seeing(true, seen.clone());
    ui.wait_for_idle();
    let scheme = light_color_scheme();
    assert_eq!(seen.navigation_color.get(), Some(scheme.on_surface));
    assert_eq!(seen.title_color.get(), Some(scheme.on_surface));
    assert_eq!(seen.action_color.get(), Some(scheme.on_surface_variant));
    assert_has(&mut ui, scheme.surface);
}

/// Slot colors a test can tell apart reach their slots.
#[test]
fn custom_colors_reach_their_slots() {
    let seen = Seen::default();
    let keep = seen.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let (title, navigation, actions) = (keep.clone(), keep.clone(), keep.clone());
            TopAppBar!(
                move || title.title_color.set(Some(LOCAL_CONTENT_COLOR.current())),
                navigation_icon = Some(move || navigation
                    .navigation_color
                    .set(Some(LOCAL_CONTENT_COLOR.current()))),
                actions = move |_| actions.action_color.set(Some(LOCAL_CONTENT_COLOR.current())),
                colors = TopAppBarColors {
                    navigation_icon_content_color: Color::RED,
                    title_content_color: Color::GREEN,
                    action_icon_content_color: Color::BLUE,
                    ..TopAppBarDefaults::top_app_bar_colors()
                }
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(seen.navigation_color.get(), Some(Color::RED));
    assert_eq!(seen.title_color.get(), Some(Color::GREEN));
    assert_eq!(seen.action_color.get(), Some(Color::BLUE));
}

fn small_bar_scrolled(centered: bool) {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::pinned_scroll_behavior();
            keep.replace(Some(behavior.clone()));
            if centered {
                CenterAlignedTopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior));
            } else {
                TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior));
            }
        });
    });
    ui.wait_for_idle();
    let behavior: TopAppBarScrollBehavior = captured.borrow().clone().unwrap();
    behavior.state().set_content_offset(-100.0);
    ui.wait_for_idle();
    let scheme = light_color_scheme();
    assert_has(&mut ui, scheme.surface_container);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_scrolledContentColor
#[test]
fn a_small_bar_scrolled_takes_the_scrolled_color() {
    small_bar_scrolled(false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_scrolledContentColor
#[test]
fn a_centered_bar_scrolled_takes_the_scrolled_color() {
    small_bar_scrolled(true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_withTitle
#[test]
fn a_small_bars_title_shows() {
    let mut ui = window(|| {
        MaterialTheme!({
            TopAppBar!({ Text!("Title") });
        });
    });
    ui.on_node_with_text("Title").assert_is_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_withTitle
#[test]
fn a_centered_bars_title_shows() {
    let mut ui = window(|| {
        MaterialTheme!({
            CenterAlignedTopAppBar!({ Text!("Title") });
        });
    });
    ui.on_node_with_text("Title").assert_is_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_withSubtitle
#[test]
fn a_subtitle_shows() {
    let mut ui = window(|| {
        MaterialTheme!({
            TopAppBar!({ Text!("Title") }, subtitle = Some(|| Text!("Subtitle")));
        });
    });
    ui.on_node_with_text("Subtitle").assert_is_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, state_restoresTopAppBarState
#[test]
fn a_state_comes_back_as_it_was() {
    thread_local! { static STATE: Cell<Option<TopAppBarState>> = const { Cell::new(None) }; }
    let mut rule = compose_test(|| {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut rule, || {
        let state = remember_top_app_bar_state!();
        STATE.with(|cell| cell.set(Some(state)));
    });
    rule.run_on_idle(|_| {
        let state = STATE.with(Cell::get).unwrap();
        state.set_height_offset_limit(-350.0);
        state.set_height_offset(-300.0);
        state.set_content_offset(-550.0);
    });
    STATE.with(|cell| cell.set(None));
    tester.emulate_save_and_restore(&mut rule);
    let state = STATE.with(Cell::get).unwrap();
    assert_eq!(state.height_offset_limit(), -350.0);
    assert_eq!(state.height_offset(), -300.0);
    assert_eq!(state.content_offset(), -550.0);
}

/// A bottom bar's too, as `BottomAppBarState.Saver`.
#[test]
fn a_bottom_state_comes_back_as_it_was() {
    thread_local! { static STATE: Cell<Option<BottomAppBarState>> = const { Cell::new(None) }; }
    let mut rule = compose_test(|| {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut rule, || {
        let state = remember_bottom_app_bar_state!();
        STATE.with(|cell| cell.set(Some(state)));
    });
    rule.run_on_idle(|_| {
        let state = STATE.with(Cell::get).unwrap();
        state.set_height_offset_limit(-350.0);
        state.set_height_offset(-300.0);
        state.set_content_offset(-550.0);
    });
    STATE.with(|cell| cell.set(None));
    tester.emulate_save_and_restore(&mut rule);
    let state = STATE.with(Cell::get).unwrap();
    assert_eq!(state.height_offset_limit(), -350.0);
    assert_eq!(state.height_offset(), -300.0);
    assert_eq!(state.content_offset(), -550.0);
}

// ---- Two rows --------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Bar {
    Medium,
    Large,
    MediumFlexible,
    LargeFlexible,
}

compose_arg!(Bar);

/// A two-row bar of `bar`, with `subtitle` where it has one, in `collapsed` and `expanded` heights
/// if given; a behavior out, its title and subtitle tagged.
#[composable]
fn TwoRowBar(
    bar: Bar,
    subtitled: bool,
    heights: Option<(f32, f32)>,
    insets: f32,
    behavior: TopAppBarScrollBehavior,
) {
    let window_insets = WindowInsets::new(insets.dp(), insets.dp(), insets.dp(), insets.dp());
    Box!(modifier = Modifier.test_tag(BAR), {
        let title = move || {
            Text!("Title", modifier = Modifier.test_tag(TITLE));
        };
        let subtitle = subtitled.then_some(move || {
            Text!("Subtitle", modifier = Modifier.test_tag(SUBTITLE));
        });
        let navigation = Some(|| FakeIcon(NAVIGATION));
        match bar {
            Bar::Medium => MediumTopAppBar!(
                title,
                navigation_icon = navigation,
                actions = |_| FakeIcon(ACTIONS),
                collapsed_height = heights.map_or(64.dp(), |(low, _)| low.dp()),
                expanded_height = heights.map_or(112.dp(), |(_, high)| high.dp()),
                window_insets = window_insets.clone(),
                scroll_behavior = Some(behavior.clone())
            ),
            Bar::Large => LargeTopAppBar!(
                title,
                navigation_icon = navigation,
                actions = |_| FakeIcon(ACTIONS),
                collapsed_height = heights.map_or(64.dp(), |(low, _)| low.dp()),
                expanded_height = heights.map_or(152.dp(), |(_, high)| high.dp()),
                window_insets = window_insets.clone(),
                scroll_behavior = Some(behavior.clone())
            ),
            Bar::MediumFlexible => MediumFlexibleTopAppBar!(
                title,
                subtitle = subtitle,
                navigation_icon = navigation,
                actions = |_| FakeIcon(ACTIONS),
                collapsed_height = heights.map_or(64.dp(), |(low, _)| low.dp()),
                expanded_height = heights
                    .map_or(if subtitled { 136.dp() } else { 112.dp() }, |(_, high)| high.dp()),
                window_insets = window_insets.clone(),
                scroll_behavior = Some(behavior.clone())
            ),
            Bar::LargeFlexible => LargeFlexibleTopAppBar!(
                title,
                subtitle = subtitle,
                navigation_icon = navigation,
                actions = |_| FakeIcon(ACTIONS),
                collapsed_height = heights.map_or(64.dp(), |(low, _)| low.dp()),
                expanded_height = heights
                    .map_or(if subtitled { 152.dp() } else { 120.dp() }, |(_, high)| high.dp()),
                window_insets = window_insets.clone(),
                scroll_behavior = Some(behavior.clone())
            ),
        }
    });
}

fn two_row_bar(
    bar: Bar,
    subtitled: bool,
    heights: Option<(f32, f32)>,
    insets: f32,
) -> (ComposeTestRule, TopAppBarScrollBehavior) {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::exit_until_collapsed_scroll_behavior();
            keep.replace(Some(behavior.clone()));
            TwoRowBar!(bar, subtitled, heights, insets, behavior);
        });
    });
    ui.wait_for_idle();
    let behavior = captured.borrow().clone().unwrap();
    (ui, behavior)
}

fn collapse(
    ui: &mut ComposeTestRule,
    behavior: &TopAppBarScrollBehavior,
    height: f32,
    content: f32,
) {
    behavior.state().set_height_offset(-height);
    behavior.state().set_content_offset(-content);
    ui.wait_for_idle();
}

fn semantic_nodes(ui: &mut ComposeTestRule, text: &str) -> usize {
    ui.on_all_nodes(has_text(text)).fetch_semantics_nodes().len()
}

/// One title for what reads the screen, whichever row shows, as `assertMediumOrLargeScrolledSemantics`.
fn one_title_read_out(bar: Bar, subtitled: bool, expanded: f32) {
    let (mut ui, behavior) = two_row_bar(bar, subtitled, None, 0.0);
    let collapsing = expanded - 64.0;
    for offset in [0.0, collapsing / 3.0, collapsing] {
        collapse(&mut ui, &behavior, offset, offset);
        assert_eq!(semantic_nodes(&mut ui, "Title"), 1, "at {offset}");
        if subtitled {
            assert_eq!(semantic_nodes(&mut ui, "Subtitle"), 1, "at {offset}");
        }
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumTopAppBar_semantics
#[test]
fn a_medium_bar_reads_one_title() {
    one_title_read_out(Bar::Medium, false, 112.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_semantics
#[test]
fn a_large_bar_reads_one_title() {
    one_title_read_out(Bar::Large, false, 152.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumFlexibleTopAppBar_withSubtitle_semantics
#[test]
fn a_medium_flexible_bar_reads_one_title_and_subtitle() {
    one_title_read_out(Bar::MediumFlexible, true, 136.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeFlexibleTopAppBar_withSubtitle_semantics
#[test]
fn a_large_flexible_bar_reads_one_title_and_subtitle() {
    one_title_read_out(Bar::LargeFlexible, true, 152.0);
}

/// Half way the title read out goes from the row below to the row on top: the row hidden has no
/// semantics.
#[test]
fn the_title_read_out_is_in_the_row_that_shows_most() {
    let (mut ui, behavior) = two_row_bar(Bar::Large, false, None, 0.0);
    let top_of_the_title = |ui: &mut ComposeTestRule| {
        ui.on_node(has_text("Title")).fetch_semantics_snapshot().bounds.top
    };
    assert!(top_of_the_title(&mut ui) > 64.0, "in the bottom row");
    collapse(&mut ui, &behavior, 44.0 - 1.0, 0.0);
    assert!(top_of_the_title(&mut ui) > 40.0, "just under half way, still the bottom row");
    collapse(&mut ui, &behavior, 44.0 + 1.0, 0.0);
    assert!(top_of_the_title(&mut ui) < 40.0, "just over half way, the top row");
}

/// How far up from the bar's bottom the last baseline of the tag's last node is, as the end of
/// `assertMediumOrLargeDefaultPositioning`.
fn bottom_baseline_distance(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    let bar = bounds(ui, BAR);
    let top =
        ui.on_all_nodes_unmerged(has_test_tag(tag)).on_last().fetch_semantics_snapshot().bounds.top;
    let baseline =
        ui.on_all_nodes_unmerged(has_test_tag(tag)).on_last().get_last_baseline_position();
    bar.bottom - (top + baseline.0)
}

/// A bar in Compose's test font: as with Roboto, a medium bar's bottom row has the room for 24 dp
/// under the baseline, which compose-rs's default font (its baseline lower in the line) hasn't.
fn in_the_sample_font(bar: Bar, heights: Option<(f32, f32)>) -> ComposeTestRule {
    let measurer = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
    compose_test_with(IntSize::new(400, 800), Density::default(), Box::new(measurer), move || {
        MaterialTheme!({
            let behavior = TopAppBarDefaults::exit_until_collapsed_scroll_behavior();
            TwoRowBar!(bar, false, heights, 0.0, behavior);
        });
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumTopAppBar_expanded_positioning
#[test]
fn a_medium_bars_bottom_title_has_its_baseline_24_dp_up() {
    let mut ui = in_the_sample_font(Bar::Medium, None);
    assert_eq!(bottom_baseline_distance(&mut ui, TITLE), 24.0);
}

/// Less padding where there's no room for it: the title stays in its row, at its top.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, placeTopAppBar
#[test]
fn with_no_room_for_its_padding_the_title_stays_in_its_row() {
    let (mut ui, _behavior) = two_row_bar(Bar::Medium, false, None, 0.0);
    let mut title = ui.on_all_nodes_unmerged(has_test_tag(TITLE)).on_last();
    assert_eq!(title.fetch_semantics_snapshot().bounds.top, 64.0);
    assert!(bottom_baseline_distance(&mut ui, TITLE) < 24.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumTopAppBar_customHeight_expanded_positioning
#[test]
fn a_medium_bar_of_custom_heights_has_its_baseline_24_dp_up() {
    let mut ui = in_the_sample_font(Bar::Medium, Some((80.0, 200.0)));
    assert_eq!(bottom_baseline_distance(&mut ui, TITLE), 24.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_expanded_positioning
#[test]
fn a_large_bars_bottom_title_has_its_baseline_28_dp_up() {
    let (mut ui, _behavior) = two_row_bar(Bar::Large, false, None, 0.0);
    assert_eq!(bottom_baseline_distance(&mut ui, TITLE), 28.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_customHeight_expanded_positioning
#[test]
fn a_large_bar_of_custom_heights_has_its_baseline_28_dp_up() {
    let (mut ui, _behavior) = two_row_bar(Bar::Large, false, Some((80.0, 200.0)), 0.0);
    assert_eq!(bottom_baseline_distance(&mut ui, TITLE), 28.0);
}

/// With a subtitle the last baseline is the subtitle's: its column's last line, inherited.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, TopAppBarMeasurePolicy
#[test]
fn a_flexible_bars_subtitle_takes_the_padding_from_the_bottom() {
    let (mut ui, _behavior) = two_row_bar(Bar::LargeFlexible, true, Some((64.0, 200.0)), 0.0);
    assert_eq!(bottom_baseline_distance(&mut ui, SUBTITLE), 28.0);
    // With no subtitle the title's is the last.
    let (mut ui, _behavior) = two_row_bar(Bar::LargeFlexible, false, Some((64.0, 200.0)), 0.0);
    assert_eq!(bottom_baseline_distance(&mut ui, TITLE), 28.0);
}

/// Part way collapsed, the baseline keeps its distance from what shows of the bar's bottom row.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_scrolled_positioning
#[test]
fn collapsing_keeps_the_bottom_titles_baseline_over_the_bars_bottom() {
    let (mut ui, behavior) = two_row_bar(Bar::Large, false, None, 0.0);
    collapse(&mut ui, &behavior, 20.0, 0.0);
    assert_eq!(bottom_baseline_distance(&mut ui, TITLE), 28.0);
}

fn expanded_in_place(bar: Bar, heights: Option<(f32, f32)>, expanded: f32) {
    let (mut ui, _) = two_row_bar(bar, false, heights, 0.0);
    let low = heights.map_or(64.0, |(low, _)| low);
    assert_eq!(bounds(&mut ui, BAR).height(), expanded);
    let navigation = bounds(&mut ui, NAVIGATION);
    assert_eq!((navigation.left, navigation.top), (4.0, (low - 24.0) / 2.0));
    let actions = bounds(&mut ui, ACTIONS);
    assert_eq!((actions.left, actions.top), (400.0 - 4.0 - 24.0, (low - 24.0) / 2.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_expanded_positioning
#[test]
fn a_large_bar_expanded_is_in_place() {
    expanded_in_place(Bar::Large, None, 152.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_customHeight_expanded_positioning
#[test]
fn a_large_bar_of_custom_heights_expanded_is_in_place() {
    expanded_in_place(Bar::Large, Some((80.0, 200.0)), 200.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumTopAppBar_customHeight_expanded_positioning
#[test]
fn a_medium_bar_of_custom_heights_expanded_is_in_place() {
    expanded_in_place(Bar::Medium, Some((36.0, 112.0)), 112.0);
}

/// A third of the way, then all of it and more in the content, as `assertMediumOrLargeScrolledHeight`.
fn scrolled_height(bar: Bar, heights: Option<(f32, f32)>, expanded: f32, insets: f32) {
    let (mut ui, behavior) = two_row_bar(bar, false, heights, insets);
    let low = heights.map_or(64.0, |(low, _)| low);
    let (whole, third) = (expanded - low, (expanded - low) / 3.0);
    collapse(&mut ui, &behavior, third, third);
    // Within half a pixel, the height being whole, as Compose's bounds assertions.
    let height = bounds(&mut ui, BAR).height();
    assert!((height - (expanded - third + 2.0 * insets)).abs() <= 0.5, "{height}");
    collapse(&mut ui, &behavior, whole, whole + third);
    assert_eq!(bounds(&mut ui, BAR).height(), low + 2.0 * insets);
    assert_eq!(behavior.state().collapsed_fraction(), 1.0);
    assert_eq!(behavior.state().overlapped_fraction(), 1.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumTopAppBar_scrolled_positioning
#[test]
fn a_medium_bar_scrolled_collapses() {
    scrolled_height(Bar::Medium, None, 112.0, 13.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_scrolled_positioning
#[test]
fn a_large_bar_scrolled_collapses() {
    scrolled_height(Bar::Large, None, 152.0, 13.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumTopAppBar_customHeight_scrolled_positioning
#[test]
fn a_medium_bar_of_custom_heights_scrolled_collapses() {
    scrolled_height(Bar::Medium, Some((40.0, 120.0)), 120.0, 13.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_customHeight_scrolled_positioning
#[test]
fn a_large_bar_of_custom_heights_scrolled_collapses() {
    scrolled_height(Bar::Large, Some((80.0, 200.0)), 200.0, 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_mediumDraggedAppBar
#[test]
fn a_medium_bar_follows_a_drag_not_let_go() {
    let (mut ui, _) = two_row_bar(Bar::Medium, false, None, 0.0);
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.down(Offset::new(0.0, 92.0));
        touch.move_to(Offset::new(0.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
}

/// The layers' alphas, the top row's title's then the bottom's.
fn title_alphas(ui: &mut ComposeTestRule) -> Vec<f32> {
    let list = ui.host().display_list();
    list.groups.iter().filter_map(|group| group.layer.as_ref().map(|layer| layer.alpha)).collect()
}

/// The top title comes in late, on Compose's easing, the bottom one goes as the bar collapses, and
/// the container goes the eased way to the scrolled color.
#[test]
fn the_rows_fade_by_how_collapsed_the_bar_is() {
    let (mut ui, behavior) = two_row_bar(Bar::Large, false, None, 0.0);
    let alphas = title_alphas(&mut ui);
    assert!(alphas.contains(&0.0) && alphas.contains(&1.0), "{alphas:?}");
    // 44 of 88: the top title's easing is under a half at a half.
    collapse(&mut ui, &behavior, 44.0, 44.0);
    let alphas = title_alphas(&mut ui);
    assert!(alphas.contains(&0.5), "the bottom one's a half: {alphas:?}");
    let top = alphas.iter().copied().find(|alpha| *alpha != 0.5 && *alpha != 1.0).unwrap_or(0.0);
    assert_eq!(top, compose_animation::CubicBezierEasing::new(0.8, 0.0, 0.8, 0.15).transform(0.5));
    collapse(&mut ui, &behavior, 88.0, 88.0);
    let alphas = title_alphas(&mut ui);
    assert!(alphas.contains(&0.0) && alphas.contains(&1.0), "{alphas:?}");
}

/// The bar's container follows the collapse itself, not a threshold.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeTopAppBar_scrolledContainerColor
#[test]
fn a_large_bars_container_goes_with_the_collapse() {
    let (mut ui, behavior) = two_row_bar(Bar::Large, false, None, 0.0);
    let scheme = light_color_scheme();
    assert_has(&mut ui, scheme.surface);
    collapse(&mut ui, &behavior, 88.0, 88.0);
    assert_has(&mut ui, scheme.surface_container);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumTopAppBar_scrolledContainerColor
#[test]
fn a_medium_bars_container_goes_with_the_collapse() {
    let (mut ui, behavior) = two_row_bar(Bar::Medium, false, None, 0.0);
    let scheme = light_color_scheme();
    assert_has(&mut ui, scheme.surface);
    collapse(&mut ui, &behavior, 48.0, 48.0);
    assert_has(&mut ui, scheme.surface_container);
}

fn fits_its_text(bar: Bar, subtitled: bool, heights: (f32, f32)) {
    let (mut ui, behavior) = two_row_bar(bar, subtitled, Some(heights), 0.0);
    let title =
        ui.on_all_nodes_unmerged(has_test_tag(TITLE)).on_last().fetch_semantics_snapshot().bounds;
    let mut text = title.height();
    if subtitled {
        text += ui
            .on_all_nodes_unmerged(has_test_tag(SUBTITLE))
            .on_last()
            .fetch_semantics_snapshot()
            .bounds
            .height();
    }
    assert_eq!(behavior.state().height_offset_limit(), -text);
    assert_eq!(bounds(&mut ui, BAR).height(), heights.0 + text);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeFlexibleTopAppBar_fitsTextIfHeightTooSmall_expanded
#[test]
fn a_large_flexible_bar_too_small_fits_its_text() {
    fits_its_text(Bar::LargeFlexible, true, (64.0, 64.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, largeFlexibleTopAppBar_fitsTextIfHeightTooSmall_collapsed
#[test]
fn a_large_flexible_bar_of_no_height_fits_its_text() {
    fits_its_text(Bar::LargeFlexible, true, (64.0, 70.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumFlexibleTopAppBar_fitsTextIfHeightTooSmall_collapsed
#[test]
fn a_medium_flexible_bar_of_no_height_fits_its_text() {
    fits_its_text(Bar::MediumFlexible, true, (64.0, 70.0));
}

// ---- Things Compose doesn't test, but the reviews found --------------------------------------------

/// A behavior handed over later works, and going without it the bar's whole again.
#[test]
fn a_behavior_given_later_works_then_goes() {
    let which = mutable_state_of(false);
    let handle = which.handle();
    let mut ui = window(move || {
        MaterialTheme!({
            let behavior = TopAppBarDefaults::exit_until_collapsed_scroll_behavior();
            LargeTopAppBar!(
                { Text!("Title") },
                modifier = Modifier.test_tag(BAR),
                window_insets = no_insets(),
                scroll_behavior = handle.get().then(|| behavior.clone())
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 152.0);
    which.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.down(Offset::new(0.0, 132.0));
        touch.move_to(Offset::new(0.0, 40.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
    which.set(false);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 152.0);
    which.set(true);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
}

/// From red to blue half way, eased, in Oklab: the value is Compose's, by hand from `lerp`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, TopAppBarColors.containerColor
#[test]
fn the_container_lerps_as_composes() {
    let colors = TopAppBarColors {
        container_color: Color::RED,
        scrolled_container_color: Color::BLUE,
        navigation_icon_content_color: Color::BLACK,
        title_content_color: Color::BLACK,
        action_icon_content_color: Color::BLACK,
        subtitle_content_color: Color::BLACK,
    };
    assert_eq!(colors.container_color(0.0), Color::RED);
    assert_eq!(colors.container_color(1.0), Color::BLUE);
    assert_eq!(colors.container_color(0.5), Color::from_argb(0xFFB54F7E));
}

/// New colors on a small bar animate, as `animateColorAsState(targetColor)`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, SingleRowTopAppBar
#[test]
fn new_colors_on_a_small_bar_animate() {
    let which = mutable_state_of(false);
    let handle = which.handle();
    let mut ui = window(move || {
        let on = handle.get();
        MaterialTheme!({
            TopAppBar!(
                { Text!("Title") },
                colors = TopAppBarColors {
                    container_color: if on { Color::BLUE } else { Color::RED },
                    ..TopAppBarDefaults::top_app_bar_colors()
                }
            );
        });
    });
    ui.wait_for_idle();
    assert_has(&mut ui, Color::RED);
    ui.set_auto_advance(false);
    which.set(true);
    ui.advance_time_by(std::time::Duration::from_millis(16));
    assert!(!fills(&mut ui).contains(&Color::BLUE), "it jumped");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_has(&mut ui, Color::BLUE);
}

/// Part way, what `animate_color_as_state` gives on the same spec, with no easing over it.
#[test]
fn the_scrolled_transition_is_a_color_animation() {
    let which = mutable_state_of(false);
    let handle = which.handle();
    let reference = Rc::new(Cell::new(None));
    let keep = reference.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::pinned_scroll_behavior();
            behavior.state().set_content_offset(if handle.get() { -500.0 } else { 0.0 });
            behavior.state().set_height_offset_limit(-64.0);
            let animated = animate_color_as_state!(
                if handle.get() { Color::BLUE } else { Color::RED },
                MaterialTheme::motion_scheme().default_effects_spec()
            );
            keep.set(Some(animated));
            TopAppBar!(
                { Text!("Title") },
                colors = TopAppBarColors {
                    container_color: Color::RED,
                    scrolled_container_color: Color::BLUE,
                    ..TopAppBarDefaults::top_app_bar_colors()
                },
                scroll_behavior = Some(behavior.clone())
            );
        });
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    which.set(true);
    ui.advance_time_by(std::time::Duration::from_millis(32));
    ui.advance_time_by(std::time::Duration::from_millis(32));
    let drawn = fills(&mut ui)[0];
    assert_eq!(drawn, reference.get().unwrap().get());
    assert!(drawn != Color::RED && drawn != Color::BLUE, "part way: {drawn:?}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
}

/// What a closure given inline changes isn't a new behavior: the latest is asked.
#[test]
fn an_inline_can_scroll_keeps_the_behavior_and_is_the_latest() {
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let answer = Rc::new(Cell::new(true));
    let seen: Rc<RefCell<Vec<TopAppBarScrollBehavior>>> = Rc::default();
    let keep = seen.clone();
    let mut ui = window(move || {
        let value = handle.get();
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::enter_always_scroll_behavior_with(
                Some(Callback::new(move || value == 0)),
                None,
                None,
            );
            keep.borrow_mut().push(behavior);
        });
    });
    ui.wait_for_idle();
    let _ = answer;
    let first = seen.borrow()[0].clone();
    first.state().set_height_offset_limit(-64.0);
    let scroll = |behavior: &TopAppBarScrollBehavior| {
        behavior
            .nested_scroll_connection()
            .on_pre_scroll(Offset::new(0.0, -10.0), compose_ui::NestedScrollSource::UserInput)
    };
    assert_eq!(scroll(&first).y, -10.0);
    tick.set(1);
    ui.wait_for_idle();
    let seen = seen.borrow();
    let last = seen.last().unwrap();
    assert!(Rc::ptr_eq(&first.nested_scroll_connection(), &last.nested_scroll_connection()));
    // The first's connection asks the closure given last.
    assert_eq!(scroll(&first).y, 0.0);
}

/// A spec that's changed is a new behavior: `is_some` alone wouldn't tell.
#[test]
fn a_new_fling_spec_is_taken() {
    let which = mutable_state_of(false);
    let handle = which.handle();
    let seen: Rc<RefCell<Vec<TopAppBarScrollBehavior>>> = Rc::default();
    let keep = seen.clone();
    let mut ui = window(move || {
        let on = handle.get();
        let keep = keep.clone();
        MaterialTheme!({
            let (first, second) = remember(|| {
                (
                    compose_animation::exponential_decay(1.0, 0.1),
                    compose_animation::exponential_decay(2.0, 0.1),
                )
            });
            let behavior = TopAppBarDefaults::enter_always_scroll_behavior_with(
                None,
                None,
                Some(if on { second } else { first }),
            );
            keep.borrow_mut().push(behavior);
        });
    });
    ui.wait_for_idle();
    which.set(true);
    ui.wait_for_idle();
    let seen = seen.borrow();
    assert_ne!(seen[0].fling_animation_spec(), seen.last().unwrap().fling_animation_spec());
}

/// A slot's style merges into the ambient one, as `ProvideContentColorTextStyle`.
#[test]
fn a_titles_style_merges_into_whats_around_it() {
    let seen = Rc::new(RefCell::new(None));
    let keep = seen.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let keep = keep.clone();
            let mut ambient = LOCAL_TEXT_STYLE.current();
            ambient.color = Color::RED;
            CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(ambient), move || {
                let keep = keep.clone();
                TopAppBar!(move || {
                    keep.replace(Some(LOCAL_TEXT_STYLE.current()));
                });
            });
        });
    });
    ui.wait_for_idle();
    let style: compose_ui::TextStyle = seen.borrow().clone().unwrap();
    assert_eq!(style.color, Color::RED);
}

/// An endless list's estimates don't fit: they stop at the largest, without a panic.
#[test]
fn an_endless_list_under_a_bar_reading_it() {
    let captured = Rc::new(Cell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let list = remember_lazy_list_state!();
            keep.set(Some(list));
            let behavior = TopAppBarDefaults::pinned_scroll_behavior_for(&list);
            Column!({
                TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior.clone()));
                LazyColumn!(
                    modifier = Modifier.height(300.dp()),
                    state = list,
                    |scope: LazyListScope| {
                        scope.items(i32::MAX as usize, |_| Spacer(Modifier.height(50.dp())));
                    }
                );
            });
        });
    });
    ui.wait_for_idle();
    let list = captured.get().unwrap();
    list.dispatch_raw_delta(5000.0);
    ui.wait_for_idle();
    let indicator = list.scroll_indicator_state().unwrap();
    assert_eq!(indicator.content_size(), i32::MAX);
    assert_eq!(indicator.scroll_offset(), 5000);
}

/// `pointerInput(Unit) {}`: what's under a bar isn't clicked.
#[test]
fn a_bar_blocks_whats_under_it() {
    let clicks = Rc::new(Cell::new(0));
    let count = clicks.clone();
    let mut ui = window(move || {
        let count = count.clone();
        MaterialTheme!({
            let count = count.clone();
            Box!(modifier = Modifier.fill_max_size(), {
                let count = count.clone();
                Box!(
                    modifier =
                        Modifier.fill_max_size().clickable(move || count.set(count.get() + 1))
                );
                TopAppBar!({ Text!("Title") }, modifier = Modifier.test_tag(BAR));
            });
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.down(Offset::new(300.0, 30.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(clicks.get(), 0);
}

/// A bar in right to left: the policy places relatively.
#[test]
fn a_bar_in_rtl_is_mirrored() {
    let mut ui = window(|| {
        CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl),
            || {
                MaterialTheme!({
                    TopAppBar!(
                        { Spacer(Modifier.width(100.dp()).height(20.dp()).test_tag(TITLE)) },
                        navigation_icon = Some(|| FakeIcon(NAVIGATION)),
                        actions = |_| FakeIcon(ACTIONS),
                        window_insets = no_insets(),
                        content_padding =
                            PaddingValues { start: 10.dp(), end: 20.dp(), ..PaddingValues::ZERO }
                    );
                });
            },
        );
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, NAVIGATION).right, 400.0 - 4.0 - 10.0);
    assert_eq!(bounds(&mut ui, ACTIONS).left, 4.0 + 20.0);
    assert_eq!(bounds(&mut ui, TITLE).right, 400.0 - 38.0 - 4.0);
}

/// Measured with an infinite height, a bar isn't collapsed: there's nothing to collapse from.
#[test]
fn a_bar_with_an_infinite_height_ignores_the_offset() {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::enter_always_scroll_behavior();
            keep.replace(Some(behavior.clone()));
            Column!(modifier = Modifier.vertical_scroll(remember_scroll_state!()), {
                TopAppBar!(
                    { Text!("Title") },
                    modifier = Modifier.test_tag(BAR),
                    window_insets = no_insets(),
                    scroll_behavior = Some(behavior.clone())
                );
            });
        });
    });
    ui.wait_for_idle();
    let behavior: TopAppBarScrollBehavior = captured.borrow().clone().unwrap();
    behavior.state().set_height_offset(-20.0);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
}

/// A state's initial offset isn't kept in the limit, as Compose's constructor: only what's set is.
#[test]
fn an_initial_offset_is_as_given() {
    let owned = TopAppBarState::new(-10.0, -50.0, 0.0);
    assert_eq!(owned.height_offset(), -50.0);
    owned.set_height_offset(-50.0);
    assert_eq!(owned.height_offset(), -10.0);
}

/// Enter-always moves the bar before the content scrolls, not after: what the content took only
/// counts for the overlap.
#[test]
fn enter_always_moves_the_bar_only_before_the_scroll() {
    let owned = TopAppBarState::new(-64.0, 0.0, 0.0);
    let behavior = TopAppBarScrollBehavior::enter_always(*owned);
    let connection = behavior.nested_scroll_connection();
    connection.on_post_scroll(
        Offset::new(0.0, -30.0),
        Offset::ZERO,
        compose_ui::NestedScrollSource::UserInput,
    );
    assert_eq!(owned.height_offset(), 0.0);
    assert_eq!(owned.content_offset(), -30.0);
    let taken = connection
        .on_pre_scroll(Offset::new(0.0, -10.0), compose_ui::NestedScrollSource::UserInput);
    assert_eq!(taken, Offset::new(0.0, -10.0));
    assert_eq!(owned.height_offset(), -10.0);
}

/// Exit-always: back at the content's start only with the bar at an end does it forget the offset.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, ExitAlwaysScrollBehavior.onPostFling
#[test]
fn exit_always_resets_the_offset_with_the_bar_at_an_end() {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let scope = remember_coroutine_scope();
            let behavior = BottomAppBarDefaults::exit_always_scroll_behavior_with(
                remember_bottom_app_bar_state!(-80.0, -40.0, -50.0),
                None,
                None,
                None,
            );
            keep.replace(Some((behavior, scope)));
        });
    });
    ui.wait_for_idle();
    let (behavior, scope) = captured.borrow().clone().unwrap();
    let state = behavior.state();
    let fling = |velocity: f32| {
        let behavior = behavior.clone();
        scope.launch(async move {
            let connection = behavior.nested_scroll_connection();
            connection.on_post_fling(Offset::ZERO, Offset::new(0.0, velocity)).await;
        });
    };
    fling(500.0);
    ui.wait_for_idle();
    assert_eq!(state.content_offset(), -50.0, "the bar's half way");
    state.set_height_offset(0.0);
    fling(500.0);
    ui.wait_for_idle();
    assert_eq!(state.content_offset(), 0.0);
}

/// Letting go of the bar fast carries it on: nearer the start, it still goes all the way.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, settleAppBar
#[test]
fn a_fast_drag_of_the_bar_carries_on() {
    let (mut ui, _) = two_row_bar(Bar::Large, false, None, 0.0);
    ui.on_node_with_tag_unmerged(BAR).perform_touch_input(|touch| {
        touch.swipe_up_with(140.0, 110.0, 20);
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
}

/// Part way collapsed, the two-row bar's container is part way, eased.
#[test]
fn a_two_row_bars_container_goes_by_the_fraction() {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::exit_until_collapsed_scroll_behavior();
            keep.replace(Some(behavior.clone()));
            LargeTopAppBar!(
                { Text!("Title") },
                window_insets = no_insets(),
                colors = TopAppBarColors {
                    container_color: Color::RED,
                    scrolled_container_color: Color::BLUE,
                    ..TopAppBarDefaults::top_app_bar_colors()
                },
                scroll_behavior = Some(behavior)
            );
        });
    });
    ui.wait_for_idle();
    let behavior: TopAppBarScrollBehavior = captured.borrow().clone().unwrap();
    collapse(&mut ui, &behavior, 44.0, 44.0);
    assert_has(&mut ui, Color::from_argb(0xFFB54F7E));
}

/// A title at the end stops before the actions.
#[test]
fn a_title_at_the_end_is_clear_of_the_actions() {
    let mut ui = window(|| {
        MaterialTheme!({
            TopAppBar!(
                { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
                actions = |_| FakeIcon(ACTIONS),
                window_insets = no_insets(),
                title_horizontal_alignment = compose_ui::Alignment::END
            );
        });
    });
    ui.wait_for_idle();
    let (title, actions) = (bounds(&mut ui, TITLE), bounds(&mut ui, ACTIONS));
    assert!(title.right <= actions.left, "{title:?} {actions:?}");
    assert!(title.right >= actions.left - 4.0, "up against them: {title:?} {actions:?}");
}

/// `TwoRowsTopAppBar` has no padding under its bottom title: it's at the row's bottom.
#[test]
fn two_rows_have_no_title_padding() {
    let mut ui = window(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.test_tag(BAR), {
                TwoRowsTopAppBar!(
                    |expanded: bool| {
                        if expanded {
                            Text!("Title", modifier = Modifier.test_tag(TITLE));
                        }
                    },
                    collapsed_height = 40.dp(),
                    expanded_height = 120.dp(),
                    window_insets = no_insets()
                );
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, TITLE).bottom, 120.0);
}

/// `can_scroll` on the overloads taking a scrollable.
#[test]
fn the_overloads_for_a_scrollable_take_can_scroll() {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let _ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let list = remember_lazy_list_state!();
            let never: Option<Callback<dyn Fn() -> bool>> = Some(Callback::new(|| false));
            let pinned = TopAppBarDefaults::pinned_scroll_behavior_with(never.clone());
            let enter = TopAppBarDefaults::enter_always_scroll_behavior_for_with(
                &list,
                never.clone(),
                None,
                None,
            );
            let pinned_for =
                TopAppBarDefaults::pinned_scroll_behavior_for_with(&list, never.clone());
            keep.replace(Some((pinned, enter, pinned_for)));
        });
    });
    let (pinned, enter, pinned_for) = captured.borrow().clone().unwrap();
    for behavior in [pinned, enter, pinned_for] {
        let connection = behavior.nested_scroll_connection();
        connection.on_post_scroll(
            Offset::new(0.0, -30.0),
            Offset::ZERO,
            compose_ui::NestedScrollSource::UserInput,
        );
        assert_eq!(behavior.state().content_offset(), 0.0);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_scrolledLazyColumn
#[test]
fn pinned_over_a_list() {
    turns_green_on_a_swipe_up(Kind::Pinned, Body::List);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_preScrolledLazyColumn
#[test]
fn pinned_over_a_list_scrolled_already_is_green_from_the_start() {
    let mut ui = color_screen(Kind::Pinned, Body::List, false, 30);
    assert_has(&mut ui, Color::GREEN);
    let height = bounds(&mut ui, CONTENT).height();
    swipe_up(&mut ui, height - 200.0, height - 1000.0);
    assert_has(&mut ui, Color::GREEN);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_customTwoRows_scrolled_positioning
#[test]
fn custom_rows_scrolled_collapse() {
    let captured = Rc::new(RefCell::new(None));
    let keep = captured.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let behavior = TopAppBarDefaults::exit_until_collapsed_scroll_behavior();
            keep.replace(Some(behavior.clone()));
            Box!(modifier = Modifier.test_tag(BAR), {
                TwoRowsTopAppBar!(
                    |expanded: bool| {
                        if expanded {
                            Text!("Expanded Title");
                        } else {
                            Text!("Collapsed Title");
                        }
                    },
                    navigation_icon = Some(|| FakeIcon(NAVIGATION)),
                    actions = |_| FakeIcon(ACTIONS),
                    collapsed_height = 40.dp(),
                    expanded_height = 120.dp(),
                    window_insets = WindowInsets::new(13.dp(), 13.dp(), 13.dp(), 13.dp()),
                    scroll_behavior = Some(behavior.clone())
                );
            });
        });
    });
    ui.wait_for_idle();
    let behavior: TopAppBarScrollBehavior = captured.borrow().clone().unwrap();
    let (whole, third) = (80.0, 80.0 / 3.0);
    collapse(&mut ui, &behavior, third, third);
    let height = bounds(&mut ui, BAR).height();
    assert!((height - (120.0 - third + 26.0)).abs() <= 0.5, "{height}");
    collapse(&mut ui, &behavior, whole, whole + third);
    assert_eq!(bounds(&mut ui, BAR).height(), 40.0 + 26.0);
}

/// Without a subtitle the title is in a box of nothing more: no group of its own for what reads the
/// screen, as Compose's; with one, a group.
#[test]
fn only_a_title_with_a_subtitle_is_a_group() {
    let mut ui = window(|| {
        MaterialTheme!({
            TopAppBar!(
                { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
                modifier = Modifier.test_tag(BAR)
            );
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag_unmerged(TITLE).on_parent().assert(has_test_tag(BAR));
    let mut ui = window(|| {
        MaterialTheme!({
            TopAppBar!(
                { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
                modifier = Modifier.test_tag(BAR),
                subtitle = Some(|| Text!("Subtitle"))
            );
        });
    });
    ui.wait_for_idle();
    let parent = ui.on_node_with_tag_unmerged(TITLE).on_parent().fetch_semantics_snapshot();
    assert_eq!(
        parent.config.get(&compose_ui::SemanticsProperties::IS_TRAVERSAL_GROUP),
        Some(&true)
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_default_positioning_respectsWindowInsets
#[test]
fn a_centered_bars_rows_are_inside_its_insets() {
    let mut ui = window(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.test_tag(BAR), {
                CenterAlignedTopAppBar!(
                    { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
                    navigation_icon = Some(|| FakeIcon(NAVIGATION)),
                    actions = |_| FakeIcon(ACTIONS),
                    window_insets = WindowInsets::new(10.dp(), 10.dp(), 10.dp(), 10.dp())
                );
            });
        });
    });
    ui.wait_for_idle();
    let bar = bounds(&mut ui, BAR);
    assert_eq!(bar.height(), 64.0 + 20.0);
    let navigation = bounds(&mut ui, NAVIGATION);
    assert_eq!((navigation.left, navigation.top), (4.0 + 10.0, 10.0 + 20.0));
    let actions = bounds(&mut ui, ACTIONS);
    assert_eq!((actions.left, actions.top), (400.0 - 10.0 - 4.0 - 24.0, 10.0 + 20.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_subtitleDefaultStyle
#[test]
fn a_centered_titles_subtitle_is_in_label_medium() {
    let seen = Rc::new(RefCell::new(None));
    let keep = seen.clone();
    let mut ui = window(move || {
        let keep = keep.clone();
        MaterialTheme!({
            let keep = keep.clone();
            TopAppBar!(
                { Text!("Title") },
                subtitle = Some(move || {
                    keep.replace(Some(LOCAL_TEXT_STYLE.current()));
                }),
                title_horizontal_alignment = compose_ui::Alignment::CENTER_HORIZONTALLY
            );
        });
    });
    ui.wait_for_idle();
    let style: compose_ui::TextStyle = seen.borrow().clone().unwrap();
    assert_eq!(style, compose_ui::TextStyle::DEFAULT.merge(&Typography::default().label_medium));
}
