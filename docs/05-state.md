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
```

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

- **Reads** register the current recompose scope, the thread-local one set during a pass. Readers are deduplicated, and stale readers are pruned when their scope is gone.
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

let todos = remember(|| mutable_state_of(Vec::<Rc<Todo>>::new()));
// Toggling writes only `done`: the list scope doesn't re-run and other rows aren't visited.
Checkbox!(checked = todo.done.get(), on_checked_change = { let done = todo.done.handle(); move |v| done.set(v) });
```

Rows receive `Rc<Todo>`. `same` is `Rc::ptr_eq`, so after an add or remove re-runs the list scope, every unchanged row **skips**. In Rust, the `Rc` *is* the identity, as an object reference is in Kotlin.

## Safety: stale handles and rules R1–R3

A handle can outlive its cell: a handler captures `done`, then the `Todo` is removed. In Rust that's a **detected** condition, never undefined behaviour, and three rules make it rare in UI code:

| Rule | In Rust |
|---|---|
| **R1** release order | Nodes and their handlers are released before the state their group owns. Kept in the runtime's end-of-pass order, and tested. |
| **R2** deferred free | Dropping an `OwnedState` retires its cell, and the cell is freed at the end of the next pass on the thread. By then, every row that received the removed `Rc<Todo>` has recomposed or left, because the list scope re-ran, and its handlers were replaced. Code later in the same handler still sees a live, detached cell. With no composition on the thread, there is no pass to wait for, and the cell is freed at once; dropping a composition frees the cells it retired, and dropping the last one frees every retired cell. |
| **R3** where captures may go | Not enforceable at compile time in Rust, since closures can go anywhere `'static`. The fallback is `try_get`/`try_set` for code that keeps handles outside the UI tree, and a panic naming the cell's creation site otherwise. Capturing an `Rc<Todo>` instead of a handle keeps the data alive, so it can never be stale. That's the recommended pattern for long-lived callbacks. |

**Reference cycles.** A closure that captures an `Rc<Todo>` and is stored inside that same `Todo`'s state would leak. That's the usual `Rc` caveat. Handles never form cycles, because they're plain indices.

## Snapshots

Observation only, for now. Reads register scopes, and writes invalidate them at once. There's no snapshot isolation, no MVCC and no concurrent composition. The cell layout reserves room for a record chain, so a later `Snapshot::take_mutable_snapshot` can add isolation without changing user code. `derived_state_of` and `snapshot_flow` come later ([10](10-roadmap.md)).
