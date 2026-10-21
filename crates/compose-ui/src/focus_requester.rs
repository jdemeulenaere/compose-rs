//! `FocusRequester`, as Compose's: a handle application code moves focus with, attached to a
//! node by `Modifier.focus_requester`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use compose_runtime::ComposeArg;

use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement};
use crate::tree::NodeId;

/// The focus requests a tree has been given, handled by its host once it's free (after the
/// event dispatched, or at the next frame).
#[derive(Default)]
pub(crate) struct FocusRequests {
    pub(crate) pending: RefCell<Vec<NodeId>>,
    /// Asks the host for a frame.
    pub(crate) wake: RefCell<Option<Rc<dyn Fn()>>>,
}

/// Moves focus to the node it's attached to (`Modifier.focus_requester`), as Compose's
/// `FocusRequester`: `remember(FocusRequester::new)`, then `request_focus()` from a callback or
/// an effect. The focus goes to the node's focus target or the first one below it, scrolled
/// into view, once the tree is free: after the event that asked, or at the next frame.
#[derive(Clone, Default)]
pub struct FocusRequester {
    inner: Rc<RequesterInner>,
}

#[derive(Default)]
struct RequesterInner {
    /// The tree and node it's attached to, if any.
    owner: RefCell<Option<(Weak<FocusRequests>, NodeId)>>,
}

impl FocusRequester {
    pub fn new() -> FocusRequester {
        FocusRequester::default()
    }

    /// Moves focus to its node, as `requestFocus()`. Returns whether it's attached to one (a
    /// requester whose node isn't composed does nothing).
    pub fn request_focus(&self) -> bool {
        let owner = self.inner.owner.borrow().clone();
        let Some((requests, node)) = owner else { return false };
        let Some(requests) = requests.upgrade() else { return false };
        requests.pending.borrow_mut().push(node);
        let wake = requests.wake.borrow().clone();
        if let Some(wake) = wake {
            wake();
        }
        true
    }

    pub(crate) fn attach(&self, requests: &Rc<FocusRequests>, node: NodeId) {
        *self.inner.owner.borrow_mut() = Some((Rc::downgrade(requests), node));
    }

    fn detach(&self) {
        self.inner.owner.borrow_mut().take();
    }
}

impl ComposeArg for FocusRequester {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
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

pub(crate) struct FocusRequesterNode(pub(crate) FocusRequester);

impl ModifierNodeElement for FocusRequesterElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(FocusRequesterNode(self.0.clone()))
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<FocusRequesterNode>()
            && !node.0.same(&self.0)
        {
            node.0.detach();
            node.0 = self.0.clone();
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
        self.0.detach();
    }
}
