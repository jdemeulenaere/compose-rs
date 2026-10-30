//! `Image`: a bitmap's size, scaled by its `ContentScale`, aligned and clipped in its node.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Rect, UnitExt, platform::DrawOp};

fn bitmap(width: u32, height: u32) -> ImageBitmap {
    ImageBitmap::from_rgba_premultiplied(width, height, vec![255; (width * height * 4) as usize])
}

/// The destination of the only image drawn.
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

#[test]
fn an_image_is_as_large_as_its_pixels_and_reads_its_description() {
    let image = bitmap(40, 20);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Image!(image.clone(), Some("A cat"), modifier = Modifier.test_tag("image"));
    });
    let node = ui.on_node_with_content_description("A cat").fetch_semantics_snapshot();
    assert_eq!((node.bounds.width(), node.bounds.height()), (40.0, 20.0));
    ui.on_node(has_role(Role::Image)).assert_exists();
    assert_eq!(drawn(&mut ui), Rect::new(0.0, 0.0, 40.0, 20.0));
}

#[test]
fn a_large_image_shrinks_to_its_constraints_keeping_its_aspect_ratio() {
    let image = bitmap(400, 200);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let image = image.clone();
        Box!(modifier = Modifier.width(100.dp()), {
            Image!(image.clone(), None, modifier = Modifier.test_tag("image"));
        });
    });
    let bounds = ui.on_node_with_tag("image").fetch_semantics_snapshot().bounds;
    assert_eq!((bounds.width(), bounds.height()), (100.0, 50.0));
    assert_eq!(drawn(&mut ui), Rect::new(0.0, 0.0, 100.0, 50.0));
    // Decorative: no role, no description.
    ui.on_node(has_role(Role::Image)).assert_does_not_exist();
}

#[composable]
fn Scaled(image: ImageBitmap, scale: ContentScale) {
    Image!(image, None, modifier = Modifier.size((100.dp(), 100.dp())), content_scale = scale);
}

#[test]
fn content_scales_map_the_bitmap_into_the_node() {
    // A 200 × 100 and an 80 × 30 bitmap in a 100 × 100 node, centered.
    let cases = [
        (ContentScale::Fit, (200, 100), Rect::new(0.0, 25.0, 100.0, 75.0)),
        (ContentScale::Crop, (200, 100), Rect::new(-50.0, 0.0, 150.0, 100.0)),
        (ContentScale::FillBounds, (200, 100), Rect::new(0.0, 0.0, 100.0, 100.0)),
        (ContentScale::FillWidth, (200, 100), Rect::new(0.0, 25.0, 100.0, 75.0)),
        (ContentScale::FillHeight, (200, 100), Rect::new(-50.0, 0.0, 150.0, 100.0)),
        (ContentScale::Inside, (200, 100), Rect::new(0.0, 25.0, 100.0, 75.0)),
        (ContentScale::Inside, (80, 30), Rect::new(10.0, 35.0, 90.0, 65.0)),
        (ContentScale::None, (200, 100), Rect::new(-50.0, 0.0, 150.0, 100.0)),
        (ContentScale::None, (80, 30), Rect::new(10.0, 35.0, 90.0, 65.0)),
        (ContentScale::FillBounds, (80, 30), Rect::new(0.0, 0.0, 100.0, 100.0)),
    ];
    for (scale, (width, height), expected) in cases {
        let image = bitmap(width, height);
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            Scaled(image.clone(), scale);
        });
        assert_eq!(drawn(&mut ui), expected, "{scale:?} {width}×{height}");
    }
}

#[test]
fn a_cropped_image_is_clipped_to_its_node() {
    let image = bitmap(200, 100);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Scaled(image.clone(), ContentScale::Crop);
    });
    let list = ui.host().display_list();
    let clip = list.ops.iter().position(|op| matches!(op, DrawOp::ClipRect(..)));
    let image = list.ops.iter().position(|op| matches!(op, DrawOp::DrawImage { .. }));
    assert!(clip.is_some() && clip < image, "{list:?}");
}

#[test]
fn the_alignment_and_alpha_are_applied() {
    let image = bitmap(20, 20);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Image(
            image.clone(),
            None,
            Modifier.size((100.dp(), 50.dp())),
            Alignment::BOTTOM_END,
            ContentScale::None,
            0.5,
        );
    });
    assert_eq!(drawn(&mut ui), Rect::new(80.0, 30.0, 100.0, 50.0));
    let list = ui.host().display_list();
    assert!(
        list.ops.iter().any(|op| matches!(op, DrawOp::DrawImage { alpha, .. } if *alpha == 0.5))
    );
}

/// A description built at run time, as Compose's `contentDescription: String?` (D1022).
#[test]
fn an_images_description_can_be_built_at_run_time() {
    let mut ui = compose_test(|| {
        let index = 7;
        Image!(bitmap(4, 4), Some(format!("Photo {index}")));
    });
    ui.on_node_with_content_description("Photo 7").assert_exists();
}
