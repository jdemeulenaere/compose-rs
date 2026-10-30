//! `NavEntry.contentKey`, Kotlin's `Any`.

use std::any::{Any, type_name};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use compose_runtime::ComposeArg;

trait ErasedKey {
    fn as_any(&self) -> &dyn Any;
    fn equals(&self, other: &dyn ErasedKey) -> bool;
    fn hash_with(&self, hasher: &mut dyn Hasher);
    fn debug(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<K: Hash + Eq + fmt::Debug + 'static> ErasedKey for K {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn equals(&self, other: &dyn ErasedKey) -> bool {
        other.as_any().downcast_ref::<K>().is_some_and(|other| other == self)
    }

    /// With its type's name, as what's keyed by a hash alone (a `key(…)`, a saved state's place)
    /// has no `==` to tell `IdA(1)` from `IdB(1)`.
    fn hash_with(&self, mut hasher: &mut dyn Hasher) {
        type_name::<K>().hash(&mut hasher);
        self.hash(&mut hasher);
    }

    fn debug(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

/// What identifies an entry's content and its decorators' state, as Kotlin's `Any`: any value with
/// `==` and a hash, read back by type. Entries with equal keys share them. A `&str` becomes a
/// `String`, so `downcast_ref::<String>()` reads either.
#[derive(Clone)]
pub struct ContentKey(Rc<dyn ErasedKey>);

impl ContentKey {
    /// `value` as a key: a `&str` is a `String`, so they're equal, and a key stays itself.
    pub fn new(value: impl IntoContentKey) -> ContentKey {
        value.into_content_key()
    }

    /// The value, if it's a `K`.
    pub fn downcast_ref<K: 'static>(&self) -> Option<&K> {
        self.0.as_any().downcast_ref()
    }
}

/// What makes a [`ContentKey`]: any value with `==` and a hash.
pub trait IntoContentKey {
    fn into_content_key(self) -> ContentKey;
}

impl<K: Hash + Eq + fmt::Debug + 'static> IntoContentKey for K {
    fn into_content_key(self) -> ContentKey {
        let value: &dyn Any = &self;
        if let Some(key) = value.downcast_ref::<ContentKey>() {
            return key.clone();
        }
        if let Some(text) = value.downcast_ref::<&'static str>() {
            return ContentKey(Rc::new((*text).to_string()));
        }
        ContentKey(Rc::new(self))
    }
}

impl PartialEq for ContentKey {
    fn eq(&self, other: &ContentKey) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0.equals(&*other.0)
    }
}

impl Eq for ContentKey {}

impl Hash for ContentKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut state: &mut dyn Hasher = state;
        self.0.hash_with(&mut state);
    }
}

impl fmt::Debug for ContentKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.debug(formatter)
    }
}

impl ComposeArg for ContentKey {
    fn same(&self, other: &ContentKey) -> bool {
        self == other
    }
}
