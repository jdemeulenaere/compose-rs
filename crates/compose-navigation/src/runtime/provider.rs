//! `EntryProvider.kt`.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::rc::Rc;

use super::content_key::{ContentKey, IntoContentKey};
use super::entry::{NavEntry, default_content_key, group_of};
use super::metadata::NavMetadata;

/// A class of keys of type `T`, what [`EntryProviderScope::entry_of`] takes, as Kotlin's `K::class`.
/// `T` itself is one, of every key not of another. With an enum of keys, a variant's payload can be
/// one:
///
/// ```ignore
/// impl KeyClass<Screen> for Details {
///     fn from_key(key: &Screen) -> Option<&Details> {
///         match key {
///             Screen::Details(details) => Some(details),
///             _ => None,
///         }
///     }
/// }
/// ```
pub trait KeyClass<T>: 'static {
    /// `key` as one of this class, if it's one.
    fn from_key(key: &T) -> Option<&Self>;
}

impl<T: 'static> KeyClass<T> for T {
    fn from_key(key: &T) -> Option<&T> {
        Some(key)
    }
}

type Metadata<T> = Rc<dyn Fn(&T) -> NavMetadata>;

struct KeyProvider<T> {
    key: T,
    /// Made of the key if it has none.
    content_key: Option<ContentKey>,
    metadata: Metadata<T>,
    content: Rc<dyn Fn(T)>,
    group: u64,
}

struct ClassProvider<T> {
    class: TypeId,
    matches: Rc<dyn Fn(&T) -> bool>,
    content_key: Rc<dyn Fn(&T) -> ContentKey>,
    metadata: Metadata<T>,
    content: Rc<dyn Fn(T)>,
    group: u64,
}

/// What [`entry_provider`]'s builder is given, as `EntryProviderScope`.
pub struct EntryProviderScope<T: 'static> {
    key_providers: Vec<KeyProvider<T>>,
    class_providers: Vec<ClassProvider<T>>,
}

/// An entry of a key being added: what's optional in Kotlin's `entry(key, contentKey, metadata)`.
pub struct EntryBuilder<'a, T> {
    provider: &'a mut KeyProvider<T>,
}

impl<T: 'static> EntryBuilder<'_, T> {
    /// Its content key, what the key prints and its type by default.
    pub fn with_content_key(self, content_key: impl IntoContentKey) -> Self {
        self.provider.content_key = Some(content_key.into_content_key());
        self
    }

    pub fn with_metadata(self, metadata: NavMetadata) -> Self {
        self.provider.metadata = Rc::new(move |_| metadata.clone());
        self
    }

    /// Its metadata made of its key, the first time it's asked for.
    pub fn with_metadata_of(self, metadata: impl Fn(&T) -> NavMetadata + 'static) -> Self {
        self.provider.metadata = Rc::new(metadata);
        self
    }
}

/// An entry of a class of keys being added: what's optional in Kotlin's `entry<K>(…)`.
pub struct ClassEntryBuilder<'a, T, K> {
    provider: &'a mut ClassProvider<T>,
    class: PhantomData<K>,
}

impl<T: 'static, K: KeyClass<T>> ClassEntryBuilder<'_, T, K> {
    /// Its content key made of its key, as `clazzContentKey`.
    pub fn with_content_key_of<C: IntoContentKey>(
        self,
        content_key: impl Fn(&K) -> C + 'static,
    ) -> Self {
        self.provider.content_key = Rc::new(move |key| {
            let key = K::from_key(key).expect("a key of its class");
            content_key(key).into_content_key()
        });
        self
    }

    pub fn with_metadata(self, metadata: NavMetadata) -> Self {
        self.provider.metadata = Rc::new(move |_| metadata.clone());
        self
    }

    /// Its metadata made of its key, the first time it's asked for.
    pub fn with_metadata_of(self, metadata: impl Fn(&K) -> NavMetadata + 'static) -> Self {
        self.provider.metadata =
            Rc::new(move |key| metadata(K::from_key(key).expect("a key of its class")));
        self
    }
}

impl<T: Clone + PartialEq + Hash + fmt::Debug + 'static> EntryProviderScope<T> {
    /// An entry showing `content` for `key`, as `entry(key) { … }`. Panics if `key` has one.
    pub fn entry<F: Fn(T) + 'static>(&mut self, key: T, content: F) -> EntryBuilder<'_, T> {
        assert!(
            !self.key_providers.iter().any(|provider| provider.key == key),
            "An `entry` with the key `key` has already been added: {key:?}."
        );
        self.key_providers.push(KeyProvider {
            content_key: None,
            key,
            metadata: Rc::new(|_| NavMetadata::new()),
            content: Rc::new(content),
            group: group_of::<F, T>(),
        });
        let provider = self.key_providers.last_mut().expect("just added");
        EntryBuilder { provider }
    }

    /// [`entry`](Self::entry), as `addEntryProvider`.
    pub fn add_entry_provider<F: Fn(T) + 'static>(
        &mut self,
        key: T,
        content: F,
    ) -> EntryBuilder<'_, T> {
        self.entry(key, content)
    }

    /// An entry showing `content` for each key of the class `K`, as `entry<K> { … }`. A key's own
    /// entry goes before it, and `T`'s, of every key, after every other class's, whatever the order
    /// they're added in. Panics if `K` has one.
    pub fn entry_of<K: KeyClass<T> + Clone, F: Fn(K) + 'static>(
        &mut self,
        content: F,
    ) -> ClassEntryBuilder<'_, T, K> {
        let class = TypeId::of::<K>();
        let class_name = std::any::type_name::<K>();
        assert!(
            !self.class_providers.iter().any(|provider| provider.class == class),
            "An `entry` with the same `clazz` has already been added: {class_name}."
        );
        self.class_providers.push(ClassProvider {
            class,
            matches: Rc::new(|key| K::from_key(key).is_some()),
            content_key: Rc::new(default_content_key),
            metadata: Rc::new(|_| NavMetadata::new()),
            content: Rc::new(move |key| {
                content(K::from_key(&key).expect("a key of its class").clone());
            }),
            group: group_of::<F, K>(),
        });
        let provider = self.class_providers.last_mut().expect("just added");
        ClassEntryBuilder { provider, class: PhantomData }
    }

    /// [`entry_of`](Self::entry_of), as `addEntryProvider(clazz)`.
    pub fn add_entry_provider_of<K: KeyClass<T> + Clone, F: Fn(K) + 'static>(
        &mut self,
        content: F,
    ) -> ClassEntryBuilder<'_, T, K> {
        self.entry_of(content)
    }

    fn build(
        self,
        fallback: impl Fn(T) -> NavEntry<T> + 'static,
    ) -> impl Fn(T) -> NavEntry<T> + 'static {
        let EntryProviderScope { key_providers, mut class_providers } = self;
        // Kotlin's classes are exact: no `entry<K>` takes another's keys, so `T`'s comes last.
        class_providers.sort_by_key(|provider| provider.class == TypeId::of::<T>());
        let metadata_cache: RefCell<HashMap<u64, Vec<(T, NavMetadata)>>> = RefCell::default();
        move |key: T| {
            let class_provider = class_providers.iter().find(|provider| (provider.matches)(&key));
            let key_provider = key_providers.iter().find(|provider| provider.key == key);
            let hash = {
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                hasher.finish()
            };
            let cached = metadata_cache.borrow().get(&hash).and_then(|bucket| {
                bucket
                    .iter()
                    .find(|(cached, _)| *cached == key)
                    .map(|(_, metadata)| metadata.clone())
            });
            let metadata = cached.unwrap_or_else(|| {
                // A class's goes first, as Kotlin's.
                let metadata = match (class_provider, key_provider) {
                    (Some(provider), _) => (provider.metadata)(&key),
                    (None, Some(provider)) => (provider.metadata)(&key),
                    (None, None) => NavMetadata::new(),
                };
                metadata_cache
                    .borrow_mut()
                    .entry(hash)
                    .or_default()
                    .push((key.clone(), metadata.clone()));
                metadata
            });
            if let Some(provider) = key_provider {
                let content_key =
                    provider.content_key.clone().unwrap_or_else(|| default_content_key(&key));
                return NavEntry::make(
                    key,
                    content_key,
                    metadata,
                    provider.content.clone(),
                    provider.group,
                );
            }
            if let Some(provider) = class_provider {
                let content_key = (provider.content_key)(&key);
                return NavEntry::make(
                    key,
                    content_key,
                    metadata,
                    provider.content.clone(),
                    provider.group,
                );
            }
            fallback(key)
        }
    }
}

/// What makes the entries of a back stack's keys, of the entries `builder` adds, as
/// `entryProvider { … }`. A key with none panics. The DSL, for what's shared with Kotlin's shape; a
/// plain `match` is the usual way, as an enum of keys tells a screen by itself, and is any
/// `Fn(T) -> NavEntry<T>`:
///
/// ```ignore
/// let provider = |key: Screen| match key {
///     Screen::Home => NavEntry::new(key, |_| Home()),
///     Screen::Details(id) => NavEntry::new(key, move |_| Details(id)),
/// };
/// ```
pub fn entry_provider<T: Clone + PartialEq + Hash + fmt::Debug + 'static>(
    builder: impl FnOnce(&mut EntryProviderScope<T>),
) -> impl Fn(T) -> NavEntry<T> + Clone + 'static {
    entry_provider_with_fallback(|key: T| panic!("Unknown screen {key:?}"), builder)
}

/// [`entry_provider`], with what makes the entry of a key with none, as its `fallback`.
pub fn entry_provider_with_fallback<T: Clone + PartialEq + Hash + fmt::Debug + 'static>(
    fallback: impl Fn(T) -> NavEntry<T> + 'static,
    builder: impl FnOnce(&mut EntryProviderScope<T>),
) -> impl Fn(T) -> NavEntry<T> + Clone + 'static {
    let mut scope = EntryProviderScope { key_providers: Vec::new(), class_providers: Vec::new() };
    builder(&mut scope);
    let provider = Rc::new(scope.build(fallback));
    move |key| provider(key)
}
