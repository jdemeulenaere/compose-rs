//! The measure pass (docs/06-ui.md, "Layout"): from the root, measuring only what changed.

use std::mem;
use std::panic::{self, AssertUnwindSafe};

use crate::constraints::Constraints;
use crate::layout::{Measurable, MeasureResult, MeasureScope};
use crate::nodes::Taken;
use crate::tree::{Layer, NodeId, Observation, UiTree, capability, observation_key};
use crate::unit::{IntOffset, IntSize};

impl UiTree {
    /// Measures and places the tree in a root of `size` pixels. Does nothing when nothing
    /// changed since the last call with this size.
    pub fn layout(&mut self, size: IntSize) -> bool {
        if self.window != size {
            // Popups are measured with the window's size, whatever their host's constraints.
            for index in 0..self.overlays.len() {
                let overlay = self.overlays[index];
                if let Some(host) = self.get(overlay).and_then(|value| value.parent) {
                    self.invalidate_measure(host);
                }
            }
        }
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
        self.laid_out_since_callbacks = true;
        true
    }

    /// Places the overlays in the root with their providers, from their anchors' bounds (inside
    /// the modifiers of the layout around the popup, the parent of its host, as Compose's
    /// `parentLayoutCoordinates`), and moves the focus into those that ask for it when they
    /// first appear; once one that took it is gone, what had it before gets it back.
    fn position_overlays(&mut self) {
        let mut overlays = std::mem::take(&mut self.overlays);
        overlays.retain(|node| self.get(*node).is_some_and(|value| value.overlay));
        self.order_sibling_overlays(&mut overlays);
        self.overlays = overlays;
        // Closed overlays first give the focus back, so one opened in the same frame (a
        // dialog replacing a menu) gives it back in turn once it closes.
        self.restore_focus_of_closed_overlays();
        for index in 0..self.overlays.len() {
            let node = self.overlays[index];
            let value = self.node(node);
            let size = value.layers[0].size;
            let anchor_node = value.parent.and_then(|host| self.node(host).parent).or(value.parent);
            let anchor = anchor_node.map_or(crate::unit::Rect::new(0.0, 0.0, 0.0, 0.0), |anchor| {
                let inner = self.node(anchor).layers.len() - 1;
                let origin = self.layer_origin(anchor, inner);
                let size = self.node(anchor).layers[inner].measured;
                crate::unit::Rect::from_offset_size(origin, size.into())
            });
            let window = self.window;
            let Some((provider, focus)) = self.overlay_node(node).map(|overlay| {
                (overlay.provider.clone(), overlay.focus_on_open && !overlay.focused_once)
            }) else {
                continue;
            };
            let direction = self.node(node).layout_direction;
            let position = provider.position(anchor, window, direction, size);
            let value = self.node_mut(node);
            value.overlay_origin = crate::unit::Offset::new(position.x as f32, position.y as f32);
            if focus {
                for entry in value.chain.iter_mut() {
                    if let Some(overlay) = entry.downcast_mut::<crate::overlay::OverlayNode>() {
                        overlay.focused_once = true;
                    }
                }
                let before = self.focused();
                self.focus_restore.push((node, before));
                let mut actions = std::mem::take(&mut self.pending_actions);
                self.focus_into(node, false, &mut actions);
                self.pending_actions = actions;
            }
        }
    }

    /// Overlays of one host (a popup and its outside) in the host's order of children,
    /// whenever each became one: the popup over its outside.
    fn order_sibling_overlays(&self, overlays: &mut [NodeId]) {
        if overlays.len() < 2 {
            return;
        }
        let child_index = |node: NodeId| {
            let parent = self.node(node).parent?;
            self.node(parent).children.iter().position(|child| child.node == node)
        };
        for first in 0..overlays.len() {
            let parent = self.node(overlays[first]).parent;
            let positions: Vec<usize> = (first..overlays.len())
                .filter(|&index| self.node(overlays[index]).parent == parent)
                .collect();
            if positions.len() < 2 || positions[0] != first {
                continue;
            }
            let mut members: Vec<NodeId> = positions.iter().map(|&index| overlays[index]).collect();
            members.sort_by_key(|&node| child_index(node));
            for (&index, member) in positions.iter().zip(members) {
                overlays[index] = member;
            }
        }
    }

    /// Gives the focus back to what had it before an overlay that took it, once that overlay
    /// is gone and nothing else took it meanwhile.
    fn restore_focus_of_closed_overlays(&mut self) {
        let mut index = 0;
        while index < self.focus_restore.len() {
            let (overlay, before) = self.focus_restore[index];
            let takes_focus = self.get(overlay).is_some_and(|value| value.overlay)
                && self.overlay_node(overlay).is_some_and(|overlay| overlay.focus_on_open);
            if takes_focus {
                index += 1;
                continue;
            }
            self.focus_restore.remove(index);
            // Overlays opened over it pass their restore on to it.
            for later in 0..self.focus_restore.len() {
                if self.focus_restore[later].1.is_some_and(|node| self.get(node).is_none()) {
                    self.focus_restore[later].1 = before;
                }
            }
            let focus_lost = self.focused().is_none_or(|focused| self.get(focused).is_none());
            if focus_lost && let Some(before) = before.filter(|node| self.get(*node).is_some()) {
                let mut actions = std::mem::take(&mut self.pending_actions);
                self.focus_into(before, false, &mut actions);
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
        // Measured, it draws again.
        self.invalidate_draw_of(node);
        self.measure_layer(node, 0, constraints)
    }

    /// Measures `parent`'s child at `index` with `constraints` before the layout that places it
    /// (a lazy list's next item, between frames), as Compose's `premeasure`: that layout, measuring
    /// it with the same constraints, finds it measured. Returns its size, or `None` without such
    /// a child.
    pub fn premeasure(
        &mut self,
        parent: NodeId,
        index: usize,
        constraints: Constraints,
    ) -> Option<IntSize> {
        let child = self.get(parent)?.children.get(index)?.node;
        Some(self.measure_node(child, constraints))
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
            // Its parent's size depends on its intrinsics, not just its size.
            if value.intrinsics_asked {
                return true;
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
            let mut relative = Vec::new();
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut scope = MeasureScope { tree: &mut *self, node, run, relative: Vec::new() };
                let content = Measurable { node, layer: layer as u32 + 1 };
                let mut measure = || {
                    modifier_node.as_layout().expect("compose-ui: a layout node").measure(
                        &mut scope,
                        content,
                        constraints,
                    )
                };
                let result = match &observer {
                    Some(observer) => {
                        observer.observe(observation_key(node, Observation::Measure), measure)
                    }
                    None => measure(),
                };
                relative = scope.relative;
                result
            }));
            self.node_mut(node).chain[index] = modifier_node;
            let result = self.recover(node, result);
            self.mirror(relative, result.width);
            result
        } else {
            let policy = value.policy.clone();
            let children = mem::take(&mut value.children);
            for child in &children {
                let child = self.node_mut(child.node);
                child.placed = false;
                child.intrinsics_asked = false;
            }
            let run = self.next_measure_run();
            let observer = self.observer.clone();
            let mut relative = Vec::new();
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut scope = MeasureScope { tree: &mut *self, node, run, relative: Vec::new() };
                let result = match &observer {
                    Some(observer) => observer
                        .observe(observation_key(node, Observation::Measure), || {
                            policy.measure(&mut scope, &children, constraints)
                        }),
                    None => policy.measure(&mut scope, &children, constraints),
                };
                relative = scope.relative;
                result
            }));
            self.node_mut(node).children = children;
            let result = self.recover(node, result);
            self.mirror(relative, result.width);
            result
        };
        let measured = IntSize::new(result.width.max(0), result.height.max(0));
        let size = constraints.constrain(measured);
        let value = &mut self.node_mut(node).layers[layer];
        value.measured = measured;
        value.size = size;
        // A new measured size within the same coerced one moves the centering.
        let offset = value.position + centering(value);
        if value.offset != offset {
            value.offset = offset;
            self.invalidate_position(node, layer);
        }
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

    /// Mirrors what a right-to-left layout of `width` placed relatively: each at its `x` from
    /// the right edge, as Compose's `placeRelative` (placement order kept).
    fn mirror(&mut self, relative: Vec<(crate::layout::Placeable, IntOffset)>, width: i32) {
        for (placeable, at) in relative {
            let measurable = placeable.measurable;
            let position = IntOffset::new(width - at.x - placeable.width, at.y);
            let entry = &mut self.node_mut(measurable.node).layers[measurable.layer as usize];
            entry.position = position;
            let offset = position + centering(entry);
            if entry.offset != offset {
                entry.offset = offset;
                self.invalidate_position(measurable.node, measurable.layer as usize);
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
        let offset = position + centering(entry);
        let moved = entry.offset != offset;
        entry.offset = offset;
        if layer == 0 {
            value.placed = true;
            self.place_serial += 1;
            let serial = self.place_serial;
            self.node_mut(node).place_order = serial;
        }
        if moved {
            self.invalidate_position(node, layer);
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
