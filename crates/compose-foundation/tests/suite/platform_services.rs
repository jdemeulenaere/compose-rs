//! Platform services as composition locals, as Compose's: `LOCAL_URI_HANDLER`,
//! `LOCAL_CLIPBOARD_MANAGER` and `LOCAL_HAPTIC_FEEDBACK`, the host's by default (set on its
//! tree), fakes where a subtree provides them; `combined_clickable`'s long click vibrates.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    HapticFeedback, HapticFeedbackType, LOCAL_CLIPBOARD_MANAGER, LOCAL_HAPTIC_FEEDBACK,
    LOCAL_URI_HANDLER, Offset, PointerEvent, UnitExt, UriHandler, platform::UiHost,
};

#[derive(Default)]
struct FakeUriHandler {
    opened: RefCell<Vec<String>>,
}

impl UriHandler for FakeUriHandler {
    fn open_uri(&self, uri: &str) {
        self.opened.borrow_mut().push(uri.to_string());
    }
}

#[derive(Default)]
struct FakeHapticFeedback {
    performed: RefCell<Vec<HapticFeedbackType>>,
}

impl HapticFeedback for FakeHapticFeedback {
    fn perform_haptic_feedback(&self, feedback_type: HapticFeedbackType) {
        self.performed.borrow_mut().push(feedback_type);
    }
}

#[composable]
fn Link(uri: &'static str) {
    let handler = LOCAL_URI_HANDLER.current();
    Box!(modifier = Modifier.size(20.dp()).test_tag(uri).clickable(move || handler.open_uri(uri)));
}

#[test]
fn the_uri_handler_is_the_hosts_unless_a_subtree_provides_one() {
    let host_handler = Rc::new(FakeUriHandler::default());
    let provided = Rc::new(FakeUriHandler::default());
    let host = UiHost::headless(400, 400);
    let handler: Rc<dyn UriHandler> = host_handler.clone();
    host.with_tree(|tree| tree.set_uri_handler(handler));
    let fake: Rc<dyn UriHandler> = provided.clone();
    let mut ui = ComposeTestRule::with_host(host, move || {
        let fake = fake.clone();
        Column!({
            Link("https://host.example");
            CompositionLocalProvider(LOCAL_URI_HANDLER.provides(fake.clone()), || {
                Link("https://provided.example")
            });
        });
    });
    ui.on_node_with_tag("https://host.example").perform_click();
    ui.on_node_with_tag("https://provided.example").perform_click();
    assert_eq!(*host_handler.opened.borrow(), ["https://host.example"]);
    assert_eq!(*provided.opened.borrow(), ["https://provided.example"]);
}

#[test]
fn the_clipboard_manager_is_the_one_text_fields_use() {
    let mut ui = compose_test(|| {
        let clipboard = LOCAL_CLIPBOARD_MANAGER.current();
        let copy = move || clipboard.set_text("copied");
        Box!(modifier = Modifier.size(20.dp()).test_tag("copy").clickable(copy));
    });
    ui.on_node_with_tag("copy").perform_click();
    let clipboard = ui.host().with_tree(|tree| tree.clipboard());
    assert_eq!(clipboard.get_text().as_deref(), Some("copied"));
    assert!(clipboard.has_text());
}

#[test]
fn a_long_click_performs_long_press_haptic_feedback() {
    let haptics = Rc::new(FakeHapticFeedback::default());
    let host = UiHost::headless(400, 400);
    let feedback: Rc<dyn HapticFeedback> = haptics.clone();
    host.with_tree(|tree| tree.set_haptic_feedback(feedback));
    let long_clicks = Rc::new(RefCell::new(0));
    let counted = long_clicks.clone();
    let mut ui = ComposeTestRule::with_host(host, move || {
        let counted = counted.clone();
        Column!({
            Box!(
                modifier = Modifier.size(100.dp()).combined_clickable(
                    Some(callback({
                        let counted = counted.clone();
                        move || *counted.borrow_mut() += 1
                    })),
                    None,
                    || {},
                )
            );
            Box!(modifier = Modifier.size(100.dp()).combined_clickable(None, None, || {}));
        });
    });
    ui.set_auto_advance(false);
    let press = |ui: &mut ComposeTestRule, y: f32, hold: u64| {
        ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, y)));
        ui.wait_for_idle();
        ui.advance_time_by(Duration::from_millis(hold));
        ui.host().send_pointer(PointerEvent::release(Offset::new(10.0, y)));
        ui.wait_for_idle();
        ui.advance_time_by(Duration::from_millis(500));
    };
    // A click: no feedback.
    press(&mut ui, 10.0, 50);
    assert!(haptics.performed.borrow().is_empty());
    // Held past the timeout: the long click, with feedback.
    press(&mut ui, 10.0, 600);
    assert_eq!(*long_clicks.borrow(), 1);
    assert_eq!(*haptics.performed.borrow(), [HapticFeedbackType::LongPress]);
    // Held without a long click: nothing to feel.
    press(&mut ui, 110.0, 600);
    assert_eq!(haptics.performed.borrow().len(), 1);
}

#[test]
fn a_provided_haptic_feedback_reaches_a_long_click_in_its_subtree() {
    let haptics = Rc::new(FakeHapticFeedback::default());
    let feedback: Rc<dyn HapticFeedback> = haptics.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        CompositionLocalProvider(LOCAL_HAPTIC_FEEDBACK.provides(feedback.clone()), || {
            Box!(
                modifier =
                    Modifier.size(100.dp()).combined_clickable(Some(callback(|| {})), None, || {},)
            );
        });
    });
    ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(*haptics.performed.borrow(), [HapticFeedbackType::LongPress]);
}

#[test]
fn without_a_platform_the_services_do_nothing_harmful() {
    let mut ui = compose_test(|| {
        let handler = LOCAL_URI_HANDLER.current();
        let haptics = LOCAL_HAPTIC_FEEDBACK.current();
        let act = move || {
            handler.open_uri("https://example.com");
            haptics.perform_haptic_feedback(HapticFeedbackType::Confirm);
        };
        Box!(modifier = Modifier.size(20.dp()).test_tag("act").clickable(act));
    });
    ui.on_node_with_tag("act").perform_click();
}
