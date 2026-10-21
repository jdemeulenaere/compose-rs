//! A library of composables, as a widget crate would be.

#![allow(non_snake_case)]

use std::cell::RefCell;

use compose::runtime::*;

thread_local! {
    pub static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub fn log(entry: String) {
    LOG.with(|log| log.borrow_mut().push(entry));
}

/// A small composable a library marks `#[inline]`: remembers a value, and reads `tick`, so it
/// restarts on its own.
#[composable]
#[inline]
pub fn Inlined(label: &'static str, tick: MutableState<u32>) {
    remember(|| log(format!("create {label}")));
    let tick = tick.get();
    log(format!("run {label} {tick}"));
}

/// The same, `#[inline(always)]`.
#[composable]
#[inline(always)]
pub fn InlinedAlways(label: &'static str, tick: MutableState<u32>) {
    remember(|| log(format!("create {label}")));
    let tick = tick.get();
    log(format!("run {label} {tick}"));
}

/// The same, generic.
#[composable]
pub fn Generic<T: ComposeArg + Clone + std::fmt::Debug + 'static>(label: T, tick: MutableState<u32>) {
    remember(|| log(format!("create {label:?}")));
    let tick = tick.get();
    log(format!("run {label:?} {tick}"));
}

/// The same, plain.
#[composable]
pub fn Plain(label: &'static str, tick: MutableState<u32>) {
    remember(|| log(format!("create {label}")));
    let tick = tick.get();
    log(format!("run {label} {tick}"));
}

/// Calls the same composables from inside the library (so it instantiates `Generic::<u8>`).
#[composable]
pub fn Section(tick: MutableState<u32>) {
    Inlined("section inlined", tick);
    InlinedAlways("section always", tick);
    Generic(9u8, tick);
}

#[doc(hidden)]
pub fn noop() {}

#[doc(hidden)]
pub const DEFAULT_LABEL: &str = "clicker";

/// Defaults naming items of this crate through `crate::`: they resolve here, wherever the call
/// macro is used. Like any macro's, they must be public (hidden from docs is fine).
#[composable]
pub fn Clicker(
    #[default(crate::DEFAULT_LABEL)] label: &'static str,
    #[default(crate::noop)] on_click: impl Fn() + 'static,
) {
    on_click();
    let _ = label;
}
