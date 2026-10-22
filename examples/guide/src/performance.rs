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
        Row!(
            // The spinner draws anew every frame; a boundary keeps the rest of the screen's
            // drawing (and, on Android, its render node) as it was.
            modifier = Modifier.repaint_boundary(),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            {
                IndeterminateCircularProgressIndicator!(modifier = Modifier.size(16.dp()));
                Text!("Syncing {} items", items);
            }
        );
        for index in 0..items {
            Text!("Item {}", index);
        }
    });
}
// ANCHOR_END: repaint_boundary
