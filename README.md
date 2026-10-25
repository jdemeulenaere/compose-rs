# compose-rs

A design for bringing Jetpack Compose's model to Rust: positional memoization over a slot table, a compiler-style transform that makes composables skippable and restartable, observable state, `Modifier.Node`-style modifiers, and Compose's layout model and API names, on desktop and Android.

> **Disclaimer:** this is not an officially supported Google product, and it is not affiliated with or endorsed by the Jetpack Compose team. Jetpack Compose and Android are trademarks of Google LLC.

**Status:** the runtime, UI, foundation, Material 3, animation and navigation run on desktop (Skia) and Android (`ComposeRsView`, embeddable in Kotlin Compose). By default compose-rs does what Jetpack Compose 1.12.1 does; on a Pixel 8 Pro it then starts faster, uses less memory and does about half the main-thread work per frame, and with its own optimizations on it is ahead at every frame percentile ([docs/12](docs/12-android-performance.md)). [docs/11](docs/11-status.md) lists what's covered and the known differences from Compose.

| Crate | Contents |
|---|---|
| `crates/compose` | the facade crate apps depend on (`compose::runtime`, `ui`, `foundation`, `material3`, …) |
| `crates/compose-runtime` | slot table, composer, recomposition, state, effects, composition locals |
| `crates/compose-macros` | `#[composable]` and the call macros |
| `crates/compose-ui` | layout nodes, modifiers, drawing, input, focus, semantics, `UiHost` |
| `crates/compose-foundation` | layouts, text, images, scrolling, lazy lists and grids |
| `crates/compose-material3` | Material 3 components and theme |
| `crates/compose-animation` | animations and transitions |
| `crates/compose-navigation` | `NavController` and `NavHost` |
| `crates/compose-test` | `ComposeTestRule` and event scripts |
| `crates/compose-skia` | Skia renderer and text, for desktop |
| `crates/compose-android` | the Android bridge behind `ComposeRsView` (`android/`) |
| `crates/compose-desktop` | desktop windows (winit, AccessKit) |
| `examples/` | the sample apps (counter, todo, gallery, the benchmark feed) and the guide's samples |
| `benchmarks` | criterion benchmarks |

## Getting started

### Desktop apps

```bash
cargo run -p todo      # also: counter, gallery
```

On Linux, Skia links the system's FreeType and Fontconfig, so install their development packages first:

```bash
sudo apt install build-essential pkg-config libfreetype-dev libfontconfig1-dev   # Debian, Ubuntu
sudo dnf install gcc-c++ pkgconf-pkg-config freetype-devel fontconfig-devel      # Fedora
sudo pacman -S base-devel freetype2 fontconfig                                     # Arch
```

### Android apps

Needs the Android SDK (`ANDROID_HOME`, or `~/Library/Android/sdk` on macOS), JDK 17 or newer, and an arm64 device connected with adb. The Gradle wrapper (`android/gradlew`) fetches Gradle itself, and builds the Rust library with `android/build-rust.sh`, which needs the NDK and Rust's Android target:

```bash
"$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" --install "ndk;27.2.12479018" platform-tools   # or Android Studio's SDK Manager: SDK Tools > NDK (Side by side)
rustup target add aarch64-linux-android
```

The NDK version is pinned: the apps' `ndkVersion` is 27.2.12479018, the NDK the Android Gradle plugin strips the Rust library with (without it, it packages the library unstripped). `build-rust.sh` links with the NDK `ANDROID_NDK_HOME` names, else that version under `$ANDROID_HOME/ndk/`, else the newest there, with a warning. Debug variants build the library with Cargo's `android` profile, release variants with `android-release` (fat LTO, one codegen unit: smaller and slightly faster, slower to rebuild); in both, a Rust panic aborts the app after it's logged to logcat (docs/decisions.md, D361).

```bash
android/gradlew -p android :demo:installDebug    # the gallery, todo and counter in Kotlin Compose tabs
adb shell am start -n rs.compose.demo/.MainActivity
```

### Tests and benchmarks

```bash
cargo test                                           # everything
cargo bench -p compose-benchmarks                    # runtime and frame benchmarks (criterion)
```

The Android comparison with Jetpack Compose ([docs/12](docs/12-android-performance.md)) installs both versions of the feed app, then measures them with Perfetto; `measure.py` needs a [trace_processor_shell](https://perfetto.dev/docs/analysis/trace-processor) binary:

```bash
android/gradlew -p android :bench-compose:installRelease :bench-rs:installRelease
python3 android/bench/measure.py --trace-processor path/to/trace_processor_shell --out bench-out
python3 android/bench/summarize.py bench-out/report.json
```

`--scenarios stress_startup,stress_scroll,stress_animation` measures only the stress screens ([docs/15](docs/15-android-stress-benchmark.md)).

The [developer guide](docs/guide/README.md) walks through a desktop app and an Android app.

An app depends on the facade with the `desktop` feature (and `test` for its tests), writes composables, and opens a window:

```toml
[dependencies]
compose = { path = "crates/compose", features = ["desktop"] }

[dev-dependencies]
compose = { path = "crates/compose", features = ["desktop", "test"] }
```

Tests drive the app headless through Compose's test-rule API:

```rust
use compose::test::*;

#[test]
fn counting_to_three() {
    let mut ui = compose_test(Counter);
    for _ in 0..3 {
        ui.on_node(has_text_containing("Count")).perform_click();
    }
    ui.on_node_with_text("Count: 3").assert_exists();
}
```

`COMPOSE_FRAME_STATS=1` prints each frame's timings, and `COMPOSE_SCREENSHOT=out.png` writes the last frame when the app exits. compose-rs draws graphics layers, clips and shadows as Compose does; its own optimizations beyond that are all off by default (next section).

## What user code looks like

```rust
use compose::runtime::*;
use compose::ui::*;
use compose::foundation::layout::*;
use compose::material3::*;

#[composable]
fn Counter() {
    let count = remember(|| mutable_state_of(0));
    Column!(modifier = Modifier.padding(16.dp()), vertical_arrangement = Arrangement::spaced_by(8.dp()), {
        Button!(on_click = move || count.update(|c| c + 1), {
            Text!("Count: {}", count.get());
        });
        Button!(on_click = move || count.set(0), enabled = count.get() > 0, {
            Text!("Reset");
        });
    });
}

fn main() {
    use compose::desktop::{WindowOptions, run_app};
    run_app(WindowOptions { title: "Counter".into(), size: (320, 200).into(), ..Default::default() }, Counter);
}
```

The [examples](examples) are this code, running. The design started from [sketches/](sketches): the [counter](sketches/counter.rs), [what `#[composable]` expands it to](sketches/counter.expanded.rs), and the [todo list](sketches/todo.rs) with per-item state and skipping rows.

## Developer guide

[docs/guide](docs/guide/README.md) is for Android developers who know Jetpack Compose: getting started on desktop and Android, [Kotlin and compose-rs side by side](docs/guide/kotlin-to-rust.md), [performance](docs/guide/performance.md), and the [differences from Compose](docs/guide/differences.md) an app will meet. Its samples compile and run as tests in `examples/guide`.

## Reading order

1. [Feasibility](docs/01-feasibility.md): verdict, how each part of Compose maps to Rust, prior art, effort, risks.
2. [Architecture](docs/02-architecture.md): crates, layering, threads, public API shape.
3. [The `#[composable]` transform](docs/03-composable-macro.md): keys, groups, skipping, restart scopes, lambda memoization, named arguments.
4. [Runtime](docs/04-runtime.md): slot table, composer, recomposition, `remember`, effects, compositions.
5. [State](docs/05-state.md): `MutableState` handles, owned state in data, observation, safety.
6. [UI](docs/06-ui.md): node tree, modifiers, layout, text, drawing, input, the module split.
7. [Platforms and rendering](docs/07-platforms.md): Skia on desktop, Android's `Canvas` and text stack on Android, embedding in Kotlin Compose.
8. [A fast edit-build loop](docs/08-fast-builds.md): no hot reload; how apps keep incremental rebuilds to seconds.
9. [Testing](docs/09-testing.md): test rule, headless runs, event scripts, dumps, benchmarks.
10. [Roadmap](docs/10-roadmap.md): phases and what each delivers.
11. [Status](docs/11-status.md): what's covered, and the known differences from Compose.
12. [Android performance](docs/12-android-performance.md): the same app in Jetpack Compose and compose-rs, measured with Perfetto.
13. [Drawing on Android](docs/13-android-drawing-paths.md): why compose-rs draws through `Canvas` and `RenderNode`, what `libhwui.so` exposes, and the options for drawing directly.
14. [Performance opportunities](docs/14-performance-opportunities.md): what was tried, measured and ranked, including the optimizations behind flags.
15. [Android stress benchmark](docs/15-android-stress-benchmark.md): a heavy list and a screen of hundreds of animations, in both, for jank and memory.
16. [Backporting to Compose](docs/16-backporting-to-compose.md): compose-rs's optimizations that Jetpack Compose or HWUI could adopt, with the evidence and a sketch of each change.

Every decision, with its alternatives and revert cost, is in [docs/decisions.md](docs/decisions.md); see [docs/02](docs/02-architecture.md) for how the crates fit together.

## Principles

- **Follow Jetpack Compose**: its APIs, parameter names, order and defaults, and its module split (`runtime`, `ui`, `foundation`, `foundation.layout`, `material3`), adapted only where Rust forces it.
- **Look and behave like Compose on desktop and Android**: draw and lay out text with the same Skia-based stacks Compose uses on each platform, and embed in Kotlin Compose apps on Android.
- **No composed modifiers**: modifiers are plain data elements whose state lives in UI nodes (`Modifier.Node`); no equivalent of `Modifier.composed { }`.
- **Strong skipping**: no stability inference; parameters compare by identity or cheap value equality, never by deep comparison.
- **Compile speed matters**: the transform is syntactic and its expansions stay small, calling into non-generic runtime code.
- **Zero GC**: ownership is explicit. Shared immutable data uses `Rc`, which is visible and idiomatic in Rust. State uses arena handles, not reference cycles.

## Parity mode and the optimizations

By default compose-rs does the work Jetpack Compose 1.12.1 does: Compose's prefetch and its time budget, its layers, clips and shadows, and its text relayout rule (D560–D569). The default is parity: no flags, the `performance_hint` session included (D581). Everything compose-rs does beyond Compose is a flag an app opts in to, `all` for every one.

Each flag's effect below was measured alone against parity on a Pixel 8 Pro, 5 runs a side ([docs/14](docs/14-performance-opportunities.md), "Backport candidates"; [docs/16](docs/16-backporting-to-compose.md) explains the main ones). "No change" means within the runs' spread.

| Flag | What it does | Measured alone |
|---|---|---|
| `performance_hint` | A hint session of its own for the main thread and the RenderThread, reporting both threads' CPU time (Android) | Scrolls: every percentile on both threads 24–46 % shorter; CPU-rail power +1.8 to +2.9 % (D500) |
| `draw_prefetched_ahead` | Draws and records prefetched items between frames (Android) | Scrolls: main thread p90 / p99 13–23 % shorter |
| `single_drawing_fade` | Fades a layer holding one drawing by its paint, not offscreen (as `ModulateAlpha`) | Stress animation: RenderThread work −42 %; GPU caches +31 % unless the next flag is on too |
| `flush_layers_before_dropped_paths` | Flushes HWUI's layers before a frame that drops a stroked path (Android) | Frees a frame's worth of path masks: −28 MB of graphics memory without offscreen layers (D510) |
| `reuse_paragraph_at_same_layout_width` | Keeps a text's paragraph while the width it's laid out at stays the same | Width-changing screen: 14 times fewer text layouts, main-thread work −52 % (D551); no change elsewhere |
| `keep_items_behind` | Keeps two items behind the scroll composed, with their recordings | Faster first frame on a fling reversal (D375); main thread p50 +5 % |
| `prefetch_two_ahead` | Prefetches two items ahead, not one | No change in frame durations |
| `prefetch_budget_recent_maximum` | Budgets prefetch steps by their recent maximum, with a margin, on the vsync's phase | Fewer steps past the vsync (D341, D381); no change in round 14 |
| `prefetch_at_rest` | Prefetches after a layout at rest too, not only on scrolls | No change |
| `grid_lines_beyond` | Composes and places two grid lines beyond each side | Not measured (no grid scenario) |
| `paragraph_render_nodes` | Records each paragraph into a render node of its own (Android) | A cost on the feed: main thread p90 +11 % |

A set is named the same way everywhere: `none`, `all`, `default`, or names separated by commas, where `-name` takes one out of what comes before it (`all,-performance_hint`). `default` and an empty list are the defaults: none, the same as `none`.

**Android.** `ComposeRsView` takes the flags as bits of `ComposeRsOptimizations` (`DEFAULT`, `NONE`, `ALL`, one constant per flag), as a constructor argument or through its `optimizations` property. `ComposeRsOptimizations.parse` reads a list, and needs the library loaded first:

```kotlin
ComposeRsRuntime.loadLibrary("my_app")
val view = ComposeRsView(context, "main", ComposeRsOptimizations.ALL)
view.optimizations = ComposeRsOptimizations.parse("performance_hint,draw_prefetched_ahead")
view.optimizations = ComposeRsOptimizations.PERFORMANCE_HINT or ComposeRsOptimizations.DRAW_PREFETCHED_AHEAD
```

**Desktop.** `run_app` takes `WindowOptions::optimizations`. `application`'s windows take what `compose::desktop::set_optimizations` set last. The `COMPOSE_OPTIMIZATIONS` environment variable overrides both (`COMPOSE_OPTIMIZATIONS=all cargo run -p todo`).

```rust
use compose::desktop::{Optimizations, WindowOptions, run_app};
run_app(WindowOptions { optimizations: Optimizations::all(), ..Default::default() }, App);
```

**Rust hosts and tests.** `UiHost::set_optimizations` sets a host's flags. `compose::test::use_optimizations` sets them for the test rules made on the calling thread:

```rust
use compose::ui::platform::{Optimization, Optimizations};
host.set_optimizations(Optimizations::parse("all").unwrap());
compose::test::use_optimizations(Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true));
```

**The bench app and measure.py.** bench-rs takes a list as an intent extra:

```bash
adb shell am start -n rs.compose.bench.rs/.MainActivity -e screen feed -e optimizations all
```

measure.py's `--optimizations` takes `parity` (the default: `none`, the library's default), `all`, `each` (each flag alone against parity, interleaved), or names.

**Which to pick.** `all` is the best measured case: with it, compose-rs is ahead of Compose at every percentile on both threads, on every screen measured ([docs/12](docs/12-android-performance.md), [docs/15](docs/15-android-stress-benchmark.md)). Its costs are the hint session's CPU power (above) and the idle-time work of recording prefetched items ahead. If you pick flags one by one, turn on `flush_layers_before_dropped_paths` together with `single_drawing_fade`: the fade alone grows HWUI's GPU caches by 31 % on a screen whose stroked paths change every frame.

## Verdict

**Feasible, with a clear cost profile.** Every part of Compose has a workable Rust equivalent. The genuinely hard part is a proc-macro transform (`#[composable]`) standing in for Compose's compiler plugin. The costs are the edit loop (no hot reload: an incremental Rust build of a few seconds, kept fast by design), a transform that sees syntax rather than types, and a few places where Rust's syntax and borrow rules shape the API. See [docs/01-feasibility.md](docs/01-feasibility.md).
