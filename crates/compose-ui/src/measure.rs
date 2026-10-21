//! The measure pass (docs/06-ui.md, "Layout"): from the root, measuring only what changed.

use std::mem;

use crate::constraints::Constraints;
use crate::layout::{Measurable, MeasureScope};
use crate::nodes::Taken;
use crate::tree::{NodeId, UiTree};
use crate::unit::{IntOffset, IntSize};

impl UiTree {
    /// Measures and places the tree in a root of `size` pixels. Does nothing when nothing
    /// changed since the last call with this size.
    pub fn layout(&mut self, size: IntSize) -> bool {
        let root = self.root_id();
        let constraints = Constraints::fixed(size.width, size.height);
        let value = self.node(root);
        if value.constraints == Some(constraints)
            && !value.needs_measure
            && !value.subtree_needs_measure
        {
            return false;
        }
        self.measure_node(root, constraints);
        self.place(root, 0, IntOffset::ZERO);
        self.needs_draw = true;
        true
    }

    /// Measures `node` with `constraints`, returning the size its parent sees. A node measured
    /// before with the same constraints and nothing changed inside returns its size at once; if
    /// only descendants changed, they're measured again in place, and the node itself only if
    /// one of its children's sizes changed.
    pub(crate) fn measure_node(&mut self, node: NodeId, constraints: Constraints) -> IntSize {
        let value = self.node(node);
        if value.constraints == Some(constraints)
            && !value.needs_measure
            && (!value.subtree_needs_measure || !self.remeasure_children_in_place(node))
        {
            let value = self.node_mut(node);
            value.subtree_needs_measure = false;
            return value.layers[0].size;
        }
        let value = self.node_mut(node);
        value.needs_measure = false;
        value.subtree_needs_measure = false;
        value.constraints = Some(constraints);
        self.measure_layer(node, 0, constraints)
    }

    /// Measures the dirty children of `node` again with their last constraints. Returns whether
    /// one's size changed, so `node` must be measured again.
    fn remeasure_children_in_place(&mut self, node: NodeId) -> bool {
        let count = self.node(node).children.len();
        for index in 0..count {
            let child = self.node(node).children[index].node;
            let value = self.node(child);
            if !value.needs_measure && !value.subtree_needs_measure {
                continue;
            }
            let Some(constraints) = value.constraints else { return true };
            let before = value.layers[0].size;
            if self.measure_node(child, constraints) != before {
                return true;
            }
        }
        false
    }

    /// Measures layer `layer` of `node`: a layout modifier, or past the last one the node's
    /// policy with its children.
    pub(crate) fn measure_layer(
        &mut self,
        node: NodeId,
        layer: usize,
        constraints: Constraints,
    ) -> IntSize {
        let value = self.node_mut(node);
        let result = if layer < value.layout_indices.len() {
            let index = value.layout_indices[layer] as usize;
            // Out of the chain while it runs, so it can measure the rest of its own node.
            let mut modifier_node = mem::replace(&mut value.chain[index], Box::new(Taken));
            let result = {
                let mut scope = MeasureScope { tree: self, node };
                let content = Measurable { node, layer: layer as u32 + 1 };
                modifier_node.as_layout().expect("compose-ui: a layout node").measure(
                    &mut scope,
                    content,
                    constraints,
                )
            };
            self.node_mut(node).chain[index] = modifier_node;
            result
        } else {
            let policy = value.policy.clone();
            let children = mem::take(&mut value.children);
            for child in &children {
                self.node_mut(child.node).placed = false;
            }
            let result = {
                let mut scope = MeasureScope { tree: self, node };
                policy.measure(&mut scope, &children, constraints)
            };
            self.node_mut(node).children = children;
            result
        };
        let measured = IntSize::new(result.width.max(0), result.height.max(0));
        let size = constraints.constrain(measured);
        let value = &mut self.node_mut(node).layers[layer];
        value.measured = measured;
        value.size = size;
        size
    }

    /// Places layer `layer` of `node` at `position` in the layer outside it (or its parent's
    /// content). A layer measured outside its constraints is centered in the size its parent
    /// sees, as Compose does.
    pub(crate) fn place(&mut self, node: NodeId, layer: usize, position: IntOffset) {
        let value = self.node_mut(node);
        let entry = &mut value.layers[layer];
        let centering = IntOffset::new(
            (entry.size.width - entry.measured.width) / 2,
            (entry.size.height - entry.measured.height) / 2,
        );
        entry.offset = position + centering;
        if layer == 0 {
            value.placed = true;
        }
    }
}
