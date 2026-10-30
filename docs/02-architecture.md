# 2. Architecture

## Crates

```
compose (facade: re-exports the modules below as compose::runtime, compose::ui, ...)
├── compose-macros            proc macros: #[composable], key!, the per-composable call macros
├── compose-runtime           slot table, composer, groups, remember, recompose scopes,
│                             state and observation, effects, Composition, Applier
├── compose-ui                node tree, Modifier and Modifier.Node, Layout/MeasurePolicy,
│                             the TextMeasurer and Renderer interfaces, drawing (display
│                             list), input, focus, semantics, UiHost (the host core every
│                             platform wraps, and the headless host), dumps
├── compose-foundation        Box, Column, Row, Spacer, Arrangement, padding/size/fill,
│                             weight/align (scopes), clickable, toggleable, background,
│                             border, shapes, BasicText, BasicTextField  (module
│                             foundation::layout mirrors androidx.compose.foundation.layout)
├── compose-material3         Text, Button, Checkbox, TextField, Surface, MaterialTheme
├── compose-skia              Skia backends (skia-safe): renderer (GPU and CPU raster) and
│                             SkParagraph text measurer; used on desktop and in tests
├── compose-desktop           winit window, Skia GPU surfaces, run_app
├── compose-android           ComposeRsView host: display-list replayer, StaticLayout text
│                             measurer, input, IME and accessibility bridges (a cdylib plus
│                             a Kotlin AAR, embeddable in Kotlin Compose with AndroidView)
└── compose-test              ComposeTestRule-like API over the headless host, event scripts
```

In the repository the crates live under `crates/`, and are added as the roadmap reaches them ([10](10-roadmap.md)). `benchmarks/` holds the criterion benchmarks; it isn't a default workspace member, so `cargo build` and `cargo test` don't compile criterion.

Mirroring Compose's artifacts as crates (`runtime`, `ui`, `foundation`, `material3`) keeps API ownership clear and helps build times: an app edit recompiles only the app crate, and the libraries rarely change. There's no dynamic linking for development: it saved too little to keep (D304, [08](08-fast-builds.md)).

## Dependencies between them

```
compose-macros ── (generated code calls) ──► compose-runtime
compose-ui ──► compose-runtime
compose-foundation ──► compose-ui ──► compose-runtime
compose-material3 ──► compose-foundation
compose-animation ──► compose-ui
compose-foundation ──► compose-animation   (the scrolls' animations, D820, D882)
compose-skia ──► compose-ui
compose-desktop ──► compose-skia
compose-android ──► compose-ui
compose-test ──► compose-skia
compose-test ──► compose-ui
```

`compose-runtime` knows nothing about UI, like Compose's own runtime. It talks to a tree through the `Applier` trait.

## Threads

- **One composition per thread at a time.** The current composer is a thread-local. A pass runs to completion on that thread, and state read during composition registers the thread's current scope. This is Compose's default for one `Recomposer`.
- **The UI thread** runs events, handlers, recomposition, layout and drawing. On macOS, winit needs the main thread, so `run_app` runs the event loop there and the composition on that same thread. One thread means no frame mailbox, input channel or cross-thread wake-ups to get wrong.
- **Background work** (later): effects launched with an async executor post results back to the UI thread, which then writes state.

## Public API shape

- **Composables are functions** marked `#[composable]`, named in PascalCase as in Compose (the macro allows `non_snake_case`). Each also gets a call macro of the same name for named, default and trailing arguments (`Button!(…)`); see [03](03-composable-macro.md).
- **Modules mirror Compose packages:** `compose::runtime::{remember, mutable_state_of, key, SideEffect, DisposableEffect, …}`, `compose::ui::{Modifier, Alignment, Color, Layout, …}`, `compose::foundation::{clickable, background, …}`, `compose::foundation::layout::{Column, Row, Box, …}`, `compose::material3::{Button, Text, …}`.
- **Names follow Rust conventions where Rust requires them:** snake_case functions and methods (`mutable_state_of`, `on_click`, `fill_max_width`), PascalCase types and composables. `Modifier.padding(…)` is spelled exactly as in Kotlin: `Modifier` is both a type and a `const` value of that type, which Rust allows because types and values live in different namespaces.
