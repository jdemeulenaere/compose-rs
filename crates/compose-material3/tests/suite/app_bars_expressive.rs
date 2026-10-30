//! Compose's `AppBarTest.kt`, what `app_bar-expressive` brought: subtitles, the title's alignment,
//! the flexible bars, `TwoRowsTopAppBar`, content padding, the bars' drag and fling, the scrollable
//! overloads, the flexible and exit-always bottom bars. The rest is in `suite/app_bars.rs`.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Alignment, Color, IntSize, Modifier, PaddingValues, UnitExt};

const WIDTH: f32 = 400.0;
const BAR: &str = "bar";
const TITLE: &str = "title";
const SUBTITLE: &str = "subtitle";
const NAVIGATION: &str = "navigation";
const ACTIONS: &str = "actions";

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    let content = callback(content);
    let mut ui = ComposeTestRule::sized(IntSize::new(WIDTH as i32, 800), move || {
        let content = content.clone();
        MaterialTheme!({
            content();
        });
    });
    ui.wait_for_idle();
    ui
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag_unmerged(tag).fetch_semantics_snapshot().bounds
}

#[composable]
fn FakeIcon(tag: &'static str) {
    Spacer(Modifier.size(24.dp()).test_tag(tag));
}

fn no_insets() -> WindowInsets {
    WindowInsets::new(0.dp(), 0.dp(), 0.dp(), 0.dp())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_withSubtitle
#[test]
fn a_small_bar_shows_its_subtitle_under_its_title() {
    let mut ui = rule(|| {
        TopAppBar!(
            { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
            modifier = Modifier.test_tag(BAR),
            subtitle = Some(|| Text!("Subtitle", modifier = Modifier.test_tag(SUBTITLE)))
        );
    });
    let (title, subtitle) = (bounds(&mut ui, TITLE), bounds(&mut ui, SUBTITLE));
    assert_eq!(subtitle.top, title.bottom);
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
    assert_eq!(title.left, 16.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_withSubtitleAndCustomColors
#[test]
fn a_subtitle_takes_its_own_color_and_style() {
    let seen = std::rc::Rc::new(std::cell::Cell::new(None));
    let keep = seen.clone();
    let mut ui = rule(move || {
        let keep = keep.clone();
        TopAppBar!(
            { Text!("Title") },
            colors = TopAppBarColors {
                subtitle_content_color: Color::RED,
                ..TopAppBarDefaults::top_app_bar_colors()
            },
            subtitle = Some(move || {
                keep.set(Some((LOCAL_CONTENT_COLOR.current(), LOCAL_TEXT_STYLE.current())));
            })
        );
    });
    ui.wait_for_idle();
    let (color, style) = {
        let seen = seen.take();
        seen.expect("composed")
    };
    assert_eq!(color, Color::RED);
    assert_eq!(style, light_typography().label_medium.clone());
}

fn light_typography() -> Typography {
    Typography::default()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_centeredWithSubtitle_positioning
#[test]
fn a_title_aligned_to_the_center_is_centered_in_the_bar() {
    let mut ui = rule(|| {
        TopAppBar!(
            { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
            modifier = Modifier.test_tag(BAR),
            navigation_icon = Some(|| FakeIcon(NAVIGATION)),
            actions = |_| FakeIcon(ACTIONS),
            subtitle = Some(|| Text!("Subtitle", modifier = Modifier.test_tag(SUBTITLE))),
            title_horizontal_alignment = Alignment::CENTER_HORIZONTALLY
        );
    });
    let title = bounds(&mut ui, TITLE);
    let subtitle = bounds(&mut ui, SUBTITLE);
    // Rounded to a pixel: the column's what's centered, the subtitle the wider.
    assert!(((subtitle.left + subtitle.right) / 2.0 - WIDTH / 2.0).abs() <= 1.0, "{subtitle:?}");
    assert!(((title.left + title.right) / 2.0 - WIDTH / 2.0).abs() <= 1.0, "{title:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_centeredWithSubtitle_multiActions_positioning
#[test]
fn more_actions_leave_a_centered_title_where_it_is() {
    let mut ui = rule(|| {
        TopAppBar!(
            { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
            actions = |_| {
                Spacer(Modifier.size(24.dp()));
                Spacer(Modifier.size(24.dp()));
                FakeIcon(ACTIONS);
            },
            title_horizontal_alignment = Alignment::CENTER_HORIZONTALLY
        );
    });
    let title = bounds(&mut ui, TITLE);
    assert!(((title.left + title.right) / 2.0 - WIDTH / 2.0).abs() <= 0.5, "{title:?}");
    assert_eq!(bounds(&mut ui, ACTIONS).right, WIDTH - 4.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_longTitle_doesNotOverlapActions
#[test]
fn a_long_title_stops_before_the_actions() {
    let mut ui = rule(|| {
        TopAppBar!(
            { Text!("Title ".repeat(40), modifier = Modifier.test_tag(TITLE)) },
            navigation_icon = Some(|| FakeIcon(NAVIGATION)),
            actions = |_| FakeIcon(ACTIONS)
        );
    });
    assert!(bounds(&mut ui, TITLE).right <= bounds(&mut ui, ACTIONS).left);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, smallTopAppBar_fitsTextIfHeightTooSmall
#[test]
fn a_bar_too_low_for_its_title_grows_and_collapses_that_far() {
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
    let keep = captured.clone();
    let mut ui = rule(move || {
        let behavior = TopAppBarDefaults::enter_always_scroll_behavior();
        keep.replace(Some(behavior.clone()));
        TopAppBar!(
            { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
            modifier = Modifier.test_tag(BAR),
            expanded_height = 0.dp(),
            subtitle = Some(|| Text!("Subtitle", modifier = Modifier.test_tag(SUBTITLE))),
            scroll_behavior = Some(behavior)
        );
    });
    let total = bounds(&mut ui, TITLE).height() + bounds(&mut ui, SUBTITLE).height();
    assert_eq!(bounds(&mut ui, BAR).height(), total);
    let behavior = captured.borrow().clone().unwrap();
    assert_eq!(behavior.state().height_offset_limit(), -total);
}

/// `content_padding` moves all three and adds to the height.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, TopAppBarMeasurePolicy
#[test]
fn the_content_padding_moves_the_parts() {
    let mut ui = rule(|| {
        TopAppBar!(
            { Text!("Title", modifier = Modifier.test_tag(TITLE)) },
            modifier = Modifier.test_tag(BAR),
            navigation_icon = Some(|| FakeIcon(NAVIGATION)),
            actions = |_| FakeIcon(ACTIONS),
            window_insets = no_insets(),
            content_padding = PaddingValues::new(10.dp(), 6.dp(), 20.dp(), 2.dp())
        );
    });
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0 + 8.0);
    assert_eq!(bounds(&mut ui, NAVIGATION).left, 10.0 + 4.0);
    assert_eq!(bounds(&mut ui, ACTIONS).right, WIDTH - 20.0 - 4.0);
    // Centered in what the paddings leave.
    let navigation = bounds(&mut ui, NAVIGATION);
    assert_eq!(navigation.top, ((64.0 + 8.0 + 6.0 - 2.0) - 24.0f32) / 2.0);
}

/// Unspecified or infinite, the default height, as Compose's bars.
#[test]
fn an_unspecified_height_is_the_default() {
    let mut ui = rule(|| {
        TopAppBar!(
            { Text!("Title") },
            modifier = Modifier.test_tag(BAR),
            expanded_height = compose_ui::Dp::UNSPECIFIED
        );
    });
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
}

#[composable]
fn Flexible(large: bool, subtitled: bool) {
    let subtitle =
        subtitled.then_some(|| Text!("Subtitle", modifier = Modifier.test_tag(SUBTITLE)));
    if large {
        LargeFlexibleTopAppBar!(
            { Text!("Title") },
            modifier = Modifier.test_tag(BAR),
            subtitle = subtitle
        );
    } else {
        MediumFlexibleTopAppBar!(
            { Text!("Title") },
            modifier = Modifier.test_tag(BAR),
            subtitle = subtitle
        );
    }
}

/// The flexible bars' heights go by the subtitle.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, MediumFlexibleTopAppBar
#[test]
fn the_flexible_bars_are_as_high_as_compose_says() {
    for (large, subtitled, height) in
        [(false, false, 112.0), (false, true, 136.0), (true, false, 120.0), (true, true, 152.0)]
    {
        let mut ui = rule(move || Flexible(large, subtitled));
        assert_eq!(bounds(&mut ui, BAR).height(), height, "{large} {subtitled}");
    }
}

/// Their styles: the title's and the subtitle's, each row's.
#[test]
fn the_flexible_bars_styles_are_composes() {
    thread_local! {
        static SEEN: std::cell::RefCell<Vec<(&'static str, compose_ui::TextStyle)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    fn note(name: &'static str) {
        SEEN.with(|seen| seen.borrow_mut().push((name, LOCAL_TEXT_STYLE.current())));
    }
    let mut ui = rule(|| {
        LargeFlexibleTopAppBar!(|| note("title"), subtitle = Some(|| note("subtitle")));
    });
    ui.wait_for_idle();
    let typography = Typography::default();
    let seen = SEEN.with(|seen| seen.borrow().clone());
    let styles = |name: &str| -> Vec<compose_ui::TextStyle> {
        seen.iter().filter(|it| it.0 == name).map(|it| it.1.clone()).collect()
    };
    assert_eq!(styles("title"), [typography.title_large.clone(), typography.display_small.clone()]);
    assert_eq!(
        styles("subtitle"),
        [typography.label_medium.clone(), typography.title_medium.clone()]
    );
}

fn custom_two_rows(collapsed: f32, expanded: f32, windowed: bool) -> ComposeTestRule {
    rule(move || {
        let title = move |is_expanded: bool| {
            if is_expanded {
                Text!("Expanded Title", modifier = Modifier.test_tag(TITLE));
            } else {
                Text!("Collapsed Title", modifier = Modifier.test_tag("small"));
            }
        };
        let window_insets = if windowed {
            WindowInsets::new(13.dp(), 13.dp(), 13.dp(), 13.dp())
        } else {
            no_insets()
        };
        Box!(
            modifier = Modifier.test_tag(BAR),
            content = move |_| {
                TwoRowsTopAppBar!(
                    title,
                    navigation_icon = Some(|| FakeIcon(NAVIGATION)),
                    actions = |_| FakeIcon(ACTIONS),
                    collapsed_height = collapsed.dp(),
                    expanded_height = expanded.dp(),
                    window_insets = window_insets.clone()
                );
            }
        );
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_customTwoRows_expanded
#[test]
fn custom_rows_are_told_which_they_are() {
    let mut ui = custom_two_rows(36.0, 112.0, false);
    assert_eq!(bounds(&mut ui, BAR).height(), 112.0);
    let (small, big) = (bounds(&mut ui, "small"), bounds(&mut ui, TITLE));
    assert!(small.top < 36.0, "{small:?}");
    assert!(big.bottom <= 112.0 && big.top >= 36.0, "{big:?}");
    // The icons, in the collapsed row.
    let navigation = bounds(&mut ui, NAVIGATION);
    assert_eq!(navigation.top, (36.0 - 24.0) / 2.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_customTwoRows_scrolled_positioning
#[test]
fn custom_rows_are_under_the_inset() {
    let mut ui = custom_two_rows(40.0, 120.0, true);
    // The inset's top and bottom both, on the collapsed row, as `windowInsetsPadding`.
    assert_eq!(bounds(&mut ui, BAR).height(), 120.0 + 26.0);
    assert_eq!(bounds(&mut ui, NAVIGATION).top, 13.0 + (40.0 - 24.0) / 2.0);
}

#[test]
#[should_panic(expected = "The expandedHeight is expected to be greater or equal")]
fn a_bar_that_expands_to_less_than_it_collapses_is_refused() {
    let _ = rule(|| {
        MediumTopAppBar!(
            { Text!("Title") },
            collapsed_height = 100.dp(),
            expanded_height = 50.dp()
        );
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_dragSnapToCollapsed
#[test]
fn dragging_a_bar_snaps_it_to_the_nearer_end() {
    let mut ui = rule(|| {
        let behavior = TopAppBarDefaults::exit_until_collapsed_scroll_behavior();
        LargeTopAppBar!(
            { Text!("Title") },
            modifier = Modifier.test_tag(BAR),
            scroll_behavior = Some(behavior)
        );
    });
    assert_eq!(bounds(&mut ui, BAR).height(), 152.0);
    // A slight drag: back to expanded.
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.down(compose_ui::Offset::new(0.0, 132.0));
        touch.move_to(compose_ui::Offset::new(0.0, 112.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 152.0);
    // Over half way: on to collapsed.
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.down(compose_ui::Offset::new(0.0, 132.0));
        touch.move_to(compose_ui::Offset::new(0.0, 40.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_dragWithSnapDisabled
#[test]
fn without_a_snap_a_dragged_bar_stays_where_it_is() {
    let mut ui = rule(|| {
        let behavior =
            TopAppBarDefaults::exit_until_collapsed_scroll_behavior_with(None, None, None);
        LargeTopAppBar!(
            { Text!("Title") },
            modifier = Modifier.test_tag(BAR),
            scroll_behavior = Some(behavior)
        );
    });
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.down(compose_ui::Offset::new(100.0, 132.0));
        touch.move_to(compose_ui::Offset::new(100.0, 52.0));
        touch.up();
    });
    ui.wait_for_idle();
    let height = bounds(&mut ui, BAR).height();
    assert!(height < 152.0 && height > 64.0, "{height}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_smallPinnedDraggedAppBar
#[test]
fn a_pinned_bar_doesnt_follow_a_drag() {
    let mut ui = rule(|| {
        let behavior = TopAppBarDefaults::pinned_scroll_behavior();
        TopAppBar!(
            { Text!("Title") },
            modifier = Modifier.test_tag(BAR),
            scroll_behavior = Some(behavior)
        );
    });
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.down(compose_ui::Offset::new(100.0, 60.0));
        touch.move_to(compose_ui::Offset::new(100.0, 10.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0);
}

/// A fling off a drag carries the bar on before it snaps.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, settleAppBar
#[test]
fn a_fling_carries_a_bar_on() {
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
    let keep = captured.clone();
    let mut ui = rule(move || {
        let fling = TopAppBarDefaults::fling_animation_spec();
        let behavior =
            TopAppBarDefaults::exit_until_collapsed_scroll_behavior_with(None, None, Some(fling));
        keep.replace(Some(behavior.clone()));
        LargeTopAppBar!(
            { Text!("Title") },
            modifier = Modifier.test_tag(BAR),
            scroll_behavior = Some(behavior)
        );
    });
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.swipe(
            compose_ui::Offset::new(100.0, 140.0),
            compose_ui::Offset::new(100.0, 110.0),
            30,
        );
    });
    ui.wait_for_idle();
    // 30 px up by hand; the fling goes well past, there being no snap to end it.
    let behavior = captured.borrow().clone().unwrap();
    assert!(behavior.state().height_offset() < -30.0, "{}", behavior.state().height_offset());
}

/// `can_scroll` says no: the bar stays.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, ExitUntilCollapsedScrollBehavior
#[test]
fn a_behavior_that_cant_scroll_leaves_the_bar() {
    let scope_ui = rule(|| {});
    drop(scope_ui);
    let owned = TopAppBarState::new(-100.0, 0.0, 0.0);
    let state = *owned;
    let mut ui = rule(move || {
        let scope = remember_coroutine_scope();
        let behavior = remember(move || {
            TopAppBarScrollBehavior::exit_until_collapsed(state, scope).with_can_scroll(|| false)
        });
        let connection = behavior.nested_scroll_connection();
        connection.on_pre_scroll(
            compose_ui::Offset::new(0.0, -50.0),
            compose_ui::NestedScrollSource::UserInput,
        );
        connection.on_post_scroll(
            compose_ui::Offset::new(0.0, -50.0),
            compose_ui::Offset::ZERO,
            compose_ui::NestedScrollSource::UserInput,
        );
    });
    ui.wait_for_idle();
    assert_eq!(state.height_offset(), 0.0);
    assert_eq!(state.content_offset(), 0.0);
    drop(owned);
}

/// `height` by the state: the overlap is the content's, whichever way, and "all" for a list not at
/// its start with nothing seen to scroll.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_preScrolledLazyColumn
#[test]
fn a_pinned_bar_over_a_list_scrolled_already_takes_the_scrolled_color() {
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
    let keep = captured.clone();
    let mut ui = rule(move || {
        let list = remember_lazy_list_state!(5, 0);
        let behavior = TopAppBarDefaults::pinned_scroll_behavior_for(&list);
        keep.replace(Some(behavior.clone()));
        TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior));
        LazyColumn!(modifier = Modifier.height(300.dp()), state = list, |scope: LazyListScope| {
            scope.items(40, |_| Spacer(Modifier.height(50.dp())));
        });
    });
    ui.wait_for_idle();
    let behavior = captured.borrow().clone().unwrap();
    assert_eq!(behavior.state().overlapped_fraction(), 1.0);
}

/// At its start, by contrast, nothing's overlapped.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_pinned_changeColors_scrolledLazyColumn
#[test]
fn a_pinned_bar_over_a_list_at_its_start_doesnt() {
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
    let keep = captured.clone();
    let mut ui = rule(move || {
        let list = remember_lazy_list_state!();
        let behavior = TopAppBarDefaults::pinned_scroll_behavior_for(&list);
        keep.replace(Some(behavior.clone()));
        TopAppBar!({ Text!("Title") }, scroll_behavior = Some(behavior));
        LazyColumn!(modifier = Modifier.height(300.dp()), state = list, |scope: LazyListScope| {
            scope.items(40, |_| Spacer(Modifier.height(50.dp())));
        });
    });
    ui.wait_for_idle();
    let behavior = captured.borrow().clone().unwrap();
    assert_eq!(behavior.state().overlapped_fraction(), 0.0);
}

/// The overlap's in the content's offset either way.
#[test]
fn the_overlap_goes_by_how_far_the_content_is_scrolled() {
    let owned = TopAppBarState::new(-100.0, 0.0, 0.0);
    let state = *owned;
    state.set_content_offset(-50.0);
    assert_eq!(state.overlapped_fraction(), 0.5);
    state.set_content_offset(50.0);
    assert_eq!(state.overlapped_fraction(), 0.5);
    state.set_content_offset(-500.0);
    assert_eq!(state.overlapped_fraction(), 1.0);
    drop(owned);
}

// ---- Bottom ------------------------------------------------------------------------------------

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBarWithCustomArrangement_heightIsFromSpec
#[test]
fn a_flexible_bottom_bar_is_64_dp() {
    let mut ui = rule(|| {
        FlexibleBottomAppBar!(
            modifier = Modifier.test_tag(BAR),
            horizontal_arrangement = BottomAppBarDefaults::flexible_fixed_horizontal_arrangement(),
            window_insets = no_insets(),
            |_| {}
        );
    });
    let bar = bounds(&mut ui, BAR);
    assert_eq!((bar.width(), bar.height()), (WIDTH, 64.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBarWithCustomHeight
#[test]
fn a_flexible_bottom_bar_takes_its_height() {
    for (given, height) in [(128.0, 128.0), (-1.0, 64.0), (f32::INFINITY, 64.0)] {
        let mut ui = rule(move || {
            FlexibleBottomAppBar!(
                modifier = Modifier.test_tag(BAR),
                expanded_height = compose_ui::Dp(given),
                window_insets = no_insets(),
                |_| {}
            );
        });
        assert_eq!(bounds(&mut ui, BAR).height(), height);
    }
}

/// Its content is spaced as asked, within its padding: by default, apart.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, FlexibleBottomAppBar
#[test]
fn a_flexible_bottom_bars_content_is_spaced() {
    let mut ui = rule(|| {
        FlexibleBottomAppBar!(window_insets = no_insets(), |_| {
            FakeIcon(NAVIGATION);
            FakeIcon(ACTIONS);
        });
    });
    assert_eq!(bounds(&mut ui, NAVIGATION).left, 16.0);
    assert_eq!(bounds(&mut ui, ACTIONS).right, WIDTH - 16.0);
    let mut ui = rule(|| {
        FlexibleBottomAppBar!(
            window_insets = no_insets(),
            horizontal_arrangement = BottomAppBarDefaults::flexible_fixed_horizontal_arrangement(),
            |_| {
                FakeIcon(NAVIGATION);
                FakeIcon(ACTIONS);
            }
        );
    });
    let (first, second) = (bounds(&mut ui, NAVIGATION), bounds(&mut ui, ACTIONS));
    assert_eq!(second.left - first.right, 32.0);
    assert_eq!((first.left + second.right) / 2.0, WIDTH / 2.0);
}

/// The content overload, 80 dp, its content in a row inside the padding.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_default_positioning
#[test]
fn a_bottom_bar_with_content() {
    let mut ui = rule(|| {
        ContentBottomAppBar!(
            modifier = Modifier.test_tag(BAR),
            window_insets = no_insets(),
            |_| FakeIcon(ACTIONS)
        );
    });
    assert_eq!(bounds(&mut ui, BAR).height(), 80.0);
    let icon = bounds(&mut ui, ACTIONS);
    assert_eq!(icon.left, 4.0);
    assert_eq!(icon.top, 4.0 + (76.0 - 24.0) / 2.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_exitAlways_outOfRangeOffsetHandled
#[test]
fn an_offset_out_of_range_leaves_no_bar() {
    let mut ui = rule(|| {
        let behavior = BottomAppBarDefaults::exit_always_scroll_behavior();
        let state = behavior.state();
        state.set_height_offset_limit(-f32::MAX);
        state.set_height_offset(-1000.0);
        ContentBottomAppBar!(
            modifier = Modifier.test_tag(BAR),
            scroll_behavior = Some(behavior),
            |_| {}
        );
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 0.0);
}

/// Scrolling forward takes it away, whole, and back brings it; a drag goes the other way round,
/// its height being below the finger.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBar_exitAlways_scaffoldWithFAB_scrolled_positioning
#[test]
fn exit_always_goes_with_the_content() {
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
    let keep = captured.clone();
    let mut ui = rule(move || {
        let behavior = BottomAppBarDefaults::exit_always_scroll_behavior();
        keep.replace(Some(behavior.clone()));
        BottomAppBar!(
            |_| {},
            modifier = Modifier.test_tag(BAR),
            window_insets = no_insets(),
            scroll_behavior = Some(behavior)
        );
    });
    ui.wait_for_idle();
    let behavior: BottomAppBarScrollBehavior = captured.borrow().clone().unwrap();
    assert_eq!(behavior.state().height_offset_limit(), -80.0);
    let connection = behavior.nested_scroll_connection();
    connection.on_post_scroll(
        compose_ui::Offset::new(0.0, -30.0),
        compose_ui::Offset::ZERO,
        compose_ui::NestedScrollSource::UserInput,
    );
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 50.0);
    assert_eq!(behavior.state().collapsed_fraction(), 30.0 / 80.0);
    assert_eq!(behavior.state().content_offset(), -30.0);
    // The pre-scroll takes nothing.
    assert_eq!(
        connection.on_pre_scroll(
            compose_ui::Offset::new(0.0, -10.0),
            compose_ui::NestedScrollSource::UserInput,
        ),
        compose_ui::Offset::ZERO
    );
    connection.on_post_scroll(
        compose_ui::Offset::new(0.0, 30.0),
        compose_ui::Offset::ZERO,
        compose_ui::NestedScrollSource::UserInput,
    );
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, BAR).height(), 80.0);
    assert_eq!(behavior.state().content_offset(), 0.0);
}

/// Dragged down, it collapses.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, BottomAppBarLayout
#[test]
fn dragging_a_bottom_bar_down_takes_it_away() {
    let mut ui = rule(|| {
        let behavior = BottomAppBarDefaults::exit_always_scroll_behavior_with(
            remember_bottom_app_bar_state!(),
            None,
            None,
            None,
        );
        ContentBottomAppBar!(
            modifier = Modifier.test_tag(BAR),
            window_insets = no_insets(),
            scroll_behavior = Some(behavior),
            |_| {}
        );
    });
    ui.on_node_with_tag(BAR).perform_touch_input(|touch| {
        touch.down(compose_ui::Offset::new(100.0, 10.0));
        touch.move_to(compose_ui::Offset::new(100.0, 40.0));
        touch.up();
    });
    ui.wait_for_idle();
    let height = bounds(&mut ui, BAR).height();
    assert!(height < 80.0 && height > 40.0, "{height}");
}

/// Its defaults, Compose's.
#[test]
fn the_bottom_bars_defaults_are_composes() {
    assert_eq!(BottomAppBarDefaults::CONTAINER_ELEVATION, 0.dp());
    assert_eq!(BottomAppBarDefaults::FLEXIBLE_BOTTOM_APP_BAR_HEIGHT, 64.dp());
    assert_eq!(
        BottomAppBarDefaults::FLEXIBLE_CONTENT_PADDING,
        PaddingValues::new(16.dp(), 0.dp(), 16.dp(), 0.dp())
    );
    assert_eq!(
        TopAppBarDefaults::MEDIUM_FLEXIBLE_APP_BAR_WITHOUT_SUBTITLE_EXPANDED_HEIGHT,
        112.dp()
    );
    assert_eq!(TopAppBarDefaults::MEDIUM_FLEXIBLE_APP_BAR_WITH_SUBTITLE_EXPANDED_HEIGHT, 136.dp());
    assert_eq!(
        TopAppBarDefaults::LARGE_FLEXIBLE_APP_BAR_WITHOUT_SUBTITLE_EXPANDED_HEIGHT,
        120.dp()
    );
    assert_eq!(TopAppBarDefaults::LARGE_FLEXIBLE_APP_BAR_WITH_SUBTITLE_EXPANDED_HEIGHT, 152.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_longTextDoesNotOverflowToActions
#[test]
fn a_long_centered_title_stops_before_the_actions() {
    let mut ui = rule(|| {
        CenterAlignedTopAppBar!(
            { Text!("x".repeat(45), modifier = Modifier.test_tag(TITLE)) },
            actions = |_| {
                Spacer(Modifier.size(100.dp()).test_tag(ACTIONS));
            }
        );
    });
    assert!(bounds(&mut ui, TITLE).right <= bounds(&mut ui, ACTIONS).left);
    assert!(bounds(&mut ui, TITLE).left >= 12.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, centerAlignedTopAppBar_longTextDoesNotOverflowToNavigation
#[test]
fn a_long_centered_title_starts_after_the_navigation_icon() {
    let mut ui = rule(|| {
        CenterAlignedTopAppBar!(
            { Text!("x".repeat(45), modifier = Modifier.test_tag(TITLE)) },
            navigation_icon = Some(|| {
                Spacer(Modifier.size(100.dp()).test_tag(NAVIGATION));
            })
        );
    });
    assert!(bounds(&mut ui, TITLE).left >= bounds(&mut ui, NAVIGATION).right);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, mediumFlexibleTopAppBar_fitsTextIfHeightTooSmall_expanded
#[test]
fn a_flexible_bars_row_grows_for_a_title_too_tall() {
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
    let keep = captured.clone();
    let mut ui = rule(move || {
        let behavior = TopAppBarDefaults::enter_always_scroll_behavior();
        keep.replace(Some(behavior.clone()));
        Box!(
            modifier = Modifier.test_tag(BAR),
            content = move |_| {
                MediumFlexibleTopAppBar!(
                    { Spacer(Modifier.width(30.dp()).height(30.dp()).test_tag(TITLE)) },
                    subtitle = Some(|| Spacer(Modifier.width(30.dp()).height(20.dp()))),
                    collapsed_height = 64.dp(),
                    expanded_height = 64.dp(),
                    window_insets = no_insets(),
                    scroll_behavior = Some(behavior.clone())
                );
            }
        );
    });
    let behavior = captured.borrow().clone().unwrap();
    // Both rows' titles have the same tag: the last is the expanded row's.
    assert_eq!(behavior.state().height_offset_limit(), -50.0);
    assert_eq!(bounds(&mut ui, BAR).height(), 64.0 + 50.0);
}

/// Told the fling's velocity, a settle gives back what's left, or nothing at either end; and all
/// the way back, the content's offset is reset.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, settleAppBar
#[test]
fn a_settle_is_told_its_velocity() {
    let owned = TopAppBarState::new(-100.0, 0.0, -40.0);
    let state = *owned;
    let result = std::rc::Rc::new(std::cell::Cell::new(None));
    let told = result.clone();
    let mut ui = rule(move || {
        let scope = remember_coroutine_scope();
        let told = told.clone();
        remember(move || {
            let behavior = TopAppBarScrollBehavior::exit_until_collapsed(state, scope)
                .with_snap_animation_spec(None);
            let told = told.clone();
            scope.launch(async move {
                let connection = behavior.nested_scroll_connection();
                let left = connection
                    .on_post_fling(compose_ui::Offset::ZERO, compose_ui::Offset::new(0.0, 500.0))
                    .await;
                told.set(Some((left, state.content_offset())));
                state.set_height_offset(-100.0);
                let at_end = connection
                    .on_post_fling(compose_ui::Offset::ZERO, compose_ui::Offset::new(0.0, -500.0))
                    .await;
                assert_eq!(at_end, compose_ui::Offset::ZERO);
                state.set_height_offset(-50.0);
                let between = connection
                    .on_post_fling(compose_ui::Offset::ZERO, compose_ui::Offset::new(0.0, -500.0))
                    .await;
                assert_eq!(between, compose_ui::Offset::new(0.0, -500.0));
            });
        });
    });
    ui.wait_for_idle();
    let (left, content) = result.get().expect("ran");
    assert_eq!(left, compose_ui::Offset::ZERO, "expanded: nothing to settle");
    assert_eq!(content, 0.0, "scrolled back: no rounding errors left");
    drop(owned);
}

/// With a start padding, a title pushed clear of the icon goes that much further.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, TopAppBarMeasurePolicy
#[test]
fn a_title_pushed_clear_of_the_icon_keeps_the_padding() {
    let mut ui = rule(|| {
        CenterAlignedTopAppBar!(
            { Spacer(Modifier.width(300.dp()).height(10.dp()).test_tag(TITLE)) },
            navigation_icon = Some(|| {
                Spacer(Modifier.width(100.dp()).height(10.dp()));
            }),
            window_insets = no_insets(),
            content_padding = PaddingValues::new(10.dp(), 0.dp(), 0.dp(), 0.dp())
        );
    });
    // Its slot's 104 wide with its padding; 10 more, then the title's own 4.
    assert_eq!(bounds(&mut ui, TITLE).left, 104.0 + 10.0 + 4.0);
}

/// A title too tall for its row and its bottom padding sits in it, not over the small row.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt, TopAppBarMeasurePolicy
#[test]
fn a_tall_title_gives_up_its_bottom_padding() {
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut ui = rule(move || {
        let count = count.clone();
        MediumTopAppBar!(
            move || {
                let which = count.get();
                count.set(which + 1);
                let tag = if which % 2 == 0 { "small" } else { TITLE };
                Spacer(Modifier.width(30.dp()).height(50.dp()).test_tag(tag));
            },
            collapsed_height = 64.dp(),
            expanded_height = 64.dp(),
            window_insets = no_insets()
        );
    });
    assert_eq!(bounds(&mut ui, TITLE).top, 64.0);
}
