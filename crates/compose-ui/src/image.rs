//! Bitmaps, as Compose's `ImageBitmap`: pixels drawn by the display list's `DrawImage`, decoded
//! by the platform (`compose_skia::decode_image` on desktop).

use std::sync::Arc;

/// A bitmap, as Compose's `ImageBitmap`: its size in pixels and its pixels, premultiplied RGBA,
/// a row after another. Shared (`Arc`): a clone copies nothing, and two clones are the same
/// image.
#[derive(Clone)]
pub struct ImageBitmap {
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

impl ImageBitmap {
    /// A bitmap of `width` by `height` premultiplied RGBA pixels (4 bytes each).
    ///
    /// # Panics
    ///
    /// If `pixels` isn't `width × height × 4` bytes.
    pub fn from_rgba_premultiplied(width: u32, height: u32, pixels: Vec<u8>) -> ImageBitmap {
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * 4,
            "compose-ui: an image of {width} × {height} pixels needs {} bytes",
            width as usize * height as usize * 4
        );
        ImageBitmap { width, height, pixels: pixels.into() }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, premultiplied RGBA.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// An identity for caches: the same for clones of one image.
    pub fn id(&self) -> usize {
        self.pixels.as_ptr() as usize
    }

    /// A weak reference to the pixels, for caches that mustn't keep the image alive.
    pub fn downgrade(&self) -> std::sync::Weak<[u8]> {
        Arc::downgrade(&self.pixels)
    }

    /// Whether `other` is a clone of this image.
    pub fn same_image(&self, other: &ImageBitmap) -> bool {
        Arc::ptr_eq(&self.pixels, &other.pixels)
    }
}

impl PartialEq for ImageBitmap {
    fn eq(&self, other: &Self) -> bool {
        self.same_image(other)
    }
}

impl std::fmt::Debug for ImageBitmap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "ImageBitmap({} × {})", self.width, self.height)
    }
}

impl compose_runtime::ComposeArg for ImageBitmap {
    fn same(&self, other: &Self) -> bool {
        self.same_image(other)
    }
}
