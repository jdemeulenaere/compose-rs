//! Regression tests from the adversarial review of phase C (call macros, lambda memoization),
//! each a concrete input where a memoized lambda kept a stale closure, state was lost, or the
//! call macro bound to the wrong function.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};

use compose::runtime::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static LAST: RefCell<Option<Callback<dyn Fn() -> String>>> = const { RefCell::new(None) };
    static COUNTER: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn last() -> String {
    LAST.with(|last| last.borrow().as_ref().map(|show| show()).unwrap_or_default())
}

#[composable]
fn Child(label: &'static str, show: impl Fn() -> String + 'static) {
    log(format!("{label}: {}", show()));
    LAST.with(|last| *last.borrow_mut() = Some(show.clone()));
}

fn composition() -> Composition {
    take_log();
    Composition::new(NodeTree::<()>::new())
}

// ---- Missed captures -------------------------------------------------------------------------

/// `{v:>width$}`: `width` is a local used only as a format-spec count argument. `format_names`
/// stops at the `:` and never sees it, so the lambda keeps its old width.
#[test]
fn a_format_width_argument_is_a_capture() {
    #[composable]
    fn Parent(width: MutableState<usize>) {
        let width = width.get();
        let v = 7;
        Child!("w", move || format!("[{v:>width$}]"));
    }
    let composition = composition();
    let width = mutable_state_of(2);
    let handle = width.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["w: [ 7]"]);
    width.set(4);
    composition.recompose();
    assert_eq!(take_log(), ["w: [   7]"], "width changed: the lambda must be replaced");
    assert_eq!(last(), "[   7]");
}

/// The same with an explicit positional argument and a precision count: `{:.prec$}`.
#[test]
fn a_format_precision_argument_is_a_capture() {
    #[composable]
    fn Parent(prec: MutableState<usize>) {
        let prec = prec.get();
        let x = 1.0f64 / 3.0;
        Child!("p", move || format!("{:.prec$}", x));
    }
    let composition = composition();
    let prec = mutable_state_of(1);
    let handle = prec.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["p: 0.3"]);
    prec.set(3);
    composition.recompose();
    assert_eq!(last(), "0.333", "precision changed: the lambda must be replaced");
}

/// A PascalCase local (allowed with `non_snake_case`) is taken for a constant by `bindings`,
/// so it is never a capture.
#[test]
fn a_pascal_case_local_is_a_capture() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let Limit = value.get();
        Child!("limit", move || format!("{}", Limit));
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "2", "Limit changed: the lambda must be replaced");
}

/// A local bound by a (hygiene-respecting) statement macro of the enclosing body: the walk
/// doesn't know the name, so a closure using it has no capture and isn't flagged unknown.
#[test]
fn a_local_bound_by_a_statement_macro_is_a_capture() {
    macro_rules! bind {
        ($name:ident = $value:expr) => {
            let $name = $value;
        };
    }
    #[composable]
    fn Parent(value: MutableState<u32>) {
        bind!(current = value.get());
        Child!("macro-bound", move || current.to_string());
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "2", "current changed: the lambda must be replaced");
}

// ---- Lambda slots outside region groups --------------------------------------------------------

/// Two `callback(…)`s in the branches of an `if` with the same capture types: the branches have
/// no region group (`callback` is not a likely composable call), so both probes use the same
/// slot, and the else branch is handed the then branch's closure.
#[test]
fn callbacks_in_if_branches_are_not_confused() {
    #[composable]
    fn Parent(flag: MutableState<bool>) {
        let id = 1u32;
        let show = if flag.get() {
            callback(move || format!("A{id}"))
        } else {
            callback(move || format!("B{id}"))
        };
        Child("branch", show);
    }
    let composition = composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["branch: A1"]);
    flag.set(false);
    composition.recompose();
    assert_eq!(last(), "B1", "the else branch's closure, not the then branch's");
}

/// The same through `match` arms, the natural way to pick a handler.
#[test]
fn callbacks_in_match_arms_are_not_confused() {
    #[composable]
    fn Parent(mode: MutableState<u8>) {
        let id = 1u32;
        let show = match mode.get() {
            0 => callback(move || format!("view {id}")),
            _ => callback(move || format!("edit {id}")),
        };
        Child("mode", show);
    }
    let composition = composition();
    let mode = mutable_state_of(0);
    let handle = mode.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    mode.set(1);
    composition.recompose();
    assert_eq!(last(), "edit 1");
}

/// Callbacks built in a loop take one slot each, outside any group: when the count changes, the
/// slots after them shift and `remember`ed state is lost.
#[test]
fn callbacks_built_in_a_loop_dont_shift_later_state() {
    #[composable]
    fn Parent(n: MutableState<u32>) {
        let mut handlers: Vec<Callback<dyn Fn() -> String>> = Vec::new();
        for i in 0..n.get() {
            handlers.push(callback(move || i.to_string()));
        }
        let counter = remember(|| mutable_state_of(0u32));
        COUNTER.with(|cell| cell.set(Some(counter)));
        log(format!("counter {} with {} handlers", counter.get(), handlers.len()));
    }
    let composition = composition();
    let n = mutable_state_of(2);
    let handle = n.handle();
    composition.set_content(move || Parent(handle));
    COUNTER.with(Cell::get).unwrap().set(42);
    composition.recompose();
    assert_eq!(take_log(), ["counter 0 with 2 handlers", "counter 42 with 2 handlers"]);
    n.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["counter 42 with 1 handlers"], "the counter's state is kept");
}

/// The most natural form of the above: an optional handler, `selected.map(|id| callback(…))`.
/// Selecting something adds a lambda slot, and the `remember` after it loses its state.
#[test]
fn an_optional_callback_doesnt_shift_later_state() {
    #[composable]
    fn Parent(selected: MutableState<Option<u32>>) {
        let on_delete: Option<Callback<dyn Fn() -> String>> =
            selected.get().map(|id| callback(move || format!("delete {id}")));
        let counter = remember(|| mutable_state_of(0u32));
        COUNTER.with(|cell| cell.set(Some(counter)));
        log(format!("counter {} {}", counter.get(), on_delete.is_some()));
    }
    let composition = composition();
    let selected = mutable_state_of(None);
    let handle = selected.handle();
    composition.set_content(move || Parent(handle));
    COUNTER.with(Cell::get).unwrap().set(42);
    composition.recompose();
    assert_eq!(take_log(), ["counter 0 false", "counter 42 false"]);
    selected.set(Some(7));
    composition.recompose();
    assert_eq!(take_log(), ["counter 42 true"], "the counter's state is kept");
}

/// `remember(|| callback(…))`, a natural way to make a stable handler: the macro rewrites the
/// `callback(…)` inside `remember`'s calculation into a lambda probe, which reserves and writes
/// a slot while `remember`'s own slot is still unwritten. Today the first pass panics
/// ("slots were written out of order: remember's calculation must not call remember or
/// composables"), though the user wrote neither. (Were the write allowed, the probe would take
/// a slot on the first pass only, shifting every later slot on the second.)
#[test]
fn a_callback_made_inside_remember_works() {
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        tick.get();
        let on_tap: Callback<dyn Fn()> = remember(|| callback(move || log("tap")));
        let counter = remember(|| mutable_state_of(0u32));
        COUNTER.with(|cell| cell.set(Some(counter)));
        log(format!("counter {}", counter.get()));
        let _ = on_tap;
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle));
    COUNTER.with(Cell::get).unwrap().set(42);
    composition.recompose();
    assert_eq!(take_log(), ["counter 0", "counter 42"], "the counter's state is kept");
    tick.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["counter 42"], "the counter's state is kept");
}

// ---- Call macro resolution ---------------------------------------------------------------------

mod library {
    #[compose::runtime::composable]
    pub fn Greeting() {
        super::log("library Greeting");
    }
}

/// `library::Greeting!()` expands to a bare `Greeting()` resolved at the call site: with another
/// `Greeting` in scope there, the path call silently runs that one.
#[test]
fn a_path_call_macro_calls_the_function_it_names() {
    #[composable]
    fn Greeting() {
        log("local Greeting");
    }
    #[composable]
    fn Screen() {
        library::Greeting!();
    }
    let composition = composition();
    composition.set_content(Screen);
    assert_eq!(take_log(), ["library Greeting"]);
}

// ---- Skipping with default callbacks -----------------------------------------------------------

thread_local! {
    static RUNS: Cell<u32> = const { Cell::new(0) };
}

fn noop() {}

/// A callback parameter left to its default: the call macro passes `to_callback(&(noop))`, a new
/// `Rc` every pass, so the argument is never `same` and the callee never skips (and every pass
/// allocates). Compose's `onClick: () -> Unit = {}` defaults are everywhere in Material.
#[test]
fn a_defaulted_callback_lets_its_callee_skip() {
    #[composable]
    fn Clickable(label: &'static str, #[default(noop)] on_click: impl Fn() + 'static) {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        let _ = (label, on_click);
    }
    #[composable]
    fn Screen(tick: MutableState<u32>) {
        tick.get();
        Clickable!("x");
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Screen(handle));
    RUNS.with(|runs| runs.set(0));
    for round in 1..4 {
        tick.set(round);
        composition.recompose();
    }
    assert_eq!(RUNS.with(Cell::get), 0, "same arguments every pass: the callee skips");
}
