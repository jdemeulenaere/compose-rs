//! Skia backends for compose-rs, through `skia-safe` (docs/07-platforms.md): the display-list
//! renderer, on a CPU raster surface for tests, screenshots and windows, and the SkParagraph
//! text measurer.

mod render;
mod text;

pub use render::{RasterRenderer, draw_display_list};
pub use skia_safe;
pub use text::SkiaTextMeasurer;
