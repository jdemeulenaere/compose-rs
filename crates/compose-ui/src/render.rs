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
        let density = self.density;
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
        let mut after: SmallVec<[(u16, u16); 4]> = SmallVec::new();
        if value.all_capabilities & capability::DRAW != 0 {
            let mut layer = 0usize;
            for index in 0..value.chain.len() {
                let bits = value.capabilities[index];
                if bits & capability::DRAW != 0 {
                    let mut scope = DrawScope {
                        list,
                        origin: origins[layer],
                        size: value.layers[layer].measured.into(),
                        density,
                    };
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
        let count = value.children.len();
        if count > 0 {
            translate(list, content);
            for index in 0..count {
                let child = self.node(node).children[index].node;
                // Overlays are drawn last, over everything.
                if !self.node(child).overlay {
                    self.draw_node(child, list, observer);
                }
            }
            translate(list, negate(content));
        }
        let value = self.node_mut(node);
        for &(index, layer) in after.iter().rev() {
            let mut scope = DrawScope {
                list,
                origin: origins[layer as usize],
                size: value.layers[layer as usize].measured.into(),
                density,
            };
            if let Some(draw) = value.chain[index as usize].as_draw() {
                draw.draw_after(&mut scope);
            }
        }
        translate(list, negate(position));
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
