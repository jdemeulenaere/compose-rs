//! `run_compose_ui_test`, its config and failure pipeline, idling resources and
//! `has_pending_work`.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, Modifier, UnitExt};

#[composable]
fn ClickCounter(clicks: MutableState<i32>) {
    Column!({
        Box!(
            modifier = Modifier.clickable(move || clicks.update(|count| count + 1)).size(40.dp()),
            {
                BasicText!("Click me");
            }
        );
        BasicText!("Click count: {}", clicks.get());
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/ComposeUiTestTest.kt, testStateChange
#[test]
fn a_state_change_recomposes() {
    run_compose_ui_test(|ui| {
        let clicks = mutable_state_of(0);
        let handle = clicks.handle();
        ui.set_content(move || ClickCounter(handle));
        clicks.update(|count| count + 1);
        ui.on_node(has_text_containing("Click count")).assert_text_equals(&["Click count: 1"]);
        clicks.update(|count| count + 1);
        ui.on_node(has_text_containing("Click count")).assert_text_equals(&["Click count: 2"]);
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/ComposeUiTestTest.kt, testInputInjection
#[test]
fn input_changes_state() {
    run_compose_ui_test(|ui| {
        let clicks = mutable_state_of(0);
        let handle = clicks.handle();
        ui.set_content(move || ClickCounter(handle));
        ui.on_node_with_text("Click me").perform_click();
        ui.on_node(has_text_containing("Click count")).assert_text_equals(&["Click count: 1"]);
        ui.on_node_with_text("Click me").perform_click();
        ui.on_node(has_text_containing("Click count")).assert_text_equals(&["Click count: 2"]);
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/ComposeUiTestTest.kt, testAnimation
#[test]
fn an_animation_runs_to_its_end_while_waiting() {
    run_compose_ui_test(|ui| {
        let target = mutable_state_of(0.0f32);
        let handle = target.handle();
        ui.set_content(move || {
            let offset = animate_float_as_state!(handle.get());
            Box!(modifier = Modifier.fill_max_size(), {
                Box!(
                    modifier =
                        Modifier.size(10.dp()).offset(Dp(offset.get()), 0.dp()).test_tag("box")
                );
            });
        });
        ui.on_node_with_tag("box").assert_left_position_in_root_is_equal_to(0.dp());
        target.set(100.0);
        ui.on_node_with_tag("box").assert_left_position_in_root_is_equal_to(100.dp());
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/RootExistenceAssertTest.kt, noContent_assertDoesNotExist, noContent_queryMultipleAssertZero
/// Without content, nothing exists.
#[test]
fn without_content_nothing_exists() {
    run_compose_ui_test(|ui| {
        ui.on_node_with_tag("item").assert_does_not_exist();
        ui.on_all_nodes_with_tag("item").assert_count_equals(0);
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/FailurePipelineTest.kt, customHandler_isCalled_withArtifacts_onAssertionFailure
/// A failing body runs the failure pipeline: the handlers get the error and the UI hierarchy
/// (here no screenshot: this binary installs no capture renderer), then the failure goes on.
#[test]
fn a_failure_tells_the_handlers_with_the_hierarchy() {
    let captured: Rc<RefCell<Option<FailureContext>>> = Rc::default();
    let seen = captured.clone();
    let config = ComposeUiTestConfig {
        failure_policy: TestFailurePolicy {
            failure_handlers: vec![Rc::new(move |context: &FailureContext| {
                *seen.borrow_mut() = Some(context.clone());
            })],
            ..TestFailurePolicy::default()
        },
        ..ComposeUiTestConfig::default()
    };
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_compose_ui_test_with(config, |ui| {
            ui.set_content(|| Box!(modifier = Modifier.size(10.dp()).test_tag("my_box")));
            ui.on_node_with_tag("non_existent").assert_exists();
        })
    }));
    assert!(failed.is_err(), "the failure goes on");
    let context = captured.borrow().clone().expect("the handler was told");
    assert!(context.error.contains("non_existent"), "{}", context.error);
    assert_eq!(context.artifacts.len(), 1);
    let artifact = &context.artifacts[0];
    assert_eq!(artifact.kind, FailureArtifactType::UiHierarchy);
    assert!(artifact.file_name.ends_with("_ui.txt"));
    assert!(artifact.text.as_deref().is_some_and(|text| text.contains("Tag: 'my_box'")));
}

/// A disabled capture captures nothing; a handler's own panic doesn't hide the failure.
#[test]
fn disabled_captures_and_panicking_handlers() {
    let called = Rc::new(Cell::new(false));
    let seen = called.clone();
    let config = ComposeUiTestConfig {
        failure_policy: TestFailurePolicy {
            ui_hierarchy_capture_mode: CaptureMode::Disabled,
            screenshot_capture_mode: CaptureMode::Disabled,
            failure_handlers: vec![
                Rc::new(|_: &FailureContext| panic!("a broken handler")),
                Rc::new(move |context: &FailureContext| {
                    assert!(context.artifacts.is_empty());
                    seen.set(true);
                }),
            ],
        },
        ..ComposeUiTestConfig::default()
    };
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_compose_ui_test_with(config, |_| panic!("the test's failure"))
    }));
    let panic = failed.expect_err("the failure goes on");
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"the test's failure"));
    assert!(called.get());
}

/// The config's tolerance is the rule's.
#[test]
fn the_configs_tolerance_applies() {
    let config =
        ComposeUiTestConfig { bounds_assertion_tolerance: 2.dp(), ..ComposeUiTestConfig::default() };
    run_compose_ui_test_with(config, |ui| {
        ui.set_content(|| Box!(modifier = Modifier.size(10.dp()).test_tag("box")));
        ui.on_node_with_tag("box").assert_width_is_equal_to(11.5.dp());
    });
}

struct Busy(Rc<Cell<u32>>);

impl IdlingResource for Busy {
    fn is_idle_now(&self) -> bool {
        // Busy for the first three looks.
        let left = self.0.get();
        self.0.set(left.saturating_sub(1));
        left == 0
    }
    fn diagnostic_message_if_busy(&self) -> Option<String> {
        Some("loading".into())
    }
}

/// Waiting for idle waits for the idling resources too, and fails with their diagnostics when
/// they never idle.
#[test]
fn waiting_for_idle_waits_for_idling_resources() {
    let looks = Rc::new(Cell::new(3));
    let mut ui = compose_test(|| {});
    let resource: Rc<dyn IdlingResource> = Rc::new(Busy(looks.clone()));
    ui.register_idling_resource(resource.clone());
    ui.wait_for_idle();
    assert_eq!(looks.get(), 0, "it waited until the resource idled");
    assert_eq!(ui.current_time_millis(), 0, "the clock stayed");
    looks.set(u32::MAX);
    ui.set_idle_timeout(Duration::from_millis(30));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ui.wait_for_idle()));
    let message = failed
        .expect_err("never idle")
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(message.contains("ComposeNotIdleException") && message.contains("loading"), "{message}");
    ui.unregister_idling_resource(&resource);
    ui.wait_for_idle();
}

// Compose: compose/ui/ui-test/samples/src/main/java/androidx/compose/ui/test/samples/HasPendingWorkSample.kt, hasPendingWorkSample
/// With the clock advanced by hand, `has_pending_work` says whether an animation is still
/// under way, without doing any of it.
#[test]
fn pending_work_drives_a_frame_by_frame_loop() {
    let expanded = mutable_state_of(false);
    let handle = expanded.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let height = animate_float_as_state!(if handle.get() { 100.0 } else { 10.0 });
        Box!(modifier = Modifier.size((10.dp(), Dp(height.get()))).test_tag("card"));
    });
    assert!(!ui.has_pending_work());
    expanded.set(true);
    assert!(ui.has_pending_work());
    let mut frames = 0;
    let mut heights = Vec::new();
    while ui.has_pending_work() {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        heights.push(ui.run_without_implicit_wait(|ui| {
            ui.on_node_with_tag("card").get_unclipped_bounds_in_root().height()
        }));
        frames += 1;
        assert!(frames < 200, "the animation ends");
    }
    assert!(heights.iter().any(|height| height.0 > 10.0 && height.0 < 100.0), "{heights:?}");
    assert_eq!(*heights.last().unwrap(), 100.dp());
}
