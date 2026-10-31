//! Lookahead, as Compose's `LookaheadScope` and its pass delegates (D1221): the nodes under a scope
//! are measured and placed twice, once for where they'll end up, read by what's around them with
//! `is_looking_ahead()` and the lookahead coordinates, and once for where they are now.
//!
//! The two passes' results are the same fields of a node, kept twice: `LookaheadState` holds the
//! ones of the pass not running. To run a lookahead pass over a scope's outermost root, its subtree
//! changes them around and the ordinary measuring and placing run on what they find. So nothing of
//! it is in the frame's main path but a load and a branch (`UiTree::lookahead_used`), and a tree
//! with no scope has no state.

use std::cell::Cell;
use std::mem;
use std::sync::atomic::{AtomicU32, Ordering};

use compose_runtime::composable;
use smallvec::SmallVec;

use crate::constraints::Constraints;
use crate::coordinates::LayoutCoordinates;
use crate::placement::{NodePlacements, PlacementScope};
use crate::tree::{Layer, LayoutNode, NodeId, UiTree};
use crate::unit::{IntOffset, IntSize, Offset};

/// What a `LookaheadScope` gives its content, as Compose's: the lookahead positions of what's in
/// it, and of what's in another.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LookaheadScope {
    id: u32,
}

/// The scope where a node's composed.
pub(crate) static LOCAL_LOOKAHEAD_SCOPE: compose_runtime::CompositionLocal<Option<LookaheadScope>> =
    compose_runtime::CompositionLocal::new_static("LocalLookaheadScope", || None);

thread_local! {
    /// A scope's been composed on this thread: emitting a node looks for its scope only then.
    static ANY_SCOPE: Cell<bool> = const { Cell::new(false) };
}

/// Whether a node being emitted may be in a scope.
pub(crate) fn any_scope_composed() -> bool {
    ANY_SCOPE.get()
}

impl compose_runtime::ComposeArg for LookaheadScope {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The content's nodes measured and placed ahead too, as Compose's `LookaheadScope`. It adds no
/// node: `content`'s are the children of what's around it.
#[composable]
pub fn LookaheadScope(content: impl Fn(LookaheadScope) + 'static) {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    ANY_SCOPE.set(true);
    let scope =
        compose_runtime::remember(|| LookaheadScope { id: NEXT.fetch_add(1, Ordering::Relaxed) });
    compose_runtime::CompositionLocalProvider(LOCAL_LOOKAHEAD_SCOPE.provides(Some(scope)), || {
        content(scope)
    });
}

/// The results of a node's pass that isn't running: Compose's `LookaheadPassDelegate` and its
/// coordinators' `LookaheadDelegate`s, or, in a lookahead pass, the others.
pub(crate) struct LookaheadState {
    /// The scope it was composed in; 0 if it's only under one (`set_lookahead_scope`).
    pub(crate) scope: u32,
    layers: SmallVec<[Layer; 2]>,
    constraints: Option<Constraints>,
    needs_measure: bool,
    subtree_needs_measure: bool,
    needs_placement: bool,
    subtree_needs_placement: bool,
    placed: bool,
    place_order: u64,
    placements: Option<Box<NodePlacements>>,
    content_placed_in: u64,
    measured_in: u64,
    measured_in_block: bool,
    motion_layers: u8,
    intrinsics_asked: bool,
    children_intrinsics_asked: bool,
}

impl LookaheadState {
    fn new(scope: u32) -> LookaheadState {
        LookaheadState {
            scope,
            layers: SmallVec::new(),
            constraints: None,
            needs_measure: true,
            subtree_needs_measure: false,
            needs_placement: false,
            subtree_needs_placement: false,
            placed: false,
            place_order: 0,
            placements: None,
            content_placed_in: 0,
            measured_in: 0,
            measured_in_block: false,
            motion_layers: 0,
            intrinsics_asked: false,
            children_intrinsics_asked: false,
        }
    }

    /// Changes this pass's fields and the other's around.
    fn change_with(&mut self, value: &mut LayoutNode) {
        // The chain may have changed while the other pass ran.
        self.layers.resize(value.layers.len(), Layer::default());
        if let Some(placements) = &mut self.placements {
            placements.blocks.truncate(value.layers.len());
        }
        mem::swap(&mut self.layers, &mut value.layers);
        mem::swap(&mut self.constraints, &mut value.constraints);
        mem::swap(&mut self.needs_measure, &mut value.needs_measure);
        mem::swap(&mut self.subtree_needs_measure, &mut value.subtree_needs_measure);
        mem::swap(&mut self.needs_placement, &mut value.needs_placement);
        mem::swap(&mut self.subtree_needs_placement, &mut value.subtree_needs_placement);
        mem::swap(&mut self.placed, &mut value.placed);
        mem::swap(&mut self.place_order, &mut value.place_order);
        mem::swap(&mut self.placements, &mut value.placements);
        mem::swap(&mut self.content_placed_in, &mut value.content_placed_in);
        mem::swap(&mut self.measured_in, &mut value.measured_in);
        mem::swap(&mut self.measured_in_block, &mut value.measured_in_block);
        mem::swap(&mut self.motion_layers, &mut value.motion_layers);
        mem::swap(&mut self.intrinsics_asked, &mut value.intrinsics_asked);
        mem::swap(&mut self.children_intrinsics_asked, &mut value.children_intrinsics_asked);
    }
}

/// A lookahead pass over `root`'s subtree, over when it's dropped, a panic in it included.
struct LookaheadPass<'a> {
    tree: &'a mut UiTree,
    root: NodeId,
}

impl Drop for LookaheadPass<'_> {
    fn drop(&mut self) {
        self.tree.in_lookahead = false;
        self.tree.change_passes_below(self.root);
    }
}

impl UiTree {
    /// `node`, where it's composed, is in `scope` or in none.
    pub(crate) fn set_lookahead_scope(&mut self, node: NodeId, scope: Option<LookaheadScope>) {
        let value = self.node_mut(node);
        match (scope, &mut value.lookahead) {
            (Some(scope), Some(state)) => state.scope = scope.id,
            (Some(scope), lookahead) => {
                *lookahead = Some(Box::new(LookaheadState::new(scope.id)));
                self.lookahead_used = true;
                self.invalidate_layout_of(node);
            }
            (None, Some(_)) => {
                // Composed out of any scope: unless it's under one, as what a subcomposition
                // emits may be, it's out of it.
                let parent = value.parent;
                let under_a_scope = parent
                    .and_then(|parent| self.get(parent))
                    .is_some_and(|parent| parent.lookahead.is_some());
                if !under_a_scope {
                    self.node_mut(node).lookahead = None;
                    self.invalidate_layout_of(node);
                }
            }
            (None, None) => {}
        }
    }

    /// Whether `node` is the outermost of a scope: in it, its parent not.
    fn is_outermost_lookahead_root(&self, node: NodeId) -> bool {
        let value = self.node(node);
        value.lookahead.is_some()
            && !value.overlay
            && !value
                .parent
                .and_then(|parent| self.get(parent))
                .is_some_and(|parent| parent.lookahead.is_some())
    }

    /// Changes the passes' fields around, of `root` and all under it.
    fn change_passes_below(&mut self, root: NodeId) {
        let mut state = self.node_mut(root).lookahead.take();
        let state_ref = state.get_or_insert_with(|| Box::new(LookaheadState::new(0)));
        state_ref.change_with(self.node_mut(root));
        self.node_mut(root).lookahead = state;
        for index in 0..self.node(root).children.len() {
            let child = self.node(root).children[index].node;
            self.change_passes_below(child);
        }
    }

    fn lookahead_pass(&mut self, root: NodeId) -> LookaheadPass<'_> {
        self.change_passes_below(root);
        self.in_lookahead = true;
        // It's where the other pass put it, as `placeSelf`: its parent's only placed that once.
        let value = self.node_mut(root);
        let state = value.lookahead.as_ref().expect("just changed");
        let (placed, at) = (state.placed, state.layers.first().map(|it| (it.position, it.offset)));
        value.placed = placed;
        if let (Some((position, offset)), Some(layer)) = (at, value.layers.first_mut()) {
            layer.position = position;
            layer.offset = offset;
        }
        LookaheadPass { tree: self, root }
    }

    /// Before `node`'s measured: if it's a scope's outermost and its lookahead's stale, that runs.
    #[cold]
    pub(crate) fn lookahead_measure_first(&mut self, node: NodeId, constraints: Constraints) {
        if self.in_lookahead || !self.is_outermost_lookahead_root(node) {
            return;
        }
        let state = self.node(node).lookahead.as_ref().expect("a root has it");
        if state.constraints == Some(constraints)
            && !state.needs_measure
            && !state.subtree_needs_measure
        {
            return;
        }
        let pass = self.lookahead_pass(node);
        pass.tree.measure_node(node, constraints);
        drop(pass);
        let state = self.node(node).lookahead.as_ref().expect("a root has it");
        if state.needs_placement || state.subtree_needs_placement {
            self.note_placement_below(Some(node));
        }
    }

    /// Before `node`'s blocks: if it's a scope's outermost and its lookahead has blocks waiting,
    /// they run, after what's around it, as Compose's root's placing them.
    #[cold]
    pub(crate) fn lookahead_place_first(&mut self, node: NodeId) {
        if self.in_lookahead || !self.is_outermost_lookahead_root(node) {
            return;
        }
        let state = self.node(node).lookahead.as_ref().expect("a root has it");
        if !state.needs_placement && !state.subtree_needs_placement {
            return;
        }
        let pass = self.lookahead_pass(node);
        pass.tree.place_pass(node);
        drop(pass);
    }

    /// What dirties a pass from outside dirties the one not running too: a composition, a modifier
    /// or a node asking, as `invalidateMeasurements`. Up to the scope's outermost: above it there's
    /// one pass.
    #[cold]
    pub(crate) fn invalidate_other_pass_measure(&mut self, node: NodeId) {
        let Some(state) = self.get_mut(node).and_then(|value| value.lookahead.as_mut()) else {
            return;
        };
        state.needs_measure = true;
        let mut parent = self.node(node).parent;
        while let Some(id) = parent {
            let value = self.node_mut(id);
            parent = value.parent;
            match value.lookahead.as_mut() {
                Some(state) => state.subtree_needs_measure = true,
                None => return,
            }
        }
    }

    /// A read made in `node`'s lookahead blocks changed, or its rulers are asked for again: they
    /// run again with the frame's other placing.
    #[cold]
    pub(crate) fn invalidate_other_pass_placement(&mut self, node: NodeId) {
        let Some(state) = self.get_mut(node).and_then(|value| value.lookahead.as_mut()) else {
            return;
        };
        state.needs_placement = true;
        let mut current = node;
        loop {
            let parent = self.node(current).parent;
            match parent.and_then(|parent| self.node_mut(parent).lookahead.as_mut()) {
                Some(state) => state.subtree_needs_placement = true,
                None => break,
            }
            current = parent.expect("just found");
        }
        // The outermost is what the frame's placing has to reach.
        self.note_placement_below(Some(current));
    }

    /// Where `layer` of `node` is from its outermost root's parent's content, ahead, and that
    /// root: positions summed, as `LookaheadLayoutCoordinates`.
    fn lookahead_position(&self, node: NodeId, layer: usize) -> (NodeId, IntOffset) {
        let (mut current, mut layer) = (node, layer);
        let mut sum = IntOffset::ZERO;
        loop {
            let value = self.node(current);
            let root = self.is_outermost_lookahead_root(current);
            let layers: &[Layer] = if self.in_lookahead {
                &value.layers
            } else {
                value.lookahead.as_ref().map_or(&value.layers, |state| &state.layers)
            };
            for entry in layers.iter().take(layer + 1).skip(1) {
                sum = sum + entry.offset;
            }
            if root {
                // Its position's the other pass's, as `placeSelf`.
                let main = if self.in_lookahead {
                    value.lookahead.as_ref().map_or(&value.layers, |state| &state.layers)
                } else {
                    &value.layers
                };
                return (current, sum + main.first().map_or(IntOffset::ZERO, |it| it.offset));
            }
            sum = sum + layers.first().map_or(IntOffset::ZERO, |it| it.offset);
            match value.parent {
                Some(parent) if !value.overlay => {
                    layer = self.node(parent).layers.len() - 1;
                    current = parent;
                }
                _ => return (current, sum),
            }
        }
    }

    /// The size `layer` of `node` was measured at, ahead.
    fn lookahead_size(&self, node: NodeId, layer: usize) -> IntSize {
        let value = self.node(node);
        let layers: &[Layer] = if self.in_lookahead {
            &value.layers
        } else {
            value.lookahead.as_ref().map_or(&value.layers, |state| &state.layers)
        };
        layers.get(layer).map_or(IntSize::ZERO, |it| it.measured)
    }
}

impl LookaheadScope {
    /// The coordinates of what the scope's in: its outermost's parent's content, as
    /// `lookaheadScopeCoordinates`.
    pub fn lookahead_scope_coordinates(
        &self,
        placement: &PlacementScope<'_>,
    ) -> Option<LayoutCoordinates> {
        placement.lookahead_scope_coordinates(self.id)
    }

    /// `coordinates`' lookahead results, as `toLookaheadCoordinates`.
    pub fn to_lookahead_coordinates(&self, coordinates: LayoutCoordinates) -> LookaheadCoordinates {
        LookaheadCoordinates(coordinates)
    }

    /// Where `at` in `source` is in `coordinates`, ahead, as `localLookaheadPositionOf`.
    pub fn local_lookahead_position_of(
        &self,
        coordinates: LayoutCoordinates,
        source: LayoutCoordinates,
        at: Offset,
    ) -> Offset {
        LookaheadCoordinates(coordinates).local_position_of(LookaheadCoordinates(source), at)
    }
}

/// A `LayoutCoordinates`' lookahead results, as Compose's `LookaheadLayoutCoordinates`.
#[derive(Clone, Copy, Debug)]
pub struct LookaheadCoordinates(LayoutCoordinates);

impl LookaheadCoordinates {
    /// What they're the lookahead results of.
    pub fn coordinates(&self) -> LayoutCoordinates {
        self.0
    }

    /// Its size ahead.
    #[track_caller]
    pub fn size(&self) -> IntSize {
        self.0
            .read_lookahead(|tree, node, layer| tree.lookahead_size(node, layer))
            .unwrap_or_default()
    }

    /// Where `at` in `source` is in this, ahead, as `LookaheadLayoutCoordinates.localPositionOf`:
    /// positions summed, no layer's transform in them; in two scopes, through what's around them.
    #[track_caller]
    pub fn local_position_of(&self, source: LookaheadCoordinates, at: Offset) -> Offset {
        let this = self.0.read_lookahead(|tree, node, layer| tree.lookahead_position(node, layer));
        let other =
            source.0.read_lookahead(|tree, node, layer| tree.lookahead_position(node, layer));
        let (Some((this_root, this_at)), Some((other_root, other_at))) = (this, other) else {
            return Offset::ZERO;
        };
        let (this_at, other_at) = (
            Offset::new(this_at.x as f32, this_at.y as f32),
            Offset::new(other_at.x as f32, other_at.y as f32),
        );
        if this_root == other_root {
            return other_at + at - this_at;
        }
        let parents = self.0.read_lookahead(|tree, _, _| {
            let parent_of = |root: NodeId| {
                let parent = tree.node(root).parent?;
                let last = tree.node(parent).layers.len() - 1;
                Some(tree.coordinates_of_layer(parent, last))
            };
            Some((parent_of(this_root)?, parent_of(other_root)?))
        });
        match parents.flatten() {
            Some((this_parent, other_parent)) => {
                this_parent.local_position_of(&other_parent, other_at + at) - this_at
            }
            None => other_at + at - this_at,
        }
    }
}
