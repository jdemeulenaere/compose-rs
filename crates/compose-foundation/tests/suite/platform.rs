//! What platforms give apps: back handling (`BackHandler`, dialogs dismissed by back), saved
//! state (`remember_saveable` across the host's recreation), the configuration
//! (`is_system_in_dark_theme`, `LOCAL_CONFIGURATION`), window insets paddings, the platform's
//! properties and events, and the platform's image decoder.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    BackHandler, Configuration, Density, Dialog, ImageBitmap, IntSize, Key, LOCAL_CONFIGURATION,
    LOCAL_HOST_EVENTS, LOCAL_HOST_PROPS, Popup, PopupProperties, PropValue, ScreenOrientation,
    UnitExt,
    platform::{ImageDecoder, Insets, PlatformWindowInsets, UiHost},
};

thread_local! {
    static LOG: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn take_log() -> Vec<&'static str> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn log(entry: &'static str) {
    LOG.with(|log| log.borrow_mut().push(entry));
}

// ---- Back --------------------------------------------------------------------------------

#[test]
fn the_innermost_enabled_back_handler_gets_back() {
    thread_local! { static INNER: Cell<Option<MutableState<bool>>> = const { Cell::new(None) }; }
    let mut ui = compose_test(|| {
        let inner_enabled = remember(|| mutable_state_of(true));
        INNER.set(Some(inner_enabled));
        BackHandler!(on_back = || log("outer"));
        Column!({
            BackHandler!(enabled = inner_enabled.get(), on_back = || log("inner"));
        });
    });
    assert!(ui.host().has_enabled_back_handlers());
    assert!(ui.host().on_back_pressed());
    assert_eq!(take_log(), ["inner"]);
    INNER.get().unwrap().set(false);
    ui.wait_for_idle();
    assert!(ui.host().on_back_pressed());
    assert_eq!(take_log(), ["outer"]);
}

#[test]
fn without_an_enabled_handler_back_is_not_taken() {
    thread_local! { static ON: Cell<Option<MutableState<bool>>> = const { Cell::new(None) }; }
    let mut ui = compose_test(|| {
        let on = remember(|| mutable_state_of(true));
        ON.set(Some(on));
        if on.get() {
            BackHandler!(on_back = || log("handler"));
        }
    });
    assert!(ui.host().has_enabled_back_handlers());
    // Gone from the composition: unregistered, and the platform goes back itself.
    ON.get().unwrap().set(false);
    ui.wait_for_idle();
    assert!(!ui.host().has_enabled_back_handlers());
    assert!(!ui.host().on_back_pressed());
    assert_eq!(take_log(), Vec::<&str>::new());
}

#[test]
fn back_dismisses_a_dialog_before_handlers_composed_after_it() {
    thread_local! { static OPEN: Cell<Option<MutableState<bool>>> = const { Cell::new(None) }; }
    let mut ui = compose_test(|| {
        let open = remember(|| mutable_state_of(true));
        OPEN.set(Some(open));
        if open.get() {
            Dialog!(on_dismiss_request = move || open.set(false), {
                Box!(modifier = Modifier.size(50.dp()).test_tag("dialog"));
            });
        }
        // Composed after the dialog: the dialog's own window still gets back first.
        BackHandler!(on_back = || log("screen"));
    });
    ui.on_node_with_tag("dialog").assert_exists();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_tag("dialog").assert_does_not_exist();
    assert_eq!(take_log(), Vec::<&str>::new());
    assert!(ui.host().on_back_pressed());
    assert_eq!(take_log(), ["screen"]);
}

#[test]
fn a_dialog_that_ignores_back_takes_it_without_dismissing() {
    let mut ui = compose_test(|| {
        Dialog!(
            on_dismiss_request = || log("dismissed"),
            properties = compose_ui::DialogProperties {
                dismiss_on_back_press: false,
                dismiss_on_click_outside: true,
            },
            {
                Box!(modifier = Modifier.size(50.dp()));
            }
        );
    });
    // Taken (nothing beneath gets it, the activity doesn't finish), not dismissed.
    assert!(ui.host().has_enabled_back_handlers());
    assert!(ui.host().on_back_pressed());
    assert_eq!(take_log(), Vec::<&str>::new());
}

#[test]
fn escape_that_nothing_focused_takes_goes_back() {
    let mut ui = compose_test(|| {
        BackHandler!(on_back = || log("back"));
    });
    ui.press_key(Key::Escape);
    assert_eq!(take_log(), ["back"]);
}

#[test]
fn only_a_focusable_popup_takes_back() {
    let mut ui = compose_test(|| {
        Popup!(on_dismiss_request = || log("plain"), {
            Box!(modifier = Modifier.size(20.dp()));
        });
    });
    assert!(!ui.host().has_enabled_back_handlers());
    drop(ui);
    let mut ui = compose_test(|| {
        Popup!(
            on_dismiss_request = || log("focusable"),
            properties = PopupProperties { focusable: true, ..PopupProperties::default() },
            {
                Box!(modifier = Modifier.size(20.dp()));
            }
        );
    });
    assert!(ui.host().on_back_pressed());
    assert_eq!(take_log(), ["focusable"]);
}

// ---- Saved state -------------------------------------------------------------------------

thread_local! {
    static COUNT: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
    static NAME: Cell<Option<MutableState<String>>> = const { Cell::new(None) };
}

#[composable]
fn SavedCounter() {
    let count = remember_saveable(|| mutable_state_of(0));
    let name = remember_saveable(|| mutable_state_of(String::from("first")));
    COUNT.set(Some(count));
    NAME.with(|cell| cell.set(Some(name)));
    Column!({
        BasicText!("Count: {}", count.get());
        BasicText!("Name: {}", name.get());
    });
}

/// The host is destroyed (an Android configuration change) and a new one composes the same
/// content: what `remember_saveable` remembered comes back, through bytes.
fn recreate(ui: ComposeTestRule, content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ui;
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    ComposeTestRule::with_host(host, content)
}

#[test]
fn remember_saveable_comes_back_in_a_new_host() {
    let mut ui = compose_test(SavedCounter);
    COUNT.get().unwrap().set(5);
    NAME.with(|cell| cell.take()).unwrap().set(String::from("second"));
    ui.wait_for_idle();
    ui.on_node_with_text("Count: 5").assert_exists();
    let mut ui = recreate(ui, SavedCounter);
    ui.on_node_with_text("Count: 5").assert_exists();
    ui.on_node_with_text("Name: second").assert_exists();
    // It goes on as state.
    COUNT.get().unwrap().set(6);
    ui.wait_for_idle();
    ui.on_node_with_text("Count: 6").assert_exists();
}

#[test]
fn saved_values_find_their_place_by_key_and_order() {
    thread_local! { static ITEMS: Cell<Option<MutableState<Vec<u32>>>> = const { Cell::new(None) }; }
    #[composable]
    fn Item(id: u32) {
        let clicks = remember_saveable(|| mutable_state_of(0u32));
        Box!(
            modifier = Modifier
                .size(10.dp())
                .clickable(move || clicks.update(|c| c + 1))
                .test_tag(format!("item {id}")),
            {
                BasicText!("{id}: {}", clicks.get());
            }
        );
    }
    let content = || {
        let items = remember(|| mutable_state_of(vec![1u32, 2, 3]));
        ITEMS.with(|cell| cell.set(Some(items)));
        Column!({
            for id in items.get() {
                key(id, || Item(id));
            }
            // Two unkeyed calls of one composable: told apart by their order.
            Item(10);
            Item(11);
        });
    };
    let mut ui = compose_test(content);
    ui.on_node_with_tag("item 2").perform_click();
    ui.on_node_with_tag("item 11").perform_click();
    ui.on_node_with_tag("item 11").perform_click();
    let ui = recreate(ui, content);
    let mut ui = ui;
    ui.on_node_with_text("1: 0").assert_exists();
    ui.on_node_with_text("2: 1").assert_exists();
    ui.on_node_with_text("10: 0").assert_exists();
    ui.on_node_with_text("11: 2").assert_exists();
}

#[test]
fn a_value_that_left_the_composition_is_not_saved() {
    thread_local! { static SHOWN: Cell<Option<MutableState<bool>>> = const { Cell::new(None) }; }
    let mut ui = compose_test(|| {
        let shown = remember(|| mutable_state_of(true));
        SHOWN.set(Some(shown));
        if shown.get() {
            let value = remember_saveable(|| 7u8);
            BasicText!("{value}");
        }
    });
    assert!(!ui.host().save_state().is_empty());
    SHOWN.get().unwrap().set(false);
    ui.wait_for_idle();
    assert!(ui.host().save_state().is_empty());
}

#[test]
fn a_custom_saver_converts_the_value() {
    #[derive(Clone, PartialEq, Debug)]
    struct Point(i32, i32);
    thread_local! { static POINT: Cell<Option<MutableState<Point>>> = const { Cell::new(None) }; }
    let content = || {
        let point = remember_saveable_with(
            saver(
                |point: &Point| Some((point.0, point.1).save()),
                |saved| <(i32, i32)>::restore(saved).map(|(x, y)| Point(x, y)),
            ),
            || mutable_state_of(Point(0, 0)),
        );
        POINT.with(|cell| cell.set(Some(point)));
        BasicText!("{:?}", point.get());
    };
    let mut ui = compose_test(content);
    POINT.with(|cell| cell.take()).unwrap().set(Point(3, -4));
    ui.wait_for_idle();
    let mut ui = recreate(ui, content);
    ui.on_node_with_text("Point(3, -4)").assert_exists();
}

#[test]
fn a_lazy_list_keeps_its_position_in_a_new_host() {
    thread_local! { static STATE: RefCell<Option<LazyListState>> = const { RefCell::new(None) }; }
    let content = || {
        let state = remember_lazy_list_state!();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 200.dp())),
            state = state,
            |scope: LazyListScope| {
                scope.items(100, |index| {
                    Box!(modifier = Modifier.size((100.dp(), 50.dp())), {
                        BasicText!("item {index}");
                    });
                });
            }
        );
    };
    let mut ui = compose_test(content);
    STATE.with(|cell| *cell.borrow()).unwrap().request_scroll_to_item(30, 10);
    ui.wait_for_idle();
    let mut ui = recreate(ui, content);
    let state = STATE.with(|cell| *cell.borrow()).unwrap();
    assert_eq!(state.first_visible_item_index(), 30);
    assert_eq!(state.first_visible_item_scroll_offset(), 10);
    ui.on_node_with_text("item 30").assert_exists();
}

#[test]
fn a_scroll_state_keeps_its_value_in_a_new_host() {
    thread_local! { static STATE: RefCell<Option<ScrollState>> = const { RefCell::new(None) }; }
    let content = || {
        let state = remember_scroll_state!();
        STATE.with(|cell| *cell.borrow_mut() = Some(state));
        Column!(modifier = Modifier.size((100.dp(), 100.dp())).vertical_scroll(state), {
            Box!(modifier = Modifier.size((100.dp(), 1000.dp())));
        });
    };
    let mut ui = compose_test(content);
    let state = STATE.with(|cell| *cell.borrow()).unwrap();
    state.dispatch_raw_delta(250.0 - state.value() as f32);
    ui.wait_for_idle();
    let _ui = recreate(ui, content);
    assert_eq!(STATE.with(|cell| *cell.borrow()).unwrap().value(), 250);
}

#[test]
fn saved_state_bytes_round_trip_and_reject_garbage() {
    let mut state = SavedState::default();
    state.values.insert(
        42,
        vec![
            SavedValue::Bool(true),
            SavedValue::Int(-3),
            SavedValue::Float(1.5),
            SavedValue::Text("é".into()),
            SavedValue::List(vec![SavedValue::Int(1), SavedValue::List(vec![])]),
        ],
    );
    let bytes = state.to_bytes();
    assert_eq!(SavedState::from_bytes(&bytes), Some(state));
    assert_eq!(SavedState::from_bytes(b"nope"), None);
    assert_eq!(SavedState::from_bytes(&bytes[..bytes.len() - 1]), None);
    // Values of another type than saved restore nothing (they're computed again).
    assert_eq!(<String as Saveable>::restore(&SavedValue::Int(1)), None);
    assert_eq!(<Option<u8> as Saveable>::restore(&Some(3u8).save()), Some(Some(3)));
}

// ---- Configuration -------------------------------------------------------------------------

#[test]
fn dark_theme_and_the_configuration_follow_the_host() {
    let mut ui = compose_test(|| {
        let dark = is_system_in_dark_theme();
        let configuration = LOCAL_CONFIGURATION.current();
        BasicText!(
            "{} {}x{} {:?}",
            if dark { "dark" } else { "light" },
            configuration.screen_width_dp,
            configuration.screen_height_dp,
            configuration.orientation
        );
    });
    ui.on_node_with_text("light 400x800 Portrait").assert_exists();
    let configuration =
        Configuration::from_window(IntSize::new(1600, 800), Density::new(2.0), true);
    assert_eq!(configuration.orientation, ScreenOrientation::Landscape);
    ui.host().set_configuration(configuration);
    ui.wait_for_idle();
    ui.on_node_with_text("dark 800x400 Landscape").assert_exists();
}

// ---- Window insets -----------------------------------------------------------------------

#[test]
fn insets_paddings_follow_the_host_insets() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.fill_max_size().system_bars_padding(), {
            Box!(modifier = Modifier.fill_max_size().test_tag("content"));
        });
        Box!(
            modifier = Modifier
                .fill_max_size()
                .window_insets_padding(WindowInsets::ime().only(WindowInsetsSides::BOTTOM)),
            {
                Box!(modifier = Modifier.fill_max_size().test_tag("above keyboard"));
            }
        );
    });
    let bounds = |ui: &mut ComposeTestRule, tag: &str| {
        ui.on_node_with_tag(tag).fetch_semantics_node().bounds
    };
    assert_eq!(bounds(&mut ui, "content").top, 0.0);
    ui.host().set_window_insets(PlatformWindowInsets {
        status_bars: Insets::new(0, 24, 0, 0),
        navigation_bars: Insets::new(0, 0, 10, 48),
        ime: Insets::new(0, 0, 0, 300),
        ..PlatformWindowInsets::default()
    });
    ui.wait_for_idle();
    let content = bounds(&mut ui, "content");
    assert_eq!(
        (content.left, content.top, content.right, content.bottom),
        (0.0, 24.0, 390.0, 752.0)
    );
    let above = bounds(&mut ui, "above keyboard");
    assert_eq!((above.top, above.bottom), (0.0, 500.0));
}

#[test]
fn insets_as_padding_values_are_read_in_composition() {
    let mut ui = compose_test(|| {
        let padding = WindowInsets::safe_drawing().as_padding_values();
        BasicText!("top {} bottom {}", padding.top.0, padding.bottom.0);
    });
    ui.on_node_with_text("top 0 bottom 0").assert_exists();
    ui.host().set_window_insets(PlatformWindowInsets {
        status_bars: Insets::new(0, 24, 0, 0),
        display_cutout: Insets::new(0, 30, 0, 0),
        ime: Insets::new(0, 0, 0, 100),
        ..PlatformWindowInsets::default()
    });
    ui.wait_for_idle();
    ui.on_node_with_text("top 30 bottom 100").assert_exists();
}

// ---- Props and events --------------------------------------------------------------------

#[test]
fn host_props_recompose_their_readers_and_events_reach_the_listener() {
    let mut ui = compose_test(|| {
        let props = LOCAL_HOST_PROPS.current();
        let tab = props.string("tab");
        let events = LOCAL_HOST_EVENTS.current();
        let send = move || {
            events.send("clicked", "payload");
        };
        Column!({
            BasicText!("tab {}", tab.as_deref().unwrap_or("none"));
            Box!(modifier = Modifier.size(20.dp()).clickable(send.clone()).test_tag("send"));
        });
    });
    ui.on_node_with_text("tab none").assert_exists();
    ui.host().props().set("tab", Some(PropValue::Text("todo".into())));
    ui.wait_for_idle();
    ui.on_node_with_text("tab todo").assert_exists();
    let received = Rc::new(RefCell::new(Vec::new()));
    let sink = received.clone();
    ui.host().set_event_listener(move |name, payload| {
        sink.borrow_mut().push(format!("{name}={payload}"));
    });
    ui.on_node_with_tag("send").perform_click();
    assert_eq!(*received.borrow(), ["clicked=payload"]);
}

// ---- Images --------------------------------------------------------------------------------

/// A decoder of "images" that are a width and a height byte, then gray pixels.
struct FakeDecoder;

impl ImageDecoder for FakeDecoder {
    fn decode(&self, bytes: &[u8]) -> Option<ImageBitmap> {
        let [width, height, gray] = *bytes else { return None };
        let pixels = [gray, gray, gray, 255].repeat(usize::from(width) * usize::from(height));
        Some(ImageBitmap::from_rgba_premultiplied(width.into(), height.into(), pixels))
    }
}

#[test]
fn images_decode_with_the_platform_decoder() {
    static PIXEL: [u8; 3] = [2, 3, 128];
    compose_ui::platform::set_image_decoder(Arc::new(FakeDecoder));
    let image = compose_ui::decode_image_bytes(&PIXEL).expect("decoded");
    assert_eq!((image.width(), image.height()), (2, 3));
    assert!(compose_ui::decode_image_bytes(b"not an image").is_none());
    thread_local! { static DECODED: RefCell<Vec<ImageBitmap>> = const { RefCell::new(Vec::new()) }; }
    let mut ui = compose_test(|| {
        let image = compose_ui::remember_image_resource(&PIXEL).expect("decoded");
        DECODED.with(|decoded| decoded.borrow_mut().push(image));
        let recompose = remember(|| mutable_state_of(0));
        BasicText!("{}", recompose.get());
        COUNT.set(Some(recompose));
    });
    COUNT.get().unwrap().set(1);
    ui.wait_for_idle();
    // Decoded once, where it's composed.
    DECODED.with(|decoded| {
        let decoded = decoded.borrow();
        assert_eq!(decoded.len(), 2);
        assert!(decoded[0].same_image(&decoded[1]));
    });
}

#[path = "platform_review.rs"]
mod review;
