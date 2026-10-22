//! The draw pass: records the display list from the tree (docs/06-ui.md, "Drawing").
//!
//! A repaint boundary keeps what it drew (`RetainedDrawing`), as Compose's layer keeps its
//! render node's recording: a draw records what's outside every boundary, each boundary by
//! reference, then splices in each boundary's kept operations, drawing a boundary again first
//! only if something it drew from changed (tree.rs routes the changes: `invalidate_draw_of`).

use std::mem;

use smallvec::SmallVec;

use crate::display_list::{DisplayList, DrawGroup, DrawOp};
use crate::draw::DrawScope;
use crate::graphics_layer::{GraphicsLayerNode, LayerProperties};
use crate::tree::{NodeId, Observation, UiTree, capability, observation_key};
use crate::unit::Offset;

/// What a repaint boundary drew last, kept until something it drew from changes.
pub(crate) struct RetainedDrawing {
    /// Its operations, in its own coordinates (after its position): its chain and children,
    /// the boundaries inside it by reference (`DisplayList::boundaries`).
    recording: DisplayList,
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
}

impl RetainedDrawing {
    pub(crate) fn new(layer_entry: Option<u16>) -> RetainedDrawing {
        RetainedDrawing {
            recording: DisplayList::default(),
            version: 0,
            layer_entry,
            layer: None,
            dirty: true,
            layer_dirty: true,
        }
    }
}

impl UiTree {
    /// Records the display list, if anything changed since the last draw. Returns whether it
    /// did.
    pub fn draw(&mut self) -> bool {
        if !self.needs_draw {
            return false;
        }
        let observer = self.observer.clone();
        let mut recording = mem::take(&mut self.root_recording);
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
        let mut list = mem::take(&mut self.display_list);
        list.clear();
        self.splice(&recording, &mut list, observer.as_deref());
        self.display_list = list;
        self.root_recording = recording;
        self.needs_draw = false;
        true
    }

    /// Records `node`; a custom node draws under observation (`observer`), so a state write it
    /// read draws again (D71). A repaint boundary is recorded by reference: `splice` draws it.
    fn draw_node(
        &mut self,
        node: NodeId,
        list: &mut DisplayList,
        observer: Option<&compose_runtime::ReadObserver>,
    ) {
        let value = self.node(node);
        if !value.placed {
            return;
        }
        if value.retained.is_some() {
            let position = value.layers[0].offset;
            let position = Offset::new(position.x as f32, position.y as f32);
            translate(list, position);
            list.boundaries.push((list.ops.len(), node));
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
        observer: Option<&compose_runtime::ReadObserver>,
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
        observer: Option<&compose_runtime::ReadObserver>,
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

    /// Appends `recording` to `list`, drawing each boundary it holds by reference
    /// (`draw_boundary`) where it is. Returns whether one's version changed.
    fn splice(
        &mut self,
        recording: &DisplayList,
        list: &mut DisplayList,
        observer: Option<&compose_runtime::ReadObserver>,
    ) -> bool {
        let mut changed = false;
        let mut from = 0;
        for &(index, node) in &recording.boundaries {
            list.ops.extend_from_slice(&recording.ops[from..index]);
            changed |= self.draw_boundary(node, list, observer);
            from = index;
        }
        list.ops.extend_from_slice(&recording.ops[from..]);
        changed
    }

    /// Appends repaint boundary `node`'s group to `list`: its layer's operations around its
    /// content, the content drawn again first if something it drew from changed, else spliced
    /// from what it kept, without running its draw code. Returns whether its version changed.
    fn draw_boundary(
        &mut self,
        node: NodeId,
        list: &mut DisplayList,
        observer: Option<&compose_runtime::ReadObserver>,
    ) -> bool {
        let Some(retained) = self.node_mut(node).retained.as_mut() else { return false };
        // Its flags are cleared once it's drawn: a panic drawing it leaves it to draw again.
        let (dirty, layer_dirty) = (retained.dirty, retained.layer_dirty);
        let mut recording = mem::take(&mut retained.recording);
        if dirty {
            recording.clear();
            let custom = self.node(node).all_capabilities & capability::CUSTOM != 0;
            match observer.filter(|_| custom) {
                Some(observer) => observer
                    .observe(observation_key(node, Observation::Draw), || {
                        self.draw_content(node, &mut recording, Some(observer))
                    }),
                None => self.draw_content(node, &mut recording, observer),
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
        let layer = retained.layer;
        // Its layer's operations, outside its content (`DrawGroup`).
        let layer_start = list.ops.len();
        list.merge_floor = layer_start;
        let restores = layer.map_or(0, |layer| layer.push_operations(list));
        let start = list.ops.len();
        let nested = self.splice(&recording, list, observer);
        let end = list.ops.len();
        if end == start {
            // Nothing to apply them to.
            list.ops.truncate(layer_start);
        } else {
            for _ in 0..restores {
                list.push(DrawOp::Restore);
            }
        }
        let layer_end = list.ops.len();
        list.merge_floor = layer_end;
        let changed = dirty || nested;
        if changed {
            self.draw_version += 1;
        }
        let version = self.draw_version;
        let retained = self.node_mut(node).retained.as_mut().expect("a repaint boundary");
        retained.recording = recording;
        if changed {
            retained.version = version;
        }
        if end > start {
            list.groups.push(DrawGroup {
                key: node.to_bits(),
                version: retained.version,
                start,
                end,
                layer_start,
                layer_end,
                layer,
            });
        }
        changed
    }

    /// The properties of `node`'s graphics layer whose properties are its group's, its block
    /// run under its own observation: a write to what it read changes only them.
    fn layer_properties(
        &mut self,
        node: NodeId,
        observer: Option<&compose_runtime::ReadObserver>,
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
        observer: Option<&compose_runtime::ReadObserver>,
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
        let count = self.node(node).children.len();
        if count > 0 {
            translate(list, content);
            let order = self.placement_order(node);
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
