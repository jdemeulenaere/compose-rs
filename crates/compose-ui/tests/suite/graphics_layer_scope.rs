//! Compose's `GraphicsLayerScopeTest` (ui, host tests): a layer scope's defaults, the defaults
//! again for each run of a layer's block, and its size.
//!
//! compose-rs's scope has no rotation about X or Y, camera distance, blend mode or color filter
//! yet (feature:layer-3d-and-effects in docs/parity/ui-core.md): the defaults of the fields it
//! has are checked. A scope made by `default()` has a zero size where Compose's is unspecified;
//! a block always sees its layer's size (D1925).

use std::cell::RefCell;
use std::rc::Rc;

use crate::support::*;

use compose_runtime::*;
use compose_ui::*;

fn assert_default_values(scope: &GraphicsLayerScope) {
    assert_eq!(scope.scale_x, 1.0);
    assert_eq!(scope.scale_y, 1.0);
    assert_eq!(scope.alpha, 1.0);
    assert_eq!(scope.translation_x, 0.0);
    assert_eq!(scope.translation_y, 0.0);
    assert_eq!(scope.shadow_elevation, 0.0);
    assert_eq!(scope.rotation_z, 0.0);
    assert_eq!(scope.transform_origin, TransformOrigin::CENTER);
    assert_eq!(scope.shape, RECTANGLE_SHAPE);
    assert!(!scope.clip);
    assert_eq!(scope.compositing_strategy, CompositingStrategy::Auto);
    assert_eq!(scope.ambient_shadow_color(), DEFAULT_SHADOW_COLOR);
    assert_eq!(scope.spot_shadow_color(), DEFAULT_SHADOW_COLOR);
    assert_eq!(scope.render_effect(), None);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/graphics/GraphicsLayerScopeTest.kt, initialValuesAreCorrect
#[test]
fn initial_values_are_correct() {
    assert_default_values(&GraphicsLayerScope::default());
}

/// A block setting every property in one run, then reading them in the next: each run starts
/// from the defaults, as Compose resets its reused scope before running the block again.
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/graphics/GraphicsLayerScopeTest.kt, resetValuesAreCorrect
#[test]
fn reset_values_are_correct() {
    let set = mutable_state_of(true);
    let handle = set.handle();
    let seen: Rc<RefCell<Vec<GraphicsLayerScope>>> = Rc::default();
    let applied: Rc<RefCell<Vec<GraphicsLayerScope>>> = Rc::default();
    let (record, apply) = (seen.clone(), applied.clone());
    let mut host = host(move || {
        let (record, apply) = (record.clone(), apply.clone());
        Leaf(Modifier.size(10.dp()).graphics_layer(move |scope| {
            if handle.get() {
                scope.scale_x = 2.0;
                scope.scale_y = 2.0;
                scope.alpha = 0.5;
                scope.translation_x = 5.0;
                scope.translation_y = 5.0;
                scope.shadow_elevation = 5.0;
                scope.rotation_z = 5.0;
                scope.transform_origin =
                    TransformOrigin { pivot_fraction_x: 0.7, pivot_fraction_y: 0.1 };
                scope.shape = Shape::Rounded([CornerSize::Px(5.0); 4]);
                scope.clip = true;
                scope.compositing_strategy = CompositingStrategy::Offscreen;
                scope.set_ambient_shadow_color(Color::RED);
                scope.set_spot_shadow_color(Color::RED);
                scope.set_render_effect(Some(RenderEffect::blur(3.0, 3.0, TileMode::Clamp)));
                apply.borrow_mut().push(scope.clone());
            } else {
                record.borrow_mut().push(scope.clone());
            }
        }));
    });
    assert!(seen.borrow().is_empty());
    // The first run set every property (else the reset below would check nothing).
    {
        let applied = applied.borrow();
        assert_eq!(applied.len(), 1, "the block ran once");
        let scope = &applied[0];
        assert_eq!((scope.scale_x, scope.alpha, scope.rotation_z), (2.0, 0.5, 5.0));
        assert!(scope.clip);
        assert_eq!(scope.compositing_strategy, CompositingStrategy::Offscreen);
        assert_eq!(scope.spot_shadow_color(), Color::RED);
        assert!(scope.render_effect().is_some());
    }
    set.set(false);
    host.frame();
    let seen = seen.borrow();
    assert_eq!(seen.len(), 1, "the block ran again");
    assert_eq!(applied.borrow().len(), 1);
    assert_default_values(&seen[0]);
    assert_eq!(seen[0].size, Size::new(10.0, 10.0), "the size is the layer's, not reset");
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/graphics/GraphicsLayerScopeTest.kt, testGraphicsLayerSize
#[test]
fn test_graphics_layer_size() {
    let mut scope = GraphicsLayerScope::default();
    assert_eq!(scope.size, Size::ZERO, "zero where Compose's is unspecified (D1925)");
    scope.size = Size::new(2560.0, 1400.0);
    assert_eq!((scope.size.width, scope.size.height), (2560.0, 1400.0));
    // A block sees its layer's size.
    let seen = Rc::new(RefCell::new(None));
    let record = seen.clone();
    let _host = host(move || {
        let record = record.clone();
        Leaf(
            Modifier
                .size((25.dp(), 14.dp()))
                .graphics_layer(move |scope| *record.borrow_mut() = Some(scope.size)),
        );
    });
    assert_eq!(*seen.borrow(), Some(Size::new(25.0, 14.0)));
}
