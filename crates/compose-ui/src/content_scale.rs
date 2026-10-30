//! How a content's size maps to its node's, as Compose's `androidx.compose.ui.layout.ContentScale`:
//! here, as animation's shared bounds need it as well as foundation's `Image` (D829).

use crate::unit::Size;

/// How an image's size maps to its node's, as Compose's `ContentScale`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContentScale {
    /// Scaled to fit inside, its aspect ratio kept.
    Fit,
    /// Scaled to cover, its aspect ratio kept (the rest clipped).
    Crop,
    /// Stretched to the node's size.
    FillBounds,
    /// Scaled to the node's width, its aspect ratio kept.
    FillWidth,
    /// Scaled to the node's height, its aspect ratio kept.
    FillHeight,
    /// Scaled down to fit inside if larger, never up.
    Inside,
    /// At its own size.
    None,
}

impl compose_runtime::ComposeArg for ContentScale {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ContentScale {
    /// The scale factors from a `source` size to a `destination` size, as `computeScaleFactor`.
    pub fn scale_factor(self, source: Size, destination: Size) -> (f32, f32) {
        let (width, height) =
            (destination.width / source.width, destination.height / source.height);
        let uniform = |scale: f32| (scale, scale);
        match self {
            ContentScale::Fit => uniform(width.min(height)),
            ContentScale::Crop => uniform(width.max(height)),
            ContentScale::FillBounds => (width, height),
            ContentScale::FillWidth => uniform(width),
            ContentScale::FillHeight => uniform(height),
            ContentScale::Inside => uniform(width.min(height).min(1.0)),
            ContentScale::None => uniform(1.0),
        }
    }
}
