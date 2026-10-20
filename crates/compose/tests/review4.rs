//! Findings of the fourth runtime review (after 10a535b) that need `#[composable]`. Each test
//! fails; its comment says what is expected and what happens instead.

use std::path::Path;
use std::process::Command;

// ---- Finding (major): an `#[inline]` composable from another crate never keeps its state -----

/// The fixture in `tests/review4_xcrate`: a library crate `widgets` defines composables (an
/// `#[inline]` one, an `#[inline(always)]` one, a generic one, a plain one), and an `app` crate
/// calls them from a `Screen` composable. Each composable remembers a value and reads `tick`;
/// `Screen` reads `outer`. The app alternates a pass where only the composables restart (their
/// restart functions call them) with a pass where `Screen` recomposes and calls them again, and
/// exits with status 1, naming the composables whose `remember` ran again.
///
/// Expected: no composable loses its state, in any build profile.
///
/// Actual, in a debug build (opt-level 0): `Inlined` and `InlinedAlways`, called from `app`,
/// recreate their state on *every* pass (`state recreated: ["create inlined", "create always",
/// ...]`); the release build (any codegen-units, thin or fat LTO, opt-level 1/s) keeps it. The
/// same composables called from inside `widgets` (`Section`) keep it everywhere.
///
/// Why: groups match on the restart `fn` pointer (`fn_addr_eq`, composer.rs ~151, D47), and a
/// function's address isn't a stable identity (`fn_addr_eq`'s own docs say so). At opt-level 0,
/// an upstream `#[inline]` function is instantiated a second time in the calling crate: `nm`
/// on the app binary shows two `widgets::Inlined` (one in `widgets`, one in `app`) and two
/// `FnOnce::call_once` shims of its restart closure, but a single closure body, exported by
/// `widgets`. `Screen` calls app's copy, which records app's shim; a restart calls that shim,
/// whose (shared) closure body calls `widgets`' copy, which starts the group with `widgets`'
/// shim: a different pointer, so the old group doesn't match and is replaced. The next parent
/// pass swaps back. Every `remember`, effect and kept argument of the composable is lost each
/// time, so `DisposableEffect`s re-run and cleanups fire on every frame in debug builds only.
#[test]
#[cfg_attr(miri, ignore = "spawns a process")]
fn an_inline_composable_from_another_crate_keeps_its_state() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/review4_xcrate/Cargo.toml");
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("review4_xcrate");
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--package", "app", "--manifest-path"])
        .arg(&fixture)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("run cargo on the fixture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the fixture app failed ({:?}):\n{stdout}\n{stderr}",
        output.status
    );
}
