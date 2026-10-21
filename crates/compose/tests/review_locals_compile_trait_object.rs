//! Phase E review: valid code that fails to compile.
//!
//! Compose provides services through locals: `LocalUriHandler provides handler`, where the
//! value is an interface instance (`staticCompositionLocalOf` compares by reference, and
//! `compositionLocalOf`'s structural policy uses `equals`, which every object has). Here
//! `CompositionLocalProvider` requires `T: Clone + PartialEq`, so a local holding a trait
//! object (`Rc<dyn UriHandler>`, a callback) can be declared but never provided. A reference
//! equality fallback (`Rc::ptr_eq`, as `Callback` already does) would cover it.
//!
//! Expected: compiles. Actual: E0277, `dyn UriHandler == dyn UriHandler` is not implemented.

#![allow(non_snake_case, dead_code)]

use std::rc::Rc;

use compose::runtime::*;

trait UriHandler {
    fn open(&self, uri: &str);
}

struct Logger;

impl UriHandler for Logger {
    fn open(&self, _uri: &str) {}
}

fn no_handler() -> Rc<dyn UriHandler> {
    Rc::new(Logger)
}

static LOCAL_URI_HANDLER: CompositionLocal<Rc<dyn UriHandler>> =
    CompositionLocal::new("LocalUriHandler", no_handler);

#[composable]
fn Link() {
    LOCAL_URI_HANDLER.current().open("https://example.com");
}

#[composable]
fn Screen() {
    let handler: Rc<dyn UriHandler> = Rc::new(Logger);
    CompositionLocalProvider(LOCAL_URI_HANDLER.provides(handler), || Link());
}

#[test]
fn compiles() {}
