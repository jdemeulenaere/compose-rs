//! The grid screen (`grid`, not a measured scenario): tiles in columns of at least 100 dp,
//! composed lazily.

use compose::test::*;
use compose::ui::{Density, IntSize};
use compose_rs_feed::grid::*;

#[test]
fn the_grid_shows_tiles_in_four_columns_composed_lazily() {
    let mut ui = ComposeTestRule::with_manual_clock(GridScreen);
    ui.host().resize(IntSize::new(1008, 2244));
    ui.host().set_density(Density::new(2.25));
    ui.advance_time_by_frame();
    ui.on_node_with_text("Grid").assert_exists();
    // 448 dp across, less 16 dp of padding: four columns of 102 dp, 8 dp apart.
    let first = ui.on_node_with_text("Tile 1").fetch_semantics_snapshot().bounds;
    let fifth = ui.on_node_with_text("Tile 5").fetch_semantics_snapshot().bounds;
    let fourth = ui.on_node_with_text("Tile 4").fetch_semantics_snapshot().bounds;
    assert!(fourth.left > first.left && fourth.top == first.top);
    assert!(fifth.top > first.top);
    assert_eq!(ui.count_nodes(&has_text(&format!("Tile {GRID_TILES}"))), 0);
    assert_eq!(grid_kind(4), 1);
}
