//! The desktop's platform services (`LOCAL_CLIPBOARD_MANAGER`, `LOCAL_URI_HANDLER`,
//! `LOCAL_HAPTIC_FEEDBACK`): the system clipboard, links opened by the OS, and no haptics (the
//! tree's default).

use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;

use compose_ui::platform::{MemoryClipboard, UiTree};
use compose_ui::{ClipboardManager, UriHandler};

/// Sets a window's tree's services.
pub(crate) fn install(tree: &mut UiTree) {
    tree.set_clipboard(Rc::new(SystemClipboard::new()));
    tree.set_uri_handler(Rc::new(DesktopUriHandler));
}

/// The system clipboard, through `arboard`; a clipboard in memory if the system's can't be
/// opened.
struct SystemClipboard {
    system: RefCell<Option<arboard::Clipboard>>,
    fallback: MemoryClipboard,
}

impl SystemClipboard {
    fn new() -> SystemClipboard {
        SystemClipboard {
            system: RefCell::new(arboard::Clipboard::new().ok()),
            fallback: MemoryClipboard::default(),
        }
    }
}

impl ClipboardManager for SystemClipboard {
    fn get_text(&self) -> Option<String> {
        match &mut *self.system.borrow_mut() {
            Some(system) => system.get_text().ok(),
            None => self.fallback.get_text(),
        }
    }

    fn set_text(&self, text: &str) {
        match &mut *self.system.borrow_mut() {
            Some(system) => {
                let _ = system.set_text(text);
            }
            None => self.fallback.set_text(text),
        }
    }
}

/// Opens links with the OS: `open` on macOS, `xdg-open` on Linux and the BSDs, the URL
/// protocol handler on Windows.
struct DesktopUriHandler;

impl UriHandler for DesktopUriHandler {
    fn open_uri(&self, uri: &str) {
        let Some(mut command) = open_command(uri) else {
            log::warn!("not opening {uri:?}: not a URI");
            return;
        };
        match command.spawn() {
            // Reaped on a thread of its own: the opener exits once it handed the link over.
            Ok(mut child) => {
                std::thread::spawn(move || child.wait());
            }
            Err(error) => log::warn!("opening {uri:?}: {error}"),
        }
    }
}

/// The command opening `uri`: `None` for what isn't a URI with a scheme (an argument the
/// opener would take for an option, a bare path).
fn open_command(uri: &str) -> Option<Command> {
    let scheme = uri.split_once(':')?.0;
    let valid_scheme = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid_scheme || uri.chars().any(char::is_control) {
        return None;
    }
    let mut command = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        // No shell: `cmd /c start` would interpret `&` in a query.
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        Command::new("xdg-open")
    };
    command.arg(uri);
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::open_command;

    fn arguments(uri: &str) -> Option<Vec<String>> {
        open_command(uri).map(|command| {
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect()
        })
    }

    #[test]
    fn a_uri_is_passed_to_the_opener_as_one_argument() {
        let uri = "https://example.com/a?b=1&c=2";
        assert_eq!(arguments(uri).and_then(|args| args.last().cloned()).as_deref(), Some(uri));
        assert!(arguments("mailto:someone@example.com").is_some());
    }

    #[test]
    fn what_isnt_a_uri_is_refused() {
        assert_eq!(arguments("-a Calculator"), None);
        assert_eq!(arguments("/etc/passwd"), None);
        assert_eq!(arguments("1http://example.com"), None);
        assert_eq!(arguments("https://example.com/\nsecond"), None);
        assert_eq!(arguments(""), None);
    }
}
