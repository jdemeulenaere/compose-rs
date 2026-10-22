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
    assert!(ui.on_node_with_tag("row").fetch_semantics_node().config.selectable_group);
    let node = ui.on_node_with_tag("Day").fetch_semantics_node();
    assert_eq!((node.config.role, node.config.selected), (Some(Role::RadioButton), Some(true)));
    // The selected one shows a check before its label.
    assert_eq!(ui.on_node_with_tag("Week").fetch_semantics_node().config.selected, Some(false));
    ui.on_node_with_tag("Week").perform_click();
    assert_eq!(ui.on_node_with_tag("Week").fetch_semantics_node().config.selected, Some(true));
    assert_eq!(ui.on_node_with_tag("Day").fetch_semantics_node().config.selected, Some(false));
    // Given the width, they share it.
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 400), || Segments(true));
    assert_eq!(bounds(&mut ui, "row").width(), 600.0);
    // 602 dp in three, the extra pixels to the first ones.
    assert_eq!(bounds(&mut ui, "Day").width(), 201.0);
    assert_eq!(bounds(&mut ui, "Month and year").width(), 200.0);
}

#[test]
fn a_navigation_rail_is_80_dp_its_items_56_dp() {
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
    assert_eq!((home.width(), home.left), (56.0, 12.0));
    // With a label: 4 + 32 + 4 + the label + 4 at least.
    assert!(home.height() >= 56.0, "{home:?}");
    let bare = bounds(&mut ui, "bare");
    assert_eq!((bare.width(), bare.height()), (56.0, 56.0));
    let node = ui.on_node_with_tag("Home").fetch_semantics_node();
    assert_eq!((node.config.role, node.config.selected), (Some(Role::Tab), Some(true)));
    ui.on_node_with_tag("Settings").perform_click();
    assert_eq!(ui.on_node_with_tag("Settings").fetch_semantics_node().config.selected, Some(true));
}

#[test]
fn a_scrollable_tab_row_sizes_its_tabs_and_scrolls_to_the_selected_one() {
    #[composable]
    fn Tabs(selected: MutableState<usize>) {
        MaterialTheme!({
            ScrollableTabRow!(selected.get(), modifier = Modifier.test_tag("row"), {
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
    assert!(first.width() >= 90.0);
    // Selecting the sixth scrolls it to the middle.
    selected.set(5);
    ui.advance_time_by(Duration::from_millis(500));
    let sixth = bounds(&mut ui, "tab 5");
    let middle = (sixth.left + sixth.right) / 2.0;
    assert!((middle - 200.0).abs() <= 1.0, "{sixth:?}");
    drop(owner);
}
