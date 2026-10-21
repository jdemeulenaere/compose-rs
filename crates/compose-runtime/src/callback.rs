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

// ---- Memoization -----------------------------------------------------------------------------

/// A capture, probed for its key: `(&&ArgProbe(&capture)).arg_key()` gives a [`Keyed`] key if
/// the capture's type is `ComposeArg + Clone` (autoref specialisation), and [`Unkeyed`] otherwise.
pub struct ArgProbe<'a, T>(pub &'a T);

/// A capture compared with `ComposeArg::same`.
pub struct Keyed<'a, T>(&'a T);

/// A capture that can't be compared: its lambda counts as changed on every pass.
pub struct Unkeyed;

pub trait KeyedProbe<'a, T> {
    fn arg_key(&self) -> Keyed<'a, T>;
}

impl<'a, T: ComposeArg + Clone> KeyedProbe<'a, T> for &ArgProbe<'a, T> {
    #[inline]
    fn arg_key(&self) -> Keyed<'a, T> {
        Keyed(self.0)
    }
}

pub trait UnkeyedProbe {
    fn arg_key(&self) -> Unkeyed;
}

impl<T> UnkeyedProbe for ArgProbe<'_, T> {
    #[inline]
    fn arg_key(&self) -> Unkeyed {
        Unkeyed
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
    type Owned = ();

    #[inline]
    fn same(&self, _: &()) -> bool {
        false
    }

    #[inline]
    fn to_owned(&self) {}
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
    keys: K,
    position: Option<SlotPosition>,
    _function: std::marker::PhantomData<fn() -> Box<F>>,
}

impl<K: 'static, F: ?Sized + 'static> PendingLambda<K, F> {
    /// Wraps the lambda, remembers it with its keys, and returns it.
    pub fn store<C: IntoCallback<F>>(self, lambda: C) -> Callback<F> {
        let callback = lambda.into_callback();
        if let Some(position) = self.position {
            let stored: Rc<dyn Any> = Rc::new((self.keys, callback.clone()));
            set_lambda_slot(position, stored, type_name::<(K, Callback<F>)>());
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
pub fn lambda_probe<K: LambdaKeys, F: ?Sized + 'static>(keys: K) -> LambdaProbe<K::Owned, F> {
    let Some((position, existing)) = lambda_slot(TypeId::of::<(K::Owned, Callback<F>)>()) else {
        return LambdaProbe::Miss(PendingLambda {
            keys: keys.to_owned(),
            position: None,
            _function: std::marker::PhantomData,
        });
    };
    if let Some(existing) = existing
        && let Some((kept, callback)) = existing.downcast_ref::<(K::Owned, Callback<F>)>()
        && keys.same(kept)
    {
        return LambdaProbe::Hit(callback.clone());
    }
    LambdaProbe::Miss(PendingLambda {
        keys: keys.to_owned(),
        position: Some(position),
        _function: std::marker::PhantomData,
    })
}
