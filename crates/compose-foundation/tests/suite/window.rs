//! `Popup` and `Dialog`: content over the rest of the window, hit first, dismissed by clicks
//! outside and Escape.

#![allow(non_snake_case)]

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Dialog, IntOffset, IntSize, Key, Offset, PointerEvent, Popup, PopupProperties, UnitExt,
};

thread_local! {
    static CLICKS: Cell<u32> = const { Cell::new(0) };
}

fn tap(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

#[composable]
fn Screen(open: MutableState<bool>) {
    Column!({
        Box!(modifier = Modifier.size((100.dp(), 40.dp())).test_tag("anchor"), {
            if open.get() {
                Popup!(
                    alignment = Alignment::TOP_START,
                    offset = IntOffset::new(0, 44),
                    on_dismiss_request = move || open.set(false),
                    {
                        Box!(modifier = Modifier.size((80.dp(), 60.dp())).test_tag("popup"), {
                            BasicText!("menu");
                        });
                    }
                );
            }
        });
        Box!(
            modifier = Modifier
                .size((200.dp(), 100.dp()))
                .clickable(|| CLICKS.with(|clicks| clicks.set(clicks.get() + 1)))
                .test_tag("below"),
        );
    });
}

#[test]
fn a_popup_shows_over_its_anchor_takes_no_room_and_dismisses_on_a_click_outside() {
    CLICKS.with(|clicks| clicks.set(0));
    let open = mutable_state_of(true);
    let handle = open.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || Screen(handle));
    // At the anchor's top start, 44 px down; the column doesn't make room for it.
    let popup = ui.on_node_with_tag("popup").fetch_semantics_node().bounds;
    assert_eq!((popup.left, popup.top), (0.0, 44.0));
    assert_eq!(ui.on_node_with_tag("below").fetch_semantics_node().bounds.top, 40.0);
    // Drawn last: the popup's text is the last paragraph.
    let list = ui.host().display_list();
    assert!(matches!(
        list.ops.iter().rev().find(|op| matches!(
            op,
            compose_ui::platform::DrawOp::DrawParagraph(..)
                | compose_ui::platform::DrawOp::DrawRect(..)
        )),
        Some(compose_ui::platform::DrawOp::DrawParagraph(..))
    ));
    // A tap on the popup, over the clickable beneath it: the popup takes it.
    tap(&mut ui, 20.0, 60.0);
    assert_eq!(CLICKS.with(Cell::get), 0);
    ui.on_node_with_tag("popup").assert_exists();
    // A tap outside dismisses it (a non-focusable popup lets it through: nothing is there).
    tap(&mut ui, 250.0, 250.0);
    ui.on_node_with_tag("popup").assert_does_not_exist();
    assert!(!open.get());
    assert_eq!(CLICKS.with(Cell::get), 0);
    // With the popup gone, the clickable gets taps again.
    tap(&mut ui, 20.0, 60.0);
    assert_eq!(CLICKS.with(Cell::get), 1);
}

#[test]
fn a_dialog_is_modal_takes_the_focus_and_dismisses_on_escape() {
    #[composable]
    fn WithDialog(open: MutableState<bool>) {
        Box!(
            modifier = Modifier
                .fill_max_size()
                .clickable(|| CLICKS.with(|clicks| clicks.set(clicks.get() + 1))),
            {
                if open.get() {
                    Dialog!(on_dismiss_request = move || open.set(false), {
                        Box!(modifier = Modifier.size((100.dp(), 50.dp())).test_tag("dialog"), {
                            BasicText!("Sure?");
                        });
                    });
                }
            }
        );
    }
    CLICKS.with(|clicks| clicks.set(0));
    let open = mutable_state_of(true);
    let handle = open.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || WithDialog(handle));
    let dialog = ui.on_node_with_tag("dialog").fetch_semantics_node().bounds;
    assert_eq!((dialog.left, dialog.top), (100.0, 125.0), "centered");
    // A tap inside: nothing beneath clicks, and the dialog stays.
    tap(&mut ui, 150.0, 150.0);
    assert_eq!(CLICKS.with(Cell::get), 0);
    ui.on_node_with_tag("dialog").assert_exists();
    // Escape dismisses it (it took the focus).
    ui.press_key(Key::Escape);
    ui.on_node_with_tag("dialog").assert_does_not_exist();
    assert_eq!(CLICKS.with(Cell::get), 0);
}

#[test]
fn a_focusable_popup_takes_the_focus() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!({
            Popup!(
                properties = PopupProperties { focusable: true, ..PopupProperties::default() },
                {
                    BasicText!("menu", modifier = Modifier.test_tag("menu"));
                }
            );
        });
    });
    let focused = ui.host().with_tree(|tree| tree.focused());
    assert!(focused.is_some(), "the popup took the focus");
    ui.on_node_with_tag("menu").assert_exists();
}
