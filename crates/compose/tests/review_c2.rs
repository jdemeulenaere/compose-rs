//! Regression tests from the second adversarial review of phase C (after "Fix the phase C review
//! findings"): concrete inputs where a memoized lambda keeps a stale closure, two lambdas share
//! a slot, or state shifts.

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

/// A layout-like composable with content.
#[composable]
fn Holder(content: impl Fn() + 'static) {
    content();
}

fn composition() -> Composition {
    take_log();
    Composition::new(NodeTree::<()>::new())
}

// ---- Missed captures -------------------------------------------------------------------------

/// A local `macro_rules!` defined after a local can name it (locals resolve at the macro's
/// definition). Called directly in the closure it makes the lambda "unknown", but called inside
/// a known macro's tokens (`format!("{}", show!())`) it isn't seen: `scan_tokens` doesn't look
/// for `name!` invocations, and `count` isn't in the tokens. The lambda keeps its old count.
#[test]
fn an_unknown_macro_inside_a_known_macro_makes_the_lambda_changed() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let count = value.get();
        macro_rules! shown {
            () => {
                count
            };
        }
        Child!("nested", move || format!("{}", shown!()));
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["nested: 1"]);
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "2", "count changed: the lambda must be replaced");
}

/// The same through a content block: the inner call macro's tokens are scanned, not visited,
/// so the unknown macro inside them is missed and the whole content lambda is kept.
#[test]
fn an_unknown_macro_inside_a_nested_call_macro_makes_the_content_changed() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let count = value.get();
        macro_rules! shown {
            () => {
                count
            };
        }
        Holder!({
            Child!("content", move || "x".to_string());
            log(format!("content sees {}", shown!()));
        });
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["content: x", "content sees 1"]);
    value.set(2);
    composition.recompose();
    assert!(
        take_log().contains(&"content sees 2".to_string()),
        "the content lambda must be replaced"
    );
}

/// A local PascalCase `macro_rules!` is trusted as a composable's call macro ("declarative,
/// hence hygienic"), but a local one can name a local defined before it.
#[test]
fn a_local_pascal_case_macro_naming_a_local_makes_the_lambda_changed() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let count = value.get();
        macro_rules! Shown {
            () => {
                count
            };
        }
        Child!("pascal macro", move || Shown!().to_string());
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "2", "count changed: the lambda must be replaced");
}

/// A PascalCase binding in a refutable pattern (`if let Some(Count) = …`) is taken for a unit
/// variant, so it's never a capture. With no `Count` item in scope, Rust binds it (a
/// `non_snake_case` warning, allowed here, as in review_c's `let Limit`).
#[test]
fn a_pascal_case_binding_in_if_let_is_a_capture() {
    #[composable]
    fn Parent(value: MutableState<Option<u32>>) {
        if let Some(Count) = value.get() {
            Child!("if let", move || Count.to_string());
        }
    }
    let composition = composition();
    let value = mutable_state_of(Some(1));
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    value.set(Some(2));
    composition.recompose();
    assert_eq!(last(), "2", "Count changed: the lambda must be replaced");
}

/// A PascalCase-named user macro that binds a local: PascalCase macros are taken for call
/// macros and don't taint the names they're given, so `current` is unknown to the walk.
#[test]
fn a_local_bound_by_a_pascal_case_statement_macro_is_a_capture() {
    macro_rules! Bind {
        ($name:ident = $value:expr) => {
            let $name = $value;
        };
    }
    #[composable]
    fn Parent(value: MutableState<u32>) {
        Bind!(current = value.get());
        Child!("Bind", move || current.to_string());
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

/// A call macro on the right of `&&` runs conditionally, with no region group: the two
/// capture-less lambdas below take the same slot in turn (same type `((), Callback<…>)`, keys
/// `()` always same), so after the flip the "b" call is handed the "a" closure.
#[test]
fn lambdas_behind_short_circuits_are_not_confused() {
    #[composable]
    fn Parent(flag: MutableState<bool>) {
        let on = flag.get();
        let _ = on && {
            Child!("a", move || "A".to_string());
            true
        };
        let _ = !on && {
            Child!("b", move || "B".to_string());
            true
        };
    }
    let composition = composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["a: A"]);
    flag.set(false);
    composition.recompose();
    assert_eq!(take_log(), ["b: B"], "the b call's own closure");
}

/// A `break` out of a labelled block skips the rest of the block, but only loops get groups:
/// a lambda after the break shifts the `remember` after the block.
#[test]
fn a_lambda_after_a_labelled_break_doesnt_shift_later_state() {
    #[composable]
    fn Parent(skip: MutableState<bool>) {
        let id = 1u32;
        'body: {
            if skip.get() {
                break 'body;
            }
            Child!("labelled", move || id.to_string());
        }
        let counter = remember(|| mutable_state_of(0u32));
        COUNTER.with(|cell| cell.set(Some(counter)));
        log(format!("counter {}", counter.get()));
    }
    let composition = composition();
    let skip = mutable_state_of(false);
    let handle = skip.handle();
    composition.set_content(move || Parent(handle));
    COUNTER.with(Cell::get).unwrap().set(42);
    composition.recompose();
    take_log();
    skip.set(true);
    composition.recompose();
    assert_eq!(take_log(), ["counter 42"], "the counter's state is kept");
}

// ---- Hygiene of format-string captures ---------------------------------------------------------

/// A composable generated by a `macro_rules!` with the caller's body. The key for a format
/// capture (`"{n}"`) is made as `Ident::new("n", Span::call_site())`: the call site of
/// `#[composable]`, inside the macro, so it resolves to the macro's own `n` rather than the
/// body's `n` that the format string (with the literal's span) prints. The key never changes,
/// and the lambda keeps the first `n`. (With no `n` in the macro the key is E0425 instead:
/// review_c2_compile_format_capture_hygiene.rs.)
#[test]
fn a_format_capture_keys_the_local_the_format_string_names() {
    macro_rules! screen {
        ($name:ident ($($params:tt)*) $body:block) => {
            #[composable]
            fn $name($($params)*) {
                let n = 0u32;
                let _ = n;
                $body
            }
        };
    }
    screen!(Parent(value: MutableState<u32>) {
        let n = value.get();
        Child!("hygiene", move || format!("{n}"));
    });
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["hygiene: 1"]);
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "2", "n changed: the lambda must be replaced");
}

// A call macro invoked by path outside a composable body (`set_content(|| lib::Greeting!())`)
// still expands to the bare name where it's used: a recorded limitation (D65), as a declarative
// macro can't name the module of the function it was generated for.
