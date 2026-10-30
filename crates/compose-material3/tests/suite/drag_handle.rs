//! Compose's `DragHandleTest.kt`, adapted: its sizes, and what it draws in each state, read from the
//! display list (the color's rectangle and the layer's clip) instead of Compose's pixels. All 11.

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, DpSize, DragInteraction, DragStart, EventContext, IntSize, Modifier, ModifierNode,
    ModifierNodeElement, MutableInteractionSource, Offset, Outline, PointerEvent, PointerEventPass,
    PointerId, PointerInputModifierNode, PointerType, UnitExt, platform::DrawOp,
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
    ui.on_node_with_tag(TAG)
        .assert_touch_width_is_equal_to(48.dp())
        .assert_touch_height_is_equal_to(48.dp());
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
    ui.on_node_with_tag(TAG)
        .assert_touch_width_is_equal_to(60.dp())
        .assert_touch_height_is_equal_to(60.dp());
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
    ui.on_node_with_tag(TAG)
        .assert_touch_width_is_equal_to(48.dp())
        .assert_touch_height_is_equal_to(50.dp());
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

/// A press taken as it goes through still shows: `waitForUpOrCancellation` starts at the next
/// event, as a clickable on the handle shows.
#[test]
fn a_clickable_on_it_doesnt_end_the_press() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(modifier = Modifier.test_tag(TAG).clickable(|| {}));
    });
    press(&mut ui);
    assert_draws(&mut ui, (12.0, 52.0), pressed_color(), Some(6.0));
    let center = center_of(&mut ui);
    let touch = PointerEvent::release(center).with_pointer(PointerId(0), PointerType::Touch);
    ui.host().send_pointer(touch);
    ui.wait_for_idle();
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
}

/// Consumes every event at `pass`.
struct ConsumeAt(PointerEventPass);

struct ConsumeAtNode(PointerEventPass);

impl ModifierNodeElement for ConsumeAt {
    type Node = ConsumeAtNode;
    fn create(&self) -> ConsumeAtNode {
        ConsumeAtNode(self.0)
    }
    fn update(&self, node: &mut ConsumeAtNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn name(&self) -> &'static str {
        "consume_at"
    }
}

impl ModifierNode for ConsumeAtNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for ConsumeAtNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        if pass == self.0 {
            event.consume();
        }
    }
}

/// A node on the handle taking the press on the main pass: it shows, the press being past its
/// `awaitFirstDown`; a later event it takes ends it, by the final pass.
#[test]
fn a_press_taken_on_the_main_pass_shows_until_the_next_taken_event() {
    let mut ui = handle_rule(|| {
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG).then_custom(ConsumeAt(PointerEventPass::Main))
        );
    });
    press(&mut ui);
    assert_draws(&mut ui, (12.0, 52.0), pressed_color(), Some(6.0));
    let center = center_of(&mut ui);
    let moved = PointerEvent::moved(center + Offset::new(1.0, 0.0), true)
        .with_pointer(PointerId(0), PointerType::Touch);
    ui.host().send_pointer(moved);
    ui.wait_for_idle();
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
}

/// A parent taking the press on the initial pass, before the handle hears it: no press, as
/// `awaitFirstDown(requireUnconsumed = true)`.
#[test]
fn a_press_a_parent_takes_on_the_initial_pass_doesnt_show() {
    let mut ui = handle_rule(|| {
        Box!(modifier = Modifier.then_custom(ConsumeAt(PointerEventPass::Initial)), {
            VerticalDragHandle!(modifier = Modifier.test_tag(TAG));
        });
    });
    press(&mut ui);
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DragHandleTest.kt: a drag from the app's draggable shows on the handle
#[test]
fn a_draggable_given_the_handles_source_shows_dragged() {
    use compose_foundation::gestures::{
        DraggableModifier, DraggableOptions, DraggableState, Orientation,
    };
    use compose_ui::MutableInteractionSource;
    let mut ui = handle_rule(move || {
        let drag = remember(|| DraggableState::new(|_| {}));
        let source = remember(MutableInteractionSource::new);
        VerticalDragHandle!(
            modifier = Modifier.test_tag(TAG).draggable_with(
                drag,
                Orientation::Horizontal,
                DraggableOptions {
                    interaction_source: Some(source.clone()),
                    ..DraggableOptions::default()
                }
            ),
            interaction_source = Some(source)
        );
    });
    let center = center_of(&mut ui);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.down(center);
        touch.move_by(Offset::new(20.0, 0.0));
    });
    ui.wait_for_idle();
    assert_draws(&mut ui, (12.0, 52.0), pressed_color(), Some(6.0));
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert_draws(&mut ui, (4.0, 48.0), default_color(), Some(2.0));
}
