//! What `UriDeepLinkMatcher.kt`'s `UriPatternParser` and `UriRequestParser` do with regular
//! expressions, by a matcher of the few forms they use: a literal, a `{placeholder}` and a `.*`.
//! Their regexes' laziness and greed are kept, which is which placeholder takes what.

use std::collections::HashSet;

use compose_runtime::__private::DeepLinkArguments;

use super::uri::{DeepLinkUri, decode};

/// One piece of a pattern, with the regex it stands for.
#[derive(Clone, PartialEq, Debug)]
enum Token {
    /// A literal. A path's and a fragment's are matched whatever their case, a query's as is.
    Literal(String),
    /// `http[s]?`.
    HttpOrHttps,
    /// `([^/]*?|)`: a path's or a fragment's placeholder, captured, as short as it can be.
    Placeholder,
    /// `([\s\S]+?)?`: a query's placeholder, captured, as short as it can be but not empty unless
    /// nothing else matches, when it's absent.
    QueryPlaceholder,
    /// `.*`: a wildcard, as long as it can be.
    Wildcard,
    /// `(.*)`: a wildcard, captured.
    CapturedWildcard,
    /// A literal captured: a query's flag.
    CapturedLiteral(String),
    /// `($|(\?(.)*)|(#(.)*))`: the end, or a query or fragment going on to it.
    EndOfPath,
}

/// Java's line terminators, which `.` doesn't match.
fn is_line_terminator(character: char) -> bool {
    matches!(character, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// A capture: the byte range of the text it took, or nothing if it wasn't taken. Strings are made
/// on a match, as every end of a placeholder is a candidate.
type Capture = Option<(usize, usize)>;

/// The captures of `tokens` matching all of `text`: `None` if they don't, and an absent capture
/// where one wasn't taken.
fn match_entire(tokens: &[Token], text: &str, ignore_case: bool) -> Option<Vec<Option<String>>> {
    let mut captures = Vec::new();
    if !match_from(tokens, text, 0, ignore_case, &mut captures) {
        return None;
    }
    Some(
        captures
            .into_iter()
            .map(|capture| capture.map(|(start, end)| text[start..end].to_string()))
            .collect(),
    )
}

fn literal_length(literal: &str, text: &str, ignore_case: bool) -> Option<usize> {
    if !ignore_case {
        return text.starts_with(literal).then_some(literal.len());
    }
    let mut rest = text.char_indices();
    let mut length = 0;
    for wanted in literal.chars() {
        let (at, found) = rest.next()?;
        if !wanted.to_lowercase().eq(found.to_lowercase()) {
            return None;
        }
        length = at + found.len_utf8();
    }
    Some(length)
}

/// `rest` from `end`, with what's from `start` to it as the next capture.
fn with_capture(
    rest: &[Token],
    text: &str,
    capture: Capture,
    end: usize,
    ignore_case: bool,
    captures: &mut Vec<Capture>,
) -> bool {
    captures.push(capture);
    if match_from(rest, text, end, ignore_case, captures) {
        return true;
    }
    captures.pop();
    false
}

fn match_from(
    tokens: &[Token],
    text: &str,
    at: usize,
    ignore_case: bool,
    captures: &mut Vec<Capture>,
) -> bool {
    let Some((token, rest)) = tokens.split_first() else { return at == text.len() };
    let from = &text[at..];
    match token {
        Token::Literal(literal) => match literal_length(literal, from, ignore_case) {
            Some(length) => match_from(rest, text, at + length, ignore_case, captures),
            None => false,
        },
        Token::CapturedLiteral(literal) => match literal_length(literal, from, ignore_case) {
            Some(length) => {
                let end = at + length;
                with_capture(rest, text, Some((at, end)), end, ignore_case, captures)
            }
            None => false,
        },
        Token::HttpOrHttps => {
            let Some(length) = literal_length("http", from, ignore_case) else { return false };
            let after = at + length;
            if let Some(extra) = literal_length("s", &text[after..], ignore_case)
                && match_from(rest, text, after + extra, ignore_case, captures)
            {
                return true;
            }
            match_from(rest, text, after, ignore_case, captures)
        }
        Token::Placeholder => {
            for (offset, character) in from.char_indices() {
                let end = at + offset;
                if with_capture(rest, text, Some((at, end)), end, ignore_case, captures) {
                    return true;
                }
                if character == '/' {
                    return false;
                }
            }
            with_capture(rest, text, Some((at, text.len())), text.len(), ignore_case, captures)
        }
        Token::QueryPlaceholder => {
            let ends = from.char_indices().map(|(offset, _)| at + offset).skip(1);
            if !from.is_empty() {
                for end in ends.chain([text.len()]) {
                    if with_capture(rest, text, Some((at, end)), end, ignore_case, captures) {
                        return true;
                    }
                }
            }
            with_capture(rest, text, None, at, ignore_case, captures)
        }
        Token::Wildcard | Token::CapturedWildcard => {
            let line = from.find(is_line_terminator).unwrap_or(from.len());
            let ends =
                std::iter::once(line).chain(from[..line].char_indices().rev().map(|(at, _)| at));
            for offset in ends {
                let end = at + offset;
                let matched = if *token == Token::CapturedWildcard {
                    with_capture(rest, text, Some((at, end)), end, ignore_case, captures)
                } else {
                    match_from(rest, text, end, ignore_case, captures)
                };
                if matched {
                    return true;
                }
            }
            false
        }
        Token::EndOfPath => {
            let goes_on = from.is_empty()
                || ((from.starts_with('?') || from.starts_with('#'))
                    && !from.contains(is_line_terminator));
            goes_on && rest.is_empty()
        }
    }
}

/// A placeholder of `text` from `from` on: where its `{` is, where it ends after its `}`, and its
/// name. As `\{(.+?)\}`: at least a character, and the first `}` after it.
fn find_placeholder(text: &str, from: usize) -> Option<(usize, usize, &str)> {
    let mut search = from;
    while let Some(open) = text.get(search..)?.find('{').map(|at| search + at) {
        let name_start = open + 1;
        let first = text[name_start..].chars().next();
        if let Some(first) = first.filter(|first| !is_line_terminator(*first)) {
            let after_first = name_start + first.len_utf8();
            let line_end = text[name_start..]
                .find(is_line_terminator)
                .map_or(text.len(), |at| name_start + at);
            if let Some(close) = text[after_first..line_end].find('}').map(|at| after_first + at) {
                return Some((open, close + 1, &text[name_start..close]));
            }
        }
        search = name_start;
    }
    None
}

/// Whether all of `text` is a placeholder, as `FILL_IN_PATTERN.matches`: its name may hold braces.
fn is_placeholder(text: &str) -> bool {
    text.len() >= 3
        && text.starts_with('{')
        && text.ends_with('}')
        && !text.contains(is_line_terminator)
}

/// `literal`, its `.*`s as wildcards, as `saveWildcardInRegex`.
fn push_literal(tokens: &mut Vec<Token>, literal: &str) {
    let mut parts = literal.split(".*");
    if let Some(first) = parts.next()
        && !first.is_empty()
    {
        tokens.push(Token::Literal(first.to_string()));
    }
    for part in parts {
        tokens.push(Token::Wildcard);
        if !part.is_empty() {
            tokens.push(Token::Literal(part.to_string()));
        }
    }
}

/// A segment's or a fragment's tokens, as `buildRegex`.
fn build_tokens(
    uri_pattern: &DeepLinkUri,
    segment: &str,
    args: &mut Vec<String>,
    tokens: &mut Vec<Token>,
    taken_arg_names: &mut HashSet<String>,
) {
    let mut append_at = 0;
    while let Some((open, end, name)) = find_placeholder(segment, append_at) {
        assert!(
            taken_arg_names.insert(name.to_string()),
            "Duplicate placeholder name [{name}] found in uri pattern [{uri_pattern}]."
        );
        args.push(name.to_string());
        push_literal(tokens, &segment[append_at..open]);
        tokens.push(Token::Placeholder);
        append_at = end;
    }
    push_literal(tokens, &segment[append_at..]);
}

/// A pattern's scheme, authority and path, and its arguments in order.
#[derive(Clone, Debug)]
pub struct ParsedPath {
    scheme: SchemePattern,
    tokens: Vec<Token>,
    args: Vec<String>,
}

/// What a pattern asks of a request's scheme.
#[derive(Clone, PartialEq, Debug)]
enum SchemePattern {
    AnyHttp,
    Exact(String),
}

impl ParsedPath {
    /// Whether `scheme` is the pattern's, whatever its case.
    pub(crate) fn matches_scheme(&self, scheme: &str) -> bool {
        match &self.scheme {
            SchemePattern::AnyHttp => {
                scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
            }
            SchemePattern::Exact(exact) => {
                let mut tokens = Vec::new();
                push_literal(&mut tokens, exact);
                match_entire(&tokens, scheme, true).is_some()
            }
        }
    }
}

/// Parses `uri_pattern`'s path, with its scheme and authority, as `UriPatternParser.parsePath`:
/// panics on a placeholder's name that's taken.
pub fn parse_path(uri_pattern: &DeepLinkUri, taken_arg_names: &mut HashSet<String>) -> ParsedPath {
    let segments: Vec<String> =
        uri_pattern.path_segments().into_iter().filter(|segment| !segment.is_empty()).collect();
    let mut tokens = Vec::new();
    let scheme = match uri_pattern.scheme() {
        None => {
            tokens.push(Token::HttpOrHttps);
            tokens.push(Token::Literal("://".to_string()));
            SchemePattern::AnyHttp
        }
        Some(scheme) if scheme.eq_ignore_ascii_case("https") => {
            tokens.push(Token::Literal("https://".to_string()));
            SchemePattern::Exact("https".to_string())
        }
        Some(scheme) if scheme.eq_ignore_ascii_case("http") => {
            tokens.push(Token::HttpOrHttps);
            tokens.push(Token::Literal("://".to_string()));
            SchemePattern::AnyHttp
        }
        Some(scheme) => {
            push_literal(&mut tokens, scheme);
            tokens.push(Token::Literal("://".to_string()));
            SchemePattern::Exact(scheme.to_string())
        }
    };
    if let Some(authority) = uri_pattern.authority() {
        push_literal(&mut tokens, &authority);
        if !segments.is_empty() {
            tokens.push(Token::Literal("/".to_string()));
        }
    }
    let mut args = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        build_tokens(uri_pattern, segment, &mut args, &mut tokens, taken_arg_names);
        if index + 1 != segments.len() {
            tokens.push(Token::Literal("/".to_string()));
        }
    }
    let text = uri_pattern.to_string();
    let before_query = text.split(['?', '#']).next().unwrap_or("");
    if before_query.ends_with('/') {
        tokens.push(Token::Literal("/".to_string()));
    }
    tokens.push(Token::EndOfPath);
    ParsedPath { scheme, tokens, args }
}

/// One query parameter of a pattern, as `QueryParamPattern`.
#[derive(Clone, Debug)]
struct QueryParamPattern {
    tokens: Vec<Token>,
    arguments: Vec<String>,
    is_unnamed: bool,
}

/// A pattern's query parameters by name, in order.
#[derive(Clone, Debug, Default)]
pub struct ParsedQuery {
    params: Vec<(String, QueryParamPattern)>,
}

/// Parses `uri_pattern`'s query, as `UriPatternParser.parseQuery`: panics on a parameter that's
/// there twice, a second unnamed one, or a placeholder's name that's taken.
pub fn parse_query(
    uri_pattern: &DeepLinkUri,
    taken_arg_names: &mut HashSet<String>,
) -> ParsedQuery {
    let mut parsed = ParsedQuery::default();
    let mut unnamed_param: Option<String> = None;
    for param_name in uri_pattern.query_parameter_names() {
        let query_params = uri_pattern.query_parameters(&param_name);
        assert!(
            query_params.len() <= 1,
            "Query parameter [{param_name}] must only be present once in the pattern \
             [{uri_pattern}]."
        );
        let first_param = query_params.into_iter().next().unwrap_or_default();
        let is_arg = is_placeholder(&param_name);
        let is_flag = first_param.is_empty();
        if is_arg {
            if let Some(unnamed) = &unnamed_param {
                panic!(
                    "DeepLink pattern [{uri_pattern}] already has unnamed query parameter \
                     {unnamed}. Cannot add unnamed query parameter {param_name}."
                );
            }
            unnamed_param = Some(param_name.clone());
        }
        let query_param = if is_arg || is_flag { param_name.clone() } else { first_param };
        let mut tokens = Vec::new();
        let mut arguments = Vec::new();
        let mut append_at = 0;
        while let Some((open, end, name)) = find_placeholder(&query_param, append_at) {
            push_literal(&mut tokens, &query_param[append_at..open]);
            tokens.push(Token::QueryPlaceholder);
            arguments.push(name.to_string());
            append_at = end;
        }
        push_literal(&mut tokens, &query_param[append_at..]);
        // A wildcard or a flag is an argument of the parameter's name, if nothing else is.
        if arguments.is_empty() {
            if tokens.contains(&Token::Wildcard) {
                for token in &mut tokens {
                    if *token == Token::Wildcard {
                        *token = Token::CapturedWildcard;
                    }
                }
                arguments.push(param_name.clone());
            } else if is_flag {
                tokens = vec![Token::CapturedLiteral(query_param.clone())];
                arguments.push(param_name.clone());
            }
        }
        for name in &arguments {
            assert!(
                taken_arg_names.insert(name.clone()),
                "Duplicate placeholder name [{name}] found in query of [{uri_pattern}]."
            );
        }
        parsed.params.push((
            param_name,
            QueryParamPattern { tokens, arguments, is_unnamed: is_arg || is_flag },
        ));
    }
    parsed
}

/// A pattern's fragment, and its arguments in order.
#[derive(Clone, Debug)]
pub struct ParsedFragment {
    tokens: Vec<Token>,
    args: Vec<String>,
}

/// Parses `uri_pattern`'s fragment, as `UriPatternParser.parseFragment`: panics on a placeholder's
/// name that's taken.
pub fn parse_fragment(
    uri_pattern: &DeepLinkUri,
    taken_arg_names: &mut HashSet<String>,
) -> Option<ParsedFragment> {
    let fragment = uri_pattern.fragment()?;
    let (mut tokens, mut args) = (Vec::new(), Vec::new());
    build_tokens(uri_pattern, &fragment, &mut args, &mut tokens, taken_arg_names);
    Some(ParsedFragment { tokens, args })
}

/// The path's arguments of `requested_uri`, as `UriRequestParser.extractPathArgs`: `None` if it
/// doesn't match, empty if it does and has none.
pub fn extract_path_args(
    parsed_path: &ParsedPath,
    requested_uri: &DeepLinkUri,
) -> Option<DeepLinkArguments> {
    let captures = match_entire(&parsed_path.tokens, &requested_uri.to_string(), true)?;
    Some(
        parsed_path
            .args
            .iter()
            .zip(captures)
            .map(|(name, value)| {
                (name.clone(), vec![value.map_or_else(String::new, |value| decode(&value))])
            })
            .collect(),
    )
}

/// The query's arguments of `requested_uri`, as `UriRequestParser.extractQueryArgs`: of the
/// parameters the pattern has, whichever match.
pub fn extract_query_args(
    parsed_query: &ParsedQuery,
    requested_uri: &DeepLinkUri,
) -> DeepLinkArguments {
    let query = requested_uri.query();
    let input_params: Vec<&str> =
        query.as_deref().map_or_else(Vec::new, |q| q.split('&').collect());
    let named: HashSet<&str> = parsed_query
        .params
        .iter()
        .filter(|(_, pattern)| !pattern.is_unnamed)
        .map(|(name, _)| name.as_str())
        .collect();
    let mut arguments = DeepLinkArguments::new();
    for (param_name, pattern) in &parsed_query.params {
        let values = if pattern.is_unnamed {
            input_params
                .iter()
                .filter(|part| !part.contains('=') && !named.contains(**part))
                .map(|part| part.to_string())
                .collect()
        } else {
            requested_uri.query_parameters(param_name)
        };
        for value in values {
            if let Some(captures) = match_entire(&pattern.tokens, &value, false) {
                for (name, capture) in pattern.arguments.iter().zip(captures) {
                    arguments.entry(name.clone()).or_default().push(capture.unwrap_or_default());
                }
            }
        }
    }
    arguments
}

/// The fragment's arguments of `requested_uri`, as `UriRequestParser.extractFragmentArgs`: none if
/// it doesn't match.
pub fn extract_fragment_args(
    parsed_fragment: Option<&ParsedFragment>,
    requested_uri: &DeepLinkUri,
) -> DeepLinkArguments {
    let (Some(parsed), Some(fragment)) = (parsed_fragment, requested_uri.fragment()) else {
        return DeepLinkArguments::new();
    };
    let Some(captures) = match_entire(&parsed.tokens, &fragment, true) else {
        return DeepLinkArguments::new();
    };
    parsed
        .args
        .iter()
        .zip(captures)
        .map(|(name, value)| {
            (name.clone(), vec![value.map_or_else(String::new, |value| decode(&value))])
        })
        .collect()
}
