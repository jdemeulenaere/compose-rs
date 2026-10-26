//! Compose's `ProgressIndicatorTest.kt`, adapted: the indicators' progress semantics, sizes and
//! animations.

use compose_material3::*;
use compose_test::*;
use compose_ui::{Modifier, ProgressBarRangeInfo};

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
            LinearProgressIndicator!(f32::NAN, modifier = Modifier.test_tag("linear"));
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
            CircularProgressIndicator!(f32::NAN, modifier = Modifier.test_tag("circular"));
        });
    });
    assert_eq!(
        range_info(&mut ui, "circular"),
        ProgressBarRangeInfo { current: 0.0, range: (0.0, 1.0), steps: 0 }
    );
}
