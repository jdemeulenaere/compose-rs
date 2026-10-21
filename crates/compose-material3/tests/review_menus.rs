//! Adversarial review of Material's `DropdownMenu`: its size and position, against Compose's
//! `DropdownMenuContent` and `DropdownMenuPositionProvider`.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, Offset, PointerEvent, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

/// A menu at an anchor of 40 x 40 dp, `start` and `top` dp into the window, with `items` items.
#[composable]
fn Anchored(start: i32, top: i32, items: usize) {
    MaterialTheme!({
        Box!(
            modifier =
                Modifier.padding_sides((start as f32).dp(), (top as f32).dp(), 0.dp(), 0.dp()),
            {
                Box!(modifier = Modifier.size(40.dp()).test_tag("anchor"), {
                    DropdownMenu!(true, || {}, modifier = Modifier.test_tag("menu"), {
                        for index in 0..items {
                            DropdownMenuItem!({ Text!("{}", index) }, || {});
                        }
                    });
                });
            }
        );
    });
}

/// Compose's menu is `width(IntrinsicSize.Max)`: as wide as its widest item, each at least
/// 112 dp; short items make a 112 dp menu, not a 280 dp one.
#[test]
fn a_menu_of_short_items_is_112_dp_wide() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || Anchored(0, 0, 2));
    let menu = bounds(&mut ui, "menu");
    assert_eq!(menu.width(), 112.0, "{menu:?}");
}

/// Compose anchors the menu to its parent layout's inner coordinates: the canonical
/// `Box(Modifier.padding(..)) { IconButton(..); DropdownMenu(..) }` opens the menu right under
/// the button, not under the padded box.
#[test]
fn a_menu_opens_right_under_its_anchor_inside_the_padding() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        MaterialTheme!({
            Box!(modifier = Modifier.padding(20.dp()), {
                Box!(modifier = Modifier.size(48.dp()).test_tag("button"));
                DropdownMenu!(true, || {}, modifier = Modifier.test_tag("menu"), {
                    DropdownMenuItem!({ Text!("Edit") }, || {});
                });
            });
        });
    });
    let menu = bounds(&mut ui, "menu");
    let button = bounds(&mut ui, "button");
    assert_eq!((menu.left, menu.top), (button.left, button.bottom), "{menu:?} {button:?}");
}

/// When the menu doesn't fit at the anchor's start, Compose aligns its end with the anchor's
/// end (`toLeft`), not with the window's end.
#[test]
fn a_menu_that_does_not_fit_after_its_anchor_ends_with_it() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || Anchored(340, 0, 2));
    let menu = bounds(&mut ui, "menu");
    let anchor = bounds(&mut ui, "anchor");
    assert_eq!(anchor.right, 380.0);
    assert_eq!(menu.right, anchor.right, "{menu:?} {anchor:?}");
}

/// Compose keeps a menu 48 dp from the window's top and bottom (`MenuVerticalMargin`): one that
/// would come within 48 dp of the bottom below its anchor opens above it.
#[test]
fn a_menu_keeps_48_dp_from_the_windows_bottom() {
    // The anchor at 170..210 in a 340 px window; two items: 2 x 48 + 16 = 112 px.
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 340), || Anchored(0, 170, 2));
    let menu = bounds(&mut ui, "menu");
    assert_eq!(menu.height(), 112.0);
    // Below: 210 + 112 = 322 fits the window, but not 340 - 48. Above: 170 - 112 = 58 >= 48.
    assert_eq!(menu.top, 58.0, "{menu:?}");
}

/// Compose's menu content is `verticalScroll`: a menu taller than the window scrolls to its
/// last item.
#[test]
fn a_menu_taller_than_the_window_scrolls() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || Anchored(0, 0, 20));
    let menu = bounds(&mut ui, "menu");
    let at = Offset::new(menu.left + 10.0, menu.top + 50.0);
    ui.host().send_pointer(PointerEvent::scroll(at, Offset::new(0.0, 5000.0)));
    ui.wait_for_idle();
    let last = ui.on_node_with_text("19").fetch_semantics_node().bounds;
    assert!(last.bottom <= menu.bottom, "the last item is out of the menu: {last:?} {menu:?}");
}
