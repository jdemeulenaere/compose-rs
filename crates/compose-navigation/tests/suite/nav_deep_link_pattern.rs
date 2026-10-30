//! What a pattern takes from a URI, against Navigation 3's `UriDeepLinkParserTest`: a path's, a
//! query's and a fragment's arguments.

use std::collections::HashSet;

use compose_navigation::*;

const BASE: &str = "www.testUri.com";

fn uri(rest: &str) -> DeepLinkUri {
    DeepLinkUri::new(format!("https://{BASE}{rest}"))
}

/// The path's arguments of `request` by `pattern`, both after `https://` and the base.
fn path_args(pattern: &str, request: &str) -> Option<DeepLinkArguments> {
    extract_path_args(&parse_path(&uri(pattern), &mut HashSet::new()), &uri(request))
}

fn query_args(pattern: &str, request: &str) -> DeepLinkArguments {
    extract_query_args(&parse_query(&uri(pattern), &mut HashSet::new()), &uri(request))
}

fn fragment_args(pattern: &str, request: &str) -> DeepLinkArguments {
    let parsed = parse_fragment(&uri(pattern), &mut HashSet::new());
    extract_fragment_args(parsed.as_ref(), &uri(request))
}

fn args<const N: usize>(entries: [(&str, &[&str]); N]) -> DeepLinkArguments {
    entries
        .into_iter()
        .map(|(name, values)| (name.to_string(), values.iter().map(|v| v.to_string()).collect()))
        .collect()
}

/// Of a pattern and a request whose schemes aren't https.
fn path_args_of(pattern: &str, request: &str) -> Option<DeepLinkArguments> {
    extract_path_args(
        &parse_path(&DeepLinkUri::new(pattern), &mut HashSet::new()),
        &DeepLinkUri::new(request),
    )
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_singleParam
#[test]
fn a_placeholder_takes_a_segment() {
    assert_eq!(path_args("/users/{id}", "/users/123"), Some(args([("id", &["123"])])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_singleParamWithMultiplePlaceholders
#[test]
fn placeholders_in_one_segment_split_it() {
    assert_eq!(
        path_args("/users/{first}-{last}", "/users/john-doe"),
        Some(args([("first", &["john"]), ("last", &["doe"])]))
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_wildcard
#[test]
fn a_wildcard_takes_segments_and_gives_nothing() {
    assert_eq!(path_args("/users/.*", "/users/anything/else"), Some(args([])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_mixedLiteralAndPlaceholder
#[test]
fn a_placeholder_can_follow_a_literal() {
    assert_eq!(path_args("/users/user_{id}", "/users/user_123"), Some(args([("id", &["123"])])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPath_emptyString
#[test]
fn a_placeholder_can_be_empty() {
    assert_eq!(path_args("/users/{id}/profile", "/users//profile"), Some(args([("id", &[""])])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPath_trailingEmptyString
#[test]
fn a_last_placeholder_can_be_empty() {
    assert_eq!(path_args("/users/{id}", "/users/"), Some(args([("id", &[""])])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_colonParamDelimiter
#[test]
fn a_placeholder_can_be_in_a_segment_s_parameter() {
    assert_eq!(path_args("/users;id={id}", "/users;id=123"), Some(args([("id", &["123"])])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_encodedCharacters
#[test]
fn a_path_s_argument_is_decoded() {
    assert_eq!(
        path_args("/users/{name}", "/users/john%20doe"),
        Some(args([("name", &["john doe"])]))
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_authorityOnly
#[test]
fn an_authority_alone_matches_itself() {
    assert_eq!(path_args("", ""), Some(args([])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_authorityOnlyTrailingSlash
#[test]
fn an_authority_with_a_slash_matches_itself() {
    assert_eq!(path_args("/", "/"), Some(args([])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_authorityOnlyPatternExtraTrailingSlash
#[test]
fn an_authority_s_slash_is_required_if_the_pattern_has_it() {
    assert_eq!(path_args("/", ""), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_authorityOnlyRequestExtraTrailingSlash
#[test]
fn an_authority_s_slash_is_refused_if_the_pattern_hasn_t_it() {
    assert_eq!(path_args("", "/"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_patternHasExtraTrailingSlash
#[test]
fn a_path_s_slash_is_required_if_the_pattern_has_it() {
    assert_eq!(path_args("/users/", "/users"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_requestHasExtraTrailingSlash
#[test]
fn a_path_s_slash_is_refused_if_the_pattern_hasn_t_it() {
    assert_eq!(path_args("/users", "/users/"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_mismatchedLiteral
#[test]
fn another_literal_doesn_t_match() {
    assert_eq!(path_args("/users", "/posts"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_missingLiteralInRequest
#[test]
fn a_missing_literal_doesn_t_match() {
    assert_eq!(path_args("/users/profile", "/users"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_extraLiteralInRequest
#[test]
fn an_extra_literal_doesn_t_match() {
    assert_eq!(path_args("/users", "/user/extra"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_missingPlaceholderInRequest
#[test]
fn a_missing_placeholder_doesn_t_match() {
    assert_eq!(path_args("/users/{id}", "/users"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_misorderedLiteral
#[test]
fn literals_out_of_order_don_t_match() {
    assert_eq!(path_args("/users/profile", "/profile/users"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_misorderedLiteralAndPlaceholder
#[test]
fn a_placeholder_before_its_literal_doesn_t_match() {
    assert_eq!(path_args("/users/{id}", "/123/users"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_schemeMismatch
#[test]
fn another_scheme_doesn_t_match() {
    let (pattern, request) = (format!("custom://{BASE}/users"), format!("https://{BASE}/users"));
    assert_eq!(path_args_of(&pattern, &request), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_authorityMismatch
#[test]
fn another_authority_doesn_t_match() {
    assert_eq!(path_args_of(&format!("https://{BASE}/users"), "https://wrong.com/users"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_fallbackSchemeMatchesHttp
#[test]
fn no_scheme_matches_http() {
    let (pattern, request) = (format!("{BASE}/users"), format!("http://{BASE}/users"));
    assert_eq!(path_args_of(&pattern, &request), Some(args([])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_fallbackSchemeMatchesHttps
#[test]
fn no_scheme_matches_https() {
    let (pattern, request) = (format!("{BASE}/users"), format!("https://{BASE}/users"));
    assert_eq!(path_args_of(&pattern, &request), Some(args([])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_customSchemeMatches
#[test]
fn a_custom_scheme_matches_itself() {
    let uri = format!("custom://{BASE}/users");
    assert_eq!(path_args_of(&uri, &uri), Some(args([])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_customSchemeMismatch
#[test]
fn a_custom_scheme_doesn_t_match_another() {
    let (pattern, request) = (format!("custom1://{BASE}/users"), format!("custom2://{BASE}/users"));
    assert_eq!(path_args_of(&pattern, &request), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_caseInsensitivity
#[test]
fn a_path_s_literals_ignore_case() {
    assert_eq!(path_args("/Users", "/users"), Some(args([])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_placeholderCannotContainSlash
#[test]
fn a_placeholder_doesn_t_cross_a_slash() {
    assert_eq!(path_args("/users/{id}/profile", "/users/123/456/profile"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_inputContainingEquals
#[test]
fn a_placeholder_can_hold_an_equals_sign() {
    assert_eq!(path_args("/users/{id}", "/users/id=123"), Some(args([("id", &["id=123"])])));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_queryValueContainingSlash
#[test]
fn a_query_s_slash_is_no_path_s() {
    assert_eq!(
        path_args("/users/{id}", "/users/123?redirect=fallback/path"),
        Some(args([("id", &["123"])]))
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractPathArgs_doubleSlashInRequest
#[test]
fn a_double_slash_is_a_segment_more() {
    assert_eq!(path_args("/users/{id}", "/users//123"), None);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_namedParam
#[test]
fn a_named_parameter_gives_its_value() {
    assert_eq!(query_args("?name={name}", "?name=john"), args([("name", &["john"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_namedParamsMultiple
#[test]
fn named_parameters_give_their_values() {
    assert_eq!(
        query_args("?name={name}&age={age}", "?name=john&age=30"),
        args([("name", &["john"]), ("age", &["30"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_differentOrder
#[test]
fn named_parameters_can_come_in_any_order() {
    assert_eq!(
        query_args("?name={name}&age={age}", "?age=30&name=john"),
        args([("name", &["john"]), ("age", &["30"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_unnamedParam
#[test]
fn an_unnamed_parameter_takes_what_has_no_name() {
    assert_eq!(
        query_args("?{rawQuery}", "?anything&else"),
        args([("rawQuery", &["anything", "else"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_unnamedParamEncoded
#[test]
fn an_unnamed_parameter_is_decoded() {
    assert_eq!(query_args("?{rawQuery}", "?hello%20world"), args([("rawQuery", &["hello world"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_mixedNamedAndUnnamedParam
#[test]
fn named_and_unnamed_parameters_mix() {
    assert_eq!(
        query_args("?user={name}&{other}", "?user=john&anything&else"),
        args([("name", &["john"]), ("other", &["anything", "else"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_mixedNamedUnnamedAndExtraParams
#[test]
fn a_parameter_the_pattern_hasn_t_is_ignored() {
    assert_eq!(
        query_args("?user={name}&{other}", "?something&user=john&else&extra=stuff"),
        args([("name", &["john"]), ("other", &["something", "else"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_repeatedParams
#[test]
fn a_repeated_parameter_gives_every_value() {
    assert_eq!(query_args("?list={val}", "?list=10&list=20"), args([("val", &["10", "20"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_repeatedParamsMixedMatching
#[test]
fn a_repeated_parameter_gives_those_that_match() {
    assert_eq!(
        query_args("?type=user_.*", "?type=user_admin&type=guest"),
        args([("type", &["admin"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_wildcard
#[test]
fn a_query_s_wildcard_is_its_parameter_s_argument() {
    assert_eq!(query_args("?type=.*", "?type=user_admin"), args([("type", &["user_admin"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_wildcardSuffix
#[test]
fn a_query_s_wildcard_after_a_literal_is_what_follows() {
    assert_eq!(query_args("?type=user_.*", "?type=user_admin"), args([("type", &["admin"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_wildcardPrefix
#[test]
fn a_query_s_wildcard_before_a_literal_is_what_precedes() {
    assert_eq!(query_args("?type=.*_user", "?type=admin_user"), args([("type", &["admin"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_wildcardMatchesEmptyString
#[test]
fn a_query_s_wildcard_can_be_empty() {
    assert_eq!(query_args("?type=user_.*", "?type=user_"), args([("type", &[""])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_wildcardAndPlaceholder
#[test]
fn a_query_s_wildcard_beside_a_placeholder_gives_nothing() {
    assert_eq!(query_args("?type=user_{id}_.*", "?type=user_123_admin"), args([("id", &["123"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_partialLiteralMatching
#[test]
fn a_query_s_placeholder_can_follow_a_literal() {
    assert_eq!(query_args("?type=user_{id}", "?type=user_123"), args([("id", &["123"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_multiplePlaceholdersInOneParam
#[test]
fn placeholders_in_one_parameter_split_it() {
    assert_eq!(
        query_args("?name={first}_{last}", "?name=john_doe"),
        args([("first", &["john"]), ("last", &["doe"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQuery_noQueryArgsInPattern
#[test]
fn a_pattern_of_no_query_gives_nothing() {
    assert_eq!(query_args("", "?"), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_emptyValueForNamedParam
#[test]
fn a_named_parameter_can_be_empty() {
    assert_eq!(query_args("?name={name}", "?name="), args([("name", &[""])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_encodedCharacters
#[test]
fn a_named_parameter_is_decoded() {
    assert_eq!(
        query_args("?query={query}", "?query=hello%20world"),
        args([("query", &["hello world"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_dashBetweenPlaceholder
#[test]
fn a_dash_can_split_a_parameter() {
    assert_eq!(
        query_args("?user={first}-{last}", "?user=john-doe"),
        args([("first", &["john"]), ("last", &["doe"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_commaSeparatedExtractedAsSingleValue
#[test]
fn commas_don_t_split_a_value() {
    assert_eq!(
        query_args("?colors={colorList}", "?colors=purple,red,green"),
        args([("colorList", &["purple,red,green"])])
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQuery_queryArgsFlag
#[test]
fn a_flag_gives_itself() {
    assert_eq!(query_args("?debug", "?debug"), args([("debug", &["debug"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_flagWithValueInRequestMismatch
#[test]
fn a_flag_with_a_value_isn_t_the_flag() {
    assert_eq!(query_args("?debug", "?debug=true"), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_trailingQuestionMark
#[test]
fn a_request_s_lone_question_mark_gives_nothing() {
    assert_eq!(query_args("/a", "/a?"), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_noParameters
#[test]
fn a_request_s_lone_equals_sign_gives_nothing() {
    assert_eq!(query_args("/a", "/a?="), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testParsePath_duplicatePlaceholderInSameSegmentThrows
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_twice_in_a_segment_panics() {
    parse_path(&uri("/users/{id}_{id}"), &mut HashSet::new());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testParsePath_duplicatePlaceholderAcrossSegmentsThrows
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_in_two_segments_panics() {
    parse_path(&uri("/users/{id}/details/{id}"), &mut HashSet::new());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_multipleUnnamedParamsThrows
#[test]
#[should_panic(expected = "already has unnamed query parameter")]
fn two_unnamed_parameters_panic() {
    parse_query(&uri("?{rawQuery1}&{rawQuery2}"), &mut HashSet::new());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractQueryArgs_duplicateParamNameThrows
#[test]
#[should_panic(expected = "must only be present once")]
fn a_parameter_twice_panics() {
    parse_query(&uri("?name={first}&name={last}"), &mut HashSet::new());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testParseQuery_duplicatePlaceholderInSameParamThrows
#[test]
#[should_panic(expected = "Duplicate placeholder name [first]")]
fn a_name_twice_in_a_parameter_panics() {
    parse_query(&uri("?name={first}_{first}"), &mut HashSet::new());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testParseQuery_duplicatePlaceholderAcrossParamsThrows
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_in_two_parameters_panics() {
    parse_query(&uri("?user={id}&detail={id}"), &mut HashSet::new());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractFragmentArgs_staticFragment
#[test]
fn a_literal_fragment_gives_nothing() {
    assert_eq!(fragment_args("/a#section", "/a#section"), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractFragmentArgs_placeholder
#[test]
fn a_fragment_s_placeholder_gives_its_value() {
    assert_eq!(fragment_args("/a#{id}", "/a#123"), args([("id", &["123"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractFragmentArgs_mixedLiteralAndPlaceholder
#[test]
fn a_fragment_s_placeholder_can_follow_a_literal() {
    assert_eq!(fragment_args("/a#section_{id}", "/a#section_123"), args([("id", &["123"])]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractFragmentArgs_missingLiteralFragmentInRequest
#[test]
fn no_fragment_for_a_literal_gives_nothing() {
    assert_eq!(fragment_args("/a#section", "/a"), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractFragmentArgs_missingFragmentArgInRequest
#[test]
fn no_fragment_for_a_placeholder_gives_nothing() {
    assert_eq!(fragment_args("/a#{id}", "/a"), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractFragmentArgs_mismatchedFragment
#[test]
fn another_fragment_gives_nothing() {
    assert_eq!(fragment_args("/a#section1", "/a#section2"), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testExtractFragmentArgs_extraFragmentInPattern
#[test]
fn a_fragment_the_pattern_hasn_t_gives_nothing() {
    assert_eq!(fragment_args("/a", "/a#123"), args([]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/UriDeepLinkParserTest.kt, testParseFragment_duplicatePlaceholderThrows
#[test]
#[should_panic(expected = "Duplicate placeholder name [id]")]
fn a_name_twice_in_a_fragment_panics() {
    parse_fragment(&uri("/users#section_{id}_{id}"), &mut HashSet::new());
}

/// What a URI says of itself, as `android.net.Uri` does.
#[test]
fn a_uri_says_what_it_s_made_of() {
    let uri =
        DeepLinkUri::new("https://bob@Example.com:80/a%20b//c?x=1&y=%2F&x=2&flag#frag%20ment");
    assert_eq!(uri.scheme(), Some("https"));
    assert_eq!(uri.authority().as_deref(), Some("bob@Example.com:80"));
    assert_eq!(uri.path_segments(), ["a b", "c"]);
    assert_eq!(uri.query().as_deref(), Some("x=1&y=/&x=2&flag"));
    assert_eq!(uri.fragment().as_deref(), Some("frag ment"));
    assert_eq!(uri.query_parameters("x"), ["1", "2"]);
    assert_eq!(uri.query_parameters("y"), ["/"]);
    assert_eq!(uri.query_parameters("flag"), [""]);
    assert!(uri.query_parameters("none").is_empty());
    assert_eq!(uri.query_parameter_names(), ["x", "y", "flag"]);
    assert!(uri.is_hierarchical());
    let relative = DeepLinkUri::new("example.com/a?b");
    assert_eq!(relative.scheme(), None);
    assert_eq!(relative.authority(), None);
    assert_eq!(relative.path_segments(), ["example.com", "a"]);
    let network = DeepLinkUri::new("//example.com?b#c");
    assert_eq!(network.authority().as_deref(), Some("example.com"));
    assert!(network.path_segments().is_empty());
    assert_eq!(network.query().as_deref(), Some("b"));
    let opaque = DeepLinkUri::new("mailto:bob@example.com");
    assert!(!opaque.is_hierarchical());
    assert_eq!(opaque.scheme(), Some("mailto"));
    assert!(opaque.path_segments().is_empty());
    let fragment_first = DeepLinkUri::new("https://a.com/b#c?d");
    assert_eq!(fragment_first.query(), None);
    assert_eq!(fragment_first.fragment().as_deref(), Some("c?d"));
}

/// An escape that isn't one is U+FFFD, and what's not UTF-8 too.
#[test]
fn a_bad_escape_decodes_to_the_replacement_character() {
    let segments = |text: &str| DeepLinkUri::new(format!("https://a.com/{text}")).path_segments();
    assert_eq!(segments("%"), ["\u{fffd}"]);
    assert_eq!(segments("%4"), ["\u{fffd}"]);
    assert_eq!(segments("%FF"), ["\u{fffd}"]);
    assert_eq!(segments("%E2%82%AC"), ["€"]);
    assert_eq!(segments("a+b"), ["a+b"]);
}

/// A wildcard is as long as it can be, and a query's placeholder as short: the regexes' greed and
/// laziness.
#[test]
fn a_wildcard_takes_all_it_can_and_a_placeholder_what_it_must() {
    assert_eq!(query_args("?type=.*_x", "?type=a_x_x"), args([("type", &["a_x"])]));
    assert_eq!(query_args("?type=.*_{id}", "?type=a_b_c"), args([("id", &["c"])]));
    assert_eq!(
        query_args("?name={first}_{last}", "?name=a_b_c"),
        args([("first", &["a"]), ("last", &["b_c"])])
    );
}

/// `{}` names nothing: it's a literal, as in the regex a name of one character at least.
#[test]
fn empty_braces_are_a_literal() {
    assert_eq!(path_args("/users/{}", "/users/{}"), Some(args([])));
    assert_eq!(path_args("/users/{}", "/users/1"), None);
}

/// A fragment's argument is decoded by the pattern after the URI has: twice, as there.
#[test]
fn a_fragment_s_argument_is_decoded_again() {
    assert_eq!(fragment_args("/a#{id}", "/a#x%2520y"), args([("id", &["x y"])]));
}
