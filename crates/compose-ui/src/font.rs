//! Fonts' attributes, as Compose's `androidx.compose.ui.text.font`: weight, style, family and
//! synthesis.

use std::any::Any;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

use compose_runtime::ComposeArg;

use crate::font_variation::{FontVariation, FontVariationSettings};

/// A font's weight, from 1 (thinnest) to 1000 (boldest), as Compose's `FontWeight`. Weights
/// order by their value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontWeight(u16);

impl FontWeight {
    pub const W100: FontWeight = FontWeight(100);
    pub const W200: FontWeight = FontWeight(200);
    pub const W300: FontWeight = FontWeight(300);
    pub const W400: FontWeight = FontWeight(400);
    pub const W500: FontWeight = FontWeight(500);
    pub const W600: FontWeight = FontWeight(600);
    pub const W700: FontWeight = FontWeight(700);
    pub const W800: FontWeight = FontWeight(800);
    pub const W900: FontWeight = FontWeight(900);
    /// 100.
    pub const THIN: FontWeight = FontWeight::W100;
    /// 200.
    pub const EXTRA_LIGHT: FontWeight = FontWeight::W200;
    /// 300.
    pub const LIGHT: FontWeight = FontWeight::W300;
    /// 400, the default.
    pub const NORMAL: FontWeight = FontWeight::W400;
    /// 500.
    pub const MEDIUM: FontWeight = FontWeight::W500;
    /// 600.
    pub const SEMI_BOLD: FontWeight = FontWeight::W600;
    /// 700.
    pub const BOLD: FontWeight = FontWeight::W700;
    /// 800.
    pub const EXTRA_BOLD: FontWeight = FontWeight::W800;
    /// 900.
    pub const BLACK: FontWeight = FontWeight::W900;

    /// `W100` to `W900`, as Compose's (internal) `values`.
    pub const VALUES: [FontWeight; 9] = [
        FontWeight::W100,
        FontWeight::W200,
        FontWeight::W300,
        FontWeight::W400,
        FontWeight::W500,
        FontWeight::W600,
        FontWeight::W700,
        FontWeight::W800,
        FontWeight::W900,
    ];

    /// As `FontWeight(weight)`: panics outside 1 to 1000, as Compose's throws.
    pub const fn new(weight: u16) -> FontWeight {
        assert!(weight >= 1 && weight <= 1000, "Font weight can be in range [1, 1000]");
        FontWeight(weight)
    }

    pub const fn weight(self) -> u16 {
        self.0
    }

    /// Between `start` and `stop`, rounded and kept in 1 to 1000, as Compose's
    /// `lerp(FontWeight, FontWeight, Float)`.
    pub fn lerp(start: FontWeight, stop: FontWeight, fraction: f32) -> FontWeight {
        let (start, stop) = (start.0 as i32, stop.0 as i32);
        // As `lerp(Int, Int, Float)`: the difference rounded half up, in doubles.
        let weight = start + ((stop - start) as f64 * fraction as f64 + 0.5).floor() as i32;
        FontWeight(weight.clamp(1, 1000) as u16)
    }
}

impl fmt::Debug for FontWeight {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "FontWeight(weight={})", self.0)
    }
}

impl ComposeArg for FontWeight {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Upright or italic, as `FontStyle`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FontStyle {
    Normal,
    Italic,
}

/// A font family, as `FontFamily`: the generic ones, a family installed on the system by name
/// (falling back to the default), the fonts given, or a typeface already loaded.
#[derive(Clone, PartialEq, Debug)]
pub enum FontFamily {
    /// The platform's default, as `FontFamily.Default`: what a style with no family has.
    Default,
    SansSerif,
    Serif,
    Monospace,
    Cursive,
    Named(&'static str),
    /// As Compose's `FontListFontFamily`, [`FontFamily::of`].
    FontList(FontListFontFamily),
    /// As Compose's `LoadedFontFamily`, [`FontFamily::from_typeface`].
    Loaded(Typeface),
}

impl FontFamily {
    /// A family of `fonts`, as `FontFamily(fonts)`: later ones of a weight and style are the
    /// fallbacks of the first. Panics without a font, as Compose's throws.
    pub fn of(fonts: impl IntoIterator<Item = Font>) -> FontFamily {
        let fonts: Rc<[Font]> = fonts.into_iter().collect();
        assert!(!fonts.is_empty(), "At least one font should be passed to FontFamily");
        FontFamily::FontList(FontListFontFamily { fonts })
    }

    /// A family of a loaded typeface, as `FontFamily(typeface)`.
    pub fn from_typeface(typeface: Typeface) -> FontFamily {
        FontFamily::Loaded(typeface)
    }

    /// The name of a generic family, as `GenericFontFamily.name`.
    pub fn generic_name(&self) -> Option<&'static str> {
        match self {
            FontFamily::SansSerif => Some("sans-serif"),
            FontFamily::Serif => Some("serif"),
            FontFamily::Monospace => Some("monospace"),
            FontFamily::Cursive => Some("cursive"),
            _ => None,
        }
    }
}

impl std::hash::Hash for FontFamily {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            FontFamily::Named(name) => name.hash(state),
            FontFamily::FontList(list) => list.fonts.hash(state),
            FontFamily::Loaded(typeface) => Rc::as_ptr(&typeface.0).cast::<()>().hash(state),
            _ => {}
        }
    }
}

/// The fonts of a family, as Compose's `FontListFontFamily`. Cloning shares them.
#[derive(Clone, Debug)]
pub struct FontListFontFamily {
    fonts: Rc<[Font]>,
}

impl PartialEq for FontListFontFamily {
    /// The same list, nearly always: a style's family, cloned.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.fonts, &other.fonts) || self.fonts == other.fonts
    }
}

impl FontListFontFamily {
    pub fn fonts(&self) -> &[Font] {
        &self.fonts
    }
}

/// A platform's typeface, as Compose's `Typeface` and what `FontFamily.Resolver` resolves to:
/// opaque here, what the platform's text stack made it from. Compared by identity; cloning shares
/// it.
#[derive(Clone)]
pub struct Typeface(Rc<dyn Any>);

impl Typeface {
    pub fn new(platform_typeface: impl Any) -> Typeface {
        Typeface(Rc::new(platform_typeface))
    }

    /// The platform's typeface, if it's a `T`.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

impl PartialEq for Typeface {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Typeface {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Typeface({:p})", Rc::as_ptr(&self.0))
    }
}

/// How a font is loaded, as Compose's `FontLoadingStrategy`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FontLoadingStrategy {
    /// Loaded when first asked for, stopping everything until it is; a failure is the
    /// platform's default font.
    Blocking,
    /// As `Blocking`, but a font that isn't there is no failure: the next one is tried.
    OptionalLocal,
    /// Loaded in the background, the text in a fallback until then, laid out again when it's
    /// there; given up on after [`Font::MAXIMUM_ASYNC_TIMEOUT_MILLIS`].
    Async,
}

/// A name of a font family on the device, as Compose's `DeviceFontFamilyName`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeviceFontFamilyName(crate::text::SharedString);

impl DeviceFontFamilyName {
    /// Panics on an empty name, as Compose's throws.
    pub fn new(name: impl Into<crate::text::SharedString>) -> DeviceFontFamilyName {
        let name = name.into();
        assert!(!name.is_empty(), "name may not be empty");
        DeviceFontFamilyName(name)
    }

    pub fn name(&self) -> &str {
        &self.0
    }
}

/// A font's bytes.
#[derive(Clone)]
pub enum FontBytes {
    /// In the program (`include_bytes!`), as Compose's resources are in the package.
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
}

impl FontBytes {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            FontBytes::Static(bytes) => bytes,
            FontBytes::Shared(bytes) => bytes,
        }
    }
}

impl PartialEq for FontBytes {
    /// Comparing the same bytes costs nothing, which is nearly always the comparison asked.
    fn eq(&self, other: &Self) -> bool {
        let (first, second) = (self.as_slice(), other.as_slice());
        std::ptr::eq(first, second) || first == second
    }
}

impl fmt::Debug for FontBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} bytes", self.as_slice().len())
    }
}

/// Loads a font that isn't a platform's to find, as Compose's `AndroidFont.TypefaceLoader`: from
/// a server, a font provider, anything. The platform makes the typeface: give it the bytes when
/// they're here ([`Font::from_shared_bytes`] and [`PlatformFontLoader::load_blocking`]).
///
/// A load starts inside a text's layout, under
/// [`SharedTextMeasurer`](crate::platform::SharedTextMeasurer)'s borrow: `load_blocking`, and
/// `await_load` up to its first wait, must not measure text, which would panic as a `RefCell`
/// borrowed twice.
pub trait TypefaceLoader {
    /// For a `Blocking` or `OptionalLocal` font. `Err` is Compose's exception.
    fn load_blocking(
        &self,
        platform: &dyn PlatformFontLoader,
        font: &Font,
    ) -> Result<Option<Typeface>, String>;
    /// For an `Async` font.
    fn await_load(
        self: Rc<Self>,
        platform: Rc<dyn PlatformFontLoader>,
        font: Font,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Typeface>, String>>>>;
}

/// Where a font comes from.
#[derive(Clone)]
pub enum FontSource {
    /// Compose's `Font(resId)`: a resource is its bytes here (D1119).
    Bytes(FontBytes),
    /// Compose's `Font(file)`.
    File(Rc<std::path::Path>),
    /// Compose's `Font(path, assetManager)`, on Android; on the desktop, a file.
    Asset(crate::text::SharedString),
    /// Compose's `Font(DeviceFontFamilyName)`.
    DeviceFamily(DeviceFontFamilyName),
    /// Compose's `AndroidFont` of an application: compared by its loader.
    Loader(Rc<dyn TypefaceLoader>),
}

impl PartialEq for FontSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FontSource::Bytes(first), FontSource::Bytes(second)) => first == second,
            (FontSource::File(first), FontSource::File(second)) => first == second,
            (FontSource::Asset(first), FontSource::Asset(second)) => first == second,
            (FontSource::DeviceFamily(first), FontSource::DeviceFamily(second)) => first == second,
            (FontSource::Loader(first), FontSource::Loader(second)) => Rc::ptr_eq(first, second),
            _ => false,
        }
    }
}

impl fmt::Debug for FontSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontSource::Bytes(bytes) => write!(formatter, "{bytes:?}"),
            FontSource::File(path) => write!(formatter, "file {path:?}"),
            FontSource::Asset(path) => write!(formatter, "asset {path:?}"),
            FontSource::DeviceFamily(name) => write!(formatter, "device family {:?}", name.name()),
            FontSource::Loader(loader) => write!(formatter, "loader {:p}", Rc::as_ptr(loader)),
        }
    }
}

/// What a [`Font`] may leave out, as the default parameters of Compose's `Font(…)`.
#[derive(Clone, Debug)]
pub struct FontOptions {
    pub weight: FontWeight,
    pub style: FontStyle,
    /// `None` is the source's default: `Blocking`, but `OptionalLocal` for a device family.
    pub loading_strategy: Option<FontLoadingStrategy>,
    /// `None` is the weight's and style's, as Compose's defaults; none for a device family.
    pub variation_settings: Option<FontVariationSettings>,
}

impl Default for FontOptions {
    fn default() -> Self {
        FontOptions {
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
            loading_strategy: None,
            variation_settings: None,
        }
    }
}

/// A font of a [`FontFamily`], as Compose's `Font`. Cloning shares what it comes from.
#[derive(Clone, Debug)]
pub struct Font {
    source: FontSource,
    weight: FontWeight,
    style: FontStyle,
    loading_strategy: FontLoadingStrategy,
    variation_settings: FontVariationSettings,
}

impl Font {
    /// How long an `Async` font is waited for, as `Font.MaximumAsyncTimeoutMillis`.
    pub const MAXIMUM_ASYNC_TIMEOUT_MILLIS: u64 = 15_000;

    fn from_source(source: FontSource, options: FontOptions) -> Font {
        let device = matches!(source, FontSource::DeviceFamily(_));
        let variation_settings = options.variation_settings.unwrap_or_else(|| {
            if device {
                FontVariation::empty()
            } else {
                FontVariation::settings(options.weight, options.style, [])
            }
        });
        let default_strategy =
            if device { FontLoadingStrategy::OptionalLocal } else { FontLoadingStrategy::Blocking };
        Font {
            source,
            weight: options.weight,
            style: options.style,
            loading_strategy: options.loading_strategy.unwrap_or(default_strategy),
            variation_settings,
        }
    }

    /// A font of the program's bytes (`include_bytes!`), as Compose's `Font(resId)`.
    pub fn from_bytes(bytes: &'static [u8], options: FontOptions) -> Font {
        Font::from_source(FontSource::Bytes(FontBytes::Static(bytes)), options)
    }

    /// A font of bytes the application got, as Compose's `Font(ParcelFileDescriptor)`.
    pub fn from_shared_bytes(bytes: Arc<[u8]>, options: FontOptions) -> Font {
        Font::from_source(FontSource::Bytes(FontBytes::Shared(bytes)), options)
    }

    /// A font of a file, as Compose's `Font(File)`.
    pub fn from_file(path: impl AsRef<std::path::Path>, options: FontOptions) -> Font {
        Font::from_source(FontSource::File(Rc::from(path.as_ref())), options)
    }

    /// A font of an asset, as Compose's `Font(path, assetManager)`: of the package's assets on
    /// Android, a file on the desktop.
    pub fn from_asset(path: impl Into<crate::text::SharedString>, options: FontOptions) -> Font {
        Font::from_source(FontSource::Asset(path.into()), options)
    }

    /// A font of a family on the device, as Compose's `Font(DeviceFontFamilyName)`: missing, it's
    /// skipped for the next.
    pub fn from_device_family(name: DeviceFontFamilyName, options: FontOptions) -> Font {
        Font::from_source(FontSource::DeviceFamily(name), options)
    }

    /// A font `loader` loads, as a subclass of Compose's `AndroidFont`.
    pub fn from_loader(loader: Rc<dyn TypefaceLoader>, options: FontOptions) -> Font {
        Font::from_source(FontSource::Loader(loader), options)
    }

    pub fn source(&self) -> &FontSource {
        &self.source
    }

    pub fn weight(&self) -> FontWeight {
        self.weight
    }

    pub fn style(&self) -> FontStyle {
        self.style
    }

    pub fn loading_strategy(&self) -> FontLoadingStrategy {
        self.loading_strategy
    }

    pub fn variation_settings(&self) -> &FontVariationSettings {
        &self.variation_settings
    }

    /// A family of this font, as Compose's `Font.toFontFamily`.
    pub fn to_font_family(&self) -> FontFamily {
        FontFamily::of([self.clone()])
    }
}

impl PartialEq for Font {
    /// The cheap fields first: a family's fonts mostly differ there, and bytes are long.
    fn eq(&self, other: &Self) -> bool {
        self.weight == other.weight
            && self.style == other.style
            && self.loading_strategy == other.loading_strategy
            && self.variation_settings == other.variation_settings
            && self.source == other.source
    }
}

impl std::hash::Hash for Font {
    /// Equal fonts hash alike: bytes by their length, so a key costs no pass over a font's data.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match &self.source {
            FontSource::Bytes(bytes) => bytes.as_slice().len().hash(state),
            FontSource::File(path) => path.hash(state),
            FontSource::Asset(path) => path.hash(state),
            FontSource::DeviceFamily(name) => name.hash(state),
            FontSource::Loader(loader) => Rc::as_ptr(loader).cast::<()>().hash(state),
        }
        self.weight.hash(state);
        self.style.hash(state);
        self.loading_strategy.hash(state);
        self.variation_settings.hash(state);
    }
}

/// How a family's fonts are chosen for a weight and a style, as Compose's `FontMatcher`: CSS's
/// font matching (css-fonts-4).
pub struct FontMatcher;

impl FontMatcher {
    /// The fonts of `fonts` for `weight` and `style`, in order: the first that loads is used.
    pub fn match_font(fonts: &[Font], weight: FontWeight, style: FontStyle) -> Vec<Font> {
        let exact: Vec<Font> = fonts
            .iter()
            .filter(|font| font.weight == weight && font.style == style)
            .cloned()
            .collect();
        if !exact.is_empty() {
            return exact;
        }
        let of_style: Vec<Font> =
            fonts.iter().filter(|font| font.style == style).cloned().collect();
        let search = if of_style.is_empty() { fonts } else { &of_style };
        if weight < FontWeight::W400 {
            // Lighter ones, the closest first, then heavier ones.
            FontMatcher::filter_by_closest_weight(search, weight, true, None, None)
        } else if weight > FontWeight::W500 {
            // Heavier ones, the closest first, then lighter ones.
            FontMatcher::filter_by_closest_weight(search, weight, false, None, None)
        } else {
            // Up to 500 first, then lighter ones, then those above 500.
            let found = FontMatcher::filter_by_closest_weight(
                search,
                weight,
                false,
                None,
                Some(FontWeight::W500),
            );
            if found.is_empty() {
                FontMatcher::filter_by_closest_weight(
                    search,
                    weight,
                    false,
                    Some(FontWeight::W500),
                    None,
                )
            } else {
                found
            }
        }
    }

    /// The fonts of `fonts` of the weight closest to `weight` among those in the search range,
    /// above or below it by preference, as Compose's `filterByClosestWeight`.
    pub fn filter_by_closest_weight(
        fonts: &[Font],
        weight: FontWeight,
        prefer_below: bool,
        min_search_range: Option<FontWeight>,
        max_search_range: Option<FontWeight>,
    ) -> Vec<Font> {
        let mut above: Option<FontWeight> = None;
        let mut below: Option<FontWeight> = None;
        for font in fonts {
            let possible = font.weight;
            if min_search_range.is_some_and(|min| possible < min)
                || max_search_range.is_some_and(|max| possible > max)
            {
                continue;
            }
            if possible < weight {
                if below.is_none_or(|best| possible > best) {
                    below = Some(possible);
                }
            } else if possible > weight {
                if above.is_none_or(|best| possible < best) {
                    above = Some(possible);
                }
            } else {
                above = Some(possible);
                below = Some(possible);
                break;
            }
        }
        let best = if prefer_below { below.or(above) } else { above.or(below) };
        fonts.iter().filter(|font| Some(font.weight) == best).cloned().collect()
    }
}

/// The platform's fonts, as Compose's internal `PlatformFontLoader` and its typefaces: what a text
/// stack gives [`FontFamilyResolver`] to load with.
pub trait PlatformFontLoader {
    /// `font`'s typeface now: `None` if it isn't there, `Err` for what Compose has as an
    /// exception. Never asked for an `Async` font's unless it comes as bytes for one.
    fn load_blocking(&self, font: &Font) -> Result<Option<Typeface>, String>;
    /// `font`'s typeface later, for an `Async` font. The default is `load_blocking`'s.
    fn await_load(
        self: Rc<Self>,
        font: Font,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Typeface>, String>>>> {
        let result = self.load_blocking(&font);
        Box::pin(std::future::ready(result))
    }
    /// The platform's default family at `weight` and `style`.
    fn create_default(&self, weight: FontWeight, style: FontStyle) -> Typeface;
    /// The generic family `name` at `weight` and `style`.
    fn create_named(&self, name: &str, weight: FontWeight, style: FontStyle) -> Typeface;
    /// Gives the platform back what the typefaces dropped since held: a paragraph in fonts was
    /// released, or a load ended. Nothing by default.
    fn release_dropped(&self) {}
    /// `typeface` faked to `weight` and italic, as Compose's `synthesizeTypeface`. The default
    /// leaves it (the desktop, D1121).
    fn synthesize(&self, typeface: &Typeface, _weight: FontWeight, _italic: bool) -> Typeface {
        typeface.clone()
    }
}

/// `typeface` of `font`, faked to the weight and style asked as far as `synthesis` lets it, as
/// Compose's Android `synthesizeTypeface`: a weight only from below 600 to 600 or more.
pub(crate) fn synthesize_typeface(
    loader: &dyn PlatformFontLoader,
    synthesis: FontSynthesis,
    typeface: &Typeface,
    font: &Font,
    requested_weight: FontWeight,
    requested_style: FontStyle,
) -> Typeface {
    let bold = FontWeight::W600;
    let weight = synthesis.is_weight_on()
        && font.weight != requested_weight
        && requested_weight >= bold
        && font.weight < bold;
    let style = synthesis.is_style_on() && requested_style != font.style;
    if !weight && !style {
        return typeface.clone();
    }
    let final_weight = if weight { requested_weight } else { font.weight };
    let final_style = if style { requested_style } else { font.style };
    loader.synthesize(typeface, final_weight, final_style == FontStyle::Italic)
}

/// Whether the platform may fake a bold or italic face the font family lacks, as Compose's
/// `FontSynthesis`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FontSynthesis {
    /// Never.
    None,
    /// Bold and italic, the default.
    All,
    /// Bold only.
    Weight,
    /// Italic only.
    Style,
}

impl FontSynthesis {
    /// Whether a missing bold face may be faked, as Compose's `isWeightOn`.
    pub const fn is_weight_on(self) -> bool {
        matches!(self, FontSynthesis::All | FontSynthesis::Weight)
    }

    /// Whether a missing italic face may be faked, as Compose's `isStyleOn`.
    pub const fn is_style_on(self) -> bool {
        matches!(self, FontSynthesis::All | FontSynthesis::Style)
    }
}

/// Resolves the fonts of a text's families, as Compose's `FontFamily.Resolver`, and with them
/// reaches the platform's text stack: what a [`TextMeasurer`](crate::TextMeasurer) lays out with,
/// and the host provides as [`LOCAL_FONT_FAMILY_RESOLVER`]. Compared by identity, as Compose's
/// layout cache compares it; cloning shares it.
#[derive(Clone)]
pub struct FontFamilyResolver(Rc<ResolverInner>);

struct ResolverInner {
    platform: crate::text::SharedTextMeasurer,
    fonts: Rc<crate::font_resolver::PlatformFonts>,
}

impl FontFamilyResolver {
    /// The platform's text stack, which lays out and draws what's resolved.
    pub(crate) fn platform(&self) -> &crate::text::SharedTextMeasurer {
        &self.0.platform
    }

    /// `family` (the default one if none) at `weight` and `style`, faked as far as `synthesis`
    /// lets it where it lacks them, as Compose's `resolve`.
    pub fn resolve(
        &self,
        family: Option<&FontFamily>,
        weight: FontWeight,
        style: FontStyle,
        synthesis: FontSynthesis,
    ) -> crate::font_resolver::TypefaceResult {
        self.0.fonts.resolve(family, weight, style, synthesis)
    }

    /// Loads `family`'s `Async` fonts, so a first layout in them has them, as Compose's `preload`.
    /// `Err` if one of them doesn't load, in its time, where Compose's throws.
    pub fn preload(&self, family: &FontFamily) -> impl Future<Output = Result<(), String>> + use<> {
        self.0.fonts.clone().preload(family.clone())
    }
}

/// A new resolver over `platform`, as Compose's `createFontFamilyResolver`.
pub fn create_font_family_resolver(
    platform: &crate::text::SharedTextMeasurer,
) -> FontFamilyResolver {
    let fonts = platform.borrow().fonts().unwrap_or_else(|| {
        crate::font_resolver::PlatformFonts::new(Rc::new(crate::font_resolver::NoFontLoader))
    });
    FontFamilyResolver(Rc::new(ResolverInner { platform: platform.clone(), fonts }))
}

impl PartialEq for FontFamilyResolver {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for FontFamilyResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "FontFamilyResolver({:p})", Rc::as_ptr(&self.0))
    }
}

/// The host's font family resolver, as Compose's `LocalFontFamilyResolver`.
pub static LOCAL_FONT_FAMILY_RESOLVER: compose_runtime::CompositionLocal<FontFamilyResolver> =
    compose_runtime::CompositionLocal::new_static("LocalFontFamilyResolver", || {
        panic!("CompositionLocal LocalFontFamilyResolver not present: a host provides it")
    });

/// The tree's text stack, where a node is composed: what its draw scope's renderer draws, whatever
/// [`LOCAL_FONT_FAMILY_RESOLVER`] the content provides. Crate-private so nothing can provide
/// another, and a local so a draw scope pays for it only in `draw_text`.
pub(crate) static LOCAL_TREE_TEXT_MEASURER: compose_runtime::CompositionLocal<
    crate::text::SharedTextMeasurer,
> = compose_runtime::CompositionLocal::new_static("LocalTreeTextMeasurer", || {
    panic!("CompositionLocal LocalTreeTextMeasurer not present: a host provides it")
});

macro_rules! compose_arg_by_eq {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

compose_arg_by_eq!(FontStyle, FontFamily, FontSynthesis, FontFamilyResolver);
