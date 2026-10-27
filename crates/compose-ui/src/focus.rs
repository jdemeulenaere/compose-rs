//! Focus (docs/06-ui.md, "Focus"): one focused target, as a node and the index of its focus
//! target in the node's chain; Tab order; and `on_focus_changed` listeners, told whether their
//! node or one below it has focus (Compose's `FocusState.hasFocus`), including once when they
//! attach.

use crate::focus_manager::FocusDirection;
use smallvec::SmallVec;

use crate::input::{Action, EventContext, Requests};
use crate::node_chain::NodeKinds;
use crate::nodes::OnFocusChangedNode;
use crate::tree::{NodeId, UiTree};
use crate::unit::Rect;

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
        // Content kept for reuse takes no focus (nor keys, which go to the focused node).
        if !value.kinds.any(NodeKinds::FOCUS_TARGET) || value.deactivated {
            return None;
        }
        (0..value.chain.len()).find(|&index| {
            value.chain[index].kinds.any(NodeKinds::FOCUS_TARGET)
                && value.chain[index]
                    .node
                    .as_focus_target()
                    .is_some_and(|target| target.can_focus())
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
        self.focus_into(node, true, actions);
    }

    /// Focuses the first focus target at or below `node`; with `scroll`, scrolled into view.
    pub(crate) fn focus_into(&mut self, node: NodeId, scroll: bool, actions: &mut Vec<Action>) {
        let Some(node) = self.first_focus_target_at_or_below(node) else { return };
        if self.focused() == Some(node) {
            return;
        }
        let Some(index) = self.focus_target_index(node) else { return };
        self.clear_focus_into(actions);
        self.focus = Some(Focus { node, index: index as u16 });
        self.tell_target(node, index, true, actions);
        self.focus_listeners_dirty = true;
        // A focused node in a scrolling container is scrolled into view, as Compose's; one not
        // placed yet (a lazy item composed ahead) once the next layout places it.
        if scroll && !self.is_laid_out(node) {
            self.bring_into_view_after_layout = Some(node);
        } else if scroll {
            self.bring_into_view(node);
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
                    && let Some(moved) = value.chain[index].node.bring_into_view(local, viewport)
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
        let density = self.density;
        let view_configuration = self.view_configuration();
        if let Some((value, (text_measurer, clipboard))) = self.node_and_measurer(node)
            && let Some(target) =
                value.chain.get_mut(index).and_then(|entry| entry.node.as_focus_target())
        {
            let mut context = EventContext {
                node,
                actions,
                requests: &mut requests,
                text_measurer,
                clipboard,
                pointer_position: None,
                density,
                view_configuration,
                layout_direction: value.layout_direction,
                waiting_parent: false,
            };
            target.on_focus_changed(focused, &mut context);
        }
        // Not its focus requests: a target answering focus with focus would loop.
        requests.focus = None;
        requests.clear_focus = false;
        for node in requests.draw {
            self.invalidate_draw_of(node);
        }
        self.input_method_reset |= requests.reset_input_method;
        for node in requests.measure {
            self.invalidate_measure(node);
        }
        // Its selection requests: a selection container losing the focus clears its selection.
        for request in requests.selection {
            self.handle_selection(request);
        }
    }

    /// Tells the focused target where the tap that focused it was.
    pub(crate) fn tell_focus_tap(&mut self, tap: crate::unit::Offset, actions: &mut Vec<Action>) {
        let Some(focus) = self.focus else { return };
        let layer = {
            let value = self.node(focus.node);
            value.layer_of(focus.index as usize)
        };
        let origin = self.layer_origin(focus.node, layer);
        let mut requests = Requests::default();
        let density = self.density;
        let view_configuration = self.view_configuration();
        if let Some((value, (text_measurer, clipboard))) = self.node_and_measurer(focus.node)
            && let Some(target) = value
                .chain
                .get_mut(focus.index as usize)
                .and_then(|entry| entry.node.as_focus_target())
        {
            let mut context = EventContext {
                node: focus.node,
                actions,
                requests: &mut requests,
                text_measurer,
                clipboard,
                pointer_position: Some(tap),
                density,
                view_configuration,
                layout_direction: value.layout_direction,
                waiting_parent: false,
            };
            target.on_focus_tap(tap - origin, &mut context);
        }
        for node in requests.draw {
            self.invalidate_draw_of(node);
        }
        self.input_method_reset |= requests.reset_input_method;
        // A field asking for the on-screen keyboard, as on a tap (the host wakes for it).
        if let Some(show) = requests.software_keyboard {
            self.focus_requests.software_keyboard.set(Some(show));
            self.focus_requests.wake();
        }
        for node in requests.measure {
            self.invalidate_measure(node);
        }
        // Its selection requests: a selection container losing the focus clears its selection.
        for request in requests.selection {
            self.handle_selection(request);
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
            value.chain.get(index).is_some_and(|entry| entry.kinds.any(NodeKinds::FOCUS_TARGET))
        });
        match target {
            Some(index)
                if value.chain[index]
                    .node
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
            // Deactivated content (kept for reuse) reports nothing: reused, it reports anew.
            if value.deactivated {
                continue;
            }
            if value.kinds.any(NodeKinds::FOCUS_EVENT) {
                let below = ancestors.iter().skip(1).any(|ancestor| *ancestor == node);
                for index in 0..value.chain.len() {
                    if !value.chain[index].kinds.any(NodeKinds::FOCUS_EVENT) {
                        continue;
                    }
                    let has_focus = below
                        || focus.is_some_and(|focus| {
                            focus.node == node && focus.index as usize > index
                        });
                    if let Some(listener) =
                        value.chain[index].node.downcast_mut::<OnFocusChangedNode>()
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

    /// The nodes that can take focus, in tree order: what Tab moves through. While an overlay
    /// that took the focus shows (a dialog, a menu), only its own, as a window of its own; the
    /// overlays that can't take it (a popup) have none.
    pub fn focus_order(&mut self) -> Vec<NodeId> {
        let modal = self.modal_overlay();
        let start = modal.unwrap_or(self.root_id());
        let mut order = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if !self.node(node).placed {
                continue;
            }
            if node != start && self.node(node).overlay {
                let takes_focus =
                    self.overlay_node(node).is_some_and(|overlay| overlay.focus_on_open);
                if !takes_focus || modal.is_some() {
                    continue;
                }
            }
            // A modal overlay itself is no stop: Tab wraps from its last focusable to its first,
            // as a window's root isn't one.
            if self.focus_target_index(node).is_some() && !(modal.is_some() && node == start) {
                order.push(node);
            }
            stack.extend(self.node(node).children.iter().rev().map(|child| child.node));
        }
        order
    }

    /// The topmost overlay taking the focus (a dialog, a focusable popup): while it shows, the
    /// rest of the window takes no focus nor keys.
    pub(crate) fn modal_overlay(&self) -> Option<NodeId> {
        self.overlays.iter().rev().copied().find(|&node| {
            self.get(node).is_some_and(|value| value.overlay && value.placed)
                && self.overlay_node(node).is_some_and(|overlay| overlay.focus_on_open)
        })
    }

    /// Moves focus to the next (or previous) focusable node, wrapping around, as Tab does
    /// (laying out content beyond a lazy list's bounds first, if the focus would leave it:
    /// `move_focus_in`).
    pub fn move_focus(&mut self, forward: bool) -> Vec<Action> {
        let direction = if forward { FocusDirection::Next } else { FocusDirection::Previous };
        self.move_focus_in(direction).1
    }

    /// The node Tab (`forward`) or Shift+Tab moves the focus to, and whether it wrapped around.
    pub(crate) fn tab_target(&mut self, forward: bool) -> Option<(NodeId, bool)> {
        let order = self.focus_order();
        if order.is_empty() {
            return None;
        }
        let current =
            self.focused().and_then(|focused| order.iter().position(|&node| node == focused));
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => order.len() - 1,
            (Some(index), true) => (index + 1) % order.len(),
            (Some(index), false) => (index + order.len() - 1) % order.len(),
        };
        // Entering a container that shows part of its content (a scrolled list), the first
        // node in view, as Compose's search skips what isn't placed: the items a lazy list
        // keeps placed beyond its viewport are stepped over unless the focus is inside it.
        let focused = self.focused();
        let step = |index: usize| {
            if forward {
                (index + 1) % order.len()
            } else {
                (index + order.len() - 1) % order.len()
            }
        };
        let mut candidate = next;
        let next = loop {
            if self.reachable_from(order[candidate], focused) {
                break candidate;
            }
            candidate = step(candidate);
            if candidate == next {
                break next;
            }
        };
        let wrapped = current.is_some_and(|current| (next <= current) == forward);
        Some((order[next], wrapped))
    }

    /// The innermost ancestor of `node` (or itself) laying out content beyond its bounds for
    /// focus searches (a lazy list), and the index of the modifier node doing it.
    pub(crate) fn beyond_bounds_container(&self, node: NodeId) -> Option<(NodeId, usize)> {
        let mut current = Some(node);
        while let Some(id) = current {
            let value = self.node(id);
            if let Some(index) = value
                .chain
                .iter()
                .position(|entry| entry.kinds.any(NodeKinds::BEYOND_BOUNDS_LAYOUT))
            {
                return Some((id, index));
            }
            current = if value.overlay { None } else { value.parent };
        }
        None
    }

    /// Asks `container` (its modifier node `index`) to lay out one more item beyond its bounds
    /// going `direction`, for the search: the move waits for it, made again at the next layout
    /// (D477). Returns whether it waits (`false`: nothing more that way, or the search laid out
    /// `MOST_BEYOND_STEPS` items already).
    pub(crate) fn lay_out_beyond(
        &mut self,
        container: NodeId,
        index: usize,
        direction: FocusDirection,
    ) -> bool {
        // A container that had no more that way has none for the rest of the search (D486).
        if self.beyond_search.exhausted.contains(&(container, index))
            || self.beyond_search.steps >= MOST_BEYOND_STEPS
        {
            return false;
        }
        if let Some(active) = self.beyond_search.container
            && active != (container, index)
        {
            // Another container's turn: the items the last one laid out go (D483); the count
            // goes on, so the search ends within `MOST_BEYOND_STEPS` items whatever the tree.
            self.beyond_search.container = None;
            if let Some(value) = self.get_mut(active.0)
                && active.1 < value.chain.len()
            {
                value.chain[active.1].node.end_beyond_bounds();
            }
        }
        let layout_direction = self.node(container).layout_direction;
        let more = self.get_mut(container).is_some_and(|value| {
            index < value.chain.len()
                && value.chain[index].node.lay_out_beyond_bounds(direction, layout_direction)
        });
        if !more {
            self.beyond_search.exhausted.push((container, index));
            return false;
        }
        self.beyond_search.container = Some((container, index));
        self.beyond_search.steps += 1;
        self.beyond_search.moves.push_back(direction);
        self.beyond_search.focused = self.focused();
        true
    }

    /// The beyond-bounds container around `node`'s, the next one out.
    pub(crate) fn outer_beyond_bounds_container(&self, node: NodeId) -> Option<(NodeId, usize)> {
        let value = self.node(node);
        if value.overlay {
            return None;
        }
        self.beyond_bounds_container(value.parent?)
    }

    /// The search is over: what was laid out beyond the bounds for it goes.
    pub(crate) fn end_beyond_bounds(&mut self) {
        self.beyond_search.steps = 0;
        self.beyond_search.exhausted.clear();
        if let Some((container, index)) = self.beyond_search.container.take()
            && let Some(value) = self.get_mut(container)
            && index < value.chain.len()
        {
            value.chain[index].node.end_beyond_bounds();
        }
    }

    /// Whether focus moves wait for content laid out beyond a container's bounds.
    pub fn has_beyond_bounds_moves(&self) -> bool {
        !self.beyond_search.moves.is_empty()
    }

    /// Makes the focus moves that waited for content beyond a container's bounds, now laid
    /// out (each may wait again, for one more item).
    pub fn run_beyond_bounds_moves(&mut self) -> Vec<Action> {
        let moves = std::mem::take(&mut self.beyond_search.moves);
        // The focus moved meanwhile (a click, a `FocusRequester`): that wins, the search and
        // the moves waiting go (D495).
        if !moves.is_empty() && self.focused() != self.beyond_search.focused {
            self.end_beyond_bounds();
            return Vec::new();
        }
        let mut actions = Vec::new();
        for direction in moves {
            actions.extend(self.move_focus_in(direction).1);
        }
        actions
    }

    /// Moves the focus to `target`, done searching.
    pub(crate) fn focus_found(&mut self, target: NodeId) -> Vec<Action> {
        let mut actions = Vec::new();
        self.request_focus_into(target, &mut actions);
        self.update_focus_listeners(&mut actions);
        actions
    }

    /// Whether Tab may stop at `node` coming from `focused`: it's within the bounds of each of
    /// its ancestors that clip their content, or the ancestors it's outside of (a list it's
    /// scrolled out of) hold the focus too.
    fn reachable_from(&self, node: NodeId, focused: Option<NodeId>) -> bool {
        let bounds = self.bounds_in_root(node);
        let mut ancestor = self.parent_of(node);
        while let Some(container) = ancestor {
            if self.node(container).overlay {
                break;
            }
            // Only a container clipping its content (a scroll's viewport, `clip`) hides it.
            let value = self.node(container);
            let size = value.layers[0].measured.into();
            let clips = value
                .chain
                .iter()
                .any(|entry| entry.node.hit_clip_at(size, self.density).is_some());
            let outer = self.bounds_in_root(container);
            let outside = clips
                && (bounds.right <= outer.left
                    || bounds.left >= outer.right
                    || bounds.bottom <= outer.top
                    || bounds.top >= outer.bottom);
            if outside && !focused.is_some_and(|focused| self.is_at_or_below(focused, container)) {
                return false;
            }
            ancestor = self.parent_of(container);
        }
        true
    }

    /// The actions queued outside dispatch (focus lost by a recomposition, listeners attached),
    /// for the host to run after the frame.
    pub fn take_pending_actions(&mut self) -> Vec<Action> {
        let mut actions = std::mem::take(&mut self.pending_actions);
        self.update_focus_listeners(&mut actions);
        actions
    }
}

/// The most items a focus search lays out beyond a container's bounds before it gives up
/// (leaving the container): Tab past that many unfocusable items in a row.
const MOST_BEYOND_STEPS: usize = 200;

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
    /// The focused node when the search waited: another then is a focus change from outside.
    pub(crate) focused: Option<NodeId>,
}
