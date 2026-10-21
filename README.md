# compose-rs

A design for bringing Jetpack Compose's model to Rust: positional memoization over a slot table, a compiler-style transform that makes composables skippable and restartable, observable state, `Modifier.Node`-style modifiers, and Compose's layout model and API names, on desktop and Android.

**Status:** phases A to F of the [roadmap](docs/10-roadmap.md) are implemented: the runtime (`crates/compose-runtime`), the `#[composable]` transform with call macros and lambda memoization (`crates/compose-macros`), the UI core (`crates/compose-ui`), the toolkit (`crates/compose-foundation`, `crates/compose-material3`), the test rule (`crates/compose-test`), Skia rendering and text (`crates/compose-skia`), desktop windows (`crates/compose-desktop`), and the counter and todo apps (`examples/`, `cargo run -p todo`). Android (phase G) needs the Android NDK. `cargo test` runs everything; `cargo bench -p compose-benchmarks` runs the runtime and frame benchmarks (toggling one of 1,000 todos: about 0.14 ms a headless frame, about 1 ms from input to Skia-rendered pixels at 2×; an edit to the todo app rebuilds in about 0.5 s).

| Crate | Contents |
|---|---|
| `crates/compose` | the facade: `compose::runtime`, `ui`, `foundation`, `material3`, and with features `test` and `desktop` |
| `crates/compose-runtime` | slot table, composer, restart scopes, state, `remember`, effects, composition locals, `Composition` |
| `crates/compose-macros` | `#[composable]` (skipping, restarts, call macros, lambda memoization), `#[derive(ComposeArg)]` |
| `crates/compose-ui` | layout nodes, `Modifier`, `Layout`, drawing, pointer and key input, focus, semantics, `UiHost` |
| `crates/compose-foundation` | `Box`, `Column`, `Row`, `Spacer`, `Arrangement`, `BasicText`, `BasicTextField`, scrolling, shapes |
| `crates/compose-material3` | `MaterialTheme`, `Text`, `Surface`, buttons, `Checkbox`, `TextField` |
| `crates/compose-test` | `ComposeTestRule`: finders, actions, assertions, event scripts |
| `crates/compose-skia` | Skia renderer and SkParagraph text measurer |
| `crates/compose-desktop` | `run_app`: a winit window on the main thread |
| `examples/counter`, `examples/todo` | the sample apps, with tests and event-script transcripts |
| `benchmarks` | criterion benchmarks (not a default member) |

## Getting started

```bash
cargo run -p todo
```

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

`COMPOSE_FRAME_STATS=1` prints each frame's timings, and `COMPOSE_SCREENSHOT=out.png` writes the last frame when the app exits.

## Verdict

**Feasible, with a clear cost profile.** Every part of Compose has a workable Rust equivalent. The genuinely hard part is a proc-macro transform (`#[composable]`) standing in for Compose's compiler plugin. The costs are the edit loop (no hot reload: an incremental Rust build of a few seconds, kept fast by design), a transform that sees syntax rather than types, and a few places where Rust's syntax and borrow rules shape the API. See [docs/01-feasibility.md](docs/01-feasibility.md).

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

Every decision with its alternatives and revert cost is in [docs/decisions.md](docs/decisions.md).

## Principles

- **Follow Jetpack Compose**: its APIs, parameter names, order and defaults, and its module split (`runtime`, `ui`, `foundation`, `foundation.layout`, `material3`), adapted only where Rust forces it.
- **Look and behave like Compose on desktop and Android**: draw and lay out text with the same Skia-based stacks Compose uses on each platform, and embed in Kotlin Compose apps on Android.
- **No composed modifiers**: modifiers are plain data elements whose state lives in UI nodes (`Modifier.Node`); no equivalent of `Modifier.composed { }`.
- **Strong skipping**: no stability inference; parameters compare by identity or cheap value equality, never by deep comparison.
- **Compile speed matters**: the transform is syntactic and its expansions stay small, calling into non-generic runtime code.
- **Zero GC**: ownership is explicit. Shared immutable data uses `Rc`, which is visible and idiomatic in Rust. State uses arena handles, not reference cycles.
