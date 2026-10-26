//! Capturing a node's pixels, as Compose's `captureToImage`, with the pixel assertions of
//! Compose's test utilities (`assertPixels`, `assertPixelColor`, `assertContainsColor`). The
//! frame is drawn by the renderer a test installs (`set_capture_renderer`: compose-skia's
//! `raster_colors`).

use std::sync::Mutex;

use compose_ui::platform::{DisplayList, NodeId, Optimizations, TextMeasurer};
use compose_ui::{Color, IntOffset, IntSize};

use crate::interaction::SemanticsNodeInteraction;

/// The pixels of a node, as the `ImageBitmap` `captureToImage` returns: the window's content
/// where the node is (whatever is drawn there, over a white window), row by row.
#[derive(Clone, PartialEq, Debug)]
pub struct CapturedImage {
    width: i32,
    height: i32,
    pixels: Vec<Color>,
}

/// How far each channel may be from the expected one, as the test utilities' 0.02.
const CHANNEL_TOLERANCE: f32 = 0.02;

fn channels(color: Color) -> [f32; 4] {
    [color.red(), color.green(), color.blue(), color.alpha()].map(|channel| channel as f32 / 255.0)
}

impl CapturedImage {
    /// An image of `width` by `height` pixels, row by row.
    pub fn new(width: i32, height: i32, pixels: Vec<Color>) -> CapturedImage {
        assert_eq!(pixels.len(), (width.max(0) * height.max(0)) as usize, "one color per pixel");
        CapturedImage { width, height, pixels }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn size(&self) -> IntSize {
        IntSize::new(self.width, self.height)
    }

    /// The color at (`x`, `y`).
    #[track_caller]
    pub fn pixel(&self, x: i32, y: i32) -> Color {
        assert!(
            (0..self.width).contains(&x) && (0..self.height).contains(&y),
            "compose-test: pixel ({x}, {y}) outside the {}x{} image",
            self.width,
            self.height
        );
        self.pixels[(y * self.width + x) as usize]
    }

    pub fn pixels(&self) -> &[Color] {
        &self.pixels
    }

    /// The pixel at (`x`, `y`) is `expected`, each channel within 0.02, as `assertPixelColor`.
    #[track_caller]
    pub fn assert_pixel_color(&self, expected: Color, x: i32, y: i32) -> &Self {
        let actual = self.pixel(x, y);
        let close = channels(expected)
            .iter()
            .zip(channels(actual))
            .all(|(expected, actual)| (expected - actual).abs() < CHANNEL_TOLERANCE);
        assert!(
            close,
            "compose-test: Pixel({x}, {y}) expected to be {expected:?}, but was {actual:?}"
        );
        self
    }

    /// Each pixel is the color `expected` gives for it (`None`: anything), and the image is
    /// `expected_size` if given, as `assertPixels`.
    #[track_caller]
    pub fn assert_pixels(
        &self,
        expected_size: Option<IntSize>,
        expected: impl Fn(IntOffset) -> Option<Color>,
    ) -> &Self {
        if let Some(size) = expected_size {
            assert!(
                size == self.size(),
                "compose-test: Bitmap size is wrong! Expected '{size:?}' but got '{}x{}'",
                self.width,
                self.height
            );
        }
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(color) = expected(IntOffset::new(x, y)) {
                    self.assert_pixel_color(color, x, y);
                }
            }
        }
        self
    }

    /// Some pixel is exactly `color`, as `assertContainsColor`.
    #[track_caller]
    pub fn assert_contains_color(&self, color: Color) -> &Self {
        assert!(
            self.pixels.contains(&color),
            "compose-test: The given color {color:?} was not found in the bitmap."
        );
        self
    }

    /// No pixel is exactly `color`, as `assertDoesNotContainColor`.
    #[track_caller]
    pub fn assert_does_not_contain_color(&self, color: Color) -> &Self {
        assert!(
            !self.pixels.contains(&color),
            "compose-test: The given color {color:?} was found in the bitmap."
        );
        self
    }
}

impl SemanticsNodeInteraction<'_> {
    /// The window's pixels where the node is (its bounds clipped by the clips around it), as
    /// `captureToImage`. Needs a renderer (`set_capture_renderer`).
    #[track_caller]
    pub fn capture_to_image(&mut self) -> CapturedImage {
        let node = self.fetch_semantics_node();
        let bounds = self
            .rule
            .host
            .with_tree(|tree| tree.semantics_bounds_in_root(node.id))
            .map_or(node.bounds, |(_, clipped)| clipped);
        // The node's window alone: a popup or dialog over it isn't in its pixels, as Compose's
        // windows (and capturing a popup's node gives the popup's).
        let window = self.rule.host.with_tree(|tree| tree.window_of(node.id));
        let window = render(self.rule, window);
        let left = (bounds.left.round() as i32).clamp(0, window.width);
        let top = (bounds.top.round() as i32).clamp(0, window.height);
        let right = (bounds.right.round() as i32).clamp(left, window.width);
        let bottom = (bounds.bottom.round() as i32).clamp(top, window.height);
        let mut pixels = Vec::with_capacity(((right - left) * (bottom - top)) as usize);
        for y in top..bottom {
            for x in left..right {
                pixels.push(window.pixels[(y * window.width + x) as usize]);
            }
        }
        CapturedImage::new(right - left, bottom - top, pixels)
    }
}

/// Draws a display list of `size` pixels, over white, with the host's optimizations, into its
/// colors row by row: what [`capture_to_image`](SemanticsNodeInteraction::capture_to_image)
/// renders with. compose-skia's `raster_colors` is one (D904).
pub type CaptureRenderer =
    fn(&DisplayList, IntSize, &dyn TextMeasurer, Optimizations) -> Vec<Color>;

/// The renderer [`set_capture_renderer`] installed.
static RENDERER: Mutex<Option<CaptureRenderer>> = Mutex::new(None);

/// Installs `renderer` for `capture_to_image`, for the whole test binary, as a test needing
/// pixels does once (`set_capture_renderer(compose_skia::raster_colors)`): compose-test draws
/// nothing itself, so only the crates whose tests capture link a renderer (D904).
pub fn set_capture_renderer(renderer: CaptureRenderer) {
    *RENDERER.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(renderer);
}

/// Whether a renderer is installed (the failure pipeline takes a screenshot then).
pub(crate) fn has_capture_renderer() -> bool {
    RENDERER.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_some()
}

/// A window's pixels (the main content's, or an overlay's), drawn from what the last frame
/// drew.
#[track_caller]
fn render(rule: &mut crate::ComposeTestRule, window: Option<NodeId>) -> CapturedImage {
    let renderer = *RENDERER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(renderer) = renderer else {
        panic!(
            "compose-test: capture_to_image needs a renderer: call \
             `set_capture_renderer(compose_skia::raster_colors)` first (D904)"
        );
    };
    let size = rule.host.size();
    let optimizations = rule.host.optimizations();
    let pixels = rule.host.with_tree(|tree| {
        let list = tree.window_display_list(window);
        renderer(&list, size, tree.text_measurer_ref(), optimizations)
    });
    CapturedImage::new(size.width, size.height, pixels)
}
