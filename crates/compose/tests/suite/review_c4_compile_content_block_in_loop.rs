//! Phase C fourth review: valid code that `#[composable]` fails to compile.
//!
//! A lambda nested in a closure is built from clones of its captures (D83), but one in a loop
//! body isn't nested: it moves its captures, and is built (moving them) on every iteration, hit
//! or miss. A content block in a `for` loop naming a `String` from outside the loop, the shape of
//! every list (`for item in items { Row!({ Text!(title.clone()) }) }`), then fails: the user
//! wrote no `move` and no closure, and the same block inside a content block (nested) compiles.
//!
//! Expected: compiles (build lambdas in loops from clones, as nested ones). Actual: E0382 "use of
//! moved value: `title`".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Holder(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Row(#[format] text: String) {
    let _ = text;
}

#[composable]
fn List(title: String, count: u32) {
    for index in 0..count {
        Holder!({
            Row!("{} {index}", title.clone());
        });
    }
}

#[test]
fn compiles() {}
