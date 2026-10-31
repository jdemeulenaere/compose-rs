//! Compose's `IconTest.kt` for the bitmap overload, here `BitmapIcon`; the `Painter` overload waits
//! on the `painter` feature, and the vector icon's tests are in `suite/material.rs`.

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, ImageBitmap, IntSize, Modifier, Rect, Role, UnitExt};

const TAG: &str = "icon";

fn bitmap(width: u32, height: u32) -> ImageBitmap {
    // Red, opaque, premultiplied.
    let pixels = [255u8, 0, 0, 255].repeat((width * height) as usize);
    ImageBitmap::from_rgba_premultiplied(width, height, pixels)
}

fn size_of(ui: &mut ComposeTestRule) -> (f32, f32) {
    let bounds = ui.on_node_with_tag_unmerged(TAG).fetch_semantics_snapshot().bounds;
    (bounds.width(), bounds.height())
}

/// What's drawn: where, and in what tint.
fn drawn(ui: &mut ComposeTestRule) -> (Rect, Option<Color>) {
    ui.host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawImage { dst, tint, .. } => Some((*dst, *tint)),
            _ => None,
        })
        .expect("an image")
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconTest.kt, image_noIntrinsicSize_dimensions
#[test]
fn a_bitmap_of_an_icons_size() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        MaterialTheme!({
            BitmapIcon!(bitmap(24, 24), None::<&str>, modifier = Modifier.test_tag(TAG));
        });
    });
    ui.wait_for_idle();
    assert_eq!(size_of(&mut ui), (24.0, 24.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconTest.kt, image_withIntrinsicSize_dimensions
#[test]
fn a_bitmap_is_its_own_size() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        MaterialTheme!({
            BitmapIcon!(bitmap(35, 83), None::<&str>, modifier = Modifier.test_tag(TAG));
        });
    });
    ui.wait_for_idle();
    assert_eq!(size_of(&mut ui), (35.0, 83.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconTest.kt, iconScalesToFitSize
#[test]
fn a_bitmap_scales_to_the_size_given() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        MaterialTheme!({
            BitmapIcon!(
                bitmap(24, 24),
                None::<&str>,
                modifier = Modifier.size(50.dp()).test_tag(TAG),
                tint = Color::UNSPECIFIED
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(size_of(&mut ui), (50.0, 50.0));
    assert_eq!(drawn(&mut ui).0, Rect::new(0.0, 0.0, 50.0, 50.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconTest.kt, iconUnspecifiedTintColorIgnored
#[test]
fn an_unspecified_tint_leaves_the_bitmap_as_it_is() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        MaterialTheme!({
            BitmapIcon!(bitmap(35, 83), None::<&str>, tint = Color::UNSPECIFIED);
        });
    });
    ui.wait_for_idle();
    assert_eq!(drawn(&mut ui).1, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconTest.kt, iconSpecifiedTintColorApplied
#[test]
fn a_tint_colors_the_bitmap() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        MaterialTheme!({
            BitmapIcon!(bitmap(35, 83), None::<&str>, tint = Color::BLUE);
        });
    });
    ui.wait_for_idle();
    assert_eq!(drawn(&mut ui).1, Some(Color::BLUE));
}

/// By default, the content color.
#[test]
fn the_tint_is_by_default_the_content_color() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(Color::GREEN), || {
                BitmapIcon!(bitmap(10, 10), None::<&str>);
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(drawn(&mut ui).1, Some(Color::GREEN));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconTest.kt, defaultSemanticsWhenContentDescriptionProvided
#[test]
fn a_description_makes_it_an_image() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        MaterialTheme!({
            BitmapIcon!(bitmap(100, 100), Some("qwerty"), modifier = Modifier.test_tag(TAG));
        });
    });
    ui.wait_for_idle();
    let node = ui.on_node_with_tag(TAG).fetch_semantics_snapshot();
    assert_eq!(node.config.get(&compose_ui::SemanticsProperties::ROLE), Some(&Role::Image));
    let description =
        node.config.get(&compose_ui::SemanticsProperties::CONTENT_DESCRIPTION).expect("one");
    assert_eq!(description.len(), 1);
    assert_eq!(&*description[0], "qwerty");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconTest.kt, iconRenderedCorrectlyInSmallBox
#[test]
fn in_a_small_box_it_fits() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        MaterialTheme!({
            Box!(
                modifier = Modifier.size(10.dp()),
                content = |_| {
                    BitmapIcon!(bitmap(24, 24), None::<&str>, modifier = Modifier.test_tag(TAG));
                }
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(size_of(&mut ui), (10.0, 10.0));
    assert_eq!(drawn(&mut ui).0, Rect::new(0.0, 0.0, 10.0, 10.0));
}

/// A tint that changes is drawn: the node's told, and draws again.
#[test]
fn a_tint_that_changes_is_drawn() {
    let which = mutable_state_of(false);
    let handle = which.handle();
    let image = bitmap(2, 2);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let tint = if handle.get() { Color::BLUE } else { Color::RED };
        let image = image.clone();
        MaterialTheme!({
            BitmapIcon!(image.clone(), None::<&str>, tint = tint);
        });
    });
    ui.wait_for_idle();
    let tint = |ui: &mut ComposeTestRule| {
        ui.host().display_list().ops.iter().find_map(|op| match op {
            DrawOp::DrawImage { tint, .. } => *tint,
            _ => None,
        })
    };
    assert_eq!(tint(&mut ui), Some(Color::RED));
    which.set(true);
    ui.wait_for_idle();
    assert_eq!(tint(&mut ui), Some(Color::BLUE));
}

/// Transparent draws nothing: on the wire, 0 is no tint, which would draw the bitmap as it is.
#[test]
fn a_transparent_tint_draws_nothing() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        MaterialTheme!({
            BitmapIcon!(bitmap(2, 2), None::<&str>, tint = Color::TRANSPARENT);
        });
    });
    ui.wait_for_idle();
    let images = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::DrawImage { .. }))
        .count();
    assert_eq!(images, 0);
}

/// A bitmap that isn't the shape of its box fits inside it, centered, as `ContentScale.Fit`.
#[test]
fn a_bitmap_of_another_shape_fits_centered() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        MaterialTheme!({
            BitmapIcon!(bitmap(48, 24), None::<&str>, modifier = Modifier.size(24.dp()));
        });
    });
    ui.wait_for_idle();
    let dst = ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawImage { dst, .. } => Some(*dst),
        _ => None,
    });
    assert_eq!(dst, Some(Rect::new(0.0, 6.0, 24.0, 18.0)));
}
