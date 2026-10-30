# Design: snapshots, isolation included (D702)

The spec for the state features of the [runtime ledger](../runtime.md): `snapshot-observation`, `computed-state`'s shared machinery, `state-collections`, `snapshot-flow`, `flow-adapters`, `snapshot-isolation`, and the part of `state-gaps` they touch (`referentialEqualityPolicy`, `SnapshotMutationPolicy.merge`). It replaces D16's "observation only" (revised by D702) and builds in D703 (writes during composition allowed). Subcomposition, movable content and the `Recomposer` are in [composition-context.md](composition-context.md); the two meet where the `Recomposer` composes in a mutable snapshot (§3.13).

Compose paths are relative to `frameworks/support/compose/runtime/runtime/src/commonMain/kotlin/androidx/compose/runtime/` at androidx-main `d153d87ed76` unless they name another module; compose-rs paths are relative to the repository root.

## 1. What Compose does

### 1.1 State records and record chains

A state object (`StateObject`, `snapshots/Snapshot.kt:1450-1486`) holds a linked chain of `StateRecord`s (`Snapshot.kt:1391-1448`), newest first: each record has the `snapshotId` it was written in, a `next` link, `assign(other)` (copy the other's fields in) and `create(snapshotId)`. `mutableStateOf` is `SnapshotMutableStateImpl` (`SnapshotState.kt:136-218`), whose record (`StateStateRecord`, `:191-201`) holds one `value`. A state created inside a non-global snapshot gets two records: one with the snapshot's id and one with `PreexistingSnapshotId` (1), so the object reads its initial value everywhere else (`SnapshotState.kt:150-156`). The getter is `next.readable(this).value`; the setter compares with the policy and writes through `overwritable` (`SnapshotState.kt:141-147`).

**Reading** (`readable`, `Snapshot.kt:2362-2419`) calls the current snapshot's read observer, then walks the chain for the *valid* record with the highest id: valid means `id != INVALID && id <= snapshot.id && id ∉ snapshot.invalid` (`valid`, `:2347-2360`). No valid record is `readError` ("Reading a state that was created after the snapshot was taken or in a snapshot that has not yet been applied").

**Writing** (`writableRecord`, `:2548-2588`; `overwritableRecord`, `:2590-2609`): if the readable record was born in this snapshot, it is written in place; otherwise a record is found or made with this snapshot's id (`newWritableRecordLocked`/`newOverwritableRecordLocked`, `:2611-2652`): an unused record is reused (`usedLocked`, `:2433-2478`: an `INVALID` record, or a valid one obscured by a newer valid one below the lowest pinned snapshot), else a new one is prepended. `writable` copies the readable record in first (`assign`); `overwritable` doesn't (the caller overwrites every field). The write is recorded in the snapshot's `modified` set and `notifyWrite` bumps `writeCount` and calls the write observer (`:2654-2659`). Records no open snapshot can see any more are reclaimed after applies (`overwriteUnusedRecordsLocked`, `:2480-2524`; `extraStateObjects` keeps objects with more than one live record, `:2228`).

### 1.2 Snapshot ids and the invalid set

Ids are allocated from one global counter (`nextSnapshotId`, `Snapshot.kt:2214`; `SnapshotId` is a `Long` at tip). Each snapshot has an id and an `invalid` set (`SnapshotIdSet`, `snapshots/SnapshotIdSet.kt:40-51`: a 128-bit window above a lower bound plus a sorted array below it): the ids of snapshots that were open when it was taken (`openNonGlobalSnapshots`, `Snapshot.kt:2211`), whose writes it must not see, plus, for a snapshot that advanced, the ids between its old and new ids (`advance`, `:1210-1232`). Open snapshots pin the lowest id they can read (`trackPinning`, `:772-776`; `pinningTable`, a `SnapshotDoubleIndexHeap`, `:2221`), which bounds record reuse.

### 1.3 Mutable, read-only and nested snapshots; apply and merge

`Snapshot` is a sealed class (`Snapshot.kt:63-298`): `takeSnapshot(readObserver)` (read-only, `:353`), `takeMutableSnapshot(readObserver, writeObserver)` (`:424-432`, a nested mutable snapshot of the current one when there is one), `takeNestedSnapshot`, `MutableSnapshot.takeNestedMutableSnapshot` (`:831`), `enter(block)`, `unsafeEnter`/`unsafeLeave` (`:196-204`), `dispose`, `hasPendingChanges`, `snapshotId`, `readOnly`, `root`. `withMutableSnapshot(block)` takes one, enters it, applies it (`check()` throws `SnapshotApplyConflictException` on failure) and disposes it (`:463-480`). `readOnly(block)` (`:734-747`) runs a block in a transparent read-only snapshot that throws on writes.

**Apply** (`MutableSnapshot.apply`, `:878-972`; `innerApplyLocked`, `:1080-1208`): for each modified object it finds three records: *current* (what the next snapshot would read: `readable(first, nextId, openSnapshots)`), *previous* (what this snapshot read before writing: its id, with itself and its previous ids invalid) and *applied* (what it wrote). `current == previous`: no conflict. Otherwise the object's `mergeRecords(previous, current, applied)` decides (`StateObject.mergeRecords`, `:1477-1484`, default `null`): `SnapshotMutableStateImpl` returns `current` when `policy.equivalent(current, applied)`, else a new record holding `policy.merge(previous, current, applied)`, or `null` (`SnapshotState.kt:168-186`; `SnapshotMutationPolicy.merge`, `SnapshotMutationPolicy.kt:58`, default `null`). `null` fails the apply (`SnapshotApplyResult.Failure`); a merged record is prepended with a new id (`advance()`); `current` drops the object from `modified`. Then the snapshot is closed, the global snapshot advanced (`resetGlobalSnapshotLocked`, `:2245-2261`) and apply observers called, first with the global snapshot's pending changes, then with this snapshot's. A nested mutable snapshot applies into its parent instead (`NestedMutableSnapshot.apply`, `:1822-1876`): conflicts against the parent's view, `modified` merged into the parent's, its id cleared from the parent's invalid set; no apply observers.

### 1.4 The global snapshot and `sendApplyNotifications`

The global snapshot (`GlobalSnapshot`, `Snapshot.kt:1720-1790`) is the snapshot every thread reads and writes when it isn't in one (`currentSnapshot()`, `:1381`: the thread's snapshot or the global one). Writes to it are visible at once to everything reading the global snapshot, but **apply observers don't hear them until `sendApplyNotifications()`** (`:710-713`), which advances the global snapshot (new id, `advanceGlobalSnapshot`, `:2269-2301`) and calls the apply observers with its `modified` set. The global snapshot's write observer calls the global write observers on every write (`:1726`). On Android, `GlobalSnapshotManager` (`compose/ui/ui/src/androidMain/kotlin/androidx/compose/ui/platform/GlobalSnapshotManager.android.kt:38-58`) registers one that, on the first write since the last send, schedules `sendApplyNotifications()` on the main dispatcher's next turn. `SnapshotTests.globalChangesCanBeObserved` asserts the batching: after a global write, "Nothing should have been observed yet" until `sendApplyNotifications()`.

### 1.5 Observers

- **Apply observers** (`registerApplyObserver`, `Snapshot.kt:652-660`): `(Set<Any>, Snapshot) -> Unit` per applied snapshot, returning an `ObserverHandle`. Registering advances the global snapshot first, so an observer doesn't see earlier changes.
- **Global write observers** (`registerGlobalWriteObserver`, `:676-690`).
- **Read and write observers** of a snapshot (`takeSnapshot(readObserver)`, `takeMutableSnapshot(readObserver, writeObserver)`), merged with the parent's for nested snapshots (`mergedReadObserver`, `mergedWriteObserver`, `:2145-2176`).
- **`Snapshot.observe(readObserver, writeObserver, block)`** (`:493-566`): runs `block` in a transparent snapshot (`TransparentObserverMutableSnapshot`, `:1886-2033`, reused per thread) that adds observers to the current snapshot without isolating anything.
- **`Snapshot.withoutReadObservation(block)`** (`:634-645`): runs `block` in a transparent snapshot with no read observer: composition and layout don't subscribe to what it reads.
- `isApplyObserverNotificationPending` (`:314`), `notifyObjectsInitialized` (`:698`: objects created so far count as initialized, so later writes to them are changes; it advances the current snapshot), `openSnapshotCount` (`:749`).

### 1.6 `SnapshotStateObserver`

`snapshots/SnapshotStateObserver.kt:47-726`: `SnapshotStateObserver(onChangedExecutor)`, `observeReads(scope, onValueChangedForScope, block)` (`:223-274`: runs `block` under `Snapshot.observe`, recording each read under `scope` in an `ObservedScopeMap` per `onValueChangedForScope` lambda, `:371`), `clear(scope)`, `clearIf(predicate)`, `start()` (registers an apply observer, `:326-329`), `stop()`, `notifyChanges(changes, snapshot)` (`:340`), `clear()`. On apply it maps changed objects to scopes (`recordInvalidation`, `:641-690`), re-evaluates derived and computed states read by the scopes (`rereadIndirectState`, `:692-718`), and calls `onValueChangedForScope(scope)` through the executor. It is how Compose's ui observes measure, layout and draw reads (`OwnerSnapshotObserver`).

### 1.7 The state collections

`SnapshotStateList` (`snapshots/SnapshotStateList.kt:35-256`), `SnapshotStateMap` (`snapshots/SnapshotStateMap.kt`, `persistentHashMapOf` at `:37`) and `SnapshotStateSet` (`snapshots/SnapshotStateSet.kt`, `persistentSetOf` at `:168`, an insertion-ordered set) are state objects whose single record holds a persistent collection (kotlinx.collections.immutable, vendored under `external/`) plus `modification` and `structuralChange` counters (`StateListStateRecord`, `SnapshotStateList.kt:237-256`). A mutation builds a new collection from the current one and swaps it in if no other thread changed the record meanwhile (`mutate`, `:122-140`; `conditionalUpdate`, `:159-182`; `attemptUpdate`, `:185-197`); a mutation producing the same collection writes nothing. Reads read the whole record (any read subscribes to the whole collection). `toList()`/`toMap()`/`toSet()` hand out the immutable collection itself. The collections don't override `mergeRecords`: a list written in two snapshots conflicts.

### 1.8 `derivedStateOf`'s record-based caching

`DerivedSnapshotState` (`DerivedState.kt:81-330`) is a state object whose `ResultRecord` (`:87-176`) caches `result`, its `dependencies` (each state read, with its nesting level: `ObjectIntMap<StateObject>`), the `validSnapshotId`/`validSnapshotWriteCount` it was last validated at, and `resultHash`, a hash of the identities and ids of the dependencies' readable records. A read (`value`, `:279-293`) calls the read observer, then `currentRecord` (`:185-268`): the record is valid if it has a result and either the snapshot (id, write count) is unchanged or the dependencies' hash is (`isValid`, `:111-128`; `readableHash`, `:130-170`, which brings nested derived dependencies up to date first). If not valid, the calculation runs under `Snapshot.observe` recording dependencies and their levels, and the result is stored in a new writable record unless the policy finds it equivalent to the old one. After a top-level calculation it calls `Snapshot.notifyObjectsInitialized()` (`:255-264`), which advances the snapshot, so later writes make new records and change the hash. `isInvalidFor(previousValue)` (`:295-298`) re-reads and compares with the policy.

Composition records a derived read with its dependencies (`CompositionImpl.recordReadOf`, `Composition.kt:1157-1215`: `observations.add(value, scope)`, `indirectStates.add(dependency, derivedState)`, `scope.recordDerivedStateValue(value, currentValue)`); a changed dependency invalidates the scope only if `isInvalidFor` says the derived value changed (`addPendingInvalidationsLocked`, `:1108-1147`). So a derived state nobody reads is never recomputed (`CompositionAndDerivedStateTests.onlyEvaluateDerivedStatesThatAreLive`).

### 1.9 `snapshotFlow` and `collectAsState`

`snapshotFlow(block)` (`SnapshotFlow.kt:604`, implementation `snapshotFlowImpl`, `:538-563`): a cold `Flow` that runs `block` under a read observer (in a read-only snapshot), emits its result, then waits on a conflated channel signalled by an apply observer when an applied change touches what `block` read, re-runs it and emits only if the result changed (`!=`). `SnapshotFlowManager` (`:158-219`, experimental) shares one apply observer between flows. `Flow.collectAsState(initial, context, policy)` and `StateFlow.collectAsState(context, policy)` (`:59-110`) are a remembered `mutableStateOf` written from a `LaunchedEffect(flow, context)` collecting the flow.

### 1.10 How the Recomposer composes in a mutable snapshot

The `Recomposer` registers an apply observer that turns changed objects read in composition into `snapshotInvalidations` (`Recomposer.kt:1093-1110`). Each frame it sends apply notifications after the frame clock's awaiters ran (`:635`), turns the invalidations into scope invalidations per composition (`recordComposerModifications`), and composes each invalid composition inside `composing(composition, modifiedValues)` (`:1485-1500`): a `takeMutableSnapshot` whose read observer is `composition.recordReadOf` and whose write observer is `composition.recordWriteOf` plus `modifiedValues.add` (`:1471-1483`), entered for the composition, then applied (`applyAndCheck`, `:1502-1515`: a failure is "Unsupported concurrent change during composition"). Writes made by one composition are fed to the others as if they had happened during their composition (`performRecompose`, `:1335-1361`), and compositions observing them are recomposed in the same frame (`:654-683`). Changes are applied (nodes, remember observers, side effects) after all compositions composed, outside the composition snapshots, and `Snapshot.notifyObjectsInitialized()` closes the frame (`:787`).

## 2. Where compose-rs stands

- **Cells.** A state is a cell in the thread's generational arena (`crates/compose-runtime/src/state.rs:38-205`): `CellEntry { cell: Rc<dyn Any>, readers, observed, dirty, refresh, … }`, the value a `StateCell<T> { value: RefCell<T>, equivalent: fn(&T, &T) -> bool, … }` (`state.rs:455-500`). One value per cell: a write replaces it (`write_with`, `state.rs:826-844`). Handles are `Copy` (`MutableState<T>`, `State<T>`), owners retire cells at the end of the next pass (rule R2). Handles are `!Send` (D14): each thread has its own arena and its own state.
- **Observation.** A read during composition registers the composing scope with its epoch among the cell's 8 latest readers (`register_read`, `state.rs:237-273`, D310); a read under a `platform::ReadObserver` (`crates/compose-runtime/src/observe.rs`) registers `(observer, key, epoch)` in the cell's `observed` list. **A write invalidates at once** (`invalidate_readers_except`, `state.rs:400-452`): current reader scopes are marked invalid (and their compositions' frame requesters queued), keyed observers hear their keys, derived results reading the cell are marked dirty. There is no apply, no batching, no public observer API, and no way to read without subscribing.
- **Derived state** (`crates/compose-runtime/src/derived.rs`) runs its calculation under a `ReadObserver` of its own, **at creation** and again in `flush_derived_states` for every dirty derived state alive, read or not (the ledger's "Behaviour that likely differs" 1). Every plain read pays a dirty check (`refresh_if_dirty`, `state.rs:317-329`, a second runtime borrow).
- **State lists** are `OwnedState<Vec<T>>` cells edited whole (`mutable_state_list_of`, `state.rs:599-612`); no maps, no sets.
- **Writes during composition** panic in debug builds (`write_cell_in`, `state.rs:382-387`; D15, to be lifted by D703).
- **The frame loop** (`crates/compose-ui/src/host.rs:313-472`): due tasks and frame awaiters run (they write state, which invalidated scopes at once), then `Composition::recompose` (which first runs `flush_derived_states`), layout (whose observer drains the keys writes invalidated: `apply_observed_writes`), the rounds (D487), drawing, then the tasks the pass launched.

What is reusable: the arena, handles, owners and rule R2; the reader lists with epochs and their compaction (D310), which are Compose's `observations` map stored per cell; the keyed `ReadObserver`, which is `SnapshotStateObserver`'s scope map; the executor (frame awaiters, wake handler).

## 3. Design

### 3.1 Principles

1. **One snapshot world per thread.** compose-rs state is thread-bound (D14), so the global snapshot, the id counter, the open set, the pinning heap and the observers are per thread, inside the runtime's thread-local `RefCell` (`crates/compose-runtime/src/runtime.rs:31-80`). No locks, no atomics, no retry loops: Compose's `sync { }` blocks, `attemptUpdate`'s retries and the `readable` fallbacks under the lock disappear. What isolation still means on one thread: a mutable snapshot's writes are invisible outside it until applied, a read-only snapshot is a frozen view, and an apply conflicts when something else wrote the same object meanwhile (the global snapshot between turns, a sibling snapshot, an `async` task that held a snapshot open across an `.await`).
2. **Compose's algorithms where they are observable**, verbatim: validity, readable selection, apply with previous/current/applied, merge by policy, nested apply into the parent, observer order, apply-time notification, derived validation. Its internal data structures where its tests are `n/a` (the ledger) may differ.
3. **The common path stays as cheap as today.** A cell keeps one inline record; when no snapshot other than the global one is open, a read checks one id and a global write overwrites in place. Everything isolation costs is paid only while snapshots are open (§3.16).
4. **No user code inside the runtime's borrow** (docs/04, "Re-entrancy"): observers, policies' `equivalent` and `merge`, derived calculations, `Clone` and `Drop` of values all run between borrows, as today.

### 3.2 Modules and the public API

New module `crates/compose-runtime/src/snapshots/` (`mod.rs` for `Snapshot` and the manager, `id_set.rs`, `records.rs`, `apply.rs`, `observer.rs` for `SnapshotStateObserver`, `list.rs`, `map.rs`, `set.rs`), re-exported as `compose::runtime::snapshots` (Compose's package `androidx.compose.runtime.snapshots`); `snapshot_flow` and the flow adapters in `crates/compose-runtime/src/flow.rs`, at the crate's top level as Compose's are in `androidx.compose.runtime`.

| Compose | compose-rs |
|---|---|
| `Snapshot` (sealed), `MutableSnapshot` | `snapshots::Snapshot` (an owner; `Drop` disposes it), `snapshots::MutableSnapshot` (derefs to `Snapshot`); both `!Send` |
| `Snapshot.takeSnapshot(readObserver)` | `Snapshot::take_snapshot(read_observer: Option<ReadObserverFn>) -> Snapshot` |
| `Snapshot.takeMutableSnapshot(r, w)` | `Snapshot::take_mutable_snapshot(r, w) -> MutableSnapshot` (panics in a read-only snapshot, as Compose's `error(…)`) |
| `snapshot.enter(block)`, `unsafeEnter`/`unsafeLeave` | `snapshot.enter(\|\| …)`, `snapshot.unsafe_enter() -> Option<SnapshotToken>`, `snapshot.unsafe_leave(token)` |
| `takeNestedSnapshot`, `takeNestedMutableSnapshot` | the same names, snake case |
| `apply()`, `SnapshotApplyResult` (`Success`, `Failure`, `check`), `SnapshotApplyConflictException` | `apply() -> SnapshotApplyResult` (`#[must_use]` enum, `check()` panics with `SnapshotApplyConflict`, a `Debug + Display` type naming the snapshot) |
| `dispose()`, `hasPendingChanges()`, `snapshotId`, `readOnly`, `root`, `id` | `dispose(self)` (and `Drop`), `has_pending_changes()`, `snapshot_id()`, `read_only()`, `root()` |
| `Snapshot.current`, `isInSnapshot`, `global { }`, `withMutableSnapshot { }`, `readOnly { }` | `Snapshot::current()`, `is_in_snapshot()`, `Snapshot::global(\|\| …)`, `Snapshot::with_mutable_snapshot(\|\| …)`, `Snapshot::read_only(\|\| …)` |
| `Snapshot.observe(r, w, block)`, `withoutReadObservation(block)` | `Snapshot::observe(r, w, \|\| …)`, `Snapshot::without_read_observation(\|\| …)` |
| `registerApplyObserver`, `registerGlobalWriteObserver`, `ObserverHandle` | `Snapshot::register_apply_observer(\|changed: &ChangeSet, snapshot: &Snapshot\| …) -> ObserverHandle`, `register_global_write_observer(\|state: StateObjectId\| …)`; `ObserverHandle::dispose` (and `Drop`) |
| `sendApplyNotifications`, `notifyObjectsInitialized`, `isApplyObserverNotificationPending`, `openSnapshotCount` | `Snapshot::send_apply_notifications()`, `notify_objects_initialized()`, `is_apply_observer_notification_pending()`, `open_snapshot_count()` |
| `Set<Any>` of changed objects; `Any` read/written | `ChangeSet` (iterate, `len`, `contains(impl Into<StateObjectId>)`); `StateObjectId`, a `Copy + Eq + Hash + Debug` erased id every state handle converts into (`MutableState::state_object_id()`, `From` impls) |
| `StateObject`, `StateRecord`, `readable`, `writable`, `withCurrent`, `overwritable` | `snapshots::StateRecord` trait and `snapshots::SnapshotStateObject<R>` handle (§3.3), with `with_current`, `readable`, `writable`, `overwritable` methods: what `MutableState`, the collections and apps' own state objects are built on |
| `SnapshotMutationPolicy` (`equivalent`, `merge`), `structural`/`referential`/`neverEqualPolicy` | `MutationPolicy<T>` gains `fn merge(previous: &T, current: &T, applied: &T) -> Option<T> { None }`; `referential_equality_policy()` (identity: `ComposeArg::same`, a pointer compare for `Rc`s) |
| `SnapshotStateObserver` | `snapshots::SnapshotStateObserver` (§3.8) |
| `SnapshotStateList/Map/Set`, `mutableStateListOf/MapOf/SetOf`, `toMutableStateList/Map` | §3.11 |
| `snapshotFlow`, `SnapshotFlowManager`, `collectAsState` | §3.12 |
| `Snapshot.asContextElement`, `SnapshotContextElement` | `snapshot.enter_future(future)`: a future entering the snapshot around each poll (compose-rs has no coroutine context) |
| `snapshots.tooling.observeSnapshots`, `SnapshotObserver` *(exp)* | `snapshots::tooling::observe_snapshots(observer) -> ObserverHandle`, last (step S7) |

Observer closures are `Rc<dyn Fn(StateObjectId)>` (`ReadObserverFn`, `WriteObserverFn`); `None` is no observer.

### 3.3 Data structures

**Ids and sets.** `type SnapshotId = u64` (`INVALID = 0`, `PREEXISTING = 1`, the first global snapshot 2). `IdSet` ports `SnapshotIdSet`'s layout (a lower bound, two 64-bit windows above it, a sorted `Rc<[u64]>` below), immutable and cheap to clone: `get`, `set`, `clear`, `or`, `and_not`, `add_range`, `lowest`. Its own unit tests are written from `SnapshotIdSetTests`' scenarios (the file is `n/a` as internal; the tests still pin the port).

**The manager**, a field `snapshots: Snapshots` of `Runtime`:

```rust
pub(crate) struct Snapshots {
    next_id: SnapshotId,
    global: SnapshotRef,                // the global snapshot (id, invalid = open set, modified)
    open: IdSet,                        // openNonGlobalSnapshots
    open_count: u32,                    // fast-path test: 0 = only the global snapshot exists
    pins: PinHeap,                      // lowest id an open snapshot reads: the record reuse limit
    current: Option<SnapshotRef>,       // the thread's snapshot; None = global
    view: View,                         // cached from `current`: id, floor, invalid (below)
    observers: Vec<ObserverFrame>,      // read/write observers of `observe`, merged per Compose
    paused_reads: u32,                  // without_read_observation depth
    apply_observers: Vec<(u64, Rc<dyn Fn(&ChangeSet, &Snapshot)>)>,
    global_write_observers: Vec<(u64, Rc<dyn Fn(StateObjectId)>)>,
    sending: u32,                       // is_apply_observer_notification_pending
    send_pending: bool,                 // a global write since the last send
    extra_records: Vec<StateId>,        // cells holding more than one live record (reclamation)
}

/// What a read needs, copied out with the cell's `Rc`: no extra borrow on the read path.
#[derive(Clone, Copy)]
struct View { id: SnapshotId, floor: SnapshotId, read_only: bool, global: bool }
```

`floor` is the lowest id in the current snapshot's invalid set (or `id + 1` if it is empty): a record with `id < floor` is valid without consulting the set, which is the case for nearly every read. `SnapshotRef = Rc<SnapshotInner>`:

```rust
pub(crate) struct SnapshotInner {
    id: Cell<SnapshotId>,
    invalid: RefCell<IdSet>,
    kind: Kind,                          // Global, ReadOnly, Mutable, NestedReadOnly, NestedMutable
    parent: Option<SnapshotRef>,
    read_observer: Option<ReadObserverFn>,   // already merged with the parent's
    write_observer: Option<WriteObserverFn>,
    modified: RefCell<ModifiedSet>,      // StateIds; a Vec plus a HashSet once past 8 entries
    previous_ids: RefCell<IdSet>,
    write_count: Cell<u32>,
    pin: Cell<Option<PinHandle>>,
    status: Cell<Status>,                // Open, Applied, Disposed
    nested_open: Cell<u32>,
}
```

**Records.** A cell's value storage becomes a record list, generic over the record type:

```rust
pub(crate) struct Record<R> { id: SnapshotId, version: u32, data: R }
pub(crate) struct Records<R> { head: Record<R>, rest: Vec<Record<R>> }   // `rest` empty nearly always
```

`version` comes from a per-thread write counter bumped by every write that changes a record (in place or new): it is the record identity derived states validate against (§3.10). The public face for state objects:

```rust
/// A snapshot state object's record, as Compose's `StateRecord`: what each snapshot sees.
pub trait StateRecord: 'static {
    /// A copy, for a write in a snapshot that can't overwrite the readable record (`assign`).
    fn copy(&self) -> Self where Self: Sized;
    /// Merges a conflicting write at apply (`StateObject.mergeRecords`); `None` fails the apply.
    fn merge(previous: &Self, current: &Self, applied: &Self) -> MergeResult<Self> where Self: Sized {
        MergeResult::Fail
    }
}
pub enum MergeResult<R> { Fail, Current, Applied, New(R) }

/// A Copy handle to a state object holding records of type `R`, owned by an
/// `OwnedStateObject<R>` (as `MutableState` by `OwnedState`).
pub struct SnapshotStateObject<R: StateRecord> { id: StateId, _marker: PhantomData<*const R> }
impl<R: StateRecord> SnapshotStateObject<R> {
    pub fn readable<T>(&self, f: impl FnOnce(&R) -> T) -> T;      // observed read
    pub fn with_current<T>(&self, f: impl FnOnce(&R) -> T) -> T;  // unobserved read
    pub fn writable<T>(&self, f: impl FnOnce(&mut R) -> T) -> T;  // copy-then-mutate
    pub fn overwritable<T>(&self, f: impl FnOnce(Option<&mut R>) -> R) -> T; // replace whole
}
```

The arena's erased cell becomes `Rc<dyn StateObjectCell>` (instead of `Rc<dyn Any>`) with the operations the runtime needs without knowing `R`: `readable_slot(id, invalid) -> Option<Slot>` (index, id, version), `merge(previous, current, applied, new_id) -> MergeOutcome`, `prepend_copy(slot, id)`, `reclaim(reuse_limit) -> Reclaimed` (the values it drops come back to be dropped outside the borrow), `as_any()`. `MutableState<T>` is `SnapshotStateObject<ValueRecord<T>>` with `ValueRecord<T> { value: T }` and the cell's policy (`equivalent`, `merge` fn pointers, as `StateCell` keeps `equivalent` today). `State<T>` carries a flag in its handle telling a value cell from a derived state (§3.10), so reading through it dispatches without a downcast.

`CellEntry` loses `dirty` and `refresh` (derived states validate themselves) and gains `in_global_modified: bool` (dedupes the global snapshot's modified list without hashing).

### 3.4 Reads

`MutableState::with(f)`:

1. One runtime borrow, as today: look the cell up (generation check), register the read unless `paused_reads > 0`: the composing scope (the existing reader list, D310) or the keyed observation in progress (`observed`); note whether user observer frames are active; clone the cell's `Rc` and copy `view` out.
2. Outside the borrow: if user observer frames are active, call their read observers with the cell's `StateObjectId` (Compose calls the read observer before selecting the record).
3. Borrow the cell's records (the existing `RefCell` and its location-tracking conflict panic): select the readable record. Fast path: `rest.is_empty() && head.id < view.floor` (or `view.global && open_count == 0`, where the head is always readable) → `head`. Otherwise Compose's walk (highest valid id; `valid` needs the invalid set, fetched from the runtime only in this branch). No valid record panics with Compose's `readError` message.
4. `f(&record.data.value)`.

`refresh_if_dirty`'s second borrow on every read goes away: only derived states validate (§3.10).

### 3.5 Writes

`MutableState::set(value)`:

1. Runtime borrow: look up; if the current snapshot is read-only (`take_snapshot`, `Snapshot::read_only`), panic as Compose's `reportReadonlySnapshotWrite` (`Snapshot.kt:2769`); clone the `Rc`, copy `view`, the reuse limit (`pins.lowest()`, or `next_id`), whether any open snapshot can read records at or above a given id (`open_count`, the open set's highest id).
2. Outside: select the readable record; if `equivalent(readable, &value)`, return (no write, as Compose's setter).
3. Pick the record to write, in order:
   - the readable record was born in this snapshot (`id == view.id`): overwrite it;
   - **the single-threaded fast path:** the snapshot is the global one and no open snapshot can read the readable record (no snapshots open, or every open snapshot's id is below the record's): overwrite it in place and set its id to the global id. Compose would make a second record here and reclaim the first after the next advance (§1.1); the result is the same once reclaimed, without the record;
   - a reusable record (`INVALID`, or obscured below the reuse limit: `usedLocked`'s rule);
   - a new record pushed onto `rest` (the cell joins `extra_records`).
   The replaced value (if any) is dropped after the borrow, as today.
4. Runtime borrow: bump `version`, `write_count`; record the cell in the snapshot's `modified` (the global one: `in_global_modified` flag plus a `Vec` push; others: `ModifiedSet`); if the snapshot is global, set `send_pending` and, the first time since the last send, ask the host for a turn (`executor::wake_host`, `crates/compose-runtime/src/executor.rs:447`) and call the internal global write observer (frame requests, §3.7). If a pass is composing in this composition's snapshot, invalidate this composition's readers of the cell at once (Compose's `recordWriteOf`, `Composition.kt:1233-1240`; D703).
5. Outside: user write observers (the snapshot's, the frames'), then global write observers for a global write.

**Readers are not invalidated at the write.** That moves to apply time (§3.6, §3.7), which is the behaviour change of step S1.

`update(f)` is `with` then `set`. `edit(f)` mutates in place: in the cases where `set` overwrites, it borrows the record mutably; when a copy is needed (a snapshot that must not see this write in place), it copies the readable value first, which needs `T: Clone` (§3.15). `try_get`/`try_set` keep their stale-handle behaviour.

### 3.6 Apply and merge

`MutableSnapshot::apply()` follows `MutableSnapshot.apply` (`Snapshot.kt:878-972`) step by step, without `sync`:

1. Validate it is open (not applied, not disposed: `cannotApplyASnapshotTwice`, `cannotApplyAfterADispose`).
2. If `modified` is empty: close it, advance the global snapshot (`reset_global`: global id = `next_id++`, `global.invalid = open`), and call the apply observers with the global snapshot's pending changes, if any.
3. Otherwise `inner_apply(next_id, modified, open)` (`innerApplyLocked`, `:1080-1208`): for each modified cell, `current = readable_slot(next_id, open)`, `previous = readable_slot(self.id, self.invalid ∪ {self.id} ∪ previous_ids)`; skip if `previous` is `PREEXISTING` or either is absent; on `current != previous` compute `applied` and ask the cell to merge: `Fail` returns `SnapshotApplyResult::Failure` (the snapshot stays open, as Compose's); `Applied` keeps the write; `Current` records a copy of `current` with this snapshot's id and drops the cell from `modified` (into `merged`); `New(r)` records the merged record. Merged records get a fresh id (`advance`) and are prepended. Then close, advance the global snapshot, call the apply observers with the global's pending changes, then with this snapshot's (`ChangeSet`s built from the `StateId` lists), outside the borrow. Then release the pin and reclaim (§3.9).
4. Nested: `NestedMutableSnapshot.apply` (`:1822-1876`): conflicts against the parent's id and invalid set; `modified` moves into the parent's; the parent advances if it is older than the child; the child's id and previous ids are cleared from the parent's invalid set and recorded as the parent's previous ids. No observers.

The value cell's merge is `SnapshotMutableStateImpl.mergeRecords` (`SnapshotState.kt:168-186`): `Current` if `equivalent(current, applied)`, else `New(merge(previous, current, applied)?)`, else `Fail`.

`dispose()` without apply abandons the snapshot: it is removed from the open set, its pin released, and its records become `INVALID` for reuse (Compose's `abandon`, `:1056-1078`). `Drop` of a `Snapshot` owner disposes it if it is still open.

### 3.7 The global snapshot: who sends apply notifications

Writes outside any snapshot go to the global snapshot and are batched until `Snapshot::send_apply_notifications()`, as Compose's (§1.4). compose-rs has no main dispatcher posting a turn, so the places that end a turn send:

- the executor's `run_ready`, `fire_timers` and `dispatch_frame` (`crates/compose-runtime/src/executor.rs:272-415`) end with `send_apply_notifications()` when `send_pending`: what tasks, timers and frame awaiters wrote is notified before the host continues (Compose's Recomposer sends right after `broadcastFrameClock.sendFrame`, `Recomposer.kt:635`);
- `UiHost`'s event entry points (`send_pointer`, `send_key`, `send_text`, `on_back_pressed`, semantics actions; `crates/compose-ui/src/host.rs:625-730`) send before returning, as GlobalSnapshotManager's next-turn send;
- `Composition::recompose` and `Composition::has_invalidations` (`crates/compose-runtime/src/composition.rs:59-82`) send first, replacing today's `flush_derived_states`: a test that writes and then recomposes keeps working, as Compose's `compositionTest { advance() }` sends;
- `UiHost::lay_out` sends before draining its observer (`apply_observed_writes`), so writes made by layout callbacks and measurements reach the next round.

The internal global write observer, called on the first global write since a send, wakes the host (`wake_host`), so a write from a task between frames asks for a frame as today; whether a frame is *needed* is decided when the notifications are sent and scopes invalidated. `UiHost::has_pending_work` (`host.rs:300-308`) also returns true while `send_pending`.

**The internal apply observer** runs first, before user apply observers, with each `ChangeSet`: for every changed cell it does what `invalidate_readers_except` does today (`state.rs:400-452`): current reader scopes invalidated (frame requests queued), keyed observers told their keys, and derived states reading the cell marked *maybe changed* (§3.10). Changes made inside a pass's composition snapshot invalidate their own composition at the write (§3.5 step 4) and the other compositions at the apply, as Compose's `performRecompose` feeds `modifiedValues` to them.

### 3.8 Observation APIs

- **`Snapshot::observe(read, write, block)`** pushes an `ObserverFrame { read, write, merge_parent: true }` for the duration of `block` (no snapshot is created: compose-rs needs no transparent snapshot object, the frame stack is the transparency). Reads and writes call the frames' observers innermost first, then the enclosing ones, as `mergedReadObserver` composes them. Observers taken by a snapshot (`take_snapshot(Some(r))`) are its frame's while it is entered.
- **`Snapshot::without_read_observation(block)`** increments `paused_reads` (and restores it with a guard on unwind): no composition reader, no keyed observation, no user read observer records while it runs. `RecomposerTests.readDuringWithoutReadObservationDoesntCauseRecomposition` is its test. compose-ui's text and lazy code that reads state "untracked" today (none does: there was no way) can use it.
- **`Snapshot::read_only(block)`**: the current view gets `read_only: true` for the duration; writes panic (`readOnly_*` tests).
- **`register_apply_observer`** advances the global snapshot first (Compose's `advanceGlobalSnapshot(emptyLambda)` in `:652-660`), so it doesn't hear earlier writes. Apply observers run outside the borrow, in registration order, after the internal one; `sending` counts nesting for `is_apply_observer_notification_pending()`.
- **`SnapshotStateObserver`**: a public type on the internal keyed observer (`platform::ReadObserver`, kept for compose-ui):

  ```rust
  pub struct SnapshotStateObserver { /* one ReadObserver per on-changed callback, scope ↔ key interner */ }
  impl SnapshotStateObserver {
      pub fn new(on_changed_executor: impl Fn(Box<dyn FnOnce()>) + 'static) -> SnapshotStateObserver;
      pub fn observe_reads<S: Hash + Eq + Clone + 'static, R>(
          &self, scope: S, on_value_changed_for_scope: &Rc<dyn Fn(&S)>, block: impl FnOnce() -> R) -> R;
      pub fn clear<S: Hash + Eq + 'static>(&self, scope: &S);
      pub fn clear_if(&self, predicate: impl Fn(&dyn Any) -> bool);   // scopes of every type, as Compose's Any
      pub fn start(&self); pub fn stop(&self);
      pub fn notify_changes(&self, changes: &ChangeSet, snapshot: &Snapshot);
      pub fn clear_all(&self);   // Compose's clear()
  }
  ```

  Callbacks are grouped by `Rc` identity of `on_value_changed_for_scope` (Compose's map per lambda, `:356-369`); each group interns scopes to `u64` keys for its `ReadObserver`. The internal apply handling tells the `ReadObserver`s their keys (today's `notify`, now at apply time); while started, the observer maps keys back to scopes and posts `on_value_changed_for_scope(scope)` through the executor closure, once per scope per apply. Nested `observe_reads` with the same scope add to it, as `observe_with`'s same-key rule does today. Derived and computed states read under a scope are re-evaluated before deciding (Compose's `rereadIndirectState`): the derived state's own conditional invalidation (§3.10) gives that.

### 3.9 Record reclamation

After each apply and each global advance, cells in `modified` and in `extra_records` reclaim (Compose's `overwriteUnusedRecordsLocked`, `:2480-2524`): records no open snapshot can read become `INVALID` (or are removed when more than one spare remains), keeping at most the readable record for the global snapshot plus those pinned by open snapshots. A cell back to one record leaves `extra_records`. The dropped values are returned and dropped outside the borrow. `platform::state_record_count(state)` exposes the count for the ports of `testRecordsAreReusedCorrectly`, `testSimpleReclaimingState` and `testDeferredReclaimingState_*`.

### 3.10 Derived state

`derived_state_of` moves onto records, as Compose's (§1.8), which also fixes its eager computation:

```rust
struct DerivedCell<T> {
    calculation: Box<dyn Fn() -> T>,
    equivalent: Option<fn(&T, &T) -> bool>,      // the policy; None = structural at isInvalidFor
    records: RefCell<Records<ResultRecord<T>>>,
}
struct ResultRecord<T> {
    result: Option<T>,                           // None = Unset
    valid_snapshot: SnapshotId, valid_write_count: u32,
    dependencies: Vec<(StateId, u32 /* version seen */, u8 /* nesting level */)>,
}
```

- **Creation computes nothing.** `derived_state_of` allocates the cell; the first read computes.
- **A read** registers the read of the derived cell itself (the composing scope, a keyed observation, user observers), then validates the readable `ResultRecord`: valid if it has a result and the snapshot (id, write count) is unchanged since validation, or every level-1 dependency's readable record still has the version seen (nested derived dependencies validated first, recursively, as `readableHash` does). compose-rs compares versions instead of hashing record identities: a version changes on every write, so the in-place global writes of §3.5 can't leave a stale result (Compose relies on `notifyObjectsInitialized` advancing the snapshot after each calculation for that, `DerivedState.kt:255-264`; compose-rs calls it too, for parity of snapshot ids).
- **A recomputation** runs the calculation with reads recorded into a dependency list (a thread-local calculation depth gives nesting levels; a calculation reading its own state panics with Compose's message, "A derived state calculation cannot read itself"), under a frame that keeps those reads from registering the composing scope (today's `observe_exclusive`). If the policy finds the new result equivalent to the old, the record keeps it (new dependencies); otherwise a new result is written into a writable record (in place when allowed, §3.5).
- **The composition** registers a derived read as a read of the derived cell (its reader list, as any cell). Each dependency cell's `observed` list holds the derived cell's entry (as today's per-derived `ReadObserver` does, with its key). When an apply notification names a dependency, the internal handler marks the derived state *maybe changed* and, **only if it has current readers** (a scope, a keyed observation, a user `observe_reads`, or a derived state with current readers), queues it; after the borrow, queued derived states validate (recomputing if needed) and invalidate their readers if the result changed by the policy, cascading through derived states reading them. A derived state nobody reads is not recomputed (`onlyEvaluateDerivedStatesThatAreLive`, `ensureCalculateIsNotCalledTooSoon`).
- `flush_derived_states` and `has_pending_derived_states` go (their work is in the send); `platform::flush_derived_states` stays a while as an alias of `Snapshot::send_apply_notifications` for hosts.

`computed_state_of` (the `computed-state` feature, `ComputedState` in Compose) reuses `DerivedCell` without caching outside observation; its design follows this one and is not detailed here.

### 3.11 State collections

**No persistent collections.** A collection record holds an `Rc` of a standard collection, copied on write:

```rust
struct ListRecord<T> { list: Rc<Vec<T>>, modification: u32, structural: u32 }
struct MapRecord<K, V> { map: Rc<HashMap<K, V, FixedHasher>>, modification: u32 }
struct SetRecord<T> { set: Rc<OrderedSet<T>>, modification: u32 }
```

A write goes through `writable`: when the record can be written in place (the single-threaded fast path of §3.5, or a record born in this snapshot) and nothing else holds the `Rc`, `Rc::make_mut` mutates the `Vec` in place, at the cost of a `Vec` operation. A copy (O(n)) happens only when an open snapshot needs the old record, or a `to_list()` result still shares it. Compose pays O(log₃₂ n) on every operation for persistence it needs for concurrent readers; compose-rs has none, and isolation is rare and short-lived. `FixedHasher` is `BuildHasherDefault<DefaultHasher>` so iteration order is deterministic; `OrderedSet<T>` is compose-rs's own insertion-ordered set (a `Vec<Option<T>>` of entries in order with tombstones compacted past half, and a `HashMap<T, usize>` index; `T: Clone + Hash + Eq`), since `persistentSetOf` keeps insertion order. Elements need `Clone` for copies (as Kotlin's collections copy references; `Rc` elements clone cheaply).

API, as handles with owners like `MutableState`/`OwnedState`:

```rust
pub fn mutable_state_list_of<T: Clone + 'static>() -> OwnedStateList<T>;           // derefs to SnapshotStateList<T>
pub fn mutable_state_list_from<T: Clone>(items: impl IntoIterator<Item = T>) -> OwnedStateList<T>; // toMutableStateList
pub fn mutable_state_map_of<K: Clone + Hash + Eq, V: Clone>() -> OwnedStateMap<K, V>;
pub fn mutable_state_set_of<T: Clone + Hash + Eq>() -> OwnedStateSet<T>;
pub fn snapshot_state_list<T: Clone>(size: usize, init: impl FnMut(usize) -> T) -> OwnedStateList<T>; // SnapshotStateList(size, init)

impl<T: Clone + 'static> SnapshotStateList<T> {          // Copy handle
    // reads (observed): len, is_empty, get(i) -> Option<T>, with(|v: &[T]| …), contains, index_of,
    //   last_index_of, to_list() -> ImmutableList<T>, iter_cloned() (a Vec snapshot)
    // writes: push, insert, extend, insert_all(at, items), remove(i) -> T, remove_item(&T) -> bool,
    //   remove_all(&[T]), retain(pred), retain_all(&[T]), set(i, v) -> T, clear, truncate,
    //   remove_range(range), swap, sort_by(f), edit(|v: &mut Vec<T>| …) for anything else
    // sub_list(range) -> StateSubList<T>: a live view checking `structural` (Compose's subList)
}
```

The map (`get`, `contains_key`, `insert`, `remove`, `entry`-like `update(k, f)`, `keys()`, `values()`, `entries()` as cloned snapshots, `retain`, `clear`, `to_map()`, `with(|m| …)`) and the set (`contains`, `insert`, `remove`, `retain`, `clear`, `iter` in insertion order as a snapshot, `to_set()`, `with`) follow the same shape. An operation that leaves the collection as it was writes nothing (Compose's `newList == oldList`: `retain` removing nothing, `remove_item` of an absent item, `remove_range` of an empty range, `insert` of a present set element); `clear` always writes, as `clearImpl`. Each structural change bumps `structural` (sublists detect it); `set(i, v)` is non-structural. `Debug` reads without observing (`toStringOfSnapshotStateListDoesNotTriggerReadObserver`: `with_current`). The records don't merge (`MergeResult::Fail`), as Compose's.

Saving (the `saveable-gaps` feature): `list_saver`, `map_saver`, and savers of `OwnedStateList`/`OwnedStateSet` for `remember_saveable`.

### 3.12 `snapshot_flow`, the `Stream` decision, `collect_as_state`

**The trait: `futures_core::Stream`.** `futures-core` 0.3 (no dependencies, `default-features = false, features = ["alloc"]`; already in `Cargo.lock` through zbus and async crates, not a direct dependency of any crate) defines the ecosystem's `Stream`, so streams from other libraries (channels, network clients) collect as state and compose-rs's flows compose with `futures-util` in apps that take it. compose-rs adds no combinators beyond what its own APIs need: a `next()` future (`compose_runtime::flow::next(&mut stream).await`) and nothing else.

```rust
/// As `snapshotFlow`: runs `block` (in a read-only snapshot, observing its reads), yields its
/// result, and yields again when an applied change to what it read gives a different result.
pub fn snapshot_flow<T: PartialEq + 'static>(block: impl Fn() -> T + 'static) -> SnapshotFlow<T>; // impl Stream, !Send
pub fn snapshot_flow_with<T>(manager: &SnapshotFlowManager, block: …) -> SnapshotFlow<T>;          // (exp)
pub struct SnapshotFlowManager { … }   // one apply observer shared by its flows; dispose()/Drop
```

`SnapshotFlow::poll_next`: on the first poll run `block` under `Snapshot::observe` in a read-only snapshot (`take_snapshot`), keep the set of `StateId`s read, yield the result. Later polls: if the "changed" flag is set, clear it and the read set, run again, yield if `!=` the last, else register the waker and return `Pending`. An apply observer (registered at the first poll, unregistered on `Drop`) sets the flag and wakes the waker when a `ChangeSet` intersects the read set (`snapshotFlowImpl`, `SnapshotFlow.kt:538-563`, with its conflated channel). A manager keeps one apply observer and a `StateId → flows` map (Compose's `MultiSubscriptionSnapshotFlowManager`).

**`collect_as_state`.** Compose keys the collection on the flow's identity; Rust streams have none, so the factory runs when a key changes:

```rust
pub fn collect_as_state<K: ComposeArg, S: Stream + 'static>(key: K, initial: S::Item, stream: impl FnOnce() -> S) -> State<S::Item>
    where S::Item: PartialEq + Clone;
pub fn collect_as_state_with<…, P: MutationPolicy<S::Item>>(key, initial, policy, stream) -> State<S::Item>;
```

A `remember`ed `mutable_state_of(initial)` written by a `LaunchedEffect(key, …)` that builds the stream and writes each item (`collectInto`, `SnapshotFlow.kt:98-110`). **`StateFlow`**: `flow::MutableStateFlow<T>` (`Clone` handle on `Arc<Mutex<…>>`, `Send + Sync` for `T: Send`: background threads set values; `value()`, `set_value()`, `compare_and_set`, `subscribe() -> impl Stream`), `flow::StateFlow<T>` its read-only view, and `state_flow.collect_as_state()` keyed by the flow's identity with its current value as the initial one. These are what `FlowAdapterTest` and `CollectAsStateTests` need; apps with tokio's `watch` channel adapt it through `collect_as_state`.

### 3.13 Composition in a mutable snapshot

Each pass composes inside a mutable snapshot, as `Recomposer.composing` (§1.10):

- `Pass::begin_as` (`crates/compose-runtime/src/composition.rs:276`) takes a mutable snapshot of the current one (the global one between frames; the parent's composition snapshot for a nested pass, [composition-context.md](composition-context.md) §3.2) and enters it. No observer closures: the composition's reads register through the reader lists as today, and its writes invalidate its own readers at once (§3.5 step 4).
- The snapshot is applied when **composing ends**, before the end-of-pass work (`Pass::finish`, `composition.rs:368`): releases, forgotten values, `on_remembered` and side effects run in the global snapshot, as Compose applies changes outside the composition snapshots. Its changes reach other compositions through the internal apply observer (§3.7). A failed apply panics with Compose's message; single-threaded, it needs a write to the same state by something running during the pass outside it (no such path exists in compose-rs's own code, so it would be an app bug, as in Compose).
- A paused composition's resumes each compose in a snapshot of their own (Compose's `recomposePaused`), its `apply` runs in the global one. A subcomposition composing during measurement takes its snapshot from the global one (layout runs there).
- A pass that panics disposes its snapshot unapplied (its writes are discarded) and poisons the composition, as today.
- `Snapshot::notify_objects_initialized()` runs at the end of each host frame's recomposition (`Recomposer.kt:787`). Until this step, a pass's start only advances the global snapshot (D756), so what it writes of what was made before is a change, and the end of its deferred work and of each turn advance it (D754).

This is step S6, measured on its own (§3.16): it costs a snapshot per pass, and the semantics it adds are visible only to code that reads the global snapshot during composition, writes during composition (D703) or panics mid-pass.

### 3.14 Ownership and threading

What Compose's multithreaded model becomes:

| Compose | compose-rs |
|---|---|
| One global snapshot, shared by all threads, under `sync` | One per thread, in the runtime's `RefCell`; no locks |
| `threadSnapshot` (`SnapshotThreadLocal`) | `Snapshots::current` |
| A snapshot entered on another thread, `SnapshotContextElement` resuming coroutines in it | `Snapshot` is `!Send`; `snapshot.enter_future(fut)` enters it around each poll of a future on the UI thread's executor (`SnapshotContextElementTests`' two tests) |
| `MutableSnapshot` applied from a background thread (`AndroidSnapshotTests`, `SnapshotTestsJvm`) | `n/a` (D14); background work hands results to the UI thread (`spawn_background`), which writes |
| Apply observers called on the applying thread | called on the thread (the only one that can apply) |
| `SnapshotStateList.mutate` retry loops | one attempt |
| `GlobalSnapshotManager` posting a send to the main dispatcher | turn ends send (§3.7) |
| Record reuse guarded by the pinning heap against other threads' readers | the same heap, guarding only the thread's open snapshots |

### 3.15 Migrating existing state

- `MutableState<T>`, `State<T>`, `OwnedState<T>`, `mutable_state_of`, `mutable_state_of_with`, `get`, `with`, `set`, `update`, `try_get`, `try_set`, `is_alive`, `as_state` keep their signatures. `MutationPolicy` gains `merge` with a default; `referential_equality_policy` is added.
- **`edit` requires `T: Clone`** (it may have to copy the record, §3.5). Every `edit` in the workspace today is on a `Clone` value (49 call sites, mostly `Vec`s); a non-`Clone` value can use `set`/`update`.
- **`mutable_state_list_of`** returns `OwnedStateList<T>`, whose handle `SnapshotStateList<T>` keeps `edit(|v: &mut Vec<T>| …)` and `with(|v| …)`, so its 8 call sites (examples/todo, sketches, tests) change little; `.get()` of the whole `Vec` becomes `.to_list()` or `.with(…)`.
- **Invalidation at apply time** changes what compose-rs's own tests observe right after a write: a scope is invalid, a frame requester called, a `ReadObserver` notified only after a send. Tests that check those directly after `set` without recomposing add `Snapshot::send_apply_notifications()` (Compose's tests do the same); D53's note (a synchronous frame requester runs the next pass inside the handler at its next write) becomes "at the turn's send".
- `set_while_composing` (composition local providers) and `set_derived` become writes in the composition snapshot invalidating at once (§3.5 step 4); the debug panic on writes during composition goes with D703.
- `platform::ReadObserver` stays (compose-ui's measure and draw observation); its notifications arrive at apply time. `platform::flush_derived_states` becomes an alias of `Snapshot::send_apply_notifications` for one release.
- Rule R2 is unchanged: a freed cell is gone for every snapshot (a snapshot reading a freed handle gets the stale-handle panic, `try_get` gives `None`).

### 3.16 Cost on the hot paths

Today's figures (docs/14, "Runtime core, headless", an M-series Mac): a state read in composition 6.3–6.9 ns, outside composition 5.3 ns, the counter click (a write and its recomposition) 248–255 ns. Estimates for the design, to be measured at each step with `benchmarks/benches/runtime.rs` (which gains: a write outside composition, a write then send, a read in a mutable snapshot, a derived read valid and invalid, a list push, an empty snapshot take-and-apply):

| Path | Today | With the design | Why |
|---|---|---|---|
| Read outside composition, no snapshot open | 5.3 ns | ~5 ns | one id compare; the `refresh_if_dirty` borrow on every read goes |
| Read in composition (composition snapshot current, S6) | 6.3–6.9 ns | ~6.5–7.5 ns | head id against the view's floor |
| Read with several records (isolation in use) | — | ~10–20 ns | the record walk, the invalid set |
| Global write, no snapshot open | write + invalidation at once | write ~2–4 ns more (modified flag, version); invalidation the same work at the send, once per cell per turn however many writes | |
| Write in a mutable snapshot | — | +20–40 ns (record, modified set); a collection's first write copies it | |
| Pass (S6) | — | +30–60 ns (take, enter, apply an unmodified snapshot, advance) | allocation-free: `SnapshotInner` reused per composition |
| Derived read, valid | dirty check + read | snapshot unchanged: ~2 compares; changed: one version compare per dependency | |
| Frame | — | one send (~10 ns when nothing changed) | |

So the counter click may go from ~250 to ~280–300 ns once S6 lands (a pass is ~250 ns, the snapshot ~30–60 ns); frames composing anything real (tens of µs to ms) move by less than 1 %. The gates per step: the runtime benches within ±5 % for S0–S4 (reads must not regress more), within +20 % on the counter click for S6, and `benchmarks/benches/lazy.rs` and `ui.rs` frames within ±1 % for every step (they read and write state on every frame). A device A/B (docs/12's feed and docs/15's stress screens at parity) for S1 (the change of invalidation timing) and S6.

## 4. Migration plan

Each step ships with its tests, adapted from the Compose files named (the ledger's statuses change in the same commit), and its benchmarks.

| Step | What | Compose tests it unlocks |
|---|---|---|
| **S0. Record-shaped cells** | `Rc<dyn StateObjectCell>`, `Records<R>` with one inline record, `StateObjectId`, `version`; `MutableState` on `ValueRecord<T>`; no behaviour change | none (refactor); all existing tests pass; runtime benches ±2 % |
| **S1. Observation** | the global snapshot (ids, `modified`, `send_apply_notifications`, sends at turn ends), invalidation at apply time, `register_apply_observer`, `register_global_write_observer`, `ObserverHandle`, `Snapshot::observe`, `without_read_observation`, `is_apply_observer_notification_pending`, `SnapshotStateObserver`, `Debug` of states not observing | `SnapshotStateObserverTestsCommon` (31 of 47; the 16 others need computed-state), `SnapshotStateObserverTestsJvm` (3), `SnapshotTests`' observer tests (`stateReadsCanBeObserved`, `stateWritesCanBeObserved`, `globalChangesCanBeObserved`, `applyObserverNotificationIsPendingWhileSendingApplyNotifications`, `changingAnEqualityPolicyStateToItsCurrentValueIsNotConsideredAChange`, `changingANeverEqualPolicyStateToItsCurrentValueIsConsideredAChange`, `toStringOf*`, `toStringValueOf*`, about 10), `RecomposerTests.readDuringWithoutReadObservationDoesntCauseRecomposition` |
| **S2. Derived state on records** | §3.10: lazy, validated by versions, conditional invalidation of live readers only | `CompositionAndDerivedStateTests` (19 of 20; one needs movable content), `DerivedSnapshotStateTests` (7 of 12 without isolation), the derived cases of `SnapshotStateObserverTestsCommon`; the ledger's known difference 1 closes |
| **S3. State collections** | §3.11 and the list/set savers | `SnapshotStateListTests` (59 of 66), `SnapshotStateMapTests` (most of 59), `SnapshotStateSetTests` (most of 21); with saveable-gaps, `RememberSaveableWithMutableStateTest.simpleRestoreList`/`simpleRestoreSet` |
| **S4. Flows** | `futures-core`, `snapshot_flow`, `SnapshotFlowManager`, `collect_as_state`, `MutableStateFlow`/`StateFlow` (after produce-state-scope) | `SnapshotFlowTests` (9), `CollectAsStateTests` (4), `FlowAdapterTest` (14) |
| **S5. Isolation** | ids and `IdSet`, read-only, mutable and nested snapshots, `enter`, `apply` and merge, `dispose`, the pinning heap, reclamation, `global`, `with_mutable_snapshot`, `read_only`, `unsafe_enter`/`unsafe_leave`, `enter_future`, `StateRecord`/`SnapshotStateObject` public | `SnapshotTests` (the other ~79), `SnapshotContextElementTests` (2), the snapshot tests of the list (7), map and set suites, `DerivedSnapshotStateTests` (5: `statesCanBeUsedInSnapshot`, `snapshotsAreIsolatedFromGlobalChanges`, …), `ComputedSnapshotStateTests`' 5 isolation tests (with computed-state) |
| **S6. Composition in a snapshot** | §3.13; with D703's in-frame recomposition of what composition wrote | `CompositionTests`' snapshot-isolation test, `RecomposerTests`' state-change-during-apply cases, D703's `testImminentInvalidationForCurrentGroup`, `testObservationScopes_ReadInRemember` |
| **S7. Snapshot tooling** *(exp)* | `snapshots::tooling::observe_snapshots`, `SnapshotObserver`, `SnapshotInstanceObservers` | `snapshots/tooling/SnapshotObserverTests` (14) |

S1 to S4 are the "observation and collections first" of D702; S5 and S6 the isolation after. computed-state fits after S2 (it shares §3.10), before S4.

## 5. Risks

- **The timing of invalidation changes everywhere (S1).** Every state write in compose-rs is notified later than today: at the end of its turn instead of in the write. Frames are unaffected (each sends before recomposing), but code relying on synchronous invalidation (tests asserting `has_invalidations` or a requester call right after `set`, D53's synchronous hosts) changes. Mitigation: S1 runs the whole workspace's tests, adds sends where Compose's tests have them, and keeps the device A/B gate.
- **A missed send.** A write in a place no turn end covers (a host's own callback outside `UiHost`'s entry points, a JNI upcall in compose-android) would wait for the next send. Mitigation: the global write observer wakes the host on the first write, and the host's frame sends first; a debug assertion at the start of each frame that nothing is pending after its send.
- **Derived state semantics change (S2).** Lazy evaluation fixes the ledger's difference 1 but may expose app code that relied on a calculation's side effects at creation. compose-rs's own derived uses are few (crates/compose/tests/suite/derived.rs, review_h.rs).
- **`edit` gains `T: Clone`**, a breaking change for non-`Clone` state (none in the workspace).
- **O(n) collection copies under isolation** (S3 with S5): a large list written in a long-lived mutable snapshot copies once per snapshot. Mitigation: measured in S5; a persistent vector can replace `Rc<Vec<T>>` behind the same API if it shows.
- **Record reclamation bugs** leak memory silently. Mitigation: `platform::state_record_count`, the reclaiming tests, and a debug check at thread exit that no cell holds more than one record when no snapshot is open.

## 6. Decisions to record

The orchestrator numbers them; each revises or supersedes the rows named.

1. Snapshots are per thread (D14): one global snapshot and id space per thread's runtime, no locks.
2. Invalidation happens at apply time (`send_apply_notifications` at turn ends), not at the write: revises the write-time invalidation of docs/05 and D53's wording.
3. Single-record fast path: a global write with no open snapshot able to read the record overwrites it in place (a deliberate deviation from Compose's record-per-advance, invisible to its tests).
4. Derived state validates by record versions (Compose hashes record identities): revises D122's mechanism, same semantics as Compose's.
5. State collections hold `Rc` copy-on-write standard collections, not persistent ones; sets keep insertion order with compose-rs's own `OrderedSet`.
6. `futures-core` is the `Stream` trait (a dependency decision; open question 1).
7. Composition composes in a mutable snapshot per pass (open question 2).
8. `MutableState::edit` requires `T: Clone` (open question 3).

## 7. Open questions for the user

1. **`futures-core` as a dependency of compose-runtime** for `Stream` (tiny, no dependencies, the ecosystem's trait), or compose-rs's own `Stream` trait (no interoperability with other crates' streams). Recommended: `futures-core`.
2. **Composition in a mutable snapshot (S6)**: exact Compose semantics (writes during composition isolated until the pass ends, discarded on a panic) for about 30–60 ns a pass (the counter-click micro-benchmark +10–20 %, real frames under 1 %). Recommended: yes, with the benchmark gate; if declined, passes compose in the global snapshot and the handful of tests depending on it are marked as known differences.
3. **`edit` requiring `T: Clone`**, or keeping `edit` without the bound and panicking when a copy is needed inside a snapshot. Recommended: the bound.

## 8. Alternatives rejected

- **Keeping write-time invalidation** for the internal mechanisms and batching only the public apply observers: two notification timings, and isolation needs apply-time invalidation anyway (a write in a mutable snapshot must not invalidate until applied). Compose's `globalChangesCanBeObserved` pins the batching.
- **A persistent-collections crate** for the state collections: `im`/`im-rc` (bitmaps, rand_core, rand_xoshiro, sized-chunks, typenum: heavy for compose-runtime, which depends on `log` only), `rpds` (archery), `imbl`. They buy O(log n) copies that only matter under isolation, at a constant cost on every operation. Writing compose-rs's own RRB vector is kept as the fallback (§5).
- **`indexmap` for `SnapshotStateSet`**: light (hashbrown, equivalent), in the lockfile already through toml; rejected only to keep compose-runtime's dependencies at `log` (and `futures-core`). Reasonable to take if `OrderedSet`'s removals prove slow on large sets.
- **Records as `Rc<dyn Any>` values in the arena entry** (the entry owning a list of erased records): a downcast and an `Rc` per record access; the typed `Records<R>` inside the cell keeps the value inline.
- **A transparent snapshot object per `observe`/`without_read_observation`** (Compose's `TransparentObserverMutableSnapshot`, reused per thread): compose-rs's observer frame stack needs no snapshot id or object; the tests about reusing transparent snapshots (`testObserveReusesTransparentSnapshot`, …) are about Compose's allocation strategy and are `n/a`.
- **Hashing record identities for derived states** (Compose's `readableHash`): versions are exact and cheaper to compare in Rust, where record identity would be an index that in-place writes keep.
- **Locks and `Send` state** to mirror Compose's threads: D14 holds; no Compose behaviour that compose-rs's single-threaded runtime can show needs them.

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
