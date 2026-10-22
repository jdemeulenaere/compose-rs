# compose-rs for Jetpack Compose developers

This guide is for Android developers who know Jetpack Compose. It takes you from an empty crate to a desktop app and an Android app with tests, then maps Compose's APIs one by one to compose-rs's: composables, state, effects, layouts, drawing, Material 3, text, navigation, gestures, animation, Android interop and testing. compose-rs follows Compose's model and names closely, so most of what you know carries over; the guide shows how it's written in Rust, and where it differs.

- **Getting started** (this page): a desktop app, an Android app, and their tests.
- **[From Kotlin to Rust](kotlin-to-rust.md)**: Compose code side by side with compose-rs code.
- **[Performance](performance.md)**: what's fast and why, what to avoid, and how to profile.
- **[Differences from Compose](differences.md)**: the known differences an app will run into.

The samples quoted in the guide are in [`examples/guide`](../../examples/guide). `cargo test` compiles and runs them, and a test checks that every quote matches its source. The design documents in [docs/](..) explain how each part works, and [decisions.md](../decisions.md) records why.

compose-rs isn't published yet. It runs on desktop (macOS, Windows, Linux) and on Android (arm64, API 29 and later). On Linux, the desktop build links the system's FreeType and Fontconfig: install their development packages (`libfreetype-dev` and `libfontconfig1-dev` on Debian and Ubuntu, `freetype-devel` and `fontconfig-devel` on Fedora, `freetype2` and `fontconfig` on Arch) and `pkg-config`.

## The layout of an app

This page builds an app called `hello` inside the compose-rs repository, as the examples are, so that its crates depend on the framework by path and Android's build script finds them:

```text
compose-rs/
├── Cargo.toml                 # the workspace: add examples/hello and examples/hello-android to its members
├── examples/hello/            # the app: its composables (a library) and its desktop window (a binary)
├── examples/hello-android/    # the Android library: names the app's root composables for ComposeRsView
└── android/hello/             # the Kotlin app: a copy of android/demo
```

An app in a repository of its own works the same way, with its own workspace, paths to a compose-rs checkout, and its own copy of `android/build-rust.sh` (see [Gradle](#2-gradle)).

## A desktop app

An app depends on the `compose` facade crate. Its modules mirror Compose's packages: `compose::runtime`, `compose::ui`, `compose::foundation` (and `foundation::layout`), `compose::material3`, `compose::animation` and `compose::navigation`. The `desktop` feature adds `compose::desktop`, which opens windows; the `test` feature adds `compose::test`, the test rule.

Keep the composables in a library and the window in a binary behind a feature, so the Android library can use the composables without the desktop dependencies. `examples/hello/Cargo.toml`:

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
compose = { path = "../../crates/compose" }

[dev-dependencies]
compose = { path = "../../crates/compose", features = ["test"] }
```

A composable is a function marked `#[composable]`. Calls to other composables go through call macros (`Column!(…)`, `Text!(…)`), which take Compose's named and default arguments and its trailing lambda. `examples/hello/src/lib.rs`:

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

`run_app` opens one window and runs until it's closed. Give it a `#[composable]` function: the transform only sees code inside composable functions.

<!-- snippet: src/main.rs#main -->
```rust
// examples/hello/src/main.rs
use compose::desktop::{WindowOptions, run_app};

fn main() {
    run_app(
        WindowOptions { title: "Hello".into(), size: (360, 240).into(), ..Default::default() },
        hello::Hello,
    );
}
```

`cargo run -p hello` runs it. In this repository, `cargo run -p guide` runs the same app, and `cargo run -p gallery` a tour of the toolkit.

For several windows, `compose::desktop::application` takes a composable whose `Window!(on_close_request = …, title = …, { … })` calls each open a window while they're in the composition, as Compose Desktop's `application { Window(…) }`.

Environment variables help while developing:

- `COMPOSE_FRAME_STATS=1` prints each frame's timings;
- `COMPOSE_SCREENSHOT=out.png` writes the last frame to a file when the app exits;
- `COMPOSE_EXIT_AFTER_FRAMES=n` exits after `n` frames.

### Testing it

`compose_test` composes the app in a headless window and returns a test rule with Compose's finders, actions and assertions. Each action runs frames until the UI is idle, as Compose's rule does.

<!-- snippet: tests/suite/getting_started.rs#test -->
```rust
// examples/hello/tests/hello.rs
use compose::test::*;
use hello::Hello;

#[test]
fn greets_by_name() {
    let mut ui = compose_test(Hello);
    ui.on_node_with_text("Hello, world!").assert_exists();
    ui.on_node_with_tag("name").perform_text_input("Ada");
    ui.on_node_with_text("Hello, Ada!").assert_exists();
}
```

`cargo test` runs it. Text is measured with a fixed-advance measurer in tests (every character the same width), so layouts are the same on every machine. The [testing section](kotlin-to-rust.md#testing) covers recomposition counts, the test clock, saved state and event scripts.

### The edit loop

There's no hot reload or live preview: an edit is an incremental build and a restart. With the repository's dev profile, an edit to the todo example rebuilds in about half a second. [docs/08](../08-fast-builds.md) estimates the whole loop, from the edit to the running app:

- 1 to 3 s for a small app crate in a tuned setup (docs/08's profile, a fast linker, small crates);
- 5 to 15 s untuned, mostly relinking;
- 10 to 30 s to a running app on an Android device (the Rust build, Gradle's packaging, installing and launching).

Keep the composables you're editing in a small crate, and stable code in other crates of the workspace.

## An Android app

compose-rs runs in a plain Android `View`, `ComposeRsView`. It can be an activity's whole content or one element of a Kotlin Compose screen (through `AndroidView`), and it's scrolled, clipped and transformed like any other view. The Rust code is a native library that Gradle packages with the app.

You need:

- a macOS or Linux machine to build on: `build-rust.sh` is a POSIX shell script;
- the Android SDK and NDK. The script finds the SDK through `ANDROID_HOME` (or `ANDROID_SDK_ROOT`), falling back to macOS's default location, and doesn't read Gradle's `local.properties`: on Linux, set `ANDROID_HOME`. It takes the NDK from `ANDROID_NDK_HOME`, or the newest under the SDK's `ndk/`;
- the Rust target: `rustup target add aarch64-linux-android`;
- JDK 17 or newer. The Gradle wrapper (`android/gradlew`) fetches Gradle 9.6, which loads the Android Gradle plugin 9.4 (whose built-in Kotlin support compiles the Kotlin) and the Kotlin Compose compiler plugin 2.4; Android Studio opens the `android/` project with the same wrapper.

Only arm64 devices are built for (64-bit phones and the arm64 emulator images). The minimum SDK is 29, compiled against SDK 36.

### 1. The Rust library

The Android library is a crate of its own, of type `cdylib`, depending on the app's library without its desktop feature, as `examples/android-demo` does. `examples/hello-android/Cargo.toml`:

```toml
[package]
name = "hello-android"
edition = "2024"

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
// examples/hello-android/src/lib.rs
compose_android::export_apps! {
    "hello" => hello::Hello,
}
```

### 2. Gradle

The Kotlin side is the `android/compose-rs` library module: `ComposeRsView` and its glue, without dependencies on AndroidX. Copy `android/demo` to `android/hello` and add `include(":hello")` to `android/settings.gradle.kts`, which already includes `:compose-rs`. The app's `build.gradle.kts` builds the Rust library before each build, with `build-rust.sh <cargo package> <library name> <jniLibs directory>`:

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

`build-rust.sh` runs `cargo build --release --target aarch64-linux-android -p <package>` with the NDK's linker (no `cargo-ndk` needed), then copies the library where Gradle packages it.

It builds in the compose-rs workspace, the directory above the script, so the package must be one of that workspace's members. An app in a repository of its own copies the script and changes the directory it builds in, and points Gradle at the library module: `project(":compose-rs").projectDir = file("…/compose-rs/android/compose-rs")`.

### 3. The activity

Load the library once per process, in the `Application` (declared with `android:name=".HelloApplication"` in the manifest):

```kotlin
class HelloApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        ComposeRsRuntime.loadLibrary("hello_android")
    }
}
```

Then create views by app name. In a plain activity:

```kotlin
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(ComposeRsView(this, "hello"))
    }
}
```

With `targetSdk` 35 and later, the window is edge to edge, so this view draws under the status and navigation bars. The view reports the window's insets to the app: pad its root with `Modifier.safe_drawing_padding()` (as the [interop sample](kotlin-to-rust.md#android-interop) does), or give Material's `Scaffold` its `content_window_insets`.

In a Kotlin Compose screen, through `AndroidView`:

```kotlin
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
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

Here Kotlin already keeps the content clear of the bars. The view still gets the whole window's insets, wherever it is, so the Rust app shouldn't pad for them again: the two paddings would add up.

What the view does for its app:

- It composes the app while it's attached to a window.
- It saves the app's `remember_saveable` values in its instance state, which needs a view id. It takes `R.id.compose_rs_view` unless it has one, so two views in one window need ids of their own.
- Back goes to the app's `BackHandler`s while one is enabled, then to the activity.
- Kotlin passes properties with `setProp` and hears events with `setOnEventListener` ([Android interop](kotlin-to-rust.md#android-interop)).

### 4. Build and run

The demo is a working template: `examples/android-demo` is its Rust library (the counter, todo and gallery examples) and `android/demo` its Kotlin app, which shows each example in a tab of a Kotlin Compose screen. From `android/`, with a device connected, `./gradlew :demo:installDebug` builds and installs it (`./gradlew :hello:installDebug` for the app above).

A Rust panic aborts the app at the JNI boundary; its message is in logcat under the tag `compose-rs`. [Logging](kotlin-to-rust.md#logging) covers the app's own messages, and [Profiling](performance.md#profiling) Perfetto traces of compose-rs's frame phases.
