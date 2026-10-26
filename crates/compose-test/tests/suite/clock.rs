//! The main clock and idling: frames on the test clock, Compose's rounding, waiting, and the
//! infinite-animation policy (D900, D901).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, PaddingValues, UnitExt};

#[composable]
fn Counts(count1: MutableState<i32>, count2: MutableState<i32>, sum: State<i32>) {
    LaunchedEffect((), async move {
        delay(Duration::from_millis(100)).await;
        count1.update(|count| count + 1);
        delay(Duration::from_millis(100)).await;
        count2.update(|count| count + 1);
    });
    Column!({
        BasicText!("Count1 = {}", count1.get());
        BasicText!("Count2 = {}", count2.get());
        BasicText!("Sum = {}", sum.get());
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/junit4/ApplySnapshotImmediatelyTest.kt, test
/// State written by a task shows at the next frame of the test clock, 16 ms after the write,
/// whatever the advances' lengths.
#[test]
fn state_written_between_frames_shows_at_the_next_frame() {
    let count1 = mutable_state_of(0);
    let count2 = mutable_state_of(0);
    let (one, two) = (count1.handle(), count2.handle());
    let sum = derived_state_of(move || one.get() + two.get());
    let sum_handle = sum.as_state();
    let mut ui = ComposeTestRule::empty(default_host());
    ui.set_auto_advance(false);
    ui.set_content(move || Counts(one, two, sum_handle));
    let state = |expected: (i32, i32, i32)| {
        assert_eq!((count1.get(), count2.get(), sum.get()), expected, "the states");
    };
    let display = |ui: &mut ComposeTestRule, (a, b, s): (i32, i32, i32)| {
        ui.on_node_with_text(&format!("Count1 = {a}")).assert_is_displayed();
        ui.on_node_with_text(&format!("Count2 = {b}")).assert_is_displayed();
        ui.on_node_with_text(&format!("Sum = {s}")).assert_is_displayed();
    };
    state((0, 0, 0));
    display(&mut ui, (0, 0, 0));
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(99));
    state((0, 0, 0));
    display(&mut ui, (0, 0, 0));
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(1));
    state((1, 0, 1));
    display(&mut ui, (0, 0, 0));
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(15));
    state((1, 0, 1));
    display(&mut ui, (0, 0, 0));
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(1));
    state((1, 0, 1));
    display(&mut ui, (1, 0, 1));
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(100 - 16));
    state((1, 1, 2));
    display(&mut ui, (1, 0, 1));
    ui.advance_time_by_frame();
    state((1, 1, 2));
    display(&mut ui, (1, 1, 2));
}

// Compose: compose/ui/ui-test/src/commonMain/kotlin/androidx/compose/ui/test/AbstractMainTestClock.kt, advanceTimeBy (its rounding)
/// `advance_time_by` rounds up to whole frames; ignoring the frame duration, it doesn't.
#[test]
fn advancing_rounds_up_to_whole_frames_unless_told_not_to() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    assert_eq!(ui.current_time_millis(), 0, "the clock starts at 0");
    ui.advance_time_by(Duration::from_millis(1));
    assert_eq!(ui.current_time_millis(), 16);
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(ui.current_time_millis(), 16 + 112);
    ui.advance_time_by(Duration::ZERO);
    assert_eq!(ui.current_time_millis(), 128);
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(5));
    assert_eq!(ui.current_time_millis(), 133);
    ui.advance_time_by_frame();
    assert_eq!(ui.current_time_millis(), 149);
}

/// A task's frames come 16 ms apart whatever the lengths of the advances: four 15 ms advances
/// still run three frames.
#[test]
fn frames_keep_their_cadence_across_short_advances() {
    let frames = Rc::new(Cell::new(0));
    let counted = frames.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let counted = counted.clone();
        LaunchedEffect((), async move {
            loop {
                with_frame_nanos(|_| ()).await;
                counted.set(counted.get() + 1);
            }
        });
    });
    for _ in 0..4 {
        ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(15));
    }
    assert_eq!(frames.get(), 3, "frames at 16, 32 and 48 ms within 60 ms");
}

// Compose: compose/ui/ui-test/src/commonMain/kotlin/androidx/compose/ui/test/AbstractMainTestClock.kt, advanceTimeUntil
/// `advance_time_until` moves the clock a frame at a time until the condition holds, and fails
/// past its timeout.
#[test]
fn advancing_until_a_condition_holds_stops_there_or_times_out() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    ui.advance_time_until(|ui| ui.current_time_millis() >= 100);
    assert_eq!(ui.current_time_millis(), 112);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.advance_time_until_with_timeout(Duration::from_millis(50), |_| false);
    }));
    let message = panic_message(failed.expect_err("times out"));
    assert!(message.contains("still not satisfied after 50 ms"), "{message}");
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

/// The auto-advancing clock runs frames while they're needed, and doesn't jump over delays,
/// as Compose's idling: a delay waits for the clock.
#[test]
fn waiting_for_idle_runs_frames_but_not_delays() {
    let done = Rc::new(Cell::new(false));
    let set = done.clone();
    let mut ui = compose_test(move || {
        let set = set.clone();
        LaunchedEffect((), async move {
            delay(Duration::from_secs(10)).await;
            set.set(true);
        });
    });
    ui.wait_for_idle();
    assert!(!done.get());
    assert_eq!(ui.current_time_millis(), 0, "nothing to recompose: no frame");
    ui.advance_time_by(Duration::from_secs(10));
    assert!(done.get());
}

#[composable]
fn TaggedBox() {
    Box!(modifier = Modifier.size(10.dp()).test_tag("TestTag"));
}

fn expect_timeout(action: impl FnOnce(), expected: &str) {
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action));
    let message = panic_message(failed.expect_err("a ComposeTimeoutException"));
    assert!(message.contains("ComposeTimeoutException"), "{message}");
    assert!(message.contains(expected), "{message}");
}

const TIMEOUT: Duration = Duration::from_millis(300);

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/junit4/ComposeTestRuleWaitUntilTest.kt, waitUntil_includesConditionDescription_whenSpecified
#[test]
fn waiting_until_names_the_condition_when_it_times_out() {
    let mut ui = compose_test(TaggedBox);
    expect_timeout(
        || ui.wait_until_with(Some("foo"), TIMEOUT, |_| false),
        "Condition (foo) still not satisfied after 300 ms",
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/junit4/ComposeTestRuleWaitUntilTest.kt, waitUntilNodeCount_succeedsWhen_nodeCountCorrect, waitUntilNodeCount_throwsWhen_nodeCountIncorrect
#[test]
fn waiting_until_a_node_count_succeeds_or_names_the_count() {
    let mut ui = compose_test(|| {
        TaggedBox();
        TaggedBox();
        TaggedBox();
    });
    let tagged = has_test_tag("TestTag");
    ui.wait_until_node_count(&tagged, 3, TIMEOUT, false);
    expect_timeout(
        || ui.wait_until_node_count(&tagged, 2, TIMEOUT, false),
        "Condition (exactly 2 nodes match (with tag \"TestTag\")) still not satisfied",
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/junit4/ComposeTestRuleWaitUntilTest.kt, waitUntilAtLeastOneExists_succeedsWhen_nodesExist, waitUntilAtLeastOneExists_throwsWhen_nodesDoNotExist, waitUntilExactlyOneExists_succeedsWhen_oneNodeExists, waitUntilExactlyOneExists_throwsWhen_twoNodesExist, waitUntilDoesNotExists_succeedsWhen_nodeDoesNotExist, waitUntilDoesNotExists_throwsWhen_nodeExistsUntilTimeout
#[test]
fn waiting_until_nodes_exist_or_not() {
    let tagged = has_test_tag("TestTag");
    let mut two = compose_test(|| {
        TaggedBox();
        TaggedBox();
    });
    two.wait_until_at_least_one_exists(&tagged, TIMEOUT, false);
    expect_timeout(
        || two.wait_until_exactly_one_exists(&tagged, TIMEOUT, false),
        "Condition (exactly 1 nodes match (with tag \"TestTag\"))",
    );
    let mut none = compose_test(|| Box!(modifier = Modifier.size(10.dp())));
    none.wait_until_does_not_exist(&tagged, TIMEOUT, false);
    expect_timeout(
        || none.wait_until_at_least_one_exists(&tagged, TIMEOUT, false),
        "Condition (at least one node matches (with tag \"TestTag\"))",
    );
    let mut one = compose_test(TaggedBox);
    one.wait_until_exactly_one_exists(&tagged, TIMEOUT, false);
    expect_timeout(
        || one.wait_until_does_not_exist(&tagged, TIMEOUT, false),
        "Condition (exactly 0 nodes match (with tag \"TestTag\"))",
    );
}

#[composable]
fn MergedParent(children: usize) {
    Box!(
        modifier = Modifier.semantics(|config| config.merge_descendants = true).test_tag("parent"),
        {
            for _ in 0..children {
                Box!(modifier = Modifier.size(1.dp()).test_tag("child"));
            }
        }
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/junit4/ComposeTestRuleWaitUntilTest.kt, waitUntilAtLeastOneExists_withUnmergedTree, waitUntilDoesNotExist_withUnmergedTree, waitUntilNodeCount_withUnmergedTree, waitUntilExactlyOneExists_withUnmergedTree
#[test]
fn waiting_until_looks_in_the_unmerged_tree_when_asked() {
    let child = has_test_tag("child");
    let mut ui = compose_test(|| MergedParent(1));
    expect_timeout(|| ui.wait_until_at_least_one_exists(&child, TIMEOUT, false), "");
    ui.wait_until_at_least_one_exists(&child, TIMEOUT, true);
    ui.wait_until_does_not_exist(&child, TIMEOUT, false);
    expect_timeout(|| ui.wait_until_does_not_exist(&child, TIMEOUT, true), "");
    expect_timeout(|| ui.wait_until_exactly_one_exists(&child, TIMEOUT, false), "");
    ui.wait_until_exactly_one_exists(&child, TIMEOUT, true);
    let mut two = compose_test(|| MergedParent(2));
    expect_timeout(|| two.wait_until_node_count(&child, 2, TIMEOUT, false), "");
    two.wait_until_node_count(&child, 2, TIMEOUT, true);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/junit4/SynchronizationMethodsTest.kt, runOnUiThread, runOnUiThread_void
/// `run_on_idle` waits for idle first; `run_on_ui_thread` runs at once; both return the
/// action's result.
#[test]
fn running_on_idle_waits_then_returns_the_result() {
    let shown = mutable_state_of(false);
    let handle = shown.handle();
    let mut ui = compose_test(move || {
        if handle.get() {
            BasicText!("shown");
        }
    });
    shown.set(true);
    let count = ui.run_on_ui_thread(|ui| {
        ui.run_without_implicit_wait(|ui| ui.count_nodes(&has_text("shown")))
    });
    assert_eq!(count, 0, "not recomposed yet");
    let count =
        ui.run_on_idle(|ui| ui.run_without_implicit_wait(|ui| ui.count_nodes(&has_text("shown"))));
    assert_eq!(count, 1);
}

/// A box moved right by `offset` dp of start padding, as Compose's `ClickableTestBox` with a
/// `padding(start = …)`.
#[composable]
fn ShiftedBox(offset: Dp) {
    Box!(modifier = Modifier.padding(PaddingValues { start: offset, ..PaddingValues::ZERO }), {
        Box!(modifier = Modifier.size(100.dp()).test_tag("box"));
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/InfiniteAnimationTest.kt, testInfiniteTransition_finishes
/// With the clock advancing by itself, an infinite transition doesn't keep the UI busy: it
/// doesn't run (D901).
#[test]
fn an_infinite_transition_lets_the_ui_idle() {
    let mut ui = compose_test(|| {
        let transition = remember_infinite_transition!();
        let offset = transition.animate_float(
            0.0,
            100.0,
            infinite_repeatable(tween(300), RepeatMode::Reverse),
        );
        ShiftedBox(Dp(offset.get()));
    });
    ui.run_on_idle(|_| ());
    ui.on_node_with_tag("box").assert_left_position_in_root_is_equal_to(0.dp());
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/InfiniteAnimationTest.kt, testInfiniteAnimate_finishes
/// The same for an infinite `animate`: an `Animatable` along `infinite_repeatable`.
#[test]
fn an_infinite_animatable_lets_the_ui_idle() {
    let mut ui = compose_test(|| {
        let offset = remember(|| Animatable::new(1.0f32));
        let animated = offset.clone();
        LaunchedEffect((), async move {
            animated.animate_to(0.0, infinite_repeatable(tween(300), RepeatMode::Reverse)).await;
        });
        ShiftedBox(Dp(offset.value()));
    });
    ui.run_on_idle(|_| ());
    ui.on_node_with_tag("box").assert_left_position_in_root_is_equal_to(1.dp());
}

/// Compose's `testInfiniteAnimation_autoAdvance`: an infinite transition of 0 to 100 dp over
/// 200 ms, linear; 8 dp a frame when it runs.
fn infinite_animation_runs(auto_advance: bool) {
    let mut ui = ComposeTestRule::empty(default_host());
    ui.set_auto_advance(auto_advance);
    ui.set_content(|| {
        let transition = remember_infinite_transition!();
        let offset = transition.animate_float(
            0.0,
            100.0,
            infinite_repeatable(tween(200).easing(LINEAR_EASING), RepeatMode::Reverse),
        );
        ShiftedBox(Dp(offset.get()));
    });
    // The first composition: the animation hasn't started.
    ui.on_node_with_tag("box").assert_left_position_in_root_is_equal_to(0.dp());
    ui.advance_time_by_frame();
    // Its first frame: at the start.
    ui.on_node_with_tag("box").assert_left_position_in_root_is_equal_to(0.dp());
    ui.advance_time_by_frame();
    // A frame on: 16 ms of 200 along 100 dp, if it runs.
    let expected = if auto_advance { 0.dp() } else { 8.dp() };
    ui.on_node_with_tag("box").assert_left_position_in_root_is_equal_to(expected);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/InfiniteAnimationTest.kt, testInfiniteAnimation_autoAdvancing_doesNotRun
#[test]
fn an_infinite_animation_doesnt_run_while_the_clock_advances_by_itself() {
    infinite_animation_runs(true);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/InfiniteAnimationTest.kt, testInfiniteAnimation_manualAdvancing_doesRun
#[test]
fn an_infinite_animation_runs_with_the_clock_advanced_by_hand() {
    infinite_animation_runs(false);
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/InfiniteAnimationPolicyTest.kt, withInfiniteAnimationFrameNanos_policyIsApplied, withInfiniteAnimationFrameMillis_policyIsApplied
/// Under a policy that stops infinite animations, waiting for an infinite animation's frame
/// cancels the task (its future is dropped, as a `CancellationException`).
#[test]
fn the_policy_cancels_a_task_waiting_for_an_infinite_frame() {
    struct Dropped(Rc<Cell<bool>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    for millis in [false, true] {
        let (dropped, ran) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let (guard, after) = (dropped.clone(), ran.clone());
        let mut ui = compose_test(move || {
            let (guard, after) = (guard.clone(), after.clone());
            LaunchedEffect((), async move {
                let _guard = Dropped(guard);
                if millis {
                    with_infinite_animation_frame_millis(|_| ()).await;
                } else {
                    with_infinite_animation_frame_nanos(|_| ()).await;
                }
                after.set(true);
            });
        });
        ui.wait_for_idle();
        assert!(dropped.get(), "the task was cancelled (millis: {millis})");
        assert!(!ran.get(), "the frame never came (millis: {millis})");
    }
}
