//! How a paragraph is laid out, as Compose's `ParagraphStyle`.

use compose_runtime::ComposeArg;

use crate::platform_text_style::{
    PlatformParagraphStyle, lerp_platform_paragraph, merge_platform_paragraph,
};
use crate::text_attributes::{
    Hyphens, LineBreak, LineHeightStyle, TextAlign, TextDirection, TextIndent, TextMotion,
    lerp_discrete, lerp_text_unit_inheritable,
};
use crate::unit::TextUnit;

/// How a paragraph is laid out, as Compose's `ParagraphStyle`: alignment, direction, line
/// height, indent, line breaking. What isn't set (`None`, unspecified: Compose's `Unspecified`
/// values) comes from the style it's merged over, or the defaults
/// ([`crate::TextStyle::resolve_defaults`]).
///
/// Compose rejects a negative line height when the style is made; a struct literal can't, so
/// [`ParagraphStyle::validate`] checks it, which the annotated string builder and the text
/// layout call.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ParagraphStyle {
    pub text_align: Option<TextAlign>,
    pub text_direction: Option<TextDirection>,
    pub line_height: TextUnit,
    pub text_indent: Option<TextIndent>,
    pub platform_style: Option<PlatformParagraphStyle>,
    pub line_height_style: Option<LineHeightStyle>,
    pub line_break: Option<LineBreak>,
    pub hyphens: Option<Hyphens>,
    pub text_motion: Option<TextMotion>,
}

impl ParagraphStyle {
    /// Nothing set, as `ParagraphStyle()`.
    pub const DEFAULT: ParagraphStyle = ParagraphStyle {
        text_align: None,
        text_direction: None,
        line_height: TextUnit::UNSPECIFIED,
        text_indent: None,
        platform_style: None,
        line_height_style: None,
        line_break: None,
        hyphens: None,
        text_motion: None,
    };

    /// Panics if the line height is negative, as Compose's constructor ("lineHeight can't be
    /// negative").
    pub fn validate(&self) {
        if self.line_height.is_specified() {
            assert!(
                self.line_height.value() >= 0.0,
                "lineHeight can't be negative ({})",
                self.line_height.value()
            );
        }
    }

    /// `other`'s set fields over these, as Compose's `merge`: a platform style merges.
    pub fn merge(&self, other: &ParagraphStyle) -> ParagraphStyle {
        ParagraphStyle {
            text_align: other.text_align.or(self.text_align),
            text_direction: other.text_direction.or(self.text_direction),
            line_height: other.line_height.or(self.line_height),
            text_indent: other.text_indent.or(self.text_indent),
            platform_style: merge_platform_paragraph(self.platform_style, other.platform_style),
            line_height_style: other.line_height_style.or(self.line_height_style),
            line_break: other.line_break.or(self.line_break),
            hyphens: other.hyphens.or(self.hyphens),
            text_motion: other.text_motion.or(self.text_motion),
        }
    }

    /// Between `start` and `stop`, as Compose's `lerp(ParagraphStyle, ParagraphStyle, Float)`:
    /// line heights and indents interpolated (an unset indent from none), the rest discretely.
    pub fn lerp(start: &ParagraphStyle, stop: &ParagraphStyle, fraction: f32) -> ParagraphStyle {
        ParagraphStyle {
            text_align: lerp_discrete(start.text_align, stop.text_align, fraction),
            text_direction: lerp_discrete(start.text_direction, stop.text_direction, fraction),
            line_height: lerp_text_unit_inheritable(start.line_height, stop.line_height, fraction),
            text_indent: Some(TextIndent::lerp(
                start.text_indent.unwrap_or(TextIndent::NONE),
                stop.text_indent.unwrap_or(TextIndent::NONE),
                fraction,
            )),
            platform_style: lerp_platform_paragraph(
                start.platform_style,
                stop.platform_style,
                fraction,
            ),
            line_height_style: lerp_discrete(
                start.line_height_style,
                stop.line_height_style,
                fraction,
            ),
            line_break: lerp_discrete(start.line_break, stop.line_break, fraction),
            hyphens: lerp_discrete(start.hyphens, stop.hyphens, fraction),
            text_motion: lerp_discrete(start.text_motion, stop.text_motion, fraction),
        }
    }
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        ParagraphStyle::DEFAULT
    }
}

/// As Compose's `plus`: `merge`.
impl std::ops::Add for ParagraphStyle {
    type Output = ParagraphStyle;
    fn add(self, other: ParagraphStyle) -> ParagraphStyle {
        self.merge(&other)
    }
}

impl ComposeArg for ParagraphStyle {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}
