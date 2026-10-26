//! `TextRange` and `TextFieldValue`, adapted from Compose's tests. Offsets are bytes (D1100).

use compose_ui::{AnnotatedString, SpanStyle, TextFieldValue, TextRange};

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, substring
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, substring_not_start_larger_than_end
#[test]
fn a_range_takes_its_part_of_a_text_in_either_order() {
    assert_eq!(TextRange::new(0, 3).substring("abcdef"), "abc");
    assert_eq!(TextRange::new(3, 0).substring("abcdef"), "abc");
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, min_max
#[test]
fn min_and_max_order_the_ends() {
    let range = TextRange::new(9, 8);
    assert_eq!((range.min(), range.max()), (8, 9));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, equality
#[test]
fn ranges_compare_by_their_ends() {
    assert_eq!(TextRange::new(0, 0), TextRange::new(0, 0));
    assert_eq!(TextRange::new(0, 1), TextRange::new(0, 1));
    assert_ne!(TextRange::new(0, 1), TextRange::new(0, 0));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, test_range_collapsed
#[test]
fn a_range_with_equal_ends_is_collapsed() {
    assert!(TextRange::new(0, 0).collapsed());
    assert!(TextRange::new(1, 1).collapsed());
    assert!(!TextRange::new(0, 1).collapsed());
    assert!(!TextRange::new(1, 2).collapsed());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, test_range_reversed
#[test]
fn a_range_ending_before_its_start_is_reversed() {
    assert!(!TextRange::new(0, 0).reversed());
    assert!(!TextRange::new(0, 1).reversed());
    assert!(TextRange::new(1, 0).reversed());
    assert!(TextRange::new(3, 2).reversed());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, test_intersects
#[test]
fn ranges_intersect_when_they_share_an_offset() {
    let range = TextRange::new;
    assert!(range(0, 1).intersects(range(0, 1)));
    assert!(range(0, 2).intersects(range(1, 2)));
    assert!(range(0, 2).intersects(range(0, 1)));
    assert!(range(0, 1).intersects(range(0, 2)));
    assert!(range(0, 3).intersects(range(1, 4)));
    assert!(range(1, 4).intersects(range(0, 3)));
    assert!(!range(0, 1).intersects(range(1, 2)));
    assert!(!range(0, 1).intersects(range(2, 3)));
    assert!(!range(1, 2).intersects(range(0, 1)));
    assert!(!range(2, 3).intersects(range(0, 1)));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, test_contains_range
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, test_contains_range_operator
#[test]
fn a_range_contains_the_ranges_within_it() {
    let range = TextRange::new;
    assert!(range(0, 2).contains_range(range(0, 1)));
    assert!(range(0, 2).contains_range(range(0, 2)));
    assert!(!range(0, 2).contains_range(range(0, 3)));
    assert!(!range(0, 2).contains_range(range(1, 3)));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, test_contains_offset
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, test_contains_offset_operator
#[test]
fn a_range_contains_offsets_before_its_end() {
    let range = TextRange::new(0, 2);
    assert!(range.contains(0));
    assert!(range.contains(1));
    assert!(!range.contains(2));
    assert!(!range.contains(3));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, constrain_updates_start_end_if_required
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, constrain_with_collapsed_min_max_returns_collapsed_values
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, constrain_min_max_greater_than_TextRange_values
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, constrain_min_smaller_than_TextRange_values
// (test_does_not_accept_negative_value_for_start and _end have no counterpart: offsets are unsigned)
#[test]
fn coercing_keeps_both_ends_in_bounds() {
    let range = TextRange::new;
    assert_eq!(range(0, 4).coerce_in(1, 3), range(1, 3));
    assert_eq!(range(1, 2).coerce_in(2, 2), range(2, 2));
    assert_eq!(range(2, 3).coerce_in(2, 2), range(2, 2));
    assert_eq!(range(0, 4).coerce_in(5, 6), range(5, 5));
    assert_eq!(range(5, 6).coerce_in(0, 4), range(4, 4));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextRangeTest.kt, to_string
#[test]
fn a_range_prints_as_composes() {
    assert_eq!(TextRange::new(0, 1).to_string(), "TextRange(0, 1)");
    assert_eq!(format!("{:?}", TextRange::at(1)), "TextRange(1, 1)");
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, aligns_selection_to_the_text_length
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, aligns_composition_to_text_length
// (throws_exception_for_negative_selection and _composition have no counterpart: offsets are unsigned)
#[test]
fn a_value_keeps_its_ranges_in_the_text() {
    let value = TextFieldValue::new("a", TextRange::at(2), Some(TextRange::at(2)));
    assert!(value.selection().collapsed());
    assert_eq!(value.selection().max(), 1);
    assert_eq!(value.composition(), Some(TextRange::at(1)));
}

#[test]
fn a_value_moves_ranges_inside_a_character_to_its_start() {
    // "é" is two bytes.
    let value = TextFieldValue::new("a\u{E9}b", TextRange::new(2, 4), Some(TextRange::new(0, 2)));
    assert_eq!(value.selection(), TextRange::new(1, 4));
    assert_eq!(value.composition(), Some(TextRange::new(0, 1)));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, keep_selection_that_is_less_than_text_length
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, keep_composition_that_is_less_than_text_length
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, text_and_selection_parameter_constructor_has_null_composition
#[test]
fn a_value_keeps_ranges_within_the_text() {
    let value = TextFieldValue::new("a bc", TextRange::new(0, 1), None);
    assert_eq!(value.text(), "a bc");
    assert_eq!(value.selection(), TextRange::new(0, 1));
    assert_eq!(value.composition(), None);
    let value = TextFieldValue::new("a bc", TextRange::ZERO, Some(TextRange::new(0, 1)));
    assert_eq!(value.composition(), Some(TextRange::new(0, 1)));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, equals_returns_true_for_same_instance
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, equals_returns_true_for_same_object
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, copy_sets_text_correctly
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextFieldValueTest.kt, copy_sets_selection_correctly
#[test]
fn copies_replace_one_part() {
    let value = TextFieldValue::new("a", TextRange::at(1), Some(TextRange::at(2)));
    assert_eq!(value, value.clone());
    assert_eq!(
        value.with_text("b"),
        TextFieldValue::new("b", value.selection(), value.composition())
    );
    assert_eq!(
        value.with_selection(TextRange::ZERO),
        TextFieldValue::new("a", TextRange::ZERO, value.composition())
    );
}

#[test]
fn the_selected_text_and_its_surroundings_keep_their_styles() {
    let bold = SpanStyle { font_weight: Some(compose_ui::FontWeight::BOLD), ..SpanStyle::DEFAULT };
    let text = AnnotatedString::build(|builder| {
        builder.append("ab");
        builder.with_style(bold, |builder| {
            builder.append("cd");
        });
        builder.append("ef");
    });
    let value = TextFieldValue::new(text, TextRange::new(3, 5), None);
    let selected = value.selected_text();
    assert_eq!(selected.as_str(), "de");
    assert_eq!((selected.span_styles[0].start, selected.span_styles[0].end), (0, 1));
    assert_eq!(value.text_before_selection(2).as_str(), "bc");
    assert_eq!(value.text_before_selection(10).as_str(), "abc");
    assert_eq!(value.text_after_selection(1).as_str(), "f");
    assert_eq!(value.text_after_selection(10).as_str(), "f");
    let emoji = TextFieldValue::new("\u{1F600}x\u{1F601}", TextRange::new(4, 5), None);
    // Whole characters only: three bytes of a four-byte emoji take none of it.
    assert_eq!(emoji.text_before_selection(3).as_str(), "");
    assert_eq!(emoji.text_after_selection(3).as_str(), "");
    assert_eq!(emoji.text_after_selection(4).as_str(), "\u{1F601}");
}
