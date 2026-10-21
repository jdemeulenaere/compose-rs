//! The display list as bytes for the Kotlin replayer (docs/07-platforms.md, "Drawing without
//! shipping Skia"): one buffer per frame, read on the Kotlin side with a little-endian
//! `ByteBuffer`, each operation a tag byte and its operands.
//!
//! Paths and images are resources: defined once (a `DefinePath` or `DefineImage` before their
//! first use) and dropped when the last frame using them is gone, so a frame names them by id.
//! Their definitions accumulate until the Kotlin side takes them with the frame it draws, so a
//! frame drawn twice or skipped loses none.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use compose_ui::{
    Color, DisplayList, DrawOp, ImageBitmap, Offset, Outline, Paint, PaintStyle, Path, PathCommand,
    PathFillType, Rect, StrokeCap,
};

/// The operations' tags, as `Replayer.kt` reads them.
pub mod tag {
    pub const SAVE: u8 = 0;
    pub const RESTORE: u8 = 1;
    pub const TRANSLATE: u8 = 2;
    pub const CLIP_RECT: u8 = 3;
    pub const CLIP_ROUND_RECT: u8 = 4;
    pub const DRAW_RECT: u8 = 5;
    pub const DRAW_ROUND_RECT: u8 = 6;
    pub const DRAW_LINE: u8 = 7;
    pub const DRAW_PARAGRAPH: u8 = 8;
    pub const SAVE_LAYER_ALPHA: u8 = 9;
    pub const TRANSFORM: u8 = 10;
    pub const DRAW_IMAGE: u8 = 11;
    pub const DRAW_SHADOW: u8 = 12;
    pub const DRAW_ARC: u8 = 13;
    pub const DRAW_PATH: u8 = 14;
    pub const DEFINE_PATH: u8 = 20;
    pub const DROP_PATH: u8 = 21;
    pub const DEFINE_IMAGE: u8 = 22;
    pub const DROP_IMAGE: u8 = 23;
}

/// A path command's kind, as `Replayer.kt` reads it.
mod command {
    pub const MOVE_TO: u8 = 0;
    pub const LINE_TO: u8 = 1;
    pub const QUAD_TO: u8 = 2;
    pub const CUBIC_TO: u8 = 3;
    pub const CLOSE: u8 = 4;
}

/// Little-endian writes into a byte buffer.
#[derive(Default)]
pub struct Writer {
    pub bytes: Vec<u8>,
}

impl Writer {
    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    pub fn f32(&mut self, value: f32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    fn offset(&mut self, offset: Offset) {
        self.f32(offset.x);
        self.f32(offset.y);
    }
    fn rect(&mut self, rect: &Rect) {
        self.f32(rect.left);
        self.f32(rect.top);
        self.f32(rect.right);
        self.f32(rect.bottom);
    }
    fn radii(&mut self, radii: &[f32; 4]) {
        for radius in radii {
            self.f32(*radius);
        }
    }
    fn color(&mut self, color: Color) {
        self.u32(color.argb());
    }
    /// Color, style (0 fill, 1 stroke), stroke width, cap (0 butt, 1 round, 2 square).
    fn paint(&mut self, paint: &Paint) {
        self.color(paint.color);
        match paint.style {
            PaintStyle::Fill => {
                self.u8(0);
                self.f32(0.0);
            }
            PaintStyle::Stroke(width) => {
                self.u8(1);
                self.f32(width);
            }
        }
        self.u8(match paint.cap {
            StrokeCap::Butt => 0,
            StrokeCap::Round => 1,
            StrokeCap::Square => 2,
        });
    }
    /// Kind (0 rectangle, 1 rounded), bounds, radii.
    fn outline(&mut self, outline: &Outline) {
        match outline {
            Outline::Rectangle(rect) => {
                self.u8(0);
                self.rect(rect);
                self.radii(&[0.0; 4]);
            }
            Outline::Rounded(rect, radii) => {
                self.u8(1);
                self.rect(rect);
                self.radii(radii);
            }
        }
    }
}

/// The paths and images the Kotlin side holds, by id.
#[derive(Default)]
pub struct Resources {
    paths: HashMap<usize, (Weak<Path>, u32)>,
    images: HashMap<usize, (Weak<[u8]>, u32)>,
    next_id: u32,
    /// Definitions and drops the Kotlin side hasn't taken yet.
    pub pending: Writer,
}

impl Resources {
    fn next(&mut self) -> u32 {
        self.next_id = self.next_id.wrapping_add(1);
        self.next_id
    }

    /// The id of `path`, defined (in `pending`) if new.
    fn path_id(&mut self, path: &Arc<Path>) -> u32 {
        let key = Arc::as_ptr(path) as usize;
        if let Some((weak, id)) = self.paths.get(&key)
            && weak.upgrade().is_some_and(|alive| Arc::ptr_eq(&alive, path))
        {
            return *id;
        }
        let id = self.next();
        if let Some((_, old)) = self.paths.insert(key, (Arc::downgrade(path), id)) {
            self.pending.u8(tag::DROP_PATH);
            self.pending.u32(old);
        }
        let writer = &mut self.pending;
        writer.u8(tag::DEFINE_PATH);
        writer.u32(id);
        writer.u8(match path.fill_type {
            PathFillType::NonZero => 0,
            PathFillType::EvenOdd => 1,
        });
        writer.u32(path.commands().len() as u32);
        for command in path.commands() {
            match command {
                PathCommand::MoveTo(point) => {
                    writer.u8(command::MOVE_TO);
                    writer.offset(*point);
                }
                PathCommand::LineTo(point) => {
                    writer.u8(command::LINE_TO);
                    writer.offset(*point);
                }
                PathCommand::QuadTo(control, end) => {
                    writer.u8(command::QUAD_TO);
                    writer.offset(*control);
                    writer.offset(*end);
                }
                PathCommand::CubicTo(first, second, end) => {
                    writer.u8(command::CUBIC_TO);
                    writer.offset(*first);
                    writer.offset(*second);
                    writer.offset(*end);
                }
                PathCommand::Close => writer.u8(command::CLOSE),
            }
        }
        id
    }

    /// The id of `image`, defined (with its pixels) if new.
    fn image_id(&mut self, image: &ImageBitmap) -> u32 {
        let key = image.id();
        if let Some((weak, id)) = self.images.get(&key)
            && weak.strong_count() > 0
        {
            return *id;
        }
        let id = self.next();
        if let Some((_, old)) = self.images.insert(key, (image.downgrade(), id)) {
            self.pending.u8(tag::DROP_IMAGE);
            self.pending.u32(old);
        }
        let writer = &mut self.pending;
        writer.u8(tag::DEFINE_IMAGE);
        writer.u32(id);
        writer.u32(image.width());
        writer.u32(image.height());
        writer.u32(image.pixels().len() as u32);
        writer.bytes.extend_from_slice(image.pixels());
        id
    }

    /// Drops the resources no frame can use any more (their owners are gone).
    pub fn collect(&mut self) {
        let pending = &mut self.pending;
        self.paths.retain(|_, (weak, id)| {
            let alive = weak.strong_count() > 0;
            if !alive {
                pending.u8(tag::DROP_PATH);
                pending.u32(*id);
            }
            alive
        });
        self.images.retain(|_, (weak, id)| {
            let alive = weak.strong_count() > 0;
            if !alive {
                pending.u8(tag::DROP_IMAGE);
                pending.u32(*id);
            }
            alive
        });
    }
}

/// Writes `list` into `out` (cleared first), defining in `resources` the paths and images it
/// uses.
pub fn write_display_list(list: &DisplayList, resources: &mut Resources, out: &mut Writer) {
    out.bytes.clear();
    for op in &list.ops {
        match op {
            DrawOp::Save => out.u8(tag::SAVE),
            DrawOp::Restore => out.u8(tag::RESTORE),
            DrawOp::Translate(offset) => {
                out.u8(tag::TRANSLATE);
                out.offset(*offset);
            }
            DrawOp::ClipRect(rect) => {
                out.u8(tag::CLIP_RECT);
                out.rect(rect);
            }
            DrawOp::ClipRoundRect(rect, radii) => {
                out.u8(tag::CLIP_ROUND_RECT);
                out.rect(rect);
                out.radii(radii);
            }
            DrawOp::DrawRect(rect, paint) => {
                out.u8(tag::DRAW_RECT);
                out.rect(rect);
                out.paint(paint);
            }
            DrawOp::DrawRoundRect(rect, radii, paint) => {
                out.u8(tag::DRAW_ROUND_RECT);
                out.rect(rect);
                out.radii(radii);
                out.paint(paint);
            }
            DrawOp::DrawLine(from, to, paint) => {
                out.u8(tag::DRAW_LINE);
                out.offset(*from);
                out.offset(*to);
                out.paint(paint);
            }
            DrawOp::DrawParagraph(handle, offset) => {
                out.u8(tag::DRAW_PARAGRAPH);
                out.u32(handle.0 as u32);
                out.offset(*offset);
            }
            DrawOp::SaveLayerAlpha(alpha) => {
                out.u8(tag::SAVE_LAYER_ALPHA);
                out.f32(*alpha);
            }
            DrawOp::Transform { pivot, scale, rotation, translation } => {
                out.u8(tag::TRANSFORM);
                out.offset(*pivot);
                out.f32(scale.0);
                out.f32(scale.1);
                out.f32(*rotation);
                out.offset(*translation);
            }
            DrawOp::DrawImage { image, src, dst, alpha } => {
                let id = resources.image_id(image);
                out.u8(tag::DRAW_IMAGE);
                out.u32(id);
                out.rect(src);
                out.rect(dst);
                out.f32(*alpha);
            }
            DrawOp::DrawShadow { outline, elevation, light, light_radius, ambient, spot } => {
                out.u8(tag::DRAW_SHADOW);
                out.outline(outline);
                out.f32(*elevation);
                out.f32(light.0);
                out.f32(light.1);
                out.f32(light.2);
                out.f32(*light_radius);
                out.color(*ambient);
                out.color(*spot);
            }
            DrawOp::DrawArc { oval, start_angle, sweep_angle, use_center, paint } => {
                out.u8(tag::DRAW_ARC);
                out.rect(oval);
                out.f32(*start_angle);
                out.f32(*sweep_angle);
                out.u8(u8::from(*use_center));
                out.paint(paint);
            }
            DrawOp::DrawPath { path, offset, scale, paint } => {
                let id = resources.path_id(path);
                out.u8(tag::DRAW_PATH);
                out.u32(id);
                out.offset(*offset);
                out.f32(scale.0);
                out.f32(scale.1);
                out.paint(paint);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn a_rect_is_its_tag_bounds_and_paint() {
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawRect(Rect::new(1.0, 2.0, 3.0, 4.0), Paint::fill(Color::BLACK)));
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut out);
        // Tag, 4 floats, color, style, width, cap.
        assert_eq!(out.bytes.len(), 1 + 16 + 4 + 1 + 4 + 1);
        assert_eq!(out.bytes[0], tag::DRAW_RECT);
        assert_eq!(f32::from_le_bytes(out.bytes[13..17].try_into().unwrap()), 4.0);
        assert_eq!(read_u32(&out.bytes, 17), 0xFF00_0000);
    }

    #[test]
    fn a_path_is_defined_once_and_dropped_with_its_owner() {
        let path = Arc::new(Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap());
        let mut list = DisplayList::default();
        let op = DrawOp::DrawPath {
            path: path.clone(),
            offset: Offset::ZERO,
            scale: (1.0, 1.0),
            paint: Paint::fill(Color::BLACK),
        };
        list.push(op.clone());
        list.push(op);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut out);
        let defined = resources.pending.bytes.iter().filter(|b| **b == tag::DEFINE_PATH).count();
        assert!(defined >= 1);
        assert_eq!(resources.pending.bytes[0], tag::DEFINE_PATH);
        resources.pending.bytes.clear();
        // Drawn again: nothing new to define.
        write_display_list(&list, &mut resources, &mut out);
        assert!(resources.pending.bytes.is_empty());
        drop(list);
        drop(path);
        resources.collect();
        assert_eq!(resources.pending.bytes[0], tag::DROP_PATH);
    }
}
