//! Compose's `ProgressIndicatorTest.kt`, adapted: the indicators' progress semantics, sizes and
//! animations. Not ported: the three `*_semanticsNodeBounds`/`scrollingBounds` tests, where
//! Compose's semantics bounds are 10 dp taller than the line on each side
//! (`IncreaseVerticalSemanticsBounds`): compose-rs's semantics bounds are a node's.

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{Modifier, ProgressBarRangeInfo, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

const INDETERMINATE: ProgressBarRangeInfo =
    ProgressBarRangeInfo { current: 0.0, range: (0.0, 0.0), steps: 0 };

fn determinate(current: f32) -> ProgressBarRangeInfo {
    ProgressBarRangeInfo { current, range: (0.0, 1.0), steps: 0 }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, nonMaterialSetContent
#[test]
fn an_indicator_shows_outside_a_material_theme() {
    let mut ui = compose_test(|| {
        LinearProgressIndicator!(|| 0.0, modifier = Modifier.test_tag("linear"));
    });
    ui.on_node_with_tag("linear").assert_exists();
}

/// An indicator of `progress` (a state the lambda reads), tagged "indicator".
fn reading(linear: bool, initial: f32) -> (ComposeTestRule, compose_runtime::OwnedState<f32>) {
    let progress = compose_runtime::mutable_state_of(initial);
    let handle = progress.handle();
    let ui = compose_test(move || {
        MaterialTheme!({
            let modifier = Modifier.test_tag("indicator");
            if linear {
                LinearProgressIndicator!(move || handle.get(), modifier = modifier);
            } else {
                CircularProgressIndicator!(move || handle.get(), modifier = modifier);
            }
        });
    });
    (ui, progress)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateLinearProgressIndicator_Progress
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateCircularProgressIndicator_Progress
#[test]
fn an_indicator_reports_the_progress_its_lambda_reads() {
    for linear in [true, false] {
        let (mut ui, progress) = reading(linear, 0.0);
        assert_eq!(range_info(&mut ui, "indicator"), determinate(0.0));
        progress.set(0.5);
        ui.wait_for_idle();
        assert_eq!(range_info(&mut ui, "indicator"), determinate(0.5));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateLinearProgressIndicator_ProgressIsCoercedInBounds
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateCircularProgressIndicator_ProgressIsCoercedInBounds
#[test]
fn an_indicator_reports_its_progress_between_0_and_1() {
    for linear in [true, false] {
        let (mut ui, progress) = reading(linear, -1.0);
        assert_eq!(range_info(&mut ui, "indicator"), determinate(0.0));
        progress.set(1.5);
        ui.wait_for_idle();
        assert_eq!(range_info(&mut ui, "indicator"), determinate(1.0));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateLinearProgressIndicator_Size
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateCircularProgressIndicator_Size
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateLinearProgressIndicator_visualBounds
#[test]
fn determinate_indicators_are_240_by_4_and_40_dp() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                Box!(modifier = Modifier.test_tag("linear"), {
                    LinearProgressIndicator!(|| 0.0);
                });
                Box!(modifier = Modifier.test_tag("circular"), {
                    CircularProgressIndicator!(|| 0.0);
                });
            });
        });
    });
    let (linear, circular) = (bounds(&mut ui, "linear"), bounds(&mut ui, "circular"));
    assert_eq!((linear.width(), linear.height()), (240.0, 4.0));
    assert_eq!((circular.width(), circular.height()), (40.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, indeterminateLinearProgressIndicator_Progress
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, indeterminateCircularProgressIndicator_progress
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, indeterminateLinearProgressIndicator_Size
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, indeterminateCircularProgressIndicator_Size
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, indeterminateLinearProgressIndicator_visualBounds
#[test]
fn indeterminate_indicators_report_no_progress_and_keep_their_sizes() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            Column!({
                Box!(modifier = Modifier.test_tag("linear"), {
                    IndeterminateLinearProgressIndicator!(modifier = Modifier.test_tag("line"));
                });
                Box!(modifier = Modifier.test_tag("circular"), {
                    IndeterminateCircularProgressIndicator!(modifier = Modifier.test_tag("arc"));
                });
            });
        });
    });
    ui.advance_time_by_frame();
    assert_eq!(range_info(&mut ui, "line"), INDETERMINATE);
    assert_eq!(range_info(&mut ui, "arc"), INDETERMINATE);
    let (linear, circular) = (bounds(&mut ui, "linear"), bounds(&mut ui, "circular"));
    assert_eq!((linear.width(), linear.height()), (240.0, 4.0));
    assert_eq!((circular.width(), circular.height()), (40.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateLinearProgressIndicator_sizeModifier
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, indeterminateLinearProgressIndicator_sizeModifier
#[test]
fn a_size_modifier_sizes_a_linear_indicator() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        Column!({
            Box!(modifier = Modifier.test_tag("determinate"), {
                LinearProgressIndicator!(|| 1.0, modifier = Modifier.size((100.dp(), 10.dp())));
            });
            Box!(modifier = Modifier.test_tag("indeterminate"), {
                IndeterminateLinearProgressIndicator!(
                    modifier = Modifier.size((100.dp(), 10.dp()))
                );
            });
        });
    });
    ui.advance_time_by(std::time::Duration::from_millis(100));
    for tag in ["determinate", "indeterminate"] {
        let bounds = bounds(&mut ui, tag);
        assert_eq!((bounds.width(), bounds.height()), (100.0, 10.0), "{tag}");
    }
}

fn range_info(ui: &mut ComposeTestRule, tag: &str) -> ProgressBarRangeInfo {
    ui.on_node_with_tag(tag)
        .fetch_semantics_node()
        .config
        .progress_bar_range_info
        .expect("a progress bar range")
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateLinearProgressIndicator_NaNProgress
#[test]
fn a_linear_indicator_reports_a_nan_progress_as_zero() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            LinearProgressIndicator!(|| f32::NAN, modifier = Modifier.test_tag("linear"));
        });
    });
    assert_eq!(
        range_info(&mut ui, "linear"),
        ProgressBarRangeInfo { current: 0.0, range: (0.0, 1.0), steps: 0 }
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ProgressIndicatorTest.kt, determinateCircularProgressIndicator_NaNProgress
#[test]
fn a_circular_indicator_reports_a_nan_progress_as_zero() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            CircularProgressIndicator!(|| f32::NAN, modifier = Modifier.test_tag("circular"));
        });
    });
    assert_eq!(
        range_info(&mut ui, "circular"),
        ProgressBarRangeInfo { current: 0.0, range: (0.0, 1.0), steps: 0 }
    );
}
