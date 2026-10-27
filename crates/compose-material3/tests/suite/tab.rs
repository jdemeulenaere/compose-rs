//! Compose's `TabTest.kt`, adapted: the tip's tab rows (`PrimaryTabRow`, `SecondaryTabRow` and
//! the scrollable ones) and tabs (`Tab`, `LeadingIconTab`): semantics, heights, the indicator's
//! and divider's positions (left to right and right to left), selection, the indicator's
//! animation, disabled tabs, font scale, the row's height.

use std::cell::Cell;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, Density, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, Role, UnitExt,
    platform::FixedTextMeasurer,
};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
}

/// Nodes with a selection, as Compose's `isSelectable()` (a private helper: compose-test has
/// no selection matcher yet).
fn is_selectable() -> Matcher {
    Matcher::new("selectable", |config| config.contains(&compose_ui::SemanticsProperties::SELECTED))
}

/// The selectable nodes' selection, in order.
fn selections(ui: &mut ComposeTestRule) -> Vec<bool> {
    let count = ui.count_nodes(&is_selectable());
    (0..count)
        .map(|index| {
            ui.on_all_nodes(is_selectable())
                .get(index)
                .fetch_semantics_node()
                .config
                .get(&compose_ui::SemanticsProperties::SELECTED)
                .copied()
                == Some(true)
        })
        .collect()
}

/// A red indicator `height` dp high under tab `index`, tagged "indicator".
fn red_indicator(scope: &TabIndicatorScope, index: usize, height: f32) {
    Box!(
        modifier = Modifier
            .tab_indicator_offset(scope, index, false)
            .fill_max_width()
            .height(height.dp())
            .background(Color::RED)
            .test_tag("indicator")
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, defaultSemantics
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, leadingIconTab_defaultSemantics
#[test]
fn a_tab_is_a_selected_enabled_clickable_tab_in_a_selectable_group() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                SecondaryTabRow!(0, modifier = Modifier.test_tag("row"), {
                    Tab!(
                        true,
                        || {},
                        modifier = Modifier.test_tag("tab"),
                        text = Some(|| Text!("Text"))
                    );
                });
                SecondaryTabRow!(0, modifier = Modifier.test_tag("leading row"), {
                    LeadingIconTab!(
                        true,
                        || {},
                        { Text!("Text") },
                        { Icon!(icons::filled::favorite(), None) },
                        modifier = Modifier.test_tag("leading")
                    );
                });
            });
        });
    });
    for tag in ["tab", "leading"] {
        let node = ui.on_node_with_tag(tag).fetch_semantics_node();
        assert_eq!(
            node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
            Some(Role::Tab),
            "{tag}"
        );
        assert_eq!(
            node.config.get(&compose_ui::SemanticsProperties::SELECTED).copied(),
            Some(true),
            "{tag}"
        );
        assert!(
            !node.config.contains(&compose_ui::SemanticsProperties::DISABLED)
                && node.config.contains(&compose_ui::SemanticsActions::ON_CLICK),
            "{tag}"
        );
    }
    for tag in ["row", "leading row"] {
        let group = ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node();
        assert!(
            group.config.contains(&compose_ui::SemanticsProperties::SELECTABLE_GROUP),
            "{tag}: {}",
            ui.dump_semantics()
        );
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, disabledSemantics
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, leadingIconTab_disabledSemantics
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, disabled_noClicks
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, leadingIconTab_disabled_noClicks
#[test]
fn a_disabled_tab_says_so_and_takes_no_clicks() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let click = || CLICKS.with(|cell| cell.set(cell.get() + 1));
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Column!({
                Tab!(
                    true,
                    click,
                    modifier = Modifier.test_tag("tab"),
                    enabled = false,
                    text = Some(|| Text!("Text"))
                );
                LeadingIconTab!(
                    true,
                    click,
                    { Text!("Text") },
                    { Icon!(icons::filled::favorite(), None) },
                    modifier = Modifier.test_tag("leading"),
                    enabled = false
                );
            });
        });
    });
    for tag in ["tab", "leading"] {
        let node = ui.on_node_with_tag(tag).fetch_semantics_node();
        assert_eq!(
            node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
            Some(Role::Tab)
        );
        assert_eq!(
            node.config.get(&compose_ui::SemanticsProperties::SELECTED).copied(),
            Some(true)
        );
        assert!(
            node.config.contains(&compose_ui::SemanticsProperties::DISABLED)
                && node.config.contains(&compose_ui::SemanticsActions::ON_CLICK),
            "{tag}"
        );
        ui.on_node_with_tag(tag).perform_click();
    }
    assert_eq!(CLICKS.with(Cell::get), 0);
}

fn height_of(content: impl Fn() + 'static) -> f32 {
    let content = std::rc::Rc::new(content);
    let mut ui = compose_test(move || {
        let content = content.clone();
        MaterialTheme!({
            let content = content.clone();
            Box!(modifier = Modifier.test_tag("box"), { content() });
        });
    });
    bounds(&mut ui, "box").height()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, textTab_height
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, iconTab_height
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, textAndIconTab_height
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, leadingIconTab_height
#[test]
fn tabs_are_48_dp_high_or_72_with_a_text_and_an_icon() {
    assert_eq!(height_of(|| Tab!(true, || {}, text = Some(|| Text!("Text")))), 48.0);
    assert_eq!(
        height_of(|| Tab!(true, || {}, icon = Some(|| Icon!(icons::filled::favorite(), None)))),
        48.0
    );
    assert_eq!(
        height_of(|| Tab!(
            true,
            || {},
            text = Some(|| Text!("Text and Icon")),
            icon = Some(|| Icon!(icons::filled::favorite(), None))
        )),
        72.0
    );
    assert_eq!(
        height_of(|| LeadingIconTab!(true, || {}, { Text!("Text") }, {
            Icon!(icons::filled::favorite(), None)
        })),
        48.0
    );
}

/// Two tabs in a secondary row with a red indicator, in `direction`.
fn two_tabs(direction: LayoutDirection, scrollable: bool) -> ComposeTestRule {
    compose_test(move || {
        let state = remember(|| mutable_state_of(0usize));
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
                let tabs = move || {
                    for (index, title) in ["TAB 1", "TAB 2"].into_iter().enumerate() {
                        Tab!(
                            state.get() == index,
                            move || state.set(index),
                            text = Some(move || Text!("{}", title))
                        );
                    }
                };
                let indicator =
                    Some(move |scope: TabIndicatorScope| red_indicator(&scope, state.get(), 1.0));
                Box!(modifier = Modifier.test_tag("box"), {
                    if scrollable {
                        SecondaryScrollableTabRow!(
                            state.get(),
                            modifier = Modifier.test_tag("row"),
                            indicator = indicator,
                            tabs = tabs
                        );
                    } else {
                        SecondaryTabRow!(
                            state.get(),
                            modifier = Modifier.test_tag("row"),
                            indicator = indicator,
                            tabs = tabs
                        );
                    }
                });
            });
        });
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, fixedTabRow_indicatorPosition
#[test]
fn a_fixed_rows_indicator_moves_under_the_selected_tab() {
    let mut ui = two_tabs(LayoutDirection::Ltr, false);
    let row = bounds(&mut ui, "box");
    let indicator = bounds(&mut ui, "indicator");
    assert_eq!((indicator.left, indicator.top), (0.0, row.height() - 1.0));
    ui.on_all_nodes(is_selectable()).get(1).perform_click();
    let indicator = bounds(&mut ui, "indicator");
    assert_eq!((indicator.left, indicator.top), (row.width() / 2.0, row.height() - 1.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, fixedTabRowRt_indicatorPosition_rtl
#[test]
fn right_to_left_a_fixed_rows_indicator_moves_under_the_selected_tab() {
    let mut ui = two_tabs(LayoutDirection::Rtl, false);
    let row = bounds(&mut ui, "box");
    let indicator = bounds(&mut ui, "indicator");
    assert_eq!((indicator.left, indicator.top), (row.width() / 2.0, row.height() - 1.0));
    ui.on_all_nodes(is_selectable()).get(1).perform_click();
    let indicator = bounds(&mut ui, "indicator");
    assert_eq!((indicator.left, indicator.top), (0.0, row.height() - 1.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, scrollableTabRow_indicatorPosition
#[test]
fn a_scrollable_rows_indicator_moves_under_the_selected_tab() {
    let mut ui = two_tabs(LayoutDirection::Ltr, true);
    let row = bounds(&mut ui, "box");
    let indicator = bounds(&mut ui, "indicator");
    assert_eq!((indicator.left, indicator.top), (52.0, row.height() - 1.0));
    ui.on_all_nodes(is_selectable()).get(1).perform_click();
    let indicator = bounds(&mut ui, "indicator");
    assert_eq!((indicator.left, indicator.top), (52.0 + 90.0, row.height() - 1.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, scrollableTabRow_indicatorPosition_rtl
#[test]
fn right_to_left_a_scrollable_rows_indicator_moves_under_the_selected_tab() {
    let mut ui = two_tabs(LayoutDirection::Rtl, true);
    let row = bounds(&mut ui, "box");
    let indicator = bounds(&mut ui, "indicator");
    assert!((indicator.left - (row.width() - 52.0 - 90.0)).abs() <= 1.0, "{indicator:?}");
    assert!((indicator.top - (row.height() - 1.0)).abs() <= 1.0);
    ui.on_all_nodes(is_selectable()).get(1).perform_click();
    let indicator = bounds(&mut ui, "indicator");
    assert!((indicator.left - (row.width() - 52.0 - 180.0)).abs() <= 1.0, "{indicator:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, tabRow_indicatorHeight
#[test]
fn the_indicator_keeps_its_height() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.fill_max_size(), propagate_min_constraints = true, {
                PrimaryTabRow!(
                    1,
                    indicator = Some(|scope: TabIndicatorScope| red_indicator(&scope, 1, 1.0)),
                    {
                        for (index, title) in ["TAB 1", "TAB 2"].into_iter().enumerate() {
                            Tab!(index == 1, || {}, text = Some(move || Text!("{}", title)));
                        }
                    }
                );
            });
        });
    });
    assert_eq!(bounds(&mut ui, "indicator").height(), 1.0);
}

fn divider_row(scrollable: bool) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            let divider = || {
                HorizontalDivider!(modifier = Modifier.test_tag("divider"));
            };
            let tabs = || {
                for (index, title) in ["TAB 1", "TAB 2"].into_iter().enumerate() {
                    Tab!(
                        index == 0,
                        || {},
                        modifier = Modifier.height(100.dp()),
                        text = Some(move || Text!("{}", title))
                    );
                }
            };
            Box!(modifier = Modifier.test_tag("box"), {
                if scrollable {
                    SecondaryScrollableTabRow!(
                        0,
                        modifier = Modifier.height(100.dp()),
                        divider = divider,
                        tabs = tabs
                    );
                } else {
                    SecondaryTabRow!(
                        0,
                        modifier = Modifier.height(100.dp()),
                        divider = divider,
                        tabs = tabs
                    );
                }
            });
        });
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, fixedTabRow_dividerHeight
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, scrollableTabRow_dividerHeight
#[test]
fn the_divider_lies_along_the_rows_bottom() {
    for scrollable in [false, true] {
        let mut ui = divider_row(scrollable);
        let row = bounds(&mut ui, "box");
        let divider = bounds(&mut ui, "divider");
        assert_eq!(
            (divider.left, divider.top, divider.height()),
            (0.0, row.height() - 1.0, 1.0),
            "{scrollable}"
        );
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, singleLineTab_textPosition
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, twoLineTab_textPosition
#[test]
fn a_tabs_text_is_centered_vertically() {
    for title in ["TAB", "Two line \n text"] {
        let mut ui = compose_test(move || {
            MaterialTheme!({
                Box!({
                    SecondaryTabRow!(0, modifier = Modifier.test_tag("row"), {
                        Tab!(
                            true,
                            || {},
                            text = Some(move || Text!(
                                title,
                                modifier = Modifier.test_tag("text"),
                                max_lines = 2
                            ))
                        );
                    });
                });
            });
        });
        let (row, text) = (bounds(&mut ui, "row"), bounds(&mut ui, "text"));
        assert!(
            (text.top - (row.height() - text.height()) / 2.0).abs() <= 0.5,
            "{title}: {text:?} in {row:?}"
        );
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, leadingIconTab_textAndIconPosition
#[test]
fn a_leading_icon_tab_centers_its_icon_8_dp_before_its_text() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!({
                SecondaryTabRow!(0, modifier = Modifier.test_tag("row"), {
                    LeadingIconTab!(
                        true,
                        || {},
                        { Text!("TAB", modifier = Modifier.test_tag("text")) },
                        {
                            Icon!(
                                icons::filled::favorite(),
                                None,
                                modifier = Modifier.test_tag("icon")
                            )
                        }
                    );
                });
            });
        });
    });
    let (row, text, icon) =
        (bounds(&mut ui, "row"), bounds(&mut ui, "text"), bounds(&mut ui, "icon"));
    assert_eq!(text.left, icon.right + 8.0);
    assert!((icon.left - (row.width() - icon.width() - text.width() - 8.0) / 2.0).abs() <= 0.5);
}

/// Three text tabs in a row (`scrollable`: ten in a scrollable row), the first selected.
fn text_tabs(count: usize, scrollable: bool, initial: usize) -> ComposeTestRule {
    compose_test(move || {
        let state = remember(|| mutable_state_of(initial));
        MaterialTheme!({
            let tabs = move || {
                for index in 0..count {
                    Tab!(
                        state.get() == index,
                        move || state.set(index),
                        text = Some(move || Text!("Tab {}", index + 1))
                    );
                }
            };
            if scrollable {
                SecondaryScrollableTabRow!(state.get(), tabs = tabs);
            } else {
                SecondaryTabRow!(state.get(), tabs = tabs);
            }
        });
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, fixedTabRow_initialTabSelected
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, fixedTabRow_selectNewTab
#[test]
fn a_fixed_row_selects_its_first_tab_then_the_clicked_one() {
    let mut ui = text_tabs(3, false, 0);
    assert_eq!(selections(&mut ui), vec![true, false, false]);
    ui.on_all_nodes(is_selectable()).get(2).perform_click();
    assert_eq!(selections(&mut ui), vec![false, false, true]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, fixedLeadingIconTabRow_initialTabSelected
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, leadingIconTabRow_selectNewTab
#[test]
fn a_row_of_leading_icon_tabs_selects_the_clicked_one() {
    let mut ui = compose_test(|| {
        let state = remember(|| mutable_state_of(0usize));
        MaterialTheme!({
            SecondaryTabRow!(state.get(), {
                for index in 0..3usize {
                    LeadingIconTab!(
                        state.get() == index,
                        move || state.set(index),
                        { Text!("Tab {}", index + 1) },
                        { Icon!(icons::filled::favorite(), None) }
                    );
                }
            });
        });
    });
    assert_eq!(selections(&mut ui), vec![true, false, false]);
    ui.on_all_nodes(is_selectable()).get(2).perform_click();
    assert_eq!(selections(&mut ui), vec![false, false, true]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, scrollableTabRow_initialTabSelected
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, scrollableTabRow_selectNewTab
#[test]
fn a_scrollable_row_selects_its_first_tab_then_the_clicked_one() {
    let mut ui = text_tabs(10, true, 0);
    let mut expected = vec![false; 10];
    expected[0] = true;
    assert_eq!(selections(&mut ui), expected);
    ui.on_all_nodes(is_selectable()).get(1).perform_click();
    expected.swap(0, 1);
    assert_eq!(selections(&mut ui), expected);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, scrollableTabRow_offScreenTabInitiallySelected
#[test]
fn a_scrollable_row_scrolls_to_an_initially_selected_tab_off_screen() {
    let mut ui = text_tabs(10, true, 9);
    let last = ui.on_all_nodes(is_selectable()).get(9).fetch_semantics_node();
    assert_eq!(last.config.get(&compose_ui::SemanticsProperties::SELECTED).copied(), Some(true));
    // Displayed: inside the 400 dp window.
    assert!(last.bounds.left >= 0.0 && last.bounds.right <= 400.0, "{:?}", last.bounds);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, tabRowIndicator_animatesWidthChange
#[test]
fn the_indicator_widens_to_a_wider_tab_animated() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        let state = remember(|| mutable_state_of(0usize));
        MaterialTheme!({
            Box!({
                SecondaryScrollableTabRow!(
                    state.get(),
                    indicator = Some(move |scope: TabIndicatorScope| {
                        TabRowDefaults::secondary_indicator(
                            Modifier
                                .tab_indicator_offset(&scope, state.get(), false)
                                .test_tag("indicator"),
                            TabRowDefaults::INDICATOR_HEIGHT,
                            TabRowDefaults::indicator_color(),
                        );
                    }),
                    {
                        for (index, title) in
                            ["TAB 1", "TAB 2", "TAB 3 WITH LOTS OF TEXT"].into_iter().enumerate()
                        {
                            Tab!(
                                state.get() == index,
                                move || state.set(index),
                                text = Some(move || Text!("{}", title))
                            );
                        }
                    }
                );
            });
        });
    });
    ui.advance_time_by(Duration::from_millis(100));
    let initial = bounds(&mut ui, "indicator").width();
    ui.on_all_nodes(is_selectable()).get(2).perform_click();
    ui.advance_time_by(Duration::from_millis(50));
    let middle = bounds(&mut ui, "indicator").width();
    assert!(initial < middle, "{initial} {middle}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    let last = bounds(&mut ui, "indicator").width();
    assert!(middle < last, "{middle} {last}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, fontScaleChange_height
#[test]
fn a_tab_grows_with_a_large_font_scale() {
    let mut ui = compose_test_with(
        IntSize::new(400, 800),
        Density { density: 1.0, font_scale: 10.0 },
        Box::new(FixedTextMeasurer::new()),
        || {
            MaterialTheme!({
                Box!(modifier = Modifier.test_tag("box"), {
                    Tab!(
                        true,
                        || {},
                        text = Some(|| Text!("Text", font_size = compose_ui::Sp(10.0))),
                        icon = Some(|| Icon!(icons::filled::favorite(), None))
                    );
                });
            });
        },
    );
    assert!(bounds(&mut ui, "box").height() >= 90.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, tabRow_layoutHeightRespected
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, scrollableTabRow_layoutHeightRespected
#[test]
fn a_row_keeps_within_a_maximum_height() {
    for scrollable in [false, true] {
        let height = mutable_state_of(0.0f32);
        let handle = height.handle();
        let mut ui = compose_test(move || {
            let state = remember(|| mutable_state_of(0usize));
            MaterialTheme!({
                Column!(
                    modifier = Modifier
                        .size_in(compose_ui::SizeSpec::bounds(
                            compose_ui::Dp::UNSPECIFIED,
                            compose_ui::Dp::UNSPECIFIED,
                            compose_ui::Dp::UNSPECIFIED,
                            handle.get().dp()
                        ))
                        .test_tag("tabs"),
                    {
                        let tabs = move || {
                            for index in 0..3usize {
                                Tab!(
                                    state.get() == index,
                                    move || state.set(index),
                                    text = Some(move || Text!("Tab {}", index + 1))
                                );
                            }
                        };
                        if scrollable {
                            SecondaryScrollableTabRow!(state.get(), tabs = tabs);
                        } else {
                            SecondaryTabRow!(state.get(), tabs = tabs);
                        }
                    }
                );
            });
        });
        assert_eq!(bounds(&mut ui, "tabs").height(), 0.0, "{scrollable}");
        height.set(40.0);
        ui.wait_for_idle();
        assert_eq!(bounds(&mut ui, "tabs").height(), 40.0, "{scrollable}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TabTest.kt, tabRow_noTabsHasHeightZero
#[test]
fn a_row_without_tabs_has_no_height() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            SecondaryTabRow!(0, modifier = Modifier.test_tag("row"), {});
        });
    });
    assert_eq!(bounds(&mut ui, "row").height(), 0.0);
}
