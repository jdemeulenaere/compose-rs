# Design: composition contexts, subcomposition, movable content, the Recomposer, `retain`

The spec for the composition features of the [runtime ledger](../runtime.md): `composition-context` (D701), `composition-api`, `recomposer` with `frame-clocks` and `frame-end-callback`, `movable-content`, `retain`; for the ui half of D701, `SubcomposeLayout` (the [ui-core ledger](../ui-core.md)'s 3.19 `subcompose-layout`) and `BoxWithConstraints` on it; and the question D701 left open: whether lazy layouts move onto subcomposition (the [foundation ledger](../foundation-layout-lazy.md)'s `lazy-layout-public`). Snapshots, apply notifications and composition in a mutable snapshot are in [snapshots.md](snapshots.md).

Compose paths are relative to `frameworks/support/compose/runtime/runtime/src/commonMain/kotlin/androidx/compose/runtime/` at androidx-main `d153d87ed76` unless they start with another module (`compose/ui/…`, `compose/foundation/…`, `runtime-retain/…`); compose-rs paths are relative to the repository root.

## 1. What Compose does

### 1.1 `CompositionContext`

A `CompositionContext` (`CompositionContext.kt:39-122`) is what a composition composes under: the `Recomposer` at the root (`Recomposer.kt:193`), or a point inside another composition. `rememberCompositionContext()` (`Composables.kt:517-519`) calls `Composer.buildContext()` (`GapComposer.kt:1242-1270`), which starts a reference group, remembers a `CompositionContextHolder` (a `RememberObserver` whose `onForgotten` disposes the context) holding a `CompositionContextImpl` (`GapComposer.kt:2941-3100`), and updates it on every composition with the composer's current `CompositionLocal` scope and composite key hash.

`CompositionContextImpl` delegates almost everything to its parent context (`composeInitial`, `registerComposition`, `effectCoroutineContext`, movable content, `scheduleFrameEndCallback`), and adds:

- **Locals:** `getCompositionLocalScope()` (`GapComposer.kt:3043`) returns the parent's locals at the context's position, kept in a snapshot state (`compositionLocalScope`, `:3027-3041`) so changes made during composition are visible to the rest of it. A child composition's root reads its locals from there.
- **Invalidation order:** `invalidate(child)` (`:3015-3027`) invalidates the parent composition first, then the child, "so parents always recompose before their children".
- **Composing children:** `startComposing`/`doneComposing` (`:3060-3066`) count children composing (`childrenComposing`, `GapComposer.kt:247-263`); while it is non-zero the parent records no reads (`areChildrenComposing`, `Composition.kt:717-718`, `:1177`).
- The composite key hash at the context, which child compositions start from.

A composition is made with a parent: `Composition(applier, parent)` (`Composition.kt:347`), `ReusableComposition(applier, parent)` (`:364`), `ControlledComposition(applier, parent)` (`:388`), `PausableComposition(applier, parent)` (`PausableComposition.kt:190`). Its first `setContent` calls `parent.composeInitial(this, content)` (`Composition.kt:777-780`), which reaches the `Recomposer.composeInitial` (`Recomposer.kt:1195-1260`): composed in a mutable snapshot **at once**, changes applied and effects dispatched before it returns, even when called while another composition is composing (`composerWasComposing`). Later invalidations go through the parent chain to the `Recomposer`, which recomposes the child in its frame like any composition. Disposing a parent doesn't dispose its children: their owners do (`SubcomposeLayout`'s node release, a test's `DisposableEffect`).

Compose's own tests subcompose this way during the parent's composition (`MovableContentTests.kt:2250-2268`: `Subcompose` calls `Composition(ViewApplier(host), rememberCompositionContext()).setContent(content)` in the body, emitting into a node of the parent's tree; `DeferredSubcompose` from a `LaunchedEffect`), or from an effect (`CompositionTests.kt:5296-5313`, `TestSubcomposition`).

### 1.2 `SubcomposeLayout`

`compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/layout/SubcomposeLayout.kt`. `SubcomposeLayout(state, modifier, measurePolicy)` (`:125-150`) emits a `ReusableComposeNode<LayoutNode>` whose update sets the state's root, a `rememberCompositionContext()`, the measure policy and the composition locals, and, when the composable doesn't skip, `SideEffect { state.forceRecomposeChildren() }`. Its state (`LayoutNodeSubcompositionsState`, `:520-1615`) keeps, per slot id, a **virtual** `LayoutNode` (`createNodeAt`, `:1277-1278`, `LayoutNode(isVirtual = true)`: its children fold into the parent's) and a `NodeState` (`:1319`: the slot's `ReusableComposition`, content lambda, `active` state, `forceRecompose`, paused composition).

`SubcomposeMeasureScope.subcompose(slotId, content)` (`:588-638`), callable only while measuring or laying out: takes the slot's node (or a precomposed one, or one from the reuse pool, `takeNodeFromReusables`, else a new one), moves it to the current index, and composes when the content lambda changed, the composition has invalidations or `forceRecompose` is set (`:640-669`). Composing (`:671-732`) runs inside `Snapshot.withoutReadObservation` and `ignoreRemeasureRequests`: the slot's composition is created on first use (`createSubcomposition(node, parent)` = `ReusableComposition(UiApplier(node), parent)`, `compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/platform/Subcomposition.kt:27-35`) and given `ReusableContentHost(active) { content }` with `setContent` or `setContentWithReuse`. It returns the slot node's children as measurables. After placement, `disposeOrReuseStartingFromIndex(currentIndex)` (`:739-790`) deactivates the slots this measurement didn't use into the reuse pool (by the `SubcomposeSlotReusePolicy`) or disposes them. `precompose(slotId, content)` (`:1019-1047`) and `createPausedPrecomposition` compose a slot ahead of measurement (`PrecomposedSlotHandle`: `dispose`, `premeasure`, `getSize`, `placeablesCount`, `traverseDescendants`). `SubcomposeLayout` has no intrinsics (`NoIntrinsicsMeasurePolicy`).

Subcompositions recompose for their own state changes in the `Recomposer`'s frame, before layout, like any composition; measurement composes only new slots and slots whose content changed. `BoxWithConstraints` is a `SubcomposeLayout` with one slot (`compose/foundation/foundation-layout/src/commonMain/kotlin/androidx/compose/foundation/layout/BoxWithConstraints.kt:64-67`). `LazyLayout` is one with a slot per item (`compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/LazyLayout.kt:107-140`), reusing items by content type (`LazyLayoutItemReusePolicy`) and prefetching by precomposition (`LazyLayoutPrefetchState.kt:740-773`: `createPausedPrecomposition`, `apply`).

### 1.3 `movableContentOf`

`movableContentOf(content)` (`MovableContent.kt:39-42`, with 0 to 4 parameters and `movableContentWithReceiverOf`, `:62-258`) wraps the lambda in a `MovableContent` object and returns a composable lambda calling `currentComposer.insertMovableContent(movableContent, parameter)`. The composer (`invokeMovableContentLambda`, `GapComposer.kt:2221-2285`) starts a movable group keyed by the `MovableContent` instance, roots the composite key hash at the content (so keys inside don't change as it moves), captures the locals at the call, and:

- **if the group is found in place**, recomposes it (with `providersInvalid` when the locals differ);
- **if it is being inserted**, composes nothing yet: it inserts a placeholder and reports a `MovableContentStateReference` to the parent context (`insertMovableContent`). When a movable group is removed, the composer reports it (`reportFreeMovableContent`, `:2728`, `deletedMovableContent`).

After every composition of the frame has composed, the `Recomposer` matches each insertion with a removal of the same `MovableContent` (`performInsertValues`, `Recomposer.kt:1363-1447`; `movableContentRemoved.removeLast(reference.content)`, `:1378`), across compositions, and the receiving composition moves the removed group's slots and nodes into the placeholder and recomposes it with the new parameter as "late changes" (`insertMovableContentReferences`, `GapComposer.kt:2287-2330`; `applyLateChanges`). An insertion without a removal composes fresh; a removal without an insertion is disposed after the frame (`discardUnusedMovableContentState`, `Recomposer.kt:1449`). State, remember observers (no `onForgotten`/`onRemembered`) and nodes move with the content.

### 1.4 `ReusableComposition`, `PausableComposition`, the `Composition` API

`ReusableComposition` (`Composition.kt:100-124`): `setContentWithReuse(content)` composes the new content reusing the old one's groups and nodes as `ReusableContent` with a new key does; `deactivate()` (`:1559`) deactivates its nodes and forgets its remember observers, keeping the slots for a later `setContentWithReuse`. `PausableComposition` (`PausableComposition.kt:69-112`): `setPausableContent`, `setPausableContentWithReuse` return a `PausedComposition` (`resume(shouldPause)`, `apply`, `cancel`, `isComplete`, `isApplied`, `isCancelled`). `Composition.dispose`, `isDisposed`, `hasInvalidations`; `ControlledComposition` (`:168-300`) splits composing (`composeContent`, `recompose`) from applying (`applyChanges`, `applyLateChanges`, `changesApplied`) for the `Recomposer`, and has `invalidateAll`, `isComposing`, `observesAnyOf`, `recordModificationsOf`.

### 1.5 The `Recomposer`

`Recomposer(effectCoroutineContext)` (`Recomposer.kt:193`): the root context of a thread's (or window's) compositions. `runRecomposeAndApplyChanges()` (`:564-795`), a suspending loop: await work, then in `withFrameNanos` send the frame to its broadcast clock's awaiters, send apply notifications, recompose every invalid composition (parents first, by the invalidation order of §1.1), recompose those observing values other compositions wrote, perform movable content insertions, then apply all compositions' changes, late changes, `changesApplied`, `notifyObjectsInitialized`; after the frame, discard unmatched movable content and run frame-end callbacks (`nextFrameEndCallbackQueue.markFrameComplete()`). The rest of its API: `currentState: StateFlow<State>` (`:418`; `State`: `ShutDown`, `ShuttingDown`, `Inactive`, `InactivePendingWork`, `Idle`, `PendingWork`, `:206`, derived in `deriveStateLocked`, `:352-393`), `hasPendingWork` (`:1524-1534`), `changeCount`, `awaitIdle` (`:1547`), `cancel` (`:1145`: cancels its effects' coroutines), `close` (`:1165`), `join` (`:1172`), `pauseCompositionFrameClock`/`resumeCompositionFrameClock` (`:1561-1580`), `asRecomposerInfo` (`:496`), `runningRecomposers` (companion), `scheduleFrameEndCallback` (`:1191`), `withRunningRecomposer` (`:94`).

### 1.6 `retain`

`runtime-retain/src/commonMain/kotlin/androidx/compose/runtime/retain/`. `retain(keys…, calculation)` (`Retain.kt:186-246`) is `remember(RetainKeys(keys, currentCompositeKeyHashCode, typeHash)) { … }` of a `RetainedValueHolder` (`RetainedValueHolder.kt:20-66`, a `RememberObserver`): on creation it first asks the `LocalRetainedValuesStore` (`LocalRetainedValuesStore.kt:55`, a static local defaulting to `ForgetfulRetainedValuesStore`) for a value saved under those keys (`consumeExitedValueOrDefault`), else calculates. When the holder is forgotten it gives the value to the store (`saveExitingValue`), which keeps it while it is *retaining exited values* (`ManagedRetainedValuesStore`, `ManagedRetainedValuesStore.kt:44-160`) or retires it. `RetainObserver` (`RetainObserver.kt:42-142`): `onRetained`, `onEnteredComposition`, `onExitedComposition`, `onRetired`, `onUnused`. `LocalRetainedValuesStoreProvider(store, content)` (`LocalRetainedValuesStore.kt:76-100`) provides the store and, after the content, remembers a presence indicator that tells the store its content entered composition at the **end of the frame** (`scheduleFrameEndCallback`) and exited when forgotten. `RetainedValuesStoreRegistry` (`RetainedValuesStoreRegistry.kt:46-140`) keeps a managed store per child key (Navigation 3's entries), `clearChild`, `clearChildren`, `dispose`. `RetainedEffect` (`RetainedEffect.kt:87-296`) is a `DisposableEffect` whose cleanup runs when the retained value retires.

## 2. Where compose-rs stands

- **One composition per host, no parents.** `Composition::new(applier)` (`crates/compose-runtime/src/composition.rs:33-45`) has no parent: compositions share no locals and no invalidations, and nothing orders them. Hosts (`UiHost`, `ComposeTestRule`, desktop's `application`) call `Composition::recompose` themselves.
- **One pass at a time per thread** (docs/04, "Concurrency"): `Pass::begin_as` (`composition.rs:276-366`) panics with `PassInProgress` if another pass is active; the pass state (`Runtime.active`, `frames`, `reusing`, `pause`, `pausable_pass`, `resume_next`, `abandoned_groups`, `pending_move`; `crates/compose-runtime/src/runtime.rs:31-80`) is one per thread. Compositions can compose from another pass's end-of-pass work (a popup from an effect).
- **The tree is borrowed while measuring.** `UiHost::lay_out` (`crates/compose-ui/src/host.rs:604-622`) runs `UiTree::layout` inside `Composition::with_applier`, which borrows the applier's `RefCell` for the whole layout; a `MeasureScope` holds `&mut UiTree` (`crates/compose-ui/src/layout.rs:98-106`). Composition can't run during measurement: D125's constraint.
- **Content Compose composes in measurement is composed between measure rounds:** lazy items (`LazyItems`, `crates/compose-foundation/src/lazy.rs:1545-1653`: the measurement writes the range it wants, the frame recomposes and lays out again, up to 16 rounds, D487, D492) and `BoxWithConstraints` (`crates/compose-foundation/src/layout/mod.rs:304-370`: composed for `LOCAL_CONSTRAINTS_HINT`, recomposed in a round when the measured constraints differ, D142). Popups and dialogs are overlay nodes of the same tree, composed in the same composition.
- **Reuse and pausing exist, in one composition:** reusable groups and `ReusableContentHost` (docs/04, "Reusable content", D430), `platform::remember_across_reuse`, and pausable composition by regions (`platform::PausableContent`, `crates/compose-runtime/src/pausable.rs`, D590–D599) instead of Compose's per-item paused subcompositions.
- **Movable groups** match keyed siblings within one parent (`start_movable_group`); a group never changes parent, which the local lookup relies on (each group links to its nearest provider when allocated, D310).
- **Locals** are found by walking provider groups up the slot table (`CompositionLocal::current`, `crates/compose-runtime/src/local.rs:90-120`); a provider keeps its value in a state cell, so readers are invalidated by a new value like any state reader.

## 3. Design

### 3.1 `CompositionContext` in the runtime

```rust
/// A point compositions compose under, as Compose's `CompositionContext`: the Recomposer, or a
/// place in a composition. A Copy, !Send handle; a disposed context's children compose on
/// without its locals (§3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CompositionContext { id: ContextId }

/// As `rememberCompositionContext()`.
pub fn remember_composition_context() -> CompositionContext;

impl Composition {
    /// As `Composition(applier, parent)`. `Composition::new(applier)` stays, under the thread's
    /// default Recomposer.
    pub fn with_parent<A: Applier>(applier: A, parent: CompositionContext) -> Composition;
}
```

Runtime data (in `Runtime`, `runtime.rs:31`):

```rust
pub(crate) struct Contexts { entries: Vec<ContextEntry>, free: Vec<u32>, base: u32 }  // generational, as Cells
pub(crate) struct ContextEntry {
    generation: u32,
    owner: ContextOwner,          // Recomposer(RecomposerId) | Group { composition: CompositionId, group: u32 }
    provider: ProviderRef,        // the nearest provider group at the context: { composition, group } or none
    key_hash: u64,                // the composite key hash at the context
    children: Vec<CompositionId>, // compositions made with this context, in creation order
    removed: bool,                // its group was removed (set during the parent's pass) or forgotten
}
// CompositionData gains:
//   parent: Option<ContextId>, children: Vec<CompositionId> (through its contexts),
//   applier_owner: CompositionId (itself, or the composition whose applier it shares, §3.3),
//   disposed: bool, reuse: Option<ReuseRoot> (§3.5)
```

`remember_composition_context()` is a replace group (`key_of("compose_runtime::rememberCompositionContext")`) remembering, through `remember_observer`, a holder that owns the `ContextId`: forgotten, it marks the entry removed and frees it once no child refers to it. Each time the group runs it refreshes `provider` (`SlotTable::provider_at_or_above(group)`, as `current()` starts its walk) and `key_hash`. `remove_subtree` (`runtime.rs:505-539`) marks a context removed as soon as it removes the group holding it, so the frame doesn't recompose children whose host is going (their owners dispose them in the end-of-pass work).

### 3.2 Child compositions

**Composing at once, nested.** `set_content` composes immediately, as Compose's `composeInitial`, also while another composition's pass is composing (the `Subcompose` pattern of §1.1). `Pass::begin_as` no longer refuses when a pass is active: it moves the active pass's state into a `SavedPass` on the stack (`active`, `frames` by `mem::take`, `reusing`, `pause`, `pausable_pass`, `resume_next`, `abandoned_groups`, `pending_move`), runs the nested pass, and restores it in `finish` (and in the drop guard on unwind). Rules:

- A nested pass of a composition already in the stack of active passes is a misuse (`PassInProgress`), as today.
- The nested pass's end-of-pass work runs at its `finish`, while the outer pass is still composing (Compose's `composeInitial` applies and dispatches effects before returning). Writes made by those effects are writes during composition, allowed by D703.
- Only the outermost pass frees retired cells (`Cells::begin_pass`/`end_pass` are skipped for nested passes): rule R2's "the next pass" means the next outermost pass.
- Reads in the nested pass register with its scopes (`frames.last()` is the child's frame), so the outer composition records none, as `areChildrenComposing` does.
- With composition in a snapshot ([snapshots.md](snapshots.md) §3.13), a nested pass composes in a mutable snapshot nested in the outer pass's composition snapshot and applies into it when its composing ends, as Compose's `composeInitial` called during a composition takes its snapshot from the current one.

**Locals.** `CompositionLocal::current()` (`local.rs:90`) walks provider groups as today; on reaching the child's root (`NONE`), it continues in the parent composition's table at `context.provider`, and so on up. The provider cell read registers the child's scope as a reader of the parent's provider state, so a new value invalidates the child's readers directly, in the parent's pass (providers write their cell while composing), and the child recomposes after the parent in the same frame. When static locals arrive (`local-variants`), a static provider's change invalidates its whole content, which includes invalidating every composition under a context inside it (`invalidate_all`), as Compose's `forceRecomposeChildren`. A child whose context was removed reads the locals' defaults beyond its root, and debug builds log once.

**Composite key hash.** `current_composite_key_hash` starts a child's hash from `context.key_hash` (Compose's `CompositionContextImpl.compositeKeyHashCode`); D290's cut at a `SaveableStateProvider` still applies.

**Invalidation, frames and order.** A child inherits its root's frame requester (and follows it if the root's changes). A root's `has_invalidations` includes its descendants'. Recomposing a root recomposes the tree, **parents before children**: its own pass, then each child with invalidations, depth first, in creation order, skipping children of removed contexts and disposed compositions. Composition and application are split for the tree (§3.6): every composition composes, then movable content is matched (§3.8), then every composition's end-of-pass work runs, parents first, as the Recomposer's `toApply` loop.

**Disposal.** A child is disposed by its owner (`dispose` or `Drop`), which unregisters it from its context. Disposing a parent doesn't dispose its children (Compose's rule); its contexts become removed.

### 3.3 Sharing a node tree, and lending it during measurement

A subcomposition of the UI emits into the **same** `UiTree` as its parent, under a slot node. Two mechanisms make that work with Rust's borrows, without `unsafe` (every compose-rs crate forbids it).

**Sharing.** `platform::subcomposition(parent: CompositionContext, root: NodeRef) -> Composition` (with `reusable`/`pausable` variants, §3.5) makes a child whose `CompositionData.applier` is a clone of its root composition's `Rc<RefCell<dyn ErasedApplier>>`, with `root_node = root`. Outside measurement, a child's pass borrows the shared applier per node operation exactly as the root's passes do (`apply_node_op`, `crates/compose-runtime/src/composer.rs:1065`; `Composer::with_applier`, `:1522`). The runtime tests' `Subcompose` helper uses the same function with a `NodeTree` root node.

**Lending.** During measurement the shared `RefCell` is borrowed by the layout, and the measure scope holds the only `&mut UiTree`. A measurement that must compose lends the tree:

```rust
/// Lends `applier` (the shared applier `composition` uses, which the caller holds borrowed) to
/// the passes run by `f`: what a measurement composing a slot does.
pub fn platform::with_lent_applier<A: LendableApplier, R>(
    composition: &Composition, applier: &mut A, f: impl FnOnce() -> R) -> R;

/// An applier that can stand in for itself while lent: a cheap empty value.
pub trait LendableApplier: Applier { fn placeholder() -> Self; }
```

The runtime keeps, per shared applier, a *lending cell* (`Rc<RefCell<A>>`, created holding `A::placeholder()` on the first lend and kept). `with_lent_applier` swaps the caller's `A` with the cell's placeholder (`std::mem::swap` on two `&mut A`: the real tree moves into the cell, the placeholder into the layout's reference), records `(shared Rc, lending cell)` as the lend in progress, runs `f`, and swaps back, in a drop guard so a panic restores the tree before unwinding further. While lent, every applier access of a composition sharing that `Rc` (the runtime's 17 sites go through one accessor, `applier_for(composition)`) resolves to the lending cell, so `Composer::with_applier::<UiTree>` downcasts as usual and the pass's insertions, moves, removals, deactivations and releases reach the real tree. Rules: a pass of a composition not sharing the lent applier is unaffected; `UiTree::placeholder()` is flagged, and any use of a placeholder as a tree panics with "the tree is lent to a subcomposition" (catching a pass of the *root* composition started during a lend, which would see the placeholder through the borrowed `RefCell`). The swap moves `size_of::<UiTree>()` bytes three times each way (a few hundred bytes: under 0.1 µs), only when a slot composes.

### 3.4 `SubcomposeLayout` in compose-ui

**Prerequisite (ui-core):** virtual layout nodes, as Compose's `LayoutNode(isVirtual = true)`: a node whose children count as its parent's children for measurement, placement, drawing, hit testing and semantics. A slot is one; the subcomposition's root is the slot's virtual node, so a slot can emit several layouts and the policy measures and places them as its own children. (Without virtual nodes, a slot could be a child range of the `SubcomposeLayout` node, the applier offsetting indices by the slots before it, as paused regions offset theirs; virtual nodes are what the lookahead and node-traversal work wants anyway.)

API (`crates/compose-ui/src/subcompose.rs`, re-exported as `compose::ui::layout`):

```rust
#[composable] pub fn SubcomposeLayout(
    #[default(Modifier)] modifier: Modifier,
    measure_policy: Rc<dyn SubcomposeMeasurePolicy>);
#[composable] pub fn SubcomposeLayoutWithState(            // Compose's SubcomposeLayout(state, …) overload
    state: SubcomposeLayoutState, #[default(Modifier)] modifier: Modifier,
    measure_policy: Rc<dyn SubcomposeMeasurePolicy>);

pub trait SubcomposeMeasurePolicy {
    fn measure(&self, scope: &mut SubcomposeMeasureScope<'_>, constraints: Constraints) -> MeasureResult;
}
impl SubcomposeMeasureScope<'_> {                              // derefs to MeasureScope
    /// Slot ids compare by `==` (stored as lazy.rs's `DynKey`, with their hash).
    pub fn subcompose(&mut self, slot_id: impl Hash + PartialEq + 'static, content: Callback) -> Vec<Measurable>;
}

/// A Copy handle owned by an Owned<SubcomposeLayoutState>, as the state holders (D240).
pub struct SubcomposeLayoutState { … }
impl SubcomposeLayoutState {
    pub fn new(policy: impl SubcomposeSlotReusePolicy + 'static) -> Owned<SubcomposeLayoutState>;
    pub fn precompose(&self, slot_id: impl Hash + PartialEq + 'static, content: Callback) -> PrecomposedSlotHandle;
    pub fn create_paused_precomposition(&self, slot_id: …, content: Callback) -> PausedPrecomposition;
}
pub struct PrecomposedSlotHandle;   // dispose, placeables_count, premeasure(index, constraints), get_size(index), traverse_descendants(key, f)
pub struct PausedPrecomposition;    // is_complete, resume(should_pause) -> bool, apply() -> PrecomposedSlotHandle, cancel
pub trait SubcomposeSlotReusePolicy { fn get_slots_to_retain(&self, slots: &mut SlotIdsSet); fn are_compatible(&self, slot_id: &dyn Any, reusable: &dyn Any) -> bool; }
pub fn subcompose_slot_reuse_policy(max_slots_to_retain_for_reuse: usize) -> impl SubcomposeSlotReusePolicy;
```

**The composable** remembers its state (or takes the given one), calls `remember_composition_context()`, and emits a reusable layout node whose measure policy is an adapter holding `Rc<SubcomposeState>` (the slots in folded order, slot id → slot, the reuse pool, the precomposed slots: Compose's `LayoutNodeSubcompositionsState`). A slot holds its virtual node, its `Option<ReusableComposition>` (§3.5), its content `Callback`, `force_recompose`, an `active: OwnedState<bool>` for the `ReusableContentHost` it wraps the content in, and its paused composition. As Compose (`SubcomposeLayout.kt:146-148`), when the composable's body runs (it didn't skip), a `SideEffect` sets `force_recompose` on every slot and asks for the node's remeasurement (`forceRecomposeChildren`, `SubcomposeLayout.kt:1256-1275`). The node's release disposes the slots' compositions.

**Measuring** (`SubcomposeMeasureScope::subcompose`):

```text
UiHost::lay_out ── with_applier(&mut UiTree) ── layout ── measure(SubcomposeLayout node)
   policy.measure(scope, constraints)
     scope.subcompose(slot, content)
       find or make the slot's virtual node (precomposed, from the reuse pool, or new), move it to `current_index`
       if content changed (Callback::ptr_eq) || composition.has_invalidations() || force_recompose:
         Snapshot::without_read_observation(|| ignoring remeasure requests of this node(||
           platform::with_lent_applier(&root_composition, scope.tree, ||
             slot.composition.set_content(|| ReusableContentHost(active, content))   // or set_content_with_reuse
             // a full pass: node changes into the real tree, end-of-pass work, effects
         )))
       return the virtual node's children as measurables
     … measure and place them …
   after placement: deactivate or dispose the slots from `current_index` on (the reuse policy),
     deactivation (`ReusableComposition::deactivate`, §3.5) also lending the tree
```

Slots whose state changed recompose **before layout**, in the frame's recomposition of the composition tree (§3.2), as Compose's Recomposer recomposes subcompositions in its frame; measurement then finds them without invalidations and composes nothing. Premeasurement in the idle time (`UiHost::prefetch`, `crates/compose-ui/src/prefetch.rs:79`) reaches `subcompose` through the same `MeasureScope`, so it lends the same way. Intrinsic queries on a `SubcomposeLayout` panic with Compose's message (`NoIntrinsicsMessage`, `SubcomposeLayout.kt:1008`), where compose-rs's intrinsics today measure a probe.

**The frame loop** (`UiHost::frame`, `host.rs:313-472`) becomes: tasks, timers and frame awaiters (each ending with an apply-notification send, [snapshots.md](snapshots.md) §3.7) → recompose the composition tree, parents first → layout, which subcomposes new or changed slots and deactivates unused ones → the rounds of D487 (only while lazy lists compose outside measurement, §3.10) → drawing → frame-end callbacks → the tasks the frame launched.

### 3.5 `BoxWithConstraints`, `ReusableComposition`, `PausableComposition`, the `Composition` API

**`BoxWithConstraints`** becomes Compose's: `SubcomposeLayout(modifier) { constraints → let measurables = subcompose((), content(BoxWithConstraintsScope { constraints, density })); box_policy.measure(measurables, constraints) }`. D142's hint-then-recompose goes; `LOCAL_CONSTRAINTS_HINT` (`host.rs:75-77`) remains only for lazy items until §3.10's step, then goes.

**`ReusableComposition`, `PausableComposition`** as newtypes over `Composition` (`Deref`), so the methods exist only where Compose's sealed interfaces put them:

```rust
impl Composition {
    pub fn reusable<A: Applier>(applier: A, parent: CompositionContext) -> ReusableComposition;
    pub fn pausable<A: Applier>(applier: A, parent: CompositionContext) -> PausableComposition;
    pub fn dispose(&self);           // idempotent; Drop disposes too; a pass after it is a misuse
    pub fn is_disposed(&self) -> bool;
    pub fn invalidate_all(&self);    // every scope invalid (ControlledComposition.invalidateAll)
}
impl ReusableComposition {
    pub fn set_content_with_reuse(&self, content: impl Fn() + 'static);
    pub fn deactivate(&self);
}
impl PausableComposition {          // derefs to ReusableComposition
    pub fn set_pausable_content(&self, content: impl Fn() + 'static) -> PausedComposition;
    pub fn set_pausable_content_with_reuse(&self, content: impl Fn() + 'static) -> PausedComposition;
}
```

A reusable composition's root content sits in an internal `ReusableContentHost(active)` around a `ReusableContent(reuse_key)`, the runtime's existing groups (docs/04, "Reusable content"): `set_content_with_reuse` sets new content and a new reuse key (its content reused, as Compose's), `deactivate` sets `active` false and runs a pass that deactivates the content (`deactivate_to_end_group`), its nodes deactivated and its observers forgotten at the end of the pass. A pausable composition's root content is one `PausableContent` region: `set_pausable_content` sets the content and returns `recompose_pausable()`'s `PausedComposition`, so the region machinery of D590–D599 is Compose's whole-composition pausing for it. The panic-poisons rule stays (the ledger's known difference 10).

### 3.6 The `Recomposer`

```rust
pub struct Recomposer { … }   // a Copy handle owned by Owned<Recomposer>; one default per thread
pub enum RecomposerState { ShutDown, ShuttingDown, Inactive, InactivePendingWork, Idle, PendingWork }
impl Recomposer {
    pub fn new() -> Owned<Recomposer>;
    pub fn current_thread() -> Recomposer;                 // the default one compositions use
    pub fn as_context(&self) -> CompositionContext;
    pub async fn run_recompose_and_apply_changes(&self);   // Compose's loop on the executor's frames
    pub async fn await_idle(&self);
    pub fn cancel(&self);  pub fn close(&self);  pub async fn join(&self);
    pub fn current_state(&self) -> State<RecomposerState>; // observable; a StateFlow after snapshots S4
    pub fn has_pending_work(&self) -> bool;  pub fn change_count(&self) -> u64;
    pub fn pause_composition_frame_clock(&self);  pub fn resume_composition_frame_clock(&self);
    pub fn as_recomposer_info(&self) -> RecomposerInfo;  pub fn running_recomposers() -> State<Vec<RecomposerInfo>>;
    pub fn schedule_frame_end_callback(&self, action: impl FnOnce() + 'static) -> CancellationHandle;
}
pub async fn with_running_recomposer<R>(block: impl AsyncFnOnce(Recomposer) -> R) -> R;
// platform: what hosts call in their frame instead of Composition::recompose
pub fn platform::perform_frame(recomposer: Recomposer, frame_time_nanos: u64) -> bool;
```

`perform_frame` is Compose's frame body: send apply notifications; recompose the invalid compositions of its trees, parents first (§3.2), then those observing values other compositions wrote during this frame (D703's in-frame recomposition), until none is left; match movable content (§3.8); run every composition's end-of-pass work; frame-end callbacks; `notify_objects_initialized`. That requires splitting `Pass::finish` (`composition.rs:368`) into *end composing* and *apply deferred work*, which a lone `Composition::recompose` still runs back to back. `run_recompose_and_apply_changes` loops: wait for pending work (a waker set by invalidations), then `with_frame_nanos(|time| perform_frame(…, time)).await`. `Composition::new(applier)` registers with `Recomposer::current_thread()`, so existing hosts and tests keep working; `UiHost` and compose-test move onto `perform_frame` in the ui area's step.

Effects: each composition's `LaunchedEffect` and `remember_coroutine_scope` jobs join its recomposer's job group; `cancel` cancels them and moves the state to `ShuttingDown`, `join` resolves when they ended and the run loop returned. `pause_composition_frame_clock` holds back the recomposer's own frame awaiters (a `BroadcastFrameClock` per recomposer between the thread's executor clock and its compositions' `with_frame_nanos`: the `frame-clocks` feature). Compose's concurrent composition (`concurrentCompositionsOutstanding`) is `n/a` (one thread).

### 3.7 Frame-end callbacks

`Composer::schedule_frame_end_callback`, `CompositionContext::schedule_frame_end_callback` and the Recomposer's queue the action for the end of the current frame's `perform_frame` (after effects and movable content settled), asking for a frame when idle; `CancellationHandle::cancel`. `retain` needs it (§3.9).

### 3.8 `movableContentOf`

```rust
/// As `movableContentOf`: a content lambda whose state and nodes move with it.
pub fn movable_content_of(content: impl Fn() + 'static) -> Callback;
pub fn movable_content_of_with<P: 'static>(content: impl Fn(P) + 'static) -> Callback<P>;  // 1–4 parameters as a tuple
pub fn movable_content_with_receiver_of<R: 'static, P: 'static>(content: impl Fn(R, P) + 'static) -> Callback<(R, P)>;
```

The returned `Callback` (compose-rs's composable lambda, `Rc<dyn Fn(A)>`, remembered like any `Rc`) calls `platform::current().insert_movable_content(&inner, param)`, where `inner: Rc<MovableContentInner<P>>` holds the lambda; the `Rc`'s address is its identity.

**Within a composition.** `insert_movable_content` starts a movable group (`GroupKind::Movable`, key `MOVABLE_CONTENT_KEY`, data key = the identity), keeps the parameter as the group's kept argument, roots the composite key hash at the identity, and:

1. **Found in place** (matched among the siblings, as any movable group): compose it with the new parameter.
2. **Inserting:** look in the frame's *parked* movable content for one of this identity (Compose matches the last removed first, `removeLast`). Found: relink the parked group under the current parent (the arena slot table relinks a subtree in O(1)), re-attach its top nodes at the current node index (`Applier::attach`, which paused composition introduced, also takes nodes that were detached), recompute the `enclosing_provider` links of its groups that pointed outside it (D310's links assume groups never change parent; a move is the exception, O(subtree)), invalidate the scopes in it that read a composition local if the provider chain at the new place differs, and compose it with the new parameter. Not found: insert an empty placeholder movable group and record a *pending insertion* (the removal may come later in this pass, or in another composition).
3. **Removing:** `remove_siblings`/`remove_subtree` (`runtime.rs:485-539`) park a movable-content group instead of removing it: unlinked from the table, its top nodes detached (`Applier::remove`, not released), its values kept. Groups record whether they contain movable content (propagated up when inserted), so removal only looks for it where there is some.

At the end of the composition tree's composing (§3.6; for a lone `Composition::recompose`, the end of its pass), pending insertions are matched with parked groups by identity, each match relinked into its placeholder and composed in a *late pass* restricted to that group (the walk the paused composition's resumes use to compose inside a region, with node indices counted from the placeholder's position). Unmatched insertions compose fresh content in a late pass. Unmatched parked groups are removed then, their nodes released and values forgotten in rule R1's order. Moved content's remember observers get no `on_forgotten`/`on_remembered` (state moved, not recreated).

**Across compositions.** Parked groups and pending insertions live in the Recomposer's frame, not in a composition. A group moving between two compositions' slot tables (each composition has its own arena) is transplanted: its subtree copied into the receiving table, recompose scopes repointed (`ScopeEntry.composition`, `.group`), kept arguments and slot values moved, provider links recomputed. Nodes move when both compositions share the applier (subcompositions of one tree: the common case, and `MovableContentTests`' `Subcompose`); between different appliers the state moves and the node groups emit new nodes (released ones replaced), a documented deviation no Compose test exercises.

### 3.9 `retain`

A `retain` module in compose-runtime (`compose::runtime::retain`, Compose's `runtime-retain` artifact):

```rust
pub fn retain<T: Clone + 'static>(calculation: impl FnOnce() -> T) -> T;
pub fn retain_keyed<K: ComposeArg + Hash, T: Clone + 'static>(keys: K, calculation: impl FnOnce() -> T) -> T;
pub fn retain_observer<T: RetainObserver + 'static>(calculation: impl FnOnce() -> T) -> Retained<T>;  // as remember_observer
pub trait RetainObserver { fn on_retained(&self); fn on_entered_composition(&self); fn on_exited_composition(&self); fn on_retired(&self); fn on_unused(&self); }
pub trait RetainedValuesStore { fn consume_exited_value_or_default(&self, key: &RetainKey) -> Option<Rc<dyn Any>>;
    fn save_exiting_value(&self, key: RetainKey, value: Rc<dyn Any>); fn on_content_entered_composition(&self); fn on_content_exit_composition(&self); }
pub struct ForgetfulRetainedValuesStore;  pub struct ManagedRetainedValuesStore;   // enable/disable_retaining_exited_values, is_retaining_exited_values, dispose
pub static LOCAL_RETAINED_VALUES_STORE: CompositionLocal<Rc<dyn RetainedValuesStore>>;  // static once local-variants exist
#[composable] pub fn LocalRetainedValuesStoreProvider(store: Rc<dyn RetainedValuesStore>, content: impl Fn() + 'static);
pub fn with_local_retained_values_store<R>(store: Rc<dyn RetainedValuesStore>, content: impl FnOnce() -> R) -> R;
pub struct RetainedValuesStoreRegistry;   // LocalRetainedValuesStoreProvider(key, content), clear_child, clear_children, dispose
pub fn retain_retained_values_store_registry() -> RetainedValuesStoreRegistry;  pub fn retain_managed_retained_values_store() -> …;
pub fn RetainedEffect<K: ComposeArg>(keys: K, effect: impl FnOnce(&mut RetainedEffectScope) -> RetainedEffectResult);
```

`retain_keyed` is `remember_observer_keyed(RetainKey { keys, positional: current_composite_key_hash(), type_id: TypeId::of::<T>() }, || holder)` with Compose's holder logic (`Retain.kt:209-246`, `RetainedValueHolder.kt`): consult the store first, adopt the value under a new store with a `SideEffect`, give it back on `on_forgotten`, `on_unused` on abandonment. Keys compare with `ComposeArg::same`; stores keep a multimap from `(positional, type_id)` to `(keys, value)` entries, last saved first (Compose's `SafeMultiValueMap.removeLast`). Values are `Rc<dyn Any>` in the store and cloned out typed. Plain `retain` of a value that implements `RememberObserver` can't be detected without specialization (the reason `remember_observer` exists): `retain_observer` is the observer form, and the docs say so where Compose throws. The presence indicator uses `schedule_frame_end_callback` (§3.7). Positional keys depend on compose-rs's composite key hash (D290, the ledger's known difference 7): stable across a content's exit and re-entry under the same `SaveableStateProvider`, which is where Navigation 3 retains entries; movable content roots its hash at its identity (§3.8), so retained values inside it survive moves.

### 3.10 Lazy layouts: should they move onto subcomposition?

**Recommended: yes, as the last step (C8), gated on measurements.**

Why move:

- **The public `LazyLayout`** (`LazyLayout(item_provider, modifier, prefetch_state, measure_policy)`, `LazyLayoutMeasureScope::compose(index)`) is Compose's API on `SubcomposeLayout`, and the foundation ledger's `lazy-layout-public` (`LazyLayoutTest`, 19 tests) waits on this decision; its tests, and Compose's pager and staggered grids built on `LazyLayout`, assume composition inside measurement. An API shaped around rounds would differ from Compose's in when items compose and how often.
- **Composition counts and order match Compose's** (`LazyColumnTest.recompositionOrder`, `LazyListTest`'s count tests; the foundation ledger's divergences "Lazy items are composed in rounds" and "BoxWithConstraints composes first for a hinted constraint"), and items compose exactly what the measurement asks for, one item at a time, instead of D487/D492's geometric requests (a jump composes up to about twice what it shows; zero-height items up to about 5,000 a frame).
- **compose-rs machinery that only exists to avoid subcomposition goes:** the frame's composition rounds (`MOST_ROUNDS`, `host.rs:83-84`, `:390-409`), `LOCAL_CONSTRAINTS_HINT`, `LazyItems`' range bookkeeping, `PausableContentFor` targets (a request's paused composition is the item's own `PausableComposition`, as Compose's), D575's deferred deactivation (deactivating in measurement becomes possible: Compose's behaviour).

What it costs, per item composed (estimates; the gates below decide):

| | Today | On subcomposition |
|---|---|---|
| Pass overhead | items share the frame's pass | a pass per item: begin and finish (~0.1–0.2 µs), a composition snapshot if snapshots S6 lands (~0.03–0.06 µs) |
| Tree access | direct | a lend per composing `subcompose` (~0.1 µs) |
| Locals in the item | walk to the list's providers | one hop across the context, per lookup (an item reads 20–50 locals: ~0.1–0.2 µs) |
| Layout | the list measured, then a round: composed, measured again | measured once, composing inside |
| Per visible item each measurement | the policy's lookups | `subcompose`'s slot lookup and checks (~20–40 ns) even when nothing composes |
| Memory | one arena | a `CompositionData` (~0.3 KB) and a slot-table arena per live item (visible, 7 per content type kept for reuse, prefetched: about 40 on the stress list), arena headroom up to about 2× its groups (~0.1–0.2 MB in all at worst) |

Against docs/15's headless lazy bench at parity (D599, a frame scrolling 200 px: feed 12.04 µs, stress list 17.60 µs, grid 15.52 µs, averaged over frames that mostly compose nothing; prefetching a stress-list item ~19–21 µs): per item composed, the added ~0.4–0.7 µs is about 2–3 %, in the range D598/D599 accepted for pausable composition (+1.2 to +2.8 %), partly paid back by the second layout round a composing frame no longer runs. The scroll-a-pixel frames (2.20 / 2.60 / 4.72 µs) pay `subcompose`'s per-item checks: maybe +0.3–0.6 µs (+10–20 % of a micro-frame; nothing on a device frame of ~1 ms). On the device (docs/15 round 15 at parity: stress list main thread 1.37 / 2.22 / 4.13 ms at p50 / p90 / p99, 502 ms of work a run; docs/12's feed), the expected change is within the runs' spread.

**Gates for C8:** the lazy bench's composing frames and per-item prefetch within +3 %, the scroll-a-pixel frames within +20 %; on the Pixel 8 Pro at parity, 5 interleaved runs a side on the feed and the stress list, no frame percentile beyond the runs' spread, main-thread work within +3 %, Rust heap within +5 %. If a gate fails, the step stops and the user decides between the deviation (rounds, with the public `LazyLayout` documented as differing) and the cost.

## 4. Cost on the hot paths (other than lazy layouts)

- **Nothing changes for code that uses none of it:** a composition without a parent walks locals as today (the hop is taken at the root only); `remove_subtree` checks a group's "contains movable content" bit; the applier accessor checks whether a lend is in progress (a `None` test); `Pass::begin_as` checks whether a pass is active (it already does).
- **A child composition's pass:** ~0.1–0.2 µs of begin and finish, plus its work; nested passes add the save and restore of the pass state (a few moves).
- **Recomposing a tree:** a walk of the children lists of compositions with invalidations.
- **Movable content:** O(1) relinks within a composition; O(subtree) for provider relinking and cross-composition transplants, only when content moves.
- **`BoxWithConstraints`:** one child composition and, when its content composes during measurement, a lend; it no longer composes twice when the hint was wrong (D142).
- Benchmarks: `benchmarks/benches/runtime.rs` gains a subcomposition (compose a child under a context, recompose it from a parent's local change) and a movable-content move; `ui.rs` a `BoxWithConstraints` screen. The existing runtime and ui benches stay within ±2 % for C1–C3 and C5–C7.

## 5. Migration plan

| Step | What | Compose tests it unlocks |
|---|---|---|
| **C1. The `Composition` API** | `dispose`, `is_disposed`, `invalidate_all`, `ReusableComposition` (`set_content_with_reuse`, `deactivate`), `PausableComposition` (`set_pausable_content(_with_reuse)`) with no parent yet (`Composition::reusable(applier)`), the name `current_composite_key_hash_code` | `CompositionReusingTests` (~6), `PausableCompositionTests` (~12 built on a standalone pausable composition), `CompositionTests.disposedCompositionShouldReportAsDisposed` |
| **C2. Frame clocks, the Recomposer, frame-end callbacks** | §3.6, §3.7; `Pass::finish` split; `Composition::new` under the thread's default recomposer | `BroadcastFrameClockTest` (4), `RecomposerTests` (12), `RecomposerTests.jvm` (2 of 4), integration `RecomposerTests.runningRecomposerFlow`, `RecomposerFrameEndSchedulingTests` (3 of 4), `MonotonicFrameClock` pausing |
| **C3. `CompositionContext`** | §3.1–§3.3: contexts, `with_parent`, nested passes, parents-first recomposition, locals and key hash across, `platform::subcomposition`, `with_lent_applier` (runtime tests lend a `NodeTree`) | `CompositionTests` (~8 subcomposition tests), `CompositionLocalTests` (7), `CompoundHashKeyTests` (2), `CompositionReusingTests` (1), `PausableCompositionTests`' parented ones, `CompositionInstanceTests` (5, with composition-data), `CompositionObserverTests`' subcomposition ones (with composition-observer) |
| **C4. `SubcomposeLayout` and `BoxWithConstraints`** (ui, after ui-core's virtual nodes) | §3.4, §3.5; the frame loop | `SubcomposeLayoutTest` (108, less its lookahead cases), `BoxWithConstraintsTest`'s count tests (`allTheStepsCalledExactlyOnce`, `withConstraintCallbackIsNotExecutedWithInnerRecompositions`, `updateConstraintsRecomposingWithConstraints`), the subcompose cases of `TestRuleExecutesLayoutPassesWhenWaitingForIdleTest` |
| **C5. Movable content in a composition** | §3.8 within one composition | `MovableContentTests` (~40 of 49), `CompositionAndDerivedStateTests` (1), `CompositionReusingTests` (2), `RecomposerFrameEndSchedulingTests` (1), `ErrorTraceTests`' 5 (with error-traces) |
| **C6. Movable content across compositions** | §3.8 across, transplants | `MovableContentTests`' subcompose ones (`moveContent_subcompose`, `subcomposeLifetime_*`, `deferredSubcompose_*`, `parameterPassingThroughDeferredSubcompose`), `AndroidInstrumentedMovableContentTests` (4), `AndroidMovableContentTests` (1), `PausableCompositionInstrumentedTests` (2), `ActivityRecreationTest`'s movable case |
| **C7. `retain`** | §3.9 (after C2; static locals help, not required) | `RetainTests` (33; 2 need C5), `RetainedEffectTest` (4) |
| **C8. Lazy layouts on subcomposition** (foundation; gated, §3.10) | `LazyLayout` public, lists, grids on it; prefetch by `create_paused_precomposition`; rounds, hint and targets retired | `LazyLayoutTest` (19), `PausableCompositionInstrumentedTests` (the `SubcomposeLayout` precomposition cases), the lazy composition-count tests; pager and staggered grids can then be built on `LazyLayout`, as Compose's |

C1 and C2 need nothing else; C3 needs C2's split (children composed before anyone applies); C4 needs C3 and ui-core's virtual nodes; C5 needs C2; C6 needs C3 and C5; C7 needs C2; C8 needs C4 and the foundation agent.

## 6. Risks

- **Lending.** A tree in the lending cell is invisible to the layout that lent it; a pass of the root composition during a lend would see the placeholder. Mitigations: the placeholder panics on use; the swap back is in a drop guard; debug assertions that only compositions sharing the lent applier run passes during a lend.
- **Composing inside measurement.** Nodes are inserted under the node being measured: UiTree must ignore the remeasure requests that insertion raises for that node (Compose's `ignoreRemeasureRequests`) while honouring those of other nodes; effects run by the slot's pass may write state read by the layout in progress (as in Compose, picked up at the next layout).
- **Nested passes** break the long-standing "one pass at a time" invariant that pausable composition, reusing and the walk rely on. Mitigation: the saved pass state covers every per-pass field of `Runtime` (a test asserts the field list), and the randomized model (`tests/suite/model.rs`, `pausable.rs`) gains nested subcompositions.
- **Splitting composing from applying** (C2) moves effects of composition A after the composing of composition B in the same frame. Compose's order, but a change for compose-rs hosts with several compositions (desktop's windows).
- **Movable content and D310's provider links:** a moved group's subtree must be relinked, or locals resolve to the old place's providers. The model test gains moves across parents.
- **Per-item compositions (C8)**: the cost estimates may be off; the gates of §3.10 stop the step.

## 7. Decisions to record

The orchestrator numbers them.

1. Child compositions compose at once and may nest inside another composition's pass (revises docs/04's "one pass at a time per thread").
2. Subcompositions of one node tree share its applier; a measurement composing lends the tree by swapping it into a lending cell (no `unsafe`), resolving D125's constraint and D575's rejected alternative.
3. A child's local lookup continues into its parent at the context; provider cells make a parent's new value invalidate the child's readers directly (Compose re-reads a snapshot-state locals map).
4. A composition tree recomposes parents first and applies after all compositions composed (`Pass::finish` split).
5. `BoxWithConstraints` on `SubcomposeLayout`, superseding D142 (`LOCAL_CONSTRAINTS_HINT` stays for lazy items until C8).
6. Movable content parks removed groups and matches insertions at the end of the composing of the tree; across appliers, state moves and nodes are recreated.
7. Lazy layouts move onto subcomposition (C8), superseding D487, D492 and the region targets of D590/D597 (open question 1), revising D575.

## 8. Open questions for the user

1. **Lazy layouts on subcomposition (C8):** recommended, last, behind §3.10's gates, for the public `LazyLayout`, Compose's composition counts and the end of the rounds; the alternative keeps rounds and gives `LazyLayout` a documented, round-shaped deviation.
2. **The in-frame rounds after C8.** Compose recomposes what a layout wrote in the *next* frame; compose-rs's rounds (D474, D487) recompose and lay out again within the frame. With lazy lists and `BoxWithConstraints` composing in measurement, the rounds only serve writes from layout read in composition (an `on_size_changed` writing state, for example). Recommended: drop them at parity, and keep them only as an `Optimization` flag if a measurement shows a gain.
3. **Popups and dialogs** stay overlay nodes composed in their host's composition (no Compose test depends on their being separate compositions), or become subcompositions as Compose's `Popup` (a `rememberCompositionContext` and a composition of their own). Recommended: stay, revisited if a ported test needs it.
4. **Desktop windows inheriting the application's locals** through a `CompositionContext` (closes docs/11's "Windows don't inherit the application's composition locals"). Recommended: yes, after C3, in the desktop host.
5. **Retained values across an Android activity's recreation:** compose-rs's Rust objects survive a `ComposeRsView`'s recreation in the same process, so the host can provide a `ManagedRetainedValuesStore` that outlives it, as Compose retains across configuration changes. Recommended: yes, with C7, in compose-android.

## 9. Alternatives rejected

- **An `unsafe` scoped lend** (a raw `&mut UiTree` in a thread-local, as `scoped-tls` does for shared references): every crate forbids `unsafe`, and the swap costs a few hundred bytes of copying per composing `subcompose`.
- **Composing into a pending subtree attached when the measurement returns** (the ui-core ledger's first option): the measurement needs the slot's nodes measurable before `subcompose` returns, and compose-rs's node updates return values through `with_applier` (D591's deviation), so they can't be recorded for later.
- **An emulation of `SubcomposeLayout` over the rounds** (the ledger's second option): composition counts, slot timing and precomposition would all differ from Compose's, which `SubcomposeLayoutTest` checks.
- **A tree of `Rc<RefCell<LayoutNode>>`** so layout borrows nodes one at a time: a rewrite of UiTree with a borrow on every node access.
- **Children recomposed by their owners** (each `SubcomposeLayout` recomposing its slots in its measurement): it loses parents-first order and recomposes slots only when the layout runs, where Compose recomposes them in the frame.
- **One slot-table arena shared by every composition**, so movable content moves between compositions by relinking: every composition's walk, removal and validation would have to stay within its own groups of a shared arena, for the benefit of a rare move.
- **Popup-style subcompositions for lazy items while keeping rounds**: pays the per-item composition without the parity benefit.
- **`retain` as a separate crate** (`compose-runtime-retain`, as Compose's artifact): one more crate to build for a few hundred lines on the runtime's own primitives (docs/08's compile-speed principle).

## The orchestrator's answers to the open questions (2026-10-26)

Taken while the user was away, by D700's rule (1:1 parity with Compose):

1. **`futures-core` for `Stream`:** yes.
2. **Composition in a mutable snapshot per pass (S6):** yes, gated by the benchmarks as the step says.
3. **`MutableState::edit` requires `T: Clone`:** yes.
4. **Lazy layouts onto subcomposition (C8):** yes, last, behind the gates the step gives.
5. **The in-frame rounds after C8:** dropped at parity; kept only as a compose-rs-only optimization flag if a measurement shows a gain.
6. **Popups and dialogs:** stay in their host's composition unless a ported Compose test observes the difference; then they become separate compositions as Compose's.
7. **Desktop windows inherit the application's locals through a context:** yes, after C3.
8. **A retained-values store on compose-android surviving `ComposeRsView` recreation:** yes, with C7.

Decision blocks: snapshots D750–D799, composition context D1300–D1349. S1 shifts every invalidation, so it needs the whole workspace's tests and an Android A/B (no device setting changes) before it merges.
