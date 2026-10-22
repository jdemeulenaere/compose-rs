//! The `log` crate's records in logcat (D280): compose-rs's warnings and the app's own
//! `log::info!`s, tagged with the crate that logged them (`compose_runtime`, the app's).
//! Installed by `JNI_OnLoad`, before the app's code runs, unless the app opted out
//! (`export_apps! { logger = false; … }`).

use log::{Level, Log, Metadata, Record};

#[cfg(target_os = "android")]
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(
        priority: std::ffi::c_int,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

/// Writes records to logcat (to stderr off Android, for host tests).
struct AndroidLogger;

static LOGGER: AndroidLogger = AndroidLogger;

/// Installs the logcat logger, unless the app set a logger first, at the host's level
/// (`debug` in debug builds, `info` in release ones).
pub(crate) fn install() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(compose_ui::platform::default_max_level());
    }
}

impl Log for AndroidLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let message = record.args().to_string();
        let tag = tag(record.target());
        for chunk in chunks(&message, MAX_ENTRY) {
            write(priority(record.level()), tag, chunk);
        }
    }

    fn flush(&self) {}
}

/// Logcat's entries are cut past about 4 KB: longer messages are split.
const MAX_ENTRY: usize = 4000;

/// Android's priority for `level` (`android/log.h`: VERBOSE 2 … ERROR 6).
fn priority(level: Level) -> i32 {
    match level {
        Level::Error => 6,
        Level::Warn => 5,
        Level::Info => 4,
        Level::Debug => 3,
        Level::Trace => 2,
    }
}

/// The tag: the crate of the record's target (`compose_runtime::state` → `compose_runtime`).
fn tag(target: &str) -> &str {
    let tag = target.split("::").next().unwrap_or(target);
    if tag.is_empty() { "compose-rs" } else { tag }
}

/// `text` in pieces of at most `limit` bytes, cut at character boundaries.
fn chunks(text: &str, limit: usize) -> impl Iterator<Item = &str> {
    let mut rest = text;
    let mut first = true;
    std::iter::from_fn(move || {
        if rest.is_empty() && !std::mem::take(&mut first) {
            return None;
        }
        first = false;
        let mut end = rest.len().min(limit);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 {
            // A character longer than the limit goes whole.
            end = rest.chars().next().map_or(0, char::len_utf8);
        }
        let (chunk, remaining) = rest.split_at(end);
        rest = remaining;
        Some(chunk)
    })
}

fn write(priority: i32, tag: &str, text: &str) {
    #[cfg(target_os = "android")]
    {
        let tag = std::ffi::CString::new(tag.replace('\0', " ")).unwrap_or_default();
        let text = std::ffi::CString::new(text.replace('\0', " ")).unwrap_or_default();
        // SAFETY: both are NUL-terminated strings that outlive the call.
        unsafe { __android_log_write(priority, tag.as_ptr(), text.as_ptr()) };
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("{priority} {tag}: {text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_tagged_with_their_crate() {
        assert_eq!(tag("compose_runtime::state"), "compose_runtime");
        assert_eq!(tag("my_app"), "my_app");
        assert_eq!(tag(""), "compose-rs");
        assert_eq!(priority(Level::Warn), 5);
        assert_eq!(priority(Level::Trace), 2);
    }

    #[test]
    fn long_messages_are_split_at_character_boundaries() {
        assert_eq!(chunks("", 4).collect::<Vec<_>>(), [""]);
        assert_eq!(chunks("abcdefghij", 4).collect::<Vec<_>>(), ["abcd", "efgh", "ij"]);
        // "é" is two bytes: never cut in half.
        assert_eq!(chunks("aéé", 2).collect::<Vec<_>>(), ["a", "é", "é"]);
    }

    /// Review: at the real limit, with two-, three- and four-byte characters straddling it.
    #[test]
    fn review_chunks_at_max_entry_reassemble() {
        for prefix in 0..4 {
            for ch in ['é', '€', '😀'] {
                let text: String = "a".repeat(prefix) + &ch.to_string().repeat(3000);
                let pieces: Vec<&str> = chunks(&text, MAX_ENTRY).collect();
                assert_eq!(pieces.concat(), text);
                assert!(pieces.iter().all(|p| p.len() <= MAX_ENTRY && !p.is_empty()));
                assert!(pieces[0].len() > MAX_ENTRY - 4);
            }
        }
        // A character longer than the limit goes whole, and the loop ends.
        assert_eq!(chunks("😀😀", 1).collect::<Vec<_>>(), ["😀", "😀"]);
    }
}
