//! `ImmutableList`: a list for data models and composable parameters, shared by pointer, as
//! kotlinx's `ImmutableList` is for Compose.

use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use crate::compose_arg::ComposeArg;

/// An immutable list shared by pointer (an `Rc<[T]>`): cheap to clone, and a [`ComposeArg`]
/// compared by identity, so a data model holding a list (`tags: ImmutableList<String>`) derives
/// `ComposeArg` and skips while it holds the same list. Two lists with equal contents built
/// apart are different for skipping, as `Rc`s are (two empty lists are the same): collections
/// are never compared element by element (docs/03). Keep a list in state or `remember` it; a
/// field compared by value is marked `#[compose_arg(eq)]` instead.
///
/// It derefs to a slice: `list[0]`, `list.len()`, `list.iter()`, `for item in &list`.
/// `PartialEq` compares contents.
pub struct ImmutableList<T>(Rc<[T]>);

impl<T> ImmutableList<T> {
    /// An empty list.
    pub fn new() -> ImmutableList<T> {
        ImmutableList(Rc::from(Vec::new()))
    }

    /// Whether two lists are the same list, not only equal.
    pub fn ptr_eq(&self, other: &ImmutableList<T>) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// The items, as a slice.
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}

impl<T> Clone for ImmutableList<T> {
    fn clone(&self) -> Self {
        ImmutableList(self.0.clone())
    }
}

impl<T> Default for ImmutableList<T> {
    fn default() -> Self {
        ImmutableList::new()
    }
}

impl<T> Deref for ImmutableList<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}

impl<T> AsRef<[T]> for ImmutableList<T> {
    fn as_ref(&self) -> &[T] {
        &self.0
    }
}

impl<T> From<Vec<T>> for ImmutableList<T> {
    fn from(items: Vec<T>) -> ImmutableList<T> {
        ImmutableList(Rc::from(items))
    }
}

impl<T, const N: usize> From<[T; N]> for ImmutableList<T> {
    fn from(items: [T; N]) -> ImmutableList<T> {
        ImmutableList(Rc::from(Vec::from(items)))
    }
}

impl<T: Clone> From<&[T]> for ImmutableList<T> {
    fn from(items: &[T]) -> ImmutableList<T> {
        ImmutableList(Rc::from(items))
    }
}

impl<T> FromIterator<T> for ImmutableList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> ImmutableList<T> {
        ImmutableList(items.into_iter().collect())
    }
}

impl<'a, T> IntoIterator for &'a ImmutableList<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T: PartialEq> PartialEq for ImmutableList<T> {
    /// The contents, as the slices compare (a list holding NaN isn't equal to itself).
    fn eq(&self, other: &Self) -> bool {
        *self.0 == *other.0
    }
}

impl<T: Eq> Eq for ImmutableList<T> {}

impl<T: std::hash::Hash> std::hash::Hash for ImmutableList<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<T: fmt::Debug> fmt::Debug for ImmutableList<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, formatter)
    }
}

impl<T: 'static> ComposeArg for ImmutableList<T> {
    /// The same list, or two empty ones: identity, never a traversal.
    #[inline]
    fn same(&self, other: &Self) -> bool {
        self.ptr_eq(other) || (self.0.is_empty() && other.0.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_by_identity_for_skipping_and_by_contents_for_equality() {
        let list = ImmutableList::from(vec![1, 2, 3]);
        let rebuilt: ImmutableList<i32> = (1..=3).collect();
        assert!(list.same(&list.clone()));
        assert!(!list.same(&rebuilt));
        assert_eq!(list, rebuilt);
        assert!(ImmutableList::<i32>::new().same(&ImmutableList::from([])));
    }

    #[test]
    fn reads_as_a_slice() {
        let list = ImmutableList::from(["a", "b"]);
        assert_eq!(list[1], "b");
        assert_eq!(list.len(), 2);
        assert_eq!((&list).into_iter().copied().collect::<Vec<_>>(), ["a", "b"]);
        let mut seen = Vec::new();
        for item in &list {
            seen.push(*item);
        }
        assert_eq!(seen, ["a", "b"]);
        assert_eq!(format!("{list:?}"), r#"["a", "b"]"#);
    }
}
