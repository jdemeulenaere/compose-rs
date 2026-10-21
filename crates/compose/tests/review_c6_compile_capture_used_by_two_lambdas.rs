//! Phase C sixth review: valid code that `#[composable]` fails to compile.
//!
//! D83 builds a lambda nested in another closure from clones of its captures, and the fourth
//! review did the same for lambdas in loops, but a top-level lambda still *moves* its captures
//! (`move` is added). So a parameter that isn't `Copy` (`String`, `Rc<Todo>`) can be used by one
//! top-level lambda only, and not after it: a content block reading it, then a `Text!` reading it
//! again, or a button whose click handler and content both name it. In Kotlin these are the
//! everyday shapes (`Column { Text(name) }; Text(name)`), and the user wrote no `move`: a trailing
//! block is made a `move` closure by the macro. A miss already clones each capture into the kept
//! keys; building the lambda from clones too (as nested ones are) would cost one more clone per
//! capture on a miss and nothing on a hit.
//!
//! Expected: compiles. Actual: E0382 "borrow of moved value: `name`" at the second `Label!`, and
//! E0382 for `todo`, reported at the `#[composable]` attribute of `Button`, whose call macro
//! expanded.

#![allow(non_snake_case, dead_code)]

use std::rc::Rc;

use compose::runtime::*;

struct Todo {
    id: u64,
    title: String,
}

#[composable]
fn Section(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Label(#[format] text: String) {
    let _ = text;
}

#[composable]
fn Button(on_click: impl Fn() + 'static, content: impl Fn() + 'static) {
    let _ = on_click;
    content();
}

#[composable]
fn Greeting(name: String) {
    Section!({
        Label!("Hello {name}");
    });
    Label!("Goodbye {name}");
}

#[composable]
fn TodoRow(todo: Rc<Todo>) {
    Button!(on_click = move || println!("open {}", todo.id), {
        Label!("{}", todo.title);
    });
}

#[test]
fn compiles() {}
