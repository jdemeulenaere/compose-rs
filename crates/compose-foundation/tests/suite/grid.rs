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
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
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
    state.scroll_to_item(200, 0);
    ui.wait_for_idle();
    // Item 200's row (66: items 198 to 200) at the top.
    assert_eq!(state.first_visible_item_index(), 198);
    assert_eq!(bounds(&mut ui, "200").top, 0.0);
    assert_eq!(bounds(&mut ui, "200").left, 234.0);
}
