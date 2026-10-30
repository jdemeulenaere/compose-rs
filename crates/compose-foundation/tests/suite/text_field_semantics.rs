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
