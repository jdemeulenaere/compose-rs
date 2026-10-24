//! Second adversarial review of Material's menus and dialogs, against Compose's
//! `DropdownMenu`, `DropdownMenuItem` and `AlertDialog`.

#![allow(non_snake_case)]

use std::cell::RefCell;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, Offset, PointerEvent, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
}

/// Compose's menu is `width(IntrinsicSize.Max)`: a child whose intrinsic width changes
/// remeasures the menu (intrinsics are a measure dependency). An item whose text grows from
/// "Short" to a 17-character label (121 px at 7.1 px a character, label-large's 0.1 sp
/// tracking included; 145 px with the item's 12 dp paddings) widens the menu past 112 dp; here
/// the item is measured again in place at the old width (its text wraps to two lines, still
/// within its 48 dp height), so the menu never asks its items' widths again.
#[test]
fn a_menu_widens_when_an_items_text_grows() {
    thread_local! { static LABEL: RefCell<Option<MutableState<&'static str>>> = const { RefCell::new(None) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        let label = remember(|| mutable_state_of("Short"));
        LABEL.with(|cell| *cell.borrow_mut() = Some(label));
        MaterialTheme!({
            Box!(modifier = Modifier.size(40.dp()), {
                DropdownMenu!(true, || {}, modifier = Modifier.test_tag("menu"), {
                    DropdownMenuItem!({ Text!("{}", label.get()) }, || {});
                    DropdownMenuItem!({ Text!("Two") }, || {});
                });
            });
        });
    });
    assert_eq!(bounds(&mut ui, "menu").width(), 112.0);
    LABEL.with(|cell| cell.borrow().unwrap().set("Much longer label"));
    ui.wait_for_idle();
    let menu = bounds(&mut ui, "menu");
    assert_eq!(menu.width(), 145.0, "the menu didn't widen for the longer item: {menu:?}");
}

/// Compose's `AlertDialog` gives its text `weight(1f, fill = false)`: a text taller than the
/// dialog's room takes only what the title and buttons leave, so the buttons stay in the
/// dialog. Here the text takes all of it and the buttons are pushed out.
#[test]
fn an_alert_dialogs_long_text_leaves_room_for_its_buttons() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 300), || {
        MaterialTheme!({
            AlertDialog!(
                on_dismiss_request = || {},
                modifier = Modifier.test_tag("dialog"),
                confirm_button = {
                    TextButton!(on_click = || {}, modifier = Modifier.test_tag("confirm"), {
                        Text!("OK");
                    });
                },
                title = {
                    Text!("Title");
                },
                text = {
                    Text!("{}", "word ".repeat(400));
                },
            );
        });
    });
    let dialog = bounds(&mut ui, "dialog");
    let confirm = bounds(&mut ui, "confirm");
    assert!(confirm.height() > 0.0, "the confirm button got no height: {confirm:?}");
    assert!(
        confirm.bottom <= dialog.bottom - 24.0,
        "the confirm button is out of the dialog: {confirm:?} in {dialog:?}"
    );
}

/// Compose's `DropdownMenu` takes `scrollState: ScrollState = rememberScrollState()`, remembered
/// at the menu's call site whether or not it's expanded: a menu closed and opened again shows
/// where it was scrolled. Here the scroll state is remembered inside `if expanded`, so it's
/// reset.
#[test]
fn a_reopened_menu_keeps_its_scroll_position() {
    thread_local! { static OPEN: RefCell<Option<MutableState<bool>>> = const { RefCell::new(None) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        let open = remember(|| mutable_state_of(true));
        OPEN.with(|cell| *cell.borrow_mut() = Some(open));
        MaterialTheme!({
            Box!(modifier = Modifier.size(40.dp()), {
                DropdownMenu!(
                    open.get(),
                    move || open.set(false),
                    modifier = Modifier.test_tag("menu"),
                    {
                        for index in 0..20 {
                            DropdownMenuItem!({ Text!("{}", index) }, || {});
                        }
                    }
                );
            });
        });
    });
    let menu = bounds(&mut ui, "menu");
    let at = Offset::new(menu.left + 10.0, menu.top + 50.0);
    ui.host().send_pointer(PointerEvent::scroll(at, Offset::new(0.0, 200.0)));
    ui.wait_for_idle();
    let scrolled = ui.on_node_with_text("0").fetch_semantics_node().bounds.top - menu.top;
    assert!(scrolled < 0.0, "the scroll didn't move the menu: {scrolled}");
    OPEN.with(|cell| cell.borrow().unwrap().set(false));
    ui.wait_for_idle();
    OPEN.with(|cell| cell.borrow().unwrap().set(true));
    ui.wait_for_idle();
    let menu = bounds(&mut ui, "menu");
    let reopened = ui.on_node_with_text("0").fetch_semantics_node().bounds.top - menu.top;
    assert_eq!(reopened, scrolled, "the reopened menu lost its scroll position");
}

/// Compose computes the menu's position as here, but the popup window is kept on the screen
/// (`PopupProperties.clippingEnabled`, the default: the window manager moves it inside the
/// display): a menu as tall as the room between the margins can't fit below, above, nor centered
/// on its anchor, and the fallback (`bottomToAnchorTop`, above the anchor) is clamped to the
/// window's top. Here 7 items (352 px) under an anchor at the window's top are placed at
/// y = -352, wholly outside the window.
#[test]
fn a_tall_menu_stays_in_the_window() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        MaterialTheme!({
            Box!(modifier = Modifier.size(40.dp()), {
                DropdownMenu!(true, || {}, modifier = Modifier.test_tag("menu"), {
                    for index in 0..7 {
                        DropdownMenuItem!({ Text!("{}", index) }, || {});
                    }
                });
            });
        });
    });
    let menu = bounds(&mut ui, "menu");
    assert_eq!(menu.height(), 352.0);
    assert!(menu.top >= 0.0 && menu.bottom <= 400.0, "the menu is out of the window: {menu:?}");
}

/// Locks in: an item with icons at both ends and a long text in a 280 dp menu: the text wraps
/// between the icons (12 dp from each), the trailing icon 12 dp from the menu's end, as
/// Compose's `DropdownMenuItemContent` row (`weight(1f)` text, 12 dp paddings).
#[test]
fn passes_a_long_item_with_icons_wraps_between_them_at_280_dp() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            Box!(modifier = Modifier.size(40.dp()), {
                DropdownMenu!(true, || {}, modifier = Modifier.test_tag("menu"), {
                    DropdownMenuItem!(
                        { Text!("{}", "label ".repeat(12)) },
                        || {},
                        modifier = Modifier.test_tag("item"),
                        leading_icon = {
                            Box!(modifier = Modifier.size(24.dp()).test_tag("leading"));
                        },
                        trailing_icon = {
                            Box!(modifier = Modifier.size(24.dp()).test_tag("trailing"));
                        },
                    );
                });
            });
        });
    });
    let menu = bounds(&mut ui, "menu");
    assert_eq!(menu.width(), 280.0);
    let item = bounds(&mut ui, "item");
    let leading = bounds(&mut ui, "leading");
    let trailing = bounds(&mut ui, "trailing");
    assert_eq!(leading.left, item.left + 12.0);
    assert_eq!(trailing.right, item.right - 12.0);
    let text = ui.on_node_unmerged(has_text(&"label ".repeat(12))).fetch_semantics_node().bounds;
    assert!(text.left >= leading.right + 12.0 && text.right <= trailing.left - 12.0, "{text:?}");
    assert!(text.height() > 20.0, "the text didn't wrap: {text:?}");
    assert!(item.height() >= text.height(), "{item:?} {text:?}");
}
