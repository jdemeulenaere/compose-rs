//! ViewModels, as AndroidX's `lifecycle-viewmodel`: objects outliving what shows them, kept in a
//! [`ViewModelStore`] an owner gives, made by a factory given [`CreationExtras`].
//!
//! Rust-shaped (D838): a view model embeds a [`ViewModelBase`] (Kotlin's base class) and says so by
//! [`ViewModel::base`]; a model class is the type; no reflection makes one, so a factory returns
//! `None` for what it can't make, and `view_model` of a `Default` type falls back on `default()`.

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use compose_runtime::{CoroutineScope, Owned};

/// What can be closed with a view model, as Kotlin's `AutoCloseable`. A closure is one.
pub trait AutoCloseable {
    fn close(&self);
}

impl<F: Fn()> AutoCloseable for F {
    fn close(&self) {
        self();
    }
}

/// A keyed closeable: as itself, to give back, and as what closes.
struct KeyedCloseable {
    key: String,
    value: Rc<dyn Any>,
    closeable: Rc<dyn AutoCloseable>,
}

const VIEW_MODEL_SCOPE_KEY: &str =
    "androidx.lifecycle.viewmodel.internal.ViewModelCoroutineScope.JOB_KEY";

/// A view model's scope, closed by cancelling it.
struct ScopeCloseable {
    scope: CoroutineScope,
    _owned: Option<Owned<CoroutineScope>>,
}

impl AutoCloseable for ScopeCloseable {
    fn close(&self) {
        self.scope.cancel();
    }
}

/// What every view model holds, as Kotlin's `ViewModel` base class holds its `ViewModelImpl`: its
/// closeables, its scope, whether it's cleared.
#[derive(Default)]
pub struct ViewModelBase {
    keyed: RefCell<Vec<KeyedCloseable>>,
    closeables: RefCell<Vec<Rc<dyn AutoCloseable>>>,
    is_cleared: Cell<bool>,
}

impl ViewModelBase {
    /// As `ViewModel()`.
    pub fn new() -> ViewModelBase {
        ViewModelBase::default()
    }

    /// As `ViewModel(vararg closeables)`.
    pub fn with_closeables(closeables: Vec<Rc<dyn AutoCloseable>>) -> ViewModelBase {
        let base = ViewModelBase::new();
        for closeable in closeables {
            base.add_closeable(closeable);
        }
        base
    }

    /// As `ViewModel(viewModelScope)`: the scope its tasks run in, cancelled when it's cleared.
    pub fn with_scope(scope: CoroutineScope) -> ViewModelBase {
        let base = ViewModelBase::new();
        base.add_scope(scope, None);
        base
    }

    fn add_scope(&self, scope: CoroutineScope, owned: Option<Owned<CoroutineScope>>) {
        let closeable = Rc::new(ScopeCloseable { scope, _owned: owned });
        self.add_keyed(VIEW_MODEL_SCOPE_KEY, closeable.clone(), closeable);
    }

    fn add_closeable(&self, closeable: Rc<dyn AutoCloseable>) {
        if self.is_cleared.get() {
            closeable.close();
            return;
        }
        // A set, as AndroidX's: the same one closes once.
        let mut closeables = self.closeables.borrow_mut();
        if !closeables
            .iter()
            .any(|kept| std::ptr::addr_eq(Rc::as_ptr(kept), Rc::as_ptr(&closeable)))
        {
            closeables.push(closeable);
        }
    }

    fn add_keyed(&self, key: &str, value: Rc<dyn Any>, closeable: Rc<dyn AutoCloseable>) {
        if self.is_cleared.get() {
            closeable.close();
            return;
        }
        let new = KeyedCloseable { key: key.to_string(), value, closeable };
        let old = {
            let mut keyed = self.keyed.borrow_mut();
            match keyed.iter_mut().find(|kept| kept.key == key) {
                Some(kept) => Some(std::mem::replace(kept, new)),
                None => {
                    keyed.push(new);
                    None
                }
            }
        };
        if let Some(old) = old {
            old.closeable.close();
        }
    }

    fn get_keyed(&self, key: &str) -> Option<Rc<dyn Any>> {
        self.keyed.borrow().iter().find(|kept| kept.key == key).map(|kept| kept.value.clone())
    }

    fn clear(&self) {
        if self.is_cleared.replace(true) {
            return;
        }
        let keyed: Vec<_> = self.keyed.borrow().iter().map(|kept| kept.closeable.clone()).collect();
        for closeable in keyed {
            closeable.close();
        }
        for closeable in self.closeables.take() {
            closeable.close();
        }
    }
}

/// A view model, as AndroidX's `ViewModel`: `struct Counter { base: ViewModelBase, … }`, `base`
/// returning that field. What it holds lives until its store is cleared.
pub trait ViewModel: Any {
    /// What it embeds.
    fn base(&self) -> &ViewModelBase;

    /// Called once when it's cleared, after its closeables, as `onCleared`.
    fn on_cleared(&self) {}

    /// Closes `closeable` when it's cleared, at once if it is, as `addCloseable`.
    fn add_closeable(&self, closeable: Rc<dyn AutoCloseable>) {
        self.base().add_closeable(closeable);
    }

    /// The same under `key`, what `get_closeable` gives back; one there already is closed, as
    /// `addCloseable(key, closeable)`.
    fn add_closeable_keyed<T: AutoCloseable + 'static>(&self, key: &str, closeable: Rc<T>)
    where
        Self: Sized,
    {
        self.base().add_keyed(key, closeable.clone(), closeable);
    }

    /// The closeable under `key`, if it's a `T`, as `getCloseable`.
    fn get_closeable<T: 'static>(&self, key: &str) -> Option<Rc<T>>
    where
        Self: Sized,
    {
        self.base().get_keyed(key)?.downcast().ok()
    }

    /// The scope of tasks ending when it's cleared, as `viewModelScope`: the one it was made
    /// with, or one of its own. After it's cleared, a cancelled one.
    fn view_model_scope(&self) -> CoroutineScope {
        let base = self.base();
        if let Some(kept) = base.get_keyed(VIEW_MODEL_SCOPE_KEY)
            && let Ok(kept) = kept.downcast::<ScopeCloseable>()
        {
            return kept.scope;
        }
        let owned = CoroutineScope::new();
        let scope = *owned;
        // Cleared already, it's cancelled at once and not kept, as AndroidX's.
        base.add_scope(scope, Some(owned));
        scope
    }

    /// Clears it, as Kotlin's internal `clear`: what a store does.
    #[doc(hidden)]
    fn clear(&self) {
        self.base().clear();
        self.on_cleared();
    }
}

/// A key of [`CreationExtras`], as `CreationExtras.Key<T>`. Each [`new`](Self::new) is distinct.
pub struct CreationExtrasKey<T> {
    id: u64,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for CreationExtrasKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for CreationExtrasKey<T> {}

impl<T> PartialEq for CreationExtrasKey<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> std::fmt::Debug for CreationExtrasKey<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "CreationExtras.Key@{:x}<{}>", self.id, std::any::type_name::<T>())
    }
}

impl<T> CreationExtrasKey<T> {
    /// A new key, as `CreationExtras.Key<T>()`.
    #[allow(clippy::new_without_default)]
    pub fn new() -> CreationExtrasKey<T> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        CreationExtrasKey { id: NEXT.fetch_add(1, Ordering::Relaxed), marker: PhantomData }
    }

    const fn with_id(id: u64) -> CreationExtrasKey<T> {
        CreationExtrasKey { id, marker: PhantomData }
    }
}

/// The key a view model is stored under, given in its extras, as `VIEW_MODEL_KEY`.
pub const VIEW_MODEL_KEY: CreationExtrasKey<String> = CreationExtrasKey::with_id(0);

/// One value of extras: the value and how to compare it.
#[derive(Clone)]
struct ExtrasEntry {
    id: u64,
    value: Rc<dyn Any>,
    same: fn(&dyn Any, &dyn Any) -> bool,
}

/// What a view model's factory is given, as AndroidX's `CreationExtras` and
/// `MutableCreationExtras` (one type: `set` is the mutable one's).
#[derive(Clone, Default)]
pub struct CreationExtras {
    entries: Vec<ExtrasEntry>,
}

impl CreationExtras {
    /// None, as `CreationExtras.Empty` and `MutableCreationExtras()`.
    pub fn new() -> CreationExtras {
        CreationExtras::default()
    }

    /// A copy of `initial_extras` changed by `builder`, as `CreationExtras(initialExtras) { … }`.
    pub fn with(
        initial_extras: &CreationExtras,
        builder: impl FnOnce(&mut CreationExtras),
    ) -> CreationExtras {
        let mut extras = initial_extras.clone();
        builder(&mut extras);
        extras
    }

    /// The value of `key`, as `get`.
    pub fn get<T: Clone + 'static>(&self, key: CreationExtrasKey<T>) -> Option<T> {
        let entry = self.entries.iter().find(|entry| entry.id == key.id)?;
        entry.value.downcast_ref::<T>().cloned()
    }

    /// Sets the value of `key`, as `set`.
    pub fn set<T: PartialEq + 'static>(&mut self, key: CreationExtrasKey<T>, value: T) {
        fn same<T: PartialEq + 'static>(a: &dyn Any, b: &dyn Any) -> bool {
            a.downcast_ref::<T>().zip(b.downcast_ref::<T>()).is_some_and(|(a, b)| a == b)
        }
        let entry = ExtrasEntry { id: key.id, value: Rc::new(value), same: same::<T> };
        self.put(entry);
    }

    fn put(&mut self, entry: ExtrasEntry) {
        match self.entries.iter_mut().find(|kept| kept.id == entry.id) {
            Some(kept) => *kept = entry,
            None => self.entries.push(entry),
        }
    }

    /// Whether it has `key`, as `in`.
    pub fn contains<T>(&self, key: CreationExtrasKey<T>) -> bool {
        self.entries.iter().any(|entry| entry.id == key.id)
    }
}

impl PartialEq for CreationExtras {
    fn eq(&self, other: &CreationExtras) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().all(|entry| {
                other.entries.iter().any(|theirs| {
                    theirs.id == entry.id && (entry.same)(&*entry.value, &*theirs.value)
                })
            })
    }
}

impl std::fmt::Debug for CreationExtras {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "CreationExtras({} extras)", self.entries.len())
    }
}

/// `other`'s values over these, as `plus`.
impl std::ops::Add<&CreationExtras> for &CreationExtras {
    type Output = CreationExtras;
    fn add(self, other: &CreationExtras) -> CreationExtras {
        let mut extras = self.clone();
        extras += other;
        extras
    }
}

/// `other`'s values over these, as `plusAssign`.
impl std::ops::AddAssign<&CreationExtras> for CreationExtras {
    fn add_assign(&mut self, other: &CreationExtras) {
        for entry in &other.entries {
            self.put(entry.clone());
        }
    }
}

/// A view model's type, as a `KClass`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ModelClass {
    pub type_id: TypeId,
    pub name: &'static str,
}

impl ModelClass {
    pub fn of<T: 'static>() -> ModelClass {
        ModelClass { type_id: TypeId::of::<T>(), name: std::any::type_name::<T>() }
    }
}

/// What makes view models, as AndroidX's `ViewModelProvider.Factory`: `None` for a class it can't,
/// as the default one's `UnsupportedOperationException`.
pub trait ViewModelProviderFactory {
    fn create(&self, model: ModelClass, extras: &CreationExtras) -> Option<Rc<dyn ViewModel>>;
}

/// The factory of owners with none of their own, as `DefaultViewModelProviderFactory`: it makes
/// nothing, Rust having no reflection (`view_model` of a `Default` type falls back on `default()`).
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultViewModelProviderFactory;

impl ViewModelProviderFactory for DefaultViewModelProviderFactory {
    fn create(&self, _: ModelClass, _: &CreationExtras) -> Option<Rc<dyn ViewModel>> {
        None
    }
}

type Initializer = Box<dyn Fn(&CreationExtras) -> Rc<dyn ViewModel>>;

/// A factory of initializers by type, as `viewModelFactory { initializer { … } }`.
#[derive(Default)]
pub struct InitializerViewModelFactory {
    initializers: Vec<(ModelClass, Initializer)>,
}

impl InitializerViewModelFactory {
    /// Adds what makes a `VM`, as `initializer`. Panics if there's one, as AndroidX throws.
    pub fn initializer<VM: ViewModel>(
        &mut self,
        initializer: impl Fn(&CreationExtras) -> VM + 'static,
    ) {
        let model = ModelClass::of::<VM>();
        assert!(
            !self.initializers.iter().any(|(kept, _)| *kept == model),
            "A `initializer` with the same `clazz` has already been added: {}.",
            model.name
        );
        self.initializers.push((model, Box::new(move |extras| Rc::new(initializer(extras)))));
    }
}

impl ViewModelProviderFactory for InitializerViewModelFactory {
    /// Panics without an initializer for the class, as AndroidX throws.
    fn create(&self, model: ModelClass, extras: &CreationExtras) -> Option<Rc<dyn ViewModel>> {
        let found = self.initializers.iter().find(|(kept, _)| *kept == model);
        match found {
            Some((_, initializer)) => Some(initializer(extras)),
            None => panic!("No initializer set for given class {}", model.name),
        }
    }
}

/// A factory of what `build` adds, as `viewModelFactory { … }`.
pub fn view_model_factory(
    build: impl FnOnce(&mut InitializerViewModelFactory),
) -> Rc<InitializerViewModelFactory> {
    let mut factory = InitializerViewModelFactory::default();
    build(&mut factory);
    Rc::new(factory)
}

/// A store's models by key, in order of addition.
type Models = Vec<(String, Rc<dyn ViewModel>)>;

/// The view models of a screen, as AndroidX's `ViewModelStore`: a handle (clones are the same),
/// compared by identity.
#[derive(Clone, Default)]
pub struct ViewModelStore {
    map: Rc<RefCell<Models>>,
}

impl PartialEq for ViewModelStore {
    fn eq(&self, other: &ViewModelStore) -> bool {
        Rc::ptr_eq(&self.map, &other.map)
    }
}

impl std::fmt::Debug for ViewModelStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ViewModelStore@{:x}(keys={:?})",
            Rc::as_ptr(&self.map) as usize,
            self.keys()
        )
    }
}

impl ViewModelStore {
    /// An empty store, as `ViewModelStore()`.
    pub fn new() -> ViewModelStore {
        ViewModelStore::default()
    }

    /// Keeps `view_model` under `key`, clearing one there, as `put`.
    pub fn put(&self, key: &str, view_model: Rc<dyn ViewModel>) {
        let old = {
            let mut map = self.map.borrow_mut();
            match map.iter_mut().find(|(kept, _)| kept == key) {
                Some((_, kept)) => Some(std::mem::replace(kept, view_model)),
                None => {
                    map.push((key.to_string(), view_model));
                    None
                }
            }
        };
        if let Some(old) = old {
            old.clear();
        }
    }

    /// The view model under `key`, as `get`.
    pub fn get(&self, key: &str) -> Option<Rc<dyn ViewModel>> {
        self.map.borrow().iter().find(|(kept, _)| kept == key).map(|(_, kept)| kept.clone())
    }

    /// The keys, as `keys`.
    pub fn keys(&self) -> Vec<String> {
        self.map.borrow().iter().map(|(key, _)| key.clone()).collect()
    }

    /// Clears its view models and forgets them, as `clear`. What they do meanwhile (a `put`, a
    /// `clear`) acts on the store already empty.
    pub fn clear(&self) {
        let snapshot = self.map.take();
        for (_, view_model) in snapshot {
            view_model.clear();
        }
    }
}

/// What has a store, as AndroidX's `ViewModelStoreOwner` and `HasDefaultViewModelProviderFactory`.
pub trait ViewModelStoreOwner {
    fn view_model_store(&self) -> ViewModelStore;

    /// The factory by default, as `defaultViewModelProviderFactory`.
    fn default_view_model_provider_factory(&self) -> Rc<dyn ViewModelProviderFactory> {
        Rc::new(DefaultViewModelProviderFactory)
    }

    /// The extras by default, as `defaultViewModelCreationExtras`.
    fn default_view_model_creation_extras(&self) -> CreationExtras {
        CreationExtras::new()
    }
}

impl ViewModelStoreOwner for ViewModelStore {
    fn view_model_store(&self) -> ViewModelStore {
        self.clone()
    }
}

/// Gets view models from a store, making them with a factory, as AndroidX's `ViewModelProvider`.
pub struct ViewModelProvider {
    store: ViewModelStore,
    factory: Rc<dyn ViewModelProviderFactory>,
    default_extras: CreationExtras,
}

impl ViewModelProvider {
    /// As `ViewModelProvider.create(store, factory, extras)`.
    pub fn new(
        store: ViewModelStore,
        factory: Rc<dyn ViewModelProviderFactory>,
        extras: CreationExtras,
    ) -> ViewModelProvider {
        ViewModelProvider { store, factory, default_extras: extras }
    }

    /// With the owner's store and its defaults, as `ViewModelProvider.create(owner)`.
    pub fn of(owner: &dyn ViewModelStoreOwner) -> ViewModelProvider {
        ViewModelProvider::new(
            owner.view_model_store(),
            owner.default_view_model_provider_factory(),
            owner.default_view_model_creation_extras(),
        )
    }

    /// With `factory` and the owner's store and extras, as `ViewModelProvider.create(owner, factory)`.
    pub fn of_with(
        owner: &dyn ViewModelStoreOwner,
        factory: Rc<dyn ViewModelProviderFactory>,
    ) -> ViewModelProvider {
        ViewModelProvider::new(
            owner.view_model_store(),
            factory,
            owner.default_view_model_creation_extras(),
        )
    }

    /// The `T` of this store under its default key, made if needed, as `get(modelClass)`. Panics if
    /// the factory can't.
    pub fn get<T: ViewModel>(&self) -> Rc<T> {
        self.get_keyed(&default_key::<T>())
    }

    /// The same under `key`: a view model of another type there is replaced, and cleared, as
    /// `get(key, modelClass)`.
    pub fn get_keyed<T: ViewModel>(&self, key: &str) -> Rc<T> {
        self.get_or_make(key, |_| {
            panic!(
                "Factory.create(KClass, CreationExtras) can't make {}",
                std::any::type_name::<T>()
            )
        })
    }

    /// The same, `fallback` making it if the factory doesn't.
    pub(crate) fn get_or_make<T: ViewModel>(
        &self,
        key: &str,
        fallback: impl FnOnce(&CreationExtras) -> T,
    ) -> Rc<T> {
        if let Some(kept) = self.store.get(key) {
            let kept: Rc<dyn Any> = kept;
            if let Ok(kept) = kept.downcast::<T>() {
                return kept;
            }
        }
        let mut extras = self.default_extras.clone();
        extras.set(VIEW_MODEL_KEY, key.to_string());
        let made: Rc<dyn ViewModel> = match self.factory.create(ModelClass::of::<T>(), &extras) {
            Some(made) => made,
            None => Rc::new(fallback(&extras)),
        };
        self.store.put(key, made.clone());
        let made: Rc<dyn Any> = made;
        made.downcast::<T>()
            .unwrap_or_else(|_| panic!("the factory didn't make a {}", std::any::type_name::<T>()))
    }
}

/// The key a type's view model is under, as `ViewModelProviders.getDefaultKey`.
pub(crate) fn default_key<T: 'static>() -> String {
    format!("androidx.lifecycle.ViewModelProvider.DefaultKey:{}", std::any::type_name::<T>())
}

/// A key of a store, hashing what it's given, as Compose's `Any?` keys are told apart by `equals`.
pub(crate) fn hashed_key(key: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}
