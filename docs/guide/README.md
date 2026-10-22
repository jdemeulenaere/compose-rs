# compose-rs for Jetpack Compose developers

This guide is for Android developers who know Jetpack Compose and want to write a compose-rs app. compose-rs follows Compose's model and names closely, so most of what you know carries over. The guide covers what's different.

- **Getting started** (this page): a desktop app, an Android app, and their tests.
- **[From Kotlin to Rust](kotlin-to-rust.md)**: Compose code side by side with compose-rs code, from composables and state to navigation, Android interop and testing.
- **[Performance](performance.md)**: what's fast and why, what to avoid, and how to profile.
- **[Differences from Compose](differences.md)**: the known differences an app will run into.

The design documents in [docs/](..) explain how each part works, and [decisions.md](../decisions.md) records why. The samples quoted in the guide are in [`examples/guide`](../../examples/guide). `cargo test` compiles and runs them, and a test checks that every quote matches its source.

compose-rs isn't published yet. Apps depend on the crates by path, as the examples do. It runs on desktop (macOS, Windows, Linux) and on Android (arm64, API 29 and later).

## A desktop app

An app depends on the `compose` facade crate. Its modules mirror Compose's packages: `compose::runtime`, `compose::ui`, `compose::foundation` (and `foundation::layout`), `compose::material3` and `compose::animation`. The `desktop` feature adds `compose::desktop`, which opens windows. The `test` feature adds `compose::test`, the test rule.

Keep the composables in a library and the window in a binary behind a feature, so the same library can later be built for Android without the desktop dependencies:

```toml
[package]
name = "hello"
edition = "2024"

[features]
default = ["desktop"]
desktop = ["compose/desktop"]

[[bin]]
name = "hello"
path = "src/main.rs"
required-features = ["desktop"]

[dependencies]
compose = { path = "../compose-rs/crates/compose" }

[dev-dependencies]
compose = { path = "../compose-rs/crates/compose", features = ["test"] }
```

A composable is a function marked `#[composable]`. Calls to other composables go through call macros (`Column!(…)`, `Text!(…)`), which take Compose's named and default arguments and its trailing lambda:

<!-- snippet: src/getting_started.rs#hello -->
```rust
use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

#[composable]
pub fn Hello() {
    let name = remember_saveable(|| mutable_state_of(String::new()));
    MaterialTheme!({
        Surface!(modifier = Modifier.fill_max_size(), {
            Column!(
                modifier = Modifier.padding(16.dp()),
                vertical_arrangement = Arrangement::spaced_by(8.dp()),
                {
                    TextField!(
                        value = name.get(),
                        on_value_change = move |text: String| name.set(text),
                        modifier = Modifier.test_tag("name"),
                        label = Some(|| Text!("Your name")),
                        single_line = true,
                    );
                    let shown = name.get();
                    Text!("Hello, {}!", if shown.is_empty() { "world" } else { &shown });
                }
            );
        });
    });
}
```

`run_app` opens one window and runs until it's closed. Give it a `#[composable]` function rather than a closure with calls inside: the transform only sees code inside composable functions.

<!-- snippet: src/main.rs#main -->
```rust
use compose::desktop::{WindowOptions, run_app};

fn main() {
    run_app(
        WindowOptions { title: "Hello".into(), size: (360, 240).into(), ..Default::default() },
        guide::getting_started::Hello,
    );
}
```

In this repository, `cargo run -p guide` runs this app, and `cargo run -p gallery` runs a tour of the toolkit. For several windows, `compose::desktop::application` takes a composable whose `Window!(on_close_request = …, title = …, { … })` calls each open a window while they're in the composition, as Compose Desktop's `application { Window(…) }`.

Environment variables help while developing: `COMPOSE_FRAME_STATS=1` prints each frame's timings, `COMPOSE_SCREENSHOT=out.png` writes the last frame to a file when the app exits, and `COMPOSE_EXIT_AFTER_FRAMES=n` exits after `n` frames.

### Testing it

`compose_test` composes the app in a headless window and returns a test rule with Compose's finders, actions and assertions. Each action runs frames until the UI is idle, as Compose's rule does.

<!-- snippet: tests/getting_started.rs#test -->
```rust
use compose::test::*;
use guide::getting_started::Hello;

#[test]
fn greets_by_name() {
    let mut ui = compose_test(Hello);
    ui.on_node_with_text("Hello, world!").assert_exists();
    ui.on_node_with_tag("name").perform_text_input("Ada");
    ui.on_node_with_text("Hello, Ada!").assert_exists();
}
```

`cargo test` runs it. Text is measured with a fixed-advance measurer in tests (every character the same width), so layouts are deterministic on every machine. The [testing section](kotlin-to-rust.md#testing) covers recomposition counts, the test clock, saved state and event scripts.

### The edit loop

There's no hot reload or live preview. An edit is an incremental build and a restart, which takes about a second or two for a small app crate. Keep the composables you're editing in a small crate and stable code in other crates of the workspace. [docs/08](../08-fast-builds.md) covers the profile, linker and crate layout that keep rebuilds fast.

## An Android app

compose-rs runs in a plain Android `View`, `ComposeRsView`. It can be an activity's whole content or one element of a Kotlin Compose screen (through `AndroidView`), and it's scrolled, clipped and transformed like any other view. The Rust code is a native library that Gradle packages with the app.

You need:

- the Android SDK and NDK (found through `ANDROID_HOME` and `ANDROID_NDK_HOME`, or the newest NDK under `$ANDROID_HOME/ndk`);
- the Rust target: `rustup target add aarch64-linux-android`;
- Gradle, from Android Studio or installed (the repository has no Gradle wrapper).

Only arm64 devices are built for (64-bit phones and the arm64 emulator images). The minimum SDK is 29.

### 1. The Rust library

The Android library is a crate of its own, of type `cdylib`, depending on the app's library without its desktop feature. As `examples/android-demo` does, in the repository's workspace:

```toml
[package]
name = "hello-android"

[lib]
name = "hello_android"   # libhello_android.so
crate-type = ["cdylib"]

[dependencies]
hello = { path = "../hello", default-features = false }
compose = { path = "../../crates/compose" }
compose-android = { path = "../../crates/compose-android" }
```

Its `lib.rs` names the root composables that a `ComposeRsView` can show, with `compose_android::export_apps!`:

<!-- snippet: src/android.rs#export_apps -->
```rust
compose_android::export_apps! {
    "hello" => crate::getting_started::Hello,
    "navigation" => crate::navigation::App,
    "embedded" => crate::interop::Embedded,
}
```

### 2. Gradle

The Kotlin side is the `android/compose-rs` library module: `ComposeRsView` and its glue, without dependencies on AndroidX. Include it in your project's `settings.gradle.kts`, and build the Rust library before each build of the app with `android/build-rust.sh <cargo package> <library name> <jniLibs directory>`, as `android/demo/build.gradle.kts` does:

```kotlin
android {
    defaultConfig { minSdk = 29 }
    sourceSets["main"].jniLibs.directories.add("build/rustJniLibs")
}

val buildRust = tasks.register<Exec>("buildRust") {
    val output = layout.buildDirectory.dir("rustJniLibs").get().asFile
    commandLine(rootDir.resolve("build-rust.sh").path, "hello-android", "hello_android", output.path)
}
tasks.named("preBuild") { dependsOn(buildRust) }

dependencies {
    implementation(project(":compose-rs"))
}
```

`build-rust.sh` runs `cargo build --release --target aarch64-linux-android -p <package>` with the NDK's linker (no `cargo-ndk` needed) and copies the library where Gradle packages it. It builds in the compose-rs workspace (the directory above the script), so it builds packages of that workspace, as the examples are. For an app in a workspace of its own, copy the script and change the directory it builds in, and point Gradle at the library module with `project(":compose-rs").projectDir = file("…/compose-rs/android/compose-rs")`.

### 3. The activity

Load the library once, then create views by app name. In a plain activity:

```kotlin
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ComposeRsRuntime.loadLibrary("hello_android")
        setContentView(ComposeRsView(this, "hello"))
    }
}
```

In a Kotlin Compose screen, through `AndroidView`:

```kotlin
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ComposeRsRuntime.loadLibrary("hello_android")
        setContent {
            MaterialTheme {
                Column(Modifier.safeDrawingPadding()) {
                    Text("Kotlin Compose above, compose-rs below")
                    AndroidView(
                        factory = { context -> ComposeRsView(context, "hello") },
                        modifier = Modifier.fillMaxWidth().weight(1f),
                    )
                }
            }
        }
    }
}
```

The view composes its app while it's attached to a window. It saves the app's `remember_saveable` values in its instance state, which needs a view id: it takes `R.id.compose_rs_view` unless it has one, so two views in one window need ids of their own. Back goes to the app's `BackHandler`s while one is enabled, then to the activity. Kotlin passes properties with `setProp` and hears events with `setOnEventListener` ([Android interop](kotlin-to-rust.md#android-interop)).

### 4. Build and run

The demo is a working template: `examples/android-demo` is its Rust library (the counter, todo and gallery examples) and `android/demo` its Kotlin app, which shows each example in a tab of a Kotlin Compose screen. From `android/`, with a device connected:

```bash
gradle :demo:installDebug
adb shell am start -n rs.compose.demo/.MainActivity
```

To start your own app in this repository, copy `examples/android-demo` and `android/demo`, rename the crate, the library and the Android package, and add them to the workspace's members and to `android/settings.gradle.kts`.

A Rust panic aborts the app at the JNI boundary; its message is in `adb logcat`, under the tag `compose-rs`. Perfetto traces show compose-rs's frame phases ([Performance](performance.md#profiling)).
