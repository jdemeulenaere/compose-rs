//! Third adversarial review of overlays (popups, dialogs) and the semantics tree, against
//! Jetpack Compose (each popup and dialog a window of its own, `SemanticsConfiguration`'s
//! `set` and merge policies).

#![allow(non_snake_case)]

use std::cell::RefCell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Dialog, ImageBitmap, IntSize, Key, Offset, PointerEvent, Popup, PopupProperties, UnitExt,
};

fn tap(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

fn pixel() -> ImageBitmap {
    ImageBitmap::from_rgba_premultiplied(1, 1, vec![255, 0, 0, 255])
}

// ---- Dialog semantics ------------------------------------------------------------------------

/// Compose's dialog node (`Modifier.semantics { dialog() }` on `DialogLayout`, in the dialog's
/// own window) is as large as the dialog's content: what a screen reader outlines when it
/// announces the dialog. Here it's the scrim's node, the whole window.
#[test]
fn a_dialogs_semantics_node_has_its_contents_bounds() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        Dialog!(on_dismiss_request = || {}, {
            Box!(modifier = Modifier.size((100.dp(), 50.dp())).test_tag("content"));
        });
    });
    let content = ui.on_node_with_tag("content").fetch_semantics_node().bounds;
    assert_eq!((content.left, content.top, content.width()), (150.0, 175.0, 100.0));
    let tree = ui.host().semantics_tree(true);
    let dialog = tree
        .flatten()
        .into_iter()
        .find(|node| node.config.contains(&compose_ui::SemanticsProperties::IS_DIALOG))
        .cloned();
    let dialog = dialog.expect("a dialog node");
    assert_eq!(
        (dialog.bounds.left, dialog.bounds.top, dialog.bounds.width(), dialog.bounds.height()),
        (150.0, 175.0, 100.0, 50.0),
        "the dialog node covers the window: {:?}",
        dialog.bounds
    );
}

// ---- Focus restore ---------------------------------------------------------------------------

/// The overflow menu's "Delete…" item: its click closes the (focusable) popup and opens a
/// confirmation dialog in the same frame. In Compose both are windows of their own and the main
/// window keeps its focused node (the button that opened the menu) all along, so once the
/// dialog is dismissed the button has the focus again. Here the focus is lost: the dialog
/// records "nothing focused" as what to give back, since the popup's focus went with it.
#[test]
fn a_dialog_opened_from_a_closing_popup_gives_the_focus_back_to_the_screen() {
    thread_local! { static STEP: RefCell<Option<MutableState<u32>>> = const { RefCell::new(None) }; }
    #[composable]
    fn Screen(step: MutableState<u32>) {
        Column!({
            Box!(modifier = Modifier.size(20.dp()).focusable().test_tag("button"));
            if step.get() == 1 {
                Popup!(
                    on_dismiss_request = Some(move || step.set(0)),
                    properties = PopupProperties { focusable: true, ..PopupProperties::default() },
                    {
                        Box!(
                            modifier = Modifier
                                .size(50.dp())
                                .clickable(move || step.set(2))
                                .test_tag("delete")
                        );
                    }
                );
            }
            if step.get() == 2 {
                Dialog!(on_dismiss_request = move || step.set(0), {
                    Box!(modifier = Modifier.size(50.dp()).focusable().test_tag("confirm"));
                });
            }
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let step = remember(|| mutable_state_of(0u32));
        STEP.with(|cell| *cell.borrow_mut() = Some(step));
        Screen(step);
    });
    ui.on_node_with_tag("button").request_focus().assert_is_focused();
    STEP.with(|cell| cell.borrow().unwrap().set(1));
    ui.wait_for_idle();
    let delete = ui.on_node_with_tag("delete").fetch_semantics_node().center();
    tap(&mut ui, delete.x, delete.y);
    ui.on_node_with_tag("confirm").assert_exists();
    ui.press_key(Key::Escape);
    ui.on_node_with_tag("confirm").assert_does_not_exist();
    ui.on_node_with_tag("button").assert_is_focused();
}

// ---- Semantics precedence --------------------------------------------------------------------

/// `contentDescription = …` sets the property (`SemanticsConfiguration.set` replaces the list),
/// and a layout node's modifiers apply innermost first: an outer `semantics` describing an
/// image replaces the image's own description, as for any other property. Here both are kept
/// ("Star Rating" read out).
#[test]
fn an_outer_content_description_replaces_the_images_own() {
    let mut ui = compose_test(|| {
        Image!(
            pixel(),
            Some("Star"),
            modifier = Modifier
                .size(24.dp())
                .test_tag("image")
                .semantics(|config| config.set_content_description("Rating")),
        );
    });
    let node = ui.on_node_with_tag("image").fetch_semantics_node();
    assert_eq!(
        node.config
            .get(&compose_ui::SemanticsProperties::CONTENT_DESCRIPTION)
            .into_iter()
            .flatten()
            .map(|d| d.as_str())
            .collect::<Vec<_>>(),
        ["Rating"],
    );
}

/// The wizard of review_overlay2's `passes_a_dialog_replaced_by_another_in_one_frame_…`, in a
/// `#[composable]` function. That test's content is a plain closure, whose `if` blocks aren't
/// groups: dialog B reuses dialog A's nodes (and remembered values), so the focus never leaves
/// and the test passes without exercising the restore. With A and B really different dialogs,
/// as in Compose, A's focus goes with A, B records "nothing focused" as what to give back, and
/// once B is dismissed the button that had the focus before A opened doesn't get it back.
#[test]
fn a_dialog_replaced_by_another_in_one_frame_gives_the_focus_back_once_closed() {
    thread_local! { static STEP: RefCell<Option<MutableState<u32>>> = const { RefCell::new(None) }; }
    #[composable]
    fn Wizard(step: MutableState<u32>) {
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
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        let step = remember(|| mutable_state_of(0u32));
        STEP.with(|cell| *cell.borrow_mut() = Some(step));
        Wizard(step);
    });
    let set = |value: u32| STEP.with(|cell| cell.borrow().unwrap().set(value));
    ui.on_node_with_tag("button").request_focus().assert_is_focused();
    set(1);
    ui.wait_for_idle();
    ui.on_node_with_tag("a").request_focus().assert_is_focused();
    set(2);
    ui.wait_for_idle();
    ui.on_node_with_tag("b").assert_exists();
    ui.press_key(Key::Escape);
    ui.on_node_with_tag("b").assert_does_not_exist();
    ui.on_node_with_tag("button").assert_is_focused();
}

// ---- Semantics actions -----------------------------------------------------------------------

/// A text field's semantics node offers `RequestFocus` (Compose's `CoreTextField` sets
/// `requestFocus { … }` on its node), and the bridge offers AccessKit's Focus for it (the node
/// is `focusable`). Here the node is the field's decoration box, whose own chain has no focus
/// target (the inner field, part of its node, has it): since `RequestFocus` needs a focus
/// target in the node's own chain, a screen reader can't focus a text field any more.
#[test]
fn request_focus_on_a_text_fields_node_focuses_the_field() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), || {
        BasicTextField!(
            value = String::from("Hello"),
            on_value_change = |_| {},
            modifier = Modifier.fill_max_width().test_tag("field"),
        );
    });
    let node = ui.on_node_with_tag("field").fetch_semantics_node();
    assert!(
        node.config.contains(&compose_ui::SemanticsProperties::FOCUSED),
        "the field's node isn't focusable: {:?}",
        node.config
    );
    assert!(
        ui.host().perform_semantics_action(node.id, &compose_ui::SemanticsActions::REQUEST_FOCUS),
        "RequestFocus on the text field's node wasn't performed"
    );
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_is_focused();
}
