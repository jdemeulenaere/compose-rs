//! Bitmaps, as Compose's `ImageBitmap`: pixels drawn by the display list's `DrawImage`, decoded
//! by the platform's [`ImageDecoder`] (Skia on desktop, `BitmapFactory` on Android), which apps
//! reach through [`decode_image_bytes`] and [`remember_image_resource`], or made to draw into
//! ([`ImageBitmap::new`], with a [`Canvas`](crate::canvas::Canvas)).

use std::sync::{Arc, Mutex, RwLock};

use crate::colorspace::{ColorSpace, ColorSpaces};
use crate::graphics::Color;

/// How a bitmap's pixels are kept, as Compose's `ImageBitmapConfig`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ImageBitmapConfig {
    #[default]
    Argb8888,
    Alpha8,
    Rgb565,
    F16,
    Gpu,
}

impl ImageBitmapConfig {
    /// Bytes stored per pixel for this config.
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            ImageBitmapConfig::Alpha8 => 1,
            ImageBitmapConfig::Rgb565 => 2,
            ImageBitmapConfig::Argb8888 | ImageBitmapConfig::Gpu => 4,
            ImageBitmapConfig::F16 => 8,
        }
    }
}

/// How a bitmap drawn at another size than its own is sampled, as Compose's `FilterQuality`
/// (`DrawScope.DefaultFilterQuality` is `Low`): `None` takes the nearest pixel; `Low`
/// interpolates between pixels; `Medium` also between mipmap levels; `High` resamples (Skia's
/// Mitchell cubic, as Skiko). Android's paints filter or don't: `Low` and up are its filtered
/// bitmaps (D2403).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum FilterQuality {
    None,
    #[default]
    Low,
    Medium,
    High,
}

impl compose_runtime::ComposeArg for FilterQuality {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A bitmap, as Compose's `ImageBitmap`: its size in pixels and its pixels, premultiplied RGBA,
/// a row after another. Shared (`Arc`): a clone copies nothing, and two clones are the same
/// image. A decoded or given bitmap's pixels never change; one made to draw into
/// ([`ImageBitmap::new`]) keeps what's drawn into it until its pixels are read, then draws it
/// all in one pass, into a copy if a renderer holds the pixels it drew; every clone sees it.
#[derive(Clone)]
pub struct ImageBitmap {
    width: u32,
    height: u32,
    pixels: Pixels,
}

#[derive(Clone)]
enum Pixels {
    Fixed(Arc<FixedPixels>),
    Drawable(Arc<Drawable>),
}

struct FixedPixels {
    pixels: Arc<[u8]>,
    config: ImageBitmapConfig,
    has_alpha: bool,
    color_space: ColorSpace,
}

struct Drawable {
    /// Its pixels and the drawings not in them yet, behind one lock: a drawing waits there,
    /// drawn with the others at the next read, one canvas or thread at a time.
    drawing: Mutex<Drawing>,
    config: ImageBitmapConfig,
    has_alpha: bool,
    color_space: ColorSpace,
}

/// A drawable bitmap's pixels and what's to be drawn into them.
pub(crate) struct Drawing {
    /// The pixels, as last drawn: replaced by a changed copy when a renderer may hold them.
    pixels: Arc<[u8]>,
    /// What canvases drew since, waiting to be drawn at the next read ([`ImageBitmap::pixels`]):
    /// each canvas's run of drawings in a save of its own, after the canvas's transforms and
    /// clips. It names no drawable bitmap (a canvas draws a snapshot of one), so drawing it
    /// reads no other bitmap's lock.
    pub(crate) pending: crate::display_list::DisplayList,
    /// The canvas (its id, and its state's version) whose run `pending` ends in, and the saves
    /// to restore to close it: the next drawing of that canvas in that state joins the run.
    pub(crate) open: Option<(u64, u64, usize)>,
}

impl Drawing {
    /// Closes the open run: restores its saves.
    pub(crate) fn close(&mut self) {
        if let Some((_, _, saves)) = self.open.take() {
            for _ in 0..saves {
                self.pending.ops.push(crate::display_list::DrawOp::Restore);
            }
        }
    }
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
        ImageBitmap {
            width,
            height,
            pixels: Pixels::Fixed(Arc::new(FixedPixels {
                pixels: pixels.into(),
                config: ImageBitmapConfig::Argb8888,
                has_alpha: true,
                color_space: ColorSpace::clone(&ColorSpaces::SRGB),
            })),
        }
    }

    /// A transparent sRGB bitmap of `width` by `height` to draw into, as
    /// `ImageBitmap(width, height)`.
    pub fn new(width: u32, height: u32) -> ImageBitmap {
        ImageBitmap::new_with(width, height, ImageBitmapConfig::Argb8888, true, &ColorSpaces::SRGB)
    }

    /// A transparent bitmap to draw into, as
    /// `ImageBitmap(width, height, config, hasAlpha, colorSpace)`.
    pub fn new_with(
        width: u32,
        height: u32,
        config: ImageBitmapConfig,
        has_alpha: bool,
        color_space: &ColorSpace,
    ) -> ImageBitmap {
        let mut raw = vec![0u8; width as usize * height as usize * config.bytes_per_pixel()];
        if !has_alpha {
            // As Android's `Bitmap.createBitmap` (`nativeErase(0xff000000)`): opaque 32-bit and
            // F16 bitmaps start as opaque black so untouched pixels carry alpha 1.0.
            match config {
                ImageBitmapConfig::Argb8888 | ImageBitmapConfig::Gpu => {
                    for chunk in raw.chunks_exact_mut(4) {
                        chunk[3] = 255;
                    }
                }
                ImageBitmapConfig::F16 => {
                    for chunk in raw.chunks_exact_mut(8) {
                        chunk[6..8].copy_from_slice(&0x3c00u16.to_le_bytes());
                    }
                }
                ImageBitmapConfig::Alpha8 | ImageBitmapConfig::Rgb565 => {}
            }
        }
        let pixels = raw.into();
        let drawing = Drawing { pixels, pending: Default::default(), open: None };
        let drawable = Drawable {
            drawing: Mutex::new(drawing),
            config,
            has_alpha,
            color_space: color_space.clone(),
        };
        ImageBitmap { width, height, pixels: Pixels::Drawable(Arc::new(drawable)) }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// What it was made with, as `config`: `Argb8888` for a decoded bitmap.
    pub fn config(&self) -> ImageBitmapConfig {
        match &self.pixels {
            Pixels::Fixed(fixed) => fixed.config,
            Pixels::Drawable(drawable) => drawable.config,
        }
    }

    /// Whether it has alpha, as `hasAlpha`.
    pub fn has_alpha(&self) -> bool {
        match &self.pixels {
            Pixels::Fixed(fixed) => fixed.has_alpha,
            Pixels::Drawable(drawable) => drawable.has_alpha,
        }
    }

    /// Its color space, as `colorSpace`: sRGB for a decoded bitmap.
    pub fn color_space(&self) -> &ColorSpace {
        match &self.pixels {
            Pixels::Fixed(fixed) => &fixed.color_space,
            Pixels::Drawable(drawable) => &drawable.color_space,
        }
    }

    /// The pixels now, encoded per [`Self::config`]: what was drawn into it is drawn first.
    pub fn pixels(&self) -> Arc<[u8]> {
        match &self.pixels {
            Pixels::Fixed(fixed) => fixed.pixels.clone(),
            Pixels::Drawable(drawable) => {
                let mut drawing = drawable.lock();
                self.draw_pending(drawable, &mut drawing);
                drawing.pixels.clone()
            }
        }
    }

    /// Draws what waits in `drawing` into its pixels, with the platform's rasterizer: in place
    /// if nothing else holds them, else into a copy (a renderer may hold the ones it drew).
    fn draw_pending(&self, drawable: &Drawable, drawing: &mut Drawing) {
        if drawing.pending.ops.is_empty() {
            return;
        }
        drawing.close();
        let list = std::mem::take(&mut drawing.pending);
        let Some(rasterizer) = crate::canvas::rasterizer() else {
            log::warn!(
                "compose-ui: no bitmap rasterizer: drawing into a bitmap draws nothing here"
            );
            return;
        };
        if Arc::get_mut(&mut drawing.pixels).is_none() {
            drawing.pixels = Arc::from(&drawing.pixels[..]);
        }
        let pixels = Arc::get_mut(&mut drawing.pixels).expect("compose-ui: pixels of its own");
        rasterizer.draw(
            &list,
            self.width,
            self.height,
            drawable.config,
            drawable.has_alpha,
            &drawable.color_space,
            pixels,
        );
    }

    /// Its pixels as they are now, as a bitmap that doesn't change: what a drawing of it into a
    /// bitmap reads, as a canvas reads a bitmap when it draws it.
    pub(crate) fn snapshot(&self) -> ImageBitmap {
        match &self.pixels {
            Pixels::Fixed(_) => self.clone(),
            Pixels::Drawable(drawable) => ImageBitmap {
                width: self.width,
                height: self.height,
                pixels: Pixels::Fixed(Arc::new(FixedPixels {
                    pixels: self.pixels(),
                    config: drawable.config,
                    has_alpha: drawable.has_alpha,
                    color_space: drawable.color_space.clone(),
                })),
            },
        }
    }

    /// Runs `block` on what waits to be drawn into it, its lock held (one drawing at a time):
    /// `None`, and a warning, for a decoded bitmap, whose pixels don't change.
    pub(crate) fn with_drawing<R>(&self, block: impl FnOnce(&mut Drawing) -> R) -> Option<R> {
        let Pixels::Drawable(drawable) = &self.pixels else {
            log::warn!("compose-ui: a decoded bitmap can't be drawn into");
            return None;
        };
        Some(block(&mut drawable.lock()))
    }

    /// An identity for caches of its pixels: the same for clones of one image while its pixels
    /// don't change.
    pub fn id(&self) -> usize {
        match &self.pixels {
            Pixels::Fixed(fixed) => fixed.pixels.as_ptr() as usize,
            Pixels::Drawable(_) => self.pixels().as_ptr() as usize,
        }
    }

    /// A weak reference to the pixels now, for caches that mustn't keep the image alive.
    pub fn downgrade(&self) -> std::sync::Weak<[u8]> {
        match &self.pixels {
            Pixels::Fixed(fixed) => Arc::downgrade(&fixed.pixels),
            Pixels::Drawable(_) => Arc::downgrade(&self.pixels()),
        }
    }

    /// Whether `other` is a clone of this image.
    pub fn same_image(&self, other: &ImageBitmap) -> bool {
        match (&self.pixels, &other.pixels) {
            (Pixels::Fixed(a), Pixels::Fixed(b)) => Arc::ptr_eq(a, b),
            (Pixels::Drawable(a), Pixels::Drawable(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// The color at `(x, y)` in [`Self::color_space`], unpremultiplied, as Android's `Bitmap.getColor`.
    ///
    /// # Panics
    ///
    /// If `(x, y)` is outside the bitmap.
    pub fn color(&self, x: u32, y: u32) -> Color {
        assert!(
            x < self.width && y < self.height,
            "compose-ui: pixel ({x}, {y}) past the bitmap's {} × {}",
            self.width,
            self.height
        );
        let pixels = self.pixels();
        let index = y as usize * self.width as usize + x as usize;
        let [r, g, b, a] = decode_pixel(&pixels, index, self.config(), self.has_alpha());
        Color::new_with(r, g, b, a, self.color_space())
    }

    /// Reads a part of its pixels as unpremultiplied ARGB, as `readPixels`: the `width` by
    /// `height` pixels from `(start_x, start_y)` into `buffer` from `buffer_offset`, a row every
    /// `stride` values.
    ///
    /// # Panics
    ///
    /// If the part isn't in the bitmap or doesn't fit in the buffer.
    #[allow(clippy::too_many_arguments)]
    pub fn read_pixels(
        &self,
        buffer: &mut [u32],
        start_x: u32,
        start_y: u32,
        width: u32,
        height: u32,
        buffer_offset: usize,
        stride: usize,
    ) {
        assert!(
            start_x + width <= self.width && start_y + height <= self.height,
            "compose-ui: pixels to read past the bitmap's {} × {}",
            self.width,
            self.height
        );
        let pixels = self.pixels();
        let config = self.config();
        let has_alpha = self.has_alpha();
        let color_space = self.color_space();
        if matches!(config, ImageBitmapConfig::Argb8888 | ImageBitmapConfig::Gpu)
            && has_alpha
            && color_space.is_srgb()
        {
            for row in 0..height as usize {
                for column in 0..width as usize {
                    let x = start_x as usize + column;
                    let at = ((start_y as usize + row) * self.width as usize + x) * 4;
                    let alpha = u32::from(pixels[at + 3]);
                    let unpremultiply = |channel: u8| {
                        (u32::from(channel) * 255 + alpha / 2)
                            .checked_div(alpha)
                            .map_or(0, |value| value.min(255))
                    };
                    buffer[buffer_offset + row * stride + column] = alpha << 24
                        | unpremultiply(pixels[at]) << 16
                        | unpremultiply(pixels[at + 1]) << 8
                        | unpremultiply(pixels[at + 2]);
                }
            }
            return;
        }
        for row in 0..height as usize {
            for column in 0..width as usize {
                let index =
                    (start_y as usize + row) * self.width as usize + start_x as usize + column;
                let [r, g, b, a] = decode_pixel(&pixels, index, config, has_alpha);
                buffer[buffer_offset + row * stride + column] =
                    Color::new_with(r, g, b, a, color_space).to_argb();
            }
        }
    }

    /// All its pixels as a [`PixelMap`], as `toPixelMap()`.
    pub fn to_pixel_map(&self) -> PixelMap {
        let size = self.width as usize * self.height as usize;
        self.to_pixel_map_with(0, 0, self.width, self.height, vec![0; size], 0, self.width as usize)
    }

    /// A part of its pixels read into `buffer`, as
    /// `toPixelMap(startX, startY, width, height, buffer, bufferOffset, stride)`.
    #[allow(clippy::too_many_arguments)]
    pub fn to_pixel_map_with(
        &self,
        start_x: u32,
        start_y: u32,
        width: u32,
        height: u32,
        mut buffer: Vec<u32>,
        buffer_offset: usize,
        stride: usize,
    ) -> PixelMap {
        self.read_pixels(&mut buffer, start_x, start_y, width, height, buffer_offset, stride);
        PixelMap { buffer, width, height, buffer_offset, stride }
    }

    /// Readies it to be drawn, as `prepareToDraw`: nothing to do here.
    pub fn prepare_to_draw(&self) {}
}

/// Pixels read from a bitmap, as Compose's `PixelMap`: unpremultiplied ARGB, a row every
/// `stride` values from `buffer_offset`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PixelMap {
    buffer: Vec<u32>,
    width: u32,
    height: u32,
    buffer_offset: usize,
    stride: usize,
}

impl PixelMap {
    /// The color at `(x, y)` of the part read, as `get(x, y)`.
    pub fn get(&self, x: i32, y: i32) -> Color {
        Color::from_argb(self.buffer[self.buffer_offset + y as usize * self.stride + x as usize])
    }

    pub fn buffer(&self) -> &[u32] {
        &self.buffer
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn buffer_offset(&self) -> usize {
        self.buffer_offset
    }

    pub fn stride(&self) -> usize {
        self.stride
    }
}

impl Drawable {
    fn lock(&self) -> std::sync::MutexGuard<'_, Drawing> {
        self.drawing.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
/// it. Named `remember_…` as it takes a slot: `#[composable]` puts a group around a call by that
/// name in a branch, so calling it conditionally keeps later state in place.
pub fn remember_image_resource(bytes: &'static [u8]) -> Option<ImageBitmap> {
    compose_runtime::remember_keyed((bytes.as_ptr() as usize, bytes.len()), || {
        decode_image_bytes(bytes)
    })
}

/// Decodes pixel `index` from `pixels` (`config`, `has_alpha`) into unpremultiplied `[r, g, b, a]`.
fn decode_pixel(
    pixels: &[u8],
    index: usize,
    config: ImageBitmapConfig,
    has_alpha: bool,
) -> [f32; 4] {
    match config {
        ImageBitmapConfig::Argb8888 | ImageBitmapConfig::Gpu => {
            let at = index * 4;
            let a_u8 = if has_alpha { pixels[at + 3] } else { 255 };
            if a_u8 == 0 {
                return [0.0; 4];
            }
            let scale = 1.0 / f32::from(a_u8);
            [
                f32::from(pixels[at]) * scale,
                f32::from(pixels[at + 1]) * scale,
                f32::from(pixels[at + 2]) * scale,
                f32::from(a_u8) / 255.0,
            ]
        }
        ImageBitmapConfig::Alpha8 => {
            [0.0, 0.0, 0.0, if has_alpha { f32::from(pixels[index]) / 255.0 } else { 1.0 }]
        }
        ImageBitmapConfig::Rgb565 => {
            let at = index * 2;
            let value = u16::from_le_bytes([pixels[at], pixels[at + 1]]);
            [
                f32::from((value >> 11) & 0x1f) / 31.0,
                f32::from((value >> 5) & 0x3f) / 63.0,
                f32::from(value & 0x1f) / 31.0,
                1.0,
            ]
        }
        ImageBitmapConfig::F16 => {
            let at = index * 8;
            let half = |offset: usize| {
                crate::colorspace::half::half_to_float(u16::from_le_bytes([
                    pixels[at + offset],
                    pixels[at + offset + 1],
                ]))
            };
            let (mut r, mut g, mut b) = (half(0), half(2), half(4));
            let a = if has_alpha { half(6) } else { 1.0 };
            if a > 0.0 {
                r /= a;
                g /= a;
                b /= a;
            } else {
                (r, g, b) = (0.0, 0.0, 0.0);
            }
            [r, g, b, a]
        }
    }
}

/// Encodes unpremultiplied `[r, g, b, a]` at pixel `index` in `pixels` (`config`, `has_alpha`).
fn encode_pixel(
    pixels: &mut [u8],
    index: usize,
    config: ImageBitmapConfig,
    has_alpha: bool,
    [r, g, b, a]: [f32; 4],
) {
    let a = if has_alpha { a.clamp(0.0, 1.0) } else { 1.0 };
    match config {
        ImageBitmapConfig::Alpha8 => {
            pixels[index] = (a * 255.0 + 0.5) as u8;
        }
        ImageBitmapConfig::Rgb565 => {
            let r5 = (r.clamp(0.0, 1.0) * 31.0 + 0.5) as u16;
            let g6 = (g.clamp(0.0, 1.0) * 63.0 + 0.5) as u16;
            let b5 = (b.clamp(0.0, 1.0) * 31.0 + 0.5) as u16;
            pixels[index * 2..index * 2 + 2]
                .copy_from_slice(&((r5 << 11) | (g6 << 5) | b5).to_le_bytes());
        }
        ImageBitmapConfig::Argb8888 | ImageBitmapConfig::Gpu => {
            let at = index * 4;
            let premul = |value: f32| ((value * a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            pixels[at] = premul(r);
            pixels[at + 1] = premul(g);
            pixels[at + 2] = premul(b);
            pixels[at + 3] = (a * 255.0 + 0.5) as u8;
        }
        ImageBitmapConfig::F16 => {
            let at = index * 8;
            let write = |buf: &mut [u8], value: f32| {
                buf.copy_from_slice(&crate::colorspace::half::float_to_half(value).to_le_bytes());
            };
            write(&mut pixels[at..at + 2], r * a);
            write(&mut pixels[at + 2..at + 4], g * a);
            write(&mut pixels[at + 4..at + 6], b * a);
            write(&mut pixels[at + 6..at + 8], a);
        }
    }
}

/// Converts `pixels` (`config`, `has_alpha`, `color_space`) to premultiplied `RGBAF16` in
/// [`ColorSpaces::EXTENDED_SRGB`], for renderers that draw or sample in sRGB / extended sRGB.
pub fn image_to_extended_srgb_f16(
    pixels: &[u8],
    config: ImageBitmapConfig,
    has_alpha: bool,
    color_space: &ColorSpace,
) -> Vec<u8> {
    let count = pixels.len() / config.bytes_per_pixel();
    let connector =
        (!color_space.is_srgb() && *color_space != ColorSpaces::EXTENDED_SRGB).then(|| {
            crate::colorspace::connect(color_space, &ColorSpaces::EXTENDED_SRGB, Default::default())
        });
    let mut out = vec![0u8; count * 8];
    for index in 0..count {
        let [mut r, mut g, mut b, a] = decode_pixel(pixels, index, config, has_alpha);
        if let Some(connector) = &connector {
            [r, g, b] = connector.transform(r, g, b);
        }
        encode_pixel(&mut out, index, ImageBitmapConfig::F16, true, [r, g, b, a]);
    }
    out
}

/// Converts premultiplied `RGBAF16` pixels in [`ColorSpaces::EXTENDED_SRGB`] back to `pixels`
/// (`config`, `has_alpha`, `color_space`).
pub fn extended_srgb_f16_to_image(
    src: &[u8],
    dst: &mut [u8],
    config: ImageBitmapConfig,
    has_alpha: bool,
    color_space: &ColorSpace,
) {
    let count = dst.len() / config.bytes_per_pixel();
    let connector =
        (!color_space.is_srgb() && *color_space != ColorSpaces::EXTENDED_SRGB).then(|| {
            crate::colorspace::connect(&ColorSpaces::EXTENDED_SRGB, color_space, Default::default())
        });
    for index in 0..count {
        let [mut r, mut g, mut b, a] = decode_pixel(src, index, ImageBitmapConfig::F16, true);
        if let Some(connector) = &connector {
            [r, g, b] = connector.transform(r, g, b);
        }
        encode_pixel(dst, index, config, has_alpha, [r, g, b, a]);
    }
}
