# compose-rs: Rust calls these through JNI by name (crates/compose-android/src/bridge.rs).
-keep class rs.compose.android.ComposeRsNative { *; }
-keep class rs.compose.android.ComposeRsView { *; }
-keep class rs.compose.android.TextMeasurer { *; }
-keep class rs.compose.android.ComposeRsImages { *; }
# AndroidX activity is compile-only: back goes through it only when the app has it.
-dontwarn androidx.activity.**
-keepnames class androidx.activity.OnBackPressedDispatcherOwner
# The same for lifecycles and view models; HostLifecycle.kt tells an AndroidX activity by these names.
-dontwarn androidx.lifecycle.**
-keepnames class androidx.lifecycle.LifecycleOwner
-keepnames class androidx.lifecycle.ViewModelStoreOwner
