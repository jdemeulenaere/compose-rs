//! Layouts inside `ReusableContent`: reused with a new key, they aren't measured again (their
//! measure policies are kept). Adapted from Compose's `LayoutReuseTest`.

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Alignment, Constraints, IntSize, UnitExt};

/// `content` (given a modifier counting its measurements) in a `ReusableContent` whose key
/// changes: asserts the reuse measures nothing again.
fn assert_no_remeasure_on_reuse(content: impl Fn(Modifier) + 'static) {
    let count = Rc::new(Cell::new(0u32));
    let key = mutable_state_of(0u32);
    let handle = key.handle();
    let counted = count.clone();
    // Made once, as the test's `layoutModifier`: the same modifier each composition.
    let counting = Modifier.layout(move |scope, measurable, constraints: Constraints| {
        counted.set(counted.get() + 1);
        let placeable = scope.measure(measurable, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    });
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        ReusableContent(handle.get(), || content(counting.clone()));
    });
    ui.wait_for_idle();
    assert!(count.get() > 0, "the content wasn't measured");
    count.set(0);
    key.set(1);
    ui.wait_for_idle();
    assert_eq!(count.get(), 0, "the reused content was measured again");
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseBox
#[test]
fn a_reused_box_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| Box!(modifier = modifier.size(10.dp())));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseBoxWithNonDefaultAlignment
#[test]
fn a_reused_centering_box_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| {
        Box!(modifier = modifier.size(10.dp()), content_alignment = Alignment::CENTER, {});
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseRow
#[test]
fn a_reused_row_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| Row!(modifier = modifier.size(10.dp()), |_| {}));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseRowWithNonDefaultAlignment
#[test]
fn a_reused_centering_row_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| {
        Row!(
            modifier = modifier.size(10.dp()),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            |_| {}
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseColumn
#[test]
fn a_reused_column_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| Column!(modifier = modifier.size(10.dp()), |_| {}));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseColumnWithNonDefaultAlignment
#[test]
fn a_reused_centering_column_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| {
        Column!(
            modifier = modifier.size(10.dp()),
            horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
            |_| {}
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseFlowRow
#[test]
fn a_reused_flow_row_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| FlowRow!(modifier = modifier.size(10.dp()), |_| {}));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseFlowColumn
#[test]
fn a_reused_flow_column_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| FlowColumn!(modifier = modifier.size(10.dp()), |_| {}));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt, reuseSpacer
#[test]
fn a_reused_spacer_isnt_measured_again() {
    assert_no_remeasure_on_reuse(|modifier| Spacer!(modifier = modifier.size(10.dp())));
}
