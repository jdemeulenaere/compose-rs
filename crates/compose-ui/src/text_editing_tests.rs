//! Compose's tests of the editing buffer, the edit commands and the edit processor (they use the
//! buffer's internal operations, as Compose's do). Compose's offsets are UTF-16 units; here they
//! are bytes (D1100), so a surrogate pair's 2 units are 4 bytes, and the emoji below are 4 bytes
//! each (their indices are twice Compose's).

use super::*;

const CH1: &str = "\u{1F600}";
const CH2: &str = "\u{1F601}";
const CH3: &str = "\u{1F602}";
const CH4: &str = "\u{1F603}";
const CH5: &str = "\u{1F604}";
/// Man, woman, girl, boy joined by zero-width joiners: one grapheme cluster.
const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";

fn five_emoji() -> String {
    format!("{CH1}{CH2}{CH3}{CH4}{CH5}")
}

fn buffer(text: &str, selection: TextRange) -> EditingBuffer {
    EditingBuffer::new(text, selection)
}

#[track_caller]
fn assert_buffer(
    buffer: &EditingBuffer,
    text: &str,
    cursor: Option<usize>,
    composition: Option<(usize, usize)>,
) {
    assert_eq!(buffer.to_string(), text);
    assert_eq!(buffer.cursor(), cursor);
    assert_eq!(buffer.composition(), composition.map(|(start, end)| TextRange::new(start, end)));
}

#[track_caller]
fn assert_selection(buffer: &EditingBuffer, start: usize, end: usize) {
    assert_eq!((buffer.selection_start(), buffer.selection_end()), (start, end));
}

// --- EditingBufferTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, insert
#[test]
fn inserting_puts_the_cursor_after_the_inserted_text() {
    let mut eb = buffer("", TextRange::ZERO);
    eb.replace(0, 0, "A");
    assert_buffer(&eb, "A", Some(1), None);
    assert_selection(&eb, 1, 1);
    eb.replace(1, 1, "BC");
    assert_buffer(&eb, "ABC", Some(3), None);
    eb.replace(1, 1, "D");
    assert_buffer(&eb, "ADBC", Some(2), None);
    assert_selection(&eb, 2, 2);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete
#[test]
fn replacing_with_nothing_deletes() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.replace(0, 1, "");
    assert_buffer(&eb, "BCDE", Some(0), None);
    eb.replace(0, 2, "");
    assert_buffer(&eb, "DE", Some(0), None);
    eb.replace(1, 2, "");
    assert_buffer(&eb, "D", Some(1), None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, setSelection
#[test]
fn setting_the_selection_leaves_no_cursor_until_a_replacement() {
    let mut eb = buffer("ABCDE", TextRange::new(0, 3));
    assert_buffer(&eb, "ABCDE", None, None);
    assert_selection(&eb, 0, 3);
    eb.set_selection(0, 5);
    assert_buffer(&eb, "ABCDE", None, None);
    assert_selection(&eb, 0, 5);
    eb.replace(0, 3, "X");
    assert_buffer(&eb, "XDE", Some(1), None);
    eb.set_selection(0, 2);
    assert_buffer(&eb, "XDE", None, None);
    assert_selection(&eb, 0, 2);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, setSelection_throws_whenNegativeStart
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, setSelection_throws_whenNegativeEnd
// (offsets can't be negative here; out of the text is the same failure)
#[test]
#[should_panic(expected = "offset is outside of text region")]
fn a_selection_outside_the_text_panics() {
    buffer("ABCDE", TextRange::ZERO).set_selection(0, 6);
}

#[test]
#[should_panic(expected = "inside a character")]
fn a_selection_inside_a_character_panics() {
    buffer(CH1, TextRange::ZERO).set_selection(0, 1);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, setCompostion_and_cancelComposition
#[test]
fn a_replacement_ends_the_composition_and_cancelling_removes_it() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.set_composition(0, 5);
    assert_buffer(&eb, "ABCDE", Some(0), Some((0, 5)));
    eb.replace(2, 3, "X");
    assert_buffer(&eb, "ABXDE", Some(3), None);
    eb.set_composition(2, 4);
    assert_buffer(&eb, "ABXDE", Some(3), Some((2, 4)));
    eb.cancel_composition();
    assert_buffer(&eb, "ABE", Some(2), None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, setCompostion_and_commitComposition
#[test]
fn committing_keeps_the_composed_text() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.set_composition(0, 5);
    eb.replace(2, 3, "X");
    assert_buffer(&eb, "ABXDE", Some(3), None);
    eb.set_composition(2, 4);
    assert_buffer(&eb, "ABXDE", Some(3), Some((2, 4)));
    eb.commit_composition();
    assert_buffer(&eb, "ABXDE", Some(3), None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, setCursor_and_get_cursor
#[test]
fn setting_the_cursor_collapses_the_selection_there() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    for cursor in [1, 2, 5] {
        eb.set_cursor(cursor);
        assert_buffer(&eb, "ABCDE", Some(cursor), None);
        assert_selection(&eb, cursor, cursor);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_preceding_cursor_no_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_trailing_cursor_no_composition
#[test]
fn deleting_moves_a_cursor_after_the_deletion_back() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.delete(1, 2);
    assert_buffer(&eb, "ACDE", Some(0), None);
    let mut eb = buffer("ABCDE", TextRange::at(3));
    eb.delete(1, 2);
    assert_buffer(&eb, "ACDE", Some(2), None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_preceding_selection_no_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_trailing_selection_no_composition
#[test]
fn deleting_moves_a_selection_after_the_deletion_back() {
    let mut eb = buffer("ABCDE", TextRange::new(0, 1));
    eb.delete(1, 2);
    assert_eq!(eb.to_string(), "ACDE");
    assert_selection(&eb, 0, 1);
    let mut eb = buffer("ABCDE", TextRange::new(4, 5));
    eb.delete(1, 2);
    assert_eq!(eb.to_string(), "ACDE");
    assert_selection(&eb, 3, 4);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_covered_cursor
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_covered_selection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_intersects_first_half_of_selection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_intersects_second_half_of_selection
#[test]
fn deleting_over_the_selection_shrinks_it() {
    let mut eb = buffer("ABCDE", TextRange::at(2));
    eb.delete(1, 3);
    assert_eq!(eb.to_string(), "ADE");
    assert_selection(&eb, 1, 1);
    let mut eb = buffer("ABCDE", TextRange::new(1, 3));
    eb.delete(0, 4);
    assert_eq!(eb.to_string(), "E");
    assert_selection(&eb, 0, 0);
    let mut eb = buffer("ABCDE", TextRange::new(2, 4));
    eb.delete(1, 3);
    assert_eq!(eb.to_string(), "ADE");
    assert_selection(&eb, 1, 2);
    let mut eb = buffer("ABCDEFG", TextRange::new(1, 4));
    eb.delete(3, 5);
    assert_eq!(eb.to_string(), "ABCFG");
    assert_selection(&eb, 1, 3);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_preceding_composition_no_intersection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_trailing_composition_no_intersection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_preceding_composition_intersection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_trailing_composition_intersection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_composition_contains_delrange
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferTest.kt, delete_delrange_contains_composition
#[test]
fn deleting_moves_and_shrinks_the_composition() {
    let cases = [
        ((1, 2), (2, 3), "ABDE", Some((1, 2))),
        ((3, 4), (2, 3), "ABDE", Some((2, 3))),
        ((1, 3), (2, 4), "ABE", Some((1, 2))),
        ((3, 5), (2, 4), "ABE", Some((2, 3))),
        ((2, 5), (3, 4), "ABCE", Some((2, 4))),
        ((3, 4), (2, 5), "AB", None),
    ];
    for (composition, deleted, text, expected) in cases {
        let mut eb = buffer("ABCDE", TextRange::ZERO);
        eb.set_composition(composition.0, composition.1);
        eb.delete(deleted.0, deleted.1);
        assert_buffer(&eb, text, Some(0), expected);
    }
}

// --- EditingBufferDeleteRangeTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferDeleteRangeTest.kt, test_does_not_intersect_deleted_is_after_the_target
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferDeleteRangeTest.kt, test_does_not_intersect_deleted_is_before_the_target
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferDeleteRangeTest.kt, test_deleted_covers_target
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferDeleteRangeTest.kt, test_target_covers_deleted
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferDeleteRangeTest.kt, test_deleted_same_as_target
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferDeleteRangeTest.kt, test_deleted_covers_first_half_of_target
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferDeleteRangeTest.kt, test_deleted_covers_second_half_of_target
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditingBufferDeleteRangeTest.kt, test_delete_trailing_cursor
#[test]
fn a_range_moves_and_shrinks_with_a_deletion() {
    let cases = [
        ((0, 1), (2, 3), (0, 1)),
        ((4, 5), (0, 2), (2, 3)),
        ((1, 2), (0, 3), (0, 0)),
        ((0, 3), (1, 2), (0, 2)),
        ((1, 2), (1, 2), (1, 1)),
        ((1, 4), (0, 2), (0, 2)),
        ((1, 4), (3, 5), (1, 3)),
        ((3, 3), (1, 2), (2, 2)),
    ];
    for (target, deleted, expected) in cases {
        assert_eq!(
            update_range_after_delete(
                TextRange::new(target.0, target.1),
                TextRange::new(deleted.0, deleted.1)
            ),
            TextRange::new(expected.0, expected.1),
            "target {target:?}, deleted {deleted:?}"
        );
    }
}

// --- CommitTextCommandTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_empty
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_cursor_tail
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_cursor_head
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_cursor_far_tail
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_cursor_far_head
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_empty_text_cursor_head
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_empty_text_cursor_tail
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_empty_text_cursor_far_tail
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_insert_empty_text_cursor_far_head
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_cursor_position_too_small
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_cursor_position_too_large
#[test]
fn committed_text_is_inserted_with_the_cursor_where_asked() {
    // (text, cursor, committed, new cursor position, text after, cursor after)
    let cases: [(&str, usize, &str, isize, &str, usize); 11] = [
        ("", 0, "X", 1, "X", 1),
        ("A", 1, "X", 1, "AX", 2),
        ("A", 1, "X", 0, "AX", 1),
        ("ABCDE", 1, "X", 2, "AXBCDE", 3),
        ("ABCDE", 4, "X", -2, "ABCDXE", 2),
        ("ABCDE", 1, "", 0, "ABCDE", 1),
        ("ABCDE", 1, "", 1, "ABCDE", 1),
        ("ABCDE", 1, "", 2, "ABCDE", 2),
        ("ABCDE", 4, "", -2, "ABCDE", 2),
        ("ABCDE", 5, "X", -1000, "ABCDEX", 0),
        ("ABCDE", 5, "X", 1000, "ABCDEX", 6),
    ];
    for (text, cursor, committed, position, after, cursor_after) in cases {
        let mut eb = buffer(text, TextRange::at(cursor));
        CommitTextCommand::new(committed, position).apply_to(&mut eb);
        assert_buffer(&eb, after, Some(cursor_after), None);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_cancel_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_replace_selection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/CommitTextCommandTest.kt, test_composition_and_selection
#[test]
fn committed_text_replaces_the_composition_or_else_the_selection() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.set_composition(1, 4);
    CommitTextCommand::new("X", 1).apply_to(&mut eb);
    assert_buffer(&eb, "AXE", Some(2), None);

    let mut eb = buffer("ABCDE", TextRange::new(1, 4));
    CommitTextCommand::new("X", 1).apply_to(&mut eb);
    assert_buffer(&eb, "AXE", Some(2), None);

    let mut eb = buffer("ABCDE", TextRange::new(1, 3));
    eb.set_composition(2, 4);
    CommitTextCommand::new("X", 1).apply_to(&mut eb);
    assert_buffer(&eb, "ABXE", Some(3), None);
}

#[test]
fn a_relative_cursor_inside_a_character_goes_to_its_start() {
    let mut eb = buffer("", TextRange::ZERO);
    // "é" is 2 bytes: one byte back from its end is inside it.
    CommitTextCommand::new("\u{E9}", 0).apply_to(&mut eb);
    assert_buffer(&eb, "\u{E9}", Some(0), None);
    let mut eb = buffer("a\u{E9}", TextRange::at(0));
    CommitTextCommand::new("", 3).apply_to(&mut eb);
    assert_buffer(&eb, "a\u{E9}", Some(1), None);
}

// --- SetComposingTextCommandTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_empty
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_cursor_tail
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_cursor_head
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_cursor_far_tail
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_cursor_far_head
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_empty_text_cursor_head
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_empty_text_cursor_tail
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_empty_text_cursor_far_tail
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_insert_empty_text_cursor_far_head
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_cursor_position_too_small
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_cursor_position_too_large
#[test]
fn composing_text_is_inserted_as_the_composition() {
    // (text, cursor, composed, new cursor position, text after, cursor after, composition)
    type Case =
        (&'static str, usize, &'static str, isize, &'static str, usize, Option<(usize, usize)>);
    let cases: [Case; 11] = [
        ("", 0, "X", 1, "X", 1, Some((0, 1))),
        ("A", 1, "X", 1, "AX", 2, Some((1, 2))),
        ("A", 1, "X", 0, "AX", 1, Some((1, 2))),
        ("ABCDE", 1, "X", 2, "AXBCDE", 3, Some((1, 2))),
        ("ABCDE", 4, "X", -2, "ABCDXE", 2, Some((4, 5))),
        ("ABCDE", 1, "", 0, "ABCDE", 1, None),
        ("ABCDE", 1, "", 1, "ABCDE", 1, None),
        ("ABCDE", 1, "", 2, "ABCDE", 2, None),
        ("ABCDE", 4, "", -2, "ABCDE", 2, None),
        ("ABCDE", 5, "X", -1000, "ABCDEX", 0, Some((5, 6))),
        ("ABCDE", 5, "X", 1000, "ABCDEX", 6, Some((5, 6))),
    ];
    for (text, cursor, composed, position, after, cursor_after, composition) in cases {
        let mut eb = buffer(text, TextRange::at(cursor));
        SetComposingTextCommand::new(composed, position).apply_to(&mut eb);
        assert_buffer(&eb, after, Some(cursor_after), composition);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_cancel_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_replace_selection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingTextCommandTest.kt, test_composition_and_selection
#[test]
fn composing_text_replaces_the_composition_or_else_the_selection() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.set_composition(1, 4);
    SetComposingTextCommand::new("X", 1).apply_to(&mut eb);
    assert_buffer(&eb, "AXE", Some(2), Some((1, 2)));

    let mut eb = buffer("ABCDE", TextRange::new(1, 4));
    SetComposingTextCommand::new("X", 1).apply_to(&mut eb);
    assert_buffer(&eb, "AXE", Some(2), Some((1, 2)));

    let mut eb = buffer("ABCDE", TextRange::new(1, 3));
    eb.set_composition(2, 4);
    SetComposingTextCommand::new("X", 1).apply_to(&mut eb);
    assert_buffer(&eb, "ABXE", Some(3), Some((2, 3)));
}

// --- SetComposingRegionCommandTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingRegionCommandTest.kt, test_set
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingRegionCommandTest.kt, test_set_reversed
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingRegionCommandTest.kt, test_set_too_small
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingRegionCommandTest.kt, test_set_too_large
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingRegionCommandTest.kt, test_set_too_small_and_too_large
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingRegionCommandTest.kt, test_set_too_small_and_too_large_reversed
#[test]
fn a_composing_region_is_kept_in_the_text_and_ordered() {
    let cases = [
        ((1, 4), Some((1, 4))),
        ((4, 1), Some((1, 4))),
        ((-1000, -1000), None),
        ((1000, 1000), None),
        ((-1000, 1000), Some((0, 5))),
        ((1000, -1000), Some((0, 5))),
    ];
    for ((start, end), composition) in cases {
        let mut eb = buffer("ABCDE", TextRange::ZERO);
        SetComposingRegionCommand::new(start, end).apply_to(&mut eb);
        assert_buffer(&eb, "ABCDE", Some(0), composition);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingRegionCommandTest.kt, test_preserve_ongoing_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetComposingRegionCommandTest.kt, test_preserve_selection
#[test]
fn a_composing_region_replaces_the_composition_and_keeps_the_selection() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.set_composition(1, 3);
    SetComposingRegionCommand::new(2, 4).apply_to(&mut eb);
    assert_buffer(&eb, "ABCDE", Some(0), Some((2, 4)));

    let mut eb = buffer("ABCDE", TextRange::new(1, 4));
    SetComposingRegionCommand::new(2, 4).apply_to(&mut eb);
    assert_selection(&eb, 1, 4);
    assert_eq!(eb.composition(), Some(TextRange::new(2, 4)));
}

#[test]
fn a_composing_region_inside_characters_widens_to_them() {
    let text = five_emoji();
    let mut eb = buffer(&text, TextRange::ZERO);
    SetComposingRegionCommand::new(5, 6).apply_to(&mut eb);
    assert_eq!(eb.composition(), Some(TextRange::new(4, 8)));
}

// --- SetSelectionCommandTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetSelectionCommandTest.kt, test_set
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetSelectionCommandTest.kt, test_cancel_ongoing_selection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetSelectionCommandTest.kt, test_set_reversed
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetSelectionCommandTest.kt, test_set_too_small
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetSelectionCommandTest.kt, test_set_too_large
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetSelectionCommandTest.kt, test_set_too_small_too_large
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetSelectionCommandTest.kt, test_set_too_small_too_large_reversed
#[test]
fn a_selection_is_kept_in_the_text_and_ordered() {
    let cases = [
        (TextRange::ZERO, (1, 4), (1, 4)),
        (TextRange::new(1, 4), (2, 5), (2, 5)),
        (TextRange::ZERO, (4, 1), (1, 4)),
        (TextRange::ZERO, (-1000, -1000), (0, 0)),
        (TextRange::ZERO, (1000, 1000), (5, 5)),
        (TextRange::ZERO, (-1000, 1000), (0, 5)),
        (TextRange::ZERO, (1000, -1000), (0, 5)),
    ];
    for (initial, (start, end), (expected_start, expected_end)) in cases {
        let mut eb = buffer("ABCDE", initial);
        SetSelectionCommand::new(start, end).apply_to(&mut eb);
        assert_eq!(eb.to_string(), "ABCDE");
        assert_selection(&eb, expected_start, expected_end);
        assert!(!eb.has_composition());
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/SetSelectionCommandTest.kt, test_preserve_ongoing_composition
#[test]
fn a_selection_keeps_the_composition() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.set_composition(1, 3);
    SetSelectionCommand::new(2, 4).apply_to(&mut eb);
    assert_selection(&eb, 2, 4);
    assert_eq!(eb.composition(), Some(TextRange::new(1, 3)));
}

// --- FinishComposingTextCommandTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/FinishComposingTextCommandTest.kt, test_set
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/FinishComposingTextCommandTest.kt, test_preserve_selection
#[test]
fn finishing_the_composition_keeps_its_text_and_the_selection() {
    let mut eb = buffer("ABCDE", TextRange::ZERO);
    eb.set_composition(1, 4);
    FinishComposingTextCommand.apply_to(&mut eb);
    assert_buffer(&eb, "ABCDE", Some(0), None);

    let mut eb = buffer("ABCDE", TextRange::new(1, 4));
    eb.set_composition(2, 5);
    FinishComposingTextCommand.apply_to(&mut eb);
    assert_selection(&eb, 1, 4);
    assert!(!eb.has_composition());
}

// --- DeleteSurroundingTextCommandTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_after
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_before
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_both
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_after_multiple
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_before_multiple
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_both_multiple
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_before_too_many
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_after_too_many
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_both_too_many
#[test]
fn surrounding_text_is_deleted_around_the_cursor() {
    let cases = [
        (1, (0, 1), "ACDE", 1),
        (1, (1, 0), "BCDE", 0),
        (3, (1, 1), "ABE", 2),
        (2, (0, 2), "ABE", 2),
        (3, (2, 0), "ADE", 1),
        (3, (2, 2), "A", 1),
        (3, (1000, 0), "DE", 0),
        (3, (0, 1000), "ABC", 3),
        (3, (1000, 1000), "", 0),
    ];
    for (cursor, (before, after), text, cursor_after) in cases {
        let mut eb = buffer("ABCDE", TextRange::at(cursor));
        DeleteSurroundingTextCommand::new(before, after).apply_to(&mut eb);
        assert_buffer(&eb, text, Some(cursor_after), None);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_selection_preserve
#[test]
fn surrounding_text_is_deleted_around_the_selection() {
    let mut eb = buffer("ABCDE", TextRange::new(2, 4));
    DeleteSurroundingTextCommand::new(1, 1).apply_to(&mut eb);
    assert_eq!(eb.to_string(), "ACD");
    assert_selection(&eb, 1, 3);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_composition_no_intersection_preceding_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_composition_no_intersection_trailing_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_composition_intersection_preceding_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_composition_intersection_trailing_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_covered_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, test_delete_composition_covered
#[test]
fn deleting_surrounding_text_moves_the_composition() {
    let cases = [
        ((0, 1), Some((0, 1))),
        ((4, 5), Some((2, 3))),
        ((0, 3), Some((0, 2))),
        ((3, 5), Some((2, 3))),
        ((2, 3), None),
        ((0, 5), Some((0, 3))),
    ];
    for (composition, expected) in cases {
        let mut eb = buffer("ABCDE", TextRange::at(3));
        eb.set_composition(composition.0, composition.1);
        DeleteSurroundingTextCommand::new(1, 1).apply_to(&mut eb);
        assert_buffer(&eb, "ABE", Some(2), expected);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, deletes_whenLengthAfterCursorOverflows_withMaxValue
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, deletes_whenLengthBeforeCursorOverflows_withMaxValue
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, deletes_whenLengthAfterCursorOverflows
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextCommandTest.kt, deletes_whenLengthBeforeCursorOverflows
// (the negative-length tests, throws_whenLengthBeforeInvalid and throws_whenLengthAfterInvalid, have no
// counterpart: lengths are unsigned)
#[test]
fn surrounding_lengths_that_overflow_delete_to_the_ends() {
    for length in [usize::MAX, usize::MAX - 1] {
        let mut eb = buffer("abcde", TextRange::at(4));
        DeleteSurroundingTextCommand::new(0, length).apply_to(&mut eb);
        assert_buffer(&eb, "abcd", Some(4), None);
        let mut eb = buffer("abcde", TextRange::at(1));
        DeleteSurroundingTextCommand::new(length, 0).apply_to(&mut eb);
        assert_buffer(&eb, "bcde", Some(0), None);
    }
}

#[test]
fn surrounding_lengths_reaching_into_a_character_delete_all_of_it() {
    let text = five_emoji();
    let mut eb = buffer(&text, TextRange::at(8));
    DeleteSurroundingTextCommand::new(2, 2).apply_to(&mut eb);
    assert_buffer(&eb, &format!("{CH1}{CH4}{CH5}"), Some(4), None);
}

// --- DeleteSurroundingTextInCodePointsCommandTest (every emoji here is 4 bytes, 2 UTF-16 units)

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_after
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_before
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_both
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_after_multiple
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_before_multiple
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_both_multiple
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_before_too_many
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_after_too_many
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_both_too_many
#[test]
fn surrounding_code_points_are_deleted_around_the_cursor() {
    let cases = [
        (4, (0, 1), format!("{CH1}{CH3}{CH4}{CH5}"), 4),
        (4, (1, 0), format!("{CH2}{CH3}{CH4}{CH5}"), 0),
        (12, (1, 1), format!("{CH1}{CH2}{CH5}"), 8),
        (8, (0, 2), format!("{CH1}{CH2}{CH5}"), 8),
        (12, (2, 0), format!("{CH1}{CH4}{CH5}"), 4),
        (12, (2, 2), CH1.to_string(), 4),
        (12, (1000, 0), format!("{CH4}{CH5}"), 0),
        (12, (0, 1000), format!("{CH1}{CH2}{CH3}"), 12),
        (12, (1000, 1000), String::new(), 0),
    ];
    for (cursor, (before, after), text, cursor_after) in cases {
        let mut eb = buffer(&five_emoji(), TextRange::at(cursor));
        DeleteSurroundingTextInCodePointsCommand::new(before, after).apply_to(&mut eb);
        assert_buffer(&eb, &text, Some(cursor_after), None);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_selection_preserve
#[test]
fn surrounding_code_points_are_deleted_around_the_selection() {
    let mut eb = buffer(&five_emoji(), TextRange::new(8, 16));
    DeleteSurroundingTextInCodePointsCommand::new(1, 1).apply_to(&mut eb);
    assert_eq!(eb.to_string(), format!("{CH1}{CH3}{CH4}"));
    assert_selection(&eb, 4, 12);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_composition_no_intersection_preceding_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_composition_no_intersection_trailing_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_composition_intersection_preceding_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_composition_intersection_trailing_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_covered_composition
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, test_delete_composition_covered
#[test]
fn deleting_surrounding_code_points_moves_the_composition() {
    let cases = [
        ((0, 4), Some((0, 4))),
        ((16, 20), Some((8, 12))),
        ((0, 12), Some((0, 8))),
        ((12, 20), Some((8, 12))),
        ((8, 12), None),
        ((0, 20), Some((0, 12))),
    ];
    for (composition, expected) in cases {
        let mut eb = buffer(&five_emoji(), TextRange::at(12));
        eb.set_composition(composition.0, composition.1);
        DeleteSurroundingTextInCodePointsCommand::new(1, 1).apply_to(&mut eb);
        assert_buffer(&eb, &format!("{CH1}{CH2}{CH5}"), Some(8), expected);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, deletes_whenLengthAfterCursorOverflows_withMaxValue
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, deletes_whenLengthBeforeCursorOverflows_withMaxValue
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, deletes_whenBothOverflow_withMaxValue_cursorAtStart
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, deletes_whenBothOverflow_withMaxValue_cursorAtEnd
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, deletes_whenLengthAfterCursorOverflows
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/DeleteSurroundingTextInCodePointsCommandTest.kt, deletes_whenLengthBeforeCursorOverflows
#[test]
fn surrounding_code_point_lengths_that_overflow_delete_to_the_ends() {
    for length in [usize::MAX, usize::MAX - 1] {
        let mut eb = buffer("abcde", TextRange::at(4));
        DeleteSurroundingTextInCodePointsCommand::new(0, length).apply_to(&mut eb);
        assert_buffer(&eb, "abcd", Some(4), None);
        let mut eb = buffer("abcde", TextRange::at(1));
        DeleteSurroundingTextInCodePointsCommand::new(length, 0).apply_to(&mut eb);
        assert_buffer(&eb, "bcde", Some(0), None);
    }
    for cursor in [0, 5] {
        let mut eb = buffer("abcde", TextRange::at(cursor));
        DeleteSurroundingTextInCodePointsCommand::new(usize::MAX, usize::MAX).apply_to(&mut eb);
        assert_buffer(&eb, "", Some(0), None);
    }
}

// --- BackspaceCommandTest (a device test in Compose: it needs the platform's break iterator)

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/BackspaceCommandTest.kt, test_delete
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/BackspaceCommandTest.kt, test_delete_from_offset0
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/BackspaceCommandTest.kt, test_delete_with_selection
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/BackspaceCommandTest.kt, test_delete_with_composition
#[test]
fn backspace_deletes_the_composition_the_selection_or_a_character() {
    let mut eb = buffer("ABCDE", TextRange::at(1));
    BackspaceCommand.apply_to(&mut eb);
    assert_buffer(&eb, "BCDE", Some(0), None);

    let mut eb = buffer("ABCDE", TextRange::ZERO);
    BackspaceCommand.apply_to(&mut eb);
    assert_buffer(&eb, "ABCDE", Some(0), None);

    let mut eb = buffer("ABCDE", TextRange::new(2, 3));
    BackspaceCommand.apply_to(&mut eb);
    assert_buffer(&eb, "ABDE", Some(2), None);

    let mut eb = buffer("ABCDE", TextRange::at(1));
    eb.set_composition(2, 3);
    BackspaceCommand.apply_to(&mut eb);
    assert_buffer(&eb, "ABDE", Some(1), None);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/BackspaceCommandTest.kt, test_delete_surrogate_pair
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/BackspaceCommandTest.kt, test_delete_with_selection_surrogate_pair
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/BackspaceCommandTest.kt, test_delete_with_composition_surrogate_pair
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/BackspaceCommandTest.kt, test_delete_with_composition_zwj_emoji
#[test]
fn backspace_deletes_whole_emoji() {
    let mut eb = buffer(&five_emoji(), TextRange::at(4));
    BackspaceCommand.apply_to(&mut eb);
    assert_buffer(&eb, &format!("{CH2}{CH3}{CH4}{CH5}"), Some(0), None);

    let mut eb = buffer(&five_emoji(), TextRange::new(8, 12));
    BackspaceCommand.apply_to(&mut eb);
    assert_buffer(&eb, &format!("{CH1}{CH2}{CH4}{CH5}"), Some(8), None);

    let mut eb = buffer(&five_emoji(), TextRange::at(4));
    eb.set_composition(8, 12);
    BackspaceCommand.apply_to(&mut eb);
    assert_buffer(&eb, &format!("{CH1}{CH2}{CH4}{CH5}"), Some(4), None);

    let families = format!("{FAMILY}{FAMILY}");
    let mut eb = buffer(&families, TextRange::at(FAMILY.len()));
    BackspaceCommand.apply_to(&mut eb);
    assert_buffer(&eb, FAMILY, Some(0), None);
}

// --- MoveCursorCommandTest (a device test in Compose)

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_left
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_left_multiple
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_left_from_offset0
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_right
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_right_multiple
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_right_from_offset_length
#[test]
fn the_cursor_moves_by_characters() {
    let cases = [(3, -1, 2), (3, -2, 1), (0, -1, 0), (3, 1, 4), (3, 2, 5), (5, 1, 5)];
    for (cursor, amount, expected) in cases {
        let mut eb = buffer("ABCDE", TextRange::at(cursor));
        MoveCursorCommand::new(amount).apply_to(&mut eb);
        assert_buffer(&eb, "ABCDE", Some(expected), None);
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_left_surrogate_pair
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_left_multiple_surrogate_pair
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_right_surrogate_pair
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_right_multiple_surrogate_pair
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_left_emoji
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/MoveCursorCommandTest.kt, test_right_emoji
#[test]
fn the_cursor_moves_by_whole_emoji() {
    let cases = [(12, -1, 8), (12, -2, 4), (12, 1, 16), (12, 2, 20)];
    for (cursor, amount, expected) in cases {
        let mut eb = buffer(&five_emoji(), TextRange::at(cursor));
        MoveCursorCommand::new(amount).apply_to(&mut eb);
        assert_buffer(&eb, &five_emoji(), Some(expected), None);
    }
    let families = format!("{FAMILY}{FAMILY}");
    for (amount, expected) in [(-1, 0), (1, 2 * FAMILY.len())] {
        let mut eb = buffer(&families, TextRange::at(FAMILY.len()));
        MoveCursorCommand::new(amount).apply_to(&mut eb);
        assert_buffer(&eb, &families, Some(expected), None);
    }
}

#[test]
fn moving_the_cursor_collapses_the_selection_at_its_start() {
    let mut eb = buffer("ABCDE", TextRange::new(1, 4));
    MoveCursorCommand::new(1).apply_to(&mut eb);
    assert_buffer(&eb, "ABCDE", Some(2), None);
}

#[test]
fn delete_all_empties_the_text() {
    let mut eb = buffer("ABCDE", TextRange::new(1, 4));
    eb.set_composition(0, 2);
    DeleteAllCommand.apply_to(&mut eb);
    assert_buffer(&eb, "", Some(0), None);
}

// --- EditProcessorTest

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, test_new_state_and_edit_commands
#[test]
fn a_reset_value_takes_edit_commands() {
    let mut processor = EditProcessor::new();
    let value = TextFieldValue::new("ABCDE", TextRange::ZERO, None);
    processor.reset(value.clone());
    assert_eq!(processor.buffer_state, value);
    let value = processor.apply(&[&CommitTextCommand::new("X", 1)]);
    assert_eq!(value.text(), "XABCDE");
    assert_eq!(value.selection(), TextRange::at(1));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, testNewState_bufferNotUpdated_ifSameModelStructurally
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, testNewState_new_buffer_created_if_text_is_different
#[test]
fn a_reset_makes_a_new_buffer_only_for_a_new_text() {
    let mut processor = EditProcessor::new();
    let value = TextFieldValue::new("qwerty", TextRange::ZERO, Some(TextRange::ZERO));
    processor.reset(value.clone());
    assert_eq!(processor.buffers_made, 2);
    processor.reset(value.clone());
    assert_eq!(processor.buffers_made, 2);
    processor.reset(value.with_text("abc"));
    assert_eq!(processor.buffers_made, 3);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, testNewState_compositionNotLost_ifTextIsSame_butAnnotationsAreDifferent
#[test]
fn a_reset_with_new_styles_keeps_the_buffer_and_composition() {
    let mut processor = EditProcessor::new();
    let value = TextFieldValue::new(
        AnnotatedString::build(|builder| {
            builder.append("abc");
        }),
        TextRange::ZERO,
        None,
    );
    processor.reset(value.clone());
    processor.apply(&[&SetComposingRegionCommand::new(0, 3)]);
    let buffers = processor.buffers_made;
    let styled = AnnotatedString::build(|builder| {
        builder.with_style(crate::SpanStyle::DEFAULT, |builder| {
            builder.append("abc");
        });
    });
    processor.reset(TextFieldValue::new(styled, value.selection(), Some(TextRange::new(0, 3))));
    assert_eq!(processor.buffers_made, buffers);
    assert_eq!(processor.buffer_state.composition(), Some(TextRange::new(0, 3)));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, testNewState_buffer_not_recreated_if_selection_is_different
#[test]
fn a_reset_with_a_new_selection_moves_it_in_the_buffer() {
    let mut processor = EditProcessor::new();
    let value = TextFieldValue::new("qwerty", TextRange::ZERO, Some(TextRange::ZERO));
    processor.reset(value.clone());
    let buffers = processor.buffers_made;
    processor.reset(value.with_selection(TextRange::at(1)));
    assert_eq!(processor.buffers_made, buffers);
    assert_eq!(processor.buffer.selection(), TextRange::at(1));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, testNewState_buffer_not_recreated_if_composition_is_different
#[test]
fn an_application_cant_set_a_composition() {
    let mut processor = EditProcessor::new();
    let value = TextFieldValue::new("qwerty", TextRange::ZERO, Some(TextRange::at(1)));
    processor.reset(value.clone());
    let buffers = processor.buffers_made;
    assert_eq!(processor.buffer.composition(), None);
    processor.reset(value.with_composition(None));
    assert_eq!(processor.buffers_made, buffers);
    assert_eq!(processor.buffer.composition(), None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, testNewState_reversedSelection_setsTheSelection
#[test]
fn a_reversed_selection_is_set_in_order() {
    let mut processor = EditProcessor::new();
    let value = TextFieldValue::new("qwerty", TextRange::new(2, 1), Some(TextRange::at(1)));
    processor.reset(value.clone());
    assert_eq!(processor.buffer.selection(), TextRange::new(1, 2));
    let buffers = processor.buffers_made;
    processor.reset(value.with_selection(TextRange::new(3, 0)));
    assert_eq!(processor.buffers_made, buffers);
    assert_eq!(processor.buffer.selection(), TextRange::new(0, 3));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, compositionIsCleared_when_textChanged
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, compositionIsNotCleared_when_textIsSame
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, compositionIsCleared_when_compositionReset
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, compositionIsCleared_when_compositionChanged
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, compositionIsNotCleared_when_onlySelectionChanged
#[test]
fn a_reset_keeps_the_composition_only_while_text_and_composition_stay() {
    let composed = |processor: &mut EditProcessor, start: isize, end: isize| {
        processor.apply(&[
            &CommitTextCommand::new("ab", 0),
            &SetComposingRegionCommand::new(start, end),
        ]);
    };

    let mut processor = EditProcessor::new();
    composed(&mut processor, 0, 2);
    processor.reset(processor.buffer_state.with_text("cd"));
    assert_eq!(processor.buffer_state.text(), "cd");
    assert_eq!(processor.buffer_state.composition(), None);

    let mut processor = EditProcessor::new();
    composed(&mut processor, 0, 2);
    processor.reset(processor.buffer_state.clone());
    assert_eq!(processor.buffer_state.composition(), Some(TextRange::new(0, 2)));

    let mut processor = EditProcessor::new();
    composed(&mut processor, -1, -1);
    processor.reset(processor.buffer_state.with_composition(Some(TextRange::new(0, 2))));
    assert_eq!(processor.buffer_state.composition(), None);

    let mut processor = EditProcessor::new();
    composed(&mut processor, 0, 2);
    processor.reset(processor.buffer_state.with_composition(Some(TextRange::new(0, 1))));
    assert_eq!(processor.buffer_state.composition(), None);

    let mut processor = EditProcessor::new();
    processor.apply(&[
        &CommitTextCommand::new("ab", 0),
        &SetComposingRegionCommand::new(0, 2),
        &SetSelectionCommand::new(0, 2),
    ]);
    processor.reset(processor.buffer_state.with_selection(TextRange::at(1)));
    assert_eq!(processor.buffer_state.text(), "ab");
    assert_eq!(processor.buffer_state.composition(), Some(TextRange::new(0, 2)));
    assert_eq!(processor.buffer_state.selection(), TextRange::at(1));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/EditProcessorTest.kt, throwsDescriptiveMessage_whenCommandFailsInBatch
#[test]
fn a_failing_command_panics_with_the_batch() {
    struct InvalidCommand;
    impl EditCommand for InvalidCommand {
        fn apply_to(&self, _: &mut EditingBuffer) {
            panic!("Better luck next time");
        }
    }
    let mut processor = EditProcessor::new();
    processor.buffer.replace(0, 0, "hello world");
    processor.buffer.set_selection(0, 5);
    processor.buffer.set_composition(5, 7);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        processor.apply(&[
            &CommitTextCommand::new("ab", 0),
            &InvalidCommand,
            &SetSelectionCommand::new(0, 2),
        ]);
    }))
    .expect_err("the batch should fail");
    let message = failure.downcast_ref::<String>().expect("a formatted message");
    assert_eq!(
        message,
        "Error while applying EditCommand batch to buffer (length=11, composition=null, \
         selection=TextRange(5, 5)):\n   CommitTextCommand(text.length=2, newCursorPosition=0)\n \
         > Unknown EditCommand: InvalidCommand\n   SetSelectionCommand(start=0, end=2)\nCaused by: \
         Better luck next time"
    );
}

#[test]
fn a_reversed_selection_stays_reversed_after_commands() {
    let mut processor = EditProcessor::new();
    processor.reset(TextFieldValue::new("abcde", TextRange::new(4, 1), None));
    let value = processor.apply(&[&SetSelectionCommand::new(1, 3)]);
    assert_eq!(value.selection(), TextRange::new(3, 1));
}
