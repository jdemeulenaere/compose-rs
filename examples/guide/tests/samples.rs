//! Runs the guide's samples (`src/`) and checks they do what the guide says.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose::foundation::layout::*;
use compose::foundation::{LazyListState, remember_lazy_list_state};
use compose::material3::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::platform::UiHost;
use compose::ui::*;
use guide::*;

fn wait_until(ui: &mut ComposeTestRule, matcher: impl Fn() -> Matcher) {
    for _ in 0..500 {
        ui.wait_for_idle();
        if ui.count_nodes(&matcher()) > 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("never shown:\n{}", ui.dump_semantics());
}

// ---- Composables ------------------------------------------------------------------------------

#[test]
fn greetings_take_named_and_default_arguments() {
    let mut ui = compose_test(composables::Greetings);
    for text in ["Hello, Ada.", "Hello, Grace!", "Hello, Linus.", "Hello, Barbara."] {
        ui.on_node_with_text(text).assert_exists();
    }
}

#[test]
fn sections_show_their_content() {
    let mut ui = compose_test(composables::Fruit);
    for text in ["Fruit", "Apple", "Pear", "Vegetables", "Leek"] {
        ui.on_node_with_text(text).assert_exists();
    }
}

// ---- State ------------------------------------------------------------------------------------

#[test]
fn the_counter_counts() {
    let mut ui = compose_test(state::Counter);
    ui.on_node_with_text("Clicked 0 times").perform_click();
    ui.on_node_with_text("Clicked 1 times").assert_exists();
}

#[test]
fn the_draft_keeps_what_is_typed() {
    let mut ui = compose_test(state::Draft);
    ui.on_node_with_tag("draft").perform_text_input("milk");
    ui.on_node_with_tag("draft").assert_text_equals(&["milk"]);
}

#[test]
fn a_derived_state_recomposes_only_when_its_answer_changes() {
    let mut ui = compose_test(state::LongList);
    ui.on_node_with_content_description("Back to the top").assert_does_not_exist();
    ui.host()
        .send_pointer(PointerEvent::scroll(Offset::new(100.0, 100.0), Offset::new(0.0, 300.0)));
    ui.wait_for_idle();
    ui.on_node_with_content_description("Back to the top").assert_exists();
    // Scrolling further changes the first visible item, not the answer: `LongList` stays.
    let before = ui.recomposition_counts();
    ui.host()
        .send_pointer(PointerEvent::scroll(Offset::new(100.0, 100.0), Offset::new(0.0, 300.0)));
    ui.wait_for_idle();
    assert_eq!(ui.recomposition_counts().since(&before).scopes_named("LongList"), 0);
    ui.on_node_with_content_description("Back to the top").perform_click();
    ui.on_node_with_content_description("Back to the top").assert_does_not_exist();
    ui.on_node_with_text("Item 0").assert_exists();
}

#[test]
fn the_stepper_is_hoisted() {
    let mut ui = compose_test(state::Quantity);
    ui.on_node_with_text("+").perform_click();
    ui.on_node_with_text("2").assert_exists();
    for _ in 0..3 {
        ui.on_node_with_text("-").perform_click();
    }
    ui.on_node_with_text("0").assert_exists();
}

#[test]
fn the_menu_opens_and_back_closes_it() {
    let mut ui = compose_test(state::Menu);
    assert!(!ui.host().has_enabled_back_handlers());
    ui.on_node_with_text("Open the menu").perform_click();
    assert!(ui.host().has_enabled_back_handlers(), "the open drawer takes back");
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    assert!(!ui.host().has_enabled_back_handlers());
}

#[composable]
fn OpenedProfile(opened: Rc<RefCell<Vec<String>>>) {
    state::Profile!(SharedString::from("Ada"), move |name: SharedString| opened
        .borrow_mut()
        .push(name.to_string()));
}

#[test]
fn the_profile_clones_its_name_where_lambdas_take_it() {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let recorded = opened.clone();
    let mut ui = compose_test(move || OpenedProfile(recorded.clone()));
    assert_eq!(ui.count_nodes(&has_text("Ada")), 2);
    ui.on_node_with_text("Open").perform_click();
    assert_eq!(*opened.borrow(), ["Ada"]);
}

#[test]
fn contacts_show_their_tags() {
    let mut ui = compose_test(state::ContactsScreen);
    ui.on_node_with_text("Ada (math)").assert_exists();
    ui.on_node_with_text("Grace (navy, cobol)").assert_exists();
}

// ---- Side effects -----------------------------------------------------------------------------

#[composable]
fn Subscribed(topic: MutableState<Option<&'static str>>, bus: Rc<Bus>) {
    if let Some(name) = topic.get() {
        effects::Subscription(SharedString::from(name), bus);
    }
}

use guide::effects::Bus;

#[test]
fn a_disposable_effect_cleans_up() {
    let bus = Rc::new(Bus::default());
    let topic = mutable_state_of(Some("news"));
    let (handle, shared) = (topic.handle(), bus.clone());
    let mut ui = compose_test(move || Subscribed(handle, shared.clone()));
    assert_eq!(bus.topics(), [SharedString::from("news")]);
    topic.set(Some("sports"));
    ui.wait_for_idle();
    assert_eq!(bus.topics(), [SharedString::from("sports")]);
    topic.set(None);
    ui.wait_for_idle();
    assert!(bus.topics().is_empty());
}

#[test]
fn a_side_effect_publishes_after_composing() {
    let analytics = Rc::new(effects::Analytics::default());
    let shared = analytics.clone();
    let _ui = compose_test(move || effects::Tracked(SharedString::from("Home"), shared.clone()));
    assert_eq!(*analytics.screen.borrow(), "Home");
}

#[test]
fn background_work_reads_a_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let bytes = std::fs::metadata(path).unwrap().len();
    let mut ui = compose_test(move || effects::FileSize(path.to_string()));
    let text = format!("{bytes} bytes");
    wait_until(&mut ui, || has_text(&text));
}

#[test]
fn a_handler_launches_a_snackbar() {
    let mut ui = ComposeTestRule::with_manual_clock(effects::Saver);
    ui.on_node_with_text("Save").perform_click();
    ui.advance_time_by_frame();
    ui.on_node_with_text("Saved").assert_exists();
}

// ---- Layouts ----------------------------------------------------------------------------------

#[test]
fn the_layout_samples_compose() {
    let mut ui = compose_test(|| layouts::Author(SharedString::from("Ada"), true));
    ui.on_node_with_text("Ada").assert_exists();
    ui.on_node_with_text("Follow").assert_exists();
    let mut ui = compose_test(|| layouts::Toolbar(false));
    ui.on_node_with_text("Share").assert_exists();
}

#[composable]
fn Clicks(clicks: MutableState<u32>) {
    layouts::ModifierForms!(false, move || clicks.update(|count| count + 1));
}

#[test]
fn modifier_forms_compose() {
    let clicks = mutable_state_of(0);
    let handle = clicks.handle();
    let mut ui = compose_test(move || Clicks(handle));
    ui.on_node(has_role(Role::Button)).perform_click();
    assert_eq!(clicks.get(), 1);
}

#[composable]
fn Inbox(state: LazyListState) {
    let days = remember(|| {
        let day = |title: &str, first: u64| layouts::Day {
            title: SharedString::from(title.to_string()),
            messages: (first..first + 3)
                .map(|id| layouts::Message { id, text: format!("Message {id}").into() })
                .collect(),
        };
        ImmutableList::from(vec![day("Monday", 0), day("Tuesday", 10)])
    });
    layouts::Messages(days, state);
}

#[composable]
fn ScrolledInbox() {
    let state = remember_lazy_list_state!();
    Inbox(state);
}

#[test]
fn sticky_headers_stay_at_the_top() {
    let mut ui = compose_test(ScrolledInbox);
    ui.on_node_with_text("Monday").assert_exists();
    ui.on_node_with_text("Message 11").assert_exists();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 60), ScrolledInbox);
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(100.0, 30.0), Offset::new(0.0, 40.0)));
    ui.wait_for_idle();
    // Scrolled past Monday's header, it sticks to the top over its messages.
    let header = ui.on_node_with_text("Monday").fetch_semantics_node();
    assert_eq!(header.bounds.top, 0.0, "{}", ui.dump_semantics());
}

#[composable]
fn Wrapped() {
    guide::layouts::Wrap!({
        for index in 0..3 {
            Box!(modifier = Modifier.size((150.dp(), 20.dp())).test_tag(format!("box {index}")));
        }
    });
}

#[test]
fn the_custom_layout_wraps() {
    let mut ui = compose_test(Wrapped);
    let third = ui.on_node_with_tag("box 2").fetch_semantics_node().bounds;
    assert_eq!((third.left, third.top), (0.0, 20.0));
    let second = ui.on_node_with_tag("box 1").fetch_semantics_node().bounds;
    assert_eq!((second.left, second.top), (150.0, 0.0));
}

#[test]
fn the_layout_modifier_indents() {
    let mut ui = compose_test(layouts::Outline);
    assert_eq!(ui.on_node_with_tag("section").fetch_semantics_node().bounds.left, 16.0);
}

// ---- Drawing ----------------------------------------------------------------------------------

#[test]
fn the_drawing_samples_draw() {
    let ui = compose_test(|| drawing::Gauge(0.5));
    assert!(ui.dump_display_list().contains("arc"), "{}", ui.dump_display_list());
    let ui = compose_test(drawing::Ribbon);
    assert!(ui.dump_display_list().contains("path"), "{}", ui.dump_display_list());
    let mut ui = compose_test(|| drawing::Badge(SharedString::from("New")));
    ui.on_node_with_text("New").assert_exists();
    let _ = compose_test(drawing::Gradients);
}

#[composable]
fn DimmedText() {
    drawing::Dimmed!({
        Text!("Behind the scrim");
    });
}

#[test]
fn the_scrim_draws_over_the_content() {
    let ui = compose_test(DimmedText);
    let display = ui.dump_display_list();
    let text = display.find("paragraph").expect("the text");
    let scrim = display.rfind("#4D000000").expect("the scrim");
    assert!(text < scrim, "{display}");
}

// ---- Theming ----------------------------------------------------------------------------------

#[test]
fn the_scaffold_adds_notes() {
    let mut ui = compose_test(theming::Notes);
    ui.on_node_with_text("0 notes").assert_exists();
    ui.on_node_with_text("Clear").assert_is_not_enabled();
    ui.on_node_with_content_description("Add a note").perform_click();
    ui.on_node_with_text("1 notes").assert_exists();
    ui.on_node_with_text("Clear").perform_click();
    ui.on_node_with_text("0 notes").assert_exists();
    let mut ui = compose_test(theming::Spaced);
    ui.on_node_with_text("Two").assert_exists();
    let mut ui = compose_test(|| theming::Banner(SharedString::from("Hi")));
    ui.on_node_with_text("Hi").assert_exists();
}

// ---- Text -------------------------------------------------------------------------------------

#[test]
fn texts_compose() {
    let mut ui =
        compose_test(|| text::Headline(SharedString::from("Title"), SharedString::from("Summary")));
    ui.on_node_with_text("3 unread").assert_exists();
    let mut ui = compose_test(text::Terms);
    ui.on_node_with_text("By joining, you accept the rules.").assert_exists();
}

#[composable]
fn Join(joined: Rc<RefCell<Vec<(String, String)>>>) {
    text::JoinNetwork!(move |network: String, passphrase: String| {
        joined.borrow_mut().push((network, passphrase))
    });
}

#[test]
fn the_form_moves_the_focus_and_submits() {
    let joined = Rc::new(RefCell::new(Vec::new()));
    let shared = joined.clone();
    let mut ui = compose_test(move || Join(shared.clone()));
    ui.on_node_with_tag("network").assert_is_focused();
    ui.type_text("home");
    // The keyboard's action key: `Next` moves the focus, `Done` joins.
    ui.host().perform_ime_action(ImeAction::Next);
    ui.wait_for_idle();
    ui.on_node_with_tag("passphrase").assert_is_focused();
    ui.type_text("secret");
    ui.host().perform_ime_action(ImeAction::Done);
    ui.wait_for_idle();
    assert_eq!(*joined.borrow(), [("home".to_string(), "secret".to_string())]);
}

// ---- Navigation -------------------------------------------------------------------------------

#[test]
fn navigation_pushes_and_back_pops() {
    let mut ui = compose_test(navigation::App);
    assert!(!ui.host().has_enabled_back_handlers(), "back leaves the app at the root");
    ui.on_node_with_text("Message 3").perform_click();
    ui.on_node_with_content_description("Star").perform_click();
    ui.on_node_with_content_description("Star").assert_is_on();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_text("Settings").perform_click();
    ui.on_node_with_text("Message 3").assert_does_not_exist();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_text("Message 3").assert_exists();
}

// ---- Android interop --------------------------------------------------------------------------

#[test]
fn properties_and_events_go_between_the_host_and_the_content() {
    let mut ui = compose_test(interop::Embedded);
    ui.on_node_with_text("Signed in as nobody").assert_exists();
    // What `ComposeRsView.setProp("user", "ada")` does.
    ui.host().props().set("user", Some(PropValue::Text("ada".into())));
    ui.wait_for_idle();
    ui.on_node_with_text("Signed in as ada").assert_exists();
    let events = Rc::new(RefCell::new(Vec::new()));
    let recorded = events.clone();
    ui.host().set_event_listener(move |name, payload| {
        recorded.borrow_mut().push(format!("{name}: {payload}"));
    });
    ui.on_node_with_text("Sign out").perform_click();
    assert_eq!(*events.borrow(), ["sign_out: "]);
}

#[test]
fn the_configuration_is_the_hosts() {
    let mut host = UiHost::headless(400, 800);
    let mut configuration = host.configuration();
    configuration.screen_width_dp = 800.0;
    host.set_configuration(configuration);
    let mut ui = ComposeTestRule::with_host(host, interop::Adaptive);
    ui.on_node_with_text("Two panes").assert_exists();
}

// ---- Performance ------------------------------------------------------------------------------

#[test]
fn an_animated_layer_recomposes_nothing() {
    let mut ui = ComposeTestRule::with_manual_clock(performance::LiveDot);
    ui.advance_time_by_frame();
    let before = ui.recomposition_counts();
    for _ in 0..10 {
        ui.advance_time_by_frame();
    }
    let ran = ui.recomposition_counts().since(&before);
    assert!(ran.names().is_empty(), "{:?}", ran.names());
}

#[test]
fn scrolling_recomposes_nothing_when_read_while_drawing() {
    let mut ui = compose_test(|| performance::Article(40));
    let before = ui.recomposition_counts();
    ui.host()
        .send_pointer(PointerEvent::scroll(Offset::new(100.0, 100.0), Offset::new(0.0, 200.0)));
    ui.wait_for_idle();
    let ran = ui.recomposition_counts().since(&before);
    assert!(ran.names().is_empty(), "{:?}", ran.names());
}

#[test]
fn the_spinner_is_in_a_repaint_boundary() {
    let mut ui = ComposeTestRule::with_manual_clock(|| performance::Syncing(3));
    ui.advance_time_by_frame();
    ui.on_node_with_text("Syncing 3 items").assert_exists();
}
