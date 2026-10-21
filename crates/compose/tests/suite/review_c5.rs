//! Regression tests from the fifth adversarial review of phase C (after "Fix the fourth phase C
//! review findings"): concrete inputs where a memoized lambda keeps a stale closure, a call
//! reaches another site's function, or a callee re-runs every pass. Compile failures are in
//! review_c5_compile_*.rs.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};

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

/// `format_names` reads a width argument (`name$`) as the alphanumeric run before the `$`, so
/// after the `0` flag (`{:0width$}`, zero-padded to a width taken from `width`) the run is
/// `0width`, which isn't a name: the only use of `width` is missed, the lambda gets no key, and
/// it keeps padding to the first width. `{:0width$}` is the std::fmt docs' own example of a
/// named width with zero padding (`{:#0width$x}` for hex, `{:+0width$}` with a sign: the same).
#[test]
fn a_named_width_after_the_zero_flag_is_a_capture() {
    #[composable]
    fn Parent(digits: MutableState<usize>) {
        let width = digits.get();
        Child!("padded", move || format!("{:0width$}", 7));
    }
    let composition = composition();
    let digits = mutable_state_of(2);
    let handle = digits.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["padded: 07"]);
    digits.set(3);
    composition.recompose();
    assert_eq!(last(), "007", "width changed: the lambda must be replaced");
}

// ---- Another site's function -------------------------------------------------------------------

/// The fourth review keyed zero-sized captures by their type name, so two function items never
/// share a kept callback. But `type_name` doesn't tell closures apart: every closure of a
/// function is `…::Parent::{{closure}}`. A helper generic over its handler (`impl Fn() + Clone`,
/// called by a lowercase name, so no region) is one site for every instantiation; given two
/// capture-less closures in the two branches, its probe sees the same site and the same
/// "zero-sized, same type name" key, and after the flip the b branch is handed the a closure.
#[test]
fn capture_less_closures_given_to_a_generic_helper_in_two_branches_arent_confused() {
    fn menu_item(on_click: impl Fn() + Clone + 'static) {
        Clickable!("item", on_click = on_click);
    }
    #[composable]
    fn Parent(flag: MutableState<bool>) {
        if flag.get() {
            menu_item(|| log("clicked a"));
        } else {
            menu_item(|| log("clicked b"));
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
    assert_eq!(take_log(), ["clicked b"], "the b branch's own closure");
}

/// The same collision without a helper: review_c4's `pick!` (a user macro pasting the same
/// tokens in two branches, one region around both) with capture-less closures in place of the
/// function items that test used.
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

#[test]
fn capture_less_closures_bound_in_two_branches_of_a_user_macro_arent_confused() {
    #[composable]
    fn Parent(flag: MutableState<bool>) {
        pick!(
            flag.get(),
            f = || log("clicked a"),
            || log("clicked b"),
            Clickable!("x", on_click = f)
        );
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
    assert_eq!(take_log(), ["clicked b"], "the b branch's own closure");
}

// ---- Callees that stop skipping ----------------------------------------------------------------

fn save() {
    log("saved");
}

/// The fourth review stopped tainting a function given by name (`on_click = save`), but
/// `unbound_names` still taints a whole positional argument that is a single new name, to cover
/// user macros binding it (`Scoped!(doubled, …)`). A function given positionally
/// (`Clickable!("save", save)`, as `Button(onClick = ::save)` positionally in Kotlin) is such a
/// name: every later lambda calling `save()` counts as always changed, and its callee never
/// skips.
#[test]
fn a_function_given_positionally_doesnt_make_later_lambdas_calling_it_always_changed() {
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        tick.get();
        Clickable!("save", save);
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
}

#[composable]
fn Tagged(modifier: compose::ui::Modifier) {
    let _ = modifier;
    RUNS.with(|runs| runs.set(runs.get() + 1));
}

/// The fourth review memoizes modifier-chain closures rooted at `Modifier` or at a variable
/// named `modifier`. A modifier built in steps under another name (`let base = …;
/// base.clickable(…)`, or a `row_modifier` from a branch) isn't recognized: its closure becomes
/// a new `Callback` every pass, and the callee given the modifier never skips. Renaming `base`
/// to `modifier` makes the same code skip.
#[test]
fn a_clickable_closure_on_a_modifier_built_in_steps_lets_the_callee_skip() {
    use compose::ui::{Modifier, UnitExt};
    #[composable]
    fn Parent(tick: MutableState<u32>, selectable: bool) {
        tick.get();
        let base = Modifier.fill_max_width().padding(4.dp());
        let row = if selectable { base.clickable(move || log("clicked")) } else { base };
        Tagged(row);
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle, true));
    reset_runs();
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "the modifier is the same: Tagged skips");
}

macro_rules! trace_value {
    ($value:expr) => {
        let _ = &$value;
    };
}

/// A lowercase statement macro the walk doesn't know taints every identifier it is given, even
/// a local (`trace_value!(count);`), whereas the PascalCase rule taints only new names: a local's
/// key already resolves to whatever binding is in scope where the lambda is. So a lambda after
/// it naming that local counts as always changed, and its callee never skips. A debug or trace
/// macro of the app's own, called on a value the screen then renders, is common.
#[test]
fn a_local_given_to_a_user_statement_macro_doesnt_make_later_lambdas_always_changed() {
    #[composable]
    fn Parent(tick: MutableState<u32>, count: MutableState<u32>) {
        tick.get();
        let count = count.get();
        trace_value!(count);
        Child!("count", move || count.to_string());
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let count = mutable_state_of(5);
    let (tick_handle, count_handle) = (tick.handle(), count.handle());
    composition.set_content(move || Parent(tick_handle, count_handle));
    assert_eq!(take_log(), ["count: 5"]);
    reset_runs();
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "count is the same: Child skips");
}

// ---- State shifting ----------------------------------------------------------------------------

thread_local! {
    static COUNTER: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
}

#[composable]
fn Card(modifier: compose::ui::Modifier, content: impl Fn() + 'static) {
    let _ = modifier;
    content();
}

/// The fourth review made a PascalCase macro with a non-lambda argument making a likely call a
/// region (`When!(show, Clickable!("x"))`). A composable's own call macro matches too whenever
/// an argument calls a PascalCase function: `Modifier.padding(Dp(8.0))`,
/// `shape = RoundedCornerShape(8.dp())`. In expression position (a `match` arm without braces,
/// a closure body such as `key(id, || Row!(…))`), `Regions::visit_expr_mut` then wraps the
/// macro and stops: `visit_call_macro` never runs, so its content block gets no region and the
/// branches inside get no groups. Here the hint's defaulted-callback probe comes and goes with
/// `show` in `Card`'s group, and the `remember` after it loses its state. Written
/// `Modifier.padding(8.dp())`, the same code keeps it.
#[test]
fn branches_in_the_content_of_a_call_macro_in_a_match_arm_dont_shift_later_state() {
    use compose::ui::{Dp, Modifier};
    #[composable]
    fn Parent(page: MutableState<u32>, show: MutableState<bool>) {
        let show = show.get();
        match page.get() {
            0 => Card!(Modifier.padding(Dp(8.0)), {
                if show {
                    Clickable!("hint");
                }
                let counter = remember(|| mutable_state_of(0u32));
                COUNTER.with(|cell| cell.set(Some(counter)));
                log(format!("counter {}", counter.get()));
            }),
            _ => {}
        }
    }
    let composition = composition();
    let page = mutable_state_of(0);
    let show = mutable_state_of(false);
    let (page_handle, show_handle) = (page.handle(), show.handle());
    composition.set_content(move || Parent(page_handle, show_handle));
    COUNTER.with(Cell::get).unwrap().set(42);
    composition.recompose();
    take_log();
    show.set(true);
    composition.recompose();
    assert_eq!(take_log(), ["counter 42"], "the counter's state is kept");
}

// ---- Since the fixes ---------------------------------------------------------------------------

#[composable]
fn Clearable(#[default(None)] on_clear: Option<impl Fn() + 'static>) {
    RUNS.with(|runs| runs.set(runs.get() + 1));
    if let Some(on_clear) = &on_clear {
        CLICK.with(|click| *click.borrow_mut() = Some(on_clear.clone()));
    }
}

/// `Some(lambda)` for an optional callback is memoized as a bare lambda is: unchanged captures
/// keep the callback, so the callee skips; changed ones replace it.
#[test]
fn some_lambda_for_an_optional_callback_is_memoized() {
    #[composable]
    fn Parent(tick: MutableState<u32>, label: MutableState<&'static str>) {
        tick.get();
        let label = label.get();
        Clearable!(on_clear = Some(move || log(label)));
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let label = mutable_state_of("first");
    let (tick_handle, label_handle) = (tick.handle(), label.handle());
    composition.set_content(move || Parent(tick_handle, label_handle));
    reset_runs();
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "the captures are the same: Clearable skips");
    label.set("second");
    composition.recompose();
    click();
    assert_eq!(take_log(), ["second"]);
}
