//! `Callback`: a shared function, compared by identity, and the memoization of lambdas on their
//! captures (docs/03-composable-macro.md, "Lambdas: memoized on their captures").

use std::any::{Any, TypeId, type_name};
use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use crate::compose_arg::ComposeArg;
use crate::composer::SlotPosition;
use crate::error::misuse;
use crate::runtime::with_runtime;
use crate::slot_table::SlotValue;

/// A shared function: what a composable's `impl Fn(…) + 'static` parameter becomes, so it can be
/// kept and compared by identity. It derefs to the function, so `content()` calls it.
pub struct Callback<F: ?Sized = dyn Fn()>(Rc<F>);

impl<F: ?Sized> Callback<F> {
    /// The callback wrapping `function`.
    pub fn new<C: IntoCallback<F>>(function: C) -> Callback<F> {
        function.into_callback()
    }

    /// A callback sharing an existing `Rc`.
    pub fn from_rc(function: Rc<F>) -> Callback<F> {
        Callback(function)
    }

    /// Whether two callbacks are the same function object.
    pub fn ptr_eq(&self, other: &Callback<F>) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<F: ?Sized> Clone for Callback<F> {
    fn clone(&self) -> Self {
        Callback(self.0.clone())
    }
}

impl<F: ?Sized> Deref for Callback<F> {
    type Target = F;
    fn deref(&self) -> &F {
        &self.0
    }
}

impl<F: ?Sized + 'static> ComposeArg for Callback<F> {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        self.ptr_eq(other)
    }
}

impl<F: ?Sized> fmt::Debug for Callback<F> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Callback({:p})", Rc::as_ptr(&self.0).cast::<()>())
    }
}

/// What converts into a `Callback<F>`: a closure of the right signature (up to four arguments,
/// passed by value), or a callback already.
pub trait IntoCallback<F: ?Sized> {
    fn into_callback(self) -> Callback<F>;
}

impl<F: ?Sized> IntoCallback<F> for Callback<F> {
    #[inline]
    fn into_callback(self) -> Callback<F> {
        self
    }
}

macro_rules! closures {
    ($($argument:ident),*) => {
        impl<Function, Output, $($argument),*> IntoCallback<dyn Fn($($argument),*) -> Output>
            for Function
        where
            Function: Fn($($argument),*) -> Output + 'static,
        {
            #[inline]
            fn into_callback(self) -> Callback<dyn Fn($($argument),*) -> Output> {
                Callback(Rc::new(self))
            }
        }
    };
}

closures!();
closures!(A);
closures!(A, B);
closures!(A, B, C);
closures!(A, B, C, D);
closures!(A, B, C, D, E);
closures!(A, B, C, D, E, G);
closures!(A, B, C, D, E, G, H);
closures!(A, B, C, D, E, G, H, I);

/// Converts a function argument into a callback: what call macros apply to closure arguments.
#[inline]
pub fn callback<F: ?Sized, C: IntoCallback<F>>(function: C) -> Callback<F> {
    function.into_callback()
}

/// A callback from a borrowed function (a `Callback` or a `Clone` closure): what call macros
/// apply to a callback argument given by name, so a content lambda passing it on stays `Fn`.
#[inline]
pub fn to_callback<F: ?Sized, C: IntoCallback<F> + Clone>(function: &C) -> Callback<F> {
    function.clone().into_callback()
}

/// Converts an optional function into an optional callback: what call macros apply to an
/// optional callback argument that isn't a lambda (`if editable { Some(move || …) } else { None }`).
#[inline]
pub fn optional_callback<F: ?Sized, C: IntoCallback<F>>(
    function: Option<C>,
) -> Option<Callback<F>> {
    function.map(IntoCallback::into_callback)
}

// ---- Memoization -----------------------------------------------------------------------------

/// A capture, probed for its key: `(&&&ArgProbe(&capture)).arg_key()` gives a [`Keyed`] key if
/// the capture's type is `ComposeArg + Clone` (autoref specialisation), and [`Unkeyed`]
/// otherwise, telling types apart by their `TypeId` when they are `'static`.
pub struct ArgProbe<'a, T>(pub &'a T);

/// A capture compared with `ComposeArg::same`.
pub struct Keyed<'a, T>(&'a T);

/// A capture that can't be compared: its lambda counts as changed on every pass, unless the
/// capture is zero-sized (a function item, a closure capturing nothing), so every value of its
/// type is the same.
#[derive(Clone, Copy)]
pub struct Unkeyed {
    zero_sized: bool,
    /// The capture's type: zero-sized values of the same type are the same, of different types
    /// (two function items, two closures) are not. Two closures of a function have the same
    /// name, so `'static` types are told apart by their `TypeId`.
    type_id: Option<std::any::TypeId>,
    type_name: &'static str,
}

impl Unkeyed {
    /// Always changed: what a lambda calling an unknown macro is keyed with.
    pub const CHANGED: Unkeyed = Unkeyed { zero_sized: false, type_id: None, type_name: "" };
}

pub trait KeyedProbe<'a, T> {
    fn arg_key(&self) -> Keyed<'a, T>;
}

impl<'a, T: ComposeArg + Clone> KeyedProbe<'a, T> for &&ArgProbe<'a, T> {
    #[inline]
    fn arg_key(&self) -> Keyed<'a, T> {
        Keyed(self.0)
    }
}

pub trait StaticProbe {
    fn arg_key(&self) -> Unkeyed;
}

impl<T: 'static> StaticProbe for &ArgProbe<'_, T> {
    #[inline]
    fn arg_key(&self) -> Unkeyed {
        Unkeyed {
            zero_sized: std::mem::size_of::<T>() == 0,
            type_id: Some(std::any::TypeId::of::<T>()),
            type_name: std::any::type_name::<T>(),
        }
    }
}

pub trait UnkeyedProbe {
    fn arg_key(&self) -> Unkeyed;
}

impl<T> UnkeyedProbe for ArgProbe<'_, T> {
    #[inline]
    fn arg_key(&self) -> Unkeyed {
        Unkeyed {
            zero_sized: std::mem::size_of::<T>() == 0,
            type_id: None,
            type_name: std::any::type_name::<T>(),
        }
    }
}

/// The captures of a lambda: compared with last pass's, and kept when they differ.
pub trait LambdaKeys {
    type Owned: 'static;
    fn same(&self, kept: &Self::Owned) -> bool;
    fn to_owned(&self) -> Self::Owned;
}

impl<T: ComposeArg + Clone> LambdaKeys for Keyed<'_, T> {
    type Owned = T;

    #[inline]
    fn same(&self, kept: &T) -> bool {
        kept.same(self.0)
    }

    #[inline]
    fn to_owned(&self) -> T {
        self.0.clone()
    }
}

impl LambdaKeys for Unkeyed {
    type Owned = Unkeyed;

    #[inline]
    fn same(&self, kept: &Unkeyed) -> bool {
        self.zero_sized
            && match (self.type_id, kept.type_id) {
                (Some(id), Some(kept)) => id == kept,
                _ => {
                    std::ptr::eq(kept.type_name, self.type_name) || kept.type_name == self.type_name
                }
            }
    }

    #[inline]
    fn to_owned(&self) -> Unkeyed {
        *self
    }
}

macro_rules! key_tuples {
    ($($name:ident $index:tt),*) => {
        impl<$($name: LambdaKeys),*> LambdaKeys for ($($name,)*) {
            type Owned = ($($name::Owned,)*);

            #[inline]
            #[allow(unused_variables)]
            fn same(&self, kept: &Self::Owned) -> bool {
                true $(&& self.$index.same(&kept.$index))*
            }

            #[inline]
            #[allow(clippy::unused_unit)]
            fn to_owned(&self) -> Self::Owned {
                ($(self.$index.to_owned(),)*)
            }
        }
    };
}

key_tuples!();
key_tuples!(A 0);
key_tuples!(A 0, B 1);
key_tuples!(A 0, B 1, C 2);
key_tuples!(A 0, B 1, C 2, D 3);
key_tuples!(A 0, B 1, C 2, D 3, E 4);
key_tuples!(A 0, B 1, C 2, D 3, E 4, F 5);
key_tuples!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
key_tuples!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
key_tuples!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8);
key_tuples!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9);
key_tuples!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10);
key_tuples!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11);

/// The outcome of [`lambda_probe`]: the callback kept from last pass, or a pending slot for a
/// new one.
pub enum LambdaProbe<K: 'static, F: ?Sized + 'static> {
    Hit(Callback<F>),
    Miss(PendingLambda<K, F>),
}

/// A lambda to store, with its captures' keys. Outside a composition pass there is no slot:
/// the lambda is wrapped and not remembered.
pub struct PendingLambda<K: 'static, F: ?Sized + 'static> {
    site: u64,
    keys: K,
    position: Option<SlotPosition>,
    _function: std::marker::PhantomData<fn() -> Box<F>>,
}

impl<K: 'static, F: ?Sized + 'static> LambdaProbe<K, F> {
    /// The kept callback, or `lambda` wrapped and remembered: what a memoized lambda expands to,
    /// `lambda_probe(keys).resolve(closure)`. On a hit the new closure is dropped unused.
    #[inline]
    pub fn resolve<C: IntoCallback<F>>(self, lambda: C) -> Callback<F> {
        match self {
            LambdaProbe::Hit(callback) => callback,
            LambdaProbe::Miss(pending) => pending.store(lambda),
        }
    }
}

impl<K: 'static, F: ?Sized + 'static> LambdaProbe<K, F> {
    /// The kept callback, or a clone of `value` (a callback, a function) wrapped and
    /// remembered: what a callback given by name expands to. A hit clones nothing.
    #[inline]
    pub fn resolve_cloned<C: IntoCallback<F> + Clone>(self, value: &C) -> Callback<F> {
        match self {
            LambdaProbe::Hit(callback) => callback,
            LambdaProbe::Miss(pending) => pending.store(value.clone()),
        }
    }
}

/// `probe.resolve(lambda)` for a lambda of `N` arguments, with the `Fn` bound spelled out so
/// a closure's untyped parameters (`|text| …`) take their types from the callback parameter.
macro_rules! resolvers {
    ($($name:ident ($($argument:ident),*)),*) => {
        $(
            #[doc(hidden)]
            #[inline]
            pub fn $name<K: 'static, $($argument: 'static,)* Output: 'static, C>(
                probe: LambdaProbe<K, dyn Fn($($argument),*) -> Output>,
                lambda: C,
            ) -> Callback<dyn Fn($($argument),*) -> Output>
            where
                C: Fn($($argument),*) -> Output + 'static,
            {
                probe.resolve(lambda)
            }
        )*
    };
}

resolvers!(
    resolve_0(),
    resolve_1(A),
    resolve_2(A, B),
    resolve_3(A, B, C1),
    resolve_4(A, B, C1, D),
    resolve_5(A, B, C1, D, E),
    resolve_6(A, B, C1, D, E, G),
    resolve_7(A, B, C1, D, E, G, H),
    resolve_8(A, B, C1, D, E, G, H, I)
);

/// `resolve_N` for a lambda nested in another closure: `make` builds it, from clones of its
/// captures, only on a miss, so the outer closure keeps its own (and stays `Fn`).
macro_rules! lazy_resolvers {
    ($($name:ident ($($argument:ident),*)),*) => {
        $(
            #[doc(hidden)]
            #[inline]
            pub fn $name<K: 'static, $($argument: 'static,)* Output: 'static, C>(
                probe: LambdaProbe<K, dyn Fn($($argument),*) -> Output>,
                make: impl FnOnce() -> C,
            ) -> Callback<dyn Fn($($argument),*) -> Output>
            where
                C: Fn($($argument),*) -> Output + 'static,
            {
                match probe {
                    LambdaProbe::Hit(callback) => callback,
                    LambdaProbe::Miss(pending) => pending.store(make()),
                }
            }
        )*
    };
}

lazy_resolvers!(
    resolve_lazy_0(),
    resolve_lazy_1(A),
    resolve_lazy_2(A, B),
    resolve_lazy_3(A, B, C1),
    resolve_lazy_4(A, B, C1, D),
    resolve_lazy_5(A, B, C1, D, E),
    resolve_lazy_6(A, B, C1, D, E, G),
    resolve_lazy_7(A, B, C1, D, E, G, H),
    resolve_lazy_8(A, B, C1, D, E, G, H, I)
);

/// `resolve_lazy_N` for lambdas of more arguments, through `IntoCallback`.
#[doc(hidden)]
#[inline]
pub fn resolve_lazy<K: 'static, F: ?Sized + 'static, C: IntoCallback<F>>(
    probe: LambdaProbe<K, F>,
    make: impl FnOnce() -> C,
) -> Callback<F> {
    match probe {
        LambdaProbe::Hit(callback) => callback,
        LambdaProbe::Miss(pending) => pending.store(make()),
    }
}

/// `callback(lambda)` for a lambda of `N` arguments, as `resolve_N`.
macro_rules! wrappers {
    ($($name:ident ($($argument:ident),*)),*) => {
        $(
            #[doc(hidden)]
            #[inline]
            pub fn $name<$($argument: 'static,)* Output: 'static, C>(lambda: C) -> Callback<dyn Fn($($argument),*) -> Output>
            where
                C: Fn($($argument),*) -> Output + 'static,
            {
                Callback(Rc::new(lambda))
            }
        )*
    };
}

wrappers!(
    callback_0(),
    callback_1(A),
    callback_2(A, B),
    callback_3(A, B, C1),
    callback_4(A, B, C1, D),
    callback_5(A, B, C1, D, E),
    callback_6(A, B, C1, D, E, G),
    callback_7(A, B, C1, D, E, G, H),
    callback_8(A, B, C1, D, E, G, H, I)
);

impl<K: 'static, F: ?Sized + 'static> PendingLambda<K, F> {
    /// Wraps the lambda, remembers it with its keys, and returns it.
    pub fn store<C: IntoCallback<F>>(self, lambda: C) -> Callback<F> {
        let callback = lambda.into_callback();
        if let Some(position) = self.position {
            let stored: Rc<dyn Any> = Rc::new((self.site, self.keys, callback.clone()));
            set_lambda_slot(position, stored, type_name::<(u64, K, Callback<F>)>());
        }
        callback
    }
}

fn set_lambda_slot(position: SlotPosition, stored: Rc<dyn Any>, type_name: &'static str) {
    if let Err((value, error)) =
        with_runtime(|runtime| runtime.slot_set(position, SlotValue::Plain(stored), type_name))
    {
        drop(value);
        misuse(error)
    }
}

/// The non-generic part of a probe: reserves the next slot when composing.
fn lambda_slot(type_id: TypeId) -> Option<(SlotPosition, Option<Rc<dyn Any>>)> {
    with_runtime(|runtime| runtime.is_composing().then(|| runtime.slot_next(type_id)))
}

/// Looks up the lambda remembered at this position: if its captures are all `same` as `keys`,
/// returns it; otherwise returns a [`PendingLambda`] to store the new one.
///
/// `site` identifies the lambda's code (a hash of its tokens): a slot kept by another lambda,
/// which a region group missed by the macro's guess could hand this one, is a miss, so a missed
/// group costs state, never a call of the wrong function.
pub fn lambda_probe<K: LambdaKeys, F: ?Sized + 'static>(
    site: u64,
    keys: K,
) -> LambdaProbe<K::Owned, F> {
    let Some((position, existing)) = lambda_slot(TypeId::of::<(u64, K::Owned, Callback<F>)>())
    else {
        return LambdaProbe::Miss(PendingLambda {
            site,
            keys: keys.to_owned(),
            position: None,
            _function: std::marker::PhantomData,
        });
    };
    if let Some(existing) = existing
        && let Some((kept_site, kept, callback)) =
            existing.downcast_ref::<(u64, K::Owned, Callback<F>)>()
        && *kept_site == site
        && keys.same(kept)
    {
        return LambdaProbe::Hit(callback.clone());
    }
    LambdaProbe::Miss(PendingLambda {
        site,
        keys: keys.to_owned(),
        position: Some(position),
        _function: std::marker::PhantomData,
    })
}
