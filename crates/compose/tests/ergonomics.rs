//! Phase C: call macros (positional, named, default, trailing and format arguments) and lambda
//! memoization on captures. A missed capture would keep a stale closure, so most tests check
//! that a lambda is replaced exactly when something it captures changes.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose::runtime::*;

// ---- Test support --------------------------------------------------------------------------

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static LAST: RefCell<Option<Callback<dyn Fn() -> String>>> = const { RefCell::new(None) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// A child that records what it receives, and runs (logging) only when not skipped.
#[composable]
fn Child(label: &'static str, show: impl Fn() -> String + 'static) {
    log(format!("{label}: {}", show()));
    LAST.with(|last| *last.borrow_mut() = Some(show.clone()));
}

fn last() -> String {
    LAST.with(|last| last.borrow().as_ref().map(|show| show()).unwrap_or_default())
}

fn composition() -> Composition {
    take_log();
    Composition::new(NodeTree::<()>::new())
}

// ---- Call macros ---------------------------------------------------------------------------

#[composable]
fn Labelled(
    #[format] text: String,
    #[default(1)] size: u32,
    #[default(false)] bold: bool,
    on_click: impl Fn() + 'static,
) {
    log(format!("Labelled({text}, {size}, {bold})"));
    on_click();
}

#[test]
fn call_macros_bind_positional_named_default_trailing_and_format_arguments() {
    let composition = composition();
    composition.set_content(|| {
        let name = "world";
        Labelled!("hello {name}", { log("clicked 1") });
        Labelled!("{} + {}", 1, 2, bold = true, { log("clicked 2") });
        Labelled!(String::from("plain"), 3, on_click = move || log("clicked 3"));
        Labelled!(
            text = "named".to_string(),
            size = 4,
            bold = true,
            on_click = || log("clicked 4")
        );
        Labelled("plain call".into(), 5, false, callback(|| log("clicked 5")));
    });
    assert_eq!(
        take_log(),
        [
            "Labelled(hello world, 1, false)",
            "clicked 1",
            "Labelled(1 + 2, 1, true)",
            "clicked 2",
            "Labelled(plain, 3, false)",
            "clicked 3",
            "Labelled(named, 4, true)",
            "clicked 4",
            "Labelled(plain call, 5, false)",
            "clicked 5",
        ]
    );
}

#[composable]
fn Scoped(#[default(0)] padding: u32, content: impl Fn(u32) + 'static) {
    content(padding);
}

#[test]
fn trailing_lambdas_take_the_content_parameters_arity() {
    let composition = composition();
    composition.set_content(|| {
        Scoped!({ log("ignored scope") });
        Scoped!(padding = 7, |padding| log(format!("padding {padding}")));
    });
    assert_eq!(take_log(), ["ignored scope", "padding 7"]);
}

// ---- Memoization -----------------------------------------------------------------------------

#[test]
fn a_lambda_with_unchanged_captures_lets_its_callee_skip() {
    #[composable]
    fn Parent(tick: MutableState<u32>, name: MutableState<&'static str>) {
        tick.get(); // the parent re-runs on every tick
        let name = name.get();
        Child!("child", move || name.to_string());
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let name = mutable_state_of("a");
    let (tick_handle, name_handle) = (tick.handle(), name.handle());
    composition.set_content(move || Parent(tick_handle, name_handle));
    assert_eq!(take_log(), ["child: a"]);
    tick.set(1);
    composition.recompose();
    assert!(take_log().is_empty(), "same captures: the same callback, the child skips");
    name.set("b");
    composition.recompose();
    assert_eq!(take_log(), ["child: b"], "a changed capture: a new callback");
}

#[test]
fn captures_are_found_through_shadowing_formats_nesting_and_patterns() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let value = value.get();
        // Inline format argument.
        Child!("format", move || format!("{value}"));
        // Shadowed inside the closure: the inner `value` is not a capture, the outer one is.
        Child!("shadow", move || {
            let doubled = value * 2;
            {
                let value = 100;
                let _ = value;
            }
            format!("{doubled}/{value}")
        });
        // Through a nested closure.
        Child!("nested", move || (0..1).map(|i| (value + i).to_string()).collect());
        // Bound by a pattern in the enclosing scope.
        let (first, _) = (value + 1, ());
        if let Some(second) = Some(value + 2) {
            Child!("patterns", move || format!("{first},{second}"));
        }
        // Bound by a match arm.
        match value % 2 {
            parity => Child!("match", move || format!("{parity}")),
        }
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    value.set(2);
    composition.recompose();
    let mut ran = take_log();
    ran.sort();
    assert_eq!(
        ran,
        ["format: 2", "match: 0", "nested: 2", "patterns: 3,4", "shadow: 4/2"],
        "every lambda saw its capture change"
    );
}

#[test]
fn a_lambda_calling_an_unknown_macro_is_never_kept() {
    macro_rules! value_of {
        ($state:expr) => {
            $state.get()
        };
    }
    #[composable]
    fn Parent(tick: MutableState<u32>, value: MutableState<u32>) {
        tick.get();
        Child!("unknown", move || value_of!(value).to_string());
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let value = mutable_state_of(5);
    let (tick_handle, value_handle) = (tick.handle(), value.handle());
    composition.set_content(move || Parent(tick_handle, value_handle));
    take_log();
    tick.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["unknown: 5"], "recreated: the macro might hide a capture");
}

#[test]
fn a_capture_that_is_not_compose_arg_makes_the_lambda_always_changed() {
    #[derive(Clone)]
    struct Opaque(u32);
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        tick.get();
        let opaque = Rc::new(RefCell::new(Opaque(7)));
        let shared = opaque.clone();
        let _ = shared;
        let plain = Opaque(3);
        Child!("opaque", move || plain.0.to_string());
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    tick.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["opaque: 3"]);
}

#[test]
fn explicit_callbacks_in_plain_code_are_memoized() {
    #[composable]
    fn Parent(tick: MutableState<u32>, value: MutableState<u32>) {
        tick.get();
        let value = value.get();
        Child("plain", callback(move || value.to_string()));
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let value = mutable_state_of(1);
    let (tick_handle, value_handle) = (tick.handle(), value.handle());
    composition.set_content(move || Parent(tick_handle, value_handle));
    take_log();
    tick.set(1);
    composition.recompose();
    assert!(take_log().is_empty());
    value.set(2);
    composition.recompose();
    assert_eq!(take_log(), ["plain: 2"]);
    assert_eq!(last(), "2");
}

#[test]
fn closures_passed_to_other_functions_keep_their_types() {
    #[composable(non_restartable)]
    fn Generic<F: Fn() -> u32 + 'static>(f: F) {
        log(format!("generic {}", f()));
    }
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let doubled: Vec<u32> = (0..2).map(|i| i * 2).collect();
        value.update(|v| *v);
        Generic(move || doubled.len() as u32);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| 1));
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        composition.set_content(move || Parent(handle))
    }));
    // `value.update` writes during composition: a debug-build misuse; the point here is that the
    // closures compile with their own types.
    let _ = result;
}

#[test]
fn a_skipped_composable_with_a_content_lambda_still_costs_one_comparison_per_argument() {
    thread_local! {
        static RUNS: Cell<u32> = const { Cell::new(0) };
    }
    #[composable]
    fn Card(title: &'static str, content: impl Fn() + 'static) {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        content();
    }
    #[composable]
    fn Screen(tick: MutableState<u32>) {
        tick.get();
        Card!("card", { log("content") });
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Screen(handle));
    take_log();
    for round in 1..5 {
        tick.set(round);
        composition.recompose();
    }
    assert!(take_log().is_empty(), "the card skips: its content lambda is the same callback");
    assert_eq!(RUNS.with(Cell::get), 1);
}

#[test]
fn branches_inside_content_blocks_get_region_groups() {
    thread_local! {
        static SERIAL: Cell<u32> = const { Cell::new(0) };
    }
    #[composable]
    fn Tagged(name: &'static str) {
        let serial = remember(|| SERIAL.with(|serial| serial.replace(serial.get() + 1)));
        log(format!("{name}{serial}"));
    }
    #[composable]
    fn Holder(content: impl Fn() + 'static) {
        content();
    }
    #[composable]
    fn Screen(flag: MutableState<bool>) {
        Holder!({
            if flag.get() {
                Tagged("a");
            } else {
                Tagged("b");
            }
        });
    }
    let composition = composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || Screen(handle));
    flag.set(false);
    composition.recompose();
    flag.set(true);
    composition.recompose();
    // Each branch has its own group, so its state: `b` doesn't reuse `a`'s, and `a` comes back
    // with a new one.
    assert_eq!(take_log(), ["a0", "b1", "a2"]);
}

#[test]
fn a_named_block_argument_is_a_content_lambda() {
    #[composable]
    fn Card(title: &'static str, content: impl Fn() + 'static) {
        log(format!("card {title}"));
        content();
    }
    let composition = composition();
    composition.set_content(|| {
        Card!(title = "named", content = { log("content") });
    });
    assert_eq!(take_log(), ["card named", "content"]);
}

#[test]
fn a_zero_sized_capture_keeps_its_lambda() {
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        tick.get();
        let format = |value: u32| value.to_string(); // captures nothing: zero-sized, not ComposeArg
        Child!("zero-sized", move || format(1));
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    tick.set(1);
    composition.recompose();
    assert!(take_log().is_empty(), "the capture can't differ: the child skips");
}

#[test]
fn optional_callbacks_take_a_lambda_or_none() {
    #[composable]
    fn Captioned(
        #[default(None)] label: Option<impl Fn() + 'static>,
        #[default(None)] on_change: Option<impl Fn(u32) + 'static>,
    ) {
        match &label {
            Some(label) => label(),
            None => log("no label"),
        }
        if let Some(on_change) = &on_change {
            on_change(7);
        }
    }
    let composition = composition();
    composition.set_content(|| {
        Captioned!();
        Captioned!(label = { log("label") }, on_change = |value| log(format!("changed {value}")));
        Captioned!(label = None, on_change = None);
    });
    assert_eq!(take_log(), ["no label", "label", "changed 7", "no label"]);
}
