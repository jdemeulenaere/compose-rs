//! Bitmaps, as Compose's `ImageBitmap`: pixels drawn by the display list's `DrawImage`, decoded
//! by the platform's [`ImageDecoder`] (Skia on desktop, `BitmapFactory` on Android), which apps
//! reach through [`decode_image_bytes`] and [`image_resource`].

use std::sync::{Arc, RwLock};

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

/// Decodes encoded images (PNG, JPEG, WebP, …) into bitmaps: the platform's, as the text
/// measurer is (Skia's codecs on desktop, `BitmapFactory` on Android), set once by the host
/// ([`set_image_decoder`]).
pub trait ImageDecoder: Send + Sync {
    /// The image `bytes` encode, or `None` if they're not one the platform decodes.
    fn decode(&self, bytes: &[u8]) -> Option<ImageBitmap>;
}

/// The platform's decoder, for every thread (images may be decoded off the main thread).
static DECODER: RwLock<Option<Arc<dyn ImageDecoder>>> = RwLock::new(None);

/// Sets the decoder [`decode_image_bytes`] uses: the platform host's, at startup.
pub fn set_image_decoder(decoder: Arc<dyn ImageDecoder>) {
    *DECODER.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(decoder);
}

/// Decodes an encoded image with the platform's decoder, as Android's
/// `BitmapFactory.decodeByteArray(…).asImageBitmap()`: `None` if the bytes aren't an image the
/// platform decodes, or no platform set a decoder (headless tests).
pub fn decode_image_bytes(bytes: &[u8]) -> Option<ImageBitmap> {
    let decoder = DECODER.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    decoder?.decode(bytes)
}

/// An image bundled with the app (`include_bytes!("icon.png")`), decoded once where it's
/// composed, as Compose's `ImageBitmap.imageResource(id)`: `None` if the platform can't decode
/// it.
pub fn image_resource(bytes: &'static [u8]) -> Option<ImageBitmap> {
    compose_runtime::remember_keyed((bytes.as_ptr() as usize, bytes.len()), || {
        decode_image_bytes(bytes)
    })
}
