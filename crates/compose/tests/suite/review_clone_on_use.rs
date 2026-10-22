//! Adversarial review of clone-on-use and the call macro changes (D158–D164). Each failing test
//! is a bug; the compile-fail cases are in `tests/ui/review_clone_on_use_*.rs`.

#![allow(non_snake_case, dead_code)]
// The shapes under review on purpose: an `FnOnce` through `Option::map`, a loop left by `break`.
#![allow(clippy::option_map_unit_fn, clippy::redundant_closure, clippy::never_loop)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose::runtime::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CLICKS: RefCell<Vec<Callback>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn composition() -> Composition {
    take_log();
    CLICKS.with(|clicks| clicks.borrow_mut().clear());
    Composition::new(NodeTree::<()>::new())
}

fn click_all() {
    let clicks = CLICKS.with(|clicks| std::mem::take(&mut *clicks.borrow_mut()));
    for click in &clicks {
        click();
    }
}

#[composable]
fn Section(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Label(#[format] text: String) {
    log(text);
}

#[composable]
fn Show(text: String) {
    log(format!("show {text}"));
}

#[composable]
fn Clickable(on_click: impl Fn() + 'static) {
    CLICKS.with(|clicks| clicks.borrow_mut().push(on_click.clone()));
}

// ---- An optional callback given by name isn't memoized (D162) ------------------------------

#[composable]
fn Card(#[default(None)] on_click: Option<impl Fn() + 'static>, content: impl Fn() + 'static) {
    let _ = &on_click;
    log("card");
    content();
}

#[composable]
fn Plain(on_click: impl Fn() + 'static, content: impl Fn() + 'static) {
    let _ = &on_click;
    log("plain");
    content();
}

fn handle_click() {}

/// A function given by name, required and optional; and a capture-less closure by name.
#[composable]
fn Cards(ticks: MutableState<u32>) {
    log(format!("parent {}", ticks.get()));
    Plain!(on_click = handle_click, {});
    Card!(on_click = handle_click, {});
    let local = || {};
    Card!(on_click = local, {});
}

/// D162: an optional callback takes "a callback or a closure given by name". A required callback
/// given a function by name is memoized on itself ("a `Callback`, a function or a capture-less
/// closure passes the same `Callback` every pass", docs/03), so `Plain` skips. The optional path
/// is `optional_callback(Clone::clone(&f))`, a fresh `Rc` every pass, so `Card` never skips.
#[test]
fn an_optional_callback_given_a_function_by_name_skips() {
    let composition = composition();
    let ticks = mutable_state_of(0);
    let handle = ticks.handle();
    composition.set_content(move || Cards(handle));
    assert_eq!(take_log(), ["parent 0", "plain", "card", "card"]);
    ticks.set(1);
    composition.recompose();
    // Expected: only the parent runs again. Actual: both `Card`s recompose every pass.
    assert_eq!(take_log(), ["parent 1"]);
}

// ---- Rule (b) clones what an `FnOnce` closure could move -----------------------------------

/// A sender moved into a `move` closure inside an `FnOnce` closure (`Option::map`) in a click
/// handler. `tx` is the handler's own local, which docs/03 says "moves as written". It compiled
/// before D158, and the channel disconnected once the closure ran (`for x in rx` ended). Now the
/// inner closure gets `reuse(&tx)` and the handler keeps the original: the channel stays
/// connected and `rx.iter()` would block forever.
#[composable]
fn Sends() {
    Clickable!(
        on_click = move || {
            let (tx, rx) = std::sync::mpsc::channel::<u32>();
            Some(7).map(|value| {
                let send = move || tx.send(value).unwrap();
                send();
            });
            let received: Vec<u32> = rx.try_iter().collect();
            let disconnected =
                matches!(rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Disconnected));
            log(format!("{received:?} disconnected {disconnected}"));
        }
    );
}

#[test]
fn a_sender_moved_into_a_nested_move_closure_is_dropped() {
    let composition = composition();
    composition.set_content(|| Sends());
    click_all();
    // Expected (and before D158): moved, so dropped. Actual: a clone, the original still alive.
    assert_eq!(take_log(), ["[7] disconnected true"]);
}

// ---- Edge cases that work ------------------------------------------------------------------

#[composable]
fn Compute(text: String) -> usize {
    text.len()
}

#[composable]
fn Pair(left: String, right: String) {
    log(format!("{left}{right}"));
}

fn fallible(text: String) -> Result<usize, String> {
    Ok(text.len())
}

#[derive(Clone, PartialEq, ComposeArg)]
struct Person {
    name: String,
}

#[composable]
fn Edges(r#type: String, name: String, person: Person, cell: Rc<Cell<u32>>) {
    Section!({
        // A raw identifier.
        Label!(r#type);
        // Nested call macros, the inner one given a capture by itself.
        Section!({
            Pair!(Compute!(name).to_string(), name);
        });
        // `?` and `return` in a closure around wrapped calls.
        let parse = || -> Result<usize, String> {
            let n = fallible(name)?;
            if n == 0 {
                return Err(name);
            }
            Ok(n)
        };
        let _ = parse();
        // A capture, then a local of the same name.
        Show(name);
        let name = String::from("local");
        Show(name);
        // `ref` patterns and closures with patterns.
        let Person { ref name } = person;
        Label!("{name}");
        let pick = |Person { name }: Person| name;
        Show(pick(person));
        // An `async move` closure.
        let later = async move |n: u32| cell.set(n);
        let _ = later;
        // Labels and `break` around a wrapped call.
        'outer: for _ in 0..1 {
            Show(r#type);
            break 'outer;
        }
    });
}

#[test]
fn edge_cases_compile_and_run() {
    let composition = composition();
    composition.set_content(|| {
        Edges("t".into(), "n".into(), Person { name: "p".into() }, Rc::new(Cell::new(0)))
    });
    assert_eq!(take_log(), ["t", "1n", "show n", "show local", "p", "show p", "show t"]);
}

// ---- ImmutableList: `PartialEq` takes identity for equality --------------------------------

/// `ImmutableList`'s `PartialEq` is "`ptr_eq` or contents equal" for any `T: PartialEq`, so a
/// list holding NaN equals itself (and its clones) while its contents don't: `==` disagrees
/// with the slice it derefs to. `Rc` takes this shortcut only for `T: Eq`.
#[test]
fn an_immutable_list_compares_as_its_slice() {
    let list = ImmutableList::from(vec![f64::NAN]);
    let copy = list.clone();
    assert_eq!(list == copy, list.as_slice() == copy.as_slice());
}
