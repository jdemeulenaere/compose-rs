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
pub fn mutable_state_list_of<T: 'static>() -> OwnedState<Vec<T>>;   // Compose's mutableStateListOf(): edits always invalidate
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

- **Reads** register the current recompose scope, the thread-local one set during a pass, with the scope's run (its epoch): a reader is current while its scope hasn't run again. A read searches only the cell's 8 latest readers for its scope, so a cell read by thousands of scopes (a composition local every layout node reads) doesn't make composing quadratic; a scope found further back is recorded again. A write invalidates every current reader once (invalidating a scope twice does nothing more), so copies change nothing, and drops the stale ones. When the readers fill their room, a compaction drops the stale ones and the copies, and makes room if more than half is still in use (D310; `RECENT_READERS`, `compact_readers`, which keeps each scope's latest record in order with a mark per scope, `Scopes::keep_reader`, in one linear pass; a test counts compactions, `platform::reader_compaction_count`): a cell's room holds less than four records per scope reading it at once (four to start with), at a constant amortized cost per read.
- **Writes** invalidate the readers and ask the host for a frame. The default policy (`T: PartialEq`) skips a write equal to the current value, as Compose's `structuralEqualityPolicy` does. `edit` always invalidates.
- **Writes during composition are a misuse.** In debug builds they panic with the location. Composition is a pure function of state. Compose allows backward writes with caveats; this design keeps the stricter rule until a use case appears.
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

## Safety: stale handles and rules R1–R3

A handle can outlive its cell: a handler captures `done`, then the `Todo` is removed. In Rust that's a **detected** condition, never undefined behaviour, and three rules make it rare in UI code:

| Rule | In Rust |
|---|---|
| **R1** release order | Nodes and their handlers are released before the state their group owns. Kept in the runtime's end-of-pass order, and tested. |
| **R2** deferred free | Dropping an `OwnedState` retires its cell, and the cell is freed at the end of the next pass on the thread. By then, every row that received the removed `Rc<Todo>` has recomposed or left, because the list scope re-ran, and its handlers were replaced. Code later in the same handler still sees a live, detached cell. Cells retired while a pass applies its cleanups and effects, or while a composition is disposed, wait for the pass after that. With no composition on the thread, there is no pass to wait for, and the cell is freed at once; dropping the last composition frees every retired cell. "The next pass" is the one that runs after the handler: a host whose frame requester recomposes synchronously runs it inside the handler, at its next state write, so code after that write sees the cell freed (D53). |
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
- **Freed as a state is, but stale handles don't panic.** Forgotten, the object is retired and freed at the end of the next pass (R2). A handle used after that does nothing: its writes, scrolls and animations (`scroll_by`, `open`, `show`, `snap_to`) return at once, its reads give the defaults (closed, hidden, item 0), and `CoroutineScope::launch` starts nothing, as Kotlin's cancelled scope. Debug builds print one line per call site naming the object's type and where it was made. In Kotlin a holder lives while anything references it, so a fling or a settle that outlives its screen (a host disposed mid-drag, a state swapped by `remember(key)`) just runs on unseen; a crash there would be a port's artefact. A holder's async methods take the object's `Rc` when they start, so a running animation still ends if the owner goes meanwhile. A scroll node stops its fling when it's given another state. Plain `MutableState` handles keep R3's panic: a stale state is a bug in the app's model (data dropped while a handler still writes it), which `try_get`/`try_set` handle explicitly.
- **Outside a composition** (tests, an app's model), `DrawerState::new` returns the owner, which derefs to the handle: keep it, and copy the handle out (`*state`) for closures. `CoroutineScope::new().launch(…)` drops its owner at once, which doesn't cancel the task.
- **The footgun: `let state = *DrawerState::new(…);`** compiles but drops the owner at the end of the statement, so the handle is stale from the start (and silently does nothing, with a debug-build warning). Keep the owner (`let state = DrawerState::new(…);`), or `remember` it.
- **A holder of states only** (`ScrollState`) keeps its states' handles in the handle itself, so reading it costs a state read (`try_get`, falling back to the default); others look their object up in the arena (an index, a generation check, an `Rc` clone: a few nanoseconds).
- **`FocusRequester`** owns nothing: it's a `Copy` (and `!Send`) id, and the nodes it's attached to are kept by the thread's registry while attached (D240).

## Derived state

`derived_state_of(|| …)` is a state computed from other state, as Compose's `derivedStateOf`: `let empty = remember(move || derived_state_of(move || items.with(Vec::is_empty)));`. `remember` keeps the `DerivedState` owner in the slot and returns its `Copy` `State<T>` handle. Its calculation runs under a read observer of its own (the one layout and drawing use), so the state it reads, even during composition, registers with the derived state and not with the composing scope. A write to one of them marks it dirty, in the write and without computing anything (a handler's writes may be half done), and so are the derived states reading it. A read of a dirty result (through `DerivedState::get` or its `State` handle) recomputes it first, so no read is behind what it derives from; each frame recomputes the dirty ones (`flush_derived_states`, from `Composition::recompose` and `has_invalidations`), invalidating their readers only when the result changed by its policy (`==` by default, `derived_state_of_with(policy, …)` otherwise): a screen reading a list's emptiness recomposes when the list becomes empty, not on every edit.

## Snapshots

Observation only, for now. Reads register scopes, and writes invalidate them at once. There's no snapshot isolation, no MVCC and no concurrent composition. The cell layout reserves room for a record chain, so a later `Snapshot::take_mutable_snapshot` can add isolation without changing user code. `snapshot_flow` comes later ([10](10-roadmap.md)).
