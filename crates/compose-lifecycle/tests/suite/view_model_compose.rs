//! `view_model`, `remember_view_model_store_owner` and `remember_view_model_store_provider` against
//! AndroidX's tests of the same names in `lifecycle-viewmodel-compose`.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_lifecycle::*;
use compose_runtime::*;
use compose_test::*;

#[derive(Default)]
struct TestViewModel {
    base: ViewModelBase,
    extras: Option<CreationExtras>,
    is_cleared: Cell<bool>,
}

impl ViewModel for TestViewModel {
    fn base(&self) -> &ViewModelBase {
        &self.base
    }
    fn on_cleared(&self) {
        self.is_cleared.set(true);
    }
}

/// A factory recording that it was called and with what.
#[derive(Default)]
struct FakeFactory {
    create_called: Cell<bool>,
    extras: RefCell<Option<CreationExtras>>,
}

impl ViewModelProviderFactory for FakeFactory {
    fn create(&self, _: ModelClass, extras: &CreationExtras) -> Option<Rc<dyn ViewModel>> {
        self.create_called.set(true);
        *self.extras.borrow_mut() = Some(extras.clone());
        Some(Rc::new(TestViewModel::default()))
    }
}

#[derive(Default)]
struct FakeOwner {
    store: ViewModelStore,
    factory: Rc<FakeFactory>,
}

impl ViewModelStoreOwner for FakeOwner {
    fn view_model_store(&self) -> ViewModelStore {
        self.store.clone()
    }
    fn default_view_model_provider_factory(&self) -> Rc<dyn ViewModelProviderFactory> {
        self.factory.clone()
    }
}

type Held<T> = Rc<RefCell<Option<Rc<T>>>>;

fn with_owner(owner: &Rc<FakeOwner>, content: impl FnOnce()) {
    let owner: ViewModelStoreOwnerRef = owner.clone();
    CompositionLocalProvider(LOCAL_VIEW_MODEL_STORE_OWNER.provides(Some(owner)), content);
}

fn manually(owner: &Rc<FakeOwner>, key: Option<&str>) -> Rc<TestViewModel> {
    let provider = ViewModelProvider::of(&**owner);
    match key {
        Some(key) => provider.get_keyed(key),
        None => provider.get(),
    }
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, viewModelCreatedViaDefaultFactory
#[test]
fn a_view_model_comes_from_the_local_owner_s_factory() {
    let owner = Rc::new(FakeOwner::default());
    let created: Held<TestViewModel> = Rc::default();
    let (held, seen) = (owner.clone(), created.clone());
    let _ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || *seen.borrow_mut() = Some(view_model::<TestViewModel>()));
    });
    assert!(owner.factory.create_called.get());
    assert!(Rc::ptr_eq(created.borrow().as_ref().unwrap(), &manually(&owner, None)));
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, viewModelCreatedViaDefaultFactoryWithKey
#[test]
fn a_view_model_with_a_key_is_kept_under_it() {
    let owner = Rc::new(FakeOwner::default());
    let created: Held<TestViewModel> = Rc::default();
    let (held, seen) = (owner.clone(), created.clone());
    let _ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || {
            *seen.borrow_mut() = Some(view_model_args::<TestViewModel>(ViewModelArgs {
                key: Some("test".to_string()),
                ..Default::default()
            }));
        });
    });
    assert!(owner.factory.create_called.get());
    assert!(Rc::ptr_eq(created.borrow().as_ref().unwrap(), &manually(&owner, Some("test"))));
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, viewModelCreatedViaDefaultFactoryWithKeyAndCreationExtras
#[test]
fn a_view_model_s_extras_reach_its_factory() {
    let owner = Rc::new(FakeOwner::default());
    let key = CreationExtrasKey::<String>::new();
    let mut extras = CreationExtras::new();
    extras.set(key, "value".to_string());
    let held = owner.clone();
    let _ui = compose_test(move || {
        let extras = extras.clone();
        with_owner(&held, move || {
            view_model_args::<TestViewModel>(ViewModelArgs {
                key: Some("test".to_string()),
                extras: Some(extras),
                ..Default::default()
            });
        });
    });
    assert!(owner.factory.create_called.get());
    let passed = owner.factory.extras.borrow().clone().unwrap();
    assert_eq!(passed.get(key).as_deref(), Some("value"));
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, viewModelCreatedCreationExtrasInitializer
#[test]
fn an_initializer_makes_a_view_model_instead_of_the_factory() {
    let owner = Rc::new(FakeOwner::default());
    let key = CreationExtrasKey::<String>::new();
    let created: Held<TestViewModel> = Rc::default();
    let (held, seen) = (owner.clone(), created.clone());
    let _ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || {
            *seen.borrow_mut() = Some(view_model_with(Some("test"), move |extras| {
                let extras = CreationExtras::with(extras, |extras| {
                    extras.set(key, "value".to_string());
                });
                TestViewModel { extras: Some(extras), ..Default::default() }
            }));
        });
    });
    assert!(!owner.factory.create_called.get());
    let manual = manually(&owner, Some("test"));
    assert!(Rc::ptr_eq(created.borrow().as_ref().unwrap(), &manual));
    assert_eq!(manual.extras.as_ref().unwrap().get(key).as_deref(), Some("value"));
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, viewModelCreatedViaDefaultFactoryWithCustomOwner
#[test]
fn a_view_model_of_an_owner_given_uses_it_and_not_the_local_one() {
    let (custom, local) = (Rc::new(FakeOwner::default()), Rc::new(FakeOwner::default()));
    let created: Held<TestViewModel> = Rc::default();
    let (held, given, seen) = (local.clone(), custom.clone(), created.clone());
    let _ui = compose_test(move || {
        let (given, seen) = (given.clone(), seen.clone());
        with_owner(&held, move || {
            let owner: ViewModelStoreOwnerRef = given.clone();
            *seen.borrow_mut() = Some(view_model_args::<TestViewModel>(ViewModelArgs {
                view_model_store_owner: Some(owner),
                ..Default::default()
            }));
        });
    });
    assert!(!local.factory.create_called.get());
    assert!(custom.factory.create_called.get());
    assert!(Rc::ptr_eq(created.borrow().as_ref().unwrap(), &manually(&custom, None)));
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, customFactoryIsUsedWhenProvided
// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, defaultFactoryIsNotUsedWhenCustomProvided
#[test]
fn a_factory_given_is_used_and_not_the_default() {
    let owner = Rc::new(FakeOwner::default());
    let custom = Rc::new(FakeFactory::default());
    let (held, factory) = (owner.clone(), custom.clone());
    let _ui = compose_test(move || {
        let factory = factory.clone();
        with_owner(&held, move || {
            view_model_args::<TestViewModel>(ViewModelArgs {
                factory: Some(factory),
                ..Default::default()
            });
        });
    });
    assert!(custom.create_called.get());
    assert!(!owner.factory.create_called.get());
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, customFactoryProducerIsUsedWhenProvided
#[test]
fn a_factory_and_an_owner_given_are_used() {
    let owner = Rc::new(FakeOwner::default());
    let custom = Rc::new(FakeFactory::default());
    let (held, factory) = (owner.clone(), custom.clone());
    let _ui = compose_test(move || {
        let held: ViewModelStoreOwnerRef = held.clone();
        view_model_args::<TestViewModel>(ViewModelArgs {
            view_model_store_owner: Some(held),
            factory: Some(factory.clone()),
            ..Default::default()
        });
    });
    assert!(custom.create_called.get());
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, createdWithCustomFactoryViewModelIsEqualsToCreatedManually
// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, createdWithCustomFactoryViewModelIsEqualsToCreatedManuallyWithKey
#[test]
fn a_view_model_kept_is_found_through_another_factory() {
    let owner = Rc::new(FakeOwner::default());
    let created: Held<TestViewModel> = Rc::default();
    let keyed: Held<TestViewModel> = Rc::default();
    let (held, seen, seen_keyed) = (owner.clone(), created.clone(), keyed.clone());
    let _ui = compose_test(move || {
        let (seen, seen_keyed) = (seen.clone(), seen_keyed.clone());
        with_owner(&held, move || {
            *seen.borrow_mut() = Some(view_model());
            *seen_keyed.borrow_mut() = Some(view_model_args(ViewModelArgs {
                key: Some("test".to_string()),
                ..Default::default()
            }));
        });
    });
    let provider = ViewModelProvider::of_with(&*owner, Rc::new(FakeFactory::default()));
    assert!(Rc::ptr_eq(created.borrow().as_ref().unwrap(), &provider.get()));
    assert!(Rc::ptr_eq(keyed.borrow().as_ref().unwrap(), &provider.get_keyed("test")));
}

#[test]
fn a_host_provides_a_store_a_default_type_is_made_in() {
    let created: Held<TestViewModel> = Rc::default();
    let seen = created.clone();
    let mut ui = compose_test(move || *seen.borrow_mut() = Some(view_model()));
    let store = ui.host().view_model_store();
    assert_eq!(store.keys().len(), 1);
    drop(ui);
    assert!(created.borrow().as_ref().unwrap().is_cleared.get(), "cleared with the host");
}

#[test]
fn a_store_a_platform_keeps_outlives_its_host() {
    let store = ViewModelStore::new();
    let created: Held<TestViewModel> = Rc::default();
    let seen = created.clone();
    let mut ui = ComposeTestRule::empty(compose_ui::platform::UiHost::new(
        compose_ui::IntSize::new(10, 10),
        compose_ui::Density::new(1.0),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
    ));
    ui.host().set_retained_view_model_store(store.clone());
    ui.set_content(move || *seen.borrow_mut() = Some(view_model()));
    drop(ui);
    assert!(!created.borrow().as_ref().unwrap().is_cleared.get());
    assert_eq!(store.keys().len(), 1);
}

// ---- rememberViewModelStoreOwner -------------------------------------------------------------

/// A model in an owner of its own, while `show`.
#[composable]
fn InOwnStore(show: MutableState<bool>, seen: Held<TestViewModel>, key: &'static str) {
    if show.get() {
        let owner = remember_view_model_store_owner!();
        CompositionLocalProvider(LOCAL_VIEW_MODEL_STORE_OWNER.provides(Some(owner)), move || {
            *seen.borrow_mut() = Some(view_model_with(Some(key), |_| TestViewModel::default()));
        });
    }
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreOwnerTest.android.kt, rememberViewModelStoreOwner_whenComposableRemoved_clearsViewModel
#[test]
fn an_own_store_removed_clears_its_models() {
    let show = mutable_state_of(true);
    let current: Held<TestViewModel> = Rc::default();
    let (read, seen) = (show.handle(), current.clone());
    let mut ui = compose_test(move || InOwnStore!(read, seen.clone(), "vm"));
    ui.wait_for_idle();
    show.set(false);
    ui.wait_for_idle();
    assert!(current.borrow().as_ref().unwrap().is_cleared.get());
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreOwnerTest.android.kt, rememberViewModelStoreOwner_whenAppBackgroundedAndRemoved_clearsViewModel
#[test]
fn an_own_store_removed_in_the_background_clears_its_models() {
    let show = mutable_state_of(true);
    let current: Held<TestViewModel> = Rc::default();
    let (read, seen) = (show.handle(), current.clone());
    let mut ui = compose_test(move || InOwnStore!(read, seen.clone(), "vm"));
    ui.wait_for_idle();
    ui.host().set_lifecycle_state(LifecycleState::Created);
    show.set(false);
    ui.wait_for_idle();
    assert!(current.borrow().as_ref().unwrap().is_cleared.get());
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreOwnerTest.android.kt, rememberViewModelStoreOwner_whenSiblingRemoved_clearsIndependently
#[test]
fn a_sibling_s_own_store_removed_leaves_this_one() {
    let (keep_shown, show) = (mutable_state_of(true), mutable_state_of(true));
    let (keep, remove): (Held<TestViewModel>, Held<TestViewModel>) = (Rc::default(), Rc::default());
    let (always, read, seen_keep, seen_remove) =
        (keep_shown.handle(), show.handle(), keep.clone(), remove.clone());
    let mut ui = compose_test(move || {
        InOwnStore!(always, seen_keep.clone(), "vmKeep");
        InOwnStore!(read, seen_remove.clone(), "vmRemove");
    });
    ui.wait_for_idle();
    show.set(false);
    ui.wait_for_idle();
    assert!(remove.borrow().as_ref().unwrap().is_cleared.get());
    assert!(!keep.borrow().as_ref().unwrap().is_cleared.get());
}

/// The rule of a recreated "activity": `store` kept across hosts, its model in an own store.
fn activity(store: &ViewModelStore, seen: &Held<TestViewModel>) -> ComposeTestRule {
    let mut ui = ComposeTestRule::empty(compose_ui::platform::UiHost::new(
        compose_ui::IntSize::new(10, 10),
        compose_ui::Density::new(1.0),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
    ));
    ui.host().set_retained_view_model_store(store.clone());
    let seen = seen.clone();
    ui.set_content(move || {
        let show = remember(|| mutable_state_of(true));
        InOwnStore!(show, seen.clone(), "vm");
    });
    ui
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreOwnerTest.android.kt, rememberViewModelStoreOwner_whenConfigurationChanged_retainsViewModel
#[test]
fn an_own_store_outlives_a_configuration_change() {
    let store = ViewModelStore::new();
    let current: Held<TestViewModel> = Rc::default();
    let mut ui = activity(&store, &current);
    ui.wait_for_idle();
    let before = current.borrow().clone().unwrap();
    // As an activity recreated: its lifecycle is destroyed, then its composition goes.
    ui.host().set_lifecycle_state(LifecycleState::Destroyed);
    drop(ui);
    let mut ui = activity(&store, &current);
    ui.wait_for_idle();
    let after = current.borrow().clone().unwrap();
    assert!(Rc::ptr_eq(&before, &after));
    assert!(!after.is_cleared.get());
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreOwnerTest.android.kt, rememberViewModelStoreOwner_whenActivityDestroyed_clearsViewModel
#[test]
fn an_own_store_goes_with_its_host() {
    let current: Held<TestViewModel> = Rc::default();
    let seen = current.clone();
    let mut ui = compose_test(move || {
        let show = remember(|| mutable_state_of(true));
        InOwnStore!(show, seen.clone(), "vm");
    });
    ui.wait_for_idle();
    drop(ui);
    assert!(current.borrow().as_ref().unwrap().is_cleared.get());
}

/// Two owners of a provider, the provider while `show_parent`, the second while `show_child`.
#[composable]
fn HoistedProvider(
    show_parent: MutableState<bool>,
    show_child: MutableState<bool>,
    parent: Held<TestViewModel>,
    child: Held<TestViewModel>,
) {
    if show_parent.get() {
        let provider = remember_view_model_store_provider!();
        let owner = remember_view_model_store_owner_in!(provider.clone());
        CompositionLocalProvider(LOCAL_VIEW_MODEL_STORE_OWNER.provides(Some(owner)), || {
            *parent.borrow_mut() =
                Some(view_model_with(Some("parent"), |_| TestViewModel::default()));
        });
        if show_child.get() {
            let owner = remember_view_model_store_owner_in!(provider);
            CompositionLocalProvider(LOCAL_VIEW_MODEL_STORE_OWNER.provides(Some(owner)), || {
                *child.borrow_mut() =
                    Some(view_model_with(Some("child"), |_| TestViewModel::default()));
            });
        }
    }
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreOwnerTest.android.kt, rememberViewModelStoreOwner_hoistedProvider_isolatesScopesButDefersCleanup
#[test]
fn owners_of_a_hoisted_provider_are_cleared_when_the_provider_goes() {
    let (show_parent, show_child) = (mutable_state_of(true), mutable_state_of(true));
    let (parent, child): (Held<TestViewModel>, Held<TestViewModel>) =
        (Rc::default(), Rc::default());
    let (read_parent, read_child, seen_parent, seen_child) =
        (show_parent.handle(), show_child.handle(), parent.clone(), child.clone());
    let mut ui = compose_test(move || {
        HoistedProvider!(read_parent, read_child, seen_parent.clone(), seen_child.clone());
    });
    ui.wait_for_idle();
    let (parent_model, child_model) =
        (parent.borrow().clone().unwrap(), child.borrow().clone().unwrap());
    assert!(!Rc::ptr_eq(&parent_model, &child_model));
    show_child.set(false);
    ui.wait_for_idle();
    assert!(!parent_model.is_cleared.get() && !child_model.is_cleared.get());
    show_parent.set(false);
    ui.wait_for_idle();
    assert!(parent_model.is_cleared.get() && child_model.is_cleared.get());
}

// ---- rememberViewModelStoreProvider ----------------------------------------------------------

type HeldProvider = Rc<RefCell<Option<ViewModelStoreProvider>>>;

/// A provider while `show`, and another after it that stays, under `key` if there's one.
#[composable]
fn SiblingProviders(
    show: MutableState<bool>,
    key: Option<u64>,
    first: HeldProvider,
    second: HeldProvider,
) {
    if show.get() {
        let provider = match key {
            Some(key) => remember_view_model_store_provider!(key = Some(key)),
            None => remember_view_model_store_provider!(),
        };
        *first.borrow_mut() = Some(provider);
    }
    let provider = match key {
        Some(key) => remember_view_model_store_provider!(key = Some(key)),
        None => remember_view_model_store_provider!(),
    };
    *second.borrow_mut() = Some(provider);
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreProviderTest.android.kt, rememberViewModelStoreProvider_whenParentProvided_createsLinkedProvider
#[test]
fn a_provider_can_be_remembered_with_a_parent() {
    let provider: HeldProvider = Rc::default();
    let seen = provider.clone();
    let mut ui = compose_test(move || {
        let owner = LOCAL_VIEW_MODEL_STORE_OWNER.current();
        *seen.borrow_mut() = Some(remember_view_model_store_provider!(parent = owner));
    });
    ui.wait_for_idle();
    assert!(provider.borrow().is_some());
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreProviderTest.android.kt, rememberViewModelStoreProvider_whenComposableRecomposed_returnsSameInstance
#[test]
fn a_provider_stays_the_same_when_recomposed() {
    let trigger = mutable_state_of(0);
    let providers: Rc<RefCell<Vec<ViewModelStoreProvider>>> = Rc::default();
    let (read, seen) = (trigger.handle(), providers.clone());
    let mut ui = compose_test(move || {
        let _ = read.get();
        seen.borrow_mut().push(remember_view_model_store_provider!());
    });
    ui.wait_for_idle();
    trigger.set(1);
    ui.wait_for_idle();
    let providers = providers.borrow();
    assert_eq!(providers.len(), 2, "composed again");
    assert_eq!(providers[0], providers[1]);
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreProviderTest.android.kt, rememberViewModelStoreProvider_whenParentNull_createsRootProvider
#[test]
fn a_provider_without_a_parent_has_stores_of_its_own() {
    let provider: HeldProvider = Rc::default();
    let seen = provider.clone();
    let mut ui = compose_test(move || {
        *seen.borrow_mut() = Some(remember_view_model_store_provider!(parent = None));
    });
    ui.wait_for_idle();
    let _store = provider.borrow().as_ref().unwrap().get_or_create("test_key");
}

#[composable]
fn ProviderWhile(show: MutableState<bool>, seen: HeldProvider) {
    if show.get() {
        *seen.borrow_mut() = Some(remember_view_model_store_provider!());
    }
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreProviderTest.android.kt, rememberViewModelStoreProvider_whenLeavingComposition_disposesProvider
#[test]
fn a_provider_composed_again_after_leaving_is_another() {
    let show = mutable_state_of(true);
    let provider: HeldProvider = Rc::default();
    let (read, seen) = (show.handle(), provider.clone());
    let mut ui = compose_test(move || {
        ProviderWhile!(read, seen.clone());
    });
    ui.wait_for_idle();
    let original = provider.borrow().clone().unwrap();
    show.set(false);
    ui.wait_for_idle();
    show.set(true);
    ui.wait_for_idle();
    assert_ne!(provider.borrow().clone().unwrap(), original);
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreProviderTest.android.kt, rememberViewModelStoreProvider_onDispose_doesNotAffectSiblingStores
#[test]
fn a_provider_leaving_clears_its_stores_not_a_sibling_s() {
    let show = mutable_state_of(true);
    let (a, b): (HeldProvider, HeldProvider) = (Rc::default(), Rc::default());
    let (read, seen_a, seen_b) = (show.handle(), a.clone(), b.clone());
    let mut ui = compose_test(move || {
        SiblingProviders!(read, None, seen_a.clone(), seen_b.clone());
    });
    ui.wait_for_idle();
    let store_a = a.borrow().as_ref().unwrap().get_or_create("test_key");
    let store_b = b.borrow().as_ref().unwrap().get_or_create("test_key");
    show.set(false);
    ui.wait_for_idle();
    assert_ne!(a.borrow().as_ref().unwrap().get_or_create("test_key"), store_a);
    assert_eq!(b.borrow().as_ref().unwrap().get_or_create("test_key"), store_b);
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreProviderTest.android.kt, rememberViewModelStoreProvider_withSameKey_whenOneDisposed_preservesState
#[test]
fn providers_of_one_key_share_stores_while_one_remains() {
    let show = mutable_state_of(true);
    let (a, b): (HeldProvider, HeldProvider) = (Rc::default(), Rc::default());
    let (read, seen_a, seen_b) = (show.handle(), a.clone(), b.clone());
    let mut ui = compose_test(move || {
        SiblingProviders!(read, Some(7), seen_a.clone(), seen_b.clone());
    });
    ui.wait_for_idle();
    let store_a = a.borrow().as_ref().unwrap().get_or_create("shared_child_key");
    let store_b = b.borrow().as_ref().unwrap().get_or_create("shared_child_key");
    assert_eq!(store_a, store_b);
    show.set(false);
    ui.wait_for_idle();
    assert_eq!(b.borrow().as_ref().unwrap().get_or_create("shared_child_key"), store_a);
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/RememberViewModelStoreProviderTest.android.kt, rememberViewModelStoreProvider_withUnstableParent_isRemembered
#[test]
fn a_recomposition_leaves_a_provider_s_models() {
    let count = mutable_state_of(0);
    let initial: Held<TestViewModel> = Rc::default();
    let parent = Rc::new(FakeOwner::default());
    let (read, seen) = (count.handle(), initial.clone());
    let mut ui = compose_test(move || {
        let _ = read.get();
        let seen = seen.clone();
        with_owner(&parent, move || {
            let provider = remember_view_model_store_provider!();
            let owner = remember_view_model_store_owner_in!(provider);
            CompositionLocalProvider(
                LOCAL_VIEW_MODEL_STORE_OWNER.provides(Some(owner)),
                move || {
                    let model = view_model_with(None, |_| TestViewModel::default());
                    let mut seen = seen.borrow_mut();
                    if seen.is_none() {
                        *seen = Some(model);
                    }
                },
            );
        });
    });
    ui.wait_for_idle();
    count.set(1);
    ui.wait_for_idle();
    assert!(!initial.borrow().as_ref().unwrap().is_cleared.get());
}

// ---- ViewModelStoreProvider -------------------------------------------------------------------

/// An owner by which a provider has a parent.
fn parent_owner() -> ViewModelStoreOwnerRef {
    Rc::new(FakeOwner::default())
}

fn model_in(store: &ViewModelStore) -> Rc<TestViewModel> {
    ViewModelProvider::new(
        store.clone(),
        view_model_factory(|factory| factory.initializer(|_| TestViewModel::default())),
        CreationExtras::new(),
    )
    .get()
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, getViewModelStore_whenCalledFirstTime_createsNewStore
// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, getViewModelStore_whenCalledTwiceWithSameKey_returnsSameInstance
// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, getViewModelStore_whenCalledWithDifferentKeys_returnsDifferentInstances
#[test]
fn a_provider_has_a_store_per_key() {
    let provider = ViewModelStoreProvider::new(Some(&parent_owner()), ());
    let store = provider.get_or_create("feature_A");
    assert_eq!(provider.get_or_create("feature_A"), store);
    assert_ne!(provider.get_or_create("feature_B"), store);
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, removeState_whenCountIsZero_clearsStoreImmediately
#[test]
fn a_key_no_token_holds_clears_at_once() {
    let provider = ViewModelStoreProvider::new(Some(&parent_owner()), ());
    let before = model_in(&provider.get_or_create("feature_A"));
    provider.clear_key("feature_A");
    assert!(before.is_cleared.get());
    assert!(!Rc::ptr_eq(&model_in(&provider.get_or_create("feature_A")), &before));
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, removeState_whenCountIsPositive_defersCleanupUntilReleased
#[test]
fn a_key_a_token_holds_clears_when_it_s_released() {
    let provider = ViewModelStoreProvider::new(Some(&parent_owner()), ());
    let token = provider.acquire_token("shared_feature");
    let before = model_in(&provider.get_or_create("shared_feature"));
    provider.clear_key("shared_feature");
    assert!(Rc::ptr_eq(&model_in(&provider.get_or_create("shared_feature")), &before));
    assert!(!before.is_cleared.get());
    drop(token);
    assert!(before.is_cleared.get());
    assert!(!Rc::ptr_eq(&model_in(&provider.get_or_create("shared_feature")), &before));
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, getViewModelStoreOwner_whenCalled_returnsWrapperDelegatingToParentDefaults
#[test]
fn a_provider_s_owner_has_its_parent_s_defaults() {
    let parent = Rc::new(FakeOwner::default());
    let key = CreationExtrasKey::<String>::new();
    let as_owner: ViewModelStoreOwnerRef = parent.clone();
    let owner = ViewModelStoreProvider::new(Some(&as_owner), ()).get_or_create_owner("key");
    let given = owner.default_view_model_provider_factory();
    let _ = given.create(ModelClass::of::<TestViewModel>(), &CreationExtras::new());
    assert!(parent.factory.create_called.get());
    assert!(!owner.default_view_model_creation_extras().contains(key));
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, getViewModelStoreOwner_whenCalledTwice_returnsSameWrapperAndSameStore
#[test]
fn a_provider_gives_the_same_owner_of_a_key() {
    let provider = ViewModelStoreProvider::new(Some(&parent_owner()), ());
    let (first, second) =
        (provider.get_or_create_owner("key"), provider.get_or_create_owner("key"));
    assert!(Rc::ptr_eq(&first, &second));
    assert_eq!(first.view_model_store(), second.view_model_store());
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, acquireKey_withMultipleTokens_requiresAllToBeReleasedBeforeCleanup
#[test]
fn a_key_clears_when_its_last_token_is_released() {
    let provider = ViewModelStoreProvider::new(Some(&parent_owner()), ());
    let (first, second) = (provider.acquire_token("key"), provider.acquire_token("key"));
    let before = model_in(&provider.get_or_create("key"));
    provider.clear_key("key");
    drop(first);
    assert!(Rc::ptr_eq(&model_in(&provider.get_or_create("key")), &before));
    drop(second);
    assert!(!Rc::ptr_eq(&model_in(&provider.get_or_create("key")), &before));
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, configurationChange_whenProviderRecreated_preservesStore
#[test]
fn a_provider_made_again_finds_its_parent_s_stores() {
    let parent = parent_owner();
    let first = ViewModelStoreProvider::new(Some(&parent), ());
    let before = model_in(&first.get_or_create("config_change_test"));
    let second = ViewModelStoreProvider::new(Some(&parent), ());
    assert!(Rc::ptr_eq(&model_in(&second.get_or_create("config_change_test")), &before));
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, ownerDestroy_whenParentDies_clearsChildStores
#[test]
fn a_parent_s_store_cleared_clears_the_provider_s() {
    let parent = parent_owner();
    let provider = ViewModelStoreProvider::new(Some(&parent), ());
    let model = model_in(&provider.get_or_create("child"));
    parent.view_model_store().clear();
    assert!(model.is_cleared.get());
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, constructor_whenOwnerIsNull_createsRootProvider
// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, rootProvider_managesLifecycleIndependently
#[test]
fn a_root_provider_clears_its_stores_when_it_s_told() {
    let provider = ViewModelStoreProvider::new(None, ());
    let token = provider.acquire_token("root_feature");
    let before = model_in(&provider.get_or_create("root_feature"));
    drop(token);
    assert!(!before.is_cleared.get());
    provider.clear_all_keys();
    assert!(before.is_cleared.get());
    assert!(!Rc::ptr_eq(&model_in(&provider.get_or_create("root_feature")), &before));
}

// Compose: lifecycle/lifecycle-viewmodel-savedstate/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelStoreProviderTest.kt, clearAllKeys_onlyClearsStoresForSpecificProviderKey
#[test]
fn clearing_a_provider_leaves_another_s_key() {
    let parent = parent_owner();
    let first = ViewModelStoreProvider::new(Some(&parent), "provider_1");
    let second = ViewModelStoreProvider::new(Some(&parent), "provider_2");
    let key = "shared_feature_key";
    let (model1, model2) =
        (model_in(&first.get_or_create(key)), model_in(&second.get_or_create(key)));
    first.clear_all_keys();
    assert!(!Rc::ptr_eq(&model_in(&first.get_or_create(key)), &model1));
    assert!(Rc::ptr_eq(&model_in(&second.get_or_create(key)), &model2));
}

#[test]
fn a_provider_s_defaults_are_its_own_and_an_owner_s_over_them() {
    let (mine, others) = (Rc::new(FakeFactory::default()), Rc::new(FakeFactory::default()));
    let key = CreationExtrasKey::<String>::new();
    let mut extras = CreationExtras::new();
    extras.set(key, "value".to_string());
    let provider = ViewModelStoreProvider::with_defaults(
        Some(&parent_owner()),
        (),
        Some(extras.clone()),
        Some(mine.clone()),
    );
    let owner = provider.get_or_create_owner("key");
    assert_eq!(owner.default_view_model_creation_extras(), extras);
    let _ = owner
        .default_view_model_provider_factory()
        .create(ModelClass::of::<TestViewModel>(), &CreationExtras::new());
    assert!(mine.create_called.get());
    let owner = provider.get_or_create_owner_with("other", None, Some(others.clone()));
    let _ = owner
        .default_view_model_provider_factory()
        .create(ModelClass::of::<TestViewModel>(), &CreationExtras::new());
    assert!(others.create_called.get());
    assert_eq!(owner.default_view_model_creation_extras(), extras);
}

#[test]
fn an_own_store_s_owner_takes_defaults() {
    let factory = Rc::new(FakeFactory::default());
    let (given, seen) = (factory.clone(), Held::<TestViewModel>::default());
    let held = seen.clone();
    let _ui = compose_test(move || {
        let owner = remember_view_model_store_owner!(default_factory = Some(given.clone()));
        let held = held.clone();
        CompositionLocalProvider(LOCAL_VIEW_MODEL_STORE_OWNER.provides(Some(owner)), move || {
            *held.borrow_mut() = Some(view_model());
        });
    });
    assert!(factory.create_called.get());
    assert!(seen.borrow().is_some());
}

// Compose: lifecycle/lifecycle-viewmodel-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/viewmodel/compose/ViewModelTest.android.kt, nullViewModelStoreOwner
#[test]
fn a_place_without_an_owner_reads_none() {
    let read: Rc<Cell<Option<bool>>> = Rc::default();
    let seen = read.clone();
    let _ui = compose_test(move || {
        let seen = seen.clone();
        CompositionLocalProvider(LOCAL_VIEW_MODEL_STORE_OWNER.provides(None), move || {
            seen.set(Some(LOCAL_VIEW_MODEL_STORE_OWNER.current().is_none()));
        });
    });
    assert_eq!(read.get(), Some(true));
}

#[test]
#[should_panic(expected = "No ViewModelStoreOwner was provided via LocalViewModelStoreOwner")]
fn a_model_without_an_owner_panics() {
    let _ui = compose_test(|| {
        CompositionLocalProvider(LOCAL_VIEW_MODEL_STORE_OWNER.provides(None), || {
            view_model::<TestViewModel>();
        });
    });
}
