//! Phase C second review: valid code that fails to compile.
//!
//! A closure given for a callback parameter with arguments, its parameter untyped, as in
//! Compose's `onValueChange = { text -> … text.length … }`. The probe passes the closure to
//! `LambdaProbe::resolve<C: IntoCallback<F>>` (and an unmemoized one to `callback`), a bound
//! that isn't a `Fn` bound, so rustc can't deduce the closure's signature from the parameter's
//! `Callback<dyn Fn(String)>`: E0282 "type annotations needed" as soon as the body uses the
//! argument other than by passing it on (a method call, a field). Only `move |text: String|`
//! compiles. The call macro knows the arity, so it could pass the closure through a function
//! with a `Fn(A) -> R` bound (e.g. `resolve1::<A, R>`) tied to the parameter's type.
//!
//! Expected: compiles. Actual: E0282 at `text`.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn TextField(on_change: impl Fn(String) + 'static) {
    on_change(String::new());
}

#[composable]
fn Screen(limit: usize) {
    TextField!(move |text| {
        let _ = text.len() > limit;
    });
}

#[test]
fn compiles() {}
