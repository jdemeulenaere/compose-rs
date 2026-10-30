//! Composition locals, as Compose's `CompositionLocal`: values provided to a subtree and read
//! anywhere inside it without being passed down.
//!
//! A provider is a replace group (`CompositionLocalProvider`, one key for all, as Compose's
//! `providerKey`) whose first slot holds what it provides: an entry per local, each a holder (a
//! state for a dynamic local, the value for a static one, a function for a computed one).
//! `current()` walks up the slot table from the group composing now through the provider groups
//! only (each group records, when it is allocated, the nearest provider's group above it: a
//! group's ancestors never change) to the nearest one providing the local, so a scope
//! restarting on its own finds the same provider as when its parent composed it, and a read
//! costs the providers above it, not the groups between them.
//!
//! - A **dynamic** local (`CompositionLocal::new`, Compose's `compositionLocalOf`) is held in a
//!   state: reading it subscribes, and a new value recomposes exactly the readers.
//! - A **static** local (`new_static`, `staticCompositionLocalOf`) is held as a value: reading it
//!   records nothing, and a provider whose value changes recomposes its whole content.
//! - A **computed** one (`with_computed_default`, `provides_computed`) is a function of other
//!   locals, run at each read where it's read.

use std::any::{Any, TypeId};
use std::rc::Rc;

use crate::__private::ReplaceGroup;
use crate::compose_arg::ComposeArg;
use crate::hash::key_of;
use crate::remember::remember;
use crate::runtime::{CompositionData, CompositionId, Compositions, Runtime, with_runtime};
use crate::slot_table::NONE;
use crate::state::{MutationPolicy, OwnedState};

/// The key of every provider group, as Compose's `providerKey`.
const PROVIDER_KEY: u64 = key_of("compose_runtime::CompositionLocalProvider");

/// A composition local: declared as a `static` (or a `const`), identified by its name and
/// value type, which must be unique together.
///
/// ```ignore
/// pub static LOCAL_CONTENT_COLOR: CompositionLocal<Color> =
///     CompositionLocal::new("LocalContentColor", || Color::BLACK);
/// ```
pub struct CompositionLocal<T: 'static> {
    name: &'static str,
    is_static: bool,
    default: LocalDefault<T>,
    /// When a newly provided value is the same as the last one (the dynamic state's policy, a
    /// static provider's comparison).
    equivalent: fn(&T, &T) -> bool,
}

/// What a local is outside any provider.
enum LocalDefault<T: 'static> {
    Value(fn() -> T),
    Computed(fn(&CompositionLocalAccessorScope) -> T),
}

/// Where a computed local reads other locals, as Compose's `CompositionLocalAccessorScope`:
/// where the computed value is read, or in a [`CompositionLocalMap`].
pub struct CompositionLocalAccessorScope {
    map: Option<CompositionLocalMap>,
    anchor: Option<CompositionLocalAnchor>,
}

impl CompositionLocalAccessorScope {
    /// The value of `local` where the computed value is read, as `currentValue`.
    pub fn current_value<T: Clone + 'static>(&self, local: &CompositionLocal<T>) -> T {
        match (&self.map, &self.anchor) {
            (Some(map), _) => map.get(local),
            (None, Some(anchor)) => anchor.get(local),
            (None, None) => local.current(),
        }
    }
}

/// `probe.resolve(make())` for a `provides_computed` closure, with its scope parameter's type
/// spelled out (a higher-ranked borrow, which `resolve_lazy_1` can't name).
#[doc(hidden)]
#[inline]
pub fn resolve_lazy_computed<K: 'static, T: 'static, C>(
    probe: crate::callback::LambdaProbe<K, dyn Fn(&CompositionLocalAccessorScope) -> T>,
    make: impl FnOnce() -> C,
) -> crate::callback::Callback<dyn Fn(&CompositionLocalAccessorScope) -> T>
where
    C: Fn(&CompositionLocalAccessorScope) -> T + 'static,
{
    match probe {
        crate::callback::LambdaProbe::Hit(callback) => callback,
        crate::callback::LambdaProbe::Miss(pending) => {
            pending.store_callback(crate::callback::Callback::from_rc(Rc::new(make())))
        }
    }
}

/// A value for a local, for [`CompositionLocalProvider`], as Compose's `ProvidedValue`: made by
/// [`provides`](CompositionLocal::provides), [`provides_default`](CompositionLocal::provides_default)
/// or [`provides_computed`](CompositionLocal::provides_computed).
pub struct ProvidedValue<T: 'static> {
    local: u64,
    name: &'static str,
    is_static: bool,
    can_override: bool,
    equivalent: fn(&T, &T) -> bool,
    value: Provided<T>,
}

enum Provided<T: 'static> {
    Value(T),
    Computed(Rc<dyn Fn(&CompositionLocalAccessorScope) -> T>),
}

impl<T: Clone + 'static> Clone for ProvidedValue<T> {
    fn clone(&self) -> Self {
        ProvidedValue {
            value: match &self.value {
                Provided::Value(value) => Provided::Value(value.clone()),
                Provided::Computed(compute) => Provided::Computed(compute.clone()),
            },
            ..*self
        }
    }
}

impl<T: 'static> ProvidedValue<T> {
    /// Whether it replaces a value provided above, as `canOverride`: not for
    /// [`provides_default`](CompositionLocal::provides_default).
    pub fn can_override(&self) -> bool {
        self.can_override
    }

    /// The value provided, if it isn't computed.
    pub fn value(&self) -> Option<&T> {
        match &self.value {
            Provided::Value(value) => Some(value),
            Provided::Computed(_) => None,
        }
    }

    /// The name of the local it's for.
    pub fn local_name(&self) -> &'static str {
        self.name
    }
}

/// What a provider holds for a local.
enum Holder<T: 'static> {
    /// A dynamic local's value: reading it subscribes.
    Dynamic(OwnedState<T>),
    /// A static local's value: reading it records nothing.
    Static(T),
    Computed(Rc<dyn Fn(&CompositionLocalAccessorScope) -> T>),
}

impl<T: Clone + 'static> Holder<T> {
    fn read(&self, scope: &CompositionLocalAccessorScope) -> T {
        match self {
            Holder::Dynamic(state) => state.get(),
            Holder::Static(value) => value.clone(),
            Holder::Computed(compute) => compute(scope),
        }
    }
}

/// A local's holder in a provider: `Rc<Holder<T>>` erased.
#[derive(Clone)]
struct Entry {
    local: u64,
    holder: Rc<dyn Any>,
    /// Made by this provider: its dynamic state may be written in place. One from a
    /// [`CompositionLocalContext`] belongs to the provider it was captured from.
    owned: bool,
    /// Given in the provider's pass now: entries not given are dropped after it (checked in
    /// place, so a provider's pass allocates nothing).
    given: bool,
}

/// A provider group's first slot: the locals it provides.
#[derive(Default)]
struct ProviderEntries {
    entries: std::cell::RefCell<Vec<Entry>>,
}

impl<T: 'static> CompositionLocal<T> {
    /// A dynamic local whose value outside any provider is `default()`, as `compositionLocalOf`:
    /// reading it subscribes, and a provider's new value (not [`same`](ComposeArg::same) as the
    /// last) recomposes exactly its readers.
    pub const fn new(name: &'static str, default: fn() -> T) -> CompositionLocal<T>
    where
        T: ComposeArg,
    {
        CompositionLocal {
            name,
            is_static: false,
            default: LocalDefault::Value(default),
            equivalent: <T as ComposeArg>::same,
        }
    }

    /// A dynamic local whose provided values change when `policy` says so, as
    /// `compositionLocalOf(policy) { … }`.
    pub const fn with_policy<P: MutationPolicy<T> + Copy>(
        name: &'static str,
        _policy: P,
        default: fn() -> T,
    ) -> CompositionLocal<T> {
        CompositionLocal {
            name,
            is_static: false,
            default: LocalDefault::Value(default),
            equivalent: P::equivalent,
        }
    }

    /// A static local, as `staticCompositionLocalOf`: reading it records nothing, and a
    /// provider whose value changes recomposes its whole content. For values that rarely
    /// change, read in many places.
    pub const fn new_static(name: &'static str, default: fn() -> T) -> CompositionLocal<T>
    where
        T: ComposeArg,
    {
        CompositionLocal {
            name,
            is_static: true,
            default: LocalDefault::Value(default),
            equivalent: <T as ComposeArg>::same,
        }
    }

    /// A dynamic local whose value outside any provider is computed from other locals where
    /// it's read, as `compositionLocalWithComputedDefaultOf { … }`.
    pub const fn with_computed_default(
        name: &'static str,
        compute: fn(&CompositionLocalAccessorScope) -> T,
    ) -> CompositionLocal<T>
    where
        T: ComposeArg,
    {
        CompositionLocal {
            name,
            is_static: false,
            default: LocalDefault::Computed(compute),
            equivalent: <T as ComposeArg>::same,
        }
    }

    /// Like [`with_computed_default`](CompositionLocal::with_computed_default), static when
    /// provided, as `staticCompositionLocalWithComputedDefaultOf { … }`.
    pub const fn static_with_computed_default(
        name: &'static str,
        compute: fn(&CompositionLocalAccessorScope) -> T,
    ) -> CompositionLocal<T>
    where
        T: ComposeArg,
    {
        CompositionLocal {
            name,
            is_static: true,
            default: LocalDefault::Computed(compute),
            equivalent: <T as ComposeArg>::same,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Its identity: its name and its type (an address isn't one: a `const` has a new one at
    /// each use).
    #[doc(hidden)]
    pub fn id(&self) -> u64 {
        key_of(self.name) ^ key_of(std::any::type_name::<T>()).rotate_left(29)
    }

    fn provided(&self, value: Provided<T>, can_override: bool) -> ProvidedValue<T> {
        ProvidedValue {
            local: self.id(),
            name: self.name,
            is_static: self.is_static,
            can_override,
            equivalent: self.equivalent,
            value,
        }
    }

    /// `value` for this local, as `local provides value`.
    pub fn provides(&self, value: T) -> ProvidedValue<T> {
        self.provided(Provided::Value(value), true)
    }

    /// `value` for this local unless a provider above provides it already, as
    /// `local providesDefault value`.
    pub fn provides_default(&self, value: T) -> ProvidedValue<T> {
        self.provided(Provided::Value(value), false)
    }

    /// A value computed from other locals where this one is read, as
    /// `local providesComputed { … }`. A new function makes the provider recompose its content.
    /// In a `#[composable]` function, a closure written in the call is memoized on its captures,
    /// as lambdas are (D717): the provider recomposing with the same captures keeps the function.
    pub fn provides_computed(
        &self,
        compute: impl Fn(&CompositionLocalAccessorScope) -> T + 'static,
    ) -> ProvidedValue<T> {
        self.provided(Provided::Computed(Rc::new(compute)), true)
    }

    /// [`provides_computed`](Self::provides_computed) of a function memoized on its captures:
    /// what the `#[composable]` macro makes of `provides_computed(closure)` (D717).
    #[doc(hidden)]
    pub fn provides_computed_memoized(
        &self,
        compute: crate::callback::Callback<dyn Fn(&CompositionLocalAccessorScope) -> T>,
    ) -> ProvidedValue<T> {
        self.provided(Provided::Computed(compute.into_rc()), true)
    }

    /// The value of the nearest provider above the composable composing now, or the default,
    /// as `current`. Reading a dynamic local subscribes the composing scope: a new value
    /// recomposes it.
    ///
    /// # Panics
    ///
    /// Outside composition.
    pub fn current(&self) -> T
    where
        T: Clone,
    {
        let here = CompositionLocalAccessorScope { map: None, anchor: None };
        match find_provided(self.id()) {
            Some(holder) => holder
                .downcast_ref::<Holder<T>>()
                .expect("a local's type is in its identity")
                .read(&here),
            None => self.default_value(&here),
        }
    }

    fn default_value(&self, scope: &CompositionLocalAccessorScope) -> T {
        match self.default {
            LocalDefault::Value(default) => default(),
            LocalDefault::Computed(compute) => compute(scope),
        }
    }
}

/// The holder of the nearest provider of `local` above the composable composing now.
fn find_provided(local: u64) -> Option<Rc<dyn Any>> {
    with_runtime(|runtime| {
        let composition = runtime.check_composing();
        let data = runtime.compositions.data(composition);
        let composing = runtime.frames.last().map_or(NONE, |frame| frame.group);
        let group = data.table.provider_at_or_above(composing);
        find_from(&runtime.compositions, data, group, local)
    })
}

/// The holder of `local` in the provider `group` of `data` or those above it, going on above a
/// child composition's root at its context.
fn find_from(
    compositions: &Compositions,
    data: &CompositionData,
    group: u32,
    local: u64,
) -> Option<Rc<dyn Any>> {
    let (mut data, mut group) = (data, group);
    loop {
        let table = &data.table;
        while group != NONE {
            if let Some(holder) = entries_of(table, group).and_then(|entries| {
                let entries = entries.entries.borrow();
                entries.iter().find(|entry| entry.local == local).map(|entry| entry.holder.clone())
            }) {
                return Some(holder);
            }
            group = table[group].enclosing_provider;
        }
        match above_root(compositions, data) {
            Some(above) => (data, group) = above,
            None => return in_kept_locals(compositions, data, local),
        }
    }
}

/// The composition a child composition's root is under and the nearest provider there: where
/// what's provided goes on from, as Compose's `parentContext.getCompositionLocalScope()`.
fn above_root<'a>(
    compositions: &'a Compositions,
    data: &CompositionData,
) -> Option<(&'a CompositionData, u32)> {
    compositions.context_provider(data.parent?)
}

/// The holder of `local` in what `data`'s context provided when it went, as Compose's keeps it.
fn in_kept_locals(
    compositions: &Compositions,
    data: &CompositionData,
    local: u64,
) -> Option<Rc<dyn Any>> {
    let map = compositions.kept_locals_of(data.parent?)?;
    map.entries.iter().find(|entry| entry.local == local).map(|entry| entry.holder.clone())
}

/// What's provided at `group`, a provider of `data`, and above it, as a map.
pub(crate) fn map_at(
    compositions: &Compositions,
    data: &CompositionData,
    group: u32,
) -> CompositionLocalMap {
    CompositionLocalMap { entries: entries_visible_from(compositions, data, group).into() }
}

/// A provider group's entries.
fn entries_of(table: &crate::slot_table::SlotTable, group: u32) -> Option<Rc<ProviderEntries>> {
    let slot = table[group].slots.first()?;
    if slot.value.value_type_id() != TypeId::of::<Rc<ProviderEntries>>() {
        return None;
    }
    slot.value.to_any()?.downcast_ref::<Rc<ProviderEntries>>().cloned()
}

/// What the providers above the composable composing now provide, nearest first, a local
/// once: what [`current_composition_local_map`] and [`current_composition_local_context`]
/// capture.
fn visible_entries() -> Vec<Entry> {
    with_runtime(|runtime| {
        let composition = runtime.check_composing();
        let data = runtime.compositions.data(composition);
        let composing = runtime.frames.last().map_or(NONE, |frame| frame.group);
        let group = data.table.provider_at_or_above(composing);
        entries_visible_from(&runtime.compositions, data, group)
    })
}

/// What `group`, a provider, and those above it provide, nearest first, a local once.
fn entries_visible_from(
    compositions: &Compositions,
    data: &CompositionData,
    group: u32,
) -> Vec<Entry> {
    let (mut data, mut group) = (data, group);
    let mut visible: Vec<Entry> = Vec::new();
    loop {
        let table = &data.table;
        while group != NONE {
            if let Some(entries) = entries_of(table, group) {
                for entry in entries.entries.borrow().iter() {
                    if !visible.iter().any(|seen| seen.local == entry.local) {
                        visible.push(Entry { owned: false, given: false, ..entry.clone() });
                    }
                }
            }
            group = table[group].enclosing_provider;
        }
        let Some(above) = above_root(compositions, data) else {
            if let Some(map) = data.parent.and_then(|place| compositions.kept_locals_of(place)) {
                for entry in map.entries.iter() {
                    if !visible.iter().any(|seen| seen.local == entry.local) {
                        visible.push(Entry { owned: false, given: false, ..entry.clone() });
                    }
                }
            }
            return visible;
        };
        (data, group) = above;
    }
}

/// Whether what's provided at two provider groups of a composition is the same.
pub(crate) fn same_locals_at(
    compositions: &Compositions,
    composition: u32,
    first: u32,
    second: u32,
) -> bool {
    let data = compositions.data(composition);
    let visible = |group| entries_visible_from(compositions, data, group);
    let (first, second) = (visible(first), visible(second));
    first.len() == second.len()
        && first.iter().all(|entry| {
            second
                .iter()
                .any(|other| other.local == entry.local && Rc::ptr_eq(&other.holder, &entry.holder))
        })
}

/// Whether a provider above the provider group starting now provides `local`.
fn provided_above(local: u64) -> bool {
    with_runtime(|runtime| {
        let composition = runtime.check_composing();
        let data = runtime.compositions.data(composition);
        let provider = runtime.frames.last().map_or(NONE, |frame| frame.group);
        let table = &data.table;
        let group = if provider == NONE { NONE } else { table[provider].enclosing_provider };
        find_from(&runtime.compositions, data, group, local).is_some()
    })
}

/// The locals provided at a point, as Compose's `CompositionLocalMap`
/// (`currentComposer.currentCompositionLocalMap`): [`get`](CompositionLocalMap::get) reads one
/// as `current()` would have there.
#[derive(Clone, Default)]
pub struct CompositionLocalMap {
    entries: Rc<[Entry]>,
}

impl CompositionLocalMap {
    /// A map providing nothing, as `CompositionLocalMap.Empty`: every local reads its default.
    pub fn empty() -> CompositionLocalMap {
        CompositionLocalMap::default()
    }

    /// The value of `local` in the map, or its default. Reading a dynamic local during
    /// composition subscribes the composing scope, as reading it through `current()`.
    pub fn get<T: Clone + 'static>(&self, local: &CompositionLocal<T>) -> T {
        let scope = CompositionLocalAccessorScope { map: Some(self.clone()), anchor: None };
        let id = local.id();
        match self.entries.iter().find(|entry| entry.local == id) {
            Some(entry) => entry
                .holder
                .downcast_ref::<Holder<T>>()
                .expect("a local's type is in its identity")
                .read(&scope),
            None => local.default_value(&scope),
        }
    }
}

impl std::fmt::Debug for CompositionLocalMap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "CompositionLocalMap({} locals)", self.entries.len())
    }
}

impl ComposeArg for CompositionLocalMap {
    /// The same holders, as Compose's map `equals` compares its holders.
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.entries, &other.entries)
            || (self.entries.len() == other.entries.len()
                && self
                    .entries
                    .iter()
                    .zip(other.entries.iter())
                    .all(|(a, b)| a.local == b.local && Rc::ptr_eq(&a.holder, &b.holder)))
    }
}

/// The locals provided where [`current_composition_local_map`] is called.
pub fn current_composition_local_map() -> CompositionLocalMap {
    CompositionLocalMap { entries: visible_entries().into() }
}

/// Where the locals are read at a point of a composition: what a layout node keeps, as Compose's
/// `LayoutNode.compositionLocalMap`, for its modifier nodes' `currentValueOf`. Made per layout
/// node, so it's the nearest provider group and nothing else: no map is built, and
/// [`get`](CompositionLocalAnchor::get) walks the providers when asked, outside composition too.
/// Two anchors are equal while what they read can only have changed through a dynamic local's
/// state: a provider's static value or its set of locals changing moves its version
/// (`Group::provider_version`) and those of the providers in its content, not anyone else's.
/// An anchor whose provider has moved on, or whose composition is gone, reads defaults: its node
/// is about to be composed again, or gone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CompositionLocalAnchor {
    composition: CompositionId,
    provider: u32,
    version: u32,
}

impl Default for CompositionLocalAnchor {
    fn default() -> CompositionLocalAnchor {
        CompositionLocalAnchor::NONE
    }
}

impl CompositionLocalAnchor {
    /// Reads no provider: every local reads its default.
    pub const NONE: CompositionLocalAnchor = CompositionLocalAnchor {
        composition: CompositionId { index: NONE, generation: 0 },
        provider: NONE,
        version: 0,
    };

    /// Its composition, if its provider is as it was. A composition being disposed is still there
    /// for its nodes' `on_detach`.
    fn data<'a>(&self, runtime: &'a Runtime) -> Option<&'a CompositionData> {
        let disposing = runtime.disposing_data.iter().find(|(id, _)| *id == self.composition);
        let data = match disposing {
            Some((_, data)) => data,
            None => runtime.compositions.get(self.composition)?,
        };
        let provider = data.table.groups.get(self.provider as usize)?;
        (provider.kind != crate::slot_table::GroupKind::Free
            && provider.provider_version() == self.version)
            .then_some(data)
    }

    fn table<'a>(&self, runtime: &'a Runtime) -> Option<&'a crate::slot_table::SlotTable> {
        self.data(runtime).map(|data| &data.table)
    }

    /// The value of `local` here, or its default, as `currentValueOf`. A dynamic local is a
    /// state's value: reading it is observed where reads are (measuring, drawing, a composing
    /// scope). An anchor outlives nothing: its composition gone, it reads defaults.
    pub fn get<T: Clone + 'static>(&self, local: &CompositionLocal<T>) -> T {
        let scope = CompositionLocalAccessorScope { map: None, anchor: Some(*self) };
        let id = local.id();
        match self.find(id) {
            Some(holder) => holder
                .downcast_ref::<Holder<T>>()
                .expect("a local's type is in its identity")
                .read(&scope),
            None => local.default_value(&scope),
        }
    }

    /// What's provided here, as a map: for a node reading locals often, each read then a look
    /// through what's provided rather than a walk up the providers.
    pub fn map(&self) -> CompositionLocalMap {
        if self.provider == NONE {
            return CompositionLocalMap::empty();
        }
        let entries = with_runtime(|runtime| {
            let data = self.data(runtime);
            data.map_or_else(Vec::new, |data| {
                entries_visible_from(&runtime.compositions, data, self.provider)
            })
        });
        CompositionLocalMap { entries: entries.into() }
    }

    /// Whether static `locals` read here what they read at `other`, known with a look at two
    /// provider groups: `other`'s nearest, the one around it, or a sibling of it, none of those
    /// that differ providing them. Saves the walk up to the providers of a layout node's density
    /// when a list's item is a provider of its own (`false` only says it isn't known).
    #[doc(hidden)]
    pub fn reads_same_statics_as(&self, other: &CompositionLocalAnchor, locals: &[u64]) -> bool {
        if self.composition != other.composition {
            return false;
        }
        if self.provider == other.provider {
            return self.version == other.version;
        }
        if self.provider == NONE || other.provider == NONE {
            return false;
        }
        with_runtime(|runtime| {
            // Gone or moved on, what it gave `other` isn't known.
            let Some(table) = other.table(runtime) else { return false };
            let Some(mine) = table.groups.get(self.provider as usize) else { return false };
            let theirs = &table.groups[other.provider as usize];
            let provides = |group: u32| {
                entries_of(table, group).is_some_and(|entries| {
                    let entries = entries.entries.borrow();
                    entries.iter().any(|entry| locals.contains(&entry.local))
                })
            };
            // Versions only grow: what's around it changed after `other` read it if its version
            // is the later, the nodes being composed one at a time.
            let unchanged_since = |around: u32| {
                around == NONE || table.groups[around as usize].provider_version() < other.version
            };
            if mine.enclosing_provider == other.provider {
                !provides(self.provider)
            } else if theirs.enclosing_provider == self.provider {
                unchanged_since(self.provider) && !provides(other.provider)
            } else {
                mine.enclosing_provider == theirs.enclosing_provider
                    && unchanged_since(mine.enclosing_provider)
                    && !provides(self.provider)
                    && !provides(other.provider)
            }
        })
    }

    fn find(&self, local: u64) -> Option<Rc<dyn Any>> {
        if self.provider == NONE {
            return None;
        }
        with_runtime(|runtime| {
            find_from(&runtime.compositions, self.data(runtime)?, self.provider, local)
        })
    }
}

/// The anchor of the composable composing now. Costs a look at its group: what every layout
/// node's emission asks.
pub fn current_composition_local_anchor() -> CompositionLocalAnchor {
    with_runtime(|runtime| {
        let composition = runtime.check_composing();
        let mut id = runtime.active.expect("composing").composition;
        let compositions = &runtime.compositions;
        let mut data = compositions.data(composition);
        let composing = runtime.frames.last().map_or(NONE, |frame| frame.group);
        let mut provider = data.table.provider_at_or_above(composing);
        // In a child composition with none of its own: what's above its root is what it reads, so
        // no provider here says there's none at all.
        while provider == NONE {
            let Some((above, above_data, place)) =
                data.parent.and_then(|place| compositions.context_at(place))
            else {
                break;
            };
            (id, data) = (above, above_data);
            provider = data.table.provider_at_or_above(place);
        }
        let version = if provider == NONE { 0 } else { data.table[provider].provider_version() };
        CompositionLocalAnchor { composition: id, provider, version }
    })
}

/// Every local provided at a point, to provide elsewhere (another composition, a dialog), as
/// Compose's `CompositionLocalContext`: give it to [`CompositionLocalProvider`]. A dynamic
/// local keeps following its provider's value.
#[derive(Clone, Debug, Default)]
pub struct CompositionLocalContext {
    map: CompositionLocalMap,
}

impl ComposeArg for CompositionLocalContext {
    fn same(&self, other: &Self) -> bool {
        self.map.same(&other.map)
    }
}

/// The locals provided where it's called, as `currentCompositionLocalContext`.
pub fn current_composition_local_context() -> CompositionLocalContext {
    CompositionLocalContext { map: current_composition_local_map() }
}

/// What a provider applies: the entries it had, updated by what it's given now.
#[doc(hidden)]
pub struct ProviderUpdate {
    /// The entries, each marked as it's given.
    entries: Vec<Entry>,
    /// An entry was added, removed or replaced: the content recomposes as a whole (Compose's
    /// `providersInvalid`).
    changed: bool,
}

impl ProviderUpdate {
    fn entry(&self, local: u64) -> Option<usize> {
        self.entries.iter().position(|entry| entry.local == local)
    }

    fn replace(&mut self, entry: Entry) {
        self.changed = true;
        match self.entry(entry.local) {
            Some(index) => self.entries[index] = entry,
            None => self.entries.push(entry),
        }
    }

    fn give<T: Clone + 'static>(&mut self, provided: ProvidedValue<T>) {
        let ProvidedValue { local, is_static, can_override, equivalent, value, .. } = provided;
        // `providesDefault`: nothing if a provider above provides it.
        if !can_override && provided_above(local) {
            return;
        }
        let existing = self.entry(local).map(|index| {
            self.entries[index].given = true;
            self.entries[index].clone()
        });
        let holder = existing.as_ref().and_then(|entry| {
            entry.holder.clone().downcast::<Holder<T>>().ok().map(|holder| (holder, entry.owned))
        });
        match (holder, value) {
            // A dynamic value: written in place, its readers recompose.
            (Some((holder, true)), Provided::Value(value)) if !is_static => {
                if let Holder::Dynamic(state) = &*holder {
                    state.set(value);
                    return;
                }
                self.replace(new_entry(local, is_static, equivalent, Provided::Value(value)));
            }
            (Some((holder, _)), Provided::Value(value)) if is_static => {
                if let Holder::Static(old) = &*holder
                    && equivalent(old, &value)
                {
                    return;
                }
                self.replace(new_entry(local, is_static, equivalent, Provided::Value(value)));
            }
            (Some((holder, _)), Provided::Computed(compute)) => {
                if let Holder::Computed(old) = &*holder
                    && Rc::ptr_eq(old, &compute)
                {
                    return;
                }
                self.replace(new_entry(local, is_static, equivalent, Provided::Computed(compute)));
            }
            (_, value) => self.replace(new_entry(local, is_static, equivalent, value)),
        }
    }

    fn give_context(&mut self, context: CompositionLocalContext) {
        for entry in context.map.entries.iter() {
            let index = self.entry(entry.local);
            if let Some(index) = index {
                self.entries[index].given = true;
            }
            let same =
                index.is_some_and(|index| Rc::ptr_eq(&self.entries[index].holder, &entry.holder));
            if !same {
                self.replace(Entry { owned: false, given: true, ..entry.clone() });
            }
        }
    }
}

/// A new holder for a provided value.
fn new_entry<T: Clone + 'static>(
    local: u64,
    is_static: bool,
    equivalent: fn(&T, &T) -> bool,
    value: Provided<T>,
) -> Entry {
    let holder = match value {
        Provided::Value(value) if is_static => Holder::Static(value),
        Provided::Value(value) => {
            Holder::Dynamic(crate::state::mutable_state_with_equivalence(value, equivalent))
        }
        Provided::Computed(compute) => Holder::Computed(compute),
    };
    Entry { local, holder: Rc::new(holder), owned: true, given: true }
}

/// What [`CompositionLocalProvider`] takes: a [`ProvidedValue`], a tuple of them, a `Vec` or an
/// array of them (Compose's varargs), or a [`CompositionLocalContext`].
pub trait ProvidedValues {
    #[doc(hidden)]
    fn give(self, update: &mut ProviderUpdate);
}

impl<T: Clone + 'static> ProvidedValues for ProvidedValue<T> {
    fn give(self, update: &mut ProviderUpdate) {
        update.give(self);
    }
}

impl<T: Clone + 'static> ProvidedValues for Vec<ProvidedValue<T>> {
    fn give(self, update: &mut ProviderUpdate) {
        for value in self {
            update.give(value);
        }
    }
}

impl<T: Clone + 'static, const N: usize> ProvidedValues for [ProvidedValue<T>; N] {
    fn give(self, update: &mut ProviderUpdate) {
        for value in self {
            update.give(value);
        }
    }
}

impl ProvidedValues for CompositionLocalContext {
    fn give(self, update: &mut ProviderUpdate) {
        update.give_context(self);
    }
}

macro_rules! tuple_values {
    ($($name:ident),+) => {
        impl<$($name: ProvidedValues),+> ProvidedValues for ($($name,)+) {
            #[allow(non_snake_case)]
            fn give(self, update: &mut ProviderUpdate) {
                let ($($name,)+) = self;
                $($name.give(update);)+
            }
        }
    };
}

tuple_values!(A);
tuple_values!(A, B);
tuple_values!(A, B, C);
tuple_values!(A, B, C, D);
tuple_values!(A, B, C, D, E);
tuple_values!(A, B, C, D, E, F);
tuple_values!(A, B, C, D, E, F, G);
tuple_values!(A, B, C, D, E, F, G, H);

/// Provides `values` to `content`, as Compose's `CompositionLocalProvider(values) { … }`:
/// one value, a tuple of them, a `Vec` or an array, or a [`CompositionLocalContext`]. A dynamic
/// local's new value recomposes the scopes that read it, and only those; a static local's new
/// value, or a change in what's provided, recomposes the whole content.
#[allow(non_snake_case)]
pub fn CompositionLocalProvider(values: impl ProvidedValues, content: impl FnOnce()) {
    with_composition_locals(values, content)
}

/// Provides `value` to `content` and returns what it returns, as `withCompositionLocal`.
pub fn with_composition_local<R>(value: impl ProvidedValues, content: impl FnOnce() -> R) -> R {
    with_composition_locals(value, content)
}

/// Provides `values` to `content` and returns what it returns, as `withCompositionLocals`.
pub fn with_composition_locals<R>(values: impl ProvidedValues, content: impl FnOnce() -> R) -> R {
    let group = start_provider();
    let entries: Rc<ProviderEntries> = remember(|| Rc::new(ProviderEntries::default()));
    let inserting = crate::composer::current().inserting();
    let mut update = ProviderUpdate {
        entries: std::mem::take(&mut *entries.entries.borrow_mut()),
        changed: false,
    };
    for entry in &mut update.entries {
        entry.given = false;
    }
    values.give(&mut update);
    let ProviderUpdate { entries: mut updated, mut changed } = update;
    // Locals no longer given.
    let before = updated.len();
    updated.retain(|entry| entry.given);
    changed |= updated.len() != before;
    let dropped = std::mem::replace(&mut *entries.entries.borrow_mut(), updated);
    drop(dropped);
    if changed && !inserting {
        with_runtime(|runtime| runtime.invalidate_providers());
    }
    let result = content();
    drop(group);
    result
}

/// Starts a provider's group.
fn start_provider() -> ReplaceGroup {
    let group = ReplaceGroup::start(PROVIDER_KEY, "compose_runtime::CompositionLocalProvider");
    with_runtime(|runtime| runtime.mark_provider());
    group
}
