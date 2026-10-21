//! Phase F review: failing tests for compose-material3 (docs/06-ui.md). Each test states the
//! input, what Jetpack Compose's material3 does, and what happens instead.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Modifier, Offset, PointerEvent};

/// The bounds of the field's inner text (its focus target): the field's semantics node is the
/// whole decoration, as Compose's.
fn inner_text_bounds(ui: &mut ComposeTestRule) -> compose_ui::Rect {
    ui.on_node(Matcher::new("editable", |config| config.editable)).request_focus();
    let bounds = ui.host().with_tree(|tree| tree.bounds_in_root(tree.focused().expect("focused")));
    ui.host().clear_focus();
    ui.wait_for_idle();
    bounds
}

fn click_at(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

#[composable]
fn Field() {
    let text = remember(|| mutable_state_of(String::from("hello")));
    MaterialTheme!({
        TextField!(
            value = text.get(),
            on_value_change = move |value| text.set(value),
            modifier = Modifier.test_tag("field"),
        );
    });
}

/// In Compose the decoration box is inside the text field: a tap anywhere in a `TextField`'s
/// 56 dp container is a tap on the text, and the cursor goes to the offset nearest to it (the
/// position is taken relative to the text layout and coerced into it). So a tap in the
/// container's bottom padding, right of the text, puts the cursor at the end, and "!" typed
/// then gives "hello!". Here the tap only focuses the field (`FocusOnTap`): the inner
/// `BasicTextField` isn't hit, the cursor stays where it was (at the start, D96), and the text
/// becomes "!hello". A tap below or beside the one-line text, about half of the field, never
/// moves the cursor.
#[test]
fn a_tap_in_the_padding_puts_the_cursor_at_the_nearest_offset() {
    let mut ui = compose_test(Field);
    let container = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    let input = inner_text_bounds(&mut ui);
    // In the bottom padding, past the end of the text.
    let (x, y) = (input.right - 8.0, (input.bottom + container.bottom) / 2.0);
    assert!(y > input.bottom && y < container.bottom, "{input:?} in {container:?}");
    click_at(&mut ui, x, y);
    ui.on_node_with_tag("field").assert_is_focused();
    ui.type_text("!");
    ui.on_node_with_tag("field").assert_text_equals(&["hello!"]);
}
