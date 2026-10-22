//! Sixth review round: `TabRow` and `Tab`, against Jetpack Compose's material3. Each test
//! states the input, what Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, platform::DrawOp};

fn indicator(ui: &mut ComposeTestRule) -> Option<(f32, f32)> {
    ui.host().display_list().ops.iter().rev().find_map(|op| match op {
        DrawOp::DrawRect(rect, _) if rect.height() == 3.0 => Some((rect.left, rect.width())),
        _ => None,
    })
}

#[composable]
fn Tabs(selected: MutableState<usize>, count: usize) {
    MaterialTheme!({
        TabRow!(selected.get(), modifier = Modifier.test_tag("row"), {
            for index in 0..count {
                Tab!(
                    selected.get() == index,
                    move || selected.set(index),
                    modifier = Modifier.test_tag(format!("tab{index}")),
                    text = Some(move || Text!("Tab {}", index)),
                );
            }
        });
    });
}

/// A tab row with one tab holding an icon and a text (72 dp) and one with a text alone (48
/// dp): Compose's `TabRow` measures every tab at the row's height, the tallest tab's
/// (`minHeight = maxHeight = tabRowHeight`), so the text-only tab is 72 dp tall too, its text
/// centered and its whole cell clickable. Here tabs are measured with a minimum height of 0:
/// the text-only tab stays 48 dp, at the top of a 72 dp row.
#[test]
fn every_tab_is_as_tall_as_the_row() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), || {
        MaterialTheme!({
            TabRow!(0, modifier = Modifier.test_tag("row"), {
                Tab!(
                    true,
                    || {},
                    modifier = Modifier.test_tag("both"),
                    text = Some(|| Text!("Both")),
                    icon = Some(|| Icon!(icons::filled::add(), None)),
                );
                Tab!(
                    false,
                    || {},
                    modifier = Modifier.test_tag("text"),
                    text = Some(|| Text!("Text")),
                );
            });
        });
    });
    let row = ui.on_node_with_tag("row").fetch_semantics_node().bounds;
    assert_eq!(row.height(), 72.0);
    let text = ui.on_node_with_tag("text").fetch_semantics_node().bounds;
    assert_eq!(text.height(), 72.0, "as tall as the row");
}

/// Selecting tab 2 of 3 (the indicator under tab 0): Compose's indicator offset is an
/// `animateDpAsState` of the selected tab's position, so the frame of the change still draws it
/// under tab 0, and it slides from there. Here the draw falls back to the selected tab's
/// position while the animation isn't running yet (its `LaunchedEffect` waits a frame first):
/// the frame of the change draws the indicator under tab 2, then it jumps back to tab 0 and
/// slides — a one-frame flash at the target.
#[test]
fn the_indicator_does_not_flash_at_the_new_tab_before_sliding() {
    let selected = mutable_state_of(0usize);
    let handle = selected.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Tabs(handle, 3));
    ui.advance_time_by(Duration::from_millis(50));
    assert_eq!(indicator(&mut ui).map(|(left, _)| left), Some(0.0));
    selected.set(2);
    ui.wait_for_idle();
    let (left, _) = indicator(&mut ui).expect("the indicator");
    assert!(left < 100.0, "still at tab 0 when the selection changes: {left}");
}

/// Tab 1 of 2 selected in a 400 px row (the indicator at 200, 200 wide), the window widened to
/// 800 (it's drawn at 400, 400 wide), then tab 0 selected: Compose animates from where the
/// indicator is (400, 400 wide) to tab 0. Here the `Animatable` still holds the position from
/// before the resize (200, 200 wide), so the slide starts from a place the indicator never was.
#[test]
fn the_indicator_slides_from_where_it_is_after_a_resize() {
    let selected = mutable_state_of(1usize);
    let handle = selected.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Tabs(handle, 2));
    ui.host().resize(IntSize::new(400, 200));
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(indicator(&mut ui), Some((200.0, 200.0)));
    ui.host().resize(IntSize::new(800, 200));
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(indicator(&mut ui), Some((400.0, 400.0)));
    selected.set(0);
    // A few frames into the 250 ms slide: still near where it was.
    ui.advance_time_by(Duration::from_millis(32));
    let (left, width) = indicator(&mut ui).expect("the indicator");
    assert!(left > 250.0 && width > 350.0, "near (400, 400): ({left}, {width})");
}

/// A tab row with no tabs, and one whose selected index is past its tabs: no panic, as long as
/// nothing is drawn for the indicator (Compose throws for an index past the tabs; here nothing
/// is drawn). A lock-in.
#[test]
fn passes_a_tab_row_without_tabs_or_with_a_selection_past_them() {
    let selected = mutable_state_of(5usize);
    let handle = selected.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || Tabs(handle, 0));
    assert_eq!(indicator(&mut ui), None);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || Tabs(handle, 2));
    assert_eq!(indicator(&mut ui), None);
    selected.set(1);
    ui.wait_for_idle();
    assert_eq!(indicator(&mut ui), Some((200.0, 200.0)));
}
