//! Composition locals, as Compose's `CompositionLocal`: values provided to a subtree and read
//! anywhere inside it without being passed down.
//!
//! A provider is a replace group whose first slot holds the value in a state cell. `current()`
//! walks up the slot table from the group composing now to the nearest provider of the local,
//! so a scope restarting on its own finds the same provider as when its parent composed it, and
//! reads the cell, so a new value recomposes exactly the readers (Compose's
//! `compositionLocalOf`). The walk visits provider groups only: each group records, when it is
//! allocated, the nearest provider's group above it (a group's ancestors never change), so a
//! read costs the providers above it, not the groups (often dozens) between them.

use std::any::TypeId;
use std::rc::Rc;

use crate::__private::ReplaceGroup;
use crate::compose_arg::ComposeArg;
use crate::hash::key_of;
use crate::remember::remember;
use crate::runtime::with_runtime;
use crate::slot_table::{GroupKind, NONE};
// A provider's policy: writes of a `same` value (`ComposeArg`) invalidate nothing, so locals may
// hold `Rc`s of trait objects (compared by identity) as well as plain values.
use crate::state::{OwnedState, ReferentialEqualityPolicy, mutable_state_of_with};

/// A composition local: declared as a `static` (or a `const`), identified by its name and
/// value type, which must be unique together.
///
/// ```ignore
/// pub static LOCAL_CONTENT_COLOR: CompositionLocal<Color> =
///     CompositionLocal::new("LocalContentColor", || Color::BLACK);
/// ```
pub struct CompositionLocal<T: 'static> {
    name: &'static str,
    default: fn() -> T,
}

/// A value for a local, for [`CompositionLocalProvider`], as Compose's `provides`.
pub struct ProvidedValue<T: 'static> {
    /// The local's identity and its provider groups' key.
    id: u64,
    key: u64,
    value: T,
}

/// What a provider group keeps.
struct ProviderSlot<T: 'static> {
    local: u64,
    value: OwnedState<T>,
}

impl<T: 'static> CompositionLocal<T> {
    /// A local whose value outside any provider is `default()`, as `compositionLocalOf`.
    pub const fn new(name: &'static str, default: fn() -> T) -> CompositionLocal<T> {
        CompositionLocal { name, default }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Its identity: its name and its type (an address isn't one: a `const` has a new one at
    /// each use).
    fn id(&self) -> u64 {
        key_of(self.name) ^ key_of(std::any::type_name::<T>()).rotate_left(29)
    }

    fn key(&self) -> u64 {
        key_of("compose_runtime::CompositionLocalProvider") ^ self.id().rotate_left(17)
    }

    /// `value` for this local, as `local provides value`.
    pub fn provides(&self, value: T) -> ProvidedValue<T> {
        ProvidedValue { id: self.id(), key: self.key(), value }
    }

    /// The value of the nearest provider above the composable composing now, or the default.
    /// Reading it subscribes the composing scope: a new value recomposes it.
    ///
    /// # Panics
    ///
    /// Outside composition.
    pub fn current(&self) -> T
    where
        T: Clone,
    {
        let key = self.key();
        let found = with_runtime(|runtime| {
            let composition = runtime.check_composing();
            let table = &runtime.compositions.data(composition).table;
            // The providers around the group composing now, nearest first.
            let composing = runtime.frames.last().map_or(NONE, |frame| frame.group);
            let mut group = table.provider_at_or_above(composing);
            while group != NONE {
                let entry = &table[group];
                if entry.kind == GroupKind::Replace
                    && entry.key == key
                    && let Some(slot) = entry.slots.first()
                    && slot.value.value_type_id() == TypeId::of::<Rc<ProviderSlot<T>>>()
                {
                    return slot.value.to_any();
                }
                group = entry.enclosing_provider;
            }
            None
        });
        let Some(found) = found else { return (self.default)() };
        let slot = found.downcast_ref::<Rc<ProviderSlot<T>>>().expect("type checked");
        if slot.local != self.id() {
            return (self.default)();
        }
        slot.value.handle().get()
    }
}

/// Provides `value` to `content`, as Compose's `CompositionLocalProvider(local provides value)`.
/// A changed value recomposes the scopes that read it, and only those. Nest providers for
/// several locals.
#[allow(non_snake_case)]
pub fn CompositionLocalProvider<T: Clone + ComposeArg + 'static>(
    value: ProvidedValue<T>,
    content: impl FnOnce(),
) {
    // A shim, instantiated per call site: the provider's work is instantiated per value type.
    let group = start_provider(value);
    content();
    drop(group);
}

/// Starts a provider's group and provides `value` in it.
fn start_provider<T: Clone + ComposeArg + 'static>(value: ProvidedValue<T>) -> ReplaceGroup {
    let ProvidedValue { id, key, value } = value;
    let group = ReplaceGroup::start(key, "compose_runtime::CompositionLocalProvider");
    with_runtime(|runtime| runtime.mark_provider());
    let slot: Rc<ProviderSlot<T>> = remember(|| {
        Rc::new(ProviderSlot {
            local: id,
            value: mutable_state_of_with(value.clone(), ReferentialEqualityPolicy),
        })
    });
    slot.value.handle().set(value);
    group
}
