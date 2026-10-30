//! What a stack and its entries keep, in a display: the scenarios of `NavController` and `NavHost`'s
//! tests (D291), which Navigation 3's own don't have, with a failed restore's (D1407).

use std::cell::{Cell, RefCell};
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_navigation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::UiHost;
use compose_ui::*;

#[derive(Clone, Copy, PartialEq, Debug, Saveable)]
enum Screen {
    Home,
    Detail(u32),
    Settings,
}

thread_local! {
    static STACK: Cell<Option<NavBackStack<Screen>>> = const { Cell::new(None) };
    static LIST: Cell<Option<LazyListState>> = const { Cell::new(None) };
    static INNER: RefCell<Vec<NavBackStack<Screen>>> = const { RefCell::new(Vec::new()) };
}

fn stack() -> NavBackStack<Screen> {
    STACK.get().unwrap()
}

fn keys() -> Vec<Screen> {
    stack().iter_cloned().collect()
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

fn screen_entry(screen: Screen) -> NavEntry<Screen> {
    match screen {
        Screen::Home => NavEntry::new(screen, |_| Home()),
        Screen::Detail(id) => NavEntry::new(screen, move |_| Detail(id)),
        Screen::Settings => NavEntry::new(screen, |_| {
            BasicText!("Settings");
        }),
    }
}

#[composable]
fn App() {
    let back_stack = remember_nav_back_stack([Screen::Home]);
    STACK.set(Some(back_stack));
    NavDisplay!(back_stack, entry_provider = screen_entry);
}

fn navigate(ui: &mut ComposeTestRule, screen: Screen) {
    stack().push(screen);
    ui.wait_for_idle();
}

fn back(ui: &mut ComposeTestRule) -> bool {
    let handled = ui.host().on_back_pressed();
    ui.wait_for_idle();
    handled
}

/// The host is destroyed (the process's death), and a new one composes the app with what the first
/// saved, through bytes.
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

/// `the_back_stack_and_its_states_survive_the_process`, of `NavHost`'s tests.
#[test]
fn a_stack_and_what_its_entries_saved_survive_the_process() {
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
    assert_eq!(keys(), [Screen::Home, Screen::Detail(5), Screen::Settings]);
    assert!(back(&mut ui));
    ui.on_node_with_text("Detail 5 clicks: 2, note 5").assert_exists();
    // Saved again by the new host.
    let mut ui = recreate(ui);
    navigate(&mut ui, Screen::Detail(6));
    assert!(back(&mut ui));
    assert!(back(&mut ui));
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    assert_eq!(LIST.get().unwrap().first_visible_item_index(), 42);
}

/// `an_entry_keeps_its_state_while_on_the_stack`.
#[test]
fn an_entry_keeps_what_it_saved_while_on_the_stack() {
    let mut ui = compose_test(App);
    ui.on_node_with_tag("home counter").perform_click();
    ui.on_node_with_tag("home counter").perform_click();
    LIST.get().unwrap().request_scroll_to_item(30, 10);
    ui.wait_for_idle();
    navigate(&mut ui, Screen::Detail(1));
    ui.on_node_with_tag("detail counter").perform_click();
    navigate(&mut ui, Screen::Settings);
    assert!(back(&mut ui));
    ui.on_node_with_text("Detail 1 clicks: 1, note 1").assert_exists();
    assert!(back(&mut ui));
    ui.on_node_with_text("Home clicks: 2").assert_exists();
    let list = LIST.get().unwrap();
    assert_eq!(list.first_visible_item_index(), 30);
    assert_eq!(list.first_visible_item_scroll_offset(), 10);
    ui.on_node_with_text("item 30").assert_exists();
}

/// `a_popped_entry_leaves_no_state_behind`.
#[test]
fn a_popped_entry_saves_nothing_more() {
    let mut ui = compose_test(App);
    navigate(&mut ui, Screen::Detail(7));
    ui.on_node_with_tag("detail counter").perform_click();
    navigate(&mut ui, Screen::Settings);
    assert!(saves_text(&ui.host().save_state(), "note 7"), "kept while under the top");
    assert!(back(&mut ui));
    assert!(back(&mut ui));
    assert!(!saves_text(&ui.host().save_state(), "note 7"), "gone once popped");
    navigate(&mut ui, Screen::Detail(7));
    ui.on_node_with_text("Detail 7 clicks: 0, note 7").assert_exists();
}

/// `back_pops_and_leaves_the_app_at_the_start_destination`: what the Android host's `Frame.BACK`
/// follows.
#[test]
fn back_is_wanted_only_with_something_to_pop() {
    let mut ui = compose_test(App);
    ui.wait_for_idle();
    assert!(!ui.host().has_enabled_back_handlers());
    assert!(!back(&mut ui));
    navigate(&mut ui, Screen::Detail(1));
    assert!(ui.host().has_enabled_back_handlers());
    assert!(back(&mut ui));
    ui.on_node_with_text("Home clicks: 0").assert_exists();
    assert!(!ui.host().has_enabled_back_handlers());
}

/// `navigate_twice_in_one_frame`.
#[test]
fn two_pushes_in_one_frame_make_two_entries() {
    let mut ui = compose_test(App);
    ui.wait_for_idle();
    stack().push(Screen::Detail(1));
    stack().push(Screen::Detail(2));
    ui.wait_for_idle();
    assert_eq!(keys(), [Screen::Home, Screen::Detail(1), Screen::Detail(2)]);
    ui.on_node_with_text("Detail 2 clicks: 0, note 2").assert_exists();
    assert!(stack().remove_last_or_none().is_some());
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1 clicks: 0, note 1").assert_exists();
}

/// `back_during_transition_keeps_state`, with no state left of what was popped.
#[test]
fn back_during_a_transition_keeps_what_s_under_it_and_nothing_of_what_left() {
    let mut ui = ComposeTestRule::with_manual_clock(App);
    ui.on_node_with_tag("home counter").perform_click();
    ui.advance_time_by_frame();
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    stack().push(Screen::Detail(1));
    ui.advance_time_by(Duration::from_millis(100));
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(Duration::from_millis(50));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    assert!(!saves_text(&ui.host().save_state(), "note 1"));
    // Again, and a back mid-way then forward then back.
    stack().push(Screen::Detail(2));
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(100));
    stack().remove_last_or_none();
    ui.advance_time_by(Duration::from_millis(16));
    stack().push(Screen::Detail(3));
    ui.advance_time_by(Duration::from_millis(16));
    stack().remove_last_or_none();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    let saved = ui.host().save_state();
    assert!(!saves_text(&saved, "note 2"));
    assert!(!saves_text(&saved, "note 3"));
}

/// Items arriving a frame after the entry comes back.
#[composable]
fn AsyncList() {
    let count = remember(|| mutable_state_of(0usize));
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

/// `scroll_restore_with_async_items`.
#[test]
fn a_list_s_place_comes_back_with_items_arriving_late() {
    let mut ui = compose_test(|| {
        let back_stack = remember_nav_back_stack([Screen::Home]);
        STACK.set(Some(back_stack));
        NavDisplay!(
            back_stack,
            entry_provider = |screen: Screen| {
                if screen == Screen::Home {
                    NavEntry::new(screen, |_| AsyncList())
                } else {
                    NavEntry::new(screen, |_| {
                        BasicText!("other");
                    })
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text("item 0").assert_exists();
    LIST.get().unwrap().request_scroll_to_item(40, 0);
    ui.wait_for_idle();
    navigate(&mut ui, Screen::Settings);
    stack().remove_last_or_none();
    ui.wait_for_idle();
    assert_eq!(LIST.get().unwrap().first_visible_item_index(), 40);
}

#[composable]
fn Redirecting() {
    let back_stack = stack();
    LaunchedEffect((), async move {
        back_stack.clear();
        back_stack.push(Screen::Settings);
    });
}

/// `a_redirect_in_a_launched_effect`, as the guide's `Redirect`.
#[test]
fn a_redirect_replaces_the_stack_from_an_effect() {
    let mut ui = compose_test(|| {
        let back_stack = remember_nav_back_stack([Screen::Home]);
        STACK.set(Some(back_stack));
        NavDisplay!(
            back_stack,
            entry_provider = |screen: Screen| {
                if screen == Screen::Home {
                    NavEntry::new(screen, |_| Redirecting())
                } else {
                    NavEntry::new(screen, |_| {
                        BasicText!("other");
                    })
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text("other").assert_exists();
    assert_eq!(keys(), [Screen::Settings]);
}

/// A display in each item of a list in an entry of a display.
#[composable]
fn Nested() {
    let list = remember_lazy_list_state!();
    LIST.set(Some(list));
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 200.dp())),
        state = list,
        |scope: LazyListScope| {
            scope.items(100, |index| {
                let inner = remember_nav_back_stack([Screen::Home]);
                INNER.with_borrow_mut(|stacks| {
                    if stacks.len() <= index {
                        stacks.resize(index + 1, inner);
                    }
                    stacks[index] = inner;
                });
                Box!(modifier = Modifier.size((100.dp(), 50.dp())), {
                    NavDisplay!(
                        inner,
                        entry_provider = move |screen: Screen| match screen {
                            Screen::Detail(id) => {
                                NavEntry::new(screen, move |_| Detail(id + 1000 * index as u32))
                            }
                            other => NavEntry::new(screen, move |_| {
                                BasicText!("inner {index} {other:?}");
                            }),
                        }
                    );
                });
            });
        }
    );
}

/// `nested_nav_in_lazy_in_nav`.
#[test]
fn a_display_in_a_list_keeps_its_stack_and_state_across_scrolling_away() {
    let mut ui = compose_test(|| {
        let back_stack = remember_nav_back_stack([Screen::Home]);
        STACK.set(Some(back_stack));
        NavDisplay!(
            back_stack,
            entry_provider = |screen: Screen| {
                if screen == Screen::Home {
                    NavEntry::new(screen, |_| Nested())
                } else {
                    NavEntry::new(screen, |_| {
                        BasicText!("outer other");
                    })
                }
            }
        );
    });
    ui.wait_for_idle();
    INNER.with_borrow(|stacks| stacks[1]).push(Screen::Detail(7));
    ui.wait_for_idle();
    ui.on_node_with_tag("detail counter").perform_click();
    ui.on_node_with_text("Detail 1007 clicks: 1, note 1007").assert_exists();
    LIST.get().unwrap().request_scroll_to_item(60, 0);
    ui.wait_for_idle();
    LIST.get().unwrap().request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1007 clicks: 1, note 1007").assert_exists();
    navigate(&mut ui, Screen::Settings);
    stack().remove_last_or_none();
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1007 clicks: 1, note 1007").assert_exists();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_text("inner 1 Home").assert_exists();
}

thread_local! {
    static FAIL_RESTORE: Cell<bool> = const { Cell::new(false) };
    static FRAGILE_STACK: Cell<Option<NavBackStack<Fragile>>> = const { Cell::new(None) };
}

/// A key that may fail to restore, as one of another version of the app.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Fragile {
    Home,
    Detail(u32),
    Bad,
}

impl Saveable for Fragile {
    fn save(&self) -> SavedValue {
        SavedValue::Int(match *self {
            Fragile::Home => -1,
            Fragile::Bad => -2,
            Fragile::Detail(id) => i64::from(id),
        })
    }

    fn restore(saved: &SavedValue) -> Option<Fragile> {
        match *saved {
            SavedValue::Int(-1) => Some(Fragile::Home),
            SavedValue::Int(-2) if !FAIL_RESTORE.get() => Some(Fragile::Bad),
            SavedValue::Int(id) if id >= 0 => Some(Fragile::Detail(id as u32)),
            _ => None,
        }
    }
}

#[composable]
fn FragileApp() {
    let back_stack = remember_nav_back_stack([Fragile::Home]);
    FRAGILE_STACK.set(Some(back_stack));
    NavDisplay!(
        back_stack,
        entry_provider = |key: Fragile| match key {
            Fragile::Detail(id) => NavEntry::new(key, move |_| Detail(id)),
            _ => NavEntry::new(key, |_| Home()),
        }
    );
}

fn fragile_recreated(mut ui: ComposeTestRule) -> ComposeTestRule {
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    ComposeTestRule::with_host(host, FragileApp)
}

/// `failed_restore_new_entry_gets_stale_state`: a stack that fails to restore starts again, and an
/// entry pushed again starts anew, not with what its key saved before.
#[test]
fn a_stack_that_fails_to_restore_leaves_no_state_of_what_it_held() {
    FAIL_RESTORE.set(false);
    let mut ui = compose_test(FragileApp);
    ui.wait_for_idle();
    let stack = || FRAGILE_STACK.get().unwrap();
    stack().push(Fragile::Detail(1));
    ui.wait_for_idle();
    ui.on_node_with_tag("detail counter").perform_click();
    ui.on_node_with_text("Detail 1 clicks: 1, note 1").assert_exists();
    stack().push(Fragile::Bad);
    ui.wait_for_idle();
    FAIL_RESTORE.set(true);
    let mut ui = fragile_recreated(ui);
    ui.wait_for_idle();
    FAIL_RESTORE.set(false);
    assert_eq!(stack().iter_cloned().collect::<Vec<_>>(), [Fragile::Home]);
    stack().push(Fragile::Detail(1));
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1 clicks: 0, note 1").assert_exists();
}

/// One that restores keeps what its entries saved, shown or not.
#[test]
fn a_stack_that_restores_keeps_the_state_of_what_s_under_the_top() {
    FAIL_RESTORE.set(false);
    let mut ui = compose_test(FragileApp);
    ui.wait_for_idle();
    let stack = || FRAGILE_STACK.get().unwrap();
    stack().push(Fragile::Detail(1));
    ui.wait_for_idle();
    ui.on_node_with_tag("detail counter").perform_click();
    stack().push(Fragile::Bad);
    ui.wait_for_idle();
    let mut ui = fragile_recreated(ui);
    ui.wait_for_idle();
    stack().remove_last_or_none();
    ui.wait_for_idle();
    ui.on_node_with_text("Detail 1 clicks: 1, note 1").assert_exists();
}

/// One user's saved values aren't the next one's, under the same content key: the guide keys what
/// holds them by the user.
#[test]
fn another_user_s_entries_start_anew() {
    thread_local! { static USER: Cell<Option<MutableState<u32>>> = const { Cell::new(None) }; }
    let mut ui = compose_test(|| {
        let user = remember(|| mutable_state_of(1u32));
        USER.set(Some(user));
        key(user.get(), || {
            let back_stack = remember_nav_back_stack([Screen::Home]);
            NavDisplay!(
                back_stack,
                entry_provider = |screen: Screen| { NavEntry::new(screen, |_| Home!()) }
            );
        });
    });
    ui.on_node_with_tag("home counter").perform_click();
    ui.on_node_with_text("Home clicks: 1").assert_exists();
    USER.get().unwrap().set(2);
    ui.wait_for_idle();
    ui.on_node_with_text("Home clicks: 0").assert_exists();
}
