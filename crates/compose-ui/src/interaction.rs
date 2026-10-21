//! `MutableInteractionSource`, as Compose's: where a clickable reports that it's pressed, so
//! something else (a pill around an icon) can show it.

use std::rc::Rc;

use compose_runtime::{ComposeArg, OwnedState};

/// A clickable's press, focus and hover, shared, as Compose's `MutableInteractionSource` with
/// `collectIsPressedAsState`, `collectIsFocusedAsState` and `collectIsHoveredAsState`. Remember
/// one and give it to `Modifier.clickable_interaction`.
#[derive(Clone)]
pub struct MutableInteractionSource {
    pressed: Rc<OwnedState<bool>>,
    focused: Rc<OwnedState<bool>>,
    hovered: Rc<OwnedState<bool>>,
}

impl MutableInteractionSource {
    pub fn new() -> MutableInteractionSource {
        MutableInteractionSource {
            pressed: Rc::new(compose_runtime::mutable_state_of(false)),
            focused: Rc::new(compose_runtime::mutable_state_of(false)),
            hovered: Rc::new(compose_runtime::mutable_state_of(false)),
        }
    }

    /// Whether the clickable has the focus, as `collectIsFocusedAsState` (a state read).
    pub fn is_focused(&self) -> bool {
        self.focused.get()
    }

    /// Whether a mouse is over the clickable, as `collectIsHoveredAsState` (a state read).
    pub fn is_hovered(&self) -> bool {
        self.hovered.get()
    }

    pub(crate) fn set_focused(&self, focused: bool) {
        self.focused.set(focused);
    }

    pub(crate) fn set_hovered(&self, hovered: bool) {
        self.hovered.set(hovered);
    }

    /// Whether the clickable is pressed (by a pointer or a key), as `collectIsPressedAsState`:
    /// a state read, so what reads it composes, measures or draws again.
    pub fn is_pressed(&self) -> bool {
        self.pressed.get()
    }

    pub(crate) fn set_pressed(&self, pressed: bool) {
        self.pressed.set(pressed);
    }
}

impl Default for MutableInteractionSource {
    fn default() -> Self {
        MutableInteractionSource::new()
    }
}

impl PartialEq for MutableInteractionSource {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.pressed, &other.pressed)
    }
}

impl ComposeArg for MutableInteractionSource {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// `Modifier.pointer_hover_icon`'s node: hit by the mouse, it sets the pointer's look.
struct HoverIconElement(crate::input::PointerIcon);

struct HoverIconNode(crate::input::PointerIcon);

impl crate::modifier::ModifierNodeElement for HoverIconElement {
    fn create(&self) -> Box<dyn crate::modifier::ModifierNode> {
        Box::new(HoverIconNode(self.0))
    }
    fn update(&self, node: &mut dyn crate::modifier::ModifierNode) {
        if let Some(node) = node.downcast_mut::<HoverIconNode>() {
            node.0 = self.0;
        }
    }
    fn same(&self, other: &dyn crate::modifier::ModifierNodeElement) -> bool {
        other.downcast_ref::<HoverIconElement>().is_some_and(|other| other.0 == self.0)
    }
    fn name(&self) -> &'static str {
        "pointer_hover_icon"
    }
}

impl crate::modifier::ModifierNode for HoverIconNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn crate::modifier::PointerInputModifierNode> {
        Some(self)
    }
}

impl crate::modifier::PointerInputModifierNode for HoverIconNode {
    fn on_pointer_event(
        &mut self,
        _event: &mut crate::input::PointerEvent,
        _pass: crate::input::PointerEventPass,
        _bounds: crate::unit::IntSize,
        _context: &mut crate::input::EventContext<'_>,
    ) {
    }

    fn hover_icon(&self) -> Option<crate::input::PointerIcon> {
        Some(self.0)
    }
}

impl crate::modifier::Modifier {
    /// The mouse pointer looks like `icon` over the node, as `Modifier.pointerHoverIcon`.
    pub fn pointer_hover_icon(self, icon: crate::input::PointerIcon) -> crate::modifier::Modifier {
        self.then_custom(HoverIconElement(icon))
    }
}
