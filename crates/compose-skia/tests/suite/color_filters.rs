//! Color filters and blend modes drawn by Skia: on a paint, an image and a layer.

use compose::ui::{
    BlendMode, Color, ColorFilter, ColorMatrix, ImageBitmap, Paint, Rect,
    platform::{DisplayList, DrawOp, FixedTextMeasurer, Renderer},
};
use compose_skia::RasterRenderer;

const SQUARE: Rect = Rect::new(0.0, 0.0, 10.0, 10.0);

fn render(list: &DisplayList) -> RasterRenderer {
    let mut renderer = RasterRenderer::new(10, 10);
    renderer.render(list, Color::WHITE, 1.0, &FixedTextMeasurer::default());
    renderer
}

/// A square of `color` through `filter`, over white.
fn filtered_square(color: Color, filter: ColorFilter) -> Color {
    let mut list = DisplayList::default();
    let index = list.push_color_filter(filter);
    list.push(DrawOp::DrawRect(SQUARE, Paint::fill(color).with_color_filter(index)));
    render(&list).pixel(5, 5)
}

/// A tint replaces each pixel's color, keeping its alpha (`SrcIn`); another mode blends it
/// with the pixel (`Modulate` multiplies).
#[test]
fn a_tint_recolors_by_its_blend_mode() {
    assert_eq!(filtered_square(Color::RED, ColorFilter::tint(Color::BLUE)), Color::BLUE);
    let modulated = ColorFilter::tint_with(Color::from_argb(0xFF80_FFFF), BlendMode::Modulate);
    assert_eq!(
        filtered_square(Color::from_rgb(255, 255, 0), modulated),
        Color::from_rgb(128, 255, 0)
    );
}

/// Lighting multiplies each channel, then adds; a color matrix's offsets are in 0 to 255, as
/// Compose's (Android's) `ColorMatrix`.
#[test]
fn lighting_and_matrices_transform_each_channel() {
    let lit = ColorFilter::lighting(Color::from_rgb(255, 0, 255), Color::from_rgb(0, 255, 0));
    assert_eq!(filtered_square(Color::from_rgb(255, 255, 0), lit), Color::from_rgb(255, 255, 0));
    let mut swap = ColorMatrix::from_values([0.0; 20]);
    swap.set(0, 2, 1.0); // red from blue
    swap.set(2, 0, 1.0); // blue from red
    swap.set(1, 4, 128.0); // green: half
    swap.set(3, 3, 1.0);
    let swapped = filtered_square(Color::RED, ColorFilter::color_matrix(swap));
    assert_eq!(swapped, swap.transform(Color::RED));
    assert_eq!(swapped, Color::from_rgb(0, 128, 255));
    let mut gray = ColorMatrix::new();
    gray.set_to_saturation(0.0);
    let grayed = filtered_square(Color::RED, ColorFilter::color_matrix(gray));
    assert!(
        ((grayed.to_argb() >> 16) as u8) == ((grayed.to_argb() >> 8) as u8)
            && ((grayed.to_argb() >> 8) as u8) == (grayed.to_argb() as u8),
        "{grayed:?}"
    );
}

/// A paint's blend mode combines it with what's under it: `Clear` erases, even with a
/// transparent color; `Multiply` darkens.
#[test]
fn a_paints_blend_mode_combines_it_with_the_destination() {
    let mut list = DisplayList::default();
    list.push(DrawOp::DrawRect(SQUARE, Paint::fill(Color::from_rgb(255, 255, 0))));
    let multiply = Paint::fill(Color::from_rgb(0, 255, 255)).with_blend_mode(BlendMode::Multiply);
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 5.0, 10.0), multiply));
    let clear = Paint::fill(Color::TRANSPARENT).with_blend_mode(BlendMode::Clear);
    list.push(DrawOp::DrawRect(Rect::new(5.0, 0.0, 10.0, 10.0), clear));
    let mut renderer = RasterRenderer::new(10, 10);
    renderer.render(&list, Color::TRANSPARENT, 1.0, &FixedTextMeasurer::default());
    assert_eq!(renderer.pixel(2, 5), Color::from_rgb(0, 255, 0));
    assert_eq!(((renderer.pixel(7, 5).to_argb() >> 24) as u8), 0);
}

/// A layer with a filter recolors everything drawn in it, at its alpha; an image through a
/// lighting filter is lit.
#[test]
fn a_layer_and_an_image_draw_through_their_filters() {
    let mut list = DisplayList::default();
    let tint = list.push_color_filter(ColorFilter::tint(Color::BLUE));
    list.push(DrawOp::SaveLayer {
        bounds: SQUARE,
        alpha: 1.0,
        color_filter: Some(tint),
        blend_mode: BlendMode::SrcOver,
    });
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 5.0, 10.0), Paint::fill(Color::RED)));
    list.push(DrawOp::Restore);
    let lit = list.push_color_filter(ColorFilter::lighting(Color::BLACK, Color::GREEN));
    list.push(DrawOp::DrawImage {
        image: ImageBitmap::from_rgba_premultiplied(1, 1, vec![255, 0, 0, 255]),
        src: Rect::new(0.0, 0.0, 1.0, 1.0),
        dst: Rect::new(5.0, 0.0, 10.0, 10.0),
        alpha: 1.0,
        color_filter: Some(lit),
        filter_quality: compose::ui::FilterQuality::Low,
    });
    let mut renderer = render(&list);
    assert_eq!(renderer.pixel(2, 5), Color::BLUE);
    assert_eq!(renderer.pixel(7, 5), Color::GREEN);
}

/// A transparent paint is drawn when it clears or goes through a filter: either can draw
/// something (D1900); a plain transparent one is left out.
#[test]
fn a_transparent_paint_that_clears_or_is_filtered_is_recorded() {
    use compose::foundation::layout::Box;
    use compose::test::*;
    use compose::ui::{IntSize, Modifier, UnitExt};
    compose::test::set_capture_renderer(compose_skia::raster_colors);
    let mut ui = ComposeTestRule::sized(IntSize::new(20, 20), || {
        Box!(
            modifier = Modifier.test_tag("drawn").size(10.dp()).background(Color::RED).draw_behind(
                |scope| {
                    let bounds = scope.bounds();
                    scope.draw_rect(bounds, Paint::fill(Color::TRANSPARENT));
                    let left = Rect::new(0.0, 0.0, 5.0, 10.0);
                    let clear = Paint::fill(Color::TRANSPARENT).with_blend_mode(BlendMode::Clear);
                    scope.draw_rect(left, clear);
                    let tint = ColorFilter::tint_with(Color::BLUE, BlendMode::Src);
                    let tinted =
                        scope.paint_with_color_filter(Paint::fill(Color::TRANSPARENT), Some(&tint));
                    scope.draw_rect(Rect::new(5.0, 0.0, 10.0, 10.0), tinted);
                }
            )
        );
    });
    let dump = ui.host().display_list().dump();
    assert_eq!(dump.matches("#00000000").count(), 2, "{dump}");
    assert!(dump.contains("blend Clear") && dump.contains("tint #FF0000FF Src"), "{dump}");
    ui.on_node_with_tag("drawn").capture_to_image().assert_pixel_color(Color::BLUE, 7, 5);
}
