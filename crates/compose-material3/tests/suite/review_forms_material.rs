//! Review of D235: prefix and suffix.

#![allow(non_snake_case)]

use compose_material3::*;
use compose_test::*;
use compose_ui::Modifier;

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
}

#[test]
fn in_rtl_the_prefix_is_at_the_right_and_the_suffix_at_the_left() {
    let mut ui = compose_test(|| {
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl),
            || {
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
            },
        );
    });
    let field = bounds(&mut ui, "field");
    let prefix = bounds(&mut ui, "prefix");
    let suffix = bounds(&mut ui, "suffix");
    assert_eq!(field.right - prefix.right, 16.0, "{field:?} {prefix:?}");
    assert_eq!(suffix.left - field.left, 16.0, "{field:?} {suffix:?}");
}

#[test]
fn the_text_starts_after_the_prefix_and_its_padding() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            OutlinedTextField!(
                "12".to_string(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                label = Some(|| Text!("Price")),
                prefix = Some(|| Text!("$", modifier = Modifier.test_tag("prefix"))),
                suffix = Some(|| Text!("kg", modifier = Modifier.test_tag("suffix"))),
            );
        });
    });
    let prefix = bounds(&mut ui, "prefix");
    let suffix = bounds(&mut ui, "suffix");
    let field = bounds(&mut ui, "field");
    eprintln!("DBG field {field:?} prefix {prefix:?} suffix {suffix:?}");
    // Multi-line: the prefix on the text's first line, not at the field's top.
    assert!(prefix.top > field.top);
}
