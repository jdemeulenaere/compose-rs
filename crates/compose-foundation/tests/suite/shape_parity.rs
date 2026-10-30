//! The corner-based shapes and `CornerSize` against Compose's shape tests: outlines, right to
//! left, corners scaled to fit, equality and `copy`.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::shape::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Density, LayoutDirection, Modifier, Offset, Outline, PathCommand, PointerEvent, Rect, Shape,
    Size, UnitExt,
};

const SIZE: Size = Size { width: 100.0, height: 150.0 };

fn outline(shape: &Shape, direction: LayoutDirection) -> Outline {
    shape.outline_in(SIZE, &Density::new(2.0), direction)
}

/// The points of a generic outline's path, its offset applied: what Compose's tests compare
/// (`assertPathsEquals`).
fn points(outline: &Outline) -> Vec<(f32, f32)> {
    let Outline::Generic(path, offset) = outline else { panic!("not a path: {outline:?}") };
    let mut points = Vec::new();
    for command in path.commands() {
        match command {
            PathCommand::MoveTo(point) | PathCommand::LineTo(point) => {
                points.push((point.x + offset.x, point.y + offset.y))
            }
            PathCommand::Close => {}
            other => panic!("a cut corner's path has lines only: {other:?}"),
        }
    }
    assert!(matches!(path.commands().last(), Some(PathCommand::Close)));
    points
}

/// The cut path of `cuts` (top left, top right, bottom right, bottom left) over `SIZE`.
fn cut_points(cuts: [f32; 4]) -> Vec<(f32, f32)> {
    let (width, height) = (SIZE.width, SIZE.height);
    vec![
        (0.0, cuts[0]),
        (cuts[0], 0.0),
        (width - cuts[1], 0.0),
        (width, cuts[1]),
        (width, height - cuts[2]),
        (width - cuts[2], height),
        (cuts[3], height),
        (0.0, height - cuts[3]),
    ]
}

fn bounds() -> Rect {
    Rect::new(0.0, 0.0, SIZE.width, SIZE.height)
}

// ---- RoundedCornerShape ------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, roundedUniformCorners
#[test]
fn rounded_uniform_corners_in_percent() {
    let outline = outline(&RoundedCornerShape(25), LayoutDirection::Ltr);
    assert_eq!(outline, Outline::Rounded(bounds(), [25.0; 4]));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, roundedDifferentRadius
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, roundedDifferentRadius_rtl
#[test]
fn rounded_different_corners_mirror_right_to_left() {
    let shape = RoundedCornerShape((12.0, 22.0, 32.0, 42.0));
    assert_eq!(
        outline(&shape, LayoutDirection::Ltr),
        Outline::Rounded(bounds(), [12.0, 22.0, 32.0, 42.0])
    );
    assert_eq!(
        outline(&shape, LayoutDirection::Rtl),
        Outline::Rounded(bounds(), [22.0, 12.0, 42.0, 32.0])
    );
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, createsRectangleOutlineForZeroSizedCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, createsRectangleOutlineForZeroSizedCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, createsRectangleOutlineForZeroSizedCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, createsRectangleOutlineForZeroSizedCorners
#[test]
fn zero_corners_are_a_rectangle() {
    for shape in [
        RoundedCornerShape((0.0, 0.0, 0.0, 0.0)),
        CutCornerShape((0.0, 0.0, 0.0, 0.0)),
        AbsoluteRoundedCornerShape((0.0, 0.0, 0.0, 0.0)),
        AbsoluteCutCornerShape((0.0, 0.0, 0.0, 0.0)),
    ] {
        assert_eq!(outline(&shape, LayoutDirection::Ltr), Outline::Rectangle(bounds()));
    }
}

/// The four corners of an example shape: 4 px, 3 px, 3 dp and 50 %.
fn example() -> (CornerSize, CornerSize, CornerSize, CornerSize) {
    (CornerSize::Px(4.0), CornerSize::Px(3.0), CornerSize::Dp(3.dp()), CornerSize::percent(50))
}

/// Equal shapes for the same corners, not for others, nor across kinds.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, roundedCornerShapesAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, objectsWithTheSameCornersAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, objectsWithDifferentCornersAreNotEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, notEqualsToCutCornersWithTheSameSizes
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, cutCornerShapesAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, objectsWithTheSameCornersAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, objectsWithDifferentCornersAreNotEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, roundedCornerShapesAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, objectsWithTheSameCornersAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, objectsWithDifferentCornersAreNotEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, notEqualsToCutCornersWithTheSameSizes
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, cutCornerShapesAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, objectsWithTheSameCornersAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, objectsWithDifferentCornersAreNotEquals
#[test]
fn shapes_are_equal_by_kind_and_corners() {
    let makers: [fn(_) -> Shape; 4] = [
        |corners| RoundedCornerShape(corners),
        |corners| CutCornerShape(corners),
        |corners| AbsoluteRoundedCornerShape(corners),
        |corners| AbsoluteCutCornerShape(corners),
    ];
    let (a, b, c, d) = example();
    let other = (a, CornerSize::Px(5.0), c, d);
    for (index, make) in makers.iter().enumerate() {
        assert_eq!(make((a, b, c, d)), make((a, b, c, d)), "kind {index}");
        assert_ne!(make((a, b, c, d)), make(other), "kind {index}");
        for (other_index, other_make) in makers.iter().enumerate() {
            if other_index != index {
                assert_ne!(
                    make((a, b, c, d)),
                    other_make((a, b, c, d)),
                    "{index} and {other_index}"
                );
            }
        }
    }
    assert_eq!(RoundedCornerShape(12.dp()), RoundedCornerShape(12.dp()));
    assert_eq!(CutCornerShape(10.0), CutCornerShape(10.0));
}

/// `copy` with every corner, and with some (the others kept).
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, roundedCornerUpdateAllCornerSize
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, roundedCornerUpdateTwoCornerSizes
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt, copyHasCorrectDefaults
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, cutCornerUpdateAllCornerSize
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, cutCornerUpdateTwoCornerSizes
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, copyHasCorrectDefaults
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, roundedCornerUpdateAllCornerSize
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, roundedCornerUpdateTwoCornerSizes
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, copyHasCorrectDefaults
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, cutCornerUpdateAllCornerSize
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, cutCornerUpdateTwoCornerSizes
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, copyHasCorrectDefaults
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerBasedShapeTest.kt, copyingUsesCorrectDefaults
#[test]
fn copy_replaces_the_corners_given() {
    let makers: [fn([CornerSize; 4]) -> Shape; 4] = [
        |corners| RoundedCornerShape(corners),
        |corners| CutCornerShape(corners),
        |corners| AbsoluteRoundedCornerShape(corners),
        |corners| AbsoluteCutCornerShape(corners),
    ];
    for make in makers {
        let shape = make(CornerSize::Px(10.0).corner_sizes());
        assert_eq!(shape.copy(CornerSize::Dp(5.dp())), make(CornerSize::Dp(5.dp()).corner_sizes()));
        // Two corners: the top start and the bottom end, the others kept.
        let mut corners = shape.corners().unwrap();
        corners[0] = CornerSize::Dp(3.dp());
        corners[2] = CornerSize::percent(50);
        let copied = shape.copy(corners);
        assert_eq!(
            copied.corners().unwrap(),
            [
                CornerSize::Dp(3.dp()),
                CornerSize::Px(10.0),
                CornerSize::percent(50),
                CornerSize::Px(10.0)
            ]
        );
        assert_eq!(copied, make((3.dp(), 10.0, 50, 10.0).corner_sizes()));
        // No corner changed: the same shape.
        assert_eq!(shape.copy(shape.corners().unwrap()), shape);
    }
}

// ---- CutCornerShape ----------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, cutCornersUniformCorners
#[test]
fn cut_uniform_corners() {
    assert_eq!(
        points(&outline(&CutCornerShape(10.0), LayoutDirection::Ltr)),
        cut_points([10.0; 4])
    );
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, cutCornersDifferentCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt, cutCornersDifferentCorners_rtl
#[test]
fn cut_different_corners_mirror_right_to_left() {
    let shape = CutCornerShape((12.0, 22.0, 32.0, 42.0));
    assert_eq!(
        points(&outline(&shape, LayoutDirection::Ltr)),
        cut_points([12.0, 22.0, 32.0, 42.0])
    );
    assert_eq!(
        points(&outline(&shape, LayoutDirection::Rtl)),
        cut_points([22.0, 12.0, 42.0, 32.0])
    );
}

// ---- The absolute shapes -----------------------------------------------------------------------

/// The absolute shapes don't mirror right to left.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, roundedUniformCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt, roundedDifferentRadius
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, cutCornersUniformCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt, cutCornersDifferentCorners
#[test]
fn the_absolute_shapes_keep_their_corners_right_to_left() {
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        assert_eq!(
            outline(&AbsoluteRoundedCornerShape(25), direction),
            Outline::Rounded(bounds(), [25.0; 4])
        );
        assert_eq!(
            outline(&AbsoluteRoundedCornerShape((12.0, 22.0, 32.0, 42.0)), direction),
            Outline::Rounded(bounds(), [12.0, 22.0, 32.0, 42.0]),
            "{direction:?}"
        );
        assert_eq!(
            points(&outline(&AbsoluteCutCornerShape(10.0), direction)),
            cut_points([10.0; 4])
        );
        assert_eq!(
            points(&outline(&AbsoluteCutCornerShape((12.0, 22.0, 32.0, 42.0)), direction)),
            cut_points([12.0, 22.0, 32.0, 42.0]),
            "{direction:?}"
        );
    }
}

// ---- CornerBasedShape --------------------------------------------------------------------------

fn radii(shape: &Shape, size: Size, direction: LayoutDirection) -> [f32; 4] {
    match shape.outline_in(size, &Density::new(2.0), direction) {
        Outline::Rounded(_, radii) => radii,
        Outline::Rectangle(_) => [0.0; 4],
        other => panic!("{other:?}"),
    }
}

/// Corners resolved against the size and density: px, dp (at 2) and percent.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerBasedShapeTest.kt, createOutlineCalledWithCorrectParams
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerBasedShapeTest.kt, layoutDirectionIsPassed
#[test]
fn corners_resolve_against_the_size_and_density() {
    let shape = RoundedCornerShape(example());
    let size = Size::new(100.0, 50.0);
    assert_eq!(radii(&shape, size, LayoutDirection::Ltr), [4.0, 3.0, 6.0, 25.0]);
    // Right to left, the same corners, mirrored.
    assert_eq!(radii(&shape, size, LayoutDirection::Rtl), [3.0, 4.0, 25.0, 6.0]);
}

/// The two top corners each at most the shorter side, whichever side that is.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerBasedShapeTest.kt, topCornersSizesAreNotLargerThenMinDimension
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerBasedShapeTest.kt, topCornersUse100Percent
#[test]
fn top_corners_are_at_most_the_shorter_side() {
    for shape in [RoundedCornerShape((10.0, 10.0, 0.0, 0.0)), RoundedCornerShape((100, 100, 0, 0))]
    {
        for size in [Size::new(6.0, 4.0), Size::new(4.0, 6.0)] {
            assert_eq!(radii(&shape, size, LayoutDirection::Ltr), [4.0, 4.0, 0.0, 0.0], "{size:?}");
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerBasedShapeTest.kt, overSizedEqualCornerSizes
#[test]
fn oversized_equal_corners_scale_to_half_the_shorter_side() {
    let shape = RoundedCornerShape(75);
    for size in [Size::new(6.0, 4.0), Size::new(4.0, 6.0)] {
        assert_eq!(radii(&shape, size, LayoutDirection::Ltr), [2.0; 4], "{size:?}");
    }
}

/// Oversized start corners scale down together, and end corners together, each pair keeping its
/// proportions.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerBasedShapeTest.kt, overSizedCornerSizesShouldProportionallyScale
#[test]
fn oversized_corners_scale_proportionally() {
    let shape = RoundedCornerShape((90, 30, 90, 30));
    for size in [Size::new(15.0, 10.0), Size::new(10.0, 15.0)] {
        assert_eq!(radii(&shape, size, LayoutDirection::Ltr), [7.5, 2.5, 7.5, 2.5], "{size:?}");
    }
}

/// A 40 px top start with a 0 px bottom start on a 50 px-high box stays 40 px (compose-rs once
/// clamped each corner to half the shorter side).
#[test]
fn a_corner_larger_than_half_the_side_stays_when_its_pair_fits() {
    let shape = RoundedCornerShape((40.0, 0.0, 0.0, 0.0));
    assert_eq!(radii(&shape, Size::new(100.0, 50.0), LayoutDirection::Ltr), [40.0, 0.0, 0.0, 0.0]);
}

/// A negative corner panics, as Compose throws.
#[test]
#[should_panic(expected = "can't be negative")]
fn a_negative_corner_panics() {
    let _ = RoundedCornerShape(-1.0).outline(Size::new(10.0, 10.0), &Density::new(1.0));
}

// ---- CornerSize --------------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerSizeTest.kt, pxCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerSizeTest.kt, dpCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerSizeTest.kt, intPercentCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerSizeTest.kt, zeroCorners
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerSizeTest.kt, pxCornersAreEquals
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerSizeTest.kt, dpCornersAreEquals
#[test]
fn corner_sizes_in_px_dp_and_percent() {
    let (size, density) = (Size::new(150.0, 300.0), Density::new(2.5));
    assert_eq!(CornerSize::Px(24.0).to_px(size, &density), 24.0);
    assert_eq!(CornerSize::Dp(5.dp()).to_px(size, &density), 12.5);
    assert_eq!(CornerSize::percent(15).to_px(size, &density), 22.5);
    assert_eq!(ZERO_CORNER_SIZE.to_px(size, &density), 0.0);
    assert_eq!(CornerSize::Px(24.0), CornerSize::Px(24.0));
    assert_eq!(CornerSize::Dp(8.dp()), CornerSize::Dp(8.dp()));
}

#[test]
#[should_panic(expected = "range of [0, 100]")]
fn a_percent_outside_0_to_100_panics() {
    let _ = CornerSize::percent(101);
}

// ---- GenericShape ------------------------------------------------------------------------------

/// A generic shape's outline is the path its builder draws, given the size and direction.
#[test]
fn a_generic_shape_outlines_its_builders_path() {
    let triangle = GenericShape(|path, size, direction| {
        let tip = if direction == LayoutDirection::Ltr { 0.0 } else { size.width };
        path.move_to(tip, 0.0);
        path.line_to(size.width, size.height);
        path.line_to(0.0, size.height);
        path.close();
    });
    let ltr = outline(&triangle, LayoutDirection::Ltr);
    assert!(ltr.contains(Offset::new(20.0, 140.0)) && !ltr.contains(Offset::new(90.0, 10.0)));
    let rtl = outline(&triangle, LayoutDirection::Rtl);
    assert!(rtl.contains(Offset::new(90.0, 140.0)) && !rtl.contains(Offset::new(10.0, 10.0)));
    // A shape is the same as itself only, as Compose's lambda-holding one.
    assert_eq!(triangle.clone(), triangle);
    assert_ne!(GenericShape(|_, _, _| {}), GenericShape(|_, _, _| {}));
}

/// A border of half the shorter side strokes (the stroke covers it all, as Compose's
/// `strokeWidthPx * 2 > size.minDimension` fills only past it); past half (an odd side, the
/// stroke rounded up to its half) it fills.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderRendererTest.kt, drawBorder_biggerThanBounds_fills (the rule at its edge: BorderRenderer.kt, shouldFillArea)
#[test]
fn a_border_of_half_the_side_is_a_stroke_and_past_half_a_fill() {
    use compose_foundation::layout::*;
    use compose_ui::{Color, IntSize, Modifier, RECTANGLE_SHAPE};
    let draw = |side: f32, width: f32| {
        let ui = compose_test::ComposeTestRule::sized(IntSize::new(100, 100), move || {
            let density = compose_ui::LOCAL_DENSITY.current().density;
            Box!(
                modifier = Modifier.size((side / density).dp()).border_with(
                    (width / density).dp(),
                    Color::RED,
                    RECTANGLE_SHAPE
                )
            );
        });
        ui.dump_display_list()
    };
    let half = draw(40.0, 20.0);
    assert!(half.contains("rect 10,10 20x20 #FFFF0000 stroke 20"), "{half}");
    let past = draw(41.0, 21.0);
    assert!(past.contains("rect 0,0 41x41 #FFFF0000\n"), "{past}");
}

// ---- A custom shape reading state ---------------------------------------------------------------

/// A 40 dp clickable clipped by a remembered `GenericShape` cutting its top left corner by what
/// `cut` holds, by `clipped`'s form of the clip; each tap inside it counts.
fn clipped_by_a_shape_reading_state(
    clicks: Rc<Cell<u32>>,
    cut: MutableState<f32>,
    clipped: fn(Shape) -> Modifier,
) -> ComposeTestRule {
    compose_test(move || {
        let clicks = clicks.clone();
        let shape = remember(|| {
            GenericShape(move |path, size, _| {
                let cut = cut.get();
                path.move_to(cut, 0.0);
                path.line_to(size.width, 0.0);
                path.line_to(size.width, size.height);
                path.line_to(0.0, size.height);
                path.line_to(0.0, cut);
                path.close();
            })
        });
        Box!(
            modifier = Modifier
                .size(40.dp())
                .then(clipped(shape))
                .background(compose_ui::Color::RED)
                .clickable(move || clicks.set(clicks.get() + 1))
        );
    })
}

/// Where the path the list clips to or casts its shadow from starts: the cut, by the shape above.
fn cut_drawn(ui: &mut ComposeTestRule) -> f32 {
    let list = ui.host().display_list();
    let path = list.ops.iter().find_map(|op| match op {
        DrawOp::ClipPath { path, .. } => Some(path.clone()),
        DrawOp::DrawShadow { outline: Outline::Generic(path, _), .. } => Some(path.clone()),
        _ => None,
    });
    match path.expect("a path").commands().first() {
        Some(PathCommand::MoveTo(point)) => point.x,
        other => panic!("not the shape's path: {other:?}"),
    }
}

/// A shape reading state in its outline makes the outline again when that state changes, as
/// Compose runs `createOutline` inside `observeReads` on every layer update: what clips and is
/// hit follows each write, the first and the later ones (a cached outline read nothing, so it
/// missed them all: D1063).
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeCoordinator.kt, updateLayerParameters
fn a_shape_reading_state_follows_each_write(clipped: fn(Shape) -> Modifier) {
    let clicks = Rc::new(Cell::new(0));
    let cut = mutable_state_of(10.0f32);
    let mut ui = clipped_by_a_shape_reading_state(clicks.clone(), cut.handle(), clipped);
    let tap = |ui: &mut ComposeTestRule, at: Offset| {
        ui.host().send_pointer(PointerEvent::press(at));
        ui.host().send_pointer(PointerEvent::release(at));
        ui.wait_for_idle();
    };
    let inside_at_10 = Offset::new(15.0, 1.0);
    let inside_at_20 = Offset::new(25.0, 1.0);
    assert_eq!(cut_drawn(&mut ui), 10.0);
    tap(&mut ui, inside_at_10);
    assert_eq!(clicks.get(), 1, "inside the clip");
    for next in [20.0f32, 30.0] {
        cut.set(next);
        ui.wait_for_idle();
        assert_eq!(cut_drawn(&mut ui), next, "clips to the cut after each write");
    }
    let before = clicks.get();
    tap(&mut ui, inside_at_10);
    tap(&mut ui, inside_at_20);
    assert_eq!(clicks.get(), before, "cut out by the writes: not hit");
    cut.set(5.0);
    ui.wait_for_idle();
    assert_eq!(cut_drawn(&mut ui), 5.0);
    tap(&mut ui, inside_at_10);
    assert_eq!(clicks.get(), before + 1, "the last write opens it again");
}

#[test]
fn a_clip_to_a_shape_reading_state_follows_each_write() {
    a_shape_reading_state_follows_each_write(|shape| Modifier.clip(shape));
}

#[test]
fn a_graphics_layer_clipping_to_a_shape_reading_state_follows_each_write() {
    a_shape_reading_state_follows_each_write(|shape| {
        Modifier.graphics_layer(move |layer| {
            layer.shape = shape.clone();
            layer.clip = true;
        })
    });
}

/// Its shadow's outline follows too: a shadow is the layer's outline.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeCoordinator.kt, updateLayerParameters
#[test]
fn a_shadow_of_a_shape_reading_state_follows_each_write() {
    let cut = mutable_state_of(10.0f32);
    let mut ui = clipped_by_a_shape_reading_state(Rc::default(), cut.handle(), |shape| {
        Modifier.graphics_layer(move |layer| {
            layer.shape = shape.clone();
            layer.shadow_elevation = 4.0;
        })
    });
    assert_eq!(cut_drawn(&mut ui), 10.0);
    for next in [20.0f32, 30.0] {
        cut.set(next);
        ui.wait_for_idle();
        assert_eq!(cut_drawn(&mut ui), next, "the shadow's outline after each write");
    }
}
