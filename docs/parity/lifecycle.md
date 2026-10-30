# Parity ledger: lifecycle and ViewModel

The inventory of AndroidX's lifecycle libraries against compose-rs's `compose-lifecycle`, D704: the model Navigation 3's entries need, and apps use.

## Scope

- **Reference:** the androidx checkout, under `frameworks/support/lifecycle` (its tip). Every path below is relative to it.
- **Modules:** `lifecycle-common`, `lifecycle-runtime`, `lifecycle-runtime-compose`, `lifecycle-runtime-testing`, `lifecycle-viewmodel`, `lifecycle-viewmodel-compose`, later `lifecycle-viewmodel-navigation3`. `lifecycle-viewmodel-savedstate`: its `ViewModelStoreProvider` is ported (D838); `SavedStateHandle` and the saved-state parameters with the saved-state work.
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
| `lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt` | 12 | ported | `view_model.rs` |
| `lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelStoreTest.kt` | 6 | ported | `view_model.rs` |
| `lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderTest.kt` | 8 | partial | 7 ported, `view_model.rs`; n/a: `localViewModel`, a local Kotlin class |
| `lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderGetTest.kt`, `ViewModelLazyTest.kt` | 2 | ported | `view_model.rs`, one test: no `viewModels()` delegate in Rust, `get::<T>()` is it |
| `lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelInitializerTest.kt` | 3 | ported | `view_model.rs` |
| `lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/internal/HasDefaultViewModelProviderFactoryTest.kt` | 4 | ported | `view_model.rs` |
| `lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt` | 14 | ported | `view_model.rs` |
| `lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewModelScopeTest.kt` | 7 | partial | 6 ported, `view_model.rs`; n/a: `launchesSupervisedJobs`, tasks have no exceptions to fail a sibling with |
| `lifecycle-viewmodel/src/androidHostTest/kotlin/androidx/lifecycle/NewInstanceFactoryTest.kt`, `jvmAndAndroidTest/.../DefaultViewModelProviderFactoryTest.kt`, `nativeTest/.../DefaultViewModelProviderFactoryTest.kt` | 9 | n/a | reflection: its absence is D838, checked by a panic in `view_model.rs` |
| `lifecycle-viewmodel/src/androidHostTest/kotlin/androidx/lifecycle/MockViewModelTest.kt`, `MockViewModelStoreTest.kt` | 3 | n/a | mocking a final class |
| `lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/AndroidViewModelProviderTest.kt`, `AndroidViewModelFactoryTest.kt` | 8 | n/a | `AndroidViewModel`'s `Application` |
| `lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewTreeViewModelStoreOwnerTest.kt` | 6 | n/a | Android's view tree |
| `lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt` | 19 | partial | 13 ported, `view_model_compose.rs`; 6 of saved-state owners and default arguments: `feature:saved-state` |
| `lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt` | 11 | ported | `view_model_compose.rs`; the first through a local of `None`, a dialog's composition having a provider here |
| `lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreProviderTest.android.kt` | 8 | partial | 7 ported, `view_model_compose.rs`; `withDefaultArgs`: `feature:saved-state` |
| `lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreOwnerTest.android.kt` | 8 | partial | 6 ported, `view_model_compose.rs`, the configuration change with a store kept across two hosts; 2: `feature:saved-state` |
| `lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/SavedStateHandleSaverTest.android.kt` | 9 | `feature:saved-state` | |
| `lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelInComponentActivityTest.android.kt`, `ViewModelInAppCompatActivityTest.android.kt`, `ViewModelInFragmentTest.android.kt` | 5 | n/a | a device's activities and fragments; `ComposeRsView`'s side is D838 (built, not run) |

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
| `ViewModel` (`onCleared`, `addCloseable`, keyed, `getCloseable`), `AutoCloseable`, `viewModelScope` | present: a trait over `ViewModelBase` | D838 |
| `ViewModelStore`, `ViewModelStoreOwner`, `HasDefaultViewModelProviderFactory` (its methods are the owner's, by default the same) | present | D838 |
| `ViewModelProvider` (`create`, `get`, keyed), `.Factory`, `viewModelFactory`, `initializer`, `CreationExtras`, `MutableCreationExtras` (one type), `VIEW_MODEL_KEY` | present | D838 |
| `LocalViewModelStoreOwner`, `viewModel` (`ViewModelArgs`, `view_model_with`), `rememberViewModelStoreOwner`, `rememberViewModelStoreProvider`, `ViewModelStoreProvider` | present, without the saved-state parameters | D838 |
| `SavedStateHandle`, `SavedStateViewModelFactory`, `savedStateHandle`, `saved` | missing | feature:saved-state |
| `AndroidViewModel`, `NewInstanceFactory`, `AndroidViewModelFactory`, `viewModels()` delegates, `ViewTreeViewModelStoreOwner` | n/a | D838: no reflection, nor an `Application`; Kotlin reads the view tree |

## Hosts

`UiHost` provides its `Lifecycle` as `LOCAL_LIFECYCLE_OWNER`, destroyed once its composition is disposed: `set_lifecycle_state` moves it. On Android, `ComposeRsView` follows its view tree's `LifecycleOwner`, else its activity's callbacks. On the desktop, a window is resumed focused, started in the background, created minimized or hidden. Under a test rule, it's resumed, as a test's activity.

`UiHost` also provides a store as `LOCAL_VIEW_MODEL_STORE_OWNER`, cleared last when it goes. On Android, one kept in the view tree's `ViewModelStoreOwner` if it has one, so its models outlive a configuration change (`set_retained_view_model_store`, D838: built and linted, not run on a device).
