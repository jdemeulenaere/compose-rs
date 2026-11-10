//! Compose's `RtlLayoutTest` (ui, device tests): placing absolutely and relatively in either
//! direction, a direction change measuring again, the direction a measure scope sees, a
//! direction provided back, and a child in padding and a minimum touch target, right to left.
//! Density 1. `testModifiedLayoutDirection_inIntrinsicsMeasure` is in compose-ui's
//! `intrinsics.rs`.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, DpSize, IntSize, LOCAL_LAYOUT_DIRECTION, Layout, LayoutCoordinates, LayoutDirection,
    Offset, UnitExt, measure_policy,
};

const SIZE: i32 = 100;

type Seen<T> = Rc<Cell<Option<T>>>;

/// Where a layout is in the window, and its width.
type Positioned = Seen<(Offset, i32)>;

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 400), content)
}

/// A leaf `size` pixels each way, as the test utilities' `FixedSize`.
fn fixed_size(size: i32, modifier: Modifier) {
    Layout(modifier, measure_policy(move |scope, _, _| scope.layout(size, size)), callback(|| {}));
}

/// Records its layout's position in the root, as the test's `saveLayoutInfo`.
fn save_layout_info(position: &Seen<Offset>) -> Modifier {
    let position = position.clone();
    Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
        position.set(Some(coordinates.local_to_root(Offset::ZERO)))
    })
}

/// Three 100-pixel leaves placed down a diagonal, absolutely or relatively, in `direction`, as
/// the test's `CustomLayout`; their positions in the root.
fn custom_layout(absolute: bool, direction: LayoutDirection) -> [Option<Offset>; 3] {
    let positions: [Seen<Offset>; 3] = Default::default();
    let seen = positions.clone();
    let mut ui = rule(move || {
        let seen = seen.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
            let seen = seen.clone();
            Layout(
                Modifier,
                measure_policy(move |scope, measurables, constraints| {
                    let placeables: Vec<_> =
                        measurables.iter().map(|&m| scope.measure(m, constraints)).collect();
                    let width = placeables.iter().map(|p| p.width).sum();
                    let height = placeables.iter().map(|p| p.height).sum();
                    let (mut x, mut y) = (0, 0);
                    for placeable in &placeables {
                        if absolute {
                            scope.place(placeable, x, y);
                        } else {
                            scope.place_relative(placeable, x, y);
                        }
                        x += placeable.width;
                        y += placeable.height;
                    }
                    scope.layout(width, height)
                }),
                callback(move || {
                    for position in &seen {
                        fixed_size(SIZE, save_layout_info(position));
                    }
                }),
            );
        });
    });
    ui.wait_for_idle();
    positions.map(|position| position.get())
}

fn at(x: i32, y: i32) -> Option<Offset> {
    Some(Offset::new(x as f32, y as f32))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, customLayout_absolutePositioning
#[test]
fn custom_layout_absolute_positioning() {
    let expected = [at(0, 0), at(SIZE, SIZE), at(SIZE * 2, SIZE * 2)];
    assert_eq!(custom_layout(true, LayoutDirection::Ltr), expected);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, customLayout_absolutePositioning_rtl
#[test]
fn custom_layout_absolute_positioning_rtl() {
    let expected = [at(0, 0), at(SIZE, SIZE), at(SIZE * 2, SIZE * 2)];
    assert_eq!(custom_layout(true, LayoutDirection::Rtl), expected);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, customLayout_positioning
#[test]
fn custom_layout_positioning() {
    let expected = [at(0, 0), at(SIZE, SIZE), at(SIZE * 2, SIZE * 2)];
    assert_eq!(custom_layout(false, LayoutDirection::Ltr), expected);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, customLayout_positioning_rtl
#[test]
fn custom_layout_positioning_rtl() {
    let expected = [at(SIZE * 2, 0), at(SIZE, SIZE), at(0, SIZE * 2)];
    assert_eq!(custom_layout(false, LayoutDirection::Rtl), expected);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, customLayout_updatingDirectionCausesRemeasure
#[test]
fn custom_layout_updating_direction_causes_remeasure() {
    let direction = mutable_state_of(LayoutDirection::Rtl);
    let actual: Seen<LayoutDirection> = Rc::default();
    let (provided, seen) = (direction.handle(), actual.clone());
    // The policies are made once, so a recomposition gives the nodes the same ones: only the
    // direction changing measures them again (a new policy would do it on its own).
    let parent = measure_policy(|scope, measurables, constraints| {
        let child = measurables[0];
        scope.layout_with(100, 100, move |placement| {
            let placeable = placement.measure(child, constraints);
            placement.place_relative(&placeable, 0, 0);
        })
    });
    let child = measure_policy(move |scope, _, _| {
        seen.set(Some(scope.layout_direction()));
        scope.layout(100, 100)
    });
    let mut ui = rule(move || {
        let (parent, child) = (parent.clone(), child.clone());
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(provided.get()), move || {
            let child = child.clone();
            Layout(
                Modifier,
                parent.clone(),
                callback(move || Layout(Modifier, child.clone(), callback(|| {}))),
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(actual.get(), Some(LayoutDirection::Rtl));
    direction.set(LayoutDirection::Ltr);
    ui.wait_for_idle();
    assert_eq!(actual.get(), Some(LayoutDirection::Ltr));
}

/// A leaf recording the direction its measure scope sees.
fn recording_direction(seen: &Seen<LayoutDirection>) {
    let seen = seen.clone();
    Layout(
        Modifier,
        measure_policy(move |scope, _, _| {
            seen.set(Some(scope.layout_direction()));
            scope.layout(0, 0)
        }),
        callback(|| {}),
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, testModifiedLayoutDirection_inMeasureScope
#[test]
fn test_modified_layout_direction_in_measure_scope() {
    let result: Seen<LayoutDirection> = Rc::default();
    let seen = result.clone();
    let mut ui = rule(move || {
        let seen = seen.clone();
        CompositionLocalProvider(
            LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
            move || {
                recording_direction(&seen);
            },
        );
    });
    ui.wait_for_idle();
    assert_eq!(result.get(), Some(LayoutDirection::Rtl));
}

/// The direction the window gives, provided again inside a right-to-left one.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, testRestoreLocaleLayoutDirection
#[test]
fn test_restore_locale_layout_direction() {
    let result: Seen<LayoutDirection> = Rc::default();
    let seen = result.clone();
    let mut ui = rule(move || {
        let seen = seen.clone();
        let initial = LOCAL_LAYOUT_DIRECTION.current();
        CompositionLocalProvider(
            LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
            move || {
                let seen = seen.clone();
                Box!({
                    let seen = seen.clone();
                    CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(initial), move || {
                        recording_direction(&seen);
                    });
                });
            },
        );
    });
    ui.wait_for_idle();
    assert_eq!(result.get(), Some(LayoutDirection::Ltr));
}

/// Material's minimum touch target (internal there): at least 48 dp each way, the content
/// centered, placed absolutely.
fn minimum_touch_target(size: DpSize) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let width = placeable.width.max(scope.density().round_to_px(size.width));
        let height = placeable.height.max(scope.density().round_to_px(size.height));
        let x = ((width - placeable.width) as f32 / 2.0).round() as i32;
        let y = ((height - placeable.height) as f32 / 2.0).round() as i32;
        scope.place(&placeable, x, y);
        scope.layout(width, height)
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, testChildGetsPlacedWithinContainerWithPaddingAndMinimumTouchTarget
#[test]
fn test_child_gets_placed_within_container_with_padding_and_minimum_touch_target() {
    let (outer, inner): (Positioned, Positioned) = Default::default();
    let (o, i) = (outer.clone(), inner.clone());
    let record = |seen: &Seen<(Offset, i32)>| {
        let seen = seen.clone();
        Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
            seen.set(Some((coordinates.local_to_window(Offset::ZERO), coordinates.size().width)))
        })
    };
    let (row_width, outer_box_width, padding) = (200, 56, 16);
    let mut ui = rule(move || {
        let (o, i) = (record(&o), record(&i));
        CompositionLocalProvider(
            LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
            move || {
                let (o, i) = (o.clone(), i.clone());
                Row!(modifier = Modifier.width(row_width.dp()), {
                    let i = i.clone();
                    Box!(
                        modifier = o
                            .clone()
                            .size(outer_box_width.dp())
                            .background(Color::RED)
                            .padding((padding.dp(), 0.dp()))
                            .then(minimum_touch_target(DpSize::new(48.dp(), 48.dp()))),
                        {
                            Box!(modifier = i.clone().size(30.dp()).background(Color::GRAY));
                        }
                    );
                });
            },
        );
    });
    ui.wait_for_idle();
    let (inner_offset, inner_width) = inner.get().expect("the inner box positioned");
    let (outer_offset, outer_width) = outer.get().expect("the outer box positioned");
    let (inner_width, outer_width) = (inner_width as f32, outer_width as f32);
    assert!(inner_width < outer_width);
    assert!(inner_offset.x > outer_offset.x);
    assert!(inner_width + inner_offset.x < outer_width + outer_offset.x);
    assert_eq!(outer_offset.x.round(), row_width as f32 - outer_width);
    let padding = padding as f32;
    // The outer box's left edge, the padding, the inner box's left edge.
    assert!((outer_offset.x + padding - inner_offset.x).abs() <= 1.0);
    // The inner box's right edge, the padding, the outer box's right edge.
    let outer_right_edge = outer_offset.x + outer_width;
    assert!((outer_right_edge - padding - (inner_offset.x + inner_width)).abs() <= 1.0);
}
