//! `application { Window(…) }`'s composition, headless: the windows it has, their properties,
//! close requests, and their content following the application's state.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::rc::Rc;

use compose_desktop::{Application, Window, WindowHandle, WindowSize, remember_window_state};
use compose_foundation::*;
use compose_runtime::{OwnedState, mutable_state_of};
use compose_ui::{
    Density, IntSize,
    platform::{FixedTextMeasurer, UiHost},
};

/// A headless host showing `handle`'s content, as the event loop's window does.
fn window_host(handle: &Rc<WindowHandle>) -> UiHost {
    let mut host =
        UiHost::new(IntSize::new(400, 300), Density::default(), Box::new(FixedTextMeasurer::new()));
    let handle = handle.clone();
    host.set_content(move || handle.compose_content());
    host.settle(10);
    host
}

fn texts(host: &UiHost) -> Vec<String> {
    host.semantics_tree(true)
        .flatten()
        .iter()
        .flat_map(|node| node.config.text.iter().map(|text| text.to_string()).collect::<Vec<_>>())
        .collect()
}

#[test]
fn windows_come_and_go_with_the_composition() {
    let open: Rc<OwnedState<Vec<&'static str>>> = Rc::new(mutable_state_of(vec!["Main"]));
    let windows = open.clone();
    let application = Application::new(move || {
        for title in windows.get() {
            let windows = windows.clone();
            compose_runtime::key(title, move || {
                let windows = windows.clone();
                Window!(
                    on_close_request = move || {
                        windows.set(windows.get().into_iter().filter(|t| *t != title).collect())
                    },
                    title = title.to_string(),
                    { BasicText!("{title}") }
                );
            });
        }
    });
    let titles = |application: &Application| -> Vec<String> {
        application.windows().iter().map(|window| window.title()).collect()
    };
    assert_eq!(titles(&application), vec!["Main"]);
    assert!(application.windows()[0].is_visible());
    open.set(vec!["Main", "Settings"]);
    assert!(application.recompose());
    assert_eq!(titles(&application), vec!["Main", "Settings"]);
    // The settings window's close button: its close request removes it.
    application.windows()[1].request_close();
    assert!(application.recompose());
    assert_eq!(titles(&application), vec!["Main"]);
}

#[test]
fn a_windows_content_follows_the_applications_state() {
    let count = Rc::new(mutable_state_of(0));
    let read = count.clone();
    let application = Application::new(move || {
        let read = read.clone();
        Window!(on_close_request = || {}, title = format!("Count {}", read.get()), {
            BasicText!("Clicked {} times", read.get())
        });
    });
    let window = application.windows()[0].clone();
    let mut host = window_host(&window);
    assert_eq!(texts(&host), vec!["Clicked 0 times"]);
    count.set(2);
    application.recompose();
    host.settle(10);
    assert_eq!(texts(&host), vec!["Clicked 2 times"]);
    assert_eq!(window.title(), "Count 2");
}

#[test]
fn a_window_state_holds_its_size() {
    let application = Application::new(|| {
        let state = remember_window_state(WindowSize { width: 640.0, height: 480.0 });
        Window!(on_close_request = || {}, state = state, { BasicText!("content") });
    });
    let window = application.windows()[0].clone();
    assert_eq!(window.window_state().size(), WindowSize { width: 640.0, height: 480.0 });
    window.window_state().set_size(WindowSize { width: 300.0, height: 200.0 });
    assert_eq!(window.window_state().size(), WindowSize { width: 300.0, height: 200.0 });
}
