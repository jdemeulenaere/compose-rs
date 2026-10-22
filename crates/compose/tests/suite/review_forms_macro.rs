//! Review of D232: `impl Into<T>` parameters of restartable composables.

#![allow(non_snake_case, dead_code)]

use std::cell::RefCell;

use compose::runtime::platform::*;
use compose::runtime::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[derive(Clone, PartialEq, Debug)]
struct Meters(f32);
impl From<u32> for Meters {
    fn from(value: u32) -> Meters {
        Meters(value as f32)
    }
}
impl ComposeArg for Meters {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Converting is compared by `T`'s rules: the same `T` from a different source value skips.
#[composable]
fn Logged(length: impl Into<Meters>) {
    log(format!("{length:?}"));
}

#[test]
fn a_converted_argument_skips_by_the_targets_rules() {
    let value = mutable_state_of(30u32);
    let handle = value.handle();
    let composition = Composition::new(NodeTree::<()>::new());
    take_log();
    composition.set_content(move || {
        Logged!(handle.get() / 10);
    });
    assert_eq!(take_log(), ["Meters(3.0)"]);
    value.set(31);
    composition.recompose();
    assert_eq!(take_log(), Vec::<String>::new(), "same Meters: skipped");
}

/// A default of the source type, as the generic form takes: converted as a given argument is.
#[composable]
fn DefaultOfSource(#[default(5u32)] length: impl Into<Meters>) {
    log(format!("{length:?}"));
}

/// A generic target: `Generic!(Meters(1.0))` is E0283 (the `Into::into` target can't be
/// inferred); `Generic::<Meters>(…)` called as a function compiles.
#[cfg(any())]
#[composable]
fn Generic<T: ComposeArg + Clone + std::fmt::Debug + 'static>(value: impl Into<T>) {
    log(format!("{value:?}"));
}

#[test]
fn a_default_of_the_source_type_is_converted() {
    let composition = Composition::new(NodeTree::<()>::new());
    take_log();
    composition.set_content(|| {
        DefaultOfSource!();
        DefaultOfSource!(length = 2u32);
    });
    assert_eq!(take_log(), ["Meters(5.0)", "Meters(2.0)"]);
}

#[cfg(any())]
#[composable]
fn CallsGeneric() {
    Generic::<Meters>(Meters(1.0));
    Generic!(Meters(1.0));
}

/// E0283 "type annotations needed" (documented in D232).
#[cfg(any())]
#[composable]
fn BadArgument() {
    let x: u32 = 3;
    Logged!(x.into());
}
