//! Twelfth review round: Material text fields' supporting text, against Compose's
//! `TextFieldMeasurePolicy`. Each test states the input, what Compose does, and what happens
//! instead.

#![allow(non_snake_case)]

use compose_material3::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
}

/// Input: a `TextField` with no width modifier (so its 280 dp minimum width) in a 600 px wide
/// window, with a supporting text of 80 characters (far wider than the field).
///
/// Compose: the supporting text is one of the decoration's slots, measured with the field's
/// width as its maximum (`supportingConstraints = looseConstraints.offset(...).copy(maxWidth =
/// width)`), so it wraps inside the field's 280 dp (16 dp in at each side) and the field stays
/// 280 dp wide.
///
/// Here: the decoration is a `Column` of the container and the supporting text's box, which is
/// measured against the window's width: the supporting text runs on one line past the
/// container's end and widens the whole field (and its focus and tap area) beyond 280 dp.
#[test]
fn supporting_text_wraps_within_the_field() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 400), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                supporting_text =
                    Some(|| Text!("x".repeat(80), modifier = Modifier.test_tag("support"))),
                single_line = true,
            );
        });
    });
    let field = bounds(&mut ui, "field");
    let support = bounds(&mut ui, "support");
    assert_eq!(
        (field.right - field.left, support.right <= field.right - 16.0),
        (280.0, true),
        "(the field's width, the supporting text inside it): field {field:?}, support {support:?}"
    );
}
