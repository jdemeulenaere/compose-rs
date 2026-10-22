//! Platform services content reaches through composition locals, as Compose's: opening links
//! (`LOCAL_URI_HANDLER`) and haptic feedback (`LOCAL_HAPTIC_FEEDBACK`); the clipboard's is in
//! `clipboard.rs`. The host sets the platform's on its tree (`UiTree::set_uri_handler`,
//! `set_haptic_feedback`) before composing; the locals' defaults read the composing tree's, so
//! a subtree (a test) can provide fakes as in Compose.
//!
//! Links go through [`PlatformUriHandler`], which opens only allowed schemes (`http`,
//! `https`, `mailto`, `tel` unless widened) of well-formed URIs, so a link from untrusted
//! content can't run a program through the OS's opener (D283).

use std::rc::Rc;

use compose_runtime::CompositionLocal;

use crate::tree::UiTree;

/// Opens links, as Compose's `UriHandler`: the platform's browser (Android starts an
/// `ACTION_VIEW` intent; desktop asks the OS: `open`, `xdg-open`, Windows' URL handler),
/// through [`PlatformUriHandler`]'s checks.
pub trait UriHandler {
    /// Opens `uri`. Failures (nothing handles it) are logged, not reported.
    fn open_uri(&self, uri: &str);
}

/// The kinds of haptic feedback, as Compose's `HapticFeedbackType`. Android maps each to its
/// `HapticFeedbackConstants` (with Compose's fallbacks before the API level that has it).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum HapticFeedbackType {
    /// A long press (`combined_clickable`'s long click).
    LongPress,
    /// A text selection handle moved to another character.
    TextHandleMove,
    Confirm,
    ContextClick,
    GestureEnd,
    GestureThresholdActivate,
    KeyboardTap,
    Reject,
    SegmentFrequentTick,
    SegmentTick,
    ToggleOff,
    ToggleOn,
    VirtualKey,
}

/// Performs haptic feedback, as Compose's `HapticFeedback`: Android's view vibrates
/// (`View.performHapticFeedback`); desktop does nothing.
pub trait HapticFeedback {
    fn perform_haptic_feedback(&self, feedback_type: HapticFeedbackType);
}

/// The schemes [`PlatformUriHandler`] opens unless widened: web pages, mail and calls.
pub const DEFAULT_URI_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];

/// The platform's link opener behind a check, as the host gives it: a URI is opened only if
/// it is well formed (a scheme of at least two letters, digits, `+`, `-` or `.`, starting with
/// a letter; no whitespace, control characters, quotes or backslashes) and its scheme is
/// allowed ([`DEFAULT_URI_SCHEMES`]); anything else is refused with a warning naming only its
/// scheme and host. Validate URIs from untrusted sources (a server, a message) before opening
/// them anyway: an allowed scheme can still lead somewhere unwanted.
///
/// Widen (or narrow) the schemes for a subtree by providing the platform's with others:
///
/// ```ignore
/// let handler = PlatformUriHandler::current().with_schemes(&["https", "myapp"]);
/// CompositionLocalProvider(LOCAL_URI_HANDLER.provides(Rc::new(handler)), || Content());
/// ```
#[derive(Clone)]
pub struct PlatformUriHandler {
    opener: Rc<dyn Fn(&str)>,
    schemes: Rc<[String]>,
}

impl PlatformUriHandler {
    /// The host's: `opener` opens what passes the check, with the default schemes.
    pub fn new(opener: impl Fn(&str) + 'static) -> PlatformUriHandler {
        PlatformUriHandler {
            opener: Rc::new(opener),
            schemes: DEFAULT_URI_SCHEMES.iter().map(|scheme| scheme.to_string()).collect(),
        }
    }

    /// The composing tree's (call it in composition), or one that logs what it would open
    /// outside a tree's composition or without a platform.
    pub fn current() -> PlatformUriHandler {
        of_composing_tree(|tree| tree.platform_uri_handler.clone())
            .unwrap_or_else(PlatformUriHandler::headless)
    }

    /// The same opener, opening `schemes` (compared ignoring case) instead.
    pub fn with_schemes(&self, schemes: &[&str]) -> PlatformUriHandler {
        PlatformUriHandler {
            opener: self.opener.clone(),
            schemes: schemes.iter().map(|scheme| scheme.to_ascii_lowercase()).collect(),
        }
    }

    pub fn schemes(&self) -> &[String] {
        &self.schemes
    }

    /// Without a platform (headless, tests): opening a link logs it.
    pub(crate) fn headless() -> PlatformUriHandler {
        PlatformUriHandler::new(|uri| {
            log::info!("no platform to open {} (no UriHandler set)", uri_for_log(uri));
        })
    }
}

impl UriHandler for PlatformUriHandler {
    fn open_uri(&self, uri: &str) {
        match check_uri(uri, &self.schemes) {
            Ok(()) => (self.opener)(uri),
            Err(reason) => log::warn!("not opening {}: {reason}", uri_for_log(uri)),
        }
    }
}

/// Whether `uri` is well formed and of one of `schemes`: `Err` says why not. What
/// [`PlatformUriHandler`] checks.
pub fn check_uri(uri: &str, schemes: &[impl AsRef<str>]) -> Result<(), &'static str> {
    let Some((scheme, _)) = uri.split_once(':') else {
        return Err("no scheme");
    };
    let well_formed = scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !well_formed {
        // A Windows drive path (`C:\…`) has a one-letter "scheme".
        return Err("not a URI scheme");
    }
    if uri.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '\\')) {
        return Err("whitespace, control characters, quotes or backslashes");
    }
    if !schemes.iter().any(|allowed| allowed.as_ref().eq_ignore_ascii_case(scheme)) {
        return Err("scheme not allowed (PlatformUriHandler::with_schemes widens them)");
    }
    Ok(())
}

/// What a log says of `uri`: its scheme and host (`https://example.com/…`), never its path,
/// query or user information.
pub fn uri_for_log(uri: &str) -> String {
    let clean = |text: &str, extra: &[char]| -> String {
        text.chars().filter(|c| c.is_ascii_alphanumeric() || extra.contains(c)).take(64).collect()
    };
    let Some((scheme, rest)) = uri.split_once(':') else {
        return "a string without a scheme".to_string();
    };
    let scheme = clean(scheme, &['+', '-', '.']);
    match rest.strip_prefix("//") {
        Some(authority) => {
            let authority = authority.split(['/', '?', '#', '\\']).next().unwrap_or_default();
            let host = authority.rsplit('@').next().unwrap_or_default();
            format!("{scheme}://{}/…", clean(host, &['.', '-', ':', '[', ']']))
        }
        None => format!("{scheme}:…"),
    }
}

/// What a tree has without a platform's, and desktop's: nothing happens.
pub(crate) struct NoHapticFeedback;

impl HapticFeedback for NoHapticFeedback {
    fn perform_haptic_feedback(&self, _feedback_type: HapticFeedbackType) {}
}

/// Opens links, as Compose's `LocalUriHandler`: the host's ([`PlatformUriHandler`]), or one
/// that logs the link outside a tree's composition or without a platform.
pub static LOCAL_URI_HANDLER: CompositionLocal<Rc<dyn UriHandler>> =
    CompositionLocal::new("LocalUriHandler", || {
        of_composing_tree(|tree| tree.uri_handler.clone())
            .unwrap_or_else(|| Rc::new(PlatformUriHandler::headless()))
    });

/// Haptic feedback, as Compose's `LocalHapticFeedback`: the host's
/// (`UiTree::set_haptic_feedback`), or none outside a tree's composition or without a platform.
pub static LOCAL_HAPTIC_FEEDBACK: CompositionLocal<Rc<dyn HapticFeedback>> =
    CompositionLocal::new("LocalHapticFeedback", || {
        of_composing_tree(|tree| tree.haptic_feedback.clone())
            .unwrap_or_else(|| Rc::new(NoHapticFeedback))
    });

/// `f` of the tree composing now: `None` outside a `UiTree`'s composition.
pub(crate) fn of_composing_tree<R>(f: impl FnOnce(&mut UiTree) -> R) -> Option<R> {
    if !compose_runtime::is_composing() {
        return None;
    }
    let composer = compose_runtime::platform::current();
    if !composer.applier_is::<UiTree>() {
        return None;
    }
    Some(composer.with_applier(f))
}

/// The haptic feedback for a modifier made now: `LOCAL_HAPTIC_FEEDBACK`'s while composing,
/// none otherwise (a modifier made outside composition).
pub(crate) fn current_haptic_feedback() -> Option<Rc<dyn HapticFeedback>> {
    compose_runtime::is_composing().then(|| LOCAL_HAPTIC_FEEDBACK.current())
}

impl UiTree {
    /// Sets the platform's link opener: `LOCAL_URI_HANDLER`'s value and
    /// `PlatformUriHandler::current()`. The host's, before composing.
    pub fn set_platform_uri_handler(&mut self, handler: PlatformUriHandler) {
        self.uri_handler = Rc::new(handler.clone());
        self.platform_uri_handler = handler;
    }

    /// Sets what content opens links with (`LOCAL_URI_HANDLER`), unchecked: a fake in a test.
    /// Before composing.
    pub fn set_uri_handler(&mut self, uri_handler: Rc<dyn UriHandler>) {
        self.uri_handler = uri_handler;
    }

    pub fn uri_handler(&self) -> Rc<dyn UriHandler> {
        self.uri_handler.clone()
    }

    /// Sets the haptic feedback content performs (`LOCAL_HAPTIC_FEEDBACK`): the host's, before
    /// composing.
    pub fn set_haptic_feedback(&mut self, haptic_feedback: Rc<dyn HapticFeedback>) {
        self.haptic_feedback = haptic_feedback;
    }

    pub fn haptic_feedback(&self) -> Rc<dyn HapticFeedback> {
        self.haptic_feedback.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_well_formed_uris_of_allowed_schemes_pass() {
        let ok = |uri| check_uri(uri, DEFAULT_URI_SCHEMES).is_ok();
        assert!(ok("https://example.com/a?b=1&c=2"));
        assert!(ok("HTTP://example.com"));
        assert!(ok("mailto:someone@example.com"));
        assert!(ok("tel:+1-555-0100"));
        for refused in [
            "C:\\Windows\\System32\\calc.exe",
            "c:/Windows/calc.exe",
            "\\\\server\\share",
            "file:///System/Applications/Calculator.app",
            "file://server/share",
            "search-ms:query=x",
            "ms-msdt:/id",
            "-a Calculator",
            "https://example.com/\nsecond",
            "https://example.com/a b",
            "https:\\\\server\\share",
            "https://example.com/\"quoted",
            "",
        ] {
            assert!(!ok(refused), "{refused:?}");
        }
        assert!(check_uri("myapp://open", &["myapp"]).is_ok());
    }

    #[test]
    fn logs_name_the_scheme_and_host_only() {
        assert_eq!(
            uri_for_log("https://user:secret@example.com:8080/path?token=1"),
            "https://example.com:8080/…"
        );
        assert_eq!(uri_for_log("mailto:someone@example.com"), "mailto:…");
        assert_eq!(uri_for_log("no scheme"), "a string without a scheme");
    }

    #[test]
    fn a_widened_handler_opens_its_schemes() {
        let opened = Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = opened.clone();
        let handler = PlatformUriHandler::new(move |uri| sink.borrow_mut().push(uri.to_string()));
        handler.open_uri("myapp://a");
        handler.open_uri("https://b");
        let widened = handler.with_schemes(&["MyApp"]);
        widened.open_uri("myapp://c");
        widened.open_uri("https://d");
        assert_eq!(*opened.borrow(), ["https://b", "myapp://c"]);
    }
}
