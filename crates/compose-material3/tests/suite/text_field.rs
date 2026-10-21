//! Material text fields' icons, supporting text, error state and keyboard parameters.

#![allow(non_snake_case)]

use compose_material3::*;
use compose_test::*;
use compose_ui::Modifier;

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
}

#[test]
fn icons_sit_in_48_dp_boxes_at_the_ends_and_the_text_between() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                placeholder = Some(|| Text!("hint", modifier = Modifier.test_tag("hint"))),
                leading_icon = Some(|| Icon!(
                    icons::filled::search(),
                    None,
                    modifier = Modifier.test_tag("leading")
                )),
                trailing_icon = Some(|| Icon!(
                    icons::filled::close(),
                    None,
                    modifier = Modifier.test_tag("trailing")
                )),
                single_line = true,
            );
        });
    });
    let field = bounds(&mut ui, "field");
    let leading = bounds(&mut ui, "leading");
    let trailing = bounds(&mut ui, "trailing");
    // 24 dp icons centered in 48 dp boxes at each end, vertically centered.
    assert_eq!((leading.left - field.left, leading.top - field.top), (12.0, 16.0));
    assert_eq!(field.right - trailing.right, 12.0);
    // The text (and its placeholder) 4 dp after the leading box.
    let hint = bounds(&mut ui, "hint");
    assert_eq!(hint.left - field.left, 52.0);
}

#[test]
fn supporting_text_and_errors_show_below_in_the_error_color() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            OutlinedTextField!(
                "bad".to_string(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                supporting_text =
                    Some(|| Text!("Required", modifier = Modifier.test_tag("support"))),
                is_error = true,
            );
        });
    });
    let field = bounds(&mut ui, "field");
    let support = bounds(&mut ui, "support");
    // Below the 56 dp container, 4 dp under it, 16 dp in.
    assert_eq!((support.left - field.left, support.top - field.top), (16.0, 60.0));
    let node = ui.on_node_with_tag("field").fetch_semantics_node();
    assert_eq!(node.config.error.as_deref(), Some("Invalid input"));
    // The outline and the supporting text in the error color.
    let error = light_color_scheme().error;
    let list = ui.dump_display_list();
    assert!(list.contains(&format!("{:?}", error)), "{list}");
}
