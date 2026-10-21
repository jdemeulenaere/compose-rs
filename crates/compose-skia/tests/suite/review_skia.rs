//! Skia review: the `DrawImage` replay and `decode_image`.

use compose::ui::{Color, DisplayList, DrawOp, FixedTextMeasurer, ImageBitmap, Rect, Renderer};
use compose_skia::RasterRenderer;

/// A 2 × 1 bitmap: a red pixel, then a blue one (opaque, so premultiplied is plain).
fn red_then_blue() -> ImageBitmap {
    ImageBitmap::from_rgba_premultiplied(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255])
}

fn render(ops: Vec<DrawOp>, size: i32) -> RasterRenderer {
    let mut renderer = RasterRenderer::new(size, size);
    let mut list = DisplayList::default();
    list.ops = ops;
    renderer.render(&list, Color::WHITE, 1.0, &FixedTextMeasurer::default());
    renderer
}

/// Drawing part of a bitmap (`src` = its left, red pixel) into a large `dst`: Compose's
/// `drawImage(image, srcOffset, srcSize, dstOffset, dstSize)` samples only inside `src`
/// (Skiko's `drawImageRect(..., strict = true)`; Android's `drawBitmap(src, dst)` too), so the
/// whole destination is red, as a sprite sheet's cell. Here the replay uses
/// `SrcRectConstraint::Fast` with bilinear filtering: the blue pixel next to `src` bleeds into
/// the destination's right half, which fades to purple.
#[test]
fn a_part_of_a_bitmap_is_sampled_within_its_source_rect() {
    let renderer = &mut render(
        vec![DrawOp::DrawImage {
            image: red_then_blue(),
            src: Rect::new(0.0, 0.0, 1.0, 1.0),
            dst: Rect::new(0.0, 0.0, 100.0, 100.0),
            alpha: 1.0,
        }],
        100,
    );
    assert_eq!(renderer.pixel(10, 50), Color::RED);
    assert_eq!(renderer.pixel(98, 50), Color::RED, "the right edge of the destination");
}

/// Passes (locks in): a 0 × 0 bitmap and an alpha out of range neither panic nor misdraw; a
/// translucent PNG decodes to premultiplied pixels; the same bitmap drawn twice (one Skia image,
/// cached) and a bitmap dropped between frames draw correctly.
#[test]
fn edge_case_images_draw_and_decode() {
    let empty = ImageBitmap::from_rgba_premultiplied(0, 0, Vec::new());
    let image = red_then_blue();
    let renderer = &mut render(
        vec![
            DrawOp::DrawImage {
                image: empty,
                src: Rect::new(0.0, 0.0, 0.0, 0.0),
                dst: Rect::new(0.0, 0.0, 10.0, 10.0),
                alpha: 1.0,
            },
            DrawOp::DrawImage {
                image: image.clone(),
                src: Rect::new(0.0, 0.0, 2.0, 1.0),
                dst: Rect::new(0.0, 20.0, 20.0, 30.0),
                alpha: 7.0,
            },
            DrawOp::DrawImage {
                image: image.clone(),
                src: Rect::new(0.0, 0.0, 2.0, 1.0),
                dst: Rect::new(0.0, 40.0, 20.0, 50.0),
                alpha: -1.0,
            },
        ],
        100,
    );
    assert_eq!(renderer.pixel(5, 5), Color::WHITE);
    assert_eq!(renderer.pixel(2, 25), Color::RED);
    assert_eq!(renderer.pixel(18, 25), Color::BLUE);
    assert_eq!(renderer.pixel(2, 45), Color::WHITE, "alpha clamped to 0");
    drop(image);
    // A new bitmap after the old one is gone: its own pixels, not a stale cached image.
    let green = ImageBitmap::from_rgba_premultiplied(1, 1, vec![0, 255, 0, 255]);
    let renderer = &mut render(
        vec![DrawOp::DrawImage {
            image: green,
            src: Rect::new(0.0, 0.0, 1.0, 1.0),
            dst: Rect::new(0.0, 0.0, 10.0, 10.0),
            alpha: 1.0,
        }],
        10,
    );
    assert_eq!(renderer.pixel(5, 5), Color::from_argb(0xFF00FF00));
    // Half-transparent red over transparent, through PNG: premultiplied when decoded.
    let mut source = RasterRenderer::new(2, 2);
    let mut list = DisplayList::default();
    list.push(DrawOp::DrawRect(
        Rect::new(0.0, 0.0, 2.0, 2.0),
        compose::ui::Paint::fill(Color::from_argb(0x80FF0000)),
    ));
    source.render(&list, Color::TRANSPARENT, 1.0, &FixedTextMeasurer::default());
    let decoded = compose_skia::decode_image(&source.png()).expect("a PNG");
    assert_eq!(&decoded.pixels()[0..4], &[128, 0, 0, 128]);
    assert!(compose_skia::decode_image(&[]).is_none());
    assert!(compose_skia::decode_image(&source.png()[..20]).is_none(), "a truncated header");
}

/// Passes (locks in, checked against Compose's `graphicsLayer`): a layer scaled around its
/// center, rotated, translated with `clip = true` (the clip moves with the layer, as a
/// RenderNode's clip to its bounds) and faded, replayed through Skia at density 2.
#[test]
fn graphics_layers_replay_through_skia() {
    use compose::foundation::layout::Box;
    use compose::runtime::composable;
    use compose::ui::{Density, GraphicsLayerScope, IntSize, Modifier, UiHost, UnitExt};
    use compose_skia::SkiaTextMeasurer;
    #[composable]
    #[allow(non_snake_case)]
    fn Layers() {
        compose::foundation::layout::Column!({
            // 10 dp at (0, 0) scaled by 2 around its center: (-5, -5) to (15, 15) dp.
            Box!(modifier = Modifier.size(10.dp()).scale(2.0).background(Color::RED));
            Box!(modifier = Modifier.size(20.dp()));
            // Translated 10 px right, clipped: the clip goes with it.
            Box!(
                modifier = Modifier
                    .size(10.dp())
                    .graphics_layer_with(GraphicsLayerScope {
                        translation_x: 10.0,
                        clip: true,
                        ..GraphicsLayerScope::default()
                    })
                    .background(Color::BLUE)
            );
            Box!(modifier = Modifier.size(10.dp()).alpha(0.5).background(Color::BLACK));
        });
    }
    let mut host =
        UiHost::new(IntSize::new(100, 200), Density::new(2.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(Layers);
    host.frame();
    let mut renderer = RasterRenderer::new(100, 200);
    let list = host.display_list();
    host.with_tree(|tree| renderer.render(&list, Color::WHITE, 1.0, tree.text_measurer()));
    assert_eq!(renderer.pixel(28, 28), Color::RED, "scaled past its bounds");
    assert_eq!(renderer.pixel(32, 32), Color::WHITE);
    // The blue box: laid out at y 60..80 px, x 0..20, drawn at x 10..30.
    assert_eq!(renderer.pixel(5, 70), Color::WHITE, "moved away");
    assert_eq!(renderer.pixel(25, 70), Color::BLUE, "the clip moved with it");
    let faded = renderer.pixel(10, 90);
    assert!((126..=129).contains(&faded.red()), "{faded:?}");
}
