//! Phase C seventh review: valid code that `#[composable]` fails to compile.
//!
//! A callback given by name (`on_click = on_remove`) is cloned rather than moved, "so a content
//! lambda passing it on stays `Fn`" (docs/03, call macro rules). An optional callback given by
//! name isn't: `call!` passes any `Option` value as `optional_callback(value)`, which takes it by
//! value. Forwarding an optional slot is what a wrapper composable does (Material's
//! `OutlinedTextField` hands `label`, `placeholder` and the icons on to its decoration box; a
//! `LabeledField` hands its `label` to the field inside its `Card`), and a wrapper usually
//! forwards it from inside a content block, or to two callees.
//!
//! Expected: compiles, as the same code does with `label: impl Fn() + 'static`. Actual: E0507
//! "cannot move out of value, a captured variable in an `Fn` closure" in the card's content,
//! and E0382 "use of moved value: `label`" at the second field.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Card(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Field(#[default(None)] label: Option<impl Fn() + 'static>) {
    if let Some(label) = &label {
        label();
    }
}

#[composable]
fn LabeledField(#[default(None)] label: Option<impl Fn() + 'static>) {
    Card!({
        Field!(label = label);
    });
}

#[composable]
fn TwoFields(#[default(None)] label: Option<impl Fn() + 'static>) {
    Field!(label = label);
    Field!(label = label);
}

/// The control: the same forwarding with a required callback compiles.
#[composable]
fn RequiredField(label: impl Fn() + 'static) {
    let _ = label;
}

#[composable]
fn LabeledRequiredField(label: impl Fn() + 'static) {
    Card!({
        RequiredField!(label = label);
    });
    RequiredField!(label = label);
}

#[test]
fn compiles() {}
