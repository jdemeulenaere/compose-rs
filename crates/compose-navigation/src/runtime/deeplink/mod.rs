//! Navigation 3's deep links (`androidx.navigation3.runtime.deeplink`): a [`DeepLinkRequest`] is
//! matched by [`DeepLinkMatcher`]s, a [`UriDeepLinkMatcher`] decoding a key from the arguments of a
//! URI by a pattern, and a match can bring a back stack (D1414).

mod back_stack;
mod matcher;
mod pattern;
mod request;
mod uri;
mod uri_matcher;

pub use back_stack::{BackStackMatchResult, BackStackMatcher};
pub use matcher::{
    DeepLinkFilter, DeepLinkMatchResult, DeepLinkMatcher, MatchResult, MatchResultAsAny,
    StaticKeyDeepLinkMatcher, WrappedMatchResult, mime_type_filter,
};
#[doc(hidden)]
pub use pattern::{
    ParsedFragment, ParsedPath, ParsedQuery, extract_fragment_args, extract_path_args,
    extract_query_args, parse_fragment, parse_path, parse_query,
};
pub use request::{
    DeepLinkRequest, MimeTypeExtrasKey, RequestExtras, RequestExtrasKey, RequestExtrasScope,
    empty_request_extras, mime_type_extra, request_extras,
};
pub use uri::DeepLinkUri;
pub use uri_matcher::{UriDeepLinkMatcher, UriMatchResult, serializer_saver};
