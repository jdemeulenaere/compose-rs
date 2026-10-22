//! `FocusRequester`, as Compose's: a handle application code moves focus with, attached to a
//! node by `Modifier.focus_requester`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use compose_runtime::ComposeArg;

use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement};
use crate::tree::NodeId;

/// The focus requests a tree has been given (by `FocusRequester`s and the `FocusManager`),
/// handled by its host once it's free (after the event dispatched, or at the next frame); and
/// what the `SoftwareKeyboardController` last asked, for the platform.
#[derive(Default)]
pub(crate) struct FocusRequests {
    pub(crate) pending: RefCell<Vec<FocusRequest>>,
    /// Show (true) or hide (false) the on-screen keyboard: the last request, until the host's
    /// platform takes it.
    pub(crate) software_keyboard: std::cell::Cell<Option<bool>>,
    /// Asks the host for a frame.
    pub(crate) wake: RefCell<Option<Rc<dyn Fn()>>>,
}

/// A focus change asked for outside dispatch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FocusRequest {
    /// `FocusRequester::request_focus`: the node's focus target.
    Node(NodeId),
    /// `FocusManager::clear_focus`.
    Clear { force: bool },
    /// `FocusManager::move_focus`.
    Move(crate::focus_manager::FocusDirection),
}

impl FocusRequests {
    /// Queues `request` and asks the host for a frame.
    pub(crate) fn push(&self, request: FocusRequest) {
        self.pending.borrow_mut().push(request);
        self.wake();
    }

    pub(crate) fn wake(&self) {
        let wake = self.wake.borrow().clone();
        if let Some(wake) = wake {
            wake();
        }
    }
}

/// Moves focus to the node it's attached to (`Modifier.focus_requester`), as Compose's
/// `FocusRequester`: `remember(FocusRequester::new)`, then `request_focus()` from a callback or
/// an effect. The focus goes to the node's focus target or the first one below it, scrolled
/// into view, once the tree is free: after the event that asked, or at the next frame.
///
/// A `Copy` id (D240): the nodes it's attached to are kept by the thread's registry while
/// they're attached, so it owns nothing and is never stale. It's `!Send`, as state handles
/// are: the registry is the thread's.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FocusRequester {
    id: u64,
    _thread: std::marker::PhantomData<*const ()>,
}

/// The trees and nodes a requester is attached to, first attached first.
type Attachments = Vec<(Weak<FocusRequests>, NodeId)>;

thread_local! {
    /// Each requester's attachments: only those attached somewhere have an entry.
    static ATTACHED: RefCell<HashMap<u64, Attachments>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

impl Default for FocusRequester {
    fn default() -> FocusRequester {
        FocusRequester::new()
    }
}

impl FocusRequester {
    pub fn new() -> FocusRequester {
        let id = NEXT_ID.with(|next| next.replace(next.get() + 1));
        FocusRequester { id, _thread: std::marker::PhantomData }
    }

    /// Moves focus to its node (the first it was attached to, if several), as
    /// `requestFocus()`. Returns whether it's attached to one (a requester whose node isn't
    /// composed does nothing).
    pub fn request_focus(&self) -> bool {
        let owner = ATTACHED.with(|attached| {
            attached.borrow().get(&self.id).and_then(|owners| {
                owners.iter().find_map(|(requests, node)| Some((requests.upgrade()?, *node)))
            })
        });
        let Some((requests, node)) = owner else { return false };
        requests.push(FocusRequest::Node(node));
        true
    }

    fn attach(&self, requests: &Weak<FocusRequests>, node: NodeId) {
        ATTACHED.with(|attached| {
            let mut attached = attached.borrow_mut();
            let owners = attached.entry(self.id).or_default();
            if !owners.iter().any(|(tree, at)| *at == node && tree.ptr_eq(requests)) {
                owners.push((requests.clone(), node));
            }
        });
    }

    fn detach(&self, requests: &Weak<FocusRequests>, node: NodeId) {
        // Not at all once the thread's registry is gone (a tree dropped as the thread exits).
        let _ = ATTACHED.try_with(|attached| {
            let mut attached = attached.borrow_mut();
            if let Some(owners) = attached.get_mut(&self.id) {
                owners.retain(|(tree, at)| !(*at == node && tree.ptr_eq(requests)));
                if owners.is_empty() {
                    attached.remove(&self.id);
                }
            }
        });
    }
}

impl ComposeArg for FocusRequester {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Debug for FocusRequester {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("FocusRequester")
    }
}

impl Modifier {
    /// Attaches `requester` to this node, as `Modifier.focusRequester`: its `request_focus`
    /// focuses this node's focus target (the `focusable`, text field or clickable after it) or
    /// the first one below.
    pub fn focus_requester(self, requester: FocusRequester) -> Modifier {
        self.then_custom(FocusRequesterElement(requester))
    }
}

struct FocusRequesterElement(FocusRequester);

pub(crate) struct FocusRequesterNode {
    requester: FocusRequester,
    /// The tree and node it's on, once attached.
    owner: Option<(Weak<FocusRequests>, NodeId)>,
}

impl FocusRequesterNode {
    /// Attaches its requester to `node` of the tree whose requests are `requests`.
    pub(crate) fn attach(&mut self, requests: &Rc<FocusRequests>, node: NodeId) {
        let owner = (Rc::downgrade(requests), node);
        self.requester.attach(&owner.0, owner.1);
        self.owner = Some(owner);
    }
}

impl ModifierNodeElement for FocusRequesterElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(FocusRequesterNode { requester: self.0, owner: None })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<FocusRequesterNode>()
            && !node.requester.same(&self.0)
        {
            if let Some((requests, at)) = &node.owner {
                node.requester.detach(requests, *at);
                self.0.attach(requests, *at);
            }
            node.requester = self.0;
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<FocusRequesterElement>().is_some_and(|other| other.0.same(&self.0))
    }
    fn name(&self) -> &'static str {
        "focus_requester"
    }
}

impl ModifierNode for FocusRequesterNode {
    fn on_detach(&mut self, _context: &mut crate::modifier::DetachContext<'_>) {
        if let Some((requests, at)) = self.owner.take() {
            self.requester.detach(&requests, at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A requester owns nothing: the registry has it only while it's attached, so requesters
    /// made and dropped leave nothing behind.
    #[test]
    fn a_requester_is_registered_only_while_attached() {
        let requester = FocusRequester::new();
        let requests = Rc::new(FocusRequests::default());
        let tree = Rc::downgrade(&requests);
        let node = NodeId::from_bits(7);
        assert!(!requester.request_focus());
        requester.attach(&tree, node);
        assert!(requester.request_focus());
        assert_eq!(*requests.pending.borrow(), [FocusRequest::Node(node)]);
        requester.detach(&tree, node);
        assert!(ATTACHED.with(|attached| attached.borrow().is_empty()));
        assert!(!requester.request_focus());
        assert_ne!(FocusRequester::new(), requester);
    }
}
