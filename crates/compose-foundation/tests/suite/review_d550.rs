//! Adversarial review of D550 (Compose's `newLayoutWillBeDifferent`): intrinsic probes.

#![allow(non_snake_case)]

use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{FixedTextMeasurer, UiHost};
use compose_ui::{Constraints, Density, IntSize, Layout, measure_policy};

/// A parent asking its text child's `max_intrinsic_height` at the width it then measures it
/// at (Material's tabs and segmented buttons), measured again at every `tick`. In Compose the
/// intrinsic comes from a separate cache and the real measurement has the same constraints as
/// before, so nothing is laid out again; before D550 the probe and the real measurement had the
/// same width bounds, so nothing was either. Since D550 the probe's unbounded height and the
/// real one differ in height only, and a text cut at its max lines (or clipped by the real max
/// height) is laid out again twice (or once) at every measurement of the parent.
fn layouts_per_remeasure(text: &'static str, max_lines: u32, real_max_height: i32) -> usize {
    let tick = mutable_state_of(0);
    let ticks = tick.handle();
    let (measurer, counter) = CountingTextMeasurer::new(FixedTextMeasurer::new());
    let host = UiHost::new(IntSize::new(300, 300), Density::new(1.0), Box::new(measurer));
    let mut ui = ComposeTestRule::with_host(host, move || {
        Layout!(
            measure_policy = measure_policy(move |scope, measurables, _constraints| {
                let _ = ticks.get();
                let child = measurables[0];
                let _ = scope.max_intrinsic_height(child, 200);
                let placeable =
                    scope.measure(child, Constraints::new(200, 200, 0, real_max_height));
                scope.place(&placeable, 0, 0);
                scope.layout(200, placeable.height)
            }),
            {
                BasicText!(text, max_lines = max_lines);
            }
        );
    });
    counter.clear();
    for step in 1..=5 {
        tick.set(step);
        ui.wait_for_idle();
    }
    counter.of(text).total() / 5
}

#[test]
fn an_intrinsic_height_probe_lays_a_cut_text_out_again_at_every_measurement() {
    // Cut at one line: laid out for the probe and again for the real constraints.
    assert_eq!(layouts_per_remeasure("Cut\nlines", 1, 100), 0, "cut at max lines");
}

#[test]
fn an_intrinsic_height_probe_lays_a_clipped_text_out_again_at_every_measurement() {
    // Two lines (35 px) under a 10 px maximum height.
    assert_eq!(layouts_per_remeasure("Two\nlines", u32::MAX, 10), 0, "clipped by its height");
}

#[test]
fn an_intrinsic_height_probe_keeps_a_text_that_fits() {
    assert_eq!(layouts_per_remeasure("Fits", u32::MAX, 100), 0);
}
