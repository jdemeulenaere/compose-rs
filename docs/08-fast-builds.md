# 8. A fast edit-build loop

compose-rs has **no hot reload**: an edit is an incremental Rust build followed by a restart of the app. Hot patching native Rust code is experimental, can't change the layout of live data, and would add a layer of fragile machinery, so the design leaves it out. Instead, the framework is built to keep incremental builds fast, and this page tells app developers how to keep their own loop in the 1–3 second range.

## What to expect

Rough times for a small app on a recent laptop:

| Scenario | Time | What dominates |
|---|---|---|
| First (cold) build, debug | ~1–3 min | compiling every dependency once; Skia is downloaded prebuilt, not compiled |
| **Edit one composable, rebuild, restart: tuned as below** | **~1–3 s** | rustc on the app crate (~0.5–1.5 s), a small link, app start |
| Same, untuned | ~5–15 s (estimated; not on macOS, see below) | relinking Skia, winit and all debug info into one binary on every change with a slow linker |
| Release build after an edit | ~20–60 s+ | optimization; for benchmarks and shipping only |
| Android, edit to running on a device | ~10–30 s | the Rust build, Gradle packaging, install and launch |

These are estimates for this design's shape. Measured on an M4 Pro ([14](14-performance-opportunities.md), "Build times"): an edit to examples/todo rebuilds in about 0.45 s, a clean debug build of it takes 8.6 s, and Apple's linker links the static 28 MB binary in 0.12 s, so on macOS linking doesn't dominate even untuned.

## For app developers

### 1. Keep the code you edit in a small crate

Rust recompiles whole crates. Put the UI you're actively changing in a small app crate, and move stable parts (models, data access, shared components) into separate crates in the same workspace. An edit then recompiles only the small crate:

```
my-app/
├── Cargo.toml          # workspace
├── app/                # screens and composables you edit all day: keep it small
├── components/         # your shared composables: change rarely
└── model/              # data and logic: no UI dependencies
```

### 2. Tune the dev profile

```toml
# Cargo.toml (workspace root)
[profile.dev]
opt-level = 0
debug = "line-tables-only"    # enough for backtraces; much less to write and link
incremental = true

[profile.dev.package."*"]
opt-level = 1                 # dependencies are built once; a little optimization keeps the app snappy
debug = false
```

`package."*"` also applies to the dependencies of build scripts and proc macros, and outranks `[profile.dev.build-override]`. Code that only runs during the build gains nothing from optimizing it, so list the big ones at opt-level 0, as compose-rs's workspace does for skia-bindings' build tools (bindgen, regex, clang-sys, cc and others: a clean debug build of todo went from 11.8 to 8.6 s, D303). Keep syn, quote and proc-macro2 at 1: they run in every `#[composable]` expansion.

```toml
[profile.dev.package.bindgen]
opt-level = 0
```

On macOS, also set `split-debuginfo = "unpacked"` (the default on recent toolchains), so debug info isn't copied into the binary on each link.

### 3. Use a fast linker

Linking is often most of an untuned rebuild.

```toml
# .cargo/config.toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]   # or lld

[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"
```

On macOS, Apple's current linker (Xcode 15 and later) is already fast; nothing to do.

### 4. Dynamic linking: not provided

Bevy's `dynamic_linking` builds the framework as a shared library, so an edit relinks only the app. compose-rs has no such feature (D304): a prototype (`perf/build-dynamic`, a `compose-dylib` crate) saved only 50–80 ms per app edit on macOS (todo 0.45 → 0.39 s) and cost about 0.5 s per framework edit (a 39 MB library to relink). Linking the static binary is a small share of an edit on macOS (0.12 of 0.45 s); elsewhere, use a fast linker (3).

### 5. Rebuild and relaunch on save

`bacon` or `cargo watch -x run` rebuilds and relaunches the app whenever a file is saved. `cargo check`, and rust-analyzer in the editor, report errors without building at all, much faster than a full build.

### 6. Iterate on one component at a time

To work on a single composable, run it alone rather than the whole app: a small example binary (`cargo run --example todo_row`) or a focused headless test with a screenshot snapshot (`cargo test todo_row`). Both build and start faster than the full app, and need no navigating back to the right screen after each restart.

### 7. Develop on desktop, verify on Android

The Android loop pays for Gradle packaging and installing on top of the Rust build, so iterate on desktop, where the same composables run, and switch to the device to check platform behaviour. When you do build for Android in development, build only one ABI (`cargo ndk -t arm64-v8a`) and keep Gradle's configuration cache on.

Two optional extras:
- The Cranelift code generator for debug builds (`-Zcodegen-backend=cranelift`, nightly) cuts code-generation time further.
- `cargo build --timings` shows where a build spends its time.

## Commitments of the framework

The framework keeps its share of every build small:

- **Small macro expansions.** `#[composable]` emits calls into shared, non-generic runtime functions, never large inline code ([03](03-composable-macro.md)). The call macros (`Name!`) are declarative `macro_rules!` that forward to the `call!` proc macro.
- **Few generics in the hot paths.** Content lambdas become `Rc<dyn Fn()>`, and runtime APIs take `&dyn Any` or plain words, so an app doesn't instantiate large generic code.
- **Skia prebuilt.** `skia-safe` downloads binaries; no C++ compilation in a normal build.
- **One test binary per crate.** Each crate's integration tests are modules of one binary (`tests/suite/main.rs`), so a change links one test executable per crate rather than one per file: after touching `compose-ui`, rebuilding the workspace's tests went from 87 s to 28 s. Only tests that need a process of their own stay apart: the allocation counters (a `#[global_allocator]`), the `trybuild` compile-fail cases, and the desktop shell's.
- **Measured in CI.** Compile-time benchmarks assert near-linear growth with app size, and an edit-loop benchmark tracks rebuild time after touching one composable ([09](09-testing.md)).
