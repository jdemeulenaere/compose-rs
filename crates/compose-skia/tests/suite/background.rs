//! Compose's `BackgroundTest` (foundation, device tests): `background` with a color or a brush in
//! a rectangle or a circle; the color, brush, alpha and shape changed; shapes reading state as
//! they make their outline; the layout direction a shape gets; the modifier's equality; and the
//! semantics a background sets. Drawn by Skia and checked with the test utilities' `assertShape`.
//!
//! `testInspectableParameter1` and `testInspectableParameter2` are n/a: compose-rs has no
//! inspector (`InspectableValue`).

use std::rc::Rc;

use compose::foundation::layout::Box as Container;
use compose::foundation::shape::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    Alignment, Brush, Color, CornerRadius, Density, LOCAL_DENSITY, LOCAL_LAYOUT_DIRECTION,
    LayoutDirection, Modifier, Outline, Path, RoundRect, SemanticsProperties, Shape, Size, UnitExt,
};

use super::advanced_border::{rule, shape};

const CONTENT_TAG: &str = "Content";
const SEMANTICS_TAG: &str = "semantics-test-tag";

const MAGENTA: Color = Color::from_argb(0xFFFF_00FF);
const GRAY: Color = Color::from_argb(0xFF88_8888);

/// The content in a box tagged [`CONTENT_TAG`], as the test's `SemanticParent`.
fn semantic_parent(content: impl Fn() + 'static) -> ComposeTestRule {
    let content = Rc::new(content);
    rule(move || {
        let content = content.clone();
        Container(
            Modifier.test_tag(CONTENT_TAG),
            Alignment::TOP_START,
            false,
            callback(move |_| content()),
        );
    })
}

/// A box of `size` px (as the test's `Modifier.size(40f.toDp())`) with `modifier` (made as it's
/// composed) after its size.
fn sized(size: f32, modifier: impl Fn() -> Modifier + 'static) -> ComposeTestRule {
    semantic_parent(move || {
        let side = LOCAL_DENSITY.current().to_dp(size);
        Container(
            Modifier.size(side).then(modifier()),
            Alignment::TOP_START,
            false,
            callback(|_| {}),
        );
    })
}

/// The test utilities' `assertShape(density, shape, shapeColor, backgroundColor,
/// antiAliasingGap)` on the content's pixels.
fn assert_shape(
    ui: &mut ComposeTestRule,
    shape: Shape,
    shape_color: Color,
    background_color: Option<Color>,
    anti_aliasing_gap: Option<f32>,
) {
    let density = ui.density();
    let image = ui.on_node_with_tag(CONTENT_TAG).capture_to_image();
    let mut check = ShapeCheck::new(density, shape, shape_color, background_color, &image);
    if let Some(gap) = anti_aliasing_gap {
        check.anti_aliasing_gap = gap;
    }
    image.assert_shape_with(&check);
}

/// A 20 px white box centered in a 40 px magenta one.
fn centered_white(inner: Modifier) -> ComposeTestRule {
    semantic_parent(move || {
        let density = LOCAL_DENSITY.current();
        let inner = inner.clone();
        Container(
            Modifier.size(density.to_dp(40.0)).background(MAGENTA),
            Alignment::CENTER,
            false,
            callback(move |_| {
                Container(
                    Modifier.size(density.to_dp(20.0)).then(inner.clone()),
                    Alignment::TOP_START,
                    false,
                    callback(|_| {}),
                );
            }),
        );
    })
}

/// A 20 px white rectangle in the middle of the magenta.
fn assert_centered_white(ui: &mut ComposeTestRule) {
    let density = ui.density();
    let image = ui.on_node_with_tag(CONTENT_TAG).capture_to_image();
    let mut check = ShapeCheck::new(density, Shape::Rectangle, Color::WHITE, Some(MAGENTA), &image);
    check.shape_size = Size::new(20.0, 20.0);
    image.assert_shape_with(&check);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_colorRect
#[test]
fn background_color_rect() {
    let mut ui = centered_white(Modifier.background(Color::WHITE));
    assert_centered_white(&mut ui);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_brushRect
#[test]
fn background_brush_rect() {
    let mut ui = centered_white(Modifier.background(Brush::Solid(Color::WHITE)));
    assert_centered_white(&mut ui);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_colorCircle
#[test]
fn background_color_circle() {
    let mut ui =
        sized(40.0, || Modifier.background(MAGENTA).background_with(Color::WHITE, CIRCLE_SHAPE));
    assert_shape(&mut ui, CIRCLE_SHAPE, Color::WHITE, Some(MAGENTA), Some(2.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_brushCircle
#[test]
fn background_brush_circle() {
    let mut ui = sized(40.0, || {
        Modifier.background(MAGENTA).background_with(Brush::Solid(Color::WHITE), CIRCLE_SHAPE)
    });
    assert_shape(&mut ui, CIRCLE_SHAPE, Color::WHITE, Some(MAGENTA), Some(2.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_changeColor
#[test]
fn background_change_color() {
    let color = mutable_state_of(Color::WHITE);
    let current = color.handle();
    let mut ui = sized(40.0, move || Modifier.background(current.get()));
    assert_shape(&mut ui, Shape::Rectangle, Color::WHITE, None, None);
    color.set(MAGENTA);
    ui.wait_for_idle();
    assert_shape(&mut ui, Shape::Rectangle, MAGENTA, None, None);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_changeBrush
#[test]
fn background_change_brush() {
    let brush = mutable_state_of(Brush::Solid(Color::WHITE));
    let current = brush.handle();
    let mut ui = sized(40.0, move || Modifier.background(current.get()));
    assert_shape(&mut ui, Shape::Rectangle, Color::WHITE, None, None);
    brush.set(Brush::Solid(MAGENTA));
    ui.wait_for_idle();
    assert_shape(&mut ui, Shape::Rectangle, MAGENTA, None, None);
}

/// `background(brush, alpha = …)`: compose-rs's alpha is the brush's.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_changeAlpha
#[test]
fn background_change_alpha() {
    let alpha = mutable_state_of(1.0f32);
    let current = alpha.handle();
    let mut ui = sized(40.0, move || {
        Modifier
            .background(MAGENTA)
            .background(Brush::Solid(Color::WHITE).with_alpha(current.get()))
    });
    assert_shape(&mut ui, Shape::Rectangle, Color::WHITE, None, None);
    alpha.set(0.0);
    ui.wait_for_idle();
    assert_shape(&mut ui, Shape::Rectangle, MAGENTA, None, None);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_changeShape
#[test]
fn background_change_shape() {
    let shape_state = mutable_state_of(RoundedCornerShape(10.0));
    let current = shape_state.handle();
    let mut ui = sized(40.0, move || {
        Modifier.background(MAGENTA).background_with(Color::WHITE, current.get())
    });
    assert_shape(&mut ui, RoundedCornerShape(10.0), Color::WHITE, Some(MAGENTA), Some(2.0));
    shape_state.set(CIRCLE_SHAPE);
    ui.wait_for_idle();
    assert_shape(&mut ui, CIRCLE_SHAPE, Color::WHITE, Some(MAGENTA), Some(2.0));
}

/// `observable`'s outline is a rectangle, a 50 px rounded rectangle once `round_corners` is
/// set, and a rectangle again once it's unset.
fn change_outline(observable: Shape, round_corners: OwnedState<bool>) {
    let mut ui = sized(40.0, move || {
        Modifier.background(MAGENTA).background_with(Color::WHITE, observable.clone())
    });
    assert_shape(&mut ui, Shape::Rectangle, Color::WHITE, Some(MAGENTA), Some(2.0));
    round_corners.set(true);
    ui.wait_for_idle();
    assert_shape(&mut ui, RoundedCornerShape(50.0), Color::WHITE, Some(MAGENTA), Some(2.0));
    round_corners.set(false);
    ui.wait_for_idle();
    assert_shape(&mut ui, Shape::Rectangle, Color::WHITE, Some(MAGENTA), Some(2.0));
}

/// A rectangle, or 50 px rounded corners when `round`.
fn rect_or_rounded(
    round: bool,
    size: Size,
    direction: LayoutDirection,
    density: &Density,
) -> Outline {
    if round {
        RoundedCornerShape(50.0).outline_in(size, density, direction)
    } else {
        Shape::Rectangle.outline_in(size, density, direction)
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_changeOutline_differentPaths_observableShape
#[test]
fn background_change_outline_different_paths_observable_shape() {
    let round_corners = mutable_state_of(false);
    let round = round_corners.handle();
    let observable = shape(move |size, direction| {
        rect_or_rounded(round.get(), size, direction, &Density::new(2.0))
    });
    change_outline(observable, round_corners);
}

/// `outline` added to `path`, as `Path.addOutline`.
fn add_outline(path: &Path, outline: &Outline) {
    match outline {
        Outline::Rectangle(rect) => path.add_rect(*rect),
        Outline::Rounded(rect, radii) => {
            let corner = |radius: f32| CornerRadius::new(radius, radius);
            path.add_round_rect(RoundRect::with_radii(
                *rect,
                corner(radii[0]),
                corner(radii[1]),
                corner(radii[2]),
                corner(radii[3]),
            ));
        }
        Outline::Generic(..) => unreachable!("rectangles and rounded rectangles only"),
    }
}

/// One path, reset and filled again with the outline each time.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_changeOutline_samePath_observableShape
#[test]
fn background_change_outline_same_path_observable_shape() {
    let round_corners = mutable_state_of(false);
    let round = round_corners.handle();
    let path = Path::new();
    let observable = shape(move |size, direction| {
        let outline_to_add = rect_or_rounded(round.get(), size, direction, &Density::new(2.0));
        path.reset();
        add_outline(&path, &outline_to_add);
        Outline::generic(&path)
    });
    change_outline(observable, round_corners);
}

/// The test's `rtlAwareShape`: a rectangle left to right, a circle right to left.
fn rtl_aware_shape() -> Shape {
    shape(|size, direction| match direction {
        LayoutDirection::Ltr => Shape::Rectangle.outline_in(size, &Density::new(2.0), direction),
        LayoutDirection::Rtl => CIRCLE_SHAPE.outline_in(size, &Density::new(2.0), direction),
    })
}

/// A 40 px box in `direction()` with a white background in [`rtl_aware_shape`] over magenta.
fn in_direction(direction: impl Fn() -> LayoutDirection + 'static) -> ComposeTestRule {
    let direction = Rc::new(direction);
    let rtl_aware = rtl_aware_shape();
    semantic_parent(move || {
        let rtl_aware = rtl_aware.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction()), move || {
            let side = LOCAL_DENSITY.current().to_dp(40.0);
            Container(
                Modifier
                    .size(side)
                    .background(MAGENTA)
                    .background_with(Brush::Solid(Color::WHITE), rtl_aware.clone()),
                Alignment::TOP_START,
                false,
                callback(|_| {}),
            );
        });
    })
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_rtl_initially
#[test]
fn background_rtl_initially() {
    let mut ui = in_direction(|| LayoutDirection::Rtl);
    assert_shape(&mut ui, CIRCLE_SHAPE, Color::WHITE, Some(MAGENTA), Some(2.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, background_rtl_after_switch
#[test]
fn background_rtl_after_switch() {
    let direction = mutable_state_of(LayoutDirection::Ltr);
    let current = direction.handle();
    let mut ui = in_direction(move || current.get());
    ui.run_on_idle(|_| direction.set(LayoutDirection::Rtl));
    assert_shape(&mut ui, CIRCLE_SHAPE, Color::WHITE, Some(MAGENTA), Some(2.0));
}

/// The test utilities' `assertModifierIsPure`.
fn assert_modifier_is_pure(create: impl Fn(bool) -> Modifier) {
    let (first, second, third) = (create(true), create(false), create(true));
    assert!(first.same(&third), "Modifier with same inputs should resolve true to equals call");
    assert!(
        !first.same(&second),
        "Modifier with different inputs should resolve false to equals call"
    );
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, equalInputs_shouldResolveToEquals_withColor
#[test]
fn equal_inputs_should_resolve_to_equals_with_color() {
    assert_modifier_is_pure(|toggle| {
        if toggle { Modifier.background(Color::RED) } else { Modifier.background(GRAY) }
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, equalInputs_shouldResolveToEquals_withBrush
#[test]
fn equal_inputs_should_resolve_to_equals_with_brush() {
    // Compose's gradients have no colors; these differ by their direction alone too.
    let brush1 = Brush::horizontal_gradient([Color::RED, Color::BLUE]);
    let brush2 = Brush::vertical_gradient([Color::RED, Color::BLUE]);
    assert_modifier_is_pure(|toggle| {
        Modifier.background(if toggle { brush1.clone() } else { brush2.clone() })
    });
}

/// A 10 dp box with a white background in `shape()` (made as it's composed), tagged
/// [`SEMANTICS_TAG`] after it.
fn shaped(shape: impl Fn() -> Shape + 'static) -> ComposeTestRule {
    semantic_parent(move || {
        Container(
            Modifier.size(10.dp()).background_with(Color::WHITE, shape()).test_tag(SEMANTICS_TAG),
            Alignment::TOP_START,
            false,
            callback(|_| {}),
        );
    })
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, rectangleShape_setsShapeSemanticsProperty
#[test]
fn rectangle_shape_sets_shape_semantics_property() {
    let mut ui = shaped(|| Shape::Rectangle);
    ui.on_node_with_tag(SEMANTICS_TAG)
        .assert(expect_value(&SemanticsProperties::SHAPE, Shape::Rectangle));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, roundedCornerShape_setsShapeSemanticsProperty
#[test]
fn rounded_corner_shape_sets_shape_semantics_property() {
    let mut ui = shaped(|| RoundedCornerShape(1.dp()));
    ui.on_node_with_tag(SEMANTICS_TAG)
        .assert(expect_value(&SemanticsProperties::SHAPE, RoundedCornerShape(1.dp())));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, genericShape_setsShapeSemanticsProperty
#[test]
fn generic_shape_sets_shape_semantics_property() {
    let mut ui = shaped(|| CutCornerShape(2.dp()));
    ui.on_node_with_tag(SEMANTICS_TAG)
        .assert(expect_value(&SemanticsProperties::SHAPE, CutCornerShape(2.dp())));
}

/// The node's shape semantics follow the background's shape: `from`, then `to`.
fn shape_change(from: Shape, to: Shape) {
    let state = mutable_state_of(from.clone());
    let current = state.handle();
    let mut ui = shaped(move || current.get());
    ui.on_node_with_tag(SEMANTICS_TAG).assert(expect_value(&SemanticsProperties::SHAPE, from));
    ui.run_on_idle(|_| state.set(to.clone()));
    ui.on_node_with_tag(SEMANTICS_TAG).assert(expect_value(&SemanticsProperties::SHAPE, to));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, shapeChange_fromRectangle_invalidatesSemanticsProperty
#[test]
fn shape_change_from_rectangle_invalidates_semantics_property() {
    shape_change(Shape::Rectangle, CIRCLE_SHAPE);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, shapeChange_toRectangle_invalidatesSemanticsProperty
#[test]
fn shape_change_to_rectangle_invalidates_semantics_property() {
    shape_change(CIRCLE_SHAPE, Shape::Rectangle);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, shapeChange_betweenNonRectangles_invalidatesSemanticsProperty
#[test]
fn shape_change_between_non_rectangles_invalidates_semantics_property() {
    shape_change(CIRCLE_SHAPE, CutCornerShape(2.dp()));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt, backgroundColor_setsBackgroundColorSemanticsProvider
#[test]
fn background_color_sets_background_color_semantics_provider() {
    let mut ui = semantic_parent(|| {
        Container(
            Modifier.size(10.dp()).background(Color::RED).test_tag(SEMANTICS_TAG),
            Alignment::TOP_START,
            false,
            callback(|_| {}),
        );
    });
    let node = ui.on_node_with_tag(SEMANTICS_TAG).fetch_semantics_node();
    let config = node.config();
    let provider = config.get(&SemanticsProperties::BACKGROUND_COLOR);
    let provider = provider.expect("a background color provider");
    assert_eq!((provider.0)(), Color::RED);
}
