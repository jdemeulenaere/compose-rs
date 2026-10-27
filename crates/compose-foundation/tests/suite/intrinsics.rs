//! The foundation layouts' and modifiers' intrinsic sizes, with Compose's formulas: `Row`,
//! `Column`, `Box`, `size`, `padding`, `aspect_ratio`, `fill_max_*`, `wrap_content_*`,
//! `default_min_size`, `width`/`height`/`required_width`/`required_height(IntrinsicSize)`,
//! scrolling, text and images. Adapted from Compose's `IntrinsicTest`, `RowColumnTest`,
//! `SizeTest`, `BoxTest`, `AspectRatioTest`, `PaddingTest`, `ScrollTest` and `TextLayoutTest`.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, Constraints, Dp, ImageBitmap, IntSize, IntrinsicMeasurable, IntrinsicMeasureScope,
    IntrinsicSize, LOCAL_LAYOUT_DIRECTION, Layout, LayoutCoordinates, LayoutDirection,
    LayoutModifierNode, Measurable, MeasurePolicy, MeasureResult, MeasureScope, ModifierNode,
    ModifierNodeElement, Offset, UnitExt,
};

const INF: i32 = Constraints::INFINITY;

// ---- Test layouts, as Compose's `LayoutTest` helpers --------------------------------------

/// The intrinsics of the layout under test, asked from its parent's measurement.
struct Asked<'a, 'b> {
    scope: &'a mut IntrinsicMeasureScope<'b>,
    measurable: Measurable,
}

impl Asked<'_, '_> {
    fn min_width(&mut self, height: i32) -> i32 {
        self.scope.min_intrinsic_width(self.measurable, height)
    }
    fn min_height(&mut self, width: i32) -> i32 {
        self.scope.min_intrinsic_height(self.measurable, width)
    }
    fn max_width(&mut self, height: i32) -> i32 {
        self.scope.max_intrinsic_width(self.measurable, height)
    }
    fn max_height(&mut self, width: i32) -> i32 {
        self.scope.max_intrinsic_height(self.measurable, width)
    }
}

type IntrinsicsTest = Rc<dyn Fn(&mut Asked<'_, '_>)>;

/// Runs `test` on the intrinsics of its only child from its measurement, as `testIntrinsics`.
struct TestIntrinsics {
    test: IntrinsicsTest,
    ran: Rc<Cell<bool>>,
}

impl MeasurePolicy for TestIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        _: Constraints,
    ) -> MeasureResult {
        let mut asked = Asked { scope, measurable: measurables[0] };
        (self.test)(&mut asked);
        self.ran.set(true);
        scope.layout(0, 0)
    }
}

/// As `LayoutTest.testIntrinsics`: `test` runs on each of `layouts`' intrinsics (in a root of
/// 800 px, at density 1, so dp are pixels).
fn test_intrinsics(layouts: Vec<Rc<dyn Fn()>>, test: impl Fn(&mut Asked<'_, '_>) + 'static) {
    let test: IntrinsicsTest = Rc::new(test);
    for layout in layouts {
        let ran = Rc::new(Cell::new(false));
        let (test, flag) = (test.clone(), ran.clone());
        let _ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
            let policy: Rc<dyn MeasurePolicy> =
                Rc::new(TestIntrinsics { test: test.clone(), ran: flag.clone() });
            let layout = layout.clone();
            Layout(Modifier, policy, callback(move || layout()));
        });
        assert!(ran.get(), "the intrinsics weren't asked");
    }
}

fn layout(content: impl Fn() + 'static) -> Rc<dyn Fn()> {
    Rc::new(content)
}

/// As `LayoutTest.ConstrainedBox`: its child within `constraints` (and the incoming ones), its
/// intrinsics its child's within `constraints`.
struct ConstrainedPolicy(Constraints);

impl MeasurePolicy for ConstrainedPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        incoming: Constraints,
    ) -> MeasureResult {
        let child = incoming.constrain_by(self.0);
        let placeable = measurables.first().map(|&measurable| scope.measure(measurable, child));
        if let Some(placeable) = &placeable {
            scope.place_relative(placeable, 0, 0);
        }
        let width = placeable.map_or(child.min_width, |placeable| placeable.width);
        let height = placeable.map_or(child.min_height, |placeable| placeable.height);
        scope.layout(width, height)
    }

    fn min_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        let width =
            measurables.first().map_or(0, |&child| scope.min_intrinsic_width(child, height));
        self.0.constrain_width(width)
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        let height =
            measurables.first().map_or(0, |&child| scope.min_intrinsic_height(child, width));
        self.0.constrain_height(height)
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        let width =
            measurables.first().map_or(0, |&child| scope.max_intrinsic_width(child, height));
        self.0.constrain_width(width)
    }

    fn max_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        let height =
            measurables.first().map_or(0, |&child| scope.max_intrinsic_height(child, width));
        self.0.constrain_height(height)
    }
}

#[composable]
fn ConstrainedBox(constraints: Constraints, modifier: Modifier) {
    Layout(modifier, Rc::new(ConstrainedPolicy(constraints)), callback(|| {}));
}

/// As `LayoutTest.Container` (with its `width` and `height`): its child, centered, in
/// constraints of its own size, or the child's size; Compose's default intrinsics.
struct ContainerPolicy {
    width: Option<i32>,
    height: Option<i32>,
}

impl MeasurePolicy for ContainerPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        incoming: Constraints,
    ) -> MeasureResult {
        let own = Constraints::new(
            self.width.unwrap_or(0),
            self.width.unwrap_or(INF),
            self.height.unwrap_or(0),
            self.height.unwrap_or(INF),
        );
        let container = incoming.constrain_by(own);
        let child = Constraints::new(0, container.max_width, 0, container.max_height);
        let placeable = measurables.first().map(|&measurable| scope.measure(measurable, child));
        let size = |fixed: bool, bounded: bool, max: i32, min: i32, measured: Option<i32>| {
            if fixed && bounded { max } else { measured.unwrap_or(0).max(min) }
        };
        let width = size(
            container.has_fixed_width(),
            container.has_bounded_width(),
            container.max_width,
            container.min_width,
            placeable.map(|placeable| placeable.width),
        );
        let height = size(
            container.has_fixed_height(),
            container.has_bounded_height(),
            container.max_height,
            container.min_height,
            placeable.map(|placeable| placeable.height),
        );
        if let Some(placeable) = &placeable {
            let at = Alignment::CENTER.align(
                placeable.size(),
                IntSize::new(width, height),
                scope.layout_direction(),
            );
            scope.place(placeable, at.x, at.y);
        }
        scope.layout(width, height)
    }
}

#[composable]
fn Container(
    modifier: Modifier,
    #[default(None)] width: Option<i32>,
    #[default(None)] height: Option<i32>,
    #[default(|| {})] content: impl Fn() + 'static,
) {
    Layout(modifier, Rc::new(ContainerPolicy { width, height }), content);
}

/// An empty container of `modifier`.
#[composable]
fn Empty(modifier: Modifier) {
    Container(modifier, None, None, callback(|| {}));
}

fn size_into(size: &Rc<Cell<IntSize>>) -> Modifier {
    let size = size.clone();
    Modifier
        .on_globally_positioned(move |coordinates: LayoutCoordinates| size.set(coordinates.size()))
}

fn cell() -> Rc<Cell<IntSize>> {
    Rc::new(Cell::new(IntSize::new(-1, -1)))
}

// ---- RowColumnTest -----------------------------------------------------------------------------

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withNoWeightChildren_hasCorrectIntrinsicMeasurements
// (its `alignBy` variant waits on alignment lines)
#[test]
fn a_rows_intrinsics_add_its_childrens_widths_and_take_their_largest_height() {
    let rows: Vec<Rc<dyn Fn()>> = vec![
        layout(|| {
            Row!({
                Empty(Modifier.aspect_ratio(2.0));
                ConstrainedBox(Constraints::fixed(50, 40), Modifier);
            });
        }),
        layout(|| {
            Row!(modifier = Modifier.fill_max_width(), {
                Empty(Modifier.aspect_ratio(2.0));
                ConstrainedBox(Constraints::fixed(50, 40), Modifier);
            });
        }),
        layout(|| {
            Row!(|row| {
                Empty(Modifier.aspect_ratio(2.0).align(row, Alignment::TOP));
                ConstrainedBox(
                    Constraints::fixed(50, 40),
                    Modifier.align(row, Alignment::CENTER_VERTICALLY),
                );
            });
        }),
        layout(|| {
            Row!(
                modifier = Modifier.fill_max_width(),
                horizontal_arrangement = Arrangement::START,
                {
                    Empty(Modifier.aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier);
                }
            );
        }),
        layout(|| {
            Row!(
                modifier = Modifier.fill_max_width(),
                horizontal_arrangement = Arrangement::CENTER,
                |row| {
                    Empty(Modifier.align(row, Alignment::CENTER_VERTICALLY).aspect_ratio(2.0));
                    ConstrainedBox(
                        Constraints::fixed(50, 40),
                        Modifier.align(row, Alignment::CENTER_VERTICALLY),
                    );
                }
            );
        }),
        layout(|| {
            Row!(
                modifier = Modifier.fill_max_width(),
                horizontal_arrangement = Arrangement::END,
                |row| {
                    Empty(Modifier.align(row, Alignment::BOTTOM).aspect_ratio(2.0));
                    ConstrainedBox(
                        Constraints::fixed(50, 40),
                        Modifier.align(row, Alignment::BOTTOM),
                    );
                }
            );
        }),
        layout(|| {
            Row!(
                modifier = Modifier.fill_max_width(),
                horizontal_arrangement = Arrangement::SPACE_AROUND,
                {
                    Empty(Modifier.fill_max_height().aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier.fill_max_height());
                }
            );
        }),
        layout(|| {
            Row!(
                modifier = Modifier.fill_max_width(),
                horizontal_arrangement = Arrangement::SPACE_BETWEEN,
                {
                    Empty(Modifier.aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier);
                }
            );
        }),
        layout(|| {
            Row!(
                modifier = Modifier.fill_max_width(),
                horizontal_arrangement = Arrangement::SPACE_EVENLY,
                {
                    Empty(Modifier.aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier);
                }
            );
        }),
    ];
    test_intrinsics(rows, |asked| {
        assert_eq!(asked.min_width(0), 50);
        assert_eq!(asked.min_width(25), 25 * 2 + 50);
        assert_eq!(asked.min_width(INF), 50);
        assert_eq!(asked.min_height(0), 40);
        assert_eq!(asked.min_height(70), 40);
        assert_eq!(asked.min_height(INF), 40);
        assert_eq!(asked.max_width(0), 50);
        assert_eq!(asked.max_width(25), 25 * 2 + 50);
        assert_eq!(asked.max_width(INF), 50);
        assert_eq!(asked.max_height(0), 40);
        assert_eq!(asked.max_height(70), 40);
        assert_eq!(asked.max_height(INF), 40);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withNoItems_hasCorrectIntrinsicMeasurements
#[test]
fn an_empty_row_or_column_has_no_intrinsic_size_whatever_its_spacing() {
    let layouts: Vec<Rc<dyn Fn()>> = vec![
        layout(|| {
            Row!(
                modifier = Modifier.width(IntrinsicSize::Max).height(IntrinsicSize::Max),
                horizontal_arrangement = Arrangement::spaced_by(48.dp()),
                {}
            );
        }),
        layout(|| {
            Row!(
                modifier = Modifier.width(IntrinsicSize::Min).height(IntrinsicSize::Min),
                horizontal_arrangement = Arrangement::spaced_by(48.dp()),
                {}
            );
        }),
        layout(|| {
            Column!(
                modifier = Modifier.width(IntrinsicSize::Max).height(IntrinsicSize::Max),
                vertical_arrangement = Arrangement::spaced_by(48.dp()),
                {}
            );
        }),
        layout(|| {
            Column!(
                modifier = Modifier.width(IntrinsicSize::Min).height(IntrinsicSize::Min),
                vertical_arrangement = Arrangement::spaced_by(48.dp()),
                {}
            );
        }),
    ];
    test_intrinsics(layouts, |asked| {
        assert_eq!(asked.min_width(0), 0);
        assert_eq!(asked.min_height(0), 0);
        assert_eq!(asked.max_width(0), 0);
        assert_eq!(asked.max_height(0), 0);
    });
}

/// `RowColumnTest`'s weighted children: fixed 20x30 weighted 3, fixed 30x40 weighted 2, a 2:1
/// aspect ratio weighted 2 and a fixed 20x30, in a row (or a column of their transposes).
fn weighted_children(row: bool, arrangement: u8, align: bool) -> Rc<dyn Fn()> {
    let content = move |weight: &dyn Fn(Modifier, f32) -> Modifier,
                        aligned: &dyn Fn(Modifier, u8) -> Modifier| {
        let (first, second, last) = if row {
            (Constraints::fixed(20, 30), Constraints::fixed(30, 40), Constraints::fixed(20, 30))
        } else {
            (Constraints::fixed(30, 20), Constraints::fixed(40, 30), Constraints::fixed(30, 20))
        };
        let ratio = if row { 2.0 } else { 0.5 };
        ConstrainedBox(first, aligned(weight(Modifier, 3.0), 0));
        ConstrainedBox(second, aligned(weight(Modifier, 2.0), 1));
        Empty(aligned(weight(Modifier.aspect_ratio(ratio), 2.0), 3));
        ConstrainedBox(last, aligned(Modifier, 2));
    };
    let content = Rc::new(content);
    if row {
        layout(move || {
            let content = content.clone();
            let children = move |scope: RowScope| {
                let weight = |modifier: Modifier, weight: f32| modifier.weight(scope, weight);
                let aligned = |modifier: Modifier, index: u8| match (align, arrangement, index) {
                    (false, _, _) => modifier,
                    (true, 6, _) => modifier.fill_max_height(),
                    (true, 0, 0) => modifier.align(scope, Alignment::TOP),
                    (true, 0, 1) => modifier.align(scope, Alignment::CENTER_VERTICALLY),
                    (true, 0, 2) => modifier.align(scope, Alignment::BOTTOM),
                    (true, 0, _) => modifier,
                    (true, 4, _) => modifier.align(scope, Alignment::CENTER_VERTICALLY),
                    (true, 5, _) => modifier.align(scope, Alignment::BOTTOM),
                    _ => modifier,
                };
                content(&weight, &aligned);
            };
            match arrangement {
                3 => Row!(horizontal_arrangement = Arrangement::START, content = children),
                4 => Row!(horizontal_arrangement = Arrangement::CENTER, content = children),
                5 => Row!(horizontal_arrangement = Arrangement::END, content = children),
                6 => Row!(horizontal_arrangement = Arrangement::SPACE_AROUND, content = children),
                7 => Row!(horizontal_arrangement = Arrangement::SPACE_BETWEEN, content = children),
                8 => Row!(horizontal_arrangement = Arrangement::SPACE_EVENLY, content = children),
                _ => Row!(content = children),
            }
        })
    } else {
        layout(move || {
            let content = content.clone();
            let children = move |scope: ColumnScope| {
                let weight = |modifier: Modifier, weight: f32| modifier.weight(scope, weight);
                let aligned = |modifier: Modifier, index: u8| match (align, arrangement, index) {
                    (false, _, _) => modifier,
                    (true, 6, _) => modifier.fill_max_width(),
                    (true, 0, 0) => modifier.align(scope, Alignment::START),
                    (true, 0, 1) => modifier.align(scope, Alignment::CENTER_HORIZONTALLY),
                    (true, 0, 2) => modifier.align(scope, Alignment::END),
                    (true, 0, _) => modifier,
                    (true, 4, _) => modifier.align(scope, Alignment::CENTER_HORIZONTALLY),
                    (true, 5, _) => modifier.align(scope, Alignment::END),
                    _ => modifier,
                };
                content(&weight, &aligned);
            };
            match arrangement {
                3 => Column!(vertical_arrangement = Arrangement::TOP, content = children),
                4 => Column!(vertical_arrangement = Arrangement::CENTER, content = children),
                5 => Column!(vertical_arrangement = Arrangement::BOTTOM, content = children),
                6 => Column!(vertical_arrangement = Arrangement::SPACE_AROUND, content = children),
                7 => Column!(vertical_arrangement = Arrangement::SPACE_BETWEEN, content = children),
                8 => Column!(vertical_arrangement = Arrangement::SPACE_EVENLY, content = children),
                _ => Column!(content = children),
            }
        })
    }
}

/// The variants of `RowColumnTest`'s weighted intrinsics tests: plain, aligned, and with each
/// arrangement (aligned as Compose's are).
fn weighted_variants(row: bool) -> Vec<Rc<dyn Fn()>> {
    vec![
        weighted_children(row, 0, false),
        weighted_children(row, 0, true),
        weighted_children(row, 3, false),
        weighted_children(row, 4, true),
        weighted_children(row, 5, true),
        weighted_children(row, 6, true),
        weighted_children(row, 7, false),
        weighted_children(row, 8, false),
    ]
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withWeightChildren_hasCorrectIntrinsicMeasurements
#[test]
fn a_rows_weighted_children_take_their_largest_width_per_weight_in_its_intrinsics() {
    test_intrinsics(weighted_variants(true), |asked| {
        assert_eq!(asked.min_width(0), 30 / 2 * 7 + 20);
        assert_eq!(asked.min_width(10), 30 / 2 * 7 + 20);
        assert_eq!(asked.min_width(25), 25 * 2 / 2 * 7 + 20);
        assert_eq!(asked.min_width(INF), 30 / 2 * 7 + 20);
        assert_eq!(asked.min_height(0), 40);
        assert_eq!(asked.min_height(125), 40);
        assert_eq!(asked.min_height(370), 50);
        assert_eq!(asked.min_height(INF), 40);
        assert_eq!(asked.max_width(0), 30 / 2 * 7 + 20);
        assert_eq!(asked.max_width(10), 30 / 2 * 7 + 20);
        assert_eq!(asked.max_width(25), 25 * 2 / 2 * 7 + 20);
        assert_eq!(asked.max_width(INF), 30 / 2 * 7 + 20);
        assert_eq!(asked.max_height(0), 40);
        assert_eq!(asked.max_height(125), 40);
        assert_eq!(asked.max_height(370), 50);
        assert_eq!(asked.max_height(INF), 40);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withWeightChildren_hasCorrectIntrinsicMeasurements
#[test]
fn a_columns_weighted_children_take_their_largest_height_per_weight_in_its_intrinsics() {
    test_intrinsics(weighted_variants(false), |asked| {
        assert_eq!(asked.min_width(0), 40);
        assert_eq!(asked.min_width(125), 40);
        assert_eq!(asked.min_width(370), 50);
        assert_eq!(asked.min_width(INF), 40);
        assert_eq!(asked.min_height(0), 30 / 2 * 7 + 20);
        assert_eq!(asked.min_height(10), 30 / 2 * 7 + 20);
        assert_eq!(asked.min_height(25), 25 * 2 / 2 * 7 + 20);
        assert_eq!(asked.min_height(INF), 30 / 2 * 7 + 20);
        assert_eq!(asked.max_width(0), 40);
        assert_eq!(asked.max_width(125), 40);
        assert_eq!(asked.max_width(370), 50);
        assert_eq!(asked.max_width(INF), 40);
        assert_eq!(asked.max_height(0), 30 / 2 * 7 + 20);
        assert_eq!(asked.max_height(10), 30 / 2 * 7 + 20);
        assert_eq!(asked.max_height(25), 25 * 2 / 2 * 7 + 20);
        assert_eq!(asked.max_height(INF), 30 / 2 * 7 + 20);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withNoWeightChildren_hasCorrectIntrinsicMeasurements
// (its `alignBy` variant waits on alignment lines)
#[test]
fn a_columns_intrinsics_add_its_childrens_heights_and_take_their_largest_width() {
    let columns: Vec<Rc<dyn Fn()>> = vec![
        layout(|| {
            Column!({
                Empty(Modifier.aspect_ratio(2.0));
                ConstrainedBox(Constraints::fixed(50, 40), Modifier);
            });
        }),
        layout(|| {
            Column!(|column| {
                Empty(Modifier.aspect_ratio(2.0).align(column, Alignment::START));
                ConstrainedBox(Constraints::fixed(50, 40), Modifier.align(column, Alignment::END));
            });
        }),
        layout(|| {
            Column!(modifier = Modifier.fill_max_height(), {
                Empty(Modifier.aspect_ratio(2.0));
                ConstrainedBox(Constraints::fixed(50, 40), Modifier);
            });
        }),
        layout(|| {
            Column!(
                modifier = Modifier.fill_max_height(),
                vertical_arrangement = Arrangement::TOP,
                {
                    Empty(Modifier.aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier);
                }
            );
        }),
        layout(|| {
            Column!(
                modifier = Modifier.fill_max_height(),
                vertical_arrangement = Arrangement::CENTER,
                |column| {
                    Empty(Modifier.align(column, Alignment::CENTER_HORIZONTALLY).aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier);
                }
            );
        }),
        layout(|| {
            Column!(
                modifier = Modifier.fill_max_height(),
                vertical_arrangement = Arrangement::BOTTOM,
                |column| {
                    Empty(Modifier.align(column, Alignment::END).aspect_ratio(2.0));
                    ConstrainedBox(
                        Constraints::fixed(50, 40),
                        Modifier.align(column, Alignment::END),
                    );
                }
            );
        }),
        layout(|| {
            Column!(
                modifier = Modifier.fill_max_height(),
                vertical_arrangement = Arrangement::SPACE_AROUND,
                {
                    Empty(Modifier.fill_max_width().aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier.fill_max_width());
                }
            );
        }),
        layout(|| {
            Column!(
                modifier = Modifier.fill_max_height(),
                vertical_arrangement = Arrangement::SPACE_BETWEEN,
                {
                    Empty(Modifier.aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier);
                }
            );
        }),
        layout(|| {
            Column!(
                modifier = Modifier.fill_max_height(),
                vertical_arrangement = Arrangement::SPACE_EVENLY,
                {
                    Empty(Modifier.aspect_ratio(2.0));
                    ConstrainedBox(Constraints::fixed(50, 40), Modifier);
                }
            );
        }),
    ];
    test_intrinsics(columns, |asked| {
        assert_eq!(asked.min_width(0), 50);
        assert_eq!(asked.min_width(25), 50);
        assert_eq!(asked.min_width(INF), 50);
        assert_eq!(asked.min_height(0), 40);
        assert_eq!(asked.min_height(50), 50 / 2 + 40);
        assert_eq!(asked.min_height(INF), 40);
        assert_eq!(asked.max_width(0), 50);
        assert_eq!(asked.max_width(25), 50);
        assert_eq!(asked.max_width(INF), 50);
        assert_eq!(asked.max_height(0), 40);
        assert_eq!(asked.max_height(50), 50 / 2 + 40);
        assert_eq!(asked.max_height(INF), 40);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withArrangementSpacing
// and testColumn_withArrangementSpacing
#[test]
fn the_spacing_between_children_counts_in_a_row_or_columns_main_axis_intrinsics() {
    let row = layout(|| {
        Row!(horizontal_arrangement = Arrangement::spaced_by(5.dp()), {
            Box!(modifier = Modifier.size(10.dp()));
            Box!(modifier = Modifier.size(10.dp()));
            Box!(modifier = Modifier.size(10.dp()));
        });
    });
    test_intrinsics(vec![row], |asked| {
        assert_eq!(asked.min_width(INF), 10 * 3 + 2 * 5);
        assert_eq!(asked.max_width(INF), 10 * 3 + 2 * 5);
    });
    let column = layout(|| {
        Column!(vertical_arrangement = Arrangement::spaced_by(5.dp()), {
            Box!(modifier = Modifier.size(10.dp()));
            Box!(modifier = Modifier.size(10.dp()));
            Box!(modifier = Modifier.size(10.dp()));
        });
    });
    test_intrinsics(vec![column], |asked| {
        assert_eq!(asked.min_height(INF), 10 * 3 + 2 * 5);
        assert_eq!(asked.max_height(INF), 10 * 3 + 2 * 5);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withArrangementSpacing_height
// and testColumn_withArrangementSpacing_width
#[test]
fn the_spacing_leaves_less_main_axis_space_to_a_child_asked_across() {
    let (row, column) = (cell(), cell());
    let (row_size, column_size) = (row.clone(), column.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (row_size, column_size) = (row_size.clone(), column_size.clone());
        Column!({
            Row!(
                modifier =
                    Modifier.width(15.dp()).height(IntrinsicSize::Min).then(size_into(&row_size)),
                horizontal_arrangement = Arrangement::spaced_by(5.dp()),
                {
                    Box!(modifier = Modifier);
                    Box!(modifier = Modifier.width(15.dp()).aspect_ratio(1.0));
                }
            );
            Column!(
                modifier = Modifier
                    .height(15.dp())
                    .width(IntrinsicSize::Min)
                    .then(size_into(&column_size)),
                vertical_arrangement = Arrangement::spaced_by(5.dp()),
                {
                    Box!(modifier = Modifier);
                    Box!(modifier = Modifier.height(15.dp()).aspect_ratio(1.0));
                }
            );
        });
    });
    assert_eq!(row.get().height, 15 - 5);
    assert_eq!(column.get().width, 15 - 5);
}

/// Answers its intrinsics as `RowColumnTest`'s width-in, height-out (or height-in, width-out)
/// child: across the main axis, half the size it's asked at.
struct HalfAcross {
    row: bool,
    main: i32,
}

impl MeasurePolicy for HalfAcross {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        if self.row {
            scope.layout(constraints.max_width, constraints.max_width / 2)
        } else {
            scope.layout(constraints.max_height / 2, constraints.max_height)
        }
    }
    fn min_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        if self.row { self.main / 10 } else { height / 2 }
    }
    fn min_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        if self.row { width / 2 } else { self.main / 10 }
    }
    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        if self.row { self.main * 2 } else { height / 2 }
    }
    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        if self.row { width / 2 } else { self.main * 2 }
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withWIHOChild_hasCorrectIntrinsicMeasurements
// and testColumn_withHIWOChild_hasCorrectIntrinsicMeasurements
#[test]
fn a_child_asked_across_gets_the_main_axis_space_left_by_its_siblings() {
    let (divider, column_divider) = (cell(), cell());
    let (row_size, column_size) = (divider.clone(), column_divider.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (row_size, column_size) = (row_size.clone(), column_size.clone());
        Column!({
            let (row_size, column_size) = (row_size.clone(), column_size.clone());
            Row!(modifier = Modifier.required_width(40.dp()).height(IntrinsicSize::Min), {
                let row_size = row_size.clone();
                Empty(
                    Modifier.required_width(10.dp()).fill_max_height().then(size_into(&row_size)),
                );
                Layout(Modifier, Rc::new(HalfAcross { row: true, main: 40 }), callback(|| {}));
            });
            Column!(modifier = Modifier.required_height(40.dp()).width(IntrinsicSize::Min), {
                let column_size = column_size.clone();
                Empty(
                    Modifier
                        .required_height(10.dp())
                        .fill_max_width()
                        .then(size_into(&column_size)),
                );
                Layout(Modifier, Rc::new(HalfAcross { row: false, main: 40 }), callback(|| {}));
            });
        });
    });
    assert_eq!(divider.get().height, (40 - 10) / 2);
    assert_eq!(column_divider.get().width, (40 - 10) / 2);
}

// ---- SizeTest ----------------------------------------------------------------------------------

/// A container with `modifier` around a square container: `SizeTest`'s intrinsics scene.
fn around_a_square(modifier: Modifier) -> Rc<dyn Fn()> {
    layout(move || {
        Container(modifier.clone(), None, None, callback(|| Empty(Modifier.aspect_ratio(1.0))));
    })
}

/// Asserts each intrinsic at each size: `[(size, min width, min height, max width, max
/// height)]`.
fn assert_intrinsics(asked: &mut Asked<'_, '_>, expected: &[(i32, i32, i32, i32, i32)]) {
    for &(size, min_width, min_height, max_width, max_height) in expected {
        assert_eq!(asked.min_width(size), min_width, "min width at {size}");
        assert_eq!(asked.min_height(size), min_height, "min height at {size}");
        assert_eq!(asked.max_width(size), max_width, "max width at {size}");
        assert_eq!(asked.max_height(size), max_height, "max height at {size}");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testMinWidthModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_minimum_width_is_the_least_of_its_width_intrinsics() {
    test_intrinsics(vec![around_a_square(Modifier.width_in(10.dp(), Dp::UNSPECIFIED))], |asked| {
        assert_intrinsics(
            asked,
            &[
                (0, 10, 0, 10, 0),
                (5, 10, 5, 10, 5),
                (35, 35, 35, 35, 35),
                (50, 50, 50, 50, 50),
                (INF, 10, 0, 10, 0),
            ],
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testMaxWidthModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_maximum_width_bounds_its_width_intrinsics() {
    test_intrinsics(vec![around_a_square(Modifier.width_in(Dp::UNSPECIFIED, 20.dp()))], |asked| {
        assert_intrinsics(asked, &[(0, 0, 0, 0, 0), (15, 15, 15, 15, 15), (INF, 0, 0, 0, 0)]);
        assert_eq!(asked.min_width(50), 20);
        assert_eq!(asked.max_width(50), 20);
        assert_eq!(asked.min_height(50), 50);
        assert_eq!(asked.max_height(50), 50);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testMinHeightModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_minimum_height_is_the_least_of_its_height_intrinsics() {
    test_intrinsics(vec![around_a_square(Modifier.height_in(30.dp(), Dp::UNSPECIFIED))], |asked| {
        assert_intrinsics(
            asked,
            &[(0, 0, 30, 0, 30), (15, 15, 30, 15, 30), (50, 50, 50, 50, 50), (INF, 0, 30, 0, 30)],
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testMaxHeightModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_maximum_height_bounds_its_height_intrinsics() {
    test_intrinsics(vec![around_a_square(Modifier.height_in(Dp::UNSPECIFIED, 40.dp()))], |asked| {
        assert_intrinsics(
            asked,
            &[(0, 0, 0, 0, 0), (15, 15, 15, 15, 15), (50, 50, 40, 50, 40), (INF, 0, 0, 0, 0)],
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testWidthModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_width_is_its_width_intrinsics() {
    test_intrinsics(vec![around_a_square(Modifier.width(10.dp()))], |asked| {
        assert_intrinsics(
            asked,
            &[(0, 10, 0, 10, 0), (35, 10, 35, 10, 35), (70, 10, 70, 10, 70), (INF, 10, 0, 10, 0)],
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testHeightModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_height_is_its_height_intrinsics() {
    test_intrinsics(vec![around_a_square(Modifier.height(10.dp()))], |asked| {
        assert_intrinsics(
            asked,
            &[(0, 0, 10, 0, 10), (15, 15, 10, 15, 10), (75, 75, 10, 75, 10), (INF, 0, 10, 0, 10)],
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testWidthHeightModifiers_hasCorrectIntrinsicMeasurements
#[test]
fn size_bounds_bound_both_intrinsics() {
    let bounds = Modifier.size_in(SizeSpec::bounds(10.dp(), 30.dp(), 20.dp(), 40.dp()));
    test_intrinsics(vec![around_a_square(bounds)], |asked| {
        assert_eq!(asked.min_width(0), 10);
        assert_eq!(asked.min_width(15), 15);
        assert_eq!(asked.min_width(50), 20);
        assert_eq!(asked.min_width(INF), 10);
        assert_eq!(asked.min_height(0), 30);
        assert_eq!(asked.min_height(35), 35);
        assert_eq!(asked.min_height(50), 40);
        assert_eq!(asked.min_height(INF), 30);
        assert_eq!(asked.max_width(0), 10);
        assert_eq!(asked.max_width(15), 15);
        assert_eq!(asked.max_width(50), 20);
        assert_eq!(asked.max_width(INF), 10);
        assert_eq!(asked.max_height(0), 30);
        assert_eq!(asked.max_height(35), 35);
        assert_eq!(asked.max_height(50), 40);
        assert_eq!(asked.max_height(INF), 30);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testMinSizeModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_minimum_size_is_the_least_of_both_intrinsics() {
    let bounds =
        Modifier.size_in(SizeSpec::bounds(20.dp(), 30.dp(), Dp::UNSPECIFIED, Dp::UNSPECIFIED));
    test_intrinsics(vec![around_a_square(bounds)], |asked| {
        assert_intrinsics(
            asked,
            &[
                (0, 20, 30, 20, 30),
                (10, 20, 30, 20, 30),
                (50, 50, 50, 50, 50),
                (INF, 20, 30, 20, 30),
            ],
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testMaxSizeModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_maximum_size_bounds_both_intrinsics() {
    let bounds =
        Modifier.size_in(SizeSpec::bounds(Dp::UNSPECIFIED, Dp::UNSPECIFIED, 40.dp(), 50.dp()));
    test_intrinsics(vec![around_a_square(bounds)], |asked| {
        assert_intrinsics(asked, &[(0, 0, 0, 0, 0), (15, 15, 15, 15, 15), (INF, 0, 0, 0, 0)]);
        assert_eq!(asked.min_width(50), 40);
        assert_eq!(asked.max_width(50), 40);
        assert_eq!(asked.min_height(75), 50);
        assert_eq!(asked.max_height(75), 50);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testPreferredSizeModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_size_is_both_intrinsics() {
    test_intrinsics(vec![around_a_square(Modifier.size((40.dp(), 50.dp())))], |asked| {
        assert_intrinsics(
            asked,
            &[
                (0, 40, 50, 40, 50),
                (35, 40, 50, 40, 50),
                (75, 40, 50, 40, 50),
                (INF, 40, 50, 40, 50),
            ],
        );
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testFillModifier_noChangeIntrinsicMeasurements
// and AspectRatioTest.kt, testAspectRatioModifier_intrinsicDimensions
#[test]
fn filling_leaves_the_intrinsics_alone() {
    for fill in
        [Modifier, Modifier.fill_max_width(), Modifier.fill_max_height(), Modifier.fill_max_size()]
    {
        let scene = layout(move || {
            Container(fill.clone().aspect_ratio(2.0), Some(30), Some(40), callback(|| {}));
        });
        test_intrinsics(vec![scene], |asked| {
            assert_eq!(asked.min_width(20), 40);
            assert_eq!(asked.min_width(INF), 30);
            assert_eq!(asked.max_width(20), 40);
            assert_eq!(asked.max_width(INF), 30);
            assert_eq!(asked.min_height(40), 20);
            assert_eq!(asked.min_height(INF), 40);
            assert_eq!(asked.max_height(40), 20);
            assert_eq!(asked.max_height(INF), 40);
        });
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testDefaultMinSizeModifier_hasCorrectIntrinsicMeasurements
#[test]
fn a_default_minimum_size_is_the_least_of_both_intrinsics() {
    test_intrinsics(vec![around_a_square(Modifier.default_min_size(40.dp(), 50.dp()))], |asked| {
        assert_eq!(asked.min_width(0), 40);
        assert_eq!(asked.min_width(35), 40);
        assert_eq!(asked.min_width(55), 55);
        assert_eq!(asked.min_width(INF), 40);
        assert_eq!(asked.min_height(0), 50);
        assert_eq!(asked.min_height(35), 50);
        assert_eq!(asked.min_height(55), 55);
        assert_eq!(asked.min_height(INF), 50);
        assert_eq!(asked.max_width(0), 40);
        assert_eq!(asked.max_width(55), 55);
        assert_eq!(asked.max_height(0), 50);
        assert_eq!(asked.max_height(55), 55);
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, test2DAlignedModifier_hasCorrectIntrinsicMeasurements
// and test1DAlignedModifier_hasCorrectIntrinsicMeasurements
#[test]
fn wrapping_the_content_leaves_its_intrinsics_alone() {
    for wrap in [
        Modifier.wrap_content_size_with(Alignment::TOP_START, false),
        Modifier.wrap_content_height_with(Alignment::CENTER_VERTICALLY, false),
    ] {
        let scene = layout(move || Empty(wrap.clone().aspect_ratio(2.0)));
        test_intrinsics(vec![scene], |asked| {
            assert_eq!(asked.min_width(0), 0);
            assert_eq!(asked.min_width(25), 25 * 2);
            assert_eq!(asked.min_width(INF), 0);
            assert_eq!(asked.min_height(50), 25);
            assert_eq!(asked.min_height(INF), 0);
            assert_eq!(asked.max_width(25), 25 * 2);
            assert_eq!(asked.max_width(INF), 0);
            assert_eq!(asked.max_height(50), 25);
            assert_eq!(asked.max_height(INF), 0);
        });
    }
}

/// Panics when asked any intrinsic, as `SizeTest`'s `ErrorIntrinsicsLayout`.
struct ErrorIntrinsics;

impl MeasurePolicy for ErrorIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        _: Constraints,
    ) -> MeasureResult {
        scope.layout(0, 0)
    }
    fn min_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("Error intrinsic")
    }
    fn min_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("Error intrinsic")
    }
    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("Error intrinsic")
    }
    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("Error intrinsic")
    }
}

#[composable]
fn ErrorLayout(modifier: Modifier) {
    Layout(modifier, Rc::new(ErrorIntrinsics), callback(|| {}));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testIntrinsicMeasurements_notQueriedWhenConstraintsAreFixed
#[test]
fn a_fixed_size_answers_its_intrinsics_without_asking_its_content() {
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        for size in [IntrinsicSize::Min, IntrinsicSize::Max] {
            Box!(modifier = Modifier.width(size), { ErrorLayout(Modifier.width(1.dp())) });
            Box!(modifier = Modifier.required_width(size), { ErrorLayout(Modifier.width(1.dp())) });
            Box!(modifier = Modifier.height(size), { ErrorLayout(Modifier.height(1.dp())) });
            Box!(modifier = Modifier.required_height(size), {
                ErrorLayout(Modifier.height(1.dp()))
            });
        }
    });
}

/// A box of `required` size on one axis holding a 400 px square image, sized to its own
/// intrinsic on the other: `SizeTest`'s `required*Changes*Intrinsic*` scenes.
fn image_size_under(outer: Modifier, inner: Modifier) -> IntSize {
    let size = cell();
    let recorded = size.clone();
    let _ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let bitmap = ImageBitmap::from_rgba_premultiplied(400, 400, vec![0; 400 * 400 * 4]);
        let inner = inner.clone();
        Box!(modifier = outer.clone().then(size_into(&recorded)), {
            let bitmap = bitmap.clone();
            Box!(modifier = inner.clone(), {
                Image!(bitmap.clone(), None);
            });
        });
    });
    size.get()
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, requiredWidthChangesMinIntrinsicHeight,
// requiredWidthChangesMaxIntrinsicHeight, requiredHeightChangesMinIntrinsicWidth and requiredHeightChangesMaxIntrinsicWidth
#[test]
fn a_required_size_on_one_axis_changes_the_intrinsics_on_the_other() {
    let required_width = Modifier.required_width(10.dp());
    let required_height = Modifier.required_height(10.dp());
    for size in [IntrinsicSize::Min, IntrinsicSize::Max] {
        assert_eq!(
            image_size_under(Modifier.height(size), required_width.clone()),
            IntSize::new(10, 10)
        );
        assert_eq!(
            image_size_under(Modifier.width(size), required_height.clone()),
            IntSize::new(10, 10)
        );
    }
}

/// Counts its measurements.
#[composable]
fn CountMeasures(modifier: Modifier, measures: Rc<Cell<u32>>) {
    let policy = remember(|| {
        compose_ui::measure_policy(move |scope, _, _| {
            measures.set(measures.get() + 1);
            scope.layout(0, 0)
        })
    });
    Layout(modifier, policy, callback(|| {}));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt, testModifiers_doNotCauseUnnecessaryRemeasure
#[test]
fn size_modifiers_dont_measure_their_content_again_when_a_sibling_changes() {
    let first = mutable_state_of(true);
    let handle = first.handle();
    let measures = Rc::new(Cell::new(0));
    let counted = measures.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let counted = counted.clone();
        Box!({
            if handle.get() {
                Box!(modifier = Modifier);
            } else {
                Row!({});
            }
            CountMeasures(Modifier.size(10.dp()), counted.clone());
            CountMeasures(Modifier.required_size(10.dp()), counted.clone());
            CountMeasures(
                Modifier.wrap_content_size_with(Alignment::BOTTOM_END, false),
                counted.clone(),
            );
            CountMeasures(Modifier.fill_max_size_with(0.8), counted.clone());
            CountMeasures(Modifier.default_min_size(10.dp(), 20.dp()), counted.clone());
        });
    });
    assert_eq!(measures.get(), 5);
    first.set(false);
    ui.wait_for_idle();
    assert_eq!(measures.get(), 5);
}

// ---- BoxTest, PaddingTest ----------------------------------------------------------------------

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/BoxTest.kt, testBox_hasCorrectIntrinsicMeasurements
// (without its `matchParentSize` child, which waits on that modifier and plays no part in the
// box's intrinsics)
#[test]
fn a_boxs_intrinsics_are_its_largest_childs() {
    let scene = layout(|| {
        Box!(|scope| {
            Empty(Modifier.align(scope, Alignment::TOP_START).aspect_ratio(2.0));
            ConstrainedBox(
                Constraints::fixed(90, 80),
                Modifier.align(scope, Alignment::BOTTOM_CENTER),
            );
        });
    });
    test_intrinsics(vec![scene], |asked| {
        assert_eq!(asked.min_width(0), 90);
        assert_eq!(asked.min_width(200), 400);
        assert_eq!(asked.min_width(INF), 90);
        assert_eq!(asked.min_height(0), 80);
        assert_eq!(asked.min_height(200), 100);
        assert_eq!(asked.min_height(INF), 80);
        assert_eq!(asked.max_width(0), 90);
        assert_eq!(asked.max_width(200), 400);
        assert_eq!(asked.max_width(INF), 90);
        assert_eq!(asked.max_height(0), 80);
        assert_eq!(asked.max_height(200), 100);
        assert_eq!(asked.max_height(INF), 80);
    });
}

/// Measures its only child with its constraints, as `PaddingTest`'s `TestBox`.
struct TestBox;

impl MeasurePolicy for TestBox {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        assert_eq!(measurables.len(), 1, "TestBox must have exactly one child");
        let placeable = scope.measure(measurables[0], constraints);
        scope.place_relative(&placeable, 0, 0);
        scope.layout(
            placeable.width.min(constraints.max_width),
            placeable.height.min(constraints.max_height),
        )
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, intrinsicMeasurements
#[test]
fn padding_adds_to_the_contents_intrinsics_and_leaves_it_less_room() {
    let scene = layout(|| {
        Layout(
            Modifier.padding(100.dp()),
            Rc::new(TestBox),
            callback(|| Empty(Modifier.aspect_ratio(2.0))),
        );
    });
    test_intrinsics(vec![scene], |asked| {
        let spacing = 200;
        let dimension = 300;
        let ratio = dimension - spacing;
        let (total_height, total_width) =
            ((ratio as f32 / 2.0).round() as i32 + spacing, ratio * 2 + spacing);
        assert_eq!(asked.min_width(0), spacing);
        assert_eq!(asked.min_width(dimension), total_width);
        assert_eq!(asked.min_width(INF), spacing);
        assert_eq!(asked.min_height(0), spacing);
        assert_eq!(asked.min_height(dimension), total_height);
        assert_eq!(asked.min_height(INF), spacing);
        assert_eq!(asked.max_width(0), spacing);
        assert_eq!(asked.max_width(dimension), total_width);
        assert_eq!(asked.max_width(INF), spacing);
        assert_eq!(asked.max_height(0), spacing);
        assert_eq!(asked.max_height(dimension), total_height);
        assert_eq!(asked.max_height(INF), spacing);
    });
}

// ---- IntrinsicTest -----------------------------------------------------------------------------

/// A layout measuring to `width` by `height` within its constraints, with fixed intrinsics, as
/// `IntrinsicTest`'s `FixedIntrinsicsBox`.
struct FixedIntrinsics {
    min_width: i32,
    width: i32,
    max_width: i32,
    min_height: i32,
    height: i32,
    max_height: i32,
}

/// `IntrinsicTest`'s box: min / measured / max widths 10, 20, 30 and heights 40, 50, 60.
const FIXED: FixedIntrinsics = FixedIntrinsics {
    min_width: 10,
    width: 20,
    max_width: 30,
    min_height: 40,
    height: 50,
    max_height: 60,
};

impl MeasurePolicy for FixedIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        scope.layout(
            constraints.constrain_width(self.width),
            constraints.constrain_height(self.height),
        )
    }
    fn min_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.min_width
    }
    fn min_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.min_height
    }
    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.max_width
    }
    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.max_height
    }
}

#[composable]
fn FixedIntrinsicsBox(modifier: Modifier) {
    Layout(modifier, Rc::new(FixedIntrinsics { ..FIXED }), callback(|| {}));
}

/// The size and position in the root of a fixed-intrinsics box with `modifier`, in a box, in
/// constraints `constraints` (as `IntrinsicTest`'s `ConstrainedBox`), and the constrained box's
/// size.
fn fixed_box_in(
    constraints: Option<Constraints>,
    modifier: Modifier,
) -> (IntSize, Offset, IntSize) {
    let (size, position, outer) = (cell(), Rc::new(Cell::new(Offset::new(-1.0, -1.0))), cell());
    let (recorded, at, constrained) = (size.clone(), position.clone(), outer.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (recorded, at) = (recorded.clone(), at.clone());
        let modifier =
            modifier.clone().on_globally_positioned(move |coordinates: LayoutCoordinates| {
                recorded.set(coordinates.size());
                at.set(coordinates.position_in_root());
            });
        let constrained = constrained.clone();
        Box!({
            let modifier = modifier.clone();
            match constraints {
                Some(constraints) => Layout(
                    size_into(&constrained),
                    Rc::new(ConstrainedPolicy(constraints)),
                    callback({
                        let modifier = modifier.clone();
                        move || FixedIntrinsicsBox(modifier.clone())
                    }),
                ),
                None => FixedIntrinsicsBox(modifier.clone()),
            }
        });
    });
    (size.get(), position.get(), outer.get())
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/IntrinsicTest.kt, testMinIntrinsicWidth,
// testMinIntrinsicHeight, testMaxIntrinsicWidth and testMaxIntrinsicHeight
#[test]
fn an_intrinsic_size_sizes_the_content_to_its_intrinsic_on_that_axis() {
    let at = |size: IntSize| (size, Offset::new(0.0, 0.0));
    let sized = |modifier: Modifier| {
        let (size, position, _) = fixed_box_in(None, modifier);
        (size, position)
    };
    assert_eq!(sized(Modifier.width(IntrinsicSize::Min)), at(IntSize::new(10, 50)));
    assert_eq!(sized(Modifier.height(IntrinsicSize::Min)), at(IntSize::new(20, 40)));
    assert_eq!(sized(Modifier.width(IntrinsicSize::Max)), at(IntSize::new(30, 50)));
    assert_eq!(sized(Modifier.height(IntrinsicSize::Max)), at(IntSize::new(20, 60)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/IntrinsicTest.kt, testMinIntrinsicWidth_respectsIncomingMaxConstraints,
// testMinIntrinsicWidth_respectsIncomingMinConstraints, testMinIntrinsicHeight_respectsMaxIncomingConstraints,
// testMinIntrinsicHeight_respectsMinIncomingConstraints, testMaxIntrinsicWidth_respectsMaxIncomingConstraints,
// testMaxIntrinsicWidth_respectsMinIncomingConstraints, testMaxIntrinsicHeight_respectsMaxIncomingConstraints
// and testMaxIntrinsicHeight_respectsMinIncomingConstraints
#[test]
fn an_intrinsic_size_stays_within_the_incoming_constraints() {
    let cases = [
        (Modifier.width(IntrinsicSize::Min), Constraints::new(0, 5, 0, INF), IntSize::new(5, 50)),
        (
            Modifier.width(IntrinsicSize::Min),
            Constraints::new(15, INF, 0, INF),
            IntSize::new(15, 50),
        ),
        (
            Modifier.height(IntrinsicSize::Min),
            Constraints::new(0, INF, 0, 35),
            IntSize::new(20, 35),
        ),
        (
            Modifier.height(IntrinsicSize::Min),
            Constraints::new(0, INF, 45, INF),
            IntSize::new(20, 45),
        ),
        (Modifier.width(IntrinsicSize::Max), Constraints::new(0, 25, 0, INF), IntSize::new(25, 50)),
        (
            Modifier.width(IntrinsicSize::Max),
            Constraints::new(35, INF, 0, INF),
            IntSize::new(35, 50),
        ),
        (
            Modifier.height(IntrinsicSize::Max),
            Constraints::new(0, INF, 0, 55),
            IntSize::new(20, 55),
        ),
        (
            Modifier.height(IntrinsicSize::Max),
            Constraints::new(0, INF, 65, INF),
            IntSize::new(20, 65),
        ),
    ];
    for (modifier, constraints, expected) in cases {
        let (size, position, outer) = fixed_box_in(Some(constraints), modifier);
        assert_eq!((size, outer), (expected, expected), "within {constraints:?}");
        assert_eq!(position, Offset::new(0.0, 0.0));
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/IntrinsicTest.kt, testRequiredMinIntrinsicWidth,
// testRequiredMinIntrinsicHeight, testRequiredMaxIntrinsicWidth and testRequiredMaxIntrinsicHeight
#[test]
fn a_required_intrinsic_size_overrides_fixed_incoming_constraints() {
    let fixed = Some(Constraints::fixed(100, 100));
    let cases = [
        (Modifier.required_width(IntrinsicSize::Min), IntSize::new(10, 50)),
        (Modifier.required_height(IntrinsicSize::Min), IntSize::new(20, 40)),
        (Modifier.required_width(IntrinsicSize::Max), IntSize::new(30, 50)),
        (Modifier.required_height(IntrinsicSize::Max), IntSize::new(20, 60)),
    ];
    for (modifier, expected) in cases {
        let size = cell();
        let measured = size.clone();
        let changed = modifier.on_size_changed(move |size: IntSize| measured.set(size));
        let _ = fixed_box_in(fixed, changed);
        assert_eq!(size.get(), expected);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/IntrinsicTest.kt, testMinIntrinsicWidth_intrinsicMeasurements,
// testMinIntrinsicHeight_intrinsicMeasurements, testMaxIntrinsicWidth_intrinsicMeasurements
// and testMaxIntrinsicHeight_intrinsicMeasurements
#[test]
fn an_intrinsic_size_answers_its_contents_intrinsic_of_that_size_on_its_axis() {
    let cases = [
        (Modifier.width(IntrinsicSize::Min), (10, 40, 10, 60)),
        (Modifier.height(IntrinsicSize::Min), (10, 40, 30, 40)),
        (Modifier.width(IntrinsicSize::Max), (30, 40, 30, 60)),
        (Modifier.height(IntrinsicSize::Max), (10, 60, 30, 60)),
    ];
    for (modifier, (min_width, min_height, max_width, max_height)) in cases {
        let scene = layout(move || FixedIntrinsicsBox(modifier.clone()));
        test_intrinsics(vec![scene], move |asked| {
            assert_eq!(asked.min_width(0), min_width);
            assert_eq!(asked.min_height(0), min_height);
            assert_eq!(asked.max_width(0), max_width);
            assert_eq!(asked.max_height(0), max_height);
        });
    }
}

/// Answers -1 for every intrinsic, as `testMaxIntrinsic_HandleNegative`'s policy.
struct NegativeIntrinsics;

impl MeasurePolicy for NegativeIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        _: Constraints,
    ) -> MeasureResult {
        scope.layout(0, 0)
    }
    fn min_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        -1
    }
    fn min_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        -1
    }
    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        -1
    }
    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        -1
    }
}

#[composable]
fn NegativeLayout(modifier: Modifier) {
    Layout(modifier, Rc::new(NegativeIntrinsics), callback(|| {}));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/IntrinsicTest.kt, testMaxIntrinsic_HandleNegative
#[test]
fn a_negative_intrinsic_sizes_the_content_to_zero() {
    let (min, max) = (cell(), cell());
    let (min_size, max_size) = (min.clone(), max.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (min_size, max_size) = (min_size.clone(), max_size.clone());
        Column!({
            let both = |size: IntrinsicSize, into: &Rc<Cell<IntSize>>| {
                Modifier.width(size).height(size).then(size_into(into))
            };
            NegativeLayout(both(IntrinsicSize::Min, &min_size));
            NegativeLayout(both(IntrinsicSize::Max, &max_size));
        });
    });
    assert_eq!((min.get(), max.get()), (IntSize::new(0, 0), IntSize::new(0, 0)));
}

/// Measures to 10 px within its constraints and answers 20 px intrinsics, counting the
/// intrinsics asked, as `IntrinsicTest`'s `IntrinsicCallCountingMeasurePolicy`.
struct CountingIntrinsics(Rc<Cell<u32>>);

impl CountingIntrinsics {
    fn ask(&self) -> i32 {
        self.0.set(self.0.get() + 1);
        20
    }
}

impl MeasurePolicy for CountingIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        scope.layout(constraints.constrain_width(10), constraints.constrain_height(10))
    }
    fn min_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.ask()
    }
    fn min_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.ask()
    }
    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.ask()
    }
    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        self.ask()
    }
}

#[composable]
fn CountingLayout(modifier: Modifier, calls: Rc<Cell<u32>>) {
    Layout(modifier, Rc::new(CountingIntrinsics(calls)), callback(|| {}));
}

/// Two counting layouts with `min` and `max` around them, in a column with `outer`: how many
/// intrinsics each was asked, and their sizes.
fn counted(
    outer: Modifier,
    min: Modifier,
    max: Modifier,
    row: bool,
) -> ((u32, IntSize), (u32, IntSize)) {
    let (min_calls, max_calls) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (min_size, max_size) = (cell(), cell());
    let (a, b, c, d) = (min_calls.clone(), max_calls.clone(), min_size.clone(), max_size.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (a, b, c, d) = (a.clone(), b.clone(), c.clone(), d.clone());
        let (min, max) = (min.clone(), max.clone());
        let children = move || {
            CountingLayout(min.clone().then(size_into(&c)), a.clone());
            CountingLayout(max.clone().then(size_into(&d)), b.clone());
        };
        if row {
            Row!(modifier = outer.clone(), { children() });
        } else {
            Column!(modifier = outer.clone(), { children() });
        }
    });
    ((min_calls.get(), min_size.get()), (max_calls.get(), max_size.get()))
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/IntrinsicTest.kt, testIntrinsicWidth_skippedWhenIncomingWidthIsFixed,
// testIntrinsicWidth_notSkippedWhenIncomingWidthIsNotFixed, testIntrinsicHeight_skippedWhenIncomingHeightIsFixed
// and testIntrinsicHeight_notSkippedWhenIncomingHeightIsNotFixed
#[test]
fn an_intrinsic_size_isnt_asked_when_the_incoming_size_is_fixed() {
    let fixed_width = |size: IntrinsicSize| Modifier.width(50.dp()).width(size);
    let ((min_calls, min_size), (max_calls, max_size)) =
        counted(Modifier, fixed_width(IntrinsicSize::Min), fixed_width(IntrinsicSize::Max), false);
    assert_eq!((min_calls, max_calls), (0, 0));
    assert_eq!((min_size, max_size), (IntSize::new(50, 10), IntSize::new(50, 10)));

    let ((min_calls, min_size), (max_calls, max_size)) = counted(
        Modifier.width_in(10.dp(), 100.dp()),
        Modifier.width(IntrinsicSize::Min),
        Modifier.width(IntrinsicSize::Max),
        false,
    );
    assert_eq!((min_calls, max_calls), (1, 1));
    assert_eq!((min_size, max_size), (IntSize::new(20, 10), IntSize::new(20, 10)));

    let fixed_height = |size: IntrinsicSize| Modifier.height(50.dp()).height(size);
    let ((min_calls, min_size), (max_calls, max_size)) = counted(
        Modifier,
        fixed_height(IntrinsicSize::Min),
        fixed_height(IntrinsicSize::Max),
        false,
    );
    assert_eq!((min_calls, max_calls), (0, 0));
    assert_eq!((min_size, max_size), (IntSize::new(10, 50), IntSize::new(10, 50)));

    let ((min_calls, min_size), (max_calls, max_size)) = counted(
        Modifier.height_in(10.dp(), 100.dp()),
        Modifier.height(IntrinsicSize::Min),
        Modifier.height(IntrinsicSize::Max),
        true,
    );
    assert_eq!((min_calls, max_calls), (1, 1));
    assert_eq!((min_size, max_size), (IntSize::new(10, 20), IntSize::new(10, 20)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/IntrinsicTest.kt, testRequiredWidth_notSkippedWhenIncomingWidthIsFixed
// and testRequiredHeight_notSkippedWhenIncomingHeightIsFixed
#[test]
fn a_required_intrinsic_size_is_asked_even_when_the_incoming_size_is_fixed() {
    let ((min_calls, _), (max_calls, _)) = counted(
        Modifier,
        Modifier.width(50.dp()).required_width(IntrinsicSize::Min),
        Modifier.width(50.dp()).required_width(IntrinsicSize::Max),
        false,
    );
    assert_eq!((min_calls, max_calls), (1, 1));
    let ((min_calls, _), (max_calls, _)) = counted(
        Modifier,
        Modifier.height(50.dp()).required_height(IntrinsicSize::Min),
        Modifier.height(50.dp()).required_height(IntrinsicSize::Max),
        false,
    );
    assert_eq!((min_calls, max_calls), (1, 1));
}

// ---- ScrollTest --------------------------------------------------------------------------------

/// Answers fixed intrinsics: min width 10, min height 20, max width 30, max height 40.
struct ScrolledIntrinsics;

impl MeasurePolicy for ScrolledIntrinsics {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        _: Constraints,
    ) -> MeasureResult {
        scope.layout(0, 0)
    }
    fn min_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        10
    }
    fn min_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        20
    }
    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        30
    }
    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        40
    }
}

/// A scroll in each orientation and layout direction, as `ScrollTest`'s configurations.
fn scroll_configurations() -> [(bool, LayoutDirection); 4] {
    [
        (true, LayoutDirection::Ltr),
        (true, LayoutDirection::Rtl),
        (false, LayoutDirection::Ltr),
        (false, LayoutDirection::Rtl),
    ]
}

fn scroller(vertical: bool) -> Modifier {
    let state = remember_scroll_state(0);
    if vertical { Modifier.vertical_scroll(state) } else { Modifier.horizontal_scroll(state) }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, intrinsicMeasurements
#[test]
fn a_scroll_answers_its_contents_intrinsics_unbounded() {
    for (vertical, direction) in scroll_configurations() {
        let scene = layout(move || {
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
                Layout(scroller(vertical), Rc::new(ScrolledIntrinsics), callback(|| {}));
            });
        });
        test_intrinsics(vec![scene], |asked| {
            assert_eq!(asked.min_width(INF), 10);
            assert_eq!(asked.min_height(INF), 20);
            assert_eq!(asked.max_width(INF), 30);
            assert_eq!(asked.max_height(INF), 40);
        });
    }
}

/// Records the size its content's intrinsics are asked at, as `ScrollTest`'s layout modifiers.
#[derive(Clone)]
struct RecordsAsked(Rc<Cell<i32>>);

impl PartialEq for RecordsAsked {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNodeElement for RecordsAsked {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(self.clone())
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<RecordsAsked>() {
            *node = self.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<RecordsAsked>() == Some(self)
    }
}

impl ModifierNode for RecordsAsked {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for RecordsAsked {
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
    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        self.0.set(height);
        scope.min_intrinsic_width(content, height)
    }
    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        self.0.set(width);
        scope.min_intrinsic_height(content, width)
    }
    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        self.0.set(height);
        scope.min_intrinsic_width(content, height)
    }
    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        self.0.set(width);
        scope.min_intrinsic_height(content, width)
    }
}

/// The size a scroll's content is asked its intrinsics at, under an intrinsic size of `size`
/// along the scroll (`main`) or across it.
fn asked_through_a_scroll(vertical: bool, main: bool, size: IntrinsicSize) -> i32 {
    let asked = Rc::new(Cell::new(0));
    let recorder = RecordsAsked(asked.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let intrinsic = if main == vertical { Modifier.height(size) } else { Modifier.width(size) };
        Box!(modifier = intrinsic.then(scroller(vertical)).then_custom(recorder.clone()));
    });
    asked.get()
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, minIntrinsic_mainAxis,
// minIntrinsic_crossAxis, maxIntrinsic_mainAxis and maxIntrinsic_crossAxis
#[test]
fn a_scroll_asks_its_contents_intrinsics_unbounded_along_it_and_bounded_across() {
    for (vertical, _) in scroll_configurations() {
        for size in [IntrinsicSize::Min, IntrinsicSize::Max] {
            assert_ne!(asked_through_a_scroll(vertical, true, size), INF);
            assert_eq!(asked_through_a_scroll(vertical, false, size), INF);
        }
    }
}

// ---- TextLayoutTest, images --------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextLayoutTest.kt, textLayout_intrinsicMeasurements
#[test]
fn a_texts_intrinsics_are_its_widest_word_its_whole_width_and_its_height_at_a_width() {
    let (single, double) = (cell(), cell());
    let (single_size, double_size) = (single.clone(), double.clone());
    let text = Rc::new(Cell::new(None::<(i32, i32, i32, i32, i32, i32, i32, i32, i32)>));
    let asked = text.clone();
    let _ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let (single_size, double_size, asked) =
            (single_size.clone(), double_size.clone(), asked.clone());
        Column!({
            let (single_size, double_size) = (single_size.clone(), double_size.clone());
            BasicText!(
                "aa ",
                modifier = Modifier.on_size_changed(move |size: IntSize| single_size.set(size))
            );
            BasicText!(
                "aa aa ",
                modifier = Modifier.on_size_changed(move |size: IntSize| double_size.set(size))
            );
            let asked = asked.clone();
            Layout(
                Modifier,
                compose_ui::measure_policy(move |scope, measurables, constraints| {
                    let width = scope.measure(measurables[0], constraints).width;
                    let _ = width;
                    let text = measurables[0];
                    // At the single text's width (its widest word), at the double's (all of it)
                    // and unbounded.
                    let word = scope.min_intrinsic_width(text, 0);
                    let whole = scope.max_intrinsic_width(text, 0);
                    asked.set(Some((
                        word,
                        whole,
                        scope.min_intrinsic_height(text, word),
                        scope.min_intrinsic_height(text, whole),
                        scope.min_intrinsic_height(text, INF),
                        scope.max_intrinsic_height(text, word),
                        scope.max_intrinsic_height(text, whole),
                        scope.max_intrinsic_height(text, INF),
                        0,
                    )));
                    scope.layout(0, 0)
                }),
                callback(|| BasicText!("aa aa ")),
            );
        });
    });
    let (
        word,
        whole,
        min_at_word,
        min_at_whole,
        min_unbounded,
        max_at_word,
        max_at_whole,
        max_unbounded,
        _,
    ) = text.get().expect("measured");
    let (single, double) = (single.get(), double.get());
    assert_eq!(word, single.width);
    assert_eq!(whole, double.width);
    assert!(min_at_word > single.height);
    assert_eq!(min_at_whole, single.height);
    assert_eq!(min_unbounded, single.height);
    assert!(max_at_word > single.height);
    assert_eq!(max_at_whole, single.height);
    assert_eq!(max_unbounded, single.height);
}

// ---- Text fields -------------------------------------------------------------------------------

// Review of D1204 (D1206): a text field's maximum intrinsic width, as CoreTextField's
// `maxIntrinsicWidth`, sizes it to its text on one line, the cursor beside it.
#[test]
fn a_text_field_at_its_maximum_intrinsic_width_doesnt_wrap() {
    let (intrinsic, unbounded) = (cell(), cell());
    let (at_max, natural) = (intrinsic.clone(), unbounded.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let (at_max, natural) = (at_max.clone(), natural.clone());
        Column!({
            BasicTextField!(
                "Hello there friend".to_string(),
                |_| {},
                modifier = Modifier.width(IntrinsicSize::Max).then(size_into(&at_max))
            );
            BasicTextField!(
                "Hello there friend".to_string(),
                |_| {},
                modifier = Modifier.then(size_into(&natural))
            );
        });
    });
    // The width it measures unbounded, one line high.
    assert_eq!(intrinsic.get(), unbounded.get());
}

// Review of D1204 (D1206): a text field's minimum intrinsic width is no wider than its maximum.
#[test]
fn a_text_fields_minimum_intrinsic_width_is_at_most_its_maximum() {
    for text in ["Hello there friend", "", "a"] {
        let scene = layout(move || {
            BasicTextField!(text.to_string(), |_| {});
        });
        test_intrinsics(vec![scene], |asked| {
            for height in [0, 20, INF] {
                assert!(asked.min_width(height) <= asked.max_width(height), "at {height}");
            }
        });
    }
}

// Review of D1204 (D1208): as a text's (Compose's `MultiParagraphLayoutCache.intrinsicHeight`
// caches its height by width), a text field's intrinsics lay its text out once for a width.
#[test]
fn a_text_fields_intrinsics_lay_its_text_out_once_for_the_same_width() {
    let tick = mutable_state_of(0);
    let ticks = tick.handle();
    let (measurer, counter) =
        CountingTextMeasurer::new(compose_ui::platform::FixedTextMeasurer::new());
    let host = compose_ui::platform::UiHost::new(
        IntSize::new(300, 300),
        compose_ui::Density::new(1.0),
        Box::new(measurer),
    );
    let mut ui = ComposeTestRule::with_host(host, move || {
        Layout(
            Modifier,
            compose_ui::measure_policy(move |scope, measurables, _| {
                let _ = ticks.get();
                let field = measurables[0];
                let _ = scope.max_intrinsic_height(field, 200);
                let _ = scope.min_intrinsic_height(field, 200);
                let placeable = scope.measure(field, Constraints::new(0, 200, 0, 300));
                scope.place(&placeable, 0, 0);
                scope.layout(200, placeable.height)
            }),
            callback(|| BasicTextField!("Field text".to_string(), |_| {})),
        );
    });
    counter.clear();
    for step in 1..=3 {
        tick.set(step);
        ui.wait_for_idle();
    }
    // Laid out once for that width, before the counts, and kept.
    assert_eq!(counter.of("Field text").probes, 0);
}

// ---- Layouts without intrinsics ----------------------------------------------------------------

/// Asks `content`'s maximum intrinsic width from its parent's measurement.
fn ask_the_intrinsic_width_of(content: impl Fn() + 'static) {
    let content = layout(content);
    test_intrinsics(vec![content], |asked| {
        asked.max_width(INF);
    });
}

// Review of D1204 (no Compose test pins it): Compose's `SubcomposeLayout` panics with
// `NoIntrinsicsMessage` (ui/layout/SubcomposeLayout.kt) when asked its intrinsics.
// (`NoIntrinsicsMessage`: lazy lists are built on it in Compose)
#[test]
#[should_panic(
    expected = "Asking for intrinsic measurements of SubcomposeLayout layouts is not supported"
)]
fn a_lazy_lists_intrinsics_panic() {
    ask_the_intrinsic_width_of(|| {
        LazyColumn!(|scope: LazyListScope| {
            scope.items(3, |index| BasicText!("Row {}", index));
        });
    });
}

// Review of D1204 (no Compose test pins it): Compose's `SubcomposeLayout` panics with
// `NoIntrinsicsMessage` (ui/layout/SubcomposeLayout.kt) when asked its intrinsics.
// (lazy grids are built on it in Compose)
#[test]
#[should_panic(
    expected = "Asking for intrinsic measurements of SubcomposeLayout layouts is not supported"
)]
fn a_lazy_grids_intrinsics_panic() {
    ask_the_intrinsic_width_of(|| {
        LazyVerticalGrid!(GridCells::Fixed(2), |scope: LazyGridScope| {
            scope.items(3, |index| BasicText!("Cell {}", index));
        });
    });
}

// Review of D1204 (no Compose test pins it): Compose's `SubcomposeLayout` panics with
// `NoIntrinsicsMessage` (ui/layout/SubcomposeLayout.kt) when asked its intrinsics.
// (`BoxWithConstraints` is built on it in Compose)
#[test]
#[should_panic(
    expected = "Asking for intrinsic measurements of SubcomposeLayout layouts is not supported"
)]
fn box_with_constraints_intrinsics_panic() {
    ask_the_intrinsic_width_of(|| {
        BoxWithConstraints!(|_scope: BoxWithConstraintsScope| {
            BasicText!("content");
        });
    });
}
