//! What a modifier node reaches of its tree, as Compose's `DelegatableNode` extensions: a
//! `NodeContext` while its code runs (its lifecycle callbacks), a `NodeHandle` afterwards (a
//! task, a callback, a stored closure). docs/parity/design/modifier-nodes.md §3.3, docs/06-ui.md
//! "Modifiers".

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

use crate::alignment::LayoutDirection;
use crate::coordinates::LayoutCoordinates;
use crate::focus_requester::FocusRequests;
use crate::modifier::ModifierNode;
use crate::node_chain::{EntryId, entry_state};
use crate::tree::{NodeId, UiTree};
use crate::unit::Density;

/// What a node asks its tree to do again, as the `invalidate*` functions of Compose's node
/// interfaces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Invalidation {
    /// `DrawModifierNode.invalidateDraw`: what the node draws in draws again.
    Draw,
    /// `LayoutModifierNode.invalidateMeasurement`.
    Measurement,
    /// `LayoutModifierNode.invalidatePlacement`: its blocks run; a node with none is measured.
    Placement,
    /// `invalidateRulers`.
    Rulers,
    /// `LayoutModifierNode.invalidateLayer`: the graphics layer's properties.
    Layer,
    /// `SemanticsModifierNode.invalidateSemantics`.
    Semantics,
    /// `ParentDataModifierNode.invalidateParentData`.
    ParentData,
    /// `DelegatableNode.invalidateSubtree`.
    Subtree,
    /// `DelegatableNode.invalidateMeasurementForSubtree`.
    MeasurementForSubtree,
    /// `DelegatableNode.invalidateDrawForSubtree`.
    DrawForSubtree,
}

/// What a node's callback reaches: its own place in the tree and the tree's services, as
/// Compose's `DelegatableNode` and its extension functions. Handed to `ModifierNode::on_attach`,
/// `on_detach`, `on_reset`, `on_density_change` and `on_layout_direction_change`; the node runs
/// out of its chain meanwhile, so through it the node reaches the rest of the tree, never
/// itself.
pub struct NodeContext<'a> {
    pub(crate) tree: crate::tree_access::TreeAccess<'a>,
    pub(crate) node: NodeId,
    /// The entry's index in its chain while the callback runs; `None` for an entry out of its
    /// chain (removed by a chain change, detached before it's released).
    pub(crate) index: Option<usize>,
    pub(crate) entry: EntryId,
    /// The `entry_state` flags of an entry out of its chain, or of a delegate.
    pub(crate) out_state: u8,
    /// It's a delegate of entry `index`, named `entry` (its own id), its flags in `out_state`.
    pub(crate) delegated: bool,
    /// An entry out of its chain: where it was, for the traversals from it.
    pub(crate) place: usize,
    /// Its key, if it's a `TraversableNode`: what the typed traversals look for.
    pub(crate) traverse_key: Option<&'static crate::traversal::TraverseKey>,
}

impl NodeContext<'_> {
    /// The coordinates of the layout the node is in, as `requireLayoutCoordinates`: the nearest
    /// layout modifier's after it, or the layout node's own.
    ///
    /// # Panics
    ///
    /// If the node isn't attached, as Compose's.
    pub fn require_layout_coordinates(&self) -> LayoutCoordinates {
        assert!(self.is_attached(), "compose-ui: the node isn't attached");
        // In its `on_detach`, out of its chain: where it was.
        self.tree.coordinates_at(self.node, self.index.unwrap_or(self.place))
    }

    fn state(&self) -> u8 {
        match self.index {
            Some(index) if !self.delegated => self.tree.node(self.node).chain[index].state,
            _ => self.out_state,
        }
    }

    fn add_state(&mut self, flag: u8) {
        match self.index {
            Some(index) if !self.delegated => {
                self.tree.enter().node_mut(self.node).chain[index].state |= flag;
            }
            _ => self.out_state |= flag,
        }
    }

    /// This context again, for as long as the borrow: what wraps it (`EventContext`) owns one.
    /// Its `out_state` is copied back by who asks.
    pub(crate) fn reborrow(&mut self) -> NodeContext<'_> {
        NodeContext {
            tree: self.tree.reborrow(),
            node: self.node,
            index: self.index,
            entry: self.entry,
            out_state: self.out_state,
            delegated: self.delegated,
            place: self.place,
            traverse_key: self.traverse_key,
        }
    }

    /// Runs `f` with the delegate `entry` and its context: this context's place in the tree (a
    /// delegate draws and measures where its owner does, as Compose's coordinator), its own
    /// lifecycle flags and identity.
    pub(crate) fn with_delegate<R>(
        &mut self,
        entry: &mut crate::delegation::DelegateEntry,
        f: impl FnOnce(&mut dyn ModifierNode, &mut NodeContext<'_>) -> R,
    ) -> R {
        let traverse_key = entry.traverse_key();
        let mut context = NodeContext {
            tree: self.tree.reborrow(),
            node: self.node,
            index: self.index,
            entry: entry.id(),
            out_state: entry.state(),
            delegated: true,
            place: self.place,
            traverse_key,
        };
        let result = f(entry.node_mut(), &mut context);
        let state = context.out_state;
        entry.set_state(state);
        result
    }

    /// Its chain entry's kinds as they are now. For tests.
    #[doc(hidden)]
    pub fn chain_kinds_for_test(&self) -> crate::node_chain::NodeKinds {
        match self.index {
            Some(index) => self.tree.node(self.node).chain[index].kinds,
            None => crate::node_chain::NodeKinds::NONE,
        }
    }

    /// A handle naming this node from outside its callbacks.
    pub fn handle(&mut self) -> NodeHandle {
        let attached = self.is_attached();
        if self.state() & entry_state::HAS_HANDLE == 0 {
            self.add_state(entry_state::HAS_HANDLE);
            if attached {
                self.tree.focus_requests.attached_entries.borrow_mut().insert(self.entry.0);
            }
        }
        // A delegate's chain entry is its owner's: where it is in the chain, for what's asked of it
        // with nothing but a shared look at the tree.
        let owner = match self.index {
            Some(index) => self.tree.node(self.node).chain[index].id,
            None => self.entry,
        };
        NodeHandle {
            tree: self.tree.key,
            node: self.node,
            entry: self.entry,
            owner,
            _not_send: PhantomData,
        }
    }

    /// A handle naming this node, not registered (nothing to do on its attach or detach): it
    /// reaches the node with `NodeHandle::with_node` and the nested-scroll chain above it
    /// (`NestedScrollChain::above`), but says it's never attached, has no coroutine scope and
    /// asks for nothing. For a node to keep at no cost where it may never use it.
    #[doc(hidden)]
    pub fn unregistered_handle(&self) -> NodeHandle {
        let owner = match self.index {
            Some(index) => self.tree.node(self.node).chain[index].id,
            None => self.entry,
        };
        NodeHandle {
            tree: self.tree.key,
            node: self.node,
            entry: self.entry,
            owner,
            _not_send: PhantomData,
        }
    }

    /// The layout node the modifier node is on.
    pub fn layout_node(&self) -> NodeId {
        self.node
    }

    /// The size at its place in the chain, inside the layout modifiers before it, as last measured:
    /// what a pointer event's bounds are, as Compose's `onRemeasured` tells a node there.
    #[doc(hidden)]
    pub fn measured_size(&self) -> crate::IntSize {
        let value = self.tree.node(self.node);
        let layer = self.index.map_or(0, |index| value.layer_of(index));
        value.layers.get(layer).map_or(crate::IntSize::ZERO, |layer| layer.measured)
    }

    /// Whether the node is attached, as `Modifier.Node.isAttached`: its layout node is in the
    /// tree and active (not content kept for reuse).
    pub fn is_attached(&self) -> bool {
        self.state() & entry_state::ATTACHED != 0
    }

    /// The node's coroutine scope, as `Modifier.Node.coroutineScope`: made on first use,
    /// cancelled when the node is detached (a node attached again gets a new one). Its tasks run
    /// between frames or in a frame's task step, when the tree is free.
    ///
    /// Panics if the node isn't attached, as Compose's (`requireOwner`).
    pub fn coroutine_scope(&mut self) -> compose_runtime::CoroutineScope {
        assert!(self.is_attached(), "compose-ui: a node's coroutine scope while it isn't attached");
        self.add_state(entry_state::HAS_SCOPE);
        **self
            .tree
            .focus_requests
            .scopes
            .borrow_mut()
            .entry(self.entry)
            .or_insert_with(compose_runtime::CoroutineScope::new)
    }

    /// Runs `effect` once the tree applied the changes under way, as `Modifier.Node.sideEffect`
    /// (`onEndApplyChanges`): after the pass's lifecycle callbacks, as soon as the host has
    /// released the tree after the composition pass, before the frame's layout, so the effect
    /// may write state the layout reads and use handles (`NodeHandle::with_node`). Asked
    /// outside a pass (from a task), it asks for a frame, which runs it.
    pub fn side_effect(&mut self, effect: impl FnOnce() + 'static) {
        self.tree.enter().side_effects.push(Box::new(effect));
        self.tree.focus_requests.wake();
    }

    /// Runs `action` after the layout under way, outside the tree: what a `Modifier` lambda
    /// told of a layout goes through (D172).
    pub fn run_after_layout(&mut self, action: impl FnOnce() + 'static) {
        self.tree.enter().layout_callbacks.push(crate::input::Action::Run(Box::new(action)));
    }

    /// The density where its layout node is composed, as `requireDensity()`.
    pub fn require_density(&self) -> Density {
        self.tree.node(self.node).density
    }

    /// The value of `local` where its layout node is composed, as
    /// `CompositionLocalConsumerModifierNode.currentValueOf`. Reads where its node was composed,
    /// not where its element was made. A dynamic local's read is observed as any state's (in
    /// `observe_reads`, measuring, drawing). Any node may read: `is_composition_local_consumer`
    /// is for being told when what it reads from changes.
    ///
    /// Panics if the node isn't attached, as Compose's.
    pub fn current_value_of<T: Clone + 'static>(
        &self,
        local: &compose_runtime::CompositionLocal<T>,
    ) -> T {
        assert!(
            self.is_attached(),
            "compose-ui: reading a composition local while the node isn't attached"
        );
        self.composition_locals().get(local)
    }

    /// Where its layout node reads composition locals, for `observe_reads` (whose block has no
    /// context): `context.observe_reads(|| locals.get(&LOCAL))`.
    pub fn composition_locals(&self) -> compose_runtime::CompositionLocalAnchor {
        self.tree.node(self.node).locals
    }

    /// The input mode of the node's host, as `currentValueOf(LocalInputModeManager).inputMode`
    /// (a read of its state). `LOCAL_INPUT_MODE_MANAGER`'s default is the host's only while
    /// composing: read outside composition, it would be a manager of its own.
    pub fn input_mode(&self) -> crate::input_mode::InputMode {
        self.tree.input_mode.input_mode()
    }

    /// The tree's clipboard (`UiTree::set_clipboard`), as text fields use it.
    pub fn clipboard(&self) -> std::rc::Rc<dyn crate::clipboard::ClipboardManager> {
        self.tree.clipboard.clone()
    }

    /// Whether the platform has autofill (`UiTree::is_autofill_available`), as Compose's
    /// `isAutofillAvailable`: a text field's menus offer it then.
    pub fn is_autofill_available(&self) -> bool {
        self.tree.is_autofill_available()
    }

    /// The host's floating text toolbars (`UiTree::set_text_action_modes`), if it has any.
    pub fn text_action_modes(
        &self,
    ) -> Option<std::rc::Rc<dyn crate::text_toolbar::TextActionModes>> {
        self.tree.text_action_modes.clone()
    }

    /// The layout node's layout direction, as `requireLayoutDirection()`.
    pub fn require_layout_direction(&self) -> LayoutDirection {
        self.tree.node(self.node).layout_direction
    }

    /// The text measurer, for a node that lays text out or gives paragraphs back. Borrowed until
    /// the guard goes: don't hold it through a [`TextMeasurer::measure`](crate::TextMeasurer) or a
    /// result's query, which borrow it too and panic.
    pub fn text_measurer(&mut self) -> std::cell::RefMut<'_, dyn crate::text::TextMeasurer> {
        self.tree.text_measurer.borrow_mut()
    }

    fn invalidate(&mut self, what: Invalidation) {
        // A detached node's invalidations do nothing. Compose's `invalidateDraw` and the
        // `…ForSubtree` ones too; its others fail for want of a coordinator (D1178).
        if !self.is_attached() {
            return;
        }
        // The parent data's fold asks every node of the chain, this one too (and an outer
        // layout node measuring, inside a measurement): where the tree applies its nodes'
        // requests, with every node in its chain, as a handle's.
        if what == Invalidation::ParentData && self.index.is_some() {
            let requests = &self.tree.focus_requests;
            requests.node_invalidations.borrow_mut().push((self.node, self.entry, what));
            requests.wake();
            return;
        }
        let (node, index) = (self.node, self.index);
        self.tree.enter().invalidate_entry_at(node, index, what);
        // From a task's `NodeHandle::with_node` (which Android's tick runs between frames): it
        // asks for a frame, as a handle's own requests do (D2025). Not from a frame's own steps
        // (its nodes' requests applied, its observers told), which that frame lays out and draws.
        if self.tree.borrowed_by_handle {
            self.tree.focus_requests.wake();
        }
    }

    /// Draws again what the node draws in, as `DrawModifierNode.invalidateDraw`.
    pub fn invalidate_draw(&mut self) {
        self.invalidate(Invalidation::Draw);
    }

    /// Measures the node's layout node again, as `invalidateMeasurement`.
    pub fn invalidate_measurement(&mut self) {
        self.invalidate(Invalidation::Measurement);
    }

    /// Works out again the rulers its layout node provides, as `invalidateRulers`: who read one
    /// that changed places again.
    pub fn invalidate_rulers(&mut self) {
        self.invalidate(Invalidation::Rulers);
    }

    /// Places the node's layout node again, as `invalidatePlacement`: its placement blocks run,
    /// and a layout placing inside its measurement is measured.
    pub fn invalidate_placement(&mut self) {
        self.invalidate(Invalidation::Placement);
    }

    /// Updates the properties of the graphics layer the node is in, as `invalidateLayer`.
    pub fn invalidate_layer(&mut self) {
        self.invalidate(Invalidation::Layer);
    }

    /// The node's semantics changed, as `invalidateSemantics`.
    pub fn invalidate_semantics(&mut self) {
        self.invalidate(Invalidation::Semantics);
    }

    /// Asks the platform's autofill to fill the node's layout node, as
    /// `DelegatableNode.requestAutofill`: told once the tree is free; nothing without autofill or
    /// while the node isn't placed.
    pub fn request_autofill(&mut self) {
        if !self.is_attached() {
            return;
        }
        let node = self.node;
        self.tree.enter().request_autofill_of(node);
    }

    /// The node's parent data changed, as `invalidateParentData`.
    pub fn invalidate_parent_data(&mut self) {
        self.invalidate(Invalidation::ParentData);
    }

    /// Measures, places and draws the layout node and everything below it again, as
    /// `invalidateSubtree`.
    pub fn invalidate_subtree(&mut self) {
        self.invalidate(Invalidation::Subtree);
    }

    /// Measures the layout node and everything below it again, as
    /// `invalidateMeasurementForSubtree`.
    pub fn invalidate_measurement_for_subtree(&mut self) {
        self.invalidate(Invalidation::MeasurementForSubtree);
    }

    /// Draws the layout node and everything below it again, as `invalidateDrawForSubtree`.
    pub fn invalidate_draw_for_subtree(&mut self) {
        self.invalidate(Invalidation::DrawForSubtree);
    }

    /// Measures the layout node now, with its last constraints, even if nothing asked, as
    /// `LayoutModifierNode.remeasureSync` (Compose's `forceRemeasure`): from a callback where the
    /// tree is free (a task's `NodeHandle::with_node`, a lifecycle callback at a frame's start),
    /// once the callback returns, the node back in its chain (the measurement may run it). Where
    /// the tree is in a pass (a composition's end, a measurement), it's measured in the layout
    /// that follows instead (`invalidate_measurement`). Nothing for a detached node.
    pub fn remeasure_sync(&mut self) {
        if !self.is_attached() {
            return;
        }
        if !self.tree.remeasure_allowed {
            self.invalidate(Invalidation::Measurement);
        } else if self.index.is_none() {
            // Out of its chain: nothing of the measurement is it.
            // It runs measuring code, under a reference to the tree.
            let node = self.node;
            self.tree.enter().remeasure_now(node);
        } else if !self.tree.remeasure_after.contains(&self.node) {
            let node = self.node;
            self.tree.enter().remeasure_after.push(node);
        }
    }

    /// Runs `block`, observing the state it reads, as `observeReads`: once a write changes one
    /// of them, the node's `ObserverModifierNode::on_observed_reads_changed` is called (if it
    /// answers `as_observer`), once; observing again replaces what the last observation read.
    /// Works detached too: the call comes if the node is attached when the write is told.
    pub fn observe_reads<R>(&mut self, block: impl FnOnce() -> R) -> R {
        self.add_state(entry_state::OBSERVES);
        let requests = self.tree.focus_requests.clone();
        requests.observe_entry_reads(self.node, self.entry, block)
    }

    /// The layout node's `Remeasurement`, as `RemeasurementModifier.onRemeasurementAvailable`
    /// gives it: what measures it on demand.
    pub fn remeasurement(&self) -> crate::remeasurement::Remeasurement {
        crate::remeasurement::Remeasurement::new(self.tree.key, self.node)
    }
}

/// A modifier node, named from outside its callbacks (a task, a callback, a stored closure):
/// `Copy`, `'static`, `!Send`, and never stale: a node gone answers `false` or `None`, and what
/// it asks is dropped. Its invalidations are queued and applied where the tree applies its
/// nodes' requests (a frame's start, the end of a composition pass, each layout round), so one
/// asked before a frame's layout is in that frame.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeHandle {
    tree: u32,
    node: NodeId,
    entry: EntryId,
    /// The chain entry it's in: `entry`, or the one whose delegate it is.
    owner: EntryId,
    _not_send: PhantomData<*const ()>,
}

/// A tree's entry in the thread's registry: its requests, and how a handle borrows it.
struct Registration {
    requests: Weak<FocusRequests>,
    /// Runs a function with the tree if it's free; set by its host.
    borrow: Option<Rc<TreeBorrow>>,
    /// The same to read it, which a composition running doesn't stop.
    peek: Option<Rc<TreeBorrow>>,
}

type TreeBorrow = dyn Fn(&mut dyn FnMut(&mut UiTree)) -> bool;

thread_local! {
    // FNV, not SipHash: looked up for each request a handle makes (an indication's each
    // interaction), by a counter no one chooses.
    static TREES: RefCell<HashMap<u32, Registration, BuildHasherDefault<compose_runtime::KeyHasher>>> =
        RefCell::new(HashMap::default());
}

/// Registers a new tree, whose requests are `requests`: its key.
pub(crate) fn register_tree(requests: &Rc<FocusRequests>) -> u32 {
    // Across threads: a handle read on another thread names no tree there.
    static NEXT_TREE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let key = NEXT_TREE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    requests.tree_key.set(key);
    let registration = Registration { requests: Rc::downgrade(requests), borrow: None, peek: None };
    TREES.with(|trees| trees.borrow_mut().insert(key, registration));
    key
}

/// Forgets a tree being dropped.
pub(crate) fn unregister_tree(key: u32) {
    // Not at all once the thread's registry is gone (a tree dropped as the thread exits).
    let _ = TREES.try_with(|trees| trees.borrow_mut().remove(&key));
}

/// Sets how handles borrow the tree of `key` (its host's composition holds it).
pub(crate) fn set_tree_borrow(key: u32, borrow: Rc<TreeBorrow>, peek: Rc<TreeBorrow>) {
    TREES.with(|trees| {
        if let Some(registration) = trees.borrow_mut().get_mut(&key) {
            registration.borrow = Some(borrow);
            registration.peek = Some(peek);
        }
    });
}

pub(crate) fn requests_of(key: u32) -> Option<Rc<FocusRequests>> {
    // None once the thread's registry is gone (a query in a `Drop` as the thread exits).
    TREES.try_with(|trees| trees.borrow().get(&key)?.requests.upgrade()).ok().flatten()
}

thread_local! {
    /// The trees whose `on_enter` or `on_exit` block runs: where `with_free_tree` finds them.
    static TREE_SET_ASIDE: std::cell::RefCell<Vec<UiTree>> = const { std::cell::RefCell::new(Vec::new()) };
    /// What stands in for a tree set aside, kept for the next block.
    static STAND_IN: std::cell::RefCell<Option<UiTree>> = const { std::cell::RefCell::new(None) };
}

/// Whether the tree of `key` is set aside for a block: what's done on it is inside a transaction.
pub(crate) fn tree_is_set_aside(key: u32) -> bool {
    TREE_SET_ASIDE.with(|slot| slot.borrow().iter().any(|tree| tree.key == key))
}

/// Runs `block` (an `on_enter` or `on_exit` block, application code) with `tree` where
/// `with_free_tree` finds it, as Compose's blocks run with requests acting at once (D1270).
pub(crate) fn with_tree_set_aside<R>(tree: &mut UiTree, block: impl FnOnce() -> R) -> R {
    let stand_in = STAND_IN.with(|stand_in| stand_in.borrow_mut().take()).unwrap_or_else(|| {
        UiTree::new(
            crate::unit::Density::new(1.0),
            Box::new(crate::fixed_text::FixedTextMeasurer::new()),
        )
    });
    let real = std::mem::replace(tree, stand_in);
    TREE_SET_ASIDE.with(|slot| slot.borrow_mut().push(real));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(block));
    let real = TREE_SET_ASIDE
        .with(|slot| slot.borrow_mut().pop())
        .expect("compose-ui: the tree set aside for a block is there as it returns");
    let stand_in = std::mem::replace(tree, real);
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(stand_in));
    result.unwrap_or_else(|panic| {
        // Caught above it, the next change finds no transaction under way.
        tree.focus.processing.clear();
        tree.focus.requesting = false;
        tree.focus.moved_by_tree = false;
        std::panic::resume_unwind(panic)
    })
}

/// Runs `f` with the tree of `key` if it's free (its host's composition holds it and nothing
/// borrows it), the tree allowing measurements meanwhile. Returns whether `f` ran.
pub(crate) fn with_free_tree(key: u32, f: &mut dyn FnMut(&mut UiTree)) -> bool {
    let set_aside = TREE_SET_ASIDE.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.last().is_some_and(|tree| tree.key == key) { slot.pop() } else { None }
    });
    if let Some(mut tree) = set_aside {
        let allowed = std::mem::replace(&mut tree.remeasure_allowed, true);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut tree)));
        tree.remeasure_allowed = allowed;
        TREE_SET_ASIDE.with(|slot| slot.borrow_mut().push(tree));
        return result.map(|_| true).unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    }
    let Some(borrow) = TREES.with(|trees| trees.borrow().get(&key)?.borrow.clone()) else {
        return false;
    };
    borrow(&mut |tree: &mut UiTree| {
        let allowed = std::mem::replace(&mut tree.remeasure_allowed, true);
        let borrowed = std::mem::replace(&mut tree.borrowed_by_handle, true);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(tree)));
        tree.remeasure_allowed = allowed;
        tree.borrowed_by_handle = borrowed;
        result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// Runs `read` with the tree of `key` to read it, if nothing borrows it: set aside for a block,
/// or free, a composition under way or not. Returns whether it ran.
pub(crate) fn peek_tree(key: u32, read: &mut dyn FnMut(&mut UiTree)) -> bool {
    let set_aside = TREE_SET_ASIDE
        .try_with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.last().is_some_and(|tree| tree.key == key) { slot.pop() } else { None }
        })
        .ok()
        .flatten();
    if let Some(mut tree) = set_aside {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(&mut tree)));
        TREE_SET_ASIDE.with(|slot| slot.borrow_mut().push(tree));
        return result.map(|_| true).unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    }
    let Some(peek) = TREES.try_with(|trees| trees.borrow().get(&key)?.peek.clone()).ok().flatten()
    else {
        return false;
    };
    peek(read)
}

/// A handle on chain entry `index` of `node`, registered as [`NodeContext::handle`] registers
/// one (it's told when the entry is detached, and its coroutine scope cancelled then).
pub(crate) fn entry_handle(tree: &mut UiTree, node: NodeId, index: usize) -> NodeHandle {
    let key = tree.key;
    let entry = &mut tree.node_mut(node).chain[index];
    let id = entry.id;
    if entry.state & entry_state::HAS_HANDLE == 0 {
        entry.state |= entry_state::HAS_HANDLE;
        if entry.state & entry_state::ATTACHED != 0 {
            tree.focus_requests.attached_entries.borrow_mut().insert(id.0);
        }
    }
    NodeHandle { tree: key, node, entry: id, owner: id, _not_send: PhantomData }
}

impl NodeHandle {
    /// A handle on chain entry `entry` of `node` in tree `tree`, not registered: it's attached
    /// only once [`entry_handle`] registered it.
    pub(crate) fn of_entry(tree: u32, node: NodeId, entry: EntryId) -> NodeHandle {
        NodeHandle { tree, node, entry, owner: entry, _not_send: PhantomData }
    }

    /// The layout node and the entry or delegate it names.
    pub(crate) fn place(&self) -> (NodeId, EntryId) {
        (self.node, self.entry)
    }

    /// The key of its tree.
    pub(crate) fn tree_key(&self) -> u32 {
        self.tree
    }

    /// The coordinates of the layout it's in, as `requireLayoutCoordinates`: the nearest layout
    /// modifier's after it, or the node's own.
    ///
    /// # Panics
    ///
    /// If the node isn't attached, as Compose's.
    pub fn require_layout_coordinates(&self) -> LayoutCoordinates {
        assert!(self.is_attached(), "compose-ui: the node isn't attached");
        let found = LayoutCoordinates::of_entry(self.tree, self.node, self.owner);
        found.expect("compose-ui: its tree is busy or the node has no chain entry")
    }

    /// Whether the node is attached, as `isAttached`.
    pub fn is_attached(&self) -> bool {
        requests_of(self.tree)
            .is_some_and(|requests| requests.attached_entries.borrow().contains(&self.entry.0))
    }

    /// The node's coroutine scope, as [`NodeContext::coroutine_scope`], where the node has no
    /// context (an event handler, an element's update): made on first use, the one its context
    /// gives, cancelled when the node is detached. `None` if the node isn't attached.
    pub fn coroutine_scope(&self) -> Option<compose_runtime::CoroutineScope> {
        let requests = requests_of(self.tree)?;
        if !requests.attached_entries.borrow().contains(&self.entry.0) {
            return None;
        }
        let mut scopes = requests.scopes.borrow_mut();
        Some(**scopes.entry(self.entry).or_insert_with(compose_runtime::CoroutineScope::new))
    }

    fn request(&self, what: Invalidation) {
        let Some(requests) = requests_of(self.tree) else { return };
        if !requests.attached_entries.borrow().contains(&self.entry.0) {
            // Detached: nothing, for every invalidation (some of Compose's fail, D1178).
            return;
        }
        requests.node_invalidations.borrow_mut().push((self.node, self.entry, what));
        // The layout running applies it (D2382).
        if !requests.laying_out.get() {
            requests.wake();
        }
    }

    /// Runs `block`, observing the state it reads, as `observeReads` from where a node has no
    /// context (its measuring, its drawing; as Compose's `BackgroundNode` and appearance nodes
    /// do): a node keeps the handle `NodeContext::handle` gave it in its `on_attach`. As
    /// `NodeContext::observe_reads`: once a write changes one of those reads the node's
    /// `on_observed_reads_changed` is called, once. Its reads count for this observation alone,
    /// not the measurement or drawing around it, as nested `observeReads`.
    pub fn observe_reads<R>(&self, block: impl FnOnce() -> R) -> R {
        match requests_of(self.tree) {
            Some(requests) => requests.observe_entry_reads(self.node, self.entry, block),
            None => block(),
        }
    }

    /// Asks the scrolling and responding nodes around this one (those before it in its chain,
    /// then its ancestors') to show `rect`, in the coordinates of the layout it's in (`None`: all
    /// of it), as `DelegatableNode.bringIntoView`: the future ends once they all have shown it or
    /// were interrupted (a container animates to it). `None` if the node isn't attached and
    /// placed, or its tree is busy (a frame later may do).
    pub fn bring_into_view(
        &self,
        rect: Option<crate::unit::Rect>,
    ) -> Option<crate::nested_scroll::LocalBoxFuture<'static, ()>> {
        if !self.is_attached() {
            return None;
        }
        let mut asked = None;
        let (node, owner) = (self.node, self.owner);
        with_free_tree(self.tree, &mut |tree| {
            let Some(index) = tree.node(node).chain.iter().position(|entry| entry.id == owner)
            else {
                return;
            };
            if tree.is_laid_out(node) {
                asked = Some(tree.bring_into_view_from(node, Some(index), rect).futures);
            }
        });
        asked.map(|futures| {
            Box::pin(crate::relocation::join_all(futures))
                as crate::nested_scroll::LocalBoxFuture<'static, ()>
        })
    }

    /// As `DrawModifierNode.invalidateDraw`.
    pub fn invalidate_draw(&self) {
        self.request(Invalidation::Draw);
    }

    /// As `LayoutModifierNode.invalidateMeasurement`.
    pub fn invalidate_measurement(&self) {
        self.request(Invalidation::Measurement);
    }

    /// As `LayoutModifierNode.invalidatePlacement`.
    pub fn invalidate_placement(&self) {
        self.request(Invalidation::Placement);
    }

    /// As `invalidateRulers`.
    pub fn invalidate_rulers(&self) {
        self.request(Invalidation::Rulers);
    }

    /// As `LayoutModifierNode.invalidateLayer`.
    pub fn invalidate_layer(&self) {
        self.request(Invalidation::Layer);
    }

    /// As `SemanticsModifierNode.invalidateSemantics`.
    pub fn invalidate_semantics(&self) {
        self.request(Invalidation::Semantics);
    }

    /// As `DelegatableNode.requestAutofill` (`NodeContext::request_autofill`), from where a node
    /// has no context: with the tree free, else nothing.
    pub fn request_autofill(&self) {
        let Some(requests) = requests_of(self.tree) else { return };
        if !requests.attached_entries.borrow().contains(&self.entry.0) {
            return;
        }
        let node = self.node;
        let _ = crate::focus_requester::on_free_tree(self.tree, |tree| {
            tree.request_autofill_of(node);
        });
    }

    /// As `ParentDataModifierNode.invalidateParentData`.
    pub fn invalidate_parent_data(&self) {
        self.request(Invalidation::ParentData);
    }

    /// As `invalidateSubtree`.
    pub fn invalidate_subtree(&self) {
        self.request(Invalidation::Subtree);
    }

    /// As `invalidateMeasurementForSubtree`.
    pub fn invalidate_measurement_for_subtree(&self) {
        self.request(Invalidation::MeasurementForSubtree);
    }

    /// As `invalidateDrawForSubtree`.
    pub fn invalidate_draw_for_subtree(&self) {
        self.request(Invalidation::DrawForSubtree);
    }

    /// Runs `effect` once the tree applied the changes under way, as the node's context's
    /// `side_effect` does (after the composition pass, before the frame's layout; asked outside
    /// a pass, at the next frame): for an element's `update`, which runs during the pass without
    /// the node's context, to finish its work with it (`with_node` in `effect`), as Compose's
    /// `update` does at once (D1355). Dropped if the tree is gone.
    pub fn side_effect(&self, effect: impl FnOnce() + 'static) {
        let Some(requests) = requests_of(self.tree) else { return };
        requests.side_effects.borrow_mut().push(Box::new(effect));
        requests.wake();
    }

    /// Runs `f` with the node, as an `N`, and its context, now, if the tree is free (a task
    /// resuming between frames or in a frame's task step, a side effect). `None` if the node is
    /// gone, isn't an `N`, or its tree is busy: inside a composition pass, a node's callback,
    /// an event handler or anything else running while the host holds the tree (there, a
    /// node's own context is at hand; another node is reached by invalidating it, which is
    /// queued).
    pub fn with_node<N: ModifierNode, R>(
        &self,
        f: impl FnOnce(&mut N, &mut NodeContext<'_>) -> R,
    ) -> Option<R> {
        let mut f = Some(f);
        let mut result = None;
        let (node, entry) = (self.node, self.entry);
        with_free_tree(self.tree, &mut |tree: &mut UiTree| {
            let Some(index) = tree.entry_index(node, entry) else { return };
            let Some(f) = f.take() else { return };
            result = tree.with_entry_or_delegate(node, index, entry, |modifier_node, context| {
                modifier_node.downcast_mut::<N>().map(|typed| f(typed, context))
            });
        });
        result
    }

    /// Runs `f` with the node as `N` and an event's context, as a handler of an event outside the
    /// tree's dispatch (a platform toolbar's click): what it asks (focus, the keyboard, its
    /// actions) is done after, as after an event. `None` if the node is gone, isn't an `N`, or
    /// the tree is busy.
    pub fn with_node_in_event<N: ModifierNode, R>(
        &self,
        f: impl FnOnce(&mut N, &mut crate::input::EventContext<'_>) -> R,
    ) -> Option<R> {
        let mut f = Some(f);
        let mut result = None;
        let (node, entry, owner) = (self.node, self.entry, self.owner);
        with_free_tree(self.tree, &mut |tree: &mut UiTree| {
            let Some(index) = tree.entry_index(node, entry) else { return };
            let Some(f) = f.take() else { return };
            let delegate = if entry == owner { EntryId::NO_DELEGATE } else { entry };
            let mut actions = Vec::new();
            let mut requests = crate::input::Requests::default();
            result = tree
                .with_event_context(
                    node,
                    index,
                    delegate,
                    &mut actions,
                    &mut requests,
                    None,
                    false,
                    |modifier_node, context| {
                        modifier_node.downcast_mut::<N>().map(|typed| f(typed, context))
                    },
                )
                .flatten();
            tree.apply_requests(requests, &mut actions);
            tree.pending_actions.append(&mut actions);
        });
        result
    }

    /// What the node's chain entry says of its kinds: its and its delegates', then those of the
    /// entries after it too (a delegate: only its own), as `kindSet` and `aggregateChildKindSet`.
    /// `None` if the node is gone or the tree busy. For tests.
    #[doc(hidden)]
    pub fn kinds_for_test(
        &self,
    ) -> Option<(crate::node_chain::NodeKinds, crate::node_chain::NodeKinds)> {
        let mut result = None;
        let (node, entry) = (self.node, self.entry);
        with_free_tree(self.tree, &mut |tree: &mut UiTree| {
            let Some(index) = tree.entry_index(node, entry) else { return };
            let chain_entry = &mut tree.node_mut(node).chain[index];
            result = Some(match crate::delegation::find_delegate(&mut *chain_entry.node, entry) {
                Some(delegate) => (delegate.kinds(), delegate.kinds()),
                None => (chain_entry.kinds, chain_entry.aggregate),
            });
        });
        result
    }

    /// Which layer of its layout node it measures and draws in, as `requireCoordinator`, and how
    /// many it has. `None` if the node is gone or the tree busy. For tests.
    #[doc(hidden)]
    pub fn layer_for_test(&self) -> Option<(usize, usize)> {
        let mut result = None;
        let (node, entry) = (self.node, self.entry);
        with_free_tree(self.tree, &mut |tree: &mut UiTree| {
            let Some(index) = tree.entry_index(node, entry) else { return };
            let value = tree.node(node);
            result = Some((value.layer_of(index), value.layers.len()));
        });
        result
    }
}
