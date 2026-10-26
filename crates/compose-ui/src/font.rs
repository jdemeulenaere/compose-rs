//! Fonts' attributes, as Compose's `androidx.compose.ui.text.font`: weight, style, family and
//! synthesis.

use std::fmt;

use compose_runtime::ComposeArg;

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

/// A font family, as `FontFamily`: the generic ones, or a family installed on the system by
/// name (falling back to the default).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FontFamily {
    SansSerif,
    Serif,
    Monospace,
    Cursive,
    Named(&'static str),
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

macro_rules! compose_arg_by_eq {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

compose_arg_by_eq!(FontStyle, FontFamily, FontSynthesis);
