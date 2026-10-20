# 8. A fast edit-build loop

compose-rs has **no hot reload**: an edit is an incremental Rust build followed by a restart of the app. Hot patching native Rust code is experimental, can't change the layout of live data, and would add a layer of fragile machinery, so the design leaves it out. Instead, the framework is built to keep incremental builds fast, and this page tells app developers how to keep their own loop in the 1–3 second range.

## What to expect

Rough times for a small app on a recent laptop:

| Scenario | Time | What dominates |
|---|---|---|
| First (cold) build, debug | ~1–3 min | compiling every dependency once; Skia is downloaded prebuilt, not compiled |
| **Edit one composable, rebuild, restart: tuned as below** | **~1–3 s** | rustc on the app crate (~0.5–1.5 s), a small link, app start |
| Same, untuned | ~5–15 s | relinking Skia, winit and all debug info into one binary on every change |
| Release build after an edit | ~20–60 s+ | optimization; for benchmarks and shipping only |
| Android, edit to running on a device | ~10–30 s | the Rust build, Gradle packaging, install and launch |

These are estimates for this design's shape; the edit-loop benchmark ([09](09-testing.md)) measures them for real.

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

### 4. Link the framework dynamically in dev builds

The biggest single win. With the `dynamic` feature of the `compose` crate, the framework and its heavy dependencies (Skia, winit) are built once as a shared library, so each edit relinks only your small app binary. This is the technique Bevy's `dynamic_linking` feature uses.

```toml
[features]
dev = ["compose/dynamic"]     # cargo run --features dev
```

Release builds and Android builds link statically, as usual.

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

- **Small macro expansions.** `#[composable]` emits calls into shared, non-generic runtime functions, never large inline code ([03](03-composable-macro.md)). The call macros are declarative `macro_rules!`, which expand cheaply.
- **Few generics in the hot paths.** Content lambdas become `Rc<dyn Fn()>`, and runtime APIs take `&dyn Any` or plain words, so an app doesn't instantiate large generic code.
- **Skia prebuilt.** `skia-safe` downloads binaries; no C++ compilation in a normal build.
- **Measured in CI.** Compile-time benchmarks assert near-linear growth with app size, and an edit-loop benchmark tracks rebuild time after touching one composable, with and without dynamic linking ([09](09-testing.md)).
