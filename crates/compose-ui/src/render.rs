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

use crate::delegation::dispatch_for_kind;
use crate::display_list::{Boundary, DisplayList, DrawGroup, DrawOp};
use crate::draw::DrawScope;
use crate::graphics_layer::{GraphicsLayerNode, LayerProperties};
use crate::node_chain::{EntryId, NodeKinds, entry_state};
use crate::tree::{NodeId, Observation, UiTree, observation_key};
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
    /// Those properties, computed ahead of the drawing to know what they move
    /// (`note_layer_change`), with what they were computed for: its block runs once a frame.
    pub(crate) layer_ahead:
        Option<(LayerProperties, crate::unit::Density, crate::alignment::LayoutDirection)>,
    /// Its group drew something when last listed (not `empty`).
    draws: bool,
}

/// The flag of an inner layer's group key: the others are their nodes' (`NodeId::to_bits`,
/// whose generation never reaches it).
pub(crate) const INNER_LAYER_KEY: u64 = 1 << 63;

/// A graphics layer that isn't its node's first drawing modifier: a group of its own in its
/// node's drawing (a render node on Android), holding what follows it in the chain and the
/// children, as each of Compose's graphics layers is a layer (D522). Drawn with what draws its
/// node (the node's boundary, or the nearest one around it), each time that draws it.
pub(crate) struct InnerLayer {
    /// Its chain index.
    pub(crate) entry: u16,
    /// Its group's key (`INNER_LAYER_KEY` set).
    pub(crate) key: u64,
    /// What follows it, in its node's coordinates, the boundaries in it by reference.
    recording: Arc<DisplayList>,
    version: u64,
    layer: Option<LayerProperties>,
    /// Drawn since it was last listed: its version changes when it is.
    pub(crate) drawn: bool,
    /// Its content must be drawn again (`UiTree::invalidate_inner_layers`): otherwise what
    /// draws its node draws it by reference, as it is.
    pub(crate) dirty: bool,
    /// Its group drew something when last listed (not `empty`).
    draws: bool,
}

impl InnerLayer {
    pub(crate) fn new(entry: u16, key: u64) -> InnerLayer {
        InnerLayer {
            entry,
            key,
            recording: Arc::default(),
            version: 0,
            layer: None,
            drawn: false,
            dirty: true,
            draws: false,
        }
    }
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
            layer_ahead: None,
            draws: false,
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
        if !self.needs_draw {
            return false;
        }
        self.held_paragraphs.sweep(&self.text_measurer);
        // What the last frame's drawing let go of: its frame names none of them (D1116).
        self.text_measurer.release_dropped();
        let observer = self.observer.clone();
        let mut recording = mem::take(&mut self.display_list);
        // The boundaries the last frame drew that this one won't, still composed: kept drawn
        // (with `KEEP_ITEMS_BEHIND`: Compose keeps no recording of an item no frame places).
        if self.keeps_unplaced_groups() {
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
        let mut groups = mem::take(&mut recording.groups);
        self.draw_boundaries(&recording.boundaries, &mut groups, observer.as_deref());
        recording.groups = groups;
        if self.keeps_unplaced_groups() {
            let mut kept = mem::take(&mut recording.kept);
            self.draw_kept(&mut kept);
            recording.kept = kept;
        }
        self.display_list = recording;
        self.needs_draw = false;
        true
    }

    /// Whether something drawn changed since the last draw (the next [`draw`](Self::draw)
    /// records): for tests comparing a frame's drawing with drawing everything again.
    pub fn needs_draw(&self) -> bool {
        self.needs_draw
    }

    /// The window `node` shows in: the overlay (a popup's or a dialog's) it's in, or `None`
    /// for the main content. For tests capturing a window's pixels, as Compose's popups and
    /// dialogs are windows of their own.
    pub fn window_of(&self, node: NodeId) -> Option<NodeId> {
        let mut current = Some(node);
        while let Some(id) = current {
            let value = self.get(id)?;
            if value.overlay {
                return Some(id);
            }
            current = value.parent;
        }
        None
    }

    /// A display list of one window as the last frame drew it: the main content (`None`) or
    /// one overlay, where its provider put it. Recorded afresh from the boundaries the frame
    /// kept (drawn again only if changed since), observing nothing: for tests capturing a
    /// window's pixels without the other windows over it ([`draw`](Self::draw) records them
    /// all into one list).
    pub fn window_display_list(&mut self, window: Option<NodeId>) -> DisplayList {
        let mut recording = DisplayList::default();
        match window {
            None => self.draw_node(self.root_id(), &mut recording, None),
            Some(node) => {
                if let Some(value) = self.get(node).filter(|value| value.overlay && value.placed) {
                    let placed = value.layers[0].offset;
                    let at = value.overlay_origin - Offset::new(placed.x as f32, placed.y as f32);
                    translate(&mut recording, at);
                    self.draw_node(node, &mut recording, None);
                    translate(&mut recording, negate(at));
                }
            }
        }
        let mut groups = mem::take(&mut recording.groups);
        self.draw_boundaries(&recording.boundaries, &mut groups, None);
        recording.groups = groups;
        recording
    }

    /// Notes the repaint boundaries among `groups` (the last frame's) that are composed and
    /// not placed now, unchanged since drawn: the frames list their groups while they stay so
    /// ([`DisplayList::kept`]).
    fn note_kept(&mut self, groups: &[DrawGroup]) {
        for group in groups {
            if group.key & INNER_LAYER_KEY != 0 {
                // Kept with its node's group.
                continue;
            }
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
            && retained.recording.boundaries.iter().all(|nested| self.group_unchanged(nested.key))
    }

    /// The group of `key` (a boundary's, or an inner layer's) is as last drawn, and those
    /// nested in it. An inner layer is drawn with what draws its node: unchanged while that is.
    fn group_unchanged(&self, key: u64) -> bool {
        if key & INNER_LAYER_KEY == 0 {
            return self.drawn_unchanged(NodeId::from_bits(key));
        }
        self.inner_layer(key).is_some_and(|inner| {
            !inner.dirty
                && inner.recording.boundaries.iter().all(|nested| self.group_unchanged(nested.key))
        })
    }

    /// The inner layer of `key`, if its node has it.
    fn inner_layer(&self, key: u64) -> Option<&InnerLayer> {
        let node = *self.inner_layer_owners.get(&key)?;
        self.get(node)?.inner_layers.iter().find(|inner| inner.key == key)
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

    /// Whether frames list the kept boundaries' groups (`UiHost::set_keep_unplaced_groups`):
    /// only with `Optimization::KEEP_ITEMS_BEHIND` (D380, D563).
    pub fn set_keep_unplaced_groups(&mut self, keep: bool) {
        self.keeps_unplaced = keep;
        if !self.keeps_unplaced_groups() {
            self.kept.clear();
        }
    }

    /// Frames list the kept boundaries' groups: the platform asks, and the host keeps items
    /// behind (`KEEP_ITEMS_BEHIND`).
    pub(crate) fn keeps_unplaced_groups(&self) -> bool {
        self.keeps_unplaced
            && self.optimizations.contains(crate::optimizations::Optimization::KEEP_ITEMS_BEHIND)
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
        let custom = value.kinds.any(NodeKinds::OBSERVED);
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

    /// Each layer's origin in `node`, on the stack for the chains components have (a checkbox's
    /// node has 5 layers): every node drawn asks for them.
    fn layer_origins(&self, node: NodeId) -> SmallVec<[Offset; 8]> {
        let mut origins: SmallVec<[Offset; 8]> = SmallVec::new();
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
            let (version_changed, drawn) = if boundary.key & INNER_LAYER_KEY != 0 {
                self.list_inner_layer(boundary.key, groups, observer)
            } else {
                self.draw_boundary(NodeId::from_bits(boundary.key), groups, observer)
            };
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
            let custom = self.node(node).kinds.any(NodeKinds::OBSERVED);
            match observer.filter(|_| custom) {
                Some(observer) => observer
                    .observe(observation_key(node, Observation::Draw), || {
                        self.draw_content(node, &mut recording, Some(observer))
                    }),
                None => self.draw_content(node, &mut recording, observer),
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
            let layer = self.layer_ahead(node).or_else(|| self.layer_properties(node, observer));
            if let Some(properties) = &layer {
                self.note_layer_transparency(node, properties);
            }
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
            layer: retained.layer.clone(),
            content: Arc::clone(&content),
            descendants: 0,
            empty: true,
        });
        let (nested, nested_draws) = self.draw_boundaries(&content.boundaries, groups, observer);
        // A shadow is a drawing too, under no content (as a Compose layer's).
        let shadow = groups[index].layer.as_ref().is_some_and(|layer| layer.casts_shadow());
        let draws = !content.ops.is_empty() || nested_draws || shadow;
        let retained = self.node_mut(node).retained.as_mut().expect("a repaint boundary");
        // Drawing something or nothing changes what draws it (a reference or none): a new
        // version, though only its layer changed (a shadow its block gave or took, D522).
        let changed = dirty || nested || retained.draws != draws;
        retained.draws = draws;
        if changed {
            self.draw_version += 1;
            let version = self.draw_version;
            let retained = self.node_mut(node).retained.as_mut().expect("a repaint boundary");
            retained.version = version;
            groups[index].version = version;
        }
        let descendants = groups.len() - index - 1;
        let group = &mut groups[index];
        group.descendants = descendants;
        group.empty = !draws;
        if self.node(node).kinds.any(NodeKinds::DRAW_CAPTURE) {
            self.update_capture(node, &groups[index..], changed);
        }
        (changed, draws)
    }

    /// What `node` drew, with the groups nested in it, goes to its `Modifier.capture_content`'s. A
    /// layer changing alone gives no group a new version, so its properties are taken each frame.
    fn update_capture(&mut self, node: NodeId, groups: &[DrawGroup], changed: bool) {
        let value = self.node(node);
        let capture = value.chain.iter().find_map(|entry| entry.node.draw_capture()).cloned();
        if let Some(capture) = capture {
            if changed {
                self.held_paragraphs.store(
                    &capture,
                    groups.to_vec(),
                    &mut *self.text_measurer.borrow_mut(),
                );
            } else {
                capture.refresh_layers(groups);
            }
        }
    }

    /// Appends inner layer `key`'s group to `groups`, then the groups nested in it: its content
    /// as its node last drew it (with what draws the node), its version new if it was drawn
    /// since it was last listed or a nested group's changed. Returns whether its version
    /// changed, and whether it draws anything.
    fn list_inner_layer(
        &mut self,
        key: u64,
        groups: &mut Vec<DrawGroup>,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) -> (bool, bool) {
        let owner = self.inner_layer_owners.get(&key).copied();
        // Drawn again where it is, if what it drew from changed (its node's drawing kept).
        if let Some(node) = owner
            && let Some(entry) = self
                .get(node)
                .and_then(|value| {
                    value.inner_layers.iter().find(|inner| inner.key == key && inner.dirty)
                })
                .map(|inner| inner.entry as usize)
        {
            let origins = self.layer_origins(node);
            let value = self.node(node);
            let layer = value.layer_of(entry);
            let mut unused = DisplayList::default();
            self.draw_inner_layer(node, entry, layer, &origins, &mut unused, observer);
        }
        let inner = owner.and_then(|node| {
            let value = self.get_mut(node)?;
            value.inner_layers.iter_mut().find(|inner| inner.key == key)
        });
        let Some(inner) = inner else {
            // Gone: an empty group keeps the frame's groups in step with the boundaries.
            groups.push(DrawGroup {
                key,
                version: 0,
                layer: None,
                content: Arc::default(),
                descendants: 0,
                empty: true,
            });
            return (false, false);
        };
        let drawn = mem::take(&mut inner.drawn);
        let content = Arc::clone(&inner.recording);
        let index = groups.len();
        groups.push(DrawGroup {
            key,
            version: inner.version,
            layer: inner.layer.clone(),
            content: Arc::clone(&content),
            descendants: 0,
            empty: true,
        });
        let (nested, nested_draws) = self.draw_boundaries(&content.boundaries, groups, observer);
        // A shadow is a drawing too, under no content (as a Compose layer's).
        let shadow = groups[index].layer.as_ref().is_some_and(|layer| layer.casts_shadow());
        let draws = !content.ops.is_empty() || nested_draws || shadow;
        let inner = owner.and_then(|node| {
            self.get_mut(node)?.inner_layers.iter_mut().find(|inner| inner.key == key)
        });
        // Drawing something or nothing changes what draws it: a new version (as a boundary's).
        let draws_changed = inner.as_ref().is_some_and(|inner| inner.draws != draws);
        if let Some(inner) = inner {
            inner.draws = draws;
        }
        let changed = drawn || nested || draws_changed;
        if changed {
            self.draw_version += 1;
            let version = self.draw_version;
            groups[index].version = version;
            if let Some(inner) = owner.and_then(|node| {
                self.get_mut(node)?.inner_layers.iter_mut().find(|inner| inner.key == key)
            }) {
                inner.version = version;
            }
        }
        let descendants = groups.len() - index - 1;
        let group = &mut groups[index];
        group.descendants = descendants;
        group.empty = !draws;
        (changed, draws)
    }

    /// Draws inner layer `entry` of `node` (in layer `layer` of the node): what follows it in
    /// the chain and the children, into its own recording, which `list` draws by reference
    /// where it is. Returns whether `entry` is one.
    #[allow(clippy::too_many_arguments)]
    fn draw_inner_layer(
        &mut self,
        node: NodeId,
        entry: usize,
        layer: usize,
        origins: &[Offset],
        list: &mut DisplayList,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) -> bool {
        let value = self.node_mut(node);
        let density = value.density;
        let Some(slot) = value.inner_layers.iter().position(|inner| inner.entry as usize == entry)
        else {
            return false;
        };
        if value.chain[entry].node.downcast_ref::<GraphicsLayerNode>().is_none() {
            return false;
        }
        let key = value.inner_layers[slot].key;
        if value.inner_layers[slot].dirty {
            // Its block and what it holds run under an observation of their own: a change to
            // what they read draws them again, not what's around them.
            let record = |tree: &mut UiTree| {
                let value = tree.node_mut(node);
                let size = value.layers[layer].measured.into();
                let direction = value.layout_direction;
                let graphics =
                    value.chain[entry].node.downcast_ref::<GraphicsLayerNode>().expect("a layer");
                let properties = graphics.properties(density, direction, origins[layer], size);
                let transparency_changed = graphics.note_transparency(&properties);
                let inner = &mut value.inner_layers[slot];
                // Recorded in place: the frames that shared it are gone (`draw` clears the
                // last).
                let mut recording =
                    Arc::get_mut(&mut inner.recording).map(mem::take).unwrap_or_default();
                recording.clear();
                tree.semantics_changed |= transparency_changed;
                tree.draw_chain(node, entry + 1, layer, origins, &mut recording, observer);
                let inner = &mut tree.node_mut(node).inner_layers[slot];
                match Arc::get_mut(&mut inner.recording) {
                    Some(kept) => *kept = recording,
                    None => inner.recording = Arc::new(recording),
                }
                inner.layer = Some(properties);
                inner.drawn = true;
                inner.dirty = false;
            };
            match observer {
                Some(observer) => observer
                    .observe(observation_key(node, Observation::InnerLayers), || record(self)),
                None => record(self),
            }
        }
        list.boundaries.push(Boundary { at: list.ops.len(), key });
        // Its group's operations stay its own.
        list.merge_floor = list.ops.len();
        true
    }

    /// `node`'s group layer drawn with `properties`: a service hears of its alpha coming to or
    /// leaving none, which hides what's under it or shows it (D1293).
    fn note_layer_transparency(&mut self, node: NodeId, properties: &LayerProperties) {
        let value = self.node(node);
        let Some(index) = value.retained.as_ref().and_then(|retained| retained.layer_entry) else {
            return;
        };
        let graphics = value.chain[index as usize].node.downcast_ref::<GraphicsLayerNode>();
        if graphics.is_some_and(|graphics| graphics.note_transparency(properties)) {
            self.semantics_changed = true;
        }
    }

    /// The properties computed ahead for `node`'s layer, if they're for what it is now.
    fn layer_ahead(&mut self, node: NodeId) -> Option<LayerProperties> {
        let retained = self.node_mut(node).retained.as_mut()?;
        retained.layer_ahead.as_ref()?;
        let (properties, density, direction) = retained.layer_ahead.take()?;
        let value = self.node(node);
        let index = value.retained.as_ref()?.layer_entry? as usize;
        let layer = value.layer_of(index);
        let bounds = crate::unit::Rect::from_offset_size(
            self.layer_origins(node)[layer],
            value.layers[layer].measured.into(),
        );
        (properties.bounds == bounds
            && density == value.density
            && direction == value.layout_direction)
            .then_some(properties)
    }

    /// The properties of `node`'s graphics layer whose properties are its group's, its block
    /// run under its own observation: a write to what it read changes only them.
    pub(crate) fn layer_properties(
        &mut self,
        node: NodeId,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) -> Option<LayerProperties> {
        let value = self.node(node);
        let index = value.retained.as_ref()?.layer_entry? as usize;
        let layer = value.layer_of(index);
        let origin = self.layer_origins(node)[layer];
        let size = value.layers[layer].measured.into();
        let graphics = value.chain[index].node.downcast_ref::<GraphicsLayerNode>()?;
        let (density, direction) = (value.density, value.layout_direction);
        let properties = || graphics.properties(density, direction, origin, size);
        Some(match observer {
            Some(observer) => {
                observer.observe(observation_key(node, Observation::Layer), properties)
            }
            None => properties(),
        })
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
        let density = self.node(node).density;
        let locals = self.node(node).locals;
        let mut after: SmallVec<[(u16, u16); 4]> = SmallVec::new();
        let length = self.node(node).chain.len();
        if self.node(node).kinds.any(NodeKinds::DRAW) {
            let lifted =
                self.node(node).retained.as_ref().and_then(|retained| retained.layer_entry);
            for index in from..length {
                let value = self.node_mut(node);
                let bits = value.chain[index].kinds;
                if bits.any(NodeKinds::DRAW) && lifted != Some(index as u16) {
                    // An inner layer draws the rest in a group of its own.
                    if self.draw_inner_layer(node, index, layer, origins, list, observer) {
                        self.draw_afters(node, &after, origins, list);
                        return;
                    }
                    let value = self.node_mut(node);
                    if value.chain[index].state & entry_state::DELEGATING != 0 {
                        let next_layer = layer + usize::from(bits.any(NodeKinds::LAYOUT));
                        self.draw_delegating_entry(
                            node, index, layer, next_layer, origins, list, observer,
                        );
                        self.draw_afters(node, &after, origins, list);
                        return;
                    }
                    let draws_content =
                        value.chain[index].node.as_draw().is_some_and(|draw| draw.draws_content());
                    let size = value.layers[layer].measured.into();
                    let layout_direction = value.layout_direction;
                    if draws_content {
                        // Out of the chain while it draws the rest of its node.
                        let mut entry = mem::replace(
                            &mut value.chain[index].node,
                            Box::new(crate::nodes::Taken),
                        );
                        let next_layer = layer + usize::from(bits.any(NodeKinds::LAYOUT));
                        let mut scope = DrawScope {
                            list,
                            origin: origins[layer],
                            size,
                            density,
                            layout_direction,
                            locals,
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
                        self.node_mut(node).chain[index].node = entry;
                        self.draw_afters(node, &after, origins, list);
                        return;
                    }
                    let mut scope = DrawScope {
                        list,
                        origin: origins[layer],
                        size,
                        density,
                        layout_direction,
                        locals,
                    };
                    if let Some(draw) = value.chain[index].node.as_draw() {
                        draw.draw(&mut scope);
                    }
                    after.push((index as u16, layer as u16));
                }
                if bits.any(NodeKinds::LAYOUT) {
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

    /// Draws entry `index` of `node`, which has delegates, as Compose's `drawDirect` by
    /// `dispatchForKind`: the node if it draws, else each delegate that does, in delegation
    /// order, each with what's inside it as its content (so two drawing their content draw it
    /// twice, as Compose's; one that doesn't hides it).
    #[allow(clippy::too_many_arguments)]
    fn draw_delegating_entry(
        &mut self,
        node: NodeId,
        index: usize,
        layer: usize,
        next_layer: usize,
        origins: &[Offset],
        list: &mut DisplayList,
        observer: Option<&compose_runtime::platform::ReadObserver>,
    ) {
        let value = self.node_mut(node);
        let (density, locals) = (value.density, value.locals);
        let own = value.chain[index].own;
        let size = value.layers[layer].measured.into();
        let layout_direction = value.layout_direction;
        // Out of the chain while it draws the rest of its node.
        let mut entry = mem::replace(&mut value.chain[index].node, Box::new(crate::nodes::Taken));
        dispatch_for_kind(
            &mut *entry,
            own,
            NodeKinds::DRAW,
            EntryId::NO_DELEGATE,
            &mut |modifier_node, _| {
                let Some(draw) = modifier_node.as_draw() else { return };
                let mut scope = DrawScope {
                    list: &mut *list,
                    origin: origins[layer],
                    size,
                    density,
                    layout_direction,
                    locals,
                };
                let mut content = |list: &mut DisplayList| {
                    self.draw_chain(node, index + 1, next_layer, origins, list, observer)
                };
                if draw.draws_content() {
                    let mut content_scope = crate::draw_modifiers::ContentDrawScope {
                        scope: &mut scope,
                        content: &mut content,
                    };
                    draw.draw_with_content(&mut content_scope);
                } else {
                    draw.draw(&mut scope);
                    content(&mut *scope.list);
                    draw.draw_after(&mut scope);
                }
            },
        );
        self.node_mut(node).chain[index].node = entry;
        self.note_delegates_changed(node, index);
    }

    /// The `draw_after`s of `after`'s entries, innermost first.
    fn draw_afters(
        &mut self,
        node: NodeId,
        after: &[(u16, u16)],
        origins: &[Offset],
        list: &mut DisplayList,
    ) {
        let value = self.node_mut(node);
        let (density, locals) = (value.density, value.locals);
        for &(index, layer) in after.iter().rev() {
            let mut scope = DrawScope {
                list,
                origin: origins[layer as usize],
                size: value.layers[layer as usize].measured.into(),
                density,
                layout_direction: value.layout_direction,
                locals,
            };
            if let Some(draw) = value.chain[index as usize].node.as_draw() {
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
