//! `DeepLinkRequest.kt` and `RequestExtras.kt`.

use std::any::{Any, type_name};
use std::collections::BTreeMap;
use std::fmt;
use std::ops::{Add, Sub};
use std::rc::Rc;

use super::uri::DeepLinkUri;

/// A key of a value of type `Value` in [`RequestExtras`], as `RequestExtrasKey<T>`: told apart by
/// its [`name`](Self::name), its type's, so a key type with instances of its own says which.
pub trait RequestExtrasKey {
    type Value: PartialEq + fmt::Debug + 'static;

    fn name(&self) -> String {
        type_name::<Self>().to_string()
    }
}

/// A value of a type erased: what compares and prints it, as Kotlin's `Any`.
trait ExtraValue {
    fn as_any(&self) -> &dyn Any;
    fn equals(&self, other: &dyn ExtraValue) -> bool;
    fn print(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<V: PartialEq + fmt::Debug + 'static> ExtraValue for V {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn equals(&self, other: &dyn ExtraValue) -> bool {
        other.as_any().downcast_ref::<V>().is_some_and(|other| self == other)
    }
    fn print(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

/// What [`request_extras`] builds with.
#[derive(Default)]
pub struct RequestExtrasScope {
    map: BTreeMap<String, Rc<dyn ExtraValue>>,
}

impl RequestExtrasScope {
    /// Keeps `value` by `key`.
    pub fn put<K: RequestExtrasKey>(&mut self, key: &K, value: K::Value) {
        self.map.insert(key.name(), Rc::new(value));
    }
}

/// [`RequestExtras`] made by `builder`, as `requestExtras { … }`.
pub fn request_extras(builder: impl FnOnce(&mut RequestExtrasScope)) -> RequestExtras {
    let mut scope = RequestExtrasScope::default();
    builder(&mut scope);
    RequestExtras { map: scope.map }
}

/// No extras, as `emptyRequestExtras()`.
pub fn empty_request_extras() -> RequestExtras {
    RequestExtras::default()
}

/// Values by [`RequestExtrasKey`], for a [`DeepLinkRequest`], as `RequestExtras`.
#[derive(Clone, Default)]
pub struct RequestExtras {
    map: BTreeMap<String, Rc<dyn ExtraValue>>,
}

impl RequestExtras {
    pub fn size(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn is_not_empty(&self) -> bool {
        !self.map.is_empty()
    }

    /// The value of `key`, as `extras[key]`.
    pub fn get<K: RequestExtrasKey>(&self, key: &K) -> Option<&K::Value> {
        self.map.get(&key.name())?.as_any().downcast_ref()
    }

    /// Whether `key` has a value, as `key in extras`.
    pub fn contains<K: RequestExtrasKey>(&self, key: &K) -> bool {
        self.map.contains_key(&key.name())
    }
}

/// These with `other`'s, `other`'s going first.
impl Add for RequestExtras {
    type Output = RequestExtras;

    fn add(mut self, other: RequestExtras) -> RequestExtras {
        self.map.extend(other.map);
        self
    }
}

/// These without `other`'s keys.
impl Sub for RequestExtras {
    type Output = RequestExtras;

    fn sub(mut self, other: RequestExtras) -> RequestExtras {
        self.map.retain(|name, _| !other.map.contains_key(name));
        self
    }
}

impl PartialEq for RequestExtras {
    fn eq(&self, other: &RequestExtras) -> bool {
        self.map.len() == other.map.len()
            && self.map.iter().all(|(name, value)| {
                other.map.get(name).is_some_and(|other| value.equals(&**other))
            })
    }
}

impl fmt::Debug for RequestExtras {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RequestExtras(")?;
        for (index, (name, value)) in self.map.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{name}=")?;
            value.print(formatter)?;
        }
        formatter.write_str(")")
    }
}

/// The key of what [`mime_type_extra`] gives, as `MimeTypeExtrasKey`.
#[derive(Clone, Copy, Debug)]
pub struct MimeTypeExtrasKey;

impl RequestExtrasKey for MimeTypeExtrasKey {
    type Value = String;
}

/// Extras with `mime_type`, as `DeepLinkRequest.mimeTypeExtra`.
pub fn mime_type_extra(mime_type: &str) -> RequestExtras {
    request_extras(|scope| scope.put(&MimeTypeExtrasKey, mime_type.to_string()))
}

/// A requested deep link, as `DeepLinkRequest`: a URI, extras, or both.
#[derive(Clone, PartialEq, Debug)]
pub struct DeepLinkRequest {
    uri: Option<DeepLinkUri>,
    extras: RequestExtras,
}

impl DeepLinkRequest {
    /// The request of `uri` and `extras`: panics with neither.
    pub fn new(uri: Option<DeepLinkUri>, extras: RequestExtras) -> DeepLinkRequest {
        assert!(
            uri.is_some() || extras.is_not_empty(),
            "DeepLinkRequest must have either a uri or extras."
        );
        DeepLinkRequest { uri, extras }
    }

    /// The request of a URI string.
    pub fn of(uri: &str) -> DeepLinkRequest {
        DeepLinkRequest::new(Some(DeepLinkUri::new(uri)), empty_request_extras())
    }

    pub fn uri(&self) -> Option<&DeepLinkUri> {
        self.uri.as_ref()
    }

    pub fn extras(&self) -> &RequestExtras {
        &self.extras
    }
}

impl fmt::Display for DeepLinkRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DeepLinkRequest{")?;
        if let Some(uri) = &self.uri {
            write!(formatter, " uri={uri}")?;
        }
        if self.extras.is_not_empty() {
            write!(formatter, " extras={:?}", self.extras)?;
        }
        formatter.write_str(" }")
    }
}
