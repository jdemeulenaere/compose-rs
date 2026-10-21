//! Trace sections around a frame's phases (recomposition, layout, drawing), for the platform's
//! tracer: on Android, `ATrace`, so they show in Perfetto beside the system's. Free when no
//! tracer is set.

use std::ffi::CStr;
use std::sync::OnceLock;

/// Begins and ends a section.
type Tracer = (fn(&'static CStr), fn());

static TRACER: OnceLock<Tracer> = OnceLock::new();

/// Sets the tracer (once, for the process): `begin` starts a section named as given, `end`
/// ends the innermost.
pub fn set_tracer(begin: fn(&'static CStr), end: fn()) {
    let _ = TRACER.set((begin, end));
}

/// A section, ended when dropped.
pub struct Section(Option<fn()>);

impl Drop for Section {
    fn drop(&mut self) {
        if let Some(end) = self.0 {
            end();
        }
    }
}

/// Starts a section named `name`, ended when the returned guard drops.
pub fn section(name: &'static CStr) -> Section {
    match TRACER.get() {
        Some((begin, end)) => {
            begin(name);
            Section(Some(*end))
        }
        None => Section(None),
    }
}
