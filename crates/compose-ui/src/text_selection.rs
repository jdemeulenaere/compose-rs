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
use crate::text::{SharedString, TextMeasurer};
use crate::tree::{NodeId, UiTree, capability};
use crate::unit::{IntSize, Offset};

/// A text a `SelectionContainer` can select in, as Compose's `Selectable`: a modifier node
/// returning itself from `ModifierNode::as_selectable`. Offsets are byte offsets at character
/// boundaries; positions are in the node's coordinates.
pub trait Selectable {
    /// Its text: what's selected and copied.
    fn text(&self) -> SharedString;
    /// The offset nearest to `position`, from the measurer (`None` before it's laid out).
    fn offset_at(&self, position: Offset, measurer: &mut dyn TextMeasurer) -> Option<usize>;
    /// The word (or run between words) at `offset`: what a double-click selects.
    fn word_at(&self, offset: usize) -> Range<usize>;
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

/// A point in a container's texts: the text (its node and chain index) and an offset in it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Point {
    /// The text's place in the container's order.
    order: usize,
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
}

/// A text below a container: its node, the chain index of its selectable, and its bounds in
/// the root.
#[derive(Clone, Copy, Debug)]
struct Entry {
    node: NodeId,
    index: usize,
    origin: Offset,
    size: IntSize,
}

impl UiTree {
    /// The selectable texts below `container`, in order: its subtree's nodes in placement
    /// order, without what a `DisableSelection` or a nested container holds.
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
                    entry.downcast_ref::<SelectionContainerNode>().is_some()
                        || entry.downcast_ref::<DisableSelectionNode>().is_some()
                })
            {
                continue;
            }
            let mut layer = 0;
            for index in 0..self.node(node).chain.len() {
                let bits = self.node(node).capabilities[index];
                let selectable = self
                    .get_mut(node)
                    .is_some_and(|value| value.chain[index].as_selectable().is_some());
                if selectable {
                    let origin = self.layer_origin(node, layer);
                    let size = self.node(node).layers[layer].measured;
                    entries.push(Entry { node, index, origin, size });
                }
                if bits & capability::LAYOUT != 0 {
                    layer += 1;
                }
            }
            let count = self.node(node).children.len();
            let order = self.placement_order(node);
            // Pushed last first, so the first placed comes off the stack first.
            for position in (0..count).rev() {
                let index = order.as_ref().map_or(position, |order| order[position] as usize);
                stack.push(self.node(node).children[index].node);
            }
        }
        entries
    }

    fn selectable_text(&mut self, entry: &Entry) -> SharedString {
        self.get_mut(entry.node)
            .and_then(|value| value.chain[entry.index].as_selectable().map(|text| text.text()))
            .unwrap_or(SharedString::EMPTY)
    }

    /// The point nearest to `position` (in the root): in the text whose line it's on, or
    /// before the first text it's above, or at the end of the last.
    fn point_at(&mut self, entries: &[Entry], position: Offset) -> Option<Point> {
        let last = entries.len().checked_sub(1)?;
        for (order, entry) in entries.iter().enumerate() {
            let top = entry.origin.y;
            let bottom = top + entry.size.height as f32;
            if position.y < top {
                return Some(Point { order, offset: 0 });
            }
            if position.y < bottom {
                let local = position - entry.origin;
                let (value, (measurer, _)) = self.node_and_measurer(entry.node)?;
                let offset = value.chain[entry.index]
                    .as_selectable()?
                    .offset_at(local, measurer)
                    .unwrap_or(0);
                return Some(Point { order, offset });
            }
        }
        let length = self.selectable_text(&entries[last]).len();
        Some(Point { order: last, offset: length })
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
                        let word = self
                            .get_mut(entries[point.order].node)
                            .and_then(|value| {
                                value.chain[entries[point.order].index]
                                    .as_selectable()
                                    .map(|text| text.word_at(point.offset))
                            })
                            .unwrap_or(point.offset..point.offset);
                        (
                            Point { order: point.order, offset: word.start },
                            Point { order: point.order, offset: word.end },
                        )
                    }
                    Unit::Text => {
                        let length = self.selectable_text(&entries[point.order]).len();
                        (
                            Point { order: point.order, offset: 0 },
                            Point { order: point.order, offset: length },
                        )
                    }
                };
                self.text_selection =
                    Some(TextSelection { container, anchor, focus, initial: (anchor, focus) });
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
                // From the side of what the press selected away from the pointer.
                let (start, end) = selection.initial;
                (selection.anchor, selection.focus) =
                    if point >= start { (start, point.max(end)) } else { (end, point) };
                self.text_selection = Some(selection);
                self.show_selection(&entries);
            }
            SelectionRequest::SelectAll { container } => {
                let entries = self.selectables(container);
                let Some(last) = entries.len().checked_sub(1) else { return };
                let length = self.selectable_text(&entries[last]).len();
                let (anchor, focus) =
                    (Point { order: 0, offset: 0 }, Point { order: last, offset: length });
                self.text_selection =
                    Some(TextSelection { container, anchor, focus, initial: (anchor, anchor) });
                self.show_selection(&entries);
            }
            SelectionRequest::Copy { container } => {
                let Some(selection) =
                    self.text_selection.filter(|selection| selection.container == container)
                else {
                    return;
                };
                let entries = self.selectables(container);
                let mut parts = Vec::new();
                for (order, entry) in entries.iter().enumerate() {
                    if let Some(range) = range_of(&selection, order) {
                        let text = self.selectable_text(entry);
                        let range = clamp(&text, range);
                        parts.push(text[range].to_string());
                    }
                }
                if !parts.is_empty() {
                    self.clipboard.set_text(&parts.join("\n"));
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
            && self.get(old.container).is_some()
        {
            let entries = self.selectables(old.container);
            self.text_selection = None;
            self.show_selection(&entries);
        }
    }

    /// Tells each text of `entries` its part of the selection.
    fn show_selection(&mut self, entries: &[Entry]) {
        let selection = self.text_selection;
        let mut changed = false;
        for (order, entry) in entries.iter().enumerate() {
            let range = selection.and_then(|selection| range_of(&selection, order));
            let text = self.selectable_text(entry);
            let range = range.map(|range| clamp(&text, range));
            if let Some((value, (measurer, _))) = self.node_and_measurer(entry.node)
                && let Some(selectable) = value.chain[entry.index].as_selectable()
            {
                changed |= selectable.set_selection(range, measurer);
            }
        }
        if changed {
            self.needs_draw = true;
        }
    }

    /// The text selected in a container, if any (what Cmd+C copies there).
    pub fn selected_text(&mut self) -> Option<String> {
        let selection =
            self.text_selection.filter(|selection| selection.anchor != selection.focus)?;
        let entries = self.selectables(selection.container);
        let mut parts = Vec::new();
        for (order, entry) in entries.iter().enumerate() {
            if let Some(range) = range_of(&selection, order) {
                let text = self.selectable_text(entry);
                parts.push(text[clamp(&text, range)].to_string());
            }
        }
        Some(parts.join("\n"))
    }
}

impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Point) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Point {
    fn cmp(&self, other: &Point) -> std::cmp::Ordering {
        (self.order, self.offset).cmp(&(other.order, other.offset))
    }
}

/// The part of the text at `order` the selection covers, if any.
fn range_of(selection: &TextSelection, order: usize) -> Option<Range<usize>> {
    let (start, end) = if selection.anchor <= selection.focus {
        (selection.anchor, selection.focus)
    } else {
        (selection.focus, selection.anchor)
    };
    if start == end || order < start.order || order > end.order {
        return None;
    }
    let from = if order == start.order { start.offset } else { 0 };
    let to = if order == end.order { end.offset } else { usize::MAX };
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
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(SelectionContainerNode { dragging: false, focused: false, last_press: None })
    }
    fn update(&self, _node: &mut dyn ModifierNode) {}
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<SelectionContainerElement>().is_some()
    }
    fn name(&self) -> &'static str {
        "selection_container"
    }
}

impl ModifierNode for SelectionContainerNode {
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
                let now = compose_runtime::executor::now_nanos();
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
            PointerEventType::Move if self.dragging && event.pressed => {
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
    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
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
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(DisableSelectionNode)
    }
    fn update(&self, _node: &mut dyn ModifierNode) {}
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<DisableSelectionElement>().is_some()
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
