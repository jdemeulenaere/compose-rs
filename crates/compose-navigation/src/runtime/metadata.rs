//! `NavMetadata.kt` and `NavMetadataKey.kt`: what an entry tells the display, Kotlin's
//! `Map<String, Any>`.

use std::any::{Any, type_name};
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Add;
use std::rc::Rc;

/// A key of a value of type `Value` in a [`NavMetadata`], as `NavMetadataKey<T>`. A key is told
/// apart by its [`name`](Self::name), Kotlin's `toString()`: its type's, so a key type with
/// instances of its own says which.
pub trait NavMetadataKey {
    type Value: 'static;

    fn name(&self) -> String {
        type_name::<Self>().to_string()
    }
}

/// An entry's metadata, as Kotlin's `Map<String, Any>`: values by name, read by a typed key or by
/// a name and a type. Cheap to clone. Two are equal when they hold the same values, by identity:
/// values have no `==` here, where a Kotlin map compares them, so what's equal is what was cloned
/// or merged from one place (an entry provider caches its keys' metadata for that).
#[derive(Clone, Default)]
pub struct NavMetadata(Rc<BTreeMap<String, Rc<dyn Any>>>);

impl NavMetadata {
    /// With nothing, as `emptyMap()`.
    pub fn new() -> NavMetadata {
        NavMetadata::default()
    }

    /// With `value` under `name` too, replacing one there, as `mapOf(name to value)`.
    pub fn with(self, name: impl Into<String>, value: impl Any) -> NavMetadata {
        let mut map = (*self.0).clone();
        map.insert(name.into(), Rc::new(value));
        NavMetadata(Rc::new(map))
    }

    /// The value of `key`, as `metadata[key]`: `None` if it holds none.
    pub fn get<K: NavMetadataKey>(&self, key: &K) -> Option<&K::Value> {
        self.get_named(&key.name())
    }

    /// Whether it holds a value of `key`, as `key in metadata`.
    pub fn contains<K: NavMetadataKey>(&self, key: &K) -> bool {
        self.contains_named(&key.name())
    }

    /// The value under `name`, if it's a `V`, as `metadata[name] as? V`.
    pub fn get_named<V: 'static>(&self, name: &str) -> Option<&V> {
        self.0.get(name)?.downcast_ref()
    }

    /// Whether it holds a value under `name`.
    pub fn contains_named(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Its names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
}

/// This and `other`'s, `other`'s going first where both have a name, as Kotlin's `map + map`.
impl Add for NavMetadata {
    type Output = NavMetadata;

    fn add(self, other: NavMetadata) -> NavMetadata {
        if other.is_empty() {
            return self;
        }
        let mut map = (*self.0).clone();
        map.extend(other.0.iter().map(|(name, value)| (name.clone(), value.clone())));
        NavMetadata(Rc::new(map))
    }
}

impl PartialEq for NavMetadata {
    fn eq(&self, other: &NavMetadata) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
            || (self.0.len() == other.0.len()
                && self.0.iter().zip(other.0.iter()).all(|((name, value), (other_name, other))| {
                    name == other_name && Rc::ptr_eq(value, other)
                }))
    }
}

impl fmt::Debug for NavMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_set().entries(self.0.keys()).finish()
    }
}

/// What [`metadata`]'s builder is given, as `MetadataScope`.
#[derive(Default)]
pub struct MetadataScope {
    map: BTreeMap<String, Rc<dyn Any>>,
}

impl MetadataScope {
    /// `value` under `key`, replacing one there.
    pub fn put<K: NavMetadataKey>(&mut self, key: &K, value: K::Value) {
        self.map.insert(key.name(), Rc::new(value));
    }
}

/// The metadata `builder` puts, as `metadata { put(key, value) }`.
pub fn metadata(builder: impl FnOnce(&mut MetadataScope)) -> NavMetadata {
    let mut scope = MetadataScope::default();
    builder(&mut scope);
    NavMetadata(Rc::new(scope.map))
}
