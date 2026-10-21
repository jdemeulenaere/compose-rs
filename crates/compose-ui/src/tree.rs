//! The UI tree: `LayoutNode`s in an arena behind generational `NodeId`s, and the applier the
//! composition builds it through (docs/06-ui.md, "Nodes").

use std::fmt::{self, Write as _};
use std::rc::Rc;

use compose_runtime::{Applier, ComposeArg, NodeRef};
use smallvec::SmallVec;

use crate::display_list::DisplayList;
use crate::layout::{Measurable, MeasurePolicy, StackMeasurePolicy};
use crate::modifier::{DetachContext, Element, Modifier, ModifierNode, ParentData};
use crate::nodes;
use crate::text::TextMeasurer;
use crate::unit::{Density, IntOffset, IntSize, Offset};

/// What an event handler reaches besides its node: the text measurer and the clipboard.
pub(crate) type HandlerServices<'a> =
    (&'a mut dyn TextMeasurer, &'a mut dyn crate::clipboard::Clipboard);

/// A node of the UI tree: a generational index, so a stale id never reaches a new node.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

impl NodeId {
    fn to_ref(self) -> NodeRef {
        NodeRef::new((self.generation as u64) << 32 | self.index as u64)
    }

    fn from_ref(node: NodeRef) -> NodeId {
        NodeId { index: node.bits() as u32, generation: (node.bits() >> 32) as u32 }
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#{}.{}", self.index, self.generation)
    }
}

/// What a chain entry can do, cached when the chain changes.
pub(crate) mod capability {
    pub const LAYOUT: u8 = 1;
    pub const DRAW: u8 = 2;
    pub const POINTER: u8 = 4;
    pub const SEMANTICS: u8 = 8;
    pub const FOCUS: u8 = 16;
    pub const KEY: u8 = 32;
    pub const FOCUS_EVENT: u8 = 64;
}

fn capabilities_of(node: &mut dyn ModifierNode) -> u8 {
    use capability::*;
    let mut bits = 0;
    if node.as_layout().is_some() {
        bits |= LAYOUT;
    }
    if node.as_draw().is_some() {
        bits |= DRAW;
    }
    if node.as_pointer_input().is_some() {
        bits |= POINTER;
    }
    if node.as_semantics().is_some() {
        bits |= SEMANTICS;
    }
    if node.as_focus_target().is_some() {
        bits |= FOCUS;
    }
    if node.as_key_input().is_some() {
        bits |= KEY;
    }
    if node.downcast_ref::<nodes::OnFocusChangedNode>().is_some() {
        bits |= FOCUS_EVENT;
    }
    bits
}

/// A measured layer of a node: one per layout modifier, outermost first, then the node's own
/// content (its policy and children). Each layer's offset is relative to the layer outside it;
/// the first layer's is the node's position in its parent's content.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Layer {
    /// The size the layer measured itself, before coercion into its constraints.
    pub(crate) measured: IntSize,
    /// The size its parent sees: `measured` within the constraints.
    pub(crate) size: IntSize,
    /// Where the layer outside placed it.
    pub(crate) position: IntOffset,
    /// `position`, plus the centering of a layer measured outside its constraints.
    pub(crate) offset: IntOffset,
    /// The measurement that last measured it, to reject measuring it twice.
    pub(crate) measured_in: u64,
}

/// One UI element, as Compose's `LayoutNode`.
pub struct LayoutNode {
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<Measurable>,
    pub(crate) policy: Rc<dyn MeasurePolicy>,
    pub(crate) modifier: Modifier,
    pub(crate) chain: Vec<Box<dyn ModifierNode>>,
    pub(crate) capabilities: SmallVec<[u8; 16]>,
    pub(crate) all_capabilities: u8,
    /// The chain indices of the layout modifier nodes, outermost first.
    pub(crate) layout_indices: SmallVec<[u16; 4]>,
    /// Bumped when the chain's nodes change, so pointer captures of removed nodes lapse.
    pub(crate) chain_version: u32,
    pub(crate) parent_data: ParentData,
    pub(crate) layers: SmallVec<[Layer; 2]>,
    /// The constraints of the last measurement.
    pub(crate) constraints: Option<crate::constraints::Constraints>,
    pub(crate) needs_measure: bool,
    /// Some descendant needs measuring.
    pub(crate) subtree_needs_measure: bool,
    /// Whether the parent placed it in its last measurement: unplaced nodes aren't drawn.
    pub(crate) placed: bool,
    /// The parent measurement that last measured it, to reject measuring it twice.
    pub(crate) measured_in: u64,
}

impl LayoutNode {
    pub fn new(policy: Rc<dyn MeasurePolicy>) -> LayoutNode {
        LayoutNode {
            parent: None,
            children: Vec::new(),
            policy,
            modifier: Modifier,
            chain: Vec::new(),
            capabilities: SmallVec::new(),
            all_capabilities: 0,
            layout_indices: SmallVec::new(),
            chain_version: 0,
            parent_data: ParentData::default(),
            layers: SmallVec::from_elem(Layer::default(), 1),
            constraints: None,
            needs_measure: true,
            subtree_needs_measure: false,
            placed: false,
            measured_in: 0,
        }
    }

    /// Its measured size (which its parent sees coerced into its constraints).
    pub fn measured_size(&self) -> IntSize {
        self.layers[0].measured
    }

    pub fn modifier(&self) -> &Modifier {
        &self.modifier
    }
}

struct Slot {
    generation: u32,
    node: Option<LayoutNode>,
}

/// The tree of layout nodes, and everything that runs on it: measurement, drawing, input,
/// focus and semantics. It is the composition's applier.
pub struct UiTree {
    slots: Vec<Slot>,
    free: Vec<u32>,
    root: NodeId,
    pub(crate) text_measurer: Box<dyn TextMeasurer>,
    pub(crate) clipboard: Box<dyn crate::clipboard::Clipboard>,
    pub(crate) density: Density,
    pub(crate) display_list: DisplayList,
    pub(crate) needs_draw: bool,
    pub(crate) focus: Option<crate::focus::Focus>,
    pub(crate) focus_listeners_dirty: bool,
    /// Actions queued outside dispatch, run by the host after the frame.
    pub(crate) pending_actions: Vec<crate::input::Action>,
    pub(crate) captures: Vec<(crate::input::PointerId, Vec<crate::dispatch::HitEntry>)>,
    pub(crate) measure_runs: u64,
}

impl UiTree {
    pub fn new(density: Density, text_measurer: Box<dyn TextMeasurer>) -> UiTree {
        let mut root = LayoutNode::new(Rc::new(StackMeasurePolicy));
        root.placed = true;
        UiTree {
            slots: vec![Slot { generation: 0, node: Some(root) }],
            free: Vec::new(),
            root: NodeId { index: 0, generation: 0 },
            text_measurer,
            clipboard: Box::new(crate::clipboard::MemoryClipboard::default()),
            density,
            display_list: DisplayList::default(),
            needs_draw: true,
            focus: None,
            focus_listeners_dirty: false,
            pending_actions: Vec::new(),
            captures: Vec::new(),
            measure_runs: 0,
        }
    }

    pub fn root_id(&self) -> NodeId {
        self.root
    }

    pub fn density(&self) -> Density {
        self.density
    }

    /// Changes the density: everything is measured again.
    pub fn set_density(&mut self, density: Density) {
        if self.density != density {
            self.density = density;
            for index in 0..self.slots.len() {
                if let Some(node) = &mut self.slots[index].node {
                    node.needs_measure = true;
                    node.subtree_needs_measure = true;
                }
            }
            self.needs_draw = true;
        }
    }

    pub fn text_measurer(&mut self) -> &mut dyn TextMeasurer {
        &mut *self.text_measurer
    }

    /// The display list of the last draw.
    pub fn display_list(&self) -> &DisplayList {
        &self.display_list
    }

    /// The node `node` refers to, if it's alive.
    pub fn get(&self, node: NodeId) -> Option<&LayoutNode> {
        let slot = self.slots.get(node.index as usize)?;
        if slot.generation == node.generation { slot.node.as_ref() } else { None }
    }

    pub(crate) fn get_mut(&mut self, node: NodeId) -> Option<&mut LayoutNode> {
        let slot = self.slots.get_mut(node.index as usize)?;
        if slot.generation == node.generation { slot.node.as_mut() } else { None }
    }

    /// A node, the text measurer and the clipboard, borrowed together: for event handlers.
    pub(crate) fn node_and_measurer(
        &mut self,
        node: NodeId,
    ) -> Option<(&mut LayoutNode, HandlerServices<'_>)> {
        let slot = self.slots.get_mut(node.index as usize)?;
        if slot.generation != node.generation {
            return None;
        }
        Some((slot.node.as_mut()?, (&mut *self.text_measurer, &mut *self.clipboard)))
    }

    /// Replaces the clipboard (the host gives the system's).
    pub fn set_clipboard(&mut self, clipboard: Box<dyn crate::clipboard::Clipboard>) {
        self.clipboard = clipboard;
    }

    pub fn clipboard(&mut self) -> &mut dyn crate::clipboard::Clipboard {
        &mut *self.clipboard
    }

    pub(crate) fn node(&self, node: NodeId) -> &LayoutNode {
        self.get(node).unwrap_or_else(|| panic!("compose-ui: node {node:?} is not alive"))
    }

    pub(crate) fn node_mut(&mut self, node: NodeId) -> &mut LayoutNode {
        self.get_mut(node).unwrap_or_else(|| panic!("compose-ui: node {node:?} is not alive"))
    }

    /// The id of a node the composition emitted.
    pub fn id_of(&self, node: NodeRef) -> NodeId {
        NodeId::from_ref(node)
    }

    pub fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        self.get(node)?.parent
    }

    /// The children of `node`, in order.
    pub fn children_of(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.get(node).into_iter().flat_map(|node| node.children.iter().map(|child| child.node))
    }

    /// The number of live nodes, the root included.
    pub fn node_count(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    // ---- Updates from composition -----------------------------------------------------------

    /// Marks `node` for measuring, and its ancestors as having a descendant to measure.
    pub fn invalidate_measure(&mut self, node: NodeId) {
        self.needs_draw = true;
        let Some(value) = self.get_mut(node) else { return };
        value.needs_measure = true;
        let mut parent = value.parent;
        while let Some(id) = parent {
            let value = self.node_mut(id);
            value.subtree_needs_measure = true;
            parent = value.parent;
        }
    }

    pub fn invalidate_draw(&mut self) {
        self.needs_draw = true;
    }

    pub fn set_measure_policy(&mut self, node: NodeId, policy: Rc<dyn MeasurePolicy>) {
        let value = self.node_mut(node);
        if !Rc::ptr_eq(&value.policy, &policy) {
            let old = std::mem::replace(&mut value.policy, policy);
            self.invalidate_measure(node);
            drop(old);
        }
    }

    /// Sets the node's modifier, updating its node chain element by element: an old element
    /// of the same kind as a new one, matched in order (a longest common subsequence of kinds,
    /// as Compose's chain diff), updates its node in place, keeping its state; the others'
    /// nodes are created or detached.
    pub fn set_modifier(&mut self, node: NodeId, modifier: Modifier) {
        let value = self.node_mut(node);
        if value.modifier.same(&modifier) {
            return;
        }
        let old = value.modifier.elements();
        let new = modifier.elements();
        // The common case, the same kinds in the same order: update in place, allocating nothing.
        if old.len() == new.len() && old.iter().zip(new).all(|(old, new)| old.same_kind(new)) {
            for (index, (old, new)) in old.iter().zip(new).enumerate() {
                if !old.same(new) {
                    nodes::update(new, &mut *value.chain[index]);
                }
            }
            let previous = std::mem::replace(&mut value.modifier, modifier);
            let parent_data_changed = self.refresh_chain(node);
            drop(previous);
            self.revalidate_focus(node, None);
            self.invalidate_measure(node);
            if parent_data_changed && let Some(parent) = self.parent_of(node) {
                self.invalidate_measure(parent);
            }
            return;
        }
        let matched = match_kinds(old, new);
        let mut old_nodes: SmallVec<[Option<Box<dyn ModifierNode>>; 4]> =
            std::mem::take(&mut value.chain).into_iter().map(Some).collect();
        let mut chain = Vec::with_capacity(new.len());
        let mut reused = 0;
        for (index, element) in new.iter().enumerate() {
            match matched[index] {
                Some(old_index) => {
                    let mut entry = old_nodes[old_index].take().expect("matched once");
                    if !old[old_index].same(element) {
                        nodes::update(element, &mut *entry);
                    }
                    chain.push(entry);
                    reused += 1;
                }
                None => chain.push(nodes::create(element)),
            }
        }
        let changed = reused != old.len() || reused != new.len();
        if changed {
            value.chain_version = value.chain_version.wrapping_add(1);
        }
        value.chain = chain;
        value.modifier = modifier;
        let parent_data_changed = self.refresh_chain(node);
        if changed {
            self.remap_captures(node, &matched);
        }
        let mut context = DetachContext { text_measurer: &mut *self.text_measurer };
        for mut removed in old_nodes.into_iter().flatten() {
            removed.on_detach(&mut context);
        }
        self.revalidate_focus(node, Some(&matched));
        self.invalidate_measure(node);
        if parent_data_changed && let Some(parent) = self.parent_of(node) {
            self.invalidate_measure(parent);
        }
    }

    /// Points the pointer captures of `node`'s modifier nodes at their new places in its
    /// chain, so a gesture in progress continues on a node that was kept; captures of removed
    /// nodes are dropped.
    fn remap_captures(&mut self, node: NodeId, matched: &[Option<usize>]) {
        if self.captures.is_empty() {
            return;
        }
        let value = self.node(node);
        let (version, capabilities) = (value.chain_version, value.capabilities.clone());
        let layer_of = |index: usize| {
            capabilities[..index].iter().filter(|bits| **bits & capability::LAYOUT != 0).count()
                as u16
        };
        for (_, path) in &mut self.captures {
            path.retain_mut(|entry| {
                if entry.node != node {
                    return true;
                }
                let Some(new_index) =
                    matched.iter().position(|old| *old == Some(entry.index as usize))
                else {
                    return false;
                };
                entry.index = new_index as u16;
                entry.layer = layer_of(new_index);
                entry.version = version;
                true
            });
        }
    }

    /// Recomputes what the chain's nodes can do, its layers and its parent data. Returns
    /// whether the parent data changed.
    fn refresh_chain(&mut self, node: NodeId) -> bool {
        let value = self.node_mut(node);
        value.capabilities.clear();
        value.layout_indices.clear();
        value.all_capabilities = 0;
        let mut parent_data = ParentData::default();
        for (index, entry) in value.chain.iter_mut().enumerate() {
            let bits = capabilities_of(&mut **entry);
            value.capabilities.push(bits);
            value.all_capabilities |= bits;
            if bits & capability::LAYOUT != 0 {
                value.layout_indices.push(index as u16);
            }
        }
        for element in value.modifier.elements() {
            match element {
                // The outermost wins, as Compose's parent data chain folds from the outside.
                Element::Weight(weight, fill) if parent_data.weight.is_none() => {
                    parent_data.weight = Some((*weight, *fill))
                }
                Element::Align(alignment) if parent_data.alignment.is_none() => {
                    parent_data.alignment = Some(*alignment)
                }
                _ => {}
            }
        }
        value.layers.resize(value.layout_indices.len() + 1, Layer::default());
        let changed = value.parent_data != parent_data;
        value.parent_data = parent_data;
        // A new listener reports its initial state.
        let listens = value.all_capabilities & capability::FOCUS_EVENT != 0;
        self.focus_listeners_dirty |= listens;
        changed
    }

    // ---- Dumps ------------------------------------------------------------------------------

    /// An indented dump of the tree: each node's bounds in its parent, policy, modifier and
    /// own semantics.
    pub fn dump(&mut self) -> String {
        let mut out = String::new();
        let mut stack = vec![(self.root, 0usize)];
        while let Some((id, depth)) = stack.pop() {
            let config = self.own_semantics(id).map(|(config, _)| config);
            let node = self.node(id);
            let position = node.layers[0].offset;
            let size = node.layers[0].measured;
            let _ = write!(
                out,
                "{}{} [{},{} {}x{}]",
                "  ".repeat(depth),
                node.policy.debug_name(),
                position.x,
                position.y,
                size.width,
                size.height
            );
            if !node.placed {
                out.push_str(" (not placed)");
            }
            if !node.modifier.is_empty() {
                let _ = write!(out, " {:?}", node.modifier);
            }
            if let Some(config) = config.filter(|config| !config.is_empty()) {
                let _ = write!(out, " {{{config:?}}}");
            }
            out.push('\n');
            stack.extend(node.children.iter().rev().map(|child| (child.node, depth + 1)));
        }
        out
    }

    // ---- Coordinates ------------------------------------------------------------------------

    /// The position of layer `layer` of `node` in the root, in pixels.
    pub(crate) fn layer_origin(&self, node: NodeId, layer: usize) -> Offset {
        let value = self.node(node);
        let mut origin = match value.parent {
            Some(parent) => self.layer_origin(parent, self.node(parent).layers.len() - 1),
            None => Offset::ZERO,
        };
        for layer in &value.layers[..=layer] {
            origin = origin + Offset::new(layer.offset.x as f32, layer.offset.y as f32);
        }
        origin
    }

    /// The bounds of `node` in the root, in pixels.
    pub fn bounds_in_root(&self, node: NodeId) -> crate::unit::Rect {
        let origin = self.layer_origin(node, 0);
        let size = self.node(node).layers[0].measured;
        crate::unit::Rect::from_offset_size(origin, size.into())
    }

    // ---- Releasing --------------------------------------------------------------------------

    fn alloc(&mut self, node: LayoutNode) -> NodeId {
        match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                slot.node = Some(node);
                NodeId { index, generation: slot.generation }
            }
            None => {
                self.slots.push(Slot { generation: 0, node: Some(node) });
                NodeId { index: self.slots.len() as u32 - 1, generation: 0 }
            }
        }
    }

    /// Frees `node` and its subtree, detaching their modifier nodes.
    fn free_subtree(&mut self, node: NodeId) {
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            let slot = &mut self.slots[id.index as usize];
            let Some(mut value) = slot.node.take() else { continue };
            slot.generation = slot.generation.wrapping_add(1);
            self.free.push(id.index);
            stack.extend(value.children.iter().map(|child| child.node));
            if self.focused() == Some(id) {
                self.focus = None;
                self.focus_listeners_dirty = true;
            }
            let mut context = DetachContext { text_measurer: &mut *self.text_measurer };
            for entry in &mut value.chain {
                entry.on_detach(&mut context);
            }
            drop(value);
        }
    }
}

impl Applier for UiTree {
    type Node = LayoutNode;

    fn root(&self) -> NodeRef {
        self.root.to_ref()
    }

    fn insert(&mut self, parent: NodeRef, index: usize, mut node: LayoutNode) -> NodeRef {
        let parent = NodeId::from_ref(parent);
        node.parent = Some(parent);
        let id = self.alloc(node);
        let length = self.node(parent).children.len();
        assert!(index <= length, "compose-ui: insert at {index} past {length} children");
        self.node_mut(parent).children.insert(index, Measurable { node: id, layer: 0 });
        self.refresh_chain(id);
        self.invalidate_measure(parent);
        id.to_ref()
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        let parent = NodeId::from_ref(parent);
        let removed: SmallVec<[Measurable; 8]> =
            self.node_mut(parent).children.drain(index..index + count).collect();
        for child in removed {
            if let Some(child) = self.get_mut(child.node) {
                child.parent = None;
                child.placed = false;
            }
        }
        self.invalidate_measure(parent);
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        let parent = NodeId::from_ref(parent);
        let children = &mut self.node_mut(parent).children;
        let destination = if from > to { to } else { to - count };
        assert!(destination + count <= children.len(), "compose-ui: move past the end");
        if destination < from {
            children[destination..from + count].rotate_right(count);
        } else if destination > from {
            children[from..destination + count].rotate_left(count);
        }
        self.invalidate_measure(parent);
    }

    fn release(&mut self, node: NodeRef) {
        let node = NodeId::from_ref(node);
        let Some(value) = self.get(node) else { return };
        if let Some(parent) = value.parent {
            // A node of a poisoned composition, still attached.
            if let Some(parent_value) = self.get_mut(parent) {
                parent_value.children.retain(|child| child.node != node);
            }
            self.invalidate_measure(parent);
        }
        self.free_subtree(node);
        self.needs_draw = true;
    }
}

/// For each new element, the old element of the same kind it reuses: a longest common
/// subsequence of kinds, so nodes keep their state when elements are added or removed around
/// them. Equal prefixes and suffixes are matched without the table.
fn match_kinds(old: &[Element], new: &[Element]) -> SmallVec<[Option<usize>; 8]> {
    let mut matched: SmallVec<[Option<usize>; 8]> = SmallVec::from_elem(None, new.len());
    let shortest = old.len().min(new.len());
    let prefix = (0..shortest).take_while(|&i| old[i].same_kind(&new[i])).count();
    let suffix = (0..shortest - prefix)
        .take_while(|&i| old[old.len() - 1 - i].same_kind(&new[new.len() - 1 - i]))
        .count();
    for i in 0..prefix {
        matched[i] = Some(i);
    }
    for i in 0..suffix {
        matched[new.len() - 1 - i] = Some(old.len() - 1 - i);
    }
    let old_middle = &old[prefix..old.len() - suffix];
    let new_middle = &new[prefix..new.len() - suffix];
    if old_middle.is_empty() || new_middle.is_empty() {
        return matched;
    }
    // lengths[i][j]: the LCS of old_middle[i..] and new_middle[j..].
    let (rows, columns) = (old_middle.len() + 1, new_middle.len() + 1);
    let mut lengths = vec![0u16; rows * columns];
    for i in (0..old_middle.len()).rev() {
        for j in (0..new_middle.len()).rev() {
            lengths[i * columns + j] = if old_middle[i].same_kind(&new_middle[j]) {
                lengths[(i + 1) * columns + j + 1] + 1
            } else {
                lengths[(i + 1) * columns + j].max(lengths[i * columns + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < old_middle.len() && j < new_middle.len() {
        if old_middle[i].same_kind(&new_middle[j]) {
            matched[prefix + j] = Some(prefix + i);
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * columns + j] >= lengths[i * columns + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    matched
}

/// The last segment of a type path, without generics: `compose_ui::layout::Stack` → `Stack`.
pub(crate) fn short_type_name(name: &'static str) -> &'static str {
    let name = name.split('<').next().unwrap_or(name);
    name.rsplit("::").next().unwrap_or(name)
}
