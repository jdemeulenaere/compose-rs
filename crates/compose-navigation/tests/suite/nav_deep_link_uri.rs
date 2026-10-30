//! `UriDeepLinkMatcher` and how its results rank, against Navigation 3's `UriDeepLinkMatcherTest`.

use std::cmp::Ordering;

use compose_navigation::*;

use crate::nav_deep_link::*;

fn matcher<T: DeepLinkKey + 'static>(pattern: &str) -> UriDeepLinkMatcher<T> {
    UriDeepLinkMatcher::new(DeepLinkUri::new(pattern))
}

fn result<T: DeepLinkKey + 'static>(pattern: &str, request: &str) -> Option<UriMatchResult<T>> {
    matcher::<T>(pattern).r#match(&DeepLinkRequest::of(request))
}

fn key<T: DeepLinkKey + Clone + 'static>(pattern: &str, request: &str) -> Option<T> {
    result::<T>(pattern, request).map(|result| result.key().clone())
}

fn matches(pattern: &str, request: &str) -> bool {
    result::<TestDefaultArgKey>(pattern, request).is_some()
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_schemeMismatch
#[test]
fn another_scheme_is_no_match() {
    assert!(!matches("http://example.com/path", "ftp://example.com/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_schemeHttp
#[test]
fn http_takes_https_too() {
    assert!(matches("http://example.com/path", "https://example.com/path"));
    assert!(matches("http://example.com/path", "http://example.com/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_schemeHttps
#[test]
fn https_takes_only_itself() {
    assert!(matches("https://example.com/path", "https://example.com/path"));
    assert!(!matches("https://example.com/path", "http://example.com/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_defaultSchemeHttpHttpsMatch
#[test]
fn a_network_path_takes_http_and_https() {
    assert!(matches("//example.com/path", "https://example.com/path"));
    assert!(matches("//example.com/path", "http://example.com/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_defaultSchemeHttpHttpsMatch_noAuthority
#[test]
fn no_scheme_nor_slashes_is_an_authority_all_the_same() {
    assert!(matches("www.example.com/path", "https://www.example.com/path"));
    assert!(matches("example.com/path", "http://example.com/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_schemeMatchCaseInsensitive
#[test]
fn a_scheme_ignores_case() {
    assert!(matches("http://example.com/path", "HTTP://example.com/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_authorityMismatch
#[test]
fn another_authority_is_no_match() {
    assert!(!matches("http://example.com/path", "http://example.org/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_authorityMatchCaseInsensitive
#[test]
fn an_authority_ignores_case() {
    assert!(matches("http://example.com/path", "http://EXAMPLE.COM/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_pathSegmentShorterMismatch
#[test]
fn a_shorter_path_is_no_match() {
    assert!(!matches("http://example.com/path/subpath", "http://example.com/path"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_pathSegmentLongerMismatch
#[test]
fn a_longer_path_is_no_match() {
    assert!(!matches("http://example.com/path", "http://example.com/path/extra"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_pathSegmentSizeMatch
#[test]
fn the_same_path_matches() {
    assert!(matches("http://example.com/path/subpath", "http://example.com/path/subpath"));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_pathSegmentSizeMatch_trailingWildcard
#[test]
fn a_last_wildcard_takes_more_segments() {
    assert!(matches("http://example.com/path/.*", "http://example.com/path/wildcard/segments"));
}

/// `first` ranks above `second`, as `maxOf` picks: of a key that's nothing for each, Kotlin's
/// `UriMatchResult<Any>` for results of other keys.
fn first_wins<A: Clone, B: Clone>(first: &UriMatchResult<A>, second: &UriMatchResult<B>) -> bool {
    let first = first.clone().map_key(|_| ());
    let second = second.clone().map_key(|_| ());
    first.compare_to(&second) == Ordering::Greater && second.compare_to(&first) == Ordering::Less
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_exactPathWins
#[test]
fn an_exact_path_beats_a_wildcard() {
    let request = "https://example.com/path";
    let exact = result::<TestKey>("https://example.com/path", request).unwrap();
    let wildcard = result::<TestKey>("https://example.com/.*", request).unwrap();
    assert!(first_wins(&exact, &wildcard));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_morePathArgsWin
#[test]
fn more_of_a_path_s_arguments_win() {
    let request = "https://example.com/john/123/anything";
    let more = result::<TestArgKey>("https://example.com/{name}/{id}/.*", request).unwrap();
    let fewer = result::<TestArgKey>("https://example.com/{name}/.*", request).unwrap();
    assert!(first_wins(&more, &fewer));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_moreQueryArgsWin
#[test]
fn more_of_a_query_s_arguments_win() {
    let request = "https://example.com/john?q=abc";
    let fewer = result::<TestArgKey>("https://example.com/{name}", request).unwrap();
    let more = result::<TestArgKey>("https://example.com/{name}?q={q}", request).unwrap();
    assert!(first_wins(&more, &fewer));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_exactPathWins_differentKeys
#[test]
fn an_exact_path_beats_a_placeholder_of_another_key() {
    let request = "https://example.com/path";
    let exact = result::<TestKey>("https://example.com/path", request).unwrap();
    let placeholder = result::<TestArgKey>("https://example.com/{name}", request).unwrap();
    assert!(first_wins(&exact, &placeholder));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_morePathArgsWin_differentKeys
#[test]
fn more_of_a_path_s_arguments_win_for_another_key() {
    let request = "https://example.com/john/123/anything";
    let fewer = result::<TestArgKey>("https://example.com/{name}/.*", request).unwrap();
    let more = result::<TestKey>("https://example.com/{name}/{id}/.*", request).unwrap();
    assert!(first_wins(&more, &fewer));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_moreQueryArgsWin_differentKeys
#[test]
fn more_of_a_query_s_arguments_win_for_another_key() {
    let request = "https://example.com/john?q=abc";
    let more = result::<TestArgKey>("https://example.com/{name}?q={q}", request).unwrap();
    let fewer = result::<TestKey>("https://example.com/{name}", request).unwrap();
    assert!(first_wins(&more, &fewer));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_moreFragmentArgsWin
#[test]
fn more_of_a_fragment_s_arguments_win() {
    let request = "https://example.com/john#123";
    let more = result::<TestArgKey>("https://example.com/{name}#{id}", request).unwrap();
    let fewer = result::<TestArgKey>("https://example.com/{name}", request).unwrap();
    assert!(first_wins(&more, &fewer));
}

fn assert_tie(pattern_one: &str, pattern_two: &str, request: &str) {
    let one = result::<TestDefaultArgKey>(pattern_one, request).unwrap();
    let two = result::<TestDefaultArgKey>(pattern_two, request).unwrap();
    assert_eq!(one.compare_to(&two), Ordering::Equal);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_queryAndFragmentAreEqual
#[test]
fn a_query_s_argument_ties_with_a_fragment_s() {
    assert_tie(
        "https://example.com/{name}?q={q}",
        "https://example.com/{name}#{id}",
        "https://example.com/john?q=abc#123",
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_differentQueryNamesAreEqual
#[test]
fn a_query_s_names_don_t_rank() {
    assert_tie(
        "https://example.com/{name}?a={a}",
        "https://example.com/{name}?b={b}",
        "https://example.com/john?a=1&b=2",
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_differentPathNamesAreEqual
#[test]
fn a_path_s_names_don_t_rank() {
    assert_tie(
        "https://example.com/{id}/{name}",
        "https://example.com/{first}/{last}",
        "https://example.com/123/john",
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_nonUriMatchResult
#[test]
fn a_uri_s_result_beats_any_other_kind() {
    let uri = result::<TestKey>("https://example.com/path", "https://example.com/path").unwrap();
    assert_eq!(uri.compare_to(&MatchResult::new(TestKey)), Ordering::Greater);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_wrappedSameInnerType
#[test]
fn a_uri_s_result_ranks_by_what_a_wrapped_one_wraps() {
    let request = "https://example.com/path";
    let exact = result::<TestKey>("https://example.com/path", request).unwrap();
    let wildcard = result::<TestKey>("https://example.com/.*", request).unwrap();
    assert_eq!(exact.compare_to(&WrappedMatchResult::new(wildcard.clone())), Ordering::Greater);
    assert_eq!(wildcard.compare_to(&WrappedMatchResult::new(exact.clone())), Ordering::Less);
    assert_eq!(exact.compare_to(&WrappedMatchResult::new(exact.clone())), Ordering::Equal);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, testCompareTo_wrappedDifferentInnerType
#[test]
fn a_uri_s_result_beats_another_kind_wrapped() {
    let uri = result::<TestKey>("https://example.com/path", "https://example.com/path").unwrap();
    let wrapped = WrappedMatchResult::new(MatchResult::new(TestKey));
    assert_eq!(uri.compare_to(&wrapped), Ordering::Greater);
}

/// Checks `value` decodes and `wrong` doesn't, by one placeholder of `{name}` in a path.
fn checks_placeholder<T: DeepLinkKey + PartialEq + Clone + std::fmt::Debug + 'static>(
    name: &str,
    value: &str,
    expected: T,
    wrong: &str,
) {
    let pattern = format!("https://example.com/path/{{{name}}}");
    assert_eq!(key::<T>(&pattern, &format!("https://example.com/path/{value}")), Some(expected));
    assert_eq!(key::<T>(&pattern, &format!("https://example.com/path/{wrong}")), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_booleanArgument
#[test]
fn a_placeholder_gives_a_boolean() {
    checks_placeholder("bool", "true", BooleanKey { bool: true }, "notABoolean");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_byteArgument
#[test]
fn a_placeholder_gives_a_byte() {
    checks_placeholder("byte", "1", ByteKey { byte: 1 }, "notAByte");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_shortArgument
#[test]
fn a_placeholder_gives_a_short() {
    checks_placeholder("short", "2", ShortKey { short: 2 }, "notAShort");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_longArgument
#[test]
fn a_placeholder_gives_a_long() {
    checks_placeholder("long", "4", LongKey { long: 4 }, "notALong");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_floatArgument
#[test]
fn a_placeholder_gives_a_float() {
    checks_placeholder("float", "5.0", FloatKey { float: 5.0 }, "notAFloat");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_doubleArgument
#[test]
fn a_placeholder_gives_a_double() {
    checks_placeholder("double", "6.0", DoubleKey { double: 6.0 }, "notADouble");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_charArgument
#[test]
fn a_placeholder_gives_a_char() {
    checks_placeholder("char", "a", CharKey { char: 'a' }, "");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_intArgument
#[test]
fn a_placeholder_gives_an_int() {
    checks_placeholder("int", "1", IntKey { int: 1 }, "notAnInt");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_intListArgument
#[test]
fn a_repeated_parameter_gives_a_list() {
    let pattern = "https://example.com/path?list={list}";
    let key_of = |request| key::<ListKey>(pattern, request);
    assert_eq!(key_of("https://example.com/path?list=1&list=2").unwrap().list, [1, 2]);
    assert_eq!(key_of("https://example.com/path?list=a&list=b"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_multiplePlaceholdersSingleSegment
#[test]
fn placeholders_of_a_segment_give_a_key() {
    let pattern = "https://example.com/user_user_{name}_{age}";
    assert_eq!(
        key::<SimpleKey>(pattern, "https://example.com/user_user_john_30"),
        Some(SimpleKey { name: "john".into(), age: 30 })
    );
    assert_eq!(key::<SimpleKey>(pattern, "https://example.com/user_user_john_notAnAge"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_defaultValueFallback
#[test]
fn what_s_absent_takes_its_default() {
    let pattern = "https://example.com/user?name={name}";
    assert_eq!(
        key::<DefaultKey>(pattern, "https://example.com/user?name=john"),
        Some(DefaultKey { name: "john".into(), age: 0 })
    );
    assert_eq!(
        key::<DefaultKey>(pattern, "https://example.com/user"),
        Some(DefaultKey { name: "default".into(), age: 0 })
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_pathEmptyStringArgument
#[test]
fn a_path_s_empty_string_is_a_string() {
    assert_eq!(
        key::<SimpleKey>("https://example.com/{name}/{age}", "https://example.com//30"),
        Some(SimpleKey { name: String::new(), age: 30 })
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_pathEmptyNullableIntWithNullFallback
#[test]
fn a_path_s_empty_int_takes_its_null_default() {
    assert_eq!(
        key::<NullableDefaultKey>("https://example.com/{name}/{age}", "https://example.com/john/"),
        Some(NullableDefaultKey { name: Some("john".into()), age: None })
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_pathEmptyNullableIntNoFallbackFails
#[test]
fn a_path_s_empty_nullable_int_without_a_default_is_no_match() {
    assert!(
        result::<NullableKey>("https://example.com/{name}/{age}", "https://example.com/john/")
            .is_none()
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_pathEmptyNonNullableIntFails
#[test]
fn a_path_s_empty_int_is_no_match() {
    assert!(
        result::<SimpleKey>("https://example.com/{name}/{age}", "https://example.com/john/")
            .is_none()
    );
}

const USER_QUERY: &str = "https://example.com/user?name={name}&age={age}";

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_queryEmptyStringArgument
#[test]
fn a_query_s_empty_string_is_a_string() {
    assert_eq!(
        key::<SimpleKey>(USER_QUERY, "https://example.com/user?name=&age=30"),
        Some(SimpleKey { name: String::new(), age: 30 })
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_queryEmptyNullableIntWithNullFallback
#[test]
fn a_query_s_empty_int_takes_its_null_default() {
    assert_eq!(
        key::<NullableDefaultKey>(USER_QUERY, "https://example.com/user?name=john&age="),
        Some(NullableDefaultKey { name: Some("john".into()), age: None })
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_queryEmptyNullableIntNoFallbackFails
#[test]
fn a_query_s_empty_nullable_int_without_a_default_is_no_match() {
    assert!(result::<NullableKey>(USER_QUERY, "https://example.com/user?name=john&age=").is_none());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_queryEmptyNonNullableIntFails
#[test]
fn a_query_s_empty_int_is_no_match() {
    assert!(result::<SimpleKey>(USER_QUERY, "https://example.com/user?name=john&age=").is_none());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_missingRequiredArgument
#[test]
fn a_field_the_pattern_can_t_give_is_no_match() {
    assert!(
        result::<SimpleKey>(
            "https://example.com/user?name={name}",
            "https://example.com/user?name=john"
        )
        .is_none()
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_nestedObjects
#[test]
fn placeholders_give_a_nested_key() {
    assert_eq!(
        key::<NestedKey>(
            "https://example.com/user/{name}/{age}/{flag}",
            "https://example.com/user/john/30/true"
        ),
        Some(NestedKey { user: SimpleKey { name: "john".into(), age: 30 }, flag: true })
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_enumArgument
#[test]
fn a_placeholder_gives_an_enum() {
    let pattern = "https://example.com/direction/{direction}";
    assert_eq!(
        key::<EnumKey>(pattern, "https://example.com/direction/NORTH"),
        Some(EnumKey { direction: Direction::North })
    );
    assert_eq!(key::<EnumKey>(pattern, "https://example.com/direction/WEST"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_fragmentPlaceholder
#[test]
fn a_fragment_s_placeholder_gives_a_field() {
    assert_eq!(
        key::<CharKey>("https://example.com/path#{char}", "https://example.com/path#a"),
        Some(CharKey { char: 'a' })
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_fragmentWildcard
#[test]
fn a_fragment_s_wildcard_gives_nothing() {
    let result = result::<TestKey>("https://example.com/path#.*", "https://example.com/path#123");
    assert!(result.unwrap().arguments().is_empty());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_fragmentPartialWildcard
#[test]
fn a_fragment_s_wildcard_after_a_literal_gives_nothing() {
    let result =
        result::<TestKey>("https://example.com/path#a_.*", "https://example.com/path#a_123");
    assert!(result.unwrap().arguments().is_empty());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_duplicatePlaceholderInPath_throws
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_twice_in_a_path_panics_at_the_first_match() {
    matches(
        "https://example.com/users/{id}/details/{id}",
        "https://example.com/users/123/details/456",
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_duplicatePlaceholderInQuery_throws
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_twice_in_a_query_panics_at_the_first_match() {
    matches("https://example.com?user={id}&detail={id}", "https://example.com?user=123&detail=456");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_duplicatePlaceholderInFragment_throws
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_twice_in_a_fragment_panics_at_the_first_match() {
    matches("https://example.com#section_{id}_{id}", "https://example.com#section_123_456");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_duplicatePlaceholderBetweenPathAndQuery_throws
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_in_a_path_and_a_query_panics() {
    matches("https://example.com/users/{id}?user={id}", "https://example.com/users/123?user=456");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_duplicatePlaceholderBetweenPathAndFragment_throws
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_in_a_path_and_a_fragment_panics() {
    matches(
        "https://example.com/users/{id}#section_{id}",
        "https://example.com/users/123#section_456",
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_duplicatePlaceholderBetweenQueryAndFragment_throws
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_in_a_query_and_a_fragment_panics() {
    matches(
        "https://example.com?filter={id}#section_{id}",
        "https://example.com?filter=123#section_456",
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, compare_matchersTypedOnDerivedKeys
#[test]
fn matchers_of_each_key_map_to_one_type() {
    let of = |key: TestInterface| matcher::<TestKey>("www.testuri.com").map_key(move |_| key);
    let matchers = [of(TestInterface::ImplA), of(TestInterface::ImplB)];
    let request = DeepLinkRequest::of("https://www.testuri.com");
    let keys: Vec<TestInterface> =
        matchers.iter().map(|matcher| *matcher.r#match(&request).unwrap().key()).collect();
    assert_eq!(keys, [TestInterface::ImplA, TestInterface::ImplB]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_matchResultTypePreserved
#[test]
fn a_match_gives_a_uri_s_result() {
    let result = result::<TestKey>("http://example.com/path", "https://example.com/path").unwrap();
    assert!(result.arguments().is_empty());
}

#[derive(Clone, PartialEq, Debug)]
enum Stacked {
    Test,
    Simple(SimpleKey),
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_withBackStack
#[test]
fn a_uri_s_match_builds_a_back_stack_of_its_key() {
    let matcher = matcher::<SimpleKey>(USER_QUERY)
        .map_key(Stacked::Simple)
        .with_back_stack(|result| vec![Stacked::Test, result.key().clone()]);
    let result = matcher.r#match(&DeepLinkRequest::of("https://example.com/user?name=john&age=30"));
    assert_eq!(
        result.unwrap().back_stack(),
        [Stacked::Test, Stacked::Simple(SimpleKey { name: "john".into(), age: 30 })]
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_withBackStack_interfaceKeys
#[test]
fn a_uri_s_match_builds_a_back_stack_of_other_keys() {
    let matcher = matcher::<TestKey>("https://example.com/any/")
        .map_key(|_| TestInterface::ImplB)
        .with_back_stack(|_| vec![TestInterface::ImplA, TestInterface::ImplB]);
    let result = matcher.r#match(&DeepLinkRequest::of("https://example.com/any/")).unwrap();
    assert_eq!(result.back_stack(), [TestInterface::ImplA, TestInterface::ImplB]);
    assert_eq!(*result.key(), TestInterface::ImplB);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_polymorphicConcreteArgument
#[test]
fn a_concrete_field_reads_the_flat_arguments() {
    let key = key::<ConcreteArgKey>(
        "https://example.com/path?owner={owner}&name={name}",
        "https://example.com/path?owner=theOwner&name=theName",
    );
    assert_eq!(
        key.unwrap().derived,
        ConcreteKey { owner: "theOwner".into(), name: "theName".into() }
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_polymorphicOpenClassKey
#[test]
fn an_open_class_s_key_is_a_struct() {
    assert_eq!(
        key::<TestArgKey>(
            "https://example.com/path?name={name}",
            "https://example.com/path?name=theName"
        ),
        Some(TestArgKey { name: "theName".into() })
    );
}

#[derive(Clone, PartialEq, Debug)]
struct NotAKey {
    id: i32,
}

struct NotAKeySerializer;

impl DeepLinkSerializer<NotAKey> for NotAKeySerializer {
    fn serial_name(&self) -> &str {
        "NotAKey"
    }

    fn deserialize(&self, value: &str) -> NotAKey {
        NotAKey { id: value.parse().unwrap() }
    }

    fn serialize(&self, value: &NotAKey) -> String {
        value.id.to_string()
    }
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct WithSerializerKey {
    #[deep_link(with = NotAKeySerializer)]
    arg: NotAKey,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct WithOptionalSerializerKey {
    #[deep_link(with = NotAKeySerializer, default = NotAKey { id: 7 })]
    arg: NotAKey,
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_deepLinkSerializer_interfaceImplArgument
#[test]
fn a_field_of_a_serializer_is_read_by_it() {
    assert_eq!(
        key::<WithSerializerKey>(
            "https://example.com/path?arg={arg}",
            "https://example.com/path?arg=13"
        ),
        Some(WithSerializerKey { arg: NotAKey { id: 13 } })
    );
}

/// With no value: its default, or no match.
#[test]
fn a_field_of_a_serializer_can_have_a_default() {
    assert_eq!(
        key::<WithOptionalSerializerKey>(
            "https://example.com/path?arg={arg}",
            "https://example.com/path"
        ),
        Some(WithOptionalSerializerKey { arg: NotAKey { id: 7 } })
    );
    assert!(
        result::<WithSerializerKey>(
            "https://example.com/path?arg={arg}",
            "https://example.com/path"
        )
        .is_none()
    );
}

/// Each form of an unnamed parameter's parts, a filter on a matcher, and what `match_uri` and
/// `match_arguments` leave to a matcher of an app's.
#[test]
fn a_uri_matcher_has_its_filters_and_gives_its_steps() {
    let matcher = matcher::<TestKey>("https://example.com/path")
        .with_filters(vec![mime_type_filter("image/test")]);
    let uri = DeepLinkUri::new("https://example.com/path");
    assert!(matcher.r#match(&DeepLinkRequest::of("https://example.com/path")).is_none());
    assert!(
        matcher
            .r#match(&DeepLinkRequest::new(Some(uri.clone()), mime_type_extra("image/test")))
            .is_some()
    );
    // The steps go without the filters.
    let by_uri = matcher.match_uri(&uri).unwrap();
    assert!(by_uri.is_exact_path());
    let by_arguments =
        matcher.match_arguments(Default::default(), Default::default(), Default::default());
    assert!(by_arguments.unwrap().is_exact_path());
    // A request of extras alone has no URI to match.
    let extras = DeepLinkRequest::new(None, mime_type_extra("image/test"));
    assert!(matcher.r#match(&extras).is_none());
}

/// A result made by hand is the worst of a match.
#[test]
fn a_result_by_hand_ranks_last() {
    let by_hand = UriMatchResult::new(TestKey, Default::default());
    let matched = result::<TestKey>("https://example.com/.*", "https://example.com/a").unwrap();
    assert_eq!(by_hand.compare_to(&matched), Ordering::Less);
}

/// A key's enum given by a pattern: the app's keys of one type from each of their structs.
#[test]
fn a_decoder_can_make_any_key() {
    let matcher = UriDeepLinkMatcher::with_decoder(
        DeepLinkUri::new("https://example.com/{id}"),
        |arguments| Ok(arguments["id"][0].len()),
    );
    let result = matcher.r#match(&DeepLinkRequest::of("https://example.com/abc")).unwrap();
    assert_eq!(*result.key(), 3);
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Sealed {
    Key1,
    Key2,
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkMatcherTest.kt, matchRequest_withBackStack_sealedKeys
#[test]
fn a_uri_s_match_builds_a_back_stack_of_an_enum() {
    let matcher = matcher::<TestKey>("https://example.com/any/")
        .map_key(|_| Sealed::Key2)
        .with_back_stack(|_| vec![Sealed::Key1, Sealed::Key2]);
    let result = matcher.r#match(&DeepLinkRequest::of("https://example.com/any/")).unwrap();
    assert_eq!(result.back_stack(), [Sealed::Key1, Sealed::Key2]);
    assert_eq!(*result.key(), Sealed::Key2);
}

/// The path's arguments go over the query's, and those over the fragment's: required, they're
/// what a key can't be without.
#[test]
fn a_path_s_argument_goes_over_a_query_s() {
    let matcher = matcher::<TestArgKey>("https://example.com/a");
    let of = |value: &str| DeepLinkArguments::from([("name".to_string(), vec![value.to_string()])]);
    let result = matcher.match_arguments(of("path"), of("query"), of("fragment")).unwrap();
    assert_eq!(result.key().name, "path");
    let result = matcher.match_arguments(Default::default(), of("query"), of("fragment")).unwrap();
    assert_eq!(result.key().name, "query");
}
