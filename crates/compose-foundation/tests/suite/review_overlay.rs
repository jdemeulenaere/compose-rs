//! Adversarial review of popups and dialogs (overlays): positions, focus, input routing and
//! recomposition, against Jetpack Compose's behaviour.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, Dialog, IntOffset, IntSize, Key, Offset, PointerEvent, Popup, PopupProperties,
    UnitExt,
};

fn tap(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds
}

// ---- Focus ---------------------------------------------------------------------------------

/// A dialog is its own window in Compose: Tab cycles through the dialog's focusables only, it
/// never moves the focus to the content behind the scrim (which would then take Enter and keys
/// while the dialog is modal).
#[test]
fn tab_in_a_dialog_never_moves_the_focus_behind_it() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Column!({
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("behind 1"));
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("behind 2"));
            Dialog!(on_dismiss_request = || {}, {
                Column!({
                    Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("in 1"));
                    Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("in 2"));
                });
            });
        });
    });
    let behind =
        [ui.on_node_with_tag("behind 1").node_id(), ui.on_node_with_tag("behind 2").node_id()];
    for step in 0..6 {
        ui.press_key(Key::Tab);
        let focused = ui.host().with_tree(|tree| tree.focused());
        assert!(
            focused.is_some_and(|focused| !behind.contains(&focused)),
            "Tab #{step} moved the focus behind the modal dialog"
        );
    }
}

/// In Compose a dialog's window takes the focus without the main window losing its focused
/// node: once the dialog is dismissed, what was focused before it opened is focused again.
#[test]
fn the_focus_comes_back_to_what_had_it_once_a_dialog_is_dismissed() {
    thread_local! { static OPEN: RefCell<Option<MutableState<bool>>> = const { RefCell::new(None) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let open = remember(|| mutable_state_of(false));
        OPEN.with(|cell| *cell.borrow_mut() = Some(open));
        Column!({
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("button"));
            if open.get() {
                Dialog!(on_dismiss_request = move || open.set(false), {
                    Box!(modifier = Modifier.size(50.dp()).test_tag("dialog"));
                });
            }
        });
    });
    ui.on_node_with_tag("button").request_focus().assert_is_focused();
    OPEN.with(|cell| cell.borrow().unwrap().set(true));
    ui.wait_for_idle();
    ui.on_node_with_tag("dialog").assert_exists();
    ui.press_key(Key::Escape);
    ui.on_node_with_tag("dialog").assert_does_not_exist();
    ui.on_node_with_tag("button").assert_is_focused();
}

/// The focus moving into a dialog's (or a focusable popup's) window doesn't scroll the main
/// window's containers in Compose: the dialog isn't in them.
#[test]
fn opening_a_dialog_inside_a_scrolled_column_does_not_scroll_it() {
    thread_local! {
        static STATE: RefCell<Option<ScrollState>> = const { RefCell::new(None) };
        static OPEN: RefCell<Option<MutableState<bool>>> = const { RefCell::new(None) };
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let state = remember_scroll_state!();
        let open = remember(|| mutable_state_of(false));
        STATE.with(|cell| *cell.borrow_mut() = Some(state));
        OPEN.with(|cell| *cell.borrow_mut() = Some(open));
        // A 100 px viewport, 100 px down the window.
        Column!(
            modifier = Modifier
                .padding((0.dp(), 100.dp(), 0.dp(), 0.dp()))
                .size((100.dp(), 100.dp()))
                .vertical_scroll(state),
            {
                for index in 0..20 {
                    Box!(
                        modifier = Modifier.size((100.dp(), 50.dp())).test_tag(format!("{index}"))
                    );
                    if index == 10 && open.get() {
                        Dialog!(on_dismiss_request = move || open.set(false), {
                            Box!(modifier = Modifier.size(50.dp()).test_tag("dialog"));
                        });
                    }
                }
            }
        );
    });
    let state = STATE.with(|cell| (*cell.borrow()).unwrap());
    state.dispatch_raw_delta(400.0 - state.value() as f32);
    ui.wait_for_idle();
    assert_eq!(state.value(), 400);
    OPEN.with(|cell| cell.borrow().unwrap().set(true));
    ui.wait_for_idle();
    ui.on_node_with_tag("dialog").assert_exists();
    assert_eq!(state.value(), 400, "opening the dialog scrolled the column behind it");
}

// ---- Positions -----------------------------------------------------------------------------

/// Compose anchors a popup to its parent layout's inner coordinates
/// (`parentLayoutCoordinates`: inside the parent's modifiers, such as its padding), not to the
/// parent's outer bounds.
#[test]
fn a_popup_is_anchored_inside_its_parents_padding() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(modifier = Modifier.padding(30.dp()).size(40.dp()), {
            Popup!(alignment = Alignment::BOTTOM_END, {
                Box!(modifier = Modifier.size(10.dp()).test_tag("popup"));
            });
        });
    });
    // The anchor is the 40 x 40 content at (30, 30): the popup's bottom end on (70, 70).
    let popup = bounds(&mut ui, "popup");
    assert_eq!((popup.left, popup.top), (60.0, 60.0));
}

/// A dialog is centered in the window, also once the window is resized.
#[test]
fn a_dialog_is_centered_again_when_the_window_is_resized() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(modifier = Modifier.size(100.dp()), {
            Dialog!(on_dismiss_request = || {}, {
                Box!(modifier = Modifier.size(100.dp()).test_tag("dialog"));
            });
        });
    });
    let dialog = bounds(&mut ui, "dialog");
    assert_eq!((dialog.left, dialog.top), (100.0, 100.0));
    ui.host().resize(IntSize::new(500, 700));
    ui.wait_for_idle();
    let dialog = bounds(&mut ui, "dialog");
    assert_eq!((dialog.left, dialog.top), (200.0, 300.0), "not centered in the resized window");
}

/// A popup measured with the window's size: content filling the max width fills the window,
/// also after the window grows.
#[test]
fn a_popup_is_measured_with_the_new_window_size_after_a_resize() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(modifier = Modifier.size(50.dp()), {
            Popup!({
                Box!(modifier = Modifier.fill_max_width().height(10.dp()).test_tag("popup"));
            });
        });
    });
    assert_eq!(bounds(&mut ui, "popup").width(), 300.0);
    ui.host().resize(IntSize::new(600, 300));
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "popup").width(), 600.0);
}

/// Locks in: a popup follows its anchor when a scroll moves it.
#[test]
fn passes_a_popup_follows_its_anchor_when_it_scrolls() {
    thread_local! { static STATE: RefCell<Option<ScrollState>> = const { RefCell::new(None) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let state = remember_scroll_state!();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));
        Column!(modifier = Modifier.fill_max_size().vertical_scroll(state), {
            for index in 0..20 {
                Box!(modifier = Modifier.size((100.dp(), 50.dp())), {
                    if index == 3 {
                        Popup!(offset = IntOffset::new(5, 5), {
                            Box!(modifier = Modifier.size(10.dp()).test_tag("popup"));
                        });
                    }
                });
            }
        });
    });
    assert_eq!(bounds(&mut ui, "popup").top, 155.0);
    let state = STATE.with(|cell| (*cell.borrow()).unwrap());
    state.dispatch_raw_delta(100.0 - state.value() as f32);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "popup").top, 55.0);
}

/// A popup inside a popup, both shown from the start: the inner one is over the outer one and
/// anchored where the outer one is.
#[test]
fn passes_a_nested_popup_is_over_and_anchored_in_its_parent_popup() {
    thread_local! { static CLICKS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) }; }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Box!(modifier = Modifier.size(10.dp()), {
            Popup!(offset = IntOffset::new(100, 100), {
                Box!(
                    modifier = Modifier
                        .size(80.dp())
                        .clickable(|| CLICKS.with(|c| c.borrow_mut().push("outer")))
                        .test_tag("outer"),
                    {
                        Popup!(offset = IntOffset::new(10, 10), {
                            Box!(
                                modifier = Modifier
                                    .size(20.dp())
                                    .clickable(|| CLICKS.with(|c| c.borrow_mut().push("inner")))
                                    .test_tag("inner")
                            );
                        });
                    }
                );
            });
        });
    });
    let inner = bounds(&mut ui, "inner");
    assert_eq!((inner.left, inner.top), (110.0, 110.0));
    tap(&mut ui, 115.0, 115.0);
    assert_eq!(CLICKS.with(|c| c.borrow().clone()), vec!["inner"]);
}

// ---- Pointer input -------------------------------------------------------------------------

/// A popup whose `dismiss_on_click_outside` turns on while it shows: its outside-click catcher
/// is under it, as from the start (taps on the popup still reach its content and don't
/// dismiss it).
#[test]
fn a_popup_whose_dismiss_on_click_outside_turns_on_still_takes_taps_on_itself() {
    thread_local! {
        static CLICKS: Cell<u32> = const { Cell::new(0) };
        static OUTSIDE: RefCell<Option<MutableState<bool>>> = const { RefCell::new(None) };
    }
    let open = mutable_state_of(true);
    let handle = open.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let outside = remember(|| mutable_state_of(false));
        OUTSIDE.with(|cell| *cell.borrow_mut() = Some(outside));
        Box!({
            if handle.get() {
                Popup!(
                    on_dismiss_request = move || handle.set(false),
                    properties = PopupProperties {
                        dismiss_on_click_outside: outside.get(),
                        ..PopupProperties::default()
                    },
                    {
                        Box!(
                            modifier = Modifier
                                .size(50.dp())
                                .clickable(|| CLICKS.with(|c| c.set(c.get() + 1)))
                                .test_tag("popup")
                        );
                    }
                );
            }
        });
    });
    OUTSIDE.with(|cell| cell.borrow().unwrap().set(true));
    ui.wait_for_idle();
    tap(&mut ui, 25.0, 25.0);
    assert!(open.get(), "a tap on the popup dismissed it");
    assert_eq!(CLICKS.with(Cell::get), 1, "the popup's content didn't get the tap");
}

/// On Android a non-focusable popup (`PopupProperties()`'s default) doesn't take touches
/// outside it (`FLAG_NOT_FOCUSABLE` implies `FLAG_NOT_TOUCH_MODAL`): a tap outside dismisses
/// it (`ACTION_OUTSIDE`) and also reaches what's beneath. Only a focusable popup (a menu)
/// swallows it.
#[test]
fn a_tap_outside_a_non_focusable_popup_also_reaches_what_is_beneath() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let open = mutable_state_of(true);
    let handle = open.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Box!(
            modifier = Modifier.fill_max_size().clickable(|| CLICKS.with(|c| c.set(c.get() + 1))),
            {
                if handle.get() {
                    Popup!(on_dismiss_request = move || handle.set(false), {
                        Box!(modifier = Modifier.size(50.dp()).test_tag("popup"));
                    });
                }
            }
        );
    });
    tap(&mut ui, 200.0, 200.0);
    assert!(!open.get());
    assert_eq!(CLICKS.with(Cell::get), 1, "the tap outside the popup didn't reach beneath it");
}

/// Compose's popup dismisses on the press outside it (`ACTION_DOWN` outside / `ACTION_OUTSIDE`),
/// not on the release.
#[test]
fn a_popup_is_dismissed_on_the_press_outside_it() {
    let open = mutable_state_of(true);
    let handle = open.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Box!({
            if handle.get() {
                Popup!(
                    on_dismiss_request = move || handle.set(false),
                    properties = PopupProperties { focusable: true, ..PopupProperties::default() },
                    {
                        Box!(modifier = Modifier.size(50.dp()).test_tag("popup"));
                    }
                );
            }
        });
    });
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 200.0)));
    ui.wait_for_idle();
    assert!(!open.get(), "still open after the press outside");
}

// ---- Recomposition -------------------------------------------------------------------------

/// Locks in: a dialog's dismiss callback changing is the one Escape and a scrim tap call.
#[test]
fn passes_a_dialogs_new_dismiss_callback_is_the_one_called() {
    thread_local! { static DISMISSED: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) }; }
    let count = mutable_state_of(0u32);
    let handle = count.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let value = handle.get();
        Dialog!(on_dismiss_request = move || DISMISSED.with(|d| d.borrow_mut().push(value)), {
            Box!(modifier = Modifier.size(50.dp()).test_tag("dialog"));
        });
    });
    count.set(1);
    ui.wait_for_idle();
    ui.press_key(Key::Escape);
    tap(&mut ui, 5.0, 5.0);
    count.set(2);
    ui.wait_for_idle();
    tap(&mut ui, 5.0, 5.0);
    assert_eq!(DISMISSED.with(|d| d.borrow().clone()), vec![1, 1, 2]);
}

// ---- Lazy lists ----------------------------------------------------------------------------

/// A popup in a lazy list's item that scrolled out of view: Compose disposes the item (and so
/// the popup); it is neither shown nor hit where the item last was.
#[test]
fn a_popup_in_a_lazy_item_scrolled_out_of_view_is_gone() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(100, |index| {
                    Box!(modifier = Modifier.size((100.dp(), 20.dp())), {
                        if index == 0 {
                            Popup!(offset = IntOffset::new(120, 0), {
                                Box!(
                                    modifier = Modifier
                                        .size(30.dp())
                                        .clickable(|| CLICKS.with(|c| c.set(c.get() + 1)))
                                        .test_tag("popup")
                                );
                            });
                        }
                    });
                });
            }
        );
    });
    ui.on_node_with_tag("popup").assert_exists();
    state.request_scroll_to_item(10, 0);
    ui.wait_for_idle();
    tap(&mut ui, 130.0, 10.0);
    assert_eq!(CLICKS.with(Cell::get), 0, "the popup of an item out of view took a tap");
    ui.on_node_with_tag("popup").assert_does_not_exist();
}

/// An item composed beyond the viewport (here placed past the list's end, clipped): Compose
/// composes only the visible items (a prefetched one is precomposed but never placed, and a
/// popup whose anchor isn't placed stays transparent, `canCalculatePosition` being false), so a
/// popup of an item out of view never shows nor takes taps. So does compose-rs since D470: a
/// list places only the items in view (but while an item holds the focus, D136).
#[test]
fn a_popup_in_a_lazy_item_composed_beyond_the_viewport_takes_no_taps() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(100, |index| {
                    Box!(modifier = Modifier.size((100.dp(), 20.dp())), {
                        if index == 6 {
                            Popup!(offset = IntOffset::new(120, 0), {
                                Box!(
                                    modifier = Modifier
                                        .size(30.dp())
                                        .clickable(|| CLICKS.with(|c| c.set(c.get() + 1)))
                                        .test_tag("popup")
                                );
                            });
                        }
                    });
                });
            }
        );
    });
    // Item 6 would be at 120: its popup at (120, 120).
    tap(&mut ui, 130.0, 130.0);
    tap(&mut ui, 130.0, 10.0);
    assert_eq!(CLICKS.with(Cell::get), 0, "the popup of an item beyond the viewport took a tap");
}
