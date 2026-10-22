//! Clone-on-use review: a capture given by itself to a call macro and also used in another of
//! its arguments.
//!
//! A call macro's capture is cloned once, by shadowing the name in a block around the call
//! (`{ let name = reuse(&name); Pair!(name, name.to_uppercase()) }`); the call then moves that
//! clone into the first argument and borrows it in the second. The same arguments to a function
//! (`Pair(name, name.to_uppercase())`) compile, since there each bare use is `reuse(&name)` in
//! place. Common shape: `Chip!(label, selected = label == current)`.
//!
//! Expected: compiles. Actual: E0382 "borrow of moved value: `name`".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Section(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Pair(left: String, right: String) {
    let _ = (left, right);
}

#[composable]
fn GivenAndBorrowed(name: String) {
    Section!({
        // A function: compiles.
        Pair(name, name.to_uppercase());
        // The same as a call macro: E0382.
        Pair!(name, name.to_uppercase());
    });
}

#[test]
fn compiles() {}
