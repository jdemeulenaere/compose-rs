//! compose-rs on Android (docs/07-platforms.md, "Android"): a Kotlin `ComposeRsView` (a plain
//! `View`, so it composes inside Kotlin Compose with `AndroidView`) hosts a composition here.
//!
//! - **Frames:** on a Choreographer frame, Kotlin calls `frame` (in the bridge) once: this
//!   recomposes, lays out and draws, writing the display list into a buffer. In `onDraw`,
//!   Kotlin takes the buffer and replays it on the view's `Canvas` (hardware-accelerated: the
//!   system draws it on the GPU). No Skia is shipped.
//! - **Text:** `StaticLayout` on the Kotlin side, one crossing per paragraph laid out
//!   ([`text::AndroidTextMeasurer`]).
//! - **Input:** touches and keys come one crossing per event; the soft keyboard's text through
//!   an `InputConnection`.
//! - **Tasks:** the executor's wakes and `delay`s become main-thread `Handler` posts.
//!
//! An app's Rust library names its root composables with [`export_apps!`], which also defines
//! `JNI_OnLoad`; `ComposeRsView(context, "name")` shows one.

// Every type reachable from the public API can be named (D260): a type in a private
// module that a public signature uses must be re-exported.
#![deny(unnameable_types)]

pub mod accessibility;
mod image;
pub mod input_method;
pub mod serialize;
pub mod text;

mod bridge;
mod frame_time;
mod logging;
mod services;

pub use jni;

use std::sync::OnceLock;

/// A root composable an app's library offers, and its name.
type App = (&'static str, fn());

/// The root composables an app's library offers.
static APPS: OnceLock<Vec<App>> = OnceLock::new();

/// Called by `JNI_OnLoad` (see [`export_apps!`]): keeps the VM and the apps, and installs
/// the logcat logger.
pub fn on_load(vm: jni::JavaVM, apps: &[App]) -> jni::sys::jint {
    on_load_with(vm, apps, true)
}

/// [`on_load`], installing the logcat logger only if `logger` (`export_apps!`'s
/// `logger = false;` leaves the `log` crate's logger to the app: `android_logger`, say).
pub fn on_load_with(vm: jni::JavaVM, apps: &[App], logger: bool) -> jni::sys::jint {
    let _ = APPS.set(apps.to_vec());
    if logger {
        // `log`'s records (compose-rs's warnings, the app's) go to logcat.
        logging::install();
    }
    bridge::set_vm(vm);
    jni::sys::JNI_VERSION_1_6
}

fn app(name: &str) -> Option<fn()> {
    APPS.get()?.iter().find(|(app, _)| *app == name).map(|(_, content)| *content)
}

/// Defines `JNI_OnLoad` for an app's library, naming the root composables `ComposeRsView`
/// shows by name:
///
/// ```ignore
/// compose_android::export_apps! {
///     "counter" => counter::Counter,
///     "todo" => todo::TodoApp,
/// }
/// ```
///
/// It installs a logcat logger for the `log` crate before any of the app's code runs; start
/// with `logger = false;` to install one of the app's own (in its first composable, or a
/// `JNI_OnLoad` of its own calling [`on_load_with`]).
#[macro_export]
macro_rules! export_apps {
    (logger = $logger:expr; $($name:literal => $content:path),* $(,)?) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn JNI_OnLoad(
            vm: $crate::jni::JavaVM,
            _reserved: *mut ::std::ffi::c_void,
        ) -> $crate::jni::sys::jint {
            $crate::on_load_with(vm, &[$(($name, $content as fn())),*], $logger)
        }
    };
    ($($name:literal => $content:path),* $(,)?) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn JNI_OnLoad(
            vm: $crate::jni::JavaVM,
            _reserved: *mut ::std::ffi::c_void,
        ) -> $crate::jni::sys::jint {
            $crate::on_load(vm, &[$(($name, $content as fn())),*])
        }
    };
}
