//! How text looks, as Compose's `TextStyle`: a span style's fields and a paragraph style's, with
//! a platform part.

use compose_runtime::ComposeArg;

use crate::alignment::LayoutDirection;
use crate::brush::Brush;
use crate::font::{FontFamily, FontStyle, FontSynthesis, FontWeight};
use crate::graphics::{Color, DrawStyle, Shadow};
use crate::intl::LocaleList;
use crate::paragraph_style::ParagraphStyle;
use crate::platform_text_style::PlatformTextStyle;
use crate::span_style::{SpanStyle, set_foreground};
use crate::text::SharedString;
use crate::text_attributes::{
    BaselineShift, Hyphens, LineBreak, LineHeightStyle, TextAlign, TextDecoration, TextDirection,
    TextForegroundStyle, TextGeometricTransform, TextIndent, TextMotion,
};
use crate::unit::TextUnit;

/// How text looks, as Compose's `TextStyle`: what isn't set (`None`, unspecified) comes from
/// the style it's merged over, or the measurer's defaults ([`TextStyle::resolve_defaults`]).
///
/// Its fields are a [`SpanStyle`]'s and a [`ParagraphStyle`]'s (`to_span_style`,
/// `to_paragraph_style`), and the platform part of both (`platform_style`). As in a span style,
/// `brush` paints the text when set, else `color` ([`TextStyle::foreground`]).
#[derive(Clone, Debug)]
pub struct TextStyle {
    pub color: Color,
    /// Paints the text instead of `color` when set.
    pub brush: Option<Brush>,
    /// The brush's opacity, 0 to 1 (NaN: the brush's own).
    pub alpha: f32,
    pub font_size: TextUnit,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
    pub font_synthesis: Option<FontSynthesis>,
    pub font_family: Option<FontFamily>,
    pub font_feature_settings: Option<SharedString>,
    pub letter_spacing: TextUnit,
    pub baseline_shift: Option<BaselineShift>,
    pub text_geometric_transform: Option<TextGeometricTransform>,
    pub locale_list: Option<LocaleList>,
    pub background: Color,
    pub text_decoration: Option<TextDecoration>,
    pub shadow: Option<Shadow>,
    pub draw_style: Option<DrawStyle>,
    pub text_align: Option<TextAlign>,
    pub text_direction: Option<TextDirection>,
    pub line_height: TextUnit,
    pub text_indent: Option<TextIndent>,
    pub platform_style: Option<PlatformTextStyle>,
    /// How `line_height` is spread around each line: `None` as `LineHeightStyle::DEFAULT`
    /// (proportionally, both ends trimmed), as Compose's.
    pub line_height_style: Option<LineHeightStyle>,
    pub line_break: Option<LineBreak>,
    pub hyphens: Option<Hyphens>,
    pub text_motion: Option<TextMotion>,
}

impl TextStyle {
    /// As `TextStyle.Default`: nothing set, which the theme or measurer resolves.
    pub const DEFAULT: TextStyle = TextStyle {
        color: Color::UNSPECIFIED,
        brush: None,
        alpha: f32::NAN,
        font_size: TextUnit::UNSPECIFIED,
        font_weight: None,
        font_style: None,
        font_synthesis: None,
        font_family: None,
        font_feature_settings: None,
        letter_spacing: TextUnit::UNSPECIFIED,
        baseline_shift: None,
        text_geometric_transform: None,
        locale_list: None,
        background: Color::UNSPECIFIED,
        text_decoration: None,
        shadow: None,
        draw_style: None,
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

    /// The style of `span_style` and `paragraph_style`, its platform part theirs, as Compose's
    /// internal `TextStyle(spanStyle, paragraphStyle)`.
    pub fn from_styles(span_style: SpanStyle, paragraph_style: ParagraphStyle) -> TextStyle {
        TextStyle {
            color: span_style.color,
            brush: span_style.brush,
            alpha: span_style.alpha,
            font_size: span_style.font_size,
            font_weight: span_style.font_weight,
            font_style: span_style.font_style,
            font_synthesis: span_style.font_synthesis,
            font_family: span_style.font_family,
            font_feature_settings: span_style.font_feature_settings,
            letter_spacing: span_style.letter_spacing,
            baseline_shift: span_style.baseline_shift,
            text_geometric_transform: span_style.text_geometric_transform,
            locale_list: span_style.locale_list,
            background: span_style.background,
            text_decoration: span_style.text_decoration,
            shadow: span_style.shadow,
            draw_style: span_style.draw_style,
            text_align: paragraph_style.text_align,
            text_direction: paragraph_style.text_direction,
            line_height: paragraph_style.line_height,
            text_indent: paragraph_style.text_indent,
            platform_style: PlatformTextStyle::of(
                span_style.platform_style,
                paragraph_style.platform_style,
            ),
            line_height_style: paragraph_style.line_height_style,
            line_break: paragraph_style.line_break,
            hyphens: paragraph_style.hyphens,
            text_motion: paragraph_style.text_motion,
        }
    }

    /// The span part, as Compose's `toSpanStyle`.
    pub fn to_span_style(&self) -> SpanStyle {
        SpanStyle {
            color: self.color,
            brush: self.brush.clone(),
            alpha: self.alpha,
            font_size: self.font_size,
            font_weight: self.font_weight,
            font_style: self.font_style,
            font_synthesis: self.font_synthesis,
            font_family: self.font_family.clone(),
            font_feature_settings: self.font_feature_settings.clone(),
            letter_spacing: self.letter_spacing,
            baseline_shift: self.baseline_shift,
            text_geometric_transform: self.text_geometric_transform,
            locale_list: self.locale_list.clone(),
            background: self.background,
            text_decoration: self.text_decoration,
            shadow: self.shadow,
            platform_style: self.platform_style.and_then(|style| style.span_style),
            draw_style: self.draw_style,
        }
    }

    /// The paragraph part, as Compose's `toParagraphStyle`.
    pub fn to_paragraph_style(&self) -> ParagraphStyle {
        ParagraphStyle {
            text_align: self.text_align,
            text_direction: self.text_direction,
            line_height: self.line_height,
            text_indent: self.text_indent,
            platform_style: self.platform_style.and_then(|style| style.paragraph_style),
            line_height_style: self.line_height_style,
            line_break: self.line_break,
            hyphens: self.hyphens,
            text_motion: self.text_motion,
        }
    }

    /// What the text is painted with: the brush if there's one, else the color.
    pub fn foreground(&self) -> TextForegroundStyle {
        TextForegroundStyle::of(self.color, self.brush.as_ref(), self.alpha)
    }

    /// A copy painted in `color` (no brush), as `copy(color = …)`.
    pub fn with_color(mut self, color: Color) -> TextStyle {
        let foreground = TextForegroundStyle::from_color(color);
        set_foreground(&mut self.color, &mut self.brush, &mut self.alpha, foreground);
        self
    }

    /// A copy painted with `brush` at `alpha` (no color), as `copy(brush = …, alpha = …)`.
    pub fn with_brush(mut self, brush: Option<Brush>, alpha: f32) -> TextStyle {
        let foreground = TextForegroundStyle::from_brush(brush.as_ref(), alpha);
        set_foreground(&mut self.color, &mut self.brush, &mut self.alpha, foreground);
        self
    }

    /// Whether a text in `other` is laid out as in this style, as Compose's
    /// `hasSameLayoutAffectingAttributes`: everything but what only paints (the paint,
    /// decoration, shadow and draw style, `has_same_draw_affecting_attributes`) is the same.
    pub fn has_same_layout_affecting_attributes(&self, other: &TextStyle) -> bool {
        self.paragraph_fields_equal(other)
            && self.font_size == other.font_size
            && self.font_weight == other.font_weight
            && self.font_style == other.font_style
            && self.font_synthesis == other.font_synthesis
            && self.font_family == other.font_family
            && self.font_feature_settings == other.font_feature_settings
            && self.letter_spacing == other.letter_spacing
            && self.baseline_shift == other.baseline_shift
            && self.text_geometric_transform == other.text_geometric_transform
            && self.locale_list == other.locale_list
            && self.background == other.background
            && self.platform_style.and_then(|style| style.span_style)
                == other.platform_style.and_then(|style| style.span_style)
    }

    /// Whether what only paints is the same (the paint, decoration, shadow, draw style), as
    /// Compose's `hasSameDrawAffectingAttributes`.
    pub fn has_same_draw_affecting_attributes(&self, other: &TextStyle) -> bool {
        self.foreground_equals(other)
            && self.text_decoration == other.text_decoration
            && self.shadow == other.shadow
            && self.draw_style == other.draw_style
    }

    fn foreground_equals(&self, other: &TextStyle) -> bool {
        // The common case, no brush on either side, without making the styles.
        if self.brush.is_none() && other.brush.is_none() {
            return self.color == other.color;
        }
        self.foreground() == other.foreground()
    }

    fn paragraph_fields_equal(&self, other: &TextStyle) -> bool {
        self.text_align == other.text_align
            && self.text_direction == other.text_direction
            && self.line_height == other.line_height
            && self.text_indent == other.text_indent
            && self.platform_style.and_then(|style| style.paragraph_style)
                == other.platform_style.and_then(|style| style.paragraph_style)
            && self.line_height_style == other.line_height_style
            && self.line_break == other.line_break
            && self.hyphens == other.hyphens
            && self.text_motion == other.text_motion
    }

    /// `other`'s set fields over these, as Compose's `merge(TextStyle)`: merging the default
    /// style changes nothing.
    pub fn merge(&self, other: &TextStyle) -> TextStyle {
        // Field by field, as `to_span_style().merge(…)` and `to_paragraph_style().merge(…)`
        // would, without making the parts (styles merge on every composition of a text).
        let span = |style: &TextStyle| style.platform_style.and_then(|style| style.span_style);
        let paragraph =
            |style: &TextStyle| style.platform_style.and_then(|style| style.paragraph_style);
        let mut merged = TextStyle {
            font_size: other.font_size.or(self.font_size),
            font_weight: other.font_weight.or(self.font_weight),
            font_style: other.font_style.or(self.font_style),
            font_synthesis: other.font_synthesis.or(self.font_synthesis),
            font_family: other.font_family.clone().or_else(|| self.font_family.clone()),
            font_feature_settings: other
                .font_feature_settings
                .clone()
                .or_else(|| self.font_feature_settings.clone()),
            letter_spacing: other.letter_spacing.or(self.letter_spacing),
            baseline_shift: other.baseline_shift.or(self.baseline_shift),
            text_geometric_transform: other
                .text_geometric_transform
                .or(self.text_geometric_transform),
            locale_list: other.locale_list.clone().or_else(|| self.locale_list.clone()),
            background: other.background.or(self.background),
            text_decoration: other.text_decoration.or(self.text_decoration),
            shadow: other.shadow.or(self.shadow),
            draw_style: other.draw_style.or(self.draw_style),
            text_align: other.text_align.or(self.text_align),
            text_direction: other.text_direction.or(self.text_direction),
            line_height: other.line_height.or(self.line_height),
            text_indent: other.text_indent.or(self.text_indent),
            platform_style: PlatformTextStyle::of(
                crate::platform_text_style::merge_platform_span(span(self), span(other)),
                crate::platform_text_style::merge_platform_paragraph(
                    paragraph(self),
                    paragraph(other),
                ),
            ),
            line_height_style: other.line_height_style.or(self.line_height_style),
            line_break: other.line_break.or(self.line_break),
            hyphens: other.hyphens.or(self.hyphens),
            text_motion: other.text_motion.or(self.text_motion),
            ..TextStyle::DEFAULT
        };
        if self.brush.is_none() && other.brush.is_none() {
            merged.color = other.color.or(self.color);
        } else {
            let foreground = self.foreground().merge(&other.foreground());
            set_foreground(&mut merged.color, &mut merged.brush, &mut merged.alpha, foreground);
        }
        merged
    }

    /// `other`'s set fields over this style's span part, as Compose's `merge(SpanStyle)`.
    pub fn merge_span_style(&self, other: &SpanStyle) -> TextStyle {
        TextStyle::from_styles(self.to_span_style().merge(other), self.to_paragraph_style())
    }

    /// `other`'s set fields over this style's paragraph part, as Compose's
    /// `merge(ParagraphStyle)`.
    pub fn merge_paragraph_style(&self, other: &ParagraphStyle) -> TextStyle {
        TextStyle::from_styles(self.to_span_style(), self.to_paragraph_style().merge(other))
    }

    /// Between `start` and `stop`, as Compose's `lerp(TextStyle, TextStyle, Float)`: their span
    /// and paragraph parts lerped.
    pub fn lerp(start: &TextStyle, stop: &TextStyle, fraction: f32) -> TextStyle {
        TextStyle::from_styles(
            SpanStyle::lerp(&start.to_span_style(), &stop.to_span_style(), fraction),
            ParagraphStyle::lerp(&start.to_paragraph_style(), &stop.to_paragraph_style(), fraction),
        )
    }

    /// Every unset field given its default, as Compose's `resolveDefaults`: black, 14 sp,
    /// normal weight and style, all synthesis, no letter spacing, shift, transform,
    /// decoration or shadow, a transparent background, a fill, start alignment, the layout's
    /// direction (`Content` falling back to it), no indent, simple line breaks, no hyphens,
    /// static motion. The line height, line height style and locales stay unset.
    pub fn resolve_defaults(style: &TextStyle, direction: LayoutDirection) -> TextStyle {
        let foreground = match style.foreground() {
            TextForegroundStyle::Unspecified => TextForegroundStyle::Color(Color::BLACK),
            foreground => foreground,
        };
        let mut resolved = TextStyle {
            font_size: style.font_size.or(TextUnit::sp(14.0)),
            font_weight: Some(style.font_weight.unwrap_or(FontWeight::NORMAL)),
            font_style: Some(style.font_style.unwrap_or(FontStyle::Normal)),
            font_synthesis: Some(style.font_synthesis.unwrap_or(FontSynthesis::All)),
            font_family: style.font_family.clone(),
            font_feature_settings: Some(
                style.font_feature_settings.clone().unwrap_or(SharedString::EMPTY),
            ),
            letter_spacing: style.letter_spacing.or(TextUnit::sp(0.0)),
            baseline_shift: Some(
                style.baseline_shift.unwrap_or(BaselineShift::NONE).or(BaselineShift::NONE),
            ),
            text_geometric_transform: Some(
                style.text_geometric_transform.unwrap_or(TextGeometricTransform::NONE),
            ),
            locale_list: style.locale_list.clone(),
            background: style.background.or(Color::TRANSPARENT),
            text_decoration: Some(style.text_decoration.unwrap_or(TextDecoration::NONE)),
            shadow: Some(style.shadow.unwrap_or(Shadow::NONE)),
            draw_style: Some(style.draw_style.unwrap_or(DrawStyle::Fill)),
            text_align: Some(style.text_align.unwrap_or(TextAlign::Start)),
            text_direction: Some(TextDirection::resolve(style.text_direction, direction)),
            line_height: style.line_height,
            text_indent: Some(style.text_indent.unwrap_or(TextIndent::NONE)),
            platform_style: style.platform_style,
            line_height_style: style.line_height_style,
            line_break: Some(style.line_break.unwrap_or(LineBreak::SIMPLE)),
            hyphens: Some(style.hyphens.unwrap_or(Hyphens::None)),
            text_motion: Some(style.text_motion.unwrap_or(TextMotion::Static)),
            ..TextStyle::DEFAULT
        };
        set_foreground(&mut resolved.color, &mut resolved.brush, &mut resolved.alpha, foreground);
        resolved
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle::DEFAULT
    }
}

/// Equal when everything that lays out and paints is, as Compose's: the paint compared as
/// [`TextStyle::foreground`].
impl PartialEq for TextStyle {
    fn eq(&self, other: &Self) -> bool {
        self.has_same_layout_affecting_attributes(other)
            && self.has_same_draw_affecting_attributes(other)
    }
}

impl Eq for TextStyle {}

/// Hashes what lays text out but its floats' and brushes' details (equal styles hash equally),
/// as Compose's `hashCode` over the paragraph's fields and the span's layout fields.
impl std::hash::Hash for TextStyle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.font_size.hash(state);
        self.font_weight.hash(state);
        self.font_style.hash(state);
        self.font_synthesis.hash(state);
        self.font_family.hash(state);
        self.font_feature_settings.hash(state);
        self.letter_spacing.hash(state);
        self.locale_list.hash(state);
        self.background.hash(state);
        self.text_decoration.hash(state);
        self.to_paragraph_style().hash(state);
    }
}

/// As Compose's `plus(TextStyle)`: `merge`.
impl std::ops::Add<&TextStyle> for &TextStyle {
    type Output = TextStyle;
    fn add(self, other: &TextStyle) -> TextStyle {
        self.merge(other)
    }
}

impl std::ops::Add for TextStyle {
    type Output = TextStyle;
    fn add(self, other: TextStyle) -> TextStyle {
        self.merge(&other)
    }
}

/// As Compose's `plus(SpanStyle)`.
impl std::ops::Add<SpanStyle> for TextStyle {
    type Output = TextStyle;
    fn add(self, other: SpanStyle) -> TextStyle {
        self.merge_span_style(&other)
    }
}

/// As Compose's `plus(ParagraphStyle)`.
impl std::ops::Add<ParagraphStyle> for TextStyle {
    type Output = TextStyle;
    fn add(self, other: ParagraphStyle) -> TextStyle {
        self.merge_paragraph_style(&other)
    }
}

impl ComposeArg for TextStyle {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}
