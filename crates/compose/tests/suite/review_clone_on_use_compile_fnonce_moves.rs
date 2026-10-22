//! Clone-on-use review: rule (b) ("a `move` closure or an `async move` block in a closure gets
//! clones of what it names from outside that closure") also applies inside `FnOnce` closures,
//! where a move was valid. Each composable below compiled before D158.
//!
//! - `Receiving`: a `DisposableEffect` body (`FnOnce`) moving a non-`Clone` value into the
//!   thread it spawns. E0277 "`Receiver<u32>: Clone` is not satisfied", "required by a bound in
//!   `compose::__private::reuse`".
//! - `MapsToken`: a click handler's own local moved through `Option::map` into a `move` closure.
//!   docs/03 says "What a lambda makes for itself (its locals and parameters) moves as written";
//!   it is cloned because the rule counts from the innermost closure, not the memoized lambda.
//!   E0277 "`Token: Clone` is not satisfied".
//! - `NonMoveOuter`: a non-`move` `SideEffect` closure whose inner `move` closure took `name` by
//!   value, which made the outer closure capture it by value. Now the outer one only borrows it
//!   for `reuse(&name)`: E0373 "closure may outlive the current function, but it borrows `name`".
//!
//! Expected: compiles (restrict rule (b) to the captures of the enclosing memoized lambda, which
//! are `Clone` already and can't be moved anyway). Actual: the errors above.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Clickable(on_click: impl Fn() + 'static) {
    let _ = on_click;
}

#[composable(non_restartable)]
fn Receiving(events: std::sync::mpsc::Receiver<u32>) {
    DisposableEffect((), move |scope| {
        let worker = std::thread::spawn(move || {
            for event in events {
                let _ = event;
            }
        });
        scope.on_dispose(move || drop(worker))
    });
}

struct Token;

fn consume(_: Token, _: u32) {}

#[composable]
fn MapsToken() {
    Clickable!(
        on_click = move || {
            let token = Token;
            let _ = Some(1).map(|n| {
                let run = move || consume(token, n);
                run();
            });
        }
    );
}

fn report(_: String) {}

#[composable]
fn NonMoveOuter(name: String) {
    SideEffect(|| {
        let run = move || report(name);
        run();
    });
}

#[test]
fn compiles() {}
