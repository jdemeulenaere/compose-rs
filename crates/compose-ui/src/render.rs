//! The draw pass: records the display list from the tree (docs/06-ui.md, "Drawing").

use std::mem;

use smallvec::SmallVec;

use crate::display_list::{DisplayList, DrawOp};
use crate::draw::DrawScope;
use crate::tree::{NodeId, UiTree, capability, observation_key};
use crate::unit::Offset;

impl UiTree {
    /// Records the display list, if anything changed since the last draw. Returns whether it
    /// did.
    pub fn draw(&mut self) -> bool {
        if !self.needs_draw {
            return false;
        }
        let mut list = mem::take(&mut self.display_list);
        list.clear();
        let observer = self.observer.clone();
        self.draw_node(self.root_id(), &mut list, observer.as_deref());
        // The overlays, over everything, each where its provider put it.
        for index in 0..self.overlays.len() {
            let node = self.overlays[index];
            let Some(value) = self.get(node).filter(|value| value.overlay && value.placed) else {
                continue;
            };
            let placed = value.layers[0].offset;
            let at = value.overlay_origin - Offset::new(placed.x as f32, placed.y as f32);
            translate(&mut list, at);
            self.draw_node(node, &mut list, observer.as_deref());
            translate(&mut list, negate(at));
        }
        self.display_list = list;
        self.needs_draw = false;
        true
    }

    /// Records `node`; a custom node draws under observation (`observer`), so a state write it
    /// read draws again (D71).
    fn draw_node(
        &mut self,
        node: NodeId,
        list: &mut DisplayList,
        observer: Option<&compose_runtime::ReadObserver>,
    ) {
        // One observation for the node's drawing, before and after its children.
        let custom =
            self.get(node).is_some_and(|value| value.all_capabilities & capability::CUSTOM != 0);
        match observer.filter(|_| custom) {
            Some(observer) => observer.observe(observation_key(node, true), || {
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
        let value = self.node_mut(node);
        if !value.placed {
            return;
        }
        let position = value.layers[0].offset;
        let position = Offset::new(position.x as f32, position.y as f32);
        translate(list, position);
        // Each layer's origin in the node.
        let mut origins: SmallVec<[Offset; 4]> = SmallVec::new();
        let mut origin = Offset::ZERO;
        for (index, layer) in value.layers.iter().enumerate() {
            if index > 0 {
                origin = origin + Offset::new(layer.offset.x as f32, layer.offset.y as f32);
            }
            origins.push(origin);
        }
        self.draw_chain(node, 0, 0, &origins, list, observer);
        translate(list, negate(position));
    }
}

impl UiTree {
    /// Draws `node`'s chain from entry `from` (in layer `layer`), then its children, then the
    /// `draw_after`s of the entries drawn here. An entry drawing its content
    /// (`draw_with_content`) draws the rest of the chain and the children where it asks.
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
            for index in from..length {
                let value = self.node_mut(node);
                let bits = value.capabilities[index];
                if bits & capability::DRAW != 0 {
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

fn translate(list: &mut DisplayList, by: Offset) {
    if by != Offset::ZERO {
        list.push(DrawOp::Translate(by));
    }
}
