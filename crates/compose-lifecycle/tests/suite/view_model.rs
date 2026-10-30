//! `ViewModel`, `ViewModelStore`, `ViewModelProvider`, `CreationExtras` and their factories, against
//! AndroidX's tests of the same names.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_lifecycle::*;
use compose_runtime::platform;

/// A view model telling when it's cleared.
#[derive(Default)]
struct TestViewModel {
    base: ViewModelBase,
    cleared: Cell<bool>,
    on_cleared: Option<Box<dyn Fn()>>,
}

impl TestViewModel {
    fn new() -> Rc<TestViewModel> {
        Rc::new(TestViewModel::default())
    }

    fn with_on_cleared(on_cleared: impl Fn() + 'static) -> Rc<TestViewModel> {
        Rc::new(TestViewModel { on_cleared: Some(Box::new(on_cleared)), ..Default::default() })
    }

    fn with_closeables(closeables: Vec<Rc<dyn AutoCloseable>>) -> Rc<TestViewModel> {
        Rc::new(TestViewModel {
            base: ViewModelBase::with_closeables(closeables),
            ..Default::default()
        })
    }
}

impl ViewModel for TestViewModel {
    fn base(&self) -> &ViewModelBase {
        &self.base
    }

    fn on_cleared(&self) {
        self.cleared.set(true);
        if let Some(on_cleared) = &self.on_cleared {
            on_cleared();
        }
    }
}

/// A closeable telling when it's closed.
#[derive(Default)]
struct Resource {
    is_closed: Cell<bool>,
    on_close: Option<Box<dyn Fn()>>,
}

impl AutoCloseable for Resource {
    fn close(&self) {
        self.is_closed.set(true);
        if let Some(on_close) = &self.on_close {
            on_close();
        }
    }
}

fn resource() -> Rc<Resource> {
    Rc::new(Resource::default())
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, constructor_withCloseables_doesNotClose
#[test]
fn closeables_given_at_first_stay_open() {
    let (first, second) = (resource(), resource());
    let _model = TestViewModel::with_closeables(vec![first.clone(), second.clone()]);
    assert!(!first.is_closed.get() && !second.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, constructor_withCloseables_onClear_closesResources
#[test]
fn closeables_given_at_first_close_on_clear() {
    let (first, second) = (resource(), resource());
    let model = TestViewModel::with_closeables(vec![first.clone(), second.clone()]);
    model.clear();
    assert!(first.is_closed.get() && second.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, constructor_withDuplicatedCloseables_onClear_closesResources
#[test]
fn a_closeable_given_twice_closes_once() {
    let closes = Rc::new(Cell::new(0));
    let counted = closes.clone();
    let closeable = Rc::new(Resource {
        on_close: Some(Box::new(move || counted.set(counted.get() + 1))),
        ..Default::default()
    });
    let model = TestViewModel::with_closeables(vec![closeable.clone()]);
    model.add_closeable(closeable.clone());
    model.clear();
    assert!(closeable.is_closed.get());
    assert_eq!(closes.get(), 1);
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, addCloseable_doesNotClose
#[test]
fn a_closeable_added_stays_open() {
    let (model, closeable) = (TestViewModel::new(), resource());
    model.add_closeable(closeable.clone());
    assert!(!closeable.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, addCloseable_onClear_closesResource
#[test]
fn a_closeable_added_closes_on_clear() {
    let (model, closeable) = (TestViewModel::new(), resource());
    model.add_closeable(closeable.clone());
    model.clear();
    assert!(closeable.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, addCloseable_afterCleared_closesResource
#[test]
fn a_closeable_added_after_clear_closes_at_once() {
    let (model, closeable) = (TestViewModel::new(), resource());
    model.clear();
    model.add_closeable(closeable.clone());
    assert!(closeable.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, addCloseable_withKey_doesNotClose
#[test]
fn a_keyed_closeable_can_be_got_back_open() {
    let (model, closeable) = (TestViewModel::new(), resource());
    model.add_closeable_keyed("key", closeable.clone());
    let got = model.get_closeable::<Resource>("key").unwrap();
    assert!(Rc::ptr_eq(&got, &closeable));
    assert!(!got.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, addCloseable_withKey_onClear_closesResource
#[test]
fn a_keyed_closeable_closes_on_clear() {
    let (model, closeable) = (TestViewModel::new(), resource());
    model.add_closeable_keyed("totally_not_coroutine_context", closeable.clone());
    model.clear();
    assert!(closeable.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, addCloseable_withKey_afterCleared_closesResource
#[test]
fn a_keyed_closeable_added_after_clear_closes_at_once() {
    let (model, closeable) = (TestViewModel::new(), resource());
    model.clear();
    model.add_closeable_keyed("key", closeable.clone());
    assert!(closeable.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, addCloseable_withDuplicatedKey_replacesPrevious
// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, addCloseable_withDuplicatedKey_closesPrevious
#[test]
fn a_keyed_closeable_replaces_and_closes_the_one_before() {
    let (model, first, second) = (TestViewModel::new(), resource(), resource());
    model.add_closeable_keyed("key", first.clone());
    model.add_closeable_keyed("key", second.clone());
    let got = model.get_closeable::<Resource>("key").unwrap();
    assert!(Rc::ptr_eq(&got, &second));
    assert!(first.is_closed.get() && !second.is_closed.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelTest.kt, clear_closesResources_inCleaningSequenceOrder
#[test]
fn clearing_closes_the_keyed_first_then_the_rest_in_order() {
    let order: Rc<RefCell<Vec<&str>>> = Rc::default();
    let named = |name: &'static str| {
        let order = order.clone();
        Rc::new(Resource {
            on_close: Some(Box::new(move || order.borrow_mut().push(name))),
            ..Default::default()
        })
    };
    let model = TestViewModel::with_closeables(vec![named("built 1"), named("built 2")]);
    model.add_closeable(named("plain 1"));
    model.add_closeable_keyed("customKey1", named("keyed 1"));
    model.add_closeable(named("plain 2"));
    model.add_closeable_keyed("customKey2", named("keyed 2"));
    model.clear();
    assert_eq!(*order.borrow(), ["keyed 1", "keyed 2", "built 1", "built 2", "plain 1", "plain 2"]);
}

// ---- ViewModelStore --------------------------------------------------------------------------

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelStoreTest.kt, testClear
#[test]
fn a_store_cleared_clears_its_models_and_forgets_them() {
    let store = ViewModelStore::new();
    let (first, second) = (TestViewModel::new(), TestViewModel::new());
    store.put("a", first.clone());
    store.put("b", second.clone());
    assert!(!first.cleared.get() && !second.cleared.get());
    store.clear();
    assert!(first.cleared.get() && second.cleared.get());
    assert!(store.get("a").is_none() && store.get("b").is_none());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelStoreTest.kt, testToString
#[test]
fn a_store_prints_its_keys() {
    let store = ViewModelStore::new();
    store.put("key1", TestViewModel::new());
    let printed = format!("{store:?}");
    assert!(printed.starts_with("ViewModelStore@"), "{printed}");
    assert!(printed.ends_with("(keys=[\"key1\"])"), "{printed}");
}

fn reentrant_addition(first_adds: bool) {
    let store = ViewModelStore::new();
    let reentrant = TestViewModel::new();
    let (held, added) = (store.clone(), reentrant.clone());
    let adding = TestViewModel::with_on_cleared(move || held.put("reentrant", added.clone()));
    let other = TestViewModel::new();
    if first_adds {
        store.put("a", adding.clone());
        store.put("b", other.clone());
    } else {
        store.put("a", other.clone());
        store.put("b", adding.clone());
    }
    store.clear();
    assert!(!reentrant.cleared.get());
    assert!(adding.cleared.get() && other.cleared.get());
    assert_eq!(store.keys(), ["reentrant"]);
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelStoreTest.kt, testClear_withReentrantAdditionOnLastItem_doesNotCrash
#[test]
fn a_store_clearing_takes_an_addition_by_its_last_model() {
    reentrant_addition(false);
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelStoreTest.kt, testClear_withReentrantAdditionOnFirstItem_doesNotCrash
#[test]
fn a_store_clearing_takes_an_addition_by_its_first_model() {
    reentrant_addition(true);
}

fn reentrant_clear(first_clears: bool) {
    let store = ViewModelStore::new();
    let held = store.clone();
    let clearing = TestViewModel::with_on_cleared(move || held.clear());
    let other = TestViewModel::new();
    if first_clears {
        store.put("a", clearing.clone());
        store.put("b", other.clone());
    } else {
        store.put("a", other.clone());
        store.put("b", clearing.clone());
    }
    store.clear();
    assert!(clearing.cleared.get() && other.cleared.get());
    assert!(store.keys().is_empty());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelStoreTest.kt, testClear_withReentrantClearOnFirstItem_doesNotCrash
#[test]
fn a_store_clearing_takes_a_clear_by_its_first_model() {
    reentrant_clear(true);
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelStoreTest.kt, testClear_withReentrantClearOnLastItem_doesNotCrash
#[test]
fn a_store_clearing_takes_a_clear_by_its_last_model() {
    reentrant_clear(false);
}

// ---- ViewModelProvider -----------------------------------------------------------------------

#[derive(Default)]
struct Model1 {
    base: ViewModelBase,
    cleared: Cell<bool>,
}

impl ViewModel for Model1 {
    fn base(&self) -> &ViewModelBase {
        &self.base
    }
    fn on_cleared(&self) {
        self.cleared.set(true);
    }
}

#[derive(Default)]
struct Model2 {
    base: ViewModelBase,
}

impl ViewModel for Model2 {
    fn base(&self) -> &ViewModelBase {
        &self.base
    }
}

/// Makes the two models, counting its calls.
#[derive(Default)]
struct TestFactory {
    called: Cell<u32>,
    extras: RefCell<Option<CreationExtras>>,
}

impl ViewModelProviderFactory for TestFactory {
    fn create(&self, model: ModelClass, extras: &CreationExtras) -> Option<Rc<dyn ViewModel>> {
        self.called.set(self.called.get() + 1);
        *self.extras.borrow_mut() = Some(extras.clone());
        if model == ModelClass::of::<Model1>() {
            Some(Rc::new(Model1::default()))
        } else if model == ModelClass::of::<Model2>() {
            Some(Rc::new(Model2::default()))
        } else {
            panic!("View model class not supported: {model:?}")
        }
    }
}

fn provider() -> ViewModelProvider {
    ViewModelProvider::new(
        ViewModelStore::new(),
        Rc::new(TestFactory::default()),
        CreationExtras::new(),
    )
}

/// An owner with a factory and extras.
struct OwnerWith {
    store: ViewModelStore,
    factory: Rc<TestFactory>,
    extras: CreationExtras,
}

impl ViewModelStoreOwner for OwnerWith {
    fn view_model_store(&self) -> ViewModelStore {
        self.store.clone()
    }
    fn default_view_model_provider_factory(&self) -> Rc<dyn ViewModelProviderFactory> {
        self.factory.clone()
    }
    fn default_view_model_creation_extras(&self) -> CreationExtras {
        self.extras.clone()
    }
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderTest.kt, twoViewModelsWithSameKey
#[test]
fn a_model_of_another_type_under_a_key_replaces_and_clears_the_one_there() {
    let provider = provider();
    let first = provider.get_keyed::<Model1>("the_key");
    assert!(!first.cleared.get());
    let _second = provider.get_keyed::<Model2>("the_key");
    assert!(first.cleared.get());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderTest.kt, twoViewModels
#[test]
fn a_provider_gives_the_same_model_of_a_type_each_time() {
    let provider = provider();
    let (first, second) = (provider.get::<Model1>(), provider.get::<Model2>());
    assert!(Rc::ptr_eq(&provider.get::<Model1>(), &first));
    assert!(Rc::ptr_eq(&provider.get::<Model2>(), &second));
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderTest.kt, testOwnedBy
#[test]
fn a_provider_of_an_owner_with_a_factory_uses_its_store() {
    let store = ViewModelStore::new();
    let provider = ViewModelProvider::of_with(&store, Rc::new(TestFactory::default()));
    let model = provider.get::<Model1>();
    assert!(Rc::ptr_eq(&model, &provider.get::<Model1>()));
    assert_eq!(store.keys().len(), 1);
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderTest.kt, testCustomDefaultFactory
#[test]
fn a_provider_of_an_owner_uses_its_default_factory() {
    let factory = Rc::new(TestFactory::default());
    let owner = OwnerWith {
        store: ViewModelStore::new(),
        factory: factory.clone(),
        extras: CreationExtras::new(),
    };
    let provider = ViewModelProvider::of(&owner);
    let model = provider.get::<Model1>();
    assert!(Rc::ptr_eq(&model, &provider.get::<Model1>()));
    assert_eq!(factory.called.get(), 1);
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderTest.kt, testKeyedFactory
#[test]
fn a_factory_is_given_the_model_s_key_in_its_extras() {
    let factory = Rc::new(TestFactory::default());
    let provider = ViewModelProvider::of_with(&ViewModelStore::new(), factory.clone());
    provider.get_keyed::<Model1>("customKey");
    let extras = factory.extras.borrow().clone().unwrap();
    assert_eq!(extras.get(VIEW_MODEL_KEY).as_deref(), Some("customKey"));
    // Without a key of its own: its default one.
    provider.get::<Model2>();
    let extras = factory.extras.borrow().clone().unwrap();
    assert!(extras.get(VIEW_MODEL_KEY).unwrap().contains("DefaultKey"));
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderTest.kt, testDefaultCreationExtras
// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderTest.kt, testDefaultCreationExtrasWithMutableExtras
#[test]
fn a_factory_is_given_the_owner_s_default_extras() {
    let key = CreationExtrasKey::<String>::new();
    let mut extras = CreationExtras::new();
    extras.set(key, "test_value".to_string());
    let factory = Rc::new(TestFactory::default());
    let owner = OwnerWith { store: ViewModelStore::new(), factory: factory.clone(), extras };
    // A factory given directly, and the owner's own.
    let direct = Rc::new(TestFactory::default());
    ViewModelProvider::of_with(&owner, direct.clone()).get_keyed::<Model1>("customKey");
    let seen = direct.extras.borrow().clone().unwrap();
    assert_eq!(seen.get(VIEW_MODEL_KEY).as_deref(), Some("customKey"));
    assert_eq!(seen.get(key).as_deref(), Some("test_value"));
    ViewModelProvider::of(&owner).get_keyed::<Model2>("otherKey");
    let seen = factory.extras.borrow().clone().unwrap();
    assert_eq!(seen.get(key).as_deref(), Some("test_value"));
    // A copy a factory changes, what the mutable variant does.
    let mut changed = seen.clone();
    let mutable_key = CreationExtrasKey::<String>::new();
    changed.set(mutable_key, "value".to_string());
    assert_eq!(changed.get(key).as_deref(), Some("test_value"));
    assert_eq!(changed.get(mutable_key).as_deref(), Some("value"));
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelProviderGetTest.kt, get_withReifiedType
// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/ViewModelLazyTest.kt, test
#[test]
fn a_provider_gets_a_model_by_its_type_kept_in_the_store() {
    let store = ViewModelStore::new();
    let provider = ViewModelProvider::new(
        store.clone(),
        Rc::new(TestFactory::default()),
        CreationExtras::new(),
    );
    provider.get::<Model1>();
    assert!(!store.keys().is_empty());
}

#[test]
#[should_panic(expected = "can't make")]
fn a_provider_panics_if_the_factory_can_t_make_it() {
    ViewModelProvider::of(&ViewModelStore::new()).get::<Model1>();
}

// ---- viewModelFactory -------------------------------------------------------------------------

#[derive(Default)]
struct Valued<const N: u8> {
    base: ViewModelBase,
    value: Option<String>,
}

impl<const N: u8> ViewModel for Valued<N> {
    fn base(&self) -> &ViewModelBase {
        &self.base
    }
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelInitializerTest.kt, viewModelFactory_withUniqueInitializers_withCreationExtras_returnsViewModels
#[test]
fn initializers_make_their_models_from_the_extras() {
    let (key1, key2) = (CreationExtrasKey::<String>::new(), CreationExtrasKey::<String>::new());
    let mut extras1 = CreationExtras::new();
    extras1.set(key1, "test_value1".to_string());
    let mut extras2 = CreationExtras::new();
    extras2.set(key2, "test_value2".to_string());
    let factory = view_model_factory(|factory| {
        factory.initializer(move |extras| Valued::<1> {
            value: extras.get(key1),
            ..Default::default()
        });
        factory.initializer(move |extras| Valued::<2> {
            value: extras.get(key2),
            ..Default::default()
        });
    });
    let provider1 = ViewModelProvider::new(ViewModelStore::new(), factory.clone(), extras1);
    let provider2 = ViewModelProvider::new(ViewModelStore::new(), factory, extras2);
    assert_eq!(provider1.get::<Valued<1>>().value.as_deref(), Some("test_value1"));
    assert_eq!(provider2.get::<Valued<2>>().value.as_deref(), Some("test_value2"));
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelInitializerTest.kt, viewModelFactory_withDuplicatedInitializers_throwsException
#[test]
#[should_panic(expected = "A `initializer` with the same `clazz` has already been added")]
fn two_initializers_of_a_type_panic() {
    view_model_factory(|factory| {
        factory.initializer(|_| Valued::<1>::default());
        factory.initializer(|_| Valued::<1>::default());
    });
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/ViewModelInitializerTest.kt, viewModelFactory_noInitializers_throwsException
#[test]
#[should_panic(expected = "No initializer set for given class")]
fn no_initializer_for_a_type_panics() {
    let factory = view_model_factory(|_| {});
    let _ = factory.create(ModelClass::of::<Valued<1>>(), &CreationExtras::new());
}

// ---- HasDefaultViewModelProviderFactory ------------------------------------------------------

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/internal/HasDefaultViewModelProviderFactoryTest.kt, defaultCreationExtras_ownerWithNoExtras_returnsDefault
#[test]
fn an_owner_s_extras_are_empty_by_default() {
    assert_eq!(ViewModelStore::new().default_view_model_creation_extras(), CreationExtras::new());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/internal/HasDefaultViewModelProviderFactoryTest.kt, defaultCreationExtras_ownerWithExtras_returnsExtras
#[test]
fn an_owner_gives_its_own_extras() {
    let key = CreationExtrasKey::<i32>::new();
    let mut extras = CreationExtras::new();
    extras.set(key, 1);
    let owner =
        OwnerWith { store: ViewModelStore::new(), factory: Rc::default(), extras: extras.clone() };
    assert_eq!(owner.default_view_model_creation_extras(), extras);
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/internal/HasDefaultViewModelProviderFactoryTest.kt, defaultFactory_ownerWithNoFactory_returnsDefault
#[test]
fn an_owner_s_factory_makes_nothing_by_default() {
    let factory = ViewModelStore::new().default_view_model_provider_factory();
    assert!(factory.create(ModelClass::of::<Model1>(), &CreationExtras::new()).is_none());
}

// Compose: lifecycle/lifecycle-viewmodel/src/commonTest/kotlin/androidx/lifecycle/viewmodel/internal/HasDefaultViewModelProviderFactoryTest.kt, defaultFactory_ownerWithFactory_returnsExtras
#[test]
fn an_owner_gives_its_own_factory() {
    let factory = Rc::new(TestFactory::default());
    let owner = OwnerWith {
        store: ViewModelStore::new(),
        factory: factory.clone(),
        extras: CreationExtras::new(),
    };
    let given = owner.default_view_model_provider_factory();
    assert!(given.create(ModelClass::of::<Model1>(), &CreationExtras::new()).is_some());
    assert_eq!(factory.called.get(), 1);
}

// ---- CreationExtras --------------------------------------------------------------------------

fn extras_of(key: CreationExtrasKey<String>, value: &str) -> CreationExtras {
    let mut extras = CreationExtras::new();
    extras.set(key, value.to_string());
    extras
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, creationExtras_withInitial_includesInitialValues
// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, creationExtras_withInitial_combinesWithBuilderValues
// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, creationExtras_withInitial_builderOverridesInitialValues
#[test]
fn extras_built_from_others_have_their_values_and_the_builder_s_over_them() {
    let (key1, key2) = (CreationExtrasKey::new(), CreationExtrasKey::new());
    let initial = extras_of(key1, "value1");
    assert_eq!(CreationExtras::with(&initial, |_| {}).get(key1).as_deref(), Some("value1"));
    let both = CreationExtras::with(&initial, |extras| extras.set(key2, "value2".to_string()));
    assert_eq!(both.get(key1).as_deref(), Some("value1"));
    assert_eq!(both.get(key2).as_deref(), Some("value2"));
    let over = CreationExtras::with(&initial, |extras| extras.set(key1, "overridden".to_string()));
    assert_eq!(over.get(key1).as_deref(), Some("overridden"));
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, keyFactory_returnsDistinctInstances
#[test]
fn each_key_is_distinct() {
    assert_ne!(CreationExtrasKey::<String>::new(), CreationExtrasKey::<String>::new());
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, initialExtras_originalModifiedAfterCopy_copyRemainsUnchanged
#[test]
fn a_copy_of_extras_does_not_follow_the_original() {
    let key = CreationExtrasKey::new();
    let mut original = extras_of(key, "value1");
    let copy = original.clone();
    original.set(key, "value2".to_string());
    assert_eq!(original.get(key).as_deref(), Some("value2"));
    assert_eq!(copy.get(key).as_deref(), Some("value1"));
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, equals_sameValues_isEqual
// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, equals_differentValues_isNotEqual
#[test]
fn extras_are_equal_by_their_values() {
    let key = CreationExtrasKey::new();
    assert_eq!(extras_of(key, "value1"), extras_of(key, "value1"));
    assert_ne!(extras_of(key, "value1"), extras_of(key, "value2"));
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, contains_returnsTrueForExistingKey
// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, contains_returnsFalseForNonExistingKey
#[test]
fn extras_know_their_keys() {
    let (key1, key2) = (CreationExtrasKey::new(), CreationExtrasKey::<String>::new());
    let extras = extras_of(key1, "value1");
    assert!(extras.contains(key1) && !extras.contains(key2));
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, plus_addedTogetherWithUniqueKeys_combinesValues
// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, plus_addedTogetherWithConflictingKeys_overridesFirstValue
#[test]
fn extras_added_combine_the_second_over_the_first() {
    let (key1, key2) = (CreationExtrasKey::new(), CreationExtrasKey::new());
    let sum = &extras_of(key1, "value1") + &extras_of(key2, "value2");
    assert_eq!(sum.get(key1).as_deref(), Some("value1"));
    assert_eq!(sum.get(key2).as_deref(), Some("value2"));
    let over = &extras_of(key1, "value1") + &extras_of(key1, "value2");
    assert_eq!(over.get(key1).as_deref(), Some("value2"));
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, plusAssign_addedTogetherWithUniqueKeys_combinesValues
// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, plusAssign_addedTogetherWithConflictingKeys_overridesFirstValue
#[test]
fn extras_added_in_place_combine_the_second_over_the_first() {
    let (key1, key2) = (CreationExtrasKey::new(), CreationExtrasKey::new());
    let mut extras = extras_of(key1, "value1");
    extras += &extras_of(key2, "value2");
    assert_eq!(extras.get(key1).as_deref(), Some("value1"));
    assert_eq!(extras.get(key2).as_deref(), Some("value2"));
    extras += &extras_of(key1, "value3");
    assert_eq!(extras.get(key1).as_deref(), Some("value3"));
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/CreationExtrasTest.kt, keyToString_returnsClassName
#[test]
fn a_key_prints_its_type() {
    let printed = format!("{:?}", CreationExtrasKey::<i32>::new());
    assert!(printed.starts_with("CreationExtras.Key@") && printed.ends_with("<i32>"), "{printed}");
}

// ---- viewModelScope ---------------------------------------------------------------------------

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewModelScopeTest.kt, viewModelScope_withDefaultScope_whenLaunch_cancelsOnClear
#[test]
fn a_model_s_scope_is_cancelled_on_clear() {
    let model = TestViewModel::new();
    let (first, second) = (
        model.view_model_scope().launch(std::future::pending()),
        model.view_model_scope().launch(std::future::pending()),
    );
    platform::run_ready();
    assert!(first.is_active() && second.is_active());
    model.clear();
    assert!(!first.is_active() && !second.is_active());
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewModelScopeTest.kt, viewModelScope_withDefaultScope_afterClear_launchesCancelledJob
#[test]
fn a_model_s_scope_after_clear_launches_nothing() {
    let model = TestViewModel::new();
    model.clear();
    let job = model.view_model_scope().launch(std::future::pending());
    assert!(!job.is_active());
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewModelScopeTest.kt, viewModelScope_withDefaultScope_afterClear_returnsSameScope
#[test]
fn a_model_s_scope_stays_the_same_after_clear() {
    let model = TestViewModel::new();
    let before = model.view_model_scope();
    model.clear();
    assert_eq!(model.view_model_scope(), before);
}

fn with_custom_scope()
-> (Rc<TestViewModel>, compose_runtime::Owned<compose_runtime::CoroutineScope>) {
    let scope = compose_runtime::CoroutineScope::new();
    let model =
        Rc::new(TestViewModel { base: ViewModelBase::with_scope(*scope), ..Default::default() });
    (model, scope)
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewModelScopeTest.kt, viewModelScope_withCustomScope_whenLaunch_cancelsOnClear
#[test]
fn a_model_s_scope_it_was_given_is_cancelled_on_clear() {
    let (model, _scope) = with_custom_scope();
    let job = model.view_model_scope().launch(std::future::pending());
    platform::run_ready();
    assert!(job.is_active());
    model.clear();
    assert!(!job.is_active());
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewModelScopeTest.kt, viewModelScope_withCustomScope_afterClear_launchesCancelledJob
#[test]
fn a_model_s_scope_it_was_given_after_clear_launches_nothing() {
    let (model, _scope) = with_custom_scope();
    model.clear();
    assert!(!model.view_model_scope().launch(std::future::pending()).is_active());
}

// Compose: lifecycle/lifecycle-viewmodel/src/androidDeviceTest/kotlin/androidx/lifecycle/ViewModelScopeTest.kt, viewModelScope_withCustomScope_afterClear_returnsSameScope
#[test]
fn a_model_s_scope_it_was_given_stays_after_clear() {
    let (model, scope) = with_custom_scope();
    assert_eq!(model.view_model_scope(), *scope);
    model.clear();
    assert_eq!(model.view_model_scope(), *scope);
}
