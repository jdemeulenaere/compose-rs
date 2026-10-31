# Parity ledger: the runtime

What compose-rs's runtime (`crates/compose-runtime`, `crates/compose-macros`) has of Jetpack Compose's runtime libraries, test file by test file and API by API, and what's missing. It drives the porting work: each row says whether Compose's tests there can be adapted now, need a feature first, or don't apply.

## Scope

- **Reference:** androidx-main at `d153d87ed76` (25 October 2026), `frameworks/support/compose/runtime/`: `runtime` (with its `integration-tests` and `compose-runtime-benchmark`), `runtime-saveable`, `runtime-retain`, `runtime-annotation` (no tests), `runtime-livedata`, `runtime-rxjava2`, `runtime-rxjava3`, `runtime-tracing`, `runtime-test-utils` (the mock applier and `compositionTest`, no tests) and the three lint modules (`runtime-lint`, `runtime-retain-lint`, `runtime-saveable-lint`). `compose/runtime/design` holds documents only.
- **Compiler tests:** `frameworks/support/compose/compiler` holds only a README at this commit: the compiler plugin moved to the Kotlin repository (`plugins/compose`), so its tests aren't in the checkout. The runtime semantics they pin (skipping, groups and keys, default arguments, composable lambdas, `remember`) are covered here through the runtime's own tests that exercise generated code: `CompositionTests`, `RestartTests`, `NewCodeGenTests`, `JvmCompositionTests`, `UpdateChangedFlagsTest`, `GroupSizeValidationTests`.
- **compose-rs side:** `crates/compose-runtime` (src and tests), `crates/compose-macros` (src, golden expansions), and the runtime tests that live elsewhere: `crates/compose/tests/suite` (composables written with `#[composable]`, locals, derived state, readers), `crates/compose-foundation/tests/suite` (effects, saveable state, locals through the UI) and `crates/compose-test`.
- **What counts as a test file:** every Kotlin file under a test source set (`commonTest`, `nonEmulatorCommonTest`, `jvmTest`, `jvmAndAndroidTest`, `nonEmulatorJvmTest`, `nativeTest`, `wasmJsTest`, `androidDeviceTest`, `androidHostTest`, `androidTest`, `src/test`) with at least one `@Test` method, counted with `grep -cE '^\s*@Test(\s|$|\()'`. 188 Kotlin files sit in those source sets; the 37 without a `@Test` (test utilities, `IgnoreTargets`, benchmark helpers, `Holder.kt`, `AwaitTestResult*`) are left out.

## Totals

**151 test files, 2,002 `@Test` methods.**

| Status | Files | Tests |
|---|---|---|
| `covered` | 5 | 27 |
| `ported` | 25 | 404 |
| `port` | 0 | 0 |
| `partial` | 18 | 454 |
| `feature:…` | 12 | 149 |
| `n/a` | 91 | 968 |

677 tests are ported so far (each cites its Compose test in a `// Compose:` comment), in `ported` and `partial` files.

The `feature:…` files by feature: computed-state 2 (54), movable-content 2 (5), error-traces 1 (33), composition-observer 4 (28), snapshot-isolation 1 (14), composition-data 2 (15). `partial` rows name further features they need for some of their tests (composition-context, composition-api, recomposer, movable-content, host-defaults, snapshot-isolation, computed-state).

Of the 968 `n/a` tests, 142 are Android Lint checks, 122 benchmarks, 513 tests of internal data structures and internal APIs (two slot tables, change lists, collections, snapshot id sets, the opt-in tracing API), 49 hot reload (Live Edit), and 142 platform or language specifics (threads, JVM reflection, `Parcelable`, RxJava, LiveData, Kotlin delegates and autoboxing, the Kotlin compiler's ABI and group sizes, Android tracing).

The core suite alone (`nonEmulatorCommonTest`, 1,099 tests) splits: 17 in `covered` files, 73 in `ported` files, 544 in `partial` ones, 155 behind a missing feature, 310 `n/a` (internal data structures).

## Test files

### `runtime`: the core suite (`nonEmulatorCommonTest`)

43 files, 1099 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/AbstractApplierTest.kt` | 10 | `AbstractApplier`'s cursor (`down`/`up`/`clear`) and its list `remove`/`move` helpers. | covered: the list semantics only, by `NodeTree`'s `moves_match_compose` and `insert_remove_release` (crates/compose-runtime/src/node_tree.rs); the cursor (`down`/`up`/`clear`) has no analogue (the applier addresses parents by `NodeRef`). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/BroadcastFrameClockTest.kt` | 4 | `BroadcastFrameClock`: frames sent to every awaiter, cancelling the clock, failing awaiters, lock-free cancellation. | ported: 3 in crates/compose-runtime/tests/suite/frame_clocks.rs (`locklessCancellation` keeps its single-threaded part); `failClockWhenNewAwaitersNotified` is n/a (a Kotlin exception thrown from the callback; frame_clocks.rs tests the callback itself). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CollectAsStateTests.kt` | 4 | `Flow.collectAsState`/`StateFlow.collectAsState`: initial value, updates, restart on a new flow, referential policy. | ported: 4 in crates/compose-runtime/tests/suite/flows.rs (step S4, D768). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndComputedStateTests.kt` | 30 | `computedStateOf` read in composition: invalidation only on a changed result, policies, dependency tracking, mixing with `derivedStateOf`. | feature:computed-state |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt` | 20 | `derivedStateOf` read in composition: invalidates only on a changed result, policies, only live derived states computed, dependencies cleared, nested derived states. | ported: 16 in crates/compose-runtime/tests/suite/composition_and_derived_state.rs (which found the eager computation, fixed: D711) and movable_content.rs; `changingTheDerivedStateInstanceShouldRelease`, `changingTheDerivedStateInstanceShouldClearDependencies`, `changingDerivedStateDependenciesShouldClearThem` and `changingDerivedStateShouldNotAccumulateConditionalScopes` count Compose's internal observation maps (n/a). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt` | 38 | Composition locals: dynamic and static locals, sub/deferred compositions, many providers at once, `providesDefault`, computed defaults, `providesComputed`, host defaults, `withCompositionLocal(s)`, change while removing. | partial: 32 ported in crates/compose-runtime/tests/suite/composition_local.rs (`testProvideAllLocals` drives its second composition by hand) and composition_context.rs (`subCompose_*`, `deferredSubCompose_*`); the 5 `hostDefault_*` need host defaults (feature host-defaults); `testTheUnderlyingPropertiesOfPersistentHashMap` tests a Kotlin collection (n/a). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionObserverTests.kt` | 17 | `Composition.setObserver`/`CompositionObserver`: begin/end of composition, scope enter/exit/invalidated/disposed, reads, nested observations, subcompositions. | feature:composition-observer |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt` | 26 | `ReusableContent`, `ReusableContentHost`, `ReusableComposition.setContentWithReuse`/`deactivate`, node `onReuse`/`onDeactivate`/`onRelease` ordering, composite hash under reuse. | partial: 25 ported in crates/compose-runtime/tests/suite/composition_reusing.rs, composition_api.rs, composition_context.rs and movable_content.rs (mock views have ids for `flatten`/`findTextWith`); `onReuseIsCalledInApplyStage` differs: a reused node is reset as the pass reaches it, since compose-rs applies nodes during the pass (D31; behaviour 13). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt` | 133 | The core: inserts, moves, replaces, skipping, `remember` with 0 to 5 keys, remember-observer order and abandonment, invalidation and observation scopes, applier begin/end, early returns, loops, default arguments, compound keys, subcompositions. | partial: 114 ported in crates/compose-runtime/tests/suite/composition.rs (Kotlin's inline wrappers are blocks or called closures there; a few models are state, as compose-rs memoizes content lambdas on their captures, D710) and composition_context.rs (the subcomposition tests). `testCompositionAndRecomposerDeadlock` is n/a: it's about a deadlock between threads (D14). (`movableContentNoopInDeactivatedComposition` in movable_content.rs.) Not ported: composition-api ones (`disposedCompositionShouldReportAsDisposed`, `setContentDeactivated`, `testRememberedObserver_Controlled_Dispose`), `recompositionClearsObservationsWhenDefaultParametersSkip` (defaults are evaluated by the caller: behaviour 4), the non-local returns from nested lambdas (5 `test_returnConditionally_*_nonLocal*`: Rust closures return locally), and n/a ones: Kotlin (`composableDelegates`, `composableVarargs_skipped`, `funInterface_isMemoized`, `testFunctionInstances`), Compose's hash values (`enumCompositeKey*ShouldBeStable`), internals (`composerCleanup`, `internalErrorsAreReportedAsInternal`), flows (`slotsAreUsedCorrectly_forEach`: flow-adapters). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt` | 12 | `currentCompositeKeyHashCode`: unique for adjacent/repeated/conditional calls, stable across recompositions and recreation, moves with keys, under reuse and subcomposition. | ported: crates/compose-runtime/tests/suite/compound_hash_key.rs and, the two with a child, composition_context.rs, on `current_composite_key_hash_code` (crates/compose-runtime/src/saveable.rs; Compose's name is `currentCompositeKeyHashCode`). Its values differ by design (per-composable keys plus an ordinal; cut at a `SaveableStateProvider`, D290): the tests check uniqueness and stability, not values. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt` | 18 | Memoization, state in composition, `DisposableEffect` commit/dispose order, composition locals read in effects, updated composition. | ported: 17 in crates/compose-runtime/tests/suite/effects.rs; `testState3` tests Kotlin property delegates (n/a). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/GroupSizeValidationTests.kt` | 5 | Exact slot-table group and slot counts of Spacer/Column/Text/Checkbox-like composables. | n/a: pins the Kotlin compiler's group layout; compose-rs's budgets are its own (crates/compose-runtime/tests/allocations.rs, `Composition::group_count` asserts). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/LatchTest.kt` | 2 | The internal `Latch` coroutine primitive. | n/a: internal helper of Compose's Recomposer. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ModelViewTests.kt` | 6 | Mock views driven by state models: recompose on change, scope cleanup, observer entering, next-frame visibility. | ported: 6 in crates/compose-runtime/tests/suite/restart.rs (with `RestartTests`). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt` | 49 | `movableContentOf` (0-4 params, receivers): moving content keeps state and nodes, between parents and subcompositions, nested, invalidation while moving or deleted, locals in moved content. | partial: 39 ported in crates/compose-runtime/tests/suite/movable_content.rs (the two `movableContentInvalidatedWhileDeleted_*` as one; `movableContentMoveZeroesOutOldWrapperSlots_linkComposer` is about Compose's link table: n/a). The 9 across compositions come with step C6. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/NewCodeGenTests.kt` | 7 | Static and updated compositions, single views and view groups, composable calls with parameters, keyed moves. | covered: in substance by crates/compose-runtime/tests/suite/scenarios.rs (`initial_composition_emits_nodes_in_order`, `unchanged_arguments_skip_and_changed_ones_recompose`, `keyed_items_keep_their_state_when_moved`) and crates/compose/tests/suite/composables.rs. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt` | 29 | `PausableComposition`: pausing, resuming, apply-only side effects and remember callbacks, reuse while paused, cancel, exceptions in resume/apply, background threads. | partial: 23 ported in crates/compose-runtime/tests/suite/composition_api.rs, whose workflows run in the test, and composition_context.rs; `Composition::recompose_pausable`/`PausedComposition` are also covered by crates/compose-runtime/tests/suite/pausable.rs (41, randomized) and crates/compose-foundation/tests/suite/prefetch_pausable.rs. Left: `pausableComposition_invalidStatePreservesOriginalException_*` (2) differ by design (a panic poisons the composition); `markInvalidFromBackgroundThread` and `resumeOnBackgroundThread` n/a (D14); `pausableComposition_diagnosticExceptionInApply` and `recordingApplier_reportsCorrectFailedOperationIndex` n/a: a node's update runs in the pass (D591), so no recording applier fails at apply. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ProduceStateTests.kt` | 6 | `produceState`: initial value, updates, restart on key change, cancelled on disposal, `awaitDispose`, referential policy. | ported: 6 in crates/compose-runtime/tests/suite/produce_state.rs (`produceState_restartsOnKeyChange` sees the restarted producer's write in the same `advance`: writes are seen at once, D16). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerFrameEndSchedulingTests.kt` | 4 | `scheduleFrameEndCallback`: after movable content settles, while idle, cancelling. | ported: crates/compose-runtime/tests/suite/recomposer_api.rs and movable_content.rs, on a recomposer's `perform_frame`. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt` | 12 | `Recomposer`: recomposes while open, stays open until effects join, cancel with pending work, state changes during apply, frame clock pausing; plus plain recomposition scenarios. | partial: 11 ported: 5 in crates/compose-runtime/tests/suite/recomposer.rs on `Composition::recompose` (`stateChangesDuringApplyChangesAreNotifiedBeforeFrameFinished`, step S6; `testInsertDuringRecomposition` both in the composition itself and, in composition_context.rs, in a child; `@NonSkippableComposable` is a `NeverEqual` argument; `readDuringWithoutReadObservation…` on `Snapshot::without_read_observation`), 5 in recomposer_api.rs on the executor's frames, `testSubcompositionDisposedInParent` in composition_context.rs; `validatePotentialDeadlock` n/a (D14). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RestartTests.kt` | 6 | Restarting composables and lambdas that read model state, with and without parameters; restart and skip; deleting a state's reader. | ported: 6 in crates/compose-runtime/tests/suite/restart.rs (content lambdas have no restart scope of their own here: the composable invoking them restarts, which these tests don't observe). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt` | 9 | `snapshotFlow`: emits on changes, coalesces, ignores unrelated changes, nested derived state, `SnapshotFlowManager` sharing and cancellation. | ported: 9 in crates/compose-runtime/tests/suite/flows.rs (step S4, D768; `withMutableSnapshot`'s writes plain writes and sends until S5). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/UpdateChangedFlagsTest.kt` | 6 | The compiler's `$changed` bit flags (`updateChangedFlags`). | n/a: Kotlin compiler ABI; compose-rs compares kept arguments with `ComposeArg::same`. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/collection/BitVectorTests.kt` | 6 | Internal `BitVector`. | n/a: internal collection. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/collection/MutableVectorTest.kt` | 104 | Internal `MutableVector` (a Kotlin collection type). | n/a: internal collection; `Vec` in Rust. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/collection/ScopeMapTest.kt` | 7 | Internal `ScopeMap`. | n/a: internal collection. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/composer/gapbuffer/SlotTableTests.kt` | 138 | The gap-buffer slot table: readers, writers, anchors, moves, marks, boundary conditions. | n/a: internal data structure; compose-rs's arena slot table is checked by `Composition::validate` and the randomized model (crates/compose-runtime/tests/suite/model.rs). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/ComputedSnapshotStateTests.kt` | 24 | `computedStateOf` outside composition: recomputed per read, policies, mixing with derived state, observation, writes in the calculation throw. | feature:computed-state (5 tests also need snapshot-isolation). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt` | 12 | `derivedStateOf` outside composition: caching, observation, null results, nested derived states, inside snapshots. | ported: 12 in crates/compose-runtime/tests/suite/derived_snapshot_state.rs (the snapshots' ones in step S5, D776; the 2 `stateReadsCanBeObserved…` both on `Snapshot::observe` and on a snapshot). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/PrimitiveSnapshotStateTests.kt` | 12 | `mutableIntStateOf`/`Long`/`Float`/`Double`: creation, read, write. | n/a: Kotlin autoboxing; `MutableState<i32>` is unboxed in Rust. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotContextElementTests.kt` | 2 | `Snapshot.asContextElement` entering a snapshot in a coroutine. | ported: 2 in crates/compose-runtime/tests/suite/snapshot_isolation.rs, on `Snapshot::enter_future` (D769). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotDoubleIndexHeapTests.kt` | 3 | Internal heap of pinned snapshot ids. | n/a: internal. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotIdSetTests.kt` | 18 | Internal `SnapshotIdSet` bit set. | n/a: internal; its scenarios are `IdSet`'s unit tests (crates/compose-runtime/src/snapshots/id_set.rs, D770). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateExtensionsTests.kt` | 4 | `State<Int>.asIntState()` and siblings. | n/a: Kotlin autoboxing. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt` | 66 | `SnapshotStateList`: the full `MutableList` API, sublists, iterators, concurrent modification across snapshots, `toString` not observing. | ported: 61 in crates/compose-runtime/tests/suite/state_list.rs (steps S3 and S5, D761; `stateList_forEach_singleRead` on `Snapshot::observe`); the 5 concurrent ones are n/a (D14). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt` | 59 | `SnapshotStateMap`: the `MutableMap` API, entries/keys/values views, snapshots. | ported: 43 in crates/compose-runtime/tests/suite/state_map.rs (steps S3 and S5, D761; the views are snapshots, written through the map); the 12 `verifyMutableMap…Add`/`validate…Add` are n/a (adding through a view doesn't exist), the 4 concurrent ones are n/a (D14). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt` | 47 | `SnapshotStateObserver`: observe reads per scope, stages, pausing, `withoutReadObservation`, derived/computed state invalidation, recursive apply. | partial: 28 ported in crates/compose-runtime/tests/suite/snapshot_observation.rs (steps S1, S2 and S5, D753, D760, D771); the 16 computed-state ones wait for computed-state; the 3 of the deprecated `withNoObservations` are n/a. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt` | 21 | `SnapshotStateSet`: the `MutableSet` API, ordered iteration, snapshots. | ported: 17 in crates/compose-runtime/tests/suite/state_set.rs (steps S3 and S5, D761); `validateIterator_remove` is n/a (the iteration is a copy, `retain` writes: D766), the 3 concurrent ones are n/a (D14). |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt` | 89 | Snapshots: isolation, nested and mutable snapshots, apply and conflicts, merging policies, read/write/apply observers, transparent snapshots, record reuse. | ported: 83 in crates/compose-runtime/tests/suite/snapshot_isolation.rs (step S5, D769-D790) and snapshot_observation.rs (the observers', steps S1 and S2; `stateReadsCanBeObserved` and `stateWritesCanBeObserved` on `Snapshot::observe` there, on snapshots in the first); `Transparent…` are on `Snapshot::observe`'s frames (D771), whose 5 tests of the object's reuse, and `readOnly_clearsParentSnapshotOnRelease`, are n/a. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotWeakSetTests.kt` | 5 | Internal weak set. | n/a: internal. |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt` | 14 | `observeSnapshots`/`SnapshotObserver` (experimental): creation, apply, dispose, reads and writes per snapshot. | feature:snapshot-isolation |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/tooling/CompositionDataTests.kt` | 10 | `CompositionData`/`CompositionGroup`: iterating the slot table, finding nodes and source info, groups without calls. | feature:composition-data |
| `compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/tooling/CompositionInstanceTests.kt` | 5 | `findCompositionInstance`, context groups and parent instances. | feature:composition-data (also composition-context). |

### `runtime`: other source sets (`commonTest`, `jvm*Test`, `nonEmulatorJvmTest`, native, wasm, `androidDeviceTest`)

29 files, 373 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime/src/androidDeviceTest/kotlin/androidx/compose/runtime/snapshots/ParcelableMutableStateTests.kt` | 1 | Parcelling a `MutableState`. | n/a: Android `Parcelable` (compose-rs saves state through `remember_saveable`). |
| `compose/runtime/runtime/src/androidDeviceTest/kotlin/androidx/compose/runtime/snapshots/ParcelablePrimitiveMutableStateTests.kt` | 4 | Parcelling primitive states. | n/a: Android `Parcelable`. |
| `compose/runtime/runtime/src/androidDeviceTest/kotlin/androidx/compose/runtime/snapshots/ParcelableSnapshotSetTests.kt` | 5 | Parcelling a `SnapshotStateSet`. | n/a: Android `Parcelable`. |
| `compose/runtime/runtime/src/commonTest/kotlin/androidx/compose/runtime/MultiValueMapTest.kt` | 3 | Internal `MultiValueMap`. | n/a: internal. |
| `compose/runtime/runtime/src/commonTest/kotlin/androidx/compose/runtime/composer/linkbuffer/SlotTableAddressSpaceTests.kt` | 16 | The link-buffer slot table's address space. | n/a: internal data structure. |
| `compose/runtime/runtime/src/commonTest/kotlin/androidx/compose/runtime/composer/linkbuffer/SlotTableBuilderTests.kt` | 12 | The link-buffer slot table builder. | n/a: internal data structure. |
| `compose/runtime/runtime/src/commonTest/kotlin/androidx/compose/runtime/composer/linkbuffer/SlotTableEditorTests.kt` | 11 | The link-buffer slot table editor. | n/a: internal data structure. |
| `compose/runtime/runtime/src/commonTest/kotlin/androidx/compose/runtime/composer/linkbuffer/SlotTableReaderTests.kt` | 5 | The link-buffer slot table reader. | n/a: internal data structure. |
| `compose/runtime/runtime/src/commonTest/kotlin/androidx/compose/runtime/composer/linkbuffer/SlotTableTests.kt` | 102 | The link-buffer slot table: inserts, removes, moves, marks, source information. | n/a: internal data structure (see the gap-buffer row). |
| `compose/runtime/runtime/src/commonTest/kotlin/androidx/compose/runtime/internal/FloatingPointEqualityTest.kt` | 20 | Kotlin's `==` on `Float`/`Double` per platform (NaN, signed zeros, infinities). | n/a: Kotlin platform semantics; compose-rs's float comparison for skipping is pinned by compose_arg.rs's `values_for_small_data`. |
| `compose/runtime/runtime/src/jvmAndAndroidTest/kotlin/androidx/compose/runtime/CompositionJvmTests.kt` | 1 | Composable property delegates. | n/a: Kotlin delegates. |
| `compose/runtime/runtime/src/jvmAndAndroidTest/kotlin/androidx/compose/runtime/changelist/OperationDefinitionValidationTest.kt` | 1 | Change-list operation definitions. | n/a: internal. |
| `compose/runtime/runtime/src/jvmAndAndroidTest/kotlin/androidx/compose/runtime/changelist/OperationsTest.kt` | 20 | Change-list operation stack. | n/a: internal. |
| `compose/runtime/runtime/src/jvmAndAndroidTest/kotlin/androidx/compose/runtime/tooling/RecompositionTracerTest.kt` | 12 | `RecompositionTracer` flow events for reads, writes and recompositions. | n/a: `@InternalComposeTracingApi` (opt-in internal); compose-rs's analogue is `platform::track_recompositions`. |
| `compose/runtime/runtime/src/jvmTest/kotlin/androidx/compose/runtime/composer/gapbuffer/changelist/OperationDefinitionValidationTest.kt` | 1 | Gap-buffer change-list operation definitions. | n/a: internal. |
| `compose/runtime/runtime/src/jvmTest/kotlin/androidx/compose/runtime/composer/gapbuffer/changelist/OperationsTest.kt` | 20 | Gap-buffer change-list operation stack. | n/a: internal. |
| `compose/runtime/runtime/src/nativeTest/kotlin/androidx/compose/runtime/platform/SynchronizationTest.kt` | 2 | Native locks. | n/a: platform internals. |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/JvmCompositionTests.kt` | 10 | Deadlocks and races between threads, snapshot references kept, and NaN/unspecified `Float`, `Double`, `Dp` arguments skipping. | partial: 3 of the 5 NaN/unspecified-argument skip tests ported (crates/compose-runtime/tests/suite/jvm_composition.rs, `Dp` in crates/compose/tests/suite/jvm_composition.rs); `dpSizeUnspecifiedArgumentSkips` needs `DpSize: ComposeArg` and `dpOffsetUnspecifiedArgumentSkips` a `DpOffset` (ui area); the 5 thread/GC tests are n/a (D14). |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/LiveEditTests.kt` | 30 | Live Edit: invalidating groups by key, preserving state, recovering from exceptions. | n/a: no hot reload or Live Edit (D24); a panic poisons the composition by design. |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/MonotonicFrameClockTest.kt` | 4 | Finding the frame clock in a `CoroutineContext`. | n/a: compose-rs has one executor clock per thread, no coroutine context. |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/RecomposerTests.jvm.kt` | 4 | Recomposition on concurrent snapshot invalidation, cancel reporting `ShuttingDown`, disposed invalidated compositions not leaking. | partial: `disposedInvalidatedCompositionDoesNotLeak` in crates/compose-runtime/tests/suite/recomposer.rs (the content is dropped with the composition, its scopes freed) and recomposer_api.rs (no composition list to stay on), `recomposerCancelReportsShuttingDown…` in recomposer_api.rs; the 2 concurrency tests are n/a (D14). |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/reflect/ComposableMethodTest.kt` | 9 | Calling composables through JVM reflection. | n/a: JVM reflection. |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/snapshots/SnapshotFlowTestsJvm.kt` | 3 | `snapshotFlow` manager races across threads. | n/a: threads (D14). |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsJvm.kt` | 7 | `SnapshotStateObserver` races, and derived/computed state re-read right after changes. | partial: 4 ported in crates/compose-runtime/tests/suite/snapshot_observation.rs (`nestedObservationIsClearingThePreviousScopesBeforeReexecuting` and the 3 derived-state ones, S2); the computed-state one waits for computed-state, the 2 race tests are n/a (D14). |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTestsJvm.kt` | 9 | Multithreaded snapshot reads and writes, record regressions, per-thread read-only. | n/a: state is thread-bound (D14). |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/snapshots/SnapshotThreadMapTests.kt` | 15 | Internal per-thread snapshot map. | n/a: internal. |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt` | 33 | Compose stack traces attached to exceptions from composition, remember, effects, node updates, movable content. | feature:error-traces |
| `compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/SourceInfoParseTests.kt` | 12 | Parsing the compiler's source-information strings. | n/a: Kotlin compiler format (`@ComposeToolingApi`). |
| `compose/runtime/runtime/src/wasmJsTest/kotlin/androidx/compose/runtime/internal/IdentityHashCodeTest.kt` | 1 | Wasm identity hash codes. | n/a: platform internals. |

### `runtime`: integration tests (`integration-tests/src/androidTest`)

19 files, 99 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/AndroidInstrumentedMovableContentTests.kt` | 4 | Movable content with `BoxWithConstraints`, dialogs, subcompositions. | feature:movable-content |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/AndroidMovableContentTests.kt` | 1 | A movable content parameter in `BoxWithConstraints`. | feature:movable-content |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/AndroidSnapshotTests.kt` | 1 | Committing state from a background thread. | n/a: state is thread-bound (D14). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/ComposeIntoTests.kt` | 3 | `setContent` twice on a view, committing from another thread, collecting a composition with a pending invalidation. | partial: 2 ported in crates/compose-runtime/tests/suite/recomposer.rs (`set_content` twice keeps the effects; a composition is dropped while one of its scopes is held); the thread test is n/a (D14). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/CompositeHashTests.kt` | 1 | The composite key hash stays the same across `setContent` calls. | ported: 1 in crates/compose-animation/tests/suite/composite_hash.rs (a padding for `consumeWindowInsets`, which compose-rs lacks). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/ContextParametersTests.kt` | 2 | Composables with Kotlin context parameters and their defaults. | n/a: Kotlin language feature. |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/DisposeTests.kt` | 1 | Disposing a composition runs `onDispose`. | covered: crates/compose-runtime/tests/suite/scenarios.rs `dropping_a_composition_releases_everything`. |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt` | 14 | `collectAsState` on flows and `StateFlow`: values, initial values, replacing streams and contexts, null values. | ported: 12 in crates/compose-runtime/tests/suite/flows.rs (step S4, D768); the 2 with a coroutine context are n/a (`observingOnCustomContext`, `theCurrentValueIsNotLostWhenWeReplacedContext`). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/GroupSizeTests.kt` | 6 | Group sizes of real UI composables (Spacer, Checkbox, Text, Box, Button, Column). | n/a: pins the Kotlin compiler's output. |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/LiveEditApiTests.kt` | 16 | Hot-reload API: forced recomposition, errors and recovery. | n/a: no hot reload (D24). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/LiveEditRegressionTests.kt` | 3 | Hot-reload errors in `AnimatedVisibility` and `BoxWithConstraints`. | n/a: no hot reload (D24). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt` | 10 | Paused precomposition in `SubcomposeLayout`: key changes, extra recompositions, effects applied once, movable content. | partial: compose-rs pauses lazy items' content (`PausableContent`), covered by crates/compose-foundation/tests/suite/prefetch_pausable.rs and crates/compose-runtime/tests/suite/pausable.rs; 2 tests need movable-content; `SubcomposeLayout` itself is absent (ui area). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/ProduceStateTests.kt` | 1 | `produceState` producing values over time. | covered: crates/compose-foundation/tests/suite/effects.rs `produce_state_gives_what_its_coroutine_sets`. |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/RecomposerTests.kt` | 4 | Native views with attributes, a key change recreating, frame transitions, `Recomposer.runningRecomposers`. | partial: `testSlotKeyChangeCausesRecreate` ported in crates/compose-runtime/tests/suite/recomposer.rs (mock views); `testFrameTransition` checks snapshot ids (n/a, D702); `testNativeViewWithAttributes` Android views n/a; `runningRecomposerFlow` in recomposer_api.rs. |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt` | 13 | `SideEffect` order and keys, `DisposableEffect` order and key changes, `LaunchedEffect`, frames from effects, `rememberUpdatedState` in a long-lived effect. | ported: 12 in crates/compose-runtime/tests/suite/side_effects.rs (`testAwaitFrameFromLaunchedEffect` checks the driver's frame time, not the Choreographer's); `testCoroutineScopesHaveCorrectFrameClock` is n/a (no coroutine context). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SuspendingEffectsTests.kt` | 8 | `LaunchedEffect` and `rememberCoroutineScope`: frames awaited, active with the composition, cancelled after leaving, run after apply. | covered: 6 of 8 by crates/compose-foundation/tests/suite/effects.rs (11) and crates/compose-runtime/tests/suite/holders.rs; the frame clock in the context and the parent `Job` check are n/a (no coroutine context). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/tooling/AndroidCompositionObserverTests.kt` | 1 | Observing a UI composition. | feature:composition-observer |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/tooling/CompositionRegistrationObserverTest.kt` | 9 | `Recomposer.observe`: compositions registered and unregistered (subcompositions, views, dialogs). | feature:composition-observer (needs recomposer). |
| `compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/tooling/CompositionRegistrationObserverWithUnconfinedDispatcherTest.kt` | 1 | Registration observer while the initial composition is forced. | feature:composition-observer (needs recomposer). |

### `runtime-saveable`

9 files, 83 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt` | 11 | State across activity recreation: `remember` lost, `rememberSaveable` and states restored, two views/fragments, holders, list/map savers, popups, dialogs, movable content. | partial: restoring in a new host is covered by crates/compose-foundation/tests/suite/platform.rs (`remember_saveable_comes_back_in_a_new_host`, …) and saveable_state_holder.rs (`the_holder_is_saved_with_what_it_keeps_across_recreation`); missing list/map savers (saveable-gaps) and movable-content; fragments n/a. |
| `compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt` | 14 | `rememberSaveable`: restore, savers, serializers, registry `canBeSaved`, re-registration on key or input changes, the latest saver used, unregistering on disposal. | partial: `remember_saveable(_with)` covered by crates/compose-foundation/tests/suite/platform.rs and platform_review.rs; missing `inputs`, the latest saver (compose-rs keeps the first), serializers (saveable-gaps, remember-serializable); `customKey`/`emptyKeyIsNotUsed` test the deprecated `key` (n/a). |
| `compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableWithMutableStateTest.kt` | 11 | `rememberSaveable` of `MutableState`s: values, lists, sets, `stateSaver`, serializers, nullable values. | partial: `OwnedState` values and `Option` restore today, state lists and sets too (`simpleRestoreList`, `simpleRestoreSet` in crates/compose-foundation/tests/suite/platform.rs, D762); serializers need remember-serializable. |
| `compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt` | 8 | Restoring values in loops, with keys, extra functions and conditional states. | ported: 8 in crates/compose-runtime/tests/suite/restoration.rs (a `RestorationTester` there stands for `StateRestorationTester`, recreating the composition and registry from the saved bytes). |
| `compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/SaveableStateHolderTest.kt` | 13 | `SaveableStateHolder`: screens' state saved and restored, removal, nested registries, locals, the `keys` property. | partial: covered by crates/compose-foundation/tests/suite/saveable_state_holder.rs (7) and review_holder.rs (5); missing `keys` (saveable-gaps); the other-thread test is n/a. |
| `compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/AutoSaverTest.kt` | 1 | `autoSaver` saves the value as is. | ported: 1 in crates/compose-runtime/tests/suite/saveable_state_registry.rs. |
| `compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/ListSaverTest.kt` | 5 | `listSaver`: save and restore, failures, nullable items. | ported: 4 in crates/compose-runtime/tests/suite/saveable_state_registry.rs (D762); `exceptionWhenAllItemsCantBeSaved` is n/a: `Saveable` items, so it can't compile. |
| `compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/MapSaverTest.kt` | 5 | `mapSaver`: save and restore, failures, nullable items. | ported: 3 in crates/compose-runtime/tests/suite/saveable_state_registry.rs (D762); the 2 `exceptionWhen…` are n/a: saved values are typed. |
| `compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt` | 15 | `SaveableStateRegistry`: register, unregister, restore, `performSave`, `canBeSaved`, key validation, duplicate keys. | ported: 10 in crates/compose-runtime/tests/suite/saveable_state_registry.rs; the 3 key-validation tests are n/a (keys are `u64`), the 2 `canBeSaved` ones n/a (values are typed `SavedValue`s). |

### `runtime-retain`

3 files, 41 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt` | 33 | `retain`, `RetainObserver` ordering, `RetainedValuesStore`s (forgetful, managed, registry), movable and pausable content. | partial: 30 ported in crates/compose-runtime/tests/suite/retain.rs (each pair `_linkComposer`/`_gapComposer` as one); `retain_throwsForRememberObserver_*` (3) differ by design: a generic function can't ask what its type implements, so a value with callbacks goes through `retain_observer` (D1306). |
| `compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainedEffectTest.kt` | 4 | `RetainedEffect`: retained, key changes, retired when values stop being kept. | ported: crates/compose-runtime/tests/suite/retain.rs |
| `compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/SafeMultiValueMapTest.kt` | 4 | Internal multi-value map. | n/a: internal. |

### `runtime-livedata`, `runtime-rxjava2`, `runtime-rxjava3`

5 files, 29 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime-livedata/src/androidTest/java/androidx/compose/runtime/livedata/LiveDataAdapterTest.kt` | 9 | `LiveData.observeAsState` with a lifecycle. | n/a: Android LiveData (Rust streams are flow-adapters). |
| `compose/runtime/runtime-rxjava2/src/androidDeviceTest/kotlin/androidx/compose/runtime/rxjava2/CompletableAdapterTest.kt` | 3 | RxJava 2 `Completable.subscribeAsState`. | n/a: RxJava. |
| `compose/runtime/runtime-rxjava2/src/androidDeviceTest/kotlin/androidx/compose/runtime/rxjava2/RxJava2AdapterTest.kt` | 7 | RxJava 2 `subscribeAsState`. | n/a: RxJava. |
| `compose/runtime/runtime-rxjava3/src/androidDeviceTest/kotlin/androidx/compose/runtime/rxjava3/CompletableAdapterTest.kt` | 3 | RxJava 3 `Completable.subscribeAsState`. | n/a: RxJava. |
| `compose/runtime/runtime-rxjava3/src/androidDeviceTest/kotlin/androidx/compose/runtime/rxjava3/RxJava3AdapterTest.kt` | 7 | RxJava 3 `subscribeAsState`. | n/a: RxJava. |

### `runtime-tracing`

5 files, 18 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime-tracing/runtime-tracing-benchmark/src/androidTest/java/androidx/compose/runtime/tracing/benchmark/StackBenchmark.kt` | 4 | Benchmark of the internal stack. | n/a: benchmark of internals. |
| `compose/runtime/runtime-tracing/src/androidTest/java/androidx/compose/runtime/tracing/test/ComposeTracingInitializerTest.kt` | 1 | The AndroidX Startup initializer of composition tracing. | n/a: Android tracing integration. |
| `compose/runtime/runtime-tracing/src/androidTest/java/androidx/compose/runtime/tracing/test/RecompositionTracingReceiverTest.kt` | 8 | The broadcast receiver starting and stopping recomposition tracing. | n/a: Android broadcast receiver. |
| `compose/runtime/runtime-tracing/src/test/java/androidx/compose/runtime/tracing/test/PoolTest.kt` | 2 | Internal object pool. | n/a: internal. |
| `compose/runtime/runtime-tracing/src/test/java/androidx/compose/runtime/tracing/test/StackTest.kt` | 3 | Internal stack. | n/a: internal. |

### `runtime`: benchmarks (`compose-runtime-benchmark`)

17 files, 118 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/CollectAsStateBenchmark.kt` | 8 | Benchmark: `collectAsState`. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/ComposeBenchmark.kt` | 20 | Benchmark: composing and recomposing trees of rects, observers, skipping. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/CompositionLocalBenchmark.kt` | 9 | Benchmark: providing and reading composition locals. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/DbMonsterBenchmark.kt` | 2 | Benchmark: the DBMonster table. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/DeepTreeBenchmark.kt` | 4 | Benchmark: deep trees. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/EffectsBenchmark.kt` | 5 | Benchmark: effects. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/KotlinCoroutinesBenchmark.kt` | 9 | Benchmark: coroutine primitives. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/RememberSavableBenchmark.kt` | 6 | Benchmark: `rememberSaveable`. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/RetainBenchmark.kt` | 7 | Benchmark: `retain`. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/SiblingBenchmark.kt` | 1 | Benchmark: sibling management. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/SlotTableIntegrationBenchmark.kt` | 11 | Benchmark: slot table operations. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/SnapshotFlowBenchmark.kt` | 2 | Benchmark: `snapshotFlow`. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/SnapshotFlowLockContentionBenchmark.kt` | 1 | Benchmark: `snapshotFlow` lock contention. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/SnapshotStateAutoboxingBenchmark.kt` | 6 | Benchmark: primitive state autoboxing. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/SnapshotStateObserverBenchmark.kt` | 12 | Benchmark: `SnapshotStateObserver`. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/TrivialCompositionBenchmarkValidator.kt` | 1 | Benchmark: the benchmark harness. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |
| `compose/runtime/runtime/compose-runtime-benchmark/src/androidTest/java/androidx/compose/runtime/benchmark/state/ComposeStateReadBenchmark.kt` | 14 | Benchmark: state reads. | n/a: Jetpack Benchmark on a device; compose-rs's analogues are its criterion benches (benchmarks/benches/runtime.rs, ui.rs) and docs/12. |

### Lint checks (`runtime-lint`, `runtime-retain-lint`, `runtime-saveable-lint`)

21 files, 142 tests.

| File (under `frameworks/support/`) | Tests | Covers | Status |
|---|---|---|---|
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/ApiLintVersionsTest.kt` | 1 | Lint: the lint registry's API version. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/AutoboxingStateCreationDetectorTest.kt` | 14 | Lint: `mutableStateOf<Int>` should be `mutableIntStateOf`. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/AutoboxingStateValuePropertyDetectorTest.kt` | 4 | Lint: `.value` on primitive states. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/ComposableCoroutineCreationDetectorTest.kt` | 2 | Lint: `launch`/`async` called in composition. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/ComposableFlowOperatorDetectorTest.kt` | 2 | Lint: flow operators called in composition. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/ComposableLambdaParameterDetectorTest.kt` | 9 | Lint: naming and position of composable lambda parameters. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/ComposableNamingDetectorTest.kt` | 9 | Lint: capitalized names of Unit composables. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/ComposableStateFlowValueDetectorTest.kt` | 2 | Lint: `StateFlow.value` read in composition. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/CompositionLocalNamingDetectorTest.kt` | 2 | Lint: `Local` prefix of composition locals. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/FrequentlyChangingValueDetectorTest.kt` | 4 | Lint: reads of `@FrequentlyChangingValue` in composition. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/MutableCollectionMutableStateDetectorTest.kt` | 11 | Lint: `mutableStateOf(mutableList)`. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/OpaqueUnitKeyDetectorTest.kt` | 40 | Lint: `Unit`-typed keys of `remember`/effects. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/ProduceStateDetectorTest.kt` | 2 | Lint: `produceState` producers that never write. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/RememberDetectorTest.kt` | 5 | Lint: `remember` returning `Unit`. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/RememberInCompositionDetectorSuppressionTest.kt` | 3 | Lint: suppressing `@RememberInComposition` warnings. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/RememberInCompositionDetectorTest.kt` | 5 | Lint: `@RememberInComposition` calls not remembered. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-lint/src/test/java/androidx/compose/runtime/lint/UnrememberedStateDetectorTest.kt` | 6 | Lint: `mutableStateOf` in composition without `remember`. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-retain-lint/src/test/java/androidx/compose/runtime/retain/lint/ApiLintVersionsTest.kt` | 1 | Lint: the retain lint registry's API version. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-retain-lint/src/test/java/androidx/compose/runtime/retain/lint/RetainDetectorTest.kt` | 17 | Lint: values that must not be `retain`ed (`@DoNotRetain`, contexts). | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-saveable-lint/src/test/java/androidx/compose/runtime/saveable/lint/ApiLintVersionsTest.kt` | 1 | Lint: the saveable lint registry's API version. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |
| `compose/runtime/runtime-saveable-lint/src/test/java/androidx/compose/runtime/saveable/lint/RememberSaveableDetectorTest.kt` | 2 | Lint: `rememberSaveable` of a `MutableState` given a non-state saver. | n/a: Android Lint check for Kotlin source (compose-rs relies on the type system and rustc lints). |

## The public API surface

From each module's `api/current.txt` (`restricted_current.txt` only to tell what isn't public). Deprecated APIs are left out; experimental ones are marked *(exp)*. "Has" means compose-rs has it, Rust-shaped; the rest is grouped into the features of the next section.

### What compose-rs has

`remember` (keys as a tuple: `remember_keyed`), `key`, `RememberObserver` (through `remember_observer`, not plain `remember`), `currentRecomposeScope` (`current_recompose_scope`), `RecomposeScope.invalidate`, `ReusableContent`, `ReusableContentHost`, `mutableStateOf` with a policy (`mutable_state_of`, `mutable_state_of_with`), `State`/`MutableState`, `structuralEqualityPolicy`, `neverEqualPolicy`, `referentialEqualityPolicy` (identity as `ComposeArg` defines it), `SnapshotMutationPolicy.equivalent` (`MutationPolicy`), `derivedStateOf` with and without a policy, `mutableStateListOf`/`toMutableStateList` (as an `OwnedState<Vec<T>>`, see state-collections), `compositionLocalOf` with and without a policy (`CompositionLocal::new`, `with_policy`), `staticCompositionLocalOf` (`new_static`), `compositionLocalWithComputedDefaultOf` and its static form (`with_computed_default`, `static_with_computed_default`), `CompositionLocalAccessorScope.currentValue` (`current_value`), `CompositionLocalProvider` with one value, several (a tuple, a `Vec`, an array) or a `CompositionLocalContext`, `provides`, `providesDefault`, `providesComputed`, `ProvidedValue.canOverride`/`.value`, `CompositionLocal.current`, `CompositionLocalContext` and `currentCompositionLocalContext`, `CompositionLocalMap` (`get`, `Empty` as `empty()`) and `currentComposer.currentCompositionLocalMap` (`current_composition_local_map`), `withCompositionLocal(s)`, `SideEffect` (with keys: `KeyedSideEffect`), `DisposableEffect`, `LaunchedEffect`, `rememberCoroutineScope` without `getContext`, `produceState` (keys as a tuple, a policy with `produce_state_with`, `ProduceStateScope.awaitDispose`), `rememberUpdatedState` (a `State`), `withFrameNanos`, `withFrameMillis`, `MonotonicFrameClock` (a trait; `ExecutorFrameClock` is the executor's), `BroadcastFrameClock` (`send_frame`, `cancel`, `has_awaiters`, `with_on_new_awaiters`), `PausableMonotonicFrameClock` (`pause`, `resume`, `is_paused`), `Composition` (`set_content`, `has_invalidations`; dropping it disposes), `PausedComposition` (`resume`, `apply`, `cancel`, `is_complete`, `is_applied`, `is_cancelled`), `ShouldPauseCallback`, `ComposeNode`/`ReusableComposeNode` (`…WithContent`, `…WithSkippableUpdate`), `Updater` (`set`, `update`, `init`, `init_with`, `reconcile`), `SkippableUpdater`, `Applier` (Rust-shaped: parents addressed by `NodeRef`; `reuse`, `deactivate`, `release` stand for `ComposeNodeLifecycleCallback`), `currentCompositeKeyHashCode` (as `current_composite_key_hash_code`), the `@Composable` and `@NonRestartableComposable` annotations (`#[composable]`, `#[composable(non_restartable)]`). Saveable: `rememberSaveable` without `inputs` (`remember_saveable`, `remember_saveable_with`), `Saver` and `Saver(save, restore)` (`saver`), `autoSaver` (`AutoSaver`), `SaveableStateRegistry` (`consume_restored`, `register_provider`, `perform_save`, `Entry.unregister`), `LocalSaveableStateRegistry`, `SaveableStateHolder` (`SaveableStateProvider`, `remove_state`), `rememberSaveableStateHolder`.

### Missing or partial, by feature

| Feature | Compose APIs (package `androidx.compose.runtime` unless named) | compose-rs today |
|---|---|---|
| host-content-groups | not an API of Compose's: its compiler sees every `@Composable` lambda, so an `if` in `setContent { … }` has a group | a gap to close: the plain closures given to a host (`run_app(options, || …)`, `application(|| …)`, `compose_test(|| …)`, `UiHost::set_content`, `Composition::set_content`) get no region groups, so an `if` there silently passes state to the next sibling (docs/03; found porting `RememberViewModelStoreProviderTest`, D838) |
| host-defaults | `compositionLocalWithHostDefaultOf`, `HostDefaultKey`, `HostDefaultProvider`, `LocalHostDefaultProvider` | none: the other local variants are done (local-variants) |
| state-collections | `SnapshotStateList` (the `MutableList` API, `toList`, `SnapshotStateList(size, init)`), `SnapshotStateMap` (`MutableMap` API, `toMap`), `SnapshotStateSet` (`MutableSet` API, `toSet`), `mutableStateMapOf`, `mutableStateSetOf`, `toMutableStateMap`, element-wise `mutableStateListOf` | done (step S3, D761, `compose::runtime::snapshots`): the handles, `OwnedStateList`/`Map`/`Set`, `mutable_state_list_of`/`_from`, `snapshot_state_list`, `mutable_state_map_of`/`_from`, `mutable_state_set_of`/`_from`, `to_list`, `to_map`, `to_set` |
| computed-state | `computedStateOf(policy, calculation)` | none |
| snapshot-observation | `Snapshot.withoutReadObservation`, `Snapshot.observe(readObserver, writeObserver, block)`, `Snapshot.registerApplyObserver`, `Snapshot.registerGlobalWriteObserver`, `Snapshot.sendApplyNotifications`, `Snapshot.isApplyObserverNotificationPending`, `ObserverHandle`, `snapshots.SnapshotStateObserver` (`observeReads`, `clear`, `clearIf`, `start`, `stop`, `notifyChanges`) | done (step S1, D751–D753, `compose::runtime::snapshots`): `Snapshot::without_read_observation`, `observe`, `register_apply_observer`, `register_global_write_observer`, `send_apply_notifications`, `is_apply_observer_notification_pending`, `notify_objects_initialized`, `ObserverHandle`, `ChangeSet`, `StateObjectId`, `SnapshotStateObserver` (`observe_reads`, `clear`, `clear_if`, `clear_all`, `start`, `stop`, `notify_changes`) |
| snapshot-isolation | `snapshots.Snapshot` (`takeSnapshot`, `takeMutableSnapshot`, `withMutableSnapshot`, `global`, `readOnly`, `current`, `isInSnapshot`, `enter`, `unsafeEnter`/`unsafeLeave`, `dispose`, `takeNestedSnapshot`, `hasPendingChanges`, `snapshotId`, `notifyObjectsInitialized`, `openSnapshotCount`), `MutableSnapshot` (`apply`, `takeNestedMutableSnapshot`), `SnapshotApplyResult` (`Success`, `Failure`, `check`), `SnapshotApplyConflictException`, `SnapshotMutationPolicy.merge`, `StateObject`, `StateRecord`, `readable`, `writable`, `withCurrent`, `SnapshotMutableState.policy`, `SnapshotContextElement` and `Snapshot.asContextElement`, `snapshots.tooling.observeSnapshots`, `SnapshotObserver`, `SnapshotInstanceObservers` *(exp)* | done (steps S5 and S6, D769-D795, `compose::runtime::snapshots`) but tooling (S7). `Snapshot.readOnly { }` is `Snapshot::with_read_only`, `asContextElement` is `Snapshot::enter_future`, `SnapshotApplyConflictException` is the panic payload `SnapshotApplyConflict`, `StateObject` is `SnapshotStateObject` over a `StateRecord`, with `record_snapshot_id` for `StateRecord.snapshotId` and `writable_in`/`readable_in` for `writable(state, snapshot)`. Not there: `firstStateRecord`, `prependStateRecord`, `StateRecord.next`, `create`, `assign` (the chain is the runtime's, D784), and `SnapshotMutableState.policy` (a policy is a type, whose functions the cell keeps) |
| snapshot-flow | `snapshotFlow(block)`, `snapshotFlow(manager, block)` *(exp)*, `SnapshotFlowManager` *(exp)* | done (step S4, D768): `snapshot_flow`, `snapshot_flow_with`, `flow::SnapshotFlowManager` |
| flow-adapters | `Flow.collectAsState(initial, context, policy)`, `StateFlow.collectAsState(context, policy)` | done (step S4, D768): `collect_as_state`, `collect_as_state_with`, `flow::MutableStateFlow`, `flow::StateFlow` |
| movable-content | `movableContentOf` (0 to 4 parameters), `movableContentWithReceiverOf` (0 to 3 parameters) | `movable_content_of`, `movable_content_of_with`, `movable_content_with_receiver_of` (several parameters as a tuple), in a composition (D1305); across compositions, C6 |
| composition-context | `rememberCompositionContext`, `CompositionContext` (`effectCoroutineContext`, `scheduleFrameEndCallback`), the `parent` of `Composition(applier, parent)`, `ControlledComposition(applier, parent)`, `ReusableComposition(applier, parent)`, `PausableComposition(applier, parent)` | `remember_composition_context`, `CompositionContext` (a recomposer, or a place in a composition), `Composition::with_parent`, `Composition::reusable(applier, parent)`, `Composition::pausable(applier, parent)`, `platform::{subcomposition, reusable_subcomposition, pausable_subcomposition, with_lent_applier}` (D1303, D1304); `effectCoroutineContext` by what `LaunchedEffect` and `remember_coroutine_scope` use |
| composition-api | `Composition.dispose`, `.isDisposed`; `ReusableComposition` (`setContentWithReuse`, `deactivate`); `PausableComposition` (`setPausableContent`, `setPausableContentWithReuse`); `ControlledComposition` (`composeContent`, `recompose`, `applyChanges`, `applyLateChanges`, `changesApplied`, `abandonChanges`, `invalidateAll`, `isComposing`, `hasPendingChanges`, `observesAnyOf`, `recordModificationsOf`, `recordReadOf`, `recordWriteOf`, `prepareCompose`, `delegateInvalidations`, `getAndSetShouldPauseCallback`); the name `currentCompositeKeyHashCode` | `Composition::{dispose, is_disposed, invalidate_all}`, dropping disposes; `Composition::reusable(applier, parent)`, `ReusableComposition::{set_content, set_content_with_reuse, deactivate}`; `Composition::pausable(applier, parent)`, `PausableComposition::{set_pausable_content, set_pausable_content_with_reuse}`; the name `current_composite_key_hash_code`; `Pass::{finish_composing, apply_work}` in the crate; the `parent` of each (D1300, D1301, D1303); `recompose` composes and applies in one step as the hosts' path until C4; `recompose_pausable(_for)` with `platform::PausableContent` regions |
| recomposer | `Recomposer` (`runRecomposeAndApplyChanges`, `awaitIdle`, `cancel`, `close`, `join`, `currentState`, `State` enum, `hasPendingWork`, `changeCount`, `effectCoroutineContext`, `pauseCompositionFrameClock`, `resumeCompositionFrameClock`, `asRecomposerInfo`, `runningRecomposers`), `RecomposerInfo` (`state`, `hasPendingWork`, `changeCount`, `errorState` (tooling opt-in), `observe` *(exp)*), `withRunningRecomposer`, `RecomposerErrorInformation` (tooling opt-in) | present but `errorState`, `observe`, `RecomposerErrorInformation` and `effectCoroutineContext` (D1301): hosts (`UiHost`, `ComposeTestRule`, the desktop `application`) still drive `Composition::recompose` and the executor, until the ui area's step |
| frame-end-callback | `Composer.scheduleFrameEndCallback`, `CompositionContext.scheduleFrameEndCallback`, `Recomposer.scheduleFrameEndCallback`, `CancellationHandle` | present, run by `perform_frame`, on the default recomposer not yet by a host (D1301) |
| composable-annotations | `@NonSkippableComposable`, `@ReadOnlyComposable`, `@DontMemoize`, `@ExplicitGroupsComposable` | `#[composable(non_restartable)]` only |
| saveable-gaps (`runtime.saveable`) | `listSaver`, `mapSaver`, `rememberSaveable(vararg inputs, …)` (all three non-deprecated forms take `inputs`), the latest `saver` used for saving, `SaveableStateHolder.keys`, `SaveableStateRegistry(restoredValues, canBeSaved)` with `canBeSaved`, `SaverScope.canBeSaved` | `remember_saveable(_with)` without inputs, keeping the saver of its first composition; `SaveableStateRegistry::new(restored)`; typed `SavedValue`s instead of `canBeSaved` |
| remember-serializable (`runtime.saveable`) | `rememberSerializable` (4 forms, kotlinx.serialization) | none; the Rust analogue would be serde (a dependency decision) |
| retain (`runtime.retain`) | `retain(keys…, calculation)`, `RetainObserver` (`onRetained`, `onEnteredComposition`, `onExitedComposition`, `onRetired`, `onUnused`), `RetainedValuesStore`, `ForgetfulRetainedValuesStore`, `ManagedRetainedValuesStore` (+ `retainManagedRetainedValuesStore`), `RetainedValuesStoreRegistry` (+ `retainRetainedValuesStoreRegistry`, `LocalRetainedValuesStoreProvider(key)`, `clearChild`, `clearChildren`, `dispose`), `LocalRetainedValuesStore`, `LocalRetainedValuesStoreProvider`, `withLocalRetainedValuesStore`, `RetainedEffect` (1 to 3 keys, vararg), `RetainedEffectScope.onRetire`, `RetainedEffectResult` | `compose::runtime::retain` (D1306): `retain`, `retain_keyed`, and `retain_observer`(`_keyed`) for values with callbacks; keys as one value, tuples for several; the store's `consume_exited_value` is an `Option` for `defaultValue` |
| composition-observer | `tooling.CompositionObserver`, `CompositionObserverHandle`, `ObservableComposition`, `Composition.setObserver`, `CompositionRegistrationObserver`, `Recomposer.observe`, `RecomposerInfo.observe` *(all exp)* | none |
| composition-data | `tooling.CompositionData` (`compositionGroups`, `isEmpty`, `find`), `CompositionGroup` (`key`, `node`, `data`, `identity`, `sourceInfo`, `groupSize`, `slotsSize`), `CompositionInstance`, `findCompositionInstance`, `LocalInspectionTables`, `Composer.compositionData`, `CompositionServices`, `CompositionServiceKey` | `Composition::dump` (a text dump for tests) |
| error-traces | `tooling.ComposeStackTraceMode` (`None`, `GroupKeys`, `SourceInformation`, `Auto`), `Composer.setDiagnosticStackTraceMode`, `CompositionErrorContext.attachComposeStackTrace`, `LocalCompositionErrorContext` | a panic in a restarted composable names it (crates/compose/tests/suite/review3.rs); no composable stack |
| composition-tracing | `Composer.setTracer`, `CompositionTracer`, `isTraceInProgress`, `traceEventStart`, `traceEventEnd` | `platform::track_recompositions`, `COMPOSE_FRAME_STATS` |

### Not applicable

- **Compiler ABI and generated-code hooks:** `Composer`'s methods (`changed*`, `startRestartGroup`, `startDefaults`, `cache`, `sourceInformation*`, `joinKey`, …), `ScopeUpdateScope`, `ComposerKt`, `internal.*` (`ComposableLambda`, live literals, `Decoy`, `FunctionKeyMeta`, `StabilityInferred`), `ComposeCompilerApi`, `ComposableTarget`/`ComposableOpenTarget`/`ComposableTargetMarker`/`ComposableInferredTarget*`, `@NoLiveLiterals`, `@DisallowComposableCalls`: compose-rs's transform has its own `platform::Composer` (D260).
- **By design:** `@Stable`, `@Immutable`, `@StableMarker` (no stability inference, `ComposeArg` instead, D5); `HotReloader` (no hot reload, D24); the primitive states `mutableIntStateOf`, `mutableLongStateOf`, `mutableFloatStateOf`, `mutableDoubleStateOf`, `IntState`…`MutableDoubleState`, `asIntState`… (autoboxing; `MutableState<i32>` is unboxed); `MutableState.component1/2` and the `getValue`/`setValue` delegates (Kotlin syntax); `CoroutineContext` parameters (`rememberCoroutineScope(getContext)`, `collectAsState(context)`, `CoroutineContext.monotonicFrameClock` *(exp)*): one executor per thread; `AbstractApplier` (compose-rs's helper is `NodeTree`, which addresses parents by handle rather than a `down`/`up` cursor).
- **Platform:** `reflect.ComposableMethod`, `ViewTreeHostDefaultKey`, `ActualAndroid`/`ActualJvm`, `platform.Synchronization`, `Parcelable` state, `runtime-livedata` (`observeAsState`), `runtime-rxjava2`/`3` (`subscribeAsState`), `runtime-tracing` (`ComposeTracingInitializer`, `RecompositionTracingReceiver`).
- **Internal or opt-in internal:** `ComposeRuntimeFlags` *(exp)*, `InternalComposeApi` members (`MovableContent`, `MovableContentState(Reference)`, `ControlledComposition.insertMovableContent`…), `tooling.RecompositionTracer` (`@InternalComposeTracingApi`), `CompositionScopedCoroutineScopeCanceller`, `ComposeToolingFlags`, `SourceInformation`/`parseSourceInformation`/`LocationSourceInformation`/`ParameterSourceInformation`/`IdentifiableRecomposeScope` (`@ComposeToolingApi`).
- **Lint-only annotations:** `@RememberInComposition`, `@FrequentlyChangingValue`, `@DoNotRetain` (`runtime-annotation`).

## Missing features

Sizes: S under a day, M a few days, L one to two weeks, XL multi-week. None of these is experimental in Compose unless said.

### keyed-side-effect (S): done
- `KeyedSideEffect(keys, effect)` (keys a value, a tuple, or an array for varargs); tests in crates/compose-runtime/tests/suite/side_effects.rs.

### produce-state-scope (S): done
- `ProduceStateScope` (`await_dispose`; no `CoroutineScope` part: launch from the producer's own future), `produce_state` with keys as a tuple, `produce_state_with` taking a policy; the producer runs only when its coroutine starts. Tests in crates/compose-runtime/tests/suite/produce_state.rs.

### state-gaps (S): done
- `referential_equality_policy()` (identity as `ComposeArg` defines it) and `remember_updated_state` returning a `State<T>` (written during composition, as Compose's); tests in crates/compose-runtime/tests/suite/side_effects.rs.

### compose-node (S): done
- `ComposeNode`, `ReusableComposeNode` (content and skippable-update forms), `Updater`, `SkippableUpdater`, with `Applier::node_mut` lending the node. The kit's emitters use them. Tests: crates/compose-runtime/tests/suite/composition.rs (`composeNodeSetVsUpdate`), composition_reusing.rs (`onReuseIsCalledBeforeSetter*`).

### composable-annotations (S)
- **APIs:** `#[composable(non_skippable)]` (`@NonSkippableComposable`), `read_only` (`@ReadOnlyComposable`: no group), `#[dont_memoize]` on a lambda (`@DontMemoize`), `explicit_groups` (`@ExplicitGroupsComposable`).
- **Tests:** none directly (`CompositionTests.composableWithUnstableParameters_skipped` touches skipping).
- **Depends on:** nothing; in compose-macros.

### saveable-gaps (M): `listSaver`, `mapSaver` and the state lists' and sets' savers done (D762)
- **APIs:** `listSaver`, `mapSaver`, `rememberSaveable(inputs…)` (the value reset when inputs change), saving with the latest saver passed, `SaveableStateHolder::keys`, savers for state lists and sets.
- **Tests:** `ListSaverTest`, `MapSaverTest`, `RememberSaveableTest` (`restoreCorrectValueAfterInputChanges`, `changingInputIsNotAffectingOrderOfRestoration`, `theLatestPassedSaverIsUsed`), `RememberSaveableWithMutableStateTest` (`simpleRestoreList`, `simpleRestoreSet`), `SaveableStateHolderTest` (`keysPropertyIsUpdated`, `keysPropertyIsRestored`), `ActivityRecreationTest.stateStoredViaListSaverOrMapSaverIsRestored`.
- **Depends on:** state-collections for the list and set savers.

### remember-serializable (M, optional)
- **APIs:** `rememberSerializable` (4 forms).
- **Does:** saves any serializable value without a hand-written saver. Rust-shaped: serde's `Serialize`/`Deserialize` into `SavedValue`.
- **Tests:** `RememberSaveableTest.restoreWithSerializer`, `RememberSaveableWithMutableStateTest` (`simpleSerializable`, `restoreWithSerializer`, `nullableSerializable…`).
- **Depends on:** a decision to take serde as an optional dependency.

### local-variants (M): done
- Static locals, policies, computed defaults, `provides_default`, `provides_computed`, several values per provider (one group), `CompositionLocalMap`, `CompositionLocalContext`, `with_composition_local(s)` (D713). Tests: crates/compose-runtime/tests/suite/composition_local.rs. Compose's ui declares its platform locals static (`LocalDensity`, `LocalLayoutDirection`): the ui ledger can now make compose-ui's so.

### host-defaults (S)
- **APIs:** `compositionLocalWithHostDefaultOf(key)`, `HostDefaultKey<T>`, `HostDefaultProvider` (`getHostDefault`), `LocalHostDefaultProvider`.
- **Does:** a local whose default the host gives (an Android view's, a desktop window's), through a provider local.
- **Tests:** the 5 `CompositionLocalTests.hostDefault_*`.
- **Depends on:** local-variants (done); hosts provide `LocalHostDefaultProvider` (ui area).

### host-content-groups (M, needs a decision)
- **Gap:** what Compose's compiler gives every content lambda, a closure given to a host doesn't get: `run_app(options, || …)`, `application(|| …)`, `compose_test(|| …)`, `UiHost::set_content`, `Composition::set_content`. A `#[composable]` function is the only thing the macro sees, so an `if` in such a closure has no group: with its branch gone, the next call of the same composable matches the group it left, and silently takes its `remember`s (docs/03). Tests port it unawares: `onDispose_doesNotAffectSiblingStores` failed for it, until its content was a function.
- **Does:** an `if` at a host's root keeps its state to itself, whichever way the fix goes (a proc macro at those entry points, a lint or a debug check, the closures' own source).
- **Tests:** a sibling after a removed `if` in each entry point's closure keeps its own state.
- **Depends on:** nothing; not fixed with lifecycles.

### state-collections (M): done (steps S3 and S5 of docs/parity/design/snapshots.md; D761)
- **APIs:** `SnapshotStateList<T>`, `SnapshotStateMap<K, V>`, `SnapshotStateSet<T>` with `mutable_state_list_of`, `mutable_state_map_of`, `mutable_state_set_of`, `to_mutable_state_list`, `to_mutable_state_map`: handles (as `MutableState`) with Rust-collection methods (`push`, `insert`, `remove`, `retain`, `get`, `len`, `iter`, `sort_by`, ranges for `subList`), reads subscribing, an unchanged write (`set` of an equal element) not invalidating.
- **Tests:** `SnapshotStateListTests` (66), `SnapshotStateMapTests` (59), `SnapshotStateSetTests` (21).
- **Depends on:** nothing. Lazy lists' `items(list)` in foundation could take them.

### computed-state (M)
- **APIs:** `computedStateOf(policy, calculation)`.
- **Does:** a `State` whose calculation runs at each read outside observation, and is cached while observed, invalidating readers only when its result changes by the policy; writes in the calculation are errors.
- **Tests:** `CompositionAndComputedStateTests` (30), `ComputedSnapshotStateTests` (24), 16 of `SnapshotStateObserverTestsCommon`, 2 of `SnapshotStateObserverTestsJvm`.
- **Depends on:** snapshot-observation for the observer tests; shares derived state's machinery.

### snapshot-observation (M): done (steps S1 and S2 of docs/parity/design/snapshots.md; D751–D753, D759, D760)
- **APIs:** `Snapshot::without_read_observation`, `Snapshot::observe(read_observer, write_observer, block)`, `register_apply_observer`, `register_global_write_observer`, `send_apply_notifications`, `ObserverHandle`, a public `SnapshotStateObserver` built on `platform::ReadObserver`.
- **Done:** invalidation moved to apply time (`send_apply_notifications` at turn ends, D751), the observers and `SnapshotStateObserver` (D753); 40 tests ported (crates/compose-runtime/tests/suite/snapshot_observation.rs, derived_snapshot_state.rs, recomposer.rs).
- **Does:** everything about observing reads and writes that needs no isolation: reading state without subscribing, hearing writes (batched as one "apply" per handler turn or frame, since writes here are global at once), and keyed read tracking for layout-like clients.
- **Tests:** `SnapshotStateObserverTestsCommon` (31 without computed state), `SnapshotStateObserverTestsJvm` (3), the observer tests of `SnapshotTests` (about 10), `RecomposerTests.readDuringWithoutReadObservationDoesntCauseRecomposition`.
- **Depends on:** nothing. snapshot-flow and computed-state build on it.

### snapshot-flow (M): done (step S4 of docs/parity/design/snapshots.md; D768)
- **APIs:** `snapshot_flow(block) -> impl Stream<Item = T>`; `SnapshotFlowManager` *(exp)* to share one apply observer.
- **Does:** a stream emitting `block`'s result whenever the state it read changes, coalescing changes between polls.
- **Tests:** `SnapshotFlowTests` (9).
- **Depends on:** snapshot-observation; a `Stream` trait (a `futures-core` dependency, or compose-rs's own) — a dependency decision.

### flow-adapters (S): done (step S4; D768)
- **APIs:** `collect_as_state(stream, initial)` (with a policy), and a `StateFlow` analogue (a watch-like value stream) with `collect_as_state(state_stream)`.
- **Tests:** `CollectAsStateTests` (4), integration `FlowAdapterTest` (14).
- **Depends on:** produce-state-scope; the same `Stream` decision as snapshot-flow.

### composition-api (M): done (D1300, D1301, D1303)
- **APIs:** `Composition::dispose` and `is_disposed` (besides `Drop`); `ReusableComposition` (`set_content_with_reuse`, `deactivate`); the standalone `PausableComposition` (`set_pausable_content`, `set_pausable_content_with_reuse`, returning a `PausedComposition`); the parts of `ControlledComposition` a host needs (`invalidate_all`, `is_composing`, `has_pending_changes`, composing and applying as separate steps); the name `current_composite_key_hash_code`.
- **Tests:** `CompositionReusingTests` (about 6), `PausableCompositionTests` (about 12 built on `PausableComposition(applier, parent)`), `CompositionTests.disposedCompositionShouldReportAsDisposed`.
- **Depends on:** nothing left.

### recomposer (M): done but moving the hosts onto it (D1301)
- **APIs:** `Recomposer` (state machine `Inactive`/`InactivePendingWork`/`Idle`/`PendingWork`/`ShuttingDown`/`ShutDown`, `run_recompose_and_apply_changes` as a future, `await_idle`, `cancel`, `close`, `join`, `has_pending_work`, `change_count`, `running_recomposers`), `RecomposerInfo`, `with_running_recomposer`.
- **Does:** recomposes its compositions on the frame clock, with a lifecycle hosts and tests can observe. Hosts still loop over `Composition::recompose` until C4.
- **Tests:** `RecomposerTests` (ported, but the two by threads and `testSubcompositionDisposedInParent`), `RecomposerTests.jvm` (1), integration `RecomposerTests.runningRecomposerFlow`; the registration observer tests wait for `observe`.
- **Depends on:** the hosts' step (C4). For it: effects of a recomposer whose loop isn't running get frames from the host where Compose's wait for the loop's clock, and pausing the default recomposer holds nothing, as its tasks have no owner (`WindowRecomposer.android.kt:402` pauses on stop).

### frame-clocks (S): done
- The `MonotonicFrameClock` trait, `ExecutorFrameClock`, `BroadcastFrameClock`, `PausableMonotonicFrameClock`; tests in crates/compose-runtime/tests/suite/frame_clocks.rs. The recomposer pauses its effects' frames itself, on the executor's clock (D1301).

### frame-end-callback (S): done (D1301)
- **APIs:** `schedule_frame_end_callback(action) -> CancellationHandle` on the composer, a composition context and the recomposer.
- **Does:** runs `action` at the end of the frame `perform_frame` runs, after its work, asking for a frame if idle, held while its clock is paused. Refused once it's shut down.
- **Tests:** `RecomposerFrameEndSchedulingTests` (3 of 4; 1 needs movable-content).
- **Depends on:** the hosts' step (C4): on the default recomposer, nothing runs `perform_frame` until then.

### movable-content (L; XL with moves across compositions): done in a composition (D1305), across them at C6
- **APIs:** `movableContentOf` with 0 to 4 parameters, `movableContentWithReceiverOf` with 0 to 3.
- **Does:** a composable lambda whose groups, state and nodes move with it when it's called from another place (another parent, another branch, another composition) in the same frame, instead of being disposed and recreated.
- **Tests:** `MovableContentTests` (49), `AndroidInstrumentedMovableContentTests` (4), `AndroidMovableContentTests` (1), and 1-2 tests each in `CompositionAndDerivedStateTests`, `CompositionReusingTests`, `PausableCompositionInstrumentedTests`, `ActivityRecreationTest`, `RecomposerFrameEndSchedulingTests`, `RetainTests`.
- **Depends on:** composition-context for moves between compositions and subcompositions; the slot table's group moves (exist within one parent today) must cross parents, with nodes detached and attached (`Applier::create`/`attach` exist for paused content). Apps use it for content that moves between layouts (adaptive one-pane and two-pane screens).

### composition-context (L): done; `SubcomposeLayout` on it is C4 of the design (D1303, D1304)
- **APIs:** `rememberCompositionContext`, `CompositionContext`, the `parent` of `Composition`/`ControlledComposition`/`ReusableComposition`/`PausableComposition`.
- **Does:** child compositions (subcompositions) that see their parent's locals, recompose in the parent's frame after it, and are their owners' to dispose: a parent going ends none of them, as in Compose, its place going only keeping them out of that frame.
- **Tests:** about 8 of `CompositionTests`, 7 of `CompositionLocalTests`, 2 of `CompoundHashKeyTests`, 1 of `CompositionReusingTests`, `CompositionInstanceTests`, parts of `MovableContentTests` and `PausableCompositionTests`.
- **Depends on:** nothing left here. Lazy items, `BoxWithConstraints` and popups compose in one composition until steps C4 and C8 (docs/parity/design/composition-context.md); `SubcomposeLayout` is ui's, on `platform::subcomposition`.

### retain (L): done (D1306)
- **APIs:** see the API table (`runtime-retain`, stable in Compose).
- **Does:** `retain { … }` keeps a value while its content leaves the composition temporarily (a screen in the back stack, an activity recreation), as long as the enclosing `RetainedValuesStore` retains exited values; `RetainObserver` gets retained/entered/exited/retired/unused callbacks; `RetainedEffect` is a `DisposableEffect` whose cleanup runs when the value is retired, not when its content exits.
- **Tests:** `RetainTests` (33), `RetainedEffectTest` (4).
- **Depends on:** local-variants (the store is a local; nothing beyond today's locals strictly), movable-content (2 tests), pausable composition (exists). Navigation 3 doesn't retain its entries, a `SaveableStateHolder` keeping what's saved: its users are the apps'.

### snapshot-isolation (XL): done but tooling (steps S5 and S6 of docs/parity/design/snapshots.md, D769-D795)
- **APIs:** see the API table: snapshots, mutable snapshots, apply and merge, state records, `SnapshotContextElement`, snapshot tooling observers *(exp)*.
- **Does:** MVCC state: reads in a snapshot see a consistent view, writes in a mutable snapshot are invisible until applied, conflicts merge by policy or fail.
- **Tests:** `SnapshotTests` (89), `SnapshotContextElementTests` (2), the snapshot tests of the state-collection and derived suites: ported. Waiting: `SnapshotObserverTests` (14, step S7) and the 5 isolation tests of `ComputedSnapshotStateTests` (with computed-state). Composition in a snapshot has its own tests (crates/compose-runtime/tests/suite/composition_snapshot.rs), with `stateChangesDuringApplyChangesAreNotifiedBeforeFrameFinished` and `removedScopeInvalidationsAreCleared` (now in a mutable snapshot) among the ported.
- **Depends on:** nothing now; S7 next.

### composition-observer (M, experimental)
- **APIs:** `CompositionObserver`, `CompositionObserverHandle`, `ObservableComposition`, `Composition.setObserver`, `CompositionRegistrationObserver`, `Recomposer.observe`, `RecomposerInfo.observe`.
- **Tests:** `CompositionObserverTests` (17), `AndroidCompositionObserverTests` (1), `CompositionRegistrationObserverTest` (9), `CompositionRegistrationObserverWithUnconfinedDispatcherTest` (1).
- **Depends on:** recomposer for registration; composition-context and movable-content for 6 of the 17.

### error-traces (M)
- **APIs:** `ComposeStackTraceMode`, `Composer.setDiagnosticStackTraceMode`, `CompositionErrorContext`, `LocalCompositionErrorContext`.
- **Does:** a panic from composition, `remember`, an effect or a node update carries the path of composables to it (names and source locations).
- **Tests:** `ErrorTraceTests` (33).
- **Depends on:** a table from group keys to names and locations (the transform knows them; keys hash the declaration path); movable-content for 5 tests.

### composition-data (M)
- **APIs:** `CompositionData`, `CompositionGroup`, `CompositionInstance`, `findCompositionInstance`, `LocalInspectionTables`, `CompositionServices`.
- **Does:** tooling's read-only view of the slot table (Layout Inspector).
- **Tests:** `CompositionDataTests` (10), `CompositionInstanceTests` (5).
- **Depends on:** composition-context for instances' parents; the group-name table of error-traces.

### composition-tracing (S)
- **APIs:** `Composer.setTracer`, `CompositionTracer`, `isTraceInProgress`, `traceEventStart`, `traceEventEnd`.
- **Does:** a section per composable body in system traces (Perfetto on Android), which docs/12's measurements would use.
- **Tests:** none in scope (`runtime-tracing`'s tests are Android plumbing).
- **Depends on:** the group-name table of error-traces.

### Suggested order

1. **Quick wins that unblock ports:** keyed-side-effect (S, done), produce-state-scope (S, done), state-gaps (S, done), compose-node (S, done), frame-clocks (S, done).
2. **local-variants (M, done)**; **host-defaults (S)** next, with the ui area's hosts.
3. **state-collections (M)**, then **saveable-gaps (M)**: the most-used state APIs apps miss.
4. **snapshot-observation (M)**, then **computed-state (M)**; then the `Stream` decision and **snapshot-flow (M)** with **flow-adapters (S)**.
5. **composition-api (M)**, **recomposer (M)**, **frame-end-callback (S)**: host-level shape, unblocking `RecomposerTests` and the reuse and pausable API tests.
6. **movable-content (L)**, in a composition done, then **retain (L)**: done.
7. **composition-context (L)**: done (it unblocks movable content across compositions and the ui area's `SubcomposeLayout`).
8. **host-content-groups (M)**, when its decision is made, before more tests are ported through `compose_test(|| …)` with branches.
9. **error-traces (M)**, **composition-tracing (S)**, **composition-observer (M, exp)**, **composition-data (M)**, **composable-annotations (S)**.
10. **snapshot-isolation (XL)**: S7 of its design remains; **remember-serializable** if serde is accepted.

## Notes for the porting agents

### Where ported tests go

- Runtime tests go in `crates/compose-runtime/tests/suite/` (D710), on the shared kit `mock.rs` (as Compose's `runtime-test-utils`): composables written with `#[composable]` work there through `extern crate compose_runtime as compose;`, and the suite links the runtime only. Hand-expanded composables (scenarios.rs's style) stay for what the transform can't express. UI-level runtime tests (effects with frames through a host, saveable state across hosts) go in `crates/compose-foundation/tests/suite/` with `compose::test`, as effects.rs and platform.rs do.
- One module per Compose test file, named after it (`composition.rs` for `CompositionTests`), each test citing the one it adapts on the line above it (`// Compose: compose/runtime/…/CompositionTests.kt, testInsertAContact`), written from the scenario, not the Kotlin.

### How Compose's patterns map

| Compose (`runtime-test-utils`, `compositionTest`) | compose-rs |
|---|---|
| `compositionTest { compose { … } }` with a `ViewApplier` over mock `View`s | `let mut test = composition_test(); test.compose(content);` (mock.rs: `ViewApplier` over `View`s with a name and attributes) |
| `validate { Linear { Text("a") } }`, `revalidate()`, `skip()` | `test.validate(\|v\| v.linear(\|v\| v.text("a")))` (an `Expected` builder; `'static`, kept for `revalidate`), `v.skip(n)` |
| `expectChanges()`, `expectNoChanges()`, `advance()`, `advanceCount()`, `advanceTimeBy` | the same on `CompositionTest` (`expect_changes`, `advance_count`, `advance_time_by`): frames on the manual clock, each dispatching frame awaiters, then a pass; "changes" are what the passes did to the views |
| `verifyConsistent()` | `test.verify_consistent()` (`Composition::validate`) |
| `Snapshot.sendApplyNotifications()`, `Snapshot.global {}` | nothing: writes invalidate at once |
| `var x by mutableStateOf(0)` | `let x = mutable_state_of(0);` kept alive (it's the owner), `x.set(…)`, `x.get()`; capture `x.handle()` in closures. `*mutable_state_of(0)` is stale at once |
| a captured `var log = mutableListOf<String>()`, `var count = 0`, `var scope: RecomposeScope?` | mock.rs's `Log`, `Counter`, `ScopeHolder` (shared, compared by identity, so composables can take them) |
| `remember { object : RememberObserver {…} }` | `remember_observer(calc)`: plain `remember` never calls `RememberObserver` |
| `remember(a, b) { … }`, `key(a, b) { … }` | `remember_keyed((a, b), calc)`, `key((a, b), content)` |
| `currentRecomposeScope` | `current_recompose_scope()` |
| `Text`, `Linear`, `Edit`, `SelectBox`, `Repeated`, `Wrap`, `contact`/`SelectContact` | mock.rs's emitters of the same names (`Repeated` takes the item's key) |
| `TestMonotonicFrameClock`, `advanceTimeBy` in `runTest` | the driver's manual clock (`set_manual_clock`, frames every 16.67 ms), or `platform::advance_clock`, `fire_timers`, `run_ready`, `dispatch_frame` by hand |
| `ComposeTestRule`: `setContent`, `waitForIdle`, `mainClock.autoAdvance`, `advanceTimeBy`, `advanceTimeByFrame`, `currentTime` | `compose::test::compose_test(content)`, `wait_for_idle`, `set_auto_advance`/`with_manual_clock`, `advance_time_by`, `advance_time_by_frame`, `current_time_millis`; `recomposition_counts()` for counts |
| `assertFailsWith { … }` around composition | `std::panic::catch_unwind`, then `composition.is_poisoned()`: a panic poisons the composition, it can't continue |

### What the harness lacks

> Since the harness's milestone 1 (D900–D905), compose-test has Compose's clock model (rounding, frames on the test clock, `advance_time_until`, `wait_until`, the infinite-animation policy), bounds and displayed assertions, matchers over the tree and selectors (`on_children`, `on_parent`, collections), the touch, mouse (primary button) and key injection scopes, `perform_scroll_to`, `capture_to_image` with `assert_pixels`, and `print_to_string`; `perform_click` is a touch click. The notes in this section and the mapping above that call these missing are out of date: docs/09 lists the API, and [ui-text-and-test](ui-text-and-test.md) what is still missing. Existing tests that relied on the old timing gained a frame where they wait for a recomposition (D900).

- **Async plumbing for effect tests:** done in the runtime kit (D710): crates/compose-runtime/tests/suite/mock.rs has a test `Channel` (queued or conflated, `send`, `try_receive`, a `receive` future) for Compose's `Channel`s and `CompletableDeferred`s.
- **Test-rule waits:** done in compose-test (D900); the runtime kit's `CompositionTest` has `advance`, `advance_count`, `advance_ignoring_pending_work` and `advance_time_by` on the manual clock.
- **Recreation:** crates/compose-runtime/tests/suite/restoration.rs has a `RestorationTester` (Compose's `StateRestorationTester`: the registry saved to bytes, the composition recreated); saveable tests that recreate an activity in a UI host map to saving a `UiHost`'s state and composing it in a new host, as crates/compose-foundation/tests/suite/platform.rs's `remember_saveable_comes_back_in_a_new_host` does.

### Performance of milestone 1

Measured against the fork point, alternating saved bench binaries (the machine was shared and loaded, so medians and minimums of many rounds are compared):

- **Runtime benches:** within ±2% but for `state get` (−14%) and 1,000 local reads (−13%). D715's branch deletion first cost keyed rotation about 9% (a second node operation returned from every `start_group`); it is a cold function now, whose removal waits in the runtime for `start_branch_group`. `1000 keyed rows/reverse`, suspected of +8%, measured −1% over 20 interleaved rounds in both orders (medians 1,436 vs 1,428 µs): the larger destructure in `start_group` and the per-group `slot_position` write don't show.
- **The heavy navigation screen's first frame** (`benches/ui.rs`, `navigation`) looked 4–9% slower under load. It isn't: the review's deterministic probe has the branch about 8% faster than main, with the same group and allocation counts. Not a regression.

### Behaviour that likely differs from Compose

1. **`derived_state_of` computed eagerly**: fixed (D711), it's lazy with conditional invalidation, as Compose's, and on records (D759).
2. **Writes during composition panicked in debug builds** (D15, D40): now allowed as in Compose (D703, D712); `testImminentInvalidationForCurrentGroup` and its siblings are ported.
3. **Composition locals** compared provided values with `ComposeArg::same` and were all dynamic: they now have Compose's kinds (D713); `new` compares by `same` (D97), `with_policy` by a policy, as `compositionLocalOf(policy)`.
4. **Default arguments are evaluated in the caller.** The call macro fills defaults at the call site, so a default reading state or a local (`#[default(LOCAL_X.current())]`) subscribes the caller, where Compose evaluates defaults in the callee's own group (`startDefaults`). Caught by `CompositionTests.testRestartOfDefaultFunctions`, `recompositionClearsObservationsWhenDefaultParametersSkip`, `remember_defaultParamInRestartableFunction` once their counts are ported.
5. **`remember_saveable` keeps its first saver and registration.** Compose saves with the latest saver passed and re-registers when the key changes: `RememberSaveableTest.theLatestPassedSaverIsUsed`, `reregisterWhenTheKeyIsChanged`.
6. **Content lambdas have no restart scope of their own** (docs/guide/differences.md): a state read inside a content block recomposes the composable that invokes it. Ports asserting a lambda recomposes alone (`CompositionTests.testSkippingNestedLambda`-style counts, `RestartTests` if counts are added) will see the caller recompose too.
7. **The composite key hash** uses per-composable keys plus an ordinal among same-key siblings, and stops at a `SaveableStateProvider` (D290); Compose hashes per call site from the root. `CompoundHashKeyTests`' uniqueness and stability tests pass (crates/compose-runtime/tests/suite/compound_hash_key.rs); values themselves differ.
8. **`key(…)` has no call-site component** (docs/04, "Known limitations"): two `key` sites in one parent with equal values can match each other's groups. No Compose test targets it directly; a two-site variant of `CompoundHashKeyTests.uniqueKeysGenerateUniqueCompositeKeys` would show it.
9. **`produce_state`'s producer closure ran at every composition**: fixed, it runs when its coroutine starts.
10. **A panic poisons the composition.** `PausableCompositionTests.pausableComposition_throwIn*` and `…_invalidStatePreservesOriginalException_*` expect the composition to report the original exception and stay usable in some cases; compose-rs refuses further passes and releases everything on drop.
11. **Plain `remember` of a `RememberObserver` gets no callbacks** (no specialization): Compose tests remembering an observer through `remember {}` must use `remember_observer`.
12. **Conditional content was forgotten after the values replaced later in its parent**: fixed (D714, D715). Values are forgotten in the reverse of table order: a removed subtree's and a group's unreached slots interleave with its children by position, and a conditional's branch groups share a key, a branch starting where another's group is deleting it there, as Compose's `startReplaceGroup` (an `if` without `else` gets an empty `else` group, as Compose's compiler).
13. **Nodes are applied during the pass** (D31): Compose records node changes and applies them after composing, so a node hears `onReuse` in the apply stage; compose-rs resets a reused node as the pass reaches it, before its setters (removals, deactivations and releases still wait for the end of the pass). `CompositionReusingTests.onReuseIsCalledInApplyStage` shows it.
14. **`remember_updated_state` compares by `ComposeArg::same`**, where Compose's `rememberUpdatedState` writes a `mutableStateOf` (structural, `equals`). Its usual value is a callback, which has no `PartialEq` in Rust (a Kotlin lambda's `equals` is identity, as `same` is), and Rust can't pick `PartialEq` where a type has it; `same` is `==` for small values, so the difference is shared data: a new `Rc` of equal contents counts as a change and recomposes the readers once.
15. **A task waiting for a frame resumes before that frame's composition**, where Compose dispatches a frame's continuations after its recompose and apply: a task changing state at a frame is composed in the same frame in compose-rs, a frame later in Compose. Found by the animation area (D817, D821): a `SeekableTransitionState` call can end a frame before Compose's, and an `AnimatedContent` port waits one more frame. To do after the snapshot work (S1, which rewrites `dispatch_frame` and the apply notifications): run `on_frame` in `dispatch_frame` and park the continuations until after the composition and its apply, with the per-frame work moved into `on_frame`. The tests encoding today's order say so: `crates/compose-animation/tests/suite/animated_content_scope.rs` (`a_content_kept_by_its_key_keeps_its_saved_state`) and `crates/compose-animation/tests/suite/seekable.rs` (`a_joined_value_goes_on_to_a_third_state`).
