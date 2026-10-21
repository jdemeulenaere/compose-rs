//! Thirteenth adversarial review: `application { Window(…) }` (application.rs), headless, as
//! tests/application.rs drives it: the application's composition, and a `UiHost` showing a
//! window's content as the event loop's window does.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::rc::Rc;

use compose_desktop::{Application, Window, WindowHandle};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::mutable_state_of;
use compose_ui::{Density, FixedTextMeasurer, IntSize, UiHost};

/// A headless host showing `handle`'s content, as the event loop's window does.
fn window_host(handle: &Rc<WindowHandle>) -> UiHost {
    let mut host =
        UiHost::new(IntSize::new(400, 300), Density::default(), Box::new(FixedTextMeasurer::new()));
    let handle = handle.clone();
    host.set_content(move || handle.compose_content());
    host.settle(10);
    host
}

thread_local! {
    static COUNT: compose_runtime::OwnedState<i32> = mutable_state_of(0);
}

/// An application as a `#[composable]` function, as `application`'s documentation asks.
#[compose_runtime::composable]
fn CountingApp() {
    let count = COUNT.with(|count| count.get());
    Window!(on_close_request = || {}, title = format!("Count {count}"), {
        Column!({
            BasicText!("a");
            Row!({
                BasicText!("b");
                BasicText!("c");
            });
        });
    });
}

/// Input: an application (a `#[composable]` function, as `application`'s documentation asks)
/// whose window title reads state (`format!("Count {}", count)`) and whose content is static;
/// the count changes.
///
/// Compose Desktop: the content lambda captures nothing, so it is memoized: the title changes
/// and nothing in the window recomposes. (A plain closure given to `application` doesn't get
/// its lambdas memoized: documented on `application`.)
#[test]
fn a_title_change_recomposes_no_window_content() {
    let application = Application::new(CountingApp);
    let window = application.windows()[0].clone();
    let mut host = window_host(&window);
    COUNT.with(|count| count.set(1));
    assert!(application.recompose());
    assert_eq!(window.title(), "Count 1");
    let stats = host.frame();
    assert_eq!(
        stats.bodies_run, 0,
        "the window's content recomposed although only its title changed"
    );
}

/// Input: a `Window` composed inside another window's content (a settings window opened from
/// the main window's content, as Compose Desktop allows).
///
/// Compose Desktop: a `Window` may be called anywhere, a window's content included, and opens
/// another window. compose-rs documents it as unsupported (docs/11-status.md), so a clear error
/// is the least to expect.
///
/// Here: the window's composition panics with the runtime's generic "a node of a type the
/// applier does not handle was emitted", which names neither `Window` nor the fix (move it to
/// the `application`'s content). The same message comes for a non-window composable directly in
/// `application` (`BasicText!` there).
#[test]
fn a_window_in_a_windows_content_fails_with_an_error_naming_window() {
    let application = Application::new(|| {
        Window!(on_close_request = || {}, title = "Main".to_string(), {
            Window!(on_close_request = || {}, title = "Settings".to_string(), {
                BasicText!("Settings")
            });
        });
    });
    let window = application.windows()[0].clone();
    let Err(panic) =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(window_host(&window))))
    else {
        panic!("a Window in a window's content is unsupported");
    };
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default();
    assert!(message.contains("Window"), "the error doesn't say what's wrong: {message:?}");
}
