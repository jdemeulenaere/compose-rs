//! A logger writing to stderr, for hosts without a system log of their own (desktop, tests):
//! compose-rs logs through the `log` crate's macros, and apps may too (D280).

use log::{LevelFilter, Log, Metadata, Record};

/// Writes each record to stderr as `[LEVEL target] message`.
pub struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        // `eprintln!`, so a test's output capture takes it.
        eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
    }

    fn flush(&self) {}
}

static STDERR_LOGGER: StderrLogger = StderrLogger;

/// Installs [`StderrLogger`] as the `log` crate's logger, unless the app installed one first
/// (then this does nothing), at [`default_max_level`].
pub fn install_stderr_logger() {
    if log::set_logger(&STDERR_LOGGER).is_ok() {
        log::set_max_level(default_max_level());
    }
}

/// The level a host's logger starts at: `COMPOSE_LOG` (`off`, `error`, `warn`, `info`,
/// `debug`, `trace`) if set, else `debug` in debug builds and `info` in release ones.
pub fn default_max_level() -> LevelFilter {
    let configured = std::env::var("COMPOSE_LOG").ok().and_then(|level| level.parse().ok());
    configured.unwrap_or(if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    })
}
