//! Segmented buttons, the navigation rail and scrollable tab rows.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, Role};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

#[composable]
fn Segments(fill: bool) {
    let selected = remember(|| mutable_state_of(0usize));
    MaterialTheme!({
        let labels = ["Day", "Week", "Month and year"];
        SingleChoiceSegmentedButtonRow!(
            modifier = if fill { Modifier.fill_max_width() } else { Modifier }.test_tag("row"),
            |scope: SingleChoiceSegmentedButtonRowScope| {
                for (index, label) in labels.into_iter().enumerate() {
                    SegmentedButton!(
                        scope,
                        selected.get() == index,
                        move || selected.set(index),
                        SegmentedButtonDefaults::item_shape(index, labels.len()),
                        modifier = Modifier.test_tag(label),
                        label = { Text!("{}", label) }
                    );
                }
            }
        );
    });
}

#[test]
fn segmented_buttons_share_the_row_and_select_one() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 400), || Segments(false));
    let (day, week, month) =
        (bounds(&mut ui, "Day"), bounds(&mut ui, "Week"), bounds(&mut ui, "Month and year"));
    // As wide as the widest, 40 dp high, the outlines overlapping by 1 dp.
    assert_eq!(day.width(), month.width());
    assert_eq!(week.width(), month.width());
    assert_eq!(day.height(), 40.0);
    assert_eq!(week.left, day.right - 1.0);
    assert_eq!(bounds(&mut ui, "row").width(), 3.0 * day.width() - 2.0);
    assert!(
        ui.on_node_with_tag("row")
            .fetch_semantics_node()
            .config
            .contains(&compose_ui::SemanticsProperties::SELECTABLE_GROUP)
    );
    let node = ui.on_node_with_tag("Day").fetch_semantics_node();
    assert_eq!(
        (
            node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
            node.config.get(&compose_ui::SemanticsProperties::SELECTED).copied()
        ),
        (Some(Role::RadioButton), Some(true))
    );
    // The selected one shows a check before its label.
    assert_eq!(
        ui.on_node_with_tag("Week")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied(),
        Some(false)
    );
    ui.on_node_with_tag("Week").perform_click();
    assert_eq!(
        ui.on_node_with_tag("Week")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied(),
        Some(true)
    );
    assert_eq!(
        ui.on_node_with_tag("Day")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied(),
        Some(false)
    );
    // Given the width, they share it.
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 400), || Segments(true));
    assert_eq!(bounds(&mut ui, "row").width(), 600.0);
    // 602 dp in three, the extra pixels to the first ones.
    assert_eq!(bounds(&mut ui, "Day").width(), 201.0);
    assert_eq!(bounds(&mut ui, "Month and year").width(), 200.0);
}

#[test]
fn a_navigation_rail_and_its_items_are_80_dp_wide() {
    #[composable]
    fn Rail() {
        let selected = remember(|| mutable_state_of(0usize));
        MaterialTheme!({
            NavigationRail!(
                modifier = Modifier.test_tag("rail"),
                header = Some(|_: ColumnScope| {
                    FloatingActionButton!(|| {}, modifier = Modifier.test_tag("fab"), {
                        Icon!(icons::filled::add(), Some("Add"));
                    });
                }),
                |_| {
                    for (index, name) in ["Home", "Settings"].into_iter().enumerate() {
                        NavigationRailItem!(
                            selected.get() == index,
                            move || selected.set(index),
                            { Icon!(icons::filled::home(), Some("Icon")) },
                            modifier = Modifier.test_tag(name),
                            label = { Text!("{}", name) }
                        );
                    }
                    NavigationRailItem!(
                        false,
                        || {},
                        { Icon!(icons::filled::star(), Some("Star")) },
                        modifier = Modifier.test_tag("bare")
                    );
                }
            );
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), Rail);
    let rail = bounds(&mut ui, "rail");
    assert_eq!((rail.width(), rail.height()), (80.0, 600.0));
    // The header 4 dp down, then 8 dp (4 dp from each, as the items are apart).
    let fab = bounds(&mut ui, "fab");
    assert_eq!((fab.top, fab.left), (4.0, 12.0));
    let home = bounds(&mut ui, "Home");
    assert_eq!(home.top, fab.bottom + 4.0 + 8.0 + 4.0);
    // Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/NavigationRailTest.kt, navigationRailItem_sizeAndPositions
    assert_eq!((home.width(), home.left), (80.0, 0.0));
    // With a label: 4 + 32 + 4 + the label + 4 at least.
    assert!(home.height() >= 56.0, "{home:?}");
    let bare = bounds(&mut ui, "bare");
    assert_eq!((bare.width(), bare.height()), (80.0, 56.0));
    let node = ui.on_node_with_tag("Home").fetch_semantics_node();
    assert_eq!(
        (
            node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
            node.config.get(&compose_ui::SemanticsProperties::SELECTED).copied()
        ),
        (Some(Role::Tab), Some(true))
    );
    ui.on_node_with_tag("Settings").perform_click();
    assert_eq!(
        ui.on_node_with_tag("Settings")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied(),
        Some(true)
    );
}

#[test]
fn a_scrollable_tab_row_sizes_its_tabs_and_scrolls_to_the_selected_one() {
    #[composable]
    fn Tabs(selected: MutableState<usize>) {
        MaterialTheme!({
            SecondaryScrollableTabRow!(selected.get(), modifier = Modifier.test_tag("row"), {
                for index in 0..8usize {
                    Tab!(
                        selected.get() == index,
                        move || selected.set(index),
                        modifier = Modifier.test_tag(format!("tab {index}")),
                        text = { Text!("Tab {}", index) }
                    );
                }
            });
        });
    }
    let owner = mutable_state_of(0usize);
    let selected = owner.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || Tabs(selected));
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(100));
    let row = bounds(&mut ui, "row");
    assert_eq!((row.width(), row.height()), (400.0, 48.0));
    let first = bounds(&mut ui, "tab 0");
    // 52 dp in, at least 90 dp wide.
    assert_eq!(first.left, 52.0);
    assert!(first.width() >= 90.0, "{first:?}");
    // Selecting the sixth scrolls it to the middle.
    selected.set(5);
    ui.advance_time_by(Duration::from_millis(500));
    let sixth = bounds(&mut ui, "tab 5");
    let middle = (sixth.left + sixth.right) / 2.0;
    assert!((middle - 200.0).abs() <= 1.0, "{sixth:?}");
    drop(owner);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/NavigationRailTest.kt, defaultSemantics
#[test]
fn a_rails_items_are_a_traversal_group() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            NavigationRail!({
                NavigationRailItem!(true, || {}, {}, modifier = Modifier.test_tag("item"));
            });
        });
    });
    ui.on_node_with_tag("item")
        .on_parent()
        .assert(expect_value(&compose_ui::SemanticsProperties::IS_TRAVERSAL_GROUP, true));
}

/// A label shown only when selected: the icon moves between the center and its place above the
/// label, the indicator with it, a progress at a time.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/NavigationBarTest.kt, navigationBarItemContent_withLabel_unselected_sizeAndPosition
#[test]
fn an_item_showing_its_label_when_selected_centers_its_icon_when_not() {
    use compose_ui::UnitExt;
    let selected = mutable_state_of(false);
    let read = selected.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            NavigationBar!(|row: RowScope| {
                NavigationBarItem!(
                    row,
                    read.get(),
                    || {},
                    {
                        Box!(modifier = Modifier.size(24.dp()).test_tag("icon"));
                    },
                    modifier = Modifier.test_tag("item"),
                    label = Some(|| Text!("Label")),
                    always_show_label = false
                );
            });
        });
    });
    let rect = |ui: &mut ComposeTestRule, tag: &str| {
        ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
    };
    let (item, icon) = (rect(&mut ui, "item"), rect(&mut ui, "icon"));
    assert_eq!(icon.top - item.top, (item.height() - 24.0) / 2.0);
    // Not placed, as Compose's `assertIsNotDisplayed`, and taking no taps; still naming the tab.
    ui.on_node_unmerged(has_text("Label")).assert_is_not_displayed();
    ui.on_node_with_tag("item").assert(has_text("Label"));
    selected.set(true);
    ui.wait_for_idle();
    // Selected, the icon's semantics are cleared: its place shows from the label's below it, 24 dp
    // of icon and 4 + 4 dp of indicator and gap, centered with them in the bar.
    let item = rect(&mut ui, "item");
    let label = ui.on_node_unmerged(has_text("Label")).fetch_semantics_node().bounds;
    let content = 24.0 + 8.0 + label.height();
    assert_eq!(label.top - item.top, (item.height() - content) / 2.0 + 32.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/NavigationBarTest.kt, navigationBarItem_withLongLabel_automaticallyResizesHeight
/// A tall label makes the bar taller: `defaultMinSize`, not a fixed height.
#[test]
fn a_tall_label_makes_the_bar_taller() {
    use compose_ui::UnitExt;
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!({
                NavigationBar!(modifier = Modifier.test_tag("bar"), |row: RowScope| {
                    NavigationBarItem!(
                        row,
                        true,
                        || {},
                        {
                            Box!(modifier = Modifier.size(24.dp()));
                        },
                        label = Some(|| {
                            Box!(modifier = Modifier.size(100.dp()));
                        })
                    );
                });
            });
        });
    });
    // 4 + 24 + 4 + 4 + 100 + 4.
    assert_eq!(bounds(&mut ui, "bar").height(), 140.0);
}

/// Its row is a selectable group, its height 80 dp at least.
#[test]
fn a_bars_row_is_a_selectable_group() {
    use compose_ui::UnitExt;
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!({
                NavigationBar!(modifier = Modifier.test_tag("bar"), |row: RowScope| {
                    NavigationBarItem!(row, true, || {}, {
                        Box!(modifier = Modifier.size(24.dp()));
                    });
                });
            });
        });
    });
    assert_eq!(bounds(&mut ui, "bar").height(), 80.0);
    let root = ui.host().semantics_tree(false);
    fn has_group(node: &compose_ui::platform::SemanticsNode) -> bool {
        node.config.contains(&compose_ui::SemanticsProperties::SELECTABLE_GROUP)
            || node.children.iter().any(has_group)
    }
    assert!(has_group(&root));
}

/// As `NavigationBarDefaults`: no elevation, the label's selected color `Secondary`, a 56 dp pill.
#[test]
fn a_bars_tokens_are_the_tips() {
    assert_eq!(NavigationBarDefaults::ELEVATION, compose_ui::Dp(0.0));
    thread_local! {
        static READ: std::cell::Cell<Option<NavigationBarItemColors>> = const { std::cell::Cell::new(None) };
    }
    let _ui = compose_test(|| {
        MaterialTheme!({
            READ.with(|it| it.set(Some(NavigationBarItemDefaults::colors())));
        });
    });
    let colors = READ.with(std::cell::Cell::get).expect("composed");
    assert_eq!(colors.selected_text_color, light_color_scheme().secondary);
}

/// At a fractional density, as Compose's float arithmetic rounds where it places: at 2.625, a bar
/// with a 63 px icon and a 42 px label (24 dp and 16 dp).
/// At 2.625 the paddings aren't whole pixels: Compose's float arithmetic, rounded where it places.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationBar.kt, placeLabelAndIcon
#[test]
fn a_bar_item_places_at_a_fractional_density() {
    use compose_ui::platform::FixedTextMeasurer;
    use compose_ui::{Density, IntSize, UnitExt};
    let mut ui = compose_test_with(
        IntSize::new(1080, 2400),
        Density::new(2.625),
        Box::new(FixedTextMeasurer::new()),
        || {
            MaterialTheme!({
                Box!({
                    NavigationBar!(modifier = Modifier.test_tag("bar"), |row: RowScope| {
                        NavigationBarItem!(
                            row,
                            true,
                            || {},
                            {
                                Box!(modifier = Modifier.size(24.dp()).background(Color::RED));
                            },
                            label = Some(|| {
                                Box!(
                                    modifier = Modifier.size((10.dp(), 16.dp())).test_tag("label")
                                );
                            })
                        );
                    });
                });
            });
        },
    );
    let top = |ui: &mut ComposeTestRule, tag: &str| {
        ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds.top
    };
    // Content 63 + 10.5 + 10.5 + 42 = 126; from 210 px: 42 of padding each side.
    assert_eq!(ui.on_node_with_tag("bar").fetch_semantics_node().bounds.height(), 210.0);
    assert_eq!(top(&mut ui, "label"), 42.0 + 63.0 + 21.0);
    // The indicator's padding rounded to 11, so at 31, not 30; 63 + 21 tall, not 85.
    let list = ui.dump_display_list();
    let pill = list.lines().map(str::trim).collect::<Vec<_>>();
    let at = pill.iter().position(|line| line.starts_with("round rect")).expect(&list);
    assert_eq!(pill[at - 1], "translate 466 31", "{list}");
    assert!(pill[at].starts_with("round rect 0,0 147x84"), "{list}");
    // The icon at 42 (its padding), not 41.
    assert_eq!(pill[at + 1], "translate 42 11", "{list}");
    // The ripple, the next, at 32: 31.5 rounded, not 30.
    assert_eq!(pill[at + 3], "translate -42 -10", "{list}");
}
