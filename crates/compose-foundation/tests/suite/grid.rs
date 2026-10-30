//! `BoxWithConstraints` and `LazyVerticalGrid`: content that depends on the space it gets.

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{IntSize, UnitExt};

#[test]
fn box_with_constraints_composes_for_its_constraints() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 300), || {
        BoxWithConstraints!(
            modifier = Modifier.fill_max_size(),
            |scope: BoxWithConstraintsScope| {
                if scope.max_width() > 300.dp() {
                    BasicText!("wide {}", scope.max_width().0);
                } else {
                    BasicText!("narrow");
                }
            }
        );
    });
    ui.on_node_with_text("wide 400").assert_exists();
    ui.host().resize(IntSize::new(200, 300));
    ui.wait_for_idle();
    ui.on_node_with_text("narrow").assert_exists();
    ui.on_node_with_text("wide 400").assert_does_not_exist();
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds
}

#[test]
fn a_fixed_grid_fills_its_rows_with_equal_cells() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            horizontal_arrangement = Arrangement::spaced_by(10.dp()),
            |scope: LazyGridScope| {
                scope.items(7, |index| {
                    Box!(
                        modifier =
                            Modifier.fill_max_width().height(50.dp()).test_tag(format!("{index}"))
                    );
                });
            }
        );
    });
    // (300 - 2 × 10) / 3 = 93.33: cells 94, 93, 93 wide, as Compose's.
    let first = bounds(&mut ui, "0");
    let second = bounds(&mut ui, "1");
    let fourth = bounds(&mut ui, "3");
    assert_eq!((first.left, first.width()), (0.0, 94.0));
    assert_eq!((second.left, second.width()), (104.0, 93.0));
    assert_eq!((fourth.left, fourth.top), (0.0, 50.0));
    // The last row: one item, at the start.
    assert_eq!(bounds(&mut ui, "6").left, 0.0);
}

#[test]
fn an_adaptive_grid_fits_as_many_columns_as_it_can_and_composes_lazily() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(350, 200), move || {
        LazyVerticalGrid!(GridCells::Adaptive(100.dp()), state = given, |scope: LazyGridScope| {
            scope.items(300, |index| {
                Box!(modifier = Modifier.height(100.dp()).test_tag(format!("{index}")));
            });
        });
    });
    // Three columns of 116 or 117.
    assert_eq!(bounds(&mut ui, "2").left, 234.0);
    assert_eq!(bounds(&mut ui, "3").top, 100.0);
    // Far rows aren't composed.
    ui.on_node_with_tag("200").assert_does_not_exist();
    state.request_scroll_to_item(200, 0);
    ui.wait_for_idle();
    // Item 200's row (66: items 198 to 200) at the top.
    assert_eq!(state.first_visible_item_index(), 198);
    assert_eq!(bounds(&mut ui, "200").top, 0.0);
    assert_eq!(bounds(&mut ui, "200").left, 234.0);
}

#[test]
fn a_horizontal_grid_fills_its_columns_and_scrolls_sideways() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), move || {
        LazyHorizontalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = given,
            content_padding = PaddingValues::from(10.dp()),
            horizontal_arrangement = Arrangement::spaced_by(5.dp()),
            vertical_arrangement = Arrangement::spaced_by(20.dp()),
            |scope: LazyGridScope| {
                scope.items(100, |index| {
                    Box!(modifier = Modifier.width(50.dp()).test_tag(format!("{index}")));
                });
            }
        );
    });
    // Two rows of (200 − 2 × 10 − 20) / 2 = 80, the first column inside the padding; each item
    // as tall as its row.
    let (first, second, third) = (bounds(&mut ui, "0"), bounds(&mut ui, "1"), bounds(&mut ui, "2"));
    assert_eq!((first.left, first.top, first.height()), (10.0, 10.0, 80.0));
    assert_eq!((second.left, second.top), (10.0, 110.0));
    assert_eq!((third.left, third.top), (65.0, 10.0));
    state.dispatch_raw_delta(110.0);
    ui.wait_for_idle();
    // Two columns of 55 scrolled off: the third first, items 4 and 5.
    assert_eq!(state.first_visible_item_index(), 4);
    assert_eq!(bounds(&mut ui, "4").left, 10.0);
}

#[test]
fn grids_start_at_the_right_in_a_right_to_left_layout() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Column!({
            LazyVerticalGrid!(
                GridCells::Fixed(2),
                modifier = Modifier.size((200.dp(), 50.dp())),
                content_padding = PaddingValues::new(10.dp(), 0.dp(), 0.dp(), 0.dp()),
                |scope: LazyGridScope| {
                    scope.items(4, |index| {
                        Box!(modifier = Modifier.height(20.dp()).test_tag(format!("v{index}")));
                    });
                }
            );
            LazyHorizontalGrid!(
                GridCells::Fixed(2),
                modifier = Modifier.size((200.dp(), 100.dp())),
                |scope: LazyGridScope| {
                    scope.items(4, |index| {
                        Box!(modifier = Modifier.width(30.dp()).test_tag(format!("h{index}")));
                    });
                }
            );
        });
    });
    ui.host().set_layout_direction(compose_ui::LayoutDirection::Rtl);
    ui.wait_for_idle();
    // A vertical grid's first column at the right, its start padding there; a horizontal
    // grid's first column at the right, as a `LazyRow`'s first item.
    assert_eq!(bounds(&mut ui, "v0").left, 95.0);
    assert_eq!(bounds(&mut ui, "v1").left, 0.0);
    assert_eq!(bounds(&mut ui, "h0").left, 170.0);
    assert_eq!(bounds(&mut ui, "h2").left, 140.0);
    assert_eq!(bounds(&mut ui, "h1").top, 100.0);
}

/// As a list's and Compose's (`LazyGridMeasure`'s `beforeContentPadding`): the lines scrolled
/// into the start padding stay laid out and shown there, and the viewport takes lines under the
/// end padding.
#[test]
fn a_grid_lays_out_lines_under_its_content_padding() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            content_padding = PaddingValues::symmetric(0.dp(), 30.dp()),
            |scope: LazyGridScope| {
                scope.items(40, |index| {
                    Box!(modifier = Modifier.height(20.dp()).test_tag(format!("{index}")));
                });
            }
        );
    });
    assert_eq!(bounds(&mut ui, "0").top, 30.0);
    assert_eq!(bounds(&mut ui, "7").top, 90.0);
    ui.on_node_with_tag("8").assert_does_not_exist();
    // Two lines and 10 px down: line 2 first visible, line 1 under the start padding.
    state.dispatch_raw_delta(50.0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 4);
    assert_eq!(bounds(&mut ui, "3").top, 0.0);
    assert_eq!(bounds(&mut ui, "4").top, 20.0);
    assert_eq!(bounds(&mut ui, "11").top, 80.0);
    ui.on_node_with_tag("1").assert_does_not_exist();
    ui.on_node_with_tag("12").assert_does_not_exist();
}

/// As a list's: at a negative spacing, the line overlapping the viewport's start by the spacing
/// is laid out, but not after a scroll back to fill a gap at the end (`LazyGridMeasure.kt:247`).
#[test]
fn a_negative_spacing_lays_out_the_overlapping_line_but_after_a_scroll_back() {
    let state = LazyGridState::new(8, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            content_padding = PaddingValues::new(0.dp(), 30.dp(), 0.dp(), 0.dp()),
            vertical_arrangement = Arrangement::spaced_by((-10.0).dp()),
            |scope: LazyGridScope| {
                scope.items(20, |index| {
                    Box!(modifier = Modifier.height(20.dp()).test_tag(format!("{index}")));
                });
            }
        );
    });
    // Line 4 at 30 px, a line every 10 px: line 9 ends the grid, line 1 at 0, and line 0, at
    // -10 px, overlaps the viewport by the spacing.
    assert_eq!(bounds(&mut ui, "18").top, 80.0);
    assert_eq!(bounds(&mut ui, "2").top, 0.0);
    assert_eq!(bounds(&mut ui, "0").top, -10.0);
    // To the last line, scrolled back 50 px to end the grid: the same place, without line 0.
    state.request_scroll_to_item(18, 0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 8);
    assert_eq!(bounds(&mut ui, "18").top, 80.0);
    assert_eq!(bounds(&mut ui, "2").top, 0.0);
    ui.on_node_with_tag("0").assert_does_not_exist();
}
