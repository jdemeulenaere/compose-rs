//! Second phase F review: failing tests for right-to-left text in a text field, measured by
//! compose-skia's SkParagraph measurer (docs/06-ui.md, docs/07-platforms.md). Each test states
//! the input, what Jetpack Compose (Desktop, on Skia) does, and what happens instead.
//!
//! Checked and sound (no test kept): Home and End on hard lines, empty middle lines and soft
//! wraps; a tap right of a line ending in a newline; and 300 random keys, shortcuts, taps and
//! multibyte, right-to-left, emoji and combining insertions (with filtering and length-changing
//! `on_value_change`s, single- and multi-line, at densities 1 to 2, on both measurers) panic
//! nowhere.

#![allow(non_snake_case)]

use compose::foundation::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    Density, IntSize, Key, Rect,
    platform::{DisplayList, DrawOp},
};
use compose_skia::SkiaTextMeasurer;

#[composable]
fn Hebrew() {
    let text = remember(|| mutable_state_of(String::from("שלום")));
    BasicTextField!(
        value = text.get(),
        on_value_change = move |value| text.set(value),
        modifier = Modifier.fill_max_width().test_tag("field"),
    );
}

fn hebrew() -> ComposeTestRule {
    let mut ui = compose_test_with(
        IntSize::new(300, 100),
        Density::default(),
        Box::new(SkiaTextMeasurer::new()),
        Hebrew,
    );
    // Focused, the cursor at the start (offset 0), as a value-based field starts.
    ui.on_node_with_tag("field").request_focus();
    ui
}

/// The last rectangle drawn (the focused field's cursor), in the root.
fn cursor(list: &DisplayList) -> Rect {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut last = None;
    for op in &list.ops {
        match op {
            DrawOp::Translate(offset) => {
                x += offset.x;
                y += offset.y;
            }
            DrawOp::DrawRect(rect, _) => {
                last = Some(Rect::new(rect.left + x, rect.top + y, rect.right + x, rect.bottom + y))
            }
            _ => {}
        }
    }
    last.expect("a cursor")
}

/// A right-to-left text starts at the right edge of its field in Compose: the field's text is
/// laid out with the field's constraints (`TextDelegate.layout`: at least `minWidth`, which a
/// `fillMaxWidth` field makes its whole width), and start-aligned text in a right-to-left
/// paragraph sits at its right. So "שלום" in a 300 px field is drawn at the right, and the
/// cursor before its first letter is near x = 300. Here the field loosens its constraints
/// before laying the text out (`constraints.offset(…).loosen()`), so the paragraph is only as
/// wide as the word (about 29 px) and drawn at the left: the Hebrew sits at the left edge,
/// with the cursor at x ≈ 29.
#[test]
fn right_to_left_text_starts_at_the_right_of_its_field() {
    let mut ui = hebrew();
    let field = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    let cursor = cursor(&ui.host().display_list());
    assert!(
        cursor.left > field.right - 20.0,
        "the cursor at the start of a right-to-left text is at {cursor:?} in {field:?}"
    );
}

/// Left moves the cursor visually left in Compose's text field: `moveCursorLeft()` is
/// `if (isLtr()) moveCursorPrev() else moveCursorNext()`, the direction being the
/// paragraph's at the cursor. In "שלום" with the cursor at the start (its right edge), Left
/// moves after the first letter, so "X" typed then gives "שXלום". Here Left always moves to
/// the previous grapheme (nothing before the start), and the text becomes "Xשלום". (Right is
/// mirrored the same way: it moves the cursor visually left in right-to-left text.)
#[test]
fn left_moves_visually_left_in_right_to_left_text() {
    let mut ui = hebrew();
    ui.press_key(Key::DirectionLeft);
    ui.type_text("X");
    ui.on_node_with_tag("field").assert_text_equals(&["שXלום"]);
}
