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
        // The overlays first, the topmost first: what one hits, nothing beneath it gets.
        for index in (0..self.overlays.len()).rev() {
            let node = self.overlays[index];
            let Some(value) = self.get(node).filter(|value| value.overlay && value.placed) else {
                continue;
            };
            let placed = value.layers[0].offset;
            let at = value.overlay_origin - Offset::new(placed.x as f32, placed.y as f32);
            let inside = contains(value.layers[0].size, position - value.overlay_origin);
            let pass_through = self.overlay_node(node).is_some_and(|overlay| overlay.pass_through);
            // A point in an overlay is its, handled or not, as a popup's window takes it; unless
            // it passes its hits through (a non-focusable popup's outside).
            if (self.hit_node(node, position - at, &mut path) || inside) && !pass_through {
                return path;
            }
        }
        self.hit_node(self.root_id(), position, &mut path);
        path
    }

    fn hit_node(&mut self, node: NodeId, point: Offset, path: &mut Vec<HitEntry>) -> bool {
        let value = self.node(node);
        if !value.placed {
            return false;
        }
        // As Compose: a node's bounds don't limit what's hit inside it (an `offset`'s content,
        // children placed outside), only clips do.
        let offset = value.layers[0].offset;
        let local = point - Offset::new(offset.x as f32, offset.y as f32);
        let density = self.density;
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
        if value.all_capabilities & (capability::POINTER | capability::DRAW) != 0 {
            for index in 0..value.chain.len() {
                let bits = value.capabilities[index];
                if bits & capability::DRAW != 0
                    && let Some(clip) = value.chain[index].hit_clip()
                {
                    let size = value.layers[layer].measured.into();
                    if !clip.outline(size, &density).contains(local - origins[layer]) {
                        // Outside the clip: nothing after it, nor the children, is hit.
                        return path.len() > start;
                    }
                }
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
        // The last placed (drawn on top) first.
        let order = self.placement_order(node);
        for position in (0..count).rev() {
            let index = order.as_ref().map_or(position, |order| order[position] as usize);
            let child = self.node(node).children[index].node;
            // Overlays are hit first, on their own.
            if self.node(child).overlay {
                continue;
            }
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
                let density = tree.density;
                let Some((value, (text_measurer, clipboard))) = tree.node_and_measurer(entry.node)
                else {
                    return;
                };
                let bounds = value.layers[entry.layer as usize].measured;
                event.position = position - origin;
                let mut context = EventContext {
                    node: entry.node,
                    actions: &mut actions,
                    requests: &mut requests,
                    text_measurer,
                    clipboard,
                    pointer_position: Some(position),
                    density,
                    layout_direction: value.layout_direction,
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

    /// Ends the gesture of `pointer`, without a release: a touch the system cancelled.
    pub fn cancel_pointer(&mut self, pointer: crate::input::PointerId) {
        let Some(index) = self.captures.iter().position(|(captured, _)| *captured == pointer)
        else {
            return;
        };
        let (_, path) = self.captures.remove(index);
        for entry in path {
            if let Some(value) = self.get_mut(entry.node)
                && value.chain_version == entry.version
                && let Some(handler) = value.chain[entry.index as usize].as_pointer_input()
            {
                handler.on_cancel_pointer_input();
            }
        }
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

    pub(crate) fn apply_requests(&mut self, requests: Requests, actions: &mut Vec<Action>) {
        if requests.draw {
            self.needs_draw = true;
        }
        self.input_method_reset |= requests.reset_input_method;
        for node in requests.measure {
            self.invalidate_measure(node);
        }
        if requests.clear_focus {
            self.clear_focus_into(actions);
        }
        if let Some(node) = requests.focus {
            self.request_focus_into(node, actions);
            if let Some(tap) = requests.focus_tap {
                self.tell_focus_tap(tap, actions);
            }
        }
        self.update_focus_listeners(actions);
    }

    // ---- Semantics actions --------------------------------------------------------------

    /// Performs `action` on `node` (a semantics node's id), as an accessibility service asks:
    /// the first of its semantics modifier nodes offering it does it. Returns whether one did,
    /// and the queued actions.
    pub fn perform_semantics_action(
        &mut self,
        node: NodeId,
        action: crate::semantics::SemanticsAction,
    ) -> (bool, Vec<Action>) {
        let mut actions = Vec::new();
        if self.get(node).is_none() {
            return (false, actions);
        }
        if action == crate::semantics::SemanticsAction::RequestFocus {
            // Offered only by a node whose semantics say it's focusable (its own focus target,
            // or a part's: a text field's inner field), as Compose's `RequestFocus`.
            let focusable = self.focus_target_index(node).is_some() || {
                fn find(tree: &crate::semantics::SemanticsNode, id: NodeId) -> Option<bool> {
                    if tree.id == id {
                        return Some(tree.config.focusable);
                    }
                    tree.children.iter().find_map(|child| find(child, id))
                }
                find(&self.semantics_tree(false), node).unwrap_or(false)
            };
            if !focusable {
                return (false, actions);
            }
            let actions = self.request_focus(node);
            let focused = self.focused().is_some_and(|focused| self.is_at_or_below(focused, node));
            return (focused, actions);
        }
        let mut requests = Requests::default();
        let mut performed = false;
        for index in 0..self.node(node).chain.len() {
            if self.node(node).capabilities[index] & capability::SEMANTICS == 0 {
                continue;
            }
            let density = self.density;
            let Some((value, (text_measurer, clipboard))) = self.node_and_measurer(node) else {
                break;
            };
            let mut context = EventContext {
                node,
                actions: &mut actions,
                requests: &mut requests,
                text_measurer,
                clipboard,
                pointer_position: None,
                density,
                layout_direction: value.layout_direction,
            };
            if let Some(semantics) = value.chain[index].as_semantics()
                && semantics.perform_action(action, &mut context)
            {
                performed = true;
                break;
            }
        }
        self.apply_requests(requests, &mut actions);
        (performed, actions)
    }

    // ---- Keys and text ----------------------------------------------------------------------

    /// The key input nodes an event to the focused node visits: the focused node's, innermost
    /// first (those after its focus target too, as Compose's `lastLocalKeyInputNode`), then
    /// each ancestor's.
    ///
    /// The path ends at the overlay holding the focus, as a window's keys stay in it; with
    /// nothing focused, keys go to the topmost overlay that takes the focus (a dialog's Escape).
    fn key_path(&mut self) -> Vec<(NodeId, usize)> {
        let mut path = Vec::new();
        let mut node = self.focused().or_else(|| self.modal_overlay());
        while let Some(id) = node {
            let value = self.node(id);
            for index in (0..value.chain.len()).rev() {
                if value.capabilities[index] & capability::KEY != 0 {
                    path.push((id, index));
                }
            }
            node = if value.overlay { None } else { value.parent };
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
            let density = self.density;
            let Some((value, (text_measurer, clipboard))) = self.node_and_measurer(node) else {
                continue;
            };
            let mut context = EventContext {
                node,
                actions: &mut actions,
                requests: &mut requests,
                text_measurer,
                clipboard,
                pointer_position: None,
                density,
                layout_direction: value.layout_direction,
            };
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
        self.dispatch_to_key_path(|handler, context| handler.on_text_input(text, context))
    }

    /// Sends an input method's composition to the focused node and its ancestors until one
    /// takes it.
    pub fn dispatch_preedit(
        &mut self,
        text: &str,
        cursor: Option<(usize, usize)>,
    ) -> (bool, Vec<Action>) {
        self.dispatch_to_key_path(|handler, context| handler.on_preedit(text, cursor, context))
    }

    /// Where the focused node takes text, in the root: where an input method's candidate window
    /// goes.
    pub fn text_input_area(&mut self) -> Option<crate::unit::Rect> {
        // The key path, walked without collecting it: asked every frame.
        let mut next = self.focused();
        while let Some(node) = next {
            next = self.node(node).parent;
            for index in (0..self.node(node).chain.len()).rev() {
                if self.node(node).capabilities[index] & capability::KEY == 0 {
                    continue;
                }
                let layer = {
                    let value = self.node(node);
                    value.capabilities[..index]
                        .iter()
                        .filter(|bits| **bits & capability::LAYOUT != 0)
                        .count()
                };
                let area = self.get_mut(node)?.chain[index]
                    .as_key_input()
                    .and_then(|handler| handler.text_input_area());
                if let Some(area) = area {
                    let origin = self.layer_origin(node, layer);
                    return Some(crate::unit::Rect::new(
                        area.left + origin.x,
                        area.top + origin.y,
                        area.right + origin.x,
                        area.bottom + origin.y,
                    ));
                }
            }
        }
        None
    }

    fn dispatch_to_key_path(
        &mut self,
        mut send: impl FnMut(
            &mut dyn crate::modifier::KeyInputModifierNode,
            &mut EventContext<'_>,
        ) -> bool,
    ) -> (bool, Vec<Action>) {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        let mut consumed = false;
        for (node, index) in self.key_path() {
            let density = self.density;
            let Some((value, (text_measurer, clipboard))) = self.node_and_measurer(node) else {
                continue;
            };
            let mut context = EventContext {
                node,
                actions: &mut actions,
                requests: &mut requests,
                text_measurer,
                clipboard,
                pointer_position: None,
                density,
                layout_direction: value.layout_direction,
            };
            if let Some(handler) = value.chain[index].as_key_input()
                && send(handler, &mut context)
            {
                consumed = true;
                break;
            }
        }
        self.apply_requests(requests, &mut actions);
        (consumed, actions)
    }
}
