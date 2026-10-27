//! `progress_semantics` and `Canvas` against Compose's `ProgressSemanticsTest` and `CanvasTest`.

#![allow(non_snake_case)]

use crate::interaction_helpers::SemanticsReads;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, Modifier, Paint, ProgressBarRangeInfo, UnitExt};

const TAG: &str = "linear";

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ProgressSemanticsTest.kt, determinateProgress_testSemantics
#[test]
fn a_determinate_progress_says_its_value_in_its_range() {
    let progress = mutable_state_of(0.0f32);
    let handle = progress.handle();
    let mut ui = compose_test(move || {
        Box!(
            modifier = Modifier
                .test_tag(TAG)
                .progress_semantics_with(handle.get(), (0.0, 1.0), 0)
                .size(50.dp())
                .background(Color::CYAN)
        );
    });
    let range = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag(TAG).fetch_semantics_node().config.progress_of()
    };
    for value in [0.0, 0.005, 0.5] {
        progress.set(value);
        ui.wait_for_idle();
        assert_eq!(
            range(&mut ui),
            Some(ProgressBarRangeInfo { current: value, range: (0.0, 1.0), steps: 0 })
        );
    }
    // Outside the range, kept in it.
    progress.set(2.0);
    ui.wait_for_idle();
    assert_eq!(range(&mut ui).map(|info| info.current), Some(1.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ProgressSemanticsTest.kt, indeterminateProgress_testSemantics
#[test]
fn an_indeterminate_progress_says_so() {
    let mut ui = compose_test(|| {
        Box!(
            modifier =
                Modifier.test_tag(TAG).progress_semantics().size(50.dp()).background(Color::CYAN)
        );
    });
    let config = ui.on_node_with_tag(TAG).fetch_semantics_node().config;
    assert_eq!(config.progress_of(), Some(ProgressBarRangeInfo::INDETERMINATE));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CanvasTest.kt, canvas_contentDescription
#[test]
fn a_canvas_is_described_by_its_content_description() {
    let mut ui = compose_test(|| {
        Canvas_with(Modifier.test_tag("canvas"), "cd", |_| {});
    });
    let config = ui.on_node_with_tag("canvas").fetch_semantics_node().config;
    assert_eq!(config.content_descriptions(), ["cd"]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CanvasTest.kt, canvas_noSize_emptyCanvas
#[test]
fn a_canvas_without_a_size_is_empty() {
    let mut ui = compose_test(|| {
        Canvas_with(Modifier.test_tag("canvas"), "empty", |_| {});
    });
    let bounds = ui.on_node_with_tag("canvas").fetch_semantics_node().bounds;
    assert_eq!((bounds.width(), bounds.height()), (0.0, 0.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CanvasTest.kt, canvas_exactSizes
#[test]
fn a_canvas_is_the_size_its_modifier_gives_and_draws_there() {
    let mut ui = compose_test(|| {
        Canvas(Modifier.size(100.dp()).test_tag("canvas"), |scope| {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(Color::RED));
        });
    });
    let bounds = ui.on_node_with_tag("canvas").fetch_semantics_node().bounds;
    assert_eq!((bounds.width(), bounds.height()), (100.0, 100.0));
    assert!(ui.dump_display_list().contains("#FFFF0000"), "{}", ui.dump_display_list());
}
