//! `retain`, as Compose's `runtime-retain` (D1306): values that outlive the content that made them,
//! kept by a [`RetainedValuesStore`] while it says so, for that content to find when it comes back.
//!
//! A value goes to its store when its content leaves the composition, under its key (where it was,
//! its type, and the keys it was given). A store that keeps it gives it back to content that asks
//! with the same key, last kept first. Each is told where it is, if it's a [`RetainObserver`].
//!
//! Compose throws on retaining a `RememberObserver` that isn't a `RetainObserver`. A generic function
//! can't ask that here, so a value with callbacks goes through [`retain_observer`], which is why
//! [`remember_observer`](crate::remember_observer) exists too.

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::__private::ReplaceGroup;
use crate::compose_arg::ComposeArg;
use crate::effects::SideEffect;
use crate::hash::{hash_key, key_of};
use crate::local::{CompositionLocal, with_composition_locals};
use crate::recomposer::{CancellationHandle, Recomposer};
use crate::remember::{RememberObserver, remember_observer_keyed};
use crate::saveable::current_composite_key_hash_code;

/// What a retained value is told of its life, as `RetainObserver`: retained once, in and out of the
/// composition as its content is, and retired once no store keeps it. Never retained, a value made
/// by a composition that's abandoned hears [`on_unused`](RetainObserver::on_unused).
pub trait RetainObserver: 'static {
    fn on_retained(&self) {}
    fn on_entered_composition(&self) {}
    fn on_exited_composition(&self) {}
    fn on_retired(&self) {}
    fn on_unused(&self) {}
}

/// Keys given to [`retain_keyed`], compared as a `remember`'s.
trait RetainKeys: Any {
    fn same_as(&self, other: &dyn RetainKeys) -> bool;
    fn as_any(&self) -> &dyn Any;
}

impl<K: ComposeArg + 'static> RetainKeys for K {
    fn same_as(&self, other: &dyn RetainKeys) -> bool {
        other.as_any().downcast_ref::<K>().is_some_and(|other| self.same(other))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// What a retained value is kept under: where it was composed, its type, and its keys.
#[derive(Clone)]
pub struct RetainKey {
    keys: Option<Rc<dyn RetainKeys>>,
    positional: u64,
    type_id: TypeId,
}

impl PartialEq for RetainKey {
    fn eq(&self, other: &RetainKey) -> bool {
        self.positional == other.positional
            && self.type_id == other.type_id
            && match (&self.keys, &other.keys) {
                (None, None) => true,
                (Some(keys), Some(others)) => (**keys).same_as(&**others),
                _ => false,
            }
    }
}

impl ComposeArg for RetainKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A value retained: what a [`RetainedValuesStore`] keeps.
#[derive(Clone)]
pub struct RetainedValue {
    value: Rc<dyn Any>,
    observer: Option<Rc<dyn RetainObserver>>,
}

impl RetainedValue {
    /// No store keeps it any more.
    pub fn retire(&self) {
        if let Some(observer) = &self.observer {
            observer.on_retired();
        }
    }
}

/// What keeps the values of content that left the composition, as `RetainedValuesStore`.
pub trait RetainedValuesStore: 'static {
    /// The value kept under `key`, the last one, which it no longer keeps.
    fn consume_exited_value(&self, key: &RetainKey) -> Option<RetainedValue>;
    /// `value`'s content left: it keeps it under `key`, or retires it.
    fn save_exiting_value(&self, key: RetainKey, value: RetainedValue);
    /// The content under it entered the composition, at the end of that frame.
    fn on_content_entered_composition(&self);
    /// The content under it left.
    fn on_content_exit_composition(&self);
    /// Whether what leaves is kept, as `isRetainingExitedValues`: never, by default.
    fn is_retaining_exited_values(&self) -> bool {
        false
    }
}

/// The store that keeps nothing, as `ForgetfulRetainedValuesStore`: `retain` is `remember` with it,
/// which is what a composition has by default.
pub struct ForgetfulRetainedValuesStore;

impl RetainedValuesStore for ForgetfulRetainedValuesStore {
    fn consume_exited_value(&self, _key: &RetainKey) -> Option<RetainedValue> {
        None
    }

    fn save_exiting_value(&self, _key: RetainKey, value: RetainedValue) {
        value.retire();
    }

    fn on_content_entered_composition(&self) {}

    fn on_content_exit_composition(&self) {}
}

thread_local! {
    static FORGETFUL: Rc<dyn RetainedValuesStore> = Rc::new(ForgetfulRetainedValuesStore);
}

/// The one [`ForgetfulRetainedValuesStore`] of the thread.
pub fn forgetful_retained_values_store() -> Rc<dyn RetainedValuesStore> {
    FORGETFUL.with(Rc::clone)
}

/// The store `retain` goes to, as `LocalRetainedValuesStore`.
pub static LOCAL_RETAINED_VALUES_STORE: CompositionLocal<Rc<dyn RetainedValuesStore>> =
    CompositionLocal::new_static("LocalRetainedValuesStore", forgetful_retained_values_store);

/// A store its owner says keeps or doesn't, as `ManagedRetainedValuesStore`: while its content is
/// out of the composition and it's enabled, what left stays, and what's not taken back when its
/// content comes in is retired.
pub struct ManagedRetainedValuesStore {
    enabled: Cell<bool>,
    disposed: Cell<bool>,
    content_composed: Cell<bool>,
    kept: RefCell<Vec<(RetainKey, RetainedValue)>>,
}

impl ManagedRetainedValuesStore {
    pub fn new() -> Rc<ManagedRetainedValuesStore> {
        Rc::new(ManagedRetainedValuesStore {
            enabled: Cell::new(true),
            disposed: Cell::new(false),
            content_composed: Cell::new(false),
            kept: RefCell::new(Vec::new()),
        })
    }

    /// # Panics
    ///
    /// Once it's disposed.
    pub fn enable_retaining_exited_values(&self) {
        assert!(
            !self.disposed.get(),
            "Cannot call enableRetainingExitedValues on a disposed store"
        );
        self.enabled.set(true);
    }

    /// What it keeps is retired, and what leaves from now on.
    pub fn disable_retaining_exited_values(&self) {
        self.enabled.set(false);
        self.purge();
    }

    pub fn dispose(&self) {
        self.disposed.set(true);
        self.disable_retaining_exited_values();
    }

    fn purge(&self) {
        let kept = std::mem::take(&mut *self.kept.borrow_mut());
        for (_, value) in kept {
            value.retire();
        }
    }
}

impl RetainedValuesStore for ManagedRetainedValuesStore {
    fn consume_exited_value(&self, key: &RetainKey) -> Option<RetainedValue> {
        let mut kept = self.kept.borrow_mut();
        let at = kept.iter().rposition(|(other, _)| other == key)?;
        Some(kept.remove(at).1)
    }

    fn save_exiting_value(&self, key: RetainKey, value: RetainedValue) {
        if self.is_retaining_exited_values() {
            self.kept.borrow_mut().push((key, value));
        } else {
            value.retire();
        }
    }

    fn on_content_entered_composition(&self) {
        if self.disposed.get() {
            return;
        }
        assert!(
            !self.content_composed.get(),
            "ManagedValuesStore tried to enter composition twice. Did you attempt to install the \
             same store multiple times or into two compositions?"
        );
        self.purge();
        self.content_composed.set(true);
    }

    fn on_content_exit_composition(&self) {
        if self.disposed.get() {
            return;
        }
        // Already unwinding from the misuse below it: a second panic would abort the process.
        if std::thread::panicking() {
            self.content_composed.set(false);
            return;
        }
        assert!(
            self.content_composed.get(),
            "ManagedValuesStore tried to leave composition twice. Is the store installed in \
             multiple places?"
        );
        assert!(
            self.kept.borrow().is_empty(),
            "Attempted to start retaining exited values with pending exited values"
        );
        self.content_composed.set(false);
    }

    fn is_retaining_exited_values(&self) -> bool {
        self.enabled.get() && !self.content_composed.get()
    }
}

/// A value in the composition: Compose's `RetainedValueHolder`.
struct Holder {
    key: RetainKey,
    value: RetainedValue,
    owner: RefCell<Rc<dyn RetainedValuesStore>>,
    newly_retained: Cell<bool>,
}

impl RememberObserver for Holder {
    fn on_remembered(&self) {
        if let Some(observer) = &self.value.observer {
            if self.newly_retained.replace(false) {
                observer.on_retained();
            }
            observer.on_entered_composition();
        }
    }

    fn on_forgotten(&self) {
        if let Some(observer) = &self.value.observer {
            observer.on_exited_composition();
        }
        let owner = self.owner.borrow().clone();
        owner.save_exiting_value(self.key.clone(), self.value.clone());
    }

    fn on_abandoned(&self) {
        if let Some(observer) = &self.value.observer {
            observer.on_unused();
        }
    }
}

fn retained_rc<T: 'static>(
    keys: Option<Rc<dyn RetainKeys>>,
    calculation: impl FnOnce() -> (Rc<T>, Option<Rc<dyn RetainObserver>>),
) -> Rc<T> {
    let key = RetainKey {
        keys,
        positional: current_composite_key_hash_code(),
        type_id: TypeId::of::<T>(),
    };
    let store = LOCAL_RETAINED_VALUES_STORE.current();
    let made = store.clone();
    let lookup = key.clone();
    let holder = remember_observer_keyed(key, move || {
        let (value, newly_retained) = match made.consume_exited_value(&lookup) {
            Some(value) => (value, false),
            None => {
                let (value, observer) = calculation();
                (RetainedValue { value, observer }, true)
            }
        };
        Holder {
            key: lookup,
            value,
            owner: RefCell::new(made),
            newly_retained: Cell::new(newly_retained),
        }
    });
    // Under another store now: it's that one's to keep.
    if !Rc::ptr_eq(&holder.owner.borrow(), &store) {
        let adopted = holder.clone();
        SideEffect(move || *adopted.owner.borrow_mut() = store.clone());
    }
    holder.value.value.clone().downcast::<T>().expect("its key holds its type")
}

/// A value that outlives its content if the store it's under keeps it, as `retain`: `calculation`'s,
/// or what the store kept of it. A value with callbacks goes through [`retain_observer`].
pub fn retain<T: Clone + 'static>(calculation: impl FnOnce() -> T) -> T {
    (*retained_rc::<T>(None, move || (Rc::new(calculation()), None))).clone()
}

/// [`retain`], a new value for new `keys`, as `retain(keys) { … }`.
pub fn retain_keyed<K: ComposeArg + 'static, T: Clone + 'static>(
    keys: K,
    calculation: impl FnOnce() -> T,
) -> T {
    (*retained_rc::<T>(Some(Rc::new(keys)), move || (Rc::new(calculation()), None))).clone()
}

/// [`retain`] of a [`RetainObserver`], shared with whoever holds it: it's told of its life.
pub fn retain_observer<T: RetainObserver>(calculation: impl FnOnce() -> T) -> Rc<T> {
    retained_rc::<T>(None, move || {
        let value = Rc::new(calculation());
        let observer: Rc<dyn RetainObserver> = value.clone();
        (value, Some(observer))
    })
}

/// [`retain_observer`], a new value for new `keys`.
pub fn retain_observer_keyed<K: ComposeArg + 'static, T: RetainObserver>(
    keys: K,
    calculation: impl FnOnce() -> T,
) -> Rc<T> {
    retained_rc::<T>(Some(Rc::new(keys)), move || {
        let value = Rc::new(calculation());
        let observer: Rc<dyn RetainObserver> = value.clone();
        (value, Some(observer))
    })
}

/// Tells a store when the content under it is there, after the content's own values: Compose's
/// `RetainContentPresenceIndicator`.
struct PresenceIndicator {
    store: Rc<dyn RetainedValuesStore>,
    recomposer: Recomposer,
    /// The composition it's in, whose tree's frame says the content's there.
    owner: Option<crate::runtime::CompositionId>,
    entered: Rc<Cell<bool>>,
    pending: Cell<Option<CancellationHandle>>,
}

impl RememberObserver for PresenceIndicator {
    fn on_remembered(&self) {
        let (store, entered) = (self.store.clone(), self.entered.clone());
        self.pending.set(Some(self.recomposer.schedule_frame_end_callback_of(
            self.owner,
            Box::new(move || {
                store.on_content_entered_composition();
                entered.set(true);
            }),
        )));
    }

    fn on_forgotten(&self) {
        if let Some(pending) = self.pending.take() {
            pending.cancel();
        }
        if self.entered.replace(false) {
            self.store.on_content_exit_composition();
        }
    }

    fn on_abandoned(&self) {
        if let Some(pending) = self.pending.take() {
            pending.cancel();
        }
    }
}

const PROVIDER_KEY: u64 = key_of("compose_runtime::LocalRetainedValuesStoreProvider");

/// `content` retaining in `store`, as `LocalRetainedValuesStoreProvider`.
#[allow(non_snake_case)]
pub fn LocalRetainedValuesStoreProvider(
    store: Rc<dyn RetainedValuesStore>,
    content: impl FnOnce(),
) {
    with_local_retained_values_store(store, content);
}

/// [`LocalRetainedValuesStoreProvider`] returning what `content` does, as
/// `withLocalRetainedValuesStore`.
pub fn with_local_retained_values_store<R>(
    store: Rc<dyn RetainedValuesStore>,
    content: impl FnOnce() -> R,
) -> R {
    let group =
        ReplaceGroup::start(PROVIDER_KEY, "compose_runtime::LocalRetainedValuesStoreProvider");
    let provided = LOCAL_RETAINED_VALUES_STORE.provides(store.clone());
    let result = with_composition_locals(provided, content);
    // After the content, for its callbacks to come in the right order around its values'.
    let recomposer =
        crate::runtime::composing_recomposer().unwrap_or_else(Recomposer::current_thread);
    let indicator_store = store.clone();
    let owner = crate::runtime::composing_composition();
    remember_observer_keyed(store, move || PresenceIndicator {
        store: indicator_store,
        recomposer,
        owner,
        entered: Rc::new(Cell::new(false)),
        pending: Cell::new(None),
    });
    drop(group);
    result
}

/// A [`ManagedRetainedValuesStore`] that's itself retained, as `retainManagedRetainedValuesStore`:
/// disposed once it's retired.
pub fn retain_managed_retained_values_store() -> Rc<ManagedRetainedValuesStore> {
    struct Wrapper(Rc<ManagedRetainedValuesStore>);
    impl RetainObserver for Wrapper {
        fn on_retired(&self) {
            self.0.dispose();
        }
        fn on_unused(&self) {
            self.0.dispose();
        }
    }
    retain_observer(|| Wrapper(ManagedRetainedValuesStore::new())).0.clone()
}

/// A store under another: its key hash, the key, and the store.
type RetainedChild = (u64, Rc<dyn Any>, Rc<ManagedRetainedValuesStore>);

/// Stores by key, for content that comes and goes by key, as `RetainedValuesStoreRegistry`.
#[derive(Default)]
pub struct RetainedValuesStoreRegistry {
    disposed: Cell<bool>,
    children: RefCell<Vec<RetainedChild>>,
}

impl RetainedValuesStoreRegistry {
    pub fn new() -> Rc<RetainedValuesStoreRegistry> {
        Rc::default()
    }

    /// `content` retaining in the store of `key`, as `LocalRetainedValuesStoreProvider(key)`.
    ///
    /// # Panics
    ///
    /// Once it's disposed.
    #[allow(non_snake_case)]
    pub fn LocalRetainedValuesStoreProvider<K: std::hash::Hash + 'static>(
        &self,
        key: K,
        content: impl FnOnce(),
    ) {
        let store: Rc<dyn RetainedValuesStore> = self.store_for_child(key);
        LocalRetainedValuesStoreProvider(store, content);
    }

    fn store_for_child<K: std::hash::Hash + 'static>(
        &self,
        key: K,
    ) -> Rc<ManagedRetainedValuesStore> {
        assert!(
            !self.disposed.get(),
            "Cannot get a RetainedValuesStore after a RetainedValuesStoreRegistry has been disposed."
        );
        let hash = hash_key(&key);
        let mut children = self.children.borrow_mut();
        if let Some((.., store)) = children.iter().find(|(other, ..)| *other == hash) {
            return store.clone();
        }
        let store = ManagedRetainedValuesStore::new();
        children.push((hash, Rc::new(key), store.clone()));
        store
    }

    /// The store of `key` goes, and what it keeps.
    pub fn clear_child<K: std::hash::Hash>(&self, key: K) {
        let key = hash_key(&key);
        let removed = {
            let mut children = self.children.borrow_mut();
            let at = children.iter().position(|(other, ..)| *other == key);
            at.map(|at| children.remove(at).2)
        };
        if let Some(store) = removed {
            store.dispose();
        }
    }

    /// The stores whose key `predicate` says go, and what they keep, as `clearChildren`.
    pub fn clear_children(&self, predicate: impl Fn(&dyn Any) -> bool) {
        let children = std::mem::take(&mut *self.children.borrow_mut());
        let (cleared, kept): (Vec<_>, Vec<_>) =
            children.into_iter().partition(|(_, key, _)| predicate(&**key));
        *self.children.borrow_mut() = kept;
        for (.., store) in cleared {
            store.dispose();
        }
    }

    /// Every store goes, and none's made after.
    pub fn dispose(&self) {
        self.disposed.set(true);
        self.clear_children(|_| true);
    }
}

/// A [`RetainedValuesStoreRegistry`] that's itself retained, as `retainRetainedValuesStoreRegistry`:
/// disposed once it's retired.
pub fn retain_retained_values_store_registry() -> Rc<RetainedValuesStoreRegistry> {
    struct Wrapper(Rc<RetainedValuesStoreRegistry>);
    impl RetainObserver for Wrapper {
        fn on_retired(&self) {
            self.0.dispose();
        }
        fn on_unused(&self) {
            self.0.dispose();
        }
    }
    retain_observer(|| Wrapper(RetainedValuesStoreRegistry::new())).0.clone()
}

/// What a [`RetainedEffect`] runs in, as `RetainedEffectScope`.
pub struct RetainedEffectScope(());

/// What runs when a [`RetainedEffect`] is retired, as `RetainedEffectResult`.
pub struct RetainedEffectResult(Box<dyn FnOnce()>);

impl RetainedEffectScope {
    /// What runs when it's retired, as `onRetire`.
    pub fn on_retire(&self, on_retired: impl FnOnce() + 'static) -> RetainedEffectResult {
        RetainedEffectResult(Box::new(on_retired))
    }
}

type RetainedEffectBody = Box<dyn FnOnce(&RetainedEffectScope) -> RetainedEffectResult>;

struct RetainedEffectImpl {
    effect: Cell<Option<RetainedEffectBody>>,
    on_retire: Cell<Option<RetainedEffectResult>>,
}

impl RetainObserver for RetainedEffectImpl {
    fn on_retained(&self) {
        if let Some(effect) = self.effect.take() {
            self.on_retire.set(Some(effect(&RetainedEffectScope(()))));
        }
    }

    fn on_retired(&self) {
        if let Some(on_retire) = self.on_retire.take() {
            (on_retire.0)();
        }
    }
}

/// An effect that runs when it's retained and whose cleanup runs when it's retired, as
/// `RetainedEffect`: it stays through its content's leaving while its store keeps it.
#[allow(non_snake_case)]
pub fn RetainedEffect<K: ComposeArg + 'static>(
    keys: K,
    effect: impl FnOnce(&RetainedEffectScope) -> RetainedEffectResult + 'static,
) {
    retain_observer_keyed(keys, move || RetainedEffectImpl {
        effect: Cell::new(Some(Box::new(effect))),
        on_retire: Cell::new(None),
    });
}
