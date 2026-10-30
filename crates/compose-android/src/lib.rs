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
mod performance_hint;
mod services;

pub use jni;

/// A lifecycle state from the ordinal of AndroidX's `Lifecycle.State`, which has the same order.
/// `ComposeRsView` sends what it follows (D837).
#[doc(hidden)]
pub fn lifecycle_state(ordinal: i32) -> compose_lifecycle::LifecycleState {
    use compose_lifecycle::LifecycleState::*;
    match ordinal {
        0 => Destroyed,
        1 => Initialized,
        2 => Created,
        3 => Started,
        _ => Resumed,
    }
}

/// Animations on this thread (the main thread) follow Android's animator duration scale, as
/// Compose's `MotionDurationScale` does (D836). The setting holds what `adb` puts in it: a negative
/// one, which Compose would throw on while animating, is 0, not a crash of the app.
pub fn set_animator_duration_scale(scale: f32) {
    compose_ui::set_motion_duration_scale(animator_duration_scale_of(scale));
}

fn animator_duration_scale_of(setting: f32) -> f32 {
    if setting < 0.0 { 0.0 } else { setting }
}

thread_local! {
    static ANIMATOR_DURATION_SCALE_READER: std::cell::Cell<fn() -> f32> =
        const { std::cell::Cell::new(|| 1.0) };
}

/// Has the setting read with `read` the first time an animation on this thread needs it, not
/// before: `ComposeRsRuntime` starts watching it then (D839).
#[doc(hidden)]
pub fn read_animator_duration_scale_with(read: fn() -> f32) {
    ANIMATOR_DURATION_SCALE_READER.with(|reader| reader.set(read));
    compose_ui::set_motion_duration_scale_source(|| {
        animator_duration_scale_of(ANIMATOR_DURATION_SCALE_READER.with(std::cell::Cell::get)())
    });
}

thread_local! {
    /// The stores `AndroidXViewModels.kt` holds across configuration changes, by token.
    static RETAINED_VIEW_MODELS: std::cell::RefCell<RetainedViewModels> = Default::default();
}

#[derive(Default)]
struct RetainedViewModels {
    last_token: i64,
    stores: std::collections::HashMap<i64, compose_lifecycle::ViewModelStore>,
}

/// A view model store outliving the hosts given it, for the holder Kotlin keeps in a view tree's
/// `ViewModelStoreOwner`: its token, never 0 (D838).
#[doc(hidden)]
pub fn new_retained_view_model_store() -> i64 {
    RETAINED_VIEW_MODELS.with_borrow_mut(|retained| {
        retained.last_token += 1;
        retained.stores.insert(retained.last_token, compose_lifecycle::ViewModelStore::new());
        retained.last_token
    })
}

/// The store of `token`, if it's not been cleared.
#[doc(hidden)]
pub fn retained_view_model_store(token: i64) -> Option<compose_lifecycle::ViewModelStore> {
    RETAINED_VIEW_MODELS.with_borrow(|retained| retained.stores.get(&token).cloned())
}

/// Forgets `token`'s store and clears it: its holder is cleared for good.
#[doc(hidden)]
pub fn clear_retained_view_model_store(token: i64) {
    // Out of the borrow: a view model's `on_cleared` runs any code.
    let store = RETAINED_VIEW_MODELS.with_borrow_mut(|retained| retained.stores.remove(&token));
    if let Some(store) = store {
        store.clear();
    }
}

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
        /// # Safety
        /// Called by the VM loading the library, with itself.
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn JNI_OnLoad(
            vm: *mut $crate::jni::sys::JavaVM,
            _reserved: *mut ::std::ffi::c_void,
        ) -> $crate::jni::sys::jint {
            // SAFETY: the VM loading the library.
            let vm = unsafe { $crate::jni::JavaVM::from_raw(vm) };
            $crate::on_load_with(vm, &[$(($name, $content as fn())),*], $logger)
        }
    };
    ($($name:literal => $content:path),* $(,)?) => {
        /// # Safety
        /// Called by the VM loading the library, with itself.
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn JNI_OnLoad(
            vm: *mut $crate::jni::sys::JavaVM,
            _reserved: *mut ::std::ffi::c_void,
        ) -> $crate::jni::sys::jint {
            // SAFETY: the VM loading the library.
            let vm = unsafe { $crate::jni::JavaVM::from_raw(vm) };
            $crate::on_load(vm, &[$(($name, $content as fn())),*])
        }
    };
}
