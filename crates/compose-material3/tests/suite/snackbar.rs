//! Snackbars: shown one at a time by `show_snackbar`, dismissed after their duration or by
//! their action, in the scaffold's slot.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;

thread_local! {
    static RESULTS: RefCell<Vec<SnackbarResult>> = const { RefCell::new(Vec::new()) };
}

fn screen(host: SnackbarHostState, scope: Rc<RefCell<Option<CoroutineScope>>>) -> impl Fn() {
    move || {
        let coroutines = remember_coroutine_scope();
        *scope.borrow_mut() = Some(coroutines);
        MaterialTheme!({
            Scaffold!(
                snackbar_host = move || SnackbarHost!(host),
                floating_action_button = || {
                    FloatingActionButton!(|| {}, modifier = Modifier.test_tag("fab"), {});
                },
                |_padding| {}
            );
        });
    }
}

fn show(
    scope: &Rc<RefCell<Option<CoroutineScope>>>,
    host: &SnackbarHostState,
    message: &str,
    action: Option<&str>,
) {
    let (host, message, action) = (*host, message.to_string(), action.map(str::to_string));
    scope.borrow().as_ref().unwrap().launch(async move {
        let result = host
            .show_snackbar_with(message, action.map(Into::into), false, SnackbarDuration::Short)
            .await;
        RESULTS.with(|results| results.borrow_mut().push(result));
    });
}

#[test]
fn snackbars_show_one_at_a_time_for_their_duration() {
    RESULTS.with(|results| results.borrow_mut().clear());
    let host = SnackbarHostState::new();
    let scope = Rc::new(RefCell::new(None));
    let mut ui = ComposeTestRule::with_manual_clock(screen(*host, scope.clone()));
    show(&scope, &host, "First", None);
    show(&scope, &host, "Second", Some("Undo"));
    ui.advance_time_by(Duration::from_millis(500));
    ui.on_node_with_text("First").assert_exists();
    ui.on_node_with_text("Second").assert_does_not_exist();
    // Above the FAB, centered.
    let snackbar = ui.on_node_with_text("First").fetch_semantics_node().bounds;
    let fab = ui.on_node_with_tag("fab").fetch_semantics_node().bounds;
    assert!(snackbar.bottom <= fab.top, "{snackbar:?} {fab:?}");
    // Four seconds on, the first is dismissed and the second shows.
    ui.advance_time_by(Duration::from_millis(4000));
    assert_eq!(RESULTS.with(|results| results.borrow().clone()), vec![SnackbarResult::Dismissed]);
    ui.on_node_with_text("Second").assert_exists();
    // Its action ends it as performed.
    ui.on_node_with_text("Undo").perform_click();
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(
        RESULTS.with(|results| results.borrow().clone()),
        vec![SnackbarResult::Dismissed, SnackbarResult::ActionPerformed]
    );
    ui.on_node_with_text("Second").assert_does_not_exist();
    assert!(host.current_snackbar_data().is_none());
}

#[test]
fn cancelling_show_snackbar_takes_it_away_and_lets_the_next_show() {
    RESULTS.with(|results| results.borrow_mut().clear());
    let host = SnackbarHostState::new();
    let scope = Rc::new(RefCell::new(None));
    let mut ui = ComposeTestRule::with_manual_clock(screen(*host, scope.clone()));
    let job = {
        let host = *host;
        scope.borrow().as_ref().unwrap().launch(async move {
            host.show_snackbar_with("Cancelled", None, false, SnackbarDuration::Indefinite).await;
        })
    };
    show(&scope, &host, "Next", None);
    ui.advance_time_by(Duration::from_millis(500));
    ui.on_node_with_text("Cancelled").assert_exists();
    job.cancel();
    ui.advance_time_by(Duration::from_millis(500));
    ui.on_node_with_text("Cancelled").assert_does_not_exist();
    ui.on_node_with_text("Next").assert_exists();
}

/// `showSnackbar(message)`: a message alone is shown for `Short`, without an action.
#[test]
fn a_message_alone_is_a_short_snackbar() {
    let visuals = SnackbarVisuals::from("Saved");
    assert_eq!(visuals.message.as_str(), "Saved");
    assert_eq!(visuals.action_label, None);
    assert!(!visuals.with_dismiss_action);
    assert_eq!(visuals.duration, SnackbarDuration::Short);
}
