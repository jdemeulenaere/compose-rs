# 1. Feasibility

## Verdict

Feasible. Every piece of Compose's design has a workable Rust equivalent. The genuinely hard piece is **the transform**: Compose's compiler plugin becomes a proc macro, `#[composable]`. It sees syntax but not types, so where the Kotlin plugin infers, it delegates decisions to traits that rustc resolves.

The main costs are build latency, with no hot reload, and a few places where Rust's syntax and borrow rules make the API differ from Kotlin's. Hot reload is deliberately left out: patching native Rust code in a running app is experimental and fragile. The design keeps incremental builds fast instead ([08](08-fast-builds.md)).

## How each part of Compose maps to Rust

| Compose | Rust design | Difficulty |
|---|---|---|
| Compiler plugin rewriting `@Composable` | `#[composable]` proc macro, syntactic; types handled through traits | Medium-high |
| `$composer` parameter | thread-local current composer | Low |
| Group keys from source positions | declaration path plus structural position, computed by the macro as `const` hashes | Low |
| Slot table, groups, deferred removals | a generational arena tree of keyed groups, with Rust-native slot values (`Box<dyn Any>`) | Medium |
| `$changed` bits, skipping | each callee compares its arguments with its kept ones through a `ComposeArg` trait | Medium |
| Restart scopes | a `fn` pointer plus a slot of kept arguments | Medium |
| Lambda memoization (strong skipping) | the macro finds a closure's captures syntactically and remembers it keyed on them | Medium |
| `mutableStateOf`, snapshots (observation) | `Copy` generational handles plus `OwnedState<T>` for state inside data | Low-medium |
| Named and default arguments | a call macro per composable (`Button!(on_click = …)`) | Medium |
| Trailing lambdas | the last argument of the call macro is a block | Low |
| `Modifier` chains, `Modifier.Node` | an ordered chain of plain-data elements diffed into long-lived nodes | Low-medium |
| `Layout`, `MeasurePolicy`, `Column`, `Row` | `Layout` + `MeasurePolicy` trait; `Column` and `Row` written on it, as in Compose | Medium |
| Text | behind a `TextMeasurer`: Skia's paragraph module on desktop, Android's `StaticLayout` on Android, the stacks Compose uses on each | Medium |
| Rendering | a display list replayed by Skia on desktop and by Android's system `Canvas` on Android | Medium |
| Embedding in Android apps (`AndroidView`) | `ComposeRsView`, a plain `View` drawing in `onDraw`, with a thin Kotlin layer for input, keyboard and accessibility | Medium |
| Live Edit, hot reload | **not supported**; fast incremental rebuilds instead ([08](08-fast-builds.md)) | — |
| Testing (`ComposeTestRule`) | a headless host and a Rust test API with event scripts | Low-medium |

## Kotlin versus Rust: what changes for the programmer

| | Compose in Kotlin | This design in Rust |
|---|---|---|
| Transform | type-aware compiler plugin: knows which calls are composable, parameter types and captures | syntactic: composable calls recognised by name, stability through traits, captures found by a scope-aware walk |
| Named, default and trailing arguments | native | through a generated call macro (`Column!(…)`); plain calls with every argument also work |
| Borrowed or shared parameters | any object reference, compared by identity | a borrow can't be kept across passes: composables taking `&T` are non-restartable (Compose's `@NonRestartableComposable`), and data to skip on is passed as `Rc<T>` or state handles |
| Handlers capturing state | capture the state object | capture a `Copy` state handle; a handle to freed state is detected (a panic or `None`), never undefined behaviour |
| Memory | garbage collected | owned: remembered values and state are freed when their group leaves or their owner is dropped |
| Edit loop | Live Edit, previews | rebuild and restart: about 1–3 s for a small app with a tuned setup ([08](08-fast-builds.md)) |
| Threads | snapshots allow composition off the main thread | one composition per thread; state handles are `!Send`, so cross-thread misuse is a compile error |

**Compile speed is the main cost.** Mitigations: keep the macro's expansions small (calls into shared non-generic runtime functions), avoid generics in the hot paths (content lambdas become `Rc<dyn Fn()>`), split the libraries into crates that change rarely, link them dynamically in development, and document how apps keep their own loop fast ([08](08-fast-builds.md)).

## Prior art

| Project | Model | What it has / lacks against this design |
|---|---|---|
| [compose-rt](https://crates-io.ganymede-static.progval.net/crates/compose-rt/0.9.1) | positional memoization runtime modelled on Compose's | has: a slot-table-style runtime. Lacks: a transform macro, skipping, restart scopes, a UI layer |
| Dioxus | React-like components and hooks, virtual DOM, Subsecond hot patching | hooks are positional, like `remember`; it renders by diffing a virtual tree, not by node groups with skipping, and its APIs follow React rather than Compose |
| Leptos, Sycamore | fine-grained signals, arena `Copy` signals | the `Copy`-handle state ergonomics this design borrows; no slot table or composition |
| [Cranpose](https://github.com/samoylenkodmitry/Cranpose) | Jetpack Compose in Rust: `#[composable]` proc macro, thread-local composer, region groups, kept arguments compared with `PartialEq`, `Copy` state handles, `Modifier.Node`-style chains, full snapshots, wgpu rendering, desktop and mobile | **the closest prior art, and far more mature**: it validates the core of this design at scale. It differs on: location-based keys (structural only under hot reload), `PartialEq` comparisons, closures always counting as changed (no lambda memoization), spec structs instead of named arguments, and its own renderer rather than the platforms' Skia stacks |
| Xilem, Masonry (Linebender) | view-tree diffing over a retained widget tree | a good reference for widgets, text (Parley) and rendering (Vello); alpha |
| Freya, Floem, iced, egui | various retained or immediate models | none follows Compose's runtime model |

Apart from Cranpose, none of them provides Compose's combination of a slot table, restartable and skippable composables, `Modifier.Node` and Compose's API. Against Cranpose, this design's additions are: lambda memoization (so composables taking handlers skip), identity-first comparisons, structural keys in every build, call macros for Kotlin-like arguments, owned state inside data, and drawing and text through the platforms' own Skia-based stacks for fidelity to Compose.

## Effort

A rough size:

| Part | Contents | Estimate |
|---|---|---|
| `compose-runtime` | slot table, composer, restart scopes, state, effects, with a model-checked randomized test | 5–7k lines |
| `compose-macros` | keys, groups, skipping, restart, lambda memoization, call macros | 3–4k lines |
| `compose-ui` | node tree, modifiers, layout, text and renderer interfaces, display list, input, headless host | 8–10k lines |
| foundation, layout, material3 | Compose's toolkit composables in Rust | 3–4k lines |
| `compose-skia`, `compose-desktop` | Skia renderer and paragraph measurer, window, input mapping | 3k lines |
| `compose-android` | replayer, `StaticLayout` measurer, input, keyboard and accessibility bridges, Rust and Kotlin | 3–4k lines |
| tests, examples, benchmarks | | 5k+ lines |

That's roughly 33–43k lines. The macro is where most of the risk and time will go; the runtime and UI layers follow well-understood designs.

## Risks

1. **The macro's syntactic limits.** Recognising composable calls by name (a PascalCase function or a `!`-call) and finding captures syntactically can be wrong in edge cases: shadowing, macros inside closures. Mitigation: be conservative; a wrong guess may cost a skip but must never break correctness. See [03](03-composable-macro.md).
2. **The edit loop.** Without hot reload, every change is a rebuild and restart, and an untuned setup can take 5–15 s. Mitigation: the framework's own build rules, dynamic linking in development, and a guide for apps ([08](08-fast-builds.md)); an edit-loop benchmark watches it.
3. **Ergonomics under the borrow checker.** Handlers are `'static`, so they capture `Copy` handles or `Rc`s, never borrows. `Copy` state handles make the common case clean; the rest needs a visible `.clone()` or `.handle()`.
4. **IDE support for call macros.** rust-analyzer expands proc and declarative macros, but completion inside `Button!(…)` is weaker than in a plain call. Mitigation: every composable stays callable as a plain function with all its arguments.
