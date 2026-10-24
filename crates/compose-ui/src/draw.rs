//! The draw scope modifier nodes record into (docs/06-ui.md, "Drawing").

use std::sync::Arc;

use crate::display_list::{DisplayList, DrawOp};
use crate::graphics::{Color, Outline, Paint, Shape};
use crate::path::{Path, PathData};
use crate::text::ParagraphHandle;
use crate::unit::{Density, Offset, Rect, Size};

/// Whether `paint` draws nothing: a transparent color and no shader (Material's transparent
/// containers). Its drawing isn't recorded, as Skia drops it (`SkPaint::nothingToDraw`): a
/// frame doesn't copy, send and replay it.
fn draws_nothing(paint: &Paint) -> bool {
    paint.color.alpha() == 0 && paint.shader.is_none()
}

/// What a draw node draws with, as `DrawScope`: its size, in pixels, and the display list.
/// Coordinates are the node's own: (0, 0) is its top left.
pub struct DrawScope<'a> {
    pub(crate) list: &'a mut DisplayList,
    pub(crate) origin: Offset,
    pub(crate) size: Size,
    pub(crate) density: Density,
    pub(crate) layout_direction: crate::alignment::LayoutDirection,
}

impl DrawScope<'_> {
    pub fn size(&self) -> Size {
        self.size
    }

    pub fn density(&self) -> Density {
        self.density
    }

    /// The node's layout direction, as `DrawScope.layoutDirection`: what's drawn from the start
    /// (a slider's track, a switch's thumb) is mirrored in right-to-left.
    pub fn layout_direction(&self) -> crate::alignment::LayoutDirection {
        self.layout_direction
    }

    fn to_list(&self, rect: Rect) -> Rect {
        Rect::new(
            rect.left + self.origin.x,
            rect.top + self.origin.y,
            rect.right + self.origin.x,
            rect.bottom + self.origin.y,
        )
    }

    /// The whole node.
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.size.width, self.size.height)
    }

    pub fn draw_rect(&mut self, rect: Rect, paint: Paint) {
        if draws_nothing(&paint) {
            return;
        }
        let rect = self.to_list(rect);
        self.list.push(DrawOp::DrawRect(rect, paint));
    }

    pub fn draw_round_rect(&mut self, rect: Rect, radii: [f32; 4], paint: Paint) {
        if draws_nothing(&paint) {
            return;
        }
        let rect = self.to_list(rect);
        self.list.push(DrawOp::DrawRoundRect(rect, radii, paint));
    }

    /// Fills `shape` over the whole node.
    pub fn draw_shape(&mut self, shape: &Shape, color: Color) {
        let outline = shape.outline_in(self.size, &self.density, self.layout_direction());
        self.draw_outline(outline, Paint::fill(color));
    }

    pub fn draw_outline(&mut self, outline: Outline, paint: Paint) {
        match outline {
            Outline::Rectangle(rect) => self.draw_rect(rect, paint),
            Outline::Rounded(rect, radii) => self.draw_round_rect(rect, radii, paint),
        }
    }

    /// Draws an arc of the oval in `oval` from `start_angle` degrees (0 at 3 o'clock,
    /// clockwise) over `sweep_angle`, as `drawArc(useCenter = false)`.
    pub fn draw_arc(&mut self, oval: Rect, start_angle: f32, sweep_angle: f32, paint: Paint) {
        self.draw_arc_with(oval, start_angle, sweep_angle, false, paint);
    }

    /// `draw_arc`, closed through the oval's center if `use_center` (a pie slice), as
    /// `drawArc(useCenter)`.
    pub fn draw_arc_with(
        &mut self,
        oval: Rect,
        start_angle: f32,
        sweep_angle: f32,
        use_center: bool,
        paint: Paint,
    ) {
        if draws_nothing(&paint) {
            return;
        }
        let oval = self.to_list(oval);
        self.list.push(DrawOp::DrawArc { oval, start_angle, sweep_angle, use_center, paint });
    }

    /// Draws the circle of `radius` around `center`, as `drawCircle` (Compose's defaults: half
    /// the smaller side, around `self.center()`).
    pub fn draw_circle(&mut self, center: Offset, radius: f32, paint: Paint) {
        let oval =
            Rect::new(center.x - radius, center.y - radius, center.x + radius, center.y + radius);
        self.draw_oval(oval, paint);
    }

    /// Draws the oval inscribed in `oval`, as `drawOval` (a whole turn of an arc).
    pub fn draw_oval(&mut self, oval: Rect, paint: Paint) {
        self.draw_arc_with(oval, 0.0, 360.0, false, paint);
    }

    /// The center of the drawing, as `DrawScope.center`.
    pub fn center(&self) -> Offset {
        Offset::new(self.size.width / 2.0, self.size.height / 2.0)
    }

    /// A paint for `brush` over this drawing (its gradient positions in this scope's
    /// coordinates, defaulting to its size), at `alpha`, `style`d: what the draw calls take
    /// for a brush, as Compose's `drawRect(brush, …)` and friends. A gradient is kept by this
    /// display list (`DisplayList::shaders`): the paint is for this scope's drawing only.
    pub fn brush_paint(
        &mut self,
        brush: &crate::brush::Brush,
        alpha: f32,
        style: crate::graphics::PaintStyle,
    ) -> Paint {
        let mut paint = Paint::fill(brush.color(alpha));
        paint.style = style;
        if let Some(shader) = brush.shader(self.size, self.origin, alpha) {
            paint.shader = Some(self.list.push_shader(shader));
        }
        paint
    }

    // ---- Scoped transforms and clips, as Compose's `DrawScope` extensions -----------------

    /// Draws `block` rotated by `degrees` (clockwise) around `pivot`, as `rotate(degrees,
    /// pivot) { … }`.
    pub fn rotate(&mut self, degrees: f32, pivot: Offset, block: impl FnOnce(&mut DrawScope<'_>)) {
        self.save();
        self.transform(pivot, (1.0, 1.0), degrees, Offset::ZERO);
        block(self);
        self.restore();
    }

    /// Draws `block` scaled by `scale_x`, `scale_y` around `pivot`, as `scale(scaleX, scaleY,
    /// pivot) { … }`.
    pub fn scale(
        &mut self,
        scale_x: f32,
        scale_y: f32,
        pivot: Offset,
        block: impl FnOnce(&mut DrawScope<'_>),
    ) {
        self.save();
        self.transform(pivot, (scale_x, scale_y), 0.0, Offset::ZERO);
        block(self);
        self.restore();
    }

    /// Draws `block` moved by `left`, `top`, as `translate(left, top) { … }`.
    pub fn translate(&mut self, left: f32, top: f32, block: impl FnOnce(&mut DrawScope<'_>)) {
        let origin = self.origin;
        self.origin = origin + Offset::new(left, top);
        block(self);
        self.origin = origin;
    }

    /// Draws `block` in this drawing less the insets (its size too), as `inset(left, top,
    /// right, bottom) { … }`.
    pub fn inset(
        &mut self,
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        block: impl FnOnce(&mut DrawScope<'_>),
    ) {
        let (origin, size) = (self.origin, self.size);
        self.origin = origin + Offset::new(left, top);
        self.size =
            Size::new((size.width - left - right).max(0.0), (size.height - top - bottom).max(0.0));
        block(self);
        self.origin = origin;
        self.size = size;
    }

    /// Draws `block` clipped to `rect`, as `clipRect(left, top, right, bottom) { … }`.
    pub fn clip_rect(&mut self, rect: Rect, block: impl FnOnce(&mut DrawScope<'_>)) {
        self.save();
        self.clip_outline(Outline::Rectangle(rect));
        block(self);
        self.restore();
    }

    /// Draws `block` clipped to `path`, as `clipPath(path) { … }`.
    pub fn clip_path(&mut self, path: impl SharedPath, block: impl FnOnce(&mut DrawScope<'_>)) {
        self.save();
        let offset = self.origin;
        self.list.push(DrawOp::ClipPath { path: path.path_data(), offset });
        block(self);
        self.restore();
    }

    pub fn draw_line(&mut self, from: Offset, to: Offset, paint: Paint) {
        if draws_nothing(&paint) {
            return;
        }
        let (from, to) = (from + self.origin, to + self.origin);
        self.list.push(DrawOp::DrawLine(from, to, paint));
    }

    pub fn draw_paragraph(&mut self, paragraph: ParagraphHandle, position: Offset) {
        let position = position + self.origin;
        self.list.push(DrawOp::DrawParagraph(paragraph, position));
    }

    /// Saves the clip, to restore with [`restore`](DrawScope::restore).
    pub fn save(&mut self) {
        self.list.push(DrawOp::Save);
    }

    pub fn restore(&mut self) {
        self.list.push(DrawOp::Restore);
    }

    /// Draws the part `src` of `image` (in its pixels) into `dst` (in the node's coordinates), at
    /// `alpha`.
    pub fn draw_image(
        &mut self,
        image: &crate::image::ImageBitmap,
        src: Rect,
        dst: Rect,
        alpha: f32,
    ) {
        let dst = self.to_list(dst);
        self.list.push(DrawOp::DrawImage { image: image.clone(), src, dst, alpha });
    }

    /// Draws the shadows of `outline` (in the node's coordinates) raised by `elevation` pixels,
    /// in `ambient_color` and `spot_color` (their alphas set to Compose's), as a graphics
    /// layer's `shadowElevation` does: from a light 300 dp above the window's top, 600 dp up,
    /// 800 dp wide, as Compose Desktop's.
    pub fn draw_shadow(
        &mut self,
        outline: Outline,
        elevation: f32,
        ambient_color: Color,
        spot_color: Color,
    ) {
        if elevation <= 0.0 {
            return;
        }
        let outline = match outline {
            Outline::Rectangle(rect) => Outline::Rectangle(self.to_list(rect)),
            Outline::Rounded(rect, radii) => Outline::Rounded(self.to_list(rect), radii),
        };
        let density = self.density;
        let px = |dp: f32| density.to_px(crate::unit::Dp(dp));
        self.list.push(DrawOp::DrawShadow {
            outline,
            elevation,
            light: (0.0, -px(300.0), px(600.0)),
            light_radius: px(800.0),
            ambient: ambient_color.with_alpha(0.039),
            spot: spot_color.with_alpha(0.19),
        });
    }

    /// Draws `path` (in the node's coordinates), as `drawPath`: a `&Path`, its contents as they
    /// are now (shared, nothing copied), or a `&Arc<PathData>`, contents that never change (an
    /// `ImageVector`'s).
    pub fn draw_path(&mut self, path: impl SharedPath, paint: Paint) {
        self.draw_path_scaled(path, Offset::ZERO, (1.0, 1.0), paint);
    }

    /// Draws `path` scaled by `scale`, then moved by `offset` (in the node's coordinates): a
    /// vector image's viewport fitted to its size.
    pub fn draw_path_scaled(
        &mut self,
        path: impl SharedPath,
        offset: Offset,
        scale: (f32, f32),
        paint: Paint,
    ) {
        if draws_nothing(&paint) {
            return;
        }
        let offset = offset + self.origin;
        self.list.push(DrawOp::DrawPath { path: path.path_data(), offset, scale, paint });
    }

    /// Saves, and draws what follows into a layer composited with `alpha` at the next restore.
    pub fn save_layer_alpha(&mut self, alpha: f32) {
        self.list.push(DrawOp::SaveLayerAlpha(alpha));
    }

    /// Transforms what follows until the restore of a save made before: scaled and rotated
    /// (degrees, clockwise) around `pivot` (in the node's coordinates), then moved by
    /// `translation`.
    pub fn transform(
        &mut self,
        pivot: Offset,
        scale: (f32, f32),
        rotation: f32,
        translation: Offset,
    ) {
        let pivot = pivot + self.origin;
        self.list.push(DrawOp::Transform { pivot, scale, rotation, translation });
    }

    pub fn clip_outline(&mut self, outline: Outline) {
        match outline {
            Outline::Rectangle(rect) => {
                let rect = self.to_list(rect);
                self.list.push(DrawOp::ClipRect(rect));
            }
            Outline::Rounded(rect, radii) => {
                let rect = self.to_list(rect);
                self.list.push(DrawOp::ClipRoundRect(rect, radii));
            }
        }
    }
}

/// A path the display list can hold: a `&Path`, whose contents now are recorded (shared, nothing
/// copied: a recording keeps them, whatever the path does next, D531), or a `&Arc<PathData>`,
/// contents that never change (an `ImageVector`'s).
pub trait SharedPath {
    /// The contents drawn.
    fn path_data(self) -> Arc<PathData>;
}

impl SharedPath for &Arc<PathData> {
    fn path_data(self) -> Arc<PathData> {
        Arc::clone(self)
    }
}

impl SharedPath for &Path {
    fn path_data(self) -> Arc<PathData> {
        self.data()
    }
}
