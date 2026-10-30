//! `UriDeepLinkMatcher.kt`: the matcher itself, and its result.

use std::cell::OnceCell;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;

use compose_runtime::__private::{
    DeepLinkArguments, DeepLinkDecoderError, DeepLinkKey, DeepLinkSerializer,
};
use compose_runtime::{SavedValue, Saver, saver};

use super::matcher::{DeepLinkFilter, DeepLinkMatchResult, DeepLinkMatcher};
use super::pattern::{
    ParsedFragment, ParsedPath, ParsedQuery, extract_fragment_args, extract_path_args,
    extract_query_args, parse_fragment, parse_path, parse_query,
};
use super::request::DeepLinkRequest;
use super::uri::DeepLinkUri;

/// `serializer` as a saver, so that a value it reads is saved as its string, as
/// `rememberSerializable(serializer = …)`.
pub fn serializer_saver<T: 'static>(
    serializer: impl DeepLinkSerializer<T> + 'static,
) -> impl Saver<T> {
    let serializer = Rc::new(serializer);
    let restoring = serializer.clone();
    saver(
        move |value| Some(SavedValue::Text(serializer.serialize(value))),
        move |saved| match saved {
            SavedValue::Text(text) => Some(restoring.deserialize(text)),
            _ => None,
        },
    )
}

struct ParsedPattern {
    path: ParsedPath,
    query: ParsedQuery,
    fragment: Option<ParsedFragment>,
}

type KeyDecoder<T> = Rc<dyn Fn(&DeepLinkArguments) -> Result<T, DeepLinkDecoderError>>;

/// Whether all of `text` starts as a scheme does: RFC 3986's section 3.1.
fn has_scheme(text: &str) -> bool {
    let mut characters = text.chars();
    characters.next().is_some_and(|first| first.is_ascii_alphabetic())
        && characters.find(|character| {
            !character.is_ascii_alphanumeric() && !matches!(character, '+' | '-' | '.')
        }) == Some(':')
}

/// Matches requests by a URI pattern and decodes a key from their arguments, as `UriDeepLinkMatcher`.
///
/// A pattern's path is literals, `{placeholder}`s (several in a segment too) and `.*`; its query
/// names parameters (`?name={name}`), or takes what's unnamed (`?{rest}`), and a repeated parameter
/// gives a collection all its values; its fragment is as a segment. A pattern with no scheme takes
/// `http` and `https`, as `http` does; `https` takes itself. Its parsing panics, at the first
/// match, on a placeholder's name used twice.
///
/// Kotlin's is an open class with `matchUri` and `matchArguments` to override: a matcher of an
/// app's holds this one and calls them.
///
/// A key is what's decoded: a struct of fields, with `#[derive(DeepLinkKey)]`.
///
/// ```
/// # extern crate compose_runtime as compose;
/// use compose_navigation::{DeepLinkKey, DeepLinkRequest, DeepLinkUri, DeepLinkMatcher, UriDeepLinkMatcher};
///
/// #[derive(DeepLinkKey)]
/// struct User {
///     id: u32,
/// }
///
/// let matcher = UriDeepLinkMatcher::<User>::new(DeepLinkUri::new("example.com/users/{id}"));
/// let request = DeepLinkRequest::of("https://example.com/users/7");
/// assert!(matcher.try_match(&request).is_some());
/// ```
///
/// What Kotlin's decoder refuses when it runs doesn't compile: a map,
///
/// ```compile_fail,E0277
/// # extern crate compose_runtime as compose;
/// use std::collections::HashMap;
///
/// use compose_navigation::DeepLinkKey;
///
/// #[derive(DeepLinkKey)]
/// struct OfMap {
///     map: HashMap<i32, String>,
/// }
/// ```
///
/// and a collection of keys.
///
/// ```compile_fail,E0277
/// # extern crate compose_runtime as compose;
/// use compose_navigation::DeepLinkKey;
///
/// #[derive(DeepLinkKey)]
/// struct User {
///     id: u32,
/// }
///
/// #[derive(DeepLinkKey)]
/// struct OfKeys {
///     users: Vec<User>,
/// }
/// ```
pub struct UriDeepLinkMatcher<T> {
    normalized_uri_pattern: DeepLinkUri,
    decode: KeyDecoder<T>,
    filters: Vec<DeepLinkFilter>,
    parsed_pattern: OnceCell<ParsedPattern>,
}

impl<T> UriDeepLinkMatcher<T> {
    /// A matcher of `uri_pattern`, decoding a [`DeepLinkKey`].
    pub fn new(uri_pattern: DeepLinkUri) -> UriDeepLinkMatcher<T>
    where
        T: DeepLinkKey + 'static,
    {
        UriDeepLinkMatcher::with_decoder(uri_pattern, |arguments| {
            T::decode_field(arguments, "")?.ok_or_else(|| DeepLinkDecoderError::missing(""))
        })
    }

    /// A matcher of `uri_pattern`, with what makes a key of the arguments.
    pub fn with_decoder(
        uri_pattern: DeepLinkUri,
        decode: impl Fn(&DeepLinkArguments) -> Result<T, DeepLinkDecoderError> + 'static,
    ) -> UriDeepLinkMatcher<T> {
        // Without a scheme or `//`, what comes first is a path, not an authority.
        let text = uri_pattern.to_string();
        let normalized_uri_pattern = if has_scheme(&text) || text.starts_with("//") {
            uri_pattern
        } else {
            DeepLinkUri::new(format!("//{text}"))
        };
        UriDeepLinkMatcher {
            normalized_uri_pattern,
            decode: Rc::new(decode),
            filters: Vec::new(),
            parsed_pattern: OnceCell::new(),
        }
    }

    /// This matcher with `filters` too.
    pub fn with_filters(mut self, filters: Vec<DeepLinkFilter>) -> UriDeepLinkMatcher<T> {
        self.filters = filters;
        self
    }

    /// This matcher's key made into another: an enum of the app's keys, say, Kotlin's covariance.
    pub fn map_key<U>(self, map: impl Fn(T) -> U + 'static) -> UriDeepLinkMatcher<U>
    where
        T: 'static,
    {
        let decode = self.decode;
        UriDeepLinkMatcher {
            normalized_uri_pattern: self.normalized_uri_pattern,
            decode: Rc::new(move |arguments| decode(arguments).map(&map)),
            filters: self.filters,
            parsed_pattern: OnceCell::new(),
        }
    }

    fn parsed_pattern(&self) -> &ParsedPattern {
        self.parsed_pattern.get_or_init(|| {
            let pattern = &self.normalized_uri_pattern;
            let mut taken_arg_names = HashSet::new();
            let path = parse_path(pattern, &mut taken_arg_names);
            let query = parse_query(pattern, &mut taken_arg_names);
            let fragment = parse_fragment(pattern, &mut taken_arg_names);
            ParsedPattern { path, query, fragment }
        })
    }

    /// Matches a URI, as `matchUri`: its scheme, authority and path, then a key of what they and
    /// its query and fragment give.
    pub fn match_uri(&self, uri: &DeepLinkUri) -> Option<UriMatchResult<T>> {
        let parsed = self.parsed_pattern();
        if !parsed.path.matches_scheme(uri.scheme().unwrap_or("")) {
            return None;
        }
        let authority = uri.authority()?.to_lowercase();
        if Some(authority) != self.normalized_uri_pattern.authority().map(|a| a.to_lowercase()) {
            return None;
        }
        let path_args = extract_path_args(&parsed.path, uri)?;
        let query_args = extract_query_args(&parsed.query, uri);
        let fragment_args = extract_fragment_args(parsed.fragment.as_ref(), uri);
        self.match_arguments(path_args, query_args, fragment_args)
    }

    /// Makes a key of the arguments, as `matchArguments`: the path's go first as they're required,
    /// then the query's. `None` if it can't be made.
    pub fn match_arguments(
        &self,
        path_args: DeepLinkArguments,
        query_args: DeepLinkArguments,
        fragment_args: DeepLinkArguments,
    ) -> Option<UriMatchResult<T>> {
        let matching_path_argument_count = path_args.len() as i32;
        let is_exact_path = path_args.is_empty()
            && !self.normalized_uri_pattern.path_segments().iter().any(|segment| segment == ".*");
        let mut arguments = fragment_args;
        arguments.extend(query_args);
        arguments.extend(path_args);
        let key = (self.decode)(&arguments).ok()?;
        Some(UriMatchResult { key, arguments, is_exact_path, matching_path_argument_count })
    }
}

impl<T: 'static> DeepLinkMatcher<T> for UriDeepLinkMatcher<T> {
    type Result = UriMatchResult<T>;

    fn filters(&self) -> &[DeepLinkFilter] {
        &self.filters
    }

    fn match_request(&self, request: &DeepLinkRequest) -> Option<UriMatchResult<T>> {
        self.match_uri(request.uri()?)
    }
}

/// A [`UriDeepLinkMatcher`]'s result, as `UriMatchResult`. Against another, a better match is, in
/// order: of a URI, against anything else; of an exact path, with no argument or wildcard; of more
/// of the path's arguments; of any argument; of more arguments.
#[derive(Clone, Debug)]
pub struct UriMatchResult<T> {
    key: T,
    arguments: DeepLinkArguments,
    is_exact_path: bool,
    matching_path_argument_count: i32,
}

impl<T> UriMatchResult<T> {
    /// A result not from a matcher: the worst of a match.
    pub fn new(key: T, arguments: DeepLinkArguments) -> UriMatchResult<T> {
        UriMatchResult { key, arguments, is_exact_path: false, matching_path_argument_count: -1 }
    }

    /// The arguments of the request.
    pub fn arguments(&self) -> &DeepLinkArguments {
        &self.arguments
    }

    /// Whether the path matched with no argument or wildcard.
    pub fn is_exact_path(&self) -> bool {
        self.is_exact_path
    }

    /// This result of another key, to rank it with results of other types, as Kotlin's
    /// `UriMatchResult<Any>`.
    pub fn map_key<U>(self, map: impl FnOnce(T) -> U) -> UriMatchResult<U> {
        UriMatchResult {
            key: map(self.key),
            arguments: self.arguments,
            is_exact_path: self.is_exact_path,
            matching_path_argument_count: self.matching_path_argument_count,
        }
    }
}

impl<T: 'static> DeepLinkMatchResult<T> for UriMatchResult<T> {
    fn key(&self) -> &T {
        &self.key
    }

    fn compare_to(&self, other: &dyn DeepLinkMatchResult<T>) -> Ordering {
        let other = other.wrapped().unwrap_or(other);
        let Some(other) = other.as_any().downcast_ref::<UriMatchResult<T>>() else {
            return Ordering::Greater;
        };
        self.is_exact_path
            .cmp(&other.is_exact_path)
            .then(self.matching_path_argument_count.cmp(&other.matching_path_argument_count))
            .then((!self.arguments.is_empty()).cmp(&!other.arguments.is_empty()))
            .then(self.arguments.len().cmp(&other.arguments.len()))
    }
}
