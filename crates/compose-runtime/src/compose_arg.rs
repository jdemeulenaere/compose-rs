//! `ComposeArg`: how kept arguments and keys compare. Identity first, contents only where cheap.

use std::rc::Rc;
use std::sync::Arc;

use crate::state::{MutableState, OwnedState, State};

/// Compares a composable's argument with the one kept from the last pass, for skipping.
///
/// **The rule: identity first, then contents only where that's cheap.** Shared data (`Rc`,
/// `Arc`, state handles, callbacks) compares by identity; small values by contents. Collections
/// are never compared element by element, so `Vec` and `HashMap` don't implement it: wrap them
/// in `Rc`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` can't be compared for skipping: it doesn't implement `ComposeArg`",
    label = "this argument's type doesn't implement `ComposeArg`",
    note = "large or deep data is compared by identity: wrap it in `Rc<…>`",
    note = "or derive `ComposeArg` for your own types, or mark the composable \
            `#[composable(non_restartable)]`"
)]
pub trait ComposeArg: 'static {
    /// Whether `other` is the same as `self` for skipping. Cheap: never a deep traversal.
    fn same(&self, other: &Self) -> bool;
}

macro_rules! by_value {
    ($($type:ty),* $(,)?) => {
        $(impl ComposeArg for $type {
            #[inline]
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

by_value!(
    bool,
    char,
    (),
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    String,
    &'static str
);

macro_rules! float {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            /// As Kotlin's boxed `equals`: bitwise, with every NaN the same.
            #[inline]
            fn same(&self, other: &Self) -> bool {
                self.to_bits() == other.to_bits() || (self.is_nan() && other.is_nan())
            }
        })*
    };
}

float!(f32, f64);

impl<T: ?Sized + 'static> ComposeArg for Rc<T> {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(self, other)
    }
}

impl<T: ?Sized + 'static> ComposeArg for Arc<T> {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(self, other)
    }
}

impl<T: 'static> ComposeArg for MutableState<T> {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<T: 'static> ComposeArg for State<T> {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<T: 'static> ComposeArg for OwnedState<T> {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        self.handle() == other.handle()
    }
}

impl<T: ComposeArg> ComposeArg for Option<T> {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Some(a), Some(b)) => a.same(b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl<T: ComposeArg, const N: usize> ComposeArg for [T; N] {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| a.same(b))
    }
}

macro_rules! tuple {
    ($($name:ident $index:tt),+) => {
        impl<$($name: ComposeArg),+> ComposeArg for ($($name,)+) {
            #[inline]
            fn same(&self, other: &Self) -> bool {
                $(self.$index.same(&other.$index))&&+
            }
        }
    };
}

tuple!(A 0);
tuple!(A 0, B 1);
tuple!(A 0, B 1, C 2);
tuple!(A 0, B 1, C 2, D 3);
tuple!(A 0, B 1, C 2, D 3, E 4);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_for_shared_data() {
        let a = Rc::new(vec![1, 2, 3]);
        let b = Rc::new(vec![1, 2, 3]);
        assert!(a.same(&a.clone()));
        assert!(!a.same(&b));
        let c: Rc<str> = Rc::from("x");
        assert!(c.same(&c.clone()));
    }

    #[test]
    fn values_for_small_data() {
        assert!(3.same(&3));
        assert!(!3.same(&4));
        assert!(String::from("a").same(&String::from("a")));
        assert!("a".same(&"a"));
        assert!(f32::NAN.same(&f32::NAN));
        assert!(!0.0f32.same(&-0.0));
        assert!((1, "a", Some(2.5f64)).same(&(1, "a", Some(2.5))));
        assert!(![1, 2].same(&[1, 3]));
    }
}
