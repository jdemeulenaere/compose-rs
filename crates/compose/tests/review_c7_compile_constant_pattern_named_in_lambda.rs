//! Phase C seventh review: valid code that `#[composable]` fails to compile.
//!
//! In a refutable pattern (a `match` arm, `if let`), a PascalCase identifier may be a constant, a
//! unit variant or a binding, so the walk binds it *tainted*, and a lambda in the arm naming it
//! counts as always changed (docs/03, step 2). It is still a capture, and since D115 every
//! capture is rebound in the lambda's builder: `let LIMIT = Clone::clone(&LIMIT);`. When the name
//! is a constant or a unit variant (the usual case: `MAX => …`, or an enum's variants imported
//! with `use Tab::*`), that `let` is itself a refutable pattern. Before D115 a top-level lambda
//! only took a key, `ArgProbe(&LIMIT)`, which compiled; now every such lambda fails, content
//! blocks naming the constant in a format string included.
//!
//! Expected: compiles. Actual: E0005 "refutable pattern in local binding" at `LIMIT` in the
//! button's handler, at `LIMIT` in the label's format string, and at `Home` in the tab's handler.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Button(on_click: impl Fn() + 'static) {
    let _ = on_click;
}

#[composable]
fn Section(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Label(#[format] text: String) {
    let _ = text;
}

const LIMIT: u32 = 10;

#[composable]
fn Counter(count: MutableState<u32>) {
    match count.get() {
        LIMIT => {
            Section!({
                Label!("You reached {LIMIT}");
            });
            Button!(on_click = move || count.set(LIMIT - 1));
        }
        _ => Button!(on_click = move || count.set(count.get() + 1)),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tab {
    Home,
    Settings,
}

use Tab::*;

#[composable]
fn TabBar(tab: MutableState<Tab>) {
    match tab.get() {
        // Tapping the selected tab again resets it.
        Home => Button!(on_click = move || tab.set(Home)),
        Settings => Button!(on_click = move || tab.set(Home)),
    }
}

#[test]
fn compiles() {}
