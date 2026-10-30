//! `DeepLinkMatcher.kt`, `StaticKeyDeepLinkMatcher.kt` and `WrappedMatchResult.kt`.

use std::any::Any;
use std::cmp::Ordering;
use std::rc::Rc;

use super::back_stack::BackStackMatcher;
use super::request::{DeepLinkRequest, MimeTypeExtrasKey};

/// A filter of a [`DeepLinkMatcher`], as `DeepLinkMatcher.Filter`: what it declares must be in a
/// request, which may have more.
#[derive(Clone)]
pub struct DeepLinkFilter(Rc<dyn Fn(&DeepLinkRequest) -> bool>);

impl DeepLinkFilter {
    pub fn new(filter: impl Fn(&DeepLinkRequest) -> bool + 'static) -> DeepLinkFilter {
        DeepLinkFilter(Rc::new(filter))
    }

    /// Whether `request` passes.
    pub fn filter_request(&self, request: &DeepLinkRequest) -> bool {
        (self.0)(request)
    }
}

/// A filter of a request's mime type, whatever its case, as `DeepLinkMatcher.mimeTypeFilter`.
pub fn mime_type_filter(mime_type: &str) -> DeepLinkFilter {
    let mime_type = mime_type.to_string();
    DeepLinkFilter::new(move |request| {
        request
            .extras()
            .get(&MimeTypeExtrasKey)
            .is_some_and(|requested| mime_type.to_lowercase() == requested.to_lowercase())
    })
}

/// What tells a result's type, for [`DeepLinkMatchResult::compare_to`]'s `is`: only results
/// implement it, not what holds them.
pub trait MatchResultAsAny<T> {
    fn as_any(&self) -> &dyn Any;
}

impl<T, R: DeepLinkMatchResult<T>> MatchResultAsAny<T> for R {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// What a [`DeepLinkMatcher`] returns on a match, as `DeepLinkMatcher.MatchResult`: Kotlin's open
/// class is this trait, and [`MatchResult`] its instances.
pub trait DeepLinkMatchResult<T>: MatchResultAsAny<T> + 'static {
    /// The navigation key the link is for.
    fn key(&self) -> &T;

    /// How it ranks against `other`, the greatest being the best match. As good as any by default.
    fn compare_to(&self, other: &dyn DeepLinkMatchResult<T>) -> Ordering {
        let _ = other;
        Ordering::Equal
    }

    /// The result it wraps, if it's a wrapping one, as Kotlin's `is WrappedMatchResult`.
    fn wrapped(&self) -> Option<&dyn DeepLinkMatchResult<T>> {
        None
    }
}

/// A result of a key and nothing more, as `DeepLinkMatcher.MatchResult(key)`.
#[derive(Clone, PartialEq, Debug)]
pub struct MatchResult<T> {
    key: T,
}

impl<T> MatchResult<T> {
    pub fn new(key: T) -> MatchResult<T> {
        MatchResult { key }
    }
}

impl<T: 'static> DeepLinkMatchResult<T> for MatchResult<T> {
    fn key(&self) -> &T {
        &self.key
    }
}

/// A result held behind a handle, the same result for all it does.
impl<T: 'static> DeepLinkMatchResult<T> for Rc<dyn DeepLinkMatchResult<T>> {
    fn key(&self) -> &T {
        (**self).key()
    }

    fn compare_to(&self, other: &dyn DeepLinkMatchResult<T>) -> Ordering {
        (**self).compare_to(other)
    }

    fn wrapped(&self) -> Option<&dyn DeepLinkMatchResult<T>> {
        Some(&**self)
    }
}

/// A result layered over another without changing it, as `WrappedMatchResult`: it ranks as what it
/// wraps, against another wrapped one's inside.
#[derive(Clone)]
pub struct WrappedMatchResult<T> {
    match_result: Rc<dyn DeepLinkMatchResult<T>>,
}

impl<T: 'static> WrappedMatchResult<T> {
    pub fn new(match_result: impl DeepLinkMatchResult<T>) -> WrappedMatchResult<T> {
        WrappedMatchResult { match_result: Rc::new(match_result) }
    }

    /// What it wraps.
    pub fn match_result(&self) -> &dyn DeepLinkMatchResult<T> {
        &*self.match_result
    }
}

impl<T: 'static> DeepLinkMatchResult<T> for WrappedMatchResult<T> {
    fn key(&self) -> &T {
        self.match_result.key()
    }

    fn compare_to(&self, other: &dyn DeepLinkMatchResult<T>) -> Ordering {
        self.match_result.compare_to(other.wrapped().unwrap_or(other))
    }

    fn wrapped(&self) -> Option<&dyn DeepLinkMatchResult<T>> {
        Some(&*self.match_result)
    }
}

/// What matches deep links to a key `T`, as `DeepLinkMatcher<T, R>`: `Result` is `R`, Kotlin's
/// covariance being an enum of keys or [`UriDeepLinkMatcher::map_key`](super::UriDeepLinkMatcher).
pub trait DeepLinkMatcher<T> {
    type Result;

    /// What a request must pass before [`match_request`](Self::match_request) is asked.
    fn filters(&self) -> &[DeepLinkFilter] {
        &[]
    }

    /// The result of `request`, which has passed the filters, or `None`.
    fn match_request(&self, request: &DeepLinkRequest) -> Option<Self::Result>;

    /// The result of `request`, if it matches, as `match`, a keyword here: filters first, so that
    /// no key is made and thrown away.
    #[doc(alias = "match")]
    fn try_match(&self, request: &DeepLinkRequest) -> Option<Self::Result> {
        if self.filters().iter().all(|filter| filter.filter_request(request)) {
            self.match_request(request)
        } else {
            None
        }
    }

    /// This matcher building a back stack upon a match, from the result, as `withBackStack`. It's
    /// built when it's first asked for.
    fn with_back_stack<K>(
        self,
        back_stack_builder: impl Fn(&dyn DeepLinkMatchResult<T>) -> Vec<K> + 'static,
    ) -> BackStackMatcher<Self, T, K>
    where
        Self: Sized,
    {
        BackStackMatcher::new(self, back_stack_builder)
    }
}

/// Matches by filters and gives `key`, as `StaticKeyDeepLinkMatcher`.
#[derive(Clone)]
pub struct StaticKeyDeepLinkMatcher<T> {
    key: T,
    filters: Vec<DeepLinkFilter>,
}

impl<T> StaticKeyDeepLinkMatcher<T> {
    pub fn new(key: T, filters: Vec<DeepLinkFilter>) -> StaticKeyDeepLinkMatcher<T> {
        StaticKeyDeepLinkMatcher { key, filters }
    }

    pub fn key(&self) -> &T {
        &self.key
    }
}

impl<T: Clone> DeepLinkMatcher<T> for StaticKeyDeepLinkMatcher<T> {
    type Result = MatchResult<T>;

    fn filters(&self) -> &[DeepLinkFilter] {
        &self.filters
    }

    fn match_request(&self, _request: &DeepLinkRequest) -> Option<MatchResult<T>> {
        Some(MatchResult::new(self.key.clone()))
    }
}
