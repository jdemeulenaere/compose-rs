//! The measure pass (docs/06-ui.md, "Layout"): from the root, measuring only what changed.

use std::mem;
use std::panic::{self, AssertUnwindSafe};

use crate::constraints::Constraints;
use crate::layout::{Measurable, MeasureResult, MeasureScope};
use crate::nodes::Taken;
use crate::tree::{Layer, NodeId, UiTree, capability, observation_key};
use crate::unit::{IntOffset, IntSize};

impl UiTree {
    /// Measures and places the tree in a root of `size` pixels. Does nothing when nothing
    /// changed since the last call with this size.
    pub fn layout(&mut self, size: IntSize) -> bool {
        self.window = size;
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
        self.position_overlays();
        self.needs_draw = true;
        true
    }

    /// Places the overlays in the root with their providers, from their anchors' bounds (the
    /// layout around the popup, the parent of its host), and moves the focus into those that
    /// ask for it when they first appear.
    fn position_overlays(&mut self) {
        let mut overlays = std::mem::take(&mut self.overlays);
        overlays.retain(|node| self.get(*node).is_some_and(|value| value.overlay));
        self.overlays = overlays;
        for index in 0..self.overlays.len() {
            let node = self.overlays[index];
            let value = self.node(node);
            let size = value.layers[0].size;
            let anchor_node = value.parent.and_then(|host| self.node(host).parent).or(value.parent);
            let anchor = anchor_node.map_or(crate::unit::Rect::new(0.0, 0.0, 0.0, 0.0), |anchor| {
                self.bounds_in_root(anchor)
            });
            let window = self.window;
            let Some((provider, focus)) = self.node(node).chain.iter().find_map(|entry| {
                entry.downcast_ref::<crate::overlay::OverlayNode>().map(|overlay| {
                    (overlay.provider.clone(), overlay.focus_on_open && !overlay.focused_once)
                })
            }) else {
                continue;
            };
            let position = provider.position(anchor, window, size);
            let value = self.node_mut(node);
            value.overlay_origin = crate::unit::Offset::new(position.x as f32, position.y as f32);
            if focus {
                for entry in value.chain.iter_mut() {
                    if let Some(overlay) = entry.downcast_mut::<crate::overlay::OverlayNode>() {
                        overlay.focused_once = true;
                    }
                }
                let mut actions = std::mem::take(&mut self.pending_actions);
                self.request_focus_into(node, &mut actions);
                self.pending_actions = actions;
            }
        }
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
    ///
    /// A panic in application code (a policy, a modifier node) unwinds through here: the node
    /// and its children are put back first, and the node marked for measuring again, so the
    /// tree stays whole and a later frame or its drop is sound.
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
            let custom = value.capabilities[index] & capability::CUSTOM != 0;
            let run = self.next_measure_run();
            let observer = if custom { self.observer.clone() } else { None };
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut scope = MeasureScope { tree: &mut *self, node, run };
                let content = Measurable { node, layer: layer as u32 + 1 };
                let mut measure = || {
                    modifier_node.as_layout().expect("compose-ui: a layout node").measure(
                        &mut scope,
                        content,
                        constraints,
                    )
                };
                match &observer {
                    Some(observer) => observer.observe(observation_key(node, false), measure),
                    None => measure(),
                }
            }));
            self.node_mut(node).chain[index] = modifier_node;
            self.recover(node, result)
        } else {
            let policy = value.policy.clone();
            let children = mem::take(&mut value.children);
            for child in &children {
                self.node_mut(child.node).placed = false;
            }
            let run = self.next_measure_run();
            let observer = self.observer.clone();
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut scope = MeasureScope { tree: &mut *self, node, run };
                match &observer {
                    Some(observer) => observer.observe(observation_key(node, false), || {
                        policy.measure(&mut scope, &children, constraints)
                    }),
                    None => policy.measure(&mut scope, &children, constraints),
                }
            }));
            self.node_mut(node).children = children;
            self.recover(node, result)
        };
        let measured = IntSize::new(result.width.max(0), result.height.max(0));
        let size = constraints.constrain(measured);
        let value = &mut self.node_mut(node).layers[layer];
        value.measured = measured;
        value.size = size;
        // A new measured size within the same coerced one moves the centering.
        value.offset = value.position + centering(value);
        size
    }

    /// The result of application measuring code, or, if it panicked, the node marked for
    /// measuring again and the panic resumed.
    fn recover(
        &mut self,
        node: NodeId,
        result: std::thread::Result<MeasureResult>,
    ) -> MeasureResult {
        match result {
            Ok(result) => result,
            Err(panic) => {
                self.node_mut(node).constraints = None;
                self.invalidate_measure(node);
                panic::resume_unwind(panic)
            }
        }
    }

    fn next_measure_run(&mut self) -> u64 {
        self.measure_runs += 1;
        self.measure_runs
    }

    /// Places layer `layer` of `node` at `position` in the layer outside it (or its parent's
    /// content). A layer measured outside its constraints is centered in the size its parent
    /// sees, as Compose does.
    pub(crate) fn place(&mut self, node: NodeId, layer: usize, position: IntOffset) {
        let value = self.node_mut(node);
        let entry = &mut value.layers[layer];
        entry.position = position;
        entry.offset = position + centering(entry);
        if layer == 0 {
            value.placed = true;
        }
    }
}

/// The offset centering a layer measured outside its constraints in the size its parent sees.
fn centering(layer: &Layer) -> IntOffset {
    IntOffset::new(
        (layer.size.width - layer.measured.width) / 2,
        (layer.size.height - layer.measured.height) / 2,
    )
}
