//! Right-to-left layout review: failing tests for text fields in a right-to-left layout,
//! measured by compose-skia's SkParagraph measurer (docs/06-ui.md, "Layout direction"; D139).
//! Each test states the input, what Jetpack Compose (Desktop) does, and what happens instead.

#![allow(non_snake_case)]

use compose::foundation::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{Density, IntSize, LayoutDirection, UnitExt, platform::UiHost};
use compose_skia::SkiaTextMeasurer;

#[composable]
fn EmptyField() {
    let text = remember(|| mutable_state_of(String::new()));
    BasicTextField!(
        value = text.get(),
        on_value_change = move |value| text.set(value),
        modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
        single_line = true,
    );
}

/// The cursor's left edge, in the window, of the focused field.
fn cursor_left(ui: &mut ComposeTestRule) -> f32 {
    ui.on_node_with_tag("field").request_focus();
    ui.wait_for_idle();
    ui.host().text_input_area().expect("a focused field takes text").left
}

/// Input: an empty single-line 200 px `BasicTextField` in a 300 px window, laid out left to
/// right, then the window's direction set to right to left (`UiHost::set_layout_direction`,
/// as a locale change does). The field moves to the window's right (100..300).
///
/// Compose: the text is laid out again in the new direction (`TextDelegate` is keyed on the
/// layout direction): an empty paragraph takes the layout's direction, and the cursor sits at
/// the field's right edge (about 298), as when the field is first composed right to left.
///
/// Instead: `TextFieldNode::measure` keeps its paragraph while the constraints and density
/// are the same (crates/compose-foundation/src/text_field.rs, the `self.constraints !=
/// Some(constraints)` check doesn't look at the direction, unlike `TextNode`'s): the
/// left-to-right paragraph stays, and the cursor is at the field's left edge (100).
#[test]
fn an_empty_field_puts_its_cursor_at_the_right_once_the_direction_turns_right_to_left() {
    let mut ui = compose_test_with(
        IntSize::new(300, 100),
        Density::default(),
        Box::new(SkiaTextMeasurer::new()),
        EmptyField,
    );
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.wait_for_idle();
    assert_eq!(ui.on_node_with_tag("field").fetch_semantics_snapshot().bounds.left, 100.0);
    let left = cursor_left(&mut ui);
    assert!(left > 250.0, "the cursor at the field's right edge: {left}");
}

/// The same field composed right to left from the start: the cursor at its right edge.
#[test]
fn passes_an_empty_field_composed_right_to_left_has_its_cursor_at_the_right() {
    let mut host =
        UiHost::new(IntSize::new(300, 100), Density::default(), Box::new(SkiaTextMeasurer::new()));
    host.set_layout_direction(LayoutDirection::Rtl);
    let mut ui = ComposeTestRule::with_host(host, EmptyField);
    let left = cursor_left(&mut ui);
    assert!(left > 250.0, "the cursor at the field's right edge: {left}");
}
