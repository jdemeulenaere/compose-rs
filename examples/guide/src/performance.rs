//! Performance patterns (docs/guide/performance.md).

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: deferred
#[composable]
pub fn LiveDot() {
    let transition = remember_infinite_transition("Live");
    let alpha = transition.animate_float(
        0.2,
        1.0,
        infinite_repeatable(tween(800).easing(LINEAR_EASING), RepeatMode::Reverse),
    );
    Box!(
        modifier = Modifier
            .size(12.dp())
            // Read while drawing, as `graphicsLayer { alpha = … }`: an animation frame sets the
            // layer's alpha, and nothing recomposes, measures or draws again.
            .graphics_layer(move |layer| layer.alpha = alpha.get())
            .background_with(Color::RED, CIRCLE_SHAPE)
            .test_tag("dot")
    );
    // Not `Modifier.alpha(alpha.get())`: a read in composition recomposes this scope each frame.
}
// ANCHOR_END: deferred

// ANCHOR: read_late
#[composable]
pub fn Article(paragraphs: u32) {
    let scroll = remember_scroll_state!();
    Box!(modifier = Modifier.fill_max_size(), {
        Column!(modifier = Modifier.vertical_scroll(scroll), {
            for index in 0..paragraphs {
                Text!("Paragraph {}", index, modifier = Modifier.padding(16.dp()));
            }
        });
        // Give the child the holder (a Copy handle), not `scroll.value()`: read here, the value
        // would recompose `Article`, its whole column included, at each scrolled pixel.
        ProgressBar!(scroll);
    });
}

#[composable]
fn ProgressBar(scroll: ScrollState) {
    let color = MaterialTheme::color_scheme().primary;
    Box!(
        modifier = Modifier.fill_max_width().height(4.dp()).draw_behind(move |scope| {
            // Read while drawing: a scroll draws this bar again, and recomposes nothing.
            let fraction = scroll.value() as f32 / scroll.max_value().max(1) as f32;
            let size = scope.size();
            scope.draw_rect(
                Rect::new(0.0, 0.0, size.width * fraction, size.height),
                Paint::fill(color),
            );
        })
    );
}
// ANCHOR_END: read_late

// ANCHOR: repaint_boundary
#[composable]
pub fn Syncing(items: u32) {
    Column!({
        Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
            // The spinner draws anew every frame; a boundary around it keeps the rest of the
            // screen's drawing (and, on Android, its render node) as it was.
            IndeterminateCircularProgressIndicator!(
                modifier = Modifier.size(16.dp()).repaint_boundary()
            );
            Text!("Syncing {} items", items);
        });
        for index in 0..items {
            Text!("Item {}", index);
        }
    });
}
// ANCHOR_END: repaint_boundary

// ANCHOR: paths
#[composable]
pub fn Heartbeat() {
    // A shape that doesn't change: built once and remembered, so the platform keeps it (Android
    // defines it once).
    let heart = remember(|| Path::from_svg("M12,21 L3,12 Q3,4 12,8 Q21,4 21,12 Z").unwrap());
    // A shape that changes every frame: one path, remembered and reset in the drawing block, as
    // Compose's `remember { Path() }` and `path.reset()` (Android refills its native path).
    let trace = remember(Path::new);
    let transition = remember_infinite_transition("Heartbeat");
    let phase = transition.animate_float(
        0.0,
        1.0,
        infinite_repeatable(tween(1000).easing(LINEAR_EASING), RepeatMode::Restart),
    );
    Canvas(Modifier.size(48.dp()).test_tag("heartbeat"), move |scope| {
        scope.draw_path_scaled(&heart, Offset::ZERO, (2.0, 2.0), Paint::fill(Color::RED));
        let size = scope.size();
        trace.reset();
        for point in 0..=24 {
            let along = point as f32 / 24.0;
            let wave = (std::f32::consts::TAU * (along + phase.get())).sin();
            let (x, y) = (size.width * along, size.height * (0.5 - 0.25 * wave));
            if point == 0 {
                trace.move_to(x, y);
            } else {
                trace.line_to(x, y);
            }
        }
        scope.draw_path(&trace, Paint::stroke(Color::WHITE, 2.0));
    });
}
// ANCHOR_END: paths
