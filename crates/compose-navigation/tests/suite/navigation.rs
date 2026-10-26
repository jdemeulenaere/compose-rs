//! `NavController` and `NavHost` (D291): the back stack, each entry's saveable state, back, and
//! the stack saved across the host's recreation.

use std::cell::Cell;

use compose_animation::{fade_in, fade_out};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_navigation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::UiHost;
use compose_ui::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Screen {
    Home,
    Detail(u32),
    Settings,
}

impl Saveable for Screen {
    fn save(&self) -> SavedValue {
        SavedValue::Int(match *self {
            Screen::Home => -1,
            Screen::Settings => -2,
            Screen::Detail(id) => i64::from(id),
        })
    }

    fn restore(saved: &SavedValue) -> Option<Screen> {
        match *saved {
            SavedValue::Int(-1) => Some(Screen::Home),
            SavedValue::Int(-2) => Some(Screen::Settings),
            SavedValue::Int(id) => u32::try_from(id).ok().map(Screen::Detail),
            _ => None,
        }
    }
}

thread_local! {
    static NAV: Cell<Option<NavController<Screen>>> = const { Cell::new(None) };
    static LIST: Cell<Option<LazyListState>> = const { Cell::new(None) };
    static FADE: Cell<bool> = const { Cell::new(false) };
}

fn nav() -> NavController<Screen> {
    NAV.get().unwrap()
}

#[composable]
fn App() {
    let nav_controller = remember_nav_controller(Screen::Home);
    NAV.set(Some(nav_controller));
    let content = move |screen: Screen| match screen {
        Screen::Home => Home(),
        Screen::Detail(id) => Detail(id),
        Screen::Settings => BasicText!("Settings"),
    };
    if FADE.get() {
        NavHost!(
            nav_controller,
            enter_transition = fade_in(),
            exit_transition = fade_out(),
            content = content
        );
    } else {
        NavHost!(nav_controller, content = content);
    }
}

/// A saved counter and a list of a hundred items.
#[composable]
fn Home() {
    let clicks = remember_saveable(|| mutable_state_of(0u32));
    let list = remember_lazy_list_state!();
    LIST.set(Some(list));
    Column!({
        Box!(
            modifier = Modifier
                .size(20.dp())
                .clickable(move || clicks.update(|count| count + 1))
                .test_tag("home counter"),
        );
        BasicText!("Home clicks: {}", clicks.get());
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 200.dp())),
            state = list,
            |scope: LazyListScope| {
                scope.items(100, |index| {
                    Box!(modifier = Modifier.size((100.dp(), 50.dp())), {
                        BasicText!("item {index}");
                    });
                });
            }
        );
    });
}

/// A saved counter and a saved note, "note {id}".
#[composable]
fn Detail(id: u32) {
    let clicks = remember_saveable(|| mutable_state_of(0u32));
    let note = remember_saveable(|| format!("note {id}"));
    Column!({
        Box!(
            modifier = Modifier
                .size(20.dp())
                .clickable(move || clicks.update(|count| count + 1))
                .test_tag("detail counter"),
        );
        BasicText!("Detail {id} clicks: {}, {note}", clicks.get());
    });
}

fn destinations() -> Vec<Screen> {
    nav().current_back_stack().into_iter().map(|entry| entry.destination).collect()
}

fn navigate(ui: &mut ComposeTestRule, screen: Screen) {
    nav().navigate(screen);
    ui.wait_for_idle();
}

fn back(ui: &mut ComposeTestRule) -> bool {
    let handled = ui.host().on_back_pressed();
    ui.wait_for_idle();
    handled
}

/// The host is destroyed (the process's death) and a new one composes the app with what the
/// first saved, through bytes.
fn recreate(mut ui: ComposeTestRule) -> ComposeTestRule {
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    ComposeTestRule::with_host(host, App)
}

/// Whether `state` holds the text `text`, anywhere.
fn saves_text(state: &SavedState, text: &str) -> bool {
    fn holds(value: &SavedValue, text: &str) -> bool {
        match value {
            SavedValue::Text(value) => value == text,
            SavedValue::List(values) => values.iter().any(|value| holds(value, text)),
            _ => false,
        }
    }
    state.values.values().flatten().any(|value| holds(value, text))
}

#[test]
fn navigate_shows_the_destination_and_pop_goes_back() {
    let mut ui = compose_test(App);
    ui.on_node_with_text("Home clicks: 0").assert_exists();
    assert_eq!(nav().current_destination(), Screen::Home);
    assert_eq!(nav().previous_back_stack_entry(), None);
    navigate(&mut ui, Screen::Detail(1));
    ui.on_node_with_text("Detail 1 clicks: 0, note 1").assert_exists();
    ui.on_node_with_text("Home clicks: 0").assert_does_not_exist();
    assert_eq!(nav().current_destination(), Screen::Detail(1));
    assert_eq!(nav().previous_back_stack_entry().unwrap().destination, Screen::Home);
    navigate(&mut ui, Screen::Settings);
    assert_eq!(destinations(), [Screen::Home, Screen::Detail(1), Screen::Settings]);
    assert!(nav().pop_back_stack());
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1 clicks: 0, note 1").assert_exists();
    assert!(nav().pop_back_stack());
    ui.wait_for_idle();
    ui.on_node_with_text("Home clicks: 0").assert_exists();
    // The start destination stays.
    assert!(!nav().pop_back_stack());
    assert_eq!(destinations(), [Screen::Home]);
}

#[test]
fn pop_up_to_and_single_top() {
    let mut ui = compose_test(App);
    for screen in [Screen::Detail(1), Screen::Detail(2), Screen::Settings] {
        navigate(&mut ui, screen);
    }
    // Pops up to Detail(1), it included, then pushes Detail(3).
    nav().navigate_with(
        Screen::Detail(3),
        NavOptions {
            pop_up_to: Some(Screen::Detail(1)),
            pop_up_to_inclusive: true,
            ..NavOptions::default()
        },
    );
    ui.wait_for_idle();
    assert_eq!(destinations(), [Screen::Home, Screen::Detail(3)]);
    ui.on_node_with_text("Detail 3 clicks: 0, note 3").assert_exists();
    // Single top: the top isn't pushed again, and keeps its entry (and its state).
    let top = nav().current_back_stack_entry();
    ui.on_node_with_tag("detail counter").perform_click();
    nav().navigate_with(
        Screen::Detail(3),
        NavOptions { launch_single_top: true, ..NavOptions::default() },
    );
    ui.wait_for_idle();
    assert_eq!(nav().current_back_stack_entry(), top);
    ui.on_node_with_text("Detail 3 clicks: 1, note 3").assert_exists();
    // Without it, a second entry of the destination, with state of its own.
    navigate(&mut ui, Screen::Detail(3));
    assert_eq!(destinations(), [Screen::Home, Screen::Detail(3), Screen::Detail(3)]);
    ui.on_node_with_text("Detail 3 clicks: 0, note 3").assert_exists();
    // A destination not on the stack pops nothing.
    nav().navigate_with(
        Screen::Settings,
        NavOptions { pop_up_to: Some(Screen::Detail(9)), ..NavOptions::default() },
    );
    ui.wait_for_idle();
    assert_eq!(destinations().len(), 4);
    // The single-top home of a bottom navigation bar: pop up to the start, keep it.
    nav().navigate_with(
        Screen::Home,
        NavOptions {
            pop_up_to: Some(Screen::Home),
            launch_single_top: true,
            ..NavOptions::default()
        },
    );
    ui.wait_for_idle();
    assert_eq!(destinations(), [Screen::Home]);
    // `pop_back_stack_to`.
    navigate(&mut ui, Screen::Detail(1));
    navigate(&mut ui, Screen::Settings);
    assert!(nav().pop_back_stack_to(&Screen::Detail(1), false));
    assert_eq!(destinations(), [Screen::Home, Screen::Detail(1)]);
    assert!(!nav().pop_back_stack_to(&Screen::Home, true), "the stack is never emptied");
    assert!(nav().pop_back_stack_to(&Screen::Home, false));
    assert_eq!(destinations(), [Screen::Home]);
}

#[test]
fn an_entry_keeps_its_state_while_on_the_stack() {
    let mut ui = compose_test(App);
    ui.on_node_with_tag("home counter").perform_click();
    ui.on_node_with_tag("home counter").perform_click();
    LIST.get().unwrap().request_scroll_to_item(30, 10);
    ui.wait_for_idle();
    navigate(&mut ui, Screen::Detail(1));
    ui.on_node_with_tag("detail counter").perform_click();
    navigate(&mut ui, Screen::Settings);
    assert!(back(&mut ui));
    // The detail's counter, then the home's counter and list position, come back.
    ui.on_node_with_text("Detail 1 clicks: 1, note 1").assert_exists();
    assert!(back(&mut ui));
    ui.on_node_with_text("Home clicks: 2").assert_exists();
    let list = LIST.get().unwrap();
    assert_eq!(list.first_visible_item_index(), 30);
    assert_eq!(list.first_visible_item_scroll_offset(), 10);
    ui.on_node_with_text("item 30").assert_exists();
}

#[test]
fn a_popped_entry_leaves_no_state_behind() {
    let mut ui = compose_test(App);
    navigate(&mut ui, Screen::Detail(7));
    ui.on_node_with_tag("detail counter").perform_click();
    navigate(&mut ui, Screen::Settings);
    // Kept while under the top…
    assert!(saves_text(&ui.host().save_state(), "note 7"));
    assert!(back(&mut ui));
    assert!(back(&mut ui));
    // …and gone once popped.
    assert!(!saves_text(&ui.host().save_state(), "note 7"));
    // Navigating there again is a new entry, with new state.
    navigate(&mut ui, Screen::Detail(7));
    ui.on_node_with_text("Detail 7 clicks: 0, note 7").assert_exists();
}

#[test]
fn back_pops_and_leaves_the_app_at_the_start_destination() {
    let mut ui = compose_test(App);
    // At the start, no handler wants back: the platform leaves the app.
    assert!(!ui.host().has_enabled_back_handlers());
    assert!(!back(&mut ui));
    navigate(&mut ui, Screen::Detail(1));
    assert!(ui.host().has_enabled_back_handlers());
    assert!(back(&mut ui));
    ui.on_node_with_text("Home clicks: 0").assert_exists();
    assert!(!ui.host().has_enabled_back_handlers());
}

#[test]
fn a_screens_own_back_handler_goes_before_the_host() {
    thread_local! { static BACKS: Cell<u32> = const { Cell::new(0) }; }
    let mut ui = compose_test(|| {
        let nav_controller = remember_nav_controller(Screen::Home);
        NAV.set(Some(nav_controller));
        NavHost!(nav_controller, |screen: Screen| {
            if screen == Screen::Settings {
                BackHandler!(on_back = || BACKS.set(BACKS.get() + 1));
            }
            BasicText!("{screen:?}");
        });
    });
    navigate(&mut ui, Screen::Settings);
    assert!(back(&mut ui));
    assert_eq!(BACKS.get(), 1);
    assert_eq!(nav().current_destination(), Screen::Settings);
}

#[test]
fn the_back_stack_and_its_states_survive_the_process() {
    let mut ui = compose_test(App);
    ui.on_node_with_tag("home counter").perform_click();
    LIST.get().unwrap().request_scroll_to_item(42, 0);
    ui.wait_for_idle();
    navigate(&mut ui, Screen::Detail(5));
    ui.on_node_with_tag("detail counter").perform_click();
    ui.on_node_with_tag("detail counter").perform_click();
    navigate(&mut ui, Screen::Settings);
    let mut ui = recreate(ui);
    ui.on_node_with_text("Settings").assert_exists();
    assert_eq!(destinations(), [Screen::Home, Screen::Detail(5), Screen::Settings]);
    assert!(back(&mut ui));
    ui.on_node_with_text("Detail 5 clicks: 2, note 5").assert_exists();
    // Saved again by the new host, entry ids included: a new entry doesn't take an old id.
    let mut ui = recreate(ui);
    navigate(&mut ui, Screen::Detail(6));
    let ids: Vec<u64> = nav().current_back_stack().iter().map(|entry| entry.id).collect();
    let mut distinct = ids.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), ids.len(), "{ids:?}");
    assert!(back(&mut ui));
    assert!(back(&mut ui));
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    assert_eq!(LIST.get().unwrap().first_visible_item_index(), 42);
}

#[test]
fn with_a_fade_entries_keep_their_state_too() {
    FADE.set(true);
    let mut ui = compose_test(App);
    ui.on_node_with_tag("home counter").perform_click();
    navigate(&mut ui, Screen::Detail(1));
    // The fade is over: only the detail is shown.
    ui.on_node_with_text("Home clicks: 1").assert_does_not_exist();
    ui.on_node_with_text("Detail 1 clicks: 0, note 1").assert_exists();
    assert!(back(&mut ui));
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    ui.on_node_with_text("Detail 1 clicks: 0, note 1").assert_does_not_exist();
    assert!(!saves_text(&ui.host().save_state(), "note 1"));
}
