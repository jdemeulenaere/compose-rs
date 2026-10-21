//! Regression tests from the sixth adversarial review of phase C (after "Fix the fifth phase C
//! review findings"): concrete inputs where a call reaches another function, or a callee re-runs
//! every pass. Compile failures and macro panics are in review_c6_compile_*.rs.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};

use compose::runtime::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static RUNS: Cell<u32> = const { Cell::new(0) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn runs() -> u32 {
    RUNS.with(Cell::get)
}

fn reset_runs() {
    RUNS.with(|runs| runs.set(0));
}

fn composition() -> Composition {
    take_log();
    reset_runs();
    Composition::new(NodeTree::<()>::new())
}

// ---- Another function called -------------------------------------------------------------------

mod material {
    #[compose::runtime::composable]
    pub fn Button(label: &'static str) {
        super::log(format!("material Button {label}"));
    }
}

mod widgets {
    #[compose::runtime::composable]
    pub fn Button(label: &'static str) {
        super::log(format!("widgets Button {label}"));
    }
}

use material::Button as MaterialButton;
use widgets::Button;

/// Two libraries both export a `Button`, so one is imported under another name (`use
/// material::Button as MaterialButton;`, the usual way out of a name clash; Kotlin's `import …
/// as …`). The `use` imports the call macro under the new name too, but its expansion calls the
/// function by the name it was declared with: `MaterialButton!("b")` becomes `Button("b")`,
/// which resolves where the macro is used, to the other library's `Button`. Same signature, so
/// it compiles and silently calls the wrong composable. The fifth review passes the path first
/// for a call macro invoked by path (`material::Button!`, D99), but not for a single segment;
/// passing `@path (MaterialButton)` there too would call the function the caller named.
#[test]
#[ignore = "accepted limitation (D114): a call macro calls by its declared name"]
fn a_call_macro_imported_under_another_name_calls_the_function_it_names() {
    #[composable]
    fn Screen() {
        Button!("a");
        MaterialButton!("b");
    }
    let composition = composition();
    composition.set_content(Screen);
    assert_eq!(take_log(), ["widgets Button a", "material Button b"]);
}

// ---- Callees that never skip -------------------------------------------------------------------

#[composable]
fn Badge(text: &'static str) {
    log(text);
}

#[composable]
fn Field(#[default(None)] label: Option<impl Fn() + 'static>) {
    RUNS.with(|runs| runs.set(runs.get() + 1));
    if let Some(label) = &label {
        label();
    }
}

/// The fifth review memoizes `Some(lambda)` for an optional callback: `Memoize::mark` marks the
/// closure inside `Some(…)` (its first statement is the marker). But `Regions` then visits the
/// argument as a plain expression, and a closure making a likely composable call is a region of
/// its own: `wrap_expr` puts the region guard in front of the marker. `call!` no longer finds the
/// marker first, takes the closure for an unmarked one and wraps it in a new `Callback` every
/// pass, so `Field` never skips. An optional *content* lambda (`label`, `placeholder`,
/// `leading_icon` in Material's `TextField`) is exactly the closure that makes composable calls.
/// The fifth review's test passed because its lambda (`move || log(label)`) makes none, and
/// `label = move || Badge("name")` without `Some` skips (below). The same loss inside a content
/// block is a compile error: review_c6_compile_some_content_lambda_in_content.rs.
#[test]
fn some_content_lambda_for_an_optional_callback_is_memoized() {
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        tick.get();
        Field!(label = Some(move || Badge("name")));
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle));
    assert_eq!(take_log(), ["name"]);
    reset_runs();
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "the lambda captures nothing: the same callback, Field skips");
}

/// The control for the test above: passes today.
#[test]
fn a_bare_content_lambda_for_an_optional_callback_is_memoized() {
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        tick.get();
        Field!(label = move || Badge("name"));
    }
    let composition = composition();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    composition.set_content(move || Parent(handle));
    reset_runs();
    tick.set(1);
    composition.recompose();
    assert_eq!(runs(), 0, "the same callback: Field skips");
}

#[composable]
fn Tagged(modifier: compose::ui::Modifier) {
    let _ = modifier;
    RUNS.with(|runs| runs.set(runs.get() + 1));
}

/// D107 memoizes modifier-chain callbacks on chains rooted at `Modifier`, at a variable named
/// `modifier`, or at a local bound to a modifier (`let base = Modifier.…`, `let base: Modifier =
/// …`). A *parameter* typed `Modifier` under another name (`row_modifier: Modifier`, as Compose's
/// `contentModifier`, or any composable taking two modifiers) isn't recognized, though its type is
/// written out: `Memoize::new` binds parameters without looking at their types. Its `clickable`
/// closure becomes a new `Callback` every pass, and `Tagged` never skips. The same parameter
/// named `modifier` skips.
#[test]
fn a_clickable_closure_on_a_modifier_parameter_lets_the_callee_skip() {
    use compose::ui::{Modifier, UnitExt};
    #[composable]
    fn Item(tick: MutableState<u32>, row_modifier: Modifier) {
        tick.get();
        Tagged(row_modifier.clickable(move || log("clicked")));
    }
    #[composable]
    fn Parent(tick: MutableState<u32>) {
        Item(tick, Modifier.padding(4.dp()));
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
