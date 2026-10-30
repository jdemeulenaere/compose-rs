//! `DeepLinkUri.kt`: as Android's `android.net.Uri`, which it is there, and its `UriCodec`.

use std::collections::BTreeSet;
use std::fmt;

/// A URI, parsed as it's asked, as `android.net.Uri`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeepLinkUri {
    uri: String,
}

impl DeepLinkUri {
    /// `uri`, an encoded URI string, as `DeepLinkUri(uriString)`.
    pub fn new(uri: impl Into<String>) -> DeepLinkUri {
        DeepLinkUri { uri: uri.into() }
    }

    /// Where the scheme ends, if there's a `:`.
    fn scheme_separator(&self) -> Option<usize> {
        self.uri.find(':')
    }

    /// Where the scheme's part ends: its separator's next byte.
    fn after_scheme(&self) -> usize {
        self.scheme_separator().map_or(0, |separator| separator + 1)
    }

    /// The scheme: `http`.
    pub fn scheme(&self) -> Option<&str> {
        self.scheme_separator().map(|separator| &self.uri[..separator])
    }

    /// Whether it's like `http://google.com`: a relative URI is, and an absolute one if what
    /// follows the scheme starts with a `/`.
    pub fn is_hierarchical(&self) -> bool {
        match self.scheme_separator() {
            None => true,
            Some(separator) => self.uri.as_bytes().get(separator + 1) == Some(&b'/'),
        }
    }

    /// The decoded authority, after `//`: `bob@google.com:80`.
    pub fn authority(&self) -> Option<String> {
        let rest = self.uri.get(self.after_scheme()..)?.strip_prefix("//")?;
        let end = rest.find(['/', '\\', '?', '#']).unwrap_or(rest.len());
        Some(decode(&rest[..end]))
    }

    /// The decoded path's segments: an opaque URI has none, and an empty segment isn't one.
    pub fn path_segments(&self) -> Vec<String> {
        if !self.is_hierarchical() {
            return Vec::new();
        }
        self.encoded_path().split('/').filter(|segment| !segment.is_empty()).map(decode).collect()
    }

    /// As `parsePath`: what's after the authority and before the query and the fragment.
    fn encoded_path(&self) -> &str {
        let start = self.after_scheme();
        let rest = &self.uri[start..];
        let path = match rest.strip_prefix("//") {
            Some(after) => match after.find(['/', '\\', '?', '#']) {
                Some(at) if matches!(after.as_bytes()[at], b'?' | b'#') => return "",
                Some(at) => &after[at..],
                None => return "",
            },
            None => rest,
        };
        let end = path.find(['?', '#']).unwrap_or(path.len());
        &path[..end]
    }

    fn encoded_query(&self) -> Option<&str> {
        let rest = &self.uri[self.after_scheme()..];
        let start = rest.find('?')?;
        if rest.find('#').is_some_and(|fragment| fragment < start) {
            return None;
        }
        let query = &rest[start + 1..];
        let end = query.find('#').unwrap_or(query.len());
        Some(&query[..end])
    }

    /// The decoded query, after `?` and before `#`.
    pub fn query(&self) -> Option<String> {
        self.encoded_query().map(decode)
    }

    /// The decoded fragment, after `#`.
    pub fn fragment(&self) -> Option<String> {
        let rest = &self.uri[self.after_scheme()..];
        rest.find('#').map(|start| decode(&rest[start + 1..]))
    }

    /// The query's parts, as `name` and what follows its `=`, if it has one.
    fn query_parts(&self) -> Vec<(&str, Option<&str>)> {
        assert!(self.is_hierarchical(), "This isn't a hierarchical URI.");
        let Some(query) = self.encoded_query() else { return Vec::new() };
        let mut parts: Vec<&str> = query.split('&').collect();
        // Android's walk stops at the end: a last `&` starts nothing.
        if parts.len() > 1 && parts.last() == Some(&"") {
            parts.pop();
        }
        parts
            .into_iter()
            .map(|part| match part.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (part, None),
            })
            .collect()
    }

    /// The decoded values of the parameter `key`, in order: an empty one for a name with no `=`.
    pub fn query_parameters(&self, key: &str) -> Vec<String> {
        let encoded = encode(key);
        self.query_parts()
            .into_iter()
            .filter(|(name, _)| *name == encoded)
            .map(|(_, value)| value.map_or_else(String::new, decode))
            .collect()
    }

    /// The decoded names of the parameters, each once, by their first time.
    pub fn query_parameter_names(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.query_parts()
            .into_iter()
            .map(|(name, _)| decode(name))
            .filter(|name| seen.insert(name.clone()))
            .collect()
    }
}

impl fmt::Display for DeepLinkUri {
    /// The encoded string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.uri)
    }
}

/// `s` with what isn't a letter, a number or one of `_-!.~'()*` as `%`-escaped UTF-8.
pub(crate) fn encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    let mut buffer = [0; 4];
    for character in s.chars() {
        if character.is_ascii_alphanumeric() || "_-!.~'()*".contains(character) {
            encoded.push(character);
        } else {
            for byte in character.encode_utf8(&mut buffer).bytes() {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    encoded
}

/// `s` with its `%`-escaped octets decoded as UTF-8, what's invalid as U+FFFD, as `UriCodec.decode`
/// with neither `convertPlus` nor `throwOnFailure`.
pub(crate) fn decode(s: &str) -> String {
    let mut decoded = String::with_capacity(s.len());
    let mut escaped: Vec<u8> = Vec::new();
    let flush = |escaped: &mut Vec<u8>, decoded: &mut String| {
        if !escaped.is_empty() {
            decoded.push_str(&String::from_utf8_lossy(escaped));
            escaped.clear();
        }
    };
    let mut characters = s.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            flush(&mut escaped, &mut decoded);
            decoded.push(character);
            continue;
        }
        let mut value = 0u8;
        for _ in 0..2 {
            let Some(digit) = characters.next() else {
                flush(&mut escaped, &mut decoded);
                decoded.push(char::REPLACEMENT_CHARACTER);
                return decoded;
            };
            let Some(digit) = digit.to_digit(16) else {
                flush(&mut escaped, &mut decoded);
                decoded.push(char::REPLACEMENT_CHARACTER);
                break;
            };
            value = value.wrapping_mul(16).wrapping_add(digit as u8);
        }
        escaped.push(value);
    }
    flush(&mut escaped, &mut decoded);
    decoded
}
