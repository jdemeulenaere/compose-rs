//! A logger writing to stderr, for hosts without a system log of their own (desktop, tests):
//! compose-rs logs through the `log` crate's macros, and apps may too (D280).

use std::sync::OnceLock;

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Which records a logger writes: a level, and levels for some targets, from a
/// `COMPOSE_LOG`-style list (`info`, `warn,my_app=debug,compose_runtime::state=off`). A
/// target's level is that of its longest listed prefix (by `::` segments), else the default.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogFilter {
    default: LevelFilter,
    targets: Vec<(String, LevelFilter)>,
}

impl LogFilter {
    /// Every record up to `level`.
    pub fn new(level: LevelFilter) -> LogFilter {
        LogFilter { default: level, targets: Vec::new() }
    }

    /// Parses a comma-separated list of `level` (the default) and `target=level` items.
    pub fn parse(spec: &str) -> Result<LogFilter, String> {
        let mut filter = LogFilter::new(default_max_level());
        for item in spec.split(',').map(str::trim).filter(|item| !item.is_empty()) {
            let level = |text: &str| -> Result<LevelFilter, String> {
                text.trim().parse().map_err(|_| format!("{text:?} is not a level"))
            };
            match item.split_once('=') {
                Some((target, value)) if !target.trim().is_empty() => {
                    filter.targets.push((target.trim().to_string(), level(value)?));
                }
                Some(_) => return Err(format!("{item:?} names no target")),
                None => filter.default = level(item)?,
            }
        }
        Ok(filter)
    }

    /// The level for `target`.
    pub fn level_for(&self, target: &str) -> LevelFilter {
        self.targets
            .iter()
            .filter(|(prefix, _)| {
                target == prefix
                    || target
                        .strip_prefix(prefix.as_str())
                        .is_some_and(|rest| rest.starts_with("::"))
            })
            .max_by_key(|(prefix, _)| prefix.len())
            .map_or(self.default, |(_, level)| *level)
    }

    pub fn enabled(&self, target: &str, level: Level) -> bool {
        level <= self.level_for(target)
    }

    /// The most verbose level anything passes: the `log` crate's maximum.
    pub fn max_level(&self) -> LevelFilter {
        self.targets.iter().map(|(_, level)| *level).fold(self.default, Ord::max)
    }
}

/// Writes each record its filter passes to stderr as `[LEVEL target] message`.
pub struct StderrLogger {
    filter: OnceLock<LogFilter>,
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.filter.get().is_none_or(|filter| filter.enabled(metadata.target(), metadata.level()))
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            // `eprintln!`, so a test's output capture takes it.
            eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

static STDERR_LOGGER: StderrLogger = StderrLogger { filter: OnceLock::new() };

/// Installs [`StderrLogger`] as the `log` crate's logger, unless the app installed one first
/// (then this does nothing), filtering by `COMPOSE_LOG` (a [`LogFilter`] list) if set, else at
/// [`default_max_level`]. An invalid `COMPOSE_LOG` is warned about, once, and ignored.
pub fn install_stderr_logger() {
    let configured = std::env::var("COMPOSE_LOG").ok();
    let parsed = configured.as_deref().map(LogFilter::parse);
    let filter = match &parsed {
        Some(Ok(filter)) => filter.clone(),
        _ => LogFilter::new(default_max_level()),
    };
    let max_level = filter.max_level();
    let _ = STDERR_LOGGER.filter.set(filter);
    if log::set_logger(&STDERR_LOGGER).is_ok() {
        log::set_max_level(max_level);
        if let (Some(value), Some(Err(error))) = (configured, parsed) {
            log::warn!("ignoring COMPOSE_LOG={value:?}: {error}");
        }
    }
}

/// The level a host's logger starts at: `debug` in debug builds, `info` in release ones.
pub fn default_max_level() -> LevelFilter {
    if cfg!(debug_assertions) { LevelFilter::Debug } else { LevelFilter::Info }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_takes_a_level_and_levels_by_target() {
        let filter = LogFilter::parse("warn, my_app=debug,my_app::net=off").unwrap();
        assert!(filter.enabled("other", Level::Warn));
        assert!(!filter.enabled("other", Level::Info));
        assert!(filter.enabled("my_app", Level::Debug));
        assert!(filter.enabled("my_app::ui", Level::Debug));
        assert!(!filter.enabled("my_app::net", Level::Error));
        assert!(filter.enabled("my_app::network", Level::Debug), "my_app's, not my_app::net's");
        assert_eq!(filter.max_level(), LevelFilter::Debug);
        assert_eq!(LogFilter::parse("trace").unwrap().max_level(), LevelFilter::Trace);
    }

    #[test]
    fn an_invalid_filter_says_why() {
        assert!(LogFilter::parse("loud").is_err());
        assert!(LogFilter::parse("my_app=loud").is_err());
        assert!(LogFilter::parse("=info").is_err());
        assert_eq!(LogFilter::parse("").unwrap(), LogFilter::new(default_max_level()));
    }
}
