//! How text's values are saved for `remember_saveable`, as Compose's `Savers.kt`: each type a
//! [`Saveable`], saving what Compose's saver saves (a span style's font family, brush, draw
//! style and platform part aren't saved, as in Compose).

use compose_runtime::saveable::{Saveable, SavedValue};

use crate::font::{FontStyle, FontSynthesis, FontWeight};
use crate::graphics::{Color, Shadow};
use crate::intl::{Locale, LocaleList};
use crate::paragraph_style::ParagraphStyle;
use crate::platform_text_style::{EmojiSupportMatch, PlatformParagraphStyle};
use crate::span_style::SpanStyle;
use crate::text::SharedString;
use crate::text_attributes::{
    BaselineShift, Hyphens, LineBreak, LineBreakStrategy, LineBreakStrictness, LineBreakWordBreak,
    LineHeightAlignment, LineHeightMode, LineHeightStyle, LineHeightTrim, TextAlign,
    TextDecoration, TextDirection, TextGeometricTransform, TextIndent, TextMotion,
};
use crate::unit::{Offset, TextUnit, TextUnitType};

fn list(saved: &SavedValue) -> Option<&[SavedValue]> {
    match saved {
        SavedValue::List(values) => Some(values),
        _ => None,
    }
}

fn float(saved: &SavedValue) -> Option<f32> {
    f32::restore(saved)
}

/// An enum saved as its index in `values`.
fn save_index<T: PartialEq>(value: &T, values: &[T]) -> SavedValue {
    SavedValue::Int(values.iter().position(|candidate| candidate == value).unwrap_or(0) as i64)
}

fn restore_index<T: Copy>(saved: &SavedValue, values: &[T]) -> Option<T> {
    values.get(usize::restore(saved)?).copied()
}

/// Unspecified saves as `false`, else its value and type, as `TextUnit.Saver`.
impl Saveable for TextUnit {
    fn save(&self) -> SavedValue {
        if self.is_unspecified() {
            return SavedValue::Bool(false);
        }
        let unit = if self.is_sp() { 1 } else { 2 };
        SavedValue::List(vec![self.value().save(), SavedValue::Int(unit)])
    }
    fn restore(saved: &SavedValue) -> Option<TextUnit> {
        if *saved == SavedValue::Bool(false) {
            return Some(TextUnit::UNSPECIFIED);
        }
        let [value, unit] = list(saved)? else { return None };
        let unit = match i64::restore(unit)? {
            1 => TextUnitType::Sp,
            2 => TextUnitType::Em,
            _ => return None,
        };
        Some(TextUnit::new(float(value)?, unit))
    }
}

/// Unspecified saves as `false`, else its coordinates, as `Offset.Saver`.
impl Saveable for Offset {
    fn save(&self) -> SavedValue {
        if !self.is_specified() {
            return SavedValue::Bool(false);
        }
        SavedValue::List(vec![self.x.save(), self.y.save()])
    }
    fn restore(saved: &SavedValue) -> Option<Offset> {
        if *saved == SavedValue::Bool(false) {
            return Some(Offset::UNSPECIFIED);
        }
        let [x, y] = list(saved)? else { return None };
        Some(Offset::new(float(x)?, float(y)?))
    }
}

/// Its packed ARGB, as `Color.Saver` (unspecified included).
impl Saveable for Color {
    fn save(&self) -> SavedValue {
        SavedValue::Int(self.argb() as i64)
    }
    fn restore(saved: &SavedValue) -> Option<Color> {
        u32::try_from(i64::restore(saved)?).ok().map(Color::from_argb)
    }
}

/// As `Shadow.Saver`: color, offset, blur radius.
impl Saveable for Shadow {
    fn save(&self) -> SavedValue {
        SavedValue::List(vec![self.color.save(), self.offset.save(), self.blur_radius.save()])
    }
    fn restore(saved: &SavedValue) -> Option<Shadow> {
        let [color, offset, blur_radius] = list(saved)? else { return None };
        Some(Shadow {
            color: Color::restore(color)?,
            offset: Offset::restore(offset)?,
            blur_radius: float(blur_radius)?,
        })
    }
}

/// Its weight, as `FontWeight.Saver`.
impl Saveable for FontWeight {
    fn save(&self) -> SavedValue {
        SavedValue::Int(self.weight() as i64)
    }
    fn restore(saved: &SavedValue) -> Option<FontWeight> {
        let weight = u16::try_from(i64::restore(saved)?).ok()?;
        (1..=1000).contains(&weight).then(|| FontWeight::new(weight))
    }
}

/// Its multiplier, as `BaselineShift.Saver`.
impl Saveable for BaselineShift {
    fn save(&self) -> SavedValue {
        self.0.save()
    }
    fn restore(saved: &SavedValue) -> Option<BaselineShift> {
        float(saved).map(BaselineShift)
    }
}

/// Its mask (1 underline, 2 line through), as `TextDecoration.Saver`.
impl Saveable for TextDecoration {
    fn save(&self) -> SavedValue {
        SavedValue::Int(i64::from(self.underline) | i64::from(self.line_through) << 1)
    }
    fn restore(saved: &SavedValue) -> Option<TextDecoration> {
        let mask = i64::restore(saved)?;
        (0..4)
            .contains(&mask)
            .then_some(TextDecoration { underline: mask & 1 != 0, line_through: mask & 2 != 0 })
    }
}

/// As `TextGeometricTransform.Saver`: scale and skew.
impl Saveable for TextGeometricTransform {
    fn save(&self) -> SavedValue {
        SavedValue::List(vec![self.scale_x.save(), self.skew_x.save()])
    }
    fn restore(saved: &SavedValue) -> Option<TextGeometricTransform> {
        let [scale_x, skew_x] = list(saved)? else { return None };
        Some(TextGeometricTransform { scale_x: float(scale_x)?, skew_x: float(skew_x)? })
    }
}

/// As `TextIndent.Saver`: both indents.
impl Saveable for TextIndent {
    fn save(&self) -> SavedValue {
        SavedValue::List(vec![self.first_line.save(), self.rest_line.save()])
    }
    fn restore(saved: &SavedValue) -> Option<TextIndent> {
        let [first_line, rest_line] = list(saved)? else { return None };
        Some(TextIndent {
            first_line: TextUnit::restore(first_line)?,
            rest_line: TextUnit::restore(rest_line)?,
        })
    }
}

/// Its language tag, as `Locale.Saver`.
impl Saveable for Locale {
    fn save(&self) -> SavedValue {
        SavedValue::Text(self.to_language_tag().to_string())
    }
    fn restore(saved: &SavedValue) -> Option<Locale> {
        String::restore(saved).map(Locale::new)
    }
}

/// Its locales, as `LocaleList.Saver`.
impl Saveable for LocaleList {
    fn save(&self) -> SavedValue {
        SavedValue::List(self.iter().map(Saveable::save).collect())
    }
    fn restore(saved: &SavedValue) -> Option<LocaleList> {
        let locales: Option<Vec<Locale>> = list(saved)?.iter().map(Locale::restore).collect();
        locales.map(LocaleList::new)
    }
}

impl Saveable for SharedString {
    fn save(&self) -> SavedValue {
        SavedValue::Text(self.to_string())
    }
    fn restore(saved: &SavedValue) -> Option<SharedString> {
        String::restore(saved).map(SharedString::from)
    }
}

const FONT_STYLES: [FontStyle; 2] = [FontStyle::Normal, FontStyle::Italic];
const FONT_SYNTHESES: [FontSynthesis; 4] =
    [FontSynthesis::None, FontSynthesis::All, FontSynthesis::Weight, FontSynthesis::Style];
const TEXT_ALIGNS: [TextAlign; 6] = [
    TextAlign::Left,
    TextAlign::Right,
    TextAlign::Center,
    TextAlign::Justify,
    TextAlign::Start,
    TextAlign::End,
];
const TEXT_DIRECTIONS: [TextDirection; 5] = [
    TextDirection::Ltr,
    TextDirection::Rtl,
    TextDirection::Content,
    TextDirection::ContentOrLtr,
    TextDirection::ContentOrRtl,
];
const HYPHENS: [Hyphens; 2] = [Hyphens::None, Hyphens::Auto];
const TEXT_MOTIONS: [TextMotion; 2] = [TextMotion::Static, TextMotion::Animated];
const EMOJI_MATCHES: [EmojiSupportMatch; 3] =
    [EmojiSupportMatch::Default, EmojiSupportMatch::None, EmojiSupportMatch::All];
const TRIMS: [LineHeightTrim; 4] = [
    LineHeightTrim::FirstLineTop,
    LineHeightTrim::LastLineBottom,
    LineHeightTrim::Both,
    LineHeightTrim::None,
];
const MODES: [LineHeightMode; 4] = [
    LineHeightMode::Fixed,
    LineHeightMode::Minimum,
    LineHeightMode::Tight,
    LineHeightMode::PerLine,
];
const STRATEGIES: [LineBreakStrategy; 4] = [
    LineBreakStrategy::Simple,
    LineBreakStrategy::HighQuality,
    LineBreakStrategy::Balanced,
    LineBreakStrategy::Unspecified,
];
const STRICTNESSES: [LineBreakStrictness; 5] = [
    LineBreakStrictness::Default,
    LineBreakStrictness::Loose,
    LineBreakStrictness::Normal,
    LineBreakStrictness::Strict,
    LineBreakStrictness::Unspecified,
];
const WORD_BREAKS: [LineBreakWordBreak; 3] =
    [LineBreakWordBreak::Default, LineBreakWordBreak::Phrase, LineBreakWordBreak::Unspecified];

macro_rules! saveable_enums {
    ($($type:ty => $values:expr),* $(,)?) => {
        $(impl Saveable for $type {
            fn save(&self) -> SavedValue {
                save_index(self, &$values)
            }
            fn restore(saved: &SavedValue) -> Option<$type> {
                restore_index(saved, &$values)
            }
        })*
    };
}

saveable_enums!(
    FontStyle => FONT_STYLES,
    FontSynthesis => FONT_SYNTHESES,
    TextAlign => TEXT_ALIGNS,
    TextDirection => TEXT_DIRECTIONS,
    Hyphens => HYPHENS,
    TextMotion => TEXT_MOTIONS,
    EmojiSupportMatch => EMOJI_MATCHES,
);

/// As `PlatformParagraphStyle.Saver`: font padding and emoji matching.
impl Saveable for PlatformParagraphStyle {
    fn save(&self) -> SavedValue {
        SavedValue::List(vec![self.include_font_padding.save(), self.emoji_support_match.save()])
    }
    fn restore(saved: &SavedValue) -> Option<PlatformParagraphStyle> {
        let [padding, emoji] = list(saved)? else { return None };
        Some(PlatformParagraphStyle {
            include_font_padding: bool::restore(padding)?,
            emoji_support_match: EmojiSupportMatch::restore(emoji)?,
        })
    }
}

/// As `LineHeightStyle.Saver`: alignment, trim, mode.
impl Saveable for LineHeightStyle {
    fn save(&self) -> SavedValue {
        SavedValue::List(vec![
            self.alignment.top_ratio.save(),
            save_index(&self.trim, &TRIMS),
            save_index(&self.mode, &MODES),
        ])
    }
    fn restore(saved: &SavedValue) -> Option<LineHeightStyle> {
        let [alignment, trim, mode] = list(saved)? else { return None };
        Some(LineHeightStyle {
            alignment: LineHeightAlignment { top_ratio: float(alignment)? },
            trim: restore_index(trim, &TRIMS)?,
            mode: restore_index(mode, &MODES)?,
        })
    }
}

/// As `LineBreak.Saver`: strategy, strictness, word break.
impl Saveable for LineBreak {
    fn save(&self) -> SavedValue {
        SavedValue::List(vec![
            save_index(&self.strategy, &STRATEGIES),
            save_index(&self.strictness, &STRICTNESSES),
            save_index(&self.word_break, &WORD_BREAKS),
        ])
    }
    fn restore(saved: &SavedValue) -> Option<LineBreak> {
        let [strategy, strictness, word_break] = list(saved)? else { return None };
        Some(LineBreak {
            strategy: restore_index(strategy, &STRATEGIES)?,
            strictness: restore_index(strictness, &STRICTNESSES)?,
            word_break: restore_index(word_break, &WORD_BREAKS)?,
        })
    }
}

/// As `ParagraphStyleSaver`: every field.
impl Saveable for ParagraphStyle {
    fn save(&self) -> SavedValue {
        SavedValue::List(vec![
            self.text_align.save(),
            self.text_direction.save(),
            self.line_height.save(),
            self.text_indent.save(),
            self.platform_style.save(),
            self.line_height_style.save(),
            self.line_break.save(),
            self.hyphens.save(),
            self.text_motion.save(),
        ])
    }
    fn restore(saved: &SavedValue) -> Option<ParagraphStyle> {
        let [align, direction, height, indent, platform, height_style, line_break, hyphens, motion] =
            list(saved)?
        else {
            return None;
        };
        Some(ParagraphStyle {
            text_align: Option::restore(align)?,
            text_direction: Option::restore(direction)?,
            line_height: TextUnit::restore(height)?,
            text_indent: Option::restore(indent)?,
            platform_style: Option::restore(platform)?,
            line_height_style: Option::restore(height_style)?,
            line_break: Option::restore(line_break)?,
            hyphens: Option::restore(hyphens)?,
            text_motion: Option::restore(motion)?,
        })
    }
}

/// As `SpanStyleSaver`: the paint's color and every field but the font family, brush, draw
/// style and platform part (Compose's saver doesn't save them either).
impl Saveable for SpanStyle {
    fn save(&self) -> SavedValue {
        SavedValue::List(vec![
            self.foreground().color().save(),
            self.font_size.save(),
            self.font_weight.save(),
            self.font_style.save(),
            self.font_synthesis.save(),
            self.font_feature_settings.save(),
            self.letter_spacing.save(),
            self.baseline_shift.save(),
            self.text_geometric_transform.save(),
            self.locale_list.save(),
            self.background.save(),
            self.text_decoration.save(),
            self.shadow.save(),
        ])
    }
    fn restore(saved: &SavedValue) -> Option<SpanStyle> {
        let [
            color,
            font_size,
            font_weight,
            font_style,
            font_synthesis,
            font_feature_settings,
            letter_spacing,
            baseline_shift,
            text_geometric_transform,
            locale_list,
            background,
            text_decoration,
            shadow,
        ] = list(saved)?
        else {
            return None;
        };
        Some(SpanStyle {
            color: Color::restore(color)?,
            font_size: TextUnit::restore(font_size)?,
            font_weight: Option::restore(font_weight)?,
            font_style: Option::restore(font_style)?,
            font_synthesis: Option::restore(font_synthesis)?,
            font_feature_settings: Option::restore(font_feature_settings)?,
            letter_spacing: TextUnit::restore(letter_spacing)?,
            baseline_shift: Option::restore(baseline_shift)?,
            text_geometric_transform: Option::restore(text_geometric_transform)?,
            locale_list: Option::restore(locale_list)?,
            background: Color::restore(background)?,
            text_decoration: Option::restore(text_decoration)?,
            shadow: Option::restore(shadow)?,
            ..SpanStyle::DEFAULT
        })
    }
}
