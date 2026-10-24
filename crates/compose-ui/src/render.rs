//! The draw pass: records the display list from the tree (docs/06-ui.md, "Drawing").
//!
//! A repaint boundary keeps what it drew (`RetainedDrawing`), as Compose's layer keeps its
//! render node's recording: a draw records what's outside every boundary, each boundary by
//! reference, then lists each boundary's group, its kept recording shared, not copied (D270),
//! drawing a boundary again first only if something it drew from changed (tree.rs routes the
//! changes: `invalidate_draw_of`).

use std::mem;
use std::sync::Arc;

use smallvec::SmallVec;

use crate::display_list::{Boundary, DisplayList, DrawGroup, DrawOp};
use crate::draw::DrawScope;
use crate::graphics_layer::{GraphicsLayerNode, LayerProperties};
use crate::tree::{NodeId, Observation, UiTree, capability, observation_key};
use crate::unit::Offset;

/// The most repaint boundaries composed and not placed whose groups the frames keep listing
/// ([`DisplayList::kept`]), the oldest dropped first: a lazy list's items kept on each side of
/// the wanted ones (two behind the scroll, D375, and the two prefetched ahead once placed, after
/// a reversal), for two lists.
pub const KEPT_CAPACITY: usize = 8;

/// What a repaint boundary drew last, kept until something it drew from changes.
pub(crate) struct RetainedDrawing {
    /// Its operations, in its own coordinates (after its position): its chain and children,
    /// the boundaries inside it by reference (`DisplayList::boundaries`). Shared with the
    /// frames drawing it (`DrawGroup::content`).
    recording: Arc<DisplayList>,
    /// Its group's version: from the tree's counter, given anew whenever its operations, or
    /// a nested boundary's, are drawn again.
    version: u64,
    /// The chain index of the graphics layer whose properties are its group's, outside its
    /// content: the first drawing modifier of its node, if it is one.
    pub(crate) layer_entry: Option<u16>,
    /// That layer's properties, from its block's last run.
    layer: Option<LayerProperties>,
    /// Its operations must be drawn again.
    pub(crate) dirty: bool,
    /// Its layer's properties must be computed again (its block read state that changed).
    pub(crate) layer_dirty: bool,
    /// It's in the tree's `path_drawings`: its recording drew a `Path` (D531).
    tracks_paths: bool,
}

impl RetainedDrawing {
    pub(crate) fn new(layer_entry: Option<u16>) -> RetainedDrawing {
        RetainedDrawing {
            recording: Arc::default(),
            version: 0,
            layer_entry,
            layer: None,
            dirty: true,
            layer_dirty: true,
            tracks_paths: false,
        }
    }
}

impl UiTree {
    /// Records the display list, if anything changed since the last draw. Returns whether it
    /// did.
    ///
    /// State written since the frame last applied observed writes (by layout itself: a lazy
    /// list's or a scroll's measured state, a hover the layout moved) is applied first, so a
    /// kept boundary drawing it is drawn again in this frame. A measurement it invalidates is
    /// left to the next frame, which the write asked for (as before boundaries kept anything).
    pub fn draw(&mut self) -> bool {
        self.apply_observed_writes();
        // Boundaries measured ahead that a platform doesn't draw ahead (or not yet) are
        // forgotten once this frame places them, or they go.
        if !self.measured_ahead.is_empty() {
            self.has_draw_ahead_work();
        }
        // A path changed since it was drawn (by composition, a handler) draws again what drew it.
        self.invalidate_changed_paths();
        if !self.needs_draw {
            return false;
        }
        let observer = self.observer.clone();
        let mut recording = mem::take(&mut self.display_list);
        // The boundaries the last frame drew that this one won't, still composed: kept drawn.
        if self.keeps_unplaced {
            self.note_kept(&recording.groups);
        }
        // Cleared first: the last frame's groups let go of the recordings they shared, which a
        // boundary drawn again records into in place.
        recording.clear();
        self.draw_node(self.root_id(), &mut recording, observer.as_deref());
        // The overlays, over everything, each where its provider put it.
        for index in 0..self.overlays.len() {
            let node = self.overlays[index];
            let Some(value) = self.get(node).filter(|value| value.overlay && value.placed) else {
                continue;
            };
            let placed = value.layers[0].offset;
            let at = value.overlay_origin - Offset::new(placed.x as f32, placed.y as f32);
            translate(&mut recording, at);
            self.draw_node(node, &mut recording, observer.as_deref());
            translate(&mut recording, negate(at));
        }
        recording.finish_paths();
        let mut groups = mem::take(&mut recording.groups);
        self.draw_boundaries(&recording.boundaries, &mut groups, observer.as_deref());
        recording.groups = groups;
        if self.keeps_unplaced {
            let mut kept = mem::take(&mut recording.kept);
            self.draw_kept(&mut kept);
            recording.kept = kept;
        }
        self.display_list = recording;
        self.needs_draw = false;
        // A path drawn here, then changed by a drawing after it (another boundary's block
        // resetting it): the next frame draws again what drew it as it was.
        self.invalidate_changed_paths();
        true
    }

    /// Draws again what drew a `Path` that changed since (D531): the boundaries whose recordings
    /// drew one, and what's drawn outside every boundary. So a recording, and the bytes a
    /// platform keeps for its group's key and version (D157), never hold a path older than the
    /// path is. A drawing changing its own paths isn't drawn again for that: what it noted is
    /// their generations once it was done.
    fn invalidate_changed_paths(&mut self) {
        if self.display_list.paths_changed() {
            self.needs_draw = true;
        }
        if self.path_drawings.is_empty() {
            return;
        }
        let mut nodes = mem::take(&mut self.path_drawings);
        let mut changed: SmallVec<[NodeId; 4]> = SmallVec::new();
        nodes.retain(|node| {
            let Some(retained) = self.get_mut(*node).and_then(|value| value.retained.as_mut())
            else {
                return false;
            };
            if retained.recording.paths.is_empty() {
                retained.tracks_paths = false;
                return false;
            }
            if !retained.dirty && retained.recording.paths_changed() {
                changed.push(*node);
            }
            true
        });
        self.path_drawings = nodes;
        for node in changed {
            self.invalidate_draw_of(node);
        }
    }

    /// Notes the repaint boundaries among `groups` (the last frame's) that are composed and
    /// not placed now, unchanged since drawn: the frames list their groups while they stay so
    /// ([`DisplayList::kept`]).
    fn note_kept(&mut self, groups: &[DrawGroup]) {
        for group in groups {
            let node = NodeId::from_bits(group.key);
            if !group.empty && self.stays_kept(node) && !self.kept.contains(&node) {
                self.kept.push(node);
            }
        }
    }

    /// `node` is a repaint boundary in the tree, not placed, its content and layer unchanged
    /// since it was last drawn, and those of the boundaries nested in it: what it drew is what
    /// the frame placing it again draws. One that changed isn't drawn while not placed (a
    /// spinner in an item scrolled away runs nothing, as in Compose): the frame placing it
    /// draws it.
    fn stays_kept(&self, node: NodeId) -> bool {
        self.get(node).is_some_and(|value| {
            !value.placed && !value.overlay && value.parent.is_some() && self.drawn_unchanged(node)
        })
    }

    /// `node` is a repaint boundary whose content and layer are as last drawn, and those of the
    /// boundaries nested in it.
    fn drawn_unchanged(&self, node: NodeId) -> bool {
        let Some(retained) = self.get(node).and_then(|value| value.retained.as_ref()) else {
            return false;
        };
        !retained.dirty
            && !retained.layer_dirty
            && retained
                .recording
                .boundaries
                .iter()
                .all(|nested| self.drawn_unchanged(NodeId::from_bits(nested.key)))
    }

    /// Appends to `kept` the groups of the boundaries kept drawn ([`note_kept`]) that still
    /// are, the newest `KEPT_CAPACITY`, each followed by its nested ones, shared: nothing in
    /// them changed, so nothing is drawn.
    ///
    /// [`note_kept`]: UiTree::note_kept
    fn draw_kept(&mut self, kept: &mut Vec<DrawGroup>) {
        let mut nodes = mem::take(&mut self.kept);
        nodes.retain(|node| self.stays_kept(*node));
        if nodes.len() > KEPT_CAPACITY {
            nodes.drain(..nodes.len() - KEPT_CAPACITY);
        }
        for node in &nodes {
            self.draw_boundary(*node, kept, None);
        }
        self.kept = nodes;
    }

    /// Whether frames list the kept boundaries' groups (`UiHost::set_keep_unplaced_groups`).
    pub fn set_keep_unplaced_groups(&mut self, keep: bool) {
        self.keeps_unplaced = keep;
        if !keep {
            self.kept.clear();
        }
    }

    /// How many boundaries are kept drawn: for tests.
    #[doc(hidden)]
    pub fn kept_count(&self) -> usize {
        self.kept.len()
    }

    /// Whether a repaint boundary measured ahead (`premeasure`) waits to be drawn ahead
    /// ([`draw_ahead`](UiTree::draw_ahead)): those that no longer do (gone, placed, drawn) are
    /// forgotten.
    pub fn has_draw_ahead_work(&mut self) -> bool {
        let mut measured = mem::take(&mut self.measured_ahead);
        measured.retain(|node| self.waits_to_draw_ahead(*node));
        self.measured_ahead = measured;
        !self.measured_ahead.is_empty()
    }

    /// Forgets the boundaries measured ahead, which the frames placing them then draw: a
    /// platform gives up drawing them ahead when an idle time with nothing else to do is too
    /// short for one.
    pub fn cancel_draw_ahead(&mut self) {
        self.measured_ahead.clear();
    }

    /// How many nodes measured ahead are noted, forgetting none: for tests.
    #[doc(hidden)]
    pub fn measured_ahead_count(&self) -> usize {
        self.measured_ahead.len()
    }

    /// `node` is a repaint boundary in the tree, not placed, whose content or layer changed
    /// since it was last drawn.
    fn waits_to_draw_ahead(&self, node: NodeId) -> bool {
        self.get(node).is_some_and(|value| {
            !value.placed
                && value.parent.is_some()
                && value
                    .retained
                    .as_ref()
                    .is_some_and(|retained| retained.dirty || retained.layer_dirty)
        })
    }

    /// Draws ahead the oldest repaint boundary measured ahead of the layout placing it
    /// (`premeasure`: a lazy list's prefetched item) and not drawn since: its group, then the
    /// groups nested in it, as the frame placing it would draw them. Its content is in its own
    /// coordinates and placing it doesn't draw it again (moving a boundary marks its parent:
    /// `invalidate_position`), so that frame shares what it drew, with its version. `None`
    /// without such a boundary.
    ///
    /// Called between frames, after `prefetch`, by a platform that records groups (Android's
    /// render nodes): the frame revealing an item then neither draws nor records it.
    pub fn draw_ahead(&mut self) -> Option<Vec<DrawGroup>> {
        let node = loop {
            if self.measured_ahead.is_empty() {
                return None;
            }
            let node = self.measured_ahead.remove(0);
            if self.waits_to_draw_ahead(node) {
                break node;
            }
        };
        let observer = self.observer.clone();
        let mut groups = Vec::new();
        self.draw_boundary(node, &mut groups, observer.as_deref());
        Some(groups)
    }

    /// Records `node`; a custom node draws under observation (`observer`), so a state write it
    /// read draws again (D71). A repaint boundary is recorded by reference: `draw_boundary`
    /// draws it.
    fn draw_node(
        &mut self,
        node: NodeId,
        list: &mut DisplayList,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) {
        let value = self.node(node);
        if !value.placed {
            return;
        }
        if value.retained.is_some() {
            let position = value.layers[0].offset;
            let position = Offset::new(position.x as f32, position.y as f32);
            translate(list, position);
            list.boundaries.push(Boundary { at: list.ops.len(), key: node.to_bits() });
            // Its group's operations stay its own.
            list.merge_floor = list.ops.len();
            translate(list, negate(position));
            return;
        }
        // One observation for the node's drawing, before and after its children.
        let custom = value.all_capabilities & capability::CUSTOM != 0;
        match observer.filter(|_| custom) {
            Some(observer) => observer.observe(observation_key(node, Observation::Draw), || {
                self.draw_node_observed(node, list, Some(observer))
            }),
            None => self.draw_node_observed(node, list, observer),
        }
    }

    fn draw_node_observed(
        &mut self,
        node: NodeId,
        list: &mut DisplayList,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) {
        let position = self.node(node).layers[0].offset;
        let position = Offset::new(position.x as f32, position.y as f32);
        translate(list, position);
        self.draw_content(node, list, observer);
        translate(list, negate(position));
    }

    /// Draws `node`'s chain and children, in its own coordinates.
    fn draw_content(
        &mut self,
        node: NodeId,
        list: &mut DisplayList,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) {
        let origins = self.layer_origins(node);
        self.draw_chain(node, 0, 0, &origins, list, observer);
    }

    /// Each layer's origin in `node`.
    fn layer_origins(&self, node: NodeId) -> SmallVec<[Offset; 4]> {
        let mut origins: SmallVec<[Offset; 4]> = SmallVec::new();
        let mut origin = Offset::ZERO;
        for (index, layer) in self.node(node).layers.iter().enumerate() {
            if index > 0 {
                origin = origin + Offset::new(layer.offset.x as f32, layer.offset.y as f32);
            }
            origins.push(origin);
        }
        origins
    }

    /// Appends to `groups` the group of each boundary in `boundaries` (a recording's), in
    /// order, each followed by the ones nested in it. Returns whether one's version changed,
    /// and whether one draws anything.
    fn draw_boundaries(
        &mut self,
        boundaries: &[Boundary],
        groups: &mut Vec<DrawGroup>,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) -> (bool, bool) {
        let (mut changed, mut draws) = (false, false);
        for boundary in boundaries {
            let (version_changed, drawn) =
                self.draw_boundary(NodeId::from_bits(boundary.key), groups, observer);
            changed |= version_changed;
            draws |= drawn;
        }
        (changed, draws)
    }

    /// Appends repaint boundary `node`'s group to `groups`, then the groups nested in it: its
    /// content drawn again first if something it drew from changed, else shared from what it
    /// kept (not copied), without running its draw code. Returns whether its version changed,
    /// and whether it draws anything.
    fn draw_boundary(
        &mut self,
        node: NodeId,
        groups: &mut Vec<DrawGroup>,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) -> (bool, bool) {
        let Some(retained) = self.node_mut(node).retained.as_mut() else {
            // No longer a boundary: an empty group keeps the frame's groups in step with the
            // boundaries naming them.
            groups.push(DrawGroup {
                key: node.to_bits(),
                version: 0,
                layer: None,
                content: Arc::default(),
                descendants: 0,
                empty: true,
            });
            return (false, false);
        };
        // Its flags are cleared once it's drawn: a panic drawing it leaves it to draw again.
        let (dirty, layer_dirty) = (retained.dirty, retained.layer_dirty);
        if dirty {
            // Recorded in place: the frames that shared it are gone (`draw` clears the last).
            let mut recording =
                Arc::get_mut(&mut retained.recording).map(mem::take).unwrap_or_default();
            recording.clear();
            let custom = self.node(node).all_capabilities & capability::CUSTOM != 0;
            match observer.filter(|_| custom) {
                Some(observer) => observer
                    .observe(observation_key(node, Observation::Draw), || {
                        self.draw_content(node, &mut recording, Some(observer))
                    }),
                None => self.draw_content(node, &mut recording, observer),
            }
            recording.finish_paths();
            let draws_paths = !recording.paths.is_empty();
            let retained = self.node_mut(node).retained.as_mut().expect("a repaint boundary");
            if draws_paths && !retained.tracks_paths {
                retained.tracks_paths = true;
                // Listed once, though its node became a boundary again since.
                if !self.path_drawings.contains(&node) {
                    self.path_drawings.push(node);
                }
            }
            let retained = self.node_mut(node).retained.as_mut().expect("a repaint boundary");
            match Arc::get_mut(&mut retained.recording) {
                Some(kept) => *kept = recording,
                // A copy of an earlier frame still holds it.
                None => retained.recording = Arc::new(recording),
            }
        }
        // Its size (the pivot, the clip) may have changed with its content.
        if dirty || layer_dirty {
            let layer = self.layer_properties(node, observer);
            if let Some(retained) = self.node_mut(node).retained.as_mut() {
                retained.layer = layer;
            }
        }
        let retained = self.node_mut(node).retained.as_mut().expect("a repaint boundary");
        retained.dirty = false;
        retained.layer_dirty = false;
        let content = Arc::clone(&retained.recording);
        let index = groups.len();
        groups.push(DrawGroup {
            key: node.to_bits(),
            version: retained.version,
            layer: retained.layer,
            content: Arc::clone(&content),
            descendants: 0,
            empty: true,
        });
        let (nested, nested_draws) = self.draw_boundaries(&content.boundaries, groups, observer);
        let changed = dirty || nested;
        if changed {
            self.draw_version += 1;
            let version = self.draw_version;
            let retained = self.node_mut(node).retained.as_mut().expect("a repaint boundary");
            retained.version = version;
            groups[index].version = version;
        }
        let draws = !content.ops.is_empty() || nested_draws;
        let descendants = groups.len() - index - 1;
        let group = &mut groups[index];
        group.descendants = descendants;
        group.empty = !draws;
        (changed, draws)
    }

    /// The properties of `node`'s graphics layer whose properties are its group's, its block
    /// run under its own observation: a write to what it read changes only them.
    fn layer_properties(
        &mut self,
        node: NodeId,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) -> Option<LayerProperties> {
        let value = self.node(node);
        let index = value.retained.as_ref()?.layer_entry? as usize;
        let layer = value.capabilities[..index]
            .iter()
            .filter(|bits| **bits & capability::LAYOUT != 0)
            .count();
        let origin = self.layer_origins(node)[layer];
        let size = value.layers[layer].measured.into();
        let graphics = value.chain[index].downcast_ref::<GraphicsLayerNode>()?;
        let scope = match observer {
            Some(observer) => {
                observer.observe(observation_key(node, Observation::Layer), || graphics.scope())
            }
            None => graphics.scope(),
        };
        Some(LayerProperties::new(&scope, origin, size))
    }
}

impl UiTree {
    /// Draws `node`'s chain from entry `from` (in layer `layer`), then its children, then the
    /// `draw_after`s of the entries drawn here. An entry drawing its content
    /// (`draw_with_content`) draws the rest of the chain and the children where it asks. A
    /// boundary's graphics layer whose properties are its group's isn't drawn here.
    fn draw_chain(
        &mut self,
        node: NodeId,
        from: usize,
        mut layer: usize,
        origins: &[Offset],
        list: &mut DisplayList,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) {
        let density = self.density;
        let mut after: SmallVec<[(u16, u16); 4]> = SmallVec::new();
        let length = self.node(node).chain.len();
        if self.node(node).all_capabilities & capability::DRAW != 0 {
            let lifted =
                self.node(node).retained.as_ref().and_then(|retained| retained.layer_entry);
            for index in from..length {
                let value = self.node_mut(node);
                let bits = value.capabilities[index];
                if bits & capability::DRAW != 0 && lifted != Some(index as u16) {
                    let draws_content =
                        value.chain[index].as_draw().is_some_and(|draw| draw.draws_content());
                    let size = value.layers[layer].measured.into();
                    let layout_direction = value.layout_direction;
                    if draws_content {
                        // Out of the chain while it draws the rest of its node.
                        let mut entry =
                            mem::replace(&mut value.chain[index], Box::new(crate::nodes::Taken));
                        let next_layer = layer + usize::from(bits & capability::LAYOUT != 0);
                        let mut scope = DrawScope {
                            list,
                            origin: origins[layer],
                            size,
                            density,
                            layout_direction,
                        };
                        let mut content = |list: &mut DisplayList| {
                            self.draw_chain(node, index + 1, next_layer, origins, list, observer)
                        };
                        let mut content_scope = crate::draw_modifiers::ContentDrawScope {
                            scope: &mut scope,
                            content: &mut content,
                        };
                        if let Some(draw) = entry.as_draw() {
                            draw.draw_with_content(&mut content_scope);
                        }
                        self.node_mut(node).chain[index] = entry;
                        self.draw_afters(node, &after, origins, list);
                        return;
                    }
                    let mut scope =
                        DrawScope { list, origin: origins[layer], size, density, layout_direction };
                    if let Some(draw) = value.chain[index].as_draw() {
                        draw.draw(&mut scope);
                    }
                    after.push((index as u16, layer as u16));
                }
                if bits & capability::LAYOUT != 0 {
                    layer += 1;
                }
            }
        }
        let content = *origins.last().expect("a layer");
        if !self.node(node).children.is_empty() {
            translate(list, content);
            let order = self.placement_order(node);
            let count = order.as_ref().map_or(self.node(node).children.len(), |order| order.len());
            for position in 0..count {
                let index = order.as_ref().map_or(position, |order| order[position] as usize);
                let child = self.node(node).children[index].node;
                // Overlays are drawn last, over everything.
                if !self.node(child).overlay {
                    self.draw_node(child, list, observer);
                }
            }
            translate(list, negate(content));
        }
        self.draw_afters(node, &after, origins, list);
    }

    /// The `draw_after`s of `after`'s entries, innermost first.
    fn draw_afters(
        &mut self,
        node: NodeId,
        after: &[(u16, u16)],
        origins: &[Offset],
        list: &mut DisplayList,
    ) {
        let density = self.density;
        let value = self.node_mut(node);
        for &(index, layer) in after.iter().rev() {
            let mut scope = DrawScope {
                list,
                origin: origins[layer as usize],
                size: value.layers[layer as usize].measured.into(),
                density,
                layout_direction: value.layout_direction,
            };
            if let Some(draw) = value.chain[index as usize].as_draw() {
                draw.draw_after(&mut scope);
            }
        }
    }
}

/// `-offset`, without negative zeros.
fn negate(offset: Offset) -> Offset {
    Offset::new(0.0 - offset.x, 0.0 - offset.y)
}

/// Moves what follows by `by`: merged into a translation just before (a node that drew
/// nothing between its move in and out leaves none).
fn translate(list: &mut DisplayList, by: Offset) {
    if by == Offset::ZERO {
        return;
    }
    if list.ops.len() > list.merge_floor
        && let Some(DrawOp::Translate(last)) = list.ops.last_mut()
    {
        *last = *last + by;
        if *last == Offset::ZERO {
            list.ops.pop();
        }
        return;
    }
    list.push(DrawOp::Translate(by));
}
