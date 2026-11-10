//! Compose's `DrawModifierTest` (ui, device tests, the one in `androidx.compose.ui`): draw
//! modifiers on layouts, drawn by Skia and checked pixel by pixel, as the test's
//! `validateSquareColors` checks a device's: nested draws, draws in order with the children's,
//! clips around `draw_content`, an unplaced child not drawn, the size a draw gets inside padding,
//! the layout direction it gets, and state read in a draw.
//!
//! `simpleDrawTestLegacyFallback` is n/a: it forces Android's pre-`RenderNode` fallback.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    Color, CompositingStrategy, Constraints, ContentDrawScope, DrawModifierNode, IntSize,
    LOCAL_LAYOUT_DIRECTION, Layout, LayoutDirection, LayoutModifierNode, Measurable, MeasureResult,
    MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Offset, Outline, Paint, Rect, Size,
    measure_policy,
};

const YELLOW: Color = Color::from_argb(0xFFFF_FF00);
const DARK_RED: Color = Color::from_argb(0xFF80_0000);
const DARK_BLUE: Color = Color::from_argb(0xFF00_0080);
const GREEN: Color = Color::from_argb(0xFF00_FF00);

/// Compose's test `SquareModel`: a size and two colors in state.
#[derive(Clone)]
struct SquareModel {
    size: i32,
    outer: Rc<OwnedState<Color>>,
    inner: Rc<OwnedState<Color>>,
}

impl SquareModel {
    fn new(size: i32, outer: Color, inner: Color) -> SquareModel {
        let state = |color| Rc::new(mutable_state_of(color));
        SquareModel { size, outer: state(outer), inner: state(inner) }
    }

    /// Fills its layout with the inner or outer color, read as it draws, as the test's
    /// `Modifier.background(model, isInner)`.
    fn fill(&self, inner: bool) -> Modifier {
        let color = if inner { self.inner.handle() } else { self.outer.handle() };
        Modifier.draw_behind(move |scope| {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(color.get()));
        })
    }
}

/// Fills its layout with `color`, as the test's `Modifier.background(color)`.
fn background(color: Color) -> Modifier {
    Modifier.draw_behind(move |scope| {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color));
    })
}

/// Its content padded by `size` pixels on each side, as the test utilities' `Padding`.
fn Padding(size: i32, modifier: Modifier, content: impl Fn() + 'static) {
    Layout(
        modifier,
        measure_policy(move |scope, measurables, constraints| {
            let diff = size * 2;
            let shrink = |max: i32, bounded: bool| {
                if bounded { (max - diff).max(0) } else { Constraints::INFINITY }
            };
            let inner = Constraints::new(
                (constraints.min_width - diff).max(0),
                shrink(constraints.max_width, constraints.has_bounded_width()),
                (constraints.min_height - diff).max(0),
                shrink(constraints.max_height, constraints.has_bounded_height()),
            );
            let (mut width, mut height) = (size, size);
            for &measurable in measurables {
                let placeable = scope.measure(measurable, inner);
                scope.place_relative(&placeable, size, size);
                width = width.max(placeable.width + diff);
                height = height.max(placeable.height + diff);
            }
            scope.layout(width, height)
        }),
        callback(content),
    );
}

/// At least `size` pixels each way, its children at its origin, as the test utilities'
/// `AtLeastSize`.
fn AtLeastSize(size: i32, modifier: Modifier, content: impl Fn() + 'static) {
    Layout(
        modifier,
        measure_policy(move |scope, measurables, constraints| {
            let grow = |max: i32, bounded: bool| {
                if bounded { size.max(max) } else { Constraints::INFINITY }
            };
            let inner = Constraints::new(
                size.max(constraints.min_width),
                grow(constraints.max_width, constraints.has_bounded_width()),
                size.max(constraints.min_height),
                grow(constraints.max_height, constraints.has_bounded_height()),
            );
            let (mut width, mut height) = (size, size);
            for &measurable in measurables {
                let placeable = scope.measure(measurable, inner);
                scope.place(&placeable, 0, 0);
                width = width.max(placeable.width);
                height = height.max(placeable.height);
            }
            scope.layout(width, height)
        }),
        callback(content),
    );
}

/// Exactly `size` pixels each way, its children too, as the test utilities' `FixedSize`.
fn FixedSize(size: i32, modifier: Modifier, content: impl Fn() + 'static) {
    Layout(
        modifier,
        measure_policy(move |scope, measurables, _| {
            for &measurable in measurables {
                let placeable = scope.measure(measurable, Constraints::fixed(size, size));
                scope.place_relative(&placeable, 0, 0);
            }
            scope.layout(size, size)
        }),
        callback(content),
    );
}

/// The content inset by `padding` pixels, as the test utilities' `Modifier.padding(Int)`.
fn padding(padding: i32) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints.offset(-padding * 2, -padding * 2));
        scope.place_relative(&placeable, padding, padding);
        scope.layout(
            constraints.constrain_width(placeable.width + padding * 2),
            constraints.constrain_height(placeable.height + padding * 2),
        )
    })
}

/// The content at the top left of the largest size the constraints allow, as the test
/// utilities' `AlignTopLeft`.
fn align_top_left() -> Modifier {
    Modifier.layout(|scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints.copy_max_dimensions());
        scope.place_relative(&placeable, 0, 0);
        scope.layout(constraints.max_width, constraints.max_height)
    })
}

/// A rule over a window of `total` pixels each way, drawing with Skia.
fn rule(total: i32, content: impl Fn() + 'static) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    ComposeTestRule::sized(IntSize::new(total, total), content)
}

/// The window is `inner` in its middle square of `size` pixels and `outer` around it, as the
/// test's `validateSquareColors(outerColor, innerColor, size)` (three times `size` in all).
#[track_caller]
fn validate_square_colors(ui: &mut ComposeTestRule, outer: Color, inner: Color, size: i32) {
    ui.wait_for_idle();
    let total = size * 3;
    let image = ui.on_root().capture_to_image();
    let (start, end) = ((total - size) / 2, total - (total - size) / 2);
    image.assert_pixels(Some(IntSize::new(total, total)), |at| {
        let inside = (start..end).contains(&at.x) && (start..end).contains(&at.y);
        Some(if inside { inner } else { outer })
    });
}

/// The test's `composeSquares`: the outer color padding the inner one's square. The modifiers
/// are made once, outside the content, so a color changing draws again only by the state its
/// draw read (a modifier made again in a composition would draw anew whatever it read).
fn compose_squares(model: &SquareModel) -> ComposeTestRule {
    let (size, outer, inner) = (model.size, model.fill(false), model.fill(true));
    rule(size * 3, move || {
        let inner = inner.clone();
        Padding(size, outer.clone(), move || AtLeastSize(size, inner.clone(), || {}));
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, simpleDrawTest
#[test]
fn simple_draw_test() {
    let model = SquareModel::new(10, YELLOW, DARK_RED);
    let mut ui = compose_squares(&model);
    validate_square_colors(&mut ui, YELLOW, DARK_RED, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, nestedDrawTest
#[test]
fn nested_draw_test() {
    let model = SquareModel::new(10, YELLOW, DARK_RED);
    let m = model.clone();
    let mut ui = rule(30, move || {
        let (outer, size) = (m.outer.handle(), m.size as f32);
        // The outer color, then the content clipped to the middle square.
        let clipped = Modifier.draw_with_content(move |scope: &mut ContentDrawScope<'_, '_>| {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(outer.get()));
            scope.save();
            scope.clip_outline(Outline::Rectangle(Rect::new(size, size, size * 2.0, size * 2.0)));
            scope.draw_content();
            scope.restore();
        });
        let fill = m.fill(true);
        AtLeastSize(30, clipped, move || AtLeastSize(30, fill.clone(), || {}));
    });
    validate_square_colors(&mut ui, YELLOW, DARK_RED, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, recomposeDrawTest
#[test]
fn recompose_draw_test() {
    let model = SquareModel::new(10, DARK_BLUE, Color::WHITE);
    let mut ui = compose_squares(&model);
    validate_square_colors(&mut ui, DARK_BLUE, Color::WHITE, 10);
    model.outer.set(DARK_RED);
    model.inner.set(YELLOW);
    validate_square_colors(&mut ui, DARK_RED, YELLOW, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, noPlaceNoDraw
#[test]
fn no_place_no_draw() {
    let model = SquareModel::new(20, GREEN, Color::WHITE);
    let m = model.clone();
    let mut ui = rule(60, move || {
        let m = m.clone();
        Layout(
            Modifier,
            measure_policy(|scope, measurables, constraints| {
                let placeables: Vec<_> =
                    measurables.iter().map(|&m| scope.measure(m, constraints)).collect();
                // Only the first is placed.
                scope.place(&placeables[0], 0, 0);
                scope.layout(placeables[0].width, placeables[0].height)
            }),
            callback(move || {
                Padding(m.size * 3, m.fill(false), || {});
                Padding(m.size, m.fill(true), || {});
            }),
        );
    });
    validate_square_colors(&mut ui, GREEN, GREEN, 20);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawOrderWithChildren
#[test]
fn draw_order_with_children() {
    let model = SquareModel::new(20, GREEN, Color::WHITE);
    let m = model.clone();
    let mut ui = rule(60, move || {
        let (outer, inner) = (m.outer.handle(), m.inner.handle());
        let content_drawing = Modifier.draw_with_content(move |scope| {
            // The outer color everywhere, then, clipped to the middle square, the content and
            // the inner color's bottom half.
            let (size, bounds) = (scope.size(), scope.bounds());
            scope.draw_rect(bounds, Paint::fill(outer.get()));
            let offset = size.width / 3.0;
            scope.save();
            scope.clip_outline(Outline::Rectangle(Rect::new(
                offset,
                offset,
                offset * 2.0,
                offset * 2.0,
            )));
            scope.draw_content();
            let bottom = Rect::new(0.0, size.height / 2.0, size.width, size.height);
            scope.draw_rect(bottom, Paint::fill(inner.get()));
            scope.restore();
        });
        // The inner color's top half, clipped by the modifier before.
        let padding_content = Modifier.draw_behind(move |scope| {
            let size = scope.size();
            let top = Rect::new(0.0, 0.0, size.width, size.height / 2.0);
            scope.draw_rect(top, Paint::fill(inner.get()));
        });
        Padding(m.size * 3, content_drawing.then(padding_content), || {});
    });
    validate_square_colors(&mut ui, GREEN, Color::WHITE, 20);
}

/// The display list's only layer's properties: the 10 × 10 box's, with `strategy` at `alpha`.
fn layer_with(strategy: CompositingStrategy, alpha: f32) -> compose::ui::platform::LayerProperties {
    let mut ui = rule(30, move || {
        FixedSize(
            10,
            Modifier
                .graphics_layer(move |layer| {
                    layer.compositing_strategy = strategy;
                    layer.alpha = alpha;
                })
                .then(background(Color::RED)),
            || {},
        );
    });
    let list = ui.host().display_list();
    let layers: Vec<_> = list.groups.iter().filter_map(|group| group.layer.clone()).collect();
    let [layer] = &layers[..] else { panic!("one layer\n{}", list.dump()) };
    layer.clone()
}

/// `Auto`: no offscreen buffer at alpha 1 (Android's `isUsingCompositingLayer` false), and
/// overlapping drawings composited together below it (`hasOverlappingRendering`): offscreen,
/// its alpha not on each drawing.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, testCompositingStrategyAuto
#[test]
fn test_compositing_strategy_auto() {
    assert!(!layer_with(CompositingStrategy::Auto, 1.0).offscreen());
    let faded = layer_with(CompositingStrategy::Auto, 0.5);
    assert!(faded.offscreen());
    assert_eq!(faded.modulated_alpha(), None);
}

/// `ModulateAlpha`: never offscreen, its alpha on each drawing (no overlapping rendering).
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, testCompositingStrategyModulateAlpha
#[test]
fn test_compositing_strategy_modulate_alpha() {
    assert!(!layer_with(CompositingStrategy::ModulateAlpha, 1.0).offscreen());
    let faded = layer_with(CompositingStrategy::ModulateAlpha, 0.5);
    assert!(!faded.offscreen());
    assert_eq!(faded.modulated_alpha(), Some(0.5));
}

/// `Offscreen`: always in an offscreen buffer (Android's hardware layer), alpha 1 included.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, testCompositingStrategyAlways
#[test]
fn test_compositing_strategy_always() {
    assert!(layer_with(CompositingStrategy::Offscreen, 1.0).offscreen());
    let faded = layer_with(CompositingStrategy::Offscreen, 0.5);
    assert!(faded.offscreen());
    assert_eq!(faded.modulated_alpha(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, testDrawWithLayoutNotPlaced
#[test]
fn test_draw_with_layout_not_placed() {
    let drawn = Rc::new(Cell::new(false));
    let d = drawn.clone();
    let mut ui = rule(30, move || {
        let d = d.clone();
        Layout(
            Modifier,
            // Neither measured nor placed.
            measure_policy(|scope, _, _| scope.layout(20, 20)),
            callback(move || {
                let d = d.clone();
                AtLeastSize(30, Modifier.draw_behind(move |_| d.set(true)), || {});
            }),
        );
    });
    ui.wait_for_idle();
    assert!(!drawn.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, parentSizeForDrawIsProvidedWithoutPadding
#[test]
fn parent_size_for_draw_is_provided_without_padding() {
    let size = Rc::new(Cell::new(Size::ZERO));
    let s = size.clone();
    let mut ui = rule(200, move || {
        let s = s.clone();
        let drawn_content = Modifier.draw_behind(move |scope| s.set(scope.size()));
        AtLeastSize(100, padding(10).then(drawn_content), || {});
    });
    ui.wait_for_idle();
    assert_eq!(size.get(), Size::new(100.0, 100.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, parentSizeForDrawInsideRepaintBoundaryIsProvidedWithoutPadding
#[test]
fn parent_size_for_draw_inside_repaint_boundary_is_provided_without_padding() {
    let size = Rc::new(Cell::new(Size::ZERO));
    let s = size.clone();
    let mut ui = rule(200, move || {
        let s = s.clone();
        let modifier =
            padding(10).graphics_layer(|_| {}).draw_behind(move |scope| s.set(scope.size()));
        AtLeastSize(100, modifier, || {});
    });
    ui.wait_for_idle();
    assert_eq!(size.get(), Size::new(100.0, 100.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawModifier_drawPositioning
#[test]
fn draw_modifier_draw_positioning() {
    let mut ui = rule(30, || {
        FixedSize(30, background(Color::BLUE), || {
            FixedSize(10, padding(10).then(background(Color::WHITE)), || {});
        });
    });
    validate_square_colors(&mut ui, Color::BLUE, Color::WHITE, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawModifier_testLayoutDirection
#[test]
fn draw_modifier_test_layout_direction() {
    let direction = Rc::new(Cell::new(None));
    let d = direction.clone();
    let mut ui = rule(100, move || {
        let d = d.clone();
        CompositionLocalProvider(
            LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
            move || {
                let d = d.clone();
                FixedSize(
                    50,
                    Modifier.draw_behind(move |scope| d.set(Some(scope.layout_direction()))),
                    || {},
                );
            },
        );
    });
    ui.wait_for_idle();
    assert_eq!(direction.get(), Some(LayoutDirection::Rtl));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawModifier_modelChangesOnRoot
#[test]
fn draw_modifier_model_changes_on_root() {
    let model = SquareModel::new(10, GREEN, Color::WHITE);
    // Made once, so only the state the draw read draws the change.
    let (outer, inner) = (model.fill(false), padding(10).then(model.fill(true)));
    let mut ui = rule(30, move || {
        let inner = inner.clone();
        FixedSize(30, outer.clone(), move || FixedSize(10, inner.clone(), || {}));
    });
    validate_square_colors(&mut ui, GREEN, Color::WHITE, 10);
    model.inner.set(YELLOW);
    validate_square_colors(&mut ui, GREEN, YELLOW, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawModifier_modelChangesOnRepaintBoundary
#[test]
fn draw_modifier_model_changes_on_repaint_boundary() {
    let model = SquareModel::new(10, GREEN, Color::WHITE);
    // Made once, so only the state the draw read draws the change.
    let (outer, inner) = (
        background(GREEN),
        Modifier.graphics_layer(|_| {}).then(padding(10)).then(model.fill(true)),
    );
    let mut ui = rule(30, move || {
        let inner = inner.clone();
        FixedSize(30, outer.clone(), move || FixedSize(10, inner.clone(), || {}));
    });
    validate_square_colors(&mut ui, GREEN, Color::WHITE, 10);
    model.inner.set(YELLOW);
    validate_square_colors(&mut ui, GREEN, YELLOW, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawModifier_oneModifier
#[test]
fn draw_modifier_one_modifier() {
    let mut ui = rule(30, || {
        let color_modifier = Modifier.draw_behind(|scope| {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(Color::BLUE));
            scope.draw_rect(Rect::new(10.0, 10.0, 20.0, 20.0), Paint::fill(Color::WHITE));
        });
        FixedSize(30, color_modifier, || {});
    });
    validate_square_colors(&mut ui, Color::BLUE, Color::WHITE, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawModifier_nestedModifiers
#[test]
fn draw_modifier_nested_modifiers() {
    let mut ui = rule(30, || {
        FixedSize(30, background(Color::BLUE), || {
            Padding(10, Modifier, || FixedSize(10, background(Color::WHITE), || {}));
        });
    });
    validate_square_colors(&mut ui, Color::BLUE, Color::WHITE, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawModifier_withLayoutModifier
#[test]
fn draw_modifier_with_layout_modifier() {
    let mut ui = rule(30, || {
        FixedSize(30, background(Color::BLUE), || {
            FixedSize(10, padding(10).then(background(Color::WHITE)), || {});
        });
    });
    validate_square_colors(&mut ui, Color::BLUE, Color::WHITE, 10);
}

/// The content drawn moved by (10, 10), around a transform pushed before `draw_content`.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, drawModifier_withLayout
#[test]
fn draw_modifier_with_layout() {
    let mut ui = rule(30, || {
        let draw_and_offset = Modifier.draw_with_content(|scope| {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(Color::BLUE));
            scope.save();
            scope.transform(Offset::ZERO, (1.0, 1.0), 0.0, Offset::new(10.0, 10.0));
            scope.draw_content();
            scope.restore();
        });
        FixedSize(30, draw_and_offset, || {
            FixedSize(10, align_top_left().then(background(Color::WHITE)), || {});
        });
    });
    validate_square_colors(&mut ui, Color::BLUE, Color::WHITE, 10);
}

/// A state read in a child's draw draws the child again, not what's outside its parent's
/// layer. The content is in a repaint boundary, as Compose's root is a layer: compose-rs's root
/// isn't one, so what's outside every boundary is recorded on each draw (D1164).
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, doubleDraw
#[test]
fn double_draw() {
    let offset = mutable_state_of(0);
    let (inner, outer) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (o, i, at) = (outer.clone(), inner.clone(), offset.handle());
    let mut ui = rule(30, move || {
        let (o, i) = (o.clone(), i.clone());
        let outer_modifier =
            Modifier.draw_behind(move |_| o.set(o.get() + 1)).graphics_layer(|_| {});
        let content = move || {
            let i = i.clone();
            let line = Modifier.draw_behind(move |scope| {
                let at = at.get() as f32;
                // A hairline, as `Stroke.HairlineWidth`.
                let paint = Paint::stroke(Color::BLUE, 0.0);
                scope.draw_line(Offset::new(at, 0.0), Offset::new(0.0, at), paint);
                i.set(i.get() + 1);
            });
            FixedSize(10, line, || {});
        };
        let outer_modifier = outer_modifier.clone();
        let root = move || FixedSize(30, outer_modifier.clone(), content.clone());
        FixedSize(30, Modifier.repaint_boundary(), root);
    });
    ui.wait_for_idle();
    offset.set(10);
    inner.set(0);
    outer.set(0);
    ui.wait_for_idle();
    assert_eq!(inner.get(), 1);
    assert_eq!(outer.get(), 0);
}

/// A node both laying out (its content 10 × 10 in its middle) and drawing (its color over its
/// whole size), as the test's `LayoutAndDrawModifier`.
struct LayoutAndDraw(Color);

impl ModifierNodeElement for LayoutAndDraw {
    type Node = LayoutAndDrawNode;
    fn create(&self) -> LayoutAndDrawNode {
        LayoutAndDrawNode(self.0)
    }
    fn update(&self, node: &mut LayoutAndDrawNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

struct LayoutAndDrawNode(Color);

impl ModifierNode for LayoutAndDrawNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for LayoutAndDrawNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, Constraints::fixed(10, 10));
        let (x, y) = (
            (constraints.max_width - placeable.width) / 2,
            (constraints.max_height - placeable.height) / 2,
        );
        scope.place_relative(&placeable, x, y);
        scope.layout(constraints.max_width, constraints.max_height)
    }
}

impl DrawModifierNode for LayoutAndDrawNode {
    fn draw(&mut self, scope: &mut compose::ui::DrawScope<'_>) {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(self.0));
    }
}

/// The node's draw is in its content's coordinates: the 10 × 10 middle, as Compose's
/// `LayoutModifier` and `DrawModifier` in one draws in the coordinates its layout gives its
/// content (compose-rs draws it over its own 30 × 30, white at (0, 0)).
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/DrawModifierTest.kt, modifier_combinedModifiers
#[test]
#[ignore = "a layout and draw node draws in its own coordinates, not its content's (D1341)"]
fn modifier_combined_modifiers() {
    let mut ui = rule(30, || {
        FixedSize(30, background(Color::BLUE), || {
            // Compose's `JustConstraints`: its minimum size, its content not measured.
            Layout(
                Modifier.then_custom(LayoutAndDraw(Color::WHITE)),
                measure_policy(|scope, _, constraints| {
                    scope.layout(constraints.min_width, constraints.min_height)
                }),
                callback(|| {}),
            );
        });
    });
    validate_square_colors(&mut ui, Color::BLUE, Color::WHITE, 10);
}
