//! What keeps content composed while it's needed, as Compose's `PinnableContainer`: a lazy list's
//! item stays composed and placed, scrolled away, while what's in it has the focus.

use std::rc::Rc;

use compose_runtime::CompositionLocal;

use crate::input::EventContext;

/// Content that can be asked to stay, as Compose's `PinnableContainer`.
pub trait PinnableContainer {
    /// Keeps it until the handle is released, as `pin()`.
    fn pin(&self) -> PinnedHandle;
}

/// What a `pin` gives, as `PinnableContainer.PinnedHandle`.
pub struct PinnedHandle(Box<dyn FnOnce()>);

impl PinnedHandle {
    /// A handle calling `release` when it's released.
    pub fn new(release: impl FnOnce() + 'static) -> PinnedHandle {
        PinnedHandle(Box::new(release))
    }

    /// Lets it go, as `release()`.
    pub fn release(self) {
        (self.0)();
    }
}

/// The container around what's composed, as Compose's `LocalPinnableContainer`.
pub static LOCAL_PINNABLE_CONTAINER: CompositionLocal<Option<Rc<dyn PinnableContainer>>> =
    CompositionLocal::new("LocalPinnableContainer", || None);

/// What a focusable node keeps while it has the focus: the container around it pinned, read as
/// the focus comes (nothing on first frames), as `FocusableNode.retrievePinnableContainer`.
#[derive(Default)]
pub struct FocusPin(Option<PinnedHandle>);

impl FocusPin {
    /// The node's focus changed.
    pub fn focus_changed(&mut self, focused: bool, context: &EventContext<'_>) {
        if focused {
            if self.0.is_none()
                && let Some(container) = context.context.current_value_of(&LOCAL_PINNABLE_CONTAINER)
            {
                self.0 = Some(container.pin());
            }
        } else if let Some(handle) = self.0.take() {
            handle.release();
        }
    }
}
