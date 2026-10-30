//! Selecting non-editable text, as Compose's `SelectionContainer`: a press and drag inside a
//! container selects from where it went down to the pointer, across the texts in it (in
//! order); a double-click selects a word, a triple-click a whole text; Cmd+A (Ctrl+A) selects
//! all and Cmd+C (Ctrl+C) copies, the texts joined by new lines. A click elsewhere in it, or
//! the container losing focus, clears the selection. `DisableSelection` leaves its content out.
//!
//! The texts are `Selectable` modifier nodes (foundation's `BasicText`); the tree finds them
//! below the container when a gesture asks, as it does focus targets.

use std::ops::Range;

use crate::input::{
    EventContext, Key, KeyEvent, KeyEventType, PointerEvent, PointerEventPass, PointerEventType,
};
use crate::modifier::{
    FocusTargetModifierNode, KeyInputModifierNode, Modifier, ModifierNode, ModifierNodeElement,
    PointerInputModifierNode,
};
use crate::node_chain::NodeKinds;
use crate::text::{SharedString, TextMeasurer};
use crate::tree::{NodeId, UiTree};
use crate::unit::{IntSize, Offset};

/// A text a `SelectionContainer` can select in, as Compose's `Selectable`: a modifier node
/// returning itself from `ModifierNode::as_selectable`. Offsets are byte offsets at character
/// boundaries; positions are in the node's coordinates.
pub trait Selectable {
    /// Its text: what's selected and copied.
    fn text(&self) -> SharedString;
    /// The offset nearest to `position`, from the measurer (`None` before it's laid out).
    fn offset_at(&self, position: Offset, measurer: &mut dyn TextMeasurer) -> Option<usize>;
    /// The word at `offset`, as `getWordBoundary` (`platform::word_boundary`): what a
    /// double-click selects.
    fn word_at(&self, offset: usize) -> Range<usize>;
    /// Bumped when its text changes: a selection ending in it is dropped then.
    fn version(&self) -> u64 {
        0
    }
    /// Shows `range` selected (highlighted), or nothing. Returns whether that changed.
    fn set_selection(
        &mut self,
        range: Option<Range<usize>>,
        measurer: &mut dyn TextMeasurer,
    ) -> bool;
}

/// What a press selects, and a drag after it extends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Unit {
    Character,
    Word,
    /// A whole text.
    Text,
}

/// A selection gesture or command, handled by the tree once the event's dispatch is over.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum SelectionRequest {
    /// A press at `position` (in the root) starts a selection.
    Start {
        container: NodeId,
        position: Offset,
        unit: Unit,
    },
    /// The pointer, dragged to `position`, moves the selection's end there.
    Extend {
        container: NodeId,
        position: Offset,
    },
    SelectAll {
        container: NodeId,
    },
    Copy {
        container: NodeId,
    },
    Clear {
        container: NodeId,
    },
}

/// A point in a container's texts: a text (its node, the chain index of its selectable, and
/// the version of its text) and an offset in it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Point {
    node: NodeId,
    index: usize,
    version: u64,
    offset: usize,
}

/// The selection in a container: from `anchor` (where it started) to `focus` (where it ends).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct TextSelection {
    container: NodeId,
    anchor: Point,
    focus: Point,
    /// What the press selected (a word, a text): a drag keeps it whole.
    initial: (Point, Point),
    unit: Unit,
}

/// A text below a container: its node, the chain index of its selectable, its bounds in the
/// root, and its text's version.
#[derive(Clone, Copy, Debug)]
struct Entry {
    node: NodeId,
    index: usize,
    origin: Offset,
    size: IntSize,
    version: u64,
}

impl Entry {
    fn point(&self, offset: usize) -> Point {
        Point { node: self.node, index: self.index, version: self.version, offset }
    }

    fn holds(&self, point: &Point) -> bool {
        self.node == point.node && self.index == point.index && self.version == point.version
    }
}

/// A selection's ends as places in a container's texts: `(order, offset)`, first to last.
type Span = ((usize, usize), (usize, usize));

impl UiTree {
    /// The selectable texts below `container`, in order: its subtree's nodes in placement
    /// order, without what a nested container or a `DisableSelection` modifier holds.
    fn selectables(&mut self, container: NodeId) -> Vec<Entry> {
        let mut entries = Vec::new();
        let mut stack = vec![container];
        while let Some(node) = stack.pop() {
            let Some(value) = self.get(node) else { continue };
            if !value.placed {
                continue;
            }
            if node != container
                && value.chain.iter().any(|entry| {
                    entry.node.downcast_ref::<SelectionContainerNode>().is_some()
                        || entry.node.downcast_ref::<DisableSelectionNode>().is_some()
                })
            {
                continue;
            }
            let mut layer = 0;
            for index in 0..self.node(node).chain.len() {
                let bits = self.node(node).chain[index].kinds;
                let version = self
                    .get_mut(node)
                    .and_then(|value| value.chain[index].selectable().map(|text| text.version()));
                if let Some(version) = version {
                    let origin = self.layer_origin(node, layer);
                    let size = self.node(node).layers[layer].measured;
                    entries.push(Entry { node, index, origin, size, version });
                }
                if bits.any(NodeKinds::LAYOUT) {
                    layer += 1;
                }
            }
            let order = self.placement_order(node);
            let count = order.as_ref().map_or(self.node(node).children.len(), |order| order.len());
            // Pushed last first, so the first placed comes off the stack first.
            for position in (0..count).rev() {
                let index = order.as_ref().map_or(position, |order| order[position] as usize);
                stack.push(self.node(node).children[index].node);
            }
        }
        entries
    }

    fn selectable_text(&mut self, node: NodeId, index: usize) -> SharedString {
        self.get_mut(node)
            .and_then(|value| value.chain[index].selectable().map(|text| text.text()))
            .unwrap_or(SharedString::EMPTY)
    }

    /// The offset nearest to `position` (in the root) in `entry`'s text.
    fn offset_in(&mut self, entry: &Entry, position: Offset) -> usize {
        let local = position - entry.origin;
        self.node_and_measurer(entry.node)
            .and_then(|(value, (mut measurer, _))| {
                value.chain[entry.index].selectable()?.offset_at(local, &mut *measurer)
            })
            .unwrap_or(0)
    }

    /// The point nearest to `position` (in the root), as Compose's selection finds it: in the
    /// text under it; else, on a text's line (a row of texts), in the last text starting
    /// before it; else at the start of the first text below it, or the end of the last.
    fn point_at(&mut self, entries: &[Entry], position: Offset) -> Option<Point> {
        let last = entries.last()?;
        let within = |entry: &Entry| {
            position.y >= entry.origin.y && position.y < entry.origin.y + entry.size.height as f32
        };
        if let Some(entry) = entries.iter().find(|entry| {
            within(entry)
                && position.x >= entry.origin.x
                && position.x < entry.origin.x + entry.size.width as f32
        }) {
            let offset = self.offset_in(entry, position);
            return Some(entry.point(offset));
        }
        let row: Vec<&Entry> = entries.iter().filter(|entry| within(entry)).collect();
        if let Some(first) = row.first() {
            return Some(match row.iter().rev().find(|entry| entry.origin.x <= position.x) {
                // Past its end (or in a gap after it): its end.
                Some(entry) => {
                    let length = self.selectable_text(entry.node, entry.index).len();
                    entry.point(length)
                }
                None => first.point(0),
            });
        }
        if let Some(entry) = entries.iter().find(|entry| position.y < entry.origin.y) {
            return Some(entry.point(0));
        }
        let length = self.selectable_text(last.node, last.index).len();
        Some(last.point(length))
    }

    /// The selection's ends in `entries`' order, or `None` if a text it ends in is gone or
    /// changed (the selection is then dropped, as Compose's).
    fn span(selection: &TextSelection, entries: &[Entry]) -> Option<Span> {
        let place = |point: &Point| {
            entries.iter().position(|entry| entry.holds(point)).map(|order| (order, point.offset))
        };
        let (anchor, focus) = (place(&selection.anchor)?, place(&selection.focus)?);
        Some(if anchor <= focus { (anchor, focus) } else { (focus, anchor) })
    }

    /// The word around `point`.
    fn word(&mut self, point: Point) -> Range<usize> {
        self.get_mut(point.node)
            .and_then(|value| {
                value.chain[point.index].selectable().map(|text| text.word_at(point.offset))
            })
            .unwrap_or(point.offset..point.offset)
    }

    fn order_of(entries: &[Entry], point: &Point) -> Option<(usize, usize)> {
        entries.iter().position(|entry| entry.holds(point)).map(|order| (order, point.offset))
    }

    /// Handles a selection gesture or command.
    pub(crate) fn handle_selection(&mut self, request: SelectionRequest) {
        match request {
            SelectionRequest::Start { container, position, unit } => {
                let entries = self.selectables(container);
                self.clear_other_selection(container);
                let Some(point) = self.point_at(&entries, position) else { return };
                let (anchor, focus) = match unit {
                    Unit::Character => (point, point),
                    Unit::Word => {
                        let word = self.word(point);
                        (Point { offset: word.start, ..point }, Point { offset: word.end, ..point })
                    }
                    Unit::Text => {
                        let length = self.selectable_text(point.node, point.index).len();
                        (Point { offset: 0, ..point }, Point { offset: length, ..point })
                    }
                };
                self.text_selection = Some(TextSelection {
                    container,
                    anchor,
                    focus,
                    initial: (anchor, focus),
                    unit,
                });
                self.show_selection(&entries);
            }
            SelectionRequest::Extend { container, position } => {
                let Some(mut selection) =
                    self.text_selection.filter(|selection| selection.container == container)
                else {
                    return;
                };
                let entries = self.selectables(container);
                let Some(point) = self.point_at(&entries, position) else { return };
                let (Some(start), Some(end), Some(at)) = (
                    Self::order_of(&entries, &selection.initial.0),
                    Self::order_of(&entries, &selection.initial.1),
                    Self::order_of(&entries, &point),
                ) else {
                    self.text_selection = None;
                    self.show_selection(&entries);
                    return;
                };
                // A word selection extends by words, a text selection by texts.
                let snap = |tree: &mut UiTree, forward: bool| match selection.unit {
                    Unit::Character => point,
                    Unit::Word => {
                        let word = tree.word(point);
                        Point { offset: if forward { word.end } else { word.start }, ..point }
                    }
                    Unit::Text => {
                        let length = tree.selectable_text(point.node, point.index).len();
                        Point { offset: if forward { length } else { 0 }, ..point }
                    }
                };
                // From the side of what the press selected away from the pointer.
                (selection.anchor, selection.focus) = if at >= start {
                    let focus = snap(self, true);
                    let focus_at = (at.0, focus.offset);
                    (selection.initial.0, if focus_at >= end { focus } else { selection.initial.1 })
                } else {
                    (selection.initial.1, snap(self, false))
                };
                self.text_selection = Some(selection);
                self.show_selection(&entries);
            }
            SelectionRequest::SelectAll { container } => {
                let entries = self.selectables(container);
                let (Some(first), Some(last)) = (entries.first(), entries.last()) else { return };
                let length = self.selectable_text(last.node, last.index).len();
                let (anchor, focus) = (first.point(0), last.point(length));
                self.text_selection = Some(TextSelection {
                    container,
                    anchor,
                    focus,
                    initial: (anchor, anchor),
                    unit: Unit::Character,
                });
                self.show_selection(&entries);
            }
            SelectionRequest::Copy { container } => {
                if self.text_selection.is_some_and(|selection| selection.container == container)
                    && let Some(text) = self.selected_text()
                {
                    self.clipboard.set_text(&text);
                }
            }
            SelectionRequest::Clear { container } => {
                if self.text_selection.is_some_and(|selection| selection.container == container) {
                    let entries = self.selectables(container);
                    self.text_selection = None;
                    self.show_selection(&entries);
                }
            }
        }
    }

    /// A selection started in another container clears this one's.
    fn clear_other_selection(&mut self, container: NodeId) {
        if let Some(old) = self.text_selection
            && old.container != container
        {
            self.text_selection = None;
            if self.get(old.container).is_some() {
                let entries = self.selectables(old.container);
                self.show_selection(&entries);
            }
        }
    }

    /// Tells each text of `entries` its part of the selection (dropping a selection whose
    /// texts are gone or changed).
    fn show_selection(&mut self, entries: &[Entry]) {
        let span = self.text_selection.and_then(|selection| Self::span(&selection, entries));
        if span.is_none() {
            self.text_selection = None;
        }
        for (order, entry) in entries.iter().enumerate() {
            let range = span.and_then(|span| range_of(span, order));
            let text = self.selectable_text(entry.node, entry.index);
            let range = range.map(|range| clamp(&text, range));
            let changed =
                self.node_and_measurer(entry.node).is_some_and(|(value, (mut measurer, _))| {
                    value.chain[entry.index]
                        .selectable()
                        .is_some_and(|text| text.set_selection(range, &mut *measurer))
                });
            if changed {
                self.invalidate_draw_of(entry.node);
            }
        }
    }

    /// The text selected in a container, if any (what Cmd+C copies there): `None` when nothing
    /// is, or a text it ends in is gone or changed.
    pub fn selected_text(&mut self) -> Option<String> {
        let selection =
            self.text_selection.filter(|selection| selection.anchor != selection.focus)?;
        let entries = self.selectables(selection.container);
        let Some(span) = Self::span(&selection, &entries) else {
            self.text_selection = None;
            self.show_selection(&entries);
            return None;
        };
        let mut parts = Vec::new();
        for (order, entry) in entries.iter().enumerate() {
            if let Some(range) = range_of(span, order) {
                let text = self.selectable_text(entry.node, entry.index);
                parts.push(text[clamp(&text, range)].to_string());
            }
        }
        Some(parts.join("\n"))
    }
}

/// The part of the text at `order` a selection spanning `span` covers, if any.
fn range_of(span: Span, order: usize) -> Option<Range<usize>> {
    let (start, end) = span;
    if start == end || order < start.0 || order > end.0 {
        return None;
    }
    let from = if order == start.0 { start.1 } else { 0 };
    let to = if order == end.0 { end.1 } else { usize::MAX };
    Some(from..to)
}

/// `range` within `text`, at character boundaries.
fn clamp(text: &str, range: Range<usize>) -> Range<usize> {
    let floor = |offset: usize| {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    };
    floor(range.start)..floor(range.end)
}

// ---- The container's node ------------------------------------------------------------------

/// A press within this long of the last, and this close to it, is a double- or triple-click.
const MULTI_CLICK_NANOS: u64 = 500_000_000;
const MULTI_CLICK_DISTANCE: f32 = 8.0;

struct SelectionContainerElement;

/// `SelectionContainer`'s node: it turns presses, drags and keys into selection requests, and
/// takes the focus a press gives it (Cmd+C reaches it).
pub(crate) struct SelectionContainerNode {
    /// A press is selecting.
    dragging: bool,
    focused: bool,
    /// When and where the last press was, and how many came in a row.
    last_press: Option<(u64, Offset, u32)>,
}

impl ModifierNodeElement for SelectionContainerElement {
    type Node = SelectionContainerNode;
    fn create(&self) -> SelectionContainerNode {
        SelectionContainerNode { dragging: false, focused: false, last_press: None }
    }
    fn update(&self, _node: &mut SelectionContainerNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "selection_container"
    }
}

impl ModifierNode for SelectionContainerNode {
    /// Reused: no press selecting, no focus, no click run (the tree drops its selection).
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.dragging = false;
        self.focused = false;
        self.last_press = None;
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for SelectionContainerNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Main {
            return;
        }
        let container = context.node;
        let Some(position) = context.pointer_position() else { return };
        match event.kind {
            PointerEventType::Press => {
                if event.consumed {
                    // A clickable inside took it: the selection goes, as Compose's tap.
                    context.requests.selection.push(SelectionRequest::Clear { container });
                    return;
                }
                let now = compose_runtime::platform::now_nanos();
                let clicks = match self.last_press {
                    Some((at, where_, count))
                        if now.saturating_sub(at) <= MULTI_CLICK_NANOS
                            && (where_ - position).x.hypot((where_ - position).y)
                                <= MULTI_CLICK_DISTANCE =>
                    {
                        count + 1
                    }
                    _ => 1,
                };
                self.last_press = Some((now, position, clicks));
                let unit = match clicks {
                    1 => Unit::Character,
                    2 => Unit::Word,
                    _ => Unit::Text,
                };
                self.dragging = true;
                context.request_focus();
                context.requests.selection.push(SelectionRequest::Start {
                    container,
                    position,
                    unit,
                });
            }
            PointerEventType::Move if self.dragging && event.pressed && !event.consumed => {
                event.consume();
                context.requests.selection.push(SelectionRequest::Extend { container, position });
            }
            PointerEventType::Release => self.dragging = false,
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.dragging = false;
    }
}

impl FocusTargetModifierNode for SelectionContainerNode {
    fn on_focus_changed(
        &mut self,
        _previous: crate::focus::FocusState,
        current: crate::focus::FocusState,
        context: &mut EventContext<'_>,
    ) {
        let focused = current.is_focused();
        if self.focused && !focused {
            context.requests.selection.push(SelectionRequest::Clear { container: context.node });
        }
        self.focused = focused;
    }
}

impl KeyInputModifierNode for SelectionContainerNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        let shortcut =
            if cfg!(target_os = "macos") { event.modifiers.meta } else { event.modifiers.ctrl };
        if event.kind != KeyEventType::KeyDown || !shortcut {
            return false;
        }
        let container = context.node;
        let request = match event.key {
            Key::Character(c) if c.eq_ignore_ascii_case(&'c') => {
                SelectionRequest::Copy { container }
            }
            Key::Character(c) if c.eq_ignore_ascii_case(&'a') => {
                SelectionRequest::SelectAll { container }
            }
            _ => return false,
        };
        context.requests.selection.push(request);
        true
    }
}

/// `DisableSelection`'s node: a mark the container's search stops at.
pub(crate) struct DisableSelectionNode;

struct DisableSelectionElement;

impl ModifierNodeElement for DisableSelectionElement {
    type Node = DisableSelectionNode;
    fn create(&self) -> DisableSelectionNode {
        DisableSelectionNode
    }
    fn update(&self, _node: &mut DisableSelectionNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "disable_selection"
    }
}

impl ModifierNode for DisableSelectionNode {}

impl Modifier {
    /// Makes this node's texts selectable, as `SelectionContainer` does for its content.
    pub fn selection_container(self) -> Modifier {
        self.then_custom(SelectionContainerElement)
    }

    /// Leaves this node's texts out of a selection container, as `DisableSelection`.
    pub fn disable_selection(self) -> Modifier {
        self.then_custom(DisableSelectionElement)
    }
}

impl TextSelection {
    /// The container it's in.
    pub(crate) fn container(&self) -> NodeId {
        self.container
    }
}
