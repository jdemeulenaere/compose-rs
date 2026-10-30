//! Capturing a node's pixels, as Compose's `captureToImage`, with the pixel assertions of
//! Compose's test utilities (`assertPixels`, `assertPixelColor`, `assertContainsColor`). The
//! frame is drawn by the renderer a test installs (`set_capture_renderer`: compose-skia's
//! `raster_colors`).

use std::cell::Cell;

use compose_ui::platform::{DisplayList, NodeId, Optimizations, TextMeasurer};
use compose_ui::{
    Color, Density, Dp, IntOffset, IntSize, LayoutDirection, Offset, Rect, Shape, Size,
};

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

    /// `shape` filled with `shape_color` over the whole image, `background_color` around it if
    /// given, as the test utilities' `assertShape(density, shape, shapeColor, backgroundColor)`:
    /// pixels within 1 dp of an edge (anti-aliased) aren't checked.
    #[track_caller]
    pub fn assert_shape(
        &self,
        density: Density,
        shape: Shape,
        shape_color: Color,
        background_color: Option<Color>,
    ) -> &Self {
        self.assert_shape_with(&ShapeCheck::new(
            density,
            shape,
            shape_color,
            background_color,
            self,
        ))
    }

    /// `shape` filled with `shape_color`, `horizontal_padding` and `vertical_padding` in from
    /// the image's edges, over `background_color`, as `assertShape(density, horizontalPadding,
    /// verticalPadding, backgroundColor, shapeColor, shape)`.
    #[track_caller]
    pub fn assert_shape_padded(
        &self,
        density: Density,
        horizontal_padding: Dp,
        vertical_padding: Dp,
        background_color: Color,
        shape_color: Color,
        shape: Shape,
    ) -> &Self {
        let mut check = ShapeCheck::new(density, shape, shape_color, Some(background_color), self);
        check.shape_size = Size::new(
            self.width as f32 - density.to_px(horizontal_padding) * 2.0,
            self.height as f32 - density.to_px(vertical_padding) * 2.0,
        );
        self.assert_shape_with(&check)
    }

    /// The shape and its background as `check` says, as the full `assertShape`: inside the
    /// shape (farther than the gap from its edge, away from its center), the shape's color;
    /// outside it and inside the background's shape, the background's; pixels outside the
    /// background's bounds aren't checked.
    #[track_caller]
    pub fn assert_shape_with(&self, check: &ShapeCheck) -> &Self {
        let bounds = |center: Offset, size: Size| {
            Rect::new(
                center.x - size.width / 2.0,
                center.y - size.height / 2.0,
                center.x + size.width / 2.0,
                center.y + size.height / 2.0,
            )
        };
        let shape_bounds = bounds(check.shape_center, check.shape_size);
        let background_bounds = bounds(check.background_center, check.background_size);
        let outline = |shape: &Shape, bounds: Rect| {
            shape.outline_in(
                Size::new(bounds.width(), bounds.height()),
                &check.density,
                LayoutDirection::Ltr,
            )
        };
        let shape_outline = outline(&check.shape, shape_bounds);
        let background_outline = outline(&check.background_shape, background_bounds);
        let gap = check.anti_aliasing_gap;
        let inside = |outline: &compose_ui::Outline, bounds: Rect, point: Offset| {
            outline.contains(Offset::new(point.x - bounds.left, point.y - bounds.top))
        };
        // Farther from the center by the gap (inside even there: well inside), or closer to it
        // (outside even there: well outside), as the test utilities' anchor moves.
        let farther = |x: f32, y: f32, center: Offset| {
            let move_away = |value: f32, anchor: f32| {
                if value > anchor {
                    value + gap
                } else if value < anchor {
                    value - gap
                } else {
                    value
                }
            };
            Offset::new(move_away(x, center.x), move_away(y, center.y))
        };
        let closer = |x: f32, y: f32, center: Offset| {
            let move_in = |value: f32, anchor: f32| {
                if value > anchor {
                    (value - gap).max(anchor)
                } else if value < anchor {
                    (value + gap).min(anchor)
                } else {
                    value
                }
            };
            Offset::new(move_in(x, center.x), move_in(y, center.y))
        };
        let in_bounds = |bounds: Rect, x: f32, y: f32| {
            bounds.left <= x + gap
                && bounds.top <= y + gap
                && bounds.right >= x - gap
                && bounds.bottom >= y - gap
        };
        let range = |from: f32, to: f32, limit: i32| {
            (from.max(0.0).round() as i32)..(to.min(limit as f32).round() as i32)
        };
        let rows = range(
            shape_bounds.top.min(background_bounds.top),
            shape_bounds.bottom.max(background_bounds.bottom),
            self.height,
        );
        for y in rows {
            let columns = range(
                shape_bounds.left.min(background_bounds.left),
                shape_bounds.right.max(background_bounds.right),
                self.width,
            );
            for x in columns {
                let (fx, fy) = (x as f32, y as f32);
                let in_shape = in_bounds(shape_bounds, fx, fy);
                let in_background = in_bounds(background_bounds, fx, fy);
                let shape_inside = in_shape
                    && inside(&shape_outline, shape_bounds, farther(fx, fy, check.shape_center));
                if shape_inside {
                    self.assert_pixel_color(check.shape_color, x, y);
                    continue;
                }
                let Some(background_color) = check.background_color else { continue };
                let shape_outside = !in_shape
                    || !inside(&shape_outline, shape_bounds, closer(fx, fy, check.shape_center));
                if in_background
                    && shape_outside
                    && inside(
                        &background_outline,
                        background_bounds,
                        farther(fx, fy, check.background_center),
                    )
                {
                    self.assert_pixel_color(background_color, x, y);
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
        let node = self.fetch_semantics_snapshot();
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

thread_local! {
    /// The renderer [`set_capture_renderer`] installed on this thread.
    static RENDERER: Cell<Option<CaptureRenderer>> = const { Cell::new(None) };
}

/// Installs `renderer` for `capture_to_image` on this thread (each test's: the UI is the
/// thread's too), as a test needing pixels does first
/// (`set_capture_renderer(compose_skia::raster_colors)`), or a fake one: compose-test draws
/// nothing itself, so only the crates whose tests capture link a renderer (D904).
pub fn set_capture_renderer(renderer: CaptureRenderer) {
    RENDERER.with(|installed| installed.set(Some(renderer)));
}

/// Removes this thread's renderer.
pub fn clear_capture_renderer() {
    RENDERER.with(|installed| installed.set(None));
}

/// Whether a renderer is installed (the failure pipeline takes a screenshot then).
pub(crate) fn has_capture_renderer() -> bool {
    RENDERER.with(Cell::get).is_some()
}

/// A window's pixels (the main content's, or an overlay's), drawn from what the last frame
/// drew.
#[track_caller]
fn render(rule: &mut crate::ComposeTestRule, window: Option<NodeId>) -> CapturedImage {
    let Some(renderer) = RENDERER.with(Cell::get) else {
        panic!(
            "compose-test: capture_to_image needs a renderer: call \
             `set_capture_renderer(compose_skia::raster_colors)` first (D904)"
        );
    };
    let size = rule.host.size();
    let optimizations = rule.host.optimizations();
    let pixels = rule.host.with_tree(|tree| {
        let list = tree.window_display_list(window);
        renderer(&list, size, &*tree.text_measurer_ref(), optimizations)
    });
    CapturedImage::new(size.width, size.height, pixels)
}

/// What [`CapturedImage::assert_shape_with`] checks, as the test utilities' `assertShape`
/// parameters: the shape's bounding box `shape_size` centered at `shape_center`, the
/// background's likewise, and the gap left unchecked along their edges (1 dp by default).
#[derive(Clone, Debug)]
pub struct ShapeCheck {
    pub density: Density,
    pub shape: Shape,
    pub shape_color: Color,
    pub shape_size: Size,
    pub shape_center: Offset,
    pub background_shape: Shape,
    /// `None`: only the shape's pixels are checked.
    pub background_color: Option<Color>,
    pub background_size: Size,
    pub background_center: Offset,
    pub anti_aliasing_gap: f32,
}

impl ShapeCheck {
    /// `shape` and a rectangular background both filling `image`, with a 1 dp gap.
    pub fn new(
        density: Density,
        shape: Shape,
        shape_color: Color,
        background_color: Option<Color>,
        image: &CapturedImage,
    ) -> ShapeCheck {
        let size = Size::new(image.width as f32, image.height as f32);
        let center = Offset::new(size.width / 2.0, size.height / 2.0);
        ShapeCheck {
            density,
            shape,
            shape_color,
            shape_size: size,
            shape_center: center,
            background_shape: Shape::Rectangle,
            background_color,
            background_size: size,
            background_center: center,
            anti_aliasing_gap: density.to_px(Dp(1.0)),
        }
    }
}
