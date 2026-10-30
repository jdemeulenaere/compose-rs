//! Compose's `InteractiveListTest.kt`, adapted: the tip's `ListItem` overloads (here
//! `ListItem`, `ClickableListItem`, `SelectableListItem`, `ToggleableListItem`): sizes,
//! the parts' positions under each vertical alignment (left to right and right to left),
//! semantics and long clicks, the default content padding. `SegmentedListItem` and the
//! precision-pointer padding aren't in compose-rs yet.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, PointerEvent, ToggleableState,
    UnitExt,
};

const ROOT_WIDTH: f32 = 400.0;

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
}

fn item_size(content: impl Fn() + 'static) -> (f32, f32) {
    let content = Rc::new(content);
    let mut ui = compose_test(move || {
        let content = content.clone();
        MaterialTheme!({
            let content = content.clone();
            Box!(modifier = Modifier.test_tag("box"), { content() });
        });
    });
    let bounds = bounds(&mut ui, "box");
    (bounds.width(), bounds.height())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, listItem_oneLine_size
#[test]
fn a_one_line_item_is_56_dp_high() {
    let size = item_size(|| {
        ClickableListItem!(|| {}, {
            Text!("Content");
        });
    });
    assert_eq!(size, (ROOT_WIDTH, 56.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, listItem_twoLine_size
#[test]
fn a_two_line_item_is_72_dp_high() {
    let size = item_size(|| {
        ClickableListItem!(|| {}, supporting_content = Some(|| Text!("Supporting")), {
            Text!("Content");
        });
    });
    assert_eq!(size, (ROOT_WIDTH, 72.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, listItem_threeLineOverlineAndSupporting_size
#[test]
fn an_item_with_an_overline_and_supporting_text_is_88_dp_high() {
    let size = item_size(|| {
        ClickableListItem!(
            || {},
            overline_content = Some(|| Text!("Overline")),
            supporting_content = Some(|| Text!("Supporting")),
            {
                Text!("Content");
            }
        );
    });
    assert_eq!(size, (ROOT_WIDTH, 88.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, listItem_threeLineSupportingMultiline_size
#[test]
fn an_item_with_two_lines_of_supporting_text_is_88_dp_high() {
    let size = item_size(|| {
        ClickableListItem!(|| {}, supporting_content = Some(|| Text!("Supporting\nLine 2")), {
            Text!("Content");
        });
    });
    assert_eq!(size, (ROOT_WIDTH, 88.0));
}

/// An item `height` dp high with every part tagged, in `direction`, aligned by `alignment`.
fn positioned(
    height: f32,
    direction: LayoutDirection,
    alignment: Option<compose_ui::VerticalAlignment>,
) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
                ClickableListItem!(
                    || {},
                    modifier = Modifier.height(height.dp()),
                    leading_content =
                        Some(|| Box!(modifier = Modifier.test_tag("leading").size(48.dp()))),
                    trailing_content =
                        Some(|| Box!(modifier = Modifier.test_tag("trailing").size(48.dp()))),
                    overline_content =
                        Some(|| Text!("Overline", modifier = Modifier.test_tag("overline"))),
                    supporting_content =
                        Some(|| Text!("Supporting", modifier = Modifier.test_tag("supporting"))),
                    vertical_alignment = alignment,
                    {
                        Text!("Content", modifier = Modifier.test_tag("content"));
                    }
                );
            });
        });
    })
}

/// The breakpoint between centered and top alignment (60 dp) plus the 20 dp of vertical padding.
const BREAKPOINT_HEIGHT: f32 = 60.0 + 20.0;

fn assert_centered(ui: &mut ComposeTestRule, height: f32, rtl: bool) {
    let (leading, trailing) = (bounds(ui, "leading"), bounds(ui, "trailing"));
    let (overline, content, supporting) =
        (bounds(ui, "overline"), bounds(ui, "content"), bounds(ui, "supporting"));
    if rtl {
        assert_eq!(leading.right, ROOT_WIDTH - 16.0);
        assert_eq!(trailing.left, 16.0);
        let main_right = leading.left - 12.0;
        for part in [overline, content, supporting] {
            assert_eq!(part.right, main_right);
        }
    } else {
        assert_eq!(leading.left, 16.0);
        assert_eq!(trailing.right, ROOT_WIDTH - 16.0);
        let main_left = leading.right + 12.0;
        for part in [overline, content, supporting] {
            assert_eq!(part.left, main_left);
        }
    }
    let round = |value: f32| value.round();
    assert_eq!(leading.top, round((height - leading.height()) / 2.0));
    assert_eq!(trailing.top, round((height - trailing.height()) / 2.0));
    let stack = overline.height() + content.height() + supporting.height();
    assert!((overline.top - (height - stack) / 2.0).abs() <= 1.0, "{overline:?} {stack}");
}

fn assert_at_top(ui: &mut ComposeTestRule, rtl: bool) {
    let (leading, trailing) = (bounds(ui, "leading"), bounds(ui, "trailing"));
    let overline = bounds(ui, "overline");
    assert_eq!((leading.top, trailing.top, overline.top), (10.0, 10.0, 10.0));
    if rtl {
        assert_eq!((leading.right, trailing.left), (ROOT_WIDTH - 16.0, 16.0));
        assert_eq!(overline.right, leading.left - 12.0);
    } else {
        assert_eq!((leading.left, trailing.right), (16.0, ROOT_WIDTH - 16.0));
        assert_eq!(overline.left, leading.right + 12.0);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, clickableListItem_verticalAlignmentCenter_positioning
#[test]
fn a_short_item_centers_its_parts() {
    let height = BREAKPOINT_HEIGHT - 10.0;
    let mut ui = positioned(height, LayoutDirection::Ltr, None);
    assert_centered(&mut ui, height, false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, clickableListItem_verticalAlignmentTop_positioning
#[test]
fn a_tall_item_puts_its_parts_at_the_top() {
    let mut ui = positioned(BREAKPOINT_HEIGHT + 10.0, LayoutDirection::Ltr, None);
    assert_at_top(&mut ui, false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, clickableListItem_verticalAlignmentCenter_positioning_rtl
#[test]
fn right_to_left_a_short_item_centers_its_parts() {
    let height = BREAKPOINT_HEIGHT - 10.0;
    let mut ui = positioned(height, LayoutDirection::Rtl, None);
    assert_centered(&mut ui, height, true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, clickableListItem_verticalAlignmentTop_positioning_rtl
#[test]
fn right_to_left_a_tall_item_puts_its_parts_at_the_top() {
    let mut ui = positioned(BREAKPOINT_HEIGHT + 10.0, LayoutDirection::Rtl, None);
    assert_at_top(&mut ui, true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, clickableListItem_customVerticalAlignment_positioning
#[test]
fn an_item_aligned_at_the_bottom_stacks_its_parts_from_the_bottom() {
    let mut ui = positioned(300.0, LayoutDirection::Ltr, Some(Alignment::BOTTOM));
    let bottom = 300.0 - 10.0;
    let (leading, trailing) = (bounds(&mut ui, "leading"), bounds(&mut ui, "trailing"));
    let (overline, content, supporting) =
        (bounds(&mut ui, "overline"), bounds(&mut ui, "content"), bounds(&mut ui, "supporting"));
    assert_eq!((leading.bottom, trailing.bottom, supporting.bottom), (bottom, bottom, bottom));
    assert_eq!(content.bottom, bottom - supporting.height());
    assert_eq!(overline.bottom, bottom - supporting.height() - content.height());
}

fn long_click(ui: &mut ComposeTestRule, tag: &str) {
    let node = bounds(ui, tag);
    let center =
        compose_ui::Offset::new((node.left + node.right) / 2.0, (node.top + node.bottom) / 2.0);
    ui.set_auto_advance(false);
    ui.host().send_pointer(PointerEvent::press(center));
    ui.advance_time_by(Duration::from_millis(1000));
    ui.host().send_pointer(PointerEvent::release(center));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, clickableListItem_semantics
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, clickableListItem_longClick
#[test]
fn a_clickable_item_clicks_and_long_clicks() {
    thread_local! {
        static CLICKS: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
    }
    let count = |long: bool| {
        CLICKS.with(|cell| {
            let (clicks, long_clicks) = cell.get();
            cell.set(if long { (clicks, long_clicks + 1) } else { (clicks + 1, long_clicks) });
        })
    };
    let mut ui = compose_test(move || {
        MaterialTheme!({
            ClickableListItem!(
                move || count(false),
                modifier = Modifier.test_tag("list"),
                on_long_click = Some(move || count(true)),
                {
                    Text!("Content");
                }
            );
        });
    });
    let node = ui.on_node_unmerged(has_test_tag("list")).fetch_semantics_snapshot();
    assert!(
        node.config.contains(&compose_ui::SemanticsActions::ON_CLICK)
            && node.config.contains(&compose_ui::SemanticsActions::ON_LONG_CLICK)
    );
    ui.on_node_unmerged(has_test_tag("list")).perform_click();
    assert_eq!(CLICKS.with(Cell::get), (1, 0));
    long_click(&mut ui, "list");
    assert_eq!(CLICKS.with(Cell::get), (1, 1));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, selectableListItem_semantics
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, selectableListItem_longClick
#[test]
fn a_selectable_item_says_it_is_selected_and_long_clicks() {
    let selected = mutable_state_of(false);
    let handle = selected.handle();
    thread_local! { static LONG_CLICKED: Cell<bool> = const { Cell::new(false) }; }
    let mut ui = compose_test(move || {
        MaterialTheme!({
            SelectableListItem!(
                handle.get(),
                move || handle.set(!handle.get()),
                modifier = Modifier.test_tag("list"),
                on_long_click = Some(|| LONG_CLICKED.with(|cell| cell.set(true))),
                {
                    Text!("Content");
                }
            );
        });
    });
    let selection = |ui: &mut ComposeTestRule| {
        ui.on_node_unmerged(has_test_tag("list"))
            .fetch_semantics_snapshot()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied()
    };
    assert_eq!(selection(&mut ui), Some(false));
    long_click(&mut ui, "list");
    assert!(LONG_CLICKED.with(Cell::get));
    assert_eq!(selection(&mut ui), Some(false));
    ui.on_node_unmerged(has_test_tag("list")).perform_click();
    assert_eq!(selection(&mut ui), Some(true));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, toggleableListItem_semantics
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, toggleableListItem_longClick
#[test]
fn a_toggleable_item_turns_on_and_long_clicks() {
    let checked = mutable_state_of(false);
    let handle = checked.handle();
    thread_local! { static LONG_CLICKED: Cell<bool> = const { Cell::new(false) }; }
    let mut ui = compose_test(move || {
        MaterialTheme!({
            ToggleableListItem!(
                handle.get(),
                move |value| handle.set(value),
                modifier = Modifier.test_tag("list"),
                on_long_click = Some(|| LONG_CLICKED.with(|cell| cell.set(true))),
                {
                    Text!("Content");
                }
            );
        });
    });
    let state = |ui: &mut ComposeTestRule| {
        ui.on_node_unmerged(has_test_tag("list"))
            .fetch_semantics_snapshot()
            .config
            .get(&compose_ui::SemanticsProperties::TOGGLEABLE_STATE)
            .copied()
    };
    assert_eq!(state(&mut ui), Some(ToggleableState::Off));
    long_click(&mut ui, "list");
    assert!(LONG_CLICKED.with(Cell::get));
    assert_eq!(state(&mut ui), Some(ToggleableState::Off));
    ui.on_node_unmerged(has_test_tag("list")).perform_click();
    assert_eq!(state(&mut ui), Some(ToggleableState::On));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveListTest.kt, listItem_contentPadding_default
#[test]
fn the_default_content_padding_is_16_dp_at_the_sides_and_10_dp_above_and_below() {
    let padding = ListItemDefaults::CONTENT_PADDING;
    assert_eq!(
        (padding.start, padding.top, padding.end, padding.bottom),
        (16.dp(), 10.dp(), 16.dp(), 10.dp())
    );
}

/// A selected item takes the selected container and shape; a disabled one says so.
#[test]
fn a_selected_item_takes_the_selected_colors_and_a_disabled_one_says_so() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                SelectableListItem!(true, || {}, modifier = Modifier.test_tag("selected"), {
                    Text!("Selected");
                });
                ListItem!(modifier = Modifier.test_tag("disabled"), enabled = false, {
                    Text!("Disabled");
                });
            });
        });
    });
    let scheme = light_color_scheme();
    let fills: Vec<_> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            compose_ui::platform::DrawOp::DrawRoundRect(_, radii, paint) => {
                Some((radii[0], paint.color))
            }
            _ => None,
        })
        .collect();
    // The selected item on the large (16 dp) shape in the secondary container; the disabled one
    // on the extra small (4 dp) shape in the surface.
    assert!(fills.contains(&(16.0, scheme.secondary_container)), "{fills:?}");
    assert!(fills.contains(&(4.0, scheme.surface)), "{fills:?}");
    assert!(
        ui.on_node_with_tag("disabled")
            .fetch_semantics_snapshot()
            .config
            .contains(&compose_ui::SemanticsProperties::DISABLED)
    );
}
