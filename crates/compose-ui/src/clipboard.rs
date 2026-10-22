//! The clipboard, as Compose's `ClipboardManager`: what text fields and `SelectionContainer`
//! copy to and paste from, and what content reaches as `LOCAL_CLIPBOARD_MANAGER`. The host's
//! (the system's on desktop and Android), or one in memory (headless and tests).

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::CompositionLocal;

/// A text clipboard, as Compose's `ClipboardManager` (plain text: no annotations).
pub trait ClipboardManager {
    /// The clipboard's text, if it holds some.
    fn get_text(&self) -> Option<String>;
    /// Puts `text` on the clipboard.
    fn set_text(&self, text: &str);
    /// Whether it holds a non-empty text.
    fn has_text(&self) -> bool {
        self.get_text().is_some_and(|text| !text.is_empty())
    }
}

/// A clipboard in memory: the default, and what tests use.
#[derive(Default)]
pub struct MemoryClipboard {
    text: RefCell<Option<String>>,
}

impl ClipboardManager for MemoryClipboard {
    fn get_text(&self) -> Option<String> {
        self.text.borrow().clone()
    }

    fn set_text(&self, text: &str) {
        *self.text.borrow_mut() = Some(text.to_string());
    }
}

/// The clipboard, as Compose's `LocalClipboardManager`: the host's (`UiTree::set_clipboard`),
/// which text fields use too, or one in memory outside a tree's composition. Provide another
/// to a subtree to fake it in a test (text fields keep the tree's).
pub static LOCAL_CLIPBOARD_MANAGER: CompositionLocal<Rc<dyn ClipboardManager>> =
    CompositionLocal::new("LocalClipboardManager", || {
        crate::platform_services::of_composing_tree(|tree| tree.clipboard.clone())
            .unwrap_or_else(|| OUTSIDE_A_TREE.with(Rc::clone))
    });

thread_local! {
    /// The clipboard outside a tree's composition: one per thread, so what's copied there can
    /// be pasted there.
    static OUTSIDE_A_TREE: Rc<dyn ClipboardManager> = Rc::new(MemoryClipboard::default());
}
