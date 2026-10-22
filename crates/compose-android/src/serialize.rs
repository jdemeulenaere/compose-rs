//! The display list as bytes for the Kotlin replayer (docs/07-platforms.md, "Drawing without
//! shipping Skia"): one buffer per frame, read on the Kotlin side with a little-endian
//! `ByteBuffer`, each operation a tag byte and its operands.
//!
//! Paths and images are resources: defined once (a `DefinePath` or `DefineImage` before their
//! first use) and dropped when the last frame using them is gone, so a frame names them by id.
//! Their definitions wait until the Kotlin side takes them with the frame it draws, so a frame
//! drawn twice or skipped loses none; one dropped before it was taken is cancelled.

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
    /// A repaint boundary's operations: its key (u64), their length in bytes (u32), then
    /// them. The replayer keeps their recording while they stay the same.
    pub const GROUP: u8 = 15;
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

/// A resource change the Kotlin side hasn't taken yet.
enum Pending {
    Path { id: u32, path: Weak<Path> },
    Image { id: u32, width: u32, height: u32, pixels: Weak<[u8]> },
    DropPath(u32),
    DropImage(u32),
}

/// The paths and images the Kotlin side holds, by id.
#[derive(Default)]
pub struct Resources {
    paths: HashMap<usize, (Weak<Path>, u32)>,
    images: HashMap<usize, (Weak<[u8]>, u32)>,
    next_id: u32,
    /// Definitions and drops the Kotlin side hasn't taken yet: a definition dropped before it
    /// was taken is cancelled, so this stays bounded however many frames go undrawn.
    pending: Vec<Pending>,
}

impl Resources {
    fn next(&mut self) -> u32 {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }

    /// Drops path `id`: its definition, if still pending, or the Kotlin side's.
    fn drop_path(&mut self, id: u32) {
        let before = self.pending.len();
        self.pending
            .retain(|op| !matches!(op, Pending::Path { id: defined, .. } if *defined == id));
        if self.pending.len() == before {
            self.pending.push(Pending::DropPath(id));
        }
    }

    fn drop_image(&mut self, id: u32) {
        let before = self.pending.len();
        self.pending
            .retain(|op| !matches!(op, Pending::Image { id: defined, .. } if *defined == id));
        if self.pending.len() == before {
            self.pending.push(Pending::DropImage(id));
        }
    }

    /// The id of `path`, defined if new.
    fn path_id(&mut self, path: &Arc<Path>) -> u32 {
        let key = Arc::as_ptr(path) as usize;
        if let Some((weak, id)) = self.paths.get(&key)
            && weak.upgrade().is_some_and(|alive| Arc::ptr_eq(&alive, path))
        {
            return *id;
        }
        let id = self.next();
        if let Some((_, old)) = self.paths.insert(key, (Arc::downgrade(path), id)) {
            self.drop_path(old);
        }
        self.pending.push(Pending::Path { id, path: Arc::downgrade(path) });
        id
    }

    /// The id of `image`, defined if new; 0 (nothing) for an empty one.
    fn image_id(&mut self, image: &ImageBitmap) -> u32 {
        if image.width() == 0 || image.height() == 0 {
            return 0;
        }
        let key = image.id();
        if let Some((weak, id)) = self.images.get(&key)
            && weak.strong_count() > 0
        {
            return *id;
        }
        let id = self.next();
        if let Some((_, old)) = self.images.insert(key, (image.downgrade(), id)) {
            self.drop_image(old);
        }
        self.pending.push(Pending::Image {
            id,
            width: image.width(),
            height: image.height(),
            pixels: image.downgrade(),
        });
        id
    }

    /// Drops the resources no frame can use any more (their owners are gone).
    pub fn collect(&mut self) {
        let dead_paths: Vec<u32> = self
            .paths
            .values()
            .filter(|(weak, _)| weak.strong_count() == 0)
            .map(|(_, id)| *id)
            .collect();
        self.paths.retain(|_, (weak, _)| weak.strong_count() > 0);
        for id in dead_paths {
            self.drop_path(id);
        }
        let dead_images: Vec<u32> = self
            .images
            .values()
            .filter(|(weak, _)| weak.strong_count() == 0)
            .map(|(_, id)| *id)
            .collect();
        self.images.retain(|_, (weak, _)| weak.strong_count() > 0);
        for id in dead_images {
            self.drop_image(id);
        }
    }

    /// Whether changes wait for the Kotlin side.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Appends the pending changes to `out` (a definition whose owner is already gone is
    /// skipped), and forgets them: the Kotlin side has them.
    pub fn write_pending(&mut self, out: &mut Writer) {
        for op in self.pending.drain(..) {
            match op {
                Pending::Path { id, path } => {
                    let Some(path) = path.upgrade() else { continue };
                    write_path(out, id, &path);
                }
                Pending::Image { id, width, height, pixels } => {
                    let Some(pixels) = pixels.upgrade() else { continue };
                    out.u8(tag::DEFINE_IMAGE);
                    out.u32(id);
                    out.u32(width);
                    out.u32(height);
                    out.u32(pixels.len() as u32);
                    out.bytes.extend_from_slice(&pixels);
                }
                Pending::DropPath(id) => {
                    out.u8(tag::DROP_PATH);
                    out.u32(id);
                }
                Pending::DropImage(id) => {
                    out.u8(tag::DROP_IMAGE);
                    out.u32(id);
                }
            }
        }
    }
}

fn write_path(writer: &mut Writer, id: u32, path: &Path) {
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
}

/// Whether the layer opening at `start` (a `SaveLayerAlpha`) holds one drawing with a paint,
/// among saves, clips and moves: its alpha can go into that paint, without an offscreen layer
/// (a GPU pass each frame on Android). Returns the index of its closing `Restore` if so.
fn foldable_layer(ops: &[DrawOp], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut drawings = 0;
    for (index, op) in ops.iter().enumerate().skip(start + 1) {
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha(_) => {
                if matches!(op, DrawOp::SaveLayerAlpha(_)) {
                    return None;
                }
                depth += 1;
            }
            DrawOp::Restore if depth == 0 => return (drawings == 1).then_some(index),
            DrawOp::Restore => depth -= 1,
            DrawOp::Translate(_)
            | DrawOp::ClipRect(_)
            | DrawOp::ClipRoundRect(..)
            | DrawOp::Transform { .. } => {}
            DrawOp::DrawRect(..)
            | DrawOp::DrawRoundRect(..)
            | DrawOp::DrawLine(..)
            | DrawOp::DrawArc { .. }
            | DrawOp::DrawPath { .. } => drawings += 1,
            // Text, images and shadows draw what a paint's alpha doesn't reach.
            DrawOp::DrawParagraph(..) | DrawOp::DrawImage { .. } | DrawOp::DrawShadow { .. } => {
                return None;
            }
        }
    }
    None
}

/// `paint` with its color's alpha multiplied by `alpha`.
fn faded(paint: &Paint, alpha: f32) -> Paint {
    if alpha >= 1.0 {
        return *paint;
    }
    let argb = paint.color.argb();
    let faded = ((argb >> 24) as f32 * alpha.clamp(0.0, 1.0)).round() as u32;
    Paint { color: Color::from_argb((faded << 24) | (argb & 0x00FF_FFFF)), ..*paint }
}

/// Writes `list` into `out` (cleared first), defining in `resources` the paths and images it
/// uses.
pub fn write_display_list(list: &DisplayList, resources: &mut Resources, out: &mut Writer) {
    out.bytes.clear();
    // Layers folded into their one drawing's paint: where each ends, and its alpha.
    let mut folds: Vec<(usize, f32)> = Vec::new();
    // The groups, outer before inner (they nest), and the open ones: where each ends and
    // where its length goes.
    // Of two with the same range, the outer one was drawn last.
    let mut groups: Vec<_> =
        list.groups.iter().enumerate().filter(|(_, group)| group.start < group.end).collect();
    groups.sort_by_key(|&(index, group)| {
        (group.start, std::cmp::Reverse(group.end), std::cmp::Reverse(index))
    });
    let mut groups = groups.into_iter().map(|(_, group)| group).peekable();
    let mut open: Vec<(usize, usize)> = Vec::new();
    for (index, op) in list.ops.iter().enumerate() {
        while folds.last().is_some_and(|(end, _)| *end < index) {
            folds.pop();
        }
        close_groups(&mut open, index, out);
        while let Some(group) = groups.next_if(|group| group.start == index) {
            out.u8(tag::GROUP);
            out.u32(group.key as u32);
            out.u32((group.key >> 32) as u32);
            open.push((group.end, out.bytes.len()));
            out.u32(0);
        }
        let alpha: f32 = folds.iter().map(|(_, alpha)| alpha).product();
        let faded_paint = |paint: &Paint| faded(paint, alpha);
        if let DrawOp::SaveLayerAlpha(layer_alpha) = op
            && let Some(end) = foldable_layer(&list.ops, index)
        {
            folds.push((end, *layer_alpha));
            out.u8(tag::SAVE);
            continue;
        }
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
                out.paint(&faded_paint(paint));
            }
            DrawOp::DrawRoundRect(rect, radii, paint) => {
                out.u8(tag::DRAW_ROUND_RECT);
                out.rect(rect);
                out.radii(radii);
                out.paint(&faded_paint(paint));
            }
            DrawOp::DrawLine(from, to, paint) => {
                out.u8(tag::DRAW_LINE);
                out.offset(*from);
                out.offset(*to);
                out.paint(&faded_paint(paint));
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
                out.paint(&faded_paint(paint));
            }
            DrawOp::DrawPath { path, offset, scale, paint } => {
                let id = resources.path_id(path);
                out.u8(tag::DRAW_PATH);
                out.u32(id);
                out.offset(*offset);
                out.f32(scale.0);
                out.f32(scale.1);
                out.paint(&faded_paint(paint));
            }
        }
    }
    close_groups(&mut open, usize::MAX, out);
}

/// Closes the open groups ending at or before `index`, writing their lengths.
fn close_groups(open: &mut Vec<(usize, usize)>, index: usize, out: &mut Writer) {
    while let Some(&(end, at)) = open.last() {
        if end > index {
            break;
        }
        open.pop();
        let length = (out.bytes.len() - at - 4) as u32;
        out.bytes[at..at + 4].copy_from_slice(&length.to_le_bytes());
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
    fn a_layer_with_one_drawing_fades_its_paint_instead() {
        let mut list = DisplayList::default();
        list.push(DrawOp::SaveLayerAlpha(0.5));
        list.push(DrawOp::ClipRect(Rect::new(0.0, 0.0, 4.0, 4.0)));
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        list.push(DrawOp::Restore);
        // After it, the paint is whole again.
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut out);
        assert_eq!(out.bytes[0], tag::SAVE);
        // Save, clip (17 bytes), then the rect: its color at 1 + 17 + 1 + 16.
        assert_eq!(read_u32(&out.bytes, 35), 0x8000_0000);
        let last = out.bytes.len() - 27;
        assert_eq!(out.bytes[last], tag::DRAW_RECT);
        assert_eq!(read_u32(&out.bytes, last + 17), 0xFF00_0000);
        // Two drawings: a layer, as their overlap must fade together.
        list.ops
            .insert(3, DrawOp::DrawRect(Rect::new(1.0, 1.0, 2.0, 2.0), Paint::fill(Color::WHITE)));
        write_display_list(&list, &mut resources, &mut out);
        assert_eq!(out.bytes[0], tag::SAVE_LAYER_ALPHA);
    }

    #[test]
    fn groups_nest_and_carry_their_length() {
        let rect = || DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLACK));
        let mut list = DisplayList::default();
        for _ in 0..3 {
            list.push(rect());
        }
        // The outer group holds ops 0..3, the inner 1..2; an empty one is left out. They come
        // inner first, as drawing closes them.
        list.groups.push(compose_ui::DrawGroup { key: 2, start: 1, end: 2 });
        list.groups.push(compose_ui::DrawGroup { key: 3, start: 2, end: 2 });
        list.groups.push(compose_ui::DrawGroup { key: 1 << 40 | 1, start: 0, end: 3 });
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut out);
        let rect_bytes = 1 + 16 + 4 + 1 + 4 + 1;
        let header = 1 + 8 + 4;
        let bytes = &out.bytes;
        assert_eq!(bytes.len(), 2 * header + 3 * rect_bytes);
        assert_eq!(bytes[0], tag::GROUP);
        assert_eq!((read_u32(bytes, 1), read_u32(bytes, 5)), (1, 1 << 8));
        assert_eq!(read_u32(bytes, 9) as usize, header + 3 * rect_bytes);
        let inner = header + rect_bytes;
        assert_eq!(bytes[inner], tag::GROUP);
        assert_eq!(read_u32(bytes, inner + 1), 2);
        assert_eq!(read_u32(bytes, inner + 9) as usize, rect_bytes);
        assert_eq!(bytes[inner + header], tag::DRAW_RECT);
    }

    /// A boundary holding only a boundary: the same range, the outer one (drawn last) first.
    #[test]
    fn of_two_groups_with_the_same_range_the_outer_opens_first() {
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLACK)));
        list.groups.push(compose_ui::DrawGroup { key: 1, start: 0, end: 1 });
        list.groups.push(compose_ui::DrawGroup { key: 2, start: 0, end: 1 });
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut out);
        assert_eq!((out.bytes[0], read_u32(&out.bytes, 1)), (tag::GROUP, 2));
        assert_eq!((out.bytes[13], read_u32(&out.bytes, 14)), (tag::GROUP, 1));
    }

    /// A folded layer's `Save` opens the group as any operation does.
    #[test]
    fn a_group_can_start_with_a_folded_layer() {
        let mut list = DisplayList::default();
        list.push(DrawOp::SaveLayerAlpha(0.5));
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        list.push(DrawOp::Restore);
        list.groups.push(compose_ui::DrawGroup { key: 7, start: 0, end: 3 });
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut out);
        assert_eq!(out.bytes[0], tag::GROUP);
        assert_eq!(read_u32(&out.bytes, 9) as usize, out.bytes.len() - 13);
        assert_eq!(out.bytes[13], tag::SAVE);
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
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        assert_eq!(pending.bytes[0], tag::DEFINE_PATH);
        // Drawn again: nothing new to define.
        write_display_list(&list, &mut resources, &mut out);
        assert!(!resources.has_pending());
        drop(list);
        drop(path);
        resources.collect();
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        assert_eq!(pending.bytes[0], tag::DROP_PATH);
    }
}
