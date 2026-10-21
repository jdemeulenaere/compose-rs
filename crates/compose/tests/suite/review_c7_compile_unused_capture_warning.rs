//! Phase C seventh review: valid code that `#[composable]` fails to compile under
//! `#![deny(warnings)]` (or CI's `-D warnings`).
//!
//! Over-counting captures was harmless (docs/03, step 6: "the lambda is just recreated more
//! often than necessary"). Since D115 each capture is rebound in the lambda's builder, `let id =
//! Clone::clone(&id);`, with the span of the identifier the walk found. When the walk over-counts
//! (an identifier in macro tokens that isn't a use: a struct literal's field name, a format
//! macro's named argument, a `tracing` field), the rebinding is never used, and rustc reports
//! `unused variable: id` at the user's field name, where nothing is wrong. Before D115 a
//! top-level lambda only borrowed the name for its key, which counted as a use.
//!
//! Expected: compiles without warnings. Actual: "unused variable: `id`" at the field name `id`
//! in `Opened { id: … }`, an error here.

#![deny(unused_variables)]
#![allow(non_snake_case, dead_code)]

use std::rc::Rc;

use compose::runtime::*;

#[composable]
fn Button(on_click: impl Fn() + 'static) {
    let _ = on_click;
}

#[composable]
fn Label(#[format] text: String) {
    let _ = text;
}

struct Todo {
    id: u64,
    title: String,
}

#[derive(Debug)]
struct Opened {
    id: u64,
}

#[composable]
fn TodoRow(id: u64, todo: Rc<Todo>) {
    Label!("#{id}: {}", todo.title);
    Button!(on_click = move || println!("{:?}", Opened { id: todo.id }));
}

#[test]
fn compiles() {}
