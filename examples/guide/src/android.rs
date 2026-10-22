//! The Android entry point (docs/guide/README.md, "An Android app"): the root composables a
//! `ComposeRsView` shows by name. In the guide's layout this is the `lib.rs` of
//! `examples/hello-android`, where `hello` is the app's library crate; here the macro is in the
//! guide's library, with `hello` naming the module that plays its part, so that it compiles.

use crate::getting_started as hello;

// ANCHOR: export_apps
// examples/hello-android/src/lib.rs
compose_android::export_apps! {
    "hello" => hello::Hello,
}
// ANCHOR_END: export_apps
