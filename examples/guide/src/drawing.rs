//! Drawing (docs/guide/kotlin-to-rust.md, "Drawing").

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: canvas
#[composable]
pub fn Gauge(fraction: f32) {
    let track = MaterialTheme::color_scheme().surface_variant;
    let progress = MaterialTheme::color_scheme().primary;
    Canvas(Modifier.size(96.dp()).test_tag("gauge"), move |scope| {
        let stroke = scope.density().to_px(8.dp());
        let size = scope.size();
        let oval = Rect::new(
            stroke / 2.0,
            stroke / 2.0,
            size.width - stroke / 2.0,
            size.height - stroke / 2.0,
        );
        scope.draw_arc(oval, 135.0, 270.0, Paint::stroke_with(track, stroke, StrokeCap::Round));
        let sweep = 270.0 * fraction.clamp(0.0, 1.0);
        scope.draw_arc(oval, 135.0, sweep, Paint::stroke_with(progress, stroke, StrokeCap::Round));
    });
}
// ANCHOR_END: canvas

// ANCHOR: draw_behind
#[composable]
pub fn Badge(label: SharedString) {
    let color = MaterialTheme::color_scheme().error_container;
    Text!(
        label,
        modifier = Modifier
            .draw_behind(move |scope| {
                let radius = scope.size().height / 2.0;
                scope.draw_round_rect(scope.bounds(), [radius; 4], Paint::fill(color));
            })
            .padding((8.dp(), 2.dp()))
    );
}

#[composable]
pub fn Dimmed(content: impl Fn() + 'static) {
    Box!(
        modifier = Modifier.draw_with_content(|scope| {
            scope.draw_content(); // the content first, then a scrim over it
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(Color::BLACK.with_alpha(0.3)));
        }),
        {
            content();
        }
    );
}
// ANCHOR_END: draw_behind

// ANCHOR: draw_with_cache
#[composable]
pub fn Ribbon() {
    Box!(
        modifier = Modifier.size((120.dp(), 48.dp())).draw_with_cache(|cache| {
            // Runs again only for a new size (or density, direction, or state it read).
            let size = cache.size();
            let path = Path::new();
            path.move_to(0.0, 0.0);
            path.line_to(size.width, 0.0);
            path.line_to(size.width - size.height / 2.0, size.height / 2.0);
            path.line_to(size.width, size.height);
            path.line_to(0.0, size.height);
            path.close();
            let brush = Brush::horizontal_gradient([
                Color::from_argb(0xFF6750A4),
                Color::from_argb(0xFF7D5260),
            ]);
            // Runs at each draw, with what the cache built.
            cache.on_draw_behind(move |scope| {
                let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
                scope.draw_path(&path, paint);
            })
        })
    );
}
// ANCHOR_END: draw_with_cache

// ANCHOR: brushes
#[composable]
pub fn Gradients() {
    Column!(vertical_arrangement = Arrangement::spaced_by(8.dp()), {
        // A brush where Compose takes one: `background(brush)`, `background(brush, shape)`.
        Box!(
            modifier = Modifier
                .size((96.dp(), 32.dp()))
                .background(Brush::horizontal_gradient([Color::RED, Color::BLUE]))
        );
        Box!(
            modifier = Modifier.size(64.dp()).background_with(
                Brush::radial_gradient([Color::WHITE, Color::BLACK]).with_alpha(0.5),
                CIRCLE_SHAPE
            )
        );
        // Stops and every parameter: `Brush.linearGradient(0f to red, 1f to blue, start, end, tileMode)`.
        Box!(
            modifier = Modifier.size(64.dp()).background(Brush::linear_gradient_with(
                [(0.0, Color::RED), (0.3, Color::YELLOW), (1.0, Color::BLUE)],
                Offset::ZERO,
                Offset::INFINITE,
                TileMode::Clamp,
            ))
        );
    });
}
// ANCHOR_END: brushes
