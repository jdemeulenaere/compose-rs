//! Regression tests from the second review of phase B (`#[composable]`). Each test failed at
//! `3733426`, fixed since; its comment describes the defect.

#![allow(non_snake_case)]
#![allow(unused_macros, clippy::let_unit_value)] // deliberate: what the review tested

use std::cell::{Cell, RefCell};
use std::fmt;

use compose::runtime::platform::*;
use compose::runtime::*;

// ---- Test support (as in review_macro.rs) --------------------------------------------------

#[derive(Clone, PartialEq)]
struct Label(String);

impl fmt::Debug for Label {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type Tree = NodeTree<Label>;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SERIAL: Cell<u32> = const { Cell::new(0) };
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn next_serial() -> u32 {
    SERIAL.with(|serial| serial.replace(serial.get() + 1))
}

fn new_composition() -> Composition {
    take_log();
    SERIAL.with(|serial| serial.set(0));
    Composition::new(Tree::new())
}

fn tree(composition: &Composition) -> String {
    composition.with_applier(|tree: &mut Tree| tree.dump())
}

fn Leaf(label: &str) {
    let composer = current();
    let node = composer.start_node(key_of("review2::Leaf"), || Label(label.to_string()));
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    composer.end_group();
}

/// A row with its own remembered state: `id:t<serial>`.
#[composable]
fn Row(id: u32) {
    let serial = remember(next_serial);
    Leaf(&format!("{id}:t{serial}"));
}

/// Runs `content` with `list`, then with `next`, and returns the second tree.
fn run_list(content: fn(MutableState<Vec<u32>>), list: Vec<u32>, next: Vec<u32>) -> String {
    let composition = new_composition();
    let state = mutable_state_of(list);
    let handle = state.handle();
    composition.set_content(move || content(handle));
    state.set(next);
    composition.recompose();
    tree(&composition)
}

/// Runs `content` with `flag` true, then false, and returns the second tree.
fn run_flag(content: fn(MutableState<bool>)) -> String {
    let composition = new_composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || content(handle));
    flag.set(false);
    composition.recompose();
    tree(&composition)
}

// ---- 1. `key` written as `let _ = key(…)` in a loop with a `continue` ----------------------

/// The fix for the first review's finding 2 recognises only an expression statement
/// `key(…);`. `key` returns its content's value, so `let height = key(id, || …);` (or
/// `let _ = key(…)`, or `_ = key(…)`) is just as natural, and in a loop with a `continue` that
/// `let` gets a per-statement region group: every item's movable group sits alone in its
/// iteration's region (matched by position), so keyed identity is lost again and reordering the
/// list recreates every row's state.
///
/// Expected: rows move with their state: "3:t2\n1:t0\n2:t1\n".
/// Actual: "3:t3\n1:t4\n2:t5\n" (every row recreated).
#[test]
fn a_key_bound_by_let_keeps_keyed_identity_in_a_loop_with_continue() {
    #[composable]
    fn Column(list: MutableState<Vec<u32>>) {
        for id in list.get() {
            if id == 0 {
                continue;
            }
            let _ = key(id, || Row(id));
        }
    }
    assert_eq!(run_list(Column, vec![1, 2, 3], vec![3, 1, 2]), "3:t2\n1:t0\n2:t1\n");
}

// ---- 2. Attributes of statements wrapped in a loop with a `continue` -----------------------

macro_rules! emit {
    ($e:expr) => {
        $e
    };
}

/// In a loop with a `continue`, `wrap_statements` rewrites a macro statement as
/// `{ region; mac!(…); }` from `mac.mac` alone: the statement's attributes (`mac.attrs`) are
/// dropped. A `#[cfg(…)]`-ed-out macro statement is then compiled in and runs: code the user
/// configured away shows up in the UI.
///
/// Expected: "2:t0\n" (the `#[cfg(any())]` statement doesn't exist).
/// Actual: "99:t0\n2:t1\n".
#[test]
fn a_cfg_ed_out_macro_statement_stays_out_in_a_loop_with_continue() {
    #[composable]
    fn List(list: MutableState<Vec<u32>>) {
        for id in list.get() {
            if id == 0 {
                continue;
            }
            #[cfg(any())]
            emit!(Row(99));
            Row(id);
        }
    }
    let composition = new_composition();
    let state = mutable_state_of(vec![2u32]);
    let handle = state.handle();
    composition.set_content(move || List(handle));
    assert_eq!(tree(&composition), "2:t0\n");
}

// ---- 3. Composable calls in closures that run during composition ----------------------------

/// docs/03: "Missing a region group is safe: groups carry their callee's key and slots carry
/// their type, so a mismatch removes and recreates state rather than misreading it." That holds
/// only when the missed call and the call that takes its place call different composables. When
/// they call the same one, nothing mismatches: the later call silently takes the state of the
/// earlier call site.
///
/// The common case: a closure that runs during composition and conditionally or repeatedly,
/// `Option::map`, `bool::then`, `unwrap_or_else`, `Iterator::for_each`. The macro walks into the
/// closure (it adds regions for `if`s inside it) but gives the closure body itself no group, so
/// its calls share positional groups with the calls after it.
///
/// Expected: "2:t1\n" (the header row is gone, the second call site keeps its own state).
/// Actual: "2:t0\n" (the second call site took the header's state).
#[test]
fn a_conditional_call_in_a_closure_does_not_lend_its_state_to_the_next_call() {
    #[composable]
    fn Screen(show_header: MutableState<bool>) {
        show_header.get().then(|| Row(1));
        Row(2);
    }
    let composition = new_composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || Screen(handle));
    assert_eq!(tree(&composition), "1:t0\n2:t1\n");
    flag.set(false);
    composition.recompose();
    assert_eq!(tree(&composition), "2:t1\n");
}

/// The iterator form of a loop gets no loop region: removing an item makes the call after the
/// loop take the removed item's state.
///
/// Expected (as with a `for` loop, which gets a region): "1:t0\n99:t2\n".
/// Actual: "1:t0\n99:t1\n" (the footer took row 2's state).
#[test]
fn a_for_each_loop_does_not_lend_its_state_to_the_next_call() {
    #[composable]
    fn List(list: MutableState<Vec<u32>>) {
        list.get().into_iter().for_each(|id| Row(id));
        Row(99);
    }
    assert_eq!(run_list(List, vec![1, 2], vec![1]), "1:t0\n99:t2\n");
}

/// A composable called through a local closure (or a generic `F: Fn()` parameter, or a `fn()`
/// pointer: only parameters whose type syntactically names `Fn`/`FnMut`/`FnOnce` count) in a
/// branch: the branch gets no region, and the call after the branch takes its state.
///
/// Expected: "2:t1\n". Actual: "2:t0\n".
#[test]
fn a_call_through_a_local_closure_in_a_branch_gets_a_region() {
    #[composable]
    fn Screen(show_header: MutableState<bool>) {
        let header = || Row(1);
        if show_header.get() {
            header();
        }
        Row(2);
    }
    assert_eq!(run_flag(Screen), "2:t1\n");
}

#[composable(non_restartable)]
fn Card<F: Fn()>(show: bool, content: F) {
    if show {
        content();
    }
    Row(2);
}

/// As above, with a generic content parameter: `content: F` where `F: Fn()` isn't recognised
/// as a callable, so `if show { content() }` gets no region.
///
/// Expected: "2:t1\n". Actual: "2:t0\n".
#[test]
fn a_call_of_a_generic_fn_parameter_in_a_branch_gets_a_region() {
    #[composable]
    fn Screen(show: MutableState<bool>) {
        Card(show.get(), || Row(1));
    }
    assert_eq!(run_flag(Screen), "2:t1\n");
}

// ---- 4. The restart function names the composable unqualified ------------------------------

mod material {
    use super::*;
    #[composable]
    pub fn Title(text: String) {
        Leaf(&format!("material {text}"));
    }
}

/// The restart closure calls `Title(…)` by its bare name from inside the body, so an item or
/// `use` in the body with the same name (a wrapper importing the composable it wraps) shadows
/// it: restarting the outer `Title` (its state changed) runs `material::Title` with the kept
/// arguments instead. The wrapper's own work (here, reading the state and upper-casing) is lost
/// and the UI changes.
///
/// Expected after the restart: "material HELLO!\n". Actual: "material hello\n".
#[test]
fn a_restart_runs_the_composable_itself_not_a_shadowing_item() {
    thread_local! {
        static BANG: Cell<Option<MutableState<bool>>> = const { Cell::new(None) };
    }
    #[composable]
    fn Title(text: String) {
        use material::Title;
        let bang = remember(|| mutable_state_of(false));
        BANG.with(|cell| cell.set(Some(bang)));
        let suffix = if bang.get() { "!" } else { "" };
        Title(format!("{}{suffix}", text.to_uppercase()));
    }
    let composition = new_composition();
    composition.set_content(|| Title("hello".to_string()));
    assert_eq!(tree(&composition), "material HELLO\n");
    BANG.with(|cell| cell.get()).expect("composed").set(true);
    composition.recompose();
    assert_eq!(tree(&composition), "material HELLO!\n");
}

// ---- 5. Same-named composables still share remembered state -------------------------------

mod light {
    use super::*;
    #[composable]
    pub fn Badge(count: u32) {
        let serial = remember(next_serial);
        Leaf(&format!("light {count}:t{serial}"));
    }
}

mod dark {
    use super::*;
    #[composable]
    pub fn Badge(count: u32) {
        let serial = remember(next_serial);
        Leaf(&format!("dark {count}:t{serial}"));
    }
}

/// The first review's fix stops `dark::Badge` from *skipping* on `light::Badge`'s group (same
/// key: crate + file + name), but `dark::Badge` still runs *inside* that group and reads its
/// remembered state, which belongs to another function: state attached to the wrong call.
/// docs/03 accepts this "as in Compose, where a generic composable is one function", but these
/// are two unrelated functions, which Compose keys apart. The runtime already knows (the restart
/// functions differ): it could treat the group as not matching and insert a new one.
///
/// Expected: "dark 1:t1\n" (dark::Badge's own state). Actual: "dark 1:t0\n".
#[test]
fn same_named_composables_do_not_share_remembered_state() {
    #[composable]
    fn Toolbar(dark_theme: MutableState<bool>) {
        let badge: fn(u32) = if dark_theme.get() { dark::Badge } else { light::Badge };
        badge(1);
    }
    let composition = new_composition();
    let dark_theme = mutable_state_of(false);
    let handle = dark_theme.handle();
    composition.set_content(move || Toolbar(handle));
    assert_eq!(tree(&composition), "light 1:t0\n");
    dark_theme.set(true);
    composition.recompose();
    assert_eq!(tree(&composition), "dark 1:t1\n");
}

// ---- 6. A PascalCase macro is trusted not to expand to a `continue` ------------------------

/// The reference: a loop with a plain `continue` gets one group per call site.
#[composable]
fn WithContinue(skip: MutableState<u32>) {
    for i in 0..3 {
        Row(10 + i);
        if i == skip.get() {
            continue;
        }
        Row(20 + i);
    }
}

macro_rules! SkipIf {
    ($condition:expr) => {
        if $condition {
            continue;
        }
    };
}

/// D52 trusts "the standard macros and composables' call macros" not to expand to a
/// `continue`, and recognises call macros by a PascalCase name alone. A user macro with a
/// PascalCase name that expands to a `continue` (an unlabelled `continue` isn't hygienic) then
/// disables the per-statement groups, and state jumps between call sites, as in the first
/// review's finding with a snake_case macro. (Phase C's call macros are generated by
/// `#[composable]`; the macro could trust only names it generates, or only `Name!` where `Name`
/// is also called as a function.)
///
/// Expected: the same tree as `WithContinue`. Actual: a row of the second call site takes the
/// state of a row of the first one.
#[test]
fn a_pascal_case_macro_in_a_loop_is_not_trusted_to_have_no_continue() {
    #[composable]
    fn WithPascalMacro(skip: MutableState<u32>) {
        for i in 0..3 {
            Row(10 + i);
            SkipIf!(i == skip.get());
            Row(20 + i);
        }
    }
    let run = |content: fn(MutableState<u32>)| {
        let composition = new_composition();
        let skip = mutable_state_of(1);
        let handle = skip.handle();
        composition.set_content(move || content(handle));
        skip.set(2);
        composition.recompose();
        tree(&composition)
    };
    let reference = run(WithContinue);
    assert_eq!(run(WithPascalMacro), reference);
}
