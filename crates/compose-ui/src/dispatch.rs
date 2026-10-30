//! Input dispatch on the tree: pointer hit testing and passes, focus, and key and text events
//! to the focused node (docs/06-ui.md, "Input, focus and semantics").

use smallvec::SmallVec;

use crate::input::{
    Action, EventContext, Key, KeyEvent, KeyEventType, PointerEvent, PointerEventPass,
    PointerEventType, Requests,
};
use crate::node_chain::NodeKinds;
use crate::tree::{NodeId, UiTree};
use crate::unit::{IntSize, Offset};

/// A pointer input node on a hit path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct HitEntry {
    pub(crate) node: NodeId,
    pub(crate) index: u16,
    pub(crate) layer: u16,
    pub(crate) version: u32,
    /// An observer: on the path only if something inside it is hit.
    pub(crate) observer: bool,
}

/// Whether the entries pushed from `start` hit anything but observers; if not, they're dropped.
fn hit_since(path: &mut Vec<HitEntry>, start: usize) -> bool {
    if path[start..].iter().any(|entry| !entry.observer) {
        true
    } else {
        path.truncate(start);
        false
    }
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
        if value.kinds.any(NodeKinds::POINTER_INPUT | NodeKinds::DRAW) {
            for index in 0..value.chain.len() {
                let bits = value.chain[index].kinds;
                let size = value.layers[layer].measured.into();
                if bits.any(NodeKinds::DRAW)
                    && let Some(clip) = value.chain[index].node.hit_clip_at(size, density)
                    && !clip
                        .outline_in(size, &density, value.layout_direction)
                        .contains(local - origins[layer])
                {
                    // Outside the clip: nothing after it, nor the children, is hit.
                    return hit_since(path, start);
                }
                if bits.any(NodeKinds::POINTER_INPUT)
                    && contains(value.layers[layer].measured, local - origins[layer])
                {
                    path.push(HitEntry {
                        node,
                        index: index as u16,
                        layer: layer as u16,
                        version: value.chain_version,
                        observer: value.chain[index].node.is_pointer_observer(),
                    });
                }
                if bits.any(NodeKinds::LAYOUT) {
                    layer += 1;
                }
            }
        }
        let content = local - *origins.last().expect("a layer");
        // The last placed (drawn on top) first.
        let order = self.placement_order(node);
        let count = order.as_ref().map_or(value.children.len(), |order| order.len());
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
        hit_since(path, start)
    }

    /// Dispatches a pointer event at `position` in the root. A press captures its hit path:
    /// the pointer's moves and its release go to the same nodes. Returns the actions handlers
    /// queued, to run once the tree is free.
    pub fn dispatch_pointer(&mut self, mut event: PointerEvent) -> Vec<Action> {
        crate::nested_scroll::begin_pointer_event();
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
        // A mouse's hover: what it left hears `Exit`, what it came over `Enter`, as Compose's
        // (not while a button is held: a drag stays with what it pressed).
        let hovering = event.pointer_type == crate::input::PointerType::Mouse
            && event.kind != PointerEventType::Scroll
            && (!event.pressed || event.kind == PointerEventType::Press);
        if event.pointer_type == crate::input::PointerType::Mouse
            && event.kind != PointerEventType::Scroll
        {
            self.mouse = (!event.pressed).then_some(position);
        }
        if hovering {
            let now = if event.kind == PointerEventType::Release || captured.is_some() {
                self.hit_test(position)
            } else {
                path.clone()
            };
            self.update_hover(now, event, &mut actions, &mut requests);
        }
        for pass in [PointerEventPass::Initial, PointerEventPass::Main, PointerEventPass::Final] {
            if pass == PointerEventPass::Main {
                for entry in path.iter().rev() {
                    self.deliver_pointer(
                        entry,
                        &mut event,
                        pass,
                        position,
                        &mut actions,
                        &mut requests,
                    );
                }
            } else {
                for entry in path.iter() {
                    self.deliver_pointer(
                        entry,
                        &mut event,
                        pass,
                        position,
                        &mut actions,
                        &mut requests,
                    );
                }
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
                && let Some(handler) = value.chain[entry.index as usize].node.as_pointer_input()
            {
                handler.on_cancel_pointer_input();
            }
        }
    }

    /// Ends every pointer gesture in progress, as when the window loses the pointer.
    /// Sends one pointer event, in a pass, to one entry of a hit path.
    fn deliver_pointer(
        &mut self,
        entry: &HitEntry,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        position: Offset,
        actions: &mut Vec<Action>,
        requests: &mut Requests,
    ) {
        if self.get(entry.node).is_none_or(|value| value.chain_version != entry.version) {
            return;
        }
        let origin = self.layer_origin(entry.node, entry.layer as usize);
        let density = self.density;
        let view_configuration = self.view_configuration();
        let Some((value, (text_measurer, clipboard))) = self.node_and_measurer(entry.node) else {
            return;
        };
        let bounds = value.layers[entry.layer as usize].measured;
        event.position = position - origin;
        let mut context = EventContext {
            node: entry.node,
            actions,
            requests,
            text_measurer,
            clipboard,
            pointer_position: Some(position),
            density,
            view_configuration,
            layout_direction: value.layout_direction,
        };
        if let Some(handler) = value.chain[entry.index as usize].node.as_pointer_input() {
            handler.on_pointer_event(event, pass, bounds, &mut context);
        }
        event.position = position;
    }

    /// Moves the mouse's hover to the entries of `now`: `Exit` to those it left, `Enter` to
    /// those it came over (in the main pass).
    fn update_hover(
        &mut self,
        now: Vec<HitEntry>,
        event: PointerEvent,
        actions: &mut Vec<Action>,
        requests: &mut Requests,
    ) {
        let before = std::mem::take(&mut self.hovered);
        let same = |a: &HitEntry, b: &HitEntry| {
            a.node == b.node && a.index == b.index && a.version == b.version
        };
        let left: Vec<HitEntry> =
            before.iter().filter(|old| !now.iter().any(|new| same(old, new))).cloned().collect();
        let entered: Vec<HitEntry> =
            now.iter().filter(|new| !before.iter().any(|old| same(old, new))).cloned().collect();
        self.deliver_hover(&left, PointerEventType::Exit, event, actions, requests);
        self.deliver_hover(&entered, PointerEventType::Enter, event, actions, requests);
        self.hovered = now;
    }

    /// Delivers `kind` (`Enter` or `Exit`) to `entries` (outer to inner), each its own event,
    /// through the three passes as any event: initial outer to inner, main inner to outer,
    /// final outer to inner.
    fn deliver_hover(
        &mut self,
        entries: &[HitEntry],
        kind: PointerEventType,
        event: PointerEvent,
        actions: &mut Vec<Action>,
        requests: &mut Requests,
    ) {
        let position = event.position;
        let mut events = vec![PointerEvent { kind, consumed: false, ..event }; entries.len()];
        for pass in [PointerEventPass::Initial, PointerEventPass::Main, PointerEventPass::Final] {
            let mut order: Vec<usize> = (0..entries.len()).collect();
            if pass == PointerEventPass::Main {
                order.reverse();
            }
            for index in order {
                self.deliver_pointer(
                    &entries[index],
                    &mut events[index],
                    pass,
                    position,
                    actions,
                    requests,
                );
            }
        }
    }

    /// The mouse pointer's look where it is: the innermost hovered node's icon.
    pub fn pointer_icon(&mut self) -> crate::input::PointerIcon {
        let hovered = std::mem::take(&mut self.hovered);
        let icon = hovered.iter().rev().find_map(|entry| {
            let value = self.get_mut(entry.node)?;
            if value.chain_version != entry.version {
                return None;
            }
            value.chain[entry.index as usize].node.as_pointer_input()?.hover_icon()
        });
        self.hovered = hovered;
        icon.unwrap_or_default()
    }

    /// Hit-tests the resting mouse again (after a layout moved what's under it): what it left
    /// hears `Exit`, what it's now over `Enter`.
    pub fn refresh_hover(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(position) = self.mouse else { return actions };
        let mut requests = Requests::default();
        let now = self.hit_test(position);
        self.update_hover(now, PointerEvent::moved(position, false), &mut actions, &mut requests);
        self.apply_requests(requests, &mut actions);
        actions
    }

    /// The mouse left the window: what it was over hears `Exit`.
    pub fn pointer_left(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        self.mouse = None;
        let mut event = PointerEvent::moved(Offset::new(-1.0, -1.0), false);
        event.kind = PointerEventType::Exit;
        self.update_hover(Vec::new(), event, &mut actions, &mut requests);
        self.apply_requests(requests, &mut actions);
        actions
    }

    pub fn cancel_pointers(&mut self) {
        for (_, path) in std::mem::take(&mut self.captures) {
            for entry in path {
                if let Some(value) = self.get_mut(entry.node)
                    && value.chain_version == entry.version
                    && let Some(handler) = value.chain[entry.index as usize].node.as_pointer_input()
                {
                    handler.on_cancel_pointer_input();
                    // A gesture's drawing (a press, a drag) ends without a context to say so.
                    self.invalidate_draw_of(entry.node);
                }
            }
        }
    }

    pub(crate) fn apply_requests(&mut self, requests: Requests, actions: &mut Vec<Action>) {
        for node in requests.draw {
            self.invalidate_draw_of(node);
        }
        self.input_method_reset |= requests.reset_input_method;
        if let Some(show) = requests.software_keyboard {
            self.focus_requests.software_keyboard.set(Some(show));
            self.focus_requests.wake();
        }
        for node in requests.measure {
            self.invalidate_measure(node);
        }
        if requests.clear_focus {
            self.clear_focus_into(actions);
        }
        if let Some(forward) = requests.move_focus {
            let moved = self.move_focus(forward);
            actions.extend(moved);
        }
        // A press inside a nested container is its: the containers around it let it be.
        let mut started: Option<NodeId> = None;
        for request in requests.selection {
            if let crate::text_selection::SelectionRequest::Start { container, .. } = request {
                if started.is_some_and(|inner| self.is_at_or_below(inner, container)) {
                    continue;
                }
                started = Some(container);
            }
            self.handle_selection(request);
        }
        if let Some(node) = requests.focus {
            self.request_focus_into(node, actions);
            if let Some(tap) = requests.focus_tap {
                self.tell_focus_tap(tap, actions);
            }
        }
        self.update_focus_listeners(actions);
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
                if value.chain[index].kinds.any(NodeKinds::KEY_INPUT) {
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
            let view_configuration = self.view_configuration();
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
                view_configuration,
                layout_direction: value.layout_direction,
            };
            if let Some(handler) = value.chain[index].node.as_key_input()
                && handler.on_key_event(&event, &mut context)
            {
                consumed = true;
                break;
            }
        }
        self.apply_requests(requests, &mut actions);
        if !consumed && event.key == Key::Tab && event.kind == KeyEventType::KeyDown {
            actions.extend(self.move_focus(!event.modifiers.shift));
            // A search waiting for a lazy list's next item asks for the frame laying it out.
            if self.has_beyond_bounds_moves() {
                self.needs_draw = true;
            }
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

    /// Sends an input method's action key to the focused node and its ancestors until one takes
    /// it.
    pub fn dispatch_ime_action(
        &mut self,
        action: crate::text_input::ImeAction,
    ) -> (bool, Vec<Action>) {
        self.dispatch_to_key_path(|handler, context| handler.on_ime_action(action, context))
    }

    /// Sends an input method's edit to the focused node and its ancestors until one takes it.
    pub fn dispatch_input_method_command(
        &mut self,
        command: &crate::text_editing::InputMethodCommand,
    ) -> (bool, Vec<Action>) {
        self.dispatch_to_key_path(|handler, context| {
            handler.on_input_method_command(command, context)
        })
    }

    /// What an input method's edits apply to in the focused node taking them.
    pub fn input_method_value(&mut self) -> Option<crate::text_editing::TextFieldValue> {
        let mut next = self.focused();
        while let Some(node) = next {
            next = self.node(node).parent;
            for index in (0..self.node(node).chain.len()).rev() {
                if !self.node(node).chain[index].kinds.any(NodeKinds::KEY_INPUT) {
                    continue;
                }
                let value = self.get_mut(node)?.chain[index]
                    .node
                    .as_key_input()
                    .and_then(|handler| handler.input_method_value());
                if value.is_some() {
                    return value;
                }
            }
        }
        None
    }

    /// What the focused node taking text asks of an on-screen keyboard: its `ImeOptions`, the
    /// defaults if it gives none, `None` if no focused node takes text.
    pub fn text_input_options(&mut self) -> Option<crate::text_input::ImeOptions> {
        let mut next = self.focused();
        while let Some(node) = next {
            next = self.node(node).parent;
            for index in (0..self.node(node).chain.len()).rev() {
                if !self.node(node).chain[index].kinds.any(NodeKinds::KEY_INPUT) {
                    continue;
                }
                let handler = self.get_mut(node)?.chain[index].node.as_key_input();
                if let Some(handler) = handler
                    && handler.text_input_area().is_some()
                {
                    return Some(handler.ime_options().unwrap_or_default());
                }
            }
        }
        None
    }

    /// Where the focused node takes text, in the root: where an input method's candidate window
    /// goes.
    pub fn text_input_area(&mut self) -> Option<crate::unit::Rect> {
        // The key path, walked without collecting it: asked every frame.
        let mut next = self.focused();
        while let Some(node) = next {
            next = self.node(node).parent;
            for index in (0..self.node(node).chain.len()).rev() {
                if !self.node(node).chain[index].kinds.any(NodeKinds::KEY_INPUT) {
                    continue;
                }
                let layer = {
                    let value = self.node(node);
                    value.layer_of(index)
                };
                let area = self.get_mut(node)?.chain[index]
                    .node
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
            let view_configuration = self.view_configuration();
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
                view_configuration,
                layout_direction: value.layout_direction,
            };
            if let Some(handler) = value.chain[index].node.as_key_input()
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
