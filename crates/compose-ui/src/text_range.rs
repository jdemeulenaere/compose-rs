//! A range of a text, as Compose's `TextRange`: a selection, a composition, a span. Offsets are
//! UTF-8 byte offsets at character boundaries, as every text offset in compose-rs (D1100), where
//! Compose's are UTF-16 indices.

use std::fmt;

use compose_runtime::ComposeArg;
use compose_runtime::saveable::{Saveable, SavedValue};

/// A range of a text, from `start` to `end` (exclusive), as Compose's `TextRange`. `start` may
/// be after `end`: a selection made backwards (`reversed`). Offsets are UTF-8 byte offsets
/// (D1100).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    /// As `TextRange.Zero`.
    pub const ZERO: TextRange = TextRange { start: 0, end: 0 };

    /// From `start` to `end`, as `TextRange(start, end)`.
    pub const fn new(start: usize, end: usize) -> TextRange {
        TextRange { start, end }
    }

    /// A collapsed range at `index` (a cursor), as `TextRange(index)`.
    pub const fn at(index: usize) -> TextRange {
        TextRange { start: index, end: index }
    }

    /// The smaller end.
    pub const fn min(self) -> usize {
        if self.start < self.end { self.start } else { self.end }
    }

    /// The larger end.
    pub const fn max(self) -> usize {
        if self.start < self.end { self.end } else { self.start }
    }

    /// `start` is `end`: a cursor.
    pub const fn collapsed(self) -> bool {
        self.start == self.end
    }

    /// `start` is after `end`.
    pub const fn reversed(self) -> bool {
        self.start > self.end
    }

    /// `max - min`.
    pub const fn length(self) -> usize {
        self.max() - self.min()
    }

    /// Whether the two share an offset, as Compose's `intersects`.
    pub const fn intersects(self, other: TextRange) -> bool {
        self.min() < other.max() && other.min() < self.max()
    }

    /// Whether `other` is within this range (an equal range is), as Compose's
    /// `contains(TextRange)`.
    pub const fn contains_range(self, other: TextRange) -> bool {
        self.min() <= other.min() && other.max() <= self.max()
    }

    /// Whether `offset` is in `min..max`, as Compose's `contains(Int)`.
    pub const fn contains(self, offset: usize) -> bool {
        self.min() <= offset && offset < self.max()
    }

    /// Both ends kept in `minimum..=maximum`, as Compose's `coerceIn`.
    pub fn coerce_in(self, minimum: usize, maximum: usize) -> TextRange {
        TextRange {
            start: self.start.clamp(minimum, maximum),
            end: self.end.clamp(minimum, maximum),
        }
    }

    /// `min..max`.
    pub const fn range(self) -> std::ops::Range<usize> {
        self.min()..self.max()
    }

    /// The part of `text` in the range (`min..max`), as Compose's `CharSequence.substring`.
    /// Panics if an end isn't at a character boundary of `text`, as slicing does.
    pub fn substring(self, text: &str) -> &str {
        &text[self.min()..self.max()]
    }
}

impl From<std::ops::Range<usize>> for TextRange {
    fn from(range: std::ops::Range<usize>) -> TextRange {
        TextRange::new(range.start, range.end)
    }
}

impl fmt::Debug for TextRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "TextRange({}, {})", self.start, self.end)
    }
}

impl fmt::Display for TextRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl ComposeArg for TextRange {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Saved as its two ends, as `TextRange.Saver`.
impl Saveable for TextRange {
    fn save(&self) -> SavedValue {
        (self.start, self.end).save()
    }
    fn restore(saved: &SavedValue) -> Option<TextRange> {
        <(usize, usize)>::restore(saved).map(|(start, end)| TextRange::new(start, end))
    }
}
