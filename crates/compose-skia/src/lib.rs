//! Skia backends for compose-rs, through `skia-safe` (docs/07-platforms.md): the display-list
//! renderer, on a CPU raster surface for tests, screenshots and windows, and the SkParagraph
//! text measurer.

// Every type reachable from the public API can be named (D260): a type in a private
// module that a public signature uses must be re-exported.
#![deny(unnameable_types)]

mod render;
mod text;

pub use render::{
    RasterRenderer, SkiaImageDecoder, decode_image, draw_display_list, install_path_operator,
    raster_colors,
};
pub use skia_safe;
pub use text::SkiaTextMeasurer;
