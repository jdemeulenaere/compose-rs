//! Tests from the adversarial review of D290 and D291: entry ids, transitions, stale
//! controllers, nesting.

use std::cell::Cell;
use std::time::Duration;

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

thread_local! {
    static NAV: Cell<Option<NavController<Screen>>> = const { Cell::new(None) };
    static FAIL_SETTINGS: Cell<bool> = const { Cell::new(false) };
    static FADE: Cell<bool> = const { Cell::new(false) };
    static ASYNC_COUNT: Cell<Option<MutableState<usize>>> = const { Cell::new(None) };
    static LIST: Cell<Option<LazyListState>> = const { Cell::new(None) };
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
            SavedValue::Int(-2) if !FAIL_SETTINGS.get() => Some(Screen::Settings),
            SavedValue::Int(-2) => None,
            SavedValue::Int(id) => u32::try_from(id).ok().map(Screen::Detail),
            _ => None,
        }
    }
}

fn nav() -> NavController<Screen> {
    NAV.get().unwrap()
}

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

#[composable]
fn Home() {
    let clicks = remember_saveable(|| mutable_state_of(0u32));
    Column!({
        Box!(
            modifier = Modifier
                .size(20.dp())
                .clickable(move || clicks.update(|count| count + 1))
                .test_tag("home counter"),
        );
        BasicText!("Home clicks: {}", clicks.get());
    });
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

fn recreate(mut ui: ComposeTestRule) -> ComposeTestRule {
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    ComposeTestRule::with_host(host, App)
}

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

/// A destination that no longer restores: the controller starts again at its start
/// destination with next_id 1, but the NavHost's holder still keeps the old entries' states
/// by id. A new entry takes an old id and gets another destination's state.
#[test]
fn failed_restore_new_entry_gets_stale_state() {
    let mut ui = compose_test(App);
    nav().navigate(Screen::Detail(1));
    ui.wait_for_idle();
    ui.on_node_with_tag("detail counter").perform_click();
    ui.on_node_with_tag("detail counter").perform_click();
    nav().navigate(Screen::Settings);
    ui.wait_for_idle();
    FAIL_SETTINGS.set(true);
    let mut ui = recreate(ui);
    FAIL_SETTINGS.set(false);
    assert_eq!(nav().current_destination(), Screen::Home);
    // The old Detail(1) state is still kept under id 1, never removed.
    let leaked = saves_text(&ui.host().save_state(), "note 1");
    nav().navigate(Screen::Detail(9));
    ui.wait_for_idle();
    let shown = ui.count_nodes(&has_text_containing("Detail 9"));
    let fresh = ui.count_nodes(&has_text("Detail 9 clicks: 0, note 9"));
    let stale = ui.count_nodes(&has_text("Detail 9 clicks: 2, note 1"));
    assert!(
        !leaked && fresh == 1,
        "leaked old state after failed restore: {leaked}; shown {shown} fresh: {fresh}; stale: {stale}"
    );
}

#[test]
fn navigate_twice_in_one_frame() {
    let mut ui = compose_test(App);
    nav().navigate(Screen::Detail(1));
    nav().navigate(Screen::Detail(2));
    ui.wait_for_idle();
    let stack = nav().current_back_stack();
    let destinations: Vec<_> = stack.iter().map(|entry| entry.destination).collect();
    assert_eq!(destinations, [Screen::Home, Screen::Detail(1), Screen::Detail(2)]);
    assert!(stack[0].id != stack[1].id && stack[1].id != stack[2].id && stack[0].id != stack[2].id);
    ui.on_node_with_text("Detail 2 clicks: 0, note 2").assert_exists();
    assert!(nav().pop_back_stack());
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1 clicks: 0, note 1").assert_exists();
}

/// Back while the fade to Detail(1) runs: Home's state comes back.
#[test]
fn back_during_transition_keeps_state() {
    FADE.set(true);
    let mut ui = ComposeTestRule::with_manual_clock(App);
    ui.on_node_with_tag("home counter").perform_click();
    // The frame that recomposes for it: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    nav().navigate(Screen::Detail(1));
    ui.advance_time_by(Duration::from_millis(100));
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(Duration::from_millis(50));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    assert!(!saves_text(&ui.host().save_state(), "note 1"));
    // Again, and a back mid-way then forward then back.
    nav().navigate(Screen::Detail(2));
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(100));
    nav().pop_back_stack();
    ui.advance_time_by(Duration::from_millis(16));
    nav().navigate(Screen::Detail(3));
    ui.advance_time_by(Duration::from_millis(16));
    nav().pop_back_stack();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    let saved = ui.host().save_state();
    assert!(!saves_text(&saved, "note 2"));
    assert!(!saves_text(&saved, "note 3"));
}

/// Reads on a controller whose owner went: D240's revision says holders' reads degrade.
#[test]
fn stale_controller_reads() {
    thread_local! { static SHOW: Cell<Option<MutableState<bool>>> = const { Cell::new(None) }; }
    let mut ui = compose_test(move || {
        let show = remember(|| mutable_state_of(true));
        SHOW.set(Some(show));
        if show.get() {
            let nav_controller = remember_nav_controller(Screen::Home);
            if NAV.get().is_none() {
                NAV.set(Some(nav_controller));
            }
        }
    });
    SHOW.get().unwrap().set(false);
    ui.wait_for_idle();
    SHOW.get().unwrap().set(true);
    ui.wait_for_idle();
    SHOW.get().unwrap().set(false);
    ui.wait_for_idle();
    nav().navigate(Screen::Settings);
    assert!(!nav().pop_back_stack());
    let result = std::panic::catch_unwind(|| nav().current_back_stack().len());
    assert!(result.is_ok(), "current_back_stack on a stale controller panics");
}

/// Async content: the list's items arrive a frame after the entry comes back.
#[composable]
fn AsyncList() {
    let count = remember(|| mutable_state_of(0usize));
    ASYNC_COUNT.set(Some(count));
    LaunchedEffect((), async move {
        count.set(100);
    });
    let list = remember_lazy_list_state!();
    LIST.set(Some(list));
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 200.dp())),
        state = list,
        move |scope: LazyListScope| {
            scope.items(count.get(), |index| {
                Box!(modifier = Modifier.size((100.dp(), 50.dp())), {
                    BasicText!("item {index}");
                });
            });
        }
    );
}

#[test]
fn scroll_restore_with_async_items() {
    let mut ui = compose_test(|| {
        let nav_controller = remember_nav_controller(Screen::Home);
        NAV.set(Some(nav_controller));
        NavHost!(nav_controller, |screen: Screen| match screen {
            Screen::Home => AsyncList(),
            _ => BasicText!("other"),
        });
    });
    ui.on_node_with_text("item 0").assert_exists();
    LIST.get().unwrap().request_scroll_to_item(40, 0);
    ui.wait_for_idle();
    nav().navigate(Screen::Settings);
    ui.wait_for_idle();
    nav().pop_back_stack();
    ui.wait_for_idle();
    assert_eq!(LIST.get().unwrap().first_visible_item_index(), 40, "scroll lost with async items");
}

/// A NavHost given a new controller (a logout: `remember_keyed(user, …)`): the new start entry
/// has id 0 as the old one did, so it keeps the old entry's state.
#[test]
fn new_controller_in_same_host_reuses_state() {
    thread_local! { static USER: Cell<Option<MutableState<u32>>> = const { Cell::new(None) }; }
    #[derive(Clone, Copy, PartialEq)]
    struct User(u32);
    impl ComposeArg for User {
        fn same(&self, other: &Self) -> bool {
            self == other
        }
    }
    let mut ui = compose_test(|| {
        let user = remember(|| mutable_state_of(1u32));
        USER.set(Some(user));
        let owned = remember_keyed(User(user.get()), || NavController::new(Screen::Detail(1)));
        let nav_controller = owned;
        NAV.set(Some(nav_controller));
        NavHost!(nav_controller, |screen: Screen| match screen {
            Screen::Detail(id) => Detail(id),
            _ => BasicText!("other"),
        });
    });
    ui.on_node_with_tag("detail counter").perform_click();
    ui.on_node_with_tag("detail counter").perform_click();
    ui.on_node_with_text("Detail 1 clicks: 2, note 1").assert_exists();
    USER.get().unwrap().set(2);
    ui.wait_for_idle();
    let fresh = ui.count_nodes(&has_text("Detail 1 clicks: 0, note 1"));
    assert_eq!(
        fresh,
        1,
        "a new controller's start entry shows the old one's state:\n{}",
        ui.dump_semantics()
    );
}

/// An exiting entry's BackHandler during the exit transition.
#[test]
fn exiting_entry_back_handler_takes_back() {
    thread_local! { static BACKS: Cell<u32> = const { Cell::new(0) }; }
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        let nav_controller = remember_nav_controller(Screen::Home);
        NAV.set(Some(nav_controller));
        NavHost!(
            nav_controller,
            enter_transition = fade_in(),
            exit_transition = fade_out(),
            content = |screen: Screen| {
                if screen == Screen::Settings {
                    BackHandler!(on_back = || BACKS.set(BACKS.get() + 1));
                }
                BasicText!("{screen:?}");
            }
        );
    });
    ui.set_auto_advance(true);
    nav().navigate(Screen::Detail(1));
    ui.wait_for_idle();
    nav().navigate(Screen::Settings);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    // The app bar's arrow pops Settings; back pressed while it fades out.
    nav().pop_back_stack();
    ui.advance_time_by(Duration::from_millis(100));
    assert!(ui.host().on_back_pressed());
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(
        (BACKS.get(), nav().current_destination()),
        (0, Screen::Home),
        "back went to the exiting Settings' handler"
    );
}

/// Navigating while composing is a state write during composition, allowed as in Compose
/// (D703): the host shows the new destination at the next pass.
#[test]
fn navigate_in_composition() {
    let mut ui = compose_test(|| {
        let nav_controller = remember_nav_controller(Screen::Home);
        NAV.set(Some(nav_controller));
        NavHost!(nav_controller, move |screen: Screen| match screen {
            Screen::Home => {
                // A redirect written in composition.
                nav_controller.navigate(Screen::Settings);
            }
            _ => BasicText!("other"),
        });
    });
    ui.wait_for_idle();
    assert_eq!(nav().current_destination(), Screen::Settings);
    ui.on_node_with_text("other").assert_exists();
}

/// A redirect goes in an effect.
#[test]
fn a_redirect_in_a_launched_effect() {
    let mut ui = compose_test(|| {
        let nav_controller = remember_nav_controller(Screen::Home);
        NAV.set(Some(nav_controller));
        NavHost!(nav_controller, move |screen: Screen| match screen {
            Screen::Home => {
                LaunchedEffect((), async move {
                    nav_controller.navigate_with(
                        Screen::Settings,
                        NavOptions {
                            pop_up_to: Some(Screen::Home),
                            pop_up_to_inclusive: true,
                            ..NavOptions::default()
                        },
                    );
                });
            }
            _ => BasicText!("other"),
        });
    });
    ui.on_node_with_text("other").assert_exists();
    assert_eq!(nav().current_back_stack().len(), 1);
    assert_eq!(nav().current_destination(), Screen::Settings);
}

thread_local! {
    static INNER:std::cell::RefCell<Vec<NavController<Screen>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A NavHost inside each item of a lazy list inside an outer NavHost's entry.
#[composable]
fn Nested() {
    let list = remember_lazy_list_state!();
    LIST.set(Some(list));
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 200.dp())),
        state = list,
        |scope: LazyListScope| {
            scope.items(100, |index| {
                let inner = remember_nav_controller(Screen::Home);
                INNER.with(|v| {
                    let mut v = v.borrow_mut();
                    if v.len() <= index {
                        v.resize(index + 1, inner);
                    }
                    v[index] = inner;
                });
                Box!(modifier = Modifier.size((100.dp(), 50.dp())), {
                    NavHost!(inner, move |screen: Screen| match screen {
                        Screen::Detail(id) => Detail(id + 1000 * index as u32),
                        other => BasicText!("inner {index} {other:?}"),
                    });
                });
            });
        }
    );
}

#[test]
fn nested_nav_in_lazy_in_nav() {
    let mut ui = compose_test(|| {
        let nav_controller = remember_nav_controller(Screen::Home);
        NAV.set(Some(nav_controller));
        NavHost!(nav_controller, |screen: Screen| match screen {
            Screen::Home => Nested(),
            _ => BasicText!("outer other"),
        });
    });
    INNER.with(|v| v.borrow()[1]).navigate(Screen::Detail(7));
    ui.wait_for_idle();
    ui.on_node_with_tag("detail counter").perform_click();
    ui.on_node_with_text("Detail 1007 clicks: 1, note 1007").assert_exists();
    LIST.get().unwrap().request_scroll_to_item(60, 0);
    ui.wait_for_idle();
    LIST.get().unwrap().request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1007 clicks: 1, note 1007").assert_exists();
    nav().navigate(Screen::Settings);
    ui.wait_for_idle();
    nav().pop_back_stack();
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1007 clicks: 1, note 1007").assert_exists();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_text("inner 1 Home").assert_exists();
}
