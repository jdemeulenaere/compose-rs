//! Image review: failing tests for `Image`'s measurement, and tests locking in behaviour checked
//! against Jetpack Compose's `Image` (`Modifier.paint` with `sizeToIntrinsics = true`, then
//! `PainterNode.modifyConstraints`).
//!
//! Compose sizes a painter node from its intrinsic size *scaled by its `ContentScale`*: the
//! intrinsic size is constrained, the `ContentScale` computes a scale from the intrinsic size to
//! that constrained size, and the scaled size (constrained again) is the node's minimum. Here
//! the measurement always scales down to fit (as `ContentScale.Fit`), whatever the scale.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Rect, UnitExt, platform::DrawOp};

fn bitmap(width: u32, height: u32) -> ImageBitmap {
    ImageBitmap::from_rgba_premultiplied(width, height, vec![255; (width * height * 4) as usize])
}

fn size_of(ui: &mut ComposeTestRule, tag: &str) -> (f32, f32) {
    let bounds = ui.on_node_with_tag(tag).fetch_semantics_node().bounds;
    (bounds.width(), bounds.height())
}

fn drawn(ui: &mut ComposeTestRule) -> Rect {
    let list = ui.host().display_list();
    let images: Vec<Rect> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawImage { dst, .. } => Some(*dst),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1, "{list:?}");
    images[0]
}

/// The common "full-width banner" pattern: `Image(bitmap, contentScale = FillWidth,
/// modifier = Modifier.fillMaxWidth())`. Compose: constraints are width 300 (fixed), height
/// 0..300; the 100 × 50 intrinsic size constrains to 300 × 50, `FillWidth` scales by 3, so the
/// node is 300 × 150 and the whole bitmap shows. Here the node is 300 × 50 (scaled to fit), and
/// the bitmap, drawn 300 × 150 centered in it, is clipped to its middle third.
#[test]
fn a_fill_width_image_is_as_tall_as_its_scaled_width() {
    let image = bitmap(100, 50);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Image!(
            image.clone(),
            None,
            modifier = Modifier.fill_max_width().test_tag("image"),
            content_scale = ContentScale::FillWidth,
        );
    });
    assert_eq!(size_of(&mut ui, "image"), (300.0, 150.0));
    assert_eq!(drawn(&mut ui), Rect::new(0.0, 0.0, 300.0, 150.0));
}

#[composable]
fn Narrow(image: ImageBitmap, scale: ContentScale) {
    // Loose constraints: width 0..200, height 0..300.
    Box!(modifier = Modifier.width(200.dp()), {
        Image!(image.clone(), None, modifier = Modifier.test_tag("image"), content_scale = scale);
    });
}

/// A 400 × 100 bitmap in at most 200 × 300. Compose constrains the intrinsic size to 200 × 100
/// and scales the intrinsic size by the content scale towards it: `Fit` and `Inside` 0.5 (200 ×
/// 50), `FillWidth` 0.5 (200 × 50), `Crop`, `FillHeight` and `None` 1 (400 × 100, constrained
/// to 200 × 100), `FillBounds` 0.5 × 1 (200 × 100). Here every scale gives 200 × 50.
#[test]
fn a_constrained_image_is_sized_by_its_content_scale() {
    let cases = [
        (ContentScale::Fit, (200.0, 50.0)),
        (ContentScale::Inside, (200.0, 50.0)),
        (ContentScale::FillWidth, (200.0, 50.0)),
        (ContentScale::Crop, (200.0, 100.0)),
        (ContentScale::FillHeight, (200.0, 100.0)),
        (ContentScale::None, (200.0, 100.0)),
        (ContentScale::FillBounds, (200.0, 100.0)),
    ];
    let mut wrong = Vec::new();
    for (scale, expected) in cases {
        let image = bitmap(400, 100);
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            Narrow(image.clone(), scale);
        });
        let size = size_of(&mut ui, "image");
        if size != expected {
            wrong.push(format!("{scale:?}: {size:?}, Compose {expected:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Passes (locks in behaviour checked against Compose): an image larger than a fixed size
/// still fills it, a smaller one keeps its own size under loose constraints, and changing the
/// bitmap (or its description) of a composed `Image` re-measures and re-describes it.
#[test]
fn a_new_bitmap_resizes_and_redescribes_the_image() {
    let small = bitmap(40, 20);
    let large = bitmap(80, 60);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), {
        let (small, large) = (small.clone(), large.clone());
        move || {
            let showing_large = remember(|| mutable_state_of(false));
            let (image, description) = if showing_large.get() {
                (large.clone(), "large")
            } else {
                (small.clone(), "small")
            };
            Column!({
                Image!(image.clone(), Some(description), modifier = Modifier.test_tag("image"));
                Box!(
                    modifier =
                        Modifier.test_tag("switch").clickable(move || showing_large.set(true)),
                    {
                        Spacer!(Modifier.size(10.dp()));
                    }
                );
            });
        }
    });
    assert_eq!(size_of(&mut ui, "image"), (40.0, 20.0));
    ui.on_node_with_content_description("small").assert_exists();
    ui.on_node_with_tag("switch").perform_click();
    assert_eq!(size_of(&mut ui, "image"), (80.0, 60.0));
    ui.on_node_with_content_description("large").assert_exists();
    ui.on_node_with_content_description("small").assert_does_not_exist();
    assert_eq!(drawn(&mut ui), Rect::new(0.0, 0.0, 80.0, 60.0));
}

/// Passes (locks in): an empty (0 × 0) bitmap and a zero-height one neither panic nor draw;
/// the node takes its minimum constraints, as Compose's (a zero destination scales to
/// `Size.Zero`).
#[test]
fn an_empty_bitmap_takes_the_minimum_size_and_draws_nothing() {
    let empty = bitmap(0, 0);
    let flat = bitmap(30, 0);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (empty, flat) = (empty.clone(), flat.clone());
        Column!({
            Image!(empty.clone(), None, modifier = Modifier.test_tag("empty"));
            Image!(flat.clone(), None, modifier = Modifier.test_tag("flat"));
        });
    });
    assert_eq!(size_of(&mut ui, "empty"), (0.0, 0.0));
    assert_eq!(size_of(&mut ui, "flat"), (0.0, 0.0));
    let list = ui.host().display_list();
    assert!(!list.ops.iter().any(|op| matches!(op, DrawOp::DrawImage { .. })), "{list:?}");
}

/// Passes (locks in): a bitmap's intrinsic size is in pixels, as Compose's `BitmapPainter`: at
/// density 2 a 40 × 20 bitmap is 40 × 20 px (20 × 10 dp), not doubled.
#[test]
fn a_bitmap_is_measured_in_pixels_at_density_two() {
    let image = bitmap(40, 20);
    let mut ui = compose_test_with(
        IntSize::new(300, 300),
        compose_ui::Density::new(2.0),
        Box::new(compose_ui::platform::FixedTextMeasurer::default()),
        move || {
            Image!(image.clone(), None, modifier = Modifier.test_tag("image"));
        },
    );
    assert_eq!(size_of(&mut ui, "image"), (40.0, 20.0));
    assert_eq!(drawn(&mut ui), Rect::new(0.0, 0.0, 40.0, 20.0));
}
