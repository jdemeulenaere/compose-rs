//! The pieces of a text a screen reader moves through (`text_segments.rs`, D1650).

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::RefCell;
use std::rc::Rc;

use compose_android::accessibility::find_shown;
use compose_android::text_segments::{Granularity, TextSegments};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_ui::platform::UiHost;
use compose_ui::*;

/// Where `c` is in `text`, in UTF-16 units.
fn at(text: &str, c: char) -> i32 {
    let bytes = text.find(c).expect("in the text");
    text[..bytes].encode_utf16().count() as i32
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, characterIterator_following, characterIterator_following_rtl
#[test]
fn characters_follow_one_by_one_in_either_direction_of_text() {
    let text = "abc";
    let segments = TextSegments::new(text);
    let next = |current| segments.following(Granularity::Character, current);
    assert_eq!(next(0), Some((0, 1)));
    assert_eq!(next(1), Some((1, 2)));
    assert_eq!(next(2), Some((2, 3)));
    assert_eq!(next(3), None);

    let hebrew = TextSegments::new("\u{5d0}\u{5d1}\u{5d2}");
    assert_eq!(hebrew.following(Granularity::Character, 0), Some((0, 1)));
    assert_eq!(hebrew.following(Granularity::Character, 3), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, characterIterator_preceding, characterIterator_preceding_rtl
#[test]
fn characters_precede_one_by_one_in_either_direction_of_text() {
    let segments = TextSegments::new("abc");
    let previous = |current| segments.preceding(Granularity::Character, current);
    assert_eq!(previous(3), Some((2, 3)));
    assert_eq!(previous(2), Some((1, 2)));
    assert_eq!(previous(1), Some((0, 1)));
    assert_eq!(previous(0), None);

    let hebrew = TextSegments::new("\u{5d0}\u{5d1}\u{5d2}");
    assert_eq!(hebrew.preceding(Granularity::Character, 3), Some((2, 3)));
    assert_eq!(hebrew.preceding(Granularity::Character, 0), None);
}

#[test]
fn a_character_is_what_the_reader_sees_and_is_counted_in_utf16() {
    // A flag: two scalars, four units. An accented letter: two scalars, two units.
    let text = "\u{1f1eb}\u{1f1f7}e\u{301}x";
    let segments = TextSegments::new(text);
    let next = |current| segments.following(Granularity::Character, current);
    assert_eq!(next(0), Some((0, 4)));
    assert_eq!(next(4), Some((4, 6)));
    assert_eq!(next(6), Some((6, 7)));
    // From inside one, the next.
    assert_eq!(next(1), Some((4, 6)));
    assert_eq!(segments.preceding(Granularity::Character, 7), Some((6, 7)));
    assert_eq!(segments.preceding(Granularity::Character, 4), Some((0, 4)));
    assert_eq!(segments.preceding(Granularity::Character, 5), Some((0, 4)));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, wordIterator_following
#[test]
fn words_follow_skipping_what_is_between_them() {
    let text = "abc def-ghi. jkl";
    let segments = TextSegments::new(text);
    let next = |current| segments.following(Granularity::Word, current);
    assert_eq!(next(0), Some((0, 3)));
    // In a word: the rest of it.
    assert_eq!(next(at(text, 'b')), Some((at(text, 'b'), 3)));
    // A dash breaks words, and what's between is skipped.
    assert_eq!(next(3), Some((at(text, 'd'), at(text, 'f') + 1)));
    assert_eq!(next(at(text, 'j')), Some((at(text, 'j'), at(text, 'l') + 1)));
    assert_eq!(next(segments.len()), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, wordIterator_preceding
#[test]
fn words_precede_skipping_what_is_between_them() {
    let text = "abc def-ghi. jkl";
    let segments = TextSegments::new(text);
    let previous = |current| segments.preceding(Granularity::Word, current);
    assert_eq!(previous(segments.len()), Some((at(text, 'j'), segments.len())));
    // In a word: what's of it before.
    assert_eq!(previous(at(text, 'h')), Some((at(text, 'g'), at(text, 'g') + 1)));
    assert_eq!(previous(at(text, 'd')), Some((0, 3)));
    assert_eq!(previous(0), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, wordIterator_following_rtl, wordIterator_preceding_rtl
#[test]
fn words_of_right_to_left_text_go_by_the_text_order() {
    let text = "\u{5d0}\u{5d1}\u{5d2} \u{5d3}\u{5d4}-\u{5d5}\u{5d6}. \u{5d7}\u{5d8}";
    let segments = TextSegments::new(text);
    let next = |current| segments.following(Granularity::Word, current);
    assert_eq!(next(0), Some((0, 3)));
    assert_eq!(next(at(text, '\u{5d3}')), Some((at(text, '\u{5d3}'), at(text, '-'))));
    assert_eq!(next(segments.len()), None);
    let previous = |current| segments.preceding(Granularity::Word, current);
    assert_eq!(previous(segments.len()), Some((at(text, '\u{5d7}'), segments.len())));
    assert_eq!(previous(at(text, '.') + 1), Some((at(text, '\u{5d5}'), at(text, '\u{5d6}') + 1)));
    assert_eq!(previous(0), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, paragraphIterator_following
#[test]
fn paragraphs_follow_between_new_lines() {
    let text = "abc\ndefg\nhijk.";
    let segments = TextSegments::new(text);
    let next = |current| segments.following(Granularity::Paragraph, current);
    assert_eq!(next(0), Some((0, 3)));
    assert_eq!(next(at(text, 'c') + 1), Some((at(text, 'd'), at(text, 'g') + 1)));
    assert_eq!(next(at(text, 'h')), Some((at(text, 'h'), segments.len())));
    assert_eq!(next(segments.len()), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, paragraphIterator_preceding
#[test]
fn paragraphs_precede_between_new_lines() {
    let text = "abc\ndefg\nhijk.";
    let segments = TextSegments::new(text);
    let previous = |current| segments.preceding(Granularity::Paragraph, current);
    assert_eq!(previous(segments.len()), Some((at(text, 'h'), segments.len())));
    assert_eq!(previous(at(text, 'h')), Some((at(text, 'd'), at(text, 'g') + 1)));
    assert_eq!(previous(0), None);
}

#[test]
fn empty_lines_are_no_paragraphs() {
    let text = "ab\n\n\ncd";
    let segments = TextSegments::new(text);
    assert_eq!(segments.following(Granularity::Paragraph, 2), Some((5, 7)));
    assert_eq!(segments.preceding(Granularity::Paragraph, 5), Some((0, 2)));
}

#[test]
fn more_than_one_separator_is_skipped_both_ways() {
    let text = "ab ,, cd";
    let segments = TextSegments::new(text);
    assert_eq!(segments.following(Granularity::Word, 2), Some((at(text, 'c'), segments.len())));
    assert_eq!(segments.preceding(Granularity::Word, at(text, 'c')), Some((0, 2)));
}

#[test]
fn a_run_of_letters_unicode_breaks_inside_is_one_word() {
    // Each ideograph is a word of its own by Unicode's rules.
    let text = "\u{4f60}\u{597d} \u{4e16}";
    let segments = TextSegments::new(text);
    assert_eq!(segments.following(Granularity::Word, 0), Some((0, 2)));
    assert_eq!(segments.following(Granularity::Word, 2), Some((3, 4)));
    assert_eq!(segments.preceding(Granularity::Word, 4), Some((3, 4)));
    assert_eq!(segments.preceding(Granularity::Word, 3), Some((0, 2)));
    assert_eq!(segments.preceding(Granularity::Word, 2), Some((0, 2)));
}

#[test]
fn nothing_is_moved_through_in_an_empty_text() {
    let segments = TextSegments::new("");
    assert_eq!(segments.following(Granularity::Character, 0), None);
    assert_eq!(segments.preceding(Granularity::Word, 0), None);
    assert_eq!(segments.following(Granularity::Paragraph, 0), None);
}

/// `text`'s layout, wrapped in a text `width` wide.
fn laid_out(text: &'static str, width: Dp) -> (UiHost, TextLayoutResult) {
    let result: Rc<RefCell<Option<TextLayoutResult>>> = Rc::default();
    let heard = result.clone();
    let mut host = UiHost::headless(400, 800);
    host.set_content(move || {
        let heard = heard.clone();
        BasicText!(
            text,
            modifier = Modifier.width(width),
            on_text_layout = Some(move |layout| *heard.borrow_mut() = Some(layout))
        );
    });
    host.settle(5);
    let layout = result.borrow().clone().expect("laid out");
    (host, layout)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, lineIterator_following
#[test]
fn lines_follow_from_the_start_of_one_or_the_next_from_inside_it() {
    let text = "abcdefgh";
    let (_host, layout) = laid_out(text, Dp(20.0));
    assert!(layout.line_count >= 3, "{} lines", layout.line_count);
    let segments = TextSegments::new(text);
    let line = |offset: i32| layout.get_line_for_offset(offset as usize);

    let first = segments.following(Granularity::Line(&layout), 0).expect("a line");
    assert_eq!(first.0, 0);
    assert_eq!(line(first.1 - 1), 0);
    assert_eq!(line(first.1), 1);

    // In the middle of line 1: line 2.
    let next = segments.following(Granularity::Line(&layout), first.1 + 1).expect("a line");
    assert_eq!(line(next.0), 2);
    assert_eq!(line(next.1 - 1), 2);
    assert_eq!(line(next.1), 3);
    assert_eq!(segments.following(Granularity::Line(&layout), segments.len()), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, lineIterator_preceding
#[test]
fn lines_precede_from_the_end_of_one_or_the_one_before_from_inside_it() {
    let text = "abcdefgh";
    let (_host, layout) = laid_out(text, Dp(20.0));
    let segments = TextSegments::new(text);
    let line = |offset: i32| layout.get_line_for_offset(offset as usize);

    let last = segments.preceding(Granularity::Line(&layout), segments.len()).expect("a line");
    let last_line = line(last.0);
    assert_eq!(line(last.0 - 1), last_line - 1);
    assert_eq!(line(last.1), last_line);

    // In the middle of the last line: the one before.
    let before = segments.preceding(Granularity::Line(&layout), last.0 + 1).expect("a line");
    assert_eq!(line(before.0), last_line - 1);
    assert_eq!(line(before.0 - 1), last_line - 2);
    assert_eq!(line(before.1 - 1), last_line - 1);
    assert_eq!(line(before.1), last_line);
    assert_eq!(segments.preceding(Granularity::Line(&layout), 0), None);
}

/// A field of a long text in a scroller: its layout, its text, and how high it's shown, a page.
fn long_text() -> (UiHost, TextLayoutResult, &'static str, f32) {
    let text: String = (0..500).map(|line| format!("Line: {line}")).collect::<Vec<_>>().join("\n");
    let text: &'static str = text.leak();
    let mut host = UiHost::headless(400, 800);
    host.set_content(move || {
        let scroll = compose_foundation::remember_scroll_state!();
        Column!(modifier = Modifier.fill_max_size().vertical_scroll(scroll), {
            BasicTextField!(String::from(text), |_| {}, modifier = Modifier.test_tag("field"));
        });
    });
    host.settle(5);
    let root = host.accessibility_tree();
    let node = by_tag(&root, "field").expect("tagged");
    let (_, shown) = find_shown(&root, node.semantics_id).expect("there");
    let mut layouts = Vec::new();
    host.perform_semantics_action_with(node.id, &SemanticsActions::GET_TEXT_LAYOUT_RESULT, |get| {
        get(&mut layouts)
    });
    let layout = layouts.remove(0);
    (host, layout, text, shown.height())
}

fn by_tag<'a>(
    node: &'a compose_ui::platform::SemanticsSnapshot,
    tag: &str,
) -> Option<&'a compose_ui::platform::SemanticsSnapshot> {
    if node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|it| it.as_str() == tag) {
        return Some(node);
    }
    node.children.iter().find_map(|child| by_tag(child, tag))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, pageIterator_following
#[test]
fn a_page_following_is_about_the_height_of_the_node() {
    let (_host, layout, text, height) = long_text();
    let segments = TextSegments::new(text);
    let range = segments.following(Granularity::Page(&layout, height), 0).expect("a page");
    let (first, last) = (
        layout.get_line_for_offset(range.0 as usize),
        layout.get_line_for_offset(range.1 as usize),
    );
    let line_height = layout.get_line_bottom(last) - layout.get_line_top(last);
    let step = layout.get_line_top(last) - layout.get_line_top(first);
    assert!((step - height).abs() < line_height, "{step} about {height}");
    assert_eq!(segments.following(Granularity::Page(&layout, height), segments.len()), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt, pageIterator_preceding
#[test]
fn a_page_preceding_is_about_the_height_of_the_node() {
    let (_host, layout, text, height) = long_text();
    let segments = TextSegments::new(text);
    let range =
        segments.preceding(Granularity::Page(&layout, height), segments.len()).expect("a page");
    let (first, last) = (
        layout.get_line_for_offset(range.0 as usize),
        layout.get_line_for_offset(range.1 as usize),
    );
    let line_height = layout.get_line_bottom(last) - layout.get_line_top(last);
    let step = layout.get_line_top(last) - layout.get_line_top(first);
    assert!((step - height).abs() < line_height, "{step} about {height}");
    assert_eq!(segments.preceding(Granularity::Page(&layout, height), 0), None);
}
