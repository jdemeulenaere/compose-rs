# 4. Runtime (`compose-runtime`)

The composition runtime: a slot table of keyed groups, a composer that generated code drives, restart scopes, and state observation. It has a safe Rust API and knows nothing about UI.

## Data model

| Concept | Representation |
|---|---|
| Composition, scope, node | generational 64-bit handles (`Handle`); misuse is a `ComposeError`, never undefined behaviour |
| Group | an arena tree of keyed groups (restart, replace, movable, node) |
| Slot | `Slot::Value(Box<dyn Any>)` with its `TypeId`, plus `Slot::Word(u64)` for small `Copy` data, so `remember(|| 0)` doesn't box |
| Restart function | `fn(&dyn Any)` plus the kept-argument slot: see [03](03-composable-macro.md) |
| Composable lambda | `Callback<A>` = `Rc<dyn Fn(A)>`, compared with `Rc::ptr_eq` |
| Applier | a trait over a node tree, generic over the node type `N` |

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
    pub fn update_kept<K: 'static + Clone>(&self, new: K, changed: impl Fn(&K, &K) -> bool) -> bool;
    pub fn current_scope(&self) -> RecomposeScope;
}

pub fn current() -> Composer;   // thread-local; panics outside a pass with a clear message
```

Generated code, and the few hand-written primitives such as `remember` and `key`, are the only users of `Composer`. Applications never see it, as in Compose.

## Passes and recomposition

The pass algorithm:

- **Matching.** Starting a group reuses the next old sibling with the same key, scanning forward. Unvisited old children are removed when their parent group ends.
- **Deferred removals.** Removals happen at the end of the pass, in a fixed release order: nodes and their handlers before the state of the group that owned them (rule R1 of [05](05-state.md)).
- **Restart scopes.** Invalidated scopes recompose in tree order. A scope inside a removed group never runs, and none runs twice in one pass. A child scope that recomposes alone below a skipped parent must match its existing group, not recreate it and leak the old one. These are subtle, so a model-checked randomized test (random insertions, removals and keyed moves at every position, checked against a reference model) guards them.
- **Concurrency.** One pass at a time per thread. A scope invalidated during a pass is recomposed by the next one.
- **Steady state.** A steady-state pass allocates nothing. A test with a counting `#[global_allocator]` checks it.

## `remember` and friends

These are written as ordinary Rust on top of the composer, with no compiler support, as in Compose:

```rust
pub fn remember<T: Remember>(calc: impl FnOnce() -> T) -> T::Output;
pub fn remember_keyed<K: ComposeArg, T: Remember>(keys: K, calc: impl FnOnce() -> T) -> T::Output;
pub fn key<K: ComposeArg + Hash>(key: K, content: impl FnOnce());
```

`remember` returns `T::Output`, through a small trait:

| Remembered `T` | What the slot keeps | What the call returns |
|---|---|---|
| `Copy` or cheap `Clone` values | the value | a clone of it |
| `OwnedState<V>` (from `mutable_state_of`) | the owned state, dropped when the group leaves | its `Copy` handle `MutableState<V>` |
| `Rc<V>` | the `Rc` | a clone of the `Rc` |

So `let count = remember(|| mutable_state_of(0));` gives a `Copy` `MutableState<i32>`, exactly as `val count = remember { mutableStateOf(0) }` gives a state object in Kotlin. The slot owns the state, and leaving the group frees it, after the nodes and their handlers are released.

**Remember observers.** A remembered value implementing `RememberObserver` (`on_remembered`, `on_forgotten`, `on_abandoned`) gets Compose's callbacks. `Drop` covers the common case, so most types need nothing.

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
```

`run_app` in `compose-desktop`, `ComposeRsView` in `compose-android` and `ComposeTestRule` in `compose-test` own a composition each. Applications rarely touch `Composition` directly.

## Errors

Protocol misuse is a `ComposeError` value internally, and a panic with a clear message at the public boundary: unbalanced groups, a slot outside a group, a composer used off its thread, a disposed composition. Misuse comes from generated code or hand-written primitives, never from correct user code, so a panic is the right signal. Debug builds also validate every handle, and release builds keep the O(1) generation checks.
