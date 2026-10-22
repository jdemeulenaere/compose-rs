//! `export_apps!` with `logger = false;`: the app keeps the `log` crate's logger for itself.

fn empty() {}

compose_android::export_apps! {
    logger = false;
    "empty" => empty,
}

#[test]
fn an_app_can_opt_out_of_the_logcat_logger() {
    // `JNI_OnLoad` is defined (it needs a VM to call).
    let _: extern "system" fn(compose_android::jni::JavaVM, *mut std::ffi::c_void) -> i32 =
        JNI_OnLoad;
}
