//! The examples as an Android library, `libcompose_rs_demo.so`: `ComposeRsView(context,
//! "gallery")` in android/demo shows one.

compose_android::export_apps! {
    "counter" => counter::Counter,
    "todo" => todo::TodoApp,
    "gallery" => gallery::Gallery,
}
