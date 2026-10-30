//! `NavKey.kt`, `NavBackStack.kt` and `RememberNavBackStack.kt`.

use std::fmt;
use std::ops::Deref;

use compose_runtime::snapshots::{OwnedStateList, SnapshotStateList, mutable_state_list_from};
use compose_runtime::{ComposeArg, ImmutableList, Remember, Saveable, remember_saveable};

/// A list of keys or decorators that a function reads, as Kotlin's `List<T>`: a state list is
/// observed where it's read, a plain one is what it is.
pub trait NavList<T> {
    fn to_nav_list(&self) -> Vec<T>;
}

impl<T: Clone> NavList<T> for Vec<T> {
    fn to_nav_list(&self) -> Vec<T> {
        self.clone()
    }
}

impl<T: Clone, const N: usize> NavList<T> for [T; N] {
    fn to_nav_list(&self) -> Vec<T> {
        self.to_vec()
    }
}

impl<T: Clone> NavList<T> for &[T] {
    fn to_nav_list(&self) -> Vec<T> {
        self.to_vec()
    }
}

impl<T: Clone> NavList<T> for ImmutableList<T> {
    fn to_nav_list(&self) -> Vec<T> {
        self.iter().cloned().collect()
    }
}

impl<T: Clone + 'static> NavList<T> for SnapshotStateList<T> {
    fn to_nav_list(&self) -> Vec<T> {
        self.with(Vec::clone)
    }
}

impl<T: Clone + 'static> NavList<T> for NavBackStack<T> {
    fn to_nav_list(&self) -> Vec<T> {
        self.base.to_nav_list()
    }
}

/// What can be a key of a saved back stack, as `NavKey`: `Saveable` for Kotlin's `@Serializable`,
/// with what entries read of a key. An enum of the app's screens is the usual one: Kotlin's sealed
/// classes and open polymorphism need no registering of subclasses.
pub trait NavKey: Saveable + Clone + PartialEq + fmt::Debug + 'static {}

impl<T: Saveable + Clone + PartialEq + fmt::Debug + 'static> NavKey for T {}

/// A back stack the app changes, showing as state, as `NavBackStack`: a `Copy` handle to a
/// [`SnapshotStateList`], which it derefs to (`push`, `remove`, `pop`-like edits).
/// [`remember_nav_back_stack`] saves it; [`new`](Self::new) returns its owner.
pub struct NavBackStack<T: 'static> {
    base: SnapshotStateList<T>,
}

impl<T> Clone for NavBackStack<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for NavBackStack<T> {}

impl<T> PartialEq for NavBackStack<T> {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
    }
}

impl<T> Eq for NavBackStack<T> {}

impl<T> ComposeArg for NavBackStack<T> {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<T> Deref for NavBackStack<T> {
    type Target = SnapshotStateList<T>;
    fn deref(&self) -> &SnapshotStateList<T> {
        &self.base
    }
}

impl<T> fmt::Debug for NavBackStack<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("NavBackStack").field(&self.base).finish()
    }
}

impl<T: 'static> NavBackStack<T> {
    /// The handle of `base`, as `NavBackStack(base)`.
    pub fn of(base: SnapshotStateList<T>) -> NavBackStack<T> {
        NavBackStack { base }
    }

    /// A stack of `elements`, as `NavBackStack(vararg elements)`, and its owner.
    #[track_caller]
    #[allow(clippy::new_ret_no_self)]
    pub fn new(elements: impl IntoIterator<Item = T>) -> OwnedNavBackStack<T> {
        let owner = mutable_state_list_from(elements);
        OwnedNavBackStack { handle: NavBackStack { base: owner.handle() }, owner }
    }

    /// Its last key off it, as `removeLastOrNull()`.
    pub fn remove_last_or_none(&self) -> Option<T>
    where
        T: Clone + PartialEq,
    {
        let last = self.base.len().checked_sub(1)?;
        Some(self.base.remove(last))
    }
}

/// What owns a [`NavBackStack`]: it derefs to the handle, and `remember` keeps it.
pub struct OwnedNavBackStack<T: 'static> {
    handle: NavBackStack<T>,
    owner: OwnedStateList<T>,
}

impl<T> Deref for OwnedNavBackStack<T> {
    type Target = NavBackStack<T>;
    fn deref(&self) -> &NavBackStack<T> {
        &self.handle
    }
}

impl<T> OwnedNavBackStack<T> {
    pub fn handle(&self) -> NavBackStack<T> {
        self.handle
    }

    pub fn is_alive(&self) -> bool {
        self.owner.is_alive()
    }
}

impl<T: 'static> Remember for OwnedNavBackStack<T> {
    type Stored = OwnedNavBackStack<T>;
    type Output = NavBackStack<T>;

    fn into_stored(self) -> OwnedNavBackStack<T> {
        self
    }

    fn output(stored: &OwnedNavBackStack<T>) -> NavBackStack<T> {
        stored.handle
    }
}

/// A stack starting with `elements`, kept in the composition and saved across the host's recreation
/// and the process's death, as `rememberNavBackStack`. What can't be restored (a key of another
/// version of the app) starts again.
#[track_caller]
pub fn remember_nav_back_stack<T: NavKey>(
    elements: impl IntoIterator<Item = T>,
) -> NavBackStack<T> {
    let base = remember_saveable(|| mutable_state_list_from(elements));
    NavBackStack { base }
}
