//! `NavEntry.kt`.

use std::any::{TypeId, type_name};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use compose_runtime::{ComposeArg, composable};

use super::content_key::{ContentKey, IntoContentKey};
use super::metadata::NavMetadata;

/// The group a closure's content runs in, as Compose's composable lambda has its own by its place
/// in the source: a closure's type is that place. Another closure's content at the same position
/// replaces it; the same closure with another key keeps its state, as Kotlin's (a decorator or a
/// display tells them apart).
pub(crate) fn group_of<F: 'static>() -> u64 {
    let mut hasher = DefaultHasher::new();
    TypeId::of::<F>().hash(&mut hasher);
    hasher.finish()
}

/// `content` in a group of `group`.
pub(crate) fn in_group(group: u64, content: impl FnOnce()) {
    let composer = compose_runtime::__private::current();
    composer.start_replace_group(group);
    content();
    composer.end_group();
}

/// `defaultContentKey`: what the key prints, and its type.
pub(crate) fn default_content_key<T: fmt::Debug>(key: &T) -> ContentKey {
    format!("{key:?}:{}", type_name::<T>()).into_content_key()
}

#[derive(Clone)]
struct EntryInner<T> {
    key: T,
    content_key: ContentKey,
    metadata: NavMetadata,
    content: Rc<dyn Fn(T)>,
    content_group: u64,
}

/// A key and the content it shows, as `NavEntry`: a handle, what an entry provider makes of a key
/// of the back stack. Its [`content_key`](Self::content_key) identifies its content and its
/// decorators' state: entries sharing one share them.
pub struct NavEntry<T: 'static> {
    inner: Rc<EntryInner<T>>,
}

impl<T> Clone for NavEntry<T> {
    fn clone(&self) -> Self {
        NavEntry { inner: self.inner.clone() }
    }
}

impl<T> ComposeArg for NavEntry<T> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T: Clone + 'static> NavEntry<T> {
    /// `key` showing `content`, its content key made of what `key` prints and its type.
    pub fn new<F: Fn(T) + 'static>(key: T, content: F) -> NavEntry<T>
    where
        T: fmt::Debug,
    {
        let content_key = default_content_key(&key);
        NavEntry::make(key, content_key, NavMetadata::new(), Rc::new(content), group_of::<F>())
    }

    /// As `NavEntry(navEntry, content)`: `entry`'s key, content key and metadata, showing `content`.
    pub fn from_entry<F: Fn(T) + 'static>(entry: &NavEntry<T>, content: F) -> NavEntry<T> {
        let inner = &entry.inner;
        NavEntry::make(
            inner.key.clone(),
            inner.content_key.clone(),
            inner.metadata.clone(),
            Rc::new(content),
            group_of::<F>(),
        )
    }

    pub(crate) fn make(
        key: T,
        content_key: ContentKey,
        metadata: NavMetadata,
        content: Rc<dyn Fn(T)>,
        content_group: u64,
    ) -> NavEntry<T> {
        NavEntry {
            inner: Rc::new(EntryInner { key, content_key, metadata, content, content_group }),
        }
    }

    /// With `content_key`.
    pub fn with_content_key(mut self, content_key: impl IntoContentKey) -> NavEntry<T> {
        Rc::make_mut(&mut self.inner).content_key = content_key.into_content_key();
        self
    }

    /// With `metadata`.
    pub fn with_metadata(mut self, metadata: NavMetadata) -> NavEntry<T> {
        Rc::make_mut(&mut self.inner).metadata = metadata;
        self
    }

    /// Composes its content with its key, as `Content()`, in a scope of its own.
    #[allow(non_snake_case)]
    pub fn Content(&self) {
        NavEntryContent(self.clone());
    }
}

impl<T> NavEntry<T> {
    pub fn content_key(&self) -> &ContentKey {
        &self.inner.content_key
    }

    pub fn metadata(&self) -> &NavMetadata {
        &self.inner.metadata
    }
}

#[composable]
#[allow(non_snake_case)]
fn NavEntryContent<T: Clone + 'static>(entry: NavEntry<T>) {
    let inner = &entry.inner;
    in_group(inner.content_group, || (inner.content)(inner.key.clone()));
}

/// Equal as Kotlin's: their key, content key and metadata, and the same content.
impl<T: PartialEq> PartialEq for NavEntry<T> {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.inner, &other.inner);
        Rc::ptr_eq(a, b)
            || (a.key == b.key
                && a.content_key == b.content_key
                && a.metadata == b.metadata
                && Rc::ptr_eq(&a.content, &b.content))
    }
}

impl<T: fmt::Debug> fmt::Debug for NavEntry<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NavEntry")
            .field("key", &self.inner.key)
            .field("content_key", &self.inner.content_key)
            .field("metadata", &self.inner.metadata)
            .finish_non_exhaustive()
    }
}
