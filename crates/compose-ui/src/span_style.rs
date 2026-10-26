//! How a span of text looks, as Compose's `SpanStyle`.

use compose_runtime::ComposeArg;

use crate::brush::Brush;
use crate::font::{FontFamily, FontStyle, FontSynthesis, FontWeight};
use crate::graphics::{Color, DrawStyle, Shadow};
use crate::intl::LocaleList;
use crate::platform_text_style::{PlatformSpanStyle, lerp_platform_span, merge_platform_span};
use crate::text::SharedString;
use crate::text_attributes::{
    BaselineShift, TextDecoration, TextForegroundStyle, TextGeometricTransform, lerp_discrete,
    lerp_text_unit_inheritable,
};
use crate::unit::TextUnit;

/// How a span of text looks, as Compose's `SpanStyle`: what isn't set (`None`, unspecified)
/// comes from the text's style or an enclosing span.
///
/// The text is painted with `brush` (and `alpha`) when there's one, else `color`, as
/// Compose's two constructors: [`SpanStyle::foreground`] is that paint, and a solid color brush
/// is its color. Built with struct update from [`SpanStyle::DEFAULT`]; [`SpanStyle::with_color`]
/// and [`SpanStyle::with_brush`] replace the paint as Compose's `copy(color)` and
/// `copy(brush, alpha)` do.
#[derive(Clone, Debug)]
pub struct SpanStyle {
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
    /// OpenType feature settings, as CSS's `font-feature-settings` ("smcp, liga 0").
    pub font_feature_settings: Option<SharedString>,
    pub letter_spacing: TextUnit,
    pub baseline_shift: Option<BaselineShift>,
    pub text_geometric_transform: Option<TextGeometricTransform>,
    pub locale_list: Option<LocaleList>,
    /// Painted behind the span's glyphs.
    pub background: Color,
    pub text_decoration: Option<TextDecoration>,
    pub shadow: Option<Shadow>,
    pub platform_style: Option<PlatformSpanStyle>,
    pub draw_style: Option<DrawStyle>,
}

impl SpanStyle {
    /// Nothing set, as `SpanStyle()`.
    pub const DEFAULT: SpanStyle = SpanStyle {
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
        platform_style: None,
        draw_style: None,
    };

    /// What the text is painted with: the brush if there's one, else the color.
    pub fn foreground(&self) -> TextForegroundStyle {
        TextForegroundStyle::of(self.color, self.brush.as_ref(), self.alpha)
    }

    /// This style painted with `foreground` (its fields set as Compose's constructors do).
    pub(crate) fn with_foreground(mut self, foreground: TextForegroundStyle) -> SpanStyle {
        set_foreground(&mut self.color, &mut self.brush, &mut self.alpha, foreground);
        self
    }

    /// A copy painted in `color` (no brush), as `copy(color = …)`.
    pub fn with_color(self, color: Color) -> SpanStyle {
        self.with_foreground(TextForegroundStyle::from_color(color))
    }

    /// A copy painted with `brush` at `alpha` (no color), as `copy(brush = …, alpha = …)`.
    pub fn with_brush(self, brush: Option<Brush>, alpha: f32) -> SpanStyle {
        self.with_foreground(TextForegroundStyle::from_brush(brush.as_ref(), alpha))
    }

    /// `other`'s set fields over these, as Compose's `merge`: a brush wins over a color, a
    /// platform style merges.
    pub fn merge(&self, other: &SpanStyle) -> SpanStyle {
        SpanStyle {
            font_size: other.font_size.or(self.font_size),
            font_weight: other.font_weight.or(self.font_weight),
            font_style: other.font_style.or(self.font_style),
            font_synthesis: other.font_synthesis.or(self.font_synthesis),
            font_family: other.font_family.or(self.font_family),
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
            platform_style: merge_platform_span(self.platform_style, other.platform_style),
            draw_style: other.draw_style.or(self.draw_style),
            ..SpanStyle::DEFAULT
        }
        .with_foreground(self.foreground().merge(&other.foreground()))
    }

    /// Between `start` and `stop`, as Compose's `lerp(SpanStyle, SpanStyle, Float)`: numbers,
    /// colors, weights, shifts, transforms and shadows interpolated (an unset one from its
    /// default: normal weight, no shift or transform, a shadow from its transparent self), the
    /// rest discretely (`start` below half way).
    pub fn lerp(start: &SpanStyle, stop: &SpanStyle, fraction: f32) -> SpanStyle {
        SpanStyle {
            font_family: lerp_discrete(start.font_family, stop.font_family, fraction),
            font_size: lerp_text_unit_inheritable(start.font_size, stop.font_size, fraction),
            font_weight: Some(FontWeight::lerp(
                start.font_weight.unwrap_or(FontWeight::NORMAL),
                stop.font_weight.unwrap_or(FontWeight::NORMAL),
                fraction,
            )),
            font_style: lerp_discrete(start.font_style, stop.font_style, fraction),
            font_synthesis: lerp_discrete(start.font_synthesis, stop.font_synthesis, fraction),
            font_feature_settings: lerp_discrete(
                &start.font_feature_settings,
                &stop.font_feature_settings,
                fraction,
            )
            .clone(),
            letter_spacing: lerp_text_unit_inheritable(
                start.letter_spacing,
                stop.letter_spacing,
                fraction,
            ),
            baseline_shift: Some(BaselineShift::lerp(
                start.baseline_shift.unwrap_or(BaselineShift(0.0)),
                stop.baseline_shift.unwrap_or(BaselineShift(0.0)),
                fraction,
            )),
            text_geometric_transform: Some(TextGeometricTransform::lerp(
                start.text_geometric_transform.unwrap_or(TextGeometricTransform::NONE),
                stop.text_geometric_transform.unwrap_or(TextGeometricTransform::NONE),
                fraction,
            )),
            locale_list: lerp_discrete(&start.locale_list, &stop.locale_list, fraction).clone(),
            background: Color::lerp(start.background, stop.background, fraction),
            text_decoration: lerp_discrete(start.text_decoration, stop.text_decoration, fraction),
            shadow: lerp_shadow(start.shadow, stop.shadow, fraction),
            platform_style: lerp_platform_span(start.platform_style, stop.platform_style, fraction),
            draw_style: lerp_discrete(start.draw_style, stop.draw_style, fraction),
            ..SpanStyle::DEFAULT
        }
        .with_foreground(TextForegroundStyle::lerp(
            &start.foreground(),
            &stop.foreground(),
            fraction,
        ))
    }

    /// Whether a text in `other` is laid out as in this style, as Compose's internal
    /// `hasSameLayoutAffectingAttributes`.
    pub(crate) fn has_same_layout_affecting_attributes(&self, other: &SpanStyle) -> bool {
        self.font_size == other.font_size
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
            && self.platform_style == other.platform_style
    }

    /// Whether what only paints is the same (paint, decoration, shadow, draw style), as
    /// Compose's internal `hasSameNonLayoutAttributes`.
    pub(crate) fn has_same_non_layout_attributes(&self, other: &SpanStyle) -> bool {
        self.foreground() == other.foreground()
            && self.text_decoration == other.text_decoration
            && self.shadow == other.shadow
            && self.draw_style == other.draw_style
    }
}

/// Sets a style's `color`, `brush` and `alpha` fields to `foreground`, as Compose's constructors
/// hold it: a color without a brush, a brush and its alpha without a color.
pub(crate) fn set_foreground(
    color: &mut Color,
    brush: &mut Option<Brush>,
    alpha: &mut f32,
    foreground: TextForegroundStyle,
) {
    match foreground {
        TextForegroundStyle::Unspecified => {
            (*color, *brush, *alpha) = (Color::UNSPECIFIED, None, f32::NAN);
        }
        TextForegroundStyle::Color(value) => {
            (*color, *brush, *alpha) = (value, None, f32::NAN);
        }
        TextForegroundStyle::Brush { brush: value, alpha: value_alpha } => {
            (*color, *brush, *alpha) = (Color::UNSPECIFIED, Some(value), value_alpha);
        }
    }
}

/// Between two optional shadows, as Compose's `nullSafeLerp`: a missing one is the other made
/// transparent.
pub(crate) fn lerp_shadow(
    start: Option<Shadow>,
    stop: Option<Shadow>,
    fraction: f32,
) -> Option<Shadow> {
    let transparent = |shadow: Shadow| Shadow { color: shadow.color.with_alpha(0.0), ..shadow };
    match (start, stop) {
        (None, None) => None,
        (None, Some(stop)) => Some(Shadow::lerp(transparent(stop), stop, fraction)),
        (Some(start), None) => Some(Shadow::lerp(start, transparent(start), fraction)),
        (Some(start), Some(stop)) => Some(Shadow::lerp(start, stop, fraction)),
    }
}

impl Default for SpanStyle {
    fn default() -> Self {
        SpanStyle::DEFAULT
    }
}

/// Equal when everything that lays out and paints is, as Compose's: the paint compared as
/// [`SpanStyle::foreground`] (a solid brush equals its color).
impl PartialEq for SpanStyle {
    fn eq(&self, other: &Self) -> bool {
        self.has_same_layout_affecting_attributes(other)
            && self.has_same_non_layout_attributes(other)
    }
}

/// As Compose's `plus`: `merge`.
impl std::ops::Add<&SpanStyle> for &SpanStyle {
    type Output = SpanStyle;
    fn add(self, other: &SpanStyle) -> SpanStyle {
        self.merge(other)
    }
}

impl std::ops::Add for SpanStyle {
    type Output = SpanStyle;
    fn add(self, other: SpanStyle) -> SpanStyle {
        self.merge(&other)
    }
}

impl ComposeArg for SpanStyle {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}
