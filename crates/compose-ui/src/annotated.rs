//! Styled text, as Compose's `AnnotatedString`: a string with annotations over byte ranges of
//! it (span and paragraph styles, string, text-to-speech and link annotations, bullets), built
//! with `AnnotatedString::build` (Compose's `buildAnnotatedString`).
//!
//! Ranges are UTF-8 byte offsets (D140, D1100). Annotations keep the order they were added in:
//! a later span style's set fields win where spans overlap.

use std::fmt;
use std::ops::Range;
use std::rc::Rc;

use compose_runtime::ComposeArg;

use crate::brush::Brush;
use crate::graphics::{Color, CornerSize, DrawStyle, Shape};
use crate::paragraph_style::ParagraphStyle;
use crate::span_style::SpanStyle;
use crate::text::SharedString;
use crate::text_attributes::TextIndent;
use crate::text_style::TextStyle;
use crate::unit::{TextUnit, TextUnitType};

/// An item over bytes `start..end` of a text, with a tag (empty unless set), as Compose's
/// `AnnotatedString.Range`. `start` may equal `end` (an empty range), not exceed it.
#[derive(Clone, PartialEq, Debug)]
pub struct AnnotatedRange<T> {
    pub item: T,
    pub start: usize,
    pub end: usize,
    pub tag: SharedString,
}

impl<T> AnnotatedRange<T> {
    /// As `Range(item, start, end)`: no tag. Panics if `start > end` ("Reversed range is not
    /// supported", as Compose's).
    pub fn new(item: T, start: usize, end: usize) -> AnnotatedRange<T> {
        AnnotatedRange::tagged(item, start, end, SharedString::EMPTY)
    }

    /// As `Range(item, start, end, tag)`.
    pub fn tagged(
        item: T,
        start: usize,
        end: usize,
        tag: impl Into<SharedString>,
    ) -> AnnotatedRange<T> {
        assert!(start <= end, "Reversed range is not supported");
        AnnotatedRange { item, start, end, tag: tag.into() }
    }

    fn with_item<U>(&self, item: U) -> AnnotatedRange<U> {
        AnnotatedRange { item, start: self.start, end: self.end, tag: self.tag.clone() }
    }
}

/// A span style over a range, as `AnnotatedString.Range<SpanStyle>`.
pub type SpanRange = AnnotatedRange<SpanStyle>;

/// A string annotation's value, as Compose's `StringAnnotation` (its tag is the range's).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct StringAnnotation(pub SharedString);

/// What a text-to-speech engine reads a range as, as Compose's `TtsAnnotation`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TtsAnnotation {
    /// Read character by character, as Compose's `VerbatimTtsAnnotation`.
    Verbatim(SharedString),
}

/// Called when a link is clicked, as Compose's `LinkInteractionListener`. Compared by pointer.
#[derive(Clone)]
pub struct LinkInteractionListener(Rc<dyn Fn(&LinkAnnotation)>);

impl LinkInteractionListener {
    pub fn new(on_click: impl Fn(&LinkAnnotation) + 'static) -> LinkInteractionListener {
        LinkInteractionListener(Rc::new(on_click))
    }

    /// Calls the listener, as Compose's `onClick`.
    pub fn on_click(&self, link: &LinkAnnotation) {
        (self.0)(link)
    }
}

impl PartialEq for LinkInteractionListener {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for LinkInteractionListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LinkInteractionListener")
    }
}

/// How a link looks in each of its states, as Compose's `TextLinkStyles`: each set style
/// merges over the one before (focused, then hovered, then pressed).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct TextLinkStyles {
    pub style: Option<SpanStyle>,
    pub focused_style: Option<SpanStyle>,
    pub hovered_style: Option<SpanStyle>,
    pub pressed_style: Option<SpanStyle>,
}

/// A link over a range, as Compose's `LinkAnnotation`: a URL the platform opens, or a
/// clickable the listener handles. Foundation's text makes links clickable and styles them.
#[derive(Clone, PartialEq, Debug)]
pub enum LinkAnnotation {
    /// As `LinkAnnotation.Url`: opened with the URI handler unless a listener is set.
    Url {
        url: SharedString,
        styles: Option<TextLinkStyles>,
        link_interaction_listener: Option<LinkInteractionListener>,
    },
    /// As `LinkAnnotation.Clickable`: the listener handles it, by tag.
    Clickable {
        tag: SharedString,
        styles: Option<TextLinkStyles>,
        link_interaction_listener: Option<LinkInteractionListener>,
    },
}

impl LinkAnnotation {
    /// A URL link, as `LinkAnnotation.Url(url)`.
    pub fn url(url: impl Into<SharedString>) -> LinkAnnotation {
        LinkAnnotation::Url { url: url.into(), styles: None, link_interaction_listener: None }
    }

    /// A clickable link, as `LinkAnnotation.Clickable(tag, linkInteractionListener = …)`.
    pub fn clickable(
        tag: impl Into<SharedString>,
        link_interaction_listener: Option<LinkInteractionListener>,
    ) -> LinkAnnotation {
        LinkAnnotation::Clickable { tag: tag.into(), styles: None, link_interaction_listener }
    }

    /// Its styles, as Compose's `styles`.
    pub fn styles(&self) -> Option<&TextLinkStyles> {
        match self {
            LinkAnnotation::Url { styles, .. } | LinkAnnotation::Clickable { styles, .. } => {
                styles.as_ref()
            }
        }
    }

    /// Its listener, as Compose's `linkInteractionListener`.
    pub fn link_interaction_listener(&self) -> Option<&LinkInteractionListener> {
        match self {
            LinkAnnotation::Url { link_interaction_listener, .. }
            | LinkAnnotation::Clickable { link_interaction_listener, .. } => {
                link_interaction_listener.as_ref()
            }
        }
    }

    /// A copy with other styles, as `copy(styles = …)`.
    pub fn with_styles(&self, styles: Option<TextLinkStyles>) -> LinkAnnotation {
        let mut link = self.clone();
        match &mut link {
            LinkAnnotation::Url { styles: own, .. }
            | LinkAnnotation::Clickable { styles: own, .. } => *own = styles,
        }
        link
    }

    /// A copy with another listener, as `copy(linkInteractionListener = …)`.
    pub fn with_link_interaction_listener(
        &self,
        listener: Option<LinkInteractionListener>,
    ) -> LinkAnnotation {
        let mut link = self.clone();
        match &mut link {
            LinkAnnotation::Url { link_interaction_listener, .. }
            | LinkAnnotation::Clickable { link_interaction_listener, .. } => {
                *link_interaction_listener = listener
            }
        }
        link
    }
}

/// A bullet drawn before a paragraph, as Compose's `Bullet`: its shape and size, its gap to the
/// text, and its paint (the text's when no brush). The model; foundation's text draws it.
/// Bullets compare their alphas by bits (NaN, the default, equals NaN).
#[derive(Clone, Debug)]
pub struct Bullet {
    pub shape: Shape,
    pub width: TextUnit,
    pub height: TextUnit,
    pub padding: TextUnit,
    pub brush: Option<Brush>,
    pub alpha: f32,
    pub draw_style: DrawStyle,
}

impl Bullet {
    /// As `Bullet.DefaultIndentation`: 1 em.
    pub const DEFAULT_INDENTATION: TextUnit = TextUnit::em(1.0);
    /// As `Bullet.DefaultSize`: 0.25 em.
    pub const DEFAULT_SIZE: TextUnit = TextUnit::em(0.25);
    /// As `Bullet.DefaultPadding`: 0.25 em.
    pub const DEFAULT_PADDING: TextUnit = TextUnit::em(0.25);

    /// As `Bullet.Default`: a circle of the default size and padding.
    pub fn default_bullet() -> Bullet {
        Bullet {
            shape: Shape::Rounded([CornerSize::Percent(50.0); 4]),
            width: Bullet::DEFAULT_SIZE,
            height: Bullet::DEFAULT_SIZE,
            padding: Bullet::DEFAULT_PADDING,
            brush: None,
            alpha: f32::NAN,
            draw_style: DrawStyle::Fill,
        }
    }
}

impl PartialEq for Bullet {
    fn eq(&self, other: &Self) -> bool {
        self.shape == other.shape
            && self.width == other.width
            && self.height == other.height
            && self.padding == other.padding
            && self.brush == other.brush
            && self.alpha.to_bits() == other.alpha.to_bits()
            && self.draw_style == other.draw_style
    }
}

impl Default for Bullet {
    fn default() -> Self {
        Bullet::default_bullet()
    }
}

/// What a range of an annotated string holds, as Compose's `AnnotatedString.Annotation`
/// subtypes (its deprecated `UrlAnnotation` and the experimental `DrawAnnotation` aside).
#[derive(Clone, PartialEq, Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "span styles are most annotations: boxing them would allocate each"
)]
pub enum Annotation {
    SpanStyle(SpanStyle),
    ParagraphStyle(ParagraphStyle),
    StringAnnotation(StringAnnotation),
    Tts(TtsAnnotation),
    Link(LinkAnnotation),
    Bullet(Bullet),
}

impl From<SpanStyle> for Annotation {
    fn from(style: SpanStyle) -> Annotation {
        Annotation::SpanStyle(style)
    }
}

impl From<ParagraphStyle> for Annotation {
    fn from(style: ParagraphStyle) -> Annotation {
        Annotation::ParagraphStyle(style)
    }
}

impl From<StringAnnotation> for Annotation {
    fn from(annotation: StringAnnotation) -> Annotation {
        Annotation::StringAnnotation(annotation)
    }
}

impl From<TtsAnnotation> for Annotation {
    fn from(annotation: TtsAnnotation) -> Annotation {
        Annotation::Tts(annotation)
    }
}

impl From<LinkAnnotation> for Annotation {
    fn from(link: LinkAnnotation) -> Annotation {
        Annotation::Link(link)
    }
}

impl From<Bullet> for Annotation {
    fn from(bullet: Bullet) -> Annotation {
        Annotation::Bullet(bullet)
    }
}

/// A string with annotations over byte ranges of it, as Compose's `AnnotatedString`. Cloning
/// shares both. Paragraph styles may nest or cover the same range, not partly overlap (made
/// so, the string panics, as Compose's throws).
#[derive(Clone, PartialEq)]
pub struct AnnotatedString {
    text: SharedString,
    annotations: Rc<[AnnotatedRange<Annotation>]>,
}

impl AnnotatedString {
    /// Plain text, no annotations, as `AnnotatedString(text)`.
    pub fn new(text: impl Into<SharedString>) -> AnnotatedString {
        AnnotatedString { text: text.into(), annotations: Rc::from([]) }
    }

    /// `text` with `annotations`, as `AnnotatedString(text, annotations)`. Panics if paragraph
    /// styles partly overlap.
    pub fn with_annotations(
        text: impl Into<SharedString>,
        annotations: impl IntoIterator<Item = AnnotatedRange<Annotation>>,
    ) -> AnnotatedString {
        let annotations: Rc<[AnnotatedRange<Annotation>]> = annotations.into_iter().collect();
        check_paragraph_overlaps(&annotations);
        AnnotatedString { text: text.into(), annotations }
    }

    /// `text` with span styles and paragraph styles, as `AnnotatedString(text, spanStyles,
    /// paragraphStyles)`: the spans first.
    pub fn with_styles(
        text: impl Into<SharedString>,
        span_styles: impl IntoIterator<Item = AnnotatedRange<SpanStyle>>,
        paragraph_styles: impl IntoIterator<Item = AnnotatedRange<ParagraphStyle>>,
    ) -> AnnotatedString {
        let spans = span_styles.into_iter().map(|range| AnnotatedRange {
            item: Annotation::SpanStyle(range.item),
            start: range.start,
            end: range.end,
            tag: range.tag,
        });
        let paragraphs = paragraph_styles.into_iter().map(|range| AnnotatedRange {
            item: Annotation::ParagraphStyle(range.item),
            start: range.start,
            end: range.end,
            tag: range.tag,
        });
        AnnotatedString::with_annotations(text, spans.chain(paragraphs))
    }

    /// The whole text in `span_style` (and `paragraph_style`), as `AnnotatedString(text,
    /// spanStyle, paragraphStyle)`.
    pub fn with_span_style(
        text: impl Into<SharedString>,
        span_style: SpanStyle,
        paragraph_style: Option<ParagraphStyle>,
    ) -> AnnotatedString {
        let text: SharedString = text.into();
        let length = text.len();
        AnnotatedString::with_styles(
            text,
            [AnnotatedRange::new(span_style, 0, length)],
            paragraph_style.map(|style| AnnotatedRange::new(style, 0, length)),
        )
    }

    /// The whole text in `paragraph_style`, as `AnnotatedString(text, paragraphStyle)`.
    pub fn with_paragraph_style(
        text: impl Into<SharedString>,
        paragraph_style: ParagraphStyle,
    ) -> AnnotatedString {
        let text: SharedString = text.into();
        let length = text.len();
        AnnotatedString::with_styles(text, [], [AnnotatedRange::new(paragraph_style, 0, length)])
    }

    /// Builds one, as `buildAnnotatedString { … }`.
    pub fn build(build: impl FnOnce(&mut AnnotatedStringBuilder)) -> AnnotatedString {
        let mut builder = AnnotatedStringBuilder::default();
        build(&mut builder);
        builder.to_annotated_string()
    }

    /// The text, as Compose's `text`.
    pub fn text(&self) -> &SharedString {
        &self.text
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

    /// Every annotation, in the order added, as Compose's (internal) `annotations`.
    pub fn annotations(&self) -> &[AnnotatedRange<Annotation>] {
        &self.annotations
    }

    /// Whether there are span styles: a measurer lays plain text out without runs.
    pub fn has_span_styles(&self) -> bool {
        self.span_style_ranges().next().is_some()
    }

    fn span_style_ranges(&self) -> impl Iterator<Item = (&SpanStyle, usize, usize)> {
        self.annotations.iter().filter_map(|range| match &range.item {
            Annotation::SpanStyle(style) => Some((style, range.start, range.end)),
            _ => None,
        })
    }

    /// The span styles, in the order added, as Compose's `spanStyles`.
    pub fn span_styles(&self) -> Vec<AnnotatedRange<SpanStyle>> {
        self.annotations
            .iter()
            .filter_map(|range| match &range.item {
                Annotation::SpanStyle(style) => Some(range.with_item(style.clone())),
                _ => None,
            })
            .collect()
    }

    /// The paragraph styles, in the order added, as Compose's `paragraphStyles`.
    pub fn paragraph_styles(&self) -> Vec<AnnotatedRange<ParagraphStyle>> {
        self.annotations
            .iter()
            .filter_map(|range| match &range.item {
                Annotation::ParagraphStyle(style) => Some(range.with_item(*style)),
                _ => None,
            })
            .collect()
    }

    fn filter_annotations<T>(
        &self,
        start: usize,
        end: usize,
        mut keep: impl FnMut(&AnnotatedRange<Annotation>) -> Option<T>,
    ) -> Vec<AnnotatedRange<T>> {
        self.annotations
            .iter()
            .filter(|range| intersect(start, end, range.start, range.end))
            .filter_map(|range| keep(range).map(|item| range.with_item(item)))
            .collect()
    }

    /// The string annotations with `tag` intersecting `start..end`, as Compose's
    /// `getStringAnnotations(tag, start, end)`: their values.
    pub fn get_string_annotations_with_tag(
        &self,
        tag: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnnotatedRange<SharedString>> {
        self.filter_annotations(start, end, |range| match &range.item {
            Annotation::StringAnnotation(annotation) if range.tag.as_str() == tag => {
                Some(annotation.0.clone())
            }
            _ => None,
        })
    }

    /// The string annotations intersecting `start..end`, as Compose's
    /// `getStringAnnotations(start, end)`.
    pub fn get_string_annotations(
        &self,
        start: usize,
        end: usize,
    ) -> Vec<AnnotatedRange<SharedString>> {
        self.filter_annotations(start, end, |range| match &range.item {
            Annotation::StringAnnotation(annotation) => Some(annotation.0.clone()),
            _ => None,
        })
    }

    /// Whether a string annotation with `tag` intersects `start..end`, as Compose's
    /// `hasStringAnnotations`.
    pub fn has_string_annotations(&self, tag: &str, start: usize, end: usize) -> bool {
        self.annotations.iter().any(|range| {
            matches!(range.item, Annotation::StringAnnotation(_))
                && range.tag.as_str() == tag
                && intersect(start, end, range.start, range.end)
        })
    }

    /// The text-to-speech annotations intersecting `start..end`, as Compose's
    /// `getTtsAnnotations`.
    pub fn get_tts_annotations(
        &self,
        start: usize,
        end: usize,
    ) -> Vec<AnnotatedRange<TtsAnnotation>> {
        self.filter_annotations(start, end, |range| match &range.item {
            Annotation::Tts(annotation) => Some(annotation.clone()),
            _ => None,
        })
    }

    /// The links intersecting `start..end`, as Compose's `getLinkAnnotations`.
    pub fn get_link_annotations(
        &self,
        start: usize,
        end: usize,
    ) -> Vec<AnnotatedRange<LinkAnnotation>> {
        self.filter_annotations(start, end, |range| match &range.item {
            Annotation::Link(link) => Some(link.clone()),
            _ => None,
        })
    }

    /// Whether a link intersects `start..end`, as Compose's `hasLinkAnnotations`.
    pub fn has_link_annotations(&self, start: usize, end: usize) -> bool {
        self.annotations.iter().any(|range| {
            matches!(range.item, Annotation::Link(_))
                && intersect(start, end, range.start, range.end)
        })
    }

    /// Whether the annotations are the same, as Compose's `hasEqualAnnotations`.
    pub fn has_equal_annotations(&self, other: &AnnotatedString) -> bool {
        Rc::ptr_eq(&self.annotations, &other.annotations) || self.annotations == other.annotations
    }

    /// Whether the annotations that lay text out are the same, as Compose's
    /// `hasSameLayoutAffectingAnnotations`: every annotation here does (draw annotations,
    /// Compose's only exception, aren't in compose-rs yet).
    pub fn has_same_layout_affecting_annotations(&self, other: &AnnotatedString) -> bool {
        self.has_equal_annotations(other)
    }

    /// The annotations each replaced by `transform`, as Compose's `mapAnnotations`.
    pub fn map_annotations(
        &self,
        transform: impl FnMut(&AnnotatedRange<Annotation>) -> AnnotatedRange<Annotation>,
    ) -> AnnotatedString {
        let annotations: Vec<_> = self.annotations.iter().map(transform).collect();
        AnnotatedString::with_annotations(self.text.clone(), annotations)
    }

    /// The annotations each replaced by those `transform` gives, as Compose's
    /// `flatMapAnnotations`.
    pub fn flat_map_annotations(
        &self,
        transform: impl FnMut(&AnnotatedRange<Annotation>) -> Vec<AnnotatedRange<Annotation>>,
    ) -> AnnotatedString {
        let annotations: Vec<_> = self.annotations.iter().flat_map(transform).collect();
        AnnotatedString::with_annotations(self.text.clone(), annotations)
    }

    /// Bytes `start..end` of the text with the annotations over them, as Compose's
    /// `subSequence`: a range intersecting them is cut to them and moved with them (an empty
    /// range at `start` is kept). The whole text is this string itself. Panics if `start > end`
    /// or an end is past the text or inside a character.
    pub fn sub_sequence(&self, start: usize, end: usize) -> AnnotatedString {
        assert!(start <= end, "start ({start}) should be less or equal to end ({end})");
        if start == 0 && end == self.text.len() {
            return self.clone();
        }
        let text = &self.text.as_str()[start..end];
        let annotations: Vec<_> = self
            .annotations
            .iter()
            .filter(|range| intersect(start, end, range.start, range.end))
            .map(|range| AnnotatedRange {
                item: range.item.clone(),
                start: start.max(range.start) - start,
                end: end.min(range.end) - start,
                tag: range.tag.clone(),
            })
            .collect();
        AnnotatedString { text: text.to_string().into(), annotations: annotations.into() }
    }

    /// The annotations over bytes `start..end`, moved to start at 0, as Compose's
    /// `getLocalAnnotations`: none for an empty range.
    fn local_annotations(
        &self,
        start: usize,
        end: usize,
        keep: impl Fn(&Annotation) -> bool,
    ) -> Vec<AnnotatedRange<Annotation>> {
        if start == end {
            return Vec::new();
        }
        self.annotations
            .iter()
            .filter(|range| keep(&range.item) && intersect(start, end, range.start, range.end))
            .map(|range| AnnotatedRange {
                item: range.item.clone(),
                start: range.start.clamp(start, end) - start,
                end: range.end.clamp(start, end) - start,
                tag: range.tag.clone(),
            })
            .collect()
    }

    /// The paragraphs the text lays out as, in order and covering it, each in `default` merged
    /// with the paragraph styles over it (nested ones merged into their parents), as Compose's
    /// internal `normalizedParagraphStyles`: an empty text is one empty paragraph.
    pub fn normalized_paragraph_styles(
        &self,
        default: &ParagraphStyle,
    ) -> Vec<AnnotatedRange<ParagraphStyle>> {
        let mut sorted = self.paragraph_styles();
        sorted.sort_by_key(|range| range.start);
        let length = self.text.len();
        let mut result: Vec<AnnotatedRange<ParagraphStyle>> = Vec::new();
        let mut last_added = 0;
        let mut stack: Vec<AnnotatedRange<ParagraphStyle>> = Vec::new();
        for range in sorted {
            let current = AnnotatedRange::new(default.merge(&range.item), range.start, range.end);
            while last_added < current.start
                && let Some(last) = stack.last().cloned()
            {
                if current.start < last.end {
                    result.push(AnnotatedRange::new(last.item, last_added, current.start));
                    last_added = current.start;
                } else {
                    result.push(AnnotatedRange::new(last.item, last_added, last.end));
                    last_added = last.end;
                    while stack.last().is_some_and(|top| top.end == last_added) {
                        stack.pop();
                    }
                }
            }
            if last_added < current.start {
                result.push(AnnotatedRange::new(*default, last_added, current.start));
                last_added = current.start;
            }
            match stack.last().cloned() {
                Some(last) if last.start == current.start && last.end == current.end => {
                    stack.pop();
                    let merged = last.item.merge(&current.item);
                    stack.push(AnnotatedRange::new(merged, current.start, current.end));
                }
                Some(last) if last.start == last.end => {
                    result.push(AnnotatedRange::new(last.item, last.start, last.end));
                    stack.pop();
                    stack.push(current);
                }
                Some(last) => {
                    assert!(last.end >= current.end, "Paragraph overlap not allowed");
                    let merged = last.item.merge(&current.item);
                    stack.push(AnnotatedRange::new(merged, current.start, current.end));
                }
                None => stack.push(current),
            }
        }
        while last_added <= length
            && let Some(last) = stack.last().cloned()
        {
            result.push(AnnotatedRange::new(last.item, last_added, last.end));
            last_added = last.end;
            while stack.last().is_some_and(|top| top.end == last_added) {
                stack.pop();
            }
        }
        if last_added < length {
            result.push(AnnotatedRange::new(*default, last_added, length));
        }
        if result.is_empty() {
            result.push(AnnotatedRange::new(*default, 0, 0));
        }
        result
    }

    /// Bytes `start..end` with their annotations but the paragraph styles, as Compose's
    /// internal `substringWithoutParagraphStyles`: what one paragraph of a multi-paragraph text
    /// lays out.
    pub fn substring_without_paragraph_styles(&self, start: usize, end: usize) -> AnnotatedString {
        let annotations = self
            .local_annotations(start, end, |item| !matches!(item, Annotation::ParagraphStyle(_)));
        AnnotatedString {
            text: if start == end {
                SharedString::EMPTY
            } else {
                self.text.as_str()[start..end].to_string().into()
            },
            annotations: annotations.into(),
        }
    }

    /// The runs of the text with the same span styles: each byte range with the style it's
    /// drawn in (the spans covering it over `style`, in their order) and its background.
    /// Covers the whole text.
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
        for (_, start, end) in self.span_style_ranges() {
            bounds.push(bound(start));
            bounds.push(bound(end));
        }
        bounds.sort_unstable();
        bounds.dedup();
        bounds
            .windows(2)
            .filter(|pair| pair[0] < pair[1])
            .map(|pair| {
                let range = pair[0]..pair[1];
                let covering = || {
                    self.span_style_ranges().filter(|(_, start, end)| {
                        bound(*start) <= range.start && range.end <= bound(*end)
                    })
                };
                let style = covering().fold(style.clone(), |style, (span, _, _)| {
                    style.merge_span_style(&relative_to(span, &style))
                });
                let background = covering().fold(Color::UNSPECIFIED, |background, (span, _, _)| {
                    span.background.or(background)
                });
                (range, style, background)
            })
            .collect()
    }
}

/// Panics if paragraph styles partly overlap, as Compose's `AnnotatedString` constructor
/// ("Paragraph overlap not allowed"): sorted by start (stably), each must end before the
/// enclosing one ends, or start after it.
fn check_paragraph_overlaps(annotations: &[AnnotatedRange<Annotation>]) {
    let mut paragraphs: Vec<(usize, usize)> = annotations
        .iter()
        .filter(|range| matches!(range.item, Annotation::ParagraphStyle(_)))
        .map(|range| (range.start, range.end))
        .collect();
    if paragraphs.len() < 2 {
        return;
    }
    paragraphs.sort_by_key(|(start, _)| *start);
    let mut previous_ends = vec![paragraphs[0].1];
    for &(start, end) in &paragraphs[1..] {
        while let Some(&previous_end) = previous_ends.last() {
            if start >= previous_end {
                previous_ends.pop();
            } else {
                assert!(
                    end <= previous_end,
                    "Paragraph overlap not allowed, end {end} should be less than or equal to \
                     {previous_end}"
                );
                break;
            }
        }
        previous_ends.push(end);
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

impl fmt::Debug for AnnotatedString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnnotatedString")
            .field("text", &self.text)
            .field("annotations", &&*self.annotations)
            .finish()
    }
}

/// The plain text, as Compose's `toString`.
impl fmt::Display for AnnotatedString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.text.as_str())
    }
}

/// As Compose's `plus`: this text then `other`, with both's annotations.
impl std::ops::Add<&AnnotatedString> for &AnnotatedString {
    type Output = AnnotatedString;
    fn add(self, other: &AnnotatedString) -> AnnotatedString {
        let mut builder = AnnotatedStringBuilder::from(self.clone());
        builder.append_annotated(other);
        builder.to_annotated_string()
    }
}

impl std::ops::Add for AnnotatedString {
    type Output = AnnotatedString;
    fn add(self, other: AnnotatedString) -> AnnotatedString {
        &self + &other
    }
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
        self.text.same(&other.text) && self.has_equal_annotations(other)
    }
}

/// An annotation being built: its end set when popped (or the text's end when built).
#[derive(Clone)]
struct OpenRange {
    item: Annotation,
    start: usize,
    end: Option<usize>,
    tag: SharedString,
}

/// Builds an `AnnotatedString`, as Compose's `AnnotatedString.Builder`: text appended, styles
/// and annotations added over ranges or pushed and popped around what's appended.
#[derive(Default, Clone)]
pub struct AnnotatedStringBuilder {
    text: String,
    annotations: Vec<OpenRange>,
    /// The pushed annotations not popped yet: indices into `annotations`.
    stack: Vec<usize>,
    /// The bullet lists entered: their indentation and bullet.
    bullet_lists: Vec<(TextUnit, Bullet)>,
}

impl From<&str> for AnnotatedStringBuilder {
    /// As `Builder(text)`.
    fn from(text: &str) -> AnnotatedStringBuilder {
        let mut builder = AnnotatedStringBuilder::default();
        builder.append(text);
        builder
    }
}

impl From<AnnotatedString> for AnnotatedStringBuilder {
    /// As `Builder(annotatedString)`.
    fn from(text: AnnotatedString) -> AnnotatedStringBuilder {
        let mut builder = AnnotatedStringBuilder::default();
        builder.append_annotated(&text);
        builder
    }
}

impl AnnotatedStringBuilder {
    /// An empty builder, as `Builder()`.
    pub fn new() -> AnnotatedStringBuilder {
        AnnotatedStringBuilder::default()
    }

    /// An empty builder with room for `capacity` bytes, as `Builder(capacity)`.
    pub fn with_capacity(capacity: usize) -> AnnotatedStringBuilder {
        AnnotatedStringBuilder { text: String::with_capacity(capacity), ..Default::default() }
    }

    /// The text's length so far, in bytes, as Compose's `length`.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Appends text, as `append(String)`.
    pub fn append(&mut self, text: &str) -> &mut Self {
        self.text.push_str(text);
        self
    }

    /// Appends a character, as `append(Char)`.
    pub fn append_char(&mut self, character: char) -> &mut Self {
        self.text.push(character);
        self
    }

    /// Appends styled text with its annotations, as `append(AnnotatedString)`.
    pub fn append_annotated(&mut self, text: &AnnotatedString) -> &mut Self {
        let start = self.text.len();
        self.text.push_str(text.as_str());
        for range in text.annotations() {
            self.annotations.push(OpenRange {
                item: range.item.clone(),
                start: start + range.start,
                end: Some(start + range.end),
                tag: range.tag.clone(),
            });
        }
        self
    }

    /// Appends bytes `start..end` of styled text with the annotations over them, as
    /// `append(AnnotatedString, start, end)`.
    pub fn append_annotated_range(
        &mut self,
        text: &AnnotatedString,
        start: usize,
        end: usize,
    ) -> &mut Self {
        let insertion = self.text.len();
        self.text.push_str(&text.as_str()[start..end]);
        for range in text.local_annotations(start, end, |_| true) {
            self.annotations.push(OpenRange {
                item: range.item,
                start: insertion + range.start,
                end: Some(insertion + range.end),
                tag: range.tag,
            });
        }
        self
    }

    fn add(&mut self, item: Annotation, start: usize, end: usize, tag: SharedString) -> &mut Self {
        self.annotations.push(OpenRange { item, start, end: Some(end), tag });
        self
    }

    /// Styles bytes `start..end`, as `addStyle(SpanStyle, start, end)`.
    pub fn add_style(&mut self, style: SpanStyle, start: usize, end: usize) -> &mut Self {
        self.add(Annotation::SpanStyle(style), start, end, SharedString::EMPTY)
    }

    /// Styles the paragraph over bytes `start..end`, as `addStyle(ParagraphStyle, start, end)`.
    pub fn add_paragraph_style(
        &mut self,
        style: ParagraphStyle,
        start: usize,
        end: usize,
    ) -> &mut Self {
        self.add(Annotation::ParagraphStyle(style), start, end, SharedString::EMPTY)
    }

    /// Annotates bytes `start..end` with `annotation` under `tag`, as `addStringAnnotation`.
    pub fn add_string_annotation(
        &mut self,
        tag: impl Into<SharedString>,
        annotation: impl Into<SharedString>,
        start: usize,
        end: usize,
    ) -> &mut Self {
        let item = Annotation::StringAnnotation(StringAnnotation(annotation.into()));
        self.add(item, start, end, tag.into())
    }

    /// Annotates bytes `start..end` for text-to-speech, as `addTtsAnnotation`.
    pub fn add_tts_annotation(
        &mut self,
        annotation: TtsAnnotation,
        start: usize,
        end: usize,
    ) -> &mut Self {
        self.add(Annotation::Tts(annotation), start, end, SharedString::EMPTY)
    }

    /// Makes bytes `start..end` a link, as `addLink`.
    pub fn add_link(&mut self, link: LinkAnnotation, start: usize, end: usize) -> &mut Self {
        self.add(Annotation::Link(link), start, end, SharedString::EMPTY)
    }

    /// Puts a bullet before the paragraph over bytes `start..end`, as `addBullet`.
    pub fn add_bullet(&mut self, bullet: Bullet, start: usize, end: usize) -> &mut Self {
        self.add(Annotation::Bullet(bullet), start, end, SharedString::EMPTY)
    }

    /// Puts a bullet before the paragraph over bytes `start..end`, indented by `indentation`,
    /// as `addBullet(bullet, indentation, start, end)`: the indent is a paragraph style.
    pub fn add_bullet_with_indentation(
        &mut self,
        bullet: Bullet,
        indentation: TextUnit,
        start: usize,
        end: usize,
    ) -> &mut Self {
        let indent = TextIndent { first_line: indentation, rest_line: indentation };
        let style = ParagraphStyle { text_indent: Some(indent), ..ParagraphStyle::DEFAULT };
        self.add_paragraph_style(style, start, end);
        self.add_bullet(bullet, start, end)
    }

    fn push(&mut self, item: Annotation, tag: SharedString) -> usize {
        self.stack.push(self.annotations.len());
        self.annotations.push(OpenRange { item, start: self.text.len(), end: None, tag });
        self.stack.len() - 1
    }

    /// Styles what's appended until popped, as `pushStyle(SpanStyle)`: returns its index for
    /// `pop_to`.
    pub fn push_style(&mut self, style: SpanStyle) -> usize {
        self.push(Annotation::SpanStyle(style), SharedString::EMPTY)
    }

    /// Styles the paragraph appended until popped, as `pushStyle(ParagraphStyle)`.
    pub fn push_paragraph_style(&mut self, style: ParagraphStyle) -> usize {
        self.push(Annotation::ParagraphStyle(style), SharedString::EMPTY)
    }

    /// Annotates what's appended until popped, as `pushStringAnnotation`.
    pub fn push_string_annotation(
        &mut self,
        tag: impl Into<SharedString>,
        annotation: impl Into<SharedString>,
    ) -> usize {
        self.push(Annotation::StringAnnotation(StringAnnotation(annotation.into())), tag.into())
    }

    /// Annotates what's appended until popped for text-to-speech, as `pushTtsAnnotation`.
    pub fn push_tts_annotation(&mut self, annotation: TtsAnnotation) -> usize {
        self.push(Annotation::Tts(annotation), SharedString::EMPTY)
    }

    /// Makes what's appended until popped a link, as `pushLink`.
    pub fn push_link(&mut self, link: LinkAnnotation) -> usize {
        self.push(Annotation::Link(link), SharedString::EMPTY)
    }

    /// Puts a bullet before the paragraph appended until popped, as `pushBullet`.
    pub fn push_bullet(&mut self, bullet: Bullet) -> usize {
        self.push(Annotation::Bullet(bullet), SharedString::EMPTY)
    }

    /// Ends the last annotation pushed, as `pop()`. Panics when nothing was pushed ("Nothing to
    /// pop.", as Compose's throws).
    pub fn pop(&mut self) -> &mut Self {
        let index = self.stack.pop().expect("Nothing to pop.");
        self.annotations[index].end = Some(self.text.len());
        self
    }

    /// Ends the annotations pushed from index `index` on, as `pop(index)`. Panics unless
    /// `index` is one still pushed.
    pub fn pop_to(&mut self, index: usize) -> &mut Self {
        assert!(index < self.stack.len(), "{index} should be less than {}", self.stack.len());
        while self.stack.len() > index {
            self.pop();
        }
        self
    }

    fn with<R>(&mut self, index: usize, block: impl FnOnce(&mut Self) -> R) -> R {
        let result = block(self);
        self.pop_to(index);
        result
    }

    /// Styles what `block` appends, as `withStyle(SpanStyle)`.
    pub fn with_style<R>(&mut self, style: SpanStyle, block: impl FnOnce(&mut Self) -> R) -> R {
        let index = self.push_style(style);
        self.with(index, block)
    }

    /// Styles the paragraph `block` appends, as `withStyle(ParagraphStyle)`.
    pub fn with_paragraph_style<R>(
        &mut self,
        style: ParagraphStyle,
        block: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let index = self.push_paragraph_style(style);
        self.with(index, block)
    }

    /// Annotates what `block` appends, as `withAnnotation(tag, annotation)`.
    pub fn with_annotation<R>(
        &mut self,
        tag: impl Into<SharedString>,
        annotation: impl Into<SharedString>,
        block: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let index = self.push_string_annotation(tag, annotation);
        self.with(index, block)
    }

    /// Annotates what `block` appends for text-to-speech, as `withAnnotation(ttsAnnotation)`.
    pub fn with_tts_annotation<R>(
        &mut self,
        annotation: TtsAnnotation,
        block: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let index = self.push_tts_annotation(annotation);
        self.with(index, block)
    }

    /// Makes what `block` appends a link, as `withLink`.
    pub fn with_link<R>(&mut self, link: LinkAnnotation, block: impl FnOnce(&mut Self) -> R) -> R {
        let index = self.push_link(link);
        self.with(index, block)
    }

    /// A bullet list around what `block` appends, as `withBulletList`: a paragraph indented by
    /// `indentation` (added to an enclosing list's, whose unit it must share), its items
    /// (`with_bullet_list_item`) taking `bullet`.
    pub fn with_bullet_list<R>(
        &mut self,
        indentation: TextUnit,
        bullet: Bullet,
        block: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let indentation = match self.bullet_lists.last() {
            Some((outer, _)) => {
                assert!(
                    outer.unit_type() == indentation.unit_type(),
                    "Indentation unit types of nested bullet lists must match. Current {outer:?} \
                     and previous is {indentation:?}"
                );
                match indentation.unit_type() {
                    TextUnitType::Sp | TextUnitType::Em => {
                        TextUnit::new(indentation.value() + outer.value(), indentation.unit_type())
                    }
                    TextUnitType::Unspecified => indentation,
                }
            }
            None => indentation,
        };
        let indent = TextIndent { first_line: indentation, rest_line: indentation };
        let index = self.push_paragraph_style(ParagraphStyle {
            text_indent: Some(indent),
            ..ParagraphStyle::DEFAULT
        });
        self.bullet_lists.push((indentation, bullet));
        let result = block(self);
        self.bullet_lists.pop();
        self.pop_to(index);
        result
    }

    /// An item of the bullet list around it, as `withBulletListItem`: a paragraph with the
    /// list's indentation and a bullet (`bullet`, else the list's).
    pub fn with_bullet_list_item<R>(
        &mut self,
        bullet: Option<Bullet>,
        block: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let (indentation, list_bullet) = self
            .bullet_lists
            .last()
            .cloned()
            .unwrap_or((Bullet::DEFAULT_INDENTATION, Bullet::default_bullet()));
        let indent = TextIndent { first_line: indentation, rest_line: indentation };
        let paragraph = self.push_paragraph_style(ParagraphStyle {
            text_indent: Some(indent),
            ..ParagraphStyle::DEFAULT
        });
        let bullet = self.push_bullet(bullet.unwrap_or(list_bullet));
        let result = block(self);
        self.pop_to(bullet);
        self.pop_to(paragraph);
        result
    }

    /// The text so far: annotations still pushed end at its end, as `toAnnotatedString`.
    /// Panics if paragraph styles partly overlap.
    pub fn to_annotated_string(&self) -> AnnotatedString {
        let length = self.text.len();
        let annotations: Vec<_> = self
            .annotations
            .iter()
            .map(|range| AnnotatedRange {
                item: range.item.clone(),
                start: range.start,
                end: range.end.unwrap_or(length),
                tag: range.tag.clone(),
            })
            .collect();
        AnnotatedString::with_annotations(self.text.clone(), annotations)
    }
}
