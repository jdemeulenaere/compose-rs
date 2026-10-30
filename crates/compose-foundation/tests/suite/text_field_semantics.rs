//! What a text field and a text offer an accessibility service besides their text (D1299): the
//! field's click, selection, clipboard and action key, as semantics actions, and their layouts.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    ImeAction, SemanticsActions as A, SemanticsProperties as P, TextLayoutResult, TextRange,
    UnitExt,
};

/// The field with `value`, its keyboard's action `Search` adding to `searched`.
#[composable]
fn Field(value: MutableState<String>, password: bool, searched: MutableState<i32>) {
    BasicTextField!(
        value.get(),
        move |new| value.set(new),
        modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
        keyboard_options =
            KeyboardOptions { ime_action: ImeAction::Search, ..KeyboardOptions::DEFAULT },
        keyboard_actions =
            KeyboardActions::new().on_search(move || searched.set(searched.get() + 1)),
        visual_transformation = if password {
            PasswordVisualTransformation::new().into()
        } else {
            VisualTransformation::NONE
        },
    );
}

fn field(text: &str, password: bool) -> (ComposeTestRule, MutableState<String>, MutableState<i32>) {
    let value = Box::leak(Box::new(mutable_state_of(text.to_string()))).handle();
    let searched = Box::leak(Box::new(mutable_state_of(0))).handle();
    let ui = compose_test(move || Field(value, password, searched));
    (ui, value, searched)
}

fn select(ui: &mut ComposeTestRule, start: i32, end: i32) -> bool {
    let mut done = false;
    ui.on_node_with_tag("field").perform_semantics_action_with(&A::SET_SELECTION, |set| {
        done = set(start, end, false);
    });
    done
}

fn selection(ui: &mut ComposeTestRule) -> TextRange {
    let node = ui.on_node_with_tag("field").fetch_semantics_snapshot();
    *node.config.get(&P::TEXT_SELECTION_RANGE).expect("a field's selection")
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldTest.kt, semantics_setTextSetSelectionActions, semantics_setTextSetSelection_original_invalidIndex
#[test]
fn set_selection_moves_the_selection_and_refuses_what_is_not_in_the_text() {
    let (mut ui, value, _) = field("hello", false);
    assert!(select(&mut ui, 1, 3));
    assert_eq!(selection(&mut ui), TextRange::new(1, 3));
    assert_eq!(value.get(), "hello", "the text stays");
    assert!(!select(&mut ui, 1, 3), "it's there already");
    assert!(!select(&mut ui, -1, 3));
    assert!(!select(&mut ui, 1, 6));
    assert_eq!(selection(&mut ui), TextRange::new(1, 3));
    // Reversed, it's a selection from its end.
    assert!(select(&mut ui, 4, 2));
    assert_eq!(selection(&mut ui), TextRange::new(4, 2));
}

#[test]
fn set_selection_is_in_what_is_shown_and_lands_on_the_characters_under_a_mask() {
    // Two bytes each in the text, and three in the mask.
    let (mut ui, _, _) = field("éé", true);
    assert!(select(&mut ui, 3, 6));
    assert_eq!(selection(&mut ui), TextRange::new(3, 6));
}

#[test]
fn copying_and_cutting_are_offered_with_a_selection_and_never_of_a_password() {
    let (mut ui, value, _) = field("hello", false);
    let offered = |ui: &mut ComposeTestRule| {
        let node = ui.on_node_with_tag("field").fetch_semantics_snapshot();
        (node.config.contains(&A::COPY_TEXT), node.config.contains(&A::CUT_TEXT))
    };
    assert_eq!(offered(&mut ui), (false, false));
    select(&mut ui, 0, 2);
    assert_eq!(offered(&mut ui), (true, true));

    ui.on_node_with_tag("field").perform_semantics_action(&A::COPY_TEXT);
    let clipboard = ui.host().with_tree(|tree| tree.clipboard());
    assert_eq!(clipboard.get_text().as_deref(), Some("he"));
    assert_eq!(value.get(), "hello");

    select(&mut ui, 2, 5);
    ui.on_node_with_tag("field").perform_semantics_action(&A::CUT_TEXT);
    assert_eq!(clipboard.get_text().as_deref(), Some("llo"));
    assert_eq!(value.get(), "he");
    assert_eq!(offered(&mut ui), (false, false), "the cursor is all that's left");

    ui.on_node_with_tag("field").perform_semantics_action(&A::PASTE_TEXT);
    assert_eq!(value.get(), "hello");

    let (mut ui, _, _) = field("secret", true);
    select(&mut ui, 0, 3);
    assert_eq!(offered(&mut ui), (false, false));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldTest.kt, semantics_clickAction
#[test]
fn a_click_focuses_the_field_and_asks_for_the_keyboard() {
    let (mut ui, _, _) = field("hello", false);
    ui.on_node_with_tag("field").assert_is_not_focused();
    ui.on_node_with_tag("field").perform_semantics_action(&A::ON_CLICK);
    ui.on_node_with_tag("field").assert_is_focused();
    assert_eq!(ui.host().take_software_keyboard_request(), Some(true));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldTest.kt, semantics_setTextSetSelectionActions
#[test]
fn set_text_puts_the_cursor_at_the_end() {
    let (mut ui, value, _) = field("", false);
    ui.on_node_with_tag("field").perform_semantics_action_with(&A::SET_TEXT, |set| {
        set("Hello".into());
    });
    assert_eq!(value.get(), "Hello");
    assert_eq!(selection(&mut ui), TextRange::new(5, 5));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldTest.kt, semantics_imeEnterAction
#[test]
fn the_keyboards_action_runs_from_semantics() {
    let (mut ui, _, searched) = field("hello", false);
    ui.on_node_with_tag("field").assert(has_ime_action(ImeAction::Search));
    ui.on_node_with_tag("field").perform_semantics_action(&A::ON_IME_ACTION);
    assert_eq!(searched.get(), 1);
}

#[test]
fn a_text_and_a_field_give_their_layouts() {
    let mut ui = compose_test(|| {
        Column!({
            BasicText!("one", modifier = Modifier.test_tag("text"));
            BasicTextField!(
                String::from("secret"),
                |_| {},
                modifier = Modifier.test_tag("field"),
                visual_transformation = PasswordVisualTransformation::new(),
            );
        });
    });
    let mut layout_of = |tag: &str| {
        let mut out: Vec<TextLayoutResult> = Vec::new();
        ui.on_node_unmerged(has_test_tag(tag)).perform_semantics_action_with(
            &A::GET_TEXT_LAYOUT_RESULT,
            |get| {
                get(&mut out);
            },
        );
        assert_eq!(out.len(), 1, "{tag}");
        out.remove(0)
    };
    let text = layout_of("text");
    assert_eq!(text.layout_input.text.text().as_str(), "one");
    assert_eq!(text.line_count, 1);
    assert_eq!(text.get_line_end(0, false), 3);
    let shown = layout_of("field");
    assert_eq!(shown.layout_input.text.text().as_str(), "\u{2022}".repeat(6));
}

/// A focused field with `text`, its cursor at `cursor`.
fn focused_at(text: &str, cursor: i32) -> (ComposeTestRule, MutableState<String>) {
    let (mut ui, value, _) = field(text, false);
    ui.on_node_with_tag("field").request_focus();
    ui.wait_for_idle();
    select(&mut ui, cursor, cursor);
    ui.wait_for_idle();
    (ui, value)
}

#[test]
fn a_selection_set_while_composing_lands_on_a_character_of_what_is_shown() {
    for offset in [3, 4] {
        let (mut ui, value) = focused_at("abéégh", 2);
        ui.host().send_preedit("你", None);
        ui.wait_for_idle();
        // Inside what's being composed.
        assert!(select(&mut ui, offset, offset));
        ui.wait_for_idle();
        assert_eq!(value.get(), "ab你éégh", "committed");
        assert_eq!(selection(&mut ui), TextRange::new(2, 2));
        // Going on from there doesn't cut a character.
        ui.host().send_key(compose_ui::KeyEvent::down(compose_ui::Key::Backspace));
        ui.wait_for_idle();
        assert_eq!(value.get(), "a你éégh");
    }
}

#[test]
fn a_selection_set_while_composing_can_reach_what_is_composed_and_no_further() {
    let (mut ui, value) = focused_at("ab", 2);
    ui.host().send_preedit("cde", None);
    ui.wait_for_idle();
    assert!(!select(&mut ui, 6, 6), "past what's shown");
    assert!(!select(&mut ui, 5, 5), "the cursor is there");
    assert!(select(&mut ui, 4, 4));
    ui.wait_for_idle();
    assert_eq!(value.get(), "abcde");
    assert_eq!(selection(&mut ui), TextRange::new(4, 4));
}

#[test]
fn the_selection_while_composing_is_the_cursor_in_what_is_shown() {
    let (mut ui, _) = focused_at("ab", 2);
    ui.host().send_preedit("hello", None);
    ui.wait_for_idle();
    assert_eq!(selection(&mut ui), TextRange::new(7, 7));
    ui.host().send_preedit("hello", Some((1, 1)));
    ui.wait_for_idle();
    assert_eq!(selection(&mut ui), TextRange::new(3, 3));
}

#[test]
fn a_refused_set_text_leaves_the_selection_and_nothing_for_the_next_key() {
    let value = Box::leak(Box::new(mutable_state_of(String::from("hello")))).handle();
    let accepts = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut ui = compose_test(move || {
        BasicTextField!(
            value.get(),
            move |new| {
                if accepts.get() {
                    value.set(new);
                }
            },
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
        );
    });
    ui.on_node_with_tag("field").request_focus();
    ui.wait_for_idle();
    accepts.set(true);
    ui.wait_for_idle();
    select(&mut ui, 1, 1);
    ui.wait_for_idle();
    accepts.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("field").perform_semantics_action_with(&A::SET_TEXT, |set| {
        set("refused".into());
    });
    ui.wait_for_idle();
    assert_eq!(value.get(), "hello");
    assert_eq!(selection(&mut ui), TextRange::new(1, 1), "where it was");

    accepts.set(true);
    ui.wait_for_idle();
    ui.host().send_text("X");
    ui.wait_for_idle();
    assert_eq!(value.get(), "hXello");
    assert_eq!(selection(&mut ui), TextRange::new(2, 2));
}

#[test]
fn set_text_with_the_text_there_still_moves_the_cursor_to_its_end() {
    let (mut ui, value) = focused_at("hello", 1);
    ui.on_node_with_tag("field").perform_semantics_action_with(&A::SET_TEXT, |set| {
        set("hello".into());
    });
    ui.wait_for_idle();
    assert_eq!(value.get(), "hello");
    assert_eq!(selection(&mut ui), TextRange::new(5, 5));
}

#[test]
fn set_text_replaces_what_is_being_composed_and_the_keyboard_starts_over() {
    let (mut ui, value) = focused_at("old", 3);
    ui.host().send_preedit("你", None);
    ui.wait_for_idle();
    ui.host().take_input_method_reset();
    ui.on_node_with_tag("field").perform_semantics_action_with(&A::SET_TEXT, |set| {
        set("new".into());
    });
    ui.wait_for_idle();
    assert_eq!(value.get(), "new");
    let node = ui.on_node_with_tag("field").fetch_semantics_snapshot();
    assert_eq!(node.config.get(&P::EDITABLE_TEXT).map(|it| it.as_str()), Some("new"));
    assert!(ui.host().take_input_method_reset());
}

#[test]
fn insert_text_at_cursor_commits_what_is_composed_and_goes_after_it() {
    let (mut ui, value) = focused_at("ab", 1);
    ui.host().send_preedit("x", None);
    ui.wait_for_idle();
    ui.on_node_with_tag("field").perform_semantics_action_with(&A::INSERT_TEXT_AT_CURSOR, |it| {
        it("yz".into());
    });
    ui.wait_for_idle();
    assert_eq!(value.get(), "axyzb");
    assert_eq!(selection(&mut ui), TextRange::new(4, 4));
}

#[test]
fn a_masks_offsets_are_those_of_the_characters_under_it() {
    let (mut ui, value, _) = field("abc", true);
    ui.on_node_with_tag("field").request_focus();
    ui.wait_for_idle();
    // The mask is three bytes a character.
    assert!(select(&mut ui, 3, 3));
    ui.wait_for_idle();
    assert_eq!(selection(&mut ui), TextRange::new(3, 3));
    ui.host().send_text("X");
    ui.wait_for_idle();
    assert_eq!(value.get(), "aXbc");
}

/// `12-34` for `1234`.
struct Grouped;

impl VisualTransform for Grouped {
    fn filter(&self, text: &str) -> TransformedText {
        struct Mapping;
        impl OffsetMapping for Mapping {
            fn original_to_transformed(&self, offset: usize) -> usize {
                if offset <= 2 { offset } else { offset + 1 }
            }
            fn transformed_to_original(&self, offset: usize) -> usize {
                if offset <= 2 {
                    offset
                } else if offset == 3 {
                    2
                } else {
                    offset - 1
                }
            }
        }
        TransformedText {
            text: format!("{}-{}", &text[..2.min(text.len())], &text[2.min(text.len())..]),
            offset_mapping: std::rc::Rc::new(Mapping),
        }
    }
}

#[test]
fn a_transformations_selection_goes_through_its_mapping_both_ways() {
    let mut ui = compose_test(|| {
        BasicTextField!(
            String::from("1234"),
            |_| {},
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
            visual_transformation = VisualTransformation::new(Grouped),
        );
    });
    assert!(select(&mut ui, 4, 4));
    assert_eq!(selection(&mut ui), TextRange::new(4, 4), "3 of the text");
    assert!(select(&mut ui, 3, 3));
    assert_eq!(selection(&mut ui), TextRange::new(2, 2), "the dash is at 2 of the text");
    assert!(!select(&mut ui, 2, 2), "there already");
}

#[test]
fn a_stale_copy_or_cut_of_a_field_become_a_password_does_nothing() {
    let value = Box::leak(Box::new(mutable_state_of(String::from("secret")))).handle();
    let masked = Box::leak(Box::new(mutable_state_of(false))).handle();
    let mut ui = compose_test(move || {
        BasicTextField!(
            value.get(),
            move |new| value.set(new),
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
            visual_transformation = if masked.get() {
                PasswordVisualTransformation::new().into()
            } else {
                VisualTransformation::NONE
            },
        );
    });
    select(&mut ui, 0, 3);
    ui.wait_for_idle();
    let node = ui.on_node_with_tag("field").fetch_semantics_snapshot().id;
    let copy = ui.host().semantics_action(node, &A::COPY_TEXT).expect("offered");
    let cut = ui.host().semantics_action(node, &A::CUT_TEXT).expect("offered");
    masked.set(true);
    ui.wait_for_idle();
    assert!(!copy());
    assert!(!cut());
    ui.wait_for_idle();
    let clipboard = ui.host().with_tree(|tree| tree.clipboard());
    assert_eq!(clipboard.get_text(), None);
    assert_eq!(value.get(), "secret");
}

#[test]
fn a_disabled_fields_selection_stays() {
    let mut ui = compose_test(|| {
        BasicTextField!(
            String::from("hello"),
            |_| {},
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
            enabled = false,
        );
    });
    assert!(!select(&mut ui, 1, 3));
    assert_eq!(selection(&mut ui), TextRange::new(0, 0));
}
