//! The placement pass, as Compose's placement blocks: a layout's `layout_with` block places its
//! children after the measure pass, top down, once its node has been placed. A state it reads
//! places it again without measuring it. Most layouts place inside `measure` (`MeasureScope::
//! place`): that's a block reading nothing, and keeps nothing (docs/parity/design/
//! placement-and-lookahead.md §3).

use std::rc::Rc;

use smallvec::SmallVec;

use crate::alignment::LayoutDirection;
use crate::constraints::Constraints;
use crate::coordinates::{LayoutCoordinates, with_position_reads_of};
use crate::layout::{Measurable, MeasureScope, Placeable};
use crate::ruler::{LayerRulers, Ruler, RulerSource};
use crate::tree::{NodeId, Observation, UiTree, observation_key};
use crate::unit::{Density, IntOffset, Offset};

type Block = Rc<dyn Fn(&mut PlacementScope<'_>)>;

/// A node whose placing code asked where it is, as Compose's `coordinatesAccessedDuringPlacement`
/// and `onCoordinatesUsed`.
#[derive(Clone, Copy)]
pub(crate) struct CoordinateReader {
    pub(crate) node: NodeId,
    /// Where it and its ancestors were then.
    pub(crate) signature: u64,
    /// Its placement blocks asked.
    pub(crate) blocks: bool,
    /// What was told it was placed asked.
    pub(crate) told: bool,
}

/// The block a layer's measurement left its placing to.
pub(crate) struct LayerBlock {
    block: Block,
    /// That measurement: what it measured, a block can't measure again, as Compose's
    /// `measuredByParent`.
    measure_run: u64,
    /// It hasn't run since: what it places was just measured, no child being placed yet.
    first: bool,
    /// It ran early, for an alignment line, and needn't when the node is placed.
    pub(crate) early: bool,
}

/// The placement blocks of a node's layers.
#[derive(Default)]
pub(crate) struct NodePlacements {
    /// What each layer provides of rulers (D1218).
    pub(crate) rulers: SmallVec<[Option<crate::ruler::LayerRulers>; 1]>,
    /// Per layer: the block of its last measurement, if it left its placing to one.
    pub(crate) blocks: SmallVec<[Option<LayerBlock>; 2]>,
}

impl NodePlacements {
    fn any(&self) -> bool {
        self.blocks.iter().any(Option::is_some)
    }
}

/// What a placement block places with, as `Placeable.PlacementScope`.
pub struct PlacementScope<'a> {
    scope: MeasureScope<'a>,
    /// The width the layout reported, to mirror in.
    width: i32,
    /// The measurement of the layer whose block this is: what it measured can't be measured here.
    measure_run: u64,
}

impl PlacementScope<'_> {
    /// Places `placeable` at `x`, `y` in the layout's coordinates, as `Placeable.place`.
    ///
    /// # Panics
    ///
    /// If this run already placed it, or the measurement it's part of did (the fused path and a
    /// block don't mix: `layout_with` panics on a child placed before it).
    pub fn place(&mut self, placeable: &Placeable, x: i32, y: i32) {
        self.scope.place(placeable, x, y);
    }

    /// As [`place`](Self::place), at `position`.
    pub fn place_at(&mut self, placeable: &Placeable, position: IntOffset) {
        self.scope.place_at(placeable, position);
    }

    /// Places `placeable` at `x` from the start, as `placeRelative`: in a right-to-left layout,
    /// mirrored in the layout's width.
    pub fn place_relative(&mut self, placeable: &Placeable, x: i32, y: i32) {
        // A layout of no width mirrors nothing, as `placeAutoMirrored`.
        let x = match self.layout_direction() {
            LayoutDirection::Rtl if self.width != 0 => self.width - x - placeable.width,
            _ => x,
        };
        self.place(placeable, x, y);
    }

    /// Measures `measurable`, as measuring in Compose's placement block may: a new size places
    /// this layout again rather than measuring it.
    ///
    /// # Panics
    ///
    /// If this run already measured it, or the measurement this block is from did, as
    /// `trackMeasurementByParent`.
    pub fn measure(&mut self, measurable: Measurable, constraints: Constraints) -> Placeable {
        let tree = &*self.scope.base.tree;
        let value = tree.node(measurable.node);
        let measured_in = if measurable.layer == 0 {
            value.measured_in
        } else {
            value.layers[measurable.layer as usize].measured_in
        };
        assert!(
            measured_in != self.measure_run,
            "compose-ui: measured twice: in a measurement and its placement block"
        );
        let placeable = self.scope.measure(measurable, constraints);
        if measurable.layer == 0 {
            self.scope.base.tree.enter().node_mut(measurable.node).measured_in_block = true;
        }
        placeable
    }

    /// The coordinates of what's placing, as `coordinates`: a query of where they are makes this
    /// block run again when that changes (D1212), the others (`size`, `is_attached`) don't.
    /// `None` while an alignment line is worked out, as Compose's: it runs again when it's placed.
    pub fn coordinates(&self) -> Option<LayoutCoordinates> {
        if self.scope.base.tree.placing_for_alignment {
            self.scope.base.tree.asked_while_aligning.set(true);
            return None;
        }
        Some(self.scope.base.tree.coordinates_of_layer(self.scope.base.node, self.scope.layer))
    }

    /// `MeasureScope::with_motion_frame_of_reference_placement`.
    pub fn with_motion_frame_of_reference_placement<R>(
        &mut self,
        block: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let motion = std::mem::replace(&mut self.scope.motion, true);
        let result = block(self);
        self.scope.motion = motion;
        result
    }

    /// `placeable[line]`: its position in the placeable, or `AlignmentLine::UNSPECIFIED`. What it
    /// reads changing places this again.
    pub fn alignment_line(
        &mut self,
        placeable: &Placeable,
        line: impl Into<crate::alignment_line::AlignmentLine>,
    ) -> i32 {
        let measurable = placeable.measurable;
        self.scope.base.tree.enter().read_alignment_line(measurable, line.into(), true)
    }

    /// `ruler`'s value in these coordinates, as `ruler.current(default)`: its nearest provider's,
    /// at this layout or around it, else `default`. A change to it places this again; so does its
    /// reader moving, as it asks where it is.
    pub fn current(&mut self, ruler: &Ruler, default: f32) -> f32 {
        if let Some(derivation) = ruler.derivation() {
            return derivation(self, default);
        }
        // Unavailable while working out a line, as Compose's: it runs again when it's placed.
        if self.scope.base.tree.placing_for_alignment {
            self.scope.base.tree.asked_while_aligning.set(true);
            return default;
        }
        let node = self.scope.base.node;
        self.scope.base.tree.enter().note_ruler_reader(ruler, node);
        let target = self.coordinates().expect("placing");
        let (mut at, mut layer) = (node, self.scope.layer);
        loop {
            let tree = &*self.scope.base.tree;
            let value = tree.node(at);
            let provided = value.placements.as_ref().and_then(|placements| {
                let rulers = placements.rulers.get(layer)?.as_ref()?;
                match &rulers.source {
                    RulerSource::Block(_) => {
                        rulers.values.iter().find(|(it, _)| it == ruler).map(|(_, value)| *value)
                    }
                    RulerSource::Lazily(is_provided, provider) => {
                        let (is_provided, provider) = (is_provided.clone(), provider.clone());
                        is_provided(ruler).then(|| {
                            let mut scope = tree.ruler_scope(at, layer);
                            provider(&mut scope, ruler);
                            scope.value_of(ruler)
                        })?
                    }
                }
            });
            if let Some(provided) = provided.filter(|provided| !provided.is_nan()) {
                let source = tree.coordinates_of_layer(at, layer);
                return ruler.in_coordinates(provided, &source, &target);
            }
            if layer > 0 {
                layer -= 1;
                continue;
            }
            match value.parent {
                Some(parent) if !value.overlay => {
                    at = parent;
                    layer = tree.node(parent).layers.len() - 1;
                }
                _ => return default,
            }
        }
    }

    /// The layout direction where the node is composed.
    pub fn layout_direction(&self) -> LayoutDirection {
        self.scope.layout_direction()
    }

    /// The density where the node is composed.
    pub fn density(&self) -> Density {
        self.scope.density()
    }

    /// The value of `local` where the node is composed, as `MeasureScope::current_value_of`.
    pub fn current_value_of<T: Clone + 'static>(
        &self,
        local: &compose_runtime::CompositionLocal<T>,
    ) -> T {
        self.scope.current_value_of(local)
    }
}

impl UiTree {
    /// Keeps what the measurement of `layer` of `node` provides of rulers.
    pub(crate) fn set_rulers(&mut self, node: NodeId, layer: usize, source: RulerSource) {
        let placements = self.node_mut(node).placements.get_or_insert_with(Default::default);
        if placements.rulers.len() <= layer {
            placements.rulers.resize_with(layer + 1, || None);
        }
        placements.rulers[layer] = Some(LayerRulers { source, values: Default::default() });
    }

    /// Keeps `block`, what the measurement `run` of `layer` of `node` left its placing to, for the
    /// placement pass. What it replaces is the caller's to drop, its captures being application
    /// code's, outside the tree.
    pub(crate) fn set_placement_block(
        &mut self,
        node: NodeId,
        layer: usize,
        measure_run: u64,
        block: Block,
    ) -> Option<LayerBlock> {
        let value = self.node_mut(node);
        let placements = value.placements.get_or_insert_with(Default::default);
        if placements.blocks.len() <= layer {
            placements.blocks.resize_with(layer + 1, || None);
        }
        let earlier = placements.blocks[layer].replace(LayerBlock {
            block,
            measure_run,
            first: true,
            early: false,
        });
        self.note_needs_placement(node);
        earlier
    }

    /// `node` asked to be placed again, as `invalidatePlacement`, or a child measured in its block
    /// has a new size: its blocks run, or, placing inside its measurement, it's measured.
    pub(crate) fn invalidate_placement_of(&mut self, node: NodeId) {
        let Some(value) = self.get(node) else { return };
        if value.placements.as_ref().is_some_and(|placements| placements.any()) {
            self.mark_needs_placement(node);
        } else {
            self.invalidate_layout_of(node);
        }
    }

    /// What `node`'s blocks read changed: they run again in the next layout's placement pass,
    /// and it isn't measured, as `requestRelayout`. Those of a measurement that made none since
    /// are over: nothing to do.
    pub(crate) fn placement_read_changed(&mut self, node: NodeId) {
        let Some(value) = self.get(node) else { return };
        if value.placements.as_ref().is_some_and(|placements| placements.any()) {
            self.mark_needs_placement(node);
        }
    }

    /// `node`'s blocks must run. Not placed or deactivated, it only waits, as `requestRelayout`
    /// marking it pending: nothing's asked of a frame until it's placed (`place_in_run`).
    pub(crate) fn mark_needs_placement(&mut self, node: NodeId) {
        // What ran early isn't what's asked now.
        if self.alignment_lines_read
            && let Some(placements) = &mut self.node_mut(node).placements
        {
            for block in placements.blocks.iter_mut().flatten() {
                block.early = false;
            }
        }
        self.note_needs_placement(node);
    }

    /// `mark_needs_placement`, for a new block: what's inside it ran as it should.
    fn note_needs_placement(&mut self, node: NodeId) {
        let value = self.node_mut(node);
        value.needs_placement = true;
        if value.placed && !value.deactivated {
            let parent = value.parent;
            self.note_placement_below(parent);
            self.needs_draw = true;
        }
    }

    /// `node` and its ancestors have something below them for the placement pass.
    pub(crate) fn note_placement_below(&mut self, node: Option<NodeId>) {
        let mut current = node;
        while let Some(id) = current {
            let value = self.node_mut(id);
            if value.subtree_needs_placement {
                return;
            }
            value.subtree_needs_placement = true;
            current = value.parent;
        }
    }

    /// Runs the blocks waiting in `node`'s subtree, top down: each once its node is placed, the
    /// children it places after it. A child not placed keeps what it waits for until it is.
    pub(crate) fn place_pass(&mut self, node: NodeId) {
        let ran = self.node(node).needs_placement;
        if ran {
            self.run_placement_blocks(node);
            // What measuring its own content asked of it is done too.
            self.node_mut(node).needs_placement = false;
        }
        // Cleared after: what the children's blocks place climbs no higher than this.
        if self.node(node).subtree_needs_placement {
            for index in 0..self.node(node).children.len() {
                let Some(child) = self.node(node).children.get(index).map(|child| child.node)
                else {
                    break;
                };
                let value = self.node(child);
                if value.placed
                    && !value.deactivated
                    && (value.needs_placement || value.subtree_needs_placement)
                {
                    self.place_pass(child);
                }
            }
            self.node_mut(node).subtree_needs_placement = false;
        }
        if ran && self.alignment_lines_read {
            self.alignment_uses_placed(node);
        }
    }

    /// `node` was placed with blocks waiting in it or under it, passed over until now.
    #[cold]
    pub(crate) fn placed_with_blocks_waiting(&mut self, node: NodeId) {
        let parent = self.node(node).parent;
        self.note_placement_below(parent);
    }

    /// Runs `node`'s blocks, outermost first, under one observation: what they read places it
    /// again. Each is read when its turn comes: an outer block measuring what's inside it makes
    /// the inner one. A panic leaves the flags as they were, for a later layout (`place_pass`
    /// clears them after).
    pub(crate) fn run_placement_blocks(&mut self, node: NodeId) {
        if !self.new_layers.is_empty() {
            self.run_new_layers();
        }
        let lines = self.alignment_lines_read;
        if lines {
            self.clear_alignment_uses_in_placement(node);
        }
        self.node_mut(node).laying_out = true;
        // Some ran already to know a line, what they read kept.
        let adding = lines
            && (self.node(node).placements.as_ref())
                .is_some_and(|it| it.blocks.iter().flatten().any(|block| block.early));
        let observer = self.observer.clone();
        let mut run = || {
            let mut layer = 0;
            let mut again = false;
            while let Some(found) = self.block_of_layer(node, layer) {
                self.capture_rulers(node, layer);
                if let Some((block, measure_run, first)) = found {
                    again |= !first;
                    self.run_placement_block(node, layer, measure_run, first, &block);
                }
                layer += 1;
            }
            again
        };
        let (again, reads) = with_position_reads_of(node, || match &observer {
            Some(observer) if adding => {
                observer.observe_adding(observation_key(node, Observation::Placement), run)
            }
            Some(observer) => observer.observe(observation_key(node, Observation::Placement), run),
            None => run(),
        });
        self.node_mut(node).laying_out = false;
        if reads || self.node(node).reader_at != u32::MAX {
            self.note_reader(node, reads, false);
        }
        if again && self.node(node).kinds.any(crate::node_chain::NodeKinds::ON_PLACED) {
            self.placed_layers.push((node, 0));
        }
    }

    /// `node`'s placing code ran, asking where it is if `reads`: its blocks, or with `told` what
    /// it told of its placing. It runs again when that changes.
    pub(crate) fn note_reader(&mut self, node: NodeId, reads: bool, told: bool) {
        let at = self.node(node).reader_at;
        if !reads && at == u32::MAX {
            return;
        }
        let signature = self.signature_of_reader(node);
        let at = if at == u32::MAX {
            let reader = CoordinateReader { node, signature, blocks: false, told: false };
            self.coordinate_readers.push(reader);
            let at = self.coordinate_readers.len() - 1;
            self.node_mut(node).reader_at = at as u32;
            at
        } else {
            at as usize
        };
        let reader = &mut self.coordinate_readers[at];
        if told {
            reader.told = reads;
            if reads {
                reader.signature = signature;
            }
        } else {
            reader.blocks = reads;
            // What was told it was placed hears of the move this block's run takes in.
            if reader.signature != signature {
                reader.signature = signature;
                if reader.told {
                    self.placed_layers.push((node, 0));
                }
            }
        }
        let reader = &self.coordinate_readers[at];
        if !reader.blocks && !reader.told {
            self.forget_reader(at);
        }
    }

    /// Reader `at` isn't one any more.
    fn forget_reader(&mut self, at: usize) {
        let gone = self.coordinate_readers.swap_remove(at);
        if let Some(value) = self.get_mut(gone.node) {
            value.reader_at = u32::MAX;
        }
        if let Some(moved) = self.coordinate_readers.get(at) {
            let node = moved.node;
            self.node_mut(node).reader_at = at as u32;
        }
    }

    /// Where a reader is: the layer inside all of its node, as it may ask from any, and where
    /// each of its ancestors is. A parent and grandparent moving by opposite amounts leave its
    /// position in the root as it was, not its position in its parent.
    pub(crate) fn signature_of_reader(&self, node: NodeId) -> u64 {
        let mix = |hash: u64, point: Offset| {
            let bits = (point.x.to_bits() as u64) << 32 | point.y.to_bits() as u64;
            (hash.rotate_left(5) ^ bits).wrapping_mul(0x517c_c1b7_2722_0a95)
        };
        let last = self.node(node).layers.len() - 1;
        let mut hash = mix(0, self.window_origin);
        let mut current = node;
        let mut layer = last;
        let mut point = Offset::ZERO;
        loop {
            let value = self.node(current);
            point = self.map_out_of_node(current, layer, point);
            hash = mix(hash, point);
            match value.parent {
                Some(parent) if !value.overlay => {
                    current = parent;
                    layer = self.node(parent).layers.len() - 1;
                }
                _ => return hash,
            }
        }
    }

    /// Runs the blocks of those that asked where they are and aren't there any more, and tells
    /// what told them, as `requestLayoutIfCoordinatesAreUsedAndNotifyChildren`. Until none moves:
    /// what they place may be another. Top down, each runs once: what's above it moved it first,
    /// or it wasn't waiting.
    pub(crate) fn place_readers_moved(&mut self, root: NodeId) {
        loop {
            let mut moved = false;
            let mut index = 0;
            while index < self.coordinate_readers.len() {
                let reader = self.coordinate_readers[index];
                let Some(value) = self.get(reader.node) else {
                    self.forget_reader(index);
                    continue;
                };
                if value.placed
                    && !value.deactivated
                    && !value.needs_placement
                    && self.is_placed_in_root(reader.node)
                {
                    let signature = self.signature_of_reader(reader.node);
                    if signature != reader.signature {
                        self.coordinate_readers[index].signature = signature;
                        if reader.told {
                            self.placed_layers.push((reader.node, 0));
                        }
                        if reader.blocks {
                            self.mark_needs_placement(reader.node);
                            moved = true;
                        }
                    }
                }
                index += 1;
            }
            if !moved {
                return;
            }
            self.place_pass(root);
        }
    }

    /// The block of `layer` of `node` now, if it has one (with what its run needs), or `None` past
    /// its last.
    #[allow(clippy::type_complexity)]
    fn block_of_layer(&mut self, node: NodeId, layer: usize) -> Option<Option<(Block, u64, bool)>> {
        let blocks = &mut self.node_mut(node).placements.as_mut()?.blocks;
        Some(blocks.get_mut(layer)?.as_mut().and_then(|entry| {
            if std::mem::take(&mut entry.early) {
                return None;
            }
            let first = std::mem::replace(&mut entry.first, false);
            Some((entry.block.clone(), entry.measure_run, first))
        }))
    }

    /// The block of `layer` of `node`, if it waits, now, and not again when the node's placed:
    /// a line's read of what's inside it, by a modifier around it whose own block may not exist.
    pub(crate) fn run_block_for_alignment(&mut self, node: NodeId, layer: usize) {
        let value = self.node_mut(node);
        let waits = value.needs_placement;
        let Some(entry) = (value.placements.as_mut())
            .and_then(|it| it.blocks.get_mut(layer)?.as_mut())
            .filter(|entry| !entry.early && (entry.first || waits))
        else {
            return;
        };
        entry.early = true;
        let first = std::mem::replace(&mut entry.first, false);
        let (block, measure_run) = (entry.block.clone(), entry.measure_run);
        let aligning = std::mem::replace(&mut self.placing_for_alignment, true);
        let asked = self.asked_while_aligning.replace(false);
        let observer = self.observer.clone();
        let mut run = || self.run_placement_block(node, layer, measure_run, first, &block);
        match &observer {
            Some(observer) => {
                observer.observe_adding(observation_key(node, Observation::Placement), run)
            }
            None => run(),
        }
        self.placing_for_alignment = aligning;
        if self.asked_while_aligning.get()
            && let Some(entry) = (self.node_mut(node).placements.as_mut())
                .and_then(|it| it.blocks.get_mut(layer)?.as_mut())
        {
            entry.early = false;
        }
        self.asked_while_aligning.set(asked);
    }

    fn run_placement_block(
        &mut self,
        node: NodeId,
        layer: usize,
        measure_run: u64,
        first: bool,
        block: &Block,
    ) {
        let value = self.node_mut(node);
        let width = value.layers[layer].measured.width;
        let places_children = layer + 1 == value.layers.len();
        if !first && places_children {
            // What it doesn't place now isn't placed, as `markSubtreeAsNotPlaced`.
            for index in 0..self.node(node).children.len() {
                let child = self.node(node).children[index].node;
                let child = self.node_mut(child);
                child.placed = false;
                child.measured_in_block = false;
            }
            self.invalidate_children_draw_of(node);
        }
        let run = self.next_measure_run();
        let mut scope = MeasureScope::new(&mut *self, node, run);
        scope.layer = layer;
        let _open = scope.base.tree.open();
        let mut placement = PlacementScope { scope, width, measure_run };
        block(&mut placement);
    }
}
