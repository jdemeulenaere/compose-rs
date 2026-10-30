//! A target a test holds on to.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};

use super::support::*;

/// A target node a test holds on to, as Compose's tests keep a `FocusTargetModifierNode`: it has
/// a focusability the test changes and tells its changes as (previous, current).
#[derive(Clone, Copy)]
pub struct HeldTarget {
    focusability: &'static Cell<Focusability>,
    handle: &'static Cell<Option<NodeHandle>>,
    changes: &'static RefCell<Vec<(FocusState, FocusState)>>,
}

impl HeldTarget {
    pub fn new(focusability: Focusability) -> HeldTarget {
        HeldTarget {
            focusability: cell(focusability),
            handle: cell(None),
            changes: Box::leak(Box::default()),
        }
    }

    /// The element of this node.
    pub fn modifier(self) -> Modifier {
        Modifier.then_custom(HeldElement(self))
    }

    pub fn handle(self) -> NodeHandle {
        self.handle.get().expect("attached")
    }

    /// Its state, `Inactive` before it's attached.
    pub fn state(self) -> FocusState {
        self.handle.get().map_or(Inactive, |handle| handle.focus_state())
    }

    /// Requests the focus for it, as its `requestFocus`.
    pub fn request(self) -> bool {
        self.handle().request_focus(Enter)
    }

    /// Sets its focusability, before it's attached too.
    pub fn set_focusability(self, focusability: Focusability) {
        self.focusability.set(focusability);
        if let Some(handle) = self.handle.get() {
            handle.invalidate_focus_properties();
        }
    }

    /// What it was told, and forgets it.
    pub fn take_changes(self) -> Vec<(FocusState, FocusState)> {
        std::mem::take(&mut *self.changes.borrow_mut())
    }
}

struct HeldElement(HeldTarget);

struct HeldNode {
    held: HeldTarget,
}

impl ModifierNodeElement for HeldElement {
    type Node = HeldNode;
    fn create(&self) -> HeldNode {
        HeldNode { held: self.0 }
    }
    fn update(&self, _node: &mut HeldNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "held_target"
    }
}

impl ModifierNode for HeldNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.held.handle.set(Some(context.handle()));
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn should_auto_invalidate(&self) -> bool {
        false
    }
}

impl FocusTargetModifierNode for HeldNode {
    fn focusability(&self) -> Focusability {
        self.held.focusability.get()
    }
    fn on_focus_changed(
        &mut self,
        previous: FocusState,
        current: FocusState,
        _context: &mut EventContext<'_>,
    ) {
        self.held.changes.borrow_mut().push((previous, current));
    }
}
