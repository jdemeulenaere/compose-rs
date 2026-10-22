//! Platform services content reaches through composition locals, as Compose's: opening links
//! (`LOCAL_URI_HANDLER`) and haptic feedback (`LOCAL_HAPTIC_FEEDBACK`); the clipboard's is in
//! `clipboard.rs`. The host sets the platform's on its tree (`UiTree::set_uri_handler`,
//! `set_haptic_feedback`) before composing; the locals' defaults read the composing tree's, so
//! a subtree (a test) can provide fakes as in Compose.

use std::rc::Rc;

use compose_runtime::CompositionLocal;

use crate::tree::UiTree;

/// Opens links, as Compose's `UriHandler`: the platform's browser (Android starts an
/// `ACTION_VIEW` intent; desktop asks the OS: `open`, `xdg-open`, the shell on Windows).
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

/// What a tree has without a platform's (headless, tests): opening a link logs it.
pub(crate) struct LoggingUriHandler;

impl UriHandler for LoggingUriHandler {
    fn open_uri(&self, uri: &str) {
        log::info!("compose-ui: no platform to open {uri:?} (no UriHandler set)");
    }
}

/// What a tree has without a platform's, and desktop's: nothing happens.
pub(crate) struct NoHapticFeedback;

impl HapticFeedback for NoHapticFeedback {
    fn perform_haptic_feedback(&self, _feedback_type: HapticFeedbackType) {}
}

/// Opens links, as Compose's `LocalUriHandler`: the host's (`UiTree::set_uri_handler`), or
/// one that logs the link outside a tree's composition or without a platform.
pub static LOCAL_URI_HANDLER: CompositionLocal<Rc<dyn UriHandler>> =
    CompositionLocal::new("LocalUriHandler", || {
        of_composing_tree(|tree| tree.uri_handler.clone())
            .unwrap_or_else(|| Rc::new(LoggingUriHandler))
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
    /// Sets how content opens links (`LOCAL_URI_HANDLER`): the host's, before composing.
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
