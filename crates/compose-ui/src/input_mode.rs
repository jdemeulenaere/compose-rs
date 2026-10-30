//! The input mode, as Compose's `InputMode` and `InputModeManager`: whether the user is touching
//! the screen or using keys, which decides whether what's `Focusability::SYSTEM_DEFINED` can take
//! the focus. The host says which it is (`UiHost::set_input_mode`).

use std::rc::Rc;

use compose_runtime::{ComposeArg, CompositionLocal, OwnedState};

/// How the user is using the UI, as Compose's `InputMode`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum InputMode {
    /// A touch screen: nothing but what asks for it (a text field) shows the focus.
    Touch,
    /// Keys: everything that can take the focus can.
    Keyboard,
}

/// What reads and asks to change the input mode, as Compose's `InputModeManager`:
/// `LOCAL_INPUT_MODE_MANAGER.current()`.
#[derive(Clone)]
pub struct InputModeManager {
    mode: Rc<OwnedState<InputMode>>,
}

impl InputModeManager {
    pub(crate) fn new() -> InputModeManager {
        InputModeManager { mode: Rc::new(compose_runtime::mutable_state_of(InputMode::Keyboard)) }
    }

    /// The input mode, as state: what reads it is told when it changes, as `inputMode`.
    pub fn input_mode(&self) -> InputMode {
        self.mode.get()
    }

    /// Asks for a mode, as `requestInputMode`: whether it's the mode now (a host decides: a
    /// headless or desktop one always grants it).
    pub fn request_input_mode(&self, mode: InputMode) -> bool {
        self.mode.set(mode);
        true
    }

    pub(crate) fn set_input_mode(&self, mode: InputMode) {
        self.mode.set(mode);
    }
}

impl ComposeArg for InputModeManager {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.mode, &other.mode)
    }
}

impl std::fmt::Debug for InputModeManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "InputModeManager({:?})", self.input_mode())
    }
}

/// The composing tree's input mode manager, as Compose's `LocalInputModeManager`.
pub static LOCAL_INPUT_MODE_MANAGER: CompositionLocal<InputModeManager> =
    CompositionLocal::new("LocalInputModeManager", || {
        if compose_runtime::is_composing() {
            let composer = compose_runtime::platform::current();
            if composer.applier_is::<crate::tree::UiTree>() {
                return composer
                    .with_applier(|tree: &mut crate::tree::UiTree| tree.input_mode.clone());
            }
        }
        InputModeManager::new()
    });
