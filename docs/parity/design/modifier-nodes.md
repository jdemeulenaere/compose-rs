# Design: `Modifier.Node` infrastructure on par with Compose's

The spec for the modifier-node features of the [ui-core ledger](../ui-core.md) §3: 3.4 `modifier-node-lifecycle`, 3.5 `delegating-node`, 3.6 `node-traversal`, 3.7 `observer-node`, 3.8 `composition-locals-in-nodes`, 3.9 `modifier-locals`, 3.10 `parent-data`, 3.12 `remeasurement`, the node half of 3.15 `on-placed`, and the element-diffing rules behind divergence §5.5 ("any modifier change measures its node again"). Placement, coordinates and lookahead are in [placement-and-lookahead.md](placement-and-lookahead.md); focus and semantics nodes in [focus-and-semantics.md](focus-and-semantics.md). Both build on this document's chain entries, kinds, contexts and handles.

Compose paths are relative to `frameworks/support/compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/` at androidx-main `d153d87ed76`; compose-rs paths to the repository root.

## 1. What Compose does

### The node and its life

`Modifier.Node` (`Modifier.kt:168-356`) is an abstract class every modifier node extends. Beside the subclass's state it carries:

- `kindSet` and `aggregateChildKindSet` (`Modifier.kt:194-200`): bit sets of the node kinds it implements, and of those of every node after it in its chain, so walks prune whole chains (`NodeChain.kt:284-292`). The kinds are listed in `node/NodeKind.kt:76-166` (`Any`, `Layout`, `Draw`, `Semantics`, `PointerInput`, `Locals`, `ParentData`, `OnRemeasured`, `GlobalPositionAware`, `ApproachMeasure`, `FocusTarget`, `FocusProperties`, `FocusEvent`, `KeyInput`, `RotaryInput`, `CompositionLocalConsumer`, `SoftKeyboardKeyInput`, `Traversable`, `BringIntoView`, `Unplaced`, `IndirectPointerInput`, `OnPlaced`, `BeyondBoundsLayout`), computed once per class from the interfaces it implements (`NodeKind.kt:216-296`).
- `parent`/`child` links within its layout node's chain and its `coordinator` (the layout layer it draws and measures in).
- `isAttached` (`Modifier.kt:222`), set by `markAsAttached` and cleared by `markAsDetached`, and the two-step attach (`markAsAttached` for the whole subtree first, then `runAttachLifecycle`, `Modifier.kt:250-283`) so that `onAttach` sees every node of the hierarchy marked attached.
- `onAttach`, `onDetach`, `onReset` (`Modifier.kt:316-343`); `coroutineScope` (`Modifier.kt:185-192`: a child job of the owner's context, created on first use, cancelled with `ModifierNodeDetachedCancellationException` in `markAsDetached`, `Modifier.kt:285-298`); `sideEffect` (`Modifier.kt:351-353`: run when the owner ends applying changes); `shouldAutoInvalidate` (`Modifier.kt:240-241`).

`DelegatableNode` (`node/DelegatableNode.kt:48-90`) adds `node`, `onDensityChange`, `onLayoutDirectionChange` (called head to tail by `LayoutNode.density`/`layoutDirection` setters, `node/LayoutNode.kt:758-777`) and `RegistrationHandle`. Its extension functions are the node's view of the tree: `requireDensity`, `requireLayoutDirection`, `requireLayoutCoordinates`, `requireGraphicsContext` (`DelegatableNode.kt:388-420`), `invalidateSubtree`, `invalidateMeasurementForSubtree`, `invalidateDrawForSubtree` (`DelegatableNode.kt:428-475`), and the internal visitors (below, "Delegation and traversal").

**When the lifecycle runs.** `LayoutNode.attach` (`LayoutNode.kt:501-562`) marks its chain attached, attaches its children (recursively, each marking then running), then runs its own chain's `onAttach`: so `onAttach` runs **children before parents**, head to tail within a chain (`NodeChain.kt:301-326`). `LayoutNode.detach` (`LayoutNode.kt:572-614`) runs its chain's `onDetach` tail to head, then detaches the children, then marks its chain detached: `onDetach` runs **parents before children**. A node reused or deactivated (`onReuse`, `onDeactivate`, `LayoutNode.kt:1454-1495`) goes through `resetModifierState` = `NodeChain.resetState` (`NodeChain.kt:242-246`): `onReset` tail to head, then `onDetach`, then marked detached; on reuse it is marked attached and `onAttach` runs again (`LayoutNode.kt:1471-1472`), **before** the new content's modifier updates are applied.

### Elements and the chain diff

`ModifierNodeElement<N>` (`node/ModifierNodeElement.kt:38-105`) is `create(): N`, `update(node: N)`, `equals`/`hashCode` (abstract), and `InspectorInfo.inspectableProperties()` (default: the class name and its properties by reflection). `NodeChain.updateFrom` (`NodeChain.kt:106-226`) diffs the new element list against the old: a linear pass while the sizes agree, falling back to a Myers diff (`node/MyersDiff.kt`) for the rest at the first structural change. For each pair, `actionForModifiers` (`NodeChain.kt:767-775`): equal elements are **reused** without update; elements of the same class are **updated** (`updateNode`, `NodeChain.kt:631-654`) and then **auto-invalidated**; otherwise the old node is removed and a new one created.

**Auto-invalidation** (`NodeKind.kt:298-405`) invalidates by the node's kinds, not wholesale, for each of three phases (updated, inserted, removed), skipping updates of a node whose `shouldAutoInvalidate` is false: `Layout` → `invalidateMeasurement` (and the coordinator released on removal); `OnRemeasured` → `invalidateMeasurements`; `OnPlaced` → `requestRelayout`; `GlobalPositionAware` → observer count and `invalidateOnPositioned`; `Draw` → `invalidateDraw`; `Semantics` → semantics invalidated; `ParentData` → `invalidateParentData`; `FocusProperties` (if it sets `canFocus`) → `invalidateFocusProperties`; `FocusEvent` → `invalidateFocusEvent`; `IndirectPointerInput` removed → cancel. A colour change of a background redraws it; it doesn't measure.

### Specialised kinds and their invalidation

- `LayoutModifierNode` (`node/LayoutModifierNode.kt:50-176`): `measure`, the four intrinsics (default: measure with intrinsic measurables, `NodeMeasuringIntrinsics`), and `invalidateMeasurement` (`requireLayoutNode().invalidateMeasurements()`), `invalidatePlacement` (`requestRelayout`), `invalidateLayer`, `remeasureSync` (`forceRemeasure`), `requestRemeasure` (internal), `invalidateRulers`, `updateLayerBlock` (`:457`).
- `DrawModifierNode` (`node/DrawModifierNode.kt:31-57`): `ContentDrawScope.draw()`, `onMeasureResultChanged` (default `invalidateDraw`), `invalidateDraw` (`requireCoordinator(Any).invalidateLayer()`), `dispatchDraw` to delegates.
- `SemanticsModifierNode` (`node/SemanticsModifierNode.kt:40-128`): `shouldClearDescendantSemantics`, `shouldMergeDescendantSemantics`, `isImportantForBounds`, `applySemantics`, `invalidateSemantics`.
- `ObserverModifierNode` (`node/ObserverModifierNode.kt:29-68`): `onObservedReadsChanged`, called once after a snapshot changes what `observeReads { }` read; observe again to keep observing.
- `CompositionLocalConsumerModifierNode` (`node/CompositionLocalConsumerModifierNode.kt:35-75`): `currentValueOf(local)` reads the layout node's `compositionLocalMap` (captured where the node was composed); a new map auto-invalidates consumer nodes (`LayoutNode.kt:789-803`).
- `ModifierLocalModifierNode` (`modifier/ModifierLocalModifierNode.kt:121-180`): `providedValues`, `provide(key, value)`, `ModifierLocal<T>.current` (the nearest provider among the nodes before it in its chain and its ancestors' chains, else the local's default). Legacy `modifierLocalProvider`/`modifierLocalConsumer` elements re-run consumers when providers change (`modifier/ModifierLocalManager.kt:41-140`).
- `ParentDataModifierNode` (`node/ParentDataModifierNode.kt:33-45`): `Density.modifyParentData(parentData: Any?): Any?`, folded tail to head into `Measurable.parentData`; `invalidateParentData`.
- `GlobalPositionAwareModifierNode` (`node/GlobalPositionAwareModifierNode.kt:36-44`): `onGloballyPositioned(coordinates)`.
- `LayoutAwareModifierNode` (`node/LayoutAwareModifierNode.kt:39-58`): `onPlaced(coordinates)` (after its parent and itself are placed, before what's inside it is), `onRemeasured(size)` (`MeasuredSizeAwareModifierNode`); `UnplacedAwareModifierNode.onUnplaced`. `onRemeasured` runs in `NodeCoordinator.onMeasured` without read observation (`node/NodeCoordinator.kt:387-393`); `onPlaced` in `NodeCoordinator.onPlaced` (`:534-536`).

### Delegation and traversal

`DelegatingNode` (`node/DelegatingNode.kt:30-278`) keeps its delegates in a singly linked list (newest at the head); `delegate(node)` (`:77-134`) points the delegate's `node` at the delegating root, merges its kinds into the owner's (`updateNodeKindSet`, `:194-237`), and if the owner is attached attaches the delegate at once (mark, run, auto-invalidate inserted); `undelegate` (`:136-178`) detaches and recomputes. A node may delegate to at most one `LayoutModifierNode` unless it implements `LayoutModifierNode` itself (`validateDelegateKindSet`, `:180-192`). Lifecycle propagates to delegates (`:247-277`: attach marks delegates first and runs them before the owner; detach runs the owner first). `dispatchForKind` (`DelegatableNode.kt:573-627`) calls the node if it implements the kind, else each delegate of that kind, in the order they were delegated to.

The internal visitors (`DelegatableNode.kt:92-357`) walk the chain links and the layout tree, pruning by `aggregateChildKindSet`: `visitAncestors(mask, includeSelf)`, `nearestAncestor`, `visitChildren(mask, zOrder)` (the first matching node down each branch, children in z order when asked), `visitSubtreeIf`, `visitLocalDescendants`/`visitLocalAncestors` (one chain). `TraversableNode` (`node/TraversableNode.kt:28-218`) is the public face: a `traverseKey`, `findNearestAncestor(key)`, `traverseAncestors`, `traverseChildren`, `traverseDescendants` with `TraverseDescendantsAction` (`ContinueTraversal`, `SkipSubtreeAndContinueTraversal`, `CancelTraversal`), each in a by-key and a by-own-type flavour.

## 2. Where compose-rs stands

- **Elements** (`crates/compose-ui/src/modifier.rs:722-794`): built-ins are `Element` enum variants with `same_kind` (variant or `TypeId`) and `same` (equality); user elements are `Element::Custom(Rc<dyn ModifierNodeElement>)` with `create() -> Box<dyn ModifierNode>`, `update(&self, &mut dyn ModifierNode)` (downcasting), `same(&self, &dyn ModifierNodeElement)` and `name()` (`modifier.rs:861-872`). 35 custom elements across the crates.
- **Nodes** (`modifier.rs:883-978`): `ModifierNode` with capability getters (`as_layout`, `as_draw`, `as_pointer_input`, `as_semantics`, `as_focus_target`, `as_key_input`, `as_global_position_aware`, `as_selectable`) and compose-rs extras (`is_repaint_boundary`, `hit_clip`, `is_pointer_observer`, `bring_into_view`, `is_beyond_bounds_layout`, …); lifecycle `on_detach(&mut DetachContext)` (resources: a paragraph handed back to the text measurer) and `on_reset()` (D431). No attach, no `is_attached`, no coroutine scope, no way for a node to reach its tree outside a callback that is handed an `EventContext`. 52 `impl ModifierNode` across the crates.
- **The chain** (`crates/compose-ui/src/tree.rs:143-188`): `LayoutNode.chain: Vec<Box<dyn ModifierNode>>` with `capabilities: SmallVec<[u8; 16]>` (8 bits, all used: `tree.rs:86-96`) and `all_capabilities`. `refresh_chain` (`tree.rs:837-977`) recomputes them, the layout layers, repaint boundaries, inner layers, overlays, positioned registration, z-index and the fixed `ParentData` after any change.
- **Diffing** (`tree.rs:739-800`, `match_kinds` `tree.rs:1361-1404`): the same fast path as Compose (same kinds in the same order: update in place), else a longest common subsequence of kinds. Every element change calls `invalidate_measure(node)`, a colour change included (divergence §5.5).
- **Invalidation from nodes**: only from event handlers, through `EventContext::invalidate_draw`/`invalidate_measure` (`crates/compose-ui/src/input.rs:319-330`, applied after dispatch), and through observed state reads in measure and draw (`tree.rs:20-46`, `apply_observed_writes` `tree.rs:681-707`). Nodes that need more keep `OwnedState`s and read them while drawing (`nodes.rs:763-769`, `CombinedClickableNode::pressed`).
- **Reuse** (D431, D434, D435): reused or deactivated nodes are reset (`on_reset`) after the pass, after the new elements' updates, and never detached.
- **Parent data**: a fixed `ParentData { weight, alignment }` (`modifier.rs:578-584`) read from built-in elements.

## 3. Design

### 3.1 Chain entries and kinds

Replace `chain: Vec<Box<dyn ModifierNode>>` and `capabilities` with one vector of entries:

```rust
/// A node's kinds, as Compose's `NodeKind` masks (node/NodeKind.kt:76-166).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct NodeKinds(u32);

impl NodeKinds {
    // Compose's kinds, in its order.
    pub const LAYOUT: NodeKinds = NodeKinds(1 << 0);
    pub const DRAW: NodeKinds = NodeKinds(1 << 1);
    pub const SEMANTICS: NodeKinds = NodeKinds(1 << 2);
    pub const POINTER_INPUT: NodeKinds = NodeKinds(1 << 3);
    pub const LOCALS: NodeKinds = NodeKinds(1 << 4);
    pub const PARENT_DATA: NodeKinds = NodeKinds(1 << 5);
    pub const ON_REMEASURED: NodeKinds = NodeKinds(1 << 6);
    pub const GLOBAL_POSITION_AWARE: NodeKinds = NodeKinds(1 << 7);
    pub const APPROACH_MEASURE: NodeKinds = NodeKinds(1 << 8);
    pub const FOCUS_TARGET: NodeKinds = NodeKinds(1 << 9);
    pub const FOCUS_PROPERTIES: NodeKinds = NodeKinds(1 << 10);
    pub const FOCUS_EVENT: NodeKinds = NodeKinds(1 << 11);
    pub const KEY_INPUT: NodeKinds = NodeKinds(1 << 12);
    pub const ROTARY_INPUT: NodeKinds = NodeKinds(1 << 13);
    pub const COMPOSITION_LOCAL_CONSUMER: NodeKinds = NodeKinds(1 << 14);
    pub const SOFT_KEYBOARD_KEY_INPUT: NodeKinds = NodeKinds(1 << 15);
    pub const TRAVERSABLE: NodeKinds = NodeKinds(1 << 16);
    pub const BRING_INTO_VIEW: NodeKinds = NodeKinds(1 << 17);
    pub const UNPLACED: NodeKinds = NodeKinds(1 << 18);
    pub const INDIRECT_POINTER_INPUT: NodeKinds = NodeKinds(1 << 19);
    pub const ON_PLACED: NodeKinds = NodeKinds(1 << 20);
    pub const BEYOND_BOUNDS_LAYOUT: NodeKinds = NodeKinds(1 << 21);
    // compose-rs's own, formerly capability bits or chain scans.
    pub const OBSERVED: NodeKinds = NodeKinds(1 << 24);      // was `capability::CUSTOM`
    pub const GRAPHICS_LAYER: NodeKinds = NodeKinds(1 << 25);
    pub const OVERLAY: NodeKinds = NodeKinds(1 << 26);
    pub const FOCUS_REQUESTER: NodeKinds = NodeKinds(1 << 27);
    pub const SELECTABLE: NodeKinds = NodeKinds(1 << 28);
    pub const REPAINT_BOUNDARY: NodeKinds = NodeKinds(1 << 29);
}

pub(crate) struct ChainEntry {
    pub(crate) node: Box<dyn ModifierNode>,
    /// Its kinds and its delegates' (Compose's `kindSet`).
    pub(crate) kinds: NodeKinds,
    /// Its kinds and those of every entry after it (Compose's `aggregateChildKindSet`).
    pub(crate) aggregate: NodeKinds,
    /// Stable across chain diffs and reuse: what handles, hit paths and the focus name.
    pub(crate) id: EntryId,
    pub(crate) state: EntryState,   // u8 flags: ATTACHED, ATTACH_RAN, AWAITING_INSERT_INVALIDATION, AWAITING_UPDATE_INVALIDATION
    /// `coroutine_scope()`, created on first use, cancelled after `on_detach`.
    pub(crate) scope: Option<compose_runtime::Owned<compose_runtime::CoroutineScope>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct EntryId(u32);   // from a per-tree counter
```

`LayoutNode` keeps `kinds: NodeKinds` (the head's aggregate, the old `all_capabilities`) and `layout_indices`. Kinds are probed per instance once when the entry is created (today's `capabilities_of`, `tree.rs:98-123`, extended with the new getters), and again when its delegates change (§3.6); they never change otherwise, as in Compose. The probe costs what it costs today (a virtual call per getter, once per created node).

The compose-rs-only behaviours that today scan the chain by downcast (`GraphicsLayerNode`, `OverlayNode`, `FocusRequesterNode`, `OnFocusChangedNode`) become kinds, so walks prune by bits rather than by `downcast_ref` on every entry.

### 3.2 The node trait

```rust
pub trait ModifierNode: Any {
    // Kinds (one getter each; default None/false). Existing: as_layout, as_draw,
    // as_pointer_input, as_semantics, as_focus_target, as_key_input,
    // as_global_position_aware, as_selectable. New:
    fn as_parent_data(&mut self) -> Option<&mut dyn ParentDataModifierNode> { None }
    fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> { None }
    fn as_measured_size_aware(&mut self) -> Option<&mut dyn MeasuredSizeAwareModifierNode> { None }
    fn as_unplaced_aware(&mut self) -> Option<&mut dyn UnplacedAwareModifierNode> { None }
    fn as_observer(&mut self) -> Option<&mut dyn ObserverModifierNode> { None }
    fn as_modifier_local(&mut self) -> Option<&mut dyn ModifierLocalModifierNode> { None }
    fn as_traversable(&self) -> Option<&dyn TraversableNode> { None }
    fn is_composition_local_consumer(&self) -> bool { false }
    // as_approach_layout: placement-and-lookahead.md; as_focus_properties, as_focus_event,
    // as_focus_requester: focus-and-semantics.md.

    // Lifecycle, as Modifier.Node's.
    fn on_attach(&mut self, _cx: &mut NodeContext<'_>) {}
    fn on_detach(&mut self, _cx: &mut NodeContext<'_>) {}
    fn on_reset(&mut self, _cx: &mut NodeContext<'_>) {}          // was `on_reset(&mut self)`
    fn on_density_change(&mut self, _cx: &mut NodeContext<'_>) {}
    fn on_layout_direction_change(&mut self, _cx: &mut NodeContext<'_>) {}
    /// The node is dropped (removed from its chain for good, or its layout node released):
    /// give back what it holds outside itself. compose-rs's; was `on_detach(DetachContext)`.
    fn on_release(&mut self, _cx: &mut ReleaseContext<'_>) {}
    /// As `shouldAutoInvalidate`.
    fn should_auto_invalidate(&self) -> bool { true }
    /// Its delegates, for a delegating node (§3.7).
    fn delegates(&mut self) -> Option<&mut Delegates> { None }

    // compose-rs extras stay: is_repaint_boundary, hit_clip(_at), is_pointer_observer,
    // bring_into_view, is_beyond_bounds_layout, lay_out_beyond_bounds, end_beyond_bounds.
}
```

**Why `on_release` is split from `on_detach`.** Today's `on_detach` hands paragraphs back to the text measurer (D431's caches). Compose's `onDetach` also runs when a node is reused or deactivated (§1), and the node is attached again after; if compose-rs's detach released paragraphs, every lazy item reuse would lay its text out again, which is what D431 measured as losing most of reuse's gain. So Compose's `onDetach` is the lifecycle callback, and `on_release` is called once, when the node itself is dropped (`free_subtree`, an entry removed by a chain diff, an undelegated delegate). `ReleaseContext` is today's `DetachContext` renamed (the text measurer).

### 3.3 Contexts and handles

A node reaches its tree through a context while its code runs, and through a handle afterwards.

```rust
/// What a node's callback reaches: its own place in the tree and the tree's services.
pub struct NodeContext<'a> {
    pub(crate) tree: TreeAccess<'a>,   // &mut UiTree, the running entry taken out of its chain
    pub(crate) node: NodeId,
    pub(crate) entry: EntryId,
    pub(crate) delegate: DelegatePath, // 0: the entry's own node; else its delegate (§3.7)
}

impl NodeContext<'_> {
    pub fn handle(&self) -> NodeHandle;
    pub fn is_attached(&self) -> bool;
    pub fn coroutine_scope(&mut self) -> compose_runtime::CoroutineScope;
    pub fn side_effect(&mut self, effect: impl FnOnce() + 'static);
    pub fn require_density(&self) -> Density;
    pub fn require_layout_direction(&self) -> LayoutDirection;
    pub fn require_layout_coordinates(&self) -> LayoutCoordinates;   // placement doc
    pub fn current_value_of<T: Clone + 'static>(&self, local: &'static CompositionLocal<T>) -> T;
    pub fn observe_reads<R>(&mut self, block: impl FnOnce() -> R) -> R;
    // invalidation (§3.4), traversal (§3.8), modifier locals (§3.9)
}

/// A node, named from outside its callbacks (a coroutine, a callback, a stored closure):
/// `Copy`, `'static`, `!Send`; never stale (a gone node answers `false`/`None`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeHandle {
    tree: TreeKey,      // the tree's key in the thread's registry
    node: NodeId,
    entry: EntryId,
    delegate: DelegatePath,
}

impl NodeHandle {
    pub fn is_attached(&self) -> bool;
    pub fn invalidate_draw(&self);
    pub fn invalidate_measurement(&self);
    pub fn invalidate_placement(&self);
    pub fn invalidate_layer(&self);
    pub fn invalidate_semantics(&self);
    pub fn invalidate_parent_data(&self);
    pub fn invalidate_subtree(&self);
    pub fn invalidate_measurement_for_subtree(&self);
    pub fn invalidate_draw_for_subtree(&self);
    /// Runs `f` with the node and its context now, if the tree is free (a coroutine
    /// resuming between frames or in a frame's task step); `None` if the node is gone or
    /// the tree is busy (a debug assertion: a handle must not be used from inside a pass).
    pub fn with_node<N: ModifierNode, R>(
        &self,
        f: impl FnOnce(&mut N, &mut NodeContext<'_>) -> R,
    ) -> Option<R>;
}
```

- **The registry.** Each `UiTree` registers itself at creation in a thread-local registry under a `TreeKey(u32)`, as a `Weak` to its host's tree cell; the host registers the closure through which a handle borrows the tree (`UiHost` holds the composition, whose applier cell owns the tree, `crates/compose-ui/src/host.rs:289-291`). `FocusRequester`'s thread-local registry (`crates/compose-ui/src/focus_requester.rs:66-73`, D240) is the precedent.
- **The request queue.** Handle invalidations never touch the tree directly: they push onto the tree's `TreeRequests`, which is today's `FocusRequests` (`focus_requester.rs:13-50`) renamed and extended with `invalidations: RefCell<Vec<(NodeId, EntryId, Invalidation)>>`, and wake the host. The tree drains it at the points where it drains observed writes today: at the start of a frame after the tasks ran (`host.rs:313-320`), at the end of each composition pass (`on_end_changes`), and in each round of `UiHost::lay_out` (`host.rs:604-622`) before `layout`. So an invalidation asked from a coroutine, a `side_effect` or an event callback is measured or drawn in the frame that follows it, or in the same frame when asked before its layout, as Compose's.
- **Contexts apply at once.** Inside a callback the tree is at hand: `NodeContext::invalidate_*` set the flags directly, as `EventContext`'s requests do today after dispatch. `EventContext` becomes a wrapper over a `NodeContext` plus the event's own data (pointer position), so pointer, key and semantics handlers get the whole node API.
- **Coroutine scope.** `coroutine_scope()` creates the entry's `Owned<CoroutineScope>` on first use (as Compose's lazy `scope`) and returns its `Copy` handle; marking the entry detached cancels it after `on_detach` returns (`Modifier.kt:285-298`). Tasks run on the runtime executor between frames or in a frame's task step, when the tree is free, so `handle.with_node` works from them. `PointerInputDriver` (`crates/compose-ui/src/pointer_input.rs:559-566`) keeps its own scope for its gesture task; Compose's `SuspendingPointerInputModifierNode` uses the node's scope, which a later input-area step can adopt.
- **`side_effect`** queues `effect` on the tree; the queue runs at `on_end_changes`, after the pass's lifecycle callbacks (§3.5), as Compose's `registerOnEndApplyChangesListener`. Outside a composition pass (from an event), it runs at the end of the dispatch, with the dispatch's actions.

### 3.4 Invalidation

Each Compose invalidation maps onto an existing tree operation, scoped to the entry where Compose scopes it to the coordinator:

| Compose | compose-rs (`UiTree`) |
|---|---|
| `DrawModifierNode.invalidateDraw` (`requireCoordinator(Any).invalidateLayer()`) | Draw again the group the entry draws in: the inner layer of the nearest graphics-layer entry before it, if that is an inner layer (`tree.rs:579-590`), else what draws the node before its inner layers (`invalidate_draw_before_inner_layers`, `tree.rs:602-604`). A new `invalidate_draw_of_entry(node, entry)`. |
| `LayoutModifierNode.invalidateMeasurement`, `invalidateMeasurements` | `invalidate_layout_of(node)` (`tree.rs:519-521`); inside a lookahead scope, the lookahead measurement too (placement doc §7.6). |
| `invalidatePlacement`, `requestRelayout` | Placement doc §3.4 (`needs_placement`). Until the placement pass lands: `invalidate_layout_of`, documented as the fused pass's cost. |
| `invalidateLayer` | The graphics layer at or before the entry records its content and properties again (`OwnedLayer.invalidate`); with none, what draws the node draws it again, as `wrappedBy`'s (`NodeCoordinator.kt:1475-1481`, D1180). |
| `remeasureSync` / `Remeasurement.forceRemeasure` | `UiTree::remeasure_now(node)`: `measure_node(node, last constraints)` and its placement, synchronously. Allowed from a `NodeContext` outside a measure or draw pass (an event, a task); a debug assertion inside a pass. Ledger 3.12. |
| `requestRemeasure` (internal) | `invalidate_measure(node)`. |
| `invalidateSemantics` | The node's cached configuration dirty (focus-and-semantics doc §B.4). |
| `invalidateParentData` | Recompute the node's parent data (§3.10); if it changed, `invalidate_measure(parent)` (today's rule, `tree.rs:758-760`). |
| `invalidateSubtree` | For the node and every descendant: `needs_measure`, every boundary and inner layer dirty, semantics dirty (`LayoutNode.kt:1342`, D1180). |
| `invalidateMeasurementForSubtree` | `needs_measure` on every node of the subtree, `subtree_needs_measure` up the ancestors. |
| `invalidateDrawForSubtree` | Every retained drawing and inner layer of the subtree dirty (the subtree part of `invalidate_draw`, `tree.rs:565-577`). |
| `DrawModifierNode.onMeasureResultChanged` | Called on the draw node when the measured size of its layer changes (in `measure_layer`, `crates/compose-ui/src/measure.rs:339-350`); the default does nothing, since a remeasured node already draws again there (`measure.rs:183-186`). |
| `ParentDataModifierNode`, `GlobalPositionAware`, `OnPlaced`, `FocusProperties`, `FocusEvent` | Their own docs (§3.10, placement doc, focus doc). |

All of them are no-ops on a detached entry: as Compose's `invalidateDraw` and the three `…ForSubtree` ones (`if (node.isAttached)`), where its `invalidateMeasurement`, `invalidatePlacement`, `invalidateLayer` and `invalidateSemantics` fail without a coordinator (D1178).

### 3.5 The lifecycle in compose-rs's passes

compose-rs applies changes during composition (the applier is called as the composer goes), and application code may not write state then (D434). So lifecycle callbacks are **queued during the pass and run at `on_end_changes`** (`tree.rs:1331-1341`), in Compose's order, before `side_effect`s:

1. **Inserted subtrees** (`Applier::insert` into an attached parent, `Applier::attach` of a paused composition's node, `tree.rs:1179-1220`): marked attached at insertion, pre-order (so `is_attached` is true in every `on_attach`); the `on_attach`s run at the end of the pass in Compose's post-order: a node's children before it, head to tail in each chain, delegates before their owner (`DelegatingNode.kt:265-268`). The tree keeps the roots of the subtrees attached in the pass (`attached_roots: Vec<NodeId>`), and walks only them.
2. **Removed subtrees** (`Applier::remove`, `tree.rs:1222-1233`; Compose detaches at removal): `on_detach` runs pre-order, tail to head in each chain, the owner before its delegates; then the entries are marked detached (scopes cancelled). The nodes stay allocated until `release` (rule R1, docs/06); `release` calls `on_release`. A node moved among its parent's children (`Applier::move_nodes`) stays attached, its nodes untold, as Compose's `LayoutNode.move`. A node removed and attached again in the same pass is, as implemented, treated as a move too: its queued detach and attach cancel out. Compose's movable content detaches and attaches it (`removeAt`/`insertAt`; ModifierNodeAttachOrderTest's `nodesAreAttachedAndDetachedWhenMovedBetweenLayouts`): to follow when the runtime has movable content.
3. **Chain changes** (`set_modifier`): removed entries run `on_detach` then `on_release`; inserted entries are marked attached and run `on_attach`; both queued, in chain order (Compose attaches inserted nodes in `structuralUpdate` as it goes).
4. **Reuse and deactivation** (`Applier::reuse`, `deactivate`, `tree.rs:1263-1324`): Compose's `resetState` then attach: `on_reset` tail to head, `on_detach` tail to head, marked detached; on reuse, marked attached and `on_attach` head to tail. This revises D431 (which kept nodes attached and only reset them): the nodes and their caches stay, but they go through detach and attach, so a node whose `on_attach` starts work (a coroutine) restarts it for the new content, as in Compose. **Known difference, kept from D434/D435:** compose-rs runs the reset after the new content's element updates (the updates happen during the pass; the callbacks after it), where Compose resets before updating. ModifierNodeReuseAndDeactivationTest's cases that assert that interleaving are adapted to compose-rs's order and say so.
5. **Density and layout direction**: `set_layout_direction` (`tree.rs:710-719`) and the per-node density of §3.9 queue `on_layout_direction_change`/`on_density_change` for the node's entries, head to tail, run with the lifecycle callbacks.

The callbacks run with the entry out of its chain (the `Taken` pattern of `measure_layer`, `measure.rs:262-288`), so its code may traverse and invalidate the rest of the tree through its context, but never reaches itself through it.

### 3.6 Elements and the diff

**A typed element trait**, as Compose's `ModifierNodeElement<N>`:

```rust
pub trait ModifierNodeElement: 'static {
    type Node: ModifierNode;
    fn create(&self) -> Self::Node;
    /// Updates `node`, which an element of this type created (Compose's `update(node: N)`).
    fn update(&self, node: &mut Self::Node);
    /// Whether `other` is equal, so the node needs no update (Compose's `equals`).
    fn same(&self, other: &Self) -> bool;
    fn name(&self) -> &'static str { std::any::type_name::<Self>() }
    /// As `InspectorInfo.inspectableProperties()`: what dumps show.
    fn inspectable_properties(&self, info: &mut InspectorInfo) { info.name = Some(self.name()); }
}

/// The object-safe form the chain stores (`Element::Custom(Rc<dyn AnyModifierNodeElement>)`).
pub trait AnyModifierNodeElement: Any { /* create_boxed, update_dyn, same_dyn, … */ }
impl<E: ModifierNodeElement> AnyModifierNodeElement for E { /* downcasts once, here */ }
```

`#[derive(PartialEq)]` elements write `fn same(&self, other: &Self) -> bool { self == other }`. The 35 custom elements lose their `downcast_mut`/`downcast_ref` boilerplate. The blanket impl is a handful of small functions per element type, which compile-speed review should accept; the alternative is keeping today's untyped trait and adding only the new methods. `InspectorInfo { name: Option<&'static str>, value: Option<String>, properties: Vec<(&'static str, String)> }` feeds `Modifier`'s `Debug` and `dump_tree`; built-in elements fill it from their data. The inspector tests stay n/a (the ledger's rows); the API exists for parity and dumps.

**The diff** keeps compose-rs's rules, which are Compose's: `same` elements are reused untouched; elements of the same kind (variant or type) are updated in place; others are replaced. Two changes:

1. **Auto-invalidation by kinds** replaces `set_modifier`'s blanket `invalidate_measure` (`tree.rs:756-757`, `795-796`), per Compose's table (§1, `NodeKind.kt:333-405`), for updated entries (unless `should_auto_invalidate` is false), inserted entries and removed entries. A built-in `Element` variant has fixed kinds: `Padding`, `Offset`, `MinimumTouchTarget`, `Size`, `Fill` are `LAYOUT`; `Weight`, `Align` are `PARENT_DATA`; `ZIndex` invalidates its parent's drawing order (today's `tree.rs:947-953`); `Background`, `Border` are `DRAW`; `Clip` is `DRAW | GRAPHICS_LAYER` (its layer's properties); `Clickable`, `Toggleable` are `POINTER_INPUT | SEMANTICS | FOCUS_TARGET | KEY_INPUT | DRAW`; `Focusable` is `FOCUS_TARGET | SEMANTICS`; `OnFocusChanged` is `FOCUS_EVENT`; `OnKeyEvent` is `KEY_INPUT`; `TestTag`, `Semantics` are `SEMANTICS`. A change to the chain's structure that adds or removes a layout entry still measures the node (its layers change); one that adds or removes a graphics layer still goes through `refresh_chain`'s boundary logic. This removes divergence §5.5 and makes `CustomLayoutAndMeasureTest.updatingModifierIsNotCausingParentsRelayout` and the invalidation counts of `ModifierNodeReuseAndDeactivationTest` portable.
2. **Compose's Myers diff for structural changes.** `match_kinds` finds a longest common subsequence, as Myers does, but breaks ties differently when the same kind appears several times (which old node a new element keeps). `NodeChainTests` checks node identities through such edits. Port `MyersDiffTests` as unit tests of `match_kinds` first; if they or `NodeChainTests` disagree, port Compose's algorithm (`node/MyersDiff.kt`): it runs only on structural changes, rare in frames.

### 3.7 Delegation

As built (D1169, D1170, D1173, D1179): delegates are named by their own `EntryId` instead of a `DelegatePath` (an index shifts when an earlier delegate goes); `delegate_attached(node, context)` attaches inside the call from a callback, `delegate` once the code returns; compose-rs's extras stay the chain's node's. The rest is as below.

**Reaching the delegator.** Compose's delegate calls its `node`, or a sibling, through a field. Here a delegate can't reach its owner while either runs: the owner is out of its chain, its delegates inside it, and the borrow is the owner's. So what they share goes in an `Rc` they both hold (a `Cell` or `RefCell` of the clickable's interaction state), made in `create` before the delegate is, and what a delegate asks *of* its owner goes through a deferred handle:

```rust
struct Shared { owner: Cell<Option<NodeHandle>>, pressed: Cell<bool> }

// The owner, in `create`: `self.delegates.delegate(Gesture { shared: shared.clone() })`.
// The owner, in `on_attach`: `self.shared.owner.set(Some(context.handle()));`, so what its
// delegates run after it has it; a delegate's `on_attach` runs first, so it doesn't count on it.
// A delegate: `self.shared.pressed.set(true); if let Some(owner) = self.shared.owner.get() { owner.invalidate_draw(); }`
```

A handle's invalidations are queued, so they work with the tree busy; running the owner's code (`with_node`) doesn't (it answers `None` until the tree is free), so a delegate asks with `NodeContext::side_effect(move || { owner.with_node(…); })`.

A delegating node owns a `Delegates` container and returns it from `ModifierNode::delegates`:

```rust
pub struct Delegates {
    entries: SmallVec<[DelegateEntry; 2]>,   // in delegation order
    /// Delegated or undelegated since the tree last looked: the tree syncs kinds and lifecycle.
    changed: bool,
    pending_release: SmallVec<[Box<dyn ModifierNode>; 1]>,
}
struct DelegateEntry { node: Box<dyn ModifierNode>, kinds: NodeKinds, id: EntryId, state: EntryState, scope: Option<Owned<CoroutineScope>> }

/// A typed reference to a delegate, as the node Compose's `delegate()` returns.
#[derive(Clone, Copy)]
pub struct Delegate<N> { id: EntryId, _node: PhantomData<fn() -> N> }

impl Delegates {
    pub fn delegate<N: ModifierNode>(&mut self, node: N) -> Delegate<N>;
    pub fn undelegate<N: ModifierNode>(&mut self, delegate: Delegate<N>);
    pub fn get<N: ModifierNode>(&self, delegate: Delegate<N>) -> &N;
    pub fn get_mut<N: ModifierNode>(&mut self, delegate: Delegate<N>) -> &mut N;
}
```

- `delegate` may be called anywhere the node's code runs (`create`, `update`, a callback). After any node code returns, the tree checks `changed` on the entries it ran and syncs: new delegates take kinds, an `EntryId`, and, if the owner is attached, are marked attached, run `on_attach` and are auto-invalidated as inserted (`DelegatingNode.kt:110-131`); undelegated ones run `on_detach` (if attached), are auto-invalidated as removed, then `on_release`. The owner's `kinds` become its own probe's union the delegates' (recursively); if `LAYOUT` appeared or went, `refresh_chain` resizes the layers and measures the node (Compose's `syncCoordinators`).
- **Rules**, as Compose's: at most one delegate of kind `LAYOUT` unless the owner implements `as_layout` itself (`DelegatingNode.kt:180-192`): a panic naming both types. A node already delegated (by another owner) can't be delegated again: the container owns it by value, so this is a compile-time guarantee in Rust.
- **Dispatch** follows `dispatchForKind` (`DelegatableNode.kt:573-627`): for a kind, the node itself if its getter answers, else each delegate of that kind, recursively, in delegation order. Every walk that today calls `entry.as_x()` calls a helper `for_each_of_kind(entry, KIND, |node| …)` instead; a node without delegates costs one extra branch when its getter answers `None` but its kinds have the bit (never, without delegates). The layout layer of an entry is found by `layout_node_of(entry)` (Compose's `asLayoutModifierNode`, `DelegatableNode.kt:534-557`).
- **Paths**: a delegate is named by a `DelegatePath(u16)`: its index in a pre-order walk of the entry's delegate tree, 0 for the owner. Hit paths (`HitEntry`, `crates/compose-ui/src/dispatch.rs:15-22`), the focused target (focus doc) and handles carry `(entry, path)`.
- **Static delegation stays possible**: a node may keep a sub-node as a typed field and forward one getter to it (today's `CombinedClickableNode`, `nodes.rs:926-949`). That costs nothing but is not Compose's delegation (no separate lifecycle, one kind each); `Delegates` is the Compose-shaped tool.

### 3.8 Traversal

As built (D1171, D1172, D1177): the visitors are on `NodeContext`, the caller's own place being theirs to start from, and each traversal callback also gets the visited node's context, as Compose's visited node is a whole `DelegatableNode`. Traversals stop at overlays. The rest is as below.

**`Taken` ancestors.** A node whose code is running is out of its chain, with a `Taken` in its place: its kinds stay, its node doesn't answer. So a traversal from inside a callback finds neither its caller nor any node up the stack (a traversal's callback's own traversal doesn't find the node that started it, where Compose's would): keep what they need in the callback's closure.

Internal visitors on `UiTree`, as Compose's, walking chain entries and the layout tree, pruning by `aggregate` kinds:

```rust
#[derive(Clone, Copy)]
pub(crate) struct EntryRef { node: NodeId, entry: EntryId, delegate: DelegatePath }

impl UiTree {
    pub(crate) fn visit_ancestors(&mut self, from: EntryRef, kinds: NodeKinds, include_self: bool,
        f: &mut dyn FnMut(&mut dyn ModifierNode, EntryRef) -> ControlFlow<()>);
    pub(crate) fn nearest_ancestor(&mut self, from: EntryRef, kinds: NodeKinds) -> Option<EntryRef>;
    pub(crate) fn visit_children(&mut self, from: EntryRef, kinds: NodeKinds, z_order: bool, f: …);
    pub(crate) fn visit_subtree_if(&mut self, from: EntryRef, kinds: NodeKinds, z_order: bool,
        f: &mut dyn FnMut(&mut dyn ModifierNode, EntryRef) -> bool);
    pub(crate) fn visit_local_ancestors(…); pub(crate) fn visit_local_descendants(…);
}
```

- "Ancestors" are the entries before `from` in its chain (and its delegator), then the chains of its layout ancestors from their tail; "children" the first matching entry down each branch, the layout children taken in z order (`placement_order`, `tree.rs:387-409`) when asked, else in child order. Deactivated nodes are skipped, as Compose's attached check skips them. Overlays are separate roots, as Compose's popups: walks up stop at one, walks down skip them (D1177).
- `f` gets the visited node, never the tree, so the loop re-borrows the tree per step and allocates nothing (`visit_children`' branch stack is a `SmallVec`).
- The `ModifierNodeVisit*` tests drive these internal visitors from test code, outside callbacks, through test hooks in `compose::ui::platform` (as the ledger proposes, D260's rule for internals).

The public API, on `NodeContext` (the calling node is `from`) and on `UiTree` for test code:

```rust
/// As `TraversableNode`: a key shared by the nodes a traversal looks for.
pub trait TraversableNode: ModifierNode { fn traverse_key(&self) -> &'static TraverseKey; }
/// Compared by address: declare one as a `static`.
pub struct TraverseKey { name: &'static str }
pub enum TraverseDescendantsAction { ContinueTraversal, SkipSubtreeAndContinueTraversal, CancelTraversal }

impl NodeContext<'_> {
    pub fn find_nearest_ancestor_with_key(&mut self, key: &'static TraverseKey) -> Option<&mut dyn TraversableNode>;
    pub fn find_nearest_ancestor<T: TraversableNode>(&mut self) -> Option<&mut T>;   // same type and key
    pub fn traverse_ancestors_with_key(&mut self, key: &'static TraverseKey, f: impl FnMut(&mut dyn TraversableNode) -> bool);
    pub fn traverse_ancestors<T: TraversableNode>(&mut self, f: impl FnMut(&mut T) -> bool);
    pub fn traverse_children_with_key(…); pub fn traverse_children<T: TraversableNode>(…);
    pub fn traverse_descendants_with_key(&mut self, key: &'static TraverseKey,
        f: impl FnMut(&mut dyn TraversableNode) -> TraverseDescendantsAction);
    pub fn traverse_descendants<T: TraversableNode>(&mut self, f: impl FnMut(&mut T) -> TraverseDescendantsAction);
}
```

Compose's `traverseKey: Any` compared with `==` becomes a `&'static TraverseKey` compared by address (the same pattern as `ModifierLocal` and `SemanticsPropertyKey` below); the typed variants match the node's type and key, as `areObjectsOfSameType(this, it)`. A traversal from inside a callback never visits the running entry (it is out of its chain); the tests that visit a caller's own delegator are ported as visits from the test.

### 3.9 Observer nodes, composition locals, modifier locals

**`ObserverModifierNode`**: `trait ObserverModifierNode { fn on_observed_reads_changed(&mut self, cx: &mut NodeContext<'_>); }`, and `NodeContext::observe_reads(block)` observes `block`'s state reads under a key naming the entry. The tree's `ReadObserver` keys are `(NodeId, kind)` (`tree.rs:32-46`); entry observers get their own `ReadObserver` keyed by `EntryId`, with a side map `EntryId → NodeId`. A write that changes what was read queues `on_observed_reads_changed` once (the key is forgotten when it fires, as Compose's one-shot); the call runs at the next drain point, outside passes, if the entry is still attached (`ObserverNodeOwnerScope.isObservationValid`).

As built (D1182, D1183): the handle is `CompositionLocalAnchor`, with an epoch so a provider's change that recomposes its content gives its nodes a new one; `MeasureScope` and `DrawScope` read through it too, and `observe_reads` in those takes the node's handle: **known difference**, `NodeHandle::observe_reads` where Compose's node calls `observeReads` itself, measuring and drawing having no `NodeContext` (the scopes' closures hold the tree for the content). `LOCAL_DENSITY`, `LOCAL_LAYOUT_DIRECTION` and `LOCAL_VIEW_CONFIGURATION` are static, and they are resolved only when a node's anchor changes, so no per-emission walk remains.

**`CompositionLocalConsumerModifierNode`**: `NodeContext::current_value_of(&LOCAL)` reads the layout node's captured locals. This needs a runtime handle the runtime ledger lists under `local-variants` (`CompositionLocalMap`, `currentCompositionLocalMap`): compose-rs's locals are provider groups in the slot table found by walking the enclosing providers (`crates/compose-runtime/src/local.rs:1-10`, `86-120`), so the handle is an anchor to the nearest provider group above the emitting group, and `get(local)` walks from it outside composition. A provided value is a state cell, so a read inside measure, draw, semantics or `observe_reads` is observed for free. `emit_layout_node` (`crates/compose-ui/src/layout.rs:324-342`) stores the anchor on the node; when it changes, the node's consumer entries are auto-invalidated as updated (`LayoutNode.kt:789-803`). A read before attach or after detach panics, as Compose's (`CompositionLocalConsumerModifierNode.kt:71-75`).

**Per-node density** (divergence §5.2): `LayoutNode` gains `density: Density`, read from `LOCAL_DENSITY` where it is composed, as `layout_direction` is from `LOCAL_LAYOUT_DIRECTION` today (`layout.rs:333-337`); `MeasureScope::density`, `DrawScope`'s density and `EventContext::density` read the node's. A change measures and draws the node again and queues `on_density_change` (§3.5); `UiTree::set_density` stays for the host's root value. The two locals are resolved together in one provider walk at emission (a runtime helper reading two locals in one walk), so a node emission costs one walk, as today.

**Modifier locals**:

```rust
/// As `ModifierLocal<T>`: declared as a `static`, compared by address.
pub struct ModifierLocal<T: 'static> { name: &'static str, default: fn() -> T }
pub fn modifier_local_of<T>(name: &'static str, default: fn() -> T) -> ModifierLocal<T>;   // const fn

pub trait ModifierLocalModifierNode: ModifierNode {
    fn provided_values(&mut self) -> &mut ModifierLocalMap;
}
/// As `ModifierLocalMap`/`modifierLocalMapOf`: a few keys, inline.
pub struct ModifierLocalMap { entries: SmallVec<[(LocalKey, Box<dyn Any>); 2]> }

impl NodeContext<'_> {
    /// As `ModifierLocal<T>.current`: the nearest provider before this node, else the default.
    pub fn modifier_local_current<T: Clone + 'static>(&mut self, local: &'static ModifierLocal<T>) -> T;
    /// As `provide(key, value)`: the key must be in `provided_values`.
    pub fn provide<T: 'static>(&mut self, local: &'static ModifierLocal<T>, value: T);
}
// The legacy element API, as Compose's (not deprecated):
impl Modifier {
    pub fn modifier_local_provider<T: 'static>(self, local: &'static ModifierLocal<T>, value: impl Fn() -> T + 'static) -> Modifier;
    pub fn modifier_local_consumer(self, consumer: impl Fn(&mut ModifierLocalReadScope) + 'static) -> Modifier;
}
```

`current` visits ancestors of kind `LOCALS` (§3.8) and reads the first map holding the key. Consumers (`modifier_local_consumer`) run when attached and whenever a provider of a key they read is inserted, updated or removed above them: the tree keeps, per frame, the providers that changed and their keys (Compose's `ModifierLocalManager.triggerUpdates`, `ModifierLocalManager.kt:68-118`), and at the drain point re-runs the consumers below each, found with `visit_subtree_if`, that read the key. A consumer records the keys it read in its last run.

### 3.10 Parent data

`trait ParentDataModifierNode { fn modify_parent_data(&self, density: Density, parent_data: Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>>; }`, folded tail to head, as Compose's, into `LayoutNode.custom_parent_data: Option<Rc<dyn Any>>`, recomputed in `refresh_chain` and by `invalidate_parent_data`; a change measures the parent. `MeasureScope::parent_data_any(measurable) -> Option<&dyn Any>` reads it (Compose's `Measurable.parentData`, also on `IntrinsicMeasurable`); `Modifier.layout_id(id)` and `Measurable::layout_id()` are built on it (`LayoutIdParentData`). compose-rs's fixed `ParentData { weight, alignment }` stays as the fast path that `Row`, `Column` and `Box` read from their built-in elements (`tree.rs:957-972`): its data never boxes. If foundation later ports Compose's `RowColumnParentData` as a parent-data node, it can move over; nothing here requires it.

### 3.11 Layout-aware nodes

`LayoutAwareModifierNode { fn on_placed(&mut self, coordinates: LayoutCoordinates, cx: &mut NodeContext<'_>) {} fn on_remeasured(&mut self, size: IntSize, cx: &mut NodeContext<'_>) {} }`, `MeasuredSizeAwareModifierNode` (`on_remeasured` only), `UnplacedAwareModifierNode { fn on_unplaced(&mut self, cx) }`, and `GlobalPositionAwareModifierNode` (exists; its `on_globally_positioned` gets a live `LayoutCoordinates` and a context instead of returning an `Action`).

- `on_remeasured` runs in `measure_layer` after the entry's layer is measured, without read observation (`NodeCoordinator.kt:387-393`), on every measurement of that layer; its context may invalidate but not measure.
- `on_placed`, `on_unplaced` and `on_globally_positioned` need the placement pass and live coordinates: placement doc §3.5. Until then they run after the layout with today's snapshot, as `on_globally_positioned` does (D172). As built (D1186, D1187): `on_placed` and `on_remeasured` so, `on_unplaced` with PL-2; a layer's change alone reports, compared by `LayerPositionalProperties`' values.
- The `Modifier.on_size_changed`, `on_globally_positioned`, `on_placed` and `on_remeasured` lambdas keep D172's rule: the node queues the application lambda, which runs after the layout outside the tree, so it may write state. Compose calls them synchronously; the difference is only when the lambda runs within the frame.

## 4. Cost on the hot paths

The headless benchmarks (benchmarks/benches/ui.rs, lazy.rs) and the allocation test are the gates; the numbers below are estimates from the code paths, to be confirmed by running them at each step (docs/14's figures: counter click ~250 ns, todo toggle ~195 µs (mostly hit tests), first frame of the feed 247 µs and of the 200-post screen 4.3 ms, an item scrolled into view 34.5 µs (feed) and 19.1 µs (stress list), a steady animation frame 1.46 µs).

| Path | Change | Estimate |
|---|---|---|
| Modifier diff, unchanged kinds (every recomposition of a node whose modifier changed) | `invalidate_measure` replaced by invalidation by the updated entries' kinds | Cheaper: a draw-only change (a colour, a `clickable`'s callback) no longer measures the node's layers and policy: one policy run (~100-300 ns) saved per such node per frame; an `animate*AsState` driving a background no longer measures each frame |
| Modifier diff, structural | Lifecycle queued; Myers instead of LCS (only if needed) | Rare in frames; within noise |
| Node creation and insertion | `ChainEntry` (40 bytes instead of 17 per entry); `EntryId` from a counter; marking attached; queued `on_attach` (a virtual no-op call per entry) | Per entry ~3-5 ns: about 30-55 µs on the 200-post first frame (3,615 layout nodes, docs/14, ~3 entries each), about 1 % of its 4.3 ms. If it shows above 1 %, the tree skips callbacks for entries whose type declares no lifecycle (a `LIFECYCLE` kind set by a `fn has_lifecycle` override): measured first, not built first |
| Reuse (lazy scroll) | `on_reset` + `on_detach` + `on_attach` per entry instead of `on_reset` | Three virtual calls per entry instead of one; nodes, caches and paragraphs kept (`on_release` doesn't run): within the item's 19-35 µs, ~0.1 µs |
| Measure | `measure_layer` finds the layout node through `layout_node_of` (one more branch for delegating entries); `on_remeasured` only for entries of that kind (a bit test on the node's kinds) | Within noise |
| Draw, hit test, semantics, focus walks | Kinds `u32` instead of `u8`; `for_each_of_kind` instead of `as_x()` | Within noise; walks that scanned by `downcast_ref` (overlay, graphics layer, requester) prune by bits: slightly cheaper |
| Steady frames | Draining the request queue (an empty `RefCell<Vec>` check) at three points | ~2 ns per frame |
| Memory | `LayoutNode` loses `capabilities` (`SmallVec<[u8; 16]>`) and gains `density`, `custom_parent_data`, the locals anchor | Roughly +60 bytes per node with three entries; ~0.2 MB on the 200-post screen (3,615 nodes) |

## 5. Migration plan

Each step lands with its tests (ported from Compose, citing them, in `crates/compose-ui/tests/suite` or foundation's suite), the ledger rows updated, and the benchmarks above run before and after.

| Step | Content | Compose tests it makes portable |
|---|---|---|
| MN-1 | `NodeKinds`, `ChainEntry`, `EntryId`; `capabilities` and the downcast scans replaced by kinds; no behaviour change | `MyersDiffTests` (as unit tests of `match_kinds`); the structural cases of `NodeChainTests` |
| MN-2 | Typed `ModifierNodeElement` + `AnyModifierNodeElement`, `InspectorInfo`; auto-invalidation by kinds and `should_auto_invalidate`; Myers diff if MN-1's ports disagree | `CustomLayoutAndMeasureTest.updatingModifierIsNotCausingParentsRelayout`; `GraphicsLayerModifierTest` (equality); the invalidation-count cases of `ModifierNodeReuseAndDeactivationTest`; `NodeChainTests` (node identities) |
| MN-3 | Lifecycle: `on_attach`, `on_detach`, `is_attached`, `on_release` split, the pass-end queue and its orders, reuse and deactivation as detach/attach (revising D431), `NodeContext`, `NodeHandle`, the registry and request queue, `coroutine_scope`, `side_effect`, `on_density_change`/`on_layout_direction_change` (tree-wide density) | `ModifierNodeAttachOrderTest`, `NodeChainTests` (attach/detach), `ModifierNodeReuseAndDeactivationTest` (lifecycle, adapted per §3.5), the tree-wide part of `ModifierNodeOnDensityChangeTest` |
| MN-4 | Node-initiated invalidation (§3.4), `remeasure_now`/`Remeasurement`/`RemeasurementModifier` (ledger 3.12), `on_measure_result_changed` | `InvalidateSubtreeTest`, `RemeasurementModifierTest`; the state-reading cases of `ModifierNodeReuseAndDeactivationTest` |
| MN-5 | `Delegates`, dispatch by kind, delegate paths in hit paths and focus | `DelegatingNodeTest`; the delegated cases of `OnSizeChangedTest`, `OnGloballyPositionedTest`; `CustomLayoutAndMeasureTest.instancesKeepDelegates`; with the focus doc, `CombinedFocusModifierNodeTest` |
| MN-6 | Internal visitors, `TraversableNode` and its public functions | `TraversableModifierNodeTest`, `ModifierNodeAncestorsTest`, `ModifierNodeChildTest`, `ModifierNodeNearestAncestorTest`, the seven `ModifierNodeVisit*Test` files (their subcompose cases wait for `SubcomposeLayout`) |
| MN-7 | `ObserverModifierNode`, `observe_reads` | `ObserverModifierNodeTest` |
| MN-8 | Captured composition locals (runtime `CompositionLocalMap` first), `current_value_of`, per-node density | `CompositionLocalConsumerModifierNodeTest`, `CompositionLocalMapInjectionTest`, `LayoutNodeDensityTest`, the rest of `ModifierNodeOnDensityChangeTest`; input's `PointerInputDensityTest` |
| MN-9 | Modifier locals, the provider/consumer elements and their update manager | `ModifierLocalSameLayoutNodeTest`, `ModifierLocalMultiLayoutNodeTest`, `ModifierLocalProviderConsumerOrderTest`, `ModifierLocalMapTest`, `ModifierLocalConsumerEntityTest` |
| MN-10 | `ParentDataModifierNode`, `layout_id` | `ParentDataModifierTest`, `LayoutIdTest` |
| MN-11 | `LayoutAwareModifierNode`, `MeasuredSizeAwareModifierNode` (done: `on_remeasured` in place, `on_placed` after the layout, D1186); `UnplacedAwareModifierNode` (with the placement doc's PL-2) | The `onRemeasured` cases of `OnSizeChangedTest`; with PL-2: `UnplacedAwareModifierNodeTest`, `PlacementReusableNodeTest`, the `onPlaced` cases of `PlacedChildTest` |

**Order and parallelism.** MN-1 → MN-2 → MN-3 are serial (they rewrite `tree.rs`'s chain, `set_modifier`, `refresh_chain` and the applier). After MN-3: MN-4 and MN-7 are small and independent; MN-5 and MN-6 touch every dispatch site and should be serial with each other (MN-5 first: traversal visits delegates); MN-9 needs MN-6; MN-10 needs only MN-2; MN-8 waits for the runtime's locals handle. Foundation's nodes (clickable, focusable, scroll, draggable) can move onto `Delegates` and `coroutine_scope` once MN-3 and MN-5 land; that is the foundation area's work.

## 6. Decisions to record

Numbers to be assigned from the workstream's block.

- **Lifecycle on reuse follows Compose** (detach and attach around a reset), revising D431; caches survive because releasing moves to a separate `on_release`. Alternative: keep D431 (reset only), which leaves `nodesAreDetachedWhenReused` failing by design and gives `on_attach`-started work no restart on reuse.
- **Lifecycle callbacks run at the end of the composition pass**, in Compose's orders; an element's `update` of a reused node runs before its reset (D434's constraint). Alternative: callbacks during the pass, which would let application code write state while composing.
- **Handles and a request queue** for invalidation from outside callbacks (the `FocusRequester` registry's pattern, D240, D148). Alternative: back-pointers from nodes to the tree (shared ownership of the tree, against compose-rs's arena design).
- **Typed `ModifierNodeElement`** with an erased object-safe form. Alternative: keep the untyped trait.
- **Auto-invalidation by kinds**, ending divergence §5.5.
- **Keys compared by address** (`TraverseKey`, `ModifierLocal`, and in the focus doc `SemanticsPropertyKey`): statics, never consts.

## 7. Open questions for the user

1. Revise D431 so reused nodes are detached and attached again (Compose's contract), keeping the caches through the `on_release` split? (Recommended: yes.)
2. Change `ModifierNodeElement` to the typed form, a breaking change for every custom element in the repository and in apps? (Recommended: yes, now, while the API has few users.)
3. Per-node density from `LOCAL_DENSITY`: it costs one combined provider walk per layout-node emission. Accept if `first frame, big screen` regresses by under 1 %?

## The orchestrator's answers to the open questions (2026-10-26)

Taken while the user was away, by D700's rule (1:1 parity with Compose; compose-rs-only fast paths may stay alongside):

1. **D431 revised:** reused nodes go through Compose's reset, detach, attach cycle; caches survive through `on_release`. Yes.
2. **Typed `ModifierNodeElement` now:** yes. compose-rs has no compatibility constraint; every custom element in the repository moves in the same change.
3. **Per-node density:** accepted if the "first frame, big screen" benchmark regresses by under 1 %; above that, stop and report.
4. **Immediate `place` inside `measure`:** stays public, documented as compose-rs's fast path beside Compose's `layout {}` shape.
5. **Decision blocks:** modifier nodes D1150–D1199 (MN steps), placement and lookahead D1200–D1249 (PL steps), focus and semantics D1250–D1299 (FO and SE steps).
6. **`CompositionLocalMap` and `current_composite_key_hash`:** the runtime workstream's milestone 1 delivers `CompositionLocalMap` (local-variants); `current_composite_key_hash` exists. MN-8 waits for the runtime's merge.
