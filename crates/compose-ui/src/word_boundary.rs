//! The word around an offset, as Compose's `getWordBoundary` (D1108): its `WordIterator` over a
//! word break iterator, with its `WordBoundary` rules. The breaks are Unicode's word boundaries
//! (UAX #29, `unicode-segmentation`: apostrophes and periods inside words, marks, viramas and
//! tone marks with their letters, emoji sequences whole), as ICU's word instance without its
//! dictionaries, except that a run of Thai, Lao, Myanmar or Khmer is one word until there's one
//! (D1114); a break between an emoji and a letter, digit or emoji is bridged, as Compose's
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

/// Whether a character alone is an emoji: Unicode's `Emoji_Presentation` (17.0's `emoji-data`).
/// A symbol that's text until a variation selector says otherwise (★ never, ❤ with one) isn't.
fn is_emoji_presentation(c: char) -> bool {
    matches!(c as u32,
        0x231A..=0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3 | 0x25FD..=0x25FE | 0x2614..=0x2615
        | 0x2648..=0x2653 | 0x267F | 0x2693 | 0x26A1 | 0x26AA..=0x26AB | 0x26BD..=0x26BE
        | 0x26C4..=0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2..=0x26F3 | 0x26F5 | 0x26FA | 0x26FD
        | 0x2705 | 0x270A..=0x270B | 0x2728 | 0x274C | 0x274E | 0x2753..=0x2755 | 0x2757
        | 0x2795..=0x2797 | 0x27B0 | 0x27BF | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55 | 0x1F004
        | 0x1F0CF | 0x1F18E | 0x1F191..=0x1F19A | 0x1F1E6..=0x1F1FF | 0x1F201 | 0x1F21A | 0x1F22F
        | 0x1F232..=0x1F236 | 0x1F238..=0x1F23A | 0x1F250..=0x1F251 | 0x1F300..=0x1F320
        | 0x1F32D..=0x1F335 | 0x1F337..=0x1F37C | 0x1F37E..=0x1F393 | 0x1F3A0..=0x1F3CA
        | 0x1F3CF..=0x1F3D3 | 0x1F3E0..=0x1F3F0 | 0x1F3F4 | 0x1F3F8..=0x1F43E | 0x1F440
        | 0x1F442..=0x1F4FC | 0x1F4FF..=0x1F53D | 0x1F54B..=0x1F54E | 0x1F550..=0x1F567 | 0x1F57A
        | 0x1F595..=0x1F596 | 0x1F5A4 | 0x1F5FB..=0x1F64F | 0x1F680..=0x1F6C5 | 0x1F6CC
        | 0x1F6D0..=0x1F6D2 | 0x1F6D5..=0x1F6D8 | 0x1F6DC..=0x1F6DF | 0x1F6EB..=0x1F6EC
        | 0x1F6F4..=0x1F6FC | 0x1F7E0..=0x1F7EB | 0x1F7F0 | 0x1F90C..=0x1F93A | 0x1F93C..=0x1F945
        | 0x1F947..=0x1F9FF | 0x1FA70..=0x1FA7C | 0x1FA80..=0x1FA8A | 0x1FA8E..=0x1FAC6 | 0x1FAC8
        | 0x1FACD..=0x1FADC | 0x1FADF..=0x1FAEA | 0x1FAEF..=0x1FAF8)
}

const VARIATION_SELECTOR_16: char = '\u{FE0F}';

/// What only an emoji before it makes part of one: a joiner, variation selector, keycap or tag.
fn is_emoji_component(c: char) -> bool {
    matches!(c as u32, 0x200D | 0xFE0F | 0x20E3 | 0xE0020..=0xE007F)
}

/// Whether `c` at the start of `after` begins an emoji: one alone, or one a variation selector
/// after it makes.
fn starts_emoji(c: char, after: &str) -> bool {
    is_emoji_presentation(c)
        || (!c.is_alphabetic()
            && !is_emoji_component(c)
            && after.chars().nth(1) == Some(VARIATION_SELECTOR_16))
}

/// Whether the character at `offset` of `text` is part of an emoji, as Compose asks EmojiCompat:
/// one that begins one, or a component after one.
fn is_emoji_at(text: &str, offset: usize) -> bool {
    let Some(c) = text[offset..].chars().next() else { return false };
    if !is_emoji_component(c) {
        return starts_emoji(c, &text[offset..]);
    }
    let before = &text[..offset];
    let base = before.char_indices().rev().find(|&(_, c)| !is_emoji_component(c));
    base.is_some_and(|(at, base)| starts_emoji(base, &text[at..]))
}

/// The scripts Unicode's word boundaries leave to a dictionary, breaking between their letters
/// (Line_Break Complex_Context; ICU's dictionaries find their words).
#[derive(Clone, Copy, PartialEq)]
enum DictionaryScript {
    Thai,
    Lao,
    Myanmar,
    Khmer,
}

/// `c`'s script if it's one a dictionary would break, and part of a word there (a letter, mark or
/// digit: their punctuation still ends one).
fn dictionary_script(c: char) -> Option<DictionaryScript> {
    use GeneralCategory::*;
    let script = match c as u32 {
        0x0E00..=0x0E7F => DictionaryScript::Thai,
        0x0E80..=0x0EFF => DictionaryScript::Lao,
        0x1000..=0x109F | 0xA9E0..=0xA9FF | 0xAA60..=0xAA7F | 0x116D0..=0x116E3 => {
            DictionaryScript::Myanmar
        }
        0x1780..=0x17FF | 0x19E0..=0x19FF => DictionaryScript::Khmer,
        _ => return None,
    };
    let in_word = is_letter_or_digit(c)
        || matches!(get_general_category(c), NonspacingMark | SpacingMark | EnclosingMark);
    in_word.then_some(script)
}

/// The word breaks of `text`, in order, its start and end included: Unicode's, without those
/// inside a run of Thai, Lao, Myanmar or Khmer, which is one word until there's a dictionary
/// (D1114).
pub fn word_breaks(text: &str) -> Vec<usize> {
    let mut breaks: Vec<usize> = text
        .split_word_bound_indices()
        .map(|(start, _)| start)
        .filter(|&at| {
            let before = text[..at].chars().next_back().and_then(dictionary_script);
            let after = text[at..].chars().next().and_then(dictionary_script);
            before.is_none() || before != after
        })
        .collect();
    breaks.push(text.len());
    breaks
}

/// A text's word breaks, and Compose's `WordIterator` over them.
struct WordIterator<'a> {
    text: &'a str,
    /// The breaks, in order: the text's start and end included.
    breaks: Vec<usize>,
}

impl<'a> WordIterator<'a> {
    fn new(text: &'a str) -> WordIterator<'a> {
        WordIterator { text, breaks: word_breaks(text) }
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
        let left_emoji = is_emoji_at(self.text, offset - left.len_utf8());
        let right_emoji = is_emoji_at(self.text, offset);
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
        self.at(offset).is_some_and(is_letter_or_digit) || is_emoji_at(self.text, offset)
    }

    fn after_word(&self, offset: usize) -> bool {
        self.before(offset)
            .is_some_and(|c| is_letter_or_digit(c) || is_emoji_at(self.text, offset - c.len_utf8()))
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
