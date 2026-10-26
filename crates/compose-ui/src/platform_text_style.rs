//! A text style's platform part, as Compose's Android `PlatformTextStyle`,
//! `PlatformParagraphStyle`, `PlatformSpanStyle` and `EmojiSupportMatch`: the model, with
//! Compose's merge and lerp. compose-rs's measurers never include the font padding.

use compose_runtime::ComposeArg;

use crate::text_attributes::lerp_discrete;

/// Which emoji EmojiCompat replaces, as Compose's `EmojiSupportMatch`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum EmojiSupportMatch {
    /// The ones the platform's font can't draw.
    #[default]
    Default,
    /// None.
    None,
    /// All of them.
    All,
}

/// A paragraph style's platform part, as Compose's Android `PlatformParagraphStyle`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PlatformParagraphStyle {
    /// Extra space for the font's tallest glyphs above and below lines (false by default).
    pub include_font_padding: bool,
    pub emoji_support_match: EmojiSupportMatch,
}

impl PlatformParagraphStyle {
    /// As `PlatformParagraphStyle.Default`.
    pub const DEFAULT: PlatformParagraphStyle = PlatformParagraphStyle {
        include_font_padding: false,
        emoji_support_match: EmojiSupportMatch::Default,
    };

    /// `other` if there's one, as Compose's `merge` (Android's takes `other` whole).
    pub fn merge(self, other: Option<PlatformParagraphStyle>) -> PlatformParagraphStyle {
        other.unwrap_or(self)
    }

    /// Between `start` and `stop`, as Compose's: `start` when their font paddings agree, else
    /// each field discretely.
    pub fn lerp(
        start: PlatformParagraphStyle,
        stop: PlatformParagraphStyle,
        fraction: f32,
    ) -> PlatformParagraphStyle {
        if start.include_font_padding == stop.include_font_padding {
            return start;
        }
        PlatformParagraphStyle {
            emoji_support_match: lerp_discrete(
                start.emoji_support_match,
                stop.emoji_support_match,
                fraction,
            ),
            include_font_padding: lerp_discrete(
                start.include_font_padding,
                stop.include_font_padding,
                fraction,
            ),
        }
    }
}

/// A span style's platform part, as Compose's Android `PlatformSpanStyle` (which holds
/// nothing).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PlatformSpanStyle;

impl PlatformSpanStyle {
    /// As `PlatformSpanStyle.Default`.
    pub const DEFAULT: PlatformSpanStyle = PlatformSpanStyle;

    /// As Compose's Android `merge`: this one.
    pub fn merge(self, _other: Option<PlatformSpanStyle>) -> PlatformSpanStyle {
        self
    }

    /// As Compose's Android `lerp`: `start`.
    pub fn lerp(start: PlatformSpanStyle, _stop: PlatformSpanStyle, _: f32) -> PlatformSpanStyle {
        start
    }
}

/// A text style's platform part, as Compose's Android `PlatformTextStyle`: its span and
/// paragraph parts.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PlatformTextStyle {
    pub span_style: Option<PlatformSpanStyle>,
    pub paragraph_style: Option<PlatformParagraphStyle>,
}

impl PlatformTextStyle {
    /// As `PlatformTextStyle(includeFontPadding)`.
    pub fn with_include_font_padding(include_font_padding: bool) -> PlatformTextStyle {
        PlatformTextStyle {
            span_style: None,
            paragraph_style: Some(PlatformParagraphStyle {
                include_font_padding,
                ..PlatformParagraphStyle::DEFAULT
            }),
        }
    }

    /// As `PlatformTextStyle(emojiSupportMatch)`.
    pub fn with_emoji_support_match(emoji_support_match: EmojiSupportMatch) -> PlatformTextStyle {
        PlatformTextStyle {
            span_style: None,
            paragraph_style: Some(PlatformParagraphStyle {
                emoji_support_match,
                ..PlatformParagraphStyle::DEFAULT
            }),
        }
    }

    /// The style for a span part and a paragraph part, `None` without either, as Compose's
    /// `createPlatformTextStyleInternal`.
    pub(crate) fn of(
        span_style: Option<PlatformSpanStyle>,
        paragraph_style: Option<PlatformParagraphStyle>,
    ) -> Option<PlatformTextStyle> {
        (span_style.is_some() || paragraph_style.is_some())
            .then_some(PlatformTextStyle { span_style, paragraph_style })
    }
}

/// `start` and `stop`'s merge for styles holding an optional part: either alone, or both
/// merged.
pub(crate) fn merge_platform_paragraph(
    this: Option<PlatformParagraphStyle>,
    other: Option<PlatformParagraphStyle>,
) -> Option<PlatformParagraphStyle> {
    match (this, other) {
        (None, other) => other,
        (this, None) => this,
        (Some(this), other) => Some(this.merge(other)),
    }
}

pub(crate) fn merge_platform_span(
    this: Option<PlatformSpanStyle>,
    other: Option<PlatformSpanStyle>,
) -> Option<PlatformSpanStyle> {
    match (this, other) {
        (None, other) => other,
        (this, None) => this,
        (Some(this), other) => Some(this.merge(other)),
    }
}

/// Between two optional parts: none for two nones, else the parts (or the default) lerped.
pub(crate) fn lerp_platform_paragraph(
    start: Option<PlatformParagraphStyle>,
    stop: Option<PlatformParagraphStyle>,
    fraction: f32,
) -> Option<PlatformParagraphStyle> {
    if start.is_none() && stop.is_none() {
        return None;
    }
    Some(PlatformParagraphStyle::lerp(
        start.unwrap_or(PlatformParagraphStyle::DEFAULT),
        stop.unwrap_or(PlatformParagraphStyle::DEFAULT),
        fraction,
    ))
}

pub(crate) fn lerp_platform_span(
    start: Option<PlatformSpanStyle>,
    stop: Option<PlatformSpanStyle>,
    fraction: f32,
) -> Option<PlatformSpanStyle> {
    if start.is_none() && stop.is_none() {
        return None;
    }
    Some(PlatformSpanStyle::lerp(
        start.unwrap_or(PlatformSpanStyle::DEFAULT),
        stop.unwrap_or(PlatformSpanStyle::DEFAULT),
        fraction,
    ))
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

compose_arg_by_eq!(EmojiSupportMatch, PlatformParagraphStyle, PlatformSpanStyle, PlatformTextStyle);
