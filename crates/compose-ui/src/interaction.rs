//! `MutableInteractionSource`, as Compose's: where a clickable reports that it's pressed, so
//! something else (a pill around an icon) can show it.

use std::rc::Rc;

use compose_runtime::{ComposeArg, OwnedState};

/// A clickable's press, shared, as Compose's `MutableInteractionSource` with
/// `collectIsPressedAsState`. Remember one and give it to `Modifier.clickable_interaction`.
#[derive(Clone)]
pub struct MutableInteractionSource {
    pressed: Rc<OwnedState<bool>>,
}

impl MutableInteractionSource {
    pub fn new() -> MutableInteractionSource {
        MutableInteractionSource { pressed: Rc::new(compose_runtime::mutable_state_of(false)) }
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
