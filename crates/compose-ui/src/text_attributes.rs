//! The values text styles hold, as Compose's `androidx.compose.ui.text.style`: decorations,
//! alignment and direction, line heights, baseline shifts, geometric transforms, indents,
//! motion, line breaking and hyphens, and a text's foreground (a color or a brush).
//!
//! Compose's `Unspecified` values of the enum-like types are `None` in the styles' `Option`
//! fields (`TextAlign`, `TextDirection`, `LineBreak`, `Hyphens`).

use std::fmt;

use compose_runtime::ComposeArg;

use crate::brush::Brush;
use crate::graphics::Color;
use crate::unit::{TextUnit, lerp_f32};

macro_rules! compose_arg_by_eq {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

/// `start` below half way, `stop` from there, as Compose's `lerpDiscrete`.
pub(crate) fn lerp_discrete<T>(start: T, stop: T, fraction: f32) -> T {
    if fraction < 0.5 { start } else { stop }
}

/// Lines drawn through or under text, as `TextDecoration` (combined with `|` or `+`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextDecoration {
    pub underline: bool,
    pub line_through: bool,
}

impl TextDecoration {
    pub const NONE: TextDecoration = TextDecoration { underline: false, line_through: false };
    pub const UNDERLINE: TextDecoration = TextDecoration { underline: true, line_through: false };
    pub const LINE_THROUGH: TextDecoration =
        TextDecoration { underline: false, line_through: true };

    /// All of `decorations` together, as `TextDecoration.combine`.
    pub fn combine(decorations: &[TextDecoration]) -> TextDecoration {
        decorations.iter().fold(TextDecoration::NONE, |all, decoration| all | *decoration)
    }

    /// Whether every line of `other` is in this decoration, as Compose's `contains` (`NONE` is
    /// in every decoration).
    pub fn contains(self, other: TextDecoration) -> bool {
        (self | other) == self
    }
}

impl std::ops::BitOr for TextDecoration {
    type Output = TextDecoration;
    fn bitor(self, other: TextDecoration) -> TextDecoration {
        TextDecoration {
            underline: self.underline || other.underline,
            line_through: self.line_through || other.line_through,
        }
    }
}

/// As Compose's `plus`.
impl std::ops::Add for TextDecoration {
    type Output = TextDecoration;
    fn add(self, other: TextDecoration) -> TextDecoration {
        TextDecoration {
            underline: self.underline || other.underline,
            line_through: self.line_through || other.line_through,
        }
    }
}

/// As Compose's `toString`: `TextDecoration.Underline`, `TextDecoration[Underline,
/// LineThrough]`.
impl fmt::Display for TextDecoration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.underline, self.line_through) {
            (false, false) => formatter.write_str("TextDecoration.None"),
            (true, false) => formatter.write_str("TextDecoration.Underline"),
            (false, true) => formatter.write_str("TextDecoration.LineThrough"),
            (true, true) => formatter.write_str("TextDecoration[Underline, LineThrough]"),
        }
    }
}

impl fmt::Debug for TextDecoration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

/// How lines sit in a paragraph's width, as `TextAlign`: `Start` and `End` follow the text's
/// direction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,
}

/// Which way a paragraph's text runs, as Compose's `TextDirection`: fixed, or from its content
/// (its first strong character), falling back to the layout's direction (`Content`), or to
/// left-to-right or right-to-left.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TextDirection {
    Ltr,
    Rtl,
    Content,
    ContentOrLtr,
    ContentOrRtl,
}

impl TextDirection {
    /// The direction a paragraph lays out in under `layout_direction`, as Compose's
    /// `resolveTextDirection`: `Content` falls back to the layout's direction, and an
    /// unspecified one (`None`) is the layout's.
    pub fn resolve(
        direction: Option<TextDirection>,
        layout_direction: crate::alignment::LayoutDirection,
    ) -> TextDirection {
        use crate::alignment::LayoutDirection;
        match (direction, layout_direction) {
            (Some(TextDirection::Content), LayoutDirection::Ltr) => TextDirection::ContentOrLtr,
            (Some(TextDirection::Content), LayoutDirection::Rtl) => TextDirection::ContentOrRtl,
            (None, LayoutDirection::Ltr) => TextDirection::Ltr,
            (None, LayoutDirection::Rtl) => TextDirection::Rtl,
            (Some(direction), _) => direction,
        }
    }
}

/// The direction a paragraph or a run of text was laid out in, as Compose's
/// `ResolvedTextDirection`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ResolvedTextDirection {
    Ltr,
    Rtl,
}

/// What shows when text doesn't fit, as `TextOverflow`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[non_exhaustive]
pub enum TextOverflow {
    /// Cut at the bounds.
    #[default]
    Clip,
    /// The last line shown ends with "…".
    Ellipsis,
    /// Drawn past the bounds.
    Visible,
}

/// Where a line's extra height goes, as `LineHeightStyle.Alignment`: the share of it above the
/// text (`top_ratio`, 0 to 1), or `PROPORTIONAL` (-1: as the font's ascent and descent share
/// its height).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LineHeightAlignment {
    pub top_ratio: f32,
}

impl LineHeightAlignment {
    pub const TOP: LineHeightAlignment = LineHeightAlignment { top_ratio: 0.0 };
    pub const CENTER: LineHeightAlignment = LineHeightAlignment { top_ratio: 0.5 };
    pub const PROPORTIONAL: LineHeightAlignment = LineHeightAlignment { top_ratio: -1.0 };
    pub const BOTTOM: LineHeightAlignment = LineHeightAlignment { top_ratio: 1.0 };

    /// As `Alignment(topRatio)`: panics unless `top_ratio` is in 0 to 1, or -1, as Compose's
    /// throws.
    pub fn new(top_ratio: f32) -> LineHeightAlignment {
        assert!(
            (0.0..=1.0).contains(&top_ratio) || top_ratio == -1.0,
            "topRatio should be in [0..1] range or -1"
        );
        LineHeightAlignment { top_ratio }
    }
}

impl Eq for LineHeightAlignment {}

impl std::hash::Hash for LineHeightAlignment {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.top_ratio.to_bits().hash(state);
    }
}

/// Whether the first line's space above the text and the last line's below it are left out,
/// as `LineHeightStyle.Trim`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LineHeightTrim {
    FirstLineTop,
    LastLineBottom,
    Both,
    None,
}

impl LineHeightTrim {
    pub fn trims_first_line_top(self) -> bool {
        matches!(self, LineHeightTrim::FirstLineTop | LineHeightTrim::Both)
    }

    pub fn trims_last_line_bottom(self) -> bool {
        matches!(self, LineHeightTrim::LastLineBottom | LineHeightTrim::Both)
    }
}

/// How the line height applies to each line, as `LineHeightStyle.Mode`: every line exactly the
/// line height (`Fixed`), at least it (`Minimum`), at least it without the font's extra
/// padding (`Tight`), or the line height applied per line of the paragraph (`PerLine`). The
/// measurers lay text out in `Fixed`; the others are the model's for now.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum LineHeightMode {
    #[default]
    Fixed,
    Minimum,
    Tight,
    PerLine,
}

/// How a line height is spread around each line's text, as Compose's `LineHeightStyle`. A
/// style without one (`TextStyle::line_height_style` `None`) takes `LineHeightStyle::DEFAULT`,
/// proportional with both ends trimmed, as Compose's `createCharSequence` does without
/// `includeFontPadding` (which compose-rs doesn't have). Material 3's text styles center it and
/// trim nothing (`DefaultLineHeightStyle`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LineHeightStyle {
    pub alignment: LineHeightAlignment,
    pub trim: LineHeightTrim,
    pub mode: LineHeightMode,
}

impl LineHeightStyle {
    /// As `LineHeightStyle.Default`: proportional, both ends trimmed, fixed.
    pub const DEFAULT: LineHeightStyle = LineHeightStyle {
        alignment: LineHeightAlignment::PROPORTIONAL,
        trim: LineHeightTrim::Both,
        mode: LineHeightMode::Fixed,
    };

    /// As `LineHeightStyle(alignment, trim)`: the `Fixed` mode.
    pub const fn new(alignment: LineHeightAlignment, trim: LineHeightTrim) -> LineHeightStyle {
        LineHeightStyle { alignment, trim, mode: LineHeightMode::Fixed }
    }
}

/// How far a span's baseline moves up, as a multiple of its ascent, as Compose's
/// `BaselineShift` (negative moves it down).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BaselineShift(pub f32);

impl BaselineShift {
    pub const SUPERSCRIPT: BaselineShift = BaselineShift(0.5);
    pub const SUBSCRIPT: BaselineShift = BaselineShift(-0.5);
    pub const NONE: BaselineShift = BaselineShift(0.0);
    /// NaN: decided elsewhere.
    pub const UNSPECIFIED: BaselineShift = BaselineShift(f32::NAN);

    pub fn multiplier(self) -> f32 {
        self.0
    }

    pub fn is_specified(self) -> bool {
        !self.0.is_nan()
    }

    /// `self` if specified, else `other`, as `takeOrElse`.
    pub fn or(self, other: BaselineShift) -> BaselineShift {
        if self.is_specified() { self } else { other }
    }

    /// Between `start` and `stop`, as Compose's `lerp(BaselineShift, BaselineShift, Float)`.
    pub fn lerp(start: BaselineShift, stop: BaselineShift, fraction: f32) -> BaselineShift {
        BaselineShift(lerp_f32(start.0, stop.0, fraction))
    }
}

/// A horizontal scale and skew of glyphs, as Compose's `TextGeometricTransform`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextGeometricTransform {
    pub scale_x: f32,
    pub skew_x: f32,
}

impl TextGeometricTransform {
    /// No transform: a scale of 1, no skew (Compose's internal `None`, and its defaults).
    pub const NONE: TextGeometricTransform = TextGeometricTransform { scale_x: 1.0, skew_x: 0.0 };

    /// Between `start` and `stop`, as Compose's `lerp`.
    pub fn lerp(
        start: TextGeometricTransform,
        stop: TextGeometricTransform,
        fraction: f32,
    ) -> TextGeometricTransform {
        TextGeometricTransform {
            scale_x: lerp_f32(start.scale_x, stop.scale_x, fraction),
            skew_x: lerp_f32(start.skew_x, stop.skew_x, fraction),
        }
    }
}

impl Default for TextGeometricTransform {
    fn default() -> Self {
        TextGeometricTransform::NONE
    }
}

/// How far a paragraph's first line and the others are indented, as Compose's `TextIndent`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TextIndent {
    pub first_line: TextUnit,
    pub rest_line: TextUnit,
}

impl TextIndent {
    /// As `TextIndent.None`: no indent.
    pub const NONE: TextIndent =
        TextIndent { first_line: TextUnit::sp(0.0), rest_line: TextUnit::sp(0.0) };

    /// Between `start` and `stop`, as Compose's `lerp(TextIndent, TextIndent, Float)`: each
    /// indent as a `TextUnit` that may be inherited (discretely when unspecified).
    pub fn lerp(start: TextIndent, stop: TextIndent, fraction: f32) -> TextIndent {
        TextIndent {
            first_line: lerp_text_unit_inheritable(start.first_line, stop.first_line, fraction),
            rest_line: lerp_text_unit_inheritable(start.rest_line, stop.rest_line, fraction),
        }
    }
}

impl Default for TextIndent {
    fn default() -> Self {
        TextIndent::NONE
    }
}

/// Between `start` and `stop`, or discretely when either is unspecified, as Compose's
/// `lerpTextUnitInheritable`.
pub(crate) fn lerp_text_unit_inheritable(
    start: TextUnit,
    stop: TextUnit,
    fraction: f32,
) -> TextUnit {
    if start.is_unspecified() || stop.is_unspecified() {
        lerp_discrete(start, stop, fraction)
    } else {
        TextUnit::lerp(start, stop, fraction)
    }
}

/// How text is positioned when it moves, as Compose's `TextMotion`: `Static` for crisp still
/// text (font hinting, no subpixel positioning), `Animated` for text that scales or moves
/// (linear, subpixel positioned).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TextMotion {
    Static,
    Animated,
}

/// How lines break, as `LineBreak.Strategy`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LineBreakStrategy {
    /// Greedy: as much on each line as fits.
    Simple,
    /// Fewer hyphens and more even lines.
    HighQuality,
    /// Lines of about the same width.
    Balanced,
    Unspecified,
}

/// Which line-breaking rules apply, for CJK text, as `LineBreak.Strictness`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LineBreakStrictness {
    Default,
    Loose,
    Normal,
    Strict,
    Unspecified,
}

/// Where words may break, as `LineBreak.WordBreak`: anywhere a word may (`Default`), or between
/// phrases for Korean and Japanese (`Phrase`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LineBreakWordBreak {
    Default,
    Phrase,
    Unspecified,
}

/// How a paragraph's lines break, as Compose's `LineBreak` (Android's line break config): a
/// strategy, a strictness and a word break. Desktop's SkParagraph breaks greedily whatever the
/// configuration.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LineBreak {
    pub strategy: LineBreakStrategy,
    pub strictness: LineBreakStrictness,
    pub word_break: LineBreakWordBreak,
}

impl LineBreak {
    /// Simple, normal, default word breaks: for short text (labels, buttons); the default.
    pub const SIMPLE: LineBreak = LineBreak {
        strategy: LineBreakStrategy::Simple,
        strictness: LineBreakStrictness::Normal,
        word_break: LineBreakWordBreak::Default,
    };
    /// Balanced, loose, by phrases: for titles.
    pub const HEADING: LineBreak = LineBreak {
        strategy: LineBreakStrategy::Balanced,
        strictness: LineBreakStrictness::Loose,
        word_break: LineBreakWordBreak::Phrase,
    };
    /// High quality, strict, default word breaks: for longer text.
    pub const PARAGRAPH: LineBreak = LineBreak {
        strategy: LineBreakStrategy::HighQuality,
        strictness: LineBreakStrictness::Strict,
        word_break: LineBreakWordBreak::Default,
    };
}

/// Whether words are hyphenated at line breaks, as Compose's `Hyphens`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Hyphens {
    None,
    /// Where the language's hyphenation patterns allow (Android).
    Auto,
}

/// What a text is painted with, as Compose's (internal) `TextForegroundStyle`: a color, a brush
/// with an alpha, or nothing set. A style's `color`, `brush` and `alpha` fields come together
/// as one of these ([`TextForegroundStyle::of`]): a brush wins over the color, and a solid
/// color brush is a color.
#[derive(Clone, Debug, Default)]
pub enum TextForegroundStyle {
    #[default]
    Unspecified,
    Color(Color),
    /// A gradient (never `Brush::Solid`) and the alpha it's drawn with (NaN: the brush's own).
    Brush {
        brush: Brush,
        alpha: f32,
    },
}

/// Equal paints: a brush's alpha compared by its bits (NaN equals NaN), as Compose's data
/// classes compare floats.
impl PartialEq for TextForegroundStyle {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TextForegroundStyle::Unspecified, TextForegroundStyle::Unspecified) => true,
            (TextForegroundStyle::Color(a), TextForegroundStyle::Color(b)) => a == b,
            (
                TextForegroundStyle::Brush { brush: a, alpha: a_alpha },
                TextForegroundStyle::Brush { brush: b, alpha: b_alpha },
            ) => a == b && a_alpha.to_bits() == b_alpha.to_bits(),
            _ => false,
        }
    }
}

impl TextForegroundStyle {
    /// A color's style: unspecified for `Color::UNSPECIFIED`.
    pub fn from_color(color: Color) -> TextForegroundStyle {
        if color.is_specified() {
            TextForegroundStyle::Color(color)
        } else {
            TextForegroundStyle::Unspecified
        }
    }

    /// A brush's style: a solid brush is its color with `alpha` applied, a gradient keeps
    /// `alpha`; none is unspecified.
    pub fn from_brush(brush: Option<&Brush>, alpha: f32) -> TextForegroundStyle {
        match brush {
            None => TextForegroundStyle::Unspecified,
            Some(Brush::Solid(color)) => TextForegroundStyle::from_color(color.modulate(alpha)),
            Some(brush) => TextForegroundStyle::Brush { brush: brush.clone(), alpha },
        }
    }

    /// A style's foreground from its `color`, `brush` and `alpha` fields: the brush if there's
    /// one, else the color, as Compose's constructors.
    pub fn of(color: Color, brush: Option<&Brush>, alpha: f32) -> TextForegroundStyle {
        match brush {
            Some(brush) => TextForegroundStyle::from_brush(Some(brush), alpha),
            None => TextForegroundStyle::from_color(color),
        }
    }

    /// The color (unspecified for a brush), as Compose's `color`.
    pub fn color(&self) -> Color {
        match self {
            TextForegroundStyle::Color(color) => *color,
            _ => Color::UNSPECIFIED,
        }
    }

    /// The brush (none for a color), as Compose's `brush`.
    pub fn brush(&self) -> Option<&Brush> {
        match self {
            TextForegroundStyle::Brush { brush, .. } => Some(brush),
            _ => None,
        }
    }

    /// The alpha: a color's own, a brush's given one, NaN when unspecified, as Compose's.
    pub fn alpha(&self) -> f32 {
        match self {
            TextForegroundStyle::Unspecified => f32::NAN,
            TextForegroundStyle::Color(color) => color.alpha() as f32 / 255.0,
            TextForegroundStyle::Brush { alpha, .. } => *alpha,
        }
    }

    /// `other` over this one, as Compose's `merge`: a brush wins over a color, a brush's
    /// unspecified alpha keeps this brush's, and an unspecified `other` keeps this one.
    pub fn merge(&self, other: &TextForegroundStyle) -> TextForegroundStyle {
        match (self, other) {
            (
                TextForegroundStyle::Brush { alpha: this_alpha, .. },
                TextForegroundStyle::Brush { brush, alpha },
            ) => TextForegroundStyle::Brush {
                brush: brush.clone(),
                alpha: if alpha.is_nan() { *this_alpha } else { *alpha },
            },
            (_, TextForegroundStyle::Brush { .. }) => other.clone(),
            (TextForegroundStyle::Brush { .. }, _) => self.clone(),
            (_, TextForegroundStyle::Unspecified) => self.clone(),
            _ => other.clone(),
        }
    }

    /// Between `start` and `stop`, as Compose's `lerp`: colors in Oklab, two brushes discretely
    /// with their alphas interpolated, a brush and a color discretely.
    pub fn lerp(
        start: &TextForegroundStyle,
        stop: &TextForegroundStyle,
        fraction: f32,
    ) -> TextForegroundStyle {
        match (start, stop) {
            (TextForegroundStyle::Brush { .. }, TextForegroundStyle::Brush { .. }) => {
                TextForegroundStyle::from_brush(
                    lerp_discrete(start.brush(), stop.brush(), fraction),
                    lerp_f32(start.alpha(), stop.alpha(), fraction),
                )
            }
            (TextForegroundStyle::Brush { .. }, _) | (_, TextForegroundStyle::Brush { .. }) => {
                lerp_discrete(start, stop, fraction).clone()
            }
            _ => {
                TextForegroundStyle::from_color(Color::lerp(start.color(), stop.color(), fraction))
            }
        }
    }
}

compose_arg_by_eq!(
    TextDecoration,
    TextAlign,
    TextDirection,
    ResolvedTextDirection,
    TextOverflow,
    LineHeightStyle,
    BaselineShift,
    TextGeometricTransform,
    TextIndent,
    TextMotion,
    LineBreak,
    Hyphens,
    TextForegroundStyle
);
