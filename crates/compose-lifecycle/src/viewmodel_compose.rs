//! View models in composition, as AndroidX's `lifecycle-viewmodel-compose`, and
//! `ViewModelStoreProvider`, which `lifecycle-viewmodel-savedstate` holds (D838).

use std::cell::{Cell, RefCell};
use std::hash::Hash;
use std::rc::Rc;

use compose_runtime::{
    CompositionLocal, DisposableEffect, composable, current_composite_key_hash, remember_keyed,
};

use crate::composition::LOCAL_LIFECYCLE_OWNER;
use crate::lifecycle::LifecycleState;
use crate::viewmodel::{
    CreationExtras, DefaultViewModelProviderFactory, ViewModel, ViewModelBase, ViewModelProvider,
    ViewModelProviderFactory, ViewModelStore, ViewModelStoreOwner, default_key, hashed_key,
    view_model_factory,
};

/// A store's owner, shared.
pub type ViewModelStoreOwnerRef = Rc<dyn ViewModelStoreOwner>;

/// The store of what's composing, as AndroidX's `LocalViewModelStoreOwner`: the host's unless
/// something provides another.
pub static LOCAL_VIEW_MODEL_STORE_OWNER: CompositionLocal<Option<ViewModelStoreOwnerRef>> =
    CompositionLocal::new("LocalViewModelStoreOwner", || None);

fn local_owner() -> ViewModelStoreOwnerRef {
    LOCAL_VIEW_MODEL_STORE_OWNER
        .current()
        .expect("No ViewModelStoreOwner was provided via LocalViewModelStoreOwner")
}

/// What `view_model_with_args` is given, each defaulting as AndroidX's `viewModel(…)`'s.
#[derive(Default)]
pub struct ViewModelArgs {
    pub view_model_store_owner: Option<ViewModelStoreOwnerRef>,
    pub key: Option<String>,
    pub factory: Option<Rc<dyn ViewModelProviderFactory>>,
    pub extras: Option<CreationExtras>,
}

/// The `VM` of the local store, kept there, made by its default factory or, with none, by
/// `default()`, as AndroidX's `viewModel<VM>()` (which reflection makes).
pub fn view_model<VM: ViewModel + Default>() -> Rc<VM> {
    view_model_args(ViewModelArgs::default())
}

/// The same with AndroidX's other parameters, as `viewModel<VM>(owner, key, factory, extras)`.
pub fn view_model_args<VM: ViewModel + Default>(args: ViewModelArgs) -> Rc<VM> {
    get(args, |_| VM::default())
}

/// The `VM` of the local store under `key` (none: its default one), `initializer` making it, as
/// `viewModel(key = …) { … }`.
pub fn view_model_with<VM: ViewModel>(
    key: Option<&str>,
    initializer: impl Fn(&CreationExtras) -> VM + 'static,
) -> Rc<VM> {
    view_model_owned_with(local_owner(), key, initializer)
}

/// The same in `owner`'s store, as `viewModel(viewModelStoreOwner, key) { … }`.
pub fn view_model_owned_with<VM: ViewModel>(
    owner: ViewModelStoreOwnerRef,
    key: Option<&str>,
    initializer: impl Fn(&CreationExtras) -> VM + 'static,
) -> Rc<VM> {
    let factory = view_model_factory(|factory| factory.initializer(initializer));
    get(
        ViewModelArgs {
            view_model_store_owner: Some(owner),
            key: key.map(str::to_string),
            factory: Some(factory),
            extras: None,
        },
        |_| unreachable!("an initializer makes it"),
    )
}

fn get<VM: ViewModel>(args: ViewModelArgs, fallback: impl FnOnce(&CreationExtras) -> VM) -> Rc<VM> {
    let owner = args.view_model_store_owner.unwrap_or_else(local_owner);
    let factory = args.factory.unwrap_or_else(|| owner.default_view_model_provider_factory());
    let extras = args.extras.unwrap_or_else(|| owner.default_view_model_creation_extras());
    let provider = ViewModelProvider::new(owner.view_model_store(), factory, extras);
    let key = args.key.unwrap_or_else(default_key::<VM>);
    provider.get_or_make(&key, fallback)
}

/// One key's store and who holds it, as `ViewModelStoreProvider.Entry`.
struct Entry {
    key: u64,
    store: ViewModelStore,
    ref_count: Cell<i32>,
    is_disposable: Cell<bool>,
}

/// The entries, a view model of the parent store so they outlive a configuration change, as
/// `ViewModelStoreProvider.StateHolder`.
#[derive(Default)]
struct StateHolder {
    base: ViewModelBase,
    entries: RefCell<Vec<Rc<Entry>>>,
}

impl ViewModel for StateHolder {
    fn base(&self) -> &ViewModelBase {
        &self.base
    }

    fn on_cleared(&self) {
        self.clear_entries();
    }
}

impl StateHolder {
    fn get_or_create(&self, key: u64) -> Rc<Entry> {
        let mut entries = self.entries.borrow_mut();
        if let Some(entry) = entries.iter().find(|entry| entry.key == key) {
            return entry.clone();
        }
        let entry = Rc::new(Entry {
            key,
            store: ViewModelStore::new(),
            ref_count: Cell::new(0),
            is_disposable: Cell::new(false),
        });
        entries.push(entry.clone());
        entry
    }

    fn remove(&self, key: u64) {
        let removed = {
            let mut entries = self.entries.borrow_mut();
            entries.iter().position(|entry| entry.key == key).map(|index| entries.remove(index))
        };
        if let Some(entry) = removed {
            entry.store.clear();
        }
    }

    fn clear_key(&self, key: u64) {
        let entry = self.entries.borrow().iter().find(|entry| entry.key == key).cloned();
        if let Some(entry) = entry {
            entry.is_disposable.set(true);
            if entry.ref_count.get() <= 0 {
                self.remove(key);
            }
        }
    }

    fn clear_entries(&self) {
        let entries: Vec<_> = self.entries.borrow().clone();
        for entry in entries {
            entry.is_disposable.set(true);
            if entry.ref_count.get() <= 0 {
                self.remove(entry.key);
            }
        }
    }
}

/// `ProviderMarkerKey`'s key.
const PROVIDER_MARKER_KEY: u64 = 0x5052_4f56_4944_4552;

/// A store's owner with its provider's defaults, as `ViewModelStoreOwner(viewModelStore, …)`.
struct ProvidedOwner {
    store: ViewModelStore,
    extras: CreationExtras,
    factory: Rc<dyn ViewModelProviderFactory>,
}

impl ViewModelStoreOwner for ProvidedOwner {
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

/// Stores by key, kept in a parent's store, as AndroidX's `ViewModelStoreProvider`: each lives
/// while something holds a token for it. A handle, compared by identity. Keys are told apart by
/// their hash. Its saved-state parameters wait for the saved-state library.
#[derive(Clone)]
pub struct ViewModelStoreProvider {
    inner: Rc<ProviderInner>,
}

struct ProviderInner {
    parent_store: Option<ViewModelStore>,
    parent_key: String,
    default_extras: CreationExtras,
    default_factory: Rc<dyn ViewModelProviderFactory>,
    state_holder: RefCell<Option<Rc<StateHolder>>>,
    owners: RefCell<Vec<(u64, ViewModelStoreOwnerRef)>>,
}

impl PartialEq for ViewModelStoreProvider {
    fn eq(&self, other: &ViewModelStoreProvider) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl compose_runtime::ComposeArg for ViewModelStoreProvider {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Debug for ViewModelStoreProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ViewModelStoreProvider")
    }
}

impl ViewModelStoreProvider {
    /// A provider keeping its stores in `parent`'s store under `parent_key` (none: its own), with
    /// `parent`'s defaults, as `ViewModelStoreProvider(parentOwner, parentKey)`.
    pub fn new(
        parent: Option<&ViewModelStoreOwnerRef>,
        parent_key: impl Hash,
    ) -> ViewModelStoreProvider {
        ViewModelStoreProvider::with_defaults(parent, parent_key, None, None)
    }

    /// [`new`](Self::new) with defaults of its own for its owners' view models (each `None`:
    /// `parent`'s), as the constructor's `defaultCreationExtras` and `defaultFactory`.
    pub fn with_defaults(
        parent: Option<&ViewModelStoreOwnerRef>,
        parent_key: impl Hash,
        default_extras: Option<CreationExtras>,
        default_factory: Option<Rc<dyn ViewModelProviderFactory>>,
    ) -> ViewModelStoreProvider {
        ViewModelStoreProvider {
            inner: Rc::new(ProviderInner {
                parent_store: parent.map(|parent| parent.view_model_store()),
                parent_key: format!(
                    "androidx.lifecycle.ViewModelStoreProvider:{:x}",
                    hashed_key(parent_key)
                ),
                default_extras: default_extras.unwrap_or_else(|| {
                    parent.map_or_else(CreationExtras::new, |parent| {
                        parent.default_view_model_creation_extras()
                    })
                }),
                default_factory: default_factory.unwrap_or_else(|| {
                    parent.map_or_else(
                        || {
                            Rc::new(DefaultViewModelProviderFactory)
                                as Rc<dyn ViewModelProviderFactory>
                        },
                        |parent| parent.default_view_model_provider_factory(),
                    )
                }),
                state_holder: RefCell::new(None),
                owners: RefCell::new(Vec::new()),
            }),
        }
    }

    fn state_holder(&self) -> Rc<StateHolder> {
        let mut holder = self.inner.state_holder.borrow_mut();
        if let Some(holder) = &*holder {
            return holder.clone();
        }
        let made = match &self.inner.parent_store {
            Some(store) => {
                let provider = ViewModelProvider::new(
                    store.clone(),
                    Rc::new(DefaultViewModelProviderFactory),
                    CreationExtras::new(),
                );
                provider.get_or_make(&self.inner.parent_key, |_| StateHolder::default())
            }
            None => Rc::new(StateHolder::default()),
        };
        *holder = Some(made.clone());
        made
    }

    /// Holds `key`'s store until the token's dropped: then, if it's disposable, it goes, as
    /// `acquireToken`.
    pub fn acquire_token(&self, key: impl Hash) -> ReferenceToken {
        self.acquire_token_hashed(hashed_key(key))
    }

    fn acquire_token_hashed(&self, key: u64) -> ReferenceToken {
        let entry = self.state_holder().get_or_create(key);
        entry.ref_count.set(entry.ref_count.get() + 1);
        ReferenceToken { provider: self.clone(), entry }
    }

    /// `key`'s store, as `getOrCreate`.
    pub fn get_or_create(&self, key: impl Hash) -> ViewModelStore {
        self.state_holder().get_or_create(hashed_key(key)).store.clone()
    }

    /// `key`'s store's owner, as `getOrCreateOwner`.
    pub fn get_or_create_owner(&self, key: impl Hash) -> ViewModelStoreOwnerRef {
        self.get_or_create_owner_with(key, None, None)
    }

    /// [`get_or_create_owner`](Self::get_or_create_owner), a first call giving the owner defaults
    /// other than the provider's, as its `defaultCreationExtras` and `defaultFactory`.
    pub fn get_or_create_owner_with(
        &self,
        key: impl Hash,
        default_extras: Option<CreationExtras>,
        default_factory: Option<Rc<dyn ViewModelProviderFactory>>,
    ) -> ViewModelStoreOwnerRef {
        let key = hashed_key(key);
        if let Some((_, owner)) = self.inner.owners.borrow().iter().find(|(kept, _)| *kept == key) {
            return owner.clone();
        }
        let owner: ViewModelStoreOwnerRef = Rc::new(ProvidedOwner {
            store: self.state_holder().get_or_create(key).store.clone(),
            extras: default_extras.unwrap_or_else(|| self.inner.default_extras.clone()),
            factory: default_factory.unwrap_or_else(|| self.inner.default_factory.clone()),
        });
        self.inner.owners.borrow_mut().push((key, owner.clone()));
        owner
    }

    /// Clears `key`'s store once nothing holds it, as `clearKey`.
    pub fn clear_key(&self, key: impl Hash) {
        let key = hashed_key(key);
        self.inner.owners.borrow_mut().retain(|(kept, _)| *kept != key);
        self.state_holder().clear_key(key);
    }

    /// Clears every store once nothing holds it, unless the provider itself is held, as
    /// `clearAllKeys`.
    pub fn clear_all_keys(&self) {
        let holder = self.state_holder();
        let marker =
            holder.entries.borrow().iter().find(|entry| entry.key == PROVIDER_MARKER_KEY).cloned();
        if marker.is_some_and(|marker| marker.ref_count.get() > 0) {
            return;
        }
        self.inner.owners.borrow_mut().clear();
        holder.clear_entries();
    }
}

/// A hold on a provider's key, released when dropped, as `ReferenceToken`.
pub struct ReferenceToken {
    provider: ViewModelStoreProvider,
    entry: Rc<Entry>,
}

impl Drop for ReferenceToken {
    fn drop(&mut self) {
        let entry = &self.entry;
        entry.ref_count.set(entry.ref_count.get() - 1);
        if entry.is_disposable.get() && entry.ref_count.get() <= 0 {
            self.provider.inner.owners.borrow_mut().retain(|(kept, _)| *kept != entry.key);
            self.provider.state_holder().remove(entry.key);
        }
    }
}

/// A provider of this place in the composition, its stores cleared when it leaves (not on a
/// configuration change: the lifecycle's destroyed by then), as AndroidX's
/// `rememberViewModelStoreProvider`. `parent` defaults to the local store's owner (`None`: a root
/// of its own); give `key` to share it between places. Its defaults are read once.
#[composable]
pub fn remember_view_model_store_provider(
    #[default(Some(crate::local_view_model_store_owner()))] parent: Option<ViewModelStoreOwnerRef>,
    #[default(None)] key: Option<u64>,
    #[default(None)] default_creation_extras: Option<CreationExtras>,
    #[default(None)] default_factory: Option<Rc<dyn ViewModelProviderFactory>>,
) -> ViewModelStoreProvider {
    let key = key.unwrap_or_else(current_composite_key_hash);
    let provider = {
        let held = parent.clone();
        remember_keyed((parent, key), move || {
            ViewModelStoreProvider::with_defaults(
                held.as_ref(),
                key,
                default_creation_extras,
                default_factory,
            )
        })
    };
    let lifecycle = LOCAL_LIFECYCLE_OWNER.current();
    let disposed = provider.clone();
    DisposableEffect((provider.clone(), lifecycle.clone()), move |scope| {
        let token = disposed.acquire_token_hashed(PROVIDER_MARKER_KEY);
        scope.on_dispose(move || {
            drop(token);
            if lifecycle.current_state().is_at_least(LifecycleState::Created) {
                disposed.clear_all_keys();
            }
        })
    });
    provider
}

/// A store's owner of this place in the composition, its view models cleared when it leaves, as
/// AndroidX's `rememberViewModelStoreOwner(parent, …)`. Its defaults are read once.
#[composable]
pub fn remember_view_model_store_owner(
    #[default(Some(crate::local_view_model_store_owner()))] parent: Option<ViewModelStoreOwnerRef>,
    #[default(None)] default_creation_extras: Option<CreationExtras>,
    #[default(None)] default_factory: Option<Rc<dyn ViewModelProviderFactory>>,
) -> ViewModelStoreOwnerRef {
    let key = current_composite_key_hash();
    let provider = remember_view_model_store_provider!(
        parent = parent,
        key = Some(key),
        default_creation_extras = default_creation_extras,
        default_factory = default_factory
    );
    remember_view_model_store_owner_in!(provider, key = Some(key))
}

/// A store's owner of `provider`, held while it's in the composition: a place's own without `key`,
/// shared with others of the key with it, as AndroidX's
/// `rememberViewModelStoreOwner(key, provider)`.
#[composable]
pub fn remember_view_model_store_owner_in(
    provider: ViewModelStoreProvider,
    #[default(None)] key: Option<u64>,
) -> ViewModelStoreOwnerRef {
    let key = key.unwrap_or_else(current_composite_key_hash);
    let owner = {
        let held = provider.clone();
        remember_keyed((provider.clone(), key), move || held.get_or_create_owner(key))
    };
    let holding = provider;
    DisposableEffect(owner.clone(), move |scope| {
        let token = holding.acquire_token(key);
        scope.on_dispose(move || drop(token))
    });
    owner
}
