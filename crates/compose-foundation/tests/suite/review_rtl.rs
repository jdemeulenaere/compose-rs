//! Right-to-left layout review: failing tests for `place_relative`, the scrolling containers and
//! the layouts mirrored in a right-to-left layout (docs/06-ui.md, "Layout direction"; D139).
//! Each test states the input, what Jetpack Compose does, and what happens instead.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{IntSize, LayoutDirection, UnitExt};

fn rtl(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), content);
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.wait_for_idle();
    ui
}

fn left(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds.left
}

/// Input: right to left, a 100 px `Row` with `horizontal_scroll` (at the window's right,
/// 100..200) holding five clickable 50 px boxes: box 0 at 150, box 1 at 100, boxes 2..4 beyond
/// the viewport's left edge. Box 3 (at 0..50 in the window, -100..-50 in the viewport) is
/// focused.
///
/// Compose: the focused box is brought into view (`ContentInViewNode`, in the scroll's reversed
/// direction): the scroll moves 100 px forward, box 3 at the viewport's left edge (100).
///
/// Instead: `ScrollNode::bring_into_view` computes the delta as in a left-to-right scroll: box
/// 3 before the viewport's left edge asks for -100 px, a scroll backward clamped at 0. Nothing
/// moves and the focused box stays out of view.
#[test]
fn focusing_a_box_beyond_the_left_of_a_right_to_left_scroll_brings_it_into_view() {
    let state = ScrollState::new(0);
    let given = state.clone();
    let mut ui = rtl(move || {
        Row!(modifier = Modifier.width(100.dp()).horizontal_scroll(given.clone()), {
            for index in 0..5 {
                Box!(
                    modifier = Modifier.size(50.dp()).clickable(|| {}).test_tag(format!("{index}"))
                );
            }
        });
    });
    assert_eq!(left(&mut ui, "3"), 0.0);
    ui.on_node_with_tag("3").request_focus();
    ui.wait_for_idle();
    assert_eq!(state.value(), 100, "scrolled forward to show box 3");
    assert_eq!(left(&mut ui, "3"), 100.0);
}

/// The same in left to right, for reference: box 3 (at 150..200, 50 px past the viewport's
/// right edge) is brought into view by scrolling 100 px.
#[test]
fn passes_focusing_a_box_beyond_the_right_of_a_left_to_right_scroll_brings_it_into_view() {
    let state = ScrollState::new(0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        Row!(modifier = Modifier.width(100.dp()).horizontal_scroll(given.clone()), {
            for index in 0..5 {
                Box!(
                    modifier = Modifier.size(50.dp()).clickable(|| {}).test_tag(format!("{index}"))
                );
            }
        });
    });
    ui.on_node_with_tag("3").request_focus();
    ui.wait_for_idle();
    assert_eq!(state.value(), 100);
    assert_eq!(left(&mut ui, "3"), 50.0);
}

/// Input: right to left, a 100 px `LazyRow` (at 100..200) of 10 clickable 30 px items: item 0
/// at 170, item 1 at 140, item 2 at 110, item 3 at 80..110, 20 px past the viewport's left
/// edge. Item 3 is focused.
///
/// Compose: the item is brought into view: the list scrolls 20 px forward, item 3 at the
/// viewport's left edge (100).
///
/// Instead: `LazyScrollNode::bring_into_view` asks for -20 px, a scroll backward from the
/// start: nothing moves.
#[test]
fn focusing_an_item_past_the_left_of_a_right_to_left_lazy_row_brings_it_into_view() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut ui = rtl(move || {
        LazyRow!(
            modifier = Modifier.size_wh(100.dp(), 30.dp()),
            state = given.clone(),
            |scope: LazyListScope| {
                scope.items(10, |index| {
                    Box!(
                        modifier =
                            Modifier.size(30.dp()).clickable(|| {}).test_tag(format!("{index}"))
                    );
                });
            }
        );
    });
    assert_eq!(left(&mut ui, "3"), 80.0);
    ui.on_node_with_tag("3").request_focus();
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_scroll_offset(), 20, "scrolled 20 px forward");
    assert_eq!(left(&mut ui, "3"), 100.0);
}

/// Input: right to left, a 105 px `LazyColumn` (at 95..200) with `horizontal_alignment =
/// CENTER_HORIZONTALLY` and a 100 px item: 5 px to share.
///
/// Compose: a vertical list aligns its items with the layout direction and places them
/// absolutely (`LazyListMeasuredItem`: `horizontalAlignment.align(width, layoutWidth,
/// layoutDirection)`, then `placeWithLayer`): the 2.5 px center rounds to 3 from the left, as
/// in left to right: the item at 98.
///
/// Instead: the list aligns left to right and mirrors the result (`place_relative`,
/// crates/compose-foundation/src/lazy.rs, `direction = LayoutDirection::Ltr`): 3 px from the
/// right, the item at 97, a pixel off from the same list's left-to-right position and from a
/// `Column`'s.
#[test]
fn a_centered_lazy_column_item_rounds_as_in_left_to_right() {
    let mut ui = rtl(|| {
        LazyColumn!(
            modifier = Modifier.size_wh(105.dp(), 100.dp()),
            horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
            |scope: LazyListScope| {
                scope.item(|| {
                    Box!(modifier = Modifier.size_wh(100.dp(), 20.dp()).test_tag("item"));
                });
            }
        );
    });
    assert_eq!(left(&mut ui, "item"), 98.0);
}
