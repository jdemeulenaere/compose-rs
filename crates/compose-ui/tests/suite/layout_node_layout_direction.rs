//! Compose's `LayoutNodeLayoutDirectionTest` (ui, device tests): a change of the provided layout
//! direction measures again, draws again (with a layer before or after the drawing) and makes
//! a layer's outline again.

use std::cell::Cell;
use std::rc::Rc;

use crate::support::*;

use compose_runtime::*;
use compose_ui::*;

type Seen = Rc<Cell<Option<LayoutDirection>>>;

/// The direction `seen` has after the first frame, and after the provided one turns right to
/// left, for the content `make` returns under `LOCAL_LAYOUT_DIRECTION`. `make` runs once, before
/// composing, so the content's policy, modifiers and shape are the same in each recomposition
/// (as Compose's test holds them in `val`s outside `setContent`): only the direction changes.
fn before_and_after(make: impl FnOnce(&Seen) -> Rc<dyn Fn()>) -> [Option<LayoutDirection>; 2] {
    let direction = mutable_state_of(LayoutDirection::Ltr);
    let seen: Seen = Rc::default();
    let provided = direction.handle();
    let content = make(&seen);
    let mut host = host(move || {
        let content = content.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(provided.get()), move || {
            content()
        });
    });
    let before = seen.get();
    direction.set(LayoutDirection::Rtl);
    host.frame();
    [before, seen.get()]
}

const BOTH: [Option<LayoutDirection>; 2] = [Some(LayoutDirection::Ltr), Some(LayoutDirection::Rtl)];

/// Records the direction its draw scope has.
fn recording(seen: &Seen) -> Modifier {
    let seen = seen.clone();
    Modifier.draw_behind(move |scope| seen.set(Some(scope.layout_direction())))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeLayoutDirectionTest.kt, compositionLocalLayoutDirectionChangeTriggersRemeasure
#[test]
fn composition_local_layout_direction_change_triggers_remeasure() {
    let directions = before_and_after(|seen| {
        let seen = seen.clone();
        let policy = measure_policy(move |scope, _, _| {
            seen.set(Some(scope.layout_direction()));
            scope.layout(0, 0)
        });
        Rc::new(move || Layout(Modifier, policy.clone(), callback(|| {})))
    });
    assert_eq!(directions, BOTH);
}

/// Compose's `Canvas`: a leaf filling its parent, drawing.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeLayoutDirectionTest.kt, compositionLocalLayoutDirectionChangeTriggersRedraw
#[test]
fn composition_local_layout_direction_change_triggers_redraw() {
    let directions = before_and_after(|seen| {
        let modifier = Modifier.fill_max_size().then(recording(seen));
        Rc::new(move || Leaf(modifier.clone()))
    });
    assert_eq!(directions, BOTH);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeLayoutDirectionTest.kt, compositionLocalLayoutDirectionChangeTriggersRedrawLayerBeforeDraw
#[test]
fn composition_local_layout_direction_change_triggers_redraw_layer_before_draw() {
    let directions = before_and_after(|seen| {
        let modifier = Modifier.fill_max_size().graphics_layer(|_| {}).then(recording(seen));
        Rc::new(move || Leaf(modifier.clone()))
    });
    assert_eq!(directions, BOTH);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeLayoutDirectionTest.kt, compositionLocalLayoutDirectionChangeTriggersRedrawLayerAfterDraw
#[test]
fn composition_local_layout_direction_change_triggers_redraw_layer_after_draw() {
    let directions = before_and_after(|seen| {
        let modifier = Modifier.fill_max_size().then(recording(seen)).graphics_layer(|_| {});
        Rc::new(move || Leaf(modifier.clone()))
    });
    assert_eq!(directions, BOTH);
}

/// A rectangle recording the direction it was made in.
struct RecordingShape(Seen);

impl CustomShape for RecordingShape {
    fn create_outline(&self, size: Size, direction: LayoutDirection, _: &Density) -> Outline {
        self.0.set(Some(direction));
        Outline::Rectangle(Rect::new(0.0, 0.0, size.width, size.height))
    }

    fn same(&self, other: &dyn CustomShape) -> bool {
        (other as &dyn std::any::Any)
            .downcast_ref::<RecordingShape>()
            .is_some_and(|other| Rc::ptr_eq(&self.0, &other.0))
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeLayoutDirectionTest.kt, layoutDirectionChangeRequestsLayerOutlineUpdate
#[test]
fn layout_direction_change_requests_layer_outline_update() {
    let directions = before_and_after(|seen| {
        let shape = Shape::custom(RecordingShape(seen.clone()));
        let modifier = Modifier
            .layout(|scope, measurable, _| {
                let placeable = scope.measure(measurable, Constraints::fixed(100, 100));
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            })
            .graphics_layer(move |layer| {
                layer.shape = shape.clone();
                layer.clip = true;
            });
        Rc::new(move || Stack(modifier.clone(), callback(|| {})))
    });
    assert_eq!(directions, BOTH);
}
