//! `FocusProperties`, as Compose's (`focus/FocusProperties.kt`): what `Modifier.focus_properties`
//! and `FocusPropertiesModifierNode`s say of the target below them: whether it can take the
//! focus, where each direction goes from it, what happens on entering and leaving it, and its
//! rectangle for searches. Folded on demand, from the target up to the next target above.

use std::rc::Rc;

use crate::focus::{FocusPlace, FocusTargetRef};
use crate::focus_manager::FocusDirection;
use crate::focus_requester::FocusRequester;
use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement};
use crate::node_chain::EntryId;
use crate::tree::{NodeId, UiTree};
use crate::unit::Rect;

/// An `on_enter` or `on_exit` block.
pub type FocusEnterExitBlock = Rc<dyn Fn(&mut FocusEnterExitScope)>;

/// What's said of a focus target, as Compose's `FocusProperties`.
#[derive(Clone)]
pub struct FocusProperties {
    /// Whether it can take the focus. One that can't still is a target: its children can, as a
    /// group's.
    pub can_focus: bool,
    /// Where `FocusDirection::Next` goes from it (`FocusRequester::DEFAULT`: the search decides;
    /// `FocusRequester::CANCEL`: it doesn't move).
    pub next: FocusRequester,
    pub previous: FocusRequester,
    pub up: FocusRequester,
    pub down: FocusRequester,
    pub left: FocusRequester,
    pub right: FocusRequester,
    /// Where `Left` goes from it in a left-to-right layout, `Right` in a right-to-left one,
    /// before `left` and `right`.
    pub start: FocusRequester,
    /// Where `Right` goes from it in a left-to-right layout, `Left` in a right-to-left one,
    /// before `left` and `right`.
    pub end: FocusRequester,
    /// Runs when the focus is entering it (from outside, to it or below it): it may cancel or
    /// redirect the change.
    pub on_enter: Option<FocusEnterExitBlock>,
    /// Runs when the focus is leaving it (it had it, or something below it had): it may cancel or
    /// redirect the change.
    pub on_exit: Option<FocusEnterExitBlock>,
    /// Its rectangle in its own coordinates for searches, its bounds if unset
    /// (`FocusProperties::UNSET_FOCUS_RECT`).
    pub focus_rect: Rect,
}

impl FocusProperties {
    /// What `focus_rect` is until it's set, as `UnsetFocusRect`.
    pub const UNSET_FOCUS_RECT: Rect = Rect::new(f32::NAN, f32::NAN, f32::NAN, f32::NAN);

    /// Sets `on_enter`.
    pub fn set_on_enter(&mut self, block: impl Fn(&mut FocusEnterExitScope) + 'static) {
        self.on_enter = Some(Rc::new(block));
    }

    /// Sets `on_exit`.
    pub fn set_on_exit(&mut self, block: impl Fn(&mut FocusEnterExitScope) + 'static) {
        self.on_exit = Some(Rc::new(block));
    }
}

impl Default for FocusProperties {
    fn default() -> FocusProperties {
        FocusProperties {
            can_focus: true,
            next: FocusRequester::DEFAULT,
            previous: FocusRequester::DEFAULT,
            up: FocusRequester::DEFAULT,
            down: FocusRequester::DEFAULT,
            left: FocusRequester::DEFAULT,
            right: FocusRequester::DEFAULT,
            start: FocusRequester::DEFAULT,
            end: FocusRequester::DEFAULT,
            on_enter: None,
            on_exit: None,
            focus_rect: FocusProperties::UNSET_FOCUS_RECT,
        }
    }
}

/// What an `on_enter` or `on_exit` block is given, as Compose's `FocusEnterExitScope`. The tree is
/// in a transaction meanwhile: `FocusRequester`s and the `FocusManager` asked here act as the block
/// returns, and `request_focus` here acts at once, with its result (D1270).
pub struct FocusEnterExitScope {
    tree: u32,
    requested_focus_direction: FocusDirection,
    canceled: bool,
}

impl FocusEnterExitScope {
    pub(crate) fn new(tree: u32, direction: FocusDirection) -> FocusEnterExitScope {
        FocusEnterExitScope { tree, requested_focus_direction: direction, canceled: false }
    }

    /// The direction the focus is moving in: `Enter` or `Exit` for a request or a clear.
    pub fn requested_focus_direction(&self) -> FocusDirection {
        self.requested_focus_direction
    }

    /// Cancels the change, as `cancelFocusChange`.
    pub fn cancel_focus_change(&mut self) {
        self.canceled = true;
    }

    pub(crate) fn is_canceled(&self) -> bool {
        self.canceled
    }

    /// The restorer's: `restoreFocusedChild` of what's at or below entry `entry` of `node`.
    fn restore_focused_child_at(&mut self, node: NodeId, entry: EntryId) -> bool {
        self.on_tree(|tree| {
            tree.self_and_children_targets(node, entry)
                .into_iter()
                .any(|target| tree.restore_focused_child_of(Some(target)))
        })
    }

    /// `f` on the tree set aside for this block.
    fn on_tree(&self, f: impl FnOnce(&mut UiTree) -> bool) -> bool {
        let mut f = Some(f);
        let mut done = false;
        crate::node_context::with_free_tree(self.tree, &mut |tree: &mut UiTree| {
            if let Some(f) = f.take() {
                done = f(tree);
            }
        });
        done
    }

    /// The restorer's: `saveFocusedChild` of what's at or below entry `entry` of `node`.
    fn save_focused_child_at(&mut self, node: NodeId, entry: EntryId) -> bool {
        self.on_tree(|tree| {
            tree.self_and_children_targets(node, entry)
                .into_iter()
                .any(|target| tree.save_focused_child_of(Some(target)))
        })
    }
}

/// A node saying what the focus target below it is, as `FocusPropertiesModifierNode`: asked each
/// time they're needed. `NodeContext::invalidate_focus_properties` tells the tree they changed.
pub trait FocusPropertiesModifierNode {
    fn apply_focus_properties(&self, properties: &mut FocusProperties);
}

impl Modifier {
    /// Says what the focus target after it in the chain, or below it, is, as
    /// `Modifier.focusProperties`: the outermost wins. The block may read state: a write making
    /// the focused target unable to keep the focus clears it.
    pub fn focus_properties(self, scope: impl Fn(&mut FocusProperties) + 'static) -> Modifier {
        self.then_custom(FocusPropertiesElement(Rc::new(scope)))
    }

    /// Restores the focus to the child that last had it when it enters this group, as
    /// `Modifier.focusRestorer`: to `fallback` if there's none to restore
    /// (`FocusRequester::DEFAULT`: what an `Enter` would pick; `CANCEL`: it doesn't enter).
    pub fn focus_restorer(self, fallback: FocusRequester) -> Modifier {
        self.then_custom(FocusRestorerElement(fallback))
    }
}

type PropertiesScope = Rc<dyn Fn(&mut FocusProperties)>;

struct FocusPropertiesElement(PropertiesScope);

impl ModifierNodeElement for FocusPropertiesElement {
    type Node = FocusPropertiesNode;
    fn create(&self) -> FocusPropertiesNode {
        FocusPropertiesNode(self.0.clone())
    }
    fn update(&self, node: &mut FocusPropertiesNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    fn name(&self) -> &'static str {
        "focus_properties"
    }
}

struct FocusPropertiesNode(PropertiesScope);

impl ModifierNode for FocusPropertiesNode {
    fn as_focus_properties(&mut self) -> Option<&mut dyn FocusPropertiesModifierNode> {
        Some(self)
    }
}

impl FocusPropertiesModifierNode for FocusPropertiesNode {
    fn apply_focus_properties(&self, properties: &mut FocusProperties) {
        (self.0)(properties);
    }
}

struct FocusRestorerElement(FocusRequester);

impl ModifierNodeElement for FocusRestorerElement {
    type Node = FocusRestorerNode;
    fn create(&self) -> FocusRestorerNode {
        FocusRestorerNode {
            fallback: Rc::new(std::cell::Cell::new(self.0)),
            on_enter: None,
            on_exit: None,
        }
    }
    fn update(&self, node: &mut FocusRestorerNode) {
        node.fallback.set(self.0);
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn name(&self) -> &'static str {
        "focus_restorer"
    }
}

/// As `FocusRestorerNode`: on exit saves the child with the focus, on enter restores it.
struct FocusRestorerNode {
    fallback: Rc<std::cell::Cell<FocusRequester>>,
    /// Its blocks, made when it's attached: they name its place in the tree.
    on_enter: Option<FocusEnterExitBlock>,
    on_exit: Option<FocusEnterExitBlock>,
}

impl ModifierNode for FocusRestorerNode {
    fn as_focus_properties(&mut self) -> Option<&mut dyn FocusPropertiesModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        let (node, entry) = (context.node, context.entry);
        let fallback = self.fallback.clone();
        self.on_enter = Some(Rc::new(move |scope| {
            // Restoring requests the focus, which redirects the change; failing, by default the
            // focus enters this group.
            let fallback = fallback.get();
            if !scope.restore_focused_child_at(node, entry) && fallback != FocusRequester::DEFAULT {
                if fallback == FocusRequester::CANCEL {
                    scope.cancel_focus_change();
                } else {
                    fallback.request_focus();
                }
            }
        }));
        self.on_exit = Some(Rc::new(move |scope| {
            scope.save_focused_child_at(node, entry);
        }));
    }
    fn on_detach(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.on_enter = None;
        self.on_exit = None;
    }
}

impl FocusPropertiesModifierNode for FocusRestorerNode {
    fn apply_focus_properties(&self, properties: &mut FocusProperties) {
        properties.on_enter = self.on_enter.clone();
        properties.on_exit = self.on_exit.clone();
    }
}

impl UiTree {
    /// The targets of a node (its own or its delegates') and the first ones down each branch
    /// below it, as `visitSelfAndChildren(Nodes.FocusTarget)`. `entry` names a chain entry or a
    /// delegate.
    pub(crate) fn self_and_children_targets(
        &mut self,
        node: NodeId,
        entry: EntryId,
    ) -> smallvec::SmallVec<[FocusTargetRef; 8]> {
        let mut targets = smallvec::SmallVec::new();
        let Some(index) = self.entry_index(node, entry) else { return targets };
        let chain_entry = &mut self.node_mut(node).chain[index];
        let id = chain_entry.id;
        if chain_entry.kinds.any(crate::node_chain::NodeKinds::FOCUS_TARGET) {
            if id == entry {
                crate::delegation::dispatch_for_kind(
                    &mut *chain_entry.node,
                    chain_entry.own,
                    crate::node_chain::NodeKinds::FOCUS_TARGET,
                    EntryId::NO_DELEGATE,
                    &mut |_, delegate| targets.push(FocusTargetRef { node, entry: id, delegate }),
                );
            } else if let Some(delegate) =
                crate::delegation::find_delegate(&mut *chain_entry.node, entry)
                && delegate.kinds().any(crate::node_chain::NodeKinds::FOCUS_TARGET)
            {
                crate::delegation::dispatch_delegate_for_kind(
                    delegate,
                    crate::node_chain::NodeKinds::FOCUS_TARGET,
                    &mut |_, delegate| targets.push(FocusTargetRef { node, entry: id, delegate }),
                );
            }
        }
        targets.extend(self.targets_below_place(node, index + 1));
        targets
    }

    /// `place`'s children's: the target at `place`, for the public `focused_rect`.
    pub(crate) fn focused_rect_of(&mut self, place: FocusPlace) -> Option<Rect> {
        let target = place?;
        let state = self.focus_state_of(target);
        if !state.has_focus() {
            return None;
        }
        let index = self.target_index(target)?;
        let value = self.node(target.node);
        let layer = value.layer_of(index).min(value.layers.len() - 1);
        let origin = self.layer_origin(target.node, layer);
        let active = self.focus.active?;
        let rect = self.focus_rect_of(active);
        Some(Rect::new(
            rect.left - origin.x,
            rect.top - origin.y,
            rect.right - origin.x,
            rect.bottom - origin.y,
        ))
    }
}
