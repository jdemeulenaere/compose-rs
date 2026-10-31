//! Alignment lines, as Compose's `AlignmentLine` and `LayoutNodeAlignmentLines`: positions a layout
//! gives with its size (`MeasureScope::provide_alignment_lines`), asked of a child while measuring
//! or placing (`MeasureScope::alignment_line`), inherited through what's placed. Reading one may
//! run blocks waiting below it. A change tells only who read. D1219 says what a tree where nothing's
//! read pays.

use std::sync::{Mutex, PoisonError};

use smallvec::SmallVec;

use crate::layout::Measurable;
use crate::tree::{LayoutNode, NodeId, UiTree};
use crate::unit::Offset;

/// A position a layout gives its ancestors, along one axis, as Compose's `AlignmentLine`: what
/// [`HorizontalAlignmentLine`] and [`VerticalAlignmentLine`] make. What merges two children's is
/// a function, kept apart, so a line is a number: `Copy`, small in the parent data every node has,
/// and [`FIRST_BASELINE`] a constant.
#[derive(Clone, Copy)]
pub struct AlignmentLine(u32);

/// The bit of a line saying it's horizontal.
const HORIZONTAL: u32 = 1 << 31;

/// The mergers of the lines made, after the two baselines'. A line's made once, as Compose's are,
/// so a program making them over and over keeps them: the only growth, a word each.
static MERGERS: Mutex<Vec<Merger>> = Mutex::new(Vec::new());

type Merger = fn(i32, i32) -> i32;

impl AlignmentLine {
    /// What's read of a line nothing gives, as `AlignmentLine.Unspecified`.
    pub const UNSPECIFIED: i32 = i32::MIN;

    fn make(horizontal: bool, merger: fn(i32, i32) -> i32) -> AlignmentLine {
        let mut mergers = MERGERS.lock().unwrap_or_else(PoisonError::into_inner);
        mergers.push(merger);
        let index = mergers.len() as u32 + 1;
        AlignmentLine(if horizontal { index | HORIZONTAL } else { index })
    }

    /// Whether it's a position along the height.
    pub fn is_horizontal(&self) -> bool {
        self.0 & HORIZONTAL != 0
    }

    fn merge(&self, first: i32, second: i32) -> i32 {
        match self.0 & !HORIZONTAL {
            0 => first.min(second),
            1 => first.max(second),
            index => {
                let mergers = MERGERS.lock().unwrap_or_else(PoisonError::into_inner);
                let merger = mergers[index as usize - 2];
                // Not holding it: it's application code.
                drop(mergers);
                merger(first, second)
            }
        }
    }

    fn of(&self, point: crate::unit::IntOffset) -> i32 {
        if self.is_horizontal() { point.y } else { point.x }
    }
}

impl PartialEq for AlignmentLine {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for AlignmentLine {}

impl std::fmt::Debug for AlignmentLine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let axis = if self.is_horizontal() { "Horizontal" } else { "Vertical" };
        write!(formatter, "{axis}AlignmentLine({})", self.0 & !HORIZONTAL)
    }
}

/// A line across a layout, its position along the height, as `HorizontalAlignmentLine`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HorizontalAlignmentLine(AlignmentLine);

impl HorizontalAlignmentLine {
    /// A new line, `merger` giving what a layout inherits from two children's.
    pub fn new(merger: fn(i32, i32) -> i32) -> HorizontalAlignmentLine {
        HorizontalAlignmentLine(AlignmentLine::make(true, merger))
    }
}

/// A line down a layout, its position along the width, as `VerticalAlignmentLine`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VerticalAlignmentLine(AlignmentLine);

impl VerticalAlignmentLine {
    /// A new line, `merger` giving what a layout inherits from two children's.
    pub fn new(merger: fn(i32, i32) -> i32) -> VerticalAlignmentLine {
        VerticalAlignmentLine(AlignmentLine::make(false, merger))
    }
}

impl From<HorizontalAlignmentLine> for AlignmentLine {
    fn from(line: HorizontalAlignmentLine) -> AlignmentLine {
        line.0
    }
}

impl From<VerticalAlignmentLine> for AlignmentLine {
    fn from(line: VerticalAlignmentLine) -> AlignmentLine {
        line.0
    }
}

/// The baseline of a layout's first line of text, as `FirstBaseline`.
pub const FIRST_BASELINE: HorizontalAlignmentLine =
    HorizontalAlignmentLine(AlignmentLine(HORIZONTAL));

/// The baseline of a layout's last line of text, as `LastBaseline`.
pub const LAST_BASELINE: HorizontalAlignmentLine =
    HorizontalAlignmentLine(AlignmentLine(1 | HORIZONTAL));

type Lines = SmallVec<[(AlignmentLine, i32); 2]>;

/// What a node gave and inherited of alignment lines, and who read them, as Compose's
/// `AlignmentLines`. A node has one once it gives one or it's asked.
pub(crate) struct AlignmentLines {
    /// Per layer, what its last measurement gave, as its `MeasureResult.alignmentLines`.
    provided: SmallVec<[(u32, AlignmentLine, i32); 2]>,
    /// What its measurement in progress gave so far.
    giving: SmallVec<[(u32, AlignmentLine, i32); 2]>,
    /// What its content gives: inherited, and its policy's.
    map: Lines,
    /// `map` may be stale.
    dirty: bool,
    used_during_parent_measurement: bool,
    used_during_parent_layout: bool,
    previous_used_during_parent_layout: bool,
    used_by_modifier_measurement: bool,
    used_by_modifier_layout: bool,
    /// The closest at or above it asked, by its parent or its modifiers.
    query_owner: Option<NodeId>,
    /// See `lines_changed_for_coordinates`.
    told_coordinates: u32,
}

impl Default for AlignmentLines {
    fn default() -> AlignmentLines {
        AlignmentLines {
            provided: SmallVec::new(),
            giving: SmallVec::new(),
            map: SmallVec::new(),
            dirty: true,
            used_during_parent_measurement: false,
            used_during_parent_layout: false,
            previous_used_during_parent_layout: false,
            used_by_modifier_measurement: false,
            used_by_modifier_layout: false,
            query_owner: None,
            told_coordinates: 0,
        }
    }
}

/// A node's alignment lines, if it has any.
fn lines_of(value: &LayoutNode) -> Option<&AlignmentLines> {
    value.placements.as_ref()?.alignment.as_deref()
}

fn lines_of_mut(value: &mut LayoutNode) -> Option<&mut AlignmentLines> {
    value.placements.as_mut()?.alignment.as_deref_mut()
}

/// The baselines of the layout node of `layer`, if it's a text's (`ModifierNode::baselines`).
fn baselines_at(value: &LayoutNode, layer: usize) -> Option<(i32, i32)> {
    let index = *value.layout_indices.get(layer)? as usize;
    value.chain.get(index)?.node.baselines()
}

/// What a layer gives of `line`: a text's baselines, asked of it, or what it gave.
fn provided_at(value: &LayoutNode, layer: usize, line: AlignmentLine) -> Option<i32> {
    if (line == FIRST_BASELINE.0 || line == LAST_BASELINE.0)
        && let Some((first, last)) = baselines_at(value, layer)
    {
        return Some(if line == FIRST_BASELINE.0 { first } else { last });
    }
    lines_of(value).and_then(|lines| lines.provided_at(layer, line))
}

/// All a node's layers give, a text's baselines with the others.
fn all_provided(value: &LayoutNode) -> SmallVec<[(u32, AlignmentLine, i32); 4]> {
    let mut provided = SmallVec::new();
    for layer in 0..value.layout_indices.len() {
        if let Some((first, last)) = baselines_at(value, layer) {
            provided.push((layer as u32, FIRST_BASELINE.0, first));
            provided.push((layer as u32, LAST_BASELINE.0, last));
        }
    }
    if let Some(lines) = lines_of(value) {
        provided.extend(lines.provided.iter().copied());
    }
    provided
}

impl AlignmentLines {
    fn queried(&self) -> bool {
        self.used_during_parent_measurement
            || self.previous_used_during_parent_layout
            || self.used_by_modifier_measurement
            || self.used_by_modifier_layout
    }

    fn provided_at(&self, layer: usize, line: AlignmentLine) -> Option<i32> {
        (self.provided.iter())
            .find(|(at, it, _)| *at as usize == layer && *it == line)
            .map(|(_, _, position)| *position)
    }
}

/// Blocks running to know a line: over when it's dropped, a panic in them included.
pub(crate) struct AlignmentPass<'a> {
    pub(crate) tree: &'a mut UiTree,
    aligning: bool,
    asked: bool,
}

impl Drop for AlignmentPass<'_> {
    fn drop(&mut self) {
        self.tree.placing_for_alignment = self.aligning;
        self.tree.asked_while_aligning.set(self.asked);
    }
}

impl UiTree {
    pub(crate) fn alignment_pass(&mut self) -> AlignmentPass<'_> {
        let aligning = std::mem::replace(&mut self.placing_for_alignment, true);
        let asked = self.asked_while_aligning.replace(false);
        AlignmentPass { tree: self, aligning, asked }
    }

    /// A text's baselines changed, or came: `ModifierNode::baselines` says what they are.
    pub(crate) fn baselines_changed(&mut self, node: NodeId) {
        self.on_alignments_changed(node);
    }

    fn alignment_mut(&mut self, node: NodeId) -> &mut AlignmentLines {
        let placements = self.node_mut(node).placements.get_or_insert_with(Default::default);
        placements.alignment.get_or_insert_with(Default::default)
    }

    /// What layer `layer` of `node`'s measurement gives so far.
    pub(crate) fn give_alignment_lines(
        &mut self,
        node: NodeId,
        layer: usize,
        lines: impl IntoIterator<Item = (AlignmentLine, i32)>,
    ) {
        let giving = &mut self.alignment_mut(node).giving;
        giving.retain(|(at, _, _)| *at as usize != layer);
        giving.extend(lines.into_iter().map(|(line, position)| (layer as u32, line, position)));
    }

    /// Layer `layer` of `node` was measured: what it gave's kept, and a change told, as
    /// `measureResult`'s setter.
    pub(crate) fn alignment_lines_measured(&mut self, node: NodeId, layer: usize) {
        let Some(lines) = lines_of_mut(self.node_mut(node)) else { return };
        let at = |entry: &&(u32, AlignmentLine, i32)| entry.0 as usize == layer;
        let same = lines.provided.iter().filter(at).count()
            == lines.giving.iter().filter(at).count()
            && (lines.provided.iter().filter(at)).all(|entry| lines.giving.contains(entry));
        if same {
            lines.giving.retain(|entry| entry.0 as usize != layer);
            return;
        }
        lines.provided.retain(|entry| entry.0 as usize != layer);
        let mut index = 0;
        while index < lines.giving.len() {
            if lines.giving[index].0 as usize == layer {
                let entry = lines.giving.remove(index);
                lines.provided.push(entry);
            } else {
                index += 1;
            }
        }
        self.on_alignments_changed(node);
    }

    /// As `onAlignmentsChanged`: what `node` and those above it inherit may be stale, and who read
    /// them measures or places again.
    pub(crate) fn on_alignments_changed(&mut self, node: NodeId) {
        let mut current = Some(node);
        while let Some(id) = current {
            let Some(value) = self.get_mut(id) else { return };
            let parent = value.parent;
            if let Some(lines) = lines_of_mut(value) {
                lines.dirty = true;
                let (in_measure, in_layout) = (
                    lines.used_during_parent_measurement,
                    lines.previous_used_during_parent_layout || lines.used_during_parent_layout,
                );
                let (modifier_measure, modifier_layout) =
                    (lines.used_by_modifier_measurement, lines.used_by_modifier_layout);
                if modifier_layout {
                    lines.told_coordinates = lines.told_coordinates.wrapping_add(1);
                }
                if let Some(parent) = parent {
                    if in_measure {
                        self.request_lines_measure(parent);
                    } else if in_layout {
                        self.request_lines_placement(parent);
                    }
                } else {
                    return;
                }
                if modifier_measure {
                    self.request_lines_measure(id);
                }
                if modifier_layout {
                    self.request_lines_placement(id);
                }
            }
            current = parent;
        }
    }

    fn request_lines_measure(&mut self, node: NodeId) {
        // Measuring, it sees what changed, as `requestRemeasure`.
        if self.node(node).measuring {
            return;
        }
        self.invalidate_layout_of(node);
        self.line_requests.push(node);
    }

    fn request_lines_placement(&mut self, node: NodeId) {
        // As `requestRelayout`.
        let value = self.node(node);
        if value.measuring || value.laying_out {
            return;
        }
        self.invalidate_placement_of(node);
        self.line_requests.push(node);
    }

    /// A layout that read one whose node moved may now be stale in the same layout (what's above
    /// a placement pass was walked past): it's done until nothing asks, as `measureAndLayout`.
    pub(crate) fn settle_line_requests(
        &mut self,
        root: NodeId,
        constraints: crate::constraints::Constraints,
    ) {
        // A layout whose every run moves what it reads never settles: a frame ends.
        for _ in 0..8 {
            let requests = std::mem::take(&mut self.line_requests);
            if requests.is_empty() {
                return;
            }
            for node in requests {
                let Some(value) = self.get(node) else { continue };
                let (measure, placement, placed) =
                    (value.needs_measure, value.needs_placement, value.placed);
                let parent = value.parent;
                if measure {
                    let mut above = parent;
                    while let Some(id) = above {
                        let value = self.node_mut(id);
                        value.subtree_needs_measure = true;
                        above = value.parent;
                    }
                }
                if placement && placed {
                    self.note_placement_below(parent);
                }
            }
            self.measure_node(root, constraints);
            self.place(root, 0, crate::unit::IntOffset::ZERO);
            let value = self.node(root);
            if value.needs_placement || value.subtree_needs_placement {
                self.place_pass(root);
            }
        }
    }

    /// `layer` of `node` moved in what's around it, as `invalidateAlignmentLinesFromPositionChange`.
    pub(crate) fn alignment_position_changed(&mut self, node: NodeId, layer: usize) {
        if layer > 0 {
            self.on_alignments_changed(node);
            return;
        }
        let value = self.node(node);
        if value.layers.len() == 1 {
            self.on_alignments_changed(node);
        } else if let Some(parent) = value.parent {
            self.on_alignments_changed(parent);
        }
    }

    /// A measurement of `node` starts: what it read before, of its children and its content, it
    /// says again if it still reads it.
    pub(crate) fn alignment_measure_starts(&mut self, node: NodeId) {
        self.node_mut(node).measuring = true;
        if let Some(lines) = lines_of_mut(self.node_mut(node)) {
            lines.used_by_modifier_measurement = false;
        }
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            if let Some(lines) = lines_of_mut(self.node_mut(child)) {
                lines.used_during_parent_measurement = false;
            }
        }
    }

    /// `node`'s blocks start: what they read they say again, as `layoutChildrenBlock`.
    pub(crate) fn clear_alignment_uses_in_placement(&mut self, node: NodeId) {
        if let Some(lines) = lines_of_mut(self.node_mut(node)) {
            lines.used_by_modifier_layout = false;
        }
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            if let Some(lines) = lines_of_mut(self.node_mut(child)) {
                lines.used_during_parent_layout = false;
            }
        }
    }

    /// `node`'s blocks ran: as the end of `layoutChildrenBlock` and of `layoutChildren`.
    pub(crate) fn alignment_uses_placed(&mut self, node: NodeId) {
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            if let Some(lines) = lines_of_mut(self.node_mut(child)) {
                lines.previous_used_during_parent_layout = lines.used_during_parent_layout;
            }
        }
        self.finish_layout_children(node);
    }

    fn finish_layout_children(&mut self, node: NodeId) {
        let Some(lines) = lines_of_mut(self.node_mut(node)) else {
            // Asked through what's above it, it needs its own.
            if self.required(node) {
                self.recalculate_lines(node);
            }
            return;
        };
        if lines.used_during_parent_layout {
            lines.previous_used_during_parent_layout = true;
        }
        if lines.dirty && self.required(node) {
            self.recalculate_lines(node);
        }
    }

    /// Whether what's above it relies on `node`'s lines, as `required`.
    fn required(&mut self, node: NodeId) -> bool {
        self.recalculate_query_owner(node);
        lines_of(self.node(node)).is_some_and(|lines| lines.query_owner.is_some())
    }

    fn query_owner_of(&self, node: NodeId) -> Option<NodeId> {
        lines_of(self.node(node)).and_then(|lines| lines.query_owner)
    }

    fn is_queried(&self, node: NodeId) -> bool {
        self.get(node).and_then(lines_of).is_some_and(|it| it.queried())
    }

    /// As `recalculateQueryOwner`.
    fn recalculate_query_owner(&mut self, node: NodeId) {
        let owner = if self.is_queried(node) {
            Some(node)
        } else {
            let Some(parent) = self.node(node).parent else { return };
            match self.query_owner_of(parent).filter(|owner| self.is_queried(*owner)) {
                Some(owner) => Some(owner),
                None => {
                    let Some(owner) = self.query_owner_of(node) else { return };
                    if self.is_queried(owner) {
                        return;
                    }
                    match self.get(owner).and_then(|value| value.parent) {
                        Some(above) => {
                            self.recalculate_query_owner(above);
                            self.query_owner_of(above)
                        }
                        None => None,
                    }
                }
            }
        };
        if owner.is_some() || lines_of(self.node(node)).is_some() {
            self.alignment_mut(node).query_owner = owner;
        }
    }

    /// `measurable[line]`, as `MeasurePassDelegate.get` and `NodeCoordinator.get`.
    pub(crate) fn read_alignment_line(
        &mut self,
        reader: NodeId,
        measurable: Measurable,
        line: AlignmentLine,
        placing: bool,
    ) -> i32 {
        // The first in the tree: who's reading didn't say it's measuring.
        if !self.alignment_lines_read {
            self.alignment_lines_read = true;
            if !placing {
                self.node_mut(reader).measuring = true;
            }
        }
        let (node, layer) = (measurable.node, measurable.layer as usize);
        let from_parent = layer == 0;
        if from_parent {
            let lines = self.alignment_mut(node);
            if placing {
                lines.used_during_parent_layout = true;
            } else {
                lines.used_during_parent_measurement = true;
            }
        }
        self.alignment_line_at(node, layer, line, from_parent, placing)
    }

    /// The line at `layer` of `node` in what's around the layer, as `get`.
    fn alignment_line_at(
        &mut self,
        node: NodeId,
        layer: usize,
        line: AlignmentLine,
        from_parent: bool,
        placing: bool,
    ) -> i32 {
        let last = self.node(node).layers.len() - 1;
        let measured = if layer == last {
            self.calculate_alignment_lines(node, from_parent, placing);
            (lines_of(self.node(node)).into_iter().flat_map(|lines| lines.map.iter()))
                .find(|(it, _)| *it == line)
                .map_or(AlignmentLine::UNSPECIFIED, |(_, position)| *position)
        } else {
            match provided_at(self.node(node), layer, line) {
                Some(position) => position,
                None => {
                    let inner = self.alignment_line_at(node, layer + 1, line, from_parent, placing);
                    if inner == AlignmentLine::UNSPECIFIED {
                        return inner;
                    }
                    // To know where its content is, as `replace`.
                    self.run_block_for_alignment(node, layer);
                    inner + line.of(self.node(node).layers[layer + 1].offset)
                }
            }
        };
        if measured == AlignmentLine::UNSPECIFIED {
            return measured;
        }
        measured + line.of(crate::measure::centering(&self.node(node).layers[layer]))
    }

    /// As `calculateAlignmentLines`.
    fn calculate_alignment_lines(&mut self, node: NodeId, from_parent: bool, placing: bool) {
        if from_parent {
            self.layout_children(node);
        } else {
            let lines = self.alignment_mut(node);
            if placing {
                lines.used_by_modifier_layout = true;
            } else {
                lines.used_by_modifier_measurement = true;
            }
            let last = self.node(node).layers.len() - 1;
            self.run_block_for_alignment(node, last);
            self.finish_layout_children(node);
        }
    }

    /// As `layoutChildren` for its lines: the node's blocks, if they wait; what it inherits, if
    /// it's stale and asked.
    fn layout_children(&mut self, node: NodeId) {
        self.recalculate_query_owner(node);
        if self.node(node).needs_placement {
            let pass = self.alignment_pass();
            pass.tree.run_placement_blocks(node);
            pass.tree.node_mut(node).needs_placement = false;
            // Where it is it wasn't told: it runs in the placement pass too.
            if pass.tree.asked_while_aligning.get() {
                pass.tree.mark_needs_placement(node);
            }
            drop(pass);
            self.alignment_uses_placed(node);
        } else {
            self.finish_layout_children(node);
        }
    }

    /// As `recalculate`: what `node`'s placed children give, in its content.
    fn recalculate_lines(&mut self, node: NodeId) {
        // A line goes through what's around it, as placing them would have run them.
        if !self.new_layers.is_empty() {
            self.run_new_layers();
        }
        let mut map = Lines::new();
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            let value = self.node(child);
            if !value.placed || value.deactivated {
                continue;
            }
            if lines_of(value).is_none_or(|lines| lines.dirty) {
                self.layout_children(child);
            }
            let value = self.node(child);
            let last = value.layers.len() - 1;
            let inherited = lines_of(value).map(|lines| lines.map.clone());
            let of_modifiers = all_provided(value);
            for (line, position) in inherited.into_iter().flatten() {
                self.add_alignment_line(&mut map, line, position, child, last);
            }
            for layer in (0..last).rev() {
                for (_, line, _) in of_modifiers.iter().filter(|(at, _, _)| *at as usize == layer) {
                    let value = self.node(child);
                    let position = provided_at(value, layer, *line).expect("just found")
                        + line.of(crate::measure::centering(&value.layers[layer]));
                    self.add_alignment_line(&mut map, *line, position, child, layer);
                }
            }
        }
        let value = self.node_mut(node);
        let last = value.layers.len() as u32 - 1;
        let provided = all_provided(value);
        let placements = value.placements.get_or_insert_with(Default::default);
        let lines = placements.alignment.get_or_insert_with(Default::default);
        for (at, line, position) in provided {
            if at == last {
                match map.iter_mut().find(|(it, _)| *it == line) {
                    Some(entry) => entry.1 = position,
                    None => map.push((line, position)),
                }
            }
        }
        lines.map = map;
        lines.dirty = false;
    }

    /// As `addAlignmentLine`: `position` in `layer` of `child`, out to its parent's content, one
    /// nearer the outside giving it instead; merged in `map`.
    fn add_alignment_line(
        &self,
        map: &mut Lines,
        line: AlignmentLine,
        position: i32,
        child: NodeId,
        layer: usize,
    ) {
        let value = self.node(child);
        let outer = (0..layer).find_map(|at| {
            let position = provided_at(value, at, line)?;
            Some((at, position + line.of(crate::measure::centering(&value.layers[at]))))
        });
        let (layer, position) = outer.unwrap_or((layer, position));
        let point = self.map_line_out_of_node(child, layer, position as f32);
        let position = if line.is_horizontal() { point.y } else { point.x }.round() as i32;
        match map.iter_mut().find(|(it, _)| *it == line) {
            Some(entry) => entry.1 = line.merge(entry.1, position),
            None => map.push((line, position)),
        }
    }

    /// `(position, position)` in `layer` of `node`, through what's around the layer in the node and
    /// its place, as `toParentPosition` up the coordinators.
    fn map_line_out_of_node(&self, node: NodeId, layer: usize, position: f32) -> Offset {
        use crate::graphics_layer::GraphicsLayerNode;
        use crate::layout_callbacks::layer_origin;
        let value = self.node(node);
        let mut point = Offset::new(position, position) + layer_origin(value, layer);
        // What wraps the layer: all before its own layout node, those of its own segment too.
        for index in (0..value.entries_of_layer(layer).end).rev() {
            let Some(graphics) = value.chain[index].node.downcast_ref::<GraphicsLayerNode>() else {
                continue;
            };
            let at = value.layer_of(index);
            let map = graphics.layer_map(layer_origin(value, at), value.layers[at].measured.into());
            point = map.map(point);
        }
        let position = value.layers[0].offset;
        point + Offset::new(position.x as f32, position.y as f32)
    }

    /// `line` at `layer` of `node` now, the tree as it was last laid out, running nothing: what a
    /// `LayoutCoordinates` is told.
    pub(crate) fn peek_alignment_line(
        &self,
        node: NodeId,
        layer: usize,
        line: AlignmentLine,
    ) -> i32 {
        let value = self.node(node);
        let measured = if let Some(position) = provided_at(value, layer, line) {
            position
        } else if layer + 1 < value.layers.len() {
            match self.peek_alignment_line(node, layer + 1, line) {
                AlignmentLine::UNSPECIFIED => return AlignmentLine::UNSPECIFIED,
                inner => inner + line.of(value.layers[layer + 1].offset),
            }
        } else {
            self.peek_inherited_line(node, line)
        };
        if measured == AlignmentLine::UNSPECIFIED {
            return measured;
        }
        measured + line.of(crate::measure::centering(&value.layers[layer]))
    }

    /// What `node`'s placed children give of `line` in its content, as `recalculate`, working out
    /// and keeping nothing.
    fn peek_inherited_line(&self, node: NodeId, line: AlignmentLine) -> i32 {
        let mut merged = AlignmentLine::UNSPECIFIED;
        for child in &self.node(node).children {
            let value = self.node(child.node);
            if !value.placed || value.deactivated {
                continue;
            }
            let last = value.layers.len() - 1;
            // The outermost that gives it, or what's in it, from its last layer.
            let given = (0..=last).find_map(|at| {
                let position = provided_at(value, at, line)?;
                let centered = if at < last {
                    line.of(crate::measure::centering(&value.layers[at]))
                } else {
                    0
                };
                Some((at, position + centered))
            });
            let (layer, position) = match given {
                Some(given) => given,
                None => match self.peek_inherited_line(child.node, line) {
                    AlignmentLine::UNSPECIFIED => continue,
                    inner => (last, inner),
                },
            };
            let point = self.map_line_out_of_node(child.node, layer, position as f32);
            let position = if line.is_horizontal() { point.y } else { point.x }.round() as i32;
            merged = if merged == AlignmentLine::UNSPECIFIED {
                position
            } else {
                line.merge(merged, position)
            };
        }
        merged
    }

    /// What `node` gives from `layer` in, as `providedAlignmentLines`.
    pub(crate) fn provided_alignment_lines(
        &self,
        node: NodeId,
        layer: usize,
    ) -> Vec<AlignmentLine> {
        let mut lines = Vec::new();
        for (at, line, _) in all_provided(self.node(node)) {
            if at as usize >= layer && !lines.contains(&line) {
                lines.push(line);
            }
        }
        lines
    }

    /// How many changes to `node`'s lines those asking through coordinates heard of: another's a
    /// reason to tell them, its position or size the same.
    pub(crate) fn lines_changed_for_coordinates(&self, node: NodeId) -> u32 {
        lines_of(self.node(node)).map_or(0, |lines| lines.told_coordinates)
    }

    /// A `LayoutCoordinates` asked `node`'s lines: as a modifier placing, it hears of a change.
    pub(crate) fn note_alignment_asked_by_coordinates(&mut self) {
        let asked = std::mem::take(&mut *self.focus_requests.alignment_asked.borrow_mut());
        for node in asked {
            if self.get(node).is_some() {
                self.alignment_mut(node).used_by_modifier_layout = true;
                self.alignment_lines_read = true;
            }
        }
    }

    /// As `resetAlignmentLines`: `node`, reused or gone, has no one reading.
    pub(crate) fn reset_alignment_lines(&mut self, node: NodeId) {
        if let Some(lines) = lines_of_mut(self.node_mut(node)) {
            lines.dirty = true;
            lines.used_during_parent_measurement = false;
            lines.previous_used_during_parent_layout = false;
            lines.used_during_parent_layout = false;
            lines.used_by_modifier_measurement = false;
            lines.used_by_modifier_layout = false;
            lines.query_owner = None;
        }
    }
}
