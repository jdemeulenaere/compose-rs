//! The desktop's platform services (`LOCAL_CLIPBOARD_MANAGER`, `LOCAL_URI_HANDLER`,
//! `LOCAL_HAPTIC_FEEDBACK`): the system clipboard, links opened by the OS, and no haptics (the
//! tree's default).

use std::cell::RefCell;
use std::process::{Command, Stdio};
use std::rc::Rc;

use compose_ui::platform::{MemoryClipboard, UiTree};
use compose_ui::{ClipboardManager, PlatformUriHandler, uri_for_log};

/// Sets a window's tree's services.
pub(crate) fn install(tree: &mut UiTree) {
    tree.set_clipboard(Rc::new(SystemClipboard::new()));
    tree.set_platform_uri_handler(PlatformUriHandler::new(open_with_os));
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

/// Opens a link `PlatformUriHandler` checked (an allowed scheme, no whitespace, quotes or
/// backslashes) with the OS: `open` on macOS, `xdg-open` on Linux and the BSDs, the URL
/// protocol handler on Windows. No shell is involved, and the opener's input and output are
/// closed.
fn open_with_os(uri: &str) {
    let mut command = open_command(uri);
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match command.spawn() {
        // Reaped on a thread of its own: the opener exits once it handed the link over.
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(error) => log::warn!("opening {}: {error}", uri_for_log(uri)),
    }
}

/// The command opening `uri`, its one argument.
fn open_command(uri: &str) -> Command {
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
    // `rundll32` hands its command line's rest to the handler as is: given verbatim, not quoted
    // as an argument (`check_uri` let no space or quote through).
    #[cfg(windows)]
    std::os::windows::process::CommandExt::raw_arg(&mut command, uri);
    #[cfg(not(windows))]
    command.arg(uri);
    command
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use compose_ui::{PlatformUriHandler, UriHandler};

    use super::open_command;

    /// The opener's arguments for `uri` through the host's handler: `None` if it's refused.
    fn arguments(uri: &str) -> Option<Vec<String>> {
        let opened = Rc::new(RefCell::new(None));
        let sink = opened.clone();
        let handler = PlatformUriHandler::new(move |uri| {
            let command = open_command(uri);
            let arguments = command.get_args().map(|arg| arg.to_string_lossy().into_owned());
            *sink.borrow_mut() = Some(arguments.collect());
        });
        handler.open_uri(uri);
        opened.take()
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

    /// Review: what still reaches the opener.
    #[test]
    fn review_dangerous_inputs_are_refused() {
        // A Windows drive path has a one-letter "scheme": `rundll32 url.dll,FileProtocolHandler`
        // runs an executable given its path.
        assert_eq!(arguments("C:\\Windows\\System32\\calc.exe"), None, "drive path");
        // `open file:///…/X.app` launches an app; `.command` files run in Terminal.
        assert_eq!(arguments("file:///System/Applications/Calculator.app"), None, "file:");
    }
}
