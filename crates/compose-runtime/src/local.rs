//! Composition locals, as Compose's `CompositionLocal`: values provided to a subtree and read
//! anywhere inside it without being passed down.
//!
//! A provider is a replace group whose first slot holds the value in a state cell. `current()`
//! walks up the slot table from the group composing now to the nearest provider of the local,
//! so a scope restarting on its own finds the same provider as when its parent composed it, and
//! reads the cell, so a new value recomposes exactly the readers (Compose's
//! `compositionLocalOf`).

use std::any::TypeId;
use std::rc::Rc;

use crate::__private::ReplaceGroup;
use crate::hash::key_of;
use crate::remember::remember;
use crate::runtime::with_runtime;
use crate::slot_table::{GroupKind, NONE};
use crate::state::{OwnedState, mutable_state_of_with, structural_equality_policy};

/// A composition local: declare it as a `static`, whose address is its identity.
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
    local: &'static CompositionLocal<T>,
    value: T,
}

/// What a provider group keeps.
struct ProviderSlot<T: 'static> {
    local: usize,
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

    fn id(&'static self) -> usize {
        self as *const CompositionLocal<T> as usize
    }

    fn key(&'static self) -> u64 {
        key_of("compose_runtime::CompositionLocalProvider") ^ (self.id() as u64).rotate_left(17)
    }

    /// `value` for this local, as `local provides value`.
    pub fn provides(&'static self, value: T) -> ProvidedValue<T> {
        ProvidedValue { local: self, value }
    }

    /// The value of the nearest provider above the composable composing now, or the default.
    /// Reading it subscribes the composing scope: a new value recomposes it.
    ///
    /// # Panics
    ///
    /// Outside composition.
    pub fn current(&'static self) -> T
    where
        T: Clone,
    {
        let key = self.key();
        let found = with_runtime(|runtime| {
            let composition = runtime.check_composing();
            let table = &runtime.compositions.data(composition).table;
            let mut group = runtime.frames.last().map_or(NONE, |frame| frame.group);
            while group != NONE {
                let entry = &table[group];
                if entry.kind == GroupKind::Replace
                    && entry.key == key
                    && let Some(slot) = entry.slots.first()
                    && slot.value.value_type_id() == TypeId::of::<Rc<ProviderSlot<T>>>()
                {
                    return Some(slot.value.to_any());
                }
                group = entry.parent;
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
pub fn CompositionLocalProvider<T: Clone + PartialEq + 'static>(
    value: ProvidedValue<T>,
    content: impl FnOnce(),
) {
    let ProvidedValue { local, value } = value;
    let group = ReplaceGroup::start(local.key(), "compose_runtime::CompositionLocalProvider");
    let slot: Rc<ProviderSlot<T>> = remember(|| {
        Rc::new(ProviderSlot {
            local: local.id(),
            value: mutable_state_of_with(value.clone(), structural_equality_policy()),
        })
    });
    slot.value.handle().set_while_composing(value);
    drop(slot);
    content();
    drop(group);
}
