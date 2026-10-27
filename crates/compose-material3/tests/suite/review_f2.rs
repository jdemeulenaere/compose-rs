//! Second phase F review: failing tests for compose-material3's `TextField` (docs/06-ui.md).
//! Each test states the input, what Jetpack Compose's material3 does, and what happens
//! instead.
//!
//! Checked and sound (no test kept): typing past a placeholder (which leaves the composition
//! before the inner field) keeps the field's node, focus and cursor, and deleting back to empty
//! brings the placeholder back without losing focus.

#![allow(non_snake_case)]

use compose_material3::*;
use compose_test::*;
use compose_ui::{Modifier, UnitExt};

/// The bounds of the field's inner text (its focus target), in the root.
fn inner_text_bounds(ui: &mut ComposeTestRule) -> compose_ui::Rect {
    ui.on_node(Matcher::new("editable", |config| {
        config.contains(&compose_ui::SemanticsProperties::EDITABLE_TEXT)
    }))
    .request_focus();
    let bounds = ui.host().with_tree(|tree| tree.bounds_in_root(tree.focused().expect("focused")));
    ui.host().clear_focus();
    ui.wait_for_idle();
    bounds
}

/// A `TextField` with no width given is `TextFieldDefaults.MinWidth` (280 dp) wide in Compose:
/// `defaultMinSize(minWidth = 280.dp)` gives the decoration a minimum, and
/// `TextFieldMeasurePolicy` sizes the field from its input (which, holding a short text, takes
/// that minimum) rather than from the maximum, the well-known reason fields are given
/// `fillMaxWidth()`. Here the decoration's column is `fill_max_width()`, so an unsized field
/// takes all the width it is offered: 400 px in the 400 px root, and a field in a `Row` next
/// to a button pushes the button out.
#[test]
fn an_unsized_text_field_is_280_dp_wide() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(
                value = String::from("Ada"),
                on_value_change = |_| {},
                modifier = Modifier.test_tag("field"),
            );
        });
    });
    let field = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    assert_eq!(field.width(), 280.0, "{field:?}");
}

/// A taller field (a notes field, `Modifier.height(120.dp)`) keeps its text at the top in
/// Compose: `TextFieldMeasurePolicy` places a multi-line field's input at the top padding
/// (`placeWithoutLabel`: `if (singleLine) CenterVertically… else topPaddingValue`), and with a
/// label, the label at the top padding and the input under it. So the input starts at
/// y = 16. Here the decoration puts its column of label and input at the container's bottom
/// (`content_alignment = BOTTOM_START`), so in a 120 px field the first line starts at y = 80,
/// near the bottom, with the empty space above it. (A label goes to the bottom the same way,
/// and a single-line field isn't centered.)
#[test]
fn a_taller_text_field_keeps_its_text_at_the_top() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(
                value = String::new(),
                on_value_change = |_| {},
                modifier = Modifier.height(120.dp()).test_tag("field"),
            );
        });
    });
    let field = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    assert_eq!(field.height(), 120.0);
    let input = inner_text_bounds(&mut ui);
    assert_eq!(input.top, 16.0, "the input in a 120 px field is at {input:?}");
}
