# 4. Runtime (`compose-runtime`)

The composition runtime: a slot table of keyed groups, a composer that generated code drives, restart scopes, and state observation. It has a safe Rust API and knows nothing about UI.

## Data model

| Concept | Representation |
|---|---|
| Composition, scope, state cell | generational handles (an index plus a generation, 64 bits) into per-thread arenas; misuse is a `ComposeError`, never undefined behaviour |
| Group | an arena tree of keyed groups (restart, replace, movable, reusable, node), children in a linked list |
| Slot | an `Rc<dyn Any>`, which carries its `TypeId`; remember observers are a second variant. Allocated when a value is first remembered, never in steady state |
| Restart function | `fn(&dyn Any)` plus the group's kept arguments (an `Rc<dyn Any>` updated in place): see [03](03-composable-macro.md) |
| Composable lambda | `Callback<A>` = `Rc<dyn Fn(A)>`, compared with `Rc::ptr_eq` |
| Applier | a trait over a node tree with an associated node type; the runtime keeps `NodeRef` handles the applier assigns |

**Typed slots.** Each value slot records its `TypeId`. Reading a slot with the wrong type is a `ComposeError`, not a panic in release: **keys are never trusted for memory safety**, so a key collision or a mismatched group can only recreate state, never misread it.

## The composer

```rust
pub struct Composer { /* index of the pass on this thread */ }

impl Composer {
    pub fn start_restart_group(&self, key: u64);
    pub fn start_replace_group(&self, key: u64);
    pub fn start_movable_group(&self, key: u64, data_key: u64);
    pub fn start_reusable_group(&self, key: u64, data_key: u64);   // ReusableContent's
    pub fn deactivate_to_end_group(&self, changed: bool);          // ReusableContentHost's
    pub fn start_node<N>(&self, key: u64, factory: impl FnOnce() -> N) -> NodeRef;
    pub fn start_reusable_node<N>(&self, key: u64, factory: impl FnOnce() -> N) -> NodeRef;
    pub fn end_group(&self);
    pub fn end_restart_group(&self, restart: fn(&dyn Any));
    pub fn skipping(&self) -> bool;
    pub fn skip_to_group_end(&self);
    /// `changed` compares the kept arguments with the new ones; `make` builds the new kept
    /// value only when they differ, so a steady-state call clones nothing.
    pub fn update_kept<K: 'static>(&self, changed: impl FnOnce(&K) -> bool, make: impl FnOnce() -> K) -> bool;
    /// `update_kept`'s steps, which generated code writes inline (`get`, `set`, `put`): a
    /// composable then instantiates no closures (D200).
    pub fn take_kept(&self) -> KeptArguments;
    pub fn current_scope(&self) -> RecomposeScope;
    pub fn with_applier<A: Applier, R>(&self, f: impl FnOnce(&mut A) -> R) -> R;
    pub fn record_side_effect(&self, effect: impl FnOnce() + 'static);
}

pub fn current() -> Composer;   // thread-local; panics outside a pass with a clear message
```

Generated code, and the few hand-written primitives such as `remember` and `key`, are the only users of `Composer`. Applications never see it, as in Compose: it and `current()` live in `compose_runtime::platform` (D260). A `Composer` is a handle to its thread's runtime, valid in any pass on that thread, so a content lambda may capture it and run again in a later pass.

## Passes and recomposition

The pass algorithm:

- **Matching.** Starting a group reuses the first old sibling not yet matched with the same kind, key and data key, scanning forward from the current position, and moves it (and its nodes) to that position if needed. Unvisited old children are removed when their parent group ends. The scan is linear: reversing 1,000 keyed items costs about 1.4 ms; an index of the pending siblings can come later if profiles ask for it.
- **Node changes.** Insertions and moves are applied to the applier at once, top-down, as the pass reaches them; removed nodes are detached at once and **released** at the end of the pass. Adjacent moves are coalesced, as Compose's `recordMoveNode` does, so removing the first of many keyed items costs one move. The applier is part of composition while the pass runs: it may read state but not write it.
- **Deferred removals.** The rest of a removal happens at the end of the pass, in a fixed order: deactivated nodes (reusable content, below) and released nodes and their handlers first, then the removed groups' values (remember observers get `on_forgotten`, in reverse order of remembering), then new observers' `on_remembered`, then side effects (rule R1 of [05](05-state.md)).
- **Restart scopes.** Invalidating a scope marks its ancestor groups dirty. A pass walks from the root: a skipped group descends into its dirty children only, and restarts the invalid scopes it meets with their kept arguments. So invalidated scopes recompose in tree order, a scope inside a removed group never runs, and none runs twice in one pass. A restarted scope matches its existing group, so a child scope that recomposes alone below a skipped parent reuses its group. These are subtle, so a model-checked randomized test (random insertions, removals, keyed and unkeyed moves at every position, value writes, branch toggles and replaced arguments, each pass checked against a reference model) guards them.
- **Observation.** A read records the reading scope and the scope's *epoch*, bumped when its body is known to run (`skipping()` says no, or `update_kept` reports a change), never just because the group started: a scope that may still skip keeps its reads current. A write invalidates only the scopes whose latest run read the cell: a scope that stopped reading a state isn't recomposed by it, and a skipped scope keeps its reads.
- **Concurrency.** One pass at a time per thread. A scope invalidated during a pass is recomposed by the next one.
- **Steady state.** A steady-state pass allocates nothing: a recomposition with no structural change, including kept arguments that change (updated in place) and `SideEffect`s (their closure waits in a remembered slot). A test with a counting `#[global_allocator]` checks it.
- **End of the pass.** The pass is over before its deferred work runs: releases, `on_forgotten`, drops, `on_remembered` and side effects may compose other compositions (a popup composed from an effect). Frame requests made while a pass composes or applies its deferred work, or while a composition is disposed, are delivered when that work ends, so a requester may recompose at once, and, as in Compose, all of a pass's effects run before the next pass of its composition. Cells retired meanwhile wait too: a pass started from an effect doesn't free them early.
- **Re-entrancy.** All runtime state lives in one thread-local `RefCell`, and no user code (bodies, `remember` calculations, `ComposeArg::same`, `Clone` and `Drop` of user values, appliers, effects) runs while it is borrowed: values are moved out first and dropped afterwards.

## `remember` and friends

These are written as ordinary Rust on top of the composer, with no compiler support, as in Compose:

```rust
pub fn remember<T: Remember>(calc: impl FnOnce() -> T) -> T::Output;
pub fn remember_keyed<K: ComposeArg, T: Remember>(keys: K, calc: impl FnOnce() -> T) -> T::Output;
pub fn remember_observer<T: RememberObserver>(calc: impl FnOnce() -> T) -> Observed<T>;
pub fn key<K: ComposeArg + Hash, R>(key: K, content: impl FnOnce() -> R) -> R;
```

`remember` returns `T::Output`, through a small trait:

| Remembered `T` | What the slot keeps | What the call returns |
|---|---|---|
| `Copy` or cheap `Clone` values | the value | a clone of it |
| `OwnedState<V>` (from `mutable_state_of`) | the owned state, dropped when the group leaves | its `Copy` handle `MutableState<V>` |
| `Rc<V>` | the `Rc` | a clone of the `Rc` |

So `let count = remember(|| mutable_state_of(0));` gives a `Copy` `MutableState<i32>`, exactly as `val count = remember { mutableStateOf(0) }` gives a state object in Kotlin. The slot owns the state, and leaving the group frees it, after the nodes and their handlers are released.

**Remember observers.** A value remembered with `remember_observer` implements `RememberObserver` (`on_remembered`, `on_forgotten`, `on_abandoned`) and gets Compose's callbacks: exactly one of `on_forgotten` (after `on_remembered`) or `on_abandoned` (its pass never completed). Plain `remember` can't detect the trait without specialization, hence the separate function. `Drop` covers the common case, so most types need nothing.

## Reusable content

As Compose's `ReusableContent(key) { … }` and `ReusableContentHost(active) { … }` (D430), what lazy lists reuse items with:

- **Reusing.** A reusable group (`start_reusable_group(key, data_key)`) matches by its key alone. Met with another data key, its content is *reused* until the group ends, as Compose's `startReusableGroup`: its slots read as empty, so every `remember` computes anew and the value it replaces is forgotten at the end of the pass (an observer's `on_forgotten`, an effect disposed, a `LaunchedEffect` cancelled and launched again); every body runs (`skipping()` says no); groups inside match as usual (a `key(…)` that changed gets new content); a reusable node (`start_reusable_node`, as `ReusableComposeNode`: `Layout`'s) is kept, the applier resetting it first (`Applier::reuse`), and the pass updates it; other nodes (`start_node`, as `ComposeNode`) are replaced. `ReusableContent(key, content)` is that group around `content`; `SaveableStateProvider` is one, as Compose's.
- **Deactivating.** `ReusableContentHost(active, content)` is a reusable group whose data key is `active`. Inactive, its content isn't run but kept: `deactivate_to_end_group` empties the slots of its remember observers (forgotten at the end of the pass; plain values stay until reused or removed), frees its recompose scopes (a state it read recomposes nothing, and a scope invalid then never runs), deactivates its nodes at the end of the pass (`Applier::deactivate`, before releases: they stay in the tree) and skips the group. Active again, its data key changed: its content is reused as above. Removed while deactivated, it's disposed as any content, its observers not forgotten twice.

The model-checked randomized test covers them: items whose content is reusable, deactivated, activated again and given new reuse keys, under nodes that are reusable and nodes that aren't.

## Effects

Effects are written in Rust on remember observers, as in Compose:

- `SideEffect(|| …)` runs after a successful pass.
- `DisposableEffect(keys, |scope| { …; scope.on_dispose(|| …) })` gets the cleanup when the keys change or the group leaves.
- `LaunchedEffect(keys, async move { … })` runs a future on the UI thread's executor after the pass, and cancels it (drops the future) when the keys change or the call leaves the composition. A future given on a pass whose keys are the same is dropped unpolled.
- `produce_state(initial, key, |state| async move { … })` (as `produceState`) is state a coroutine sets, launched as `LaunchedEffect`; `remember_updated_state(value)` (as `rememberUpdatedState`) keeps the latest value for effects started earlier (a plain cell updated at each composition, not state, since composition doesn't write state).
- `remember_coroutine_scope()` is a `CoroutineScope` for handlers: `scope.launch(async move { … })` returns a `Job` (`cancel`, `is_active`), and the scope cancels its jobs when the call leaves the composition. The scope is a `Copy` handle owned by the slot ([05](05-state.md#state-holders), D240): freed a pass after it leaves, it stays a cancelled scope (launching in it starts nothing). `CoroutineScope::new()` returns an owner (`Owned<CoroutineScope>`) for a scope of one's own.

## Tasks and the frame clock

The runtime's executor is the UI thread's (apps see `CoroutineScope`, `Job`, `delay`, `with_frame_nanos`; hosts drive it through `compose_runtime::platform`: `run_ready`, `fire_timers`, `dispatch_frame`, `set_wake_handler`, D260), as Compose's main dispatcher and `MonotonicFrameClock`. Tasks are futures polled on that thread, never during composition. Their wakers are `Send`, so a background thread completing work (a channel, a network client) wakes the task, which then writes state on the UI thread. `delay(duration)` waits by the executor's clock. `with_frame_nanos(|time| …)` waits for the next frame and runs with its time, before that frame recomposes, so an animation's state is the frame's. Hosts drive it: each frame fires due timers, polls ready tasks, resumes frame awaiters, then recomposes, lays out and draws, then polls the tasks the pass launched. A task woken again after its poll in a turn (one that yields: pending after waking itself) is polled at the next turn, so a yielding loop can't hold the thread and frames come between its steps, as with Compose's `yield()`. What the tasks write before the pass is that frame's: it asks for no other frame, so an animation runs one step per frame. `UiHost::set_content` composes at the next frame, as `setContent`. A cancelled task's `delay` and frame wait are removed with it. `set_wake_handler` (called from any thread) asks the host for a turn of its loop, and `next_timer` says when the next `delay` ends. The tasks `dispatch_frame` resumes don't call it: it polls them itself (a flag of that executor's, set by a guard around the wakes, so another thread's executor and a waker's panic are unaffected: D321). Nor do the frame awaiters after the first since the last frame: the host runs frames while `has_frame_awaiters`, so one wake asks for the frame they all wait for (on Android each wake is a JNI upcall; an animation frame had one per animated value, D421). Resuming a frame awaiter costs a poll, with no scan of the others: a task's waker is made once and cloned for each poll, a turn tracks the tasks it polled in a set, and a resumed `with_frame_nanos` has nothing to remove when dropped (568 awaiters: 169 µs a frame on an M-series Mac before, 21 µs after, D421). The clock is real (`Instant`) or manual, for tests ([09](09-testing.md)).

**Background work**, as `withContext(Dispatchers.IO) { … }`: `spawn_background(|| work())` runs a `Send` closure on a small pool of std threads (created as work comes, at most 64 as `Dispatchers.IO`, each gone after a minute idle) and returns a future (`Background<T>`) the UI thread's task awaits; the closure's thread stores the result and wakes the task through its `Send` waker, so it resumes on the UI thread and writes state there: `LaunchedEffect(path.clone(), async move { let text = spawn_background(move || std::fs::read_to_string(path)).await; contents.set(text.unwrap_or_default()); })`. Dropping the future (its task cancelled) never blocks: work not started yet never starts, and the result of work already running is dropped on its thread when it ends (blocking work can't be interrupted, as Compose's `withContext` can't cancel a blocking call). A panic in the work panics the awaiting task; one escaping it on a pool thread (a discarded result's drop) is reported and the thread goes on. On Android, whose library builds with `panic = "abort"` (D361), any of these panics aborts the process instead, after it's logged. Polling the future again after it completed panics, as std's futures do. A tokio-based client needs its runtime: give the runtime its own thread and await its results, `spawn_background(move || handle.block_on(request))` (holding a pool thread while it runs) or a one-shot channel the runtime's task sends on (a future woken from that thread, holding none) (D231).

## Compositions

```rust
let composition = Composition::new(applier);   // owned; Drop disposes it and releases everything
composition.set_content(|| App());
composition.recompose();                        // runs a pass if any scope is invalid
composition.dump();                             // a text dump of groups and slots, for tests
composition.set_frame_requester(|| …);          // called when a scope becomes invalid
```

The content is held by a restart group like any composable's, with the content lambda as its kept argument, so `set_content` with new content recomposes against the old one.

**Disposal.** The thread's runtime has no destructor, so it stays usable from every other thread-local's destructor, whatever their order. At thread exit, compositions still alive are emptied while everything works, and closed: a handle still owned (by a thread-local) stays usable as an empty composition until its owner drops it. Once nothing is left (no composition, cell or scope), the runtime replaces itself by an empty one, so a thread that dropped everything leaks nothing; the new arenas continue the old generations, so stale handles still read as freed. What a thread leaves owned by other thread-locals at exit stays allocated rather than dropped where using state would abort. Dropping a composition releases its nodes, then its values; state cells those values retire stay alive until the disposal ends, so values of one group may rely on each other in `Drop`.

**Panics.** A panic that unwinds out of a pass poisons the composition: the slot table can't be trusted to continue, so further passes panic, and dropping it still releases everything exactly once (observers remembered by the failed pass get `on_abandoned`).

`application` in `compose-desktop` (one composition for its windows, and one per window), `ComposeRsView` in `compose-android` (planned) and `ComposeTestRule` in `compose-test` own a composition each. Applications rarely touch `Composition` directly.

## Errors

Protocol misuse is a `ComposeError` value internally, and a panic with a clear message at the public boundary: unbalanced groups, a slot outside a group, a composer used off its thread, a disposed composition. Misuse comes from generated code or hand-written primitives, never from correct user code, so a panic is the right signal. Debug builds also validate every handle, and release builds keep the O(1) generation checks.

**Known limitations.** Generations are 32-bit: after 2^31 reuses of one arena slot, a stale handle could alias a new cell of the same type, which is memory-safe (a wrong value, never undefined behaviour). `key(…)` groups have no call-site key, so two `key` call sites in one parent with equal values can match each other's groups (state recreated or shared, never misread). A scope that invalidates itself during the pass that then runs it leaves its ancestors dirty, costing one extra empty pass.
