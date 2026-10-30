//! The measure pass (docs/06-ui.md, "Layout"): from the root, measuring only what changed.

use std::mem;
use std::ops::ControlFlow;
use std::panic::{self, AssertUnwindSafe};

use crate::constraints::Constraints;
use crate::delegation::{dispatch_with_context, first_of_kind};
use crate::layout::{Intrinsic, IntrinsicMeasureScope, Measurable, MeasureResult, MeasureScope};
use crate::node_chain::{NodeKinds, entry_state};
use crate::nodes::Taken;
use crate::tree::{Layer, NodeId, Observation, UiTree, observation_key};
use crate::tree_access::TreeAccess;
use crate::unit::{IntOffset, IntSize};

impl UiTree {
    /// Measures and places the tree in a root of `size` pixels. Does nothing when nothing
    /// changed since the last call with this size.
    pub fn layout(&mut self, size: IntSize) -> bool {
        // As Compose's as a write applies: what's about to ask reads what they give. With nothing
        // handed out, nothing can ask before the draw.
        if !self.new_layers.is_empty() && self.focus_requests.coordinates_given.get() {
            self.run_new_layers();
        }
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
        self.focus_requests.container_size.set(size);
        let root = self.root_id();
        let constraints = Constraints::fixed(size.width, size.height);
        let value = self.node(root);
        if value.constraints == Some(constraints)
            && !value.needs_measure
            && !value.subtree_needs_measure
            && !value.needs_placement
            && !value.subtree_needs_placement
        {
            return false;
        }
        self.measure_node(root, constraints);
        self.place(root, 0, IntOffset::ZERO);
        let value = self.node(root);
        if value.needs_placement || value.subtree_needs_placement {
            self.place_pass(root);
        }
        if !self.coordinate_readers.is_empty() {
            self.place_readers_moved(root);
        }
        self.position_overlays();
        // Where the popups just went is where their readers are.
        if !self.coordinate_readers.is_empty() {
            self.place_readers_moved(root);
        }
        self.needs_draw = true;
        self.laid_out_since_callbacks = true;
        true
    }

    /// The window as a popup's is, Android's visible display frame: without the status bars
    /// above, and without the larger of the keyboard and the navigation bars below. A read of
    /// their states, so what measures with it is measured again when they change.
    pub(crate) fn visible_window(&self) -> crate::unit::Rect {
        use crate::configuration::InsetsType;
        let insets = &self.window_insets;
        let top = insets.insets(InsetsType::StatusBars).top;
        let bottom = insets
            .insets(InsetsType::Ime)
            .bottom
            .max(insets.insets(InsetsType::NavigationBars).bottom);
        let window = self.window;
        crate::unit::Rect::new(
            0.0,
            top as f32,
            window.width as f32,
            (window.height - bottom).max(top) as f32,
        )
    }

    /// Places the overlays in the root with their providers, from their anchors' bounds (inside
    /// the modifiers of the layout around the popup, the parent of its host, as Compose's
    /// `parentLayoutCoordinates`), and moves the focus into those that ask for it when they
    /// first appear; once one that took it is gone, the window below it takes the keys (D1284).
    fn position_overlays(&mut self) {
        let mut overlays = std::mem::take(&mut self.overlays);
        overlays.retain(|node| self.get(*node).is_some_and(|value| value.overlay));
        self.order_sibling_overlays(&mut overlays);
        self.overlays = overlays;
        // Closed windows go first, so one opened in the same frame (a dialog replacing a menu)
        // stacks on what's below them.
        self.close_gone_windows();
        for index in 0..self.overlays.len() {
            let node = self.overlays[index];
            let value = self.node(node);
            // Put aside: no window until it's reused, opening again then (`OverlayNode::on_reset`).
            if value.deactivated {
                continue;
            }
            let size = value.layers[0].size;
            let anchor_node = value.parent.and_then(|host| self.node(host).parent).or(value.parent);
            let anchor = anchor_node.map_or(crate::unit::Rect::new(0.0, 0.0, 0.0, 0.0), |anchor| {
                let inner = self.node(anchor).layers.len() - 1;
                let origin = self.layer_origin(anchor, inner);
                let size = self.node(anchor).layers[inner].measured;
                crate::unit::Rect::from_offset_size(origin, size.into())
            });
            let window = self.visible_window();
            let window =
                IntSize::new(window.width().round() as i32, window.height().round() as i32);
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
                    if let Some(overlay) = entry.node.downcast_mut::<crate::overlay::OverlayNode>()
                    {
                        overlay.focused_once = true;
                    }
                }
                // A request from its content's effect came first: it's kept (D1268). It
                // became a window just now if it wasn't one.
                self.window_opened(node);
                let token = self.overlay_node(node).and_then(|overlay| overlay.window.clone());
                if let Some(token) = token {
                    token.0.set(Some(node));
                }
                let active = self.in_window(Some(node), |tree| tree.focus.active);
                if active.is_none() {
                    self.in_window(Some(node), |tree| tree.focus_without_scrolling(node));
                }
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

    /// Measures `node` with `constraints`, returning the size its parent sees. A node measured
    /// before with the same constraints and nothing changed inside returns its size at once; if
    /// only descendants changed, they're measured again in place, and the node itself only if
    /// one of its children's sizes changed.
    ///
    /// Intrinsics don't touch this cache (they measure nothing): a node whose intrinsics were
    /// asked is measured again by the parent that asked (D1201).
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
        // What its semantics read may have changed with what measured it (a lazy list's
        // ranges, a text field's text): computed again when next asked (D1257).
        value.semantics = None;
        // What this measurement asks of its descendants' intrinsics is asked anew, unless its
        // own were asked: then what its parent asked through it holds (Compose's
        // `clearSubtreeIntrinsicsUsage`).
        if value.children_intrinsics_asked && !value.intrinsics_asked {
            self.clear_intrinsics_usage_below(node);
        }
        let value = self.node_mut(node);
        if value.inner_layers.is_empty() {
            // Measured, it draws again.
            self.invalidate_draw_of(node);
            return self.measure_layer(node, 0, constraints);
        }
        // With inner layers, what it lays out (its children) is theirs: what's before them draws
        // again only if a layer's size changed (a moved layer draws it again: `place`), as
        // Compose's layer keeps what's around it when its content is measured again (D522).
        let sizes: smallvec::SmallVec<[IntSize; 4]> =
            value.layers.iter().map(|layer| layer.measured).collect();
        self.invalidate_inner_layers(node);
        let size = self.measure_layer(node, 0, constraints);
        let value = self.node(node);
        if value.layers.len() != sizes.len()
            || value.layers.iter().zip(&sizes).any(|(layer, size)| layer.measured != *size)
        {
            self.invalidate_draw_of(node);
        }
        size
    }

    /// Measures `node` now with its last constraints, whether or not it asked, and so places
    /// its content, as Compose's `forceRemeasure` (`Remeasurement`, `remeasureSync`): its
    /// position in its parent stays; a new size measures its parent in the next layout, as
    /// Compose's `doRemeasure` asks it. Nothing for a deactivated node or one out of the tree;
    /// the root, or a node never measured, is measured in the next layout.
    pub(crate) fn remeasure_now(&mut self, node: NodeId) {
        let root = self.root_id();
        let Some(value) = self.get(node) else { return };
        // Out of the tree, or content kept for reuse: no layout measures it either.
        if value.deactivated || !value.attached {
            return;
        }
        let Some(constraints) = value.constraints.filter(|_| node != root) else {
            self.invalidate_layout_of(node);
            return;
        };
        let before = value.layers[0].size;
        self.node_mut(node).needs_measure = true;
        // A measurement isn't a place to measure from (its callbacks invalidate instead).
        let allowed = mem::replace(&mut self.remeasure_allowed, false);
        let result = panic::catch_unwind(AssertUnwindSafe(|| self.measure_node(node, constraints)));
        self.remeasure_allowed = allowed;
        let size = result.unwrap_or_else(|panic| panic::resume_unwind(panic));
        self.laid_out_since_callbacks = true;
        // Popups measured in it are placed by their providers, as after a layout.
        if !self.overlays.is_empty() {
            self.position_overlays();
        }
        if size != before
            && let Some(parent) = self.parent_of(node)
        {
            self.invalidate_measure(parent);
        }
    }

    /// Measures `parent`'s child at `index` with `constraints` before the layout that places it
    /// (a lazy list's next item, between frames), as Compose's `premeasure`: that layout, measuring
    /// it with the same constraints, finds it measured. Returns its size, or `None` without such
    /// a child. With `Optimization::DRAW_PREFETCHED_AHEAD`, a repaint boundary measured so is
    /// drawn ahead too, if the platform asks (`draw_ahead`, D350, D564).
    pub fn premeasure(
        &mut self,
        parent: NodeId,
        index: usize,
        constraints: Constraints,
    ) -> Option<IntSize> {
        let child = self.get(parent)?.children.get(index)?.node;
        let size = self.measure_node(child, constraints);
        if self.optimizations.contains(crate::optimizations::Optimization::DRAW_PREFETCHED_AHEAD)
            && !self.measured_ahead.contains(&child)
        {
            self.measured_ahead.push(child);
        }
        Some(size)
    }

    /// Measures the dirty children of `node` again with their last constraints. Returns whether
    /// one's size changed, so `node` must be measured again.
    fn remeasure_children_in_place(&mut self, node: NodeId) -> bool {
        let count = self.node(node).children.len();
        for index in 0..count {
            let child = self.node(node).children[index].node;
            let value = self.node(child);
            if (!value.needs_measure && !value.subtree_needs_measure) || value.deactivated {
                continue;
            }
            // Its parent (or a layout above whose intrinsics were asked through it) asked its
            // intrinsics, which may have changed: that layout measures again, as Compose's
            // `invalidateIntrinsicsParent`.
            if value.intrinsics_asked && self.intrinsics_invalidated(child) {
                return true;
            }
            let Some(constraints) = value.constraints else { return true };
            let before = value.layers[0].size;
            let in_block = value.measured_in_block;
            if self.measure_node(child, constraints) != before {
                // Its parent's measurement didn't see it, its placement block did.
                if !in_block {
                    return true;
                }
                self.invalidate_placement_of(node);
            }
        }
        false
    }

    /// Whether `node`, whose intrinsics its parent asked, needs measuring itself, or a
    /// descendant does whose intrinsics were asked through it (each one's flag set up to `node`):
    /// what asked may have changed (Compose's `invalidateIntrinsicsParent`).
    fn intrinsics_invalidated(&self, node: NodeId) -> bool {
        let value = self.node(node);
        value.needs_measure
            || value.subtree_needs_measure
                && value.children_intrinsics_asked
                && value.children.iter().any(|child| {
                    let value = self.node(child.node);
                    value.intrinsics_asked
                        && !value.deactivated
                        && (value.needs_measure || value.subtree_needs_measure)
                        && self.intrinsics_invalidated(child.node)
                })
    }

    /// Forgets which intrinsics `node`'s measurement asked below it: its children's, and
    /// theirs asked through them (Compose's `clearSubtreeIntrinsicsUsage`).
    fn clear_intrinsics_usage_below(&mut self, node: NodeId) {
        self.node_mut(node).children_intrinsics_asked = false;
        for index in 0..self.node(node).children.len() {
            let child = self.node(node).children[index].node;
            let value = self.node_mut(child);
            if value.intrinsics_asked {
                value.intrinsics_asked = false;
                if value.children_intrinsics_asked {
                    self.clear_intrinsics_usage_below(child);
                }
            }
        }
    }

    /// The intrinsic `kind` at `value` (the size on the other axis) of layer `layer` of `node`:
    /// its layout modifier's intrinsic function with the rest as its content, or past the last
    /// one its policy's with its children. Nothing is measured, placed or kept.
    pub(crate) fn intrinsic(
        &mut self,
        node: NodeId,
        layer: usize,
        kind: Intrinsic,
        value: i32,
    ) -> i32 {
        let entry = self.node_mut(node);
        if layer < entry.layout_indices.len() {
            let index = entry.layout_indices[layer] as usize;
            // Out of the chain while it runs, so it can ask the rest of its own node.
            let own = entry.chain[index].own;
            let mut modifier_node = mem::replace(&mut entry.chain[index].node, Box::new(Taken));
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut scope = IntrinsicMeasureScope { tree: TreeAccess::new(&mut *self), node };
                let _open = scope.tree.open();
                let content = Measurable { node, layer: layer as u32 + 1 };
                let layout = first_of_kind(&mut *modifier_node, own, NodeKinds::LAYOUT)
                    .and_then(|node| node.as_layout())
                    .expect("compose-ui: a layout node");
                match kind {
                    Intrinsic::MinWidth => layout.min_intrinsic_width(&mut scope, content, value),
                    Intrinsic::MinHeight => layout.min_intrinsic_height(&mut scope, content, value),
                    Intrinsic::MaxWidth => layout.max_intrinsic_width(&mut scope, content, value),
                    Intrinsic::MaxHeight => layout.max_intrinsic_height(&mut scope, content, value),
                }
            }));
            self.node_mut(node).chain[index].node = modifier_node;
            return result.unwrap_or_else(|panic| panic::resume_unwind(panic));
        }
        let policy = entry.policy.clone();
        let children = mem::take(&mut entry.children);
        // Deactivated children (content kept for reuse) aren't there for the policy.
        let active: Vec<Measurable>;
        let measurables: &[Measurable] =
            if children.iter().any(|child| self.node(child.node).deactivated) {
                active = (children.iter())
                    .filter(|child| !self.node(child.node).deactivated)
                    .copied()
                    .collect();
                &active
            } else {
                &children
            };
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut scope = IntrinsicMeasureScope { tree: TreeAccess::new(&mut *self), node };
            let _open = scope.tree.open();
            match kind {
                Intrinsic::MinWidth => policy.min_intrinsic_width(&mut scope, measurables, value),
                Intrinsic::MinHeight => policy.min_intrinsic_height(&mut scope, measurables, value),
                Intrinsic::MaxWidth => policy.max_intrinsic_width(&mut scope, measurables, value),
                Intrinsic::MaxHeight => policy.max_intrinsic_height(&mut scope, measurables, value),
            }
        }));
        self.node_mut(node).children = children;
        result.unwrap_or_else(|panic| panic::resume_unwind(panic))
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
        // Its last measurement's block is over, whatever this one does. Dropped with nothing of
        // the tree in hand: what it captured is application code's.
        let earlier = value.placements.as_mut().and_then(|it| it.blocks.get_mut(layer)?.take());
        drop(earlier);
        let value = self.node_mut(node);
        let earlier_rulers =
            value.placements.as_mut().and_then(|it| it.rulers.get_mut(layer)?.take());
        let value = self.node_mut(node);
        let result = if layer < value.layout_indices.len() {
            let index = value.layout_indices[layer] as usize;
            // Out of the chain while it runs, so it can measure the rest of its own node.
            let own = value.chain[index].own;
            let delegating = value.chain[index].state & entry_state::DELEGATING != 0;
            let mut modifier_node = mem::replace(&mut value.chain[index].node, Box::new(Taken));
            let custom = value.chain[index].kinds.any(NodeKinds::OBSERVED);
            let run = self.next_measure_run();
            let observer = if custom { self.observer.clone() } else { None };
            let mut relative = Vec::new();
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut scope = MeasureScope::new(&mut *self, node, run);
                scope.layer = layer;
                let _open = scope.base.tree.open();
                let content = Measurable { node, layer: layer as u32 + 1 };
                let mut measure = || {
                    first_of_kind(&mut *modifier_node, own, NodeKinds::LAYOUT)
                        .and_then(|node| node.as_layout())
                        .expect("compose-ui: a layout node")
                        .measure(&mut scope, content, constraints)
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
            self.node_mut(node).chain[index].node = modifier_node;
            if delegating {
                self.note_delegates_changed(node, index);
            }
            let result = self.recover(node, result);
            self.mirror(relative, result.width);
            result
        } else {
            let policy = value.policy.clone();
            let children = mem::take(&mut value.children);
            // Deactivated children (content kept for reuse) aren't measured, so not placed: at
            // the end, as a lazy list keeps them, the others are a prefix; anywhere else, a copy.
            let mut active = children.len();
            let mut scattered = false;
            for (index, child) in children.iter().enumerate() {
                let child = self.node_mut(child.node);
                child.placed = false;
                child.measured_in_block = false;
                if child.deactivated {
                    active = active.min(index);
                } else if active < index {
                    scattered = true;
                }
            }
            let filtered: Vec<Measurable> = if scattered {
                (children.iter())
                    .filter(|child| !self.node(child.node).deactivated)
                    .copied()
                    .collect()
            } else {
                Vec::new()
            };
            let measurables: &[Measurable] =
                if scattered { &filtered } else { &children[..active] };
            let run = self.next_measure_run();
            let observer = self.observer.clone();
            let mut relative = Vec::new();
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut scope = MeasureScope::new(&mut *self, node, run);
                scope.layer = layer;
                let _open = scope.base.tree.open();
                let result = match &observer {
                    Some(observer) => observer
                        .observe(observation_key(node, Observation::Measure), || {
                            policy.measure(&mut scope, measurables, constraints)
                        }),
                    None => policy.measure(&mut scope, measurables, constraints),
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
        let value = self.node_mut(node);
        if value.placements.is_some() {
            self.rulers_measured(node, layer, earlier_rulers);
        }
        let value = self.node_mut(node);
        let layout_aware = value.kinds.any(NodeKinds::LAYOUT_AWARE);
        let value = &mut value.layers[layer];
        let resized = value.measured != measured;
        value.measured = measured;
        value.size = size;
        // A new measured size within the same coerced one moves the centering.
        let offset = value.position + centering(value);
        if value.offset != offset {
            value.offset = offset;
            self.invalidate_position(node, layer);
        }
        if resized {
            self.measure_result_changed(node, layer);
        }
        if layout_aware {
            self.layer_measured(node, layer, measured);
        }
        size
    }

    /// Layer `layer` of `node`, which has layout-aware nodes, was measured at `size`: its nodes
    /// are told, without observing what they read, and will be told it was placed
    /// (`NodeCoordinator.onMeasured`, `onPlaced`).
    #[cold]
    fn layer_measured(&mut self, node: NodeId, layer: usize, size: IntSize) {
        if self.node(node).kinds.any(NodeKinds::ON_PLACED) {
            self.placed_layers.push((node, layer as u32));
        }
        if !self.node(node).kinds.any(NodeKinds::ON_REMEASURED) {
            return;
        }
        for index in self.node(node).entries_of_layer(layer) {
            let entry = &self.node(node).chain[index];
            if !entry.kinds.any(NodeKinds::ON_REMEASURED)
                || entry.state & entry_state::ATTACHED == 0
            {
                continue;
            }
            let own = entry.own;
            let waited = mem::replace(&mut self.delegates_wait, true);
            self.with_entry(node, index, |modifier_node, context| {
                compose_runtime::snapshots::Snapshot::without_read_observation(|| {
                    let _ = dispatch_with_context(
                        modifier_node,
                        own,
                        NodeKinds::ON_REMEASURED,
                        false,
                        context,
                        &mut |modifier_node, context| {
                            if let Some(aware) = modifier_node.as_layout_aware() {
                                aware.on_remeasured(size, context);
                            } else if let Some(aware) = modifier_node.as_measured_size_aware() {
                                aware.on_remeasured(size, context);
                            }
                            ControlFlow::Continue(())
                        },
                    );
                });
            });
            self.delegates_wait = waited;
        }
    }

    /// Layer `layer` of `node` was measured at a new size: its draw nodes are told
    /// (`DrawModifierNode::on_measure_result_changed`), as Compose's coordinator tells those it
    /// draws (`onMeasureResultChanged`). The built-in elements' nodes have nothing to hear.
    fn measure_result_changed(&mut self, node: NodeId, layer: usize) {
        let value = self.node(node);
        if !value.kinds.any(NodeKinds::DRAW) || !value.kinds.any(NodeKinds::OBSERVED) {
            return;
        }
        // The entries drawing in the layer: those after the layout node measuring the layer
        // outside it, up to its own (compose-rs draws a node in the layer its own layout node
        // measures, `LayoutNode::layer_of`).
        for index in value.entries_of_layer(layer) {
            let Some(entry) = self.node(node).chain.get(index) else { return };
            let hears = NodeKinds::DRAW | NodeKinds::OBSERVED;
            if entry.kinds.all(hears) && entry.state & entry_state::ATTACHED != 0 {
                let own = entry.own;
                let waited = mem::replace(&mut self.delegates_wait, true);
                self.with_entry(node, index, |modifier_node, context| {
                    let _ = dispatch_with_context(
                        modifier_node,
                        own,
                        NodeKinds::DRAW,
                        false,
                        context,
                        &mut |modifier_node, context| {
                            if let Some(draw) = modifier_node.as_draw() {
                                draw.on_measure_result_changed(context);
                            }
                            ControlFlow::Continue(())
                        },
                    );
                });
                self.delegates_wait = waited;
            }
        }
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
        // A layout of no width mirrors nothing, as `placeAutoMirrored`.
        if width == 0 {
            return;
        }
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

    pub(crate) fn next_measure_run(&mut self) -> u64 {
        self.measure_runs += 1;
        self.measure_runs
    }

    /// Places layer `layer` of `node` at `position` in the layer outside it (or its parent's
    /// content). A layer measured outside its constraints is centered in the size its parent
    /// sees, as Compose does.
    pub(crate) fn place(&mut self, node: NodeId, layer: usize, position: IntOffset) {
        self.place_in_run(node, layer, position, 0, false);
    }

    /// `place` by the measurement or placement block `run` (0: by none, as the root is), which
    /// may place a layer once, as Compose's.
    pub(crate) fn place_in_run(
        &mut self,
        node: NodeId,
        layer: usize,
        position: IntOffset,
        run: u64,
        motion: bool,
    ) {
        let value = self.node_mut(node);
        if (motion | (value.motion_layers != 0)) && value.placed_in_motion(layer) != motion {
            value.note_motion(layer, motion);
        }
        let told = value.kinds.any(NodeKinds::ON_PLACED);
        let waiting = value.needs_placement || value.subtree_needs_placement;
        if layer == 0 {
            // A measurement or block clears its children's before it runs.
            assert!(run == 0 || !value.placed, "compose-ui: a child was placed twice");
        } else {
            assert!(run == 0 || value.content_placed_in != run, "compose-ui: placed twice");
            value.content_placed_in = run;
        }
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
        if told {
            self.placed_layers.push((node, layer as u32));
        }
        if waiting {
            self.placed_with_blocks_waiting(node);
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
