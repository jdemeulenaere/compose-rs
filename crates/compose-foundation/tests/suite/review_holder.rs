//! Tests from the adversarial review of D290: what a lazy list saves, branches between
//! visits, a key provided twice, content moved in one pass.

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

fn count_entries(state: &SavedState) -> usize {
    fn count(value: &SavedValue) -> usize {
        match value {
            SavedValue::List(values) => 1 + values.iter().map(count).sum::<usize>(),
            _ => 1,
        }
    }
    state.values.values().flatten().map(count).sum()
}

thread_local! {
    static LIST: Cell<Option<LazyListState>> = const { Cell::new(None) };
    static SAVEABLE: Cell<bool> = const { Cell::new(true) };
    static FLAG: Cell<Option<MutableState<bool>>> = const { Cell::new(None) };
    static SHOWN: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
}

#[composable]
fn BigList() {
    let state = remember_lazy_list_state!();
    LIST.set(Some(state));
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 200.dp())),
        state = state,
        |scope: LazyListScope| {
            scope.items_keyed(
                100_000,
                |index| index as u32,
                |index| {
                    if SAVEABLE.get() {
                        let clicks = remember_saveable(|| mutable_state_of(0u32));
                        BasicText!("item {index}: {}", clicks.get());
                    } else {
                        BasicText!("item {index}");
                    }
                },
            );
        }
    );
}

fn scroll_through(ui: &mut ComposeTestRule, pixels: f32, steps: usize) {
    for _ in 0..steps {
        LIST.get().unwrap().scroll_by(pixels);
        ui.wait_for_idle();
    }
}

#[test]
fn growth_without_saveable_state() {
    SAVEABLE.set(false);
    let mut ui = compose_test(BigList);
    let before = count_entries(&ui.host().save_state());
    scroll_through(&mut ui, 300.0, 2000);
    let after = count_entries(&ui.host().save_state());
    eprintln!(
        "no saveable: before {before} after {after} first {}",
        LIST.get().unwrap().first_visible_item_index()
    );
    assert!(after <= before + 4, "{before} -> {after}");
}

#[test]
fn growth_with_saveable_state_and_prefetch() {
    SAVEABLE.set(true);
    let mut ui = compose_test(BigList);
    let before = count_entries(&ui.host().save_state());
    scroll_through(&mut ui, 300.0, 200);
    let first = LIST.get().unwrap().first_visible_item_index();
    let saved = ui.host().save_state();
    let after = count_entries(&saved);
    let bytes = saved.to_bytes().len();
    eprintln!("saveable: before {before} after {after} bytes {bytes} first {first}");
    // Each item scrolled past with saveable state is kept, and saved: a list of thousands of
    // such items grows the saved state (Android's `Bundle`s hold about 1 MB), as in Compose.
    assert!(first > 1000 && after > before + first, "{before} -> {after} past {first} items");
}

/// D223: an `if` before a remember_saveable, toggled between visits.
#[composable]
fn Branchy() {
    let flag = FLAG.get().unwrap().get();
    if flag {
        let a = remember_saveable(|| mutable_state_of(100u32));
        BasicText!("a {}", a.get());
    }
    let b = remember_saveable(|| mutable_state_of(0u32));
    Box!(
        modifier =
            Modifier.size(20.dp()).clickable(move || b.update(|count| count + 1)).test_tag("b"),
    );
    BasicText!("b {}", b.get());
}

#[test]
fn branch_before_saveable_between_visits() {
    let mut ui = compose_test(|| {
        let holder = remember_saveable_state_holder();
        let shown = remember(|| mutable_state_of(1u32));
        SHOWN.set(Some(shown));
        let flag = remember(|| mutable_state_of(false));
        FLAG.set(Some(flag));
        if shown.get() == 1 {
            holder.SaveableStateProvider(1u32, Branchy);
        } else {
            BasicText!("other");
        }
    });
    for _ in 0..5 {
        ui.on_node_with_tag("b").perform_click();
    }
    ui.on_node_with_text("b 5").assert_exists();
    SHOWN.get().unwrap().set(2);
    ui.wait_for_idle();
    FLAG.get().unwrap().set(true);
    SHOWN.get().unwrap().set(1);
    ui.wait_for_idle();
    let a = ui.count_nodes(&has_text("a 100"));
    let b = ui.count_nodes(&has_text("b 5"));
    let swapped = ui.count_nodes(&has_text("a 5"));
    assert!(a == 1 && b == 1, "a 100: {a}, b 5: {b}, a 5 (swapped): {swapped}");
}

#[composable]
fn Counter(tag: &'static str) {
    let clicks = remember_saveable(|| mutable_state_of(0u32));
    Box!(
        modifier = Modifier
            .size(20.dp())
            .clickable(move || clicks.update(|count| count + 1))
            .test_tag(tag),
    );
    BasicText!("{tag} {}", clicks.get());
}

/// The same key provided at two places at once: a programming error, as in Compose (which
/// throws); debug builds panic once the pass is applied.
#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "composed at two places at once"))]
fn same_key_twice_at_once() {
    let _ui = compose_test(|| {
        let holder = remember_saveable_state_holder();
        Column!({
            holder.SaveableStateProvider("k", || Counter("first"));
            holder.SaveableStateProvider("k", || Counter("second"));
        });
    });
}

/// Content that leaves one place and comes in at another in one pass starts without its state
/// (the new place composes before the old one saves), as in Compose; moved in two passes, it
/// finds it (`saveable_state_holder::content_coming_back_elsewhere_finds_its_state`).
#[test]
fn moved_between_parents_in_one_pass() {
    thread_local! { static PLACE: Cell<Option<MutableState<u8>>> = const { Cell::new(None) }; }
    let mut ui = compose_test(|| {
        let holder = remember_saveable_state_holder();
        let place = remember(|| mutable_state_of(0u8));
        PLACE.set(Some(place));
        match place.get() {
            0 => holder.SaveableStateProvider("screen", || Counter("moved")),
            _ => Box!({
                Column!({
                    holder.SaveableStateProvider("screen", || Counter("moved"));
                });
            }),
        }
    });
    ui.on_node_with_tag("moved").perform_click();
    ui.on_node_with_text("moved 1").assert_exists();
    PLACE.get().unwrap().set(2);
    ui.wait_for_idle();
    ui.on_node_with_text("moved 0").assert_exists();
    // No panic for the key in two places within the pass: the old one left first.
    ui.on_node_with_tag("moved").perform_click();
    PLACE.get().unwrap().set(0);
    ui.wait_for_idle();
    ui.on_node_with_text("moved 0").assert_exists();
}
