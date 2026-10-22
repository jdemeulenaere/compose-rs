//! Images (docs/guide/kotlin-to-rust.md, "Images").

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: image
#[composable]
pub fn Avatar() {
    // Bundled in the library and decoded once where it's composed (`ImageBitmap.imageResource`).
    // `None` when the platform can't decode it (and in headless tests, which have no decoder).
    let bitmap = remember_image_resource(include_bytes!("checkers.png"));
    let modifier = Modifier.size(48.dp()).clip(CIRCLE_SHAPE);
    if let Some(bitmap) = bitmap {
        Image!(bitmap, Some("Avatar"), modifier = modifier, content_scale = ContentScale::Crop);
    } else {
        Box!(modifier = modifier.background(MaterialTheme::color_scheme().primary_container));
    }
}
// ANCHOR_END: image
