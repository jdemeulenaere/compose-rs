# Parity ledger: lifecycle and ViewModel

The inventory of AndroidX's lifecycle libraries against compose-rs's `compose-lifecycle`, D704: the model Navigation 3's entries need, and apps use.

## Scope

- **Reference:** the androidx checkout, under `frameworks/support/lifecycle` (its tip). Every path below is relative to it.
- **Modules:** `lifecycle-common`, `lifecycle-runtime`, `lifecycle-runtime-compose`, `lifecycle-runtime-testing`, `lifecycle-viewmodel`, `lifecycle-viewmodel-compose`, later `lifecycle-viewmodel-navigation3`. `lifecycle-viewmodel-savedstate` and its `SavedStateHandle` with the saved-state work.
- **Left out, n/a:** LiveData, `lifecycle-service`, `lifecycle-process`, the reflective and generated observers (`lifecycle-compiler`, annotations: deprecated in AndroidX too), the lint modules, `ViewTree*` (Android's view tree: `ComposeRsView` reads it in Kotlin).

**Rust-shaped** (D837): an owner is the `Lifecycle` handle itself, so `LOCAL_LIFECYCLE_OWNER` and the effects take a `Lifecycle`, and `LifecycleOwner` says how to get one. No flows exist in compose-rs, so `currentStateFlow` and `currentStateAsState()` are `Lifecycle::as_state`; `eventFlow`, `flowWithLifecycle` and `collectAsStateWithLifecycle` wait for flows.

## Test files

| Compose test file | @Test | Status | Notes |
|---|---|---|---|
| `lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt` | 26 | ported | `compose-lifecycle/tests/suite/registry.rs` |
| `lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FastSafeIterableMapTest.kt` | 8 | n/a | an internal map: the registry's list walks by order of addition, checked by the registry's tests |
| `lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/WithLifecycleStateTest.kt` | 4 | ported | `with_state.rs` |
| `lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt` | 15 | partial | 12 ported, `repeat_on_lifecycle.rs` (the `NonCancellable` tear-down: a dropped future has none to wait for); n/a: the 3 with a `withContext(customJob)` or a dispatcher, no such things here |
| `lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt` | 12 | `feature:flows` | |
| `lifecycle-runtime/src/androidDeviceTest/kotlin/androidx/lifecycle/PausingDispatcherTest.kt` | 24 | n/a | deprecated `launchWhen*` and its dispatcher |
| `lifecycle-runtime/src/androidDeviceTest/kotlin/androidx/lifecycle/LaunchWhenTest.kt` | 1 | n/a | deprecated `launchWhen*` |
| `lifecycle-runtime/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewTreeLifecycleOwnerTest.kt` | 5 | n/a | Android's view tree |
| `lifecycle-runtime/src/androidDeviceTest/kotlin/androidx/lifecycle/MissingClassTest.kt`, `androidHostTest/kotlin/NoPackageTest.kt`, `nativeTest/.../NativeLifecycleRegistryTest.kt`, `desktopTest/.../MainDispatcherCheckerTest.kt` | 6 | n/a | class loading, threads and dispatchers of other platforms: a `Lifecycle` isn't `Send` |
| `lifecycle-common/src/jvmTest/java/androidx/lifecycle/DefaultLifecycleObserverTest.kt` | 2 | ported | `observers.rs` |
| `lifecycle-common/src/jvmTest/java/androidx/lifecycle/LifecyclingTest.kt`, `ReflectiveGenericLifecycleObserverTest.kt` | 26 | n/a | reflection and generated observers |
| `lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt` | 25 | ported | `effects.rs` |
| `lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt` | 10 | ported | `drop_unless.rs` |
| `lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt` | 11 | ported | `owner.rs` |
| `lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleExtTest.kt` | 1 | ported | `owner.rs`, with the change Compose's leaves commented out |
| `lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt` | 9 | `feature:flows` | |

## Public API

| API | compose-rs | |
|---|---|---|
| `Lifecycle` (`currentState`, `addObserver`, `removeObserver`, `addObserver { … }`), `Lifecycle.State`, `Lifecycle.Event` (`upFrom`, `upTo`, `downFrom`, `downTo`, `targetState`) | present: `Lifecycle` (a handle), `LifecycleState`, `LifecycleEvent` | D837 |
| `LifecycleOwner`, `LifecycleOwner(lifecycle)`, `LifecycleEventObserver`, `DefaultLifecycleObserver`, `LifecycleObserver` | present: `LifecycleObserver` is the event observer (a closure is one), `DefaultLifecycleObserverAdapter` for the other | D837 |
| `LifecycleRegistry` (`currentState =`, `handleLifecycleEvent`, `observerCount`, `createUnsafe`) | present, `new` (no thread to check) | D837 |
| `Lifecycle.coroutineScope`, `lifecycleScope`, `repeatOnLifecycle`, `withStateAtLeast`, `withCreated`, `withStarted`, `withResumed`, `LifecycleDestroyedException` | present, the last a result's error `LifecycleDestroyed` | D837 |
| `LocalLifecycleOwner`, `LifecycleEventEffect`, `LifecycleStartEffect`, `LifecycleResumeEffect`, their scopes and results, `dropUnlessStarted`, `dropUnlessResumed`, `rememberLifecycleOwner`, `currentStateAsState` | present; keys are one `ComposeArg` (a tuple for several); `drop_unless_*` take the owner (`&LOCAL_LIFECYCLE_OWNER.current()` for Compose's default) | D837 |
| `TestLifecycleOwner` | present | D837 |
| `currentStateFlow`, `eventFlow`, `flowWithLifecycle`, `collectAsStateWithLifecycle` | missing | feature:flows |
| `LifecycleCoroutineScope.launchWhen*`, `LifecycleRegistry.markState`, `LifecycleRegistryOwner` | n/a | deprecated |

## Hosts

`UiHost` provides its `Lifecycle` as `LOCAL_LIFECYCLE_OWNER`, destroyed once its composition is disposed: `set_lifecycle_state` moves it. On Android, `ComposeRsView` follows its view tree's `LifecycleOwner`, else its activity's callbacks. On the desktop, a window is resumed focused, started in the background, created minimized or hidden. Under a test rule, it's resumed, as a test's activity.
