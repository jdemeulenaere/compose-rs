//! Second adversarial review of popups and dialogs (overlays): focus restore, key and pointer
//! routing while overlays show, against Jetpack Compose's behaviour (each popup and dialog its
//! own window, with its own focus owner).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Density, Dialog, IntSize, Key, KeyEvent, KeyEventType, LOCAL_DENSITY, Offset, PointerEvent,
    Popup, PopupProperties, UnitExt,
};

fn tap(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

fn focused(ui: &mut ComposeTestRule) -> Option<compose_ui::platform::NodeId> {
    ui.host().with_tree(|tree| tree.focused())
}

// ---- Key routing ---------------------------------------------------------------------------

/// A dialog is a window of its own in Compose: the keys its focused content doesn't consume
/// go to the dialog's window, never to the `onKeyEvent` handlers of the layouts around the
/// `Dialog` call (which are in the main window). Here Enter pressed in the dialog reaches the
/// screen's handler behind the scrim.
#[test]
fn keys_pressed_in_a_dialog_do_not_reach_the_handlers_around_it() {
    thread_local! { static KEYS: RefCell<Vec<Key>> = const { RefCell::new(Vec::new()) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Column!(
            modifier = Modifier.on_key_event(|event: KeyEvent| {
                if event.kind == KeyEventType::KeyDown {
                    KEYS.with(|keys| keys.borrow_mut().push(event.key));
                }
                false
            }),
            {
                Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("behind"));
                Dialog!(on_dismiss_request = || {}, {
                    Box!(modifier = Modifier.size(50.dp()).focusable().test_tag("in"));
                });
            }
        );
    });
    ui.on_node_with_tag("in").request_focus().assert_is_focused();
    ui.press_key(Key::Enter);
    ui.press_key(Key::Character('a'));
    assert_eq!(
        KEYS.with(|keys| keys.borrow().clone()),
        Vec::<Key>::new(),
        "keys pressed in the dialog reached a handler of the screen behind it"
    );
}

// ---- Pointer routing -----------------------------------------------------------------------

/// A focusable popup's window is touch modal in Compose (Android: no `FLAG_NOT_TOUCH_MODAL`;
/// desktop: a focusable layer takes every pointer event): presses outside it never reach what's
/// beneath, whether or not they dismiss it. Here, with `dismiss_on_click_outside` off, the
/// popup has no outside catcher and the tap reaches the clickable behind it.
#[test]
fn a_focusable_popup_not_dismissed_by_outside_clicks_still_swallows_them() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(
            modifier = Modifier.fill_max_size().clickable(|| CLICKS.with(|c| c.set(c.get() + 1))),
            {
                Popup!(
                    on_dismiss_request = || {},
                    properties = PopupProperties {
                        focusable: true,
                        dismiss_on_click_outside: false,
                        ..PopupProperties::default()
                    },
                    {
                        Box!(modifier = Modifier.size(50.dp()).test_tag("popup"));
                    }
                );
            }
        );
    });
    ui.on_node_with_tag("popup").assert_exists();
    tap(&mut ui, 200.0, 200.0);
    assert_eq!(CLICKS.with(Cell::get), 0, "a tap outside a focusable popup reached beneath it");
}

/// The same without an `on_dismiss_request`: a focusable popup still takes the focus and is
/// still touch modal.
#[test]
fn a_focusable_popup_without_a_dismiss_callback_swallows_outside_clicks() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(
            modifier = Modifier.fill_max_size().clickable(|| CLICKS.with(|c| c.set(c.get() + 1))),
            {
                Popup!(
                    properties = PopupProperties { focusable: true, ..PopupProperties::default() },
                    {
                        Box!(modifier = Modifier.size(50.dp()).test_tag("popup"));
                    }
                );
            }
        );
    });
    ui.on_node_with_tag("popup").assert_exists();
    tap(&mut ui, 200.0, 200.0);
    assert_eq!(CLICKS.with(Cell::get), 0, "a tap outside a focusable popup reached beneath it");
}

// ---- Focus ---------------------------------------------------------------------------------

/// A wizard: dialog A replaced by dialog B in one recomposition. In Compose each dialog is a
/// window of its own; the main window keeps its focused node all along, so once B is dismissed
/// too, the button that had the focus before A opened has it again.
#[test]
fn passes_a_dialog_replaced_by_another_in_one_frame_gives_the_focus_back_once_closed() {
    thread_local! { static STEP: RefCell<Option<MutableState<u32>>> = const { RefCell::new(None) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let step = remember(|| mutable_state_of(0u32));
        STEP.with(|cell| *cell.borrow_mut() = Some(step));
        Column!({
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("button"));
            if step.get() == 1 {
                Dialog!(on_dismiss_request = move || step.set(0), {
                    Box!(modifier = Modifier.size(50.dp()).focusable().test_tag("a"));
                });
            }
            if step.get() == 2 {
                Dialog!(on_dismiss_request = move || step.set(0), {
                    Box!(modifier = Modifier.size(50.dp()).focusable().test_tag("b"));
                });
            }
        });
    });
    let set = |value: u32| STEP.with(|cell| cell.borrow().unwrap().set(value));
    ui.on_node_with_tag("button").request_focus().assert_is_focused();
    set(1);
    ui.wait_for_idle();
    ui.on_node_with_tag("a").request_focus().assert_is_focused();
    // "Next": A closes and B opens in the same frame.
    set(2);
    ui.wait_for_idle();
    ui.on_node_with_tag("b").assert_exists();
    ui.press_key(Key::Escape);
    ui.on_node_with_tag("b").assert_does_not_exist();
    ui.on_node_with_tag("button").assert_is_focused();
}

/// Tab in Compose moves through focusable nodes, wrapping around; a dialog's own window (its
/// root) is no stop in the cycle. From the last focusable in the dialog, Tab goes back to the
/// first one, not to an invisible focus on the dialog itself.
#[test]
fn tab_from_the_last_focusable_of_a_dialog_wraps_to_its_first() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Dialog!(on_dismiss_request = || {}, {
            Column!({
                Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("in 1"));
                Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("in 2"));
            });
        });
    });
    ui.on_node_with_tag("in 2").request_focus().assert_is_focused();
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("in 1").assert_is_focused();
}

/// A popup that stops being focusable while it shows: on Android its window loses the focus
/// (`FLAG_NOT_FOCUSABLE` is set again) and the main window's focused node, which never changed,
/// is focused again. Here the focus is dropped altogether until the popup goes away.
#[test]
fn a_popup_that_stops_being_focusable_gives_the_focus_back() {
    thread_local! {
        static FOCUSABLE: RefCell<Option<MutableState<bool>>> = const { RefCell::new(None) };
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let focusable = remember(|| mutable_state_of(false));
        FOCUSABLE.with(|cell| *cell.borrow_mut() = Some(focusable));
        Column!({
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("button"));
            Popup!(
                on_dismiss_request = || {},
                properties =
                    PopupProperties { focusable: focusable.get(), ..PopupProperties::default() },
                {
                    Box!(modifier = Modifier.size(50.dp()).test_tag("popup"));
                }
            );
        });
    });
    let set = |value: bool| FOCUSABLE.with(|cell| cell.borrow().unwrap().set(value));
    ui.on_node_with_tag("button").request_focus().assert_is_focused();
    let button = ui.on_node_with_tag("button").node_id();
    set(true);
    ui.wait_for_idle();
    assert_ne!(focused(&mut ui), Some(button), "the focusable popup didn't take the focus");
    set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("button").assert_is_focused();
}

/// Escape (back) dismisses a dialog whatever is focused in it, as the dialog's window handles
/// back: also once the app cleared the focus in it (`focusManager.clearFocus()` after a field's
/// "Done"). Here the key has no focused node to go to, so nothing dismisses the dialog.
#[test]
fn escape_dismisses_a_dialog_after_its_focus_was_cleared() {
    let open = mutable_state_of(true);
    let handle = open.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        if handle.get() {
            Dialog!(on_dismiss_request = move || handle.set(false), {
                Box!(modifier = Modifier.size(50.dp()).focusable().test_tag("field"));
            });
        }
    });
    ui.on_node_with_tag("field").request_focus().assert_is_focused();
    let actions = ui.host().with_tree(|tree| tree.clear_focus());
    for action in actions {
        action.run();
    }
    ui.wait_for_idle();
    ui.press_key(Key::Escape);
    assert!(!open.get(), "Escape didn't dismiss the dialog once its focus was cleared");
}

/// Locks in: a dialog opened from a dialog (nested) gives the focus back to what had it in the
/// first dialog, then the first gives it back to the screen.
#[test]
fn passes_nested_dialogs_give_the_focus_back_in_turn() {
    thread_local! {
        static OUTER: RefCell<Option<MutableState<bool>>> = const { RefCell::new(None) };
        static INNER: RefCell<Option<MutableState<bool>>> = const { RefCell::new(None) };
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let outer = remember(|| mutable_state_of(false));
        let inner = remember(|| mutable_state_of(false));
        OUTER.with(|cell| *cell.borrow_mut() = Some(outer));
        INNER.with(|cell| *cell.borrow_mut() = Some(inner));
        Column!({
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("button"));
            if outer.get() {
                Dialog!(on_dismiss_request = move || outer.set(false), {
                    Column!({
                        Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("field"));
                        if inner.get() {
                            Dialog!(on_dismiss_request = move || inner.set(false), {
                                Box!(modifier = Modifier.size(20.dp()).test_tag("inner"));
                            });
                        }
                    });
                });
            }
        });
    });
    ui.on_node_with_tag("button").request_focus().assert_is_focused();
    OUTER.with(|cell| cell.borrow().unwrap().set(true));
    ui.wait_for_idle();
    ui.on_node_with_tag("field").request_focus().assert_is_focused();
    INNER.with(|cell| cell.borrow().unwrap().set(true));
    ui.wait_for_idle();
    ui.press_key(Key::Escape);
    ui.on_node_with_tag("inner").assert_does_not_exist();
    ui.on_node_with_tag("field").assert_is_focused();
    ui.press_key(Key::Escape);
    ui.on_node_with_tag("field").assert_does_not_exist();
    ui.on_node_with_tag("button").assert_is_focused();
}

// ---- Density -------------------------------------------------------------------------------

/// `LocalDensity` changing recomposes its readers (popups included), and only them: Compose's
/// `setContent` lambda doesn't run again, the owner providing the local around it.
#[test]
fn a_density_change_recomposes_its_readers_but_not_the_whole_content() {
    thread_local! {
        static CONTENT_RUNS: Cell<u32> = const { Cell::new(0) };
        static SEEN: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    }
    #[composable]
    fn Reader() {
        let density = LOCAL_DENSITY.current();
        SEEN.with(|seen| seen.borrow_mut().push(density.density));
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        CONTENT_RUNS.with(|c| c.set(c.get() + 1));
        Box!({
            Popup!({
                Reader();
            });
        });
    });
    let runs = CONTENT_RUNS.with(Cell::get);
    ui.host().set_density(Density { density: 2.0, font_scale: 1.0 });
    ui.wait_for_idle();
    assert_eq!(SEEN.with(|seen| seen.borrow().last().copied()), Some(2.0));
    assert_eq!(CONTENT_RUNS.with(Cell::get), runs, "the density change re-ran the whole content");
}
