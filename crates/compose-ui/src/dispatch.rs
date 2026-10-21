//! Input dispatch on the tree: pointer hit testing and passes, focus, and key and text events
//! to the focused node (docs/06-ui.md, "Input, focus and semantics").

use smallvec::SmallVec;

use crate::input::{
    Action, EventContext, Key, KeyEvent, KeyEventType, PointerEvent, PointerEventPass,
    PointerEventType, Requests,
};
use crate::tree::{NodeId, UiTree, capability};
use crate::unit::{IntSize, Offset};

/// A pointer input node on a hit path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct HitEntry {
    pub(crate) node: NodeId,
    pub(crate) index: u16,
    pub(crate) layer: u16,
    pub(crate) version: u32,
}

fn contains(size: IntSize, point: Offset) -> bool {
    point.x >= 0.0 && point.y >= 0.0 && point.x < size.width as f32 && point.y < size.height as f32
}

impl UiTree {
    /// The pointer input nodes under `position` (in the root), outermost first: the topmost
    /// child hit at each level, as Compose's hit testing.
    pub(crate) fn hit_test(&mut self, position: Offset) -> Vec<HitEntry> {
        let mut path = Vec::new();
        self.hit_node(self.root_id(), position, &mut path);
        path
    }

    fn hit_node(&mut self, node: NodeId, point: Offset, path: &mut Vec<HitEntry>) -> bool {
        let value = self.node(node);
        if !value.placed {
            return false;
        }
        let offset = value.layers[0].offset;
        let local = point - Offset::new(offset.x as f32, offset.y as f32);
        if !contains(value.layers[0].measured, local) {
            return false;
        }
        let start = path.len();
        let mut origin = Offset::ZERO;
        let mut layer = 0usize;
        let mut origins: SmallVec<[Offset; 4]> = SmallVec::new();
        for (index, entry) in value.layers.iter().enumerate() {
            if index > 0 {
                origin = origin + Offset::new(entry.offset.x as f32, entry.offset.y as f32);
            }
            origins.push(origin);
        }
        if value.all_capabilities & capability::POINTER != 0 {
            for index in 0..value.chain.len() {
                let bits = value.capabilities[index];
                if bits & capability::POINTER != 0
                    && contains(value.layers[layer].measured, local - origins[layer])
                {
                    path.push(HitEntry {
                        node,
                        index: index as u16,
                        layer: layer as u16,
                        version: value.chain_version,
                    });
                }
                if bits & capability::LAYOUT != 0 {
                    layer += 1;
                }
            }
        }
        let content = local - *origins.last().expect("a layer");
        let count = value.children.len();
        for index in (0..count).rev() {
            let child = self.node(node).children[index].node;
            if self.hit_node(child, content, path) {
                break;
            }
        }
        path.len() > start
    }

    /// Dispatches a pointer event at `position` in the root. A press captures its hit path:
    /// the pointer's moves and its release go to the same nodes. Returns the actions handlers
    /// queued, to run once the tree is free.
    pub fn dispatch_pointer(&mut self, mut event: PointerEvent) -> Vec<Action> {
        let position = event.position;
        let captured = self.captures.iter().position(|(pointer, _)| *pointer == event.pointer);
        let path = match (event.kind, captured) {
            (PointerEventType::Press, captured) => {
                if let Some(captured) = captured {
                    self.captures.remove(captured);
                }
                let path = self.hit_test(position);
                self.captures.push((event.pointer, path.clone()));
                path
            }
            (PointerEventType::Release, Some(captured)) => self.captures.remove(captured).1,
            (PointerEventType::Move, Some(captured)) => self.captures[captured].1.clone(),
            _ => self.hit_test(position),
        };
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        for pass in [PointerEventPass::Initial, PointerEventPass::Main, PointerEventPass::Final] {
            let mut deliver = |tree: &mut UiTree, entry: &HitEntry| {
                if tree.get(entry.node).is_none_or(|value| value.chain_version != entry.version) {
                    return;
                }
                let origin = tree.layer_origin(entry.node, entry.layer as usize);
                let value = tree.node_mut(entry.node);
                let bounds = value.layers[entry.layer as usize].measured;
                event.position = position - origin;
                let mut context = EventContext {
                    node: entry.node,
                    actions: &mut actions,
                    requests: &mut requests,
                };
                if let Some(handler) = value.chain[entry.index as usize].as_pointer_input() {
                    handler.on_pointer_event(&mut event, pass, bounds, &mut context);
                }
            };
            if pass == PointerEventPass::Main {
                path.iter().rev().for_each(|entry| deliver(self, entry));
            } else {
                path.iter().for_each(|entry| deliver(self, entry));
            }
        }
        self.apply_requests(requests, &mut actions);
        actions
    }

    /// Ends every pointer gesture in progress, as when the window loses the pointer.
    pub fn cancel_pointers(&mut self) {
        for (_, path) in std::mem::take(&mut self.captures) {
            for entry in path {
                if let Some(value) = self.get_mut(entry.node)
                    && value.chain_version == entry.version
                    && let Some(handler) = value.chain[entry.index as usize].as_pointer_input()
                {
                    handler.on_cancel_pointer_input();
                }
            }
        }
    }

    fn apply_requests(&mut self, requests: Requests, actions: &mut Vec<Action>) {
        if requests.draw {
            self.needs_draw = true;
        }
        for node in requests.measure {
            self.invalidate_measure(node);
        }
        if requests.clear_focus {
            self.clear_focus_into(actions);
        }
        if let Some(node) = requests.focus {
            self.request_focus_into(node, actions);
        }
    }

    // ---- Focus ------------------------------------------------------------------------------

    /// The focused node.
    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    /// The chain index of the node's focus target, if it has one that can take focus.
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
        actions
    }

    pub fn clear_focus(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        self.clear_focus_into(&mut actions);
        actions
    }

    fn request_focus_into(&mut self, node: NodeId, actions: &mut Vec<Action>) {
        if self.focused == Some(node) || self.focus_target_index(node).is_none() {
            return;
        }
        self.clear_focus_into(actions);
        self.focused = Some(node);
        self.notify_focus(node, true, actions);
    }

    fn clear_focus_into(&mut self, actions: &mut Vec<Action>) {
        if let Some(old) = self.focused.take() {
            self.notify_focus(old, false, actions);
        }
    }

    fn notify_focus(&mut self, node: NodeId, focused: bool, actions: &mut Vec<Action>) {
        let Some(target) = self.focus_target_index(node).or_else(|| {
            // Losing focus because it can't take it any more: tell the first target anyway.
            let value = self.get(node)?;
            (0..value.chain.len()).find(|&index| value.capabilities[index] & capability::FOCUS != 0)
        }) else {
            return;
        };
        let mut requests = Requests::default();
        let value = self.node_mut(node);
        let mut context = EventContext { node, actions, requests: &mut requests };
        if let Some(handler) = value.chain[target].as_focus_target() {
            handler.on_focus_changed(focused, &mut context);
        }
        for index in 0..target {
            if value.capabilities[index] & capability::FOCUS_EVENT != 0
                && let Some(listener) =
                    value.chain[index].downcast_ref::<crate::nodes::OnFocusChangedNode>()
            {
                context.call_with_bool(&listener.0, focused);
            }
        }
        if requests.draw {
            self.needs_draw = true;
        }
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
            self.focused.and_then(|focused| order.iter().position(|&node| node == focused));
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => order.len() - 1,
            (Some(index), true) => (index + 1) % order.len(),
            (Some(index), false) => (index + order.len() - 1) % order.len(),
        };
        self.request_focus_into(order[next], &mut actions);
        actions
    }

    // ---- Keys and text ----------------------------------------------------------------------

    /// The key input nodes an event to the focused node visits: the focused node's, from its
    /// focus target outward, then each ancestor's, innermost first.
    fn key_path(&mut self) -> Vec<(NodeId, usize)> {
        let mut path = Vec::new();
        let Some(focused) = self.focused else { return path };
        let Some(target) = self.focus_target_index(focused) else { return path };
        let mut node = Some(focused);
        let mut limit = target + 1;
        while let Some(id) = node {
            let value = self.node(id);
            for index in (0..limit.min(value.chain.len())).rev() {
                if value.capabilities[index] & capability::KEY != 0 {
                    path.push((id, index));
                }
            }
            node = value.parent;
            limit = usize::MAX;
        }
        path
    }

    /// Sends a key event to the focused node and its ancestors until one consumes it; an
    /// unconsumed Tab moves focus. Returns whether it was consumed, and the queued actions.
    pub fn dispatch_key(&mut self, event: KeyEvent) -> (bool, Vec<Action>) {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        let mut consumed = false;
        for (node, index) in self.key_path() {
            let value = self.node_mut(node);
            let mut context = EventContext { node, actions: &mut actions, requests: &mut requests };
            if let Some(handler) = value.chain[index].as_key_input()
                && handler.on_key_event(&event, &mut context)
            {
                consumed = true;
                break;
            }
        }
        self.apply_requests(requests, &mut actions);
        if !consumed && event.key == Key::Tab && event.kind == KeyEventType::KeyDown {
            actions.extend(self.move_focus(!event.modifiers.shift));
            consumed = true;
        }
        (consumed, actions)
    }

    /// Sends committed text to the focused node and its ancestors until one takes it.
    pub fn dispatch_text(&mut self, text: &str) -> (bool, Vec<Action>) {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        let mut consumed = false;
        for (node, index) in self.key_path() {
            let value = self.node_mut(node);
            let mut context = EventContext { node, actions: &mut actions, requests: &mut requests };
            if let Some(handler) = value.chain[index].as_key_input()
                && handler.on_text_input(text, &mut context)
            {
                consumed = true;
                break;
            }
        }
        self.apply_requests(requests, &mut actions);
        (consumed, actions)
    }
}
