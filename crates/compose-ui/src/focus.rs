//! Focus, as Compose's (`focus/FocusOwnerImpl.kt`, `FocusTargetNode.kt`, `FocusTransactions.kt`;
//! docs/06-ui.md "Focus", docs/parity/design/focus-and-semantics.md Part A): one active target,
//! every other state derived from it; the targets' tree found by walking the chains by kind;
//! transactions that tell targets and event nodes in Compose's order; invalidations resolved when
//! a pass ends.

use std::cell::Cell;
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;

use compose_runtime::Callback;
use smallvec::SmallVec;

use crate::focus_manager::FocusDirection;
use crate::focus_properties::FocusProperties;
use crate::input::{Action, EventContext, Requests};
use crate::input_mode::InputMode;
use crate::modifier::{FocusTargetModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::node_chain::{ChainEntry, EntryId, NodeKinds, entry_state};
use crate::node_context::NodeContext;
use crate::nodes::{OnFocusChangedNode, Taken};
use crate::tree::{NodeId, Observation, UiTree, observation_key};
use crate::unit::{Offset, Rect};

/// The focus state of a target, or of what an event node covers, as Compose's `FocusState`
/// (Compose's interface and its internal enum are one enum here, D1267).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FocusState {
    /// It has the focus.
    Active,
    /// A target below it has the focus.
    ActiveParent,
    /// It has the focus, and keeps it: other requests and clears that aren't forced are refused.
    Captured,
    /// Neither it nor anything below it has the focus.
    Inactive,
}

impl FocusState {
    /// Whether it has the focus itself, as `isFocused`.
    pub fn is_focused(self) -> bool {
        matches!(self, FocusState::Active | FocusState::Captured)
    }

    /// Whether it or a target below it has the focus, as `hasFocus`.
    pub fn has_focus(self) -> bool {
        !matches!(self, FocusState::Inactive)
    }

    /// Whether it captured the focus, as `isCaptured`.
    pub fn is_captured(self) -> bool {
        matches!(self, FocusState::Captured)
    }
}

/// Whether a target can take the focus, as Compose's `Focusability`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Focusability(u8);

impl Focusability {
    /// It can always take the focus.
    pub const ALWAYS: Focusability = Focusability(1);
    /// It can unless the input mode is touch: what clickable things use.
    pub const SYSTEM_DEFINED: Focusability = Focusability(0);
    /// It can't, though its children can: a focus group.
    pub const NEVER: Focusability = Focusability(2);

    pub(crate) fn can_focus(self, mode: impl FnOnce() -> InputMode) -> bool {
        match self {
            Focusability::ALWAYS => true,
            Focusability::SYSTEM_DEFINED => mode() != InputMode::Touch,
            _ => false,
        }
    }
}

/// A focus target: a chain entry of a layout node, and which of its delegates (`NO_DELEGATE`: its
/// node). Stable across chain diffs.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FocusTargetRef {
    pub(crate) node: NodeId,
    pub(crate) entry: EntryId,
    pub(crate) delegate: EntryId,
}

/// A place in the targets' tree: a target, or the root above them all (`None`, Compose's
/// `rootFocusNode`, which never takes the focus).
pub(crate) type FocusPlace = Option<FocusTargetRef>;

/// What a node of the focus system is told, kept while it runs (a nested transaction reaching a
/// node that is running tells it when its code has returned).
#[derive(Clone, Copy)]
enum FocusCall {
    Changed(FocusState, FocusState),
    Event(FocusState),
}

/// The tree's focus state (Compose's `FocusOwnerImpl` and `FocusInvalidationManager`).
#[derive(Default)]
pub(crate) struct FocusOwner {
    /// The target that has the focus.
    pub(crate) active: Option<FocusTargetRef>,
    /// It captured it.
    pub(crate) captured: bool,
    /// Event nodes to tell their state when the pass ends, by chain entry.
    pub(crate) invalidated_events: SmallVec<[(NodeId, EntryId); 4]>,
    /// Whether the active target may not be able to keep the focus any more: asked again when
    /// the pass ends (`invalidateFocus`).
    pub(crate) recheck_active: bool,
    /// What `saveFocusedChild` saved: the composite key hash of each target's focused child
    /// (`previouslyFocusedChildHash`), the root's under its key too.
    pub(crate) saved_children: HashMap<FocusPlace, u64>,
    /// The saveable registry's entries of what's saved (`focusRestorationEntry`).
    pub(crate) registrations: HashMap<FocusTargetRef, compose_runtime::RegistryEntry>,
    /// The targets whose `on_enter` (true) or `on_exit` run now: not run again inside themselves
    /// (`isProcessingCustomEnter`, `isProcessingCustomExit`).
    pub(crate) processing: SmallVec<[(FocusTargetRef, bool); 2]>,
    /// Calls waiting for their node to be back in its chain.
    deferred: Vec<(NodeId, EntryId, EntryId, FocusCall)>,
    /// The tree itself moves the focus (an overlay taking it or giving it back, inside a layout):
    /// nothing scrolls, and a modal overlay opening in this frame, which takes it next, doesn't
    /// shut it out.
    pub(crate) moved_by_tree: bool,
    /// The overlay whose window `active` and `captured` are of: `None`, the main one's. What
    /// takes keys and searches, but for a moment while a target of another's is asked (D1284).
    pub(crate) window: Option<NodeId>,
    /// The other windows, lowest first: their focus stays, as a window's does behind another.
    pub(crate) other_windows: SmallVec<[WindowFocus; 1]>,
    /// Arrows, the D-pad, Enter, Back and Escape move the focus if nothing takes them, as on
    /// Android; else Tab alone (D230).
    pub(crate) keys_navigate: bool,
    /// `performRequestFocus` runs: what it tells application code stops if one of them moves the
    /// focus (D1271).
    pub(crate) requesting: bool,
    /// The last search was cancelled, or refused by what can't lose the focus: a key is consumed
    /// though nothing moved (D1275).
    pub(crate) move_stopped: bool,
    /// Targets disabled while holding the focus, leaving their chains once it's gone (D1274).
    pub(crate) target_flips: SmallVec<[(NodeId, EntryId); 1]>,
    /// A search waits for a container to lay out more beyond its bounds (D477): it unwinds, and
    /// is made again after the next layout.
    pub(crate) search_waiting: bool,
}

/// The focus of a window that isn't the one in `FocusOwner`, as each of Compose's owners has its
/// own.
#[derive(Clone, Copy)]
pub(crate) struct WindowFocus {
    pub(crate) window: Option<NodeId>,
    pub(crate) active: Option<FocusTargetRef>,
    pub(crate) captured: bool,
}

/// The result of `performCustomRequestFocus` and `performCustomClearFocus`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CustomDestination {
    None,
    Cancelled,
    Redirected,
}

// ---- The nodes -----------------------------------------------------------------------------

impl Modifier {
    /// Makes the element a focus target, as `Modifier.focusTarget()`: it can take the focus, and
    /// is the parent of the targets below it. It shows nothing and handles nothing: `focusable`
    /// adds its semantics, its interactions and its scrolling into view.
    pub fn focus_target(self) -> Modifier {
        self.then_custom(FocusTargetElement)
    }

    /// Makes the element a group for focus searches, as `Modifier.focusGroup()`: a target that
    /// can't take the focus itself, its children searched together. `focus_properties` before it
    /// can make it focusable.
    pub fn focus_group(self) -> Modifier {
        self.then_custom(FocusGroupElement)
    }

    /// Calls `on_focus_event` with the state of the focus target it covers (the next one after it
    /// in its chain or below it) at each focus event of that target, a change or not, as
    /// `Modifier.onFocusEvent`.
    pub fn on_focus_event(
        self,
        on_focus_event: impl compose_runtime::IntoCallback<dyn Fn(FocusState)>,
    ) -> Modifier {
        self.then_custom(FocusEventElement(on_focus_event.into_callback()))
    }
}

struct FocusTargetElement;

impl ModifierNodeElement for FocusTargetElement {
    type Node = FocusTargetNode;
    fn create(&self) -> FocusTargetNode {
        FocusTargetNode::new(Focusability::ALWAYS, None)
    }
    fn update(&self, _node: &mut FocusTargetNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "focus_target"
    }
}

struct FocusGroupElement;

impl ModifierNodeElement for FocusGroupElement {
    type Node = FocusTargetNode;
    fn create(&self) -> FocusTargetNode {
        FocusTargetNode::new(Focusability::NEVER, None)
    }
    fn update(&self, _node: &mut FocusTargetNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "focus_group"
    }
}

/// A focus target to delegate to, as what Compose's `FocusTargetModifierNode(focusability,
/// onFocusChange)` makes. The tree holds its logic: through the handle or the context of the node
/// holding it, `focus_state`, `request_focus` and `focused_rect`.
pub struct FocusTargetNode {
    focusability: Focusability,
    on_focus_change: Option<Callback<dyn Fn(FocusState, FocusState)>>,
}

impl FocusTargetNode {
    /// A target of `focusability`; `on_focus_change` is told its state's changes (previous and
    /// current), after the transaction, as application code is.
    pub fn new(
        focusability: Focusability,
        on_focus_change: Option<Callback<dyn Fn(FocusState, FocusState)>>,
    ) -> FocusTargetNode {
        FocusTargetNode { focusability, on_focus_change }
    }

    pub fn focusability(&self) -> Focusability {
        self.focusability
    }

    /// Sets its focusability. Having the focus and not being able to keep it, it loses it, as
    /// Compose's setter: at the end of this pass, or where the tree next applies its nodes'
    /// requests (`NodeContext::invalidate_focus_properties` is what tells the tree).
    pub fn set_focusability(&mut self, focusability: Focusability, context: &mut NodeContext<'_>) {
        if self.focusability != focusability {
            self.focusability = focusability;
            context.invalidate_focus_properties();
        }
    }
}

/// Compose's factory for a target to delegate to.
pub fn focus_target_modifier_node(
    focusability: Focusability,
    on_focus_change: Option<Callback<dyn Fn(FocusState, FocusState)>>,
) -> FocusTargetNode {
    FocusTargetNode::new(focusability, on_focus_change)
}

impl ModifierNode for FocusTargetNode {
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    /// It says when it changed (`set_focusability`), as Compose's `shouldAutoInvalidate = false`.
    fn should_auto_invalidate(&self) -> bool {
        false
    }
}

impl FocusTargetModifierNode for FocusTargetNode {
    fn focusability(&self) -> Focusability {
        self.focusability
    }
    fn on_focus_changed(
        &mut self,
        previous: FocusState,
        current: FocusState,
        context: &mut EventContext<'_>,
    ) {
        if let Some(on_focus_change) = &self.on_focus_change {
            context.focus_change_action(on_focus_change.clone(), previous, current);
        }
    }
}

struct FocusEventElement(Callback<dyn Fn(FocusState)>);

impl ModifierNodeElement for FocusEventElement {
    type Node = FocusEventNode;
    fn create(&self) -> FocusEventNode {
        FocusEventNode(self.0.clone())
    }
    fn update(&self, node: &mut FocusEventNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
    fn name(&self) -> &'static str {
        "on_focus_event"
    }
}

struct FocusEventNode(Callback<dyn Fn(FocusState)>);

impl ModifierNode for FocusEventNode {
    fn as_focus_event(&mut self) -> Option<&mut dyn crate::modifier::FocusEventModifierNode> {
        Some(self)
    }
}

impl crate::modifier::FocusEventModifierNode for FocusEventNode {
    fn on_focus_event(&mut self, state: FocusState, context: &mut EventContext<'_>) {
        context.focus_event_action(self.0.clone(), state, None);
    }
}

impl crate::modifier::FocusEventModifierNode for OnFocusChangedNode {
    fn on_focus_event(&mut self, state: FocusState, context: &mut EventContext<'_>) {
        context.focus_event_action(self.callback.clone(), state, Some(self.last.clone()));
    }
}

// ---- States and the targets' tree ----------------------------------------------------------

impl UiTree {
    /// The layout node holding the focused target.
    pub fn focused(&self) -> Option<NodeId> {
        self.focus.active.map(|target| target.node)
    }

    /// Sets the active target (`activeFocusTargetNode`'s setter): a change ends a capture, and
    /// makes what's queued for application code from before it stale.
    pub(crate) fn set_active_target(&mut self, target: Option<FocusTargetRef>) {
        let previous = self.focus.active;
        self.focus.active = target;
        if target.is_none() || previous != target {
            self.focus.captured = false;
        }
        if previous != target {
            let generation = &self.focus_requests.focus_generation;
            generation.set(generation.get().wrapping_add(1));
        }
    }

    /// Where `target`'s entry is in its layout node's chain.
    pub(crate) fn target_index(&self, target: FocusTargetRef) -> Option<usize> {
        self.get(target.node)?.chain.iter().position(|entry| entry.id == target.entry)
    }

    /// Whether `target` is attached: its entry is in its chain, attached, and its delegate there.
    /// A window's view asked for the focus (`requestFocus(direction, rect)`), as
    /// `AndroidComposeView.requestFocus`: with something focused already nothing changes;
    /// else a search from `previous` (where it was, outside): whether it found something.
    pub(crate) fn owner_requested_focus(
        &mut self,
        direction: FocusDirection,
        previous: Option<crate::unit::Rect>,
    ) -> bool {
        if self.focus.active.is_some() {
            return true;
        }
        self.focus_search(direction, previous, &mut |tree, target| {
            tree.request_focus_target(target, direction)
        }) == Some(true)
    }

    /// A window's view lost the focus, as `releaseFocus`: forced, and no block has a say.
    pub(crate) fn owner_lost_focus(&mut self) {
        self.in_each_window(|tree| {
            tree.prepare_to_clear_focus(None, true, true);
            if let Some(previous) = tree.focus.active {
                tree.set_active_target(None);
                tree.dispatch_focus_callbacks(previous, FocusState::Active, FocusState::Inactive);
            }
        });
    }

    // ---- Windows ---------------------------------------------------------------------------

    /// The overlay whose window `node` is in: `None`, the main one.
    pub(crate) fn focus_window_of(&self, node: NodeId) -> Option<NodeId> {
        let mut current = Some(node);
        while let Some(id) = current {
            let value = self.get(id)?;
            if value.overlay && self.overlay_node(id).is_some_and(|it| it.focus_on_open) {
                return Some(id);
            }
            current = value.parent.or_else(|| {
                self.removed_parents
                    .iter()
                    .rfind(|(child, _)| *child == id)
                    .map(|(_, parent)| *parent)
            });
        }
        None
    }

    /// The active target and capture of the window `node` is in.
    fn focus_of_focus_window_of(&self, node: NodeId) -> (Option<FocusTargetRef>, bool) {
        if !self.focus.other_windows.is_empty() {
            let window = self.focus_window_of(node);
            if window != self.focus.window
                && let Some(other) =
                    self.focus.other_windows.iter().find(|other| other.window == window)
            {
                return (other.active, other.captured);
            }
        }
        (self.focus.active, self.focus.captured)
    }

    /// `f`, with `window`'s focus the one a transaction acts on, as each owner has its own.
    pub(crate) fn in_window<R>(
        &mut self,
        window: Option<NodeId>,
        f: impl FnOnce(&mut UiTree) -> R,
    ) -> R {
        if window == self.focus.window {
            return f(self);
        }
        let Some(at) = self.focus.other_windows.iter().position(|other| other.window == window)
        else {
            return f(self);
        };
        self.swap_window(at);
        let result = f(self);
        self.swap_window(at);
        result
    }

    fn swap_window(&mut self, at: usize) {
        let focus = &mut self.focus;
        let other = &mut focus.other_windows[at];
        mem::swap(&mut other.window, &mut focus.window);
        mem::swap(&mut other.active, &mut focus.active);
        mem::swap(&mut other.captured, &mut focus.captured);
    }

    /// `f`, in the window of `node`.
    pub(crate) fn in_window_of<R>(&mut self, node: NodeId, f: impl FnOnce(&mut UiTree) -> R) -> R {
        if self.focus.other_windows.is_empty() {
            return f(self);
        }
        let window = self.focus_window_of(node);
        self.in_window(window, f)
    }

    /// `f` in each window: what's asked of the active target, whichever it is.
    pub(crate) fn in_each_window(&mut self, mut f: impl FnMut(&mut UiTree)) {
        f(self);
        for at in 0..self.focus.other_windows.len() {
            self.swap_window(at);
            f(self);
            self.swap_window(at);
        }
    }

    /// An overlay that takes the focus attached: a window of its own, with no focus yet, what
    /// had it staying as it is (D1284).
    pub(crate) fn window_opened(&mut self, overlay: NodeId) {
        let window = Some(overlay);
        if self.focus.window == window
            || self.focus.other_windows.iter().any(|other| other.window == window)
        {
            return;
        }
        let focus = &mut self.focus;
        focus.other_windows.push(WindowFocus {
            window: focus.window,
            active: focus.active,
            captured: focus.captured,
        });
        focus.window = window;
        focus.active = None;
        focus.captured = false;
        self.window_focus_changed();
    }

    /// Overlays that stopped taking the focus or are gone: the window under the top one takes
    /// over with the focus it had (nothing's told: it never lost it).
    pub(crate) fn close_gone_windows(&mut self) {
        if self.focus.other_windows.is_empty() && self.focus.window.is_none() {
            return;
        }
        let is_open = |tree: &UiTree, window: Option<NodeId>| {
            window.is_none_or(|overlay| {
                tree.get(overlay).is_some_and(|value| value.overlay)
                    && tree.overlay_node(overlay).is_some_and(|it| it.focus_on_open)
            })
        };
        let mut changed = false;
        let mut at = 0;
        while at < self.focus.other_windows.len() {
            if is_open(self, self.focus.other_windows[at].window) {
                at += 1;
                continue;
            }
            self.swap_window(at);
            self.clear_active_target(true, true);
            self.swap_window(at);
            self.focus.other_windows.remove(at);
        }
        while !is_open(self, self.focus.window) {
            self.clear_active_target(true, true);
            let Some(below) = self.focus.other_windows.pop() else { break };
            let focus = &mut self.focus;
            focus.window = below.window;
            focus.active = below.active;
            focus.captured = below.captured;
            changed = true;
        }
        if changed {
            self.needs_draw = true;
            self.semantics_changed = true;
            self.window_focus_changed();
        }
    }

    /// `is_window_focused` reads this: written after the pass, the tree being in it.
    fn window_focus_changed(&mut self) {
        let window = self.focus.window;
        let requests = self.focus_requests.clone();
        self.focus_requests
            .side_effects
            .borrow_mut()
            .push(Box::new(move || requests.focused_window.0.set(window)));
        self.focus_requests.wake();
    }

    /// The event nodes among `events` on the path from this window's active target up: told what
    /// that gives them, and taken out of `events`.
    fn tell_events_of_active_path(&mut self, events: &mut SmallVec<[(NodeId, EntryId); 4]>) {
        if let Some(active) = self.focus.active {
            let active_state = self.focus_state_of(active);
            let kinds = NodeKinds::FOCUS_TARGET | NodeKinds::FOCUS_EVENT;
            let mut targets = 0;
            let mut current = Some(active.node);
            let mut end = self.target_index(active).map_or(0, |index| index + 1);
            while let Some(id) = current {
                let Some(value) = self.get(id) else { break };
                let next = self.focus_parent_of(id);
                if value.kinds.any(kinds) {
                    for index in (0..end.min(value.chain.len())).rev() {
                        let entry = &self.node(id).chain[index];
                        if !entry.kinds.any(kinds) {
                            continue;
                        }
                        let (entry_id, is_target) =
                            (entry.id, entry.kinds.any(NodeKinds::FOCUS_TARGET));
                        // Those between the active target and the first target above are told its
                        // state, those further up `ActiveParent`.
                        if is_target {
                            targets += 1;
                        }
                        let Some(at) = events.iter().position(|each| *each == (id, entry_id))
                        else {
                            continue;
                        };
                        events.remove(at);
                        let state =
                            if targets <= 1 { active_state } else { FocusState::ActiveParent };
                        self.tell_event_entry(id, entry_id, state);
                    }
                }
                current = next;
                end = usize::MAX;
            }
        }
    }

    fn target_is_attached(&mut self, target: FocusTargetRef) -> bool {
        let Some(index) = self.target_index(target) else { return false };
        let entry = &mut self.node_mut(target.node).chain[index];
        if entry.state & entry_state::ATTACHED == 0 {
            return false;
        }
        target.delegate == EntryId::NO_DELEGATE
            || entry.node.downcast_ref::<Taken>().is_some()
            || crate::delegation::find_delegate(&mut *entry.node, target.delegate).is_some()
    }

    /// The layout node a walk up the targets goes to from `node`: its parent (the one it was
    /// removed from, in the pass removing it), none from an overlay, a root of its own as
    /// Compose's popup is.
    pub(crate) fn focus_parent_of(&self, node: NodeId) -> Option<NodeId> {
        let value = self.get(node)?;
        if value.overlay {
            return None;
        }
        value.parent.or_else(|| {
            self.removed_parents
                .iter()
                .rfind(|(child, _)| *child == node)
                .map(|(_, parent)| *parent)
        })
    }

    /// The targets of entry `index` of `node`, as Compose's `dispatchForKind`.
    fn push_entry_targets(
        &mut self,
        node: NodeId,
        index: usize,
        targets: &mut SmallVec<[FocusTargetRef; 8]>,
    ) {
        let entry = &mut self.node_mut(node).chain[index];
        let id = entry.id;
        crate::delegation::dispatch_for_kind(
            &mut *entry.node,
            entry.own,
            NodeKinds::FOCUS_TARGET,
            EntryId::NO_DELEGATE,
            &mut |_, delegate| targets.push(FocusTargetRef { node, entry: id, delegate }),
        );
    }

    /// The targets above entry `before` of `node` (exclusive), nearest first, as
    /// `visitAncestors(Nodes.FocusTarget)`.
    pub(crate) fn targets_above_place(
        &mut self,
        node: NodeId,
        before: usize,
    ) -> SmallVec<[FocusTargetRef; 8]> {
        let mut targets = SmallVec::new();
        let mut current = Some(node);
        let mut end = before;
        while let Some(id) = current {
            let Some(value) = self.get(id) else { break };
            let next = self.focus_parent_of(id);
            if value.kinds.any(NodeKinds::FOCUS_TARGET) {
                for index in (0..end.min(value.chain.len())).rev() {
                    let entry = &self.node(id).chain[index];
                    if entry.kinds.any(NodeKinds::FOCUS_TARGET)
                        && entry.state & entry_state::ATTACHED != 0
                    {
                        self.push_entry_targets(id, index, &mut targets);
                    }
                }
            }
            current = next;
            end = usize::MAX;
        }
        targets
    }

    /// The targets above `target`, nearest first.
    pub(crate) fn targets_above(
        &mut self,
        target: FocusTargetRef,
    ) -> SmallVec<[FocusTargetRef; 8]> {
        match self.target_index(target) {
            Some(index) => self.targets_above_place(target.node, index),
            None => SmallVec::new(),
        }
    }

    /// The nearest target above `target`: `None` is the root.
    pub(crate) fn focus_parent(&mut self, target: FocusTargetRef) -> FocusPlace {
        self.targets_above(target).first().copied()
    }

    /// The state of `target`, derived as `FocusTargetNode.focusState`.
    pub(crate) fn focus_state_of(&mut self, target: FocusTargetRef) -> FocusState {
        let (active, captured) = self.focus_of_focus_window_of(target.node);
        let Some(active) = active else { return FocusState::Inactive };
        if active == target {
            return if captured { FocusState::Captured } else { FocusState::Active };
        }
        if self.targets_above(active).contains(&target) {
            FocusState::ActiveParent
        } else {
            FocusState::Inactive
        }
    }

    /// The state of a place: the root's is `ActiveParent` while anything has the focus.
    pub(crate) fn focus_state_at(&mut self, place: FocusPlace) -> FocusState {
        match place {
            Some(target) => self.focus_state_of(target),
            None if self.focus.active.is_some() => FocusState::ActiveParent,
            None => FocusState::Inactive,
        }
    }

    /// The target directly below `place` on the path to the active one, as `activeChild`.
    pub(crate) fn active_child(&mut self, place: FocusPlace) -> Option<FocusTargetRef> {
        let active = self.focus.active?;
        if place == Some(active) {
            return None;
        }
        let above = self.targets_above(active);
        match place {
            None => Some(above.last().copied().unwrap_or(active)),
            Some(target) => match above.iter().position(|each| *each == target) {
                Some(0) => Some(active),
                Some(at) => Some(above[at - 1]),
                None => None,
            },
        }
    }

    /// The first branches of a walk down from the root: the tree's root, or while a modal overlay
    /// shows, what's below that overlay's own target, as a window's root isn't a stop.
    pub(crate) fn root_branch(&self) -> (NodeId, usize) {
        match self.modal_overlay() {
            Some(modal) => {
                let chain = &self.node(modal).chain;
                let after = chain
                    .iter()
                    .rposition(|entry| entry.kinds.any(NodeKinds::FOCUS_TARGET))
                    .map_or(0, |index| index + 1);
                (modal, after)
            }
            None => (self.root_id(), 0),
        }
    }

    /// The first targets down each branch from entry `start` of `node`, as `visitChildren`.
    pub(crate) fn targets_below_place(
        &mut self,
        node: NodeId,
        start: usize,
    ) -> SmallVec<[FocusTargetRef; 8]> {
        let mut targets = SmallVec::new();
        let mut branches: SmallVec<[(NodeId, usize); 8]> = SmallVec::new();
        branches.push((node, start));
        while let Some((node, start)) = branches.pop() {
            let Some(value) = self.get(node) else { continue };
            if value.deactivated {
                continue;
            }
            let found = (start < value.chain.len()
                && value.chain[start].aggregate.any(NodeKinds::FOCUS_TARGET))
            .then(|| {
                (start..value.chain.len()).find(|&index| {
                    let entry = &value.chain[index];
                    entry.kinds.any(NodeKinds::FOCUS_TARGET)
                        && entry.state & entry_state::ATTACHED != 0
                })
            })
            .flatten();
            match found {
                Some(index) => self.push_entry_targets(node, index, &mut targets),
                None => {
                    let at = branches.len();
                    let value = self.node(node);
                    branches.extend(
                        value
                            .children
                            .iter()
                            .map(|child| child.node)
                            // An overlay is a root of its own.
                            .filter(|&child| !self.node(child).overlay)
                            .map(|child| (child, 0)),
                    );
                    branches[at..].reverse();
                }
            }
        }
        targets
    }

    /// The targets directly below `place`, as `visitChildren(Nodes.FocusTarget)`.
    pub(crate) fn focus_children(&mut self, place: FocusPlace) -> SmallVec<[FocusTargetRef; 8]> {
        match place {
            None => {
                let (node, start) = self.root_branch();
                self.targets_below_place(node, start)
            }
            Some(target) => match self.target_index(target) {
                Some(index) => self.targets_below_place(target.node, index + 1),
                None => SmallVec::new(),
            },
        }
    }

    /// Whether `target` can be found by a search: its layout node is placed, attached, and no
    /// other window has it, as `isEligibleForFocusSearch`.
    pub(crate) fn is_eligible_for_focus_search(&self, target: FocusTargetRef) -> bool {
        self.get(target.node).is_some_and(|value| value.attached)
            && self.is_placed_in_root(target.node)
            && !self.is_in_another_window(target.node)
    }

    /// Whether `node` is in another window than the one acted on: out of its searches, as
    /// Compose's `isOccludedByInteractionBarrier` is for what's behind a barrier (D1269, D1284).
    pub(crate) fn is_in_another_window(&self, node: NodeId) -> bool {
        !self.focus.other_windows.is_empty() && self.focus_window_of(node) != self.focus.window
    }

    /// The overlay whose window is acted on: the top one's, for the keys and what isn't from
    /// behind (a dialog, a focusable popup).
    pub(crate) fn modal_overlay(&self) -> Option<NodeId> {
        self.focus.window
    }

    /// Whether `node` is `ancestor` or below it.
    pub fn is_at_or_below(&self, node: NodeId, ancestor: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.get(id).and_then(|value| value.parent);
        }
        false
    }

    /// The nodes that can take the focus, in tree order, skipping what a search skips. For tests
    /// and dumps: searches walk the targets' tree.
    #[doc(hidden)]
    pub fn focus_order(&mut self) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut stack: Vec<FocusPlace> = vec![None];
        while let Some(place) = stack.pop() {
            if let Some(target) = place
                && self.is_eligible_for_focus_search(target)
                && self.fetch_focus_properties(target).can_focus
                && order.last() != Some(&target.node)
            {
                order.push(target.node);
            }
            let children = self.focus_children(place);
            stack.extend(children.into_iter().rev().map(Some));
        }
        order
    }

    // ---- Telling the nodes -----------------------------------------------------------------

    /// Tells `target` or its event nodes, as a node's code: from the chain it's in, its requests
    /// applied after it. A node that is running is told when it has returned.
    fn run_focus_call(&mut self, node: NodeId, entry: EntryId, delegate: EntryId, call: FocusCall) {
        let Some(index) =
            self.get(node).and_then(|value| value.chain.iter().position(|each| each.id == entry))
        else {
            return;
        };
        if self.node(node).chain[index].node.downcast_ref::<Taken>().is_some() {
            self.focus.deferred.push((node, entry, delegate, call));
            return;
        }
        let mut requests = Requests::default();
        let mut actions = mem::take(&mut self.pending_actions);
        self.with_event_context(
            node,
            index,
            delegate,
            &mut actions,
            &mut requests,
            None,
            false,
            |modifier_node, context| match call {
                FocusCall::Changed(previous, current) => {
                    if let Some(target) = modifier_node.as_focus_target() {
                        target.on_focus_changed(previous, current, context);
                    }
                }
                FocusCall::Event(state) => {
                    if let Some(event) = modifier_node.as_focus_event() {
                        event.on_focus_event(state, context);
                    }
                }
            },
        );
        actions.append(&mut self.pending_actions);
        self.pending_actions = actions;
        self.apply_focus_node_requests(requests);
        self.run_deferred_focus_calls();
    }

    /// The calls that waited for their nodes to be back in their chains.
    fn run_deferred_focus_calls(&mut self) {
        if self.focus.deferred.is_empty() {
            return;
        }
        for (node, entry, delegate, call) in mem::take(&mut self.focus.deferred) {
            self.run_focus_call(node, entry, delegate, call);
        }
    }

    /// Applies what a node of the focus system asked in its callback.
    fn apply_focus_node_requests(&mut self, requests: Requests) {
        let mut actions = mem::take(&mut self.pending_actions);
        self.apply_requests(requests, &mut actions);
        self.pending_actions = actions;
    }

    /// Tells `target` its state changed, then the event nodes it covers, stopping if a callback
    /// moved the focus, as `dispatchFocusCallbacks`.
    pub(crate) fn dispatch_focus_callbacks(
        &mut self,
        target: FocusTargetRef,
        previous: FocusState,
        current: FocusState,
    ) {
        let active_before = self.focus.active;
        if previous != current {
            let call = FocusCall::Changed(previous, current);
            self.run_focus_call(target.node, target.entry, target.delegate, call);
            // Its semantics say whether it's focused (D1257).
            self.invalidate_semantics_of(target.node);
        }
        if let Some(index) = self.target_index(target) {
            self.dispatch_focus_events(target.node, index, true, current, active_before);
        }
    }

    /// Tells the event nodes from entry `index` of `node` up to the next target above, as
    /// `visitSelfAndAncestors(Nodes.FocusEvent, untilType = Nodes.FocusTarget)`; with
    /// `include_index`, entry `index` (the target's own) is one of them.
    fn dispatch_focus_events(
        &mut self,
        node: NodeId,
        index: usize,
        include_index: bool,
        state: FocusState,
        active_before: Option<FocusTargetRef>,
    ) {
        let kinds = NodeKinds::FOCUS_TARGET | NodeKinds::FOCUS_EVENT;
        let mut current = Some(node);
        let mut end = if include_index { index + 1 } else { index };
        while let Some(id) = current {
            let Some(value) = self.get(id) else { return };
            let next = self.focus_parent_of(id);
            if value.kinds.any(kinds) {
                for each in (0..end.min(value.chain.len())).rev() {
                    let Some(entry) = self.node(id).chain.get(each) else { continue };
                    if !entry.kinds.any(kinds) {
                        continue;
                    }
                    let own_entry = include_index && id == node && each == index;
                    if !own_entry && entry.kinds.any(NodeKinds::FOCUS_TARGET) {
                        return;
                    }
                    if entry.kinds.any(NodeKinds::FOCUS_EVENT)
                        && entry.state & entry_state::ATTACHED != 0
                    {
                        for (entry_id, delegate) in self.focus_events_of_entry(id, each) {
                            if self.focus.active != active_before {
                                return;
                            }
                            self.run_focus_call(id, entry_id, delegate, FocusCall::Event(state));
                        }
                    }
                }
            }
            current = next;
            end = usize::MAX;
        }
    }

    /// The event nodes of entry `index` of `node`: its node, or its delegates that are.
    fn focus_events_of_entry(
        &mut self,
        node: NodeId,
        index: usize,
    ) -> SmallVec<[(EntryId, EntryId); 2]> {
        let mut events = SmallVec::new();
        let entry = &mut self.node_mut(node).chain[index];
        let id = entry.id;
        crate::delegation::dispatch_for_kind(
            &mut *entry.node,
            entry.own,
            NodeKinds::FOCUS_EVENT,
            EntryId::NO_DELEGATE,
            &mut |_, delegate| events.push((id, delegate)),
        );
        events
    }

    // ---- Properties ------------------------------------------------------------------------

    /// What `target`'s properties are, as `fetchFocusProperties`: each properties node from the
    /// target up to the next target above, nearest first (so the outermost wins).
    pub(crate) fn fetch_focus_properties(&mut self, target: FocusTargetRef) -> FocusProperties {
        let mut properties = FocusProperties::default();
        let Some(index) = self.target_index(target) else {
            properties.can_focus = false;
            return properties;
        };
        let input_mode = self.input_mode.clone();
        properties.can_focus = {
            let entry = &mut self.node_mut(target.node).chain[index];
            match entry.resolve(target.delegate).and_then(|node| node.as_focus_target()) {
                Some(node) => node.focusability().can_focus(|| input_mode.input_mode()),
                // The node running: what it answers isn't at hand, and it's in a transaction.
                None => true,
            }
        };
        let kinds = NodeKinds::FOCUS_TARGET | NodeKinds::FOCUS_PROPERTIES;
        let mut current = Some(target.node);
        let mut end = index + 1;
        'walk: while let Some(id) = current {
            let Some(value) = self.get(id) else { break };
            let next = self.focus_parent_of(id);
            if value.kinds.any(kinds) {
                for each in (0..end.min(value.chain.len())).rev() {
                    let entry = &self.node(id).chain[each];
                    if !entry.kinds.any(kinds) {
                        continue;
                    }
                    let own_entry = id == target.node && each == index;
                    if !own_entry && entry.kinds.any(NodeKinds::FOCUS_TARGET) {
                        break 'walk;
                    }
                    if entry.kinds.any(NodeKinds::FOCUS_PROPERTIES)
                        && entry.state & entry_state::ATTACHED != 0
                    {
                        let entry = &mut self.node_mut(id).chain[each];
                        crate::delegation::dispatch_for_kind(
                            &mut *entry.node,
                            entry.own,
                            NodeKinds::FOCUS_PROPERTIES,
                            EntryId::NO_DELEGATE,
                            &mut |node, _| {
                                if let Some(node) = node.as_focus_properties() {
                                    node.apply_focus_properties(&mut properties);
                                }
                            },
                        );
                    }
                }
            }
            current = next;
            end = usize::MAX;
        }
        properties
    }

    /// `fetch_focus_properties` observed: a write to what they read asks again whether the active
    /// target can keep the focus (`grantFocus`'s `observeReads`).
    fn fetch_focus_properties_observed(&mut self, target: FocusTargetRef) -> FocusProperties {
        match self.observer.clone() {
            Some(observer) => observer
                .observe(observation_key(target.node, Observation::FocusProperties), || {
                    self.fetch_focus_properties(target)
                }),
            None => self.fetch_focus_properties(target),
        }
    }

    /// Where `target` is in the root, as `focusRect()`: the rectangle its properties give, or the
    /// bounds its place in the chain gives it (after an `offset` or a `padding` before it).
    pub(crate) fn focus_rect_of(&mut self, target: FocusTargetRef) -> Rect {
        let Some(index) = self.target_index(target) else {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        };
        let custom = self.fetch_focus_properties(target).focus_rect;
        let value = self.node(target.node);
        if value.layers.is_empty() {
            return self.bounds_in_root(target.node);
        }
        let layer = value.layer_of(index).min(value.layers.len() - 1);
        let size = value.layers[layer].measured;
        let origin = self.layer_origin(target.node, layer);
        if !custom.left.is_nan() {
            return Rect::new(
                custom.left + origin.x,
                custom.top + origin.y,
                custom.right + origin.x,
                custom.bottom + origin.y,
            );
        }
        Rect::new(origin.x, origin.y, origin.x + size.width as f32, origin.y + size.height as f32)
    }

    // ---- Transactions ----------------------------------------------------------------------

    /// Asks `target` to take the focus, as `FocusTargetNode.requestFocus(direction)`: one that
    /// can't passes it to the child an `Enter` would pick.
    pub(crate) fn request_focus_target(
        &mut self,
        target: FocusTargetRef,
        direction: FocusDirection,
    ) -> bool {
        self.in_window_of(target.node, |tree| tree.request_focus_target_here(target, direction))
    }

    fn request_focus_target_here(
        &mut self,
        target: FocusTargetRef,
        direction: FocusDirection,
    ) -> bool {
        if self.fetch_focus_properties(target).can_focus {
            self.assign_focus(target, direction)
        } else {
            self.find_child_corresponding_to_focus_enter(
                Some(target),
                direction,
                &mut |tree, child| tree.assign_focus(child, direction),
            )
        }
    }

    fn assign_focus(&mut self, target: FocusTargetRef, direction: FocusDirection) -> bool {
        // The tree moving it (an overlay's), no one's block has a say.
        if self.focus.moved_by_tree {
            return self.perform_request_focus(target);
        }
        match self.perform_custom_request_focus(Some(target), direction) {
            CustomDestination::None => self.perform_request_focus(target),
            CustomDestination::Redirected => true,
            CustomDestination::Cancelled => false,
        }
    }

    /// As `performRequestFocus`: the previous active target cleared (its ancestors' states
    /// diffed with the new one's), the focus granted, then the callbacks in Compose's order,
    /// each followed by "did a callback move the focus? then stop".
    pub(crate) fn perform_request_focus(&mut self, target: FocusTargetRef) -> bool {
        let requesting = mem::replace(&mut self.focus.requesting, true);
        let done = self.request_focus_of_target(target);
        self.focus.requesting = requesting;
        done
    }

    fn request_focus_of_target(&mut self, target: FocusTargetRef) -> bool {
        let previous_active = self.focus.active;
        let previous_state = self.focus_state_of(target);
        if previous_active == Some(target) {
            // Focus events are sent again if a focused node is asked.
            self.dispatch_focus_callbacks(target, previous_state, previous_state);
            return true;
        }
        let mut previous_ancestors: SmallVec<[FocusTargetRef; 8]> = match previous_active {
            Some(previous) => self.targets_above(previous),
            None => SmallVec::new(),
        };
        let mut new_ancestors: SmallVec<[FocusTargetRef; 8]> = SmallVec::new();
        let mut clear_previous = true;
        for ancestor in self.targets_above(target) {
            if let Some(at) = previous_ancestors.iter().position(|each| *each == ancestor) {
                previous_ancestors.remove(at);
            } else {
                new_ancestors.push(ancestor);
            }
            if Some(ancestor) == previous_active {
                clear_previous = false;
            }
        }
        if clear_previous
            && let Some(previous) = previous_active
            && !self.prepare_to_clear_focus(Some(previous), self.focus.moved_by_tree, true)
        {
            // Clearing the previous node was refused.
            return false;
        }
        self.grant_focus(target);
        if clear_previous && let Some(previous) = previous_active {
            self.dispatch_focus_callbacks(previous, FocusState::Active, FocusState::Inactive);
        }
        // Top down: those of the previous active node that aren't parents any more, then the new
        // parents.
        for &ancestor in previous_ancestors.iter().rev() {
            if self.focus.active != Some(target) {
                return false;
            }
            self.dispatch_focus_callbacks(ancestor, FocusState::ActiveParent, FocusState::Inactive);
        }
        for &ancestor in new_ancestors.iter().rev() {
            if self.focus.active != Some(target) {
                return false;
            }
            let previous = if Some(ancestor) == previous_active {
                FocusState::Active
            } else {
                FocusState::Inactive
            };
            self.dispatch_focus_callbacks(ancestor, previous, FocusState::ActiveParent);
        }
        if self.focus.active != Some(target) {
            return false;
        }
        self.dispatch_focus_callbacks(target, previous_state, FocusState::Active);
        self.focus.active == Some(target)
    }

    fn grant_focus(&mut self, target: FocusTargetRef) {
        // The window has it for the content, as `requestOwnerFocus`.
        if self.focus.active.is_none() {
            self.focus_requests.owner_focus.set(Some(true));
            self.focus_requests.wake();
        }
        // The properties that made it focusable are observed: losing `can_focus` loses the focus.
        self.fetch_focus_properties_observed(target);
        self.set_active_target(Some(target));
    }

    /// As `prepareToClearFocus`: an active parent's children first, a captured target refusing
    /// unless forced.
    pub(crate) fn prepare_to_clear_focus(
        &mut self,
        place: FocusPlace,
        forced: bool,
        refresh_events: bool,
    ) -> bool {
        match self.focus_state_at(place) {
            FocusState::Active | FocusState::Inactive => true,
            FocusState::ActiveParent => {
                let child = self.active_child(place);
                let cleared = match child {
                    Some(child) => self.prepare_to_clear_focus(Some(child), forced, refresh_events),
                    None => true,
                };
                if cleared
                    && refresh_events
                    && let Some(target) = place
                {
                    self.dispatch_focus_callbacks(
                        target,
                        FocusState::ActiveParent,
                        FocusState::Inactive,
                    );
                }
                cleared
            }
            FocusState::Captured => forced,
        }
    }

    /// As `captureFocus`.
    pub(crate) fn capture_focus_target(&mut self, target: FocusTargetRef) -> bool {
        self.in_window_of(target.node, |tree| tree.capture_focus_target_here(target))
    }

    fn capture_focus_target_here(&mut self, target: FocusTargetRef) -> bool {
        match self.focus_state_of(target) {
            FocusState::Active => {
                self.focus.captured = true;
                self.dispatch_focus_callbacks(target, FocusState::Active, FocusState::Captured);
                true
            }
            FocusState::Captured => true,
            FocusState::ActiveParent | FocusState::Inactive => false,
        }
    }

    /// As `freeFocus`.
    pub(crate) fn free_focus_target(&mut self, target: FocusTargetRef) -> bool {
        self.in_window_of(target.node, |tree| tree.free_focus_target_here(target))
    }

    fn free_focus_target_here(&mut self, target: FocusTargetRef) -> bool {
        match self.focus_state_of(target) {
            FocusState::Captured => {
                self.focus.captured = false;
                self.dispatch_focus_callbacks(target, FocusState::Captured, FocusState::Active);
                true
            }
            FocusState::Active => true,
            FocusState::ActiveParent | FocusState::Inactive => false,
        }
    }

    /// As `performCustomRequestFocus`: the `on_exit`s and `on_enter`s on the way from where the
    /// focus is to `place`.
    pub(crate) fn perform_custom_request_focus(
        &mut self,
        place: FocusPlace,
        direction: FocusDirection,
    ) -> CustomDestination {
        match self.focus_state_at(place) {
            FocusState::Active | FocusState::Captured => CustomDestination::None,
            FocusState::ActiveParent => match self.active_child(place) {
                Some(child) => self.perform_custom_clear_focus(Some(child), direction),
                None => CustomDestination::None,
            },
            FocusState::Inactive => {
                let Some(target) = place else { return CustomDestination::None };
                let parent = self.focus_parent(target);
                match self.focus_state_at(parent) {
                    FocusState::Captured => CustomDestination::Cancelled,
                    FocusState::ActiveParent => {
                        self.perform_custom_request_focus(parent, direction)
                    }
                    FocusState::Active => self.perform_custom_enter(parent, direction),
                    FocusState::Inactive => {
                        match self.perform_custom_request_focus(parent, direction) {
                            CustomDestination::None => self.perform_custom_enter(parent, direction),
                            other => other,
                        }
                    }
                }
            }
        }
    }

    /// As `performCustomClearFocus`.
    pub(crate) fn perform_custom_clear_focus(
        &mut self,
        place: FocusPlace,
        direction: FocusDirection,
    ) -> CustomDestination {
        match self.focus_state_at(place) {
            FocusState::Active | FocusState::Inactive => CustomDestination::None,
            FocusState::Captured => CustomDestination::Cancelled,
            FocusState::ActiveParent => {
                let below = match self.active_child(place) {
                    Some(child) => self.perform_custom_clear_focus(Some(child), direction),
                    None => CustomDestination::None,
                };
                match below {
                    CustomDestination::None => self.perform_custom_exit(place, direction),
                    other => other,
                }
            }
        }
    }

    fn perform_custom_enter(
        &mut self,
        place: FocusPlace,
        direction: FocusDirection,
    ) -> CustomDestination {
        let Some(target) = place else { return CustomDestination::None };
        self.run_custom_enter_or_exit(target, direction, true)
    }

    fn perform_custom_exit(
        &mut self,
        place: FocusPlace,
        direction: FocusDirection,
    ) -> CustomDestination {
        let Some(target) = place else { return CustomDestination::None };
        self.run_custom_enter_or_exit(target, direction, false)
    }

    /// Runs `target`'s `on_enter` or `on_exit`, once at a time (a block moving the focus would
    /// run itself again otherwise), as `fetchCustomEnterOrExit`: it cancelled, or moved the focus.
    fn run_custom_enter_or_exit(
        &mut self,
        target: FocusTargetRef,
        direction: FocusDirection,
        enter: bool,
    ) -> CustomDestination {
        if self.focus.processing.contains(&(target, enter)) {
            return CustomDestination::None;
        }
        let properties = self.fetch_focus_properties(target);
        let block = if enter { properties.on_enter } else { properties.on_exit };
        let Some(block) = block else { return CustomDestination::None };
        self.focus.processing.push((target, enter));
        let active_before = self.focus.active;
        let canceled = self.run_enter_exit_block(&block, direction);
        self.focus.processing.retain(|each| *each != (target, enter));
        let active_after = self.focus.active;
        if canceled {
            CustomDestination::Cancelled
        } else if active_before != active_after && active_after.is_some() {
            CustomDestination::Redirected
        } else {
            CustomDestination::None
        }
    }

    /// Runs an `on_enter` or `on_exit` block: application code, inside a transaction. What it
    /// asks of requesters and the focus manager acts at once with real results, the tree set
    /// aside where they find it (D1270). Returns whether it cancelled.
    pub(crate) fn run_enter_exit_block(
        &mut self,
        block: &crate::focus_properties::FocusEnterExitBlock,
        direction: FocusDirection,
    ) -> bool {
        let key = self.key;
        crate::node_context::with_tree_set_aside(self, || {
            let mut scope = crate::focus_properties::FocusEnterExitScope::new(key, direction);
            block(&mut scope);
            scope.is_canceled()
        })
    }

    /// Clears the focus, as `clearFocus(force, refreshFocusEvents, …, focusDirection)`: unless
    /// forced, the `on_exit`s on the focused path first, any of which may refuse or redirect.
    pub(crate) fn clear_focus_with(
        &mut self,
        force: bool,
        refresh_events: bool,
        direction: FocusDirection,
    ) -> bool {
        if !force && self.perform_custom_clear_focus(None, direction) != CustomDestination::None {
            return false;
        }
        self.clear_active_target(force, refresh_events)
    }

    /// The private `clearFocus(forced, refreshFocusEvents)`.
    fn clear_active_target(&mut self, forced: bool, refresh_events: bool) -> bool {
        let Some(previous) = self.focus.active else { return true };
        if self.focus.captured && !forced {
            return false;
        }
        let above = self.targets_above(previous);
        self.set_active_target(None);
        if refresh_events {
            self.dispatch_focus_callbacks(previous, FocusState::Active, FocusState::Inactive);
            for ancestor in above {
                self.dispatch_focus_callbacks(
                    ancestor,
                    FocusState::ActiveParent,
                    FocusState::Inactive,
                );
            }
        }
        true
    }

    // ---- Lifecycle ----------------------------------------------------------------------------

    /// The subtree at `root` is being detached: the active target in it loses the focus, as
    /// `FocusTargetNode.onDetach`, while everything is still attached.
    pub(crate) fn focus_subtree_detaching(&mut self, root: NodeId) {
        self.in_each_window(|tree| {
            if let Some(active) = tree.focus.active
                && tree.is_at_or_below(active.node, root)
            {
                tree.clear_active_target(true, true);
                tree.focus.recheck_active = true;
            }
        });
        if !self.focus.saved_children.is_empty() {
            let saved: SmallVec<[FocusTargetRef; 4]> = self
                .focus
                .saved_children
                .keys()
                .flatten()
                .copied()
                .filter(|target| self.is_at_or_below(target.node, root))
                .collect();
            for target in saved {
                self.forget_saved_child_of(target);
            }
        }
    }

    /// `node` is being reset (reused or deactivated): the active target on it loses the focus, as
    /// `FocusTargetNode.onReset`.
    pub(crate) fn focus_node_resetting(&mut self, node: NodeId) {
        self.in_each_window(|tree| {
            if tree.focus.active.is_some_and(|active| active.node == node) {
                tree.clear_active_target(true, true);
            }
        });
        if !self.focus.saved_children.is_empty() {
            let saved: SmallVec<[FocusTargetRef; 4]> = self
                .focus
                .saved_children
                .keys()
                .flatten()
                .copied()
                .filter(|target| target.node == node)
                .collect();
            for target in saved {
                self.forget_saved_child_of(target);
            }
        }
    }

    /// `entries`, which a chain change removed from `node` (each with where it was, in chain
    /// order), are being detached: the active target among them loses the focus, its event nodes
    /// told, those left in the chain, and the targets above; event nodes that went are told
    /// `Inactive`, as Compose's invalidated ones it doesn't find above the active target.
    pub(crate) fn focus_entries_removed(
        &mut self,
        node: NodeId,
        entries: &mut SmallVec<[(ChainEntry, usize); 2]>,
    ) {
        if !self.focus.saved_children.is_empty() {
            let saved: SmallVec<[FocusTargetRef; 4]> = self
                .focus
                .saved_children
                .keys()
                .flatten()
                .copied()
                .filter(|target| entries.iter().any(|(entry, _)| entry.id == target.entry))
                .collect();
            for target in saved {
                self.forget_saved_child_of(target);
            }
        }
        // The window with the active target among them, if any.
        let window = self.focus.window;
        let with_it = std::iter::once(window)
            .chain(self.focus.other_windows.iter().map(|other| other.window))
            .zip(
                std::iter::once(self.focus.active)
                    .chain(self.focus.other_windows.iter().map(|other| other.active)),
            )
            .find(|(_, active)| {
                active
                    .is_some_and(|active| entries.iter().any(|(entry, _)| entry.id == active.entry))
            })
            .map(|(window, _)| window)
            .unwrap_or(window);
        self.in_window(with_it, |tree| tree.focus_entries_removed_here(node, entries));
        for removed in entries.iter_mut() {
            if removed.0.kinds.any(NodeKinds::FOCUS_EVENT) {
                self.tell_removed_event_nodes(node, removed, FocusState::Inactive);
            }
        }
    }

    fn focus_entries_removed_here(
        &mut self,
        node: NodeId,
        entries: &mut SmallVec<[(ChainEntry, usize); 2]>,
    ) {
        if let Some(active) = self.focus.active
            && let Some(at) = entries.iter().position(|(entry, _)| entry.id == active.entry)
        {
            let place = entries[at].1;
            let above = self.targets_above_place(node, place);
            self.set_active_target(None);
            let call = FocusCall::Changed(FocusState::Active, FocusState::Inactive);
            self.run_removed_entry_call(node, &mut entries[at], active.delegate, call);
            self.invalidate_semantics_of(node);
            // Up to the next target above: among those that went, then those left.
            let next_removed = entries[..at]
                .iter()
                .rposition(|(entry, _)| entry.kinds.any(NodeKinds::FOCUS_TARGET));
            for index in (next_removed.map_or(0, |next| next + 1)..at).rev() {
                self.tell_removed_event_nodes(node, &mut entries[index], FocusState::Inactive);
            }
            if next_removed.is_none() {
                self.dispatch_focus_events(node, place, false, FocusState::Inactive, None);
            }
            for index in (0..at).rev() {
                if entries[index].0.kinds.any(NodeKinds::FOCUS_TARGET) {
                    let call = FocusCall::Changed(FocusState::ActiveParent, FocusState::Inactive);
                    for delegate in removed_entry_targets(&mut entries[index].0) {
                        self.run_removed_entry_call(node, &mut entries[index], delegate, call);
                    }
                }
            }
            for ancestor in above {
                self.dispatch_focus_callbacks(
                    ancestor,
                    FocusState::ActiveParent,
                    FocusState::Inactive,
                );
            }
        }
        for removed in entries.iter_mut() {
            if removed.0.kinds.any(NodeKinds::FOCUS_EVENT) {
                self.tell_removed_event_nodes(node, removed, FocusState::Inactive);
            }
        }
    }

    fn tell_removed_event_nodes(
        &mut self,
        node: NodeId,
        removed: &mut (ChainEntry, usize),
        state: FocusState,
    ) {
        if !removed.0.kinds.any(NodeKinds::FOCUS_EVENT) {
            return;
        }
        let mut delegates: SmallVec<[EntryId; 2]> = SmallVec::new();
        let entry = &mut removed.0;
        crate::delegation::dispatch_for_kind(
            &mut *entry.node,
            entry.own,
            NodeKinds::FOCUS_EVENT,
            EntryId::NO_DELEGATE,
            &mut |_, delegate| delegates.push(delegate),
        );
        for delegate in delegates {
            self.run_removed_entry_call(node, removed, delegate, FocusCall::Event(state));
        }
    }

    /// Tells a node out of its chain.
    fn run_removed_entry_call(
        &mut self,
        node: NodeId,
        removed: &mut (ChainEntry, usize),
        delegate: EntryId,
        call: FocusCall,
    ) {
        let (entry, place) = removed;
        let (id, state) = (entry.id, entry.state);
        let Some(modifier_node) = entry.resolve(delegate) else { return };
        let mut requests = Requests::default();
        let mut actions = mem::take(&mut self.pending_actions);
        let context = NodeContext {
            tree: &mut *self,
            node,
            index: None,
            entry: id,
            out_state: state,
            delegated: false,
            place: *place,
            traverse_key: None,
        };
        let mut context = EventContext {
            context,
            actions: &mut actions,
            requests: &mut requests,
            pointer_position: None,
            waiting_parent: false,
        };
        match call {
            FocusCall::Changed(previous, current) => {
                if let Some(target) = modifier_node.as_focus_target() {
                    target.on_focus_changed(previous, current, &mut context);
                }
            }
            FocusCall::Event(state) => {
                if let Some(event) = modifier_node.as_focus_event() {
                    event.on_focus_event(state, &mut context);
                }
            }
        }
        actions.append(&mut self.pending_actions);
        self.pending_actions = actions;
        self.apply_focus_node_requests(requests);
    }

    /// What entry `index` of `node` delegates to changed: an active target that was a delegate
    /// undelegated loses the focus (it was told by its `on_detach`, if it cares).
    /// Entry `entry` of `node` should be a focus target, or not any more (D1274): holding the focus
    /// it loses it first, its focusability having said so, when the pass ends.
    pub(crate) fn flip_focus_target(&mut self, node: NodeId, entry: EntryId, wanted: bool) {
        if !wanted
            && self.focus.active.is_some_and(|active| active.node == node && active.entry == entry)
        {
            self.focus.target_flips.push((node, entry));
            self.focus.recheck_active = true;
            return;
        }
        self.apply_focus_target_flip(node, entry, wanted);
    }

    fn apply_focus_target_flip(&mut self, node: NodeId, entry: EntryId, wanted: bool) {
        let Some(index) = self.entry_index(node, entry) else { return };
        let chain_entry = &mut self.node_mut(node).chain[index];
        let before = chain_entry.kinds;
        chain_entry.set_is_focus_target(wanted);
        let after = chain_entry.kinds;
        self.entry_kinds_changed(node, before, after);
        if !wanted {
            self.forget_saved_child_of(FocusTargetRef {
                node,
                entry,
                delegate: EntryId::NO_DELEGATE,
            });
        }
        // Listeners hear another target now, those of this chain and those above it.
        let value = self.node(node);
        let events: SmallVec<[EntryId; 2]> = value
            .chain
            .iter()
            .filter(|each| each.kinds.any(NodeKinds::FOCUS_EVENT))
            .map(|each| each.id)
            .collect();
        for each in events {
            self.focus.invalidated_events.push((node, each));
        }
        let mut above = self.focus_parent_of(node);
        while let Some(id) = above {
            let Some(value) = self.get(id) else { break };
            let is_target = value.kinds.any(NodeKinds::FOCUS_TARGET);
            let events: SmallVec<[EntryId; 2]> = value
                .chain
                .iter()
                .filter(|each| each.kinds.any(NodeKinds::FOCUS_EVENT))
                .map(|each| each.id)
                .collect();
            for each in events {
                self.focus.invalidated_events.push((id, each));
            }
            if is_target {
                break;
            }
            above = self.focus_parent_of(id);
        }
    }

    pub(crate) fn focus_delegates_changed(&mut self, node: NodeId) {
        self.in_each_window(|tree| {
            if let Some(active) = tree.focus.active
                && active.node == node
            {
                if !tree.target_is_attached(active) {
                    tree.clear_active_target(true, true);
                } else {
                    tree.focus.recheck_active = true;
                }
            }
        });
    }

    /// A chain change of `node` updated, inserted or removed nodes of `kinds`, and updated the event
    /// nodes `events`: as `autoInvalidateNodeSelf`, those are told their state when the pass ends,
    /// and the active target is asked again if it can keep the focus.
    pub(crate) fn focus_chain_changed(
        &mut self,
        node: NodeId,
        kinds: NodeKinds,
        events: &[EntryId],
    ) {
        if kinds.any(NodeKinds::FOCUS_TARGET | NodeKinds::FOCUS_PROPERTIES)
            && self.focus.active.is_some()
        {
            self.focus.recheck_active = true;
        }
        for &entry in events {
            self.focus.invalidated_events.push((node, entry));
        }
    }

    /// Resolves what the pass's changes asked (`FocusInvalidationManager.invalidateNodes`): the
    /// active target asked again whether it can keep the focus; event nodes attached, updated or
    /// changed told the state the active target gives them. Costs nothing when nothing asked.
    pub(crate) fn flush_focus_invalidations(&mut self) {
        self.run_deferred_focus_calls();
        if !self.focus.recheck_active && self.focus.invalidated_events.is_empty() {
            return;
        }
        if mem::take(&mut self.focus.recheck_active) {
            self.in_each_window(|tree| tree.invalidate_active_target());
        }
        for (node, entry) in mem::take(&mut self.focus.target_flips) {
            self.apply_focus_target_flip(node, entry, false);
        }
        let events = mem::take(&mut self.focus.invalidated_events);
        if events.is_empty() {
            return;
        }
        let mut events: SmallVec<[(NodeId, EntryId); 4]> = events;
        // Attached and updated in one pass, it's told once, as Compose's invalidation set.
        if events.len() > 1 {
            let mut seen = std::collections::HashSet::with_capacity(events.len());
            events.retain(|each| seen.insert(*each));
        }
        let mut left = events;
        self.in_each_window(|tree| tree.tell_events_of_active_path(&mut left));
        let events = left;
        // Those not above the active target, or all with nothing focused.
        for (node, entry) in events {
            self.tell_event_entry(node, entry, FocusState::Inactive);
        }
    }

    fn tell_event_entry(&mut self, node: NodeId, entry: EntryId, state: FocusState) {
        let Some(index) = self.get(node).and_then(|value| {
            value.chain.iter().position(|each| {
                each.id == entry
                    && each.kinds.any(NodeKinds::FOCUS_EVENT)
                    && each.state & entry_state::ATTACHED != 0
            })
        }) else {
            return;
        };
        for (entry, delegate) in self.focus_events_of_entry(node, index) {
            self.run_focus_call(node, entry, delegate, FocusCall::Event(state));
        }
    }

    /// As `invalidateFocus`: the active target clears the focus if it can't keep it.
    fn invalidate_active_target(&mut self) {
        let Some(active) = self.focus.active else { return };
        if !self.target_is_attached(active) {
            self.clear_active_target(true, true);
            return;
        }
        if !self.fetch_focus_properties_observed(active).can_focus {
            self.clear_focus_with(true, true, FocusDirection::Exit);
        }
    }

    /// Asks again, after a write to what the active target's properties read.
    pub(crate) fn focus_properties_read_changed(&mut self, node: NodeId) {
        if self.focus.active.is_some_and(|active| active.node == node) {
            self.focus.recheck_active = true;
        }
    }

    // ---- The rest -----------------------------------------------------------------------------

    /// Focuses the first target at or below `node`'s chain's head, as a `FocusRequester` on it.
    /// Returns whether one took the focus.
    pub(crate) fn request_focus_at_or_below(
        &mut self,
        node: NodeId,
        direction: FocusDirection,
    ) -> bool {
        for target in self.targets_below_place(node, 0) {
            if self.request_focus_target(target, direction) {
                return true;
            }
        }
        false
    }

    /// Moves focus to the first target at or below `node`. Returns the actions focus listeners
    /// queued.
    pub fn request_focus(&mut self, node: NodeId) -> Vec<Action> {
        self.request_focus_at_or_below(node, FocusDirection::Enter);
        self.take_pending_actions()
    }

    /// Clears the focus, as `FocusManager.clearFocus()`.
    pub fn clear_focus(&mut self) -> Vec<Action> {
        self.clear_focus_with(false, true, FocusDirection::Exit);
        self.take_pending_actions()
    }

    /// Moves the focus into `node`, the tree moving it itself (an overlay opening or closing,
    /// inside a layout): no scrolling.
    pub(crate) fn focus_without_scrolling(&mut self, node: NodeId) {
        let moving = mem::replace(&mut self.focus.moved_by_tree, true);
        self.request_focus_at_or_below(node, FocusDirection::Enter);
        self.focus.moved_by_tree = moving;
    }

    /// What `EventContext::bring_into_view` asked of `node` (a focused node in a scrolling
    /// container is scrolled into view, as Compose's `FocusableNode`; one not placed yet, a lazy
    /// item composed ahead, once the next layout places it).
    pub(crate) fn bring_focused_into_view(&mut self, node: NodeId) {
        if self.focus.moved_by_tree {
            return;
        }
        if self.is_laid_out(node) {
            self.bring_into_view(node);
        } else {
            self.bring_into_view_after_layout = Some(node);
        }
    }

    /// Whether `node` and its ancestors are placed: its bounds are where it shows.
    pub(crate) fn is_laid_out(&self, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            let value = self.node(id);
            if !value.placed {
                return false;
            }
            current = if value.overlay { None } else { value.parent };
        }
        true
    }

    /// Asks the scrolling ancestors of `node` to show it, innermost first, each shown where
    /// the scrolling inside it will put it.
    pub fn bring_into_view(&mut self, node: NodeId) {
        let mut rect = self.bounds_in_root(node);
        // An overlay's containers are its own: the ones around its host don't show it.
        let mut ancestor = if self.node(node).overlay { None } else { self.node(node).parent };
        while let Some(id) = ancestor {
            ancestor = if self.node(id).overlay { None } else { self.node(id).parent };
            let mut layer = 0;
            for index in 0..self.node(id).chain.len() {
                let origin = self.layer_origin(id, layer.min(self.node(id).layers.len() - 1));
                let viewport =
                    self.node(id).layers[layer.min(self.node(id).layers.len() - 1)].measured;
                let local = Rect::new(
                    rect.left - origin.x,
                    rect.top - origin.y,
                    rect.right - origin.x,
                    rect.bottom - origin.y,
                );
                let is_layout = self.node(id).chain[index].kinds.any(NodeKinds::LAYOUT);
                if let Some(value) = self.get_mut(id)
                    && let Some(moved) = value.chain[index].bring_into_view(local, viewport)
                {
                    rect = Rect::new(
                        moved.left + origin.x,
                        moved.top + origin.y,
                        moved.right + origin.x,
                        moved.bottom + origin.y,
                    );
                }
                if is_layout {
                    layer += 1;
                }
            }
        }
    }

    /// Tells the focused target where the tap that focused it was.
    pub(crate) fn tell_focus_tap(&mut self, node: NodeId, tap: Offset) {
        // Refused or redirected, the tap isn't for what has the focus.
        let Some(active) =
            self.focus.active.filter(|active| self.is_at_or_below(active.node, node))
        else {
            return;
        };
        let Some(index) = self.target_index(active) else { return };
        let layer = self.node(active.node).layer_of(index);
        let origin = self.layer_origin(active.node, layer);
        let mut requests = Requests::default();
        let mut actions = mem::take(&mut self.pending_actions);
        self.with_event_context(
            active.node,
            index,
            active.delegate,
            &mut actions,
            &mut requests,
            Some(tap),
            false,
            |modifier_node, context| {
                if let Some(target) = modifier_node.as_focus_target() {
                    target.on_focus_tap(tap - origin, context);
                }
            },
        );
        actions.append(&mut self.pending_actions);
        self.pending_actions = actions;
        self.apply_focus_node_requests(requests);
    }

    /// The actions queued outside dispatch (focus lost by a recomposition, listeners attached),
    /// for the host to run after the frame.
    pub fn take_pending_actions(&mut self) -> Vec<Action> {
        self.flush_focus_invalidations();
        mem::take(&mut self.pending_actions)
    }

    /// The composite key hash of the focused child of `place` is saved, as `saveFocusedChild`.
    pub(crate) fn save_focused_child_of(&mut self, place: FocusPlace) -> bool {
        if !self.focus_state_at(place).has_focus() {
            return false;
        }
        for child in self.focus_children(place) {
            if self.focus_state_of(child).has_focus() {
                let hash = self.composite_key_hash_of(child.node);
                self.focus.saved_children.insert(place, hash);
                if let Some(target) = place {
                    self.save_focused_child_in_registry(target, hash);
                }
                return true;
            }
        }
        false
    }

    /// As `restoreFocusedChild`: the child saved, by its composite key hash (from the saveable
    /// registry, after a recreation), restores its own focused child or takes the focus.
    pub(crate) fn restore_focused_child_of(&mut self, place: FocusPlace) -> bool {
        let saved = match self.focus.saved_children.get(&place) {
            Some(saved) => Some(*saved),
            None => place.and_then(|target| self.consume_restored_focused_child(target)),
        };
        let Some(saved) = saved else { return false };
        self.focus.saved_children.insert(place, saved);
        for child in self.focus_children(place) {
            if self.composite_key_hash_of(child.node) == saved {
                return self.restore_focused_child_of(Some(child))
                    // When a request fails it tries a child: not when restoring failed, which
                    // has its own fallback.
                    || (self.fetch_focus_properties(child).can_focus
                        && self.request_focus_target(child, FocusDirection::Enter));
            }
        }
        false
    }

    /// The saveable registry's key for `target`'s saved child, as Compose's
    /// `PrevFocusedChild + compositeKeyHash`.
    fn focused_child_registry_key(&self, target: FocusTargetRef) -> u64 {
        self.composite_key_hash_of(target.node).rotate_left(13) ^ 0x7066_6300
    }

    /// The hash of where `node` is composed, as `LayoutNode.compositeKeyHash`.
    fn composite_key_hash_of(&self, node: NodeId) -> u64 {
        compose_runtime::composite_key_hash_of(self.node(node).composite_key)
    }

    /// The registry a target saves in: the one around it, else the host's.
    fn saveable_registry_of(
        &self,
        target: FocusTargetRef,
    ) -> Option<compose_runtime::SaveableStateRegistry> {
        self.node(target.node)
            .locals
            .get(&compose_runtime::LOCAL_SAVEABLE_STATE_REGISTRY)
            .or_else(|| self.saveable_registry.clone())
    }

    fn save_focused_child_in_registry(&mut self, target: FocusTargetRef, hash: u64) {
        let Some(registry) = self.saveable_registry_of(target) else { return };
        let key = self.focused_child_registry_key(target);
        let entry = registry
            .register_provider(key, move || Some(compose_runtime::SavedValue::Int(hash as i64)));
        if let Some(previous) = self.focus.registrations.insert(target, entry) {
            previous.unregister();
        }
    }

    fn consume_restored_focused_child(&mut self, target: FocusTargetRef) -> Option<u64> {
        let registry = self.saveable_registry_of(target)?;
        let key = self.focused_child_registry_key(target);
        match registry.consume_restored(key)? {
            compose_runtime::SavedValue::Int(hash) => Some(hash as u64),
            _ => None,
        }
    }

    /// `target` is gone or reset: what it saved goes, as `onDetach`.
    fn forget_saved_child_of(&mut self, target: FocusTargetRef) {
        self.focus.saved_children.remove(&Some(target));
        if let Some(entry) = self.focus.registrations.remove(&target) {
            // After the pass: a lazy item's holder saves what's registered as the item leaves,
            // its effects forgotten after its nodes are gone.
            self.focus_requests
                .side_effects
                .borrow_mut()
                .push(Box::new(move || entry.unregister()));
        }
    }
}

/// The targets of an entry out of its chain.
fn removed_entry_targets(entry: &mut ChainEntry) -> SmallVec<[EntryId; 2]> {
    let mut delegates = SmallVec::new();
    crate::delegation::dispatch_for_kind(
        &mut *entry.node,
        entry.own,
        NodeKinds::FOCUS_TARGET,
        EntryId::NO_DELEGATE,
        &mut |_, delegate| delegates.push(delegate),
    );
    delegates
}

/// The most items a focus search lays out beyond a container's bounds before it gives up
/// (leaving the container): Tab past that many unfocusable items in a row.
pub(crate) const MOST_BEYOND_STEPS: usize = 200;

/// Focus moves waiting for content a container lays out beyond its bounds, as Compose's
/// `BeyondBoundsLayout` during a focus search (D477): the moves asked meanwhile, in order, the
/// container (its node and the modifier node laying it out), and the items it laid out so far.
#[derive(Default)]
pub(crate) struct BeyondSearch {
    pub(crate) moves: std::collections::VecDeque<FocusDirection>,
    pub(crate) container: Option<(NodeId, usize)>,
    pub(crate) steps: usize,
    /// The containers that had no more to lay out in this search (D486).
    pub(crate) exhausted: Vec<(NodeId, usize)>,
    /// The focused target when the search waited: another then is a focus change from outside.
    pub(crate) focused: Option<FocusTargetRef>,
}

/// The generation of the focus: a focus change queued for application code is dropped if the
/// focus moved since, as Compose's transactions stop telling when a callback moves the focus.
pub(crate) type FocusGeneration = Rc<Cell<u64>>;
