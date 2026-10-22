//! The Android entry point (docs/guide/README.md, "Android"): the root composables a
//! `ComposeRsView` shows by name. An app's Android library is a `cdylib` crate whose `lib.rs`
//! has this macro; here it's in the guide's library so that it compiles with the rest.

// ANCHOR: export_apps
compose_android::export_apps! {
    "hello" => crate::getting_started::Hello,
    "navigation" => crate::navigation::App,
    "embedded" => crate::interop::Embedded,
}
// ANCHOR_END: export_apps
