//! The word around an offset, as Compose's `getWordBoundary` (D1108): its `WordIterator` over a
//! word break iterator, with its `WordBoundary` rules. The breaks are Unicode's word boundaries
//! (UAX #29, `unicode-segmentation`: apostrophes and periods inside words, marks, viramas and
//! tone marks with their letters, emoji sequences whole), as ICU's word instance without its
//! dictionaries; a break between an emoji and a letter, digit or emoji is bridged, as Compose's
//! `shouldBridgeEmojiBoundary`. A run of punctuation is a word of its own.

use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_segmentation::UnicodeSegmentation;

use crate::text_range::TextRange;

/// Whether `c` is punctuation, as Compose's `WordIterator.isPunctuation` (connector, dash,
/// close, final quote, initial quote, other and open punctuation).
fn is_punctuation(c: char) -> bool {
    use GeneralCategory::*;
    matches!(
        get_general_category(c),
        ConnectorPunctuation
            | DashPunctuation
            | ClosePunctuation
            | FinalPunctuation
            | InitialPunctuation
            | OtherPunctuation
            | OpenPunctuation
    )
}

/// Whether `c` is a letter or a decimal digit, as Java's `Character.isLetterOrDigit`.
fn is_letter_or_digit(c: char) -> bool {
    use GeneralCategory::*;
    matches!(
        get_general_category(c),
        UppercaseLetter
            | LowercaseLetter
            | TitlecaseLetter
            | ModifierLetter
            | OtherLetter
            | DecimalNumber
    )
}

/// Whether `c` is part of an emoji (a pictograph, or a joiner, variation selector, keycap, skin
/// tone, regional indicator or tag within one), as Compose asks EmojiCompat.
fn is_emoji(c: char) -> bool {
    matches!(c as u32,
        0x00A9 | 0x00AE | 0x203C | 0x2049 | 0x2122 | 0x2139
        | 0x200D // zero width joiner
        | 0x20E3 // combining enclosing keycap
        | 0x2194..=0x21AA
        | 0x231A..=0x23FF
        | 0x24C2
        | 0x25AA..=0x27BF
        | 0x2934 | 0x2935
        | 0x2B05..=0x2B55
        | 0x3030 | 0x303D | 0x3297 | 0x3299
        | 0xFE0F // variation selector 16
        | 0x1F000..=0x1FAFF // pictographs, regional indicators, skin tones
        | 0xE0020..=0xE007F) // tags
}

/// A text's word breaks, and Compose's `WordIterator` over them.
struct WordIterator<'a> {
    text: &'a str,
    /// The breaks, in order: the text's start and end included.
    breaks: Vec<usize>,
}

impl<'a> WordIterator<'a> {
    fn new(text: &'a str) -> WordIterator<'a> {
        let mut breaks: Vec<usize> =
            text.split_word_bound_indices().map(|(start, _)| start).collect();
        breaks.push(text.len());
        WordIterator { text, breaks }
    }

    fn before(&self, offset: usize) -> Option<char> {
        self.text[..offset].chars().next_back()
    }

    fn at(&self, offset: usize) -> Option<char> {
        self.text[offset..].chars().next()
    }

    /// Compose's `shouldBridgeEmojiBoundary`: a break between an emoji and a letter, digit or
    /// emoji isn't one.
    fn bridges(&self, offset: usize) -> bool {
        if offset == 0 || offset >= self.text.len() {
            return false;
        }
        let (Some(left), Some(right)) = (self.before(offset), self.at(offset)) else {
            return false;
        };
        let (left_emoji, right_emoji) = (is_emoji(left), is_emoji(right));
        (left_emoji || right_emoji)
            && (left_emoji || is_letter_or_digit(left))
            && (right_emoji || is_letter_or_digit(right))
    }

    /// The break before `offset` (`None` at the start), as `preceding` with bridging.
    fn prev(&self, offset: usize) -> Option<usize> {
        let index = self.breaks.partition_point(|&at| at < offset);
        self.breaks[..index].iter().rev().copied().find(|&at| !self.bridges(at))
    }

    /// The break after `offset` (`None` at the end), as `following` with bridging.
    fn next(&self, offset: usize) -> Option<usize> {
        let index = self.breaks.partition_point(|&at| at <= offset);
        self.breaks[index..].iter().copied().find(|&at| !self.bridges(at))
    }

    fn is_break(&self, offset: usize) -> bool {
        self.breaks.binary_search(&offset).is_ok() && !self.bridges(offset)
    }

    fn on_punctuation(&self, offset: usize) -> bool {
        self.at(offset).is_some_and(is_punctuation)
    }

    fn after_punctuation(&self, offset: usize) -> bool {
        self.before(offset).is_some_and(is_punctuation)
    }

    fn on_word(&self, offset: usize) -> bool {
        self.at(offset).is_some_and(|c| is_letter_or_digit(c) || is_emoji(c))
    }

    fn after_word(&self, offset: usize) -> bool {
        self.before(offset).is_some_and(|c| is_letter_or_digit(c) || is_emoji(c))
    }

    /// Compose's `getWordStart`.
    fn word_start(&self, offset: usize) -> usize {
        let found = if self.prev(offset).is_some_and(|at| self.on_punctuation(at)) {
            // A run of punctuation: its start.
            let mut at = Some(offset);
            while let Some(current) = at {
                if self.on_punctuation(current) && !self.after_punctuation(current) {
                    break;
                }
                at = self.prev(current);
            }
            at
        } else if self.on_word(offset) {
            if self.is_break(offset) && !self.after_word(offset) {
                Some(offset)
            } else {
                self.prev(offset)
            }
        } else if self.after_word(offset) {
            self.prev(offset)
        } else {
            None
        };
        found.unwrap_or(offset)
    }

    /// Compose's `getWordEnd`.
    fn word_end(&self, offset: usize) -> usize {
        let found = if self.next(offset).is_some_and(|at| self.after_punctuation(at)) {
            // A run of punctuation: its end.
            let mut at = Some(offset);
            while let Some(current) = at {
                if !self.on_punctuation(current) && self.after_punctuation(current) {
                    break;
                }
                at = self.next(current);
            }
            at
        } else if self.after_word(offset) {
            if self.is_break(offset) && !self.on_word(offset) {
                Some(offset)
            } else {
                self.next(offset)
            }
        } else if self.on_word(offset) {
            self.next(offset)
        } else {
            None
        };
        found.unwrap_or(offset)
    }
}

/// The word around `offset` of `text`, as Compose's `getWordBoundary` (its `WordIterator`'s
/// word start and end): a word ending or starting at `offset` counts, a run of punctuation is a
/// word, and between spaces it's the collapsed range at `offset`. An offset inside a character
/// counts as that character's start. Panics past the text, as Compose's iterator throws.
pub fn word_boundary(text: &str, offset: usize) -> TextRange {
    assert!(offset <= text.len(), "Invalid offset: {offset}. Valid range is [0 , {}]", text.len());
    let offset = crate::text_editing::floor_char_boundary(text, offset);
    let words = WordIterator::new(text);
    TextRange::new(words.word_start(offset), words.word_end(offset))
}
