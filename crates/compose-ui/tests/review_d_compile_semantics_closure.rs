//! Phase D review: valid code that fails to compile.
//!
//! The macro memoizes every closure argument of a method chain rooted at `Modifier`, turning
//! it into a `Callback` (D70). `Modifier.semantics` takes `impl Fn(&mut SemanticsConfiguration)`,
//! which a `Callback` isn't (and a callback can't take a reference argument, D59), so the most
//! basic use of `Modifier.semantics` inside a composable fails to compile. The same code
//! compiles outside a composable. A user's own modifier method taking a closure that isn't a
//! callback (`Modifier.then_if(cond, |m| m.padding(…))`) breaks the same way.
//!
//! Expected: compiles. Actual: E0277 / E0282 on the closure.

#![allow(non_snake_case, dead_code)]

extern crate compose_runtime as compose;

use compose_runtime::*;
use compose_ui::*;

#[composable]
fn Labelled() {
    Layout(
        Modifier.semantics(false, |config| config.text.push("label".into())),
        empty_measure_policy(),
        callback(|| {}),
    );
}

trait ThenIf {
    fn then_if(self, condition: bool, then: impl FnOnce(Modifier) -> Modifier) -> Modifier;
}

impl ThenIf for Modifier {
    fn then_if(self, condition: bool, then: impl FnOnce(Modifier) -> Modifier) -> Modifier {
        if condition { then(self) } else { self }
    }
}

#[composable]
fn Padded(padded: bool) {
    Layout(
        Modifier.then_if(padded, |modifier| modifier.padding(4.dp())),
        empty_measure_policy(),
        callback(|| {}),
    );
}

#[test]
fn compiles() {}
