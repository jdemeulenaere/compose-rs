//! The runtime's warnings go through the `log` crate (D280), so they reach whatever logger the
//! platform installed (logcat on Android). Its own binary: a logger is process-wide.

use std::sync::Mutex;

use compose_runtime::Owned;

static RECORDS: Mutex<Vec<(log::Level, String, String)>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        RECORDS.lock().unwrap().push((
            record.level(),
            record.target().to_string(),
            record.args().to_string(),
        ));
    }

    fn flush(&self) {}
}

#[test]
fn a_stale_handle_warns_through_the_logger() {
    log::set_logger(&Capture).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    // The owner dropped at once (no composition: freed at once).
    let handle = *Owned::new(7_u32, |object| object);
    assert_eq!(handle.get(), None);
    let records = RECORDS.lock().unwrap();
    let warnings: Vec<_> = records
        .iter()
        .filter(|(level, target, _)| {
            *level == log::Level::Warn && target.starts_with("compose_runtime")
        })
        .collect();
    assert_eq!(warnings.len(), 1, "{records:?}");
    assert!(warnings[0].2.contains("u32"), "{}", warnings[0].2);
    assert!(warnings[0].2.contains("after its owner"), "{}", warnings[0].2);
}
