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

/// Appends the entries of `paths` (each from the root down) to `order` as Compose's hit path
/// tree cancels them (D1931): the paths merged where they start alike, each node after the
/// nodes below it (`Node.dispatchCancel`: its children, then itself), a node told once.
fn cancel_order(paths: &[&[HitEntry]], order: &mut Vec<HitEntry>) {
    let mut firsts: Vec<HitEntry> = Vec::new();
    for path in paths {
        if let Some(first) = path.first()
            && !firsts.contains(first)
        {
            firsts.push(*first);
        }
    }
    for first in firsts {
        let below: Vec<&[HitEntry]> = paths
            .iter()
            .filter(|path| path.first() == Some(&first))
            .map(|path| &path[1..])
            .collect();
        cancel_order(&below, order);
        if !order.contains(&first) {
            order.push(first);
        }
    }
}

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
    /// One change at a time, but what starts it goes as `firstDownRefersToPrimaryMouseButtonOnly`.
    pub(crate) follows_first_down: bool,
    /// One change at a time, with each of a mouse's buttons (`hears_every_button`).
    pub(crate) every_button: bool,
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
    /// A mouse's press that said no button: its only one is taken as primary while it's down.
    primary_presumed: bool,
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
        change.wheel_step = event.wheel_step;
        change.pan_offset = event.pan_offset;
        change.scale_factor = event.scale_factor;
        change.sharing(self.consumed.clone())
    }

    /// The event for what hears a pointer one change at a time: for a mouse, its primary button
    /// alone. What another button does isn't an event; it's down only while the primary is.
    fn as_primary_button(&self) -> Option<PointerEvent> {
        let event = self.event;
        if event.pointer_type != crate::input::PointerType::Mouse {
            return Some(event);
        }
        let now = event.buttons.is_primary_pressed() || self.primary_presumed();
        let before =
            self.previous.is_some_and(|it| it.buttons.is_primary_pressed() || it.primary_presumed);
        // The primary button changing is what's a press or a release here, whatever the platform
        // calls the event it comes in: Android's come as a move or `ACTION_BUTTON_*`.
        let was_down = self.previous.is_some_and(|it| it.pressed);
        let kind = match (event.kind, before, now) {
            (PointerEventType::Press | PointerEventType::Unknown, false, true) => {
                PointerEventType::Press
            }
            (PointerEventType::Move, false, true) if was_down => PointerEventType::Press,
            (
                PointerEventType::Release | PointerEventType::Unknown | PointerEventType::Move,
                true,
                false,
            ) => PointerEventType::Release,
            (
                PointerEventType::Press | PointerEventType::Release | PointerEventType::Unknown,
                _,
                _,
            ) => return None,
            (kind, _, _) => kind,
        };
        Some(PointerEvent { kind, pressed: now, ..event })
    }
}

impl LiveChange {
    /// A mouse's press that says no button, and what follows of it saying none.
    fn primary_presumed(&self) -> bool {
        let event = &self.event;
        event.pressed
            && !event.buttons.are_any_pressed()
            && (event.kind == PointerEventType::Press
                || self.previous.is_some_and(|it| it.primary_presumed))
    }

    /// The event for what starts at any of a mouse's buttons, as Compose's on Android: its pointer
    /// comes down with the first and up with the last.
    fn as_any_button(&self) -> Option<PointerEvent> {
        let event = self.event;
        if event.pointer_type != crate::input::PointerType::Mouse {
            return Some(event);
        }
        let before = self.previous.is_some_and(|it| it.pressed);
        let kind = match (event.kind, before, event.pressed) {
            (PointerEventType::Press, false, true) => PointerEventType::Press,
            (PointerEventType::Release, true, false) => PointerEventType::Release,
            (
                PointerEventType::Press | PointerEventType::Release | PointerEventType::Unknown,
                _,
                _,
            ) => return None,
            (kind, _, _) => kind,
        };
        Some(PointerEvent { kind, ..event })
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
                    && let Some(outline) =
                        value.chain[index].hit_outline(size, density, value.layout_direction)
                    && !outline.contains(local - origin)
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
                            follows_first_down: modifier_node
                                .as_pointer_input()
                                .is_some_and(|handler| handler.first_down_follows_the_host()),
                            every_button: modifier_node
                                .as_pointer_input()
                                .is_some_and(|handler| handler.hears_every_button()),
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
        let all = self.with_resting_pointers(events);
        let mut live: smallvec::SmallVec<[LiveChange; 2]> = smallvec::SmallVec::new();
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
            let mut previous = previous;
            // A pan or pinch starting while a button is held ends that press first: what it
            // pressed is cancelled, as Compose's fake fingers take the pointer over (a host
            // releases the buttons first; this is for one that doesn't).
            if matches!(event.kind, PointerEventType::PanStart | PointerEventType::ScaleStart)
                && previous.is_some_and(|it| it.pressed)
            {
                self.cancel_pointer(event.pointer);
                previous = None;
            }
            let captured = self.captures.iter().position(|(pointer, _)| *pointer == event.pointer);
            // Another of a mouse's buttons: it stays with what the first one pressed.
            let already_down = event.pointer_type == crate::input::PointerType::Mouse
                && captured.is_some()
                && previous.is_some_and(|it| it.pressed);
            // A trackpad's pan or pinch keeps what it starts on, as a press does: Compose's is a
            // stream of fake fingers.
            let path = match (event.kind, captured) {
                (PointerEventType::Press, Some(captured)) if already_down => {
                    self.captures[captured].1.clone()
                }
                (
                    PointerEventType::Press
                    | PointerEventType::PanStart
                    | PointerEventType::ScaleStart,
                    captured,
                ) => {
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
                (PointerEventType::PanEnd | PointerEventType::ScaleEnd, Some(captured)) => {
                    self.captures.remove(captured).1
                }
                (
                    PointerEventType::Release
                    | PointerEventType::Move
                    | PointerEventType::Unknown
                    | PointerEventType::PanMove
                    | PointerEventType::ScaleChange,
                    Some(captured),
                ) => self.captures[captured].1.clone(),
                _ => self.hit_test(position),
            };
            // A mouse's hover: what it left hears `Exit`, what it came over `Enter`, as Compose's
            // (not while a button is held: a drag stays with what it pressed; nor in a pinch,
            // whose fingers don't hover).
            let is_mouse = event.pointer_type == crate::input::PointerType::Mouse;
            let hovering = is_mouse
                && event.kind != PointerEventType::Scroll
                && !event.kind.is_scale()
                && (!event.pressed || (event.kind == PointerEventType::Press && !already_down));
            if is_mouse && event.kind != PointerEventType::Scroll {
                self.mouse = (!event.pressed).then_some(position);
                if event.kind.is_pan() {
                    self.panning = event.kind != PointerEventType::PanEnd;
                }
            }
            // A stylus's hover, as the mouse's (D1677): a press ends it, as Android's hover
            // exit before a stylus touches down.
            let stylus_hovering = !is_mouse
                && matches!(
                    event.pointer_type,
                    crate::input::PointerType::Stylus | crate::input::PointerType::Eraser
                )
                && (self.stylus.is_some() || !event.pressed);
            let mut path = path;
            if stylus_hovering {
                let hovers = !event.pressed && event.kind == PointerEventType::Move;
                self.stylus = hovers.then_some(event);
                let now = if hovers { path.clone() } else { Vec::new() };
                let hover = PointerEvent { pressed: false, ..event };
                let entered = self.update_hover(now, hover, true, &mut actions, &mut requests);
                if !entered.is_empty() {
                    path.retain(|entry| !entered.contains(entry));
                }
            }
            if hovering {
                let now = if event.kind.is_pan() && captured.is_some() {
                    self.still_under(&path, position)
                } else if event.kind == PointerEventType::Release || captured.is_some() {
                    self.hit_test(position)
                } else {
                    path.clone()
                };
                let entered = self.update_hover(now, event, false, &mut actions, &mut requests);
                // A hover move hears `Enter` where it comes in, not the move too, as Compose's
                // `HitPathTracker` makes the move an `Enter` there.
                if event.kind == PointerEventType::Move && !entered.is_empty() {
                    path.retain(|entry| !entered.contains(entry));
                }
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
                self.deliver_initial_and_main(&branches, root, &mut delivery);
            }
            for &root in &branches.roots {
                self.deliver_final(&branches, root, &mut delivery);
            }
        }
        for change in &live {
            self.note_pointer_state(change);
        }
        self.apply_requests(requests, &mut actions);
        actions
    }

    /// `events` with every other pointer down, as an event tells them all (a platform's says them;
    /// others send one at a time), in the order of their ids.
    fn with_resting_pointers(
        &self,
        events: &[PointerEvent],
    ) -> smallvec::SmallVec<[(PointerEvent, bool); 4]> {
        let mut all: smallvec::SmallVec<[(PointerEvent, bool); 4]> =
            events.iter().map(|event| (*event, false)).collect();
        let Some(first) = events.first() else { return all };
        let before = all.len();
        for &(pointer, state) in &self.pointer_states {
            if state.pressed && !events.iter().any(|event| event.pointer == pointer) {
                let resting = PointerEvent {
                    pointer,
                    pointer_type: state.pointer_type,
                    pressure: state.pressure,
                    buttons: state.buttons,
                    keyboard_modifiers: first.keyboard_modifiers,
                    time_nanos: first.time_nanos,
                    ..PointerEvent::moved(state.position, true)
                };
                all.push((resting, true));
            }
        }
        if all.len() > before || before > 1 {
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
                primary_presumed: change.primary_presumed(),
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

    /// `at` and what's below it, as `Node.dispatchMainEventPass`: the initial pass going in, each child
    /// whole, and the main pass coming out, so a branch ends before the next starts; one node's
    /// delegates, each pass, in delegation order.
    fn deliver_initial_and_main(
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
        let mut current = at;
        loop {
            let branch = &branches.all[current];
            let pass = PointerEventPass::Initial;
            self.deliver_changes(&branch.entry, &branch.changes, pass, false, delivery);
            if current == last {
                break;
            }
            current = branch.children[0];
        }
        for &child in &branches.all[last].children {
            self.deliver_initial_and_main(branches, child, delivery);
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

    /// `at` and what's below it in the final pass: from the root down.
    fn deliver_final(&mut self, branches: &HitBranches, at: usize, delivery: &mut Delivery<'_>) {
        let branch = &branches.all[at];
        let pass = PointerEventPass::Final;
        self.deliver_changes(&branch.entry, &branch.changes, pass, false, delivery);
        for &child in &branch.children {
            self.deliver_final(branches, child, delivery);
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
        // Innermost first, as Compose's `Node.dispatchCancel` (D1931).
        for entry in path.into_iter().rev() {
            // Compose has no cancel of one pointer: a node's pointers go together, and it hears the
            // others no more until a new press.
            for (_, other) in &mut self.captures {
                other.retain(|it| *it != entry);
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
            // A pointer resting while another moves, comes down or up isn't a change to hear alone;
            // one that moved in the same event is (D2130).
            if change.resting
                || (change.event.kind == PointerEventType::Move
                    && *kind != PointerEventType::Move
                    && change.previous.is_some_and(|it| it.position == change.event.position))
            {
                continue;
            }
            let view = if entry.every_button
                || (entry.follows_first_down && !self.first_down_primary_only)
            {
                change.as_any_button()
            } else {
                change.as_primary_button()
            };
            let Some(mut event) = view else { continue };
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

    /// Moves the mouse's hover (a hovering `stylus`'s) to the entries of `now`: `Exit` to those
    /// it left, `Enter` to those it came over (in the main pass).
    /// Returns the entries it came over.
    fn update_hover(
        &mut self,
        now: Vec<HitEntry>,
        event: PointerEvent,
        stylus: bool,
        actions: &mut Vec<Action>,
        requests: &mut Requests,
    ) -> Vec<HitEntry> {
        let before =
            std::mem::take(if stylus { &mut self.stylus_hovered } else { &mut self.hovered });
        let same = |a: &HitEntry, b: &HitEntry| {
            a.node == b.node && a.index == b.index && a.version == b.version
        };
        let left: Vec<HitEntry> =
            before.iter().filter(|old| !now.iter().any(|new| same(old, new))).cloned().collect();
        let entered: Vec<HitEntry> =
            now.iter().filter(|new| !before.iter().any(|old| same(old, new))).cloned().collect();
        self.deliver_hover(&left, PointerEventType::Exit, event, actions, requests);
        self.deliver_hover(&entered, PointerEventType::Enter, event, actions, requests);
        if stylus {
            self.stylus_hovered = now;
        } else {
            self.hovered = now;
        }
        entered
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

    /// The mouse pointer's look where it is, as Compose's `PointerIconService.getIcon`: the
    /// innermost hovered node's icon, unless one around it overrides its descendants (the
    /// outermost such one's), the arrow over none (D1676).
    pub fn pointer_icon(&mut self) -> crate::pointer_icon::PointerIcon {
        if !self.has_hover_icons {
            return crate::pointer_icon::PointerIcon::DEFAULT;
        }
        let hovered = std::mem::take(&mut self.hovered);
        let icon = self.hover_icon_along(&hovered, false);
        self.hovered = hovered;
        icon.unwrap_or_default()
    }

    /// A hovering stylus's look where it is, as Compose's `PointerIconService.getStylusHoverIcon`:
    /// as `pointer_icon`, but `None` over no `stylus_hover_icon` (the platform's own then).
    pub fn stylus_hover_icon(&mut self) -> Option<crate::pointer_icon::PointerIcon> {
        if !self.has_hover_icons || self.stylus_hovered.is_empty() {
            return None;
        }
        let hovered = std::mem::take(&mut self.stylus_hovered);
        let icon = self.hover_icon_along(&hovered, true);
        self.stylus_hovered = hovered;
        icon
    }

    /// The icon `path` (outer to inner) shows for the mouse or a `stylus`: the outermost
    /// overriding its descendants, else the innermost.
    fn hover_icon_along(
        &mut self,
        path: &[HitEntry],
        stylus: bool,
    ) -> Option<crate::pointer_icon::PointerIcon> {
        let mut icon = None;
        for entry in path {
            let Some(value) = self.get_mut(entry.node) else { continue };
            if value.chain_version != entry.version {
                continue;
            }
            let Some(hover) = value.chain[entry.index as usize]
                .resolve(entry.delegate)
                .and_then(|node| node.as_pointer_input())
                .and_then(|node| node.hover_icon())
                .filter(|hover| hover.stylus == stylus)
            else {
                continue;
            };
            icon = Some(hover.icon.clone());
            if hover.override_descendants {
                break;
            }
        }
        icon
    }

    /// What of `path` is still under `position`: a pan comes over nothing new, as Compose's
    /// suppresses its hit tests; what scrolls away from under the cursor exits.
    fn still_under(&mut self, path: &[HitEntry], position: Offset) -> Vec<HitEntry> {
        let under = self.hit_test(position);
        path.iter().filter(|entry| under.contains(entry)).copied().collect()
    }

    /// Hit-tests the resting mouse again (after a layout moved what's under it): what it left
    /// hears `Exit`, what it's now over `Enter` (nothing new during a trackpad's pan).
    pub fn refresh_hover(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        // Nothing hovers: nothing to do, as most frames on a phone.
        if self.mouse.is_none() && self.stylus.is_none() {
            return actions;
        }
        let mut requests = Requests::default();
        if let Some(position) = self.mouse {
            let now = if self.panning {
                let hovered = self.hovered.clone();
                self.still_under(&hovered, position)
            } else {
                self.hit_test(position)
            };
            let event = PointerEvent::moved(position, false);
            self.update_hover(now, event, false, &mut actions, &mut requests);
        }
        // A hovering stylus too (D1677).
        if let Some(event) = self.stylus {
            let now = self.hit_test(event.position);
            self.update_hover(now, event, true, &mut actions, &mut requests);
        }
        self.apply_requests(requests, &mut actions);
        actions
    }

    /// The hovering stylus went out of range (or out of the window): what it was over hears
    /// `Exit` (D1677).
    pub fn stylus_left(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(mut event) = self.stylus.take() else { return actions };
        let mut requests = Requests::default();
        event.kind = PointerEventType::Exit;
        self.update_hover(Vec::new(), event, true, &mut actions, &mut requests);
        self.apply_requests(requests, &mut actions);
        actions
    }

    /// The mouse left the window: what it was over hears `Exit`.
    pub fn pointer_left(&mut self) -> Vec<Action> {
        self.pointer_left_with(crate::pointer_event::PointerKeyboardModifiers::default())
    }

    /// As [`pointer_left`](Self::pointer_left), the exit telling `keyboard_modifiers` (D2131).
    pub fn pointer_left_with(
        &mut self,
        keyboard_modifiers: crate::pointer_event::PointerKeyboardModifiers,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut requests = Requests::default();
        self.mouse = None;
        self.panning = false;
        self.pointer_states.retain(|it| it.1.pressed);
        let mut event = PointerEvent::moved(Offset::new(-1.0, -1.0), false)
            .with_keyboard_modifiers(keyboard_modifiers);
        event.kind = PointerEventType::Exit;
        self.update_hover(Vec::new(), event, false, &mut actions, &mut requests);
        self.apply_requests(requests, &mut actions);
        actions
    }

    /// Ends every pointer gesture in progress, as when the window loses the pointer.
    pub fn cancel_pointers(&mut self) {
        self.pointer_states.clear();
        self.panning = false;
        let captures = std::mem::take(&mut self.captures);
        let paths: Vec<&[HitEntry]> = captures.iter().map(|(_, path)| &path[..]).collect();
        let mut told: Vec<HitEntry> = Vec::new();
        cancel_order(&paths, &mut told);
        for entry in told {
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
        self.sync_noted_delegates();
    }

    pub(crate) fn apply_requests(&mut self, requests: Requests, actions: &mut Vec<Action>) {
        self.sync_noted_delegates();
        for node in requests.draw {
            self.invalidate_draw_of(node);
        }
        self.input_method_reset |= requests.reset_input_method;
        self.has_hover_icons |= requests.hover_icons;
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
        if !requests.bring_into_view.is_empty() {
            self.bring_focused_into_view(requests.bring_into_view);
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
        let focused = self.focus.active;
        let start = focused.map(|active| active.node).or_else(|| self.modal_overlay());
        self.kind_path(start, focused, NodeKinds::KEY_INPUT)
    }

    /// The nodes of `kind` from `start` up, innermost first, each node's delegates of the kind in
    /// delegation order: in `target`'s layout node, those before the next target after it (what's
    /// below that target is its, as `lastLocalNode`). It ends at an overlay.
    fn kind_path(
        &mut self,
        start: Option<NodeId>,
        target: Option<crate::focus::FocusTargetRef>,
        kind: NodeKinds,
    ) -> Vec<(NodeId, usize, EntryId)> {
        let mut path = Vec::new();
        let mut node = start;
        let mut end = target.and_then(|active| {
            let index = self.target_index(active)?;
            let chain = &self.node(active.node).chain;
            chain[index + 1..]
                .iter()
                .position(|entry| entry.kinds.any(NodeKinds::FOCUS_TARGET))
                .map(|next| index + 1 + next)
        });
        while let Some(id) = node {
            let value = self.node_mut(id);
            let end = end.take().unwrap_or(value.chain.len());
            if value.kinds.any(kind) {
                for index in (0..end).rev() {
                    let entry = &mut value.chain[index];
                    if !entry.kinds.any(kind) {
                        continue;
                    }
                    // The node, or each of its delegates of the kind.
                    dispatch_for_kind(
                        &mut *entry.node,
                        entry.own,
                        kind,
                        EntryId::NO_DELEGATE,
                        &mut |_, delegate| path.push((id, index, delegate)),
                    );
                }
            }
            node = if value.overlay { None } else { value.parent };
        }
        path
    }

    // ---- Indirect pointers (D1675) ---------------------------------------------------------

    /// The indirect pointer input nodes on `target`'s path, innermost first: its own (up to the
    /// next target in its chain) and its ancestors', as Compose's `focusedIndirectPointerInputNode`
    /// and its `ancestors`; the chain entries' ids, which outlive a move.
    fn indirect_path_of(
        &mut self,
        target: Option<crate::focus::FocusTargetRef>,
    ) -> Vec<(NodeId, EntryId, EntryId)> {
        let Some(active) = target.filter(|active| self.get(active.node).is_some()) else {
            return Vec::new();
        };
        let path =
            self.kind_path(Some(active.node), Some(active), NodeKinds::INDIRECT_POINTER_INPUT);
        path.into_iter()
            .map(|(node, index, delegate)| (node, self.node(node).chain[index].id, delegate))
            .collect()
    }

    /// Runs `f` on the indirect pointer input node of `entry` (a chain entry's id), if it's still
    /// there.
    fn with_indirect_node(
        &mut self,
        (node, entry, delegate): (NodeId, EntryId, EntryId),
        actions: &mut Vec<Action>,
        requests: &mut Requests,
        f: impl FnOnce(
            &mut dyn crate::indirect_pointer::IndirectPointerInputModifierNode,
            &mut EventContext<'_>,
        ),
    ) {
        let Some(index) =
            self.get(node).and_then(|value| value.chain.iter().position(|it| it.id == entry))
        else {
            return;
        };
        self.with_event_context(node, index, delegate, actions, requests, None, false, |n, c| {
            if let Some(handler) = n.as_indirect_pointer_input() {
                f(handler, c);
            }
        });
    }

    /// The focus moved from `previous`: the indirect pointer input nodes no longer on its path are
    /// told their stream ended, innermost first, as `AndroidComposeView.onFocusChanged`.
    pub(crate) fn indirect_focus_moved(&mut self, previous: Option<crate::focus::FocusTargetRef>) {
        let mut before = std::mem::take(&mut self.indirect_focus_path);
        for entry in self.indirect_path_of(previous) {
            if !before.contains(&entry) {
                before.push(entry);
            }
        }
        let after = self.indirect_path_of(self.focus.active);
        let (mut actions, mut requests) = (Vec::new(), Requests::default());
        for entry in before {
            if !after.contains(&entry) {
                self.with_indirect_node(entry, &mut actions, &mut requests, |node, _| {
                    node.on_cancel_indirect_pointer_input()
                });
            }
        }
        self.indirect_focus_path = after;
        self.apply_requests(requests, &mut actions);
        self.pending_actions.append(&mut actions);
    }

    /// Entries removed from `node`'s chain: those on the focus's path are told their stream ended
    /// as they go, as Compose's are when the focus leaves them with their node.
    pub(crate) fn indirect_entries_removed(
        &mut self,
        node: NodeId,
        entries: &mut [(crate::node_chain::ChainEntry, usize)],
    ) {
        for (entry, _) in entries.iter_mut() {
            if !entry.kinds.any(NodeKinds::INDIRECT_POINTER_INPUT) {
                continue;
            }
            let id = entry.id;
            let path = &mut self.indirect_focus_path;
            let on_path: Vec<EntryId> =
                path.iter().filter(|it| it.0 == node && it.1 == id).map(|it| it.2).collect();
            path.retain(|it| !(it.0 == node && it.1 == id));
            if !on_path.is_empty() {
                dispatch_for_kind(
                    &mut *entry.node,
                    entry.own,
                    NodeKinds::INDIRECT_POINTER_INPUT,
                    EntryId::NO_DELEGATE,
                    &mut |delegate_node, delegate| {
                        if on_path.contains(&delegate)
                            && let Some(handler) = delegate_node.as_indirect_pointer_input()
                        {
                            handler.on_cancel_indirect_pointer_input();
                        }
                    },
                );
            }
        }
    }

    /// Sends an indirect pointer event to what has the focus, as Compose's
    /// `FocusOwnerImpl.dispatchIndirectPointerEvent`: its indirect pointer input nodes, `Initial`
    /// from the outermost in, `Main` back out, `Final` in again (a node's delegates each pass in
    /// delegation order). Then, as `AndroidComposeView`, the navigation gesture detector: a fling
    /// along the primary axis moves the focus. Returns whether a change was consumed, and the
    /// queued actions.
    pub fn dispatch_indirect_pointer(
        &mut self,
        event: &crate::indirect_pointer::IndirectPointerEvent,
    ) -> (bool, Vec<Action>) {
        let (mut actions, mut requests) = (Vec::new(), Requests::default());
        // Not while the focus system has invalidations to resolve, as Compose's (b/379289347):
        // the detector still hears it, unconsumed.
        let pending = self.focus.recheck_active
            || !self.focus.invalidated_events.is_empty()
            || !self.focus.target_flips.is_empty();
        let path = if pending { Vec::new() } else { self.indirect_path_of(self.focus.active) };
        // A node's delegates together.
        let mut groups: Vec<std::ops::Range<usize>> = Vec::new();
        for (at, entry) in path.iter().enumerate() {
            match groups.last_mut() {
                Some(group) if path[group.start].0 == entry.0 && path[group.start].1 == entry.1 => {
                    group.end = at + 1
                }
                _ => groups.push(at..at + 1),
            }
        }
        for pass in [PointerEventPass::Initial, PointerEventPass::Main, PointerEventPass::Final] {
            let mut order: Vec<&std::ops::Range<usize>> = groups.iter().collect();
            if pass != PointerEventPass::Main {
                order.reverse();
            }
            for group in order {
                for &entry in &path[group.clone()] {
                    self.with_indirect_node(entry, &mut actions, &mut requests, |node, context| {
                        node.on_indirect_pointer_event(event, pass, context)
                    });
                }
            }
        }
        if !pending {
            self.indirect_focus_path = path;
        }
        self.apply_requests(requests, &mut actions);
        let consumed = !pending && event.is_consumed();
        let configuration = self.view_configuration();
        let direction = self
            .indirect_navigation
            .get_or_insert_with(Default::default)
            .on_indirect_pointer_event(event, consumed, &configuration);
        if let Some(direction) = direction {
            self.move_focus_with(direction, false);
        }
        // What a focus change queued (a cancel's, the focus's events), with or without a fling.
        actions.append(&mut self.pending_actions);
        (consumed, actions)
    }

    /// Ends the indirect pointer stream: what has the focus is told, innermost first, as
    /// `dispatchIndirectPointerCancel`, and the navigation detector ignores the rest of it.
    pub fn cancel_indirect_pointer(&mut self) -> Vec<Action> {
        let (mut actions, mut requests) = (Vec::new(), Requests::default());
        for entry in self.indirect_path_of(self.focus.active) {
            self.with_indirect_node(entry, &mut actions, &mut requests, |node, _| {
                node.on_cancel_indirect_pointer_input()
            });
        }
        if let Some(detector) = &mut self.indirect_navigation {
            detector.cancel_current_event_stream();
        }
        self.apply_requests(requests, &mut actions);
        actions
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

    /// Sends an input method's edits to the focused node and its ancestors until one takes them.
    pub fn dispatch_edit_commands(
        &mut self,
        commands: &[&dyn crate::text_editing::EditCommand],
    ) -> (bool, Vec<Action>) {
        self.dispatch_to_key_path(|handler, context| handler.on_edit_commands(commands, context))
    }

    /// What an input method's edits apply to in the focused node taking them.
    pub fn input_method_value(&mut self) -> Option<crate::text_editing::TextFieldValue> {
        self.first_on_key_path(|handler| handler.input_method_value())
    }

    /// Whether `value` is what the focused node answers; `None` if it keeps no such state.
    pub fn input_method_value_is(
        &mut self,
        value: &crate::text_editing::TextFieldValue,
    ) -> Option<bool> {
        self.first_on_key_path(|handler| handler.input_method_value_is(value))
    }

    /// What the focused field's cursor anchor depends on, with where its layer is in the root.
    pub fn cursor_anchor_key(&mut self) -> Option<(crate::text_input::CursorAnchorKey, Offset)> {
        let mut next = self.focused();
        while let Some(node) = next {
            next = self.node(node).parent;
            for index in (0..self.node(node).chain.len()).rev() {
                if !self.node(node).chain[index].kinds.any(NodeKinds::KEY_INPUT) {
                    continue;
                }
                let layer = self.node(node).layer_of(index);
                let key = self.get_mut(node)?.chain[index]
                    .node
                    .as_key_input()
                    .and_then(|handler| handler.cursor_anchor_key());
                if let Some(key) = key {
                    return Some((key, self.layer_origin(node, layer)));
                }
            }
        }
        None
    }

    /// Whether a focused node's platform preedit was ended other than by the platform.
    pub fn take_preedit_interrupted(&mut self) -> bool {
        let mut interrupted = false;
        self.first_on_key_path(|handler| {
            interrupted |= handler.take_preedit_interrupted();
            None::<()>
        });
        interrupted
    }

    /// Whether the focused nodes' input method must start over (each one's flag is taken).
    pub fn take_input_method_restart(&mut self) -> bool {
        let mut restart = false;
        self.first_on_key_path(|handler| {
            restart |= handler.take_input_method_restart();
            None::<()>
        });
        restart
    }

    /// Where the focused field's text is for an input method, its origin in the root.
    pub fn cursor_anchor(&mut self, lines: bool) -> Option<crate::text_input::CursorAnchor> {
        let mut anchor = None;
        let mut found = None;
        let _ = self.dispatch_to_key_path(|handler, context| {
            if found.is_none() {
                anchor = handler.cursor_anchor(lines, context);
                if anchor.is_some() {
                    found = Some(context.node);
                }
            }
            found.is_some()
        });
        let node = found?;
        let mut anchor = anchor?;
        for index in (0..self.node(node).chain.len()).rev() {
            if self.node(node).chain[index].kinds.any(NodeKinds::KEY_INPUT) {
                let layer = self.node(node).layer_of(index);
                let origin = self.layer_origin(node, layer);
                anchor.origin = anchor.origin + origin;
                break;
            }
        }
        Some(anchor)
    }

    /// `find` over the key input nodes on the focused node's path, the nearest first, until one
    /// answers.
    fn first_on_key_path<R>(
        &mut self,
        mut find: impl FnMut(&mut dyn crate::modifier::KeyInputModifierNode) -> Option<R>,
    ) -> Option<R> {
        let mut next = self.focused();
        while let Some(node) = next {
            next = self.node(node).parent;
            for index in (0..self.node(node).chain.len()).rev() {
                if !self.node(node).chain[index].kinds.any(NodeKinds::KEY_INPUT) {
                    continue;
                }
                if let Some(handler) = self.get_mut(node)?.chain[index].node.as_key_input()
                    && let Some(found) = find(handler)
                {
                    return Some(found);
                }
            }
        }
        None
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
