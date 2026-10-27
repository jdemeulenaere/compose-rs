//! Words around an offset, adapted from Compose's `WordBoundaryTest` (its `WordIterator`'s
//! `getWordStart` and `getWordEnd`, which `getWordBoundary` pairs): byte offsets, the start and
//! end read from `word_boundary`'s range.

use compose_ui::platform::word_boundary;

/// The byte offset of character `index` of `text`.
fn at(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map_or(text.len(), |(offset, _)| offset)
}

/// The byte offset of `c`'s first occurrence in `text`.
fn of(text: &str, c: char) -> usize {
    text.find(c).expect("in the text")
}

fn start(text: &str, offset: usize) -> usize {
    word_boundary(text, offset).start
}

fn end(text: &str, offset: usize) -> usize {
    word_boundary(text, offset).end
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordStart_out_of_boundary_too_big
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordEnd_out_of_boundary_too_big
#[test]
#[should_panic(expected = "Invalid offset")]
fn a_word_past_the_text_panics() {
    word_boundary("text", 5);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordStart_Empty_String
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordEnd_Empty_String
#[test]
fn the_word_of_an_empty_text_is_empty() {
    assert_eq!(start("", 0), 0);
    assert_eq!(end("", 0), 0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordStart
#[test]
fn word_starts() {
    let text = "abc def-ghi. jkl";
    assert_eq!(start(text, of(text, 'a')), of(text, 'a'));
    assert_eq!(start(text, of(text, 'c')), of(text, 'a'));
    assert_eq!(start(text, of(text, ' ')), of(text, 'a'));
    assert_eq!(start(text, of(text, 'd')), of(text, 'd'));
    assert_eq!(start(text, of(text, 'i')), of(text, 'g'));
    assert_eq!(start(text, of(text, 'k')), of(text, 'j'));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordEnd
#[test]
fn word_ends() {
    let text = "abc def-ghi. jkl";
    assert_eq!(end(text, of(text, 'a')), of(text, ' '));
    assert_eq!(end(text, of(text, 'c')), of(text, ' '));
    assert_eq!(end(text, of(text, ' ')), of(text, ' '));
    assert_eq!(end(text, of(text, 'd')), of(text, '-'));
    assert_eq!(end(text, of(text, 'i')), of(text, '.'));
    assert_eq!(end(text, of(text, 'k')), of(text, 'l') + 1);
}

const HEBREW: &str = "\u{05d0}\u{05d1}\u{05d2} \u{05d3}\u{05d4}-\u{05d5}\u{05d6}. \u{05d7}\u{05d8}";

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordStart_RTL
#[test]
fn word_starts_right_to_left() {
    let text = HEBREW;
    assert_eq!(start(text, of(text, '\u{05d0}')), of(text, '\u{05d0}'));
    assert_eq!(start(text, of(text, '\u{05d2}')), of(text, '\u{05d0}'));
    assert_eq!(start(text, of(text, ' ')), of(text, '\u{05d0}'));
    assert_eq!(start(text, of(text, '\u{05d4}')), of(text, '\u{05d3}'));
    assert_eq!(start(text, of(text, '-')), of(text, '\u{05d3}'));
    assert_eq!(start(text, of(text, '\u{05d5}')), of(text, '-'));
    assert_eq!(start(text, of(text, '\u{05d6}')), of(text, '\u{05d5}'));
    assert_eq!(start(text, of(text, '\u{05d7}')), of(text, '\u{05d7}'));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordEnd_RTL
#[test]
fn word_ends_right_to_left() {
    let text = HEBREW;
    assert_eq!(end(text, of(text, '\u{05d0}')), of(text, ' '));
    assert_eq!(end(text, of(text, '\u{05d2}')), of(text, ' '));
    assert_eq!(end(text, of(text, ' ')), of(text, ' '));
    assert_eq!(end(text, of(text, '\u{05d4}')), of(text, '-'));
    assert_eq!(end(text, of(text, '-')), of(text, '-') + 1);
    assert_eq!(end(text, of(text, '\u{05d5}')), of(text, '.'));
    assert_eq!(end(text, of(text, '\u{05d6}')), of(text, '.'));
    assert_eq!(end(text, of(text, '\u{05d7}')), text.len());
}

const KANA: &str = "\u{3042}\u{30A2}\u{30A3}\u{30A4}";

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordStart_CJK
#[test]
fn word_starts_in_kana() {
    let text = KANA;
    assert_eq!(start(text, at(text, 0)), at(text, 0));
    assert_eq!(start(text, at(text, 1)), at(text, 0));
    assert_eq!(start(text, at(text, 3)), at(text, 1));
    assert_eq!(start(text, text.len()), at(text, 1));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordEnd_CJK
#[test]
fn word_ends_in_kana() {
    let text = KANA;
    assert_eq!(end(text, at(text, 0)), at(text, 1));
    assert_eq!(end(text, at(text, 1)), text.len());
    assert_eq!(end(text, at(text, 3)), text.len());
    assert_eq!(end(text, text.len()), text.len());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordStart_apostropheMiddleOfWord
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordEnd_apostropheMiddleOfWord
#[test]
fn an_apostrophe_inside_a_word_keeps_it_one_word() {
    let text = "isn't he";
    for c in ['i', 'n', '\'', 't'] {
        assert_eq!(start(text, of(text, c)), 0, "{c}");
        assert_eq!(end(text, of(text, c)), of(text, 't') + 1, "{c}");
    }
    assert_eq!(start(text, of(text, 't') + 1), 0);
    assert_eq!(start(text, of(text, 'h')), of(text, 'h'));
    assert_eq!(end(text, of(text, 'h')), of(text, 'e') + 1);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordStart_isOnPunctuation
#[test]
fn word_starts_on_punctuation_group_it() {
    let text = "abc!? (^^;) def";
    assert_eq!(start(text, of(text, '!')), of(text, 'a'));
    assert_eq!(start(text, of(text, '?') + 1), of(text, '!'));
    assert_eq!(start(text, of(text, ';')), of(text, ';'));
    assert_eq!(start(text, of(text, ')')), of(text, ';'));
    assert_eq!(start(text, text.len()), of(text, 'd'));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordBoundaryTest.kt, testGetWordEnd_isOnPunctuation
#[test]
fn word_ends_on_punctuation_group_it() {
    let text = "abc!? (^^;) def";
    assert_eq!(end(text, of(text, 'a')), of(text, '!'));
    assert_eq!(end(text, of(text, '?') + 1), of(text, '?') + 1);
    assert_eq!(end(text, of(text, '(')), of(text, '(') + 1);
    assert_eq!(end(text, of(text, '(') + 2), of(text, '(') + 2);
    assert_eq!(end(text, of(text, ')') + 1), of(text, ')') + 1);
    assert_eq!(end(text, of(text, 'd')), text.len());
    assert_eq!(end(text, text.len()), text.len());
}

/// Marks and viramas belong to their letter's word (Unicode's word boundaries, as ICU's).
#[test]
fn marks_and_viramas_stay_in_their_word() {
    // "é" as e and a combining acute, then Devanagari "क्ष" (ka, virama, ssa).
    let text = "cafe\u{0301} \u{0915}\u{094D}\u{0937}";
    assert_eq!(word_boundary(text, 1), compose_ui::TextRange::new(0, 6));
    let devanagari = of(text, '\u{0915}');
    assert_eq!(word_boundary(text, devanagari), compose_ui::TextRange::new(devanagari, text.len()));
}
