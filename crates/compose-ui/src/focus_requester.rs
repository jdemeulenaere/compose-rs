//! `FocusRequester`, as Compose's: a handle application code moves focus with, attached to a
//! node by `Modifier.focus_requester`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use compose_runtime::ComposeArg;
use smallvec::SmallVec;

use crate::focus::{FocusState, FocusTargetRef};
use crate::focus_manager::FocusDirection;
use crate::focus_search::OnFound;
use crate::modifier::{FocusRequesterModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::node_chain::{EntryId, NodeKinds};
use crate::node_context::{NodeContext, NodeHandle, requests_of, with_free_tree};
use crate::tree::{NodeId, UiTree};
use crate::unit::{Offset, Rect};

/// The requests a tree has been given from outside its passes: focus requests (by
/// `FocusRequester`s and the `FocusManager`) made while it was busy, handled by its host once
/// it's free (after the event dispatched, or at the next frame); what the
/// `SoftwareKeyboardController` last asked, for the platform; and its modifier nodes'
/// invalidations through their handles. The design's `TreeRequests`
/// (docs/parity/design/modifier-nodes.md §3.3), its name kept.
#[derive(Default)]
pub(crate) struct FocusRequests {
    /// The window's whole size, as `WindowInfo::container_size` tells it.
    pub(crate) container_size: Cell<crate::unit::IntSize>,
    pub(crate) pending: RefCell<Vec<FocusRequest>>,
    /// The tree's key in the thread's registry, for what holds only this.
    pub(crate) tree_key: Cell<u32>,
    /// Changes when the focus moves: what's queued for application code before is stale.
    pub(crate) focus_generation: crate::focus::FocusGeneration,
    /// What the window's view should do: take the focus (the content gained it) or give it up
    /// (`clear_focus`), as `requestOwnerFocus` and `clearOwnerFocus`.
    pub(crate) owner_focus: std::cell::Cell<Option<bool>>,
    /// The window taking keys and searches, for `WindowInfo::is_window_focused`.
    pub(crate) focused_window: crate::focus_manager::FocusedWindow,
    /// Show (true) or hide (false) the on-screen keyboard: the last request, until the host's
    /// platform takes it.
    pub(crate) software_keyboard: std::cell::Cell<Option<bool>>,
    /// Asks the host for a frame.
    pub(crate) wake: RefCell<Option<Rc<dyn Fn()>>>,
    /// What modifier nodes asked through their handles (`NodeHandle`), applied where the tree
    /// applies observed writes (`UiTree::apply_node_requests`).
    pub(crate) node_invalidations:
        RefCell<Vec<(NodeId, crate::node_chain::EntryId, crate::node_context::Invalidation)>>,
    /// Layout nodes a `Remeasurement` asked to measure while their tree was busy: measured in
    /// the next layout.
    pub(crate) measure_requests: RefCell<Vec<NodeId>>,
    /// The reads modifier nodes observe, here so handles reach them with the tree busy.
    pub(crate) observations: RefCell<crate::observer_node::NodeObservations>,
    /// The entries handles were made for that are attached, by `EntryId`: what
    /// `NodeHandle::is_attached` answers without the tree.
    /// FNV-hashed: asked for each request a handle makes.
    pub(crate) attached_entries: RefCell<
        std::collections::HashSet<u64, std::hash::BuildHasherDefault<compose_runtime::KeyHasher>>,
    >,
    /// The coroutine scopes nodes made (`NodeContext::coroutine_scope`,
    /// `NodeHandle::coroutine_scope`), cancelled when they are detached. Here for a handle to
    /// reach one while the tree is busy (an event handler, an element's update).
    pub(crate) scopes: RefCell<
        std::collections::HashMap<
            crate::node_chain::EntryId,
            compose_runtime::Owned<compose_runtime::CoroutineScope>,
            std::hash::BuildHasherDefault<compose_runtime::KeyHasher>,
        >,
    >,
    /// What nodes asked to run through their handles (`NodeHandle::side_effect`), run with the
    /// tree's own side effects.
    pub(crate) side_effects: RefCell<Vec<Box<dyn FnOnce()>>>,
}

/// A focus change asked for while the tree was busy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum FocusRequest {
    /// The node's focus target, or the first below it: its semantics' `RequestFocus`.
    Node(NodeId),
    /// `FocusManager::clear_focus`, of a window.
    Clear { force: bool, window: Option<NodeId> },
    /// `FocusManager::move_focus`, in a window.
    Move(FocusDirection, Option<NodeId>),
    /// What a `FocusRequester` asks.
    Requester(FocusRequester, RequesterOperation),
    /// What a node asks of the targets at and below it, its own running.
    Entry(NodeId, EntryId, RequesterOperation),
    /// A semantics action bound to the node's modifier nodes (D1299).
    TextAction(NodeId, crate::modifier::TextAction),
}

/// What's asked of a requester's nodes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RequesterOperation {
    Request(FocusDirection),
    Capture,
    Free,
    Save,
    Restore,
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

/// Does `request` on the tree of `requests` now if it's free (D1268), running what it calls, else
/// queues it for when it is. Returns what it did, or `true` if queued.
pub(crate) fn perform_or_queue(requests: &Rc<FocusRequests>, request: FocusRequest) -> bool {
    let key = requests.tree_key.get();
    let queued = request.clone();
    match on_free_tree(key, |tree| tree.perform_focus_request_in_transaction(request)) {
        Some(result) => result,
        None => {
            requests.push(queued);
            true
        }
    }
}

/// Runs `f` with the tree of `key` if it's free, then what it queued for application code, now
/// that the tree is free again; asks for a frame if it changed anything. `None` if it's busy.
pub(crate) fn on_free_tree<R>(key: u32, f: impl FnOnce(&mut UiTree) -> R) -> Option<R> {
    let _turn = compose_runtime::platform::Turn::begin();
    let mut f = Some(f);
    let mut result = None;
    let mut actions = Vec::new();
    let mut dirty = false;
    // Inside a block's transaction: what it queues for application code waits for it.
    let in_block = crate::node_context::tree_is_set_aside(key);
    with_free_tree(key, &mut |tree: &mut UiTree| {
        if let Some(f) = f.take() {
            result = Some(f(tree));
            if !in_block {
                actions = tree.take_pending_actions();
            }
            dirty = tree.needs_draw || tree.semantics_changed;
        }
    });
    let result = result?;
    crate::input::run_actions(actions);
    if dirty && let Some(requests) = requests_of(key) {
        requests.wake();
    }
    Some(result)
}

/// Reads the tree of `key` if it's free.
pub(crate) fn read_free_tree<R>(key: u32, read: impl FnOnce(&mut UiTree) -> R) -> Option<R> {
    let mut read = Some(read);
    let mut result = None;
    with_free_tree(key, &mut |tree: &mut UiTree| {
        if let Some(read) = read.take() {
            result = Some(read(tree));
        }
    });
    result
}

impl UiTree {
    /// Does what a `FocusRequester` or the `FocusManager` asked. Returns its result.
    pub(crate) fn perform_focus_request_in_transaction(&mut self, request: FocusRequest) -> bool {
        match request {
            FocusRequest::Node(node) => {
                self.get(node).is_some()
                    && self.request_focus_at_or_below(node, FocusDirection::Enter)
            }
            FocusRequest::Clear { force, window } => {
                let cleared = self.in_window(window, |tree| {
                    tree.clear_focus_with(force, true, FocusDirection::Exit)
                });
                // As `clearOwnerFocus`: in keyboard mode, the view system gives it back. One view
                // stands for all the windows: it stays while another has the focus (D1284).
                if cleared && !self.a_window_has_active(|_| true) {
                    self.focus_requests.owner_focus.set(Some(false));
                    self.focus_requests.wake();
                }
                cleared
            }
            FocusRequest::Move(direction, window) => {
                self.in_window(window, |tree| tree.move_focus_with(direction, true))
            }
            FocusRequest::Requester(requester, operation) => {
                requester.perform_on_tree(self, operation)
            }
            FocusRequest::Entry(node, entry, operation) => {
                self.perform_on_entry(node, entry, operation)
            }
            FocusRequest::TextAction(node, action) => self.perform_text_action(node, action),
        }
    }

    /// Does what a `FocusRequester` or the `FocusManager` asked, now that the tree is free. Returns
    /// the actions focus listeners queued.
    pub(crate) fn perform_focus_request(
        &mut self,
        request: FocusRequest,
    ) -> Vec<crate::input::Action> {
        self.perform_focus_request_in_transaction(request);
        self.take_pending_actions()
    }

    /// `operation` on the targets at and below entry `entry` of `node` (a chain entry or a
    /// delegate), as `FocusRequesterModifierNode`'s functions.
    pub(crate) fn perform_on_entry(
        &mut self,
        node: NodeId,
        entry: EntryId,
        operation: RequesterOperation,
    ) -> bool {
        let targets = self.self_and_children_targets(node, entry);
        match operation {
            // The first target's answer, as `requestFocus`'s `return`.
            RequesterOperation::Request(direction) => match targets.first() {
                Some(&target) => self.request_focus_target(target, direction),
                None => false,
            },
            RequesterOperation::Capture => {
                targets.into_iter().any(|target| self.capture_focus_target(target))
            }
            RequesterOperation::Free => {
                targets.into_iter().any(|target| self.free_focus_target(target))
            }
            RequesterOperation::Save => {
                targets.into_iter().any(|target| self.save_focused_child_of(Some(target)))
            }
            RequesterOperation::Restore => {
                targets.into_iter().any(|target| self.restore_focused_child_of(Some(target)))
            }
        }
    }

    /// The own target of a chain entry or delegate: the first it has.
    pub(crate) fn own_target_at_entry(
        &mut self,
        node: NodeId,
        entry: EntryId,
    ) -> Option<FocusTargetRef> {
        let index = self.entry_index(node, entry)?;
        let chain_entry = &mut self.node_mut(node).chain[index];
        let id = chain_entry.id;
        if !chain_entry.kinds.any(NodeKinds::FOCUS_TARGET) {
            return None;
        }
        let mut first = None;
        let mut take = |_: &mut dyn ModifierNode, delegate: EntryId| {
            first.get_or_insert(FocusTargetRef { node, entry: id, delegate });
        };
        if id == entry {
            crate::delegation::dispatch_for_kind(
                &mut *chain_entry.node,
                chain_entry.own,
                NodeKinds::FOCUS_TARGET,
                EntryId::NO_DELEGATE,
                &mut take,
            );
        } else if let Some(delegate) =
            crate::delegation::find_delegate(&mut *chain_entry.node, entry)
        {
            crate::delegation::dispatch_delegate_for_kind(
                delegate,
                NodeKinds::FOCUS_TARGET,
                &mut take,
            );
        }
        first
    }

    /// `focus_state` of the own target of a chain entry or delegate.
    pub(crate) fn focus_state_at_entry(&mut self, node: NodeId, entry: EntryId) -> FocusState {
        match self.own_target_at_entry(node, entry) {
            Some(target) => self.focus_state_of(target),
            None => FocusState::Inactive,
        }
    }

    /// Where a chain entry or a delegate's layer is in the root.
    pub(crate) fn origin_of_entry(&mut self, node: NodeId, entry: EntryId) -> Offset {
        let Some(index) = self.entry_index(node, entry) else { return Offset::ZERO };
        let layer = self.node(node).layer_of(index);
        self.layer_origin(node, layer.min(self.node(node).layers.len() - 1))
    }

    /// As `RectManager.findFocusableNodeFromRect` and `requestFocusForChildInRootBounds`: the
    /// target nearest the root of `container`'s subtree overlapping the rectangle takes the
    /// focus. Still overlapping what has it: no change.
    pub(crate) fn request_focus_for_child_in_bounds(
        &mut self,
        container: NodeId,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    ) -> bool {
        self.in_window_of(container, |tree| {
            tree.request_focus_for_child_here(container, left, top, right, bottom)
        })
    }

    fn request_focus_for_child_here(
        &mut self,
        container: NodeId,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    ) -> bool {
        let rect = Rect::new(left as f32, top as f32, right as f32, bottom as f32);
        let overlaps = |bounds: &Rect| {
            bounds.left < rect.right
                && bounds.right > rect.left
                && bounds.top < rect.bottom
                && bounds.bottom > rect.top
        };
        // A walk down, depth first: the node at the smallest depth wins, the first on a tie.
        let mut best: Option<(usize, FocusTargetRef)> = None;
        let mut stack: SmallVec<[(NodeId, usize); 16]> = SmallVec::new();
        stack.push((container, 0));
        while let Some((node, depth)) = stack.pop() {
            let Some(value) = self.get(node) else { continue };
            if value.deactivated || (node != container && !value.placed) {
                continue;
            }
            let children: SmallVec<[NodeId; 8]> = value
                .children
                .iter()
                .rev()
                .map(|child| child.node)
                .filter(|&child| !self.node(child).overlay)
                .collect();
            stack.extend(children.into_iter().map(|child| (child, depth + 1)));
            let head = self
                .node(node)
                .chain
                .iter()
                .find(|entry| entry.kinds.any(NodeKinds::FOCUS_TARGET))
                .map(|entry| entry.id);
            // Below the container, not it (`isDescendantOf`).
            let Some(entry) = head.filter(|_| node != container) else { continue };
            let Some(target) = self.own_target_at_entry(node, entry) else { continue };
            // Its layout's bounds and its own: one moved by a modifier off where its node is laid
            // out isn't found there (the rectangles of `RectManager`, then `intersects`).
            let bounds = self.focus_rect_of(target);
            if !overlaps(&self.bounds_in_root(node)) || !overlaps(&bounds) {
                continue;
            }
            if self.focus.active == Some(target) {
                return false;
            }
            if best.is_none_or(|(smallest, _)| depth < smallest) {
                best = Some((depth, target));
            }
        }
        match best {
            Some((_, target)) => self.request_focus_target(target, FocusDirection::Enter),
            None => false,
        }
    }
}

/// Moves focus to the node it's attached to (`Modifier.focus_requester`), as Compose's
/// `FocusRequester`: `remember(FocusRequester::new)`, then `request_focus()` from a callback or
/// an effect. The focus goes to the node's focus target or the first one below it. Where the
/// tree is free (a callback, an effect, a task, a test) it's done at once and the result is real;
/// where it's busy (in composition, in a modifier node's or an `on_enter` block's code) it's done
/// once it's free, and the result is whether it's attached (D1268).
///
/// A `Copy` id (D240): the nodes it's attached to are kept by the thread's registry while
/// they're attached, so it owns nothing and is never stale. It's `!Send`, as state handles
/// are: the registry is the thread's.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FocusRequester {
    id: u64,
    _thread: std::marker::PhantomData<*const ()>,
}

/// Where a requester is attached: a tree, a layout node and the modifier node's chain entry.
type Attachment = (u32, NodeId, EntryId);

thread_local! {
    /// Each requester's attachments, first attached first: only those attached somewhere have an
    /// entry.
    static ATTACHED: RefCell<HashMap<u64, SmallVec<[Attachment; 1]>>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(2) };
}

impl Default for FocusRequester {
    fn default() -> FocusRequester {
        FocusRequester::new()
    }
}

/// What `FocusRequester::create_refs` can make: tuples of up to 16, as Compose's factory's
/// `component16`.
pub trait FocusRequesterRefs {
    fn make() -> Self;
}

macro_rules! requester_refs {
    ($($name:ident)*) => {
        impl FocusRequesterRefs for ($($name,)*) {
            fn make() -> Self {
                ($($name::new(),)*)
            }
        }
    };
}

use FocusRequester as R;
requester_refs!(R R);
requester_refs!(R R R);
requester_refs!(R R R R);
requester_refs!(R R R R R);
requester_refs!(R R R R R R);
requester_refs!(R R R R R R R);
requester_refs!(R R R R R R R R);
requester_refs!(R R R R R R R R R);
requester_refs!(R R R R R R R R R R);
requester_refs!(R R R R R R R R R R R);
requester_refs!(R R R R R R R R R R R R);

impl FocusRequester {
    /// Says a focus property leaves the direction to the search, as `FocusRequester.Default`.
    pub const DEFAULT: FocusRequester = FocusRequester::constant(0);
    /// Says a focus property doesn't move the focus, as `FocusRequester.Cancel`.
    pub const CANCEL: FocusRequester = FocusRequester::constant(1);

    const fn constant(id: u64) -> FocusRequester {
        FocusRequester { id, _thread: std::marker::PhantomData }
    }

    pub fn new() -> FocusRequester {
        let id = NEXT_ID.with(|next| next.replace(next.get() + 1));
        FocusRequester { id, _thread: std::marker::PhantomData }
    }

    /// New requesters, as `FocusRequester.createRefs()`'s destructuring:
    /// `let (first, second) = FocusRequester::create_refs();`.
    pub fn create_refs<T: FocusRequesterRefs>() -> T {
        T::make()
    }

    /// Asks its node's target to take the focus, as `requestFocus()` (`FocusDirection::Enter`).
    /// Attached to several, each is asked in the order they were attached, so the last one keeps
    /// it. Returns whether one took it (one whose callbacks moved it elsewhere didn't), `false`
    /// if it's attached to none.
    pub fn request_focus(&self) -> bool {
        self.request_focus_with(FocusDirection::Enter)
    }

    /// `request_focus` in `direction`, as `requestFocus(focusDirection)`: what the target's
    /// `on_enter`s and the child it passes the request to go by.
    pub fn request_focus_with(&self, direction: FocusDirection) -> bool {
        self.perform(RequesterOperation::Request(direction))
    }

    /// Asks the node's target, having the focus, to capture it, as `captureFocus`: it refuses
    /// requests elsewhere and clears that aren't forced until `free_focus`. Stops at the first of
    /// its nodes that does. Returns whether one did.
    pub fn capture_focus(&self) -> bool {
        self.perform(RequesterOperation::Capture)
    }

    /// Asks the node's target to free the focus it captured, as `freeFocus`. Returns whether it's
    /// free now on the first of its nodes that has it.
    pub fn free_focus(&self) -> bool {
        self.perform(RequesterOperation::Free)
    }

    /// Saves the child of the node's target that has the focus, as `saveFocusedChild`.
    pub fn save_focused_child(&self) -> bool {
        self.perform(RequesterOperation::Save)
    }

    /// Gives the focus back to the child saved, as `restoreFocusedChild`.
    pub fn restore_focused_child(&self) -> bool {
        self.perform(RequesterOperation::Restore)
    }

    fn check_is_requester(&self) {
        assert!(
            *self != FocusRequester::DEFAULT && *self != FocusRequester::CANCEL,
            "compose-ui: check whether a requester is FocusRequester::CANCEL or DEFAULT before \
             asking it anything"
        );
    }

    fn attachments(&self) -> SmallVec<[Attachment; 1]> {
        ATTACHED.with(|attached| attached.borrow().get(&self.id).cloned().unwrap_or_default())
    }

    fn perform(&self, operation: RequesterOperation) -> bool {
        self.check_is_requester();
        let attachments = self.attachments();
        let Some(&(key, ..)) = attachments.first() else {
            log::warn!(
                "compose-ui: a FocusRequester that isn't attached: did you forget a \
                 Modifier.focus_requester, or to remember it?"
            );
            return false;
        };
        let requester = *self;
        let performed = on_free_tree(key, |tree| {
            let done = requester.perform_on_tree(tree, operation);
            (done, tree.active_of_each_window())
        });
        match performed {
            Some((done, focused)) => {
                // Its callbacks ran as it returned: one that moved the focus took the request's
                // success, as it stops a Compose transaction.
                if done && matches!(operation, RequesterOperation::Request(_)) {
                    let now = read_free_tree(key, |tree| tree.active_of_each_window());
                    // The window a callback opened has nothing to say of this request.
                    return now.is_none_or(|now| {
                        focused.iter().all(|(window, active)| {
                            now.iter()
                                .find(|(other, _)| other == window)
                                .is_none_or(|(_, now)| now == active)
                        })
                    });
                }
                done
            }
            None => {
                if let Some(requests) = requests_of(key) {
                    requests.push(FocusRequest::Requester(requester, operation));
                }
                true
            }
        }
    }

    /// `operation` on this requester's nodes, as `findFocusTarget` and the others.
    pub(crate) fn perform_on_tree(&self, tree: &mut UiTree, operation: RequesterOperation) -> bool {
        let key = tree.key;
        let attachments: SmallVec<[Attachment; 1]> =
            self.attachments().into_iter().filter(|attachment| attachment.0 == key).collect();
        match operation {
            RequesterOperation::Request(direction) => {
                self.check_is_requester();
                self.find_focus_target_on_tree(tree, &mut |tree, target| {
                    tree.request_focus_target(target, direction)
                })
            }
            RequesterOperation::Capture | RequesterOperation::Free | RequesterOperation::Save => {
                attachments
                    .into_iter()
                    .any(|(_, node, entry)| tree.perform_on_entry(node, entry, operation))
            }
            RequesterOperation::Restore => {
                let mut success = false;
                for (_, node, entry) in attachments {
                    success |= tree.perform_on_entry(node, entry, operation);
                }
                success
            }
        }
    }

    /// As `findFocusTarget`: for each node attached, the first target below it that `on_found`
    /// takes. `true` if any did.
    pub(crate) fn find_focus_target_on_tree(
        &self,
        tree: &mut UiTree,
        on_found: OnFound<'_>,
    ) -> bool {
        let key = tree.key;
        let mut success = false;
        for (at, node, entry) in self.attachments() {
            if at != key {
                continue;
            }
            let Some(index) = tree.entry_index(node, entry) else { continue };
            for target in tree.targets_below_place(node, index + 1) {
                if on_found(tree, target) {
                    success = true;
                    break;
                }
            }
        }
        success
    }

    fn attach(&self, key: u32, node: NodeId, entry: EntryId) {
        ATTACHED.with(|attached| {
            let mut attached = attached.borrow_mut();
            let attachments = attached.entry(self.id).or_default();
            if !attachments.contains(&(key, node, entry)) {
                attachments.push((key, node, entry));
            }
        });
    }

    fn detach(&self, key: u32, node: NodeId, entry: EntryId) {
        // Not at all once the thread's registry is gone (a tree dropped as the thread exits).
        let _ = ATTACHED.try_with(|attached| {
            let mut attached = attached.borrow_mut();
            if let Some(attachments) = attached.get_mut(&self.id) {
                attachments.retain(|each| *each != (key, node, entry));
                if attachments.is_empty() {
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
    /// focuses the first focus target after it in the chain or below it (the `focusable`, text
    /// field or clickable after it).
    pub fn focus_requester(self, requester: FocusRequester) -> Modifier {
        self.then_custom(FocusRequesterElement(requester))
    }
}

struct FocusRequesterElement(FocusRequester);

pub(crate) struct FocusRequesterNode {
    requester: FocusRequester,
    /// The tree, the layout node and the entry it's on, while attached.
    owner: Option<Attachment>,
}

impl FocusRequesterNode {
    fn leave(&mut self) {
        if let Some((key, at, entry)) = self.owner.take() {
            self.requester.detach(key, at, entry);
        }
    }
}

impl ModifierNodeElement for FocusRequesterElement {
    type Node = FocusRequesterNode;
    fn create(&self) -> FocusRequesterNode {
        FocusRequesterNode { requester: self.0, owner: None }
    }
    fn update(&self, node: &mut FocusRequesterNode) {
        if !node.requester.same(&self.0) {
            if let Some((key, at, entry)) = node.owner {
                node.requester.detach(key, at, entry);
                self.0.attach(key, at, entry);
            }
            node.requester = self.0;
        }
    }
    fn same(&self, other: &Self) -> bool {
        other.0.same(&self.0)
    }
    fn name(&self) -> &'static str {
        "focus_requester"
    }
}

impl ModifierNode for FocusRequesterNode {
    fn as_focus_requester(&mut self) -> Option<&mut dyn FocusRequesterModifierNode> {
        Some(self)
    }
    /// A requester reaches what's attached: not a node that isn't composed, nor one kept for
    /// reuse, as Compose's detached nodes.
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let owner = (context.tree.key, context.node, context.entry);
        self.requester.attach(owner.0, owner.1, owner.2);
        self.owner = Some(owner);
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.leave();
    }
    fn on_release(&mut self, _context: &mut crate::modifier::ReleaseContext<'_>) {
        self.leave();
    }
}

impl FocusRequesterModifierNode for FocusRequesterNode {}

impl NodeContext<'_> {
    /// What this node's targets (its own, if it's one, and the first ones below it) are told:
    /// `operation`, now for those below it; if it has a target of its own it's waited for (the
    /// node is out of its chain meanwhile), and the answer is `true`.
    fn focus_requester_operation(&mut self, operation: RequesterOperation) -> bool {
        let own_target = self.index.is_some_and(|index| {
            self.tree.node(self.node).chain[index].kinds.any(NodeKinds::FOCUS_TARGET)
        });
        if own_target {
            self.tree.focus_requests.push(FocusRequest::Entry(self.node, self.entry, operation));
            return true;
        }
        self.tree.perform_on_entry(self.node, self.entry, operation)
    }

    /// As `FocusRequesterModifierNode.requestFocus()`: the first target at or below this node is
    /// asked, at once, nested in what's running. Application callbacks run after the node's code,
    /// as all. A node with a target of its own: a handle's `request_focus`, from outside its code,
    /// has the real result.
    pub fn request_focus(&mut self, direction: FocusDirection) -> bool {
        self.focus_requester_operation(RequesterOperation::Request(direction))
    }

    /// As `FocusRequesterModifierNode.captureFocus`.
    pub fn capture_focus(&mut self) -> bool {
        self.focus_requester_operation(RequesterOperation::Capture)
    }

    /// As `FocusRequesterModifierNode.freeFocus`.
    pub fn free_focus(&mut self) -> bool {
        self.focus_requester_operation(RequesterOperation::Free)
    }

    /// As `FocusRequesterModifierNode.saveFocusedChild`.
    pub fn save_focused_child(&mut self) -> bool {
        self.focus_requester_operation(RequesterOperation::Save)
    }

    /// As `FocusRequesterModifierNode.restoreFocusedChild`.
    pub fn restore_focused_child(&mut self) -> bool {
        self.focus_requester_operation(RequesterOperation::Restore)
    }

    /// The node's focus properties changed, as `invalidateFocusProperties` (and a target's
    /// focusability): the focused target is asked again if it can keep the focus, when the
    /// pass ends, or at the next frame from a callback outside one.
    pub fn invalidate_focus_properties(&mut self) {
        if self.tree.a_window_has_active(|_| true) {
            self.tree.focus.recheck_active = true;
            self.tree.focus_requests.wake();
        }
    }

    /// The state of the target this context is of (a chain entry or a delegate), as
    /// `focusState`; `Inactive` if it's none.
    pub fn focus_state(&mut self) -> FocusState {
        self.tree.focus_state_at_entry(self.node, self.entry)
    }
}

impl NodeHandle {
    fn focus_operation(&self, operation: RequesterOperation) -> bool {
        let (node, entry) = self.place();
        on_free_tree(self.tree_key(), |tree| tree.perform_on_entry(node, entry, operation))
            .unwrap_or_else(|| {
                if let Some(requests) = requests_of(self.tree_key()) {
                    requests.push(FocusRequest::Entry(node, entry, operation));
                }
                true
            })
    }

    /// As `FocusTargetModifierNode.requestFocus(direction)` and
    /// `FocusRequesterModifierNode.requestFocus()`: the first target at or below this node is
    /// asked, and what it calls runs. Where the tree is busy it waits, and answers `true`.
    pub fn request_focus(&self, direction: FocusDirection) -> bool {
        self.focus_operation(RequesterOperation::Request(direction))
    }

    /// As `captureFocus`.
    pub fn capture_focus(&self) -> bool {
        self.focus_operation(RequesterOperation::Capture)
    }

    /// As `freeFocus`.
    pub fn free_focus(&self) -> bool {
        self.focus_operation(RequesterOperation::Free)
    }

    /// As `saveFocusedChild`.
    pub fn save_focused_child(&self) -> bool {
        self.focus_operation(RequesterOperation::Save)
    }

    /// As `restoreFocusedChild`.
    pub fn restore_focused_child(&self) -> bool {
        self.focus_operation(RequesterOperation::Restore)
    }

    /// As `FocusTargetModifierNode.focusState`: of the node's own target, the first if it has
    /// several delegates. `Inactive` if it's gone, has none, or the tree is busy.
    pub fn focus_state(&self) -> FocusState {
        let (node, entry) = self.place();
        read_free_tree(self.tree_key(), |tree| tree.focus_state_at_entry(node, entry))
            .unwrap_or(FocusState::Inactive)
    }

    /// As `FocusTargetModifierNode.getFocusedRect()`: the focused rectangle in this target's
    /// coordinates, if it or a target below it has the focus.
    pub fn focused_rect(&self) -> Option<Rect> {
        let (node, entry) = self.place();
        read_free_tree(self.tree_key(), |tree| {
            let target = tree.own_target_at_entry(node, entry);
            tree.focused_rect_of(target)
        })
        .flatten()
    }

    /// The target's own rectangle in its coordinates, as `fetchFocusRect()`: what its focus
    /// properties say, else its bounds. For tests.
    #[doc(hidden)]
    pub fn fetch_focus_rect(&self) -> Rect {
        let (node, entry) = self.place();
        read_free_tree(self.tree_key(), |tree| {
            let target = tree.own_target_at_entry(node, entry)?;
            let rect = tree.focus_rect_of(target);
            let origin = tree.origin_of_entry(node, entry);
            Some(Rect::new(
                rect.left - origin.x,
                rect.top - origin.y,
                rect.right - origin.x,
                rect.bottom - origin.y,
            ))
        })
        .flatten()
        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
    }

    /// As `invalidateFocusProperties` and `focusability`'s setter, from outside the node's code.
    pub fn invalidate_focus_properties(&self) {
        let _ = on_free_tree(self.tree_key(), |tree| {
            if tree.a_window_has_active(|_| true) {
                tree.focus.recheck_active = true;
                tree.flush_focus_invalidations();
            }
        });
    }

    /// As `requestFocusForChildInRootBounds`.
    pub fn request_focus_for_child_in_root_bounds(
        &self,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    ) -> bool {
        let (node, _) = self.place();
        on_free_tree(self.tree_key(), |tree| {
            tree.request_focus_for_child_in_bounds(node, left, top, right, bottom)
        })
        .unwrap_or(false)
    }

    /// As `requestFocusForChildInLocalBounds`.
    pub fn request_focus_for_child_in_local_bounds(
        &self,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    ) -> bool {
        let (node, entry) = self.place();
        on_free_tree(self.tree_key(), |tree| {
            let origin = tree.origin_of_entry(node, entry);
            let (x, y) = (origin.x.round() as i32, origin.y.round() as i32);
            tree.request_focus_for_child_in_bounds(node, x + left, y + top, x + right, y + bottom)
        })
        .unwrap_or(false)
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
        let node = NodeId::from_bits(7);
        assert!(!requester.request_focus());
        requester.attach(1, node, EntryId(1));
        assert_eq!(requester.attachments().as_slice(), [(1, node, EntryId(1))]);
        requester.detach(1, node, EntryId(1));
        assert!(ATTACHED.with(|attached| attached.borrow().is_empty()));
        assert!(!requester.request_focus());
        assert_ne!(FocusRequester::new(), requester);
    }
}
