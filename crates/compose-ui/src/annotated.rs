//! Styled text, as Compose's `AnnotatedString`: a string with span styles over byte ranges of
//! it, built with `AnnotatedString::build`.

use std::ops::Range;
use std::sync::Arc;

use compose_runtime::ComposeArg;

use crate::graphics::Color;
use crate::span_style::SpanStyle;
use crate::text::SharedString;
use crate::text_style::TextStyle;
use crate::unit::TextUnit;

/// A span style over bytes `start..end` of a text, as `AnnotatedString.Range<SpanStyle>`.
#[derive(Clone, PartialEq, Debug)]
pub struct SpanRange {
    pub style: SpanStyle,
    pub start: usize,
    pub end: usize,
}

/// A string with styled spans, as Compose's `AnnotatedString`. Cloning shares both.
#[derive(Clone, PartialEq, Debug)]
pub struct AnnotatedString {
    pub text: SharedString,
    /// In the order they were added: a later one's set fields win where they overlap.
    pub span_styles: Arc<[SpanRange]>,
}

impl AnnotatedString {
    /// Plain text, no spans.
    pub fn new(text: impl Into<SharedString>) -> AnnotatedString {
        AnnotatedString { text: text.into(), span_styles: Arc::from([]) }
    }

    /// Builds one, as `buildAnnotatedString { … }`.
    pub fn build(build: impl FnOnce(&mut AnnotatedStringBuilder)) -> AnnotatedString {
        let mut builder = AnnotatedStringBuilder::default();
        build(&mut builder);
        builder.to_annotated_string()
    }

    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }

    /// The text's length in bytes, as Compose's `length` (in UTF-16 units there: D1100).
    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Bytes `start..end` of the text with the styles over them, as Compose's `subSequence`: a
    /// range intersecting them is cut to them and moved with them (an empty range at `start`
    /// is kept). Panics if `start > end` or an end isn't at a character boundary.
    pub fn sub_sequence(&self, start: usize, end: usize) -> AnnotatedString {
        assert!(start <= end, "start ({start}) should be less or equal to end ({end})");
        if start == 0 && end == self.text.len() {
            return self.clone();
        }
        let text = &self.text.as_str()[start..end];
        let spans: Vec<SpanRange> = self
            .span_styles
            .iter()
            .filter(|span| intersect(start, end, span.start, span.end))
            .map(|span| SpanRange {
                style: span.style.clone(),
                start: start.max(span.start) - start,
                end: end.min(span.end) - start,
            })
            .collect();
        AnnotatedString { text: text.to_string().into(), span_styles: spans.into() }
    }

    /// The runs of the text with the same spans: each byte range with the style it's drawn in
    /// (the spans covering it over `style`, in their order) and its background. Covers the
    /// whole text.
    pub fn runs(&self, style: &TextStyle) -> Vec<(Range<usize>, TextStyle, Color)> {
        let text = self.text.as_str();
        let length = text.len();
        // In the text, at character boundaries (a span ending inside a character ends before
        // it).
        let bound = |offset: usize| {
            let mut offset = offset.min(length);
            while !text.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        let mut bounds: Vec<usize> = vec![0, length];
        for span in self.span_styles.iter() {
            bounds.push(bound(span.start));
            bounds.push(bound(span.end));
        }
        bounds.sort_unstable();
        bounds.dedup();
        bounds
            .windows(2)
            .filter(|pair| pair[0] < pair[1])
            .map(|pair| {
                let range = pair[0]..pair[1];
                let covering = || {
                    self.span_styles.iter().filter(|span| {
                        bound(span.start) <= range.start && range.end <= bound(span.end)
                    })
                };
                let style = covering().fold(style.clone(), |style, span| {
                    style.merge_span_style(&relative_to(&span.style, &style))
                });
                let background = covering().fold(Color::UNSPECIFIED, |background, span| {
                    span.style.background.or(background)
                });
                (range, style, background)
            })
            .collect()
    }
}

/// Whether `l_start..l_end` and `r_start..r_end` intersect, as Compose's `intersect`: ranges are
/// closed at their start and open at their end, and an empty range intersects a range starting
/// at the same offset.
pub(crate) fn intersect(l_start: usize, l_end: usize, r_start: usize, r_end: usize) -> bool {
    ((l_start == l_end || r_start == r_end) && l_start == r_start)
        || (l_start < r_end && r_start < l_end)
}

/// `span` with an `em` font size made relative to `style`'s, as Android's `RelativeSizeSpan`
/// scales the size it's applied over (an unset size being 14 sp).
fn relative_to<'a>(span: &'a SpanStyle, style: &TextStyle) -> std::borrow::Cow<'a, SpanStyle> {
    if !span.font_size.is_em() {
        return std::borrow::Cow::Borrowed(span);
    }
    let base = style.font_size;
    let font_size = if base.is_sp() || base.is_em() {
        TextUnit::new(base.value() * span.font_size.value(), base.unit_type())
    } else {
        crate::text::DEFAULT_FONT_SIZE * span.font_size.value()
    };
    std::borrow::Cow::Owned(SpanStyle { font_size, ..span.clone() })
}

impl Default for AnnotatedString {
    fn default() -> Self {
        AnnotatedString::new("")
    }
}

impl From<&str> for AnnotatedString {
    fn from(text: &str) -> AnnotatedString {
        AnnotatedString::new(text.to_string())
    }
}

impl From<String> for AnnotatedString {
    fn from(text: String) -> AnnotatedString {
        AnnotatedString::new(text)
    }
}

impl From<SharedString> for AnnotatedString {
    fn from(text: SharedString) -> AnnotatedString {
        AnnotatedString::new(text)
    }
}

impl ComposeArg for AnnotatedString {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Builds an `AnnotatedString`, as Compose's `AnnotatedString.Builder`.
#[derive(Default)]
pub struct AnnotatedStringBuilder {
    text: String,
    spans: Vec<SpanRange>,
    /// The spans pushed and not popped yet: indices into `spans`, ended when popped.
    open: Vec<usize>,
}

impl AnnotatedStringBuilder {
    /// Appends text, in the styles pushed.
    pub fn append(&mut self, text: &str) -> &mut Self {
        self.text.push_str(text);
        self
    }

    /// Styles what's appended until the matching `pop`, as `pushStyle`.
    pub fn push_style(&mut self, style: SpanStyle) -> &mut Self {
        let start = self.text.len();
        self.open.push(self.spans.len());
        self.spans.push(SpanRange { style, start, end: start });
        self
    }

    /// Ends the last style pushed, as `pop`.
    pub fn pop(&mut self) -> &mut Self {
        if let Some(index) = self.open.pop() {
            self.spans[index].end = self.text.len();
        }
        self
    }

    /// Styles what `build` appends, as `withStyle`.
    pub fn with_style(&mut self, style: SpanStyle, build: impl FnOnce(&mut Self)) -> &mut Self {
        self.push_style(style);
        build(self);
        self.pop()
    }

    /// Styles bytes `start..end` of the text, as `addStyle`.
    pub fn add_style(&mut self, style: SpanStyle, start: usize, end: usize) -> &mut Self {
        self.spans.push(SpanRange { style, start, end });
        self
    }

    /// The text so far: styles still pushed end at its end.
    pub fn to_annotated_string(&self) -> AnnotatedString {
        let end = self.text.len();
        let mut spans = self.spans.clone();
        for &index in &self.open {
            spans[index].end = end;
        }
        spans.retain(|span| span.start < span.end);
        AnnotatedString { text: self.text.clone().into(), span_styles: spans.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FontWeight;

    #[test]
    fn spans_split_the_text_into_runs_in_their_styles() {
        let bold = SpanStyle { font_weight: Some(FontWeight::BOLD), ..SpanStyle::DEFAULT };
        let red = SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT };
        let text = AnnotatedString::build(|builder| {
            builder.append("Hello ");
            builder.with_style(bold, |builder| {
                builder.append("bold ");
                builder.with_style(red, |builder| {
                    builder.append("red");
                });
            });
            builder.append("!");
        });
        assert_eq!(text.as_str(), "Hello bold red!");
        let runs = text.runs(&TextStyle::DEFAULT);
        let ranges: Vec<_> = runs.iter().map(|(range, _, _)| range.clone()).collect();
        assert_eq!(ranges, vec![0..6, 6..11, 11..14, 14..15]);
        assert_eq!(runs[0].1, TextStyle::DEFAULT);
        assert_eq!(runs[1].1.font_weight, Some(FontWeight::BOLD));
        assert_eq!((runs[2].1.font_weight, runs[2].1.color), (Some(FontWeight::BOLD), Color::RED));
        assert_eq!(runs[3].1, TextStyle::DEFAULT);
    }
}
