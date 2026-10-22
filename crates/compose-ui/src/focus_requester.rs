//! `FocusRequester`, as Compose's: a handle application code moves focus with, attached to a
//! node by `Modifier.focus_requester`.

use std::cell::RefCell;
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
#[derive(Clone, Default)]
pub struct FocusRequester {
    inner: Rc<RequesterInner>,
}

#[derive(Default)]
struct RequesterInner {
    /// The trees and nodes it's attached to, first attached first.
    owners: RefCell<Vec<(Weak<FocusRequests>, NodeId)>>,
}

impl FocusRequester {
    pub fn new() -> FocusRequester {
        FocusRequester::default()
    }

    /// Moves focus to its node (the first it was attached to, if several), as
    /// `requestFocus()`. Returns whether it's attached to one (a requester whose node isn't
    /// composed does nothing).
    pub fn request_focus(&self) -> bool {
        let owner = self
            .inner
            .owners
            .borrow()
            .iter()
            .find_map(|(requests, node)| Some((requests.upgrade()?, *node)));
        let Some((requests, node)) = owner else { return false };
        requests.push(FocusRequest::Node(node));
        true
    }

    fn attach(&self, requests: &Weak<FocusRequests>, node: NodeId) {
        let mut owners = self.inner.owners.borrow_mut();
        if !owners.iter().any(|(tree, at)| *at == node && tree.ptr_eq(requests)) {
            owners.push((requests.clone(), node));
        }
    }

    fn detach(&self, requests: &Weak<FocusRequests>, node: NodeId) {
        self.inner.owners.borrow_mut().retain(|(tree, at)| !(*at == node && tree.ptr_eq(requests)));
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
        Box::new(FocusRequesterNode { requester: self.0.clone(), owner: None })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<FocusRequesterNode>()
            && !node.requester.same(&self.0)
        {
            if let Some((requests, at)) = &node.owner {
                node.requester.detach(requests, *at);
                self.0.attach(requests, *at);
            }
            node.requester = self.0.clone();
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
