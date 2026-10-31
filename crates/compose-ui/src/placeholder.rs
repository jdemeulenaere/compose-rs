//! Placeholders, as Compose's `Placeholder`: an empty box in a text, for an inline image or a
//! custom emoji. A measurer leaves room for one; where it went is `Paragraph::placeholder_rects`.

use std::borrow::Cow;
use std::rc::Rc;

use crate::annotated::AnnotatedRange;
use crate::paragraph::Paragraph;
use crate::text_attributes::ResolvedTextDirection;
use crate::unit::{Density, Rect, TextUnit};

/// How a placeholder sits in its line, as Compose's `PlaceholderVerticalAlign`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PlaceholderVerticalAlign {
    /// Its bottom on the baseline.
    AboveBaseline,
    /// Its top on the line's top.
    Top,
    /// Its bottom on the line's bottom.
    Bottom,
    /// Its center on the line's center.
    Center,
    /// Its top on the top of the text around it.
    TextTop,
    /// Its bottom on the bottom of the text around it.
    TextBottom,
    /// Its center on the center of the text around it.
    TextCenter,
}

/// A box for a text to leave room for, as Compose's `Placeholder`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Placeholder {
    width: TextUnit,
    height: TextUnit,
    placeholder_vertical_align: PlaceholderVerticalAlign,
}

impl Placeholder {
    /// Panics on an unspecified `width` or `height`, as Compose's throws.
    pub fn new(
        width: TextUnit,
        height: TextUnit,
        placeholder_vertical_align: PlaceholderVerticalAlign,
    ) -> Placeholder {
        assert!(width.is_specified(), "width cannot be TextUnit.Unspecified");
        assert!(height.is_specified(), "height cannot be TextUnit.Unspecified");
        Placeholder { width, height, placeholder_vertical_align }
    }

    pub fn width(&self) -> TextUnit {
        self.width
    }

    pub fn height(&self) -> TextUnit {
        self.height
    }

    pub fn placeholder_vertical_align(&self) -> PlaceholderVerticalAlign {
        self.placeholder_vertical_align
    }

    /// Its size in whole pixels, as Compose's `PlaceholderSpan` measures one in a text of
    /// `font_px` pixels: an `em` is of that font size.
    pub fn size_in(&self, font_px: f32, density: &Density) -> (f32, f32) {
        let px = |unit: TextUnit| {
            crate::text::resolve_text_unit_px(unit, font_px, density).unwrap_or(0.0).ceil()
        };
        (px(self.width), px(self.height))
    }
}

/// A text's placeholders, shared by what's laid out from it. Most texts have none, which costs
/// nothing.
pub type Placeholders = Option<Rc<[AnnotatedRange<Placeholder>]>>;

/// Panics on a placeholder past the text's end or inside a character, on any stack, as Compose's
/// `setSpan` throws.
pub fn check_placeholders(text: &str, placeholders: &[AnnotatedRange<Placeholder>]) {
    for range in placeholders {
        assert!(
            range.end <= text.len()
                && text.is_char_boundary(range.start)
                && text.is_char_boundary(range.end),
            "placeholder over {}..{} isn't in a text of {} bytes at character boundaries",
            range.start,
            range.end,
            text.len(),
        );
    }
}

/// The indices of `placeholders` a text can show, by where they start: one that starts inside an
/// earlier one is left out, its characters covered already.
pub fn placeholders_in_text_order(placeholders: &[AnnotatedRange<Placeholder>]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..placeholders.len()).collect();
    order.sort_by_key(|index| placeholders[*index].start);
    let mut covered_to = 0;
    order.retain(|index| {
        let range = &placeholders[*index];
        let kept = range.start >= covered_to && range.start < range.end;
        if kept {
            covered_to = range.end;
        }
        kept
    });
    order
}

/// `text` as direction resolution sees it: to it a placeholder is a neutral object, whatever it
/// covers (a neutral per byte here, so offsets stay the text's).
pub fn with_placeholders_neutral<'a>(
    text: &'a str,
    placeholders: &[AnnotatedRange<Placeholder>],
) -> Cow<'a, str> {
    if placeholders.is_empty() {
        return text.into();
    }
    let mut bytes = text.as_bytes().to_vec();
    for range in placeholders {
        bytes[range.start.min(text.len())..range.end.min(text.len())].fill(b'!');
    }
    String::from_utf8_lossy(&bytes).into_owned().into()
}

/// A placeholder a measurer laid out over `start..end` of its text, in pixels: its size, and the
/// ascent and descent of the font around it (positive).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ResolvedPlaceholder {
    pub start: usize,
    pub end: usize,
    pub width: f32,
    pub height: f32,
    pub align: PlaceholderVerticalAlign,
    pub ascent: f32,
    pub descent: f32,
    /// An earlier one covers where it starts: the text doesn't show it.
    pub left_out: bool,
}

impl ResolvedPlaceholder {
    /// The ascent and descent its line has to have, as Compose's `PlaceholderSpan`: the font's,
    /// taller where the placeholder needs it.
    pub fn line_metrics(&self) -> (f32, f32) {
        use PlaceholderVerticalAlign as Align;
        let (mut ascent, mut descent) = (self.ascent, self.descent);
        match self.align {
            Align::Top | Align::TextTop => descent = descent.max(self.height - ascent),
            Align::Bottom | Align::TextBottom => ascent = ascent.max(self.height - descent),
            Align::Center | Align::TextCenter => {
                if ascent + descent < self.height {
                    // Whole pixels, as Compose's integers.
                    ascent += ((self.height - (ascent + descent)) / 2.0).trunc();
                    descent = self.height - ascent;
                }
            }
            Align::AboveBaseline => ascent = ascent.max(self.height),
        }
        (ascent, descent)
    }

    /// How far above the baseline a `TextCenter` one's top is: whole pixels, as Compose's integers.
    pub fn text_center_top(&self) -> f32 {
        ((self.ascent - self.descent + self.height) / 2.0).trunc()
    }

    /// Where it went in `paragraph`, as Compose's `placeholderRects`: `None` if it was cut by the
    /// maximum lines or an ellipsis (`ellipsis_start`: where a start or middle one hides from).
    fn rect_in(&self, paragraph: &dyn Paragraph, ellipsis_start: Option<usize>) -> Option<Rect> {
        use PlaceholderVerticalAlign as Align;
        if self.left_out {
            return None;
        }
        let line = paragraph.get_line_for_offset(self.start);
        let ellipsized =
            paragraph.is_line_ellipsized(line) && self.end > paragraph.get_line_end(line, true);
        let hidden = ellipsis_start.is_some_and(|start| self.end > start);
        if ellipsized || hidden || self.end > paragraph.get_line_end(line, false) {
            return None;
        }
        let right_to_left_line =
            paragraph.get_paragraph_direction(self.start) == ResolvedTextDirection::Rtl;
        let right_to_left_character =
            paragraph.get_bidi_run_direction(self.start) == ResolvedTextDirection::Rtl;
        let edge = paragraph
            .get_horizontal_position(self.start, right_to_left_line == right_to_left_character);
        let (left, right) = if right_to_left_character {
            (edge - self.width, edge)
        } else {
            (edge, edge + self.width)
        };
        let baseline = paragraph.get_line_baseline(line);
        let (line_top, line_bottom) =
            (paragraph.get_line_top(line), paragraph.get_line_bottom(line));
        let top = match self.align {
            Align::AboveBaseline => baseline - self.height,
            Align::Top => line_top,
            Align::Bottom => line_bottom - self.height,
            Align::Center => (line_top + line_bottom - self.height) / 2.0,
            Align::TextTop => baseline - self.ascent,
            Align::TextBottom => baseline + self.descent - self.height,
            Align::TextCenter => baseline - self.text_center_top(),
        };
        Some(Rect::new(left, top, right, top + self.height))
    }
}

/// Where each of `placeholders` went in `paragraph`, a start or middle ellipsis hiding text from
/// `ellipsis_start`.
pub fn placeholder_rects(
    paragraph: &dyn Paragraph,
    placeholders: &[ResolvedPlaceholder],
    ellipsis_start: Option<usize>,
) -> Vec<Option<Rect>> {
    placeholders.iter().map(|placeholder| placeholder.rect_in(paragraph, ellipsis_start)).collect()
}
