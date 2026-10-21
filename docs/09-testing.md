# 9. Testing (`compose-test`)

## A `ComposeTestRule` for Rust

```rust
#[test]
fn counting_to_three() {
    let mut ui = compose_test(|| Counter());                 // headless host, fixed-advance text measurer
    for _ in 0..3 {
        ui.on_node(has_text_containing("Count")).perform_click();         // finds through semantics
    }
    ui.on_node_with_text("Count: 3").assert_exists();
    println!("{}", ui.dump_tree());                            // node tree with layout
}
```

- **Running.** `compose_test` owns a `Composition` and a headless host, and runs frames synchronously: an action dispatches the event, recomposes, lays out and draws before returning.
- **The main clock.** The rule switches the thread's executor to a manual clock, as Compose's `mainClock`. Waiting for idle moves it on its own (`mainClock.autoAdvance`): each frame a task waits for (`with_frame_nanos`, an animation) comes 16 ms after the last, and when only `delay`s are left the clock jumps to the next one's end, until the UI is idle (a minute of test time at most, then the test fails: an animation or a task that never ends). `ComposeTestRule::with_manual_clock` starts with it off, and `advance_time_by`, `advance_time_by_frame` and `current_time_millis` drive it, as `mainClock.advanceTimeBy`: frames come every 16 ms, and each `delay` ends at its own deadline in between. A frame's own animation steps don't ask for another frame: an animation waits for the clock.
- **Recomposition counts.** `recomposition_counts()` snapshots how many times each composable's body ran (the runtime counts them while tracking is on, which the rule turns on); `since(&before).scopes_named("TodoRow")` checks what a click recomposed.
- **Finders and actions.** Finders go through the semantics tree as in Compose testing: `on_node_with_text`, `on_node_with_tag`, `on_node_with_content_description`, `on_all_nodes…`. Actions: `perform_click`, `perform_semantics_action`, `perform_text_input`, `perform_key`, `resize`.
- **Dumps.** Node tree, layout, display list, slot table, and a PNG through Skia's CPU raster backend. They are compared as plain strings, and event scripts with their `.script.expected` transcripts.

## Test suites

- **Runtime scenarios.** `compose-runtime` is tested on its own, before the macro exists, with composables written by hand against the composer API (what the macro would generate). That includes a **model-checked randomized test**: random insertions, removals and moves at every position, keyed moves, removed and invalid scopes, each pass checked against a reference model, and every value released exactly once on disposal.
- **UI and app scenarios.** `compose-test` tests for every UI rule and for the example apps. **Event scripts** (`click "Count: 0"`, `type "milk"`, `frame`, `dump`) give a compact text format for app sessions, stored beside each example with their expected dumps.
- **Macro tests.** Golden tests of `#[composable]` expansions (unit tests in `compose-macros`, `src/golden.rs`, against `tests/expand/*.expanded.rs`, D46), compile-fail tests (with `trybuild`) for every rejected construct and every missing-`ComposeArg` diagnostic.

## Memory and performance checks

- **A counting `#[global_allocator]`** in test binaries. Steady-state frames must allocate nothing, live allocations must stay constant across 1,000 clicks or toggles, and everything must be freed when a composition is dropped: cells, slots, nodes.
- **Runtime benchmarks** (`criterion`), with these targets:
  - a counter click under 1 ms;
  - a 1,000-item toggle recomposing one row, under 4 ms frame p99 headless;
  - a slot-table operation O(1) amortized;
  - a skipped composable costing one comparison per argument.
- **Compile-time benchmarks** (planned). Compile speed is a design criterion, so it's measured: generate crates with 256, 1,024 and 4,096 composables, branches and lambdas, measure `cargo build` time and macro expansion size, and assert near-linear growth.
- **The edit loop** (planned). A benchmark builds the sample app, touches one composable, and times the incremental rebuild, with and without dynamic linking of the framework. It guards the 1–3 s target of [08](08-fast-builds.md).

## Android

- **Replayer tests** on the JVM: the Kotlin replayer is fed recorded display lists, with a mock `Canvas` checking the calls.
- **Instrumented tests** (emulator, opt-in in CI): a `ComposeRsView` inside a Kotlin Compose screen, driven with Compose's own `ComposeTestRule`, checking input, the soft keyboard, accessibility nodes and screenshots against the desktop goldens, allowing for platform text differences.
- **The JNI budget:** a test counts crossings per frame, expecting one for the frame and one per input batch, plus text measurements only for changed texts.

## Windows

Real-window tests are planned, opt-in (`COMPOSE_WINDOW_TESTS=1`); today the desktop crate's tests open a window for a smoke test only. They feed input through the window's own event handling and check pixels and frame timing. Posting real OS input needs the Accessibility permission on macOS, which only the user can grant.
