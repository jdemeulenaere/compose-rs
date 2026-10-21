//! Third adversarial review of Material's menus, surfaces and sliders, against Compose's
//! `DropdownMenu`, `Surface` and `Slider`.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
}

/// Compose's `DropdownMenu` puts its `modifier` on the menu's column, before
/// `width(IntrinsicSize.Max)`: `Modifier.width(200.dp)` makes the column 200 dp wide, and its
/// items (`fillMaxWidth()`, `sizeIn(112.dp, 280.dp)` only for the intrinsic measurement) fill
/// it. Here the menu is 200 dp wide, but its items keep their own width (the 112 dp minimum),
/// leaving the menu's right part dead to clicks.
#[test]
fn a_menu_given_a_width_has_items_as_wide_as_it() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        MaterialTheme!({
            Box!(modifier = Modifier.size(40.dp()), {
                DropdownMenu!(true, || {}, modifier = Modifier.width(200.dp()).test_tag("menu"), {
                    DropdownMenuItem!(
                        { Text!("One") },
                        || {},
                        modifier = Modifier.test_tag("item")
                    );
                });
            });
        });
    });
    assert_eq!(bounds(&mut ui, "menu").width(), 200.0);
    let item = bounds(&mut ui, "item");
    assert_eq!(item.width(), 200.0, "the item is narrower than its menu: {item:?}");
}
