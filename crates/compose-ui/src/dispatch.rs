//! Input dispatch on the tree: pointer hit testing and passes, focus, and key and text events
//! to the focused node (docs/06-ui.md, "Input, focus and semantics").

use crate::delegation::dispatch_for_kind;
use crate::input::{
    Action, EventContext, Key, KeyEvent, KeyEventType, PointerEvent, PointerEventPass,
    PointerEventType, Requests,
};
use crate::node_chain::{EntryId, NodeKinds, entry_state};
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
    /// The delegate of entry `index` it is, or `NO_DELEGATE`: the entry's node.
    pub(crate) delegate: EntryId,
}

impl HitEntry {
    /// Whether both are delegates of one chain entry.
    fn is_delegate_with(&self, other: &HitEntry) -> bool {
        self.delegate != EntryId::NO_DELEGATE
            && other.delegate != EntryId::NO_DELEGATE
            && self.node == other.node
            && self.index == other.index
    }
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
        let density = self.density;
        let value = self.node_mut(node);
        if !value.placed {
            return false;
        }
        // As Compose: a node's bounds don't limit what's hit inside it (an `offset`'s content,
        // children placed outside), only clips do.
        let offset = value.layers[0].offset;
        let local = point - Offset::new(offset.x as f32, offset.y as f32);
        let start = path.len();
        // The node's layers are offset from each other: the hit test visits every node of a tree
        // (only clips stop it), so it adds them up as it goes, keeping nothing.
        let layer_offset = |layer: usize| {
            value.layers.get(layer).map_or(Offset::ZERO, |entry| {
                Offset::new(entry.offset.x as f32, entry.offset.y as f32)
            })
        };
        let mut origin = Offset::ZERO;
        let mut layer = 0usize;
        if value.kinds.any(NodeKinds::POINTER_INPUT | NodeKinds::DRAW) {
            for index in 0..value.chain.len() {
                let bits = value.chain[index].kinds;
                let size = value.layers[layer].measured.into();
                if bits.any(NodeKinds::DRAW)
                    && let Some(clip) = value.chain[index].node.hit_clip_at(size, density)
                    && !clip
                        .outline_in(size, &density, value.layout_direction)
                        .contains(local - origin)
                {
                    // Outside the clip: nothing after it, nor the children, is hit.
                    return hit_since(path, start);
                }
                if bits.any(NodeKinds::POINTER_INPUT)
                    && contains(value.layers[layer].measured, local - origin)
                {
                    let version = value.chain_version;
                    let entry = &mut value.chain[index];
                    let mut push = |modifier_node: &mut dyn crate::modifier::ModifierNode,
                                    delegate| {
                        path.push(HitEntry {
                            node,
                            index: index as u16,
                            layer: layer as u16,
                            version,
                            observer: modifier_node.is_pointer_observer(),
                            delegate,
                        });
                    };
                    if entry.state & entry_state::DELEGATING == 0 {
                        push(&mut *entry.node, EntryId::NO_DELEGATE);
                    } else {
                        // The node, or each of its delegates that takes pointer input.
                        dispatch_for_kind(
                            &mut *entry.node,
                            entry.own,
                            NodeKinds::POINTER_INPUT,
                            EntryId::NO_DELEGATE,
                            &mut push,
                        );
                    }
                }
                if bits.any(NodeKinds::LAYOUT) {
                    layer += 1;
                    origin = origin + layer_offset(layer);
                }
            }
        }
        // The chain's walk stops at the content's layer; without it, the layers are added here.
        for next in layer + 1..value.layers.len() {
            origin = origin + layer_offset(next);
        }
        let content = local - origin;
        // The last placed (drawn on top) first.
        let order = self.placement_order(node);
        let count = order.as_ref().map_or(self.node(node).children.len(), |order| order.len());
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
        // For a press, whether something before each entry (around it) waits for a gesture past
        // it, as Compose's `hasWaitingParent`.
        let waiting = if event.kind == PointerEventType::Press {
            self.waiting_before(&path, &event)
        } else {
            Vec::new()
        };
        for pass in [PointerEventPass::Initial, PointerEventPass::Main, PointerEventPass::Final] {
            if pass == PointerEventPass::Main {
                // From the inside out; but one node's delegates in delegation order, in every
                // pass, as Compose's `dispatchForKind` in its hit path's node.
                let mut end = path.len();
                while end > 0 {
                    let mut start = end - 1;
                    while start > 0 && path[start].is_delegate_with(&path[start - 1]) {
                        start -= 1;
                    }
                    for (index, entry) in path[start..end].iter().enumerate() {
                        let waiting_parent = waiting.get(start + index).copied().unwrap_or(false);
                        self.deliver_pointer(
                            entry,
                            &mut event,
                            pass,
                            position,
                            waiting_parent,
                            &mut actions,
                            &mut requests,
                        );
                    }
                    end = start;
                }
            } else {
                for entry in path.iter() {
                    self.deliver_pointer(
                        entry,
                        &mut event,
                        pass,
                        position,
                        false,
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
                && let Some(handler) = value.chain[entry.index as usize]
                    .resolve(entry.delegate)
                    .and_then(|node| node.as_pointer_input())
            {
                handler.on_cancel_pointer_input();
                self.note_delegates_changed(entry.node, entry.index as usize);
            }
        }
        self.sync_noted_delegates();
    }

    /// For each entry of `path`, whether an entry before it (around it) waits for a gesture
    /// past a press.
    fn waiting_before(&mut self, path: &[HitEntry], event: &PointerEvent) -> Vec<bool> {
        let mut waiting = Vec::with_capacity(path.len());
        let mut any = false;
        for entry in path {
            waiting.push(any);
            if !any
                && let Some(value) = self.get_mut(entry.node)
                && value.chain_version == entry.version
                && let Some(handler) = value.chain[entry.index as usize].node.as_pointer_input()
            {
                any = handler.waits_for_gesture(event);
            }
        }
        waiting
    }

    /// Sends one pointer event, in a pass, to one entry of a hit path.
    #[allow(clippy::too_many_arguments)]
    fn deliver_pointer(
        &mut self,
        entry: &HitEntry,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        position: Offset,
        waiting_parent: bool,
        actions: &mut Vec<Action>,
        requests: &mut Requests,
    ) {
        if self.get(entry.node).is_none_or(|value| value.chain_version != entry.version) {
            return;
        }
        let origin = self.layer_origin(entry.node, entry.layer as usize);
        let bounds = self.node(entry.node).layers[entry.layer as usize].measured;
        event.position = position - origin;
        self.with_event_context(
            entry.node,
            entry.index as usize,
            entry.delegate,
            actions,
            requests,
            Some(position),
            waiting_parent,
            |modifier_node, context| {
                if let Some(handler) = modifier_node.as_pointer_input() {
                    handler.on_pointer_event(event, pass, bounds, context);
                }
            },
        );
        event.position = position;
    }

    /// Runs `f` with entry `index` of `node` (or its delegate `delegate`) and an `EventContext`:
    /// a handler's, with the node's own context in it, so it's out of its chain meanwhile, and
    /// what it delegates or undelegates is seen when it returns. `None` if it's not there.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_event_context<R>(
        &mut self,
        node: NodeId,
        index: usize,
        delegate: EntryId,
        actions: &mut Vec<Action>,
        requests: &mut Requests,
        pointer_position: Option<Offset>,
        waiting_parent: bool,
        f: impl FnOnce(&mut dyn crate::modifier::ModifierNode, &mut EventContext<'_>) -> R,
    ) -> Option<R> {
        if self.get(node).is_none_or(|value| index >= value.chain.len()) {
            return None;
        }
        let view_configuration = self.view_configuration();
        self.with_entry(node, index, |root, context| {
            let run = |modifier_node: &mut dyn crate::modifier::ModifierNode,
                       context: &mut crate::node_context::NodeContext<'_>| {
                let mut event_context = EventContext {
                    context: context.reborrow(),
                    actions,
                    requests,
                    pointer_position,
                    view_configuration,
                    waiting_parent,
                };
                let result = f(modifier_node, &mut event_context);
                let state = event_context.context.out_state;
                context.out_state = state;
                result
            };
            if delegate == EntryId::NO_DELEGATE {
                Some(run(root, context))
            } else {
                let entry = crate::delegation::find_delegate(root, delegate)?;
                Some(context.with_delegate(entry, run))
            }
        })
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
                    false,
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
            value.chain[entry.index as usize]
                .resolve(entry.delegate)?
                .as_pointer_input()?
                .hover_icon()
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

    /// Ends every pointer gesture in progress, as when the window loses the pointer.
    pub fn cancel_pointers(&mut self) {
        for (_, path) in std::mem::take(&mut self.captures) {
            for entry in path {
                if let Some(value) = self.get_mut(entry.node)
                    && value.chain_version == entry.version
                    && let Some(handler) = value.chain[entry.index as usize]
                        .resolve(entry.delegate)
                        .and_then(|node| node.as_pointer_input())
                {
                    handler.on_cancel_pointer_input();
                    self.note_delegates_changed(entry.node, entry.index as usize);
                    // A gesture's drawing (a press, a drag) ends without a context to say so.
                    self.invalidate_draw_of(entry.node);
                }
            }
        }
        self.sync_noted_delegates();
    }

    pub(crate) fn apply_requests(&mut self, requests: Requests, actions: &mut Vec<Action>) {
        self.sync_noted_delegates();
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
    fn key_path(&mut self) -> Vec<(NodeId, usize, EntryId)> {
        let mut path = Vec::new();
        let mut node = self.focused().or_else(|| self.modal_overlay());
        while let Some(id) = node {
            let value = self.node_mut(id);
            for index in (0..value.chain.len()).rev() {
                let entry = &mut value.chain[index];
                if !entry.kinds.any(NodeKinds::KEY_INPUT) {
                    continue;
                }
                // The node, or each of its delegates that takes keys.
                dispatch_for_kind(
                    &mut *entry.node,
                    entry.own,
                    NodeKinds::KEY_INPUT,
                    EntryId::NO_DELEGATE,
                    &mut |_, delegate| path.push((id, index, delegate)),
                );
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
        for (node, index, delegate) in self.key_path() {
            consumed = self
                .with_event_context(
                    node,
                    index,
                    delegate,
                    &mut actions,
                    &mut requests,
                    None,
                    false,
                    |modifier_node, context| {
                        modifier_node
                            .as_key_input()
                            .is_some_and(|handler| handler.on_key_event(&event, context))
                    },
                )
                .unwrap_or(false);
            if consumed {
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
                let entry = &mut self.get_mut(node)?.chain[index];
                let mut options = None;
                dispatch_for_kind(
                    &mut *entry.node,
                    entry.own,
                    NodeKinds::KEY_INPUT,
                    EntryId::NO_DELEGATE,
                    &mut |modifier_node, _| {
                        if options.is_none()
                            && let Some(handler) = modifier_node.as_key_input()
                            && handler.text_input_area().is_some()
                        {
                            options = Some(handler.ime_options().unwrap_or_default());
                        }
                    },
                );
                if options.is_some() {
                    return options;
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
                let entry = &mut self.get_mut(node)?.chain[index];
                let mut area = None;
                dispatch_for_kind(
                    &mut *entry.node,
                    entry.own,
                    NodeKinds::KEY_INPUT,
                    EntryId::NO_DELEGATE,
                    &mut |modifier_node, _| {
                        if area.is_none() {
                            area = modifier_node.as_key_input().and_then(|h| h.text_input_area());
                        }
                    },
                );
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
        for (node, index, delegate) in self.key_path() {
            consumed = self
                .with_event_context(
                    node,
                    index,
                    delegate,
                    &mut actions,
                    &mut requests,
                    None,
                    false,
                    |modifier_node, context| {
                        modifier_node.as_key_input().is_some_and(|handler| send(handler, context))
                    },
                )
                .unwrap_or(false);
            if consumed {
                break;
            }
        }
        self.apply_requests(requests, &mut actions);
        (consumed, actions)
    }
}
