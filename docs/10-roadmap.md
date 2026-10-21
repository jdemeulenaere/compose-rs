# 10. Roadmap

Each phase is independently testable and ends with something that runs. The order front-loads the risky parts: the runtime and the macro come first, before any UI.

| Phase | Delivers | Done when |
|---|---|---|
| **A. Runtime** | `compose-runtime`: slot table, groups, composer, restart scopes, deferred removals, `Composition`, node groups and a built-in node tree | runtime scenarios and the randomized model test pass against hand-written "expanded" composables; steady-state passes allocate nothing |
| **B. Transform** | `#[composable]`: keys, restart and replace groups, region groups, kept arguments, `ComposeArg` and its derive, skipping, `fn` restart pointers; `remember`, `key`, `mutable_state_of`, `MutableState`, `OwnedState`, `SideEffect` | expansion goldens and compile-fail tests; composables written normally pass the phase A scenarios; a skipped composable costs one comparison per argument |
| **C. Ergonomics** | lambda memoization (captures, autoref fallback), call macros (named, default, trailing and format arguments), `DisposableEffect` | the counter and todo bodies compile as in [the sketches](../sketches); a 1,000-row toggle recomposes one row (counted in the slot-table dump) |
| **D. UI core** | `compose-ui`: `LayoutNode`, `UiApplier`, `Modifier` and `Modifier.Node` diffing, `Layout`/`MeasurePolicy`, the text and renderer interfaces, display list, pointer, focus, keyboard, semantics, headless host, `compose-test`, event scripts | UI rule tests; layout and draw dumps; allocation checks |
| **E. Toolkit and apps** | `foundation`, `foundation::layout` (`Box`, `Column`, `Row` as measure policies), `material3`; `examples/counter`, `examples/todo` (with a 1,000-item screen), a toolkit tour | app sessions passing with their dumps; headless frame targets met |
| **F. Desktop** | `compose-skia` and `compose-desktop`: Skia renderer (GPU and CPU raster), SkParagraph measurer, winit, HiDPI, input mapping, screenshots, frame statistics, on one thread; the `dynamic` feature and the edit-loop benchmark | the apps in real windows; 0% idle CPU; input-to-present under 16 ms p99 on the 1,000-item list; an edit to one composable rebuilt and relaunched in under 3 s with the tuned setup of [08](08-fast-builds.md) |
| **G. Android** | `compose-android`: `ComposeRsView`, the display-list replayer on the system `Canvas`, `StaticLayout` measurer, touch, keys, soft keyboard, accessibility, lifecycle; the AAR and `cargo-ndk` build | the counter and todo apps embedded in a Kotlin Compose screen with `AndroidView`, transformed and scrolled with it; one JNI crossing per frame plus input; a todo toggle under 16 ms on a mid-range phone |
| **H. Beyond** | `LaunchedEffect` with an executor, `derived_state_of`, `CompositionLocal`, `LazyColumn`, scrolling, AccessKit on desktop, iOS and web | as each lands (done: `LaunchedEffect`, the executor and frame clock, `derived_state_of`, `CompositionLocal`, scrolling, `LazyColumn`/`LazyRow`, animations, popups and dialogs, images, AccessKit on desktop, shadows, right-to-left layout) |

**The first milestone:** after G, the counter and todo apps, written in Compose-style Rust, run natively in a desktop window and embedded in a Kotlin Compose app on Android, looking as Compose does on each, with skipping that recomposes one row per toggle, a few-second edit-build loop, and no leaks.

## Decided with the user

- Named, default and trailing arguments through call macros, first ([03](03-composable-macro.md)).
- Skipping compares by identity first, and by contents only for small values ([03](03-composable-macro.md)).
- Skia for drawing on desktop, Android's system `Canvas` on Android, and each platform's own text stack behind `TextMeasurer` ([07](07-platforms.md)).
- On Android, a plain `View` embeddable in Kotlin Compose, with no Skia shipped ([07](07-platforms.md)).
- No hot reload: a fast incremental build and restart instead ([08](08-fast-builds.md)).

## Open questions

1. **Scoped modifiers as tokens** (`Modifier.weight(row, 1.0)`) versus unchecked `Modifier.weight(1.0)` with a runtime misuse error outside `Row`/`Column`.
2. **The naming split:** `MutableState<T>` for the `Copy` handle and `OwnedState<T>` for the owner, or the reverse (`MutableState` as the owner, `StateHandle` as the handle).
3. **Publishing:** crate names (`compose-*` may be taken on crates.io), and whether to publish the runtime on its own.
4. **Cranpose:** build this design from scratch, or contribute its differences (lambda memoization, identity-first comparisons, structural keys, call macros, platform rendering) to Cranpose.
