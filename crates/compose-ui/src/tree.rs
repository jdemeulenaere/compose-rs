//! The UI tree: `LayoutNode`s in an arena behind generational `NodeId`s, and the applier the
//! composition builds it through (docs/06-ui.md, "Nodes").

use std::fmt::{self, Write as _};
use std::rc::Rc;

use compose_runtime::{
    ComposeArg,
    platform::{Applier, NodeRef},
};
use smallvec::SmallVec;

use crate::display_list::DisplayList;
use crate::layout::{Measurable, MeasurePolicy, StackMeasurePolicy};
use crate::modifier::{DetachContext, Element, Modifier, ModifierNode, ParentData};
use crate::nodes;
use crate::text::TextMeasurer;
use crate::unit::{Density, IntOffset, IntSize, Offset};

/// What a node's observed reads were made for: measuring it, drawing it, or (a graphics
/// layer's block) its layer's properties, which change without drawing it again.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Observation {
    Measure = 0,
    Draw = 1,
    Layer = 2,
}

/// The key `node`'s reads for `observation` are observed under: the kind in the top two bits.
pub(crate) fn observation_key(node: NodeId, observation: Observation) -> u64 {
    node.to_ref().bits() & !(3 << 62) | (observation as u64) << 62
}

/// The node and the kind of observation `observation_key` made `key` of.
fn observation_of(key: u64) -> (NodeId, Observation) {
    let observation = match key >> 62 {
        0 => Observation::Measure,
        1 => Observation::Draw,
        _ => Observation::Layer,
    };
    (NodeId::from_ref(NodeRef::new(key & !(3 << 62))), observation)
}

/// What an event handler reaches besides its node: the text measurer and the clipboard.
pub(crate) type HandlerServices<'a> =
    (&'a mut dyn TextMeasurer, &'a dyn crate::clipboard::ClipboardManager);

/// A node of the UI tree: a generational index, so a stale id never reaches a new node.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

impl NodeId {
    /// The id as one number, for platform bridges (accessibility nodes).
    pub fn to_bits(self) -> u64 {
        (self.generation as u64) << 32 | self.index as u64
    }

    /// The id `to_bits` gave.
    pub fn from_bits(bits: u64) -> NodeId {
        NodeId { index: bits as u32, generation: (bits >> 32) as u32 }
    }

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
    pub(crate) const LAYOUT: u8 = 1;
    pub(crate) const DRAW: u8 = 2;
    pub(crate) const POINTER: u8 = 4;
    pub(crate) const SEMANTICS: u8 = 8;
    pub(crate) const FOCUS: u8 = 16;
    pub(crate) const KEY: u8 = 32;
    pub(crate) const FOCUS_EVENT: u8 = 64;
    /// A user-defined element's node: its measuring and drawing are observed for state reads.
    pub(crate) const CUSTOM: u8 = 128;
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
    /// The layout direction where it was composed (`LOCAL_LAYOUT_DIRECTION`).
    pub(crate) layout_direction: crate::alignment::LayoutDirection,
    /// Its parent asked its intrinsic width in its last measurement: a change inside it
    /// measures the parent again, not just it in place.
    pub(crate) intrinsics_asked: bool,
    /// An overlay (`Modifier.overlay`): placed in the root at `overlay_origin`, not where its
    /// parent placed it; drawn last and hit first.
    pub(crate) overlay: bool,
    pub(crate) overlay_origin: crate::unit::Offset,
    /// When its parent last placed it: children are drawn, and hit, in placement order, as
    /// Compose's (a scaffold's bars over its content).
    pub(crate) place_order: u64,
    /// What it drew last, if it's a repaint boundary (render.rs).
    pub(crate) retained: Option<Box<crate::render::RetainedDrawing>>,
    /// The sum of its `z_index` modifiers: siblings are drawn, and hit, in its order.
    pub(crate) z_index: f32,
    /// Its content is deactivated (a lazy list's item kept for reuse): unplaced, its modifier
    /// nodes reset, left out of the walks that only concern content in use.
    pub(crate) deactivated: bool,
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
            intrinsics_asked: false,
            layout_direction: crate::alignment::LayoutDirection::Ltr,
            overlay: false,
            overlay_origin: crate::unit::Offset::ZERO,
            place_order: 0,
            retained: None,
            z_index: 0.0,
            deactivated: false,
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
    pub(crate) clipboard: Rc<dyn crate::clipboard::ClipboardManager>,
    /// The platform's services content reaches as `LOCAL_URI_HANDLER` and
    /// `LOCAL_HAPTIC_FEEDBACK`.
    pub(crate) uri_handler: Rc<dyn crate::platform_services::UriHandler>,
    pub(crate) platform_uri_handler: crate::platform_services::PlatformUriHandler,
    pub(crate) haptic_feedback: Rc<dyn crate::platform_services::HapticFeedback>,
    pub(crate) density: Density,
    /// The last frame: what's drawn outside any repaint boundary, recorded each draw (kept for
    /// its capacity), and the boundaries' groups, their kept recordings shared.
    pub(crate) display_list: DisplayList,
    /// Something to draw: a boundary to draw again, a layer's properties, or what's outside any
    /// boundary.
    pub(crate) needs_draw: bool,
    /// The last version given to a repaint boundary's operations (`DrawGroup::version`).
    pub(crate) draw_version: u64,
    /// A handler asked for the platform's input method to be reset.
    pub(crate) input_method_reset: bool,
    pub(crate) focus: Option<crate::focus::Focus>,
    pub(crate) focus_listeners_dirty: bool,
    /// Actions queued outside dispatch, run by the host after the frame.
    pub(crate) pending_actions: Vec<crate::input::Action>,
    /// The overlay nodes, in the order they became overlays: later ones over earlier ones (and
    /// siblings in their order, a popup over its outside).
    pub(crate) overlays: Vec<NodeId>,
    /// Overlays that took the focus when they opened, each with what had it before: focused
    /// again once the overlay is gone, as a window regains its focus.
    pub(crate) focus_restore: Vec<(NodeId, Option<NodeId>)>,
    /// The window's size, from the last layout.
    pub(crate) window: IntSize,
    /// Counts placements, for children's placement order.
    pub(crate) place_serial: u64,
    pub(crate) captures: Vec<(crate::input::PointerId, Vec<crate::dispatch::HitEntry>)>,
    /// What the mouse is over: its last hit path.
    pub(crate) hovered: Vec<crate::dispatch::HitEntry>,
    /// Where the mouse is, in the root, while it's in the window and no button is held.
    pub(crate) mouse: Option<crate::unit::Offset>,
    /// Observes the state measure policies and custom nodes read (D71), keyed by node.
    pub(crate) observer: Option<Rc<compose_runtime::platform::ReadObserver>>,
    pub(crate) measure_runs: u64,
    /// What `FocusRequester`s attached to this tree's nodes asked for.
    pub(crate) focus_requests: Rc<crate::focus_requester::FocusRequests>,
    /// The text selected in a `SelectionContainer`, if any.
    pub(crate) text_selection: Option<crate::text_selection::TextSelection>,
    /// A node focused before it was ever placed (a prefetched lazy item): brought into view
    /// once laid out.
    pub(crate) bring_into_view_after_layout: Option<NodeId>,
    /// What layouts queued for application code (`on_size_changed`, `on_globally_positioned`),
    /// run by the host once the layout is over.
    pub(crate) layout_callbacks: Vec<crate::input::Action>,
    /// The nodes with position-aware modifier nodes (`on_globally_positioned`).
    pub(crate) positioned: Vec<NodeId>,
    /// Something was laid out (or a position-aware node came) since the last callbacks.
    pub(crate) laid_out_since_callbacks: bool,
    /// Intrinsic probes in progress (`MeasureScope::max_intrinsic_width` measuring to ask).
    pub(crate) intrinsic_probes: u32,
    /// The window's insets, the host's state: read while measuring (insets paddings), so a
    /// change measures their readers again.
    pub(crate) window_insets:
        Option<compose_runtime::MutableState<crate::configuration::PlatformWindowInsets>>,
    /// Nodes measured ahead of the layout placing them (`premeasure`: a lazy list's prefetched
    /// items), oldest first: the repaint boundaries among them are drawn ahead too
    /// (`draw_ahead`), so the frame placing them finds them drawn.
    pub(crate) measured_ahead: Vec<NodeId>,
    /// Repaint boundaries a frame drew that are composed and not placed now (a lazy list's
    /// items kept beside the wanted ones), oldest first, `KEPT_CAPACITY` at most: each frame
    /// lists their groups (`DisplayList::kept`) while they stay so, unchanged (D380).
    pub(crate) kept: Vec<NodeId>,
    /// Frames list the kept boundaries' groups (`UiHost::set_keep_unplaced_groups`).
    pub(crate) keeps_unplaced: bool,
    /// The nodes reused in the pass under way, whose modifier nodes are reset when it ends
    /// (`run_resets`); kept for its capacity.
    pending_resets: Vec<NodeId>,
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
            clipboard: Rc::new(crate::clipboard::MemoryClipboard::default()),
            uri_handler: Rc::new(crate::platform_services::PlatformUriHandler::headless()),
            platform_uri_handler: crate::platform_services::PlatformUriHandler::headless(),
            haptic_feedback: Rc::new(crate::platform_services::NoHapticFeedback),
            density,
            display_list: DisplayList::default(),
            needs_draw: true,
            draw_version: 0,
            input_method_reset: false,
            focus: None,
            focus_listeners_dirty: false,
            pending_actions: Vec::new(),
            overlays: Vec::new(),
            focus_restore: Vec::new(),
            window: IntSize::new(0, 0),
            place_serial: 0,
            captures: Vec::new(),
            hovered: Vec::new(),
            mouse: None,
            observer: None,
            measure_runs: 0,
            focus_requests: Rc::default(),
            text_selection: None,
            bring_into_view_after_layout: None,
            layout_callbacks: Vec::new(),
            positioned: Vec::new(),
            laid_out_since_callbacks: false,
            intrinsic_probes: 0,
            window_insets: None,
            measured_ahead: Vec::new(),
            kept: Vec::new(),
            keeps_unplaced: false,
            pending_resets: Vec::new(),
        }
    }

    /// The host's window insets state, which `MeasureScope::window_insets` reads.
    pub fn set_window_insets_state(
        &mut self,
        state: compose_runtime::MutableState<crate::configuration::PlatformWindowInsets>,
    ) {
        self.window_insets = Some(state);
    }

    /// `node`'s placed children's indices in drawing order, if it isn't their order (`None` when
    /// it is, the usual case, which allocates nothing): by `z_index`, then in the order their
    /// parent placed them, as Compose's. The children not placed, which nothing draws or hits (a
    /// lazy list's items kept for reuse, or beyond the wanted ones), their placement stale, are
    /// left out: with `None`, the caller skips them.
    pub(crate) fn placement_order(&self, node: NodeId) -> Option<SmallVec<[u32; 8]>> {
        let children = &self.node(node).children;
        let order_of = |child: &Measurable| {
            let value = self.node(child.node);
            value.placed.then_some((value.z_index, value.place_order))
        };
        let before = |a: (f32, u64), b: (f32, u64)| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1));
        // In order, the usual case: one look at each child, nothing allocated.
        let mut last = None;
        let in_order = children.iter().filter_map(order_of).all(|order| {
            let in_order = last.is_none_or(|last| before(last, order).is_le());
            last = Some(order);
            in_order
        });
        if in_order {
            return None;
        }
        let mut placed: SmallVec<[(f32, u64, u32); 8]> = (children.iter().enumerate())
            .filter_map(|(index, child)| order_of(child).map(|(z, at)| (z, at, index as u32)))
            .collect();
        placed.sort_by(|a, b| before((a.0, a.1), (b.0, b.1)));
        Some(placed.into_iter().map(|(_, _, index)| index).collect())
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
            self.invalidate_draw();
        }
    }

    pub fn text_measurer(&mut self) -> &mut dyn TextMeasurer {
        &mut *self.text_measurer
    }

    /// The text measurer, shared: to render the display list with (`display_list()`) without
    /// copying it.
    pub fn text_measurer_ref(&self) -> &dyn TextMeasurer {
        &*self.text_measurer
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
        Some((slot.node.as_mut()?, (&mut *self.text_measurer, &*self.clipboard)))
    }

    /// Replaces the clipboard (the host gives the system's): what text fields use, and
    /// `LOCAL_CLIPBOARD_MANAGER`. Before composing.
    pub fn set_clipboard(&mut self, clipboard: Rc<dyn crate::clipboard::ClipboardManager>) {
        self.clipboard = clipboard;
    }

    pub fn clipboard(&self) -> Rc<dyn crate::clipboard::ClipboardManager> {
        self.clipboard.clone()
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
        // Deactivated content (kept for reuse) isn't measured: reused, it is.
        if self.get(node).is_none_or(|value| value.deactivated) {
            return;
        }
        self.invalidate_draw_of(node);
        let Some(value) = self.get_mut(node) else { return };
        value.needs_measure = true;
        let mut parent = value.parent;
        while let Some(id) = parent {
            let value = self.node_mut(id);
            value.subtree_needs_measure = true;
            parent = value.parent;
        }
    }

    /// Draws everything again: every repaint boundary too.
    pub fn invalidate_draw(&mut self) {
        self.needs_draw = true;
        for slot in &mut self.slots {
            if let Some(retained) = slot.node.as_mut().and_then(|node| node.retained.as_mut()) {
                retained.dirty = true;
                retained.layer_dirty = true;
            }
        }
    }

    /// Draws again what `node` draws in: the nearest repaint boundary at or above it, if any
    /// (what's outside every boundary is drawn each draw). An overlay is drawn at the root,
    /// outside its ancestors.
    pub(crate) fn invalidate_draw_of(&mut self, node: NodeId) {
        let mut current = Some(node);
        while let Some(id) = current {
            let Some(value) = self.get_mut(id) else { break };
            if let Some(retained) = &mut value.retained {
                retained.dirty = true;
                // A boundary nothing places (a lazy list's item prefetched or kept for reuse)
                // is drawn when a frame places it: nothing to draw now (D435).
                if !value.placed {
                    return;
                }
                break;
            }
            if value.overlay {
                break;
            }
            current = value.parent;
        }
        self.needs_draw = true;
    }

    /// Layer `layer` of `node` moved: draws again what draws the move, its parent for its
    /// position (layer 0), itself for an inner layer. A boundary's own position is its
    /// parent's: moving it keeps what it drew.
    pub(crate) fn invalidate_position(&mut self, node: NodeId, layer: usize) {
        if layer > 0 {
            self.invalidate_draw_of(node);
            return;
        }
        self.needs_draw = true;
        if let Some(value) = self.get(node)
            && !value.overlay
            && let Some(parent) = value.parent
        {
            self.invalidate_draw_of(parent);
        }
    }

    /// Observes the state policies and custom nodes read while measuring and drawing, so a
    /// write measures or draws them again (D71). `observer` calls back when one happens.
    pub fn set_read_observer(&mut self, observer: compose_runtime::platform::ReadObserver) {
        self.observer = Some(Rc::new(observer));
    }

    /// Measures and draws again what observed writes invalidated.
    pub fn apply_observed_writes(&mut self) {
        let Some(observer) = self.observer.clone() else { return };
        observer.drain_invalidated(|key| {
            let (node, observation) = observation_of(key);
            match observation {
                Observation::Measure => self.invalidate_measure(node),
                Observation::Draw if self.get(node).is_some_and(|value| !value.deactivated) => {
                    self.invalidate_draw_of(node)
                }
                Observation::Draw => {}
                Observation::Layer => {
                    // Its layer's properties only: its content and version stay.
                    self.needs_draw = true;
                    let value = self.get_mut(node);
                    if let Some(retained) = value.and_then(|value| value.retained.as_mut()) {
                        retained.layer_dirty = true;
                    }
                }
            }
        });
    }

    /// Sets the node's layout direction, measuring it again if it changed.
    pub fn set_layout_direction(
        &mut self,
        node: NodeId,
        direction: crate::alignment::LayoutDirection,
    ) {
        if self.node(node).layout_direction != direction {
            self.node_mut(node).layout_direction = direction;
            self.invalidate_measure(node);
        }
    }

    /// The node's layout direction.
    pub fn layout_direction(&self, node: NodeId) -> crate::alignment::LayoutDirection {
        self.node(node).layout_direction
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
        if self.captures.is_empty() && self.hovered.is_empty() {
            return;
        }
        let value = self.node(node);
        let (version, capabilities) = (value.chain_version, value.capabilities.clone());
        let layer_of = |index: usize| {
            capabilities[..index].iter().filter(|bits| **bits & capability::LAYOUT != 0).count()
                as u16
        };
        let remap = |entry: &mut crate::dispatch::HitEntry| {
            if entry.node != node {
                return true;
            }
            let Some(new_index) = matched.iter().position(|old| *old == Some(entry.index as usize))
            else {
                return false;
            };
            entry.index = new_index as u16;
            entry.layer = layer_of(new_index);
            entry.version = version;
            true
        };
        for (_, path) in &mut self.captures {
            path.retain_mut(remap);
        }
        // The hover too: what's hovered hears its exit after a chain change.
        self.hovered.retain_mut(remap);
    }

    /// Recomputes what the chain's nodes can do, its layers and its parent data. Returns
    /// whether the parent data changed.
    fn refresh_chain(&mut self, node: NodeId) -> bool {
        let focus_requests = self.focus_requests.clone();
        let value = self.node_mut(node);
        value.capabilities.clear();
        value.layout_indices.clear();
        value.all_capabilities = 0;
        let mut parent_data = ParentData::default();
        for (index, entry) in value.chain.iter_mut().enumerate() {
            if let Some(requester) =
                entry.downcast_mut::<crate::focus_requester::FocusRequesterNode>()
            {
                requester.attach(&focus_requests, node);
            }
            let custom = matches!(value.modifier.elements().get(index), Some(Element::Custom(_)));
            let bits = capabilities_of(&mut **entry) | if custom { capability::CUSTOM } else { 0 };
            value.capabilities.push(bits);
            value.all_capabilities |= bits;
            if bits & capability::LAYOUT != 0 {
                value.layout_indices.push(index as u16);
            }
        }
        // A repaint boundary keeps what it draws: its layer's properties apart if its first
        // drawing modifier is a graphics layer.
        let boundary = value.chain.iter().any(|entry| entry.is_repaint_boundary());
        let layer_entry = value
            .chain
            .iter()
            .position(|entry| {
                entry.downcast_ref::<crate::graphics_layer::GraphicsLayerNode>().is_some()
            })
            .filter(|&index| {
                value.capabilities[..index].iter().all(|bits| bits & capability::DRAW == 0)
            })
            .map(|index| index as u16);
        let mut parent_draws_differently = boundary != value.retained.is_some();
        match &mut value.retained {
            Some(retained) if boundary => {
                retained.layer_entry = layer_entry;
                retained.dirty = true;
                retained.layer_dirty = true;
            }
            _ if boundary => {
                value.retained = Some(Box::new(crate::render::RetainedDrawing::new(layer_entry)))
            }
            _ => value.retained = None,
        }
        let overlay = value
            .chain
            .iter()
            .any(|entry| entry.downcast_ref::<crate::overlay::OverlayNode>().is_some());
        let parent = value.parent;
        if overlay != value.overlay {
            value.overlay = overlay;
            if overlay {
                self.overlays.push(node);
            } else {
                self.overlays.retain(|existing| *existing != node);
            }
            self.needs_draw = true;
            parent_draws_differently = true;
        }
        // Its parent draws it otherwise: by reference, in place, or not (an overlay).
        if parent_draws_differently && let Some(parent) = parent {
            self.invalidate_draw_of(parent);
        }
        let value = self.node_mut(node);
        let positioned =
            value.chain.iter_mut().any(|entry| entry.as_global_position_aware().is_some());
        let z_index = value
            .modifier
            .elements()
            .iter()
            .filter_map(|element| if let Element::ZIndex(z) = element { Some(*z) } else { None })
            // From +0.0 (an empty `sum` is -0.0, which `total_cmp` orders first).
            .fold(0.0f32, |sum, z| sum + z);
        if z_index != value.z_index {
            value.z_index = z_index;
            // Its parent draws its children in another order.
            if let Some(parent) = value.parent {
                self.invalidate_draw_of(parent);
            }
        }
        if positioned {
            self.register_positioned(node);
        }
        let value = self.node_mut(node);
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
        // An overlay is where its provider put it, wherever its parent is.
        let (mut origin, skip) = match value.parent {
            _ if value.overlay => (value.overlay_origin, 1),
            Some(parent) => (self.layer_origin(parent, self.node(parent).layers.len() - 1), 0),
            None => (Offset::ZERO, 0),
        };
        for layer in value.layers[..=layer].iter().skip(skip) {
            origin = origin + Offset::new(layer.offset.x as f32, layer.offset.y as f32);
        }
        origin
    }

    /// The bounds of `node` in the root, in pixels.
    /// `node`'s overlay modifier node, if it's an overlay.
    pub(crate) fn overlay_node(&self, node: NodeId) -> Option<&crate::overlay::OverlayNode> {
        self.get(node)?
            .chain
            .iter()
            .find_map(|entry| entry.downcast_ref::<crate::overlay::OverlayNode>())
    }

    pub fn bounds_in_root(&self, node: NodeId) -> crate::unit::Rect {
        let origin = self.layer_origin(node, 0);
        let size = self.node(node).layers[0].measured;
        crate::unit::Rect::from_offset_size(origin, size.into())
    }

    // ---- Reusing ----------------------------------------------------------------------------

    /// Resets what the tree keeps of `node`'s old content, as Compose's `resetModifierState` on a
    /// node reused or deactivated: the focus, if it's here, goes; pointer captures of its
    /// modifier nodes lapse; it's drawn again. Its modifier nodes are kept, with their caches,
    /// for the new content's elements to update, as Compose's chain diff does (D431); each is
    /// reset (`ModifierNode::on_reset`) by `run_resets`, after the pass.
    fn reset_modifier_state(&mut self, node: NodeId) {
        let value = self.node_mut(node);
        if let Some(retained) = &mut value.retained {
            retained.dirty = true;
            retained.layer_dirty = true;
        }
        if value.chain.is_empty() {
            return;
        }
        value.chain_version = value.chain_version.wrapping_add(1);
        if self.focused() == Some(node) {
            self.focus = None;
            self.focus_listeners_dirty = true;
        }
        if self.text_selection.as_ref().is_some_and(|selection| selection.container() == node) {
            self.text_selection = None;
        }
    }

    /// Resets the modifier nodes of `node` (`ModifierNode::on_reset`), after the pass that reused
    /// or deactivated it: outside composition, so a reset may write state, as Compose applies a
    /// node's `onReuse` with the pass's changes.
    fn run_resets(&mut self, node: NodeId) {
        let Some(value) = self.get_mut(node) else { return };
        for entry in &mut value.chain {
            entry.on_reset();
        }
    }

    /// The focus requesters on `node` (`Modifier.focus_requester`), detached while it's
    /// deactivated: a requester of content kept for reuse focuses nothing, as Compose's
    /// detached nodes. Attached again when it's reused.
    fn set_requesters_attached(&mut self, node: NodeId, attached: bool) {
        let requests = self.focus_requests.clone();
        let value = self.node_mut(node);
        if value.all_capabilities & capability::CUSTOM == 0 {
            return;
        }
        for entry in &mut value.chain {
            if let Some(requester) =
                entry.downcast_mut::<crate::focus_requester::FocusRequesterNode>()
            {
                if attached {
                    requester.attach(&requests, node);
                } else {
                    requester.detach();
                }
            }
        }
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
            if let Some(observer) = &self.observer {
                observer.forget(observation_key(id, Observation::Measure));
                observer.forget(observation_key(id, Observation::Draw));
                observer.forget(observation_key(id, Observation::Layer));
            }
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

    /// Content reused in `node`'s place keeps it, as Compose's `LayoutNode.onReuse`: what the
    /// tree keeps of its old content is reset now (`reset_modifier_state`) and its modifier nodes
    /// after the pass (`on_end_changes`), unless that was done when it was deactivated; then
    /// composition gives it its new modifier and policy, and it's measured again.
    fn reuse(&mut self, node: NodeRef) {
        let node = NodeId::from_ref(node);
        let Some(value) = self.get_mut(node) else { return };
        if std::mem::take(&mut value.deactivated) {
            self.set_requesters_attached(node, true);
            // Its parent has a child to measure again (an ordinary layout's, or a list's).
            if let Some(parent) = self.parent_of(node) {
                self.invalidate_measure(parent);
            }
        } else {
            self.reset_modifier_state(node);
        }
        // Reset after the pass, after the new content's elements updated it, also when it was
        // reset as it was deactivated (with the old content's): a node showing its value (a
        // switch) shows the new one at once (D435).
        self.pending_resets.push(node);
        // Measured again, its ancestors marked up to the first already marked (the content
        // reused around it marked them).
        self.invalidate_draw_of(node);
        let value = self.node_mut(node);
        value.needs_measure = true;
        let mut parent = value.parent;
        while let Some(id) = parent {
            let value = self.node_mut(id);
            if value.subtree_needs_measure {
                break;
            }
            value.subtree_needs_measure = true;
            parent = value.parent;
        }
    }

    /// `node`'s content was deactivated (a lazy list's item kept for reuse, an inactive
    /// `ReusableContentHost`), as Compose's `LayoutNode.onDeactivate`, at the end of the pass: it
    /// stays in the tree, but its parent no longer measures it (`measurables` leave it out), so
    /// it isn't placed, drawn, hit, focused or in the semantics, in any layout; nothing measures
    /// it, whatever it read; its modifier nodes are reset and its focus requesters detached,
    /// until content reuses it.
    fn deactivate(&mut self, node: NodeRef) {
        let node = NodeId::from_ref(node);
        let Some(value) = self.get_mut(node) else { return };
        if std::mem::replace(&mut value.deactivated, true) {
            return;
        }
        value.placed = false;
        value.needs_measure = false;
        value.subtree_needs_measure = false;
        // The outermost deactivated node: its parent lays out and draws without it.
        if let Some(parent) = value.parent
            && self.get(parent).is_some_and(|parent| !parent.deactivated)
        {
            self.invalidate_measure(parent);
        }
        self.reset_modifier_state(node);
        self.run_resets(node);
        self.set_requesters_attached(node, false);
        self.measured_ahead.retain(|ahead| *ahead != node);
        self.kept.retain(|kept| *kept != node);
        if self.bring_into_view_after_layout == Some(node) {
            self.bring_into_view_after_layout = None;
        }
    }

    /// A pass that panicked before its end left no reset to run.
    fn on_begin_changes(&mut self) {
        self.pending_resets.clear();
    }

    /// The modifier nodes of the nodes reused in the pass are reset, now that it's over.
    fn on_end_changes(&mut self) {
        let mut pending = std::mem::take(&mut self.pending_resets);
        for node in pending.drain(..) {
            // Deactivated since (reset then), or released.
            if self.get(node).is_some_and(|value| !value.deactivated) {
                self.run_resets(node);
            }
        }
        self.pending_resets = pending;
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
