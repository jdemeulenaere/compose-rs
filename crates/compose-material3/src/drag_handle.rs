//! `VerticalDragHandle`, as Compose's: a capsule a user drags to resize panes, growing when pressed
//! or dragged (D1032). It only shows the gesture: what moves anything is the app's own
//! `Modifier.draggable` on it. That takes no interaction source yet, so a drag shows if the app
//! emits it on the handle's `interaction_source`.

use compose_foundation::layout::Box;
use compose_runtime::{MutableState, composable, mutable_state_of, remember};
use compose_ui::{
    Alignment, Color, Constraints, DpSize, EventContext, IntSize, Modifier, ModifierNode,
    ModifierNodeElement, MutableInteractionSource, Paint, PointerEvent, PointerEventPass,
    PointerEventType, PointerId, PointerInputModifierNode, Shape,
};

use crate::components::{minimum_interactive_component_size, remembered_interaction_source};
use crate::theme::MaterialTheme;

/// A drag handle's colors in each state, as Compose's `DragHandleColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DragHandleColors {
    pub color: Color,
    /// Pressed but not dragged.
    pub pressed_color: Color,
    pub dragged_color: Color,
}

/// A drag handle's shapes in each state, as Compose's `DragHandleShapes`.
#[derive(Clone, PartialEq, Debug)]
pub struct DragHandleShapes {
    pub shape: Shape,
    /// Pressed but not dragged.
    pub pressed_shape: Shape,
    pub dragged_shape: Shape,
}

/// A drag handle's sizes in each state, as Compose's `DragHandleSizes`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DragHandleSizes {
    pub size: DpSize,
    /// Pressed but not dragged.
    pub pressed_size: DpSize,
    pub dragged_size: DpSize,
}

impl compose_runtime::ComposeArg for DragHandleColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl compose_runtime::ComposeArg for DragHandleShapes {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl compose_runtime::ComposeArg for DragHandleSizes {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `VerticalDragHandleDefaults`, from `DragHandleTokens`.
pub struct VerticalDragHandleDefaults;

impl VerticalDragHandleDefaults {
    /// The outline color, on-surface pressed or dragged, as `colors()`.
    pub fn colors() -> DragHandleColors {
        let scheme = MaterialTheme::color_scheme();
        DragHandleColors {
            color: scheme.outline,
            pressed_color: scheme.on_surface,
            dragged_color: scheme.on_surface,
        }
    }

    /// As `colors(color, pressedColor, draggedColor)`: unspecified is the default.
    pub fn colors_with(
        color: Color,
        pressed_color: Color,
        dragged_color: Color,
    ) -> DragHandleColors {
        let defaults = VerticalDragHandleDefaults::colors();
        DragHandleColors {
            color: color.or(defaults.color),
            pressed_color: pressed_color.or(defaults.pressed_color),
            dragged_color: dragged_color.or(defaults.dragged_color),
        }
    }

    /// A capsule, medium corners pressed or dragged, as `shapes()`.
    pub fn shapes() -> DragHandleShapes {
        let medium = MaterialTheme::shapes().medium.clone();
        DragHandleShapes {
            shape: compose_foundation::CIRCLE_SHAPE,
            pressed_shape: medium.clone(),
            dragged_shape: medium,
        }
    }

    /// As `shapes(shape, pressedShape, draggedShape)`: `None` is the default.
    pub fn shapes_with(
        shape: Option<Shape>,
        pressed_shape: Option<Shape>,
        dragged_shape: Option<Shape>,
    ) -> DragHandleShapes {
        let defaults = VerticalDragHandleDefaults::shapes();
        DragHandleShapes {
            shape: shape.unwrap_or(defaults.shape),
            pressed_shape: pressed_shape.unwrap_or(defaults.pressed_shape),
            dragged_shape: dragged_shape.unwrap_or(defaults.dragged_shape),
        }
    }

    /// 4 by 48 dp, 12 by 52 dp pressed or dragged, as `sizes()`.
    pub const fn sizes() -> DragHandleSizes {
        DragHandleSizes {
            size: DpSize::new(compose_ui::Dp(4.0), compose_ui::Dp(48.0)),
            pressed_size: DpSize::new(compose_ui::Dp(12.0), compose_ui::Dp(52.0)),
            dragged_size: DpSize::new(compose_ui::Dp(12.0), compose_ui::Dp(52.0)),
        }
    }

    /// As `sizes(size, pressedSize, draggedSize)`: unspecified is the default.
    pub fn sizes_with(size: DpSize, pressed_size: DpSize, dragged_size: DpSize) -> DragHandleSizes {
        let defaults = VerticalDragHandleDefaults::sizes();
        let or = |size: DpSize, default: DpSize| if size.is_specified() { size } else { default };
        DragHandleSizes {
            size: or(size, defaults.size),
            pressed_size: or(pressed_size, defaults.pressed_size),
            dragged_size: or(dragged_size, defaults.dragged_size),
        }
    }
}

/// A capsule that can be dragged to change a component's size or place, as Compose's
/// `VerticalDragHandle`, meant for moving sideways (between panes). It only shows the gesture, a
/// press on its own and a drag from `interaction_source`, where the app emits it (`Modifier.draggable`
/// takes no interaction source yet).
#[composable]
#[allow(non_snake_case)]
pub fn VerticalDragHandle(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::VerticalDragHandleDefaults::sizes())] sizes: DragHandleSizes,
    #[default(crate::VerticalDragHandleDefaults::colors())] colors: DragHandleColors,
    #[default(crate::VerticalDragHandleDefaults::shapes())] shapes: DragHandleShapes,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let source = remembered_interaction_source(interaction_source);
    let pressed = remember(|| mutable_state_of(false));
    // Which state's values show, read where they're used: a press composes nothing again, and
    // lays out only the handle.
    let state = {
        let source = source.clone();
        move || {
            if source.is_dragged() {
                HandleState::Dragged
            } else if pressed.get() {
                HandleState::Pressed
            } else {
                HandleState::Rest
            }
        }
    };
    let (shape_state, size_state, color_state) = (state.clone(), state.clone(), state);
    let modifier = minimum_interactive_component_size(modifier)
        .hoverable(source.clone())
        .then_custom(PressableElement { pressed })
        .graphics_layer(move |layer| {
            layer.shape = match shape_state() {
                HandleState::Dragged => shapes.dragged_shape.clone(),
                HandleState::Pressed => shapes.pressed_shape.clone(),
                HandleState::Rest => shapes.shape.clone(),
            };
            layer.clip = true;
        })
        .layout(move |scope, measurable, _| {
            let size = match size_state() {
                HandleState::Dragged => sizes.dragged_size,
                HandleState::Pressed => sizes.pressed_size,
                HandleState::Rest => sizes.size,
            };
            let density = scope.density();
            let placeable = scope.measure(
                measurable,
                Constraints::fixed(
                    density.round_to_px(size.width),
                    density.round_to_px(size.height),
                ),
            );
            scope.place_relative(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        })
        .draw_behind(move |scope| {
            let color = match color_state() {
                HandleState::Dragged => colors.dragged_color,
                HandleState::Pressed => colors.pressed_color,
                HandleState::Rest => colors.color,
            };
            scope.draw_rect(scope.bounds(), Paint::fill(color));
        })
        .indication(source, crate::ripple::default_ripple());
    Box(modifier, Alignment::TOP_START, false, compose_runtime::callback(|_| {}));
}

#[derive(Clone, Copy)]
enum HandleState {
    Rest,
    Pressed,
    Dragged,
}

/// What the handle's `pressable` is: on the initial pass, so what's on it hears the gesture too.
struct PressableElement {
    pressed: MutableState<bool>,
}

struct PressableNode {
    pressed: MutableState<bool>,
    down: Option<PointerId>,
}

impl PressableNode {
    fn end(&mut self) {
        self.down = None;
        self.pressed.set(false);
    }
}

impl ModifierNodeElement for PressableElement {
    type Node = PressableNode;
    fn create(&self) -> PressableNode {
        PressableNode { pressed: self.pressed, down: None }
    }
    fn update(&self, node: &mut PressableNode) {
        node.pressed = self.pressed;
    }
    fn same(&self, other: &Self) -> bool {
        self.pressed == other.pressed
    }
    fn name(&self) -> &'static str {
        "drag_handle_pressable"
    }
}

impl ModifierNode for PressableNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for PressableNode {
    /// As `awaitFirstDown(Initial)`, then `waitForUpOrCancellation(Initial)`: over at its pointer's
    /// up, or moved out of the bounds, or consumed (by the final pass, what's inside having had its
    /// say).
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        match (pass, self.down) {
            (PointerEventPass::Initial, None) => {
                if event.kind == PointerEventType::Press && !event.consumed {
                    self.down = Some(event.pointer);
                    self.pressed.set(true);
                }
            }
            (PointerEventPass::Initial, Some(down)) if event.pointer == down => {
                let leaves = event.consumed || out_of_bounds(event, bounds, context);
                if event.kind == PointerEventType::Release || leaves {
                    self.end();
                }
            }
            // From the next event on, as `waitForUpOrCancellation`: the press itself may be taken.
            (PointerEventPass::Final, Some(down))
                if event.pointer == down
                    && event.consumed
                    && event.kind != PointerEventType::Press =>
            {
                self.end();
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.end();
    }
}

/// Whether `event` is out of `bounds`, extended to the minimum touch target, as `isOutOfBounds`.
fn out_of_bounds(event: &PointerEvent, bounds: IntSize, context: &EventContext<'_>) -> bool {
    let minimum = context.view_configuration().minimum_touch_target_size;
    let density = context.density();
    let horizontal = ((density.to_px(minimum.width) - bounds.width as f32) / 2.0).max(0.0);
    let vertical = ((density.to_px(minimum.height) - bounds.height as f32) / 2.0).max(0.0);
    let position = event.position;
    position.x < -horizontal
        || position.y < -vertical
        || position.x > bounds.width as f32 + horizontal
        || position.y > bounds.height as f32 + vertical
}
