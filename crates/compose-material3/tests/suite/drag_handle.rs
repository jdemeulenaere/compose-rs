//! Compose's `DragHandleTest.kt`, adapted: its sizes, and what it draws in each state, read from the
//! display list (the color's rectangle and the layer's clip) instead of Compose's pixels. All 11.

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, DpSize, DragInteraction, DragStart, IntSize, Modifier, MutableInteractionSource, Offset,
    Outline, PointerEvent, PointerId, PointerType, UnitExt, platform::DrawOp,
};

const TAG: &str = "handle";

fn handle_rule(content: impl Fn() + 'static) -> ComposeTestRule {
    let content = std::rc::Rc::new(content);
    ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let content = content.clone();
        MaterialTheme!({ content() });
    })
}

fn bounds(ui: &mut ComposeTestRule) -> compose_ui::Rect {
    ui.on_node_with_tag(TAG).fetch_semantics_node().bounds
}

fn center_of(ui: &mut ComposeTestRule) -> Offset {
    ui.on_node_with_tag(TAG).fetch_semantics_node().center()
}

fn press(ui: &mut ComposeTestRule) {
    let center = center_of(ui);
    let touch = PointerEvent::press(center).with_pointer(PointerId(0), PointerType::Touch);
    ui.host().send_pointer(touch);
    ui.wait_for_idle();
}

/// The rectangle of `color`, and the outline the handle's layer clips it to.
fn assert_draws(ui: &mut ComposeTestRule, size: (f32, f32), color: Color, corner: Option<f32>) {
    let list = ui.host().display_list();
    let drawn: Vec<_> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(rect, paint) if paint.color == color => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(drawn.len(), 1, "{color:?} in {:?}", list.ops);
    assert_eq!((drawn[0].width(), drawn[0].height()), size);
    let layers: Vec<_> = list.groups.iter().filter_map(|group| group.layer.clone()).collect();
    let clips: Vec<_> = layers.iter().filter(|layer| layer.clip).collect();
    assert_eq!(clips.len(), 1, "{layers:?}");
    match (&clips[0].outline, corner) {
        (Some(Outline::Rounded(_, radii)), Some(corner)) => assert_eq!(radii[0], corner),
        (Some(Outline::Rectangle(_)), None) => {}
        (outline, corner) => panic!("{outline:?} is not a corner of {corner:?}"),
    }
}

/// The light scheme's outline and on-surface, Compose's `defaultColor` and `pressedColor`.
fn default_color() -> Color {
    light_color_scheme().outline
}

fn pressed_color() -> Color {
    light_color_scheme().on_surface
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_defaultAccessibilitySize
#[test]
fn a_drag_handle_takes_the_minimum_interactive_size() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(modifier = Modifier.test_tag(TAG));
    });
    let bounds = bounds(&mut ui);
    assert_eq!((bounds.width(), bounds.height()), (48.0, 48.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_customAccessibilitySize
#[test]
fn a_drag_handle_takes_a_custom_minimum_interactive_size() {
    let mut ui = handle_rule(|| {
        CompositionLocalProvider(
            LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(60.dp()),
            || {
                VerticalDragHandle!(modifier = Modifier.test_tag(TAG));
            },
        );
    });
    let bounds = bounds(&mut ui);
    assert_eq!((bounds.width(), bounds.height()), (60.0, 60.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_defaultVisual
#[test]
fn a_drag_handle_is_a_4_by_48_dp_capsule_in_the_outline_color() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(modifier = Modifier.test_tag(TAG));
    });
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_pressedVisual
#[test]
fn a_pressed_drag_handle_grows_to_12_by_52_dp_on_medium_corners() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(modifier = Modifier.test_tag(TAG));
    });
    press(&mut ui);
    assert_draws(&mut ui, (12.0, 52.0), pressed_color(), Some(6.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_customSize
#[test]
fn a_drag_handle_takes_a_custom_size() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            sizes = VerticalDragHandleDefaults::sizes_with(
                DpSize::new(20.dp(), 50.dp()),
                DpSize::UNSPECIFIED,
                DpSize::UNSPECIFIED
            )
        );
    });
    assert_draws(&mut ui, (20.0, 50.0), default_color(), Some(10.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_customPressedSize
#[test]
fn a_drag_handle_takes_a_custom_pressed_size() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            sizes = VerticalDragHandleDefaults::sizes_with(
                DpSize::UNSPECIFIED,
                DpSize::new(20.dp(), 70.dp()),
                DpSize::UNSPECIFIED
            )
        );
    });
    press(&mut ui);
    assert_draws(&mut ui, (20.0, 70.0), pressed_color(), Some(10.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_customSize_ensureMinimumAccessibilitySize
#[test]
fn a_custom_size_keeps_the_minimum_interactive_size() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            sizes = VerticalDragHandleDefaults::sizes_with(
                DpSize::new(20.dp(), 50.dp()),
                DpSize::UNSPECIFIED,
                DpSize::UNSPECIFIED
            )
        );
    });
    let bounds = bounds(&mut ui);
    assert_eq!((bounds.width(), bounds.height()), (48.0, 50.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_customShape
#[test]
fn a_drag_handle_takes_a_custom_shape() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            shapes = VerticalDragHandleDefaults::shapes_with(
                Some(compose_ui::RECTANGLE_SHAPE),
                None,
                None
            )
        );
    });
    assert_draws(&mut ui, (4.0, 48.0), default_color(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_customPressedShape
#[test]
fn a_drag_handle_takes_a_custom_pressed_shape() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            shapes = VerticalDragHandleDefaults::shapes_with(
                None,
                Some(compose_ui::RECTANGLE_SHAPE),
                None
            )
        );
    });
    press(&mut ui);
    assert_draws(&mut ui, (12.0, 52.0), pressed_color(), None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_customColor
#[test]
fn a_drag_handle_takes_a_custom_color() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            colors = VerticalDragHandleDefaults::colors_with(
                Color::RED,
                Color::UNSPECIFIED,
                Color::UNSPECIFIED
            )
        );
    });
    assert_draws(&mut ui, (4.0, 48.0), Color::RED, Some(2.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt, verticalDragHandle_customPressedColor
#[test]
fn a_drag_handle_takes_a_custom_pressed_color() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            colors = VerticalDragHandleDefaults::colors_with(
                Color::UNSPECIFIED,
                Color::RED,
                Color::UNSPECIFIED
            )
        );
    });
    press(&mut ui);
    assert_draws(&mut ui, (12.0, 52.0), Color::RED, Some(6.0));
}

/// Wide enough to show them, the pressed and dragged corners are the theme's medium ones, 12 dp,
/// where a 40 dp wide capsule's would be 20.
#[test]
fn a_pressed_drag_handles_corners_are_medium() {
    let wide = DpSize::new(40.dp(), 60.dp());
    let mut ui = handle_rule(move || {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            sizes = VerticalDragHandleDefaults::sizes_with(wide, wide, wide)
        );
    });
    assert_draws(&mut ui, (40.0, 60.0), default_color(), Some(20.0));
    press(&mut ui);
    assert_draws(&mut ui, (40.0, 60.0), pressed_color(), Some(12.0));
}

/// A drag through the source wins over the rest, as `isDragged`: the dragged size, shape and
/// color, its own values here.
#[test]
fn a_dragged_drag_handle_shows_its_dragged_values() {
    let source = MutableInteractionSource::new();
    let handle_source = source.clone();
    let mut ui = handle_rule(move || {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG),
            sizes = VerticalDragHandleDefaults::sizes_with(
                DpSize::UNSPECIFIED,
                DpSize::UNSPECIFIED,
                DpSize::new(16.dp(), 60.dp())
            ),
            shapes = VerticalDragHandleDefaults::shapes_with(
                None,
                None,
                Some(compose_ui::RECTANGLE_SHAPE)
            ),
            colors = VerticalDragHandleDefaults::colors_with(
                Color::UNSPECIFIED,
                Color::UNSPECIFIED,
                Color::GREEN
            ),
            interaction_source = Some(handle_source.clone())
        );
    });
    let start = DragStart::new();
    source.try_emit(DragInteraction::Start(start));
    ui.wait_for_idle();
    assert_draws(&mut ui, (16.0, 60.0), Color::GREEN, None);
    source.try_emit(DragInteraction::Stop(start));
    ui.wait_for_idle();
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
}

/// As `pressable`: a press shows until it's let go, cancelled by a pointer leaving the handle or
/// something else taking the gesture.
#[test]
fn a_press_shows_until_its_let_go_or_it_leaves_the_handle() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(modifier = Modifier.test_tag(TAG));
    });
    let center = center_of(&mut ui);
    let touch = |event: PointerEvent| event.with_pointer(PointerId(0), PointerType::Touch);

    press(&mut ui);
    ui.host().send_pointer(touch(PointerEvent::release(center)));
    ui.wait_for_idle();
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));

    press(&mut ui);
    assert_draws(&mut ui, (12.0, 52.0), pressed_color(), Some(6.0));
    let away = center + Offset::new(200.0, 0.0);
    ui.host().send_pointer(touch(PointerEvent::moved(away, true)));
    ui.wait_for_idle();
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
    ui.host().send_pointer(touch(PointerEvent::release(away)));
    ui.wait_for_idle();
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
}

/// A draggable on it taking the gesture ends the press, as `waitForUpOrCancellation`'s check of a
/// consumed change: nothing here says it's dragged, so it's back at rest with the finger down.
#[test]
fn a_drag_taking_the_gesture_ends_the_press() {
    use compose_foundation::gestures::{DraggableModifier, DraggableState, Orientation};
    let mut ui = handle_rule(move || {
        let drag = remember(|| DraggableState::new(|_| {}));
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG).draggable(
                drag,
                Orientation::Horizontal,
                true,
                false,
                |_| {},
                |_| {}
            )
        );
    });
    let center = center_of(&mut ui);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.down(center));
    ui.wait_for_idle();
    assert_draws(&mut ui, (12.0, 52.0), pressed_color(), Some(6.0));
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        // Past the 18 dp slop, still on its 48 dp: only the drag ends the press.
        touch.move_by(Offset::new(20.0, 0.0));
    });
    ui.wait_for_idle();
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
}
