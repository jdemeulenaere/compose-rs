//! Alignment lines, as Compose's `AlignmentLine` and `LayoutNodeAlignmentLines`: positions a layout
//! gives with its size (`MeasureScope::provide_alignment_lines`), asked of a child while measuring
//! or placing (`MeasureScope::alignment_line`), inherited through what's placed. Reading one may
//! run blocks waiting below it. A change tells only who read, and a tree where nothing's read pays
//! one load in the places they'd be told from (D1219).

use std::sync::atomic::{AtomicU32, Ordering};

use smallvec::SmallVec;

use crate::layout::Measurable;
use crate::tree::{LayoutNode, NodeId, UiTree};
use crate::unit::Offset;

/// A position a layout gives its ancestors, along one axis, as Compose's `AlignmentLine`: what
/// [`HorizontalAlignmentLine`] and [`VerticalAlignmentLine`] make. What merges two children's is
/// a function, so a line is `Copy` and [`FIRST_BASELINE`] a constant.
#[derive(Clone, Copy)]
pub struct AlignmentLine {
    id: u32,
    horizontal: bool,
    merger: fn(i32, i32) -> i32,
}

impl AlignmentLine {
    /// What's read of a line nothing gives, as `AlignmentLine.Unspecified`.
    pub const UNSPECIFIED: i32 = i32::MIN;

    fn make(horizontal: bool, merger: fn(i32, i32) -> i32) -> AlignmentLine {
        static NEXT: AtomicU32 = AtomicU32::new(3);
        AlignmentLine { id: NEXT.fetch_add(1, Ordering::Relaxed), horizontal, merger }
    }

    /// Whether it's a position along the height.
    pub fn is_horizontal(&self) -> bool {
        self.horizontal
    }

    fn of(&self, point: crate::unit::IntOffset) -> i32 {
        if self.horizontal { point.y } else { point.x }
    }
}

impl PartialEq for AlignmentLine {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for AlignmentLine {}

impl std::fmt::Debug for AlignmentLine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let axis = if self.horizontal { "Horizontal" } else { "Vertical" };
        write!(formatter, "{axis}AlignmentLine({})", self.id)
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

const fn smaller(first: i32, second: i32) -> i32 {
    if first < second { first } else { second }
}

const fn larger(first: i32, second: i32) -> i32 {
    if first > second { first } else { second }
}

/// The baseline of a layout's first line of text, as `FirstBaseline`.
pub const FIRST_BASELINE: HorizontalAlignmentLine =
    HorizontalAlignmentLine(AlignmentLine { id: 1, horizontal: true, merger: smaller });

/// The baseline of a layout's last line of text, as `LastBaseline`.
pub const LAST_BASELINE: HorizontalAlignmentLine =
    HorizontalAlignmentLine(AlignmentLine { id: 2, horizontal: true, merger: larger });

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

/// What a layer gives of `line`: of a text's baselines, kept in the node, or the others.
fn provided_at(value: &LayoutNode, layer: usize, line: AlignmentLine) -> Option<i32> {
    if let Some((at, first, last)) = value.baselines
        && at as usize == layer
    {
        if line == FIRST_BASELINE.0 {
            return Some(first);
        }
        if line == LAST_BASELINE.0 {
            return Some(last);
        }
    }
    value.alignment.as_ref().and_then(|lines| lines.provided_at(layer, line))
}

/// All a node's layers give, a text's baselines with the others.
fn all_provided(value: &LayoutNode) -> SmallVec<[(u32, AlignmentLine, i32); 4]> {
    let mut provided = SmallVec::new();
    if let Some((at, first, last)) = value.baselines {
        provided.push((at, FIRST_BASELINE.0, first));
        provided.push((at, LAST_BASELINE.0, last));
    }
    if let Some(lines) = &value.alignment {
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

impl UiTree {
    fn alignment_mut(&mut self, node: NodeId) -> &mut AlignmentLines {
        self.node_mut(node).alignment.get_or_insert_with(Default::default)
    }

    /// A text's baselines, without a box for each text (D1219).
    pub(crate) fn give_baselines(&mut self, node: NodeId, layer: usize, first: i32, last: i32) {
        let baselines = Some((layer as u32, first, last));
        let value = self.node_mut(node);
        let changed = value.baselines != baselines;
        value.baselines = baselines;
        if changed && self.alignment_lines_read {
            self.on_alignments_changed(node);
        }
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
        let lines = self.alignment_mut(node);
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
            if let Some(lines) = &mut value.alignment {
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
    pub(crate) fn clear_alignment_uses_in_measure(&mut self, node: NodeId) {
        if let Some(lines) = &mut self.node_mut(node).alignment {
            lines.used_by_modifier_measurement = false;
        }
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            if let Some(lines) = &mut self.node_mut(child).alignment {
                lines.used_during_parent_measurement = false;
            }
        }
    }

    /// `node`'s blocks start: what they read they say again, as `layoutChildrenBlock`.
    pub(crate) fn clear_alignment_uses_in_placement(&mut self, node: NodeId) {
        if let Some(lines) = &mut self.node_mut(node).alignment {
            lines.used_by_modifier_layout = false;
        }
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            if let Some(lines) = &mut self.node_mut(child).alignment {
                lines.used_during_parent_layout = false;
            }
        }
    }

    /// `node`'s blocks ran: as the end of `layoutChildrenBlock` and of `layoutChildren`.
    pub(crate) fn alignment_uses_placed(&mut self, node: NodeId) {
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            if let Some(lines) = &mut self.node_mut(child).alignment {
                lines.previous_used_during_parent_layout = lines.used_during_parent_layout;
            }
        }
        self.finish_layout_children(node);
    }

    fn finish_layout_children(&mut self, node: NodeId) {
        let Some(lines) = &mut self.node_mut(node).alignment else {
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
        self.node(node).alignment.as_ref().is_some_and(|lines| lines.query_owner.is_some())
    }

    fn query_owner_of(&self, node: NodeId) -> Option<NodeId> {
        self.node(node).alignment.as_ref().and_then(|lines| lines.query_owner)
    }

    fn is_queried(&self, node: NodeId) -> bool {
        self.get(node).and_then(|value| value.alignment.as_ref()).is_some_and(|it| it.queried())
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
        if owner.is_some() || self.node(node).alignment.is_some() {
            self.alignment_mut(node).query_owner = owner;
        }
    }

    /// `measurable[line]`, as `MeasurePassDelegate.get` and `NodeCoordinator.get`.
    pub(crate) fn read_alignment_line(
        &mut self,
        measurable: Measurable,
        line: AlignmentLine,
        placing: bool,
    ) -> i32 {
        self.alignment_lines_read = true;
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
            let lines = self.node(node).alignment.as_ref();
            (lines.into_iter().flat_map(|lines| lines.map.iter()))
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
            let aligning = std::mem::replace(&mut self.placing_for_alignment, true);
            let asked = self.asked_while_aligning.replace(false);
            self.run_placement_blocks(node);
            self.node_mut(node).needs_placement = false;
            // Where it is it wasn't told: it runs in the placement pass too.
            if self.asked_while_aligning.get() {
                self.mark_needs_placement(node);
            }
            self.asked_while_aligning.set(asked);
            self.placing_for_alignment = aligning;
            self.alignment_uses_placed(node);
        } else {
            self.finish_layout_children(node);
        }
    }

    /// As `recalculate`: what `node`'s placed children give, in its content.
    fn recalculate_lines(&mut self, node: NodeId) {
        let mut map = Lines::new();
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            let value = self.node(child);
            if !value.placed || value.deactivated {
                continue;
            }
            if value.alignment.as_ref().is_none_or(|lines| lines.dirty) {
                self.layout_children(child);
            }
            let value = self.node(child);
            let last = value.layers.len() - 1;
            let inherited = value.alignment.as_ref().map(|lines| lines.map.clone());
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
        let lines = value.alignment.get_or_insert_with(Default::default);
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
        let position = if line.horizontal { point.y } else { point.x }.round() as i32;
        match map.iter_mut().find(|(it, _)| *it == line) {
            Some(entry) => entry.1 = (line.merger)(entry.1, position),
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
        let end = if layer == 0 { 0 } else { value.layout_indices[layer - 1] as usize };
        for index in (0..end).rev() {
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
            let mut merged = AlignmentLine::UNSPECIFIED;
            for child in &value.children {
                let of_child = self.node(child.node);
                if !of_child.placed || of_child.deactivated {
                    continue;
                }
                let inner = self.peek_alignment_line(child.node, 0, line);
                if inner == AlignmentLine::UNSPECIFIED {
                    continue;
                }
                // `get` counted how it's centered; its position counts it too, once in the end.
                let inner = inner - line.of(crate::measure::centering(&of_child.layers[0]));
                let point = self.map_line_out_of_node(child.node, 0, inner as f32);
                let position = if line.horizontal { point.y } else { point.x }.round() as i32;
                merged = if merged == AlignmentLine::UNSPECIFIED {
                    position
                } else {
                    (line.merger)(merged, position)
                };
            }
            merged
        };
        if measured == AlignmentLine::UNSPECIFIED {
            return measured;
        }
        measured + line.of(crate::measure::centering(&value.layers[layer]))
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
        self.node(node).alignment.as_ref().map_or(0, |lines| lines.told_coordinates)
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
        if let Some(lines) = &mut self.node_mut(node).alignment {
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
