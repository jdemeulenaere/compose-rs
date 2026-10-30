# 5. State

## Requirements

1. Reading state during composition registers the reading scope, and writing it invalidates exactly those scopes, as with Compose's snapshot observation.
2. Handlers are `'static` closures stored in UI nodes, so they must reach state **without borrowing it**.
3. State can live in a `remember` slot or **inside plain data**: a `Todo` with a `done` field in an ordinary `Vec<Todo>`. Toggling one item must recompose only its row.
4. No GC, no reference cycles, and no undefined behaviour on any misuse.

## Types

```rust
/// Owns one state cell. Not Copy; dropping it retires the cell.
pub struct OwnedState<T: 'static> { handle: MutableState<T> }

/// A Copy, thread-bound handle to a cell: what code and closures pass around.
#[derive(Copy, Clone)]
pub struct MutableState<T: 'static> { id: StateId, _marker: PhantomData<*const T> } // !Send

/// A read-only view of the same cell.
#[derive(Copy, Clone)]
pub struct State<T: 'static> { … }

pub fn mutable_state_of<T: PartialEq + 'static>(value: T) -> OwnedState<T>;
pub fn mutable_state_of_with<T: 'static>(value: T, policy: impl MutationPolicy<T>) -> OwnedState<T>;
pub fn mutable_state_list_of<T: 'static>() -> OwnedStateList<T>;     // Compose's mutableStateListOf(); maps and sets too, below
```

State holders (`DrawerState`, `LazyListState`, `CoroutineScope`, …) are `Copy` handles into the same arena ([State holders](#state-holders)).

- **Cells live in a per-thread generational arena.** A handle is an index plus a generation, and a handle of a freed cell is detected, never dereferenced. A cell is an `Rc` in the arena: an access clones it out and releases the runtime before touching the value, so a cell freed during a `with` stays alive until the `with` returns.
- **`OwnedState<T>` derefs to its `MutableState<T>`**, so `todo.done.get()` works on the owner directly, and `todo.done.handle()` gives the `Copy` handle a closure captures.
- **`remember(|| mutable_state_of(0))` returns the handle**, and the slot keeps the owner ([04](04-runtime.md)). This is the common case, and it reads like Kotlin.
- **Handles are `!Send`.** A cell belongs to the UI thread's arena, and the compiler rejects using a handle from another thread. Background work posts to the UI thread, which writes the state.

Why two types: handlers must capture something `Copy` and `'static`, and state inside data must be freed with its owner. Rust can't add implicit capture rules, so the handle is the type users name most (`MutableState`, as Compose code says), and the owner gets a separate name (`OwnedState`). Leptos uses the same arena-plus-`Copy`-handle approach, the most ergonomic answer to "closures must be `'static`" in Rust UI code.

## Reading and writing

```rust
impl<T> MutableState<T> {
    pub fn get(&self) -> T where T: Clone;         // registers a read; cheap for Copy types
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R;   // registers a read; borrows the value
    pub fn set(&self, value: T);                   // invalidates readers unless the policy says equal
    pub fn update(&self, f: impl FnOnce(&T) -> T);
    pub fn edit<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;  // always invalidates (can't tell what changed)
    pub fn try_get(&self) -> Option<T> where T: Clone;      // None if the cell was freed
    pub fn try_set(&self, value: T) -> bool;                 // false if the cell was freed
}
```

- **Reads** register the current recompose scope, the thread-local one set during a pass, with the scope's run (its epoch): a reader is current while its scope hasn't run again. A read searches only the cell's 8 latest readers for its scope, so a cell read by thousands of scopes (a composition local every layout node reads) doesn't make composing quadratic; a scope found further back is recorded again. An invalidation reaches every current reader once (invalidating a scope twice does nothing more), so copies change nothing, and drops the stale ones. When the readers fill their room, a compaction drops the stale ones and the copies, and makes room if more than half is still in use (D310; `RECENT_READERS`, `compact_readers`, which keeps each scope's latest record in order with a mark per scope, `Scopes::keep_reader`, in one linear pass; a test counts compactions, `platform::reader_compaction_count`): a cell's room holds less than four records per scope reading it at once (four to start with), at a constant amortized cost per read.
- **Writes** change the value at once, and their readers are invalidated (the host asked for a frame) when the global snapshot's changes are sent, at the end of the turn that wrote (below, [Snapshots](#snapshots); D751). The default policy (`T: PartialEq`) skips a write equal to the current value, as Compose's `structuralEqualityPolicy` does; `never_equal_policy()` invalidates at every write, and `referential_equality_policy()` (Compose's `===`) skips a write `same` as the current value by `ComposeArg`: an `Rc` by pointer, a small value by value. `edit` always invalidates.
- **Writes during composition** are allowed, as in Compose (D703), and invalidate at once, as Compose's composition snapshot's write observer does: a reader the pass will still reach recomposes in this pass, one it has gone past (a backward write) in the next ([04](04-runtime.md#passes-and-recomposition)). `remember_updated_state` and a composition local's provider write so. A composable writing a state it read, with a new value each time, recomposes at every frame, as in Compose.
- **`with` and `edit` borrow the value through a run-time borrow flag**, like a `RefCell`. Nested conflicting access panics with both locations. Compose has no equivalent, because the JVM shares references; in Rust the arena makes it necessary.

## State inside data

```rust
#[derive(ComposeArg)]                    // field-wise `same`: id by value, title by pointer, done by handle
struct Todo {
    id: u64,
    title: SharedString,
    done: OwnedState<bool>,              // owned by the Todo; freed when the Todo is dropped
}

let todos = remember(|| mutable_state_list_of::<Rc<Todo>>());
// Toggling writes only `done`: the list scope doesn't re-run and other rows aren't visited.
Checkbox!(checked = todo.done.get(), on_checked_change = { let done = todo.done.handle(); move |v| done.set(v) });
```

Rows receive `Rc<Todo>`. `same` is `Rc::ptr_eq`, so after an add or remove re-runs the list scope, every unchanged row **skips**. In Rust, the `Rc` *is* the identity, as an object reference is in Kotlin.

## State collections

`snapshots::SnapshotStateList<T>`, `SnapshotStateMap<K, V>` and `SnapshotStateSet<T>` are Compose's (D761), `Copy` handles with owners as `MutableState`: `let todos = remember(|| mutable_state_list_of::<Rc<Todo>>())`, `mutable_state_list_from(items)`, `snapshot_state_list(size, init)`, `mutable_state_map_of()`, `mutable_state_map_from(entries)`, `mutable_state_set_of()`, `mutable_state_set_from(items)`.

- **Reads** (`len`, `get`, `contains`, `index_of`, `with(|list| …)`, `iter_cloned`, `to_list`, `keys`, `values`, `entries`, `to_map`, `to_set`) subscribe to the whole collection, once each. `to_list`, `to_map` and `to_set` hand out the collection itself in O(1): a later write copies it, so what they gave stays as it was. A set iterates in insertion order. `Display` is Compose's `toString`, read without subscribing.
- **Writes** (`push`, `insert`, `add_all`, `insert_all`, `remove`, `remove_item`, `remove_all`, `retain`, `retain_all`, `remove_range`, `set`, `clear`, `truncate`, `swap`, `reverse`, `sort_by`, `edit(|list| …)` for anything else; on maps `insert`, `insert_all`, `remove`, `update`, `retain`, `clear`; on sets `insert`, `add_all`, `remove`, `remove_all`, `retain`, `retain_all`, `clear`) change it in place unless something shares it, at the cost of the `Vec` or `HashMap` operation, and need `T: Clone` for that copy. One leaving it as it was writes nothing, but a list's or a set's `clear` and `edit`: an equal value (`==`, as Compose's `newList == oldList`) given to a list's `set` or a map's `insert`, `insert_all` and `update` too, which need `PartialEq`; `set_always` and `insert_always` are for values that can't be compared (D766).
- **`sub_list(range)`** is a live view, with the list's operations; a structural change other than through it makes it panic, as Compose's `ConcurrentModificationException`. **`list_iterator(index)`** is Compose's `listIterator`, over what the list was when made, moving both ways and, over a whole list, writing where it is; a sublist's only reads. A sublist's `len` and a cursor's `has_next` read nothing of the list.
- **Saved:** `remember_saveable` keeps a list or set of `Saveable` elements; `list_saver` and `map_saver` are Compose's `listSaver` and `mapSaver` (D762).

## Flows

`snapshot_flow(|| …)` (`compose_runtime::flow`, as `snapshotFlow`) is a `futures_core::Stream` of what the block gives: its result at the first poll, then each different result after a change applied to what it read, coalescing the changes between polls; `flow::next(&mut stream).await` takes an item (D768). `snapshot_flow_with(&manager, …)` shares a `SnapshotFlowManager`'s apply observer. `collect_as_state(key, initial, || stream)` is state following a stream, any library's, as `Flow.collectAsState(initial)`: `key` says when it's another stream, as Rust's have no identity. `flow::MutableStateFlow` is kotlinx's, a value background threads set, and `state_flow.collect_as_state()` follows it.

## Safety: stale handles and rules R1–R3

A handle can outlive its cell: a handler captures `done`, then the `Todo` is removed. In Rust that's a **detected** condition, never undefined behaviour, and three rules make it rare in UI code:

| Rule | In Rust |
|---|---|
| **R1** release order | Nodes and their handlers are released before the state their group owns. Kept in the runtime's end-of-pass order, and tested. |
| **R2** deferred free | Dropping an `OwnedState` retires its cell, and the cell is freed at the end of the next pass on the thread. By then, every row that received the removed `Rc<Todo>` has recomposed or left, because the list scope re-ran, and its handlers were replaced. Code later in the same handler still sees a live, detached cell. Cells retired while a pass applies its cleanups and effects, or while a composition is disposed, wait for the pass after that. With no composition on the thread, there is no pass to wait for, and the cell is freed at once; dropping the last composition frees every retired cell. "The next pass" is the one that runs after the handler: a host whose frame requester recomposes synchronously runs it at the handler's turn's send (D53, D751), after the handler. |
| **R3** where captures may go | Not enforceable at compile time in Rust, since closures can go anywhere `'static`. The fallback is `try_get`/`try_set` for code that keeps handles outside the UI tree, and a panic naming the cell's creation site otherwise. Capturing an `Rc<Todo>` instead of a handle keeps the data alive, so it can never be stale. That's the recommended pattern for long-lived callbacks. |

**Reference cycles.** A closure that captures an `Rc<Todo>` and is stored inside that same `Todo`'s state would leak. That's the usual `Rc` caveat. Handles never form cycles, because they're plain indices.

## State holders

Compose's state holder classes (`DrawerState`, `SheetState`, `SnackbarHostState`, `TopAppBarState`, `SwipeToDismissBoxState`, `LazyListState`, `LazyGridState`, `ScrollState`, `CoroutineScope`) are `Copy` handles too, as `MutableState` is (D240), so closures and async blocks capture them without `.clone()`:

```rust
/// A Copy handle to an object in the same arena as state cells (not observable itself).
pub struct ObjectHandle<T: 'static> { … }       // get() -> Option<Rc<T>>; try_get(); is_alive()

/// Owns an object, as OwnedState owns a cell, and derefs to its handle H.
pub struct Owned<H: Copy + 'static> { … }
impl<H: Copy> Owned<H> {
    pub fn new<T: 'static>(object: T, handle: impl FnOnce(ObjectHandle<T>) -> H) -> Owned<H>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DrawerState { object: ObjectHandle<DrawerInner> }
impl DrawerState { pub fn new(initial_value: DrawerValue) -> Owned<DrawerState>; … }
```

- **`remember(|| DrawerState::new(…))` returns the handle**, and the slot keeps the `Owned` (`Remember for Owned<H>`), as for `OwnedState`; `remember_drawer_state` and the other `remember_*` functions do that. `remember_saveable_with` takes a saver of `Owned<H>` (`LazyListState::saver()`; `RememberSaveable::with_value` lends the stored value instead of cloning it).
- **Freed as a state is, but stale handles don't panic.** Forgotten, the object is retired and freed at the end of the next pass (R2). A handle used after that does nothing: its writes, scrolls and animations (`dispatch_raw_delta`, `open`, `show`, `snap_to`) return at once, its reads give the defaults (closed, hidden, item 0), and `CoroutineScope::launch` starts nothing, as Kotlin's cancelled scope. Debug builds print one line per call site naming the object's type and where it was made. In Kotlin a holder lives while anything references it, so a fling or a settle that outlives its screen (a host disposed mid-drag, a state swapped by `remember(key)`) just runs on unseen; a crash there would be a port's artefact. A holder's async methods take the object's `Rc` when they start, so a running animation still ends if the owner goes meanwhile. A scroll node stops its fling when it's given another state. Plain `MutableState` handles keep R3's panic: a stale state is a bug in the app's model (data dropped while a handler still writes it), which `try_get`/`try_set` handle explicitly.
- **Outside a composition** (tests, an app's model), `DrawerState::new` returns the owner, which derefs to the handle: keep it, and copy the handle out (`*state`) for closures. `CoroutineScope::new().launch(…)` drops its owner at once, which doesn't cancel the task.
- **The footgun: `let state = *DrawerState::new(…);`** compiles but drops the owner at the end of the statement, so the handle is stale from the start (and silently does nothing, with a debug-build warning). Keep the owner (`let state = DrawerState::new(…);`), or `remember` it.
- **A holder of states only** (`ScrollState`) keeps its states' handles in the handle itself, so reading it costs a state read (`try_get`, falling back to the default); others look their object up in the arena (an index, a generation check, an `Rc` clone: a few nanoseconds).
- **`FocusRequester`** owns nothing: it's a `Copy` (and `!Send`) id, and the nodes it's attached to are kept by the thread's registry while attached (D240).

## Derived state

`derived_state_of(|| …)` is a state computed from other state, as Compose's `derivedStateOf`: `let empty = remember(move || derived_state_of(move || items.with(Vec::is_empty)));`. `remember` keeps the `DerivedState` owner in the slot and returns its `Copy` `State<T>` handle. It's lazy, as Compose's (D711): nothing is computed before the first read (its result cell holds no value until then), and a read after what it read changed recomputes it first, so no read is behind what it derives from. Its calculation runs under a read observer of its own (the one layout and drawing use), so the state it reads, even during composition, registers with the derived state and not with the composing scope.

A derived state is a state object of its own, whose record holds the result with what the calculation read and the versions it saw (D759). Nothing happens at a write. A read validates the record: valid if no state was written since it was last found so, or if what it read has the versions it saw (derived states among them brought up to date first); if not, it recomputes, so no read is behind what it derives from, whatever an apply observer writes during a calculation's send (D765). Every calculation writes a new record, so what reads the state is computed again, as Compose's `derivedStateOf(calculation)`; with `derived_state_of_with(policy, …)`, a result the policy finds equivalent keeps its version and stops the chain (D764). No record is held across user code (D763): a `with` closure can write what the state read and read it again, its argument staying as it was. A recomputation that changes the result invalidates what read the old one. A calculation reading itself panics. At the send of a state it read:

- **Composables reading it** are invalid on the condition that its value changed (Compose's conditional invalidation, a list of derived states on the scope): the pass's walk reaching one validates them and restarts it only if one changed by its policy (`==` by default, its policy otherwise). A scope reached through its parent's body runs without the check, as Compose's. So a screen reading a list's emptiness recomposes when the list becomes empty, not on every edit, and a derived state no composable reads any more (its reader left in the same pass) isn't computed. `has_invalidations` is true for a conditional invalidation, as Compose's: the pass may find nothing to run.
- **Layout or drawing reading it** (a `ReadObserver` that isn't a derived state's): it's validated in the send, which tells the observer if it changed.
- **Nobody reading it:** nothing is computed until its next read.

## Snapshots

Compose's snapshot system is being ported in steps (docs/parity/design/snapshots.md, D750). Each thread has its own snapshot world, as it has its own state (D14): no locks. A state object's cell keeps its value in **records**, as Compose's `StateRecord` chain (`crates/compose-runtime/src/snapshots/records.rs`): one inline record, the head, holding the snapshot id that wrote it, a *version* (a per-thread write counter: what derived states validate against) and the value; older records come only while snapshots need them. A state's identity is a `snapshots::StateObjectId`, which all its handles convert into (`state_object_id()`, `From`): what observers are told. `platform::state_records(state)` gives the record count and version, for tests. Isolation is below; writes outside a snapshot are the global snapshot's, and with no snapshot open a read and a write look at nothing but a flag (D774).

**The global snapshot and apply notifications** (D751), as Compose's. A write that makes a new record in the global snapshot (the state's readable record was written in an earlier one) records the state in the global snapshot's *modified* list; one made in the snapshot that made the state isn't a change (Compose's `overwritableRecord`), so `Snapshot::notify_objects_initialized()` advances the snapshot, as Compose's tests call it, and sends what changed, as Compose's `advanceGlobalSnapshot` (D756). While only the global snapshot exists the head is overwritten in place (D752). `Snapshot::send_apply_notifications()` advances the global snapshot, invalidates what read the modified states (composables, layout and drawing observations, through the cells' reader lists) and then calls the apply observers (`Snapshot::register_apply_observer(|changes: &ChangeSet, snapshot| …)`, which hear only later changes, as registering sends first), outside the runtime's borrow. compose-rs has no main dispatcher to post Compose's next-turn send, so **turns send at their end** (`platform::Turn`): the executor's `run_ready` (and so `dispatch_frame`; `fire_timers` only wakes tasks, for the `run_ready` that follows), `UiHost`'s frame, its prefetch and drawing ahead, and its event and configuration entry points; `Composition::set_content`, `recompose`, `has_invalidations` and a paused composition's resume send first, and so do `UiHost::has_pending_work`, the layout's rounds, the drawing and prefetch before they apply what was observed. A turn's send repeats while what it ran wrote (a few times at most). A write outside any turn (Android's `setProp`, a handler a platform calls itself) wakes the host (its wake handler), and the tick that wakes it sends: `run_ready`, with no task to run, takes a turn if changes wait (D755),. Writes during composition invalidate at once (D703) and reach the apply observers at the next send; derived states validate themselves, their readers told at the send (above). A pass advances the global snapshot when it starts, standing for Compose's composition snapshot until step S6 (what it writes of what was made before is a change, D756), when it composed, as applying that snapshot does, and again after its end-of-pass work, and a turn's end advances it too, as `Recomposer`'s `notifyObjectsInitialized` closes a frame (D754): a state made by an effect or a task, read by layout or drawing, is changed by its next write, even if nothing else changed in between (a state written where it was made isn't a change, as in Compose).

**Observers.** `Snapshot::observe(read, write, block)` calls `read` with each state `block` reads and `write` with each it writes (`Rc<dyn Fn(StateObjectId)>`), in addition to the observers around it (innermost first), isolating nothing. `Snapshot::without_read_observation(block)` hides `block`'s reads from composition, from observations and read observers started outside it (one started inside it records): a barrier frame. `Snapshot::register_global_write_observer` hears each global write at once; `is_apply_observer_notification_pending()` is true while observers are called. `ObserverHandle::dispose` or dropping the handle unregisters. **`SnapshotStateObserver`** (D753) observes blocks' reads by scope (`observe_reads(scope, &callback, block)`, callbacks grouped by `Rc` identity, `clear`, `clear_if`, `clear_all`, `start`, `stop`, `notify_changes`) on the runtime's keyed `ReadObserver` (one per callback, scopes interned to keys): started, it's an apply observer as others are, in registration order (D757): the send told it the keys whose reads changed, a derived state's read among them if its value did, and it calls each scope's callback once through its executor (D760). Not started, `notify_changes` maps what it's given only (D767). A `MutableState`'s `Display` is Compose's `toString`, read without observing.

**Isolated snapshots** (D769), Compose's algorithms on one thread. `Snapshot::take_snapshot(read_observer)` is a read-only view of the state as it is; `Snapshot::take_mutable_snapshot(read, write)` is a view whose writes stay in it until `apply()`. `snapshot.enter(|| …)` runs code in it; `take_nested_snapshot`, `take_nested_mutable_snapshot` nest (a nested mutable snapshot applies to its parent), and `Snapshot::with_mutable_snapshot(|| …)` is take, enter, apply and `check()`. `Snapshot::global(|| …)` runs code in the global snapshot from anywhere; `Snapshot::with_read_only(|| …)` refuses writes; `enter_future` enters a snapshot around each poll of a future, as `asContextElement`. What's taken is an owner: dropping it disposes it, and a mutable snapshot disposed unapplied is abandoned, what it wrote gone. `Snapshot::current()`, `handle()` and an apply observer's argument are handles that don't own.

```rust
let snapshot = Snapshot::take_mutable_snapshot(None, None);
snapshot.enter(|| count.set(1));
assert_eq!(count.get(), 0);       // outside it, nothing changed yet
snapshot.apply().check();         // panics with SnapshotApplyConflict if it fails
assert_eq!(count.get(), 1);
```

A state read in a snapshot goes through the record with the highest id it can see (`View`, `Records::index_in`); nothing it can see means the state was made after it, which panics with Compose's message. A state made in a snapshot is readable everywhere, with its first value (D772). A write makes a record of the snapshot's, keeping the others while an open snapshot can read them (pins, `Records::reclaim`, D774); `set` needs nothing of the value, `edit` clones it the first time (`T: Clone`, D779). An apply decides each state it wrote against what was written meanwhile, as `innerApplyLocked`: the policy finding them equivalent drops the snapshot's write (D773), its `merge` makes a new value, and failing that the apply returns `SnapshotApplyResult::Failure(snapshot)`, the snapshot still open and unchanged. A snapshot's writes invalidate nothing until it's applied; then what read those states is invalidated and the apply observers hear them, after the global snapshot's pending changes (D777). Taking a snapshot of the global one sends what's pending, as Compose's does. Derived states keep a record per snapshot that computed them (D776). Apps' own state objects are `snapshot_state_object(record)`, on a `StateRecord` saying how to copy and merge (D778).

**Composition in a snapshot** (D791-D795). Each pass composes in a mutable snapshot of its own, applied as composing ends, as Compose's `Recomposer.composing`: what a composable writes is seen by what composes after it in the pass and by nothing else until then (`Snapshot::global(|| state.get())` in a composable gives the value from before the pass). Its readers are invalidated at the write (D703, D712), and apply observers hear it as the pass's snapshot applies. Effects, remember observers and releases run after that, in the global snapshot. A pass that panics discards what it wrote. If something writes outside the pass what the pass wrote, its apply fails with Compose's "Unsupported concurrent change during composition" and poisons the composition. A composition recomposing in a mutable snapshot nests in it, and in a read-only one panics. It costs little: a pass allocates nothing for its snapshot, a read in composition takes a state's only record as outside one, and with nothing else having written there's nothing to decide at the apply.
