//! Phase C sixth review: valid code that `#[composable]` fails to compile.
//!
//! `call!` emits the plain call with its arguments in *parameter* order, so named arguments are
//! evaluated in the callee's order rather than in the order the caller wrote them (Kotlin
//! evaluates named arguments as written). With a top-level lambda moving its captures (see
//! review_c6_compile_capture_used_by_two_lambdas.rs), a value argument written *before* the
//! lambda that reads the same capture is evaluated after the move: here `enabled =
//! !todo.done` is written first, but `Toggle`'s `on_click` comes first, so the expansion is
//! `Toggle(probe(move || … todo …), !todo.done)`. In plain Rust, `Toggle(…)` with the arguments in
//! the order written here reads `todo.done` before moving `todo`, and compiles.
//!
//! Expected: compiles (arguments evaluated in the order written, or the lambda built from a
//! clone). Actual: E0382 at `!todo.done`: `todo` was moved into the closure ("the type `Rc` does
//! not implement `Copy`").

#![allow(non_snake_case, dead_code)]

use std::rc::Rc;

use compose::runtime::*;

struct Todo {
    id: u64,
    done: bool,
}

fn remove(id: u64) {
    let _ = id;
}

#[composable]
fn Toggle(on_click: impl Fn() + 'static, #[default(true)] enabled: bool) {
    let _ = (on_click, enabled);
}

#[composable]
fn TodoRow(todo: Rc<Todo>) {
    Toggle!(enabled = !todo.done, on_click = move || remove(todo.id));
}

#[test]
fn compiles() {}
