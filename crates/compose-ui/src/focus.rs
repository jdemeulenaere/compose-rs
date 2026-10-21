//! Focus (docs/06-ui.md, "Focus"): one focused target, as a node and the index of its focus
//! target in the node's chain; Tab order; and `on_focus_changed` listeners, told whether their
//! node or one below it has focus (Compose's `FocusState.hasFocus`), including once when they
//! attach.

use smallvec::SmallVec;

use crate::input::{Action, EventContext, Requests};
use crate::nodes::OnFocusChangedNode;
use crate::tree::{NodeId, UiTree, capability};

/// The focused target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Focus {
    pub(crate) node: NodeId,
    pub(crate) index: u16,
}

impl UiTree {
    /// The focused node.
    pub fn focused(&self) -> Option<NodeId> {
        self.focus.map(|focus| focus.node)
    }

    /// The chain index of the node's first focus target that can take focus.
    pub(crate) fn focus_target_index(&mut self, node: NodeId) -> Option<usize> {
        let value = self.get_mut(node)?;
        if value.all_capabilities & capability::FOCUS == 0 {
            return None;
        }
        (0..value.chain.len()).find(|&index| {
            value.capabilities[index] & capability::FOCUS != 0
                && value.chain[index].as_focus_target().is_some_and(|target| target.can_focus())
        })
    }

    /// Moves focus to `node`'s focus target. Returns the actions focus listeners queued.
    pub fn request_focus(&mut self, node: NodeId) -> Vec<Action> {
        let mut actions = Vec::new();
        self.request_focus_into(node, &mut actions);
        self.update_focus_listeners(&mut actions);
        actions
    }

    pub fn clear_focus(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        self.clear_focus_into(&mut actions);
        self.update_focus_listeners(&mut actions);
        actions
    }

    /// Focuses `node`'s focus target or, if it has none, the first one below it, as Compose's
    /// `FocusRequester.requestFocus()`.
    pub(crate) fn request_focus_into(&mut self, node: NodeId, actions: &mut Vec<Action>) {
        let Some(node) = self.first_focus_target_at_or_below(node) else { return };
        if self.focused() == Some(node) {
            return;
        }
        let Some(index) = self.focus_target_index(node) else { return };
        self.clear_focus_into(actions);
        self.focus = Some(Focus { node, index: index as u16 });
        self.tell_target(node, index, true, actions);
        self.focus_listeners_dirty = true;
    }

    pub(crate) fn clear_focus_into(&mut self, actions: &mut Vec<Action>) {
        if let Some(focus) = self.focus.take() {
            self.tell_target(focus.node, focus.index as usize, false, actions);
            self.focus_listeners_dirty = true;
        }
    }

    /// Tells a focus target it gained or lost focus, applying what it asks for.
    fn tell_target(
        &mut self,
        node: NodeId,
        index: usize,
        focused: bool,
        actions: &mut Vec<Action>,
    ) {
        let mut requests = Requests::default();
        if let Some((value, (text_measurer, clipboard))) = self.node_and_measurer(node)
            && let Some(target) =
                value.chain.get_mut(index).and_then(|entry| entry.as_focus_target())
        {
            let mut context = EventContext {
                node,
                actions,
                requests: &mut requests,
                text_measurer,
                clipboard,
                pointer_position: None,
            };
            target.on_focus_changed(focused, &mut context);
        }
        // Not its focus requests: a target answering focus with focus would loop.
        requests.focus = None;
        requests.clear_focus = false;
        self.needs_draw |= requests.draw;
        self.input_method_reset |= requests.reset_input_method;
        for node in requests.measure {
            self.invalidate_measure(node);
        }
    }

    /// Tells the focused target where the tap that focused it was.
    pub(crate) fn tell_focus_tap(&mut self, tap: crate::unit::Offset, actions: &mut Vec<Action>) {
        let Some(focus) = self.focus else { return };
        let layer = {
            let value = self.node(focus.node);
            value.capabilities[..focus.index as usize]
                .iter()
                .filter(|bits| **bits & capability::LAYOUT != 0)
                .count()
        };
        let origin = self.layer_origin(focus.node, layer);
        let mut requests = Requests::default();
        if let Some((value, (text_measurer, clipboard))) = self.node_and_measurer(focus.node)
            && let Some(target) =
                value.chain.get_mut(focus.index as usize).and_then(|entry| entry.as_focus_target())
        {
            let mut context = EventContext {
                node: focus.node,
                actions,
                requests: &mut requests,
                text_measurer,
                clipboard,
                pointer_position: Some(tap),
            };
            target.on_focus_tap(tap - origin, &mut context);
        }
        self.needs_draw |= requests.draw;
        self.input_method_reset |= requests.reset_input_method;
        for node in requests.measure {
            self.invalidate_measure(node);
        }
    }

    /// After `node`'s chain changed: follows the focused target to its new index (`matched`
    /// maps new indices to old ones), and drops the focus if the target is gone or can't take
    /// focus any more, telling it (Compose clears focus then). Actions go to the pending ones,
    /// run after the frame.
    pub(crate) fn revalidate_focus(&mut self, node: NodeId, matched: Option<&[Option<usize>]>) {
        let Some(focus) = self.focus else { return };
        if focus.node != node {
            return;
        }
        let index = match matched {
            Some(matched) => matched.iter().position(|old| *old == Some(focus.index as usize)),
            None => Some(focus.index as usize),
        };
        let value = self.node_mut(node);
        let target = index.filter(|&index| {
            value.capabilities.get(index).is_some_and(|bits| bits & capability::FOCUS != 0)
        });
        match target {
            Some(index)
                if value.chain[index]
                    .as_focus_target()
                    .is_some_and(|target| target.can_focus()) =>
            {
                self.focus = Some(Focus { node, index: index as u16 });
            }
            Some(index) => {
                self.focus = None;
                let mut actions = std::mem::take(&mut self.pending_actions);
                self.tell_target(node, index, false, &mut actions);
                self.pending_actions = actions;
                self.focus_listeners_dirty = true;
            }
            None => {
                self.focus = None;
                self.focus_listeners_dirty = true;
            }
        }
    }

    /// Tells every `on_focus_changed` listener whose state changed (or that is new) whether its
    /// node, or a node below it, has focus.
    pub(crate) fn update_focus_listeners(&mut self, actions: &mut Vec<Action>) {
        if !self.focus_listeners_dirty {
            return;
        }
        self.focus_listeners_dirty = false;
        // The focused node and its ancestors.
        let mut ancestors: SmallVec<[NodeId; 16]> = SmallVec::new();
        let mut current = self.focused();
        while let Some(node) = current {
            ancestors.push(node);
            current = self.get(node).and_then(|value| value.parent);
        }
        let focus = self.focus;
        let mut stack = vec![self.root_id()];
        while let Some(node) = stack.pop() {
            let value = self.node_mut(node);
            if value.all_capabilities & capability::FOCUS_EVENT != 0 {
                let below = ancestors.iter().skip(1).any(|ancestor| *ancestor == node);
                for index in 0..value.chain.len() {
                    if value.capabilities[index] & capability::FOCUS_EVENT == 0 {
                        continue;
                    }
                    let has_focus = below
                        || focus.is_some_and(|focus| {
                            focus.node == node && focus.index as usize > index
                        });
                    if let Some(listener) = value.chain[index].downcast_mut::<OnFocusChangedNode>()
                        && listener.last != Some(has_focus)
                    {
                        listener.last = Some(has_focus);
                        actions.push(Action::CallWithBool(listener.callback.clone(), has_focus));
                    }
                }
            }
            stack.extend(value.children.iter().map(|child| child.node));
        }
    }

    fn first_focus_target_at_or_below(&mut self, node: NodeId) -> Option<NodeId> {
        let mut stack = vec![node];
        while let Some(current) = stack.pop() {
            if self.focus_target_index(current).is_some() {
                return Some(current);
            }
            stack.extend(self.get(current)?.children.iter().rev().map(|child| child.node));
        }
        None
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

    /// The nodes that can take focus, in tree order: what Tab moves through.
    pub fn focus_order(&mut self) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut stack = vec![self.root_id()];
        while let Some(node) = stack.pop() {
            if !self.node(node).placed {
                continue;
            }
            if self.focus_target_index(node).is_some() {
                order.push(node);
            }
            stack.extend(self.node(node).children.iter().rev().map(|child| child.node));
        }
        order
    }

    /// Moves focus to the next (or previous) focusable node, wrapping around, as Tab does.
    pub fn move_focus(&mut self, forward: bool) -> Vec<Action> {
        let order = self.focus_order();
        let mut actions = Vec::new();
        if order.is_empty() {
            return actions;
        }
        let current =
            self.focused().and_then(|focused| order.iter().position(|&node| node == focused));
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => order.len() - 1,
            (Some(index), true) => (index + 1) % order.len(),
            (Some(index), false) => (index + order.len() - 1) % order.len(),
        };
        self.request_focus_into(order[next], &mut actions);
        self.update_focus_listeners(&mut actions);
        actions
    }

    /// The actions queued outside dispatch (focus lost by a recomposition, listeners attached),
    /// for the host to run after the frame.
    pub fn take_pending_actions(&mut self) -> Vec<Action> {
        let mut actions = std::mem::take(&mut self.pending_actions);
        self.update_focus_listeners(&mut actions);
        actions
    }
}
