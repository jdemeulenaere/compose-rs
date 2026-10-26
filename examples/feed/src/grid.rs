//! The grid screen: a lazy grid of tiles, the same in `android/bench-compose`'s `Grid.kt`,
//! widget for widget. Not one of measure.py's scenarios (its scroll benchmarks are lazy lists
//! only): for ad hoc fling traces of lazy grids (D456's two lines composed beyond each side).
//!
//! - `grid`: a top app bar over a `LazyVerticalGrid` of 3,000 keyed tiles, `Adaptive(100.dp)`
//!   (four columns on the Pixel 8 Pro), 8 dp apart. A tile is a square in one of the palette's
//!   colors, rounded 12 dp, with an icon and its label; the icon is one of three, its content
//!   type.

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

use crate::COLORS;

/// The tiles of the grid.
pub const GRID_TILES: u32 = 3_000;

/// The kind of tile `index` is, which picks its icon: also its content type.
pub fn grid_kind(index: u32) -> u32 {
    index % 3
}

/// The grid: a top app bar over 3,000 tiles.
#[composable]
pub fn GridScreen() {
    let color_scheme = crate::bench_color_scheme();
    MaterialTheme!(color_scheme = color_scheme, {
        Scaffold!(
            top_bar = {
                TopAppBar!({
                    Text!("Grid");
                });
            },
            |padding: PaddingValues| {
                LazyVerticalGrid!(
                    GridCells::Adaptive(100.dp()),
                    modifier = Modifier.padding(padding).fill_max_size(),
                    content_padding = PaddingValues::all(8.dp()),
                    vertical_arrangement = Arrangement::spaced_by(8.dp()),
                    horizontal_arrangement = Arrangement::spaced_by(8.dp()),
                    |scope: LazyGridScope| {
                        scope.items_keyed_with(
                            GRID_TILES as usize,
                            |index| index as u64,
                            |index| grid_kind(index as u32),
                            |index| GridTile(index as u32),
                        );
                    }
                );
            }
        );
    });
}

#[composable]
fn GridTile(index: u32) {
    let icon = match grid_kind(index) {
        0 => icons::filled::star(),
        1 => icons::filled::favorite(),
        _ => icons::filled::thumb_up(),
    };
    Box!(
        modifier = Modifier
            .fill_max_width()
            .aspect_ratio(1.0)
            .clip(RoundedCornerShape(12.dp()))
            .background(Color::from_argb(COLORS[(index % 8) as usize])),
        content_alignment = Alignment::CENTER,
        {
            Column!(horizontal_alignment = Alignment::CENTER_HORIZONTALLY, {
                Icon!(icon, None, tint = Color::WHITE);
                Text!(
                    "Tile {}",
                    index + 1,
                    style = MaterialTheme::typography().label_medium,
                    color = Color::WHITE
                );
            });
        }
    );
}
