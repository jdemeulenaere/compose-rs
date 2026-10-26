//! The resize screen: texts in a container whose width animates every frame, the same in
//! `android/bench-compose`'s `Resize.kt`, widget for widget. Not one of measure.py's scenarios:
//! an ad hoc check of text layout across width changes (D550, D551), as a window being resized.
//!
//! - `resize`: a top app bar over a box whose content is laid out at a width animating between
//!   half and all of it (1.5 s each way, linear, read while measuring): a title, a wrapping
//!   paragraph, a centered title across the whole width, an ellipsized line, a grid of 10 × 4
//!   labelled cells ([`ResizeGrid`]) and 12 nested levels of labels ([`ResizeDeep`]).

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

use crate::{COLORS, SENTENCES, TAGS};

/// The grid's rows and columns.
pub const RESIZE_ROWS: u32 = 10;
pub const RESIZE_COLUMNS: u32 = 4;
/// The nested levels.
pub const RESIZE_LEVELS: u32 = 12;

#[composable]
pub fn ResizeScreen() {
    let color_scheme = crate::bench_color_scheme();
    MaterialTheme!(color_scheme = color_scheme, {
        Scaffold!(
            top_bar = {
                TopAppBar!(
                    {
                        Text!("Resize");
                    },
                    window_insets = crate::top_app_bar_window_insets()
                );
            },
            content_window_insets = crate::scaffold_content_window_insets(),
            |padding: PaddingValues| {
                let transition = remember_infinite_transition("Resize");
                let fraction = transition.animate_float(
                    0.5,
                    1.0,
                    infinite_repeatable(tween(1500).easing(LINEAR_EASING), RepeatMode::Reverse),
                );
                Box!(
                    modifier = Modifier.padding(padding).fill_max_size().layout(
                        move |scope, measurable, constraints| {
                            let width =
                                (constraints.max_width as f32 * fraction.get()).round() as i32;
                            let placeable = scope.measure(
                                measurable,
                                Constraints::fixed(width, constraints.max_height),
                            );
                            scope.place(&placeable, 0, 0);
                            scope.layout(constraints.max_width, constraints.max_height)
                        }
                    ),
                    {
                        ResizeContent();
                    }
                );
            }
        );
    });
}

/// What the screen resizes: as wide as it's given.
#[composable]
pub fn ResizeContent() {
    let typography = MaterialTheme::typography();
    Column!(
        modifier = Modifier.fill_max_size().padding(8.dp()),
        vertical_arrangement = Arrangement::spaced_by(4.dp()),
        {
            Text!("Resizing", style = typography.title_large.clone());
            Text!("{} {}", SENTENCES[0], SENTENCES[1], style = typography.body_medium.clone());
            Text!(
                "Centered across the width",
                modifier = Modifier.fill_max_width(),
                text_align = Some(TextAlign::Center),
                style = typography.title_medium.clone()
            );
            Text!(
                "{} {}",
                SENTENCES[2],
                SENTENCES[3],
                max_lines = 1,
                overflow = TextOverflow::Ellipsis,
                style = typography.body_small.clone()
            );
            ResizeGrid();
            ResizeDeep(0);
        }
    );
}

/// Rows of cells sharing the width, each labelled in its middle.
#[composable]
pub fn ResizeGrid() {
    Column!(vertical_arrangement = Arrangement::spaced_by(4.dp()), {
        for row in 0..RESIZE_ROWS {
            key(row, move || {
                Row!(
                    modifier = Modifier.fill_max_width(),
                    horizontal_arrangement = Arrangement::spaced_by(4.dp()),
                    |scope: RowScope| {
                        for column in 0..RESIZE_COLUMNS {
                            let index = row * RESIZE_COLUMNS + column;
                            Box!(
                                modifier = Modifier
                                    .weight(scope, 1.0)
                                    .height(28.dp())
                                    .background(Color::from_argb(COLORS[(index % 8) as usize])),
                                content_alignment = Alignment::CENTER,
                                {
                                    Text!(
                                        "R{} C{}",
                                        row + 1,
                                        column + 1,
                                        style = MaterialTheme::typography().label_medium,
                                        color = Color::WHITE
                                    );
                                }
                            );
                        }
                    }
                );
            });
        }
    });
}

/// A label, and the next level indented below it, `RESIZE_LEVELS` deep.
#[composable]
pub fn ResizeDeep(level: u32) {
    Column!(modifier = Modifier.fill_max_width().padding((6.dp(), 0.dp(), 0.dp(), 0.dp())), {
        Text!(
            "Level {} · {}",
            level + 1,
            TAGS[(level % 6) as usize],
            style = MaterialTheme::typography().label_large
        );
        if level + 1 < RESIZE_LEVELS {
            ResizeDeep(level + 1);
        }
    });
}
