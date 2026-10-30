//! Saving state across the host's recreation, as Compose's `androidx.compose.runtime.saveable`:
//! `remember_saveable` remembers a value as `remember` does, and also registers it with the
//! [`SaveableStateRegistry`] the host provides ([`LOCAL_SAVEABLE_STATE_REGISTRY`]). When the
//! platform is about to destroy the host (an Android configuration change, the process killed in
//! the background), it asks the registry for every registered value ([`perform_save`]), keeps
//! the bytes ([`SavedState::to_bytes`]), and gives them to the next host, whose
//! `remember_saveable` calls at the same places in the composition take them back instead of
//! computing their initial values.
//!
//! A value is saved as a [`SavedValue`] (booleans, numbers, strings and lists of them, what an
//! Android `Bundle` holds), converted by a [`Saver`]: the [`Saveable`] types (primitives,
//! `String`, `Option`, `Vec`, tuples) convert themselves ([`AutoSaver`], Compose's `autoSaver`);
//! others give a saver ([`saver`]). Values are found again by the composite key hash of their
//! place in the composition ([`current_composite_key_hash_code`]), as in Compose; values at the same
//! place (two calls in one composable) are told apart by their order.
//!
//! [`perform_save`]: SaveableStateRegistry::perform_save

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::rc::Rc;

pub use crate::saveable_state_holder::{SaveableStateHolder, remember_saveable_state_holder};

use crate::local::CompositionLocal;
use crate::remember::{Remember, RememberObserver, remember_observer};
use crate::runtime::with_runtime;
use crate::slot_table::{GroupKind, NONE};
use crate::state::{Owned, OwnedState, mutable_state_of};

/// A saved value: what a platform's saved state (an Android `Bundle`) can hold.
#[derive(Clone, PartialEq, Debug)]
pub enum SavedValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    List(Vec<SavedValue>),
}

/// A type that converts itself to and from a [`SavedValue`], as the types Compose's `autoSaver`
/// accepts (those a `Bundle` holds).
pub trait Saveable: Sized {
    fn save(&self) -> SavedValue;
    /// The value `saved` holds, or `None` if it holds another type (state saved by another
    /// version of the app).
    fn restore(saved: &SavedValue) -> Option<Self>;
}

impl Saveable for bool {
    fn save(&self) -> SavedValue {
        SavedValue::Bool(*self)
    }
    fn restore(saved: &SavedValue) -> Option<bool> {
        match saved {
            SavedValue::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

macro_rules! saveable_integers {
    ($($type:ty),*) => {
        $(impl Saveable for $type {
            fn save(&self) -> SavedValue {
                SavedValue::Int(*self as i64)
            }
            fn restore(saved: &SavedValue) -> Option<$type> {
                match saved {
                    SavedValue::Int(value) => <$type>::try_from(*value).ok(),
                    _ => None,
                }
            }
        })*
    };
}

saveable_integers!(i8, i16, i32, i64, isize, u8, u16, u32, usize);

impl Saveable for u64 {
    fn save(&self) -> SavedValue {
        // Its bits, as a `Bundle`'s `long`.
        SavedValue::Int(*self as i64)
    }
    fn restore(saved: &SavedValue) -> Option<u64> {
        match saved {
            SavedValue::Int(value) => Some(*value as u64),
            _ => None,
        }
    }
}

impl Saveable for f64 {
    fn save(&self) -> SavedValue {
        SavedValue::Float(*self)
    }
    fn restore(saved: &SavedValue) -> Option<f64> {
        match saved {
            SavedValue::Float(value) => Some(*value),
            _ => None,
        }
    }
}

impl Saveable for f32 {
    fn save(&self) -> SavedValue {
        SavedValue::Float(f64::from(*self))
    }
    fn restore(saved: &SavedValue) -> Option<f32> {
        match saved {
            SavedValue::Float(value) => Some(*value as f32),
            _ => None,
        }
    }
}

impl Saveable for char {
    fn save(&self) -> SavedValue {
        SavedValue::Int(i64::from(u32::from(*self)))
    }
    fn restore(saved: &SavedValue) -> Option<char> {
        match saved {
            SavedValue::Int(value) => u32::try_from(*value).ok().and_then(char::from_u32),
            _ => None,
        }
    }
}

impl Saveable for String {
    fn save(&self) -> SavedValue {
        SavedValue::Text(self.clone())
    }
    fn restore(saved: &SavedValue) -> Option<String> {
        match saved {
            SavedValue::Text(value) => Some(value.clone()),
            _ => None,
        }
    }
}

impl Saveable for Rc<str> {
    fn save(&self) -> SavedValue {
        SavedValue::Text(self.to_string())
    }
    fn restore(saved: &SavedValue) -> Option<Rc<str>> {
        match saved {
            SavedValue::Text(value) => Some(Rc::from(value.as_str())),
            _ => None,
        }
    }
}

/// `None` saves as an empty list, `Some(value)` as a list of one.
impl<T: Saveable> Saveable for Option<T> {
    fn save(&self) -> SavedValue {
        SavedValue::List(self.iter().map(Saveable::save).collect())
    }
    fn restore(saved: &SavedValue) -> Option<Option<T>> {
        match saved {
            SavedValue::List(values) if values.is_empty() => Some(None),
            SavedValue::List(values) if values.len() == 1 => T::restore(&values[0]).map(Some),
            _ => None,
        }
    }
}

impl<T: Saveable> Saveable for Vec<T> {
    fn save(&self) -> SavedValue {
        SavedValue::List(self.iter().map(Saveable::save).collect())
    }
    fn restore(saved: &SavedValue) -> Option<Vec<T>> {
        match saved {
            SavedValue::List(values) => values.iter().map(T::restore).collect(),
            _ => None,
        }
    }
}

macro_rules! saveable_tuples {
    ($(($($name:ident $index:tt),+))*) => {
        $(impl<$($name: Saveable),+> Saveable for ($($name,)+) {
            fn save(&self) -> SavedValue {
                SavedValue::List(vec![$(self.$index.save()),+])
            }
            fn restore(saved: &SavedValue) -> Option<Self> {
                let SavedValue::List(values) = saved else { return None };
                if values.len() != [$($index),+].len() {
                    return None;
                }
                Some(($($name::restore(&values[$index])?,)+))
            }
        })*
    };
}

saveable_tuples! {
    (A 0, B 1)
    (A 0, B 1, C 2)
    (A 0, B 1, C 2, D 3)
}

/// Converts values of `T` to and from saved values, as Compose's `Saver`.
pub trait Saver<T>: 'static {
    /// The saved form of `value`, or `None` to save nothing (it's computed again).
    fn save(&self, value: &T) -> Option<SavedValue>;
    /// The value `saved` holds, or `None` if it can't be restored (it's computed again).
    fn restore(&self, saved: &SavedValue) -> Option<T>;
}

/// The saver of [`Saveable`] types, as Compose's `autoSaver`.
#[derive(Clone, Copy, Default, Debug)]
pub struct AutoSaver;

impl<T: Saveable> Saver<T> for AutoSaver {
    fn save(&self, value: &T) -> Option<SavedValue> {
        Some(value.save())
    }
    fn restore(&self, saved: &SavedValue) -> Option<T> {
        T::restore(saved)
    }
}

/// A saver of two functions, as Compose's `Saver(save, restore)`.
pub struct FunctionSaver<T, Save, Restore> {
    save: Save,
    restore: Restore,
    marker: PhantomData<fn(T) -> T>,
}

impl<T: 'static, Save, Restore> Saver<T> for FunctionSaver<T, Save, Restore>
where
    Save: Fn(&T) -> Option<SavedValue> + 'static,
    Restore: Fn(&SavedValue) -> Option<T> + 'static,
{
    fn save(&self, value: &T) -> Option<SavedValue> {
        (self.save)(value)
    }
    fn restore(&self, saved: &SavedValue) -> Option<T> {
        (self.restore)(saved)
    }
}

/// A saver saving with `save` and restoring with `restore`, as Compose's `Saver(save, restore)`.
pub fn saver<T: 'static>(
    save: impl Fn(&T) -> Option<SavedValue> + 'static,
    restore: impl Fn(&SavedValue) -> Option<T> + 'static,
) -> impl Saver<T> {
    FunctionSaver { save, restore, marker: PhantomData }
}

/// A saver going through a list of `S`, as Compose's `listSaver`: `save` gives the list, `restore`
/// makes the value of it, or `None` to compute it again.
pub fn list_saver<T: 'static, S: Saveable + 'static>(
    save: impl Fn(&T) -> Vec<S> + 'static,
    restore: impl Fn(Vec<S>) -> Option<T> + 'static,
) -> impl Saver<T> {
    saver(
        move |value| Some(SavedValue::List(save(value).iter().map(Saveable::save).collect())),
        move |saved| {
            let SavedValue::List(items) = saved else { return None };
            restore(items.iter().map(S::restore).collect::<Option<Vec<S>>>()?)
        },
    )
}

/// A saver going through a map of names to saved values, as Compose's `mapSaver`: `save` gives the
/// entries, `restore` makes the value of them, or `None` to compute it again.
pub fn map_saver<T: 'static>(
    save: impl Fn(&T) -> Vec<(String, SavedValue)> + 'static,
    restore: impl Fn(HashMap<String, SavedValue>) -> Option<T> + 'static,
) -> impl Saver<T> {
    saver(
        move |value| {
            // Alternating, as Compose's: the flat list a `Bundle` holds.
            let mut flat = Vec::new();
            for (name, saved) in save(value) {
                flat.push(SavedValue::Text(name));
                flat.push(saved);
            }
            Some(SavedValue::List(flat))
        },
        move |saved| {
            let SavedValue::List(flat) = saved else { return None };
            let mut entries = HashMap::new();
            for [name, value] in flat.as_chunks::<2>().0 {
                let SavedValue::Text(name) = name else { return None };
                entries.insert(name.clone(), value.clone());
            }
            restore(entries)
        },
    )
}

/// The values a [`SaveableStateRegistry`] saved, by key; values with the same key in the order
/// they were registered.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SavedState {
    pub values: HashMap<u64, Vec<SavedValue>>,
}

/// The first bytes of an encoded [`SavedState`], with its format's version.
const MAGIC: &[u8; 4] = b"CRS1";

impl SavedState {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The state as bytes, for the platform to keep (an Android `Bundle`'s byte array):
    /// little-endian, each value a tag and its payload.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(self.values.len() as u32).to_le_bytes());
        // Sorted by key, so equal states encode equally.
        let mut keys: Vec<&u64> = self.values.keys().collect();
        keys.sort();
        for key in keys {
            let values = &self.values[key];
            bytes.extend_from_slice(&key.to_le_bytes());
            bytes.extend_from_slice(&(values.len() as u32).to_le_bytes());
            for value in values {
                write_value(value, &mut bytes);
            }
        }
        bytes
    }

    /// The state `bytes` encode ([`to_bytes`](Self::to_bytes)), or `None` if they're not one.
    pub fn from_bytes(bytes: &[u8]) -> Option<SavedState> {
        let mut reader = Reader { bytes, at: 0 };
        if reader.take(4)? != MAGIC {
            return None;
        }
        let count = reader.u32()?;
        let mut values = HashMap::new();
        for _ in 0..count {
            let key = reader.u64()?;
            let length = reader.u32()?;
            let mut list = Vec::new();
            for _ in 0..length {
                list.push(reader.value(0)?);
            }
            values.insert(key, list);
        }
        (reader.at == bytes.len()).then_some(SavedState { values })
    }
}

fn write_value(value: &SavedValue, bytes: &mut Vec<u8>) {
    match value {
        SavedValue::Bool(value) => bytes.extend_from_slice(&[0, u8::from(*value)]),
        SavedValue::Int(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        SavedValue::Float(value) => {
            bytes.push(2);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        SavedValue::Text(value) => {
            bytes.push(3);
            bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        SavedValue::List(values) => {
            bytes.push(4);
            bytes.extend_from_slice(&(values.len() as u32).to_le_bytes());
            for value in values {
                write_value(value, bytes);
            }
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let taken = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(taken)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    /// A value, nested `depth` lists deep (bounded: corrupt bytes don't overflow the stack).
    fn value(&mut self, depth: u32) -> Option<SavedValue> {
        if depth > 64 {
            return None;
        }
        Some(match self.take(1)?[0] {
            0 => SavedValue::Bool(self.take(1)?[0] != 0),
            1 => SavedValue::Int(self.u64()? as i64),
            2 => SavedValue::Float(f64::from_bits(self.u64()?)),
            3 => {
                let length = self.u32()? as usize;
                SavedValue::Text(String::from_utf8(self.take(length)?.to_vec()).ok()?)
            }
            4 => {
                let length = self.u32()?;
                let mut values = Vec::new();
                for _ in 0..length {
                    values.push(self.value(depth + 1)?);
                }
                SavedValue::List(values)
            }
            _ => return None,
        })
    }
}

/// What a registered value saves itself with.
type Provider = Rc<dyn Fn() -> Option<SavedValue>>;

#[derive(Default)]
struct RegistryInner {
    /// The values restored and not yet taken back, by key, in order.
    restored: RefCell<HashMap<u64, VecDeque<SavedValue>>>,
    /// The registered values, by key, in the order they were registered.
    providers: RefCell<HashMap<u64, Vec<(u64, Provider)>>>,
    next_entry: Cell<u64>,
}

/// Where `remember_saveable` registers its values and finds those saved, as Compose's
/// `SaveableStateRegistry`. A host owns one and provides it ([`LOCAL_SAVEABLE_STATE_REGISTRY`]).
#[derive(Clone, Default)]
pub struct SaveableStateRegistry {
    inner: Rc<RegistryInner>,
}

impl crate::ComposeArg for SaveableStateRegistry {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl SaveableStateRegistry {
    /// A registry giving back `restored` (what an earlier registry saved), if any.
    pub fn new(restored: Option<SavedState>) -> SaveableStateRegistry {
        let registry = SaveableStateRegistry::default();
        if let Some(restored) = restored {
            registry.restore(restored);
        }
        registry
    }

    /// Gives back `state` to the values registered from now on (what's left of an earlier
    /// restoration is replaced).
    pub fn restore(&self, state: SavedState) {
        *self.inner.restored.borrow_mut() =
            state.values.into_iter().map(|(key, values)| (key, values.into())).collect();
    }

    /// The next value saved under `key`, taken (each is given back once), as Compose's
    /// `consumeRestored`.
    pub fn consume_restored(&self, key: u64) -> Option<SavedValue> {
        let mut restored = self.inner.restored.borrow_mut();
        let values = restored.get_mut(&key)?;
        let value = values.pop_front();
        if values.is_empty() {
            restored.remove(&key);
        }
        value
    }

    /// Registers `provider`, called by [`perform_save`](Self::perform_save) for the value to
    /// save under `key`, until the entry returned is unregistered, as Compose's
    /// `registerProvider`.
    pub fn register_provider(
        &self,
        key: u64,
        provider: impl Fn() -> Option<SavedValue> + 'static,
    ) -> RegistryEntry {
        let id = self.inner.next_entry.get();
        self.inner.next_entry.set(id + 1);
        self.inner.providers.borrow_mut().entry(key).or_default().push((id, Rc::new(provider)));
        RegistryEntry { registry: Rc::downgrade(&self.inner), key, id }
    }

    /// Whether values are registered.
    pub fn has_providers(&self) -> bool {
        !self.inner.providers.borrow().is_empty()
    }

    /// Every registered value's saved form, by key, and the values restored and not yet taken
    /// back, as Compose's `performSave`. Values whose saver saved nothing are left out.
    pub fn perform_save(&self) -> SavedState {
        // Nothing to save (a lazy list item without saveable state, as it scrolls away).
        if self.inner.providers.borrow().is_empty() && self.inner.restored.borrow().is_empty() {
            return SavedState::default();
        }
        // The providers are called without the borrow: they read state, user code.
        let providers: Vec<(u64, Vec<Provider>)> = self
            .inner
            .providers
            .borrow()
            .iter()
            .map(|(key, entries)| {
                (*key, entries.iter().map(|(_, provider)| provider.clone()).collect())
            })
            .collect();
        // What was restored and not yet taken back is saved again (a pane not composed in this
        // configuration gets its values back in the next), as Compose's `performSave`.
        let mut values: HashMap<u64, Vec<SavedValue>> = self
            .inner
            .restored
            .borrow()
            .iter()
            .map(|(key, values)| (*key, values.iter().cloned().collect()))
            .collect();
        for (key, providers) in providers {
            let saved: Vec<SavedValue> =
                providers.iter().filter_map(|provider| provider()).collect();
            if saved.is_empty() {
                values.remove(&key);
            } else {
                values.insert(key, saved);
            }
        }
        SavedState { values }
    }
}

/// A registered value, as Compose's `SaveableStateRegistry.Entry`.
pub struct RegistryEntry {
    registry: std::rc::Weak<RegistryInner>,
    key: u64,
    id: u64,
}

impl RegistryEntry {
    /// Removes the value from the registry: it's no longer saved.
    pub fn unregister(&self) {
        let Some(registry) = self.registry.upgrade() else { return };
        let mut providers = registry.providers.borrow_mut();
        if let Some(entries) = providers.get_mut(&self.key) {
            entries.retain(|(id, _)| *id != self.id);
            if entries.is_empty() {
                providers.remove(&self.key);
            }
        }
    }
}

/// The registry `remember_saveable` uses, as Compose's `LocalSaveableStateRegistry`: none
/// outside a host that saves state (values are then only remembered).
pub static LOCAL_SAVEABLE_STATE_REGISTRY: CompositionLocal<Option<SaveableStateRegistry>> =
    CompositionLocal::new("LocalSaveableStateRegistry", || None);

/// A hash of the keys of the groups from the composition's root to the composable composing
/// now, as Compose's `currentCompositeKeyHash`: the same at the same place of the same
/// composition in another process, which is how saved values find their place again. Inside a
/// [`SaveableStateProvider`](crate::SaveableStateHolder::SaveableStateProvider), the groups
/// from the innermost provider's (unlike Compose's: its provider's registry is its values'
/// alone, so they're found wherever the provider moves).
///
/// # Panics
///
/// Outside composition.
pub fn current_composite_key_hash_code() -> u64 {
    with_runtime(|runtime| {
        let composition = runtime.check_composing();
        let group = runtime.frames.last().map_or(NONE, |frame| frame.group);
        composite_key_hash_at(&runtime.compositions, runtime.compositions.data(composition), group)
    })
}

/// Where `current_composite_key_hash_code` would be read: cheap to keep at each node, its hash
/// computed only if it's asked for (`composite_key_hash_of`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositeKeyToken {
    composition: u32,
    group: u32,
}

impl CompositeKeyToken {
    /// No place: its hash is 0.
    pub const NONE: CompositeKeyToken = CompositeKeyToken { composition: NONE, group: NONE };
}

/// The place composing now, for `composite_key_hash_of` later.
///
/// # Panics
///
/// Outside composition.
pub fn current_composite_key_token() -> CompositeKeyToken {
    with_runtime(|runtime| CompositeKeyToken {
        composition: runtime.check_composing(),
        group: runtime.frames.last().map_or(NONE, |frame| frame.group),
    })
}

/// `current_composite_key_hash_code` at a place kept by `current_composite_key_token`, while what's
/// composed there lives, its groups walked now, as `LayoutNode.compositeKeyHash`: a lazy list's
/// items differ by their providers' keys, and their registries are their own.
pub fn composite_key_hash_of(token: CompositeKeyToken) -> u64 {
    if token == CompositeKeyToken::NONE {
        return 0;
    }
    with_runtime(|runtime| {
        let data = runtime.compositions.data(token.composition);
        composite_key_hash_at(&runtime.compositions, data, token.group)
    })
}

pub(crate) fn composite_key_hash_at(
    compositions: &crate::runtime::Compositions,
    data: &crate::runtime::CompositionData,
    group: u32,
) -> u64 {
    let table = &data.table;
    let mut group = group;
    let mut hash = 0u64;
    while group != NONE {
        let entry = &table[group];
        // What a reusable composition's root adds isn't in a hash: it hashes as a plain one.
        if entry.key == crate::reusable::REUSE_ROOT_KEY {
            group = crate::slot_table::SENTINEL;
            continue;
        }
        if entry.key == crate::pausable::PAUSABLE_KEY
            && entry.parent != NONE
            && table[entry.parent].key == crate::reusable::REUSE_ROOT_KEY
        {
            group = entry.parent;
            continue;
        }
        // What's in movable content hashes alike wherever it moves.
        if entry.flags & crate::slot_table::MOVABLE != 0 {
            return hash.rotate_left(3) ^ entry.key;
        }
        // Groups of one key under one parent (two calls of a composable: keys here are per
        // composable, where Compose's are per call site) are told apart by their order among
        // themselves, whichever branch each is (Compose's `rGroupIndex` counts them all, so
        // one flipping leaves the others); a `key(…)` group by its key alone.
        let mut ordinal = 0u64;
        if entry.kind != GroupKind::Movable && entry.parent != NONE {
            let mut sibling = table[entry.parent].first_child;
            while sibling != NONE && sibling != group {
                let other = &table[sibling];
                if other.key == entry.key && other.kind == entry.kind {
                    ordinal += 1;
                }
                sibling = other.next_sibling;
            }
        }
        hash = hash.rotate_left(3)
            ^ entry.key
            ^ entry.data_key.rotate_left(29)
            ^ ordinal.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        // A `SaveableStateProvider`'s values are found by their place within it, wherever
        // it is: its registry is theirs alone (D290).
        if entry.kind == GroupKind::Reusable
            && entry.key == crate::saveable_state_holder::PROVIDER_GROUP
        {
            return hash;
        }
        group = entry.parent;
    }
    // A child composition's hash goes on from its context's, as Compose's starts from it, kept
    // when its place is gone.
    if let Some(place) = data.parent {
        hash = hash.rotate_left(3) ^ compositions.contexts.key_hash(place);
    }
    hash
}

/// What `remember_saveable` can remember: a `Clone` value (saved as itself), an
/// `OwnedState<V>` (its value saved, as Compose's `mutableStateSaver`) or an `Owned<H>` (a state
/// holder, saved by its saver), as [`Remember`] makes them.
pub trait RememberSaveable: Remember {
    /// What the saver converts.
    type Value: 'static;
    /// Calls `save` with the value `stored` holds.
    fn with_value<R>(stored: &Self::Stored, save: impl FnOnce(&Self::Value) -> R) -> R;
    fn from_value(value: Self::Value) -> Self;
}

impl<T: Clone + 'static> RememberSaveable for T {
    type Value = T;
    fn with_value<R>(stored: &T, save: impl FnOnce(&T) -> R) -> R {
        save(stored)
    }
    fn from_value(value: T) -> T {
        value
    }
}

impl<V: Clone + PartialEq + 'static> RememberSaveable for OwnedState<V> {
    type Value = V;
    fn with_value<R>(stored: &OwnedState<V>, save: impl FnOnce(&V) -> R) -> R {
        save(&stored.handle().get())
    }
    fn from_value(value: V) -> OwnedState<V> {
        mutable_state_of(value)
    }
}

impl<T: Clone + 'static> RememberSaveable for crate::snapshots::OwnedStateList<T> {
    type Value = Vec<T>;
    fn with_value<R>(stored: &Self, save: impl FnOnce(&Vec<T>) -> R) -> R {
        stored.handle().with(|items| save(items))
    }
    fn from_value(value: Vec<T>) -> Self {
        crate::snapshots::mutable_state_list_from(value)
    }
}

impl<T: Clone + std::hash::Hash + Eq + 'static> RememberSaveable
    for crate::snapshots::OwnedStateSet<T>
{
    type Value = Vec<T>;
    fn with_value<R>(stored: &Self, save: impl FnOnce(&Vec<T>) -> R) -> R {
        save(&stored.handle().iter_cloned().collect())
    }
    fn from_value(value: Vec<T>) -> Self {
        crate::snapshots::mutable_state_set_from(value)
    }
}

impl<H: Copy + 'static> RememberSaveable for Owned<H> {
    type Value = Owned<H>;
    fn with_value<R>(stored: &Owned<H>, save: impl FnOnce(&Owned<H>) -> R) -> R {
        save(stored)
    }
    fn from_value(value: Owned<H>) -> Owned<H> {
        value
    }
}

/// The value remembered, registered with the registry while it's in the composition.
struct SaveableHolder<S: 'static> {
    stored: Rc<S>,
    registration: Option<(SaveableStateRegistry, u64, Provider)>,
    entry: RefCell<Option<RegistryEntry>>,
}

impl<S: 'static> RememberObserver for SaveableHolder<S> {
    fn on_remembered(&self) {
        if let Some((registry, key, provider)) = &self.registration {
            let provider = provider.clone();
            *self.entry.borrow_mut() = Some(registry.register_provider(*key, move || provider()));
        }
    }

    fn on_forgotten(&self) {
        if let Some(entry) = self.entry.borrow_mut().take() {
            entry.unregister();
        }
    }
}

/// Like [`remember`](crate::remember), and saved across the host's recreation, as Compose's
/// `rememberSaveable { … }`: the first time this call runs at this place, it takes back the
/// value saved here by an earlier host, if any, or computes `init`. The value (a remembered
/// `OwnedState`'s value) must be [`Saveable`]; others take a saver
/// ([`remember_saveable_with`]).
///
/// ```ignore
/// let count = remember_saveable(|| mutable_state_of(0));
/// ```
pub fn remember_saveable<T>(init: impl FnOnce() -> T) -> T::Output
where
    T: RememberSaveable,
    T::Value: Saveable,
{
    remember_saveable_with(AutoSaver, init)
}

/// [`remember_saveable`] converting the value with `saver`, as Compose's
/// `rememberSaveable(saver = …) { … }` (for an `OwnedState`, its `stateSaver`).
pub fn remember_saveable_with<T: RememberSaveable>(
    saver: impl Saver<T::Value>,
    init: impl FnOnce() -> T,
) -> T::Output {
    let holder = remember_observer(move || {
        let registry = LOCAL_SAVEABLE_STATE_REGISTRY.current();
        let key = current_composite_key_hash_code();
        let restored = registry
            .as_ref()
            .and_then(|registry| registry.consume_restored(key))
            .and_then(|saved| saver.restore(&saved));
        let value = match restored {
            Some(value) => T::from_value(value),
            None => init(),
        };
        let stored = Rc::new(value.into_stored());
        let registration = registry.map(|registry| {
            let stored = stored.clone();
            let provider: Provider =
                Rc::new(move || T::with_value(&stored, |value| saver.save(value)));
            (registry, key, provider)
        });
        SaveableHolder { stored, registration, entry: RefCell::new(None) }
    });
    T::output(&holder.stored)
}
