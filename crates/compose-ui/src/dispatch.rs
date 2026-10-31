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
    /// It takes an event's changes together (`takes_whole_events`).
    pub(crate) whole: bool,
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

/// What a pointer's next change is told of its last, in the root.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointerState {
    position: Offset,
    pressed: bool,
    uptime_millis: u64,
    buttons: crate::pointer_event::PointerButtons,
    pointer_type: crate::input::PointerType,
    pressure: f32,
}

/// One pointer's change, as a dispatch has it.
struct LiveChange {
    event: PointerEvent,
    previous: Option<PointerState>,
    /// Only there as every finger down is in an event: the host said nothing of it.
    resting: bool,
    uptime_millis: u64,
    /// Whether it's consumed, what each node hearing it shares.
    consumed: std::rc::Rc<std::cell::Cell<bool>>,
}

impl LiveChange {
    fn new(event: PointerEvent, previous: Option<PointerState>, resting: bool) -> LiveChange {
        let nanos = match event.time_nanos {
            0 => compose_runtime::platform::now_nanos(),
            nanos => nanos,
        };
        LiveChange {
            event,
            previous,
            resting,
            uptime_millis: nanos / 1_000_000,
            consumed: std::rc::Rc::new(std::cell::Cell::new(event.consumed)),
        }
    }

    /// The change as it is in what's at `origin` in the root.
    fn change_at(&self, origin: Offset) -> crate::pointer_event::PointerInputChange {
        let event = &self.event;
        let position = event.position - origin;
        // Coming over or leaving a node isn't its press or its release.
        let hover = matches!(event.kind, PointerEventType::Enter | PointerEventType::Exit);
        // Where it was, the node where it is now: only the pointer's move counts.
        let previous_position = match self.previous {
            Some(previous) => position - (event.position - previous.position),
            None => position,
        };
        let mut change = crate::pointer_event::PointerInputChange::new(
            event.pointer,
            self.uptime_millis,
            position,
            event.pressed && !hover,
            self.previous.map_or(self.uptime_millis, |it| it.uptime_millis),
            previous_position,
            !hover && self.previous.is_some_and(|it| it.pressed),
            false,
        );
        change.pressure = event.pressure;
        change.pointer_type = event.pointer_type;
        change.scroll_delta = event.scroll_delta;
        change.sharing(self.consumed.clone())
    }

    /// The event for what hears a pointer one change at a time: for a mouse, its primary button
    /// alone. What another button does isn't an event; it's down only while the primary is.
    fn as_primary_button(&self) -> Option<PointerEvent> {
        let event = self.event;
        if event.pointer_type != crate::input::PointerType::Mouse {
            return Some(event);
        }
        let primary = |pressed: bool, buttons: crate::pointer_event::PointerButtons| {
            // A host that doesn't say which: its only button.
            buttons.is_primary_pressed() || (pressed && !buttons.are_any_pressed())
        };
        let now = primary(event.pressed, event.buttons);
        let before = self.previous.is_some_and(|it| primary(it.pressed, it.buttons));
        let kind = match event.kind {
            PointerEventType::Press | PointerEventType::Release | PointerEventType::Unknown => {
                match (before, now) {
                    (false, true) => PointerEventType::Press,
                    (true, false) => PointerEventType::Release,
                    _ => return None,
                }
            }
            kind => kind,
        };
        Some(PointerEvent { kind, pressed: now, ..event })
    }
}

/// The reason for one platform event, of the changes in it.
fn event_kind(events: &[PointerEvent]) -> PointerEventType {
    let any = |kind| events.iter().any(|event| event.kind == kind);
    if any(PointerEventType::Release) {
        PointerEventType::Release
    } else if any(PointerEventType::Press) {
        PointerEventType::Press
    } else {
        events.first().map_or(PointerEventType::Unknown, |event| event.kind)
    }
}

/// The pointers' hit paths of a dispatch, a shared start one branch, as `HitPathTracker`'s tree.
#[derive(Default)]
struct HitBranches {
    all: Vec<HitBranch>,
    roots: Vec<usize>,
}

struct HitBranch {
    entry: HitEntry,
    /// The changes reaching it.
    changes: smallvec::SmallVec<[usize; 2]>,
    children: smallvec::SmallVec<[usize; 2]>,
    /// Whether what's around it waits for a gesture past a press, in one of the paths.
    waiting: bool,
}

impl HitBranches {
    /// Adds the path of change `change`, with what's waiting around each of its entries.
    fn add(&mut self, path: &[HitEntry], change: usize, waiting: &[bool]) {
        let mut parent: Option<usize> = None;
        for (index, entry) in path.iter().enumerate() {
            let siblings: &[usize] = match parent {
                Some(parent) => &self.all[parent].children,
                None => &self.roots,
            };
            let found = siblings.iter().copied().find(|&at| self.all[at].entry == *entry);
            let at = match found {
                Some(at) => at,
                None => {
                    let at = self.all.len();
                    self.all.push(HitBranch {
                        entry: *entry,
                        changes: smallvec::SmallVec::new(),
                        children: smallvec::SmallVec::new(),
                        waiting: false,
                    });
                    match parent {
                        Some(parent) => self.all[parent].children.push(at),
                        None => self.roots.push(at),
                    }
                    at
                }
            };
            let branch = &mut self.all[at];
            branch.changes.push(change);
            branch.waiting |= waiting.get(index).copied().unwrap_or(false);
            parent = Some(at);
        }
    }
}

/// What a dispatch's changes go out with.
struct Delivery<'a> {
    live: &'a [LiveChange],
    kind: PointerEventType,
    actions: &'a mut Vec<Action>,
    requests: &'a mut Requests,
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
        let value = self.node_mut(node);
        let density = value.density;
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
        if value.kinds.any(NodeKinds::POINTER_INPUT | NodeKinds::DRAW | NodeKinds::DELEGATING) {
            for index in 0..value.chain.len() {
                let bits = value.chain[index].kinds;
                let size = value.layers[layer].measured.into();
                if bits.any(NodeKinds::DRAW | NodeKinds::DELEGATING)
                    && let Some(clip) = value.chain[index].hit_clip_at(size, density)
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
                            whole: modifier_node
                                .as_pointer_input()
                                .is_some_and(|handler| handler.takes_whole_events()),
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

    /// Dispatches a pointer event at `position` in the root, as `dispatch_pointers`.
    pub fn dispatch_pointer(&mut self, event: PointerEvent) -> Vec<Action> {
        self.dispatch_pointers(&[event])
    }

    /// Dispatches the changes of one platform event, each at its position in the root. A press
    /// captures its hit path: the pointer's moves and its last release go to the same nodes. A node
    /// on the paths of several hears them together (D1657), as `HitPathTracker`. Returns the actions
    /// handlers queued, to run once the tree is free.
    pub fn dispatch_pointers(&mut self, events: &[PointerEvent]) -> Vec<Action> {
        crate::nested_scroll::begin_pointer_event();
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        let all = self.with_resting_fingers(events);
        let mut live = Vec::with_capacity(all.len());
        let mut branches = HitBranches::default();
        // One pointer's path has nothing to share, and most events are one pointer's.
        let mut alone = None;
        for &(event, resting) in &all {
            let previous =
                self.pointer_states.iter().find(|it| it.0 == event.pointer).map(|it| it.1);
            // A wheel's turn leaves what's held as it was.
            let event = match (event.kind, previous) {
                (PointerEventType::Scroll, previous) => PointerEvent {
                    pressed: previous.is_some_and(|it| it.pressed),
                    buttons: previous.map_or(Default::default(), |it| it.buttons),
                    ..event
                },
                _ => event,
            };
            let position = event.position;
            let captured = self.captures.iter().position(|(pointer, _)| *pointer == event.pointer);
            // Another of a mouse's buttons: it stays with what the first one pressed.
            let already_down = event.pointer_type == crate::input::PointerType::Mouse
                && captured.is_some()
                && previous.is_some_and(|it| it.pressed);
            let path = match (event.kind, captured) {
                (PointerEventType::Press, Some(captured)) if already_down => {
                    self.captures[captured].1.clone()
                }
                (PointerEventType::Press, captured) => {
                    if let Some(captured) = captured {
                        self.captures.remove(captured);
                    }
                    let path = self.hit_test(position);
                    self.captures.push((event.pointer, path.clone()));
                    path
                }
                (PointerEventType::Release, Some(captured)) if !event.pressed => {
                    self.captures.remove(captured).1
                }
                (
                    PointerEventType::Release | PointerEventType::Move | PointerEventType::Unknown,
                    Some(captured),
                ) => self.captures[captured].1.clone(),
                _ => self.hit_test(position),
            };
            // A mouse's hover: what it left hears `Exit`, what it came over `Enter`, as Compose's
            // (not while a button is held: a drag stays with what it pressed).
            let is_mouse = event.pointer_type == crate::input::PointerType::Mouse;
            let hovering = is_mouse
                && event.kind != PointerEventType::Scroll
                && (!event.pressed || (event.kind == PointerEventType::Press && !already_down));
            if is_mouse && event.kind != PointerEventType::Scroll {
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
            if all.len() == 1 {
                alone = Some((path, waiting));
            } else {
                branches.add(&path, live.len(), &waiting);
            }
            live.push(LiveChange::new(event, previous, resting));
        }
        let kind = event_kind(events);
        let mut delivery =
            Delivery { live: &live, kind, actions: &mut actions, requests: &mut requests };
        if let Some((path, waiting)) = alone {
            self.deliver_along(&path, &waiting, &mut delivery);
        } else {
            for &root in &branches.roots {
                self.deliver_before_children(
                    &branches,
                    root,
                    PointerEventPass::Initial,
                    &mut delivery,
                );
            }
            for &root in &branches.roots {
                self.deliver_after_children(&branches, root, &mut delivery);
            }
            for &root in &branches.roots {
                self.deliver_before_children(
                    &branches,
                    root,
                    PointerEventPass::Final,
                    &mut delivery,
                );
            }
        }
        for change in &live {
            self.note_pointer_state(change);
        }
        self.apply_requests(requests, &mut actions);
        actions
    }

    /// `events` with every other finger down, as an event tells them all (a platform's says them;
    /// others send a finger at a time), in the order of their ids.
    fn with_resting_fingers(
        &self,
        events: &[PointerEvent],
    ) -> smallvec::SmallVec<[(PointerEvent, bool); 4]> {
        use crate::input::PointerType;
        let mut all: smallvec::SmallVec<[(PointerEvent, bool); 4]> =
            events.iter().map(|event| (*event, false)).collect();
        if let Some(first) = events.iter().find(|event| event.pointer_type != PointerType::Mouse) {
            for &(pointer, state) in &self.pointer_states {
                let told = events.iter().any(|event| event.pointer == pointer);
                if state.pressed && state.pointer_type != PointerType::Mouse && !told {
                    let resting = PointerEvent {
                        pointer,
                        pointer_type: state.pointer_type,
                        pressure: state.pressure,
                        buttons: first.buttons,
                        keyboard_modifiers: first.keyboard_modifiers,
                        time_nanos: first.time_nanos,
                        ..PointerEvent::moved(state.position, true)
                    };
                    all.push((resting, true));
                }
            }
            all.sort_by_key(|it| it.0.pointer.0);
        }
        all
    }

    /// Keeps what a pointer's next change will be told of this one: while it's down, or a mouse's,
    /// which is always somewhere, as `PointerInputChangeEventProducer`.
    fn note_pointer_state(&mut self, change: &LiveChange) {
        let event = &change.event;
        let kept = self.pointer_states.iter().position(|it| it.0 == event.pointer);
        if event.pressed || event.pointer_type == crate::input::PointerType::Mouse {
            let state = PointerState {
                position: event.position,
                pressed: event.pressed,
                uptime_millis: change.uptime_millis,
                buttons: event.buttons,
                pointer_type: event.pointer_type,
                pressure: event.pressure,
            };
            match kept {
                Some(kept) => self.pointer_states[kept].1 = state,
                None => self.pointer_states.push((event.pointer, state)),
            }
        } else if let Some(kept) = kept {
            self.pointer_states.swap_remove(kept);
        }
    }

    /// The only change of an event through the passes along its path, in the order of a branch's.
    fn deliver_along(&mut self, path: &[HitEntry], waiting: &[bool], delivery: &mut Delivery<'_>) {
        for entry in path {
            self.deliver_changes(entry, &[0], PointerEventPass::Initial, false, delivery);
        }
        // From the inside out; but one node's delegates in delegation order.
        let mut end = path.len();
        while end > 0 {
            let mut start = end - 1;
            while start > 0 && path[start].is_delegate_with(&path[start - 1]) {
                start -= 1;
            }
            for (index, entry) in path[start..end].iter().enumerate() {
                let waiting_parent = waiting.get(start + index).copied().unwrap_or(false);
                self.deliver_changes(entry, &[0], PointerEventPass::Main, waiting_parent, delivery);
            }
            end = start;
        }
        for entry in path {
            self.deliver_changes(entry, &[0], PointerEventPass::Final, false, delivery);
        }
    }

    /// `at` and what's below it in the initial or final pass: from the root down.
    fn deliver_before_children(
        &mut self,
        branches: &HitBranches,
        at: usize,
        pass: PointerEventPass,
        delivery: &mut Delivery<'_>,
    ) {
        let branch = &branches.all[at];
        self.deliver_changes(&branch.entry, &branch.changes, pass, false, delivery);
        for &child in &branch.children {
            self.deliver_before_children(branches, child, pass, delivery);
        }
    }

    /// `at` and what's below it in the main pass: from the inside out; but one node's delegates in
    /// delegation order, in every pass, as Compose's `dispatchForKind` in its hit path's node.
    fn deliver_after_children(
        &mut self,
        branches: &HitBranches,
        at: usize,
        delivery: &mut Delivery<'_>,
    ) {
        let mut last = at;
        while let [only] = branches.all[last].children[..]
            && branches.all[only].entry.is_delegate_with(&branches.all[last].entry)
        {
            last = only;
        }
        for &child in &branches.all[last].children {
            self.deliver_after_children(branches, child, delivery);
        }
        let mut current = at;
        loop {
            let branch = &branches.all[current];
            let pass = PointerEventPass::Main;
            self.deliver_changes(&branch.entry, &branch.changes, pass, branch.waiting, delivery);
            if current == last {
                break;
            }
            current = branch.children[0];
        }
    }

    /// Ends the gesture of `pointer`, without a release: a touch the system cancelled.
    pub fn cancel_pointer(&mut self, pointer: crate::input::PointerId) {
        self.pointer_states.retain(|it| it.0 != pointer);
        let Some(index) = self.captures.iter().position(|(captured, _)| *captured == pointer)
        else {
            return;
        };
        let (_, path) = self.captures.remove(index);
        for entry in path {
            // Told once: when its last pointer's gone.
            if self.captures.iter().any(|(_, other)| other.contains(&entry)) {
                continue;
            }
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

    /// Sends the changes of `which`, in a pass, to one entry of a hit path: together if it takes them
    /// so (`takes_whole_events`), else one at a time.
    fn deliver_changes(
        &mut self,
        entry: &HitEntry,
        which: &[usize],
        pass: PointerEventPass,
        waiting_parent: bool,
        delivery: &mut Delivery<'_>,
    ) {
        if self.get(entry.node).is_none_or(|value| value.chain_version != entry.version) {
            return;
        }
        // A block waiting in another pass: nothing to make its event for.
        if entry.whole
            && !self.node_mut(entry.node).chain[entry.index as usize]
                .resolve(entry.delegate)
                .and_then(|node| node.as_pointer_input())
                .is_some_and(|handler| handler.hears_pass(pass))
        {
            return;
        }
        let origin = self.layer_origin(entry.node, entry.layer as usize);
        let bounds = self.node(entry.node).layers[entry.layer as usize].measured;
        let Delivery { live, kind, actions, requests } = delivery;
        if entry.whole {
            let first = &live[which[0]].event;
            let event = crate::pointer_event::PointerEvents {
                changes: which.iter().map(|&at| live[at].change_at(origin)).collect(),
                buttons: first.buttons,
                keyboard_modifiers: first.keyboard_modifiers,
                kind: *kind,
            };
            let position = first.position;
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
                        handler.on_pointer_events(&event, pass, bounds, context);
                    }
                },
            );
            return;
        }
        for &at in which {
            let change = &live[at];
            // A pointer resting while another moves, comes down or up isn't a change to hear alone.
            if change.resting
                || (change.event.kind == PointerEventType::Move && *kind != PointerEventType::Move)
            {
                continue;
            }
            let Some(mut event) = change.as_primary_button() else { continue };
            let position = event.position;
            event.position = position - origin;
            event.consumed = change.consumed.get();
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
                        handler.on_pointer_event(&mut event, pass, bounds, context);
                    }
                },
            );
            change.consumed.set(event.consumed);
        }
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
        self.with_entry(node, index, |root, context| {
            let run = |modifier_node: &mut dyn crate::modifier::ModifierNode,
                       context: &mut crate::node_context::NodeContext<'_>| {
                let mut event_context = EventContext {
                    context: context.reborrow(),
                    actions,
                    requests,
                    pointer_position,
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
        let previous = self.pointer_states.iter().find(|it| it.0 == event.pointer).map(|it| it.1);
        let live: Vec<LiveChange> = entries
            .iter()
            .map(|_| {
                LiveChange::new(PointerEvent { kind, consumed: false, ..event }, previous, false)
            })
            .collect();
        let mut delivery = Delivery { live: &live, kind, actions, requests };
        for pass in [PointerEventPass::Initial, PointerEventPass::Main, PointerEventPass::Final] {
            let mut order: Vec<usize> = (0..entries.len()).collect();
            if pass == PointerEventPass::Main {
                order.reverse();
            }
            for index in order {
                self.deliver_changes(&entries[index], &[index], pass, false, &mut delivery);
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
        self.pointer_states.retain(|it| it.1.pressed);
        let mut event = PointerEvent::moved(Offset::new(-1.0, -1.0), false);
        event.kind = PointerEventType::Exit;
        self.update_hover(Vec::new(), event, &mut actions, &mut requests);
        self.apply_requests(requests, &mut actions);
        actions
    }

    /// Ends every pointer gesture in progress, as when the window loses the pointer.
    pub fn cancel_pointers(&mut self) {
        self.pointer_states.clear();
        let mut told: Vec<HitEntry> = Vec::new();
        for (_, path) in std::mem::take(&mut self.captures) {
            for entry in path {
                // Told once, whichever pointers it has.
                if told.contains(&entry) {
                    continue;
                }
                told.push(entry);
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
            self.clear_focus_with(false, true, crate::focus_manager::FocusDirection::Exit);
        }
        if let Some(forward) = requests.move_focus {
            let direction = if forward {
                crate::focus_manager::FocusDirection::Next
            } else {
                crate::focus_manager::FocusDirection::Previous
            };
            self.move_focus_with(direction, true);
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
            self.request_focus_at_or_below(node, crate::focus_manager::FocusDirection::Enter);
            if let Some(tap) = requests.focus_tap {
                self.tell_focus_tap(node, tap);
            }
        }
        if let Some(node) = requests.bring_into_view {
            self.bring_focused_into_view(node);
        }
        actions.append(&mut self.pending_actions);
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
        let focused = self.focused();
        let mut node = focused.or_else(|| self.modal_overlay());
        // From the last before the next target after the active one: what's below that target is
        // its, as `lastLocalNode`.
        let mut end = self.focus.active.filter(|_| focused.is_some()).and_then(|active| {
            let index = self.target_index(active)?;
            let chain = &self.node(active.node).chain;
            chain[index + 1..]
                .iter()
                .position(|entry| entry.kinds.any(NodeKinds::FOCUS_TARGET))
                .map(|next| index + 1 + next)
        });
        while let Some(id) = node {
            let value = self.node_mut(id);
            for index in (0..end.take().unwrap_or(value.chain.len())).rev() {
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
    pub fn dispatch_key(&mut self, event: KeyEvent) -> (bool, bool, Vec<Action>) {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        let mut consumed = false;
        let path = self.key_path();
        // The preview first, from the root down.
        for &(node, index, delegate) in path.iter().rev() {
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
                            .is_some_and(|handler| handler.on_pre_key_event(&event, context))
                    },
                )
                .unwrap_or(false);
            if consumed {
                break;
            }
        }
        let previewed = consumed;
        for (node, index, delegate) in path {
            if consumed {
                break;
            }
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
        }
        self.apply_requests(requests, &mut actions);
        // Tab everywhere; the others where the host says they move the focus (D230).
        let direction = event.to_focus_direction().filter(|_| {
            event.kind == KeyEventType::KeyDown
                && (event.key == Key::Tab || self.focus.keys_navigate)
        });
        if !consumed && let Some(direction) = direction {
            let (moved, moved_actions) = self.move_focus_in(direction);
            actions.extend(moved_actions);
            // A search waiting for a lazy list's next item asks for the frame laying it out.
            let waiting = self.has_beyond_bounds_moves();
            if waiting {
                self.needs_draw = true;
            }
            // Consumed if the focus moved, or if it was stopped from moving, as Compose's key
            // dispatch (`?: return true`, `if (!clearedFocusSuccessfully) return true`).
            consumed = moved || waiting || self.focus.move_stopped;
        }
        (consumed, previewed, actions)
    }

    /// The modifier nodes of `node` with semantics, or their delegates that have: with what they
    /// run in.
    fn semantics_entries(&mut self, node: NodeId) -> Vec<(usize, EntryId)> {
        let mut entries = Vec::new();
        let Some(value) = self.get_mut(node) else { return entries };
        for (index, entry) in value.chain.iter_mut().enumerate() {
            if !entry.kinds.any(NodeKinds::SEMANTICS) {
                continue;
            }
            dispatch_for_kind(
                &mut *entry.node,
                entry.own,
                NodeKinds::SEMANTICS,
                EntryId::NO_DELEGATE,
                &mut |_, delegate| entries.push((index, delegate)),
            );
        }
        entries
    }

    /// Asks `node`'s modifier nodes for `action`, until one does it (D1299). What it queues for
    /// the application runs as the tree is freed.
    pub(crate) fn perform_text_action(
        &mut self,
        node: NodeId,
        action: crate::modifier::TextAction,
    ) -> bool {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        let mut done = false;
        for (index, delegate) in self.semantics_entries(node) {
            done = self
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
                            .as_semantics()
                            .is_some_and(|it| it.perform_text_action(action.clone(), context))
                    },
                )
                .unwrap_or(false);
            if done {
                break;
            }
        }
        self.apply_requests(requests, &mut actions);
        self.pending_actions.append(&mut actions);
        done
    }

    /// Adds `node`'s modifier nodes' text layouts to `out`: whether one had any.
    pub(crate) fn text_layout_results(
        &mut self,
        node: NodeId,
        out: &mut Vec<crate::text::TextLayoutResult>,
    ) -> bool {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        let before = out.len();
        for (index, delegate) in self.semantics_entries(node) {
            let result = self.with_event_context(
                node,
                index,
                delegate,
                &mut actions,
                &mut requests,
                None,
                false,
                |modifier_node, context| {
                    modifier_node.as_semantics().and_then(|it| it.text_layout_result(context))
                },
            );
            out.extend(result.flatten());
        }
        out.len() > before
    }

    /// Where the first text laid out at or below `node` is in the root, as the origin of its
    /// `TextLayoutResult`'s coordinates.
    pub(crate) fn text_layout_origin(&mut self, node: NodeId) -> Option<Offset> {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        let mut nodes = vec![node];
        while let Some(node) = nodes.pop() {
            for (index, delegate) in self.semantics_entries(node) {
                let moved = self.with_event_context(
                    node,
                    index,
                    delegate,
                    &mut actions,
                    &mut requests,
                    None,
                    false,
                    |modifier_node, context| {
                        let text = modifier_node.as_semantics()?;
                        text.text_layout_result(context).map(|_| text.text_layout_offset())
                    },
                );
                if let Some(moved) = moved.flatten() {
                    let layer = self.node(node).layer_of(index);
                    let origin = self.layer_origin(node, layer);
                    return Some(Offset::new(origin.x - moved.x, origin.y - moved.y));
                }
            }
            nodes.extend(self.node(node).children.iter().rev().map(|child| child.node));
        }
        None
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
        self.text_input_node().map(|(_, options)| options)
    }

    /// Whether the focused node taking text masks it, from its own configuration: what's around it
    /// clearing its semantics, or a service's tree leaving it out, changes nothing for a keyboard.
    pub fn text_input_masks_password(&mut self) -> bool {
        let Some((field, _)) = self.text_input_node() else { return false };
        let mut next = Some(field);
        while let Some(node) = next {
            next = self.node(node).parent;
            if let Some(own) = self.own_semantics(node)
                && own.config.contains(&crate::semantics::SemanticsProperties::EDITABLE_TEXT)
            {
                return own.config.contains(&crate::semantics::SemanticsProperties::PASSWORD);
            }
        }
        false
    }

    /// The focused node taking text, or what's around it that does, and what it asks of a keyboard.
    fn text_input_node(&mut self) -> Option<(NodeId, crate::text_input::ImeOptions)> {
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
                if let Some(options) = options {
                    return Some((node, options));
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

    pub(crate) fn dispatch_to_key_path(
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
