//! Variable fonts' settings, as Compose's `androidx.compose.ui.text.font.FontVariation`: what a
//! font's axes are set to when it's loaded.

use std::fmt;
use std::rc::Rc;

use crate::font::{FontStyle, FontWeight};
use crate::text::SharedString;
use crate::unit::{Density, TextUnit};

/// The standard axes, and the settings of a font, as Compose's `FontVariation`.
pub struct FontVariation;

/// One axis's value, as Compose's `FontVariation.Setting`. Made by [`FontVariation`]'s functions;
/// an application's own axis is [`FontVariation::setting`].
#[derive(Clone, PartialEq)]
pub struct FontVariationSetting {
    axis_name: SharedString,
    value: SettingValue,
}

#[derive(Clone, Copy, PartialEq)]
enum SettingValue {
    Float(f32),
    Int(i32),
    /// In sp: scaled by the font scale when used.
    TextUnit(TextUnit),
}

impl FontVariationSetting {
    /// The axis's four letters.
    pub fn axis_name(&self) -> &str {
        &self.axis_name
    }

    /// Whether [`to_variation_value`](Self::to_variation_value) needs a density.
    pub fn needs_density(&self) -> bool {
        matches!(self.value, SettingValue::TextUnit(_))
    }

    /// The value the font's axis takes. Panics without a density, for one that
    /// [needs it](Self::needs_density): 12 sp is the same size on every screen, so only the font
    /// scale changes it.
    pub fn to_variation_value(&self, density: Option<&Density>) -> f32 {
        match self.value {
            SettingValue::Float(value) => value,
            SettingValue::Int(value) => value as f32,
            SettingValue::TextUnit(size) => {
                size.value() * density.expect("density must not be null").font_scale
            }
        }
    }
}

impl std::hash::Hash for FontVariationSetting {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.axis_name.hash(state);
        match self.value {
            SettingValue::Float(value) => value.to_bits().hash(state),
            SettingValue::Int(value) => value.hash(state),
            SettingValue::TextUnit(size) => size.value().to_bits().hash(state),
        }
    }
}

impl fmt::Debug for FontVariationSetting {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value {
            SettingValue::Float(value) => write!(formatter, "'{}' {value}", self.axis_name),
            SettingValue::Int(value) => write!(formatter, "'{}' {value}", self.axis_name),
            SettingValue::TextUnit(size) => write!(formatter, "'{}' {size:?}", self.axis_name),
        }
    }
}

/// A font's settings, each axis once, as Compose's `FontVariation.Settings`. Cloning shares them.
#[derive(Clone, PartialEq, Hash)]
pub struct FontVariationSettings {
    settings: Rc<[FontVariationSetting]>,
}

impl FontVariationSettings {
    /// Panics if an axis comes twice, as Compose's throws.
    pub fn new(settings: impl IntoIterator<Item = FontVariationSetting>) -> FontVariationSettings {
        let settings: Rc<[FontVariationSetting]> = settings.into_iter().collect();
        validate_unique_axes(&settings);
        FontVariationSettings { settings }
    }

    pub fn settings(&self) -> &[FontVariationSetting] {
        &self.settings
    }

    /// Whether one of them [needs a density](FontVariationSetting::needs_density).
    pub fn needs_density(&self) -> bool {
        self.settings.iter().any(FontVariationSetting::needs_density)
    }

    /// Whether `other` shares these, as Compose's tests' `isSameInstanceAs`.
    pub fn ptr_eq(&self, other: &FontVariationSettings) -> bool {
        Rc::ptr_eq(&self.settings, &other.settings)
    }

    /// These with `other`'s over them, each in place or after the rest, as Compose's `merge`.
    pub fn merge(&self, other: Option<&FontVariationSettings>) -> FontVariationSettings {
        match other {
            Some(other) if !other.settings.is_empty() => {
                if self.settings.is_empty() {
                    other.clone()
                } else {
                    FontVariationSettings::new(merge_lists(&self.settings, &other.settings))
                }
            }
            _ => self.clone(),
        }
    }

    /// These with `overrides` over them, as Compose's `merge(vararg)`.
    pub fn merge_settings(
        &self,
        overrides: impl IntoIterator<Item = FontVariationSetting>,
    ) -> FontVariationSettings {
        let overrides = FontVariationSettings::new(overrides);
        self.merge(Some(&overrides))
    }
}

impl fmt::Debug for FontVariationSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.settings.iter()).finish()
    }
}

fn merge_lists(
    base: &[FontVariationSetting],
    overrides: &[FontVariationSetting],
) -> Vec<FontVariationSetting> {
    let mut result = base.to_vec();
    for setting in overrides {
        match result.iter_mut().find(|kept| kept.axis_name == setting.axis_name) {
            Some(kept) => *kept = setting.clone(),
            None => result.push(setting.clone()),
        }
    }
    result
}

fn validate_unique_axes(settings: &[FontVariationSetting]) {
    for (index, setting) in settings.iter().enumerate() {
        assert!(
            settings[index + 1..].iter().all(|other| other.axis_name != setting.axis_name),
            "'{}' must be unique. Actual {settings:?}",
            setting.axis_name
        );
    }
}

impl FontVariation {
    /// No settings, as `FontVariation.Empty`.
    pub fn empty() -> FontVariationSettings {
        FontVariationSettings::new([])
    }

    /// `name`'s axis at `value`, as `FontVariation.Setting(name, value)`. Panics unless the name
    /// is four characters.
    pub fn setting(name: &str, value: f32) -> FontVariationSetting {
        assert!(
            name.chars().count() == 4,
            "Name must be exactly four characters. Actual: '{name}'"
        );
        FontVariationSetting {
            axis_name: SharedString::from(name.to_string()),
            value: SettingValue::Float(value),
        }
    }

    fn standard(axis_name: &'static str, value: SettingValue) -> FontVariationSetting {
        FontVariationSetting { axis_name: SharedString::from_static(axis_name), value }
    }

    /// `ital`, from upright (0) to italic (1).
    pub fn italic(value: f32) -> FontVariationSetting {
        assert!((0.0..=1.0).contains(&value), "'ital' must be in 0.0f..1.0f. Actual: {value}");
        FontVariation::standard("ital", SettingValue::Float(value))
    }

    /// `opsz`, the optical size: a text's size in sp.
    pub fn optical_sizing(text_size: TextUnit) -> FontVariationSetting {
        assert!(text_size.is_sp(), "'opsz' must be provided in sp units");
        FontVariation::standard("opsz", SettingValue::TextUnit(text_size))
    }

    /// `slnt`, the slant in degrees.
    pub fn slant(value: f32) -> FontVariationSetting {
        assert!((-90.0..=90.0).contains(&value), "'slnt' must be in -90f..90f. Actual: {value}");
        FontVariation::standard("slnt", SettingValue::Float(value))
    }

    /// `wdth`, the width in percents of normal.
    pub fn width(value: f32) -> FontVariationSetting {
        assert!(value > 0.0, "'wdth' must be strictly > 0.0f. Actual: {value}");
        FontVariation::standard("wdth", SettingValue::Float(value))
    }

    /// `wght`, the weight.
    pub fn weight(value: i32) -> FontVariationSetting {
        assert!((1..=1000).contains(&value), "'wght' value must be in [1, 1000]. Actual: {value}");
        FontVariation::standard("wght", SettingValue::Int(value))
    }

    /// `GRAD`, the grade: a weight that leaves the advances alone.
    pub fn grade(value: i32) -> FontVariationSetting {
        assert!((-1000..=1000).contains(&value), "'GRAD' must be in -1000..1000");
        FontVariation::standard("GRAD", SettingValue::Int(value))
    }

    /// `weight` and `style`'s axes, then `settings`, as `FontVariation.Settings(weight, style, …)`.
    pub fn settings(
        weight: FontWeight,
        style: FontStyle,
        settings: impl IntoIterator<Item = FontVariationSetting>,
    ) -> FontVariationSettings {
        let italic = if style == FontStyle::Italic { 1.0 } else { 0.0 };
        FontVariationSettings::new(
            [FontVariation::weight(i32::from(weight.weight())), FontVariation::italic(italic)]
                .into_iter()
                .chain(settings),
        )
    }
}
