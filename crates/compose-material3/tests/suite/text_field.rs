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
    assert_eq!(
        node.config.get(&compose_ui::SemanticsProperties::ERROR).cloned().as_deref(),
        Some("Invalid input")
    );
    // The outline and the supporting text in the error color.
    let error = light_color_scheme().error;
    let list = ui.dump_display_list();
    assert!(list.contains(&format!("{:?}", error)), "{list}");
}

#[test]
fn a_prefix_and_a_suffix_go_around_the_text() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(
                "12".to_string(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                prefix = Some(|| Text!("$", modifier = Modifier.test_tag("prefix"))),
                suffix = Some(|| Text!("kg", modifier = Modifier.test_tag("suffix"))),
                single_line = true,
            );
        });
    });
    let field = bounds(&mut ui, "field");
    let prefix = bounds(&mut ui, "prefix");
    let suffix = bounds(&mut ui, "suffix");
    // 16 dp in at the start and the end, on the text's line.
    assert_eq!(prefix.left - field.left, 16.0);
    assert_eq!(field.right - suffix.right, 16.0);
    assert_eq!(prefix.top, suffix.top);
    assert_eq!((prefix.top + prefix.bottom) / 2.0, (field.top + field.bottom) / 2.0);
}

#[test]
fn a_multi_line_field_is_between_min_lines_and_max_lines_high() {
    let value = compose_runtime::mutable_state_of(String::new());
    let handle = value.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            OutlinedTextField!(
                handle.get(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                max_lines = 4,
                min_lines = 3,
            );
        });
    });
    // 16 dp above and below the body-large lines (24 sp each).
    let height = |ui: &mut ComposeTestRule| {
        let field = bounds(ui, "field");
        field.bottom - field.top
    };
    assert_eq!(height(&mut ui), 32.0 + 3.0 * 24.0);
    value.set(String::from("1\n2\n3\n4\n5\n6"));
    ui.wait_for_idle();
    assert_eq!(height(&mut ui), 32.0 + 4.0 * 24.0);
}
