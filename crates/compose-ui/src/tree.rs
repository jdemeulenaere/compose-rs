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
use crate::modifier::{Element, Modifier, ParentData, ReleaseContext};
use crate::node_chain::{ChainEntry, EntryId, NodeKinds, entry_state, match_kinds};
use crate::nodes;
use crate::text::TextMeasurer;
use crate::unit::{Density, IntOffset, IntSize, Offset};

/// What a node's observed reads were made for: measuring it, drawing it, (a graphics
/// layer's block) its layer's properties, which change without drawing it again, or drawing
/// its inner layers (what follows its first inner layer in its chain, and its children), which
/// are drawn again apart from the rest of its drawing (D522).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Observation {
    Measure = 0,
    Draw = 1,
    Layer = 2,
    InnerLayers = 3,
    /// Its semantics modifiers' `apply_semantics` (D1257).
    Semantics = 4,
    /// What its active target's focus properties read.
    FocusProperties = 5,
    /// Its placement blocks (`MeasureScope::layout_with`): they run again, it isn't measured.
    Placement = 6,
    /// An inner layer's block run ahead of its drawing (`run_new_layers`), apart from what its
    /// recording reads, the drawing taking that run.
    InnerLayerBlock = 7,
    /// Its measuring in a lookahead pass (D1221): a change dirties both passes.
    LookaheadMeasure = 8,
    /// Its placement blocks in a lookahead pass.
    LookaheadPlacement = 9,
}

/// The key `node`'s reads for `observation` are observed under: the kind in the top four bits
/// (the generation's top bits go: a node's 2^28 generations wrap first).
pub(crate) fn observation_key(node: NodeId, observation: Observation) -> u64 {
    node.to_ref().bits() & !(15 << 60) | (observation as u64) << 60
}

/// The node and the kind of observation `observation_key` made `key` of.
fn observation_of(key: u64) -> (NodeId, Observation) {
    let observation = match key >> 60 {
        0 => Observation::Measure,
        1 => Observation::Draw,
        2 => Observation::Layer,
        3 => Observation::InnerLayers,
        4 => Observation::Semantics,
        5 => Observation::FocusProperties,
        6 => Observation::Placement,
        7 => Observation::InnerLayerBlock,
        8 => Observation::LookaheadMeasure,
        _ => Observation::LookaheadPlacement,
    };
    (NodeId::from_ref(NodeRef::new(key & !(15 << 60))), observation)
}

/// What an event handler reaches besides its node: the text measurer and the clipboard.
pub(crate) type HandlerServices<'a> =
    (&'a crate::text::SharedTextMeasurer, &'a dyn crate::clipboard::ClipboardManager);

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

    pub(crate) fn to_ref(self) -> NodeRef {
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

/// How many of a node's layers can be placed under a motion frame of reference.
pub(crate) const MOTION_LAYERS: usize = 32;

/// One UI element, as Compose's `LayoutNode`.
pub struct LayoutNode {
    /// The node that lays it out, past any virtual node between them.
    pub(crate) parent: Option<NodeId>,
    /// What it lays out, in order: those of its virtual nodes too, flattened, as Compose's
    /// `_unfoldedChildren`, so no pass knows about those. A virtual node's are its own.
    pub(crate) children: Vec<Measurable>,
    /// A virtual node, as `LayoutNode(isVirtual = true)`: a slot of a `SubcomposeLayout`, with its
    /// children in its parent's. It's never measured, placed or drawn.
    pub(crate) is_virtual: bool,
    /// The virtual node it's a child of.
    pub(crate) virtual_parent: Option<NodeId>,
    /// Its virtual nodes, in the order their children come in `children`.
    pub(crate) virtual_children: Vec<NodeId>,
    /// A `SubcomposeLayout`: its slots change its children while it's measured.
    pub(crate) composes_while_measured: bool,
    pub(crate) policy: Rc<dyn MeasurePolicy>,
    pub(crate) modifier: Modifier,
    pub(crate) chain: Vec<ChainEntry>,
    /// Its chain's kinds: the head's aggregate (Compose's `NodeChain.aggregateChildKindSet`).
    pub(crate) kinds: NodeKinds,
    /// The chain indices of the layout modifier nodes, outermost first.
    pub(crate) layout_indices: SmallVec<[u16; 4]>,
    /// Bumped when the chain's nodes change, so pointer captures of removed nodes lapse.
    pub(crate) chain_version: u32,
    pub(crate) parent_data: ParentData,
    /// What its `ParentDataModifierNode`s give its parent, folded tail to head
    /// (`IntrinsicMeasureScope::parent_data_any`).
    pub(crate) custom_parent_data: Option<Rc<dyn std::any::Any>>,
    pub(crate) layers: SmallVec<[Layer; 2]>,
    /// The constraints of the last measurement.
    pub(crate) constraints: Option<crate::constraints::Constraints>,
    pub(crate) needs_measure: bool,
    /// Some descendant needs measuring.
    pub(crate) subtree_needs_measure: bool,
    /// Whether it's placed, as Compose's `LayoutNode.isPlaced`: its parent placed it
    /// (`placed_by_parent`) and each of its layout modifiers placed what's inside it
    /// (`unplaced_layers` empty, D2383). Unplaced nodes aren't drawn, hit or focused.
    pub(crate) placed: bool,
    /// Whether the parent placed it in its last measurement or placement block, as Compose's
    /// `isPlacedByParent`: what measuring and placing follow (its blocks run, it can't be placed
    /// twice), placed or not inside.
    pub(crate) placed_by_parent: bool,
    /// The layers (the first `MOTION_LAYERS`, by bit) whose layout modifier didn't place what's
    /// inside it in its last measurement or placement block: the node isn't placed while one is,
    /// nor drawn or hit, as Compose's inner coordinator not placed leaves its `LayoutNode`
    /// unplaced (D2383). Most nodes have none.
    pub(crate) unplaced_layers: u32,
    /// The placement blocks of its layers (`MeasureScope::layout_with`): most nodes have none.
    pub(crate) placements: Option<Box<crate::placement::NodePlacements>>,
    /// In a `LookaheadScope`, or under one: the results of the pass not running (D1221).
    pub(crate) lookahead: Option<Box<crate::lookahead::LookaheadState>>,
    /// Its measurement is running.
    pub(crate) measuring: bool,
    /// Its placement blocks are running.
    pub(crate) laying_out: bool,
    /// A placement block of it must run.
    pub(crate) needs_placement: bool,
    /// One of a descendant must.
    pub(crate) subtree_needs_placement: bool,
    /// The measurement or block that last placed a layer inside it, to reject placing it twice.
    /// (Its parent placing it twice is `placed`, cleared before each.)
    pub(crate) content_placed_in: u64,
    /// Its parent measured it in a placement block, as `measuredByParent == InLayoutBlock`: a new
    /// size places the parent again, doesn't measure it.
    pub(crate) measured_in_block: bool,
    /// Its parent's last measurement, or the placement block run since, measured it, as Compose's
    /// `measuredByParent != NotUsed`: only then (or placed) can a new size of it change its
    /// parent, so only then does a request of it measure it in place (`remeasure_children_in_place`).
    pub(crate) measured_by_parent: bool,
    /// Where it is in `UiTree::coordinate_readers`, or `u32::MAX`.
    pub(crate) reader_at: u32,
    /// Which layers a motion frame of reference placed
    /// (`with_motion_frame_of_reference_placement`), the first `MOTION_LAYERS`.
    pub(crate) motion_layers: u32,
    /// In `UiTree::layers_moved`.
    pub(crate) layer_moved: bool,
    /// The parent measurement that last measured it, to reject measuring it twice.
    pub(crate) measured_in: u64,
    /// The layout direction where it was composed (`LOCAL_LAYOUT_DIRECTION`).
    pub(crate) layout_direction: crate::alignment::LayoutDirection,
    /// Its parent asked its intrinsics (in its measurement, or for a layout above, through
    /// its own intrinsics), as Compose's `intrinsicsUsageByParent`: a change in it measures that
    /// layout again, not just it in place (D1201).
    pub(crate) intrinsics_asked: bool,
    /// It asked a child's intrinsics since it last forgot them: one has `intrinsics_asked`.
    pub(crate) children_intrinsics_asked: bool,
    /// An overlay (`Modifier.overlay`): placed in the root at `overlay_origin`, not where its
    /// parent placed it; drawn last and hit first.
    pub(crate) overlay: bool,
    pub(crate) overlay_origin: crate::unit::Offset,
    /// When its parent last placed it: children are drawn, and hit, in placement order, as
    /// Compose's (a scaffold's bars over its content).
    pub(crate) place_order: u64,
    /// What it drew last, if it's a repaint boundary (render.rs).
    pub(crate) retained: Option<Box<crate::render::RetainedDrawing>>,
    /// Its graphics layers that aren't its first drawing modifier, each a group of its own
    /// inside its drawing (render.rs), by chain index.
    pub(crate) inner_layers: Vec<crate::render::InnerLayer>,
    /// A layer of it was placed with a layer (`place_with_layer`): its layers are in
    /// `UiTree::placement_layers` (a byte the node's padding had, where most nodes have none).
    pub(crate) has_placement_layers: bool,
    /// The sum of its `z_index` modifiers: siblings are drawn, and hit, in its order.
    pub(crate) z_index: f32,
    /// It's ordered among its siblings by its children's largest `z_index` instead (D1452).
    pub(crate) z_index_of_children: bool,
    /// Its content is deactivated (a lazy list's item kept for reuse): unplaced, its modifier
    /// nodes reset, left out of the walks that only concern content in use.
    pub(crate) deactivated: bool,
    /// In the tree: the root, or a child of an attached node (Compose's `LayoutNode.isAttached`;
    /// a deactivated node stays attached, its modifier nodes don't).
    pub(crate) attached: bool,
    /// Attached in the pass under way, with an ancestor that was (or itself) queued to attach
    /// its subtree's modifier nodes at the pass's end (`Lifecycle::Attach`).
    pub(crate) attaching: bool,
    /// Reused by a paused composition's resume: its modifier nodes stay detached until that
    /// composition is applied (D1162).
    pub(crate) reattach_pending: bool,
    /// Removed from an attached parent in the pass under way, its subtree's modifier nodes not
    /// detached yet: they are when the pass ends, wherever it is then, and attached again if it's
    /// back in the tree (a node movable content moved), as Compose's `removeAt` and `insertAt`
    /// detach and attach it (D1923).
    pub(crate) detach_pending: bool,
    /// Its own semantics, computed when a tree or an action asked and kept until invalidated
    /// (`UiTree::invalidate_semantics_of`, D1257): none for a node without semantics nodes, or
    /// not computed since.
    pub(crate) semantics: Option<Rc<crate::semantics::OwnSemantics>>,
    /// Its semantics id, as Compose's `LayoutNode.semanticsId`: from the tree's counter, another
    /// when the node is reused, so a service never sees one id for two contents (D1262).
    pub(crate) semantics_id: i32,
    /// Where its modifier nodes read composition locals, as `compositionLocalMap`.
    pub(crate) locals: compose_runtime::CompositionLocalAnchor,
    /// Its density, where it's composed (`LOCAL_DENSITY`), as `LayoutNode.density`.
    pub(crate) density: Density,
    /// Its view configuration where it's composed (`LOCAL_VIEW_CONFIGURATION`): an index in
    /// `UiTree::view_configurations`, shared by the nodes with one value, 0 for the host's.
    pub(crate) view_configuration: u16,
    /// The composite key hash where it's composed, as `LayoutNode.compositeKeyHash`: what a saved
    /// focused child is found by again. Read only for nodes with a focus target (0: not read).
    pub(crate) composite_key: compose_runtime::CompositeKeyToken,
}

impl LayoutNode {
    pub fn new(policy: Rc<dyn MeasurePolicy>) -> LayoutNode {
        LayoutNode {
            parent: None,
            children: Vec::new(),
            is_virtual: false,
            virtual_parent: None,
            virtual_children: Vec::new(),
            composes_while_measured: false,
            policy,
            modifier: Modifier,
            chain: Vec::new(),
            kinds: NodeKinds::NONE,
            layout_indices: SmallVec::new(),
            chain_version: 0,
            parent_data: ParentData::default(),
            custom_parent_data: None,
            layers: SmallVec::from_elem(Layer::default(), 1),
            constraints: None,
            needs_measure: true,
            subtree_needs_measure: false,
            placed: false,
            placed_by_parent: false,
            unplaced_layers: 0,
            placements: None,
            lookahead: None,
            measuring: false,
            laying_out: false,
            needs_placement: false,
            content_placed_in: 0,
            reader_at: u32::MAX,
            motion_layers: 0,
            subtree_needs_placement: false,
            measured_in_block: false,
            measured_by_parent: false,
            layer_moved: false,
            measured_in: 0,
            intrinsics_asked: false,
            children_intrinsics_asked: false,
            layout_direction: crate::alignment::LayoutDirection::Ltr,
            overlay: false,
            overlay_origin: crate::unit::Offset::ZERO,
            place_order: 0,
            retained: None,
            inner_layers: Vec::new(),
            has_placement_layers: false,
            z_index: 0.0,
            z_index_of_children: false,
            deactivated: false,
            attached: false,
            attaching: false,
            reattach_pending: false,
            detach_pending: false,
            semantics: None,
            semantics_id: 0,
            locals: compose_runtime::CompositionLocalAnchor::NONE,
            density: Density::default(),
            view_configuration: 0,
            composite_key: compose_runtime::CompositeKeyToken::NONE,
        }
    }

    /// Its measured size (which its parent sees coerced into its constraints).
    pub fn measured_size(&self) -> IntSize {
        self.layers[0].measured
    }

    pub fn modifier(&self) -> &Modifier {
        &self.modifier
    }

    /// The layer chain entry `index` measures and draws in: the number of layout modifier
    /// nodes before it.
    #[cold]
    pub(crate) fn note_motion(&mut self, layer: usize, motion: bool) {
        debug_assert!(
            !motion || layer < MOTION_LAYERS,
            "compose-ui: a motion frame of reference past a node's {MOTION_LAYERS}th layout \
             modifier isn't kept"
        );
        if layer < MOTION_LAYERS {
            let bit = 1u32 << layer;
            self.motion_layers =
                if motion { self.motion_layers | bit } else { self.motion_layers & !bit };
        }
    }

    pub(crate) fn placed_in_motion(&self, layer: usize) -> bool {
        layer < MOTION_LAYERS && self.motion_layers & (1 << layer) != 0
    }

    pub(crate) fn layer_of(&self, index: usize) -> usize {
        self.chain[..index].iter().filter(|entry| entry.kinds.any(NodeKinds::LAYOUT)).count()
    }

    /// The chain entries in layer `layer`: those after the layout node measuring the layer
    /// outside it, up to its own.
    pub(crate) fn entries_of_layer(&self, layer: usize) -> std::ops::Range<usize> {
        let start = match layer {
            0 => 0,
            _ => self.layout_indices[layer - 1] as usize + 1,
        };
        let end =
            self.layout_indices.get(layer).map_or(self.chain.len(), |&index| index as usize + 1);
        start..end
    }
}

/// How many view configurations a tree keeps before forgetting those no node holds.
const MOST_VIEW_CONFIGURATIONS: usize = 64;

/// What a layout node takes from the composition locals where it's composed, as
/// `LayoutNode.compositionLocalMap`'s setter does.
#[derive(Clone, Copy)]
pub(crate) struct ResolvedLocals {
    pub(crate) anchor: compose_runtime::CompositionLocalAnchor,
    pub(crate) density: Density,
    pub(crate) layout_direction: crate::alignment::LayoutDirection,
    pub(crate) view_configuration: crate::view_configuration::ViewConfiguration,
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
    pub(crate) text_measurer: crate::text::SharedTextMeasurer,
    /// The paragraphs `Modifier.capture_content`'s captures still draw.
    pub(crate) held_paragraphs: std::cell::RefCell<crate::draw_capture::HeldParagraphs>,
    pub(crate) clipboard: Rc<dyn crate::clipboard::ClipboardManager>,
    /// The platform's services content reaches as `LOCAL_URI_HANDLER` and
    /// `LOCAL_HAPTIC_FEEDBACK`.
    pub(crate) uri_handler: Rc<dyn crate::platform_services::UriHandler>,
    pub(crate) platform_uri_handler: crate::platform_services::PlatformUriHandler,
    pub(crate) haptic_feedback: Rc<dyn crate::platform_services::HapticFeedback>,
    /// The platform's floating text toolbars (Android's action modes), and the text toolbar over
    /// them (`LOCAL_TEXT_TOOLBAR`'s default), made at the first ask.
    pub(crate) text_action_modes: Option<Rc<dyn crate::text_toolbar::TextActionModes>>,
    pub(crate) text_toolbar: Option<Rc<dyn crate::text_toolbar::TextToolbar>>,
    /// The platform's edge effects, if it has any: overscroll's default on Android.
    pub(crate) edge_effects: Option<Rc<crate::edge_effect::PlatformEdgeEffects>>,
    /// The platform's magnifiers, if it has any (Android's).
    pub(crate) magnifier_factory: Option<Rc<dyn crate::magnifier::PlatformMagnifierFactory>>,
    pub(crate) density: Density,
    /// The host's view configuration, if it set one (else Android's at the density, D907).
    pub(crate) view_configuration: Option<crate::view_configuration::ViewConfiguration>,
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
    pub(crate) focus: crate::focus::FocusOwner,
    /// The input mode, which `Focusability::SYSTEM_DEFINED` reads (`LOCAL_INPUT_MODE_MANAGER`).
    pub(crate) input_mode: crate::input_mode::InputModeManager,
    /// The host's saveable registry, where a saved focused child outlives a recreation.
    pub(crate) saveable_registry: Option<compose_runtime::SaveableStateRegistry>,
    /// Actions queued outside dispatch, run by the host after the frame.
    pub(crate) pending_actions: Vec<crate::input::Action>,
    /// The overlay nodes, in the order they became overlays: later ones over earlier ones (and
    /// siblings in their order, a popup over its outside).
    pub(crate) overlays: Vec<NodeId>,
    /// The window's size, from the last layout.
    pub(crate) window: IntSize,
    /// Counts placements, for children's placement order.
    pub(crate) place_serial: u64,
    pub(crate) captures: Vec<(crate::input::PointerId, Vec<crate::dispatch::HitEntry>)>,
    /// What each pointer's next change is told of its last.
    pub(crate) pointer_states: Vec<(crate::input::PointerId, crate::dispatch::PointerState)>,
    /// `firstDownRefersToPrimaryMouseButtonOnly`, the platform's: a desktop's, unless a host says.
    pub(crate) first_down_primary_only: bool,
    /// What the mouse is over: its last hit path.
    pub(crate) hovered: Vec<crate::dispatch::HitEntry>,
    /// Where the mouse is, in the root, while it's in the window and no button is held.
    pub(crate) mouse: Option<crate::unit::Offset>,
    /// A trackpad's pan is in progress: the hover comes over nothing new until it ends.
    pub(crate) panning: bool,
    /// A hover icon node was ever hovered: until then the pointer icon is the arrow, read for
    /// nothing (D1676).
    pub(crate) has_hover_icons: bool,
    /// What a hovering stylus (or eraser) is over: its last hit path (D1677).
    pub(crate) stylus_hovered: Vec<crate::dispatch::HitEntry>,
    /// The hovering stylus's last event, while it hovers in the window.
    pub(crate) stylus: Option<crate::input::PointerEvent>,
    /// An indirect pointer input node was ever in the tree: until then their dispatch and the
    /// focus's cancels cost nothing (D1675).
    pub(crate) has_indirect_pointer_input: bool,
    /// The indirect pointer input nodes on the focus's path when the focus last changed or an
    /// event last went there, innermost first: those that go before the focus moves are told the
    /// stream ended as they go.
    pub(crate) indirect_focus_path:
        Vec<(NodeId, crate::node_chain::EntryId, crate::node_chain::EntryId)>,
    /// What turns a stream of them into focus moves, made with the first.
    pub(crate) indirect_navigation:
        Option<Box<crate::indirect_pointer::IndirectPointerNavigationGestureDetector>>,
    /// Observes the state measure policies and custom nodes read (D71), keyed by node.
    pub(crate) observer: Option<Rc<compose_runtime::platform::ReadObserver>>,
    /// Observes what semantics read (D1257), apart: a write in a frame invalidates them in that
    /// frame, asking for no other (D1688). `observer` if none.
    pub(crate) semantics_observer: Option<Rc<compose_runtime::platform::ReadObserver>>,
    pub(crate) measure_runs: u64,
    /// What `FocusRequester`s attached to this tree's nodes asked for.
    pub(crate) focus_requests: Rc<crate::focus_requester::FocusRequests>,
    /// The text selected in a `SelectionContainer`, if any.
    pub(crate) text_selection: Option<crate::text_selection::TextSelection>,
    /// Where the selection's container and its ends' texts were when its handles were last
    /// told where they point: a layout that moves none of them tells them nothing.
    pub(crate) handles_placed_for: Option<crate::text_selection::HandlesPlacedFor>,
    /// The selected texts' pinnable containers pinned, by text, so a lazy layout keeps them
    /// composed scrolled away (Compose's `pinnedHandleBySelectableId`).
    pub(crate) selection_pins: Vec<(NodeId, crate::pinnable::PinnedHandle)>,
    /// A node focused before it was ever placed (a prefetched lazy item): brought into view
    /// once laid out.
    pub(crate) bring_into_view_after_layout: Option<NodeId>,
    /// Focus moves waiting for a container to lay out content beyond its bounds (D477).
    pub(crate) beyond_search: crate::focus::BeyondSearch,
    /// What layouts queued for application code (`on_size_changed`, `on_globally_positioned`),
    /// run by the host once the layout is over.
    pub(crate) layout_callbacks: Vec<crate::input::Action>,
    /// The nodes with position-aware modifier nodes (`on_globally_positioned`).
    pub(crate) positioned: Vec<NodeId>,
    /// Something was laid out (or a position-aware node came) since the last callbacks.
    pub(crate) laid_out_since_callbacks: bool,
    /// Whether `take_layout_callbacks` ran nodes' code, which may invalidate and write state.
    pub(crate) layout_nodes_told: bool,
    /// The layers of nodes with `LayoutAwareModifierNode`s measured or placed since the
    /// callbacks last ran (`(node, layer)`, repeated), for `on_placed`.
    pub(crate) placed_layers: Vec<(NodeId, u32)>,
    /// The nodes (flagged `layer_moved`) one of whose graphics layers transforms differently
    /// since the callbacks last ran, if a node wants to know where it is.
    pub(crate) layers_moved: Vec<NodeId>,
    /// Moves when a position-aware node leaves: what the graphics layers keep of what they told
    /// is good only if they kept it since (`GraphicsLayerNode::note_transform`), so a layer no
    /// one under it listens to any more doesn't run its block for nothing.
    pub(crate) position_listeners: u32,
    /// The layout-rect callbacks, from the first registration on: `None` costs nothing.
    pub(crate) rects: Option<std::rc::Rc<std::cell::RefCell<crate::spatial::RectManager>>>,
    /// The nodes whose placing code last asked where they are: it runs again once they aren't
    /// there (D1212).
    pub(crate) coordinate_readers: Vec<crate::placement::CoordinateReader>,
    /// A node's in a `LookaheadScope`: measuring and placing look for its roots (D1221).
    pub(crate) lookahead_used: bool,
    /// Nodes whose lookahead blocks ran in this pass, and whether they asked where they are.
    pub(crate) ahead_readers: Vec<(NodeId, bool)>,
    /// What's invalidated is for the pass not ahead alone: a read made there.
    pub(crate) one_pass: bool,
    /// A lookahead pass is running: nodes under its root have that pass's fields.
    pub(crate) in_lookahead: bool,
    /// A line's been read in this tree: there's someone to tell (D1219).
    pub(crate) alignment_lines_read: bool,
    /// Blocks are running to know a line, not to place: no position or ruler is told.
    pub(crate) placing_for_alignment: bool,
    /// They asked one, and run again when it's placed.
    pub(crate) asked_while_aligning: std::cell::Cell<bool>,
    /// Those a line's change asked of, for the rest of the layout.
    pub(crate) line_requests: Vec<NodeId>,
    /// Who read which ruler, by its id, since its last change (D1218).
    pub(crate) ruler_readers: std::collections::HashMap<(u64, NodeId, bool), crate::ruler::Ruler>,
    /// The nodes with `UnplacedAwareModifierNode`s, and whether each was placed when last looked
    /// at.
    pub(crate) unplaced_aware: Vec<(NodeId, bool)>,
    /// Nodes whose graphics layers came or were updated since the last draw (`run_new_layers`).
    pub(crate) new_layers: Vec<NodeId>,
    /// Where the host's root is on the screen (`UiHost::set_window_origin`).
    pub(crate) window_origin: Offset,
    /// The last `EntryId` given to a chain entry.
    pub(crate) next_entry: u64,
    /// The last semantics id given (the root's is 0).
    last_semantics_id: i32,
    /// The window's insets, the host's state: read while measuring (insets paddings), so a
    /// change measures their readers again.
    pub(crate) window_insets: crate::configuration::WindowInsetsSource,
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
    /// compose-rs's optimizations beyond Compose's rendering the host uses (D520).
    pub(crate) optimizations: crate::optimizations::Optimizations,
    /// The node of each inner layer's group key (`render::INNER_LAYER_KEY`).
    pub(crate) inner_layer_owners: std::collections::HashMap<u64, NodeId>,
    /// The last inner layer's key, without its flag.
    pub(crate) next_inner_layer: u64,
    /// The node and first entry of the layer placed with one whose group's content is being
    /// recorded (render.rs, D1909): its drawing doesn't open it again.
    pub(crate) recording_placed: Option<(NodeId, usize)>,
    /// How many nodes have layers placed with one: while none has, the walks through a node's
    /// layers take the code they had before (D1909).
    pub(crate) placed_layer_nodes: u32,
    /// The layers each of those nodes was placed with, by layer (`place_with_layer`): a table
    /// apart, as most nodes have none (`LayoutNode::has_placement_layers` says which have).
    pub(crate) placement_layers:
        std::collections::HashMap<NodeId, Vec<crate::placement_layer::PlacementLayer>>,
    /// The tree's key in the thread's registry of trees, which its nodes' handles name it by.
    pub(crate) key: u32,
    /// A `SubcomposeLayout` composing its slots while it's measured: what they change of its
    /// children isn't a reason to measure it again, as `ignoreRemeasureRequests`.
    pub(crate) ignoring_remeasure: Option<NodeId>,
    /// The modifier nodes' lifecycle steps the pass under way queued, run when it ends
    /// (lifecycle.rs); kept for its capacity.
    pub(crate) lifecycle: Vec<crate::lifecycle::Lifecycle>,
    /// A node's semantics changed outside what recomposition and layout say (its
    /// `invalidate_semantics`, a semantics node's update): the frame's stats tell it. Set by
    /// `mark_semantics_changed`.
    pub(crate) semantics_changed: bool,
    /// How many times `semantics_changed` was set: a reader between frames (a kept semantics tree)
    /// knows whether something changed since it read (D1689).
    pub(crate) semantics_version: u64,
    /// The platform's autofill and what it was told, if installed (D1680).
    pub(crate) autofill: Option<Box<crate::autofill::AutofillTracking>>,
    /// The platform has autofill, a manager installed or not (`set_autofill_available`).
    pub(crate) autofill_available: bool,
    /// Pointer nodes came, went or changed: the resting mouse's hover is refreshed even if
    /// nothing was laid out.
    pub(crate) hover_stale: bool,
    /// `draw_ahead` is recording a premeasured repaint boundary not yet placed by its parent.
    pub(crate) drawing_ahead: bool,
    /// The lifecycle steps of nodes a paused composition's resumes reused, held until it's
    /// applied or discarded (`Applier::on_paused_end`, D1162).
    pub(crate) paused_lifecycle: Vec<crate::lifecycle::Lifecycle>,
    /// What nodes asked to run once the changes under way are applied
    /// (`NodeContext::side_effect`): the host runs them as soon as it has released the tree
    /// after a composition pass, before the layout (D1161).
    pub(crate) side_effects: Vec<Box<dyn FnOnce()>>,
    /// Nodes may be measured from their callbacks now (`NodeContext::remeasure_sync`): the tree
    /// is free of passes (a frame's start, a handle's `with_node`).
    pub(crate) remeasure_allowed: bool,
    /// A task borrows the tree through a node handle, free of any pass (`with_free_tree`): what
    /// its nodes invalidate asks for a frame (D2025). Not inside a frame's own steps.
    pub(crate) borrowed_by_handle: bool,
    /// Layout nodes whose modifier nodes asked `remeasure_sync` in a callback: measured once it
    /// returns, the entry back in its chain.
    pub(crate) remeasure_after: Vec<NodeId>,
    /// The parents of the subtrees removed in the pass under way, as they were: what's above
    /// them for the traversals of their nodes' `on_detach`, as Compose's `LayoutNode.detach` runs
    /// before it unlinks a node from its parent.
    pub(crate) removed_parents: Vec<(NodeId, NodeId)>,
    /// The roots of the subtrees being detached that the pass removed, one inside another's
    /// detach (content taken out of removed content, D1924): above each, for its nodes'
    /// `on_detach` and their coordinates, is the parent it left, not the one it went to (D1923).
    pub(crate) detaching_moved: Vec<NodeId>,
    /// The nodes the pass took out of a removed subtree still to be detached, each with the
    /// parent it left: detached with that parent, as Compose's `removeAt` detached them with
    /// the subtree before they were taken out of it (D1924).
    pub(crate) taken_out: Vec<(NodeId, NodeId)>,
    /// How many of `removed_parents` the releases of the pass under way went past: those before
    /// are detached (D1923).
    pub(crate) removed_detached: usize,
    /// What changed of modifier locals in the pass under way, looked at when it ends.
    pub(crate) modifier_local_changes: Vec<crate::modifier_local::LocalChange>,
    /// What the last layout node composed resolved of its locals: its siblings mostly share them.
    pub(crate) resolved_locals: Option<ResolvedLocals>,
    /// The view configurations provided (index 0, unused: the host's), `LayoutNode`s naming theirs.
    pub(crate) view_configurations: SmallVec<[crate::view_configuration::ViewConfiguration; 2]>,
    /// The entries whose delegates changed in a measurement, a drawing or an event's dispatch,
    /// looked at where the tree may change (`UiTree::sync_noted_delegates`).
    pub(crate) delegates_noted: Vec<(NodeId, crate::node_chain::EntryId)>,
    /// Inside a measurement: what a node's code delegated waits (`delegates_noted`).
    pub(crate) delegates_wait: bool,
    /// How many nodes' code is running (`UiTree::with_entry_raw`), one inside another through a
    /// traversal: what needs every node in its chain waits for the outermost to return.
    pub(crate) entries_running: u32,
}

impl Drop for UiTree {
    fn drop(&mut self) {
        if self.key != NO_TREE_KEY {
            crate::node_context::unregister_tree(self.key);
        }
    }
}

/// A tree registered nowhere: [`UiTree::placeholder`].
const NO_TREE_KEY: u32 = u32::MAX;

impl compose_runtime::platform::LendableApplier for UiTree {
    fn placeholder() -> UiTree {
        UiTree::placeholder()
    }
}

impl UiTree {
    pub fn new(density: Density, text_measurer: Box<dyn TextMeasurer>) -> UiTree {
        let focus_requests = Rc::new(crate::focus_requester::FocusRequests::default());
        let key = crate::node_context::register_tree(&focus_requests);
        UiTree::with_key(density, text_measurer, focus_requests, key)
    }

    /// What's in a measurement's hands while it lends its tree to a pass composing a slot: nothing
    /// asks it anything, its window being shut, and no handle names it (D1309).
    pub fn placeholder() -> UiTree {
        UiTree::with_key(
            Density::new(1.0),
            Box::new(crate::fixed_text::FixedTextMeasurer::new()),
            Rc::default(),
            NO_TREE_KEY,
        )
    }

    fn with_key(
        density: Density,
        text_measurer: Box<dyn TextMeasurer>,
        focus_requests: Rc<crate::focus_requester::FocusRequests>,
        key: u32,
    ) -> UiTree {
        let mut root = LayoutNode::new(Rc::new(StackMeasurePolicy));
        root.density = density;
        root.placed = true;
        root.placed_by_parent = true;
        root.attached = true;
        UiTree {
            ignoring_remeasure: None,
            slots: vec![Slot { generation: 0, node: Some(root) }],
            free: Vec::new(),
            root: NodeId { index: 0, generation: 0 },
            text_measurer: crate::text::SharedTextMeasurer::new(text_measurer),
            held_paragraphs: Default::default(),
            clipboard: Rc::new(crate::clipboard::MemoryClipboard::default()),
            uri_handler: Rc::new(crate::platform_services::PlatformUriHandler::headless()),
            platform_uri_handler: crate::platform_services::PlatformUriHandler::headless(),
            haptic_feedback: Rc::new(crate::platform_services::NoHapticFeedback),
            text_action_modes: None,
            text_toolbar: None,
            edge_effects: None,
            magnifier_factory: None,
            density,
            view_configuration: None,
            display_list: DisplayList::default(),
            needs_draw: true,
            draw_version: 0,
            input_method_reset: false,
            focus: crate::focus::FocusOwner::default(),
            input_mode: crate::input_mode::InputModeManager::new(),
            saveable_registry: None,
            pending_actions: Vec::new(),
            overlays: Vec::new(),
            window: IntSize::new(0, 0),
            place_serial: 0,
            captures: Vec::new(),
            pointer_states: Vec::new(),
            first_down_primary_only: true,
            hovered: Vec::new(),
            mouse: None,
            panning: false,
            has_hover_icons: false,
            stylus_hovered: Vec::new(),
            stylus: None,
            has_indirect_pointer_input: false,
            indirect_focus_path: Vec::new(),
            indirect_navigation: None,
            observer: None,
            semantics_observer: None,
            measure_runs: 0,
            focus_requests,
            text_selection: None,
            handles_placed_for: None,
            selection_pins: Vec::new(),
            bring_into_view_after_layout: None,
            beyond_search: crate::focus::BeyondSearch::default(),
            layout_callbacks: Vec::new(),
            positioned: Vec::new(),
            laid_out_since_callbacks: false,
            layout_nodes_told: false,
            placed_layers: Vec::new(),
            layers_moved: Vec::new(),
            position_listeners: 0,
            rects: None,
            coordinate_readers: Vec::new(),
            lookahead_used: false,
            ahead_readers: Vec::new(),
            one_pass: false,
            in_lookahead: false,
            alignment_lines_read: false,
            placing_for_alignment: false,
            asked_while_aligning: std::cell::Cell::new(false),
            line_requests: Vec::new(),
            ruler_readers: Default::default(),
            unplaced_aware: Vec::new(),
            new_layers: Vec::new(),
            window_origin: Offset::ZERO,
            next_entry: 0,
            last_semantics_id: 0,
            window_insets: crate::configuration::WindowInsetsSource::NONE,
            measured_ahead: Vec::new(),
            kept: Vec::new(),
            keeps_unplaced: false,
            optimizations: crate::optimizations::Optimizations::defaults(),
            inner_layer_owners: std::collections::HashMap::new(),
            next_inner_layer: 0,
            recording_placed: None,
            placed_layer_nodes: 0,
            placement_layers: std::collections::HashMap::new(),
            key,
            lifecycle: Vec::new(),
            semantics_changed: false,
            semantics_version: 0,
            autofill: None,
            autofill_available: false,
            hover_stale: false,
            drawing_ahead: false,
            paused_lifecycle: Vec::new(),
            side_effects: Vec::new(),
            remeasure_allowed: false,
            borrowed_by_handle: false,
            remeasure_after: Vec::new(),
            removed_parents: Vec::new(),
            detaching_moved: Vec::new(),
            taken_out: Vec::new(),
            removed_detached: 0,
            modifier_local_changes: Vec::new(),
            resolved_locals: None,
            view_configurations: SmallVec::new(),
            delegates_noted: Vec::new(),
            delegates_wait: false,
            entries_running: 0,
        }
    }

    /// The host's window insets state, which `MeasureScope::window_insets` reads.
    pub(crate) fn set_window_insets_source(
        &mut self,
        source: crate::configuration::WindowInsetsSource,
    ) {
        self.window_insets = source;
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
            value.placed.then(|| {
                let z = if value.z_index_of_children {
                    let inside = value.children.iter().map(|inner| self.node(inner.node).z_index);
                    inside
                        .fold(None, |most: Option<f32>, z| Some(most.map_or(z, |most| most.max(z))))
                        .unwrap_or(0.0)
                } else {
                    value.z_index
                };
                (z, value.place_order)
            })
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

    /// The density of `node`, as `LayoutInfo.density`.
    pub fn density_of(&self, node: NodeId) -> Density {
        self.get(node).map_or(self.density, |value| value.density)
    }

    pub fn density(&self) -> Density {
        self.density
    }

    /// Changes the host's density, the root's. Its content's come from `LOCAL_DENSITY`, which the
    /// host provides, each layout node taking its own when it's composed (`set_locals`).
    pub fn set_density(&mut self, density: Density) {
        if self.density != density {
            self.density = density;
            let root = self.root;
            self.set_node_density(root, density);
        }
    }

    /// Its density changed, as `LayoutNode.density`'s setter: it measures and draws again, its
    /// parent data is given again, and its attached modifier nodes are told
    /// (`ModifierNode::on_density_change`, where the tree next applies its nodes' requests).
    fn set_node_density(&mut self, node: NodeId, density: Density) {
        let value = self.node_mut(node);
        value.density = density;
        if value.chain.iter().any(|entry| entry.state & entry_state::ATTACH_RAN != 0) {
            self.queue_lifecycle(crate::lifecycle::Lifecycle::Density(node));
        }
        self.invalidate_measure(node);
        self.invalidate_drawing_of_node(node);
        self.invalidate_draw_of(node);
        if self.node(node).kinds.any(NodeKinds::PARENT_DATA)
            && self.refresh_custom_parent_data(node)
            && let Some(parent) = self.parent_of(node)
        {
            self.invalidate_measure(parent);
        }
    }

    /// `node` is composed where `resolved` says, as `LayoutNode.compositionLocalMap`'s setter: a
    /// new place, or a provider's new values, gives it its density, layout direction and view
    /// configuration, and invalidates its consumers (`is_composition_local_consumer`) by their
    /// kinds, whether they're attached or not (Compose's waits for an attach, whose invalidation
    /// comes to the same in a tree that measures a reused node when it's asked to).
    pub(crate) fn set_locals(&mut self, node: NodeId, resolved: ResolvedLocals) {
        self.resolved_locals = Some(resolved);
        let value = self.node_mut(node);
        if value.locals == resolved.anchor {
            return;
        }
        let fresh = value.locals == compose_runtime::CompositionLocalAnchor::NONE;
        value.locals = resolved.anchor;
        let view_configuration = self.view_configuration_index(resolved.view_configuration);
        let value = self.node_mut(node);
        if fresh {
            // Nothing measured or attached to tell.
            value.density = resolved.density;
            value.layout_direction = resolved.layout_direction;
            value.view_configuration = view_configuration;
            return;
        }
        if value.view_configuration != view_configuration {
            value.view_configuration = view_configuration;
            if value.chain.iter().any(|entry| entry.state & entry_state::ATTACH_RAN != 0) {
                self.queue_lifecycle(crate::lifecycle::Lifecycle::ViewConfiguration(node));
            }
        }
        if self.node(node).density != resolved.density {
            self.set_node_density(node, resolved.density);
        }
        self.set_layout_direction(node, resolved.layout_direction);
        let value = self.node_mut(node);
        if !value.kinds.any(NodeKinds::COMPOSITION_LOCAL_CONSUMER) {
            return;
        }
        let mut invalidated = NodeKinds::NONE;
        for entry in &mut value.chain {
            if entry.kinds.any(NodeKinds::COMPOSITION_LOCAL_CONSUMER) {
                invalidated |= entry.updated_kinds();
            }
        }
        self.queue_lifecycle(crate::lifecycle::Lifecycle::CompositionLocals(node));
        self.auto_invalidate(node, invalidated, NodeKinds::NONE, false);
    }

    /// `configuration`'s place in `view_configurations`, by equality: a subtree animating one
    /// (a provider of each frame's) leaves only those its nodes hold.
    fn view_configuration_index(
        &mut self,
        configuration: crate::view_configuration::ViewConfiguration,
    ) -> u16 {
        if let Some(found) =
            self.view_configurations.iter().position(|known| *known == configuration)
        {
            return found as u16 + 1;
        }
        if self.view_configurations.len() >= MOST_VIEW_CONFIGURATIONS {
            self.forget_unused_view_configurations();
        }
        self.view_configurations.push(configuration);
        assert!(self.view_configurations.len() < usize::from(u16::MAX), "too many in use");
        self.view_configurations.len() as u16
    }

    /// Keeps the view configurations nodes hold, in the same order, and moves their indices.
    fn forget_unused_view_configurations(&mut self) {
        let mut kept = vec![0u16; self.view_configurations.len() + 1];
        for node in self.slots.iter().filter_map(|slot| slot.node.as_ref()) {
            kept[usize::from(node.view_configuration)] = 1;
        }
        let mut next = 0;
        let mut index = 0;
        self.view_configurations.retain(|_| {
            index += 1;
            let keep = kept[index] != 0;
            if keep {
                next += 1;
                kept[index] = next;
            }
            keep
        });
        kept[0] = 0;
        for node in self.slots.iter_mut().filter_map(|slot| slot.node.as_mut()) {
            node.view_configuration = kept[usize::from(node.view_configuration)];
        }
    }

    pub fn text_measurer(&mut self) -> std::cell::RefMut<'_, dyn TextMeasurer> {
        self.text_measurer.borrow_mut()
    }

    /// The text measurer itself, as what's measured apart from the tree shares it.
    pub fn shared_text_measurer(&self) -> &crate::text::SharedTextMeasurer {
        &self.text_measurer
    }

    /// The text measurer, shared: to render the display list with (`display_list()`) without
    /// copying it.
    pub fn text_measurer_ref(&self) -> std::cell::Ref<'_, dyn TextMeasurer> {
        self.text_measurer.borrow()
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

    /// Gives `node` another place it's composed at: for tests of a place gone.
    #[doc(hidden)]
    pub fn set_composite_key_token(
        &mut self,
        node: NodeId,
        token: compose_runtime::CompositeKeyToken,
    ) {
        self.node_mut(node).composite_key = token;
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
        Some((slot.node.as_mut()?, (&self.text_measurer, &*self.clipboard)))
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

    /// What's above `node` (whose value is `value`) for its modifier nodes' callbacks: the parent
    /// it was removed from in the pass under way, while it's being detached or isn't in the tree
    /// (Compose detaches a node before it unlinks it), else its parent.
    pub(crate) fn parent_for_callbacks(&self, node: NodeId, value: &LayoutNode) -> Option<NodeId> {
        let removed = || {
            let mut removed = self.removed_parents.iter();
            removed.rfind(|(child, _)| *child == node).map(|(_, parent)| *parent)
        };
        if self.detaching_moved.contains(&node) {
            return removed().or(value.parent);
        }
        value.parent.or_else(removed)
    }

    /// Whether `node` is in a subtree the pass removed whose modifier nodes are still to be
    /// detached (its root's `detach_pending`).
    fn under_detach_pending(&self, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(value) = current.and_then(|id| self.get(id)) {
            if value.detach_pending {
                return true;
            }
            current = value.parent;
        }
        false
    }

    /// The parent a walk of coordinates goes to from `node` (whose value is `value`, not an
    /// overlay): while a moved subtree is being detached, the parent it left, as Compose detaches
    /// a node before it unlinks it (D1924); else its parent.
    #[inline]
    pub(crate) fn coordinates_parent(&self, node: NodeId, value: &LayoutNode) -> Option<NodeId> {
        if self.detaching_moved.is_empty() {
            return value.parent;
        }
        self.coordinates_parent_detaching(node, value)
    }

    #[cold]
    #[inline(never)]
    fn coordinates_parent_detaching(&self, node: NodeId, value: &LayoutNode) -> Option<NodeId> {
        self.parent_for_callbacks(node, value).filter(|parent| self.get(*parent).is_some())
    }

    /// The children of `node`, in order.
    pub fn children_of(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.get(node).into_iter().flat_map(|node| node.children.iter().map(|child| child.node))
    }

    /// The modifier nodes of `node`'s chain, head to tail, as `LayoutNode`'s `getModifierInfo`
    /// lists them: for tests and tools.
    pub fn modifier_nodes(
        &self,
        node: NodeId,
    ) -> impl Iterator<Item = &dyn crate::modifier::ModifierNode> + '_ {
        self.get(node).into_iter().flat_map(|node| node.chain.iter().map(|entry| &*entry.node))
    }

    /// The number of live nodes, the root included.
    pub fn node_count(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    // ---- Updates from composition -----------------------------------------------------------

    /// Marks `node` for measuring, and its ancestors as having a descendant to measure.
    pub fn invalidate_measure(&mut self, node: NodeId) {
        // Deactivated content (kept for reuse) isn't measured: reused, it is.
        self.invalidate_measure_with(node, true);
    }

    /// Measures `node` again for a state its measurement read: with inner layers, what it lays
    /// out is theirs, and what's around them draws again only if its measurement changes their
    /// sizes (`measure_node`), as Compose's layer is recorded again alone (D522).
    pub(crate) fn invalidate_layout_of(&mut self, node: NodeId) {
        self.invalidate_measure_with(node, false);
    }

    fn invalidate_measure_with(&mut self, node: NodeId, whole: bool) {
        if self.ignoring_remeasure == Some(node) {
            return;
        }
        let Some(value) = self.get_mut(node) else { return };
        if value.deactivated {
            // Deactivated content (kept for reuse) isn't measured; reused, it is if it asked
            // meanwhile, as Compose marks a deactivated node dirty without scheduling it.
            value.needs_measure = true;
            return;
        }
        if whole {
            self.invalidate_draw_of(node);
        } else {
            self.invalidate_children_draw_of(node);
        }
        if self.lookahead_used && !self.one_pass {
            self.invalidate_other_pass_measure(node);
        }
        let Some(value) = self.get_mut(node) else { return };
        value.needs_measure = true;
        let mut parent = value.parent;
        while let Some(id) = parent {
            // What's under what's being measured: it measures what it wants of it.
            if self.ignoring_remeasure == Some(id) {
                break;
            }
            let value = self.node_mut(id);
            value.subtree_needs_measure = true;
            parent = value.parent;
        }
    }

    /// Where a node given to `parent` goes: the node that lays it out, how many of that one's
    /// children come before `parent`'s, and the virtual node if `parent` is one.
    fn host_of(&self, parent: NodeId) -> (NodeId, usize, Option<NodeId>) {
        let value = self.node(parent);
        if !value.is_virtual {
            return (parent, 0, None);
        }
        let host = value.parent.expect("a virtual node has its layout's");
        let before = self.node(host).virtual_children.iter().take_while(|other| **other != parent);
        let offset = before.map(|other| self.node(*other).children.len()).sum();
        (host, offset, Some(parent))
    }

    /// A virtual node of `host`, `at` among those it has, with no children yet.
    pub(crate) fn add_virtual_node(&mut self, host: NodeId, at: usize) -> NodeId {
        let mut node = LayoutNode::new(Rc::new(StackMeasurePolicy));
        node.is_virtual = true;
        node.parent = Some(host);
        let id = self.alloc(node);
        self.node_mut(host).virtual_children.insert(at, id);
        id
    }

    /// `host`'s virtual node at `from` goes to `to`, its children with it.
    pub(crate) fn move_virtual_node(&mut self, host: NodeId, from: usize, to: usize) {
        if from == to {
            return;
        }
        let value = self.node(host);
        let node = value.virtual_children[from];
        let count = self.node(node).children.len();
        // Where its children are and go, from the children before it there and here, in one pass
        // (a lazy layout's jump moves thousands, D1477).
        let (low, high) = (from.min(to), from.max(to));
        let (mut before_low, mut through_high) = (0, 0);
        for (at, other) in value.virtual_children[..=high].iter().enumerate() {
            if at == low {
                before_low = through_high;
            }
            through_high += self.node(*other).children.len();
        }
        let (old, new) = if from < to {
            (before_low, through_high - count)
        } else {
            let at_high = self.node(value.virtual_children[high]).children.len();
            (through_high - at_high, before_low)
        };
        let value = self.node_mut(host);
        let moved: SmallVec<[Measurable; 8]> = value.children.drain(old..old + count).collect();
        value.virtual_children.remove(from);
        value.virtual_children.insert(to, node);
        value.children.splice(new..new, moved);
        self.invalidate_measure(host);
    }

    /// `host`'s virtual node `node` goes, with what's still under it.
    pub(crate) fn remove_virtual_node(&mut self, host: NodeId, node: NodeId) {
        let Some(value) = self.get(node) else { return };
        let count = value.children.len();
        if self.get(host).is_some() {
            let (_, offset, _) = self.host_of(node);
            let value = self.node_mut(host);
            value.children.drain(offset..offset + count);
            value.virtual_children.retain(|other| *other != node);
            self.invalidate_measure(host);
        }
        self.free_subtree(node);
    }

    /// `host`'s virtual nodes `nodes` go, with what's still under them: in one pass over its
    /// children, where one at a time looks for each one's place among them (a lazy layout's jump
    /// letting thousands of items go, D1477).
    pub(crate) fn remove_virtual_nodes(&mut self, host: NodeId, nodes: &[NodeId]) {
        if nodes.len() < 2 || self.get(host).is_none() {
            for &node in nodes {
                self.remove_virtual_node(host, node);
            }
            return;
        }
        let mut gone: std::collections::HashSet<NodeId> =
            nodes.iter().copied().filter(|node| self.get(*node).is_some()).collect();
        let value = self.node_mut(host);
        let virtual_children = std::mem::take(&mut value.virtual_children);
        let children = std::mem::take(&mut value.children);
        let mut kept_virtual = Vec::with_capacity(virtual_children.len());
        let mut kept = Vec::with_capacity(children.len());
        let mut offset = 0;
        for node in virtual_children {
            let count = self.node(node).children.len();
            if !gone.contains(&node) {
                kept_virtual.push(node);
                kept.extend_from_slice(&children[offset..offset + count]);
            }
            offset += count;
        }
        kept.extend_from_slice(&children[offset..]);
        let value = self.node_mut(host);
        value.virtual_children = kept_virtual;
        value.children = kept;
        self.invalidate_measure(host);
        for &node in nodes {
            if gone.remove(&node) {
                self.free_subtree(node);
            }
        }
    }

    /// The first node of a virtual node not deactivated, as a measurable.
    pub(crate) fn first_virtual_measurable(&self, node: NodeId) -> Option<Measurable> {
        (self.node(node).children.iter())
            .find(|child| self.get(child.node).is_some_and(|value| !value.deactivated))
            .copied()
    }

    /// The nodes of a virtual node, as measurables, without what's deactivated.
    pub(crate) fn virtual_measurables(&self, node: NodeId) -> Vec<Measurable> {
        (self.node(node).children.iter())
            .filter(|child| self.get(child.node).is_some_and(|value| !value.deactivated))
            .copied()
            .collect()
    }

    /// compose-rs's optimizations beyond Compose's rendering this tree's host uses
    /// (`UiHost::set_optimizations`).
    pub fn optimizations(&self) -> crate::optimizations::Optimizations {
        self.optimizations
    }

    /// Uses `optimizations` from the next frame on, which draws everything again if one that
    /// draws changed.
    pub fn set_optimizations(&mut self, optimizations: crate::optimizations::Optimizations) {
        let draws_differently = self.optimizations.draws_differently(optimizations);
        self.optimizations = optimizations;
        if !self.keeps_unplaced_groups() {
            self.kept.clear();
        }
        if !optimizations.contains(crate::optimizations::Optimization::DRAW_PREFETCHED_AHEAD) {
            self.measured_ahead.clear();
        }
        if draws_differently {
            self.invalidate_draw();
        }
    }

    /// Draws everything again: every repaint boundary too.
    pub fn invalidate_draw(&mut self) {
        self.needs_draw = true;
        for slot in &mut self.slots {
            let Some(node) = slot.node.as_mut() else { continue };
            if let Some(retained) = node.retained.as_mut() {
                retained.dirty = true;
                retained.layer_dirty = true;
            }
            for inner in &mut node.inner_layers {
                inner.dirty = true;
            }
        }
    }

    /// Draws `node`'s inner layers again (what follows its first one, and its children), not
    /// what's around them: as Compose's layer draws its content again, not its parent's.
    pub(crate) fn invalidate_inner_layers(&mut self, node: NodeId) {
        let Some(value) = self.get_mut(node) else { return };
        for inner in &mut value.inner_layers {
            inner.dirty = true;
        }
        // Nothing places it: drawn when a frame does.
        if value.placed {
            self.needs_draw = true;
        }
    }

    /// Draws again what `node` draws in: the nearest repaint boundary at or above it, if any
    /// (what's outside every boundary is drawn each draw), and its inner layers. An overlay is
    /// drawn at the root, outside its ancestors.
    pub(crate) fn invalidate_draw_of(&mut self, node: NodeId) {
        self.invalidate_drawing(node, true);
    }

    /// Draws again what `node` draws before its inner layers (a state its drawing read there
    /// changed): what draws it, not its inner layers' content, as Compose's layer keeps its
    /// recording while what draws it draws again (D522).
    pub(crate) fn invalidate_draw_before_inner_layers(&mut self, node: NodeId) {
        self.invalidate_drawing(node, false);
    }

    /// Draws again how `node` draws its children (one moved, came, went, or is drawn
    /// otherwise): its last inner layer if it has one, which holds them, else what draws it.
    pub(crate) fn invalidate_children_draw_of(&mut self, node: NodeId) {
        if self.get(node).is_some_and(|value| !value.inner_layers.is_empty()) {
            self.invalidate_inner_layers(node);
        } else {
            self.invalidate_draw_of(node);
        }
    }

    fn invalidate_drawing(&mut self, node: NodeId, own_inner_layers: bool) {
        let mut current = Some(node);
        while let Some(id) = current {
            let Some(value) = self.get_mut(id) else { break };
            // Its inner layers (D522): the last holds its children, so a change below it is
            // theirs alone; its own change may be in what's before them too.
            if !value.inner_layers.is_empty() && (id != node || own_inner_layers) {
                for inner in &mut value.inner_layers {
                    inner.dirty = true;
                }
                if id != node && value.retained.is_none() {
                    if value.placed {
                        self.needs_draw = true;
                    }
                    return;
                }
            }
            if let Some(retained) = &mut value.retained {
                if id != node && !value.inner_layers.is_empty() {
                    // A child's change, in its inner layers: its own drawing stays.
                    if value.placed {
                        self.needs_draw = true;
                    }
                    return;
                }
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
            self.invalidate_children_draw_of(parent);
        }
    }

    /// Observes the state policies and custom nodes read while measuring and drawing, so a
    /// write measures or draws them again (D71). `observer` calls back when one happens.
    pub fn set_read_observer(&mut self, observer: compose_runtime::platform::ReadObserver) {
        self.observer = Some(Rc::new(observer));
    }

    /// Observes what semantics read apart from what measuring and drawing read: `observer` calls
    /// back when a write invalidates some, and a host asks for a frame only outside one (a write
    /// a frame makes, a lazy list's layout state, is applied before it draws: D1688).
    pub fn set_semantics_read_observer(
        &mut self,
        observer: compose_runtime::platform::ReadObserver,
    ) {
        self.semantics_observer = Some(Rc::new(observer));
    }

    /// Measures and draws again what observed writes invalidated, and forgets the semantics
    /// they invalidated.
    pub fn apply_observed_writes(&mut self) {
        if let Some(semantics) = self.semantics_observer.clone() {
            semantics.drain_invalidated(|key| {
                let (node, _) = observation_of(key);
                self.invalidate_semantics_of(node);
            });
        }
        let Some(observer) = self.observer.clone() else { return };
        observer.drain_invalidated(|key| {
            let (node, observation) = observation_of(key);
            match observation {
                // Read in one pass's measuring, it's that pass's; in lookahead's, both's: what's
                // ahead moves what's now.
                Observation::Measure => {
                    self.one_pass = true;
                    self.invalidate_layout_of(node);
                    self.one_pass = false;
                }
                Observation::LookaheadMeasure => self.invalidate_layout_of(node),
                Observation::LookaheadPlacement => {
                    self.placement_read_changed(node);
                    self.invalidate_other_pass_placement(node);
                }
                Observation::Draw if self.get(node).is_some_and(|value| !value.deactivated) => {
                    // Its reads in its inner layers are theirs (`InnerLayers`).
                    self.invalidate_draw_before_inner_layers(node)
                }
                Observation::Draw => {}
                Observation::InnerLayers | Observation::InnerLayerBlock => {
                    if let Some(value) = self.get_mut(node).filter(|value| !value.deactivated) {
                        for inner in &mut value.inner_layers {
                            inner.ahead = None;
                        }
                        self.invalidate_inner_layers(node);
                        self.note_layer_change(node, false);
                        self.new_layers.push(node);
                    }
                }
                Observation::Layer => {
                    // Its layer's properties only: its content and version stay.
                    self.needs_draw = true;
                    self.new_layers.push(node);
                    let value = self.get_mut(node);
                    if let Some(retained) = value.and_then(|value| value.retained.as_mut()) {
                        retained.layer_dirty = true;
                        if retained.layer_ahead.is_some() {
                            retained.layer_ahead = None;
                        }
                    }
                    self.note_layer_change(node, true);
                }
                Observation::Semantics => self.invalidate_semantics_of(node),
                Observation::FocusProperties => self.focus_properties_read_changed(node),
                Observation::Placement => self.placement_read_changed(node),
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
            let value = self.node_mut(node);
            value.layout_direction = direction;
            // Its attached modifier nodes are told, as Compose's `onLayoutDirectionChange`.
            if value.chain.iter().any(|entry| entry.state & entry_state::ATTACH_RAN != 0) {
                self.queue_lifecycle(crate::lifecycle::Lifecycle::LayoutDirection(node));
            }
            self.invalidate_measure(node);
        }
    }

    /// The node's layout direction.
    pub fn layout_direction(&self, node: NodeId) -> crate::alignment::LayoutDirection {
        self.node(node).layout_direction
    }

    pub fn set_measure_policy(&mut self, node: NodeId, policy: Rc<dyn MeasurePolicy>) {
        let value = self.node_mut(node);
        // An equal policy is kept, as Compose's `measurePolicy` setter compares with `equals`
        // (D1354).
        if !Rc::ptr_eq(&value.policy, &policy)
            && !MeasurePolicy::same(&*value.policy, &*policy as &dyn std::any::Any)
        {
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
        let mut next_entry = self.next_entry;
        let value = self.node_mut(node);
        if value.modifier.same(&modifier) {
            return;
        }
        let old = value.modifier.elements();
        let new = modifier.elements();
        // What the updated, inserted and removed nodes' kinds say to invalidate, as Compose's
        // `autoInvalidateUpdatedNode`, `…InsertedNode` and `…RemovedNode` (NodeKind.kt).
        let mut invalidated = NodeKinds::NONE;
        // The entries whose updates delegated or undelegated, and whether a layer came or went.
        let mut delegated: SmallVec<[EntryId; 1]> = SmallVec::new();
        let mut layers_changed = false;
        // The entries with modifier locals updated.
        let mut locals_updated: SmallVec<[u16; 1]> = SmallVec::new();
        // The updated event nodes: told their state when the pass ends.
        let mut focus_events: SmallVec<[EntryId; 1]> = SmallVec::new();
        // The entries whose update makes them a focus target, or not one any more.
        let mut target_flips: SmallVec<[(EntryId, bool); 1]> = SmallVec::new();
        // The common case, the same kinds in the same order: update in place, allocating nothing.
        if old.len() == new.len() && old.iter().zip(new).all(|(old, new)| old.same_kind(new)) {
            for (index, (old, new)) in old.iter().zip(new).enumerate() {
                if !old.same(new) {
                    let entry = &mut value.chain[index];
                    nodes::update(new, &mut *entry.node);
                    entry.refresh_hit_clip();
                    if let Some(flip) = entry.focus_target_flip() {
                        target_flips.push(flip);
                    }
                    let updated = entry.after_update();
                    if updated.delegated {
                        delegated.push(entry.id);
                    }
                    layers_changed |= updated.layers;
                    let kinds = entry.updated_kinds();
                    invalidated |= kinds;
                    if kinds.any(NodeKinds::FOCUS_EVENT) {
                        focus_events.push(entry.id);
                    }
                    if entry.kinds.any(NodeKinds::LOCALS) {
                        locals_updated.push(index as u16);
                    }
                }
            }
            for &index in &locals_updated {
                self.note_local_changes(node, index as usize, true);
            }
            let value = self.node_mut(node);
            let previous = std::mem::replace(&mut value.modifier, modifier);
            if layers_changed {
                value.chain_version = value.chain_version.wrapping_add(1);
            }
            for &entry in &delegated {
                self.queue_lifecycle(crate::lifecycle::Lifecycle::SyncDelegates(node, entry));
            }
            let mut parent_data_changed = self.refresh_chain(node);
            if layers_changed {
                self.remap_same_entries(node);
            }
            // Updated parent data nodes tell the parent, as Compose's `invalidateParentData`.
            if invalidated.any(NodeKinds::PARENT_DATA) {
                parent_data_changed |= self.refresh_custom_parent_data(node);
            }
            drop(previous);
            self.focus_chain_changed(node, invalidated, &focus_events);
            for (entry, wanted) in target_flips {
                self.flip_focus_target(node, entry, wanted);
            }
            self.auto_invalidate(node, invalidated, NodeKinds::NONE, parent_data_changed);
            if invalidated.any(NodeKinds::GRAPHICS_LAYER) {
                self.note_updated_layer(node);
            }
            return;
        }
        let matched = match_kinds(old, new, Element::same_kind);
        let mut old_nodes: SmallVec<[Option<ChainEntry>; 4]> =
            std::mem::take(&mut value.chain).into_iter().map(Some).collect();
        let mut chain = Vec::with_capacity(new.len());
        let mut reused = 0;
        // New nodes of a node in the tree are attached at once, as Compose's chain update marks
        // them; their `on_attach` runs when the pass ends.
        let attached = value.attached && !value.deactivated && !value.reattach_pending;
        let mut inserted = false;
        // The inserted and removed nodes' kinds.
        let mut structural = NodeKinds::NONE;
        let mut locals_inserted: SmallVec<[u16; 1]> = SmallVec::new();
        for (index, element) in new.iter().enumerate() {
            match matched[index] {
                Some(old_index) => {
                    let mut entry = old_nodes[old_index].take().expect("matched once");
                    if !old[old_index].same(element) {
                        nodes::update(element, &mut *entry.node);
                        entry.refresh_hit_clip();
                        if let Some(flip) = entry.focus_target_flip() {
                            target_flips.push(flip);
                        }
                        let updated = entry.after_update();
                        if updated.delegated {
                            delegated.push(entry.id);
                        }
                        layers_changed |= updated.layers;
                        let kinds = entry.updated_kinds();
                        invalidated |= kinds;
                        if kinds.any(NodeKinds::FOCUS_EVENT) {
                            focus_events.push(entry.id);
                        }
                        if entry.kinds.any(NodeKinds::LOCALS) {
                            locals_updated.push(index as u16);
                        }
                    }
                    chain.push(entry);
                    reused += 1;
                }
                None => {
                    next_entry += 1;
                    let mut entry = ChainEntry::new(
                        nodes::create(element),
                        element.fixed_kinds(),
                        EntryId(next_entry),
                    );
                    if attached {
                        entry.state |= entry_state::ATTACHED;
                        inserted = true;
                    }
                    invalidated |= entry.kinds;
                    structural |= entry.kinds;
                    if entry.kinds.any(NodeKinds::LOCALS) {
                        locals_inserted.push(index as u16);
                    }
                    chain.push(entry);
                }
            }
        }
        for removed in old_nodes.iter().flatten() {
            invalidated |= removed.kinds.without(NodeKinds::LAYOUT_AWARE);
            structural |= removed.kinds;
        }
        let changed = reused != old.len() || reused != new.len() || layers_changed;
        if changed {
            value.chain_version = value.chain_version.wrapping_add(1);
            // Which layer bounds its semantics follows the chain's layout nodes.
            value.semantics = None;
        }
        value.chain = chain;
        value.modifier = modifier;
        // A node attached in this pass attaches its whole subtree when it ends.
        let attaching = value.attaching;
        self.next_entry = next_entry;
        if changed {
            self.note_autofill_dirty(node);
        }
        for &index in &locals_updated {
            self.note_local_changes(node, index as usize, true);
        }
        for &index in &locals_inserted {
            self.note_local_changes(node, index as usize, false);
        }
        for &entry in &delegated {
            self.queue_lifecycle(crate::lifecycle::Lifecycle::SyncDelegates(node, entry));
        }
        let mut parent_data_changed = self.refresh_chain(node);
        // Parent data nodes updated, inserted or removed tell the parent (`invalidated` has
        // the inserted and removed nodes' kinds too).
        if invalidated.any(NodeKinds::PARENT_DATA) {
            parent_data_changed |= self.refresh_custom_parent_data(node);
        }
        if changed {
            self.remap_captures(node, &matched);
        }
        if inserted && !attaching {
            self.queue_lifecycle(crate::lifecycle::Lifecycle::AttachEntries(node));
        }
        // The removed nodes: detached when the pass ends (if they were attached), then released.
        let mut detaching: SmallVec<[(ChainEntry, usize); 2]> = SmallVec::new();
        for (old_index, removed) in old_nodes.into_iter().enumerate() {
            let Some(mut removed) = removed else { continue };
            if removed.kinds.any(NodeKinds::LOCALS) {
                self.note_removed_locals(node, &mut removed);
            }
            if removed.state & entry_state::ATTACH_RAN != 0 {
                // Its place: before the first node kept that was after it, for what it
                // traverses in its `on_detach`.
                let place = matched
                    .iter()
                    .position(|kept| kept.is_some_and(|kept| kept > old_index))
                    .unwrap_or(matched.len());
                detaching.push((removed, place));
            } else {
                if removed.state & entry_state::HAS_HANDLE != 0 {
                    self.focus_requests.attached_entries.borrow_mut().remove(&removed.id.0);
                }
                self.forget_entry_reads(removed.id, removed.state);
                if removed.state & entry_state::DELEGATING != 0 {
                    self.release_delegates(&mut *removed.node);
                }
                self.release_node(removed.node);
            }
        }
        if !detaching.is_empty() {
            self.queue_lifecycle(crate::lifecycle::Lifecycle::Removed(node, detaching));
        }
        self.focus_chain_changed(node, invalidated, &focus_events);
        for (entry, wanted) in target_flips {
            self.flip_focus_target(node, entry, wanted);
        }
        self.auto_invalidate(node, invalidated, structural, parent_data_changed);
        if invalidated.any(NodeKinds::GRAPHICS_LAYER) {
            self.note_updated_layer(node);
        }
    }

    /// Invalidates what a chain change's nodes' `kinds` say, as Compose's auto-invalidation
    /// (`NodeKind.kt`, `autoInvalidateNodeSelf`): a layout node measures `node` again (and draws
    /// it), a draw node (a graphics layer included) draws it again; a change of its parent data
    /// measures its parent. An overlay (compose-rs's popup windows) is positioned, and takes or
    /// gives back the focus, by the layout: it lays out again, as Compose's popup updates its
    /// window. A semantics node's change forgets what the node kept and is told in the frame's
    /// stats; a pointer node come or gone has the resting mouse's hover refreshed (an
    /// `Enter` for one that came under it), as the layout does; focus has what the chain change
    /// said at the pass's end (`focus_chain_changed`).
    pub(crate) fn auto_invalidate(
        &mut self,
        node: NodeId,
        kinds: NodeKinds,
        structural: NodeKinds,
        parent_data_changed: bool,
    ) {
        // A layout-aware node come or updated (not one gone: it won't be told) sees a layout.
        let measures = NodeKinds::LAYOUT
            | NodeKinds::OVERLAY
            | NodeKinds::ON_REMEASURED
            | NodeKinds::ON_PLACED;
        if kinds.any(measures) {
            self.invalidate_measure(node);
        } else if kinds.any(NodeKinds::DRAW | NodeKinds::GRAPHICS_LAYER) {
            self.invalidate_draw_of(node);
        }
        let shown = self.get(node).is_some_and(|value| value.attached && !value.deactivated);
        if kinds.any(NodeKinds::SEMANTICS) {
            self.forget_semantics(node);
            // Only for what the tree shows: a prefetched item, or a slot kept in use but not
            // placed, isn't in the semantics (D1265).
            if shown && self.is_placed_in_root(node) {
                self.mark_semantics_changed();
            }
        }
        // A pointer node come or gone (a change of its hit area comes with a layout, which
        // refreshes the hover anyway): not one only updated, as a clickable's new callback.
        if structural.any(NodeKinds::POINTER_INPUT)
            && shown
            && (self.mouse.is_some() || self.stylus.is_some())
        {
            self.hover_stale = true;
        }
        if parent_data_changed && let Some(parent) = self.parent_of(node) {
            self.invalidate_measure(parent);
        }
    }

    /// Points the pointer captures of `node`'s modifier nodes at their new places in its
    /// chain, so a gesture in progress continues on a node that was kept; captures of removed
    /// nodes are dropped.
    pub(crate) fn remap_captures(&mut self, node: NodeId, matched: &[Option<usize>]) {
        if self.captures.is_empty() && self.hovered.is_empty() && self.stylus_hovered.is_empty() {
            return;
        }
        let value = self.node(node);
        let version = value.chain_version;
        let layers: SmallVec<[u16; 8]> =
            (0..value.chain.len()).map(|index| value.layer_of(index) as u16).collect();
        let layer_of = |index: usize| layers[index];
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
        self.stylus_hovered.retain_mut(remap);
    }

    /// Recomputes what the chain's nodes can do, its layers and its parent data. Returns
    /// whether the parent data changed.
    pub(crate) fn refresh_chain(&mut self, node: NodeId) -> bool {
        let value = self.node_mut(node);
        value.layout_indices.clear();
        // The aggregates, from the tail: each entry's kinds and those after it.
        let mut aggregate = NodeKinds::NONE;
        for entry in value.chain.iter_mut().rev() {
            aggregate |= entry.kinds;
            entry.aggregate = aggregate;
        }
        value.kinds = aggregate;
        let indirect = aggregate.any(NodeKinds::INDIRECT_POINTER_INPUT);
        for (index, entry) in value.chain.iter_mut().enumerate() {
            if entry.kinds.any(NodeKinds::LAYOUT) {
                value.layout_indices.push(index as u16);
            }
        }
        self.has_indirect_pointer_input |= indirect;
        let mut parent_draws_differently = self.refresh_layers(node);
        let value = self.node_mut(node);
        let overlay = value.kinds.any(NodeKinds::OVERLAY);
        let parent = value.parent;
        if overlay != value.overlay {
            value.overlay = overlay;
            if overlay {
                self.overlays.push(node);
                // A window of its own, at once: its content's effects may ask for the focus
                // before it's laid out.
                if self.overlay_node(node).is_some_and(|overlay| overlay.focus_on_open) {
                    self.window_opened(node);
                    if let Some(token) =
                        self.overlay_node(node).and_then(|overlay| overlay.window.clone())
                    {
                        token.0.set(Some(node));
                    }
                }
            } else {
                self.overlays.retain(|existing| *existing != node);
            }
            self.needs_draw = true;
            parent_draws_differently = true;
        }
        // Its parent draws it otherwise: by reference, in place, or not (an overlay).
        if parent_draws_differently && let Some(parent) = parent {
            self.invalidate_children_draw_of(parent);
        }
        let value = self.node_mut(node);
        let positioned = value.kinds.any(NodeKinds::GLOBAL_POSITION_AWARE);
        // From +0.0 (an empty `sum` is -0.0, which `total_cmp` orders first).
        let (mut z_index, mut of_children) = (0.0f32, false);
        for element in value.modifier.elements() {
            match element {
                Element::ZIndex(z) => z_index += *z,
                Element::LazyItemBoundary => of_children = true,
                _ => {}
            }
        }
        value.z_index_of_children = of_children;
        if z_index != value.z_index {
            value.z_index = z_index;
            // Its parent draws its children in another order; and that one's, if it goes by them.
            if let Some(parent) = value.parent {
                self.invalidate_children_draw_of(parent);
                let around = self.node(parent);
                if around.z_index_of_children
                    && let Some(grandparent) = around.parent
                {
                    self.invalidate_children_draw_of(grandparent);
                }
            }
        }
        if positioned {
            self.register_positioned(node);
        }
        if self.node(node).kinds.any(NodeKinds::UNPLACED) {
            self.register_unplaced_aware(node);
        }
        let value = self.node_mut(node);
        value.layers.resize(value.layout_indices.len() + 1, Layer::default());
        // Those of layers gone: the rest measure again, whatever they were.
        if let Some(placements) = &mut value.placements {
            placements.blocks.truncate(value.layers.len());
            let layers = value.layers.len();
            self.rulers_of_layers_gone(node, layers);
        }
        self.refresh_parent_data(node)
    }

    /// Recomputes `node`'s parent data from its elements. Returns whether it changed.
    pub(crate) fn refresh_parent_data(&mut self, node: NodeId) -> bool {
        let value = self.node_mut(node);
        let mut parent_data = ParentData::default();
        for element in value.modifier.elements() {
            match element {
                // The outermost wins, as Compose's parent data chain folds from the outside.
                Element::Weight(weight, fill) if parent_data.weight.is_none() => {
                    parent_data.weight = Some((*weight, *fill))
                }
                Element::Align(alignment) if parent_data.alignment.is_none() => {
                    parent_data.alignment = Some(*alignment)
                }
                Element::AlignBlock(block) if parent_data.alignment.is_none() => {
                    parent_data.alignment = Some(crate::modifier::ParentAlignment::Block(block.0));
                }
                Element::FillCrossAxis(fraction)
                    if parent_data.fill_cross_axis_fraction.is_none() =>
                {
                    parent_data.fill_cross_axis_fraction = Some(*fraction)
                }
                _ => {}
            }
        }
        let changed = value.parent_data != parent_data;
        value.parent_data = parent_data;
        changed
    }

    // ---- Dumps ------------------------------------------------------------------------------

    /// An indented dump of the tree: each node's bounds in its parent, policy, modifier and
    /// own semantics.
    pub fn dump(&mut self) -> String {
        let mut out = String::new();
        let mut stack = vec![(self.root, 0usize)];
        while let Some((id, depth)) = stack.pop() {
            let config = self.own_semantics(id).map(|own| own.config.clone());
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
        let value = self.get(node)?;
        if !value.kinds.any(NodeKinds::OVERLAY) {
            return None;
        }
        value
            .chain
            .iter()
            .find_map(|entry| entry.node.downcast_ref::<crate::overlay::OverlayNode>())
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
    /// reset, detached and attached again (`ModifierNode::on_reset`, `on_detach`, `on_attach`)
    /// after the pass (D1153).
    fn reset_modifier_state(&mut self, node: NodeId) {
        let value = self.node_mut(node);
        // New content where it's placed, its layer measured or not: as a node that was unplaced
        // (`markSubtreeAsPlaced`), though `placed` here is only its parent's last measurement.
        if value.kinds.any(NodeKinds::ON_PLACED) {
            self.placed_layers.push((node, 0));
        }
        let value = self.node_mut(node);
        if let Some(retained) = &mut value.retained {
            retained.dirty = true;
            retained.layer_dirty = true;
        }
        if value.chain.is_empty() {
            return;
        }
        value.chain_version = value.chain_version.wrapping_add(1);
        // A hover stays tracked, as Compose's hit path keeps the reused node: its modifier nodes forget
        // theirs (`on_reset`), and the mouse resting there doesn't enter it again until it moves.
        let version = value.chain_version;
        for entry in self.hovered.iter_mut().chain(self.stylus_hovered.iter_mut()) {
            if entry.node == node {
                entry.version = version;
            }
        }
        if self.text_selection.as_ref().is_some_and(|selection| selection.container() == node) {
            self.text_selection = None;
            self.release_selection_pins();
        }
    }

    /// Reuses `node` for new content (`Applier::reuse`, `reuse_paused` with `paused`).
    fn reuse_node(&mut self, node: NodeId, paused: bool) {
        if self.get(node).is_none() {
            return;
        }
        // Before the pass updates its elements: what's told of the focus going is what was there.
        self.focus_node_resetting(node);
        let semantics_id = self.next_semantics_id();
        // Autofill sees new content come (D1688).
        self.note_autofill_reused(node);
        let value = self.node_mut(node);
        value.semantics_id = semantics_id;
        value.composite_key = compose_runtime::CompositeKeyToken::NONE;
        let reactivated = std::mem::take(&mut value.deactivated);
        if reactivated {
            // Its parent has a child to measure again (an ordinary layout's, or a list's): it
            // measures it again if it asked to while deactivated (`needs_measure` kept), as
            // Compose's `rescheduleRemeasureOrRelayout`; its ancestors, reused before it, are
            // active again.
            if let Some(parent) = self.parent_of(node) {
                self.invalidate_measure(parent);
            }
        } else {
            // In use but not placed: its parent's last measurement left it out (a lazy list's
            // slot, the item it wants not composed then). Its parent places the new content
            // (D1264). Not a paused composition's: applied later, its content waits for the frame
            // wanting it; a prefetch composing whole (no pausable composition) asks here too.
            if !value.placed_by_parent
                && !paused
                && let Some(parent) = self.parent_of(node)
            {
                self.invalidate_measure(parent);
            }
            self.reset_modifier_state(node);
        }
        // Reset (if it wasn't when deactivated), detached and attached again after the pass,
        // after the new content's elements updated it: a node showing its value (a switch)
        // shows the new one at once from its `on_attach` (D435, D1156); after the paused
        // composition's apply, for paused content (D1162).
        if paused {
            // Reset when the resume ends, as composed at once (before the content later
            // resumes bring); attached again once the composition is applied (D1162).
            self.queue_lifecycle(crate::lifecycle::Lifecycle::Reset(node));
            self.paused_lifecycle.push(crate::lifecycle::Lifecycle::AttachEntries(node));
            self.node_mut(node).reattach_pending = true;
        } else if reactivated {
            // From the kept pool (reset and detached then): attached before the update, as
            // Compose's `onReuse` (D2392).
            self.attach_reused(node);
        } else {
            self.queue_lifecycle(crate::lifecycle::Lifecycle::Reuse(node));
        }
        // Drawn again; measured again only if something asks, as Compose's reuse reschedules
        // only a pending measurement (a deactivated node's asked meanwhile).
        self.invalidate_draw_of(node);
    }

    // ---- Releasing --------------------------------------------------------------------------

    /// A semantics id no node had (non-negative: Android's views take -1 as the host's).
    fn next_semantics_id(&mut self) -> i32 {
        self.last_semantics_id = self.last_semantics_id.wrapping_add(1) & i32::MAX;
        self.last_semantics_id
    }

    fn alloc(&mut self, mut node: LayoutNode) -> NodeId {
        node.semantics_id = self.next_semantics_id();
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

    /// Frees `node` and its subtree: their modifier nodes detached (those still attached: a
    /// subtree removed in the pass releasing it, before the pass's end detaches it) and
    /// released.
    fn free_subtree(&mut self, node: NodeId) {
        // Nodes its chains lost in this pass first, while their layout nodes live.
        self.release_removed_in_subtree(node);
        // Then those removed before it, moved elsewhere (D1923).
        self.detach_removed_before(node);
        self.detach_subtree(node);
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            let slot = &mut self.slots[id.index as usize];
            let Some(mut value) = slot.node.take() else { continue };
            slot.generation = slot.generation.wrapping_add(1);
            self.free.push(id.index);
            stack.extend(value.children.iter().map(|child| child.node));
            stack.extend(value.virtual_children.iter().copied());
            if let Some(observer) = &self.observer {
                observer.forget(observation_key(id, Observation::Measure));
                observer.forget(observation_key(id, Observation::Draw));
                observer.forget(observation_key(id, Observation::Layer));
                observer.forget(observation_key(id, Observation::InnerLayers));
                if let Some(semantics) = &self.semantics_observer {
                    semantics.forget(observation_key(id, Observation::Semantics));
                }
                observer.forget(observation_key(id, Observation::Semantics));
                observer.forget(observation_key(id, Observation::FocusProperties));
                if value.placements.is_some() {
                    observer.forget(observation_key(id, Observation::Placement));
                }
                if value.lookahead.is_some() {
                    observer.forget(observation_key(id, Observation::LookaheadMeasure));
                    observer.forget(observation_key(id, Observation::LookaheadPlacement));
                }
                if !value.inner_layers.is_empty() {
                    observer.forget(observation_key(id, Observation::InnerLayerBlock));
                }
            }
            for layer in &value.inner_layers {
                self.inner_layer_owners.remove(&layer.key);
            }
            // Its layers placed with one, dropped with it (application code's captures).
            let placed = if value.has_placement_layers {
                self.placed_layer_nodes -= 1;
                self.placement_layers.remove(&id)
            } else {
                None
            };
            if !self.ruler_readers.is_empty() {
                self.ruler_readers.retain(|(_, reader, _), _| *reader != id);
            }
            for entry in &mut value.chain {
                self.forget_entry_reads(entry.id, entry.state);
                if entry.state & entry_state::DELEGATING != 0 {
                    self.release_delegates(&mut *entry.node);
                }
            }
            // As `release_node`: what they run may ask where something is.
            let access = crate::tree_access::TreeAccess::new(&mut *self);
            let open = access.open();
            let mut context = ReleaseContext {
                text_measurer: &access.text_measurer,
                held_paragraphs: &access.held_paragraphs,
            };
            for entry in &mut value.chain {
                entry.node.on_release(&mut context);
            }
            drop(value);
            drop(placed);
            drop(open);
        }
    }
}

impl Applier for UiTree {
    type Node = LayoutNode;

    fn root(&self) -> NodeRef {
        self.root.to_ref()
    }

    fn contains(&self, node: NodeRef) -> bool {
        self.get(NodeId::from_ref(node)).is_some()
    }

    fn insert(&mut self, parent: NodeRef, index: usize, mut node: LayoutNode) -> NodeRef {
        let (parent, offset, slot) = self.host_of(NodeId::from_ref(parent));
        node.parent = Some(parent);
        node.virtual_parent = slot;
        // Attached with its parent; the first of a new subtree attaches the subtree's modifier
        // nodes when the pass ends, children's first, as Compose's `LayoutNode.attach`.
        let parent_value = self.node(parent);
        let (attached, under_attaching) = (parent_value.attached, parent_value.attaching);
        node.attached = attached;
        node.attaching = attached;
        let id = self.alloc(node);
        if attached && !under_attaching {
            self.queue_lifecycle(crate::lifecycle::Lifecycle::Attach(id));
        }
        let length = self.node(parent).children.len();
        assert!(offset + index <= length, "compose-ui: insert at {index} past {length} children");
        let child = Measurable { node: id, layer: 0 };
        self.node_mut(parent).children.insert(offset + index, child);
        if let Some(slot) = slot {
            self.node_mut(slot).children.insert(index, child);
        }
        self.refresh_chain(id);
        self.refresh_custom_parent_data(id);
        self.invalidate_measure(parent);
        id.to_ref()
    }

    /// A node of a paused composition (a lazy list's item composing between frames), in no
    /// parent until the composition is applied (`attach`): nothing lays it out or draws it
    /// until then.
    fn create(&mut self, mut node: LayoutNode) -> NodeRef {
        node.parent = None;
        let id = self.alloc(node);
        self.refresh_chain(id);
        self.refresh_custom_parent_data(id);
        id.to_ref()
    }

    fn attach(&mut self, parent: NodeRef, index: usize, node: NodeRef) {
        let id = NodeId::from_ref(node);
        let (parent, offset, slot) = self.host_of(NodeId::from_ref(parent));
        let length = self.node(parent).children.len();
        assert!(offset + index <= length, "compose-ui: attach at {index} past {length} children");
        let value = self.node_mut(id);
        assert!(value.parent.is_none(), "compose-ui: attaching a node that has a parent");
        value.parent = Some(parent);
        value.virtual_parent = slot;
        let child = Measurable { node: id, layer: 0 };
        self.node_mut(parent).children.insert(offset + index, child);
        if let Some(slot) = slot {
            self.node_mut(slot).children.insert(index, child);
        }
        self.invalidate_measure(parent);
        self.invalidate_measure(id);
        // In the tree now, its subtree too: their modifier nodes are attached when the pass ends.
        // A node movable content moved here from an attached parent in this pass is detached
        // first when the pass ends (`detach_pending`), then attached with what's attaching here.
        let parent_value = self.node(parent);
        if parent_value.attached {
            let under_attaching = parent_value.attaching;
            let mut stack = vec![id];
            while let Some(next) = stack.pop() {
                let value = self.node_mut(next);
                value.attached = true;
                value.attaching = true;
                stack.extend(value.children.iter().map(|child| child.node));
            }
            if !under_attaching {
                self.queue_lifecycle(crate::lifecycle::Lifecycle::Attach(id));
            }
        }
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        let (parent, offset, slot) = self.host_of(NodeId::from_ref(parent));
        let removed: SmallVec<[Measurable; 8]> = (self.node_mut(parent).children)
            .drain(offset + index..offset + index + count)
            .collect();
        if let Some(slot) = slot {
            self.node_mut(slot).children.drain(index..index + count);
        }
        // Out of a subtree the pass removed, whose modifier nodes are still attached (movable
        // content taken out of other movable content leaving): Compose detached them with it.
        let from_removed = !self.node(parent).attached && self.under_detach_pending(parent);
        // The last first, as Compose's `LayoutNode.removeAt` (D1924).
        for child in removed.into_iter().rev() {
            let Some(value) = self.get_mut(child.node) else { continue };
            value.parent = None;
            value.virtual_parent = None;
            value.placed_by_parent = false;
            let (was_placed, attached) = (value.placed, value.attached);
            if was_placed {
                self.mark_subtree_not_placed(child.node);
            }
            if from_removed {
                // Detached with the parent it left, or before where it went attaches it.
                self.node_mut(child.node).detach_pending = true;
                self.queue_lifecycle(crate::lifecycle::Lifecycle::Detach(child.node));
                self.removed_parents.push((child.node, parent));
                self.taken_out.push((child.node, parent));
                continue;
            }
            // Out of the tree, its subtree too: their modifier nodes are detached when the pass
            // ends (or when it releases them, first), as Compose's `LayoutNode.detach`.
            if attached {
                let mut stack = vec![child.node];
                while let Some(next) = stack.pop() {
                    let value = self.node_mut(next);
                    value.attached = false;
                    value.attaching = false;
                    stack.extend(value.children.iter().map(|child| child.node));
                    // A selection's container gone: so is its selection, and its texts' pins
                    // are let go (D2023).
                    if self.text_selection.is_some_and(|selection| selection.container() == next) {
                        self.text_selection = None;
                        self.release_selection_pins();
                    }
                }
                self.node_mut(child.node).detach_pending = true;
                self.queue_lifecycle(crate::lifecycle::Lifecycle::Detach(child.node));
                self.removed_parents.push((child.node, parent));
            }
        }
        self.invalidate_measure(parent);
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        let (parent, offset, slot) = self.host_of(NodeId::from_ref(parent));
        let destination = if from > to { to } else { to - count };
        let rotate = |children: &mut [Measurable]| {
            assert!(destination + count <= children.len(), "compose-ui: move past the end");
            if destination < from {
                children[destination..from + count].rotate_right(count);
            } else if destination > from {
                children[from..destination + count].rotate_left(count);
            }
        };
        rotate(&mut self.node_mut(parent).children[offset..]);
        if let Some(slot) = slot {
            rotate(&mut self.node_mut(slot).children);
        }
        self.invalidate_measure(parent);
    }

    /// Content reused in `node`'s place keeps it, as Compose's `LayoutNode.onReuse`: what the
    /// tree keeps of its old content is reset now (`reset_modifier_state`) and its modifier nodes
    /// reset, detached and attached again after the pass (`on_end_changes`); then composition
    /// gives it its new modifier and policy, which measure it again if they changed.
    fn reuse_paused(&mut self, node: NodeRef) -> bool {
        // A deactivated node is shown again at apply; one still shown is reset now, before the
        // pass updates it, as unpaused.
        let id = NodeId::from_ref(node);
        if self.get(id).is_none_or(|value| value.deactivated) {
            return false;
        }
        // Reset at the resume's end, attached again at the paused composition's apply
        // (`on_paused_end`): its new content may come in later resumes, frames apart (a switch
        // attached before would show the old value).
        self.reuse_node(id, true);
        true
    }

    fn reuse(&mut self, node: NodeRef) {
        self.reuse_node(NodeId::from_ref(node), false);
    }

    /// The paused composition whose resumes reused nodes is applied or discarded: their
    /// lifecycle steps run with that pass's (or where the tree next applies its requests).
    fn on_paused_end(&mut self) {
        let mut paused = std::mem::take(&mut self.paused_lifecycle);
        for step in &paused {
            if let crate::lifecycle::Lifecycle::AttachEntries(node) = step
                && let Some(value) = self.get_mut(*node)
            {
                value.reattach_pending = false;
            }
        }
        self.lifecycle.append(&mut paused);
    }

    /// `node`'s content was deactivated (a lazy list's item kept for reuse, an inactive
    /// `ReusableContentHost`), as Compose's `LayoutNode.onDeactivate`, at the end of the pass: it
    /// stays in the tree, but its parent no longer measures it (`measurables` leave it out), so
    /// it isn't placed, drawn, hit, focused or in the semantics, in any layout; nothing measures
    /// it, whatever it read (what it reads marks it to measure once reused); its modifier nodes
    /// are reset and detached, as Compose's `resetModifierState`, until content reuses it.
    fn deactivate(&mut self, node: NodeRef) {
        let node = NodeId::from_ref(node);
        let Some(value) = self.get_mut(node) else { return };
        if value.deactivated {
            return;
        }
        self.focus_subtree_deactivating(node);
        let Some(value) = self.get_mut(node) else { return };
        value.placed_by_parent = false;
        value.subtree_needs_measure = false;
        self.mark_subtree_not_placed(node);
        self.tell_unplaced_going(node);
        let Some(value) = self.get_mut(node) else { return };
        // The outermost deactivated node: its parent lays out and draws without it.
        if let Some(parent) = value.parent
            && self.get(parent).is_some_and(|parent| !parent.deactivated)
        {
            self.invalidate_measure(parent);
        }
        self.reset_modifier_state(node);
        self.reset_layout_state(node);
        // At the end of the pass already: reset and detached now, the node deactivated first so
        // what the callbacks invalidate is left to its reuse (a reset asking for a measurement
        // measures nothing around content leaving).
        self.node_mut(node).deactivated = true;
        self.reset_and_detach(node);
        self.measured_ahead.retain(|ahead| *ahead != node);
        self.kept.retain(|kept| *kept != node);
        if self.bring_into_view_after_layout == Some(node) {
            self.bring_into_view_after_layout = None;
        }
    }

    /// The modifier nodes' lifecycle steps the pass queued run now that it's over, in Compose's
    /// orders (lifecycle.rs); their side effects, once the host releases the tree.
    fn on_end_changes(&mut self) {
        self.run_lifecycle();
        // What the focus system told application code in the pass (a target removed, a listener
        // attached) runs now: the values the pass removed are forgotten next, which the callbacks
        // of what left may still write, as Compose's run in its `onEndApplyChanges` (D1273).
        let actions = std::mem::take(&mut self.pending_actions);
        crate::input::run_actions(actions);
    }

    fn release(&mut self, node: NodeRef) {
        let node = NodeId::from_ref(node);
        let Some(value) = self.get(node) else { return };
        let slot = value.virtual_parent;
        if let Some(parent) = value.parent {
            // A node of a poisoned composition, still attached.
            if let Some(parent_value) = self.get_mut(parent) {
                parent_value.children.retain(|child| child.node != node);
            }
            if let Some(slot) = slot.and_then(|slot| self.get_mut(slot)) {
                slot.children.retain(|child| child.node != node);
            }
            self.invalidate_measure(parent);
        }
        self.free_subtree(node);
        self.needs_draw = true;
    }
}

/// The last segment of a type path, without generics: `compose_ui::layout::Stack` → `Stack`.
pub(crate) fn short_type_name(name: &'static str) -> &'static str {
    let name = name.split('<').next().unwrap_or(name);
    name.rsplit("::").next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use compose_runtime::platform::Applier;

    use super::{LayoutNode, NodeId, UiTree};
    use crate::layout::StackMeasurePolicy;
    use crate::unit::Density;

    /// A small deterministic generator (SplitMix64): a failing seed fails the same way again.
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// One of `0..n`.
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn chance(&mut self, one_in: usize) -> bool {
            self.below(one_in) == 0
        }
    }

    fn layout_node() -> LayoutNode {
        LayoutNode::new(Rc::new(StackMeasurePolicy))
    }

    /// The host's virtual nodes and their children against `model`: each virtual node's in order,
    /// the host's all of them in that order, and each one's parents.
    fn check(tree: &UiTree, host: NodeId, model: &[(NodeId, Vec<NodeId>)], step: &str) {
        let value = tree.node(host);
        let slots: Vec<NodeId> = model.iter().map(|(slot, _)| *slot).collect();
        assert_eq!(value.virtual_children, slots, "virtual children after {step}");
        let all: Vec<NodeId> = model.iter().flat_map(|(_, children)| children.clone()).collect();
        let children: Vec<NodeId> = value.children.iter().map(|child| child.node).collect();
        assert_eq!(children, all, "host's children after {step}");
        for (slot, expected) in model {
            let value = tree.node(*slot);
            assert_eq!(value.parent, Some(host), "{slot:?}'s parent after {step}");
            let children: Vec<NodeId> = value.children.iter().map(|child| child.node).collect();
            assert_eq!(&children, expected, "{slot:?}'s children after {step}");
            for child in expected {
                let value = tree.node(*child);
                assert_eq!(value.parent, Some(host), "{child:?}'s parent after {step}");
                assert_eq!(value.virtual_parent, Some(*slot), "{child:?}'s slot after {step}");
            }
        }
    }

    /// Virtual nodes of 0 to 3 children each added, moved (next to each other both ways, and
    /// anywhere) and removed (the first, the last, both, all, or any of them; in order or the
    /// other way; named twice or freed already) at random, against a model: the host's children
    /// follow its virtual nodes', each in its place, and those removed are freed (D1477's moves
    /// and removals in one pass).
    #[test]
    fn random_adds_moves_and_removes_match_a_model() {
        for seed in 0..4 {
            let mut random = Random(seed);
            let mut tree = UiTree::new(
                Density::new(1.0),
                Box::new(crate::fixed_text::FixedTextMeasurer::new()),
            );
            let root = tree.root_id();
            let host = NodeId::from_ref(tree.insert(root.to_ref(), 0, layout_node()));
            let mut model: Vec<(NodeId, Vec<NodeId>)> = Vec::new();
            let mut freed: Vec<NodeId> = Vec::new();
            for step in 0..400 {
                let len = model.len();
                let kind = if len < 2 { 0 } else { random.below(5) };
                let step = match kind {
                    0 | 1 => {
                        let at = random.below(len + 1);
                        let slot = tree.add_virtual_node(host, at);
                        let children = (0..random.below(4))
                            .map(|index| {
                                NodeId::from_ref(tree.insert(slot.to_ref(), index, layout_node()))
                            })
                            .collect();
                        model.insert(at, (slot, children));
                        format!("step {step} (seed {seed}): add at {at}")
                    }
                    2 => {
                        let low = random.below(len - 1);
                        let (from, to) =
                            if random.chance(2) { (low, low + 1) } else { (low + 1, low) };
                        tree.move_virtual_node(host, from, to);
                        let slot = model.remove(from);
                        model.insert(to, slot);
                        format!("step {step} (seed {seed}): move next to it, {from} to {to}")
                    }
                    3 => {
                        let (from, to) = (random.below(len), random.below(len));
                        tree.move_virtual_node(host, from, to);
                        let slot = model.remove(from);
                        model.insert(to, slot);
                        format!("step {step} (seed {seed}): move {from} to {to}")
                    }
                    _ => {
                        let which = random.below(5);
                        let mut gone: Vec<usize> = match which {
                            0 => vec![0],
                            1 => vec![len - 1],
                            2 => vec![0, len - 1],
                            3 => (0..len).collect(),
                            _ => (0..len).filter(|_| random.chance(2)).collect(),
                        };
                        let reversed = random.chance(2);
                        if reversed {
                            gone.reverse();
                        }
                        let mut nodes: Vec<NodeId> = gone.iter().map(|at| model[*at].0).collect();
                        if !nodes.is_empty() && random.chance(3) {
                            let again = nodes[random.below(nodes.len())];
                            nodes.insert(random.below(nodes.len() + 1), again);
                        }
                        if !freed.is_empty() && random.chance(3) {
                            let old = freed[random.below(freed.len())];
                            nodes.insert(random.below(nodes.len() + 1), old);
                        }
                        let step = format!(
                            "step {step} (seed {seed}): remove {nodes:?} ({which}, reversed \
                             {reversed}) of {len}"
                        );
                        if nodes.len() == 1 && random.chance(2) {
                            tree.remove_virtual_node(host, nodes[0]);
                        } else {
                            tree.remove_virtual_nodes(host, &nodes);
                        }
                        let mut removed = Vec::new();
                        model.retain(|(slot, children)| {
                            let keep = !nodes.contains(slot);
                            if !keep {
                                removed.push(*slot);
                                removed.extend(children.iter().copied());
                            }
                            keep
                        });
                        for node in &removed {
                            assert!(tree.get(*node).is_none(), "{node:?} not freed after {step}");
                        }
                        freed.extend(removed);
                        step
                    }
                };
                check(&tree, host, &model, &step);
            }
        }
    }

    /// Panics in its `on_detach`.
    #[derive(Clone, PartialEq)]
    struct PanicOnDetach;

    struct PanicOnDetachNode;

    impl crate::modifier::ModifierNodeElement for PanicOnDetach {
        type Node = PanicOnDetachNode;
        fn create(&self) -> PanicOnDetachNode {
            PanicOnDetachNode
        }
        fn update(&self, _: &mut PanicOnDetachNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    impl crate::modifier::ModifierNode for PanicOnDetachNode {
        fn on_detach(&mut self, _: &mut crate::node_context::NodeContext<'_>) {
            panic!("on_detach panics");
        }
    }

    /// A panic in a moved node's `on_detach` leaves nothing of the pass's removals for the next
    /// (D1924): what's above a node and the cursor through them start again. A frame's start
    /// runs the lifecycle again, which forgets them too; this is for a caller that goes on.
    #[test]
    fn a_panic_in_a_moved_node_s_on_detach_forgets_the_pass_s_removals() {
        let mut tree =
            UiTree::new(Density::new(1.0), Box::new(crate::fixed_text::FixedTextMeasurer::new()));
        let root = tree.root_id();
        let (one, two) = (
            NodeId::from_ref(tree.insert(root.to_ref(), 0, layout_node())),
            NodeId::from_ref(tree.insert(root.to_ref(), 1, layout_node())),
        );
        let moved = NodeId::from_ref(tree.insert(one.to_ref(), 0, layout_node()));
        tree.set_modifier(moved, crate::modifier::Modifier.then_custom(PanicOnDetach));
        tree.run_lifecycle();
        tree.remove(one.to_ref(), 0, 1);
        tree.attach(two.to_ref(), 0, moved.to_ref());
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tree.run_lifecycle()));
        assert!(run.is_err());
        assert!(tree.removed_parents.is_empty());
        assert_eq!(tree.removed_detached, 0);
        assert!(tree.detaching_moved.is_empty());
    }
}
