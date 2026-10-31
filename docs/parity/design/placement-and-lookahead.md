# Design: placement, coordinates, intrinsics and lookahead

The spec for the ui-core ledger's layout track after the node infrastructure ([ui-core](../ui-core.md) §3.13 `placement-pass`, 3.14 `layout-coordinates`, 3.15 `on-placed`, 3.17 `intrinsics`, 3.20 `lookahead`, 3.21 `rulers`, and §4, the lookahead analysis), with what `SubcomposeLayout`, lazy layouts, `animateBounds`, shared elements and `animateItem` need from them. It builds on [modifier-nodes.md](modifier-nodes.md) (chain entries, kinds, `NodeContext`, handles, the request queue).

The governing constraint: **a tree that uses none of this pays nothing measurable.** compose-rs's measure policies measure and place in one call (docs/06, "Layout"), and the headless frames this protects are small (a steady animation frame 1.46 µs, an item scrolled into view 19-35 µs, the feed's first frame 247 µs, docs/14). Every addition below is either off the fused fast path or a branch on it.

Compose paths are relative to `frameworks/support/compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/` at androidx-main `d153d87ed76`.

## 1. What Compose does

### Measure, then place

A `MeasurePolicy` returns a `MeasureResult` whose `placeChildren()` holds the placement block (`layout/MeasureScope.kt:47-130`). The owner's `MeasureAndLayoutDelegate` measures dirty nodes, then lays them out (`node/MeasureAndLayoutDelegate.kt:579-628`): a node is placed by its parent's block calling `Placeable.place(x, y)` → `MeasurePassDelegate.placeAt` → `placeSelf` (`node/MeasurePassDelegate.kt:577-660`) → `placeOuterCoordinator` (`:662-704`), which runs the node's own placement block (`layoutChildren`, `:212-245`, under `observeLayoutSnapshotReads`) only when `layoutPending` or the node was not placed; otherwise it just moves the outer coordinator (`placeSelfApparentToRealOffset`) and the subtree stays as placed. So:

- placement runs top-down after measurement, and a node's block knows where its parent put it: `PlacementScope.coordinates` (`layout/Placeable.kt:189`) is live;
- a state read in a placement block is a **layout** read: its change re-runs the block (`requestRelayout`), not the measurement (`ModelReadsTest`);
- a block may measure a child (`MeasureInPlacementTest`);
- a parent moving doesn't re-run its descendants' blocks unless one read its coordinates while placing: `coordinatesAccessedDuringPlacement` (`placeSelf`, `:619-628`), set by any coordinates query during placement (`NodeCoordinator.onCoordinatesUsed`), and `requestLayoutIfCoordinatesAreUsedAndNotifyChildren` (`:875-890`), pruned by a count of children reading coordinates;
- placing a child twice in one block throws (`MeasuringPlacingTwiceIsNotAllowedTest`); children not placed by the block are marked unplaced with their subtree (`markSubtreeAsNotPlaced`, `:276`);
- `Placeable.measuredWidth/Height` are the size before coercion (`Placeable.kt:60-64`).

### Coordinates

`LayoutCoordinates` (`layout/LayoutCoordinates.kt:30-182`) is implemented by `NodeCoordinator`, one per layout modifier plus the inner one: `size`, `parentCoordinates`, `parentLayoutCoordinates`, `isAttached`, `localToRoot`, `localToWindow`, `localToScreen` and inverses, `localPositionOf(source, offset[, includeMotionFrameOfReference])` (`NodeCoordinator.kt:1094-1125`: up from the source to the common ancestor through each coordinator's layer transform and position, then down to this one through the inverses), `localBoundingBoxOf(source, clipBounds)`, `transformFrom`, `transformToScreen` (`:1127-1173`), `get(alignmentLine)`, `introducesMotionFrameOfReference`; and `positionInRoot`, `positionInWindow`, `positionOnScreen`, `boundsInRoot`, `boundsInWindow`, `positionInParent`, `boundsInParent`, `findRootCoordinates` (`LayoutCoordinates.kt:184-280`).

### Intrinsics

`MeasurePolicy` and `LayoutModifierNode` have four intrinsic functions (`layout/MeasurePolicy.kt:99-172`, `node/LayoutModifierNode.kt:69-121`) whose defaults run `measure` over `DefaultIntrinsicMeasurable`s (`layout/Layout.kt:301-351`): measuring one returns a fixed-size placeable whose size on the asked axis is the child's intrinsic, and placing it does nothing (`IntrinsicsMeasureScope`, `:353-385`). A parent that read a child's intrinsics is remeasured when the child's intrinsics may change (`intrinsicsUsageByParent`, `invalidateIntrinsicsParent`, `MeasurePassDelegate.kt:926-944`).

### Rulers

`MeasureScope.layout(w, h, alignmentLines, rulers: RulerScope.() -> Unit, placement)` (`MeasureScope.kt:69-90`) lets a layout provide `Ruler` values (`layout/Ruler.kt`: `VerticalRuler`, `HorizontalRuler`, `derived`, `minOf`, `maxOf`) to its subtree; a placement block reads `ruler.current(default)` (`Placeable.kt:199`), which walks up the coordinators to the nearest provider, evaluates its ruler lambda under observation, converts the value into the reader's coordinates, and registers the reader so a change re-places it (`node/LookaheadDelegate.kt:168-262`). `RectRulers` and `WindowInsetsRulers` are built on them.

### Lookahead

`LookaheadScope { content }` (`layout/LookaheadScope.kt:50-62`) emits a **virtual** layout node flagged `isVirtualLookaheadRoot`, whose scope coordinates are its parent's inner coordinator; every node below takes the closest such root as `lookaheadRoot` at attach (`node/LayoutNode.kt:534-546`), and a node carrying an `ApproachLayoutModifierNode` with no scope above becomes its own root. A node with a lookahead root has a `LookaheadPassDelegate` and its coordinators `LookaheadDelegate`s holding the lookahead size and position.

- **The outermost root** (`isOutMostLookaheadRoot`: a root whose parent has none, `node/LayoutNodeLayoutDelegate.kt:382-385`) runs the lookahead measurement of its subtree first when its parent measures it (`MeasurePassDelegate.measure`, `:469-483`), with the same constraints, then the approach (ordinary) measurement; nested scopes take part in the outer pass. In placement, the root's lookahead placement runs just before its approach placement, at the same position (`placeSelf`, `:631-656`).
- **Approach nodes** (`layout/ApproachLayoutModifierNode.kt:71-222`) run `measure` (a pass-through by default) in lookahead and `approachMeasure` in the approach with `lookaheadSize` and `lookaheadConstraints` (`ApproachMeasureScope.kt:28-120`). Before each approach measurement, `isMeasurementApproachInProgress(lookaheadSize) || constraints != lookaheadConstraints` decides whether the approach is needed; when it isn't, the wrapped content is measured with the lookahead constraints and the result snapped to the lookahead size (`node/LayoutModifierNodeCoordinator.kt:147-193`). `isPlacementApproachInProgress(lookaheadCoordinates)` is asked in placement.
- **Coordinates**: `toLookaheadCoordinates`, `lookaheadScopeCoordinates`, `localLookaheadPositionOf` (`LookaheadScope.kt:203-290`) measure lookahead positions summed from lookahead delegates, without layer transforms (`layout/LookaheadLayoutCoordinates.kt:82-140`).
- **Invalidation** is split: `requestLookaheadRemeasure`/`requestLookaheadRelayout` dirty both passes; `requestRemeasure`/`requestRelayout` only the approach (`LayoutNode.kt:1127-1190`, `MeasureAndLayoutDelegate.kt:143-310`); `invalidateMeasurements` picks lookahead for nodes in a scope.
- **Intrinsics** have lookahead and approach variants (`minApproachIntrinsicWidth`, …; `ApproachIntrinsicsMeasureScope`, `Layout.kt:387-420`).
- **Who uses it**: `Modifier.animateBounds` (`compose/animation/animation/src/commonMain/kotlin/androidx/compose/animation/AnimateBoundsModifier.kt:219-320`) is an approach node that measures at an animated size and, in its placement block, reads `coordinates` and `lookaheadScopeCoordinates` to place at an animated offset; shared elements add an overlay drawn from recorded graphics layers and movable content; `AnimatedVisibility`/`AnimatedContent`/`animateContentSize` read lookahead sizes; lazy lists compute lookahead positions for `animateItem` and keep items only the lookahead places.

## 2. Where compose-rs stands

`UiTree::layout` (`crates/compose-ui/src/measure.rs:15-43`) measures the root and places it; `measure_node` (`:157-202`) returns cached sizes by constraints and dirty flags, measures dirty descendants in place, and runs `measure_layer` (`:254-351`): one layout modifier or the policy, under observation (`Observation::Measure`). `MeasureScope::place` (`crates/compose-ui/src/layout.rs:235-251`) writes the child layer's `position` immediately (`UiTree::place`, `measure.rs:394-410`), counting the placement order and invalidating the parent's drawing when the layer moved; `place_relative` records what to mirror once the width is known (`measure.rs:372-384`). Each node keeps one `Layer` per layout modifier plus its content (`crates/compose-ui/src/tree.rs:125-140`), a node's root position is `layer_origin` summed up the ancestors (`tree.rs:1019-1031`), and `map_to_root` (`crates/compose-ui/src/layout_callbacks.rs:294-346`) maps a point to the root through layers and graphics-layer transforms. Layout callbacks run after the layout with a snapshot `LayoutCoordinates` (`layout_callbacks.rs:17-53`, D172). Intrinsics are `max_intrinsic_width/height` probes that measure the child (`layout.rs:181-219`, D137, D556). There are no placement blocks, no live coordinates, no rulers, no lookahead, and no `SubcomposeLayout` yet (D701 brings it).

## 3. Placement

### 3.1 Choice: deferred placement blocks beside immediate placement

The ledger offers two shapes: a general placement pass (L) or a narrower top-down fix-up for approach nodes only (M). This design does the general one in a form that **keeps the fused pass as the fast path**:

- `scope.place(&p, x, y)` inside `measure` stays: **immediate placement**, exactly today's code. It is equivalent to a placement block that reads nothing and asks no coordinates, so Compose semantics hold for it. Built-in policies (`Row`, `Column`, `Box`, lazy lists, Material's layouts) keep it.
- `scope.layout_with(w, h, block)` is Compose's `layout(w, h) { … }`: the block is stored and runs in a **placement pass** after the measure pass, top-down, once the node itself has been placed, with live coordinates. Code ported from Compose, and any block that reads state or coordinates while placing, uses it.

The fix-up alone would cover `animateBounds` but not `PlacementScope.coordinates` for ordinary layouts, placement-only invalidation, measuring in placement, rulers, or `onPlaced` order, which `PlacedChildTest`, `PlacementLayoutCoordinatesTest`, `ModelReadsTest`, `MeasureInPlacementTest`, `RulerTest` and the lookahead tests all need; the general pass costs only the nodes that use it.

### 3.2 API

```rust
impl MeasureScope<'_> {
    /// As Compose's `layout(width, height) { placement }`: `block` places the children once
    /// this node has been placed, in the placement pass.
    pub fn layout_with(&mut self, width: i32, height: i32,
        block: impl Fn(&mut PlacementScope<'_>) + 'static) -> MeasureResult;
    /// The same, providing rulers (§5): as `layout(w, h, rulers = { … }) { … }`.
    pub fn layout_with_rulers(&mut self, width: i32, height: i32,
        rulers: impl Fn(&mut RulerScope<'_>) + 'static,
        block: impl Fn(&mut PlacementScope<'_>) + 'static) -> MeasureResult;
}

/// As `Placeable.PlacementScope`.
pub struct PlacementScope<'a> { /* tree access, the node and layer placing, its width
                                   and direction, the pass (main or lookahead) */ }

impl PlacementScope<'_> {
    pub fn place(&mut self, placeable: &Placeable, x: i32, y: i32);
    pub fn place_with_z(&mut self, placeable: &Placeable, x: i32, y: i32, z_index: f32);
    pub fn place_relative(&mut self, placeable: &Placeable, x: i32, y: i32);   // mirrors at once: the width is known
    /// As `coordinates`: the placing layer's, live; `None` while placing for alignment lines.
    pub fn coordinates(&mut self) -> Option<LayoutCoordinates>;
    /// Measuring in placement, as Compose allows.
    pub fn measure(&mut self, measurable: Measurable, constraints: Constraints) -> Placeable;
    /// As `Ruler.current(defaultValue)` (§5).
    pub fn current(&mut self, ruler: &'static Ruler, default: f32) -> f32;
    /// As `withMotionFrameOfReferencePlacement`.
    pub fn with_motion_frame_of_reference_placement(&mut self, block: impl FnOnce(&mut PlacementScope<'_>));
    pub fn layout_direction(&self) -> LayoutDirection;
    pub fn density(&self) -> Density;
    // place_with_layer / place_relative_with_layer: ledger 3.16, a later step.
}

impl Placeable {
    /// As `measuredWidth`/`measuredHeight`: the size before coercion into the constraints.
    pub fn measured_width(&self) -> i32;
    pub fn measured_height(&self) -> i32;
}
```

`MeasureResult` stays `Copy`-sized for immediate layouts; the block goes to the node (§3.3), not into the result.

### 3.3 State

`LayoutNode` gains (`tree.rs:143-188`):

```rust
/// The deferred placement of this node's layers, for nodes that use `layout_with`
/// (most don't: `None`, one pointer).
pub(crate) placements: Option<Box<NodePlacements>>,
pub(crate) needs_placement: bool,           // a block of this node must run
pub(crate) subtree_needs_placement: bool,   // a descendant's must
/// The node's blocks asked coordinates while placing (Compose's
/// `coordinatesAccessedDuringPlacement`): a move of an ancestor places it again.
pub(crate) reads_coordinates: bool,
/// How many descendants read coordinates while placing (Compose's
/// `childrenAccessingCoordinatesDuringPlacement`): prunes the notification walk.
pub(crate) coordinate_readers_below: u32,

pub(crate) struct NodePlacements {
    /// Per layer: the block of its last measurement, if it deferred its placement.
    blocks: SmallVec<[Option<Box<dyn Fn(&mut PlacementScope<'_>)>>; 2]>,
    rulers: SmallVec<[Option<Box<dyn Fn(&mut RulerScope<'_>)>>; 1]>,
}
```

`Layer` (`tree.rs:128-140`) gains `placed_in: u64` (the placement run that placed it, to reject placing it twice, as `measured_in` rejects measuring twice).

### 3.4 The pass

`UiTree::layout` becomes measure, then place:

```text
layout(size):
    if nothing dirty and same size: return false                       (unchanged)
    measure_node(root, constraints)                                    (unchanged; immediate places happen inside)
    place(root, 0, origin)                                             (unchanged)
    if root.subtree_needs_placement or root.needs_placement:
        place_pass(root)                                               (new; one flag test otherwise)
    position_overlays(); …                                             (unchanged)

place_pass(node):
    if node.needs_placement:
        run_blocks(node)            // each layer's block, outermost first, placing the next layer or the children
        node.needs_placement = false
    if node.subtree_needs_placement:
        for child in node.children (placed, active):
            place_pass(child)
        node.subtree_needs_placement = false

run_blocks(node):
    mark the children not placed (their `placed` false), as Compose's markSubtreeAsNotPlaced
    for each layer L with a block, outermost first:
        run block(L) under observation (Observation::Placement, key: the node) with a new run id
        // inside: placement.place(child, x, y) → UiTree::place (measure.rs:394), which now also:
        //   - rejects a second placement in this run (placed_in), as Compose;
        //   - if the child moved and (child.reads_coordinates or child.coordinate_readers_below > 0):
        //       mark the readers in its subtree needs_placement (walk pruned by the counts),
        //       and their ancestors subtree_needs_placement;
        //   - if the child has blocks and is new or dirty: child.needs_placement (it is visited
        //     next by place_pass, as Compose's placeAt runs the child's block at once)
    unplaced children (placed false after the block) and their subtrees: on_unplaced (§3.5)
```

- **When blocks run.** A node measured again runs its blocks again (`measure_layer` sets `needs_placement` when the policy or modifier returned via `layout_with`, and the climb sets `subtree_needs_placement` up its ancestors). A node whose blocks read state that changed runs them again without measuring: `apply_observed_writes` (`tree.rs:681-707`) maps the new `Observation::Placement` kind to `invalidate_placement(node)` (`needs_placement`, ancestors' `subtree_needs_placement`, and the parent's children drawing invalidated, as a move does today). `LayoutModifierNode::invalidate_placement` from a node (modifier-nodes doc §3.4) does the same.
- **Order.** The children of a block are placed inside the block; their own blocks run when `place_pass` visits them, right after the block that placed them returns. Compose runs a child's block inside the parent's `place` call; the observable difference is only for a parent block that reads a child's *placement results* after placing it, which Compose doesn't define either (only its measured size, known since measurement).
- **Measuring in placement** runs `measure_node` from the block (it holds the tree); the measured child is then placed in the same block.
- **Immediate and deferred mix freely.** A deferred block's children may place their own children immediately (their measure already did), and an immediate parent may have deferred children: the placement pass reaches them through `subtree_needs_placement`, after the whole measure pass, when every ancestor position is final.
- **Right to left**: `place_relative` in a block mirrors at once (the node's width is known); the fused path keeps its deferred mirroring.
- **Intrinsic probes and premeasure** (`measure.rs:157-169`, `209-223`) don't schedule placement: a prefetched item's blocks run when a layout places it.
- **Overlays** are positioned after the placement pass, as now (`measure.rs:49-91`); their content's placement pass runs from the overlay node down like any subtree.
- **Observation keys**: the observation kind takes three bits instead of two (`tree.rs:32-46`), for `Placement` and lookahead's two kinds (§7.6); generations wrap at 2^29 in keys, harmless (a stale key only causes a spurious invalidation).
- **Placing twice** panics, in both the fused and the deferred path, ending divergence §5.1 (`MeasuringPlacingTwiceIsNotAllowedTest`, `DrawReorderingTest.placingTheSameItemTwiceIsNotAllowed…`). Each policy run already has a run id (`next_measure_run`, `measure.rs:386-389`); the place check compares it.

As built (D1209, D1210): no `Layer::placed_in`, whose 8 bytes per inline layer were measured: a parent placing twice is `placed`, cleared before each measurement and block run, and a modifier placing its content twice is one node-level `content_placed_in`. `NodePlacements::blocks` holds a `LayerBlock` per layer, with the measurement's run and whether it has run yet; a block run has its own run id. A child measured in a block is flagged (`measured_in_block`), so a new size places its parent. `blocks` follows `layers` when a chain loses a layout modifier. `place_with_z` waits for 3.16, `coordinates` and `current` for PL-2 and PL-4.

### 3.5 Placement callbacks

With the pass in place, the node callbacks of modifier-nodes §3.11 run in Compose's order: `on_placed(coordinates)` for an entry when its layer is placed, before what's inside it is placed (immediately in `UiTree::place` for a layer whose placement changed or whose node was measured; for a deferred layer, before its block runs); `on_unplaced` when a placed node stops being placed (a block skipped it, or it was deactivated); `on_globally_positioned` after the layout, as now, but with live coordinates and also after a layer-only change (a graphics layer property that moves the node: ending D172's known difference, `OnGloballyPositionedTest.onPositionedCalledWhenLayerChanged`): the tree marks positioned nodes below a layer whose properties changed (`Observation::Layer`'s drain, `tree.rs:697-704`) for notification.

## 4. Live `LayoutCoordinates`

### 4.1 The handle

```rust
/// As Compose's `LayoutCoordinates`: a handle on a layer of a node, live (its answers follow
/// the tree), `Copy`. A gone node answers `is_attached() == false` and zero sizes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LayoutCoordinates {
    tree: TreeKey,          // modifier-nodes §3.3's registry
    node: NodeId,
    layer: u16,             // 0: the node's outermost layer; layers.len()-1: its content
    flavour: Flavour,       // Main, Lookahead (§7.5)
}

impl LayoutCoordinates {
    pub fn size(&self) -> IntSize;
    pub fn is_attached(&self) -> bool;
    pub fn parent_coordinates(&self) -> Option<LayoutCoordinates>;        // the layer outside it
    pub fn parent_layout_coordinates(&self) -> Option<LayoutCoordinates>; // the parent node's content layer
    pub fn local_to_root(&self, local: Offset) -> Offset;
    pub fn local_to_window(&self, local: Offset) -> Offset;
    pub fn window_to_local(&self, window: Offset) -> Offset;
    pub fn local_to_screen(&self, local: Offset) -> Offset;   // the host's window origin on screen
    pub fn screen_to_local(&self, screen: Offset) -> Offset;
    pub fn local_position_of(&self, source: &LayoutCoordinates, relative_to_source: Offset) -> Offset;
    pub fn local_position_of_with(&self, source: &LayoutCoordinates, relative_to_source: Offset,
        include_motion_frame_of_reference: bool) -> Offset;
    pub fn local_bounding_box_of(&self, source: &LayoutCoordinates, clip_bounds: bool) -> Rect;
    pub fn transform_from(&self, source: &LayoutCoordinates, matrix: &mut Matrix);
    pub fn transform_to_screen(&self, matrix: &mut Matrix);
    pub fn introduces_motion_frame_of_reference(&self) -> bool;
    pub fn alignment_line(&self, line: AlignmentLine) -> Option<i32>;   // ledger 3.18
    // and Compose's extension functions as methods:
    pub fn position_in_root(&self) -> Offset;  pub fn position_in_window(&self) -> Offset;
    pub fn position_on_screen(&self) -> Offset; pub fn position_in_parent(&self) -> Offset;
    pub fn bounds_in_root(&self) -> Rect;      pub fn bounds_in_window(&self) -> Rect;
    pub fn bounds_in_parent(&self) -> Rect;    pub fn find_root_coordinates(&self) -> LayoutCoordinates;
}
```

It replaces today's snapshot struct of the same name (`layout_callbacks.rs:20-53`), whose five methods keep their names and meanings.

### 4.2 Reaching the tree from a handle

Compose calls these methods anywhere: inside placement blocks, in `onPlaced`, in callbacks after layout, in coroutines on stored coordinates. compose-rs's tree sits in the composition's applier cell, borrowed mutably for each pass (`host.rs:289-291`); a handle has no tree reference. The design:

- **Outside passes** (callbacks run by the host after layout, effects, tasks, tests): the handle borrows its tree through the thread's registry (modifier-nodes §3.3), immutably, for the duration of one query.
- **Inside passes** (a placement block, `on_placed`, a measure policy or a draw asking coordinates): the pass already holds `&mut UiTree`. The scopes (`PlacementScope`, `MeasureScope`, `NodeContext`) hold a raw pointer (`TreeAccess`) from which `enter()` gives a guard deref'ing to `&mut UiTree`, the window closed while it lives; sites calling application code publish the same pointer in the tree's access window (`open()`, whose guard borrows the tree). A query derives a `&UiTree` from it and runs no application code, so queries nest. No `&mut` and a query's `&` can overlap, which the types enforce; the window module is the only `unsafe`, and runs under Miri in both borrow models before merging (docs/09-testing.md, "Miri").
- A query with the window closed while the tree is borrowed (application code under something holding part of the tree: D1211's list) gets the tree busy: zeros and no panic, never undefined behaviour.

Reading a position inside a placement block makes the placing node a reader, as Compose's `onCoordinatesUsed` (D1212); nothing else is tracked on the query path.

### 4.3 The arithmetic

`local_position_of(source, offset)`: map `offset` from `source` up to the common ancestor layer, then from the ancestor down to `self` by the inverse transforms (`NodeCoordinator.kt:1104-1125`). Up is today's `map_to_root` (`layout_callbacks.rs:294-346`) stopped at the ancestor: each layer's offset, and each graphics layer's transform for the entries of that layer (`GraphicsLayerNode::properties(…).map`, `crates/compose-ui/src/graphics_layer.rs:168-260`). Down needs each layer's inverse transform: `LayerProperties::matrix()` and `Matrix::invert` from ledger 3.1 (`geometry-unit-api`, a dependency of this step). Overlays (popups) map through their `overlay_origin` to the root, as today. `transform_from` composes the same matrices. `local_bounding_box_of(…, clip_bounds)` maps the four corners and intersects with the clipping layers on the way (today's `coordinates`, `layout_callbacks.rs:241-271`). Window = root: compose-rs's root fills its window; `local_to_screen` adds the host's window origin, which hosts provide (Android's `getLocationOnScreen`, desktop's window position) through `UiHost::set_window_origin`.

Motion frame of reference (`introducesMotionFrameOfReference`, `withMotionFrameOfReferencePlacement`): a layer placed under `with_motion_frame_of_reference_placement` records the offset as a motion offset; `local_position_of_with(…, false)` subtracts motion offsets on the path. Lazy lists place items so in Compose (their scroll), which `animateBounds(animateMotionFrameOfReference = false)` relies on (`PlacementScopeMotionFrameOfReferenceTest`).

### 4.4 `require_layout_coordinates`

`NodeContext::require_layout_coordinates()` is the coordinates of the layer the entry lives in (the content layer for a non-layout entry after the last layout entry), panicking when detached, as Compose's (`node/DelegatableNode.kt:410-420`).

### 4.5 As built (D1211–D1213)

The handle names the entry ending its coordinator, not a layer index, so it follows a chain's changes and a modifier gone is detached; it has no `flavour` until §7.5. The window lives in the tree's requests, one per tree, not in a thread-local: an open costs a load and a store through the tree, which a measurement can pay. Guards restore what they found. A query reads a graphics layer's last run, which is D1214's cache: no block runs in it. §3.3's `reads_coordinates` and `coordinate_readers_below` weren't built: D1212's list of readers compares a hash of where each is after the placement pass, and nothing is on the main path but its `is_empty`. `on_placed` stays after the layout (D1186), its callbacks' reads counted there.

## 5. Rulers

Built with changes (D1218): a `Ruler` is an `Rc`d value, lazily provided values aren't cached, and there's no node bit.

```rust
pub struct Ruler { name: &'static str, kind: RulerKind /* Vertical | Horizontal */, derivation: Option<fn(&mut PlacementScope<'_>, f32) -> f32> }
// VerticalRuler, HorizontalRuler as Compose's, with `derived`, `min_of`, `max_of`; declared as statics.
pub struct RulerScope<'a> { /* the provider's coordinates, density */ }
impl RulerScope<'_> {
    pub fn coordinates(&mut self) -> LayoutCoordinates;
    pub fn provides(&mut self, ruler: &'static Ruler, value: f32);
    pub fn provides_relative(&mut self, ruler: &'static Ruler, value: f32);   // vertical: mirrored in RTL
}
```

- A provider stores its ruler block in `NodePlacements::rulers`. `PlacementScope::current(ruler, default)` walks up from the placing layer to the nearest layer whose node provides `ruler` (a `RULERS` bit on nodes prunes the walk), evaluates the provider's block lazily once per layout (cached in the provider's `NodePlacements` with the frame's layout serial), converts the value from the provider's coordinates to the reader's (`LookaheadDelegate.kt:168-200`), and records the reader in the provider's reader list (`NodeId`s, weak by generation).
- A provider measured again, re-placed or invalidated through `invalidate_rulers` (modifier-nodes §3.4) compares its new values and marks its readers `needs_placement` when one changed (`LookaheadDelegate.kt:257-262`, `429-460`); its block's state reads are observed under `Observation::Placement` keyed by the provider.
- `RectRulers` (`innermost_of`, `outermost_of`) are four rulers; `WindowInsetsRulers` are provided by the root from the host's insets (`PlatformWindowInsets`, read today by `MeasureScope::window_insets`, `layout.rs:130-134`), with their animations from the foundation insets work.
- Cost: nothing for trees that provide none; a `current` costs a walk to the provider (pruned by the bit) plus, the first time in a layout, the provider's block.

## 6. Intrinsics

```rust
pub trait MeasurePolicy: 'static {
    fn measure(&self, scope: &mut MeasureScope<'_>, measurables: &[Measurable], constraints: Constraints) -> MeasureResult;
    fn min_intrinsic_width(&self, scope: &mut IntrinsicMeasureScope<'_>, measurables: &[Measurable], height: i32) -> i32 { /* default */ }
    fn min_intrinsic_height(…, width: i32) -> i32;
    fn max_intrinsic_width(…, height: i32) -> i32;
    fn max_intrinsic_height(…, width: i32) -> i32;
}
// LayoutModifierNode: the same four, with `content: Measurable` instead of a slice.

impl IntrinsicMeasureScope<'_> {
    pub fn min_intrinsic_width(&mut self, measurable: Measurable, height: i32) -> i32;   // and the three others
    pub fn parent_data(&self, measurable: Measurable) -> ParentData;
    pub fn density(&self) -> Density; pub fn layout_direction(&self) -> LayoutDirection;
    pub fn is_looking_ahead(&self) -> bool;
}
// MeasureScope derefs to IntrinsicMeasureScope, so a policy asks its children's intrinsics while measuring.
```

- **The defaults** run `measure` in an **intrinsic mode** of `MeasureScope`: `MeasureScope` gains `intrinsic: Option<(MinOrMax, WidthOrHeight)>`; in that mode `measure(child, c)` returns a placeable whose asked axis is the child's intrinsic (min or max, given the constraint on the other axis) and whose other axis is the constraint's maximum (2^15 − 1 if it's unbounded), as `DefaultIntrinsicMeasurable`, writes no layer state, and `place`/`layout_with` do nothing (`IntrinsicsMeasureScope`). One branch in `measure` and `place` on the fast path.
- **A child's intrinsic** runs its chain: each layout entry's intrinsic function (default: the same mode around `LayoutModifierNode::measure`, `NodeMeasuringIntrinsics`, `LayoutModifierNode.kt:178-456`), then its policy's. Results are not cached (Compose doesn't); a read registers `intrinsics_asked` on the child (`tree.rs:168-170`, used by `remeasure_children_in_place`, `measure.rs:236-238`), which becomes Compose's usage flag for min and max, so a change inside the child measures the parent that read it (`RemeasureWithIntrinsicsTest`).
- **D137's probes go.** `max_intrinsic_width`/`max_intrinsic_height` keep their names and become real intrinsics; `intrinsic_probes` and its caching rule (`measure.rs:152-169`, D556) are deleted. Each layout in foundation and Material that relied on probing gets Compose's intrinsic formulas: `Row`/`Column`/`Box` (foundation's `RowColumnMeasurePolicy` intrinsics), text (`BasicText`'s min/max intrinsic widths from its paragraph: the text area's), and the menus and dividers that motivated D137. Until a policy writes its own, the default (measuring in intrinsic mode) is what Compose does too.
- **Too-large sizes**: `layout`/`layout_with` panic for sizes outside 0..2^24 as Compose's `checkMeasuredSize`, ending divergence §5.6 (`IntrinsicsMeasurementTest`'s too-large cases).
- Lookahead and approach variants: §7.7.

## 7. Lookahead

PL-5 was built with changes (D1221): a pass's fields are changed around over a scope's subtree, not read by pass everywhere, and `LookaheadCoordinates` is a type of its own.

### 7.1 The scope without virtual nodes

compose-rs has no virtual layout nodes, and adding them would touch every walk of the tree (measure, draw, hit test, semantics, focus) or the applier's index arithmetic. Instead:

```rust
/// As Compose's `LookaheadScope`: what `LookaheadScope(content)` gives its content.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LookaheadScope { id: ScopeId }

#[composable]
pub fn LookaheadScope(content: impl Fn(LookaheadScope) + 'static);
/// The scope the content of the nearest `LookaheadScope` reads.
pub(crate) static LOCAL_LOOKAHEAD_SCOPE: CompositionLocal<Option<LookaheadScope>>;
```

`LookaheadScope(content)` remembers a `ScopeId` and provides it through `LOCAL_LOOKAHEAD_SCOPE`; `emit_layout_node` reads it with the node's other locals (modifier-nodes §3.9, the same provider walk) into `LayoutNode.lookahead_scope: Option<ScopeId>`. At attach (and when the value changes), `lookahead_root` is derived as Compose's: the node's scope if it has one; else, if its chain has an `APPROACH_MEASURE` entry, the node itself (`LayoutNode.kt:540-543`). A node is an **outermost root** when its lookahead root is set and its parent's isn't (`isOutMostLookaheadRoot`); the scope's **coordinates** are its outermost roots' parent's content layer (Compose's `parent.innerCoordinator`), recorded in the tree's scope table (`ScopeId → NodeId`) when the first outermost root of the scope attaches. Nested scopes resolve to the closest provider, as Compose's closest root; movable content (runtime) moved into a scope recomposes its emission's locals there, as Compose's.

### 7.2 Lookahead state, allocated only inside scopes

```rust
/// A node's lookahead measurement and placement (Compose's LookaheadPassDelegate and its
/// coordinators' LookaheadDelegates). Allocated when the node gets a lookahead root.
pub(crate) struct LookaheadState {
    layers: SmallVec<[Layer; 2]>,
    constraints: Option<Constraints>,
    needs_measure: bool, subtree_needs_measure: bool,
    needs_placement: bool, subtree_needs_placement: bool,
    placed: bool, place_order: u64,
    placements: Option<Box<NodePlacements>>,
    /// A policy or modifier asked `is_looking_ahead()` in this pass: its approach result may
    /// differ from its lookahead one (§7.10).
    asked_looking_ahead: bool,
}
// LayoutNode: lookahead_scope: Option<ScopeId>, lookahead_root: Option<NodeId>, lookahead: Option<Box<LookaheadState>>
```

`MeasureScope` and `PlacementScope` carry the pass (`Pass::Main`, `Pass::Lookahead`); `measure_node`, `measure_layer`, `place` and the placement pass take it and read and write `layers` or `lookahead.layers`, `constraints` or `lookahead.constraints`, and the matching flags. `MeasureScope::is_looking_ahead()` (Compose's `IntrinsicMeasureScope.isLookingAhead`) answers the pass. Policies and layout modifiers run unchanged in both passes.

### 7.3 The passes in a frame

```text
measure_node(node, c, Main):
    if node is an outermost lookahead root:
        measure_node(node, c, Lookahead)        // cached by lookahead constraints and flags
        // inside, children are measured in Lookahead, and placed immediately or by
        // deferred lookahead blocks (run by the lookahead placement, below)
    … today's Main measurement (the approach)

place(node, 0, position, Main):
    if node is an outermost lookahead root:
        set its lookahead position to `position` (Compose places the root's lookahead at the
        approach position, placeSelf) and mark its lookahead placement pass to run
    … today's placement

place_pass(node):                               (§3.4)
    if node is an outermost lookahead root and its lookahead placement is dirty:
        place_pass_lookahead(node)              // its deferred lookahead blocks, top-down
    … the Main blocks
```

The lookahead pass for a scope runs only when something in it is lookahead-dirty, so a frame where only an approach animation moves runs only the approach. Nested scopes are measured inside their outer scope's lookahead pass and have no lookahead pass of their own (they aren't outermost).

### 7.4 Approach nodes

```rust
pub trait ApproachLayoutModifierNode: LayoutModifierNode {
    fn is_measurement_approach_in_progress(&mut self, lookahead_size: IntSize) -> bool;
    fn is_placement_approach_in_progress(&mut self, scope: &mut PlacementScope<'_>,
        lookahead_coordinates: LayoutCoordinates) -> bool { false }
    fn approach_measure(&mut self, scope: &mut ApproachMeasureScope<'_>, content: Measurable,
        constraints: Constraints) -> MeasureResult;
    // the four approach intrinsics, with defaults as Compose's
}
// LayoutModifierNode::measure defaults to a pass-through for approach nodes, as Compose's.

/// As `ApproachMeasureScope`: a MeasureScope with the lookahead results and the scope.
pub struct ApproachMeasureScope<'a> { /* derefs to MeasureScope */ }
impl ApproachMeasureScope<'_> {
    pub fn lookahead_size(&self) -> IntSize;
    pub fn lookahead_constraints(&self) -> Constraints;
}
impl Modifier {
    /// As `Modifier.approachLayout`.
    pub fn approach_layout(self,
        is_measurement_approach_in_progress: impl Fn(IntSize) -> bool + 'static,
        is_placement_approach_in_progress: impl Fn(&mut PlacementScope<'_>, LayoutCoordinates) -> bool + 'static,
        approach_measure: impl Fn(&mut ApproachMeasureScope<'_>, Measurable, Constraints) -> MeasureResult + 'static) -> Modifier;
}
```

In `measure_layer` for a layer whose entry has `APPROACH_MEASURE`: in `Pass::Lookahead`, run `measure`; in `Pass::Main`, follow `LayoutModifierNodeCoordinator.measure` (`:147-193`): `required = is_measurement_approach_in_progress(lookahead_size) || constraints != lookahead_constraints`; if not required, the content layer is measured with the lookahead constraints (a `force_lookahead_constraints` flag on the scope for the next layer); run `approach_measure`; if not required and the content's size equals its lookahead size but the result doesn't reach the lookahead size, report the lookahead size. `is_placement_approach_in_progress` runs in the approach placement of that layer, before its block, with the layer's lookahead coordinates. Approach nodes place through `layout_with` (they read `coordinates`), so their blocks are what the placement pass runs; nothing else in a scope needs deferred placement.

### 7.5 Lookahead coordinates

`Flavour::Lookahead` handles answer from lookahead layers: positions summed without graphics-layer transforms (`LookaheadLayoutCoordinates.kt:92-140`), the common ancestor's lookahead layers when it is in the scope, else each side's offset to its outermost root plus the root's position. `LookaheadScope` gets Compose's functions as methods:

```rust
impl LookaheadScope {
    pub fn to_lookahead_coordinates(&self, coordinates: LayoutCoordinates) -> LayoutCoordinates;
    pub fn lookahead_scope_coordinates(&self, placement: &mut PlacementScope<'_>) -> LayoutCoordinates;
    pub fn local_lookahead_position_of(&self, coordinates: LayoutCoordinates,
        source: LayoutCoordinates, relative_to_source: Offset, include_motion_frame_of_reference: bool) -> Offset;
}
```

### 7.6 Invalidation

- Composition changes, attach and detach (modifier-nodes' `invalidate_measurement`) inside a scope dirty both passes (`lookahead.needs_measure`, `needs_measure`, and both subtree flags up to the outermost root, then the main flags above it), as Compose's `invalidateMeasurements` (`LayoutNode.kt:1176-1188`).
- State reads are observed per pass: `Observation::LookaheadMeasure` and `Observation::LookaheadPlacement` keys for reads made in `Pass::Lookahead`, invalidating both passes; reads in the approach (`Observation::Measure`, `Placement`) invalidate the approach only. An animation read in `approach_measure` therefore re-runs the approach alone (`LookaheadScopeTest.lookaheadStaysTheSameDuringAnimationTest`).
- `invalidate_placement` in a scope re-places the approach; the lookahead placement only for reads made in lookahead blocks.

### 7.7 Intrinsics in lookahead

`IntrinsicMeasureScope` carries the pass; asked from an approach node's approach measurement, a child's intrinsics run the approach intrinsics of approach entries (`min_approach_intrinsic_width`, …, defaults as Compose's `ApproachIntrinsicsMeasureScope`) and the ordinary ones otherwise; asked in the lookahead pass, the ordinary ones. Compose's rule that intrinsics asked from above the lookahead root during approach use the lookahead's (`MeasurePassDelegate.kt:729-780`) is kept: a node that is an outermost root answers intrinsics from its lookahead pass when asked from outside the scope.

### 7.8 `SubcomposeLayout` and lazy layouts

- **`SubcomposeLayout`** (D701, separate work) subcomposes in whichever pass measures it first; in a scope that is the lookahead pass, and the approach pass reuses the slots it composed. The slot table's `isInLookaheadScope` handling and "slots only the lookahead used" are that feature's; this design gives it `is_looking_ahead()` and the per-pass state. `LookaheadScopeTest`'s subcompose cases and `SubcomposeLayoutTest`'s lookahead cases come with it.
- **compose-rs's lazy lists** compose between measure rounds (D487, D492). In a scope their measurement runs twice (lookahead, approach), and each writes the range it wants: the frame composes the **union**, so an item the lookahead places (where it will be) and the approach doesn't (where it is now), or the reverse, is composed. The lazy list writes its items' lookahead positions in `layout_info` for `animate_item`, and places items under `with_motion_frame_of_reference_placement` (§4.3). No `SubcomposeLayout` is needed for compose-rs's own lazy layouts; whether they move onto it is D701's open question.

### 7.9 What the consumers need

| Consumer | Needs |
|---|---|
| `Modifier.animate_bounds` (animation area) | `ApproachLayoutModifierNode`, `approach_measure` with a placement block reading `coordinates()` and `lookahead_scope_coordinates`, `local_position_of_with(…, include_motion_frame_of_reference)`, a node `coroutine_scope` for its animation (modifier-nodes §3.3), `current_value_of` for density and fonts |
| Shared elements (`SharedTransitionLayout`, `shared_element`, `shared_bounds`, `skip_to_lookahead_size`) | All of the above, plus runtime `movableContentOf`, graphics-layer recording for the overlay (ledger 3.34), and `place_with_layer` (3.16) |
| `AnimatedVisibility`, `AnimatedContent`, `animate_content_size` in a scope | `is_looking_ahead()` and lookahead sizes in their policies |
| `animate_item` in a scope | Lazy lists' lookahead positions (§7.8), `place_with_layer` |
| Material 3's `DraggableAnchors` | `is_looking_ahead()`, lookahead sizes |

### 7.10 An optional optimization

In a scope, every node is measured twice where Compose measures it twice too. compose-rs can do better without changing results: a node whose subtree has no approach entry and whose policies never asked `is_looking_ahead()` (`LookaheadState::asked_looking_ahead`, aggregated up the subtree), measured in the approach with its lookahead constraints, gets its lookahead layers copied instead of running its policy. Tests that count measurements in a scope (`LookaheadScopeTest`'s counting cases) count Compose's two, so this goes behind an optimization flag (`LOOKAHEAD_REUSES_RESULTS`, off at parity, D520's mechanism), measured before it joins `recommended`.

## 8. Cost

Estimated from the code paths, to be confirmed with `cargo bench -p compose-benchmarks --bench ui` and `--bench lazy` before and after each step (parity mode, as the benches run).

| Step | Per-node cost when unused | Where it shows |
|---|---|---|
| Placement pass | One flag test per layout (`root.subtree_needs_placement`); in `UiTree::place` a `placed` test and, for a layer inside a node, a `content_placed_in` compare and store; `LayoutNode` +16 bytes (`placements` 8, `content_placed_in` 8, flags in padding) | `first_frame_feed` +0.14 %, `first_frame_big` +0.28 %, `grid_pixel` +0.52 % (+0.18 %, +0.26 %, +0.78 % with fat LTO, inside their spreads): D1209. A layout with deferred blocks pays one boxed closure per measurement (~25-40 ns) and the block's run: built-ins stay immediate so no bench pays it |
| Coordinates | Per child measured or placed, and per `layout_with`, `enter`'s two loads and two stores; per measured layer the same to open the window; per user-defined draw node, its move out of the chain and back under a guard; per place, a byte's test; per user-defined chain entry made or updated, a call of `hit_clip`; per layout, an `is_empty`. A query runs no block. `LayoutNode` +5 bytes, in padding | Against the placement pass, 5 rounds: `first_frame_feed` +0.80 % (spread 1.33 %), `first_frame_big` +0.94 %, `grid_pixel` +1.09 % (spread 3.92 %), `list_scroll` +0.63 %, `feed_pixel` +0.62 % (spread 2.20 %), `stress_pixel` +0.59 %, `counter_click` +1.10 %, `feed_animated` +0.38 % (spread 1.98 %), `cells_animated` +0.59 %, `cells_animated_positioned` **-20.07 %**, a query running no blocks. With fat LTO and one codegen unit, as Android's build, 3 rounds: `first_frame_feed` +0.64 % (spread 1.27 %), `first_frame_big` +0.81 %, `grid_pixel` +0.54 %, `feed_pixel` +0.02 %, `counter_click` +1.20 %, `feed_animated` +0.37 %, `cells_animated_positioned` -20.01 %: D1215. No single owner; accepted |
| Live coordinates | Nothing on hot paths (queries only) | — |
| Rulers | A bit test in the provider walk only when read | — |
| Intrinsics | One branch in `MeasureScope::measure` and `place`; deleting the probe path removes a branch from `measure_node` | Within noise; screens with intrinsic layouts (menus) get cheaper (no probing measurement) or dearer (formulas walk children twice), per layout: measured by foundation's own intrinsic step |
| Lookahead | A pass check in `measure_node`/`place` (`lookahead_root.is_some()` on the node, one branch); `LayoutNode` +24 bytes (`lookahead_scope`, `lookahead_root`, `lookahead`); one more local read per node emission, folded into modifier-nodes §3.9's walk | Within noise when no scope exists. Inside a scope: about 2× the measurement of the scope's subtree on lookahead-dirty frames, 1× on approach-only frames (an animation), as Compose; §7.10 can bring the dirty frames down |

The allocation test (steady recomposition allocates nothing, docs/14) stays green: nothing here allocates on unchanged frames; `layout_with` blocks allocate only when their layout is measured.

## 9. Migration plan

Each step ships with its tests, ledger rows, and the benchmark comparison.

| Step | Content | Compose tests it makes portable |
|---|---|---|
| PL-1 | **Done** (D1209, D1210). `layout_with`, `PlacementScope` (place, place_relative, measure in placement), `NodePlacements`, the placement pass, `Observation::Placement` and `invalidate_placement`, placing twice rejected, `Placeable::measured_width/height` | `MeasureInPlacementTest` (main pass), `MeasuringPlacingTwiceIsNotAllowedTest`, the relayout cases of `MeasureAndLayoutDelegateTest`, `ModelReadsTest` (placement reads), `CustomLayoutAndMeasureTest.placeableMeasuredSize`, `LayoutCooperationTest`, the relayout-only cases of `InvalidatingNotPlacedChildTest`, `DrawReorderingTest.placingTheSameItemTwice…` |
| PL-2 | **Done** but for `on_unplaced` (next), `alignment_line` (3.18), hosts' window origins (D1211–D1213). Live `LayoutCoordinates` (handle, access window, arithmetic, Matrix inverse from ledger 3.1), `PlacementScope::coordinates`, coordinate readers, `require_layout_coordinates`, motion frame of reference; `on_placed`/`on_unplaced`/live `on_globally_positioned` (modifier-nodes MN-11), layer-only changes reported | `PlacementLayoutCoordinatesTest` (main pass), `RequireLayoutCoordinatesTest`, `OnGloballyPositionedTest` (parent-relative queries, `onPositionedCalledWhenLayerChanged`), the coordinate cases of `LayoutNodeTest`, `UnplacedAwareModifierNodeTest`, `PlacementReusableNodeTest`, most of `PlacedChildTest`; with foundation: `PlacementScopeMotionFrameOfReferenceTest` |
| PL-3 | **Done** (D1200–D1205). Intrinsics (policy and modifier functions, intrinsic mode, usage tracking, probes deleted, size checks); foundation's and text's formulas in their areas | `IntrinsicsMeasurementTest`, `RemeasureWithIntrinsicsTest`, `RemeasureWithIntrinsicsRealClockTest`, `RtlLayoutTest` (intrinsics scope), `MultiContentLayoutTest.customIntrinsics` |
| PL-4 | Rulers, `RectRulers`, `invalidate_rulers`; `WindowInsetsRulers` with the host's insets | `RulerTest` (main pass), `RectRulerTest`, `WindowInsetsRulersTest` |
| PL-5 | **Done** in code (D1221), most of its tests still to port, with PL-6 now there. Lookahead core: `LookaheadScope` and its local, `LookaheadState`, the two passes with pass-through approach, lookahead coordinates, split invalidation | The first part of `LookaheadScopeTest` (`lookaheadLayoutAnimation`, `nestedLookaheadScopeTest`, `noExtraLookaheadTest`, `localLookaheadPositionOf*`, `lookaheadStaysTheSameDuringAnimationTest`, `skipPlacementOnlyPostLookahead`, `testLookaheadPlacementInvalidatedAfterRoot`), `LookaheadDelegatesTest` |
| PL-6 | **Done** with closures, the trait and more of its tests to come (D1222). `ApproachLayoutModifierNode`, `approach_layout`, completion and snapping, approach placement | `ApproachLayoutTest`, `TogglePlacementInLookaheadScopeTest`, the approach cases of `LookaheadScopeTest`; the lookahead cases of `PlacementLayoutCoordinatesTest`, `MeasureInPlacementTest`, `PlacedChildTest` |
| PL-7 | In code but for an approach layout's own intrinsics (D1223); 3 of the eight cases ported, none of width. Lookahead and approach intrinsics | The eight intrinsics cases of `LookaheadScopeTest` and `IntrinsicsMeasurementTest`'s lookahead case |
| PL-8 | Lazy lists in a scope (union of wanted ranges, lookahead positions), with the foundation area | Foundation's `animateItem`-in-lookahead tests; unblocks animation's `animate_bounds` (`AnimateBoundsTest`) |
| PL-9 | (`firstBaselineAlignmentInLookaheadScope` and `grandparentQueryBaseline` are ported, in foundation's `alignment_line.rs`.) The baseline and `SubcomposeLayout` cases, after ledger 3.18 and D701's `SubcomposeLayout` | the subcompose cases of `LookaheadScopeTest` and `SubcomposeLayoutTest`, `RulerTest`'s lookahead cases |

**Dependencies on modifier-nodes.md**: PL-1 needs MN-1 (kinds) and MN-4's invalidation entry points; PL-2 needs MN-3 (contexts, registry) and is where MN-11's placement callbacks land; PL-5 needs MN-8's locals walk (or reads `LOCAL_LOOKAHEAD_SCOPE` directly at emission, as `LOCAL_LAYOUT_DIRECTION` is read today, if MN-8 lags). PL-3 (intrinsics) needs nothing from them and can start at once.

**Parallelism in `crates/compose-ui`.** PL-1 and PL-3 touch the same files (`layout.rs`'s `MeasureScope`, `measure.rs`) but different functions; they can run in parallel with a planned merge of `MeasureScope`'s fields. PL-2 follows PL-1. PL-4 follows PL-2. PL-5 to PL-7 are serial and touch `measure.rs` throughout; nothing else in the layout track should run beside them.

## 10. Decisions to record

Numbers to be assigned from the workstream's block.

- **Deferred placement blocks beside immediate placement**, the fused pass staying the fast path; built-in policies keep placing immediately. Alternatives: converting every policy to placement blocks (a boxed closure per measured layout, ~25-40 ns each: about 0.1 ms, 2-3 %, on the 200-post screen's first frame with its 3,615 layout nodes, docs/14), or the approach-only fix-up (§3.1).
- **Coordinates as `Copy` handles reaching the tree through a registry and a per-tree access window** (as built; the design said a thread-local), the window's raw pointer the only `unsafe`, under Miri before merging compose-ui. Alternatives: queries that take the tree as an argument (diverges from Compose's method shapes and can't serve stored coordinates), or a separate geometry arena of cells (an indirection on every layer access in measure and draw).
- **Lookahead scopes through a composition local** instead of virtual nodes. Alternative: virtual layout nodes with folded children (every walk or the applier's indices change).
- **D137's intrinsic probes are replaced by Compose's intrinsics**, revising D137 and D556.
- **Placing a child twice panics**, as Compose, ending divergence §5.1.

## 11. Open questions for the user

1. Should immediate placement in `measure` stay public API, documented as compose-rs's fast path, or become crate-internal so applications write Compose's `layout(w, h) { … }` shape only? (Recommended: public; it is semantically a block that reads nothing.)
2. The access window is `unsafe` code in a crate that has little. Accept it (with Miri before merging), or prefer coordinate queries that take a scope argument inside passes (a divergence from Compose's API)?
3. Lookahead reuse of results (§7.10) as a compose-rs optimization flag: worth building after PL-6, or leave the scope's double measurement as Compose's?

## The orchestrator's answers to the open questions (2026-10-26)

Taken while the user was away, by D700's rule:

1. **Immediate `place` inside `measure`:** stays public (compose-rs's fast path); `layout_with`/Compose's `layout {}` shape beside it.
2. **The `unsafe` access window:** accepted, on two conditions: the window's code is small and in one module, and its tests run under Miri (`cargo +nightly miri test` on that module's tests, if nightly and Miri are installed; otherwise say so in the hand-back and the user decides).
3. **The lookahead result-reuse flag:** decided after PL-6, from measurements, as a compose-rs-only optimization flag off at parity.
4. **Decision block:** D1200–D1249.
