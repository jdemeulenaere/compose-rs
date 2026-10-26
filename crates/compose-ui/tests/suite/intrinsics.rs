//! Intrinsic sizes (docs/06-ui.md, "Intrinsics"): `MeasurePolicy`'s and `LayoutModifierNode`'s
//! intrinsic functions, their defaults, which layouts a change in a child's intrinsics measures
//! again, and the size checks. Adapted from Compose's `IntrinsicsMeasurementTest`,
//! `RemeasureWithIntrinsicsTest` and `RtlLayoutTest`.

#![allow(non_snake_case)]

use crate::support;

use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::{Column, Leaf, Stack};

/// A modifier recording the size of what follows it, as the tests' `onPlaced { coords = it }`.
fn size_into(size: &Rc<Cell<IntSize>>) -> Modifier {
    let size = size.clone();
    Modifier
        .on_globally_positioned(move |coordinates: LayoutCoordinates| size.set(coordinates.size()))
}

/// A host of 400 by 400 px, density 1, showing `content`: room for the tests' layouts.
fn host(content: impl Fn() + 'static) -> UiHost {
    let mut host = UiHost::headless(400, 400);
    host.set_content(content);
    host.frame();
    host
}

fn counter() -> Rc<Cell<u32>> {
    Rc::new(Cell::new(0))
}

/// A layout of `content` with `policy`.
#[composable]
fn Custom(modifier: Modifier, policy: Rc<dyn MeasurePolicy>, content: impl Fn() + 'static) {
    Layout(modifier, policy, content);
}

/// Measures its first child at a fixed height of 1.2 times its minimum intrinsic height at an
/// unbounded width, as the parents of `IntrinsicsMeasurementTest`; counts its measurements.
struct TallerThanMinIntrinsic(Rc<Cell<u32>>);

impl MeasurePolicy for TallerThanMinIntrinsic {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        _: Constraints,
    ) -> MeasureResult {
        self.0.set(self.0.get() + 1);
        let intrinsic = scope.min_intrinsic_height(measurables[0], Constraints::INFINITY);
        let height = round_to_int(intrinsic as f32 * 1.2);
        let placeable = scope
            .measure(measurables[0], Constraints::new(0, Constraints::INFINITY, height, height));
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

/// A column of a 10 px box and, while `shown`, a second one, as the content of
/// `IntrinsicsMeasurementTest`.
#[composable]
fn TwoBoxes(shown: MutableState<bool>) {
    Column!({
        Leaf(Modifier.required_size(10.dp()));
        if shown.get() {
            Leaf(Modifier.required_size(10.dp()));
        }
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/IntrinsicsMeasurementTest.kt, intrinsicsChangeCausesParentRemeasure
#[test]
fn a_change_in_a_childs_intrinsics_measures_the_parent_that_asked_again() {
    let shown_state = mutable_state_of(true);
    let shown = shown_state.handle();
    let size = Rc::new(Cell::new(IntSize::new(0, 0)));
    let measures = counter();
    let (recorded, policy_measures) = (size.clone(), measures.clone());
    let mut host = host(move || {
        let policy: Rc<dyn MeasurePolicy> =
            Rc::new(TallerThanMinIntrinsic(policy_measures.clone()));
        Custom(size_into(&recorded), policy, callback(move || TwoBoxes(shown)));
    });
    assert_eq!(size.get().height, 24);
    shown_state.set(false);
    host.frame();
    assert_eq!(size.get().height, 12);
    shown_state.set(true);
    host.frame();
    assert_eq!(size.get().height, 24);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/IntrinsicsMeasurementTest.kt, grandchildChangeCausesRemeasure
#[test]
fn a_change_in_a_grandchilds_intrinsics_measures_the_layout_that_asked_through_its_child() {
    let shown_state = mutable_state_of(true);
    let shown = shown_state.handle();
    let size = Rc::new(Cell::new(IntSize::new(0, 0)));
    let recorded = size.clone();
    let mut host = host(move || {
        let policy: Rc<dyn MeasurePolicy> = Rc::new(TallerThanMinIntrinsic(counter()));
        Custom(
            size_into(&recorded),
            policy,
            callback(move || {
                Stack!({
                    TwoBoxes(shown);
                });
            }),
        );
    });
    assert_eq!(size.get().height, 24);
    shown_state.set(false);
    host.frame();
    assert_eq!(size.get().height, 12);
    shown_state.set(true);
    host.frame();
    assert_eq!(size.get().height, 24);
}

/// Measures its child with the constraints, and either asks its minimum intrinsic height for
/// its own or answers 100, as `IntrinsicsMeasurementTest`'s `sometimesIntrinsicsMeasurePolicy`.
struct SometimesIntrinsics(MutableState<bool>);

impl MeasurePolicy for SometimesIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(measurables[0], constraints);
        scope.place(&placeable, 0, 0);
        if self.0.get() {
            scope.layout(placeable.width, placeable.height)
        } else {
            scope.layout(100, 100)
        }
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        if self.0.get() { scope.min_intrinsic_height(measurables[0], width) } else { 100 }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/IntrinsicsMeasurementTest.kt, intrinsicsUsageChanges
#[test]
fn a_child_whose_intrinsics_are_no_longer_asked_doesnt_measure_the_layout_that_asked_them() {
    let shown_state = mutable_state_of(true);
    let shown = shown_state.handle();
    let use_intrinsics_state = mutable_state_of(true);
    let use_intrinsics = use_intrinsics_state.handle();
    let size = Rc::new(Cell::new(IntSize::new(0, 0)));
    let measures = counter();
    let (recorded, outer_measures) = (size.clone(), measures.clone());
    let mut host = host(move || {
        let outer: Rc<dyn MeasurePolicy> = Rc::new(TallerThanMinIntrinsic(outer_measures.clone()));
        Custom(
            size_into(&recorded),
            outer,
            callback(move || {
                Stack!({
                    let middle: Rc<dyn MeasurePolicy> =
                        remember(|| Rc::new(SometimesIntrinsics(use_intrinsics)));
                    Custom(Modifier, middle, callback(move || TwoBoxes(shown)));
                });
            }),
        );
    });
    measures.set(0);
    shown_state.set(false);
    host.frame();
    assert_eq!(measures.get(), 1);
    measures.set(0);
    shown_state.set(true);
    host.frame();
    assert_eq!(measures.get(), 1);
    assert_eq!(size.get().height, 24);
    use_intrinsics_state.set(false);
    host.frame();
    assert_eq!(size.get().height, 120);
    measures.set(0);
    shown_state.set(false);
    host.frame();
    assert_eq!(measures.get(), 0);
    assert_eq!(size.get().height, 120);
}

/// Answers `1 << 24` for its maximum intrinsic width.
struct TooWide;

impl MeasurePolicy for TooWide {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        _: Constraints,
    ) -> MeasureResult {
        scope.layout(0, 0)
    }

    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        1 << 24
    }
}

/// Answers `1 << 24` for its maximum intrinsic height.
struct TooTall;

impl MeasurePolicy for TooTall {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        _: Constraints,
    ) -> MeasureResult {
        scope.layout(0, 0)
    }

    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        1 << 24
    }
}

/// A layout whose policy answers a too large intrinsic, under a modifier that asks it through
/// a `layout` modifier measuring with its defaults: the intrinsic measurement panics.
fn asking_a_too_large_intrinsic_panics(width: bool) -> bool {
    let panicked = Rc::new(Cell::new(false));
    let caught = panicked.clone();
    let _host = host(move || {
        let caught = caught.clone();
        let asking = Modifier
            .layout(move |scope, content, _| {
                let asked = panic::catch_unwind(AssertUnwindSafe(|| {
                    if width {
                        scope.max_intrinsic_width(content, 100)
                    } else {
                        scope.max_intrinsic_height(content, 100)
                    }
                }));
                caught.set(asked.is_err());
                scope.layout(0, 0)
            })
            .layout(|scope, content, constraints| {
                let placeable = scope.measure(content, constraints);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            });
        let policy: Rc<dyn MeasurePolicy> = if width { Rc::new(TooWide) } else { Rc::new(TooTall) };
        Custom(asking, policy, callback(|| {}));
    });
    panicked.get()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/IntrinsicsMeasurementTest.kt, measureWidthTooLarge
#[test]
fn an_intrinsic_width_too_large_for_a_layout_panics() {
    assert!(asking_a_too_large_intrinsic_panics(true));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/IntrinsicsMeasurementTest.kt, measureHeightTooLarge
#[test]
fn an_intrinsic_height_too_large_for_a_layout_panics() {
    assert!(asking_a_too_large_intrinsic_panics(false));
}

// ---- RemeasureWithIntrinsicsTest -----------------------------------------------------------

/// A layout of its constraints' minimum size whose maximum intrinsics are `width` by `height`,
/// as `RemeasureWithIntrinsicsTest`'s `LayoutWithIntrinsics`.
struct WithIntrinsics {
    width: i32,
    height: i32,
}

impl MeasurePolicy for WithIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        scope.layout(constraints.min_width, constraints.min_height)
    }

    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.width
    }

    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.height
    }
}

#[composable]
fn LayoutWithIntrinsics(width: i32, height: i32, modifier: Modifier, content: impl Fn() + 'static) {
    // A new policy each composition, as the test's object expression.
    let policy: Rc<dyn MeasurePolicy> = Rc::new(WithIntrinsics { width, height });
    Layout(modifier, policy, content);
}

/// Measures its only child at its maximum intrinsic size when `use_intrinsics` says so, else
/// with its constraints, and answers its child's intrinsics, as `LayoutMaybeUsingIntrinsics`;
/// counts its measurements.
struct MaybeUsingIntrinsics {
    use_intrinsics: bool,
    measures: Rc<Cell<u32>>,
}

impl MeasurePolicy for MaybeUsingIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        assert_eq!(measurables.len(), 1);
        self.measures.set(self.measures.get() + 1);
        let child = if self.use_intrinsics {
            let width = scope.max_intrinsic_width(measurables[0], constraints.max_height);
            let height = scope.max_intrinsic_height(measurables[0], constraints.max_width);
            Constraints::fixed(width, height)
        } else {
            constraints
        };
        let placeable = scope.measure(measurables[0], child);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }

    fn min_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        scope.min_intrinsic_width(measurables[0], height)
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        scope.min_intrinsic_height(measurables[0], width)
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        scope.max_intrinsic_width(measurables[0], height)
    }

    fn max_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        scope.max_intrinsic_height(measurables[0], width)
    }
}

#[composable]
fn LayoutUsingIntrinsics(
    modifier: Modifier,
    measures: Rc<Cell<u32>>,
    content: impl Fn() + 'static,
) {
    let policy: Rc<dyn MeasurePolicy> = remember(|| {
        Rc::new(MaybeUsingIntrinsics { use_intrinsics: true, measures: measures.clone() })
    });
    Layout(modifier, policy, content);
}

/// A box whose children are measured with its constraints, minimums included, as
/// `Box(propagateMinConstraints = true)`, with Compose's default intrinsics.
struct PropagatingBox;

impl MeasurePolicy for PropagatingBox {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let (mut width, mut height) = (constraints.min_width, constraints.min_height);
        for &measurable in measurables {
            let placeable = scope.measure(measurable, constraints);
            scope.place(&placeable, 0, 0);
            width = width.max(placeable.width);
            height = height.max(placeable.height);
        }
        scope.layout(width, height)
    }
}

/// A layout modifier measuring its content at the content's maximum intrinsic size and
/// answering the content's intrinsics, as `RemeasureWithIntrinsicsTest`'s
/// `ModifierUsingIntrinsics`.
#[derive(Clone, Copy, PartialEq)]
struct UsingIntrinsics;

impl ModifierNodeElement for UsingIntrinsics {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(UsingIntrinsics)
    }
    fn update(&self, _: &mut dyn ModifierNode) {}
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<UsingIntrinsics>().is_some()
    }
}

impl ModifierNode for UsingIntrinsics {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for UsingIntrinsics {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let width = scope.max_intrinsic_width(content, constraints.max_height);
        let height = scope.max_intrinsic_height(content, constraints.max_width);
        let placeable = scope.measure(content, Constraints::fixed(width, height));
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }

    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        scope.min_intrinsic_width(content, height)
    }

    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        scope.min_intrinsic_height(content, width)
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        scope.max_intrinsic_width(content, height)
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        scope.max_intrinsic_height(content, width)
    }
}

/// A pass-through layout modifier whose maximum intrinsics are `width` by `height`, as
/// `RemeasureWithIntrinsicsTest`'s `Modifier.withIntrinsics`.
#[derive(Clone, Copy, PartialEq)]
struct FixedIntrinsics {
    width: i32,
    height: i32,
}

impl ModifierNodeElement for FixedIntrinsics {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(*self)
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<FixedIntrinsics>() {
            *node = *self;
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<FixedIntrinsics>() == Some(self)
    }
}

impl ModifierNode for FixedIntrinsics {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for FixedIntrinsics {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }

    fn max_intrinsic_width(
        &mut self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: Measurable,
        _: i32,
    ) -> i32 {
        self.width
    }

    fn max_intrinsic_height(
        &mut self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: Measurable,
        _: i32,
    ) -> i32 {
        self.height
    }
}

fn with_intrinsics(width: i32, height: i32) -> Modifier {
    Modifier.then_custom(FixedIntrinsics { width, height })
}

/// A cell recording a size, and the modifier reporting it.
fn recorded() -> (Rc<Cell<IntSize>>, Modifier) {
    let size = Rc::new(Cell::new(IntSize::new(-1, -1)));
    let modifier = size_into(&size);
    (size, modifier)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, remeasuringChildWhenParentUsedIntrinsicSizes
// (and RemeasureWithIntrinsicsRealClockTest.kt's, the same with the real clock)
#[test]
fn a_child_whose_intrinsics_change_is_measured_again_by_the_parent_that_used_them() {
    let intrinsics = mutable_state_of((40, 50));
    let handle = intrinsics.handle();
    let (child, modifier) = recorded();
    let mut host = host(move || {
        let modifier = modifier.clone();
        LayoutUsingIntrinsics(
            Modifier,
            counter(),
            callback(move || {
                let (width, height) = handle.get();
                LayoutWithIntrinsics(width, height, modifier.clone(), callback(|| {}));
            }),
        );
    });
    assert_eq!(child.get(), IntSize::new(40, 50));
    intrinsics.set((30, 20));
    host.frame();
    assert_eq!(child.get(), IntSize::new(30, 20));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, updatingChildIntrinsicsViaModifierWhenParentUsedIntrinsicSizes
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn a_childs_intrinsics_changed_by_its_modifier_measure_the_parent_that_used_them_again() {
    let intrinsics = mutable_state_of((40, 50));
    let handle = intrinsics.handle();
    let (child, modifier) = recorded();
    let mut host = host(move || {
        let modifier = modifier.clone();
        LayoutUsingIntrinsics(
            Modifier,
            counter(),
            callback(move || {
                let (width, height) = handle.get();
                Leaf(modifier.clone().then(with_intrinsics(width, height)));
            }),
        );
    });
    assert_eq!(child.get(), IntSize::new(40, 50));
    intrinsics.set((30, 20));
    host.frame();
    assert_eq!(child.get(), IntSize::new(30, 20));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, remeasuringGrandChildWhenGrandParentUsedIntrinsicSizes
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn a_grandchild_whose_intrinsics_change_is_measured_again_by_the_grandparent_that_used_them() {
    let intrinsics = mutable_state_of((40, 50));
    let handle = intrinsics.handle();
    let (child, modifier) = recorded();
    let mut host = host(move || {
        let modifier = modifier.clone();
        LayoutUsingIntrinsics(
            Modifier,
            counter(),
            callback(move || {
                let modifier = modifier.clone();
                Custom(
                    Modifier,
                    Rc::new(PropagatingBox),
                    callback(move || {
                        let (width, height) = handle.get();
                        LayoutWithIntrinsics(width, height, modifier.clone(), callback(|| {}));
                    }),
                );
            }),
        );
    });
    assert_eq!(child.get(), IntSize::new(40, 50));
    intrinsics.set((30, 20));
    host.frame();
    assert_eq!(child.get(), IntSize::new(30, 20));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, updatingGrandChildIntrinsicsViaModifierWhenGrandParentUsedIntrinsicSizes
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn a_grandchilds_intrinsics_changed_by_its_modifier_measure_the_grandparent_that_used_them() {
    let intrinsics = mutable_state_of((40, 50));
    let handle = intrinsics.handle();
    let (child, modifier) = recorded();
    let mut host = host(move || {
        let modifier = modifier.clone();
        LayoutUsingIntrinsics(
            Modifier,
            counter(),
            callback(move || {
                let modifier = modifier.clone();
                Custom(
                    Modifier,
                    Rc::new(PropagatingBox),
                    callback(move || {
                        let (width, height) = handle.get();
                        Leaf(modifier.clone().then(with_intrinsics(width, height)));
                    }),
                );
            }),
        );
    });
    assert_eq!(child.get(), IntSize::new(40, 50));
    intrinsics.set((30, 20));
    host.frame();
    assert_eq!(child.get(), IntSize::new(30, 20));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, nodeDoesNotCauseRemeasureOfAncestor_whenItsIntrinsicsAreUnused
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn a_node_whose_intrinsics_nobody_used_doesnt_measure_an_ancestor_that_used_its_parents() {
    let intrinsics = mutable_state_of((40, 50));
    let handle = intrinsics.handle();
    let measures = counter();
    let (parent, modifier) = recorded();
    let parent_measures = measures.clone();
    let mut host = host(move || {
        LayoutUsingIntrinsics(
            modifier.clone(),
            parent_measures.clone(),
            callback(move || {
                LayoutWithIntrinsics(
                    20,
                    20,
                    Modifier,
                    callback(move || {
                        let (width, height) = handle.get();
                        LayoutWithIntrinsics(width, height, Modifier, callback(|| {}));
                    }),
                );
            }),
        );
    });
    intrinsics.set((30, 20));
    host.frame();
    assert_eq!(measures.get(), 1);
    assert_eq!(parent.get(), IntSize::new(20, 20));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, causesRemeasureOfAllDependantAncestors
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn a_change_in_intrinsics_measures_every_ancestor_that_used_them_again() {
    let intrinsics = mutable_state_of((40, 50));
    let handle = intrinsics.handle();
    let (outer, inner) = (counter(), counter());
    let (outer_measures, inner_measures) = (outer.clone(), inner.clone());
    let mut host = host(move || {
        let inner_measures = inner_measures.clone();
        LayoutUsingIntrinsics(
            Modifier,
            outer_measures.clone(),
            callback(move || {
                let inner_measures = inner_measures.clone();
                Stack!({
                    LayoutUsingIntrinsics(
                        Modifier,
                        inner_measures.clone(),
                        callback(move || {
                            Stack!({
                                let (width, height) = handle.get();
                                LayoutWithIntrinsics(width, height, Modifier, callback(|| {}));
                            });
                        }),
                    );
                });
            }),
        );
    });
    intrinsics.set((30, 20));
    host.frame();
    assert_eq!((outer.get(), inner.get()), (2, 2));
}

/// Asks its only child's maximum intrinsics without sizing it to them, measures it with its
/// constraints and takes their maximum, as `whenConnectionFromOwnerDoesNotQueryAnymore`'s
/// parent; counts its measurements.
struct AsksButIgnores(Rc<Cell<u32>>);

impl MeasurePolicy for AsksButIgnores {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        self.0.set(self.0.get() + 1);
        scope.max_intrinsic_width(measurables[0], constraints.max_height);
        scope.max_intrinsic_height(measurables[0], constraints.max_width);
        let placeable = scope.measure(measurables[0], constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(constraints.max_width, constraints.max_height)
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, whenConnectionFromOwnerDoesNotQueryAnymore
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn intrinsics_no_longer_asked_through_a_child_dont_measure_the_parent_again() {
    let intrinsics = mutable_state_of((40, 50));
    let connection = mutable_state_of(false);
    let (handle, sized) = (intrinsics.handle(), connection.handle());
    let measures = counter();
    let parent_measures = measures.clone();
    let mut host = host(move || {
        let policy: Rc<dyn MeasurePolicy> =
            remember(|| Rc::new(AsksButIgnores(parent_measures.clone())));
        Custom(
            Modifier,
            policy,
            callback(move || {
                // A fixed size answers its intrinsics without asking its content's.
                let modifier = if sized.get() { Modifier.size(10.dp()) } else { Modifier };
                Custom(
                    modifier,
                    Rc::new(StackMeasurePolicy),
                    callback(move || {
                        let (width, height) = handle.get();
                        LayoutWithIntrinsics(width, height, Modifier, callback(|| {}));
                    }),
                );
            }),
        );
    });
    assert_eq!(measures.get(), 1);
    connection.set(true);
    host.frame();
    assert_eq!(measures.get(), 2);
    intrinsics.set((30, 20));
    host.frame();
    assert_eq!(measures.get(), 2);
}

/// `RemeasureWithIntrinsicsTest`'s modifier scene: a parent measuring its child with its
/// constraints (`use_intrinsics` false) or at its intrinsics, over a fixed 100 px box holding a
/// box whose modifier sizes it to its content's maximum intrinsics, from a layout whose
/// intrinsics change. The fixed box stops the change: the parent isn't measured again.
fn intrinsics_asked_from_a_modifier(use_intrinsics: bool) {
    let intrinsics = mutable_state_of((40, 50));
    let handle = intrinsics.handle();
    let measures = counter();
    let (boxed, modifier) = recorded();
    let parent_measures = measures.clone();
    let mut host = host(move || {
        let modifier = modifier.clone();
        let policy: Rc<dyn MeasurePolicy> = remember(|| {
            Rc::new(MaybeUsingIntrinsics { use_intrinsics, measures: parent_measures.clone() })
        });
        Custom(
            Modifier,
            policy,
            callback(move || {
                let modifier = modifier.clone();
                Stack!(modifier = Modifier.required_size(100.dp()), {
                    let modifier = modifier.clone();
                    Stack!(modifier = modifier.clone().then_custom(UsingIntrinsics), {
                        let (width, height) = handle.get();
                        LayoutWithIntrinsics(width, height, Modifier, callback(|| {}));
                    });
                });
            }),
        );
    });
    intrinsics.set((30, 20));
    host.frame();
    assert_eq!(measures.get(), 1);
    assert_eq!(boxed.get(), IntSize::new(30, 20));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, whenQueriedFromModifier
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn intrinsics_asked_by_a_modifier_measure_its_node_again_when_they_change() {
    intrinsics_asked_from_a_modifier(false);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, whenQueriedFromModifier_andAParentQueriesAbove
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn intrinsics_asked_by_a_modifier_under_a_fixed_size_dont_measure_a_parent_asking_above() {
    intrinsics_asked_from_a_modifier(true);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, childWithStatefulMeasureBlock
#[test]
fn a_child_measuring_with_side_effects_is_measured_again_after_its_intrinsics_were_asked() {
    let remeasure = mutable_state_of(0);
    let handle = remeasure.handle();
    let rect = Rc::new(Cell::new(Rect::new(0.0, 0.0, 0.0, 0.0)));
    let kept = rect.clone();
    let mut host = host(move || {
        let kept = kept.clone();
        let policy = measure_policy(move |scope, measurables, _| {
            handle.get();
            scope.min_intrinsic_height(measurables[0], 50);
            let placeable = scope.measure(measurables[0], Constraints::fixed(100, 100));
            scope.place(&placeable, 0, 0);
            scope.layout(100, 100)
        });
        Custom(
            Modifier,
            policy,
            callback(move || {
                let kept = kept.clone();
                // Keeps what its last measurement saw, as the test's `rect` drawn behind it.
                Leaf(Modifier.layout(move |scope, content, constraints| {
                    let placeable = scope.measure(content, constraints);
                    kept.set(Rect::new(
                        20.0,
                        20.0,
                        placeable.width as f32 - 20.0,
                        placeable.height as f32 - 20.0,
                    ));
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                }));
            }),
        );
    });
    assert_eq!(rect.get(), Rect::new(20.0, 20.0, 80.0, 80.0));
    remeasure.set(1);
    host.frame();
    assert_eq!(rect.get(), Rect::new(20.0, 20.0, 80.0, 80.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasureWithIntrinsicsTest.kt, introducingChildIntrinsicsViaModifierWhenParentUsedIntrinsicSizes
// (and RemeasureWithIntrinsicsRealClockTest.kt's)
#[test]
fn intrinsics_a_new_modifier_gives_a_child_measure_the_parent_that_used_them_again() {
    let with = mutable_state_of(false);
    let handle = with.handle();
    let (child, modifier) = recorded();
    let mut host = host(move || {
        let modifier = modifier.clone();
        LayoutUsingIntrinsics(
            Modifier,
            counter(),
            callback(move || {
                let extra = if handle.get() { with_intrinsics(30, 20) } else { Modifier };
                Leaf(modifier.clone().then(extra));
            }),
        );
    });
    assert_eq!(child.get(), IntSize::new(0, 0));
    with.set(true);
    host.frame();
    assert_eq!(child.get(), IntSize::new(30, 20));
}

// ---- RtlLayoutTest -----------------------------------------------------------------------------

/// Records the layout direction its maximum intrinsic width is asked in.
struct DirectionOfIntrinsics(Rc<Cell<Option<LayoutDirection>>>);

impl MeasurePolicy for DirectionOfIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        _: Constraints,
    ) -> MeasureResult {
        scope.layout(0, 0)
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.0.set(Some(scope.layout_direction()));
        0
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RtlLayoutTest.kt, testModifiedLayoutDirection_inIntrinsicsMeasure
#[test]
fn intrinsics_are_asked_in_the_layout_direction_of_the_layout() {
    let direction = Rc::new(Cell::new(None));
    let recorded = direction.clone();
    let _host = host(move || {
        let recorded = recorded.clone();
        CompositionLocalProvider(
            LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
            move || {
                let policy: Rc<dyn MeasurePolicy> =
                    Rc::new(DirectionOfIntrinsics(recorded.clone()));
                Custom(Modifier.width(IntrinsicSize::Max), policy, callback(|| {}));
            },
        );
    });
    assert_eq!(direction.get(), Some(LayoutDirection::Rtl));
}
