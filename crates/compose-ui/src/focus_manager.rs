//! `FocusManager` and `SoftwareKeyboardController`, as Compose's `LocalFocusManager` and
//! `LocalSoftwareKeyboardController`: handles application code clears or moves the focus with,
//! and shows or hides the on-screen keyboard with, from a callback or an effect.
//!
//! Both are handles on their tree's requests (as `FocusRequester`'s): what they ask is done at
//! once where the tree is free (D1268), else once it's free, after the event that asked or at the
//! next frame. Their locals' values are the composing tree's, so no host provides them.

use std::rc::{Rc, Weak};

use compose_runtime::{ComposeArg, CompositionLocal};

use crate::focus_requester::{FocusRequest, FocusRequests, perform_or_queue};
use crate::tree::UiTree;

/// Where `FocusManager::move_focus` moves the focus, as Compose's `FocusDirection`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FocusDirection {
    /// The next focusable in Tab's order, wrapping around.
    Next,
    /// The previous one, as Shift+Tab.
    Previous,
    /// The nearest focusable above the focused one (Compose's two-dimensional search).
    Up,
    Down,
    Left,
    Right,
    /// Into the focused target's children, as the D-pad's center.
    Enter,
    /// Out, to the nearest ancestor that can take the focus, as Back.
    Exit,
}

/// Clears and moves the focus, as Compose's `FocusManager`: `LOCAL_FOCUS_MANAGER.current()` in
/// composition, then `clear_focus(false)` or `move_focus(FocusDirection::Next)` from a callback.
#[derive(Clone)]
pub struct FocusManager {
    requests: Weak<FocusRequests>,
}

impl FocusManager {
    /// The composing tree's, outside composition or another kind of tree's a manager that does
    /// nothing.
    fn of_composing_tree() -> FocusManager {
        FocusManager { requests: composing_tree_requests() }
    }

    /// Clears the focus, as `clearFocus(force)`: the focused node and its ancestors hear they
    /// lost it, and a text field's input method goes. Unless forced, a target that captured the
    /// focus refuses, and each `on_exit` on the focused path may refuse or redirect.
    pub fn clear_focus(&self, force: bool) {
        if let Some(requests) = self.requests.upgrade() {
            perform_or_queue(&requests, FocusRequest::Clear { force });
        }
    }

    /// Moves the focus in `direction`, as `moveFocus(direction)`: a custom destination
    /// (`focus_properties`) first, then Compose's searches: `Next` and `Previous` through the
    /// targets in placement order, wrapping around; `Up`, `Down`, `Left` and `Right` to the
    /// nearest among the focused one's siblings, then out of its parent; `Enter` into its
    /// children and `Exit` to its parent. With nothing focused, `Next`, `Down` and `Right` start
    /// from the window's top left, `Previous`, `Up` and `Left` from its bottom right.
    ///
    /// Returns whether the focus moved (where the tree is busy, from inside a pass, the move waits
    /// for it: whether a tree was asked; a search waiting for a lazy list's next item answers
    /// `true`, as it's expected to find it, D477).
    pub fn move_focus(&self, direction: FocusDirection) -> bool {
        match self.requests.upgrade() {
            Some(requests) => perform_or_queue(&requests, FocusRequest::Move(direction)),
            None => false,
        }
    }
}

impl ComposeArg for FocusManager {
    fn same(&self, other: &Self) -> bool {
        self.requests.ptr_eq(&other.requests)
    }
}

impl std::fmt::Debug for FocusManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FocusManager")
    }
}

/// The focus manager of the composing tree, as Compose's `LocalFocusManager`.
pub static LOCAL_FOCUS_MANAGER: CompositionLocal<FocusManager> =
    CompositionLocal::new("LocalFocusManager", FocusManager::of_composing_tree);

/// Shows and hides the on-screen keyboard, as Compose's `SoftwareKeyboardController`:
/// `LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current()` in composition, then `hide()` from a callback
/// (a search field's `on_search`).
///
/// It asks the host's platform (`UiHost::take_software_keyboard_request`): Android shows its
/// keyboard for the focused text field, or hides it; desktop has none. As Compose's, `show`
/// does nothing without a focused text field.
#[derive(Clone)]
pub struct SoftwareKeyboardController {
    requests: Weak<FocusRequests>,
}

impl SoftwareKeyboardController {
    pub fn show(&self) {
        self.request(true);
    }

    pub fn hide(&self) {
        self.request(false);
    }

    fn request(&self, show: bool) {
        if let Some(requests) = self.requests.upgrade() {
            requests.software_keyboard.set(Some(show));
            requests.wake();
        }
    }
}

impl ComposeArg for SoftwareKeyboardController {
    fn same(&self, other: &Self) -> bool {
        self.requests.ptr_eq(&other.requests)
    }
}

impl std::fmt::Debug for SoftwareKeyboardController {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SoftwareKeyboardController")
    }
}

/// The keyboard controller of the composing tree, as Compose's
/// `LocalSoftwareKeyboardController`: `None` where there's no tree (another kind of
/// composition), as Compose's is where there's no keyboard.
pub static LOCAL_SOFTWARE_KEYBOARD_CONTROLLER: CompositionLocal<
    Option<SoftwareKeyboardController>,
> = CompositionLocal::new("LocalSoftwareKeyboardController", || {
    let requests = composing_tree_requests();
    (requests.strong_count() > 0).then_some(SoftwareKeyboardController { requests })
});

/// The requests of the tree composing now: none outside a `UiTree`'s composition.
fn composing_tree_requests() -> Weak<FocusRequests> {
    if !compose_runtime::is_composing() {
        return Weak::new();
    }
    let composer = compose_runtime::platform::current();
    if !composer.applier_is::<UiTree>() {
        return Weak::new();
    }
    composer.with_applier(|tree: &mut UiTree| Rc::downgrade(&tree.focus_requests))
}
