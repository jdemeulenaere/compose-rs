# 4. Runtime (`compose-runtime`)

The composition runtime: a slot table of keyed groups, a composer that generated code drives, restart scopes, and state observation. It has a safe Rust API and knows nothing about UI.

## Data model

| Concept | Representation |
|---|---|
| Composition, scope, state cell | generational handles (an index plus a generation, 64 bits) into per-thread arenas; misuse is a `ComposeError`, never undefined behaviour |
| Group | an arena tree of keyed groups (restart, replace, movable, node), children in a linked list |
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
    pub fn start_node<N>(&self, key: u64, factory: impl FnOnce() -> N) -> NodeRef;
    pub fn end_group(&self);
    pub fn end_restart_group(&self, restart: fn(&dyn Any));
    pub fn skipping(&self) -> bool;
    pub fn skip_to_group_end(&self);
    /// `changed` compares the kept arguments with the new ones; `make` builds the new kept
    /// value only when they differ, so a steady-state call clones nothing.
    pub fn update_kept<K: 'static>(&self, changed: impl FnOnce(&K) -> bool, make: impl FnOnce() -> K) -> bool;
    pub fn current_scope(&self) -> RecomposeScope;
    pub fn with_applier<A: Applier, R>(&self, f: impl FnOnce(&mut A) -> R) -> R;
    pub fn record_side_effect(&self, effect: impl FnOnce() + 'static);
}

pub fn current() -> Composer;   // thread-local; panics outside a pass with a clear message
```

Generated code, and the few hand-written primitives such as `remember` and `key`, are the only users of `Composer`. Applications never see it, as in Compose. A `Composer` is a handle to its thread's runtime, valid in any pass on that thread, so a content lambda may capture it and run again in a later pass.

## Passes and recomposition

The pass algorithm:

- **Matching.** Starting a group reuses the first old sibling not yet matched with the same kind, key and data key, scanning forward from the current position, and moves it (and its nodes) to that position if needed. Unvisited old children are removed when their parent group ends. The scan is linear: reversing 1,000 keyed items costs about 1.4 ms; an index of the pending siblings can come later if profiles ask for it.
- **Node changes.** Insertions and moves are applied to the applier at once, top-down, as the pass reaches them; removed nodes are detached at once and **released** at the end of the pass. Adjacent moves are coalesced, as Compose's `recordMoveNode` does, so removing the first of many keyed items costs one move. The applier is part of composition while the pass runs: it may read state but not write it.
- **Deferred removals.** The rest of a removal happens at the end of the pass, in a fixed order: released nodes and their handlers first, then the removed groups' values (remember observers get `on_forgotten`, in reverse order of remembering), then new observers' `on_remembered`, then side effects (rule R1 of [05](05-state.md)).
- **Restart scopes.** Invalidating a scope marks its ancestor groups dirty. A pass walks from the root: a skipped group descends into its dirty children only, and restarts the invalid scopes it meets with their kept arguments. So invalidated scopes recompose in tree order, a scope inside a removed group never runs, and none runs twice in one pass. A restarted scope matches its existing group, so a child scope that recomposes alone below a skipped parent reuses its group. These are subtle, so a model-checked randomized test (random insertions, removals, keyed and unkeyed moves at every position, value writes, branch toggles and replaced arguments, each pass checked against a reference model) guards them.
- **Observation.** A read records the reading scope and the scope's *epoch*, bumped when its body is known to run (`skipping()` says no, or `update_kept` reports a change), never just because the group started: a scope that may still skip keeps its reads current. A write invalidates only the scopes whose latest run read the cell: a scope that stopped reading a state isn't recomposed by it, and a skipped scope keeps its reads.
- **Concurrency.** One pass at a time per thread. A scope invalidated during a pass is recomposed by the next one.
- **Steady state.** A steady-state pass allocates nothing: a recomposition with no structural change, including kept arguments that change (updated in place) and `SideEffect`s (their closure waits in a remembered slot). A test with a counting `#[global_allocator]` checks it.
- **End of the pass.** The pass is over before its deferred work runs: releases, `on_forgotten`, drops, `on_remembered` and side effects may compose other compositions (a popup composed from an effect), and a frame requester may recompose at once when an effect writes state.
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

## Effects

Effects are written in Rust on remember observers, as in Compose:

- `SideEffect(|| …)` runs after a successful pass.
- `DisposableEffect(keys, |scope| { …; scope.on_dispose(|| …) })` gets the cleanup when the keys change or the group leaves.
- `LaunchedEffect(keys, async move { … })` runs a future on the UI thread's executor and cancels it on key change or exit. It's planned for after the first milestone ([10](10-roadmap.md)). It needs an executor integrated with the frame loop.

## Compositions

```rust
let composition = Composition::new(applier);   // owned; Drop disposes it and releases everything
composition.set_content(|| App());
composition.recompose();                        // runs a pass if any scope is invalid
composition.dump();                             // a text dump of groups and slots, for tests
composition.set_frame_requester(|| …);          // called when a scope becomes invalid
```

The content is held by a restart group like any composable's, with the content lambda as its kept argument, so `set_content` with new content recomposes against the old one.

**Disposal.** Dropping a composition releases its nodes, then its values; state cells those values retire stay alive until the disposal ends, so values of one group may rely on each other in `Drop`.

**Panics.** A panic that unwinds out of a pass poisons the composition: the slot table can't be trusted to continue, so further passes panic, and dropping it still releases everything exactly once (observers remembered by the failed pass get `on_abandoned`).

`run_app` in `compose-desktop`, `ComposeRsView` in `compose-android` and `ComposeTestRule` in `compose-test` own a composition each. Applications rarely touch `Composition` directly.

## Errors

Protocol misuse is a `ComposeError` value internally, and a panic with a clear message at the public boundary: unbalanced groups, a slot outside a group, a composer used off its thread, a disposed composition. Misuse comes from generated code or hand-written primitives, never from correct user code, so a panic is the right signal. Debug builds also validate every handle, and release builds keep the O(1) generation checks.

**Known limitations.** Generations are 32-bit: after 2^31 reuses of one arena slot, a stale handle could alias a new cell of the same type, which is memory-safe (a wrong value, never undefined behaviour). `key(…)` groups have no call-site key, so two `key` call sites in one parent with equal values can match each other's groups (state recreated or shared, never misread). A scope that invalidates itself during the pass that then runs it leaves its ancestors dirty, costing one extra empty pass.
