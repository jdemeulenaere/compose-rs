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

/// `text`'s word around each of its emoji sequence's two ends, with the sequence at `from`.
fn assert_one_word(text: &str, from: usize, emoji: &str) {
    let to = from + emoji.len();
    assert_eq!(end(text, from), to, "{text}: its end");
    assert_eq!(start(text, to), from, "{text}: its start");
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorMultiLanguageTest.kt, emoji_zwjFamilySequence_treatedAsSingleWord
#[test]
fn a_joined_family_is_one_word() {
    let emoji = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    assert_one_word(&format!("family {emoji} photo"), 7, emoji);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorMultiLanguageTest.kt, emoji_skinToneModifierSequence_treatedAsSingleWord
#[test]
fn an_emoji_with_a_skin_tone_is_one_word() {
    let emoji = "\u{1F44D}\u{1F3FD}";
    assert_one_word(&format!("vote {emoji} yes"), 5, emoji);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorMultiLanguageTest.kt, emoji_regionalIndicatorFlagSequence_treatedAsSingleWord
#[test]
fn a_flag_is_one_word() {
    let emoji = "\u{1F1EF}\u{1F1F5}";
    assert_one_word(&format!("visit {emoji} now"), 6, emoji);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorMultiLanguageTest.kt, emoji_keycapSequence_treatedAsSingleWord
#[test]
fn a_keycap_is_one_word() {
    let emoji = "1\u{FE0F}\u{20E3}";
    assert_one_word(&format!("step {emoji} done"), 5, emoji);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorMultiLanguageTest.kt, emoji_japaneseKanjiWithAppendedEmoji_bridgesAsSingleWord
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorMultiLanguageTest.kt, emoji_arabicRtlWithAppendedEmoji_bridgesAsSingleWord
#[test]
fn an_emoji_bridges_to_the_letter_before_it() {
    for text in ["\u{732B}\u{1F431}", "\u{645}\u{628}\u{631}\u{648}\u{643}\u{1F389}"] {
        assert_eq!(end(text, 0), text.len());
        assert_eq!(start(text, text.len()), 0);
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorTest.kt, testGetPrevWordBeginningOnTwoWordsBoundary_emoji
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorTest.kt, testGetNextWordEndOnTwoWordBoundary_emoji
#[test]
fn emoji_sequences_bridge_to_the_letters_around_them() {
    let text = "a b\u{1F9D1}\u{1F3FF}\u{200D}\u{1F9B0}c\u{1F44D}\u{1F3FE} d";
    assert_eq!(start(text, of(text, '\u{1F3FE}')), of(text, 'b'));
    assert_eq!(end(text, of(text, 'b')), text.find(" d").unwrap());
}

/// A symbol that's no emoji, or is text unless a variation selector says otherwise, isn't bridged;
/// with the selector it is (D1114).
#[test]
fn only_what_shows_as_an_emoji_bridges() {
    for symbol in ["\u{2605}", "\u{2122}", "\u{2194}", "\u{2764}", "\u{260E}"] {
        let text = format!("foo{symbol}bar");
        assert_eq!(word_boundary(&text, 1), compose_ui::TextRange::new(0, 3), "{text}");
        let bar = text.find('b').unwrap();
        assert_eq!(
            word_boundary(&text, bar + 1),
            compose_ui::TextRange::new(bar, text.len()),
            "{text}"
        );
    }
    for symbol in ["\u{1F600}", "\u{2B50}", "\u{2764}\u{FE0F}", "\u{260E}\u{FE0F}"] {
        let text = format!("foo{symbol}bar");
        assert_eq!(word_boundary(&text, 1), compose_ui::TextRange::new(0, text.len()), "{text}");
    }
    // A joiner between letters is no emoji's, and a letter with a selector is no emoji.
    let joined = "a \u{200D}b";
    assert_eq!(
        word_boundary(joined, joined.len() - 1),
        compose_ui::TextRange::new(joined.len() - 1, joined.len())
    );
    let selected = "\u{5B57}a\u{FE0F}\u{5B57}";
    assert_eq!(end(selected, of(selected, 'a')), of(selected, 'a') + 4);
}

const THAI: &str = "\u{E20}\u{E32}\u{E29}\u{E32}\u{E44}\u{E17}\u{E22}";
const LAO: &str = "\u{E9E}\u{EB2}\u{EAA}\u{EB2}\u{EA5}\u{EB2}\u{EA7}";
const MYANMAR: &str = "\u{1019}\u{103C}\u{1014}\u{103A}\u{1019}\u{102C}\u{1005}\u{102C}";
const KHMER: &str = "\u{1797}\u{17B6}\u{179F}\u{17B6}\u{1781}\u{17D2}\u{1798}\u{17C2}\u{179A}";

/// A run of a script Unicode's word boundaries leave to a dictionary is one word until there's a
/// dictionary (D1114), where Compose's ICU finds its words: not a word per letter.
#[test]
fn a_run_of_a_script_without_a_dictionary_is_one_word() {
    for script in [THAI, LAO, MYANMAR, KHMER] {
        let text = format!("{script} abc");
        let inside = at(&text, 2);
        assert_eq!(
            word_boundary(&text, 0),
            compose_ui::TextRange::new(0, script.len()),
            "{script}"
        );
        assert_eq!(
            word_boundary(&text, inside),
            compose_ui::TextRange::new(0, script.len()),
            "{script}"
        );
        assert_eq!(
            word_boundary(&text, script.len() + 2),
            compose_ui::TextRange::new(script.len() + 1, text.len()),
            "{script}"
        );
    }
}

/// Its punctuation ends a word, another script does too.
#[test]
fn a_run_ends_at_punctuation_and_at_another_script() {
    // Thai's angkhankhu, a punctuation mark.
    let text = format!("{THAI}\u{E5A}{THAI}");
    assert_eq!(end(&text, 0), THAI.len());
    assert_eq!(start(&text, text.len() - 3), THAI.len() + 3);
    let text = format!("{LAO}{KHMER}");
    assert_eq!(end(&text, 0), LAO.len());
    let text = format!("abc{THAI}");
    assert_eq!(end(&text, 0), 3);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/selection/WordIteratorMultiLanguageTest.kt, emoji_thaiWordWithEmoji_segmentsWordAndEmoji
#[test]
fn a_thai_word_ending_in_a_mark_and_an_emoji_are_two_words() {
    let word = "\u{E2A}\u{E27}\u{E31}\u{E2A}\u{E14}\u{E35}";
    let text = format!("{word}\u{1F64F}");
    assert_eq!(end(&text, 0), word.len());
    assert_eq!(end(&text, word.len()), text.len());
}
