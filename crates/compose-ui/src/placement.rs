//! The placement pass, as Compose's placement blocks: a layout's `layout_with` block places its
//! children after the measure pass, top down, once its node has been placed. A state it reads
//! places it again without measuring it. Most layouts place inside `measure` (`MeasureScope::
//! place`): that's a block reading nothing, and keeps nothing (docs/parity/design/
//! placement-and-lookahead.md §3).

use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

use smallvec::SmallVec;

use crate::alignment::LayoutDirection;
use crate::constraints::Constraints;
use crate::layout::{Measurable, MeasureScope, Placeable};
use crate::tree::{NodeId, Observation, UiTree, observation_key};
use crate::unit::{Density, IntOffset};

type Block = Rc<dyn Fn(&mut PlacementScope<'_>)>;

/// The placement blocks of a node's layers.
#[derive(Default)]
pub(crate) struct NodePlacements {
    /// Per layer: the block of its last measurement, if it left its placing to one.
    pub(crate) blocks: SmallVec<[Option<Block>; 2]>,
    /// The measurement that made the node's blocks, if none ran since: their first run is part
    /// of it (what it measured a block can't measure again).
    first_run: Option<u64>,
    /// Per layer: the constraints a block measured the layer inside it with, as a layout node
    /// answers a measurement that's the same as its last without measuring.
    content_constraints: SmallVec<[Option<Constraints>; 2]>,
}

/// What a placement block places with, as `Placeable.PlacementScope`.
pub struct PlacementScope<'a> {
    scope: MeasureScope<'a>,
    /// The width the layout reported, to mirror in.
    width: i32,
    /// This run of the block isn't its measurement's: nothing's changed unless it was asked to.
    again: bool,
}

impl PlacementScope<'_> {
    /// Places `placeable` at `x`, `y` in the layout's coordinates, as `Placeable.place`.
    ///
    /// # Panics
    ///
    /// If this run already placed it, or the measurement it's part of did.
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
        let x = match self.layout_direction() {
            LayoutDirection::Ltr => x,
            LayoutDirection::Rtl => self.width - x - placeable.width,
        };
        self.place(placeable, x, y);
    }

    /// Measures `measurable`, as measuring in Compose's placement block may: a new size places
    /// this layout again rather than measuring it.
    ///
    /// # Panics
    ///
    /// If this run or the measurement it's part of already measured it.
    pub fn measure(&mut self, measurable: Measurable, constraints: Constraints) -> Placeable {
        let layer = measurable.layer as usize;
        if layer > 0 {
            let tree = &mut *self.scope.base.tree;
            let value = tree.node_mut(measurable.node);
            let placements = value.placements.get_or_insert_with(Default::default);
            if placements.content_constraints.len() <= layer {
                placements.content_constraints.resize(layer + 1, None);
            }
            let last = placements.content_constraints[layer].replace(constraints);
            // A layer has no cache of its own: as a node measured with the same constraints.
            if self.again && last == Some(constraints) {
                let entry = &value.layers[layer];
                let (size, measured) = (entry.size, entry.measured);
                let width = size.width;
                let height = size.height;
                return Placeable { measurable, width, height, measured };
            }
        }
        let placeable = self.scope.measure(measurable, constraints);
        if layer == 0 {
            self.scope.base.tree.node_mut(measurable.node).measured_in_block = true;
        }
        placeable
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
    /// Keeps `block`, what the measurement `run` of `layer` of `node` left its placing to, for the
    /// placement pass.
    pub(crate) fn set_placement_block(
        &mut self,
        node: NodeId,
        layer: usize,
        run: u64,
        block: Block,
    ) {
        let value = self.node_mut(node);
        let placements = value.placements.get_or_insert_with(Default::default);
        if placements.blocks.len() <= layer {
            placements.blocks.resize(layer + 1, None);
        }
        placements.blocks[layer] = Some(block);
        placements.first_run = Some(run);
        self.invalidate_placement_flags(node);
    }

    /// What `node`'s blocks read changed, or it asked: they run again, in the next layout's
    /// placement pass, and it isn't measured, as `requestRelayout`. A node placing inside its
    /// measurement has no other way: it's measured.
    pub(crate) fn invalidate_placement_of(&mut self, node: NodeId) {
        let Some(value) = self.get(node) else { return };
        let has_block = value
            .placements
            .as_ref()
            .is_some_and(|placements| placements.blocks.iter().any(Option::is_some));
        if !has_block {
            self.invalidate_measure(node);
            return;
        }
        // A deactivated node's run when it's reused.
        self.invalidate_placement_flags(node);
        self.needs_draw = true;
    }

    fn invalidate_placement_flags(&mut self, node: NodeId) {
        let value = self.node_mut(node);
        value.needs_placement = true;
        let parent = value.parent;
        self.note_placement_below(parent);
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
        let value = self.node_mut(node);
        if std::mem::take(&mut value.needs_placement) {
            self.run_placement_blocks(node);
        }
        let value = self.node_mut(node);
        if std::mem::take(&mut value.subtree_needs_placement) {
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
        }
    }

    /// Runs `node`'s blocks, outermost first, under one observation: what they read places it
    /// again. A block that isn't its measurement's first has nothing placed yet.
    fn run_placement_blocks(&mut self, node: NodeId) {
        let value = self.node_mut(node);
        let Some(placements) = &mut value.placements else { return };
        let first_run = placements.first_run.take();
        let blocks: SmallVec<[(usize, Block); 2]> = (placements.blocks.iter().enumerate())
            .filter_map(|(layer, block)| Some((layer, block.clone()?)))
            .collect();
        let again = first_run.is_none();
        let observer = self.observer.clone();
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut run = || {
                for (layer, block) in &blocks {
                    self.run_placement_block(node, *layer, first_run, block);
                }
            };
            match &observer {
                Some(observer) => {
                    observer.observe(observation_key(node, Observation::Placement), run)
                }
                None => run(),
            }
        }));
        if let Err(panic) = result {
            self.node_mut(node).needs_placement = true;
            panic::resume_unwind(panic);
        }
        if again && self.node(node).kinds.any(crate::node_chain::NodeKinds::ON_PLACED) {
            self.placed_layers.push((node, 0));
        }
    }

    fn run_placement_block(
        &mut self,
        node: NodeId,
        layer: usize,
        first_run: Option<u64>,
        block: &Block,
    ) {
        let again = first_run.is_none();
        let value = self.node_mut(node);
        let width = value.layers[layer].measured.width;
        let places_children = layer + 1 == value.layers.len();
        if again && places_children {
            // What it doesn't place now isn't placed, as `markSubtreeAsNotPlaced`.
            for index in 0..self.node(node).children.len() {
                let child = self.node(node).children[index].node;
                self.node_mut(child).placed = false;
            }
            self.invalidate_children_draw_of(node);
        }
        let run = first_run.unwrap_or_else(|| self.next_measure_run());
        let mut scope = MeasureScope::new(&mut *self, node, run);
        scope.layer = layer;
        let mut placement = PlacementScope { scope, width, again };
        block(&mut placement);
    }
}
