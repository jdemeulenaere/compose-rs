//! Locales, as Compose's `androidx.compose.ui.text.intl`: what a span's `locale_list` names
//! (its glyphs' language). The model only: the platform's current locales and locale-aware case
//! mapping are the locale work's (docs/parity/ui-text-and-test.md, "locale").

use std::fmt;
use std::rc::Rc;

use compose_runtime::ComposeArg;

use crate::text::SharedString;

/// A locale, by its IETF BCP 47 language tag, as Compose's `Locale` ("en-US", "sr-Latn-SR").
/// Tags compare as written, with `_` read as `-`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Locale {
    tag: SharedString,
}

impl Locale {
    /// As `Locale(languageTag)`.
    pub fn new(language_tag: impl Into<SharedString>) -> Locale {
        let tag: SharedString = language_tag.into();
        let tag = if tag.contains('_') { SharedString::from(tag.replace('_', "-")) } else { tag };
        Locale { tag }
    }

    /// Its subtags before any extension (`-u-nu-arab`), whose short ones aren't regions.
    fn subtags(&self) -> impl Iterator<Item = &str> {
        self.tag.split('-').take_while(|subtag| subtag.len() != 1)
    }

    /// The language subtag (`en`), lower case, as Compose's `language`.
    pub fn language(&self) -> String {
        self.subtags().next().unwrap_or("").to_ascii_lowercase()
    }

    /// The four-letter script subtag (`Latn`), or empty, as Compose's `script`.
    pub fn script(&self) -> String {
        self.subtags()
            .skip(1)
            .find(|subtag| subtag.len() == 4 && subtag.chars().all(|c| c.is_ascii_alphabetic()))
            .map(|subtag| {
                let mut script = subtag.to_ascii_lowercase();
                script[..1].make_ascii_uppercase();
                script
            })
            .unwrap_or_default()
    }

    /// The region subtag (`US`, `419`), upper case, or empty, as Compose's `region`.
    pub fn region(&self) -> String {
        self.subtags()
            .skip(1)
            .find(|subtag| {
                (subtag.len() == 2 && subtag.chars().all(|c| c.is_ascii_alphabetic()))
                    || (subtag.len() == 3 && subtag.chars().all(|c| c.is_ascii_digit()))
            })
            .map(str::to_ascii_uppercase)
            .unwrap_or_default()
    }

    /// The tag, as Compose's `toLanguageTag`.
    pub fn to_language_tag(&self) -> &str {
        &self.tag
    }
}

impl fmt::Debug for Locale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.tag)
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.tag)
    }
}

/// Locales in order of preference, as Compose's `LocaleList`. Cloning shares them.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct LocaleList {
    locales: Rc<[Locale]>,
}

impl LocaleList {
    /// As `LocaleList(locales)`.
    pub fn new(locales: impl IntoIterator<Item = Locale>) -> LocaleList {
        LocaleList { locales: locales.into_iter().collect() }
    }

    /// From comma-separated language tags, as `LocaleList(languageTags)`.
    pub fn from_language_tags(language_tags: &str) -> LocaleList {
        LocaleList::new(language_tags.split(',').map(|tag| Locale::new(tag.trim().to_string())))
    }

    /// No locales, as `LocaleList.Empty`.
    pub fn empty() -> LocaleList {
        LocaleList::default()
    }

    pub fn locales(&self) -> &[Locale] {
        &self.locales
    }

    pub fn get(&self, index: usize) -> Option<&Locale> {
        self.locales.get(index)
    }

    pub fn len(&self) -> usize {
        self.locales.len()
    }

    pub fn is_empty(&self) -> bool {
        self.locales.is_empty()
    }

    pub fn contains(&self, locale: &Locale) -> bool {
        self.locales.contains(locale)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Locale> {
        self.locales.iter()
    }
}

impl fmt::Debug for LocaleList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "LocaleList(localeList={:?})", &*self.locales)
    }
}

impl ComposeArg for Locale {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ComposeArg for LocaleList {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.locales, &other.locales) || self == other
    }
}

/// The locales of the text below, as Compose's `LocalLocaleList`: none until the platform's are
/// read (docs/parity/ui-text-and-test.md, "locale").
pub static LOCAL_LOCALE_LIST: compose_runtime::CompositionLocal<LocaleList> =
    compose_runtime::CompositionLocal::new_static("LocalLocaleList", LocaleList::empty);
