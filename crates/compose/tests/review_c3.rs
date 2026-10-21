//! Regression tests from the third adversarial review of phase C (after "Fix the second phase C
//! review findings"): concrete inputs where a memoized lambda keeps a stale closure, state shifts
//! between slots, or a callee re-runs every pass. Compile failures are in review_c3_compile_*.rs.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};

use compose::runtime::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static LAST: RefCell<Option<Callback<dyn Fn() -> String>>> = const { RefCell::new(None) };
    static LAST_CHECKED: RefCell<Option<Callback<dyn Fn() -> u32>>> = const { RefCell::new(None) };
    static CLICK: RefCell<Option<Callback<dyn Fn()>>> = const { RefCell::new(None) };
    static COUNTER: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
    static RUNS: Cell<u32> = const { Cell::new(0) };
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

fn runs() -> u32 {
    RUNS.with(Cell::get)
}

#[composable]
fn Child(label: &'static str, show: impl Fn() -> String + 'static) {
    RUNS.with(|runs| runs.set(runs.get() + 1));
    log(format!("{label}: {}", show()));
    LAST.with(|last| *last.borrow_mut() = Some(show.clone()));
}

#[composable]
fn Holder(content: impl Fn() + 'static) {
    content();
}

fn noop() {}

#[composable]
fn Clickable(label: &'static str, #[default(noop)] on_click: impl Fn() + 'static) {
    let _ = label;
    CLICK.with(|click| *click.borrow_mut() = Some(on_click.clone()));
}

fn click() {
    CLICK.with(|click| click.borrow().as_ref().map(|on_click| on_click()));
}

fn composition() -> Composition {
    take_log();
    Composition::new(NodeTree::<()>::new())
}

// ---- Stale closures ----------------------------------------------------------------------------

fn go_a() {
    log("clicked a");
}

fn go_b() {
    log("clicked b");
}

/// An all-caps composable name (`H1`, `QR`, `UI`) isn't PascalCase to `is_pascal_case` (it needs
/// a lowercase letter), so neither `H1!(…)` nor `H1(…)` is a likely composable call and the
/// branches below get no region group. The two call macros' named callbacks then share one slot
/// (`probe_value`, type `((), Callback<dyn Fn()>)`), and since a function item is zero-sized its
/// key is `Unkeyed { zero_sized: true }`, which is `same` as any other zero-sized key: after the
/// flip, the "b" heading is handed `go_a`. The ZST rule (D63) erases the type of the value, so
/// any region miss turns two different functions into a hit.
#[test]
fn functions_given_by_name_in_branches_are_not_confused() {
    #[composable]
    fn H1(label: &'static str, on_click: impl Fn() + 'static) {
        let _ = label;
        CLICK.with(|click| *click.borrow_mut() = Some(on_click.clone()));
    }
    #[composable]
    fn Parent(flag: MutableState<bool>) {
        if flag.get() {
            H1!("a", on_click = go_a);
        } else {
            H1!("b", on_click = go_b);
        }
    }
    let composition = composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || Parent(handle));
    click();
    assert_eq!(take_log(), ["clicked a"]);
    flag.set(false);
    composition.recompose();
    click();
    assert_eq!(take_log(), ["clicked b"], "the b heading's own function");
}

macro_rules! With {
    ($name:ident = $value:expr, $body:block) => {{
        let $name = $value;
        $body
    }};
}

/// review_c2 made a PascalCase *statement* macro taint the new names it assigns
/// (`Bind!(current = …);`), but the same macro in expression position (a `match` arm body, a
/// closure body, a `let` initializer) goes through `visit_expr_mut`, which only rewrites it: its
/// block's lambda naming `doubled` finds no local and no taint, so it gets no key and is kept.
#[test]
fn a_name_bound_by_a_pascal_case_macro_in_expression_position_is_a_capture() {
    #[composable]
    fn Parent(value: MutableState<Option<u32>>) {
        match value.get() {
            Some(n) => With!(doubled = n * 2, {
                Child!("with", move || doubled.to_string());
            }),
            None => {}
        }
    }
    let composition = composition();
    let value = mutable_state_of(Some(1));
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["with: 2"]);
    value.set(Some(2));
    composition.recompose();
    assert_eq!(last(), "4", "doubled changed: the lambda must be replaced");
}

macro_rules! Scoped {
    ($name:ident, $value:expr, $body:block) => {{
        let $name = $value;
        $body
    }};
}

/// The taint of `assigned_names` only covers `name =`: a PascalCase statement macro binding a
/// name given positionally (`Scoped!(doubled, n * 2, { … })`, as `$name:ident, $value:expr`)
/// leaves `doubled` unknown, and the lambda in its block keeps the first value.
#[test]
fn a_name_bound_by_a_pascal_case_macro_without_equals_is_a_capture() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let n = value.get();
        Scoped!(doubled, n * 2, {
            Child!("scoped", move || doubled.to_string());
        });
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["scoped: 2"]);
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "4", "doubled changed: the lambda must be replaced");
}

/// `Memoize::new` collects the body's `macro_rules!` with a syn `Visit`, which doesn't look into
/// macro tokens: one defined inside a call macro's content block isn't in `local_macros`, so a
/// PascalCase one is trusted as a call macro there, though it names `count` (review_c2's
/// `a_local_pascal_case_macro_naming_a_local_makes_the_lambda_changed`, one level down).
#[test]
fn a_local_pascal_case_macro_in_a_content_block_makes_the_lambda_changed() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let count = value.get();
        Holder!({
            macro_rules! Shown {
                () => {
                    count
                };
            }
            Child!("in content", move || Shown!().to_string());
        });
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["in content: 1"]);
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "2", "count changed: the lambda must be replaced");
}

fn check(checked: Callback<dyn Fn() -> u32>) -> bool {
    LAST_CHECKED.with(|last| *last.borrow_mut() = Some(checked.clone()));
    checked() > 0
}

/// In a let chain, `if let Some(x) = … && check(callback(move || x))`, the later operand sees
/// `x`. `visit_expr_if_mut` visits the whole condition before binding the chain's names (and
/// `visit_expr_while_mut` the same), so `x` is no local there and the callback gets no key.
#[test]
fn a_let_chain_binding_is_a_capture_in_later_operands() {
    #[composable]
    fn Parent(value: MutableState<Option<u32>>) {
        if let Some(x) = value.get()
            && check(callback(move || x))
        {
            log("checked");
        }
    }
    let composition = composition();
    let value = mutable_state_of(Some(1));
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["checked"]);
    value.set(Some(2));
    composition.recompose();
    let checked = LAST_CHECKED.with(|last| last.borrow().as_ref().map(|checked| checked()));
    assert_eq!(checked, Some(2), "x changed: the lambda must be replaced");
}

/// `format_names` reads the literal's source text, so an escape in an inline argument
/// (`"{\x63ount}"`, which `format!` reads as `{count}` and captures `count`) isn't a name.
/// Contrived, but a real capture missed by the walk.
#[test]
fn an_escaped_format_name_is_a_capture() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let count = value.get();
        Child!("escaped", move || format!("{\x63ount}"));
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["escaped: 1"]);
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "2", "count changed: the lambda must be replaced");
}

// ---- State shifting ----------------------------------------------------------------------------

macro_rules! when {
    ($condition:expr, $($body:tt)*) => {
        if $condition {
            $($body)*
        }
    };
}

/// A call macro's defaulted (or named) callback is a slot in the *caller's* group, which the
/// caller can't see. Inside a user macro that expands to a branch, the region guess finds no
/// branch, so the slot comes and goes and the `remember` after it loses its state. The same
/// statement with a plain call (`when!(…, Plain("x"))`) keeps it (below): only the call macro's
/// hidden probe shifts.
#[test]
fn a_call_macro_in_a_user_macro_branch_doesnt_shift_later_state() {
    #[composable]
    fn Parent(show: MutableState<bool>) {
        when!(show.get(), Clickable!("x"););
        let counter = remember(|| mutable_state_of(0u32));
        COUNTER.with(|cell| cell.set(Some(counter)));
        log(format!("counter {}", counter.get()));
    }
    let composition = composition();
    let show = mutable_state_of(false);
    let handle = show.handle();
    composition.set_content(move || Parent(handle));
    COUNTER.with(Cell::get).unwrap().set(42);
    composition.recompose();
    take_log();
    show.set(true);
    composition.recompose();
    assert_eq!(take_log(), ["counter 42"], "the counter's state is kept");
}

macro_rules! either {
    ($condition:expr, $then:expr, $otherwise:expr) => {
        if $condition { $then } else { $otherwise }
    };
}

/// The same region miss with two call macros of a PascalCase composable: their named functions
/// share the slot, and (zero-sized keys being `same`) the second call is handed the first's.
#[test]
fn functions_given_by_name_in_a_user_macro_branch_are_not_confused() {
    #[composable]
    fn Parent(flag: MutableState<bool>) {
        either!(flag.get(), Clickable!("a", on_click = go_a), Clickable!("b", on_click = go_b));
    }
    let composition = composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || Parent(handle));
    click();
    assert_eq!(take_log(), ["clicked a"]);
    flag.set(false);
    composition.recompose();
    click();
    assert_eq!(take_log(), ["clicked b"], "the b call's own function");
}

#[composable]
fn Plain(label: &'static str) {
    let _ = label;
}

/// The control for the test above: passes today.
#[test]
fn a_plain_call_in_a_user_macro_branch_doesnt_shift_later_state() {
    #[composable]
    fn Parent(show: MutableState<bool>) {
        when!(show.get(), Plain("x"););
        let counter = remember(|| mutable_state_of(0u32));
        COUNTER.with(|cell| cell.set(Some(counter)));
        log(format!("counter {}", counter.get()));
    }
    let composition = composition();
    let show = mutable_state_of(false);
    let handle = show.handle();
    composition.set_content(move || Parent(handle));
    COUNTER.with(Cell::get).unwrap().set(42);
    composition.recompose();
    take_log();
    show.set(true);
    composition.recompose();
    assert_eq!(take_log(), ["counter 42"], "the counter's state is kept");
}

/// An all-caps composable (`H1`) isn't a likely composable call (see
/// `functions_given_by_name_in_branches_are_not_confused`): the `if` gets no region group, and
/// the second heading takes the first one's state, as with any missed group.
#[test]
fn an_all_caps_composable_in_a_branch_gets_a_region() {
    #[composable]
    fn H1(label: &'static str) {
        let kept = remember(|| mutable_state_of(label));
        log(format!("{label} keeps {}", kept.get()));
    }
    #[composable]
    fn Parent(show: MutableState<bool>) {
        if show.get() {
            H1("title");
        }
        H1("subtitle");
    }
    let composition = composition();
    let show = mutable_state_of(false);
    let handle = show.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["subtitle keeps subtitle"]);
    show.set(true);
    composition.recompose();
    // The subtitle keeps its own group (and skips): only the new title runs.
    assert_eq!(take_log(), ["title keeps title"]);
}

// ---- Lambdas counted as always changed ---------------------------------------------------------

/// `scan_tokens` takes any identifier followed by a `!` punct for a macro call, without looking
/// at its spacing, so `limit != 0` inside a known macro's tokens reads as a call of a macro
/// `limit!`: the lambda is always changed, a new `Callback` each pass, and `Child` never skips.
#[test]
fn a_not_equal_in_macro_tokens_keeps_the_lambda() {
    #[composable]
    fn Parent(tick: MutableState<u32>, limit: u32) {
        tick.get();
        Child!("not equal", move || format!("{}", limit != 0));
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle, 3));
    RUNS.with(|runs| runs.set(0));
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "nothing Child takes changed: it skips");
}

/// memo.rs's `KNOWN_MACROS` lacks built-ins that can't reach a local (`cfg!`, `line!`,
/// `file!`, `env!`, `module_path!`, `include_str!`; regions.rs's list has most of them), so a
/// lambda using `cfg!(debug_assertions)` is always changed and its callee never skips.
#[test]
fn a_cfg_macro_in_a_lambda_keeps_the_lambda() {
    #[composable]
    fn Parent(tick: MutableState<u32>, limit: u32) {
        tick.get();
        Child!("cfg", move || {
            if cfg!(debug_assertions) { limit.to_string() } else { String::new() }
        });
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle, 3));
    RUNS.with(|runs| runs.set(0));
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "nothing Child takes changed: it skips");
}
