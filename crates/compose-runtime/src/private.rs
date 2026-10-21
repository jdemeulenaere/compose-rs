//! What `#[composable]` generated code calls. Not a public API.
//!
//! Groups are guards closed on drop, so `return`, `break`, `continue` and `?` in a composable's
//! body keep them balanced. While a panic unwinds through them they don't close: the pass is
//! poisoned anyway, and touching the runtime then could panic again.

use std::any::{Any, TypeId};

pub use crate::callback::{
    ArgProbe, Callback, KeyedProbe, LambdaProbe, PendingLambda, Unkeyed, UnkeyedProbe, callback,
    callback_0, callback_1, callback_2, callback_3, callback_4, callback_5, callback_6, callback_7,
    callback_8, lambda_probe, resolve_0, resolve_1, resolve_2, resolve_3, resolve_4, resolve_5,
    resolve_6, resolve_7, resolve_8, resolve_lazy, resolve_lazy_0, resolve_lazy_1, resolve_lazy_2,
    resolve_lazy_3, resolve_lazy_4, resolve_lazy_5, resolve_lazy_6, resolve_lazy_7, resolve_lazy_8,
    to_callback,
};
pub use crate::compose_arg::ComposeArg;
pub use crate::composer::{Composer, current};
pub use crate::hash::key_of;
pub use compose_macros::call;

/// Whether a panic that started after the guard was created is unwinding through it. A pass may
/// run while an unrelated panic unwinds (a `Drop` writing state, with an immediate frame
/// requester): its guards end their groups as usual.
#[inline]
fn unwound(unwinding_at_start: bool) -> bool {
    !unwinding_at_start && std::thread::panicking()
}

/// The restart group of a restartable composable.
pub struct RestartGroup {
    composer: Composer,
    restart: fn(&dyn Any),
    unwinding: bool,
}

impl RestartGroup {
    /// `identity` is [`identity_of`] the restart closure, `restart` the same closure as a
    /// function pointer.
    #[inline]
    pub fn start(
        composer: Composer,
        key: u64,
        restart: fn(&dyn Any),
        identity: TypeId,
    ) -> RestartGroup {
        crate::composer::start_restart_group_of(key, identity);
        RestartGroup { composer, restart, unwinding: std::thread::panicking() }
    }
}

impl Drop for RestartGroup {
    #[inline]
    fn drop(&mut self) {
        if unwound(self.unwinding) {
            crate::composer::abandon_group();
        } else {
            self.composer.end_restart_group(self.restart);
        }
    }
}

/// The group of a non-restartable composable.
pub struct ReplaceGroup {
    composer: Composer,
    unwinding: bool,
}

impl ReplaceGroup {
    /// `identity` is [`name_of`] a marker closure of the composable.
    #[inline]
    pub fn start(key: u64, identity: &'static str) -> ReplaceGroup {
        let composer = current();
        crate::composer::start_replace_group_of(key, identity);
        ReplaceGroup { composer, unwinding: std::thread::panicking() }
    }
}

impl Drop for ReplaceGroup {
    #[inline]
    fn drop(&mut self) {
        if unwound(self.unwinding) {
            crate::composer::abandon_group();
        } else {
            self.composer.end_group();
        }
    }
}

/// A region group around a branch or a loop. Outside a composition pass it does nothing: the
/// macro's guess may put one in a handler closure that runs later, outside any pass.
pub struct Region {
    composer: Option<Composer>,
    unwinding: bool,
}

#[inline]
pub fn region(key: u64) -> Region {
    let composer = crate::composer::try_current();
    if let Some(composer) = composer {
        composer.start_replace_group(key);
    }
    Region { composer, unwinding: std::thread::panicking() }
}

impl Drop for Region {
    #[inline]
    fn drop(&mut self) {
        if let Some(composer) = self.composer {
            if unwound(self.unwinding) {
                crate::composer::abandon_group();
            } else {
                composer.end_group();
            }
        }
    }
}

/// A composable's identity: the `TypeId` of its restart closure, distinct for each composable and
/// each instantiation of a generic one, and the same in every copy of an `#[inline]` one.
#[inline]
pub fn identity_of<F: 'static>(_restart: &F) -> TypeId {
    TypeId::of::<F>()
}

/// A non-restartable composable's identity: the type name of a closure it declares, which names
/// the composable and its type arguments.
#[inline]
pub fn name_of<F>(_marker: &F) -> &'static str {
    std::any::type_name::<F>()
}

/// A clone of an argument, to keep. A function rather than `Clone::clone`, so generated code
/// doesn't trip `clippy::clone_on_copy` in the user's crate.
#[inline]
pub fn keep<T: Clone>(value: &T) -> T {
    value.clone()
}

/// A restart group's kept arguments, for its restart function to clone.
#[inline]
pub fn kept<K: 'static>(kept: &dyn Any) -> &K {
    kept.downcast_ref::<K>().expect("compose-runtime: kept arguments of another type")
}

/// The first statement of a closure body or block the caller's `#[composable]` marked with its
/// captures, for `call!` to memoize. Anywhere else (an argument a user's own macro passes on),
/// it expands to nothing.
#[doc(hidden)]
#[macro_export]
macro_rules! __compose_marked {
    ($($captures:tt)*) => {};
}

pub use crate::__compose_marked as marked;
