//! Phase C sixth review: valid code that `#[composable]` fails to compile.
//!
//! `rewrite_call_macro_arguments` taints every new name before a lone `=` in a PascalCase macro's
//! arguments, since a user's macro may bind it (`Bind!(current = …)`): `Toggle!(enabled = …)`
//! taints `enabled` (not a local) for the lambdas in and after the call. A tainted name is still
//! *captured* (`Free::use_name` pushes it, to clone it into nested lambdas), so a lambda in which
//! `enabled` appears gets a key `ArgProbe(&enabled)`. In a known macro's tokens (`println!`,
//! `format!`, `assert!`, `log::debug!`), `scan_tokens` takes every identifier for a use, field and
//! method names included: `settings.get().enabled` makes the probe name a value `enabled` that
//! doesn't exist. A named argument's name is usually a field name of the data it shows (`enabled`,
//! `checked`, `value`, `text`, `label`), so a handler logging that field after any call naming it
//! breaks. A lowercase user statement macro given `settings.enabled` (`trace_field!(…)`) taints
//! `enabled` the same way.
//!
//! Expected: compiles (a field or method name isn't a capture; a tainted name that isn't a value
//! makes the lambda changed without a key). Actual: E0425 "cannot find value `enabled` in this
//! scope", pointing at the field name inside `println!`.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[derive(Clone, Copy, PartialEq)]
struct Settings {
    enabled: bool,
}

#[composable]
fn Toggle(on_click: impl Fn() + 'static, #[default(true)] enabled: bool) {
    let _ = (on_click, enabled);
}

#[composable]
fn Screen(settings: MutableState<Settings>) {
    Toggle!(enabled = settings.get().enabled, on_click = move || {});
    Toggle!(on_click = move || println!("enabled: {}", settings.get().enabled));
}

#[test]
fn compiles() {}
