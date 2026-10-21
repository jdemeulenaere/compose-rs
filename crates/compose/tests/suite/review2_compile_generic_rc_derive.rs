//! Regression test from the second phase B review: valid code that `#[derive(ComposeArg)]` used to fail to compile.
//!
//! The derive bounds every type parameter by `ComposeArg`, even one only used behind an `Rc`
//! (compared by identity, whatever it points to). docs/03's advice for large data is "wrap it in
//! `Rc`", but a generic wrapper doing exactly that, `Shared<Vec<u32>>`, isn't a `ComposeArg`:
//! E0277 "`Vec<u32>` can't be compared for skipping" at the composable's parameter. (std's
//! derives have the same limitation; here the trait's whole point is that `Rc<T>` needs nothing
//! of `T`.)
//!
//! Expected: compiles (bound the field types, `Rc<T>: ComposeArg`, rather than the parameters).

#![allow(non_snake_case, dead_code)]

use std::rc::Rc;

use compose::runtime::*;

#[derive(Clone, ComposeArg)]
struct Shared<T> {
    items: Rc<T>,
    version: u32,
}

#[composable]
fn List(items: Shared<Vec<u32>>) {
    let _ = items.items.len();
}

#[test]
fn compiles() {}
