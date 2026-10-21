//! The clipboard text fields copy to and paste from: the host's (the system's on desktop), or
//! one in memory (headless and tests).

/// A text clipboard.
pub trait Clipboard {
    fn text(&mut self) -> Option<String>;
    fn set_text(&mut self, text: &str);
}

/// A clipboard in memory: the default, and what tests use.
#[derive(Default)]
pub struct MemoryClipboard {
    text: Option<String>,
}

impl Clipboard for MemoryClipboard {
    fn text(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn set_text(&mut self, text: &str) {
        self.text = Some(text.to_string());
    }
}
