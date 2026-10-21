//! Regression tests from the fourth adversarial review of phase C (after "Fix the third phase C
//! review findings"): concrete inputs where a memoized lambda keeps a stale closure, a call
//! reaches the wrong function, or a callee re-runs every pass. Compile failures are in
//! review_c4_compile_*.rs.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose::runtime::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static LAST: RefCell<Option<Callback<dyn Fn() -> String>>> = const { RefCell::new(None) };
    static CLICK: RefCell<Option<Callback<dyn Fn()>>> = const { RefCell::new(None) };
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

fn reset_runs() {
    RUNS.with(|runs| runs.set(0));
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

#[composable]
fn Label(#[format] text: String) {
    log(format!("label {text}"));
}

fn noop() {}

#[composable]
fn Clickable(label: &'static str, #[default(noop)] on_click: impl Fn() + 'static) {
    let _ = label;
    RUNS.with(|runs| runs.set(runs.get() + 1));
    CLICK.with(|click| *click.borrow_mut() = Some(on_click.clone()));
}

fn click() {
    CLICK.with(|click| click.borrow().as_ref().map(|on_click| on_click()));
}

fn composition() -> Composition {
    take_log();
    reset_runs();
    Composition::new(NodeTree::<()>::new())
}

// ---- Stale closures ----------------------------------------------------------------------------

/// D84 visits a call macro's arguments as expressions, which drops the format-string scan that
/// `scan_tokens` does: an inline format argument in a `#[format]` call macro's literal
/// (`Label!("n = {n}")`) is a `Lit` expression, which the visitor ignores. The content block's
/// only use of `n` is missed, the block is memoized with no keys, `Holder` skips, and the label
/// keeps showing the first value. `Text!("Count: {count}")` inside a `Column!({ … })` is the
/// most common Compose shape there is.
#[test]
fn an_inline_format_argument_of_a_call_macro_in_a_content_block_is_a_capture() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let n = value.get();
        Holder!({
            Label!("n = {n}");
        });
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["label n = 1"]);
    value.set(2);
    composition.recompose();
    assert_eq!(take_log(), ["label n = 2"], "n changed: the content block must be replaced");
}

/// The same miss for a lambda (not a content block) whose body is a call macro with a format
/// literal: `Child!(…, move || { Label!("{n}"); … })` is unusual, but a user PascalCase macro
/// taking a format string (`Log!("{n}")`, hygienic, expanding to `log(format!($($t)*))`) is the
/// same expression visit.
macro_rules! Log {
    ($($format:tt)*) => {
        log(format!($($format)*))
    };
}

#[test]
fn an_inline_format_argument_of_a_pascal_case_macro_in_a_lambda_is_a_capture() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let n = value.get();
        Clickable!("log", on_click = move || Log!("clicked {n}"));
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    value.set(2);
    composition.recompose();
    click();
    assert_eq!(take_log(), ["clicked 2"], "n changed: the lambda must be replaced");
}

macro_rules! Push {
    ($target:ident = $value:expr) => {
        $target.borrow_mut().push($value)
    };
}

/// D84 takes `name = …` in any PascalCase macro for a named argument, whose name isn't a use.
/// A user's PascalCase macro may use it as an expression (`Push!(target = value)` expanding to
/// `target.borrow_mut().push(value)`; macro_rules hygiene lets the call site's `target` through),
/// so the lambda's use of the parameter `sink` is missed and it keeps pushing into the first
/// pass's vector.
#[test]
#[ignore = "recorded limitation D98: a named argument's name is not a use"]
fn a_name_before_equals_in_a_user_pascal_case_macro_is_a_capture() {
    type Sink = Rc<RefCell<Vec<u32>>>;
    #[composable]
    fn Parent(second: MutableState<bool>, a: Sink, b: Sink) {
        let sink = if second.get() { b.clone() } else { a.clone() };
        Clickable!("push", on_click = move || Push!(sink = 1));
    }
    let composition = composition();
    let (a, b): (Sink, Sink) = Default::default();
    let second = mutable_state_of(false);
    let handle = second.handle();
    let (sink_a, sink_b) = (a.clone(), b.clone());
    composition.set_content(move || Parent(handle, sink_a.clone(), sink_b.clone()));
    second.set(true);
    composition.recompose();
    click();
    assert_eq!(
        (a.borrow().len(), b.borrow().len()),
        (0, 1),
        "sink changed: the lambda must be replaced"
    );
}

macro_rules! Split {
    (($low:ident, $high:ident) = $value:expr) => {
        let ($low, $high) = $value;
    };
}

/// The third review taints the new names in a PascalCase macro's arguments, but `unbound_names`
/// reads only the top-level tokens: names a user macro binds from inside a group, as a tuple
/// pattern (`Split!((low, high) = …);`), a slice or a struct pattern, aren't tainted. `high` is
/// then neither a local nor tainted, the lambda gets no key, and it keeps the first value.
#[test]
fn a_name_bound_in_a_group_by_a_pascal_case_macro_is_a_capture() {
    #[composable]
    fn Parent(value: MutableState<u32>) {
        let n = value.get();
        Split!((low, high) = (n, n * 10));
        let _ = low;
        // A block body: `high` directly in `Child!`'s top-level tokens would be tainted by
        // `Child!` itself.
        Child!("split", move || { high.to_string() });
    }
    let composition = composition();
    let value = mutable_state_of(1);
    let handle = value.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["split: 10"]);
    value.set(2);
    composition.recompose();
    assert_eq!(last(), "20", "high changed: the lambda must be replaced");
}

// ---- Wrong function ----------------------------------------------------------------------------

mod remote {
    use super::*;

    #[composable]
    pub fn Label(#[format] text: String) {
        log(format!("remote label {text}"));
    }
}

/// The third review's fix for path statement macros (`crate::Bind!(…);`) imports the path as a
/// statement in the enclosing block: `remote::Label!("b");` becomes `use remote::Label;
/// Label!("b");`. A `use` in a block is in scope in the whole block, before it too, so the
/// earlier `Label!("a")` (the local call macro, textually scoped) now expands to a call of
/// `Label` that resolves to `remote::Label`: the wrong function is called, silently, since both
/// take a `String`. The same goes for a plain `Label(…)` call before it.
#[test]
fn a_path_statement_macro_doesnt_redirect_earlier_calls_in_its_block() {
    #[composable]
    fn Parent() {
        Label!("a");
        remote::Label!("b");
    }
    let composition = composition();
    composition.set_content(Parent);
    assert_eq!(take_log(), ["label a", "remote label b"]);
}

fn go_a() {
    log("clicked a");
}

fn go_b() {
    log("clicked b");
}

macro_rules! pick {
    ($flag:expr, $name:ident = $a:expr, $b:expr, $body:expr) => {
        if $flag {
            let $name = $a;
            $body
        } else {
            let $name = $b;
            $body
        }
    };
}

/// D88's guarantee (a missed region group costs state, never a call of the wrong function)
/// rests on the site id telling lambdas apart by their code. A macro pasting the same tokens in
/// two branches makes two sites with the same code: here a user macro binding `f` differently
/// in each. The third review made it a region, but one around the whole macro, not per branch,
/// so the branches' `Clickable!` probes share a slot; their site ids are equal, and the keys
/// (`f`, a zero-sized function item, `Unkeyed { zero_sized: true }` whatever its type) are
/// `same`: after the flip, the b branch is handed `go_a`.
#[test]
fn the_same_code_in_two_branches_of_a_user_macro_isnt_handed_the_other_function() {
    #[composable]
    fn Parent(flag: MutableState<bool>) {
        pick!(flag.get(), f = go_a, go_b, Clickable!("x", on_click = f));
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
    assert_eq!(take_log(), ["clicked b"], "the b branch's own function");
}

/// The same with no macro: a plain generic helper calling a call macro (call macros work
/// anywhere, D65) is one site for every instantiation, and a lowercase call is no likely call,
/// so the branches get no region. `F` is `go_a`'s type in one branch and `go_b`'s in the other,
/// both zero-sized, so the keys are `same` and the b branch is handed `go_a`.
#[test]
fn a_generic_helper_called_in_two_branches_isnt_handed_the_other_function() {
    fn clickable<F: Fn() + Clone + 'static>(on_click: F) {
        Clickable!("x", on_click = on_click);
    }
    #[composable]
    fn Parent(flag: MutableState<bool>) {
        if flag.get() {
            clickable(go_a);
        } else {
            clickable(go_b);
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
    assert_eq!(take_log(), ["clicked b"], "the b branch's own function");
}

// ---- Callees that stop skipping ----------------------------------------------------------------

fn save() {
    log("saved");
}

/// `rewrite_call_macro` taints every new name in a PascalCase macro's top-level tokens, to cover
/// user macros binding them. A function given by name (`on_click = save`) is such a name, so
/// every later lambda calling `save()` counts as always changed: a new callback each pass, and
/// its callee never skips.
#[test]
fn a_function_given_by_name_doesnt_make_later_lambdas_calling_it_always_changed() {
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        tick.get();
        Clickable!("save", on_click = save);
        Child!("after", move || {
            save();
            String::from("x")
        });
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle));
    reset_runs();
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "nothing the callees take changed: both skip");
    let _ = last();
}

#[composable]
fn Tagged(modifier: compose::ui::Modifier) {
    let _ = modifier;
    RUNS.with(|runs| runs.set(runs.get() + 1));
}

/// D86 memoizes closures in modifier chains rooted at the path `Modifier`. Compose's idiom is
/// to extend the modifier a composable is given (`modifier.clickable { … }`): rooted at a local,
/// the chain isn't recognized, its closure becomes a new `Callback` every pass, and the callee
/// receiving the modifier never skips (and the callback and the chain are allocated each pass).
#[test]
fn a_clickable_closure_on_a_modifier_parameter_lets_the_callee_skip() {
    use compose::ui::Modifier;
    #[composable]
    fn Parent(tick: MutableState<u32>, modifier: Modifier) {
        tick.get();
        Tagged(modifier.clickable(move || log("clicked")));
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle, Modifier));
    reset_runs();
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "the modifier is the same: Tagged skips");
}

/// The control: rooted at `Modifier`, the same chain is memoized and the callee skips.
#[test]
fn a_clickable_closure_on_modifier_lets_the_callee_skip() {
    use compose::ui::Modifier;
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        tick.get();
        Tagged(Modifier.clickable(move || log("clicked")));
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle));
    reset_runs();
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "the modifier is the same: Tagged skips");
}

// ---- State shifting ----------------------------------------------------------------------------

thread_local! {
    static COUNTER: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
}

macro_rules! When {
    ($condition:expr, $body:expr) => {
        if $condition {
            $body;
        }
    };
}

/// The third review made user macros making likely calls regions (`when!(show, Clickable!(…))`),
/// but only lowercase ones: a PascalCase macro is taken for a call macro, so the same user macro
/// named `When!` gets no region. The defaulted callback's probe of `Clickable!("x")`, a slot in
/// the caller's group, comes and goes with `show`, and the `remember` after it loses its state.
/// (Blocks given to it are content regions; an expression isn't.)
#[test]
fn a_call_macro_in_a_pascal_case_user_macro_branch_doesnt_shift_later_state() {
    #[composable]
    fn Parent(show: MutableState<bool>) {
        When!(show.get(), Clickable!("x"));
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
