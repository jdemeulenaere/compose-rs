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

- **Running.** `compose_test` owns a `Composition` and a headless host, and runs frames synchronously. `ComposeTestRule::empty(host)` then `set_content(content)` is `createComposeRule()` with a later `setContent`, for a test that sets the clock up first.
- **The main clock** (D900). The rule switches the thread's executor to a manual clock, as Compose's `mainClock`, and `current_time_millis` counts from 0. Frames come on the test clock as Compose's `TestMonotonicFrameClock` gives them: one frame duration (16 ms) after something first needed one since the last frame (a recomposition, a task in `with_frame_nanos`). Work that needs no recomposition (tasks, layout, drawing, focus) runs at once, as Compose's measure and layout passes run outside the test clock. So a state written at 100 ms shows at 116 ms, and an action's recomposition is a frame later.
  - **Waiting for idle** (`wait_for_idle`, which actions and finders do) runs what's due and, while a frame is needed and the clock advances by itself (`set_auto_advance`, as `mainClock.autoAdvance`, on by default), moves the clock a frame at a time; it fails after a minute of test time. Pending `delay`s don't keep it busy: advance the clock past them. `ComposeTestRule::with_manual_clock` (or `set_auto_advance(false)` before `set_content`) leaves frames to the test.
  - **Advancing.** `advance_time_by(duration)` rounds up to whole frames, as `advanceTimeBy(ms)`; `advance_time_by_ignoring_frame_duration` doesn't; `advance_time_by_frame`; `advance_time_until(condition)` (1 s of test time by default). The timers and frames due on the way run in time order, each at its own time.
  - **Waiting until** a condition holds: `wait_until`, `wait_until_with(description, timeout, …)`, `wait_until_node_count`, `wait_until_at_least_one_exists`, `wait_until_exactly_one_exists`, `wait_until_does_not_exist` (counting checks where Compose counts real time); `run_on_idle`, `run_on_ui_thread`, `run_without_implicit_wait`.
  - **Infinite animations** (D901) don't run while the clock advances by itself, as Compose's test `InfiniteAnimationPolicy`: an infinite transition's or an `infinite_repeatable` animation's task is cancelled at its first frame, so the UI idles. With the clock advanced by hand they run.
- **Finders** go through the semantics tree, merged or not (`_unmerged`), waiting for idle first: `on_node`, `on_node_with_text`/`_tag`/`_content_description`, `on_root`, `on_all_nodes…` (a collection: `get(i)`, `on_first`, `on_last`, `filter`, `filter_to_one`, `fetch_semantics_nodes`, `assert_count_equals`, `assert_all`, `assert_any`). From a node: `on_parent`, `on_child`, `on_children`, `on_child_at`, `on_sibling(s)`, `on_ancestors`, `on_descendants`.
- **Matchers** see a node in its tree (D905): `has_text`, `has_text_with(text, substring, ignore_case)`, `has_text_exactly`, `has_content_description…`, `has_test_tag`, `has_role`, `has_state_description`, `has_progress_bar_range_info`, `is_enabled`, `is_toggleable`, `is_on`/`is_off`, `is_selectable`, `is_selected`, `is_focusable`, `is_focused`/`is_not_focused`, `is_dialog`, `is_popup`, `is_editable`, `has_click_action`, `has_scroll_action`, `has_set_text_action`, `has_request_focus_action`, `is_root`, `has_parent`, `has_any_ancestor`/`child`/`descendant`/`sibling`, combined with `&`, `|`, `!`; `Matcher::with_node(description, |node| …)` for others.
- **Assertions:** `assert_exists`, `assert_does_not_exist`, `assert(matcher)`, the states (`assert_is_enabled`, `_on`, `_selected`, `_focused`, `_toggleable`, `assert_has_click_action`…), `assert_text_equals` (as sets) and `assert_text_contains`, `assert_content_description_equals`/`_contains`, `assert_value_equals`, `assert_range_info_equals`; `assert_is_displayed`/`assert_is_not_displayed` (placed, clipped by the clips around, inside the window); bounds in dp with a tolerance (0.5 dp, or the rule's `set_bounds_assertion_tolerance`): `assert_width_is_equal_to`, `assert_height_is_equal_to`, `_is_at_least`, `assert_position_in_root_is_equal_to`, `assert_left_`/`assert_top_position_in_root_is_equal_to` (each with a `_with_tolerance` form), `assert_touch_width`/`height_is_equal_to`, `get_unclipped_bounds_in_root`, `get_bounds_in_root`, `Dp::assert_is_equal_to`.
- **Actions** send real input events at their times on the test clock (D903): `perform_click` (a touch click: down, a move 16 ms later, up), `perform_touch_input(|touch| …)` (`down`/`down_pointer`, `move_to`, `move_by`, `up`, `cancel`, `click`, `double_click`, `long_click`, `swipe`, `swipe_up`/`down`/`left`/`right`, `swipe_with_velocity`, `multi_touch_swipe`, `pinch`, `advance_event_time`, and the node's geometry: `center`, `top_left`, `percent_offset`…), `perform_mouse_input` (`move_to`, `press`/`release`, `click`, `double_click`, `long_click`, `enter`/`exit`, `scroll`, `smooth_scroll`, `animate_move_to`, `drag_and_drop`), `perform_key_input` (`key_down`/`key_up`, `press_key`, `with_key_down`, `with_key_toggled`, held keys repeating, `is_shift_down`, the lock keys), `perform_semantics_action`, `perform_scroll_to`, `perform_text_input`, `perform_ime_action`, `request_focus`, `perform_key_press`.
- **Pixels** (D904): `capture_to_image()` draws the node's window (the main content, or the popup's or dialog's it's in) with Skia's raster renderer and crops it to the node's clipped bounds; `CapturedImage::assert_pixels`, `assert_pixel_color` (each channel within 0.02), `assert_contains_color`. It needs compose-test's `skia` feature, which the facade's `desktop` and `test` features enable together.
- **Recomposition counts.** `recomposition_counts()` snapshots how many times each composable's body ran (the runtime counts them while tracking is on, which the rule turns on); `since(&before).scopes_named("TodoRow")` checks what a click recomposed.
- **Dumps.** `print_to_string` (Compose's format), node tree, layout, display list, slot table, and a PNG through Skia's CPU raster backend. They are compared as plain strings, and event scripts with their `.script.expected` transcripts.

What's still missing from Compose's ui-test, and why, is in the [ui-test ledger](parity/ui-text-and-test.md) (its harness features).

## Test suites

- **Runtime scenarios.** `compose-runtime` is tested on its own, before the macro exists, with composables written by hand against the composer API (what the macro would generate). That includes a **model-checked randomized test**: random insertions, removals and moves at every position, keyed moves, removed and invalid scopes, each pass checked against a reference model, and every value released exactly once on disposal.
- **UI and app scenarios.** `compose-test` tests for every UI rule and for the example apps. **Event scripts** (`click "Count: 0"`, `type "milk"`, `frame`, `dump`) give a compact text format for app sessions, stored beside each example with their expected dumps.
- **The guide's samples** (D266). The developer guide quotes its Rust samples from a crate of their own, `examples/guide` (a default member, so `cargo test` compiles and runs them), by region: `// ANCHOR: name` … `// ANCHOR_END: name` in a source, and `<!-- snippet: src/file.rs#name -->` before the fenced block quoting it. Its tests fail when a quote differs from its region, a region isn't quoted, or a `rust` block has no marker; `UPDATE_GUIDE=1` rewrites the quotes. Samples standing for an app's own crates (`hello`) name the guide's modules through a `use … as hello` outside the quoted region. Kotlin, Gradle and TOML excerpts aren't compiled.
- **Retained drawing.** A randomized test (`crates/compose-android/tests/retained_groups.rs`, `randomized_inner_layers`: 40 seeds × 120 steps, more with `INNER_LAYER_SEEDS` and `INNER_LAYER_STEPS`) checks each incremental frame's bytes against a fresh host's (D522); `released_paragraphs.rs` checks that no kept node draws a reused paragraph (D195).
- **Macro tests.** Golden tests of `#[composable]` expansions (unit tests in `compose-macros`, `src/golden.rs`, against `tests/expand/*.expanded.rs`, D46), compile-fail tests (with `trybuild`) for every rejected construct and every missing-`ComposeArg` diagnostic.

## Memory and performance checks

- **A counting `#[global_allocator]`** in test binaries. Steady-state frames must allocate nothing, live allocations must stay constant across 1,000 clicks or toggles, and everything must be freed when a composition is dropped: cells, slots, nodes.
- **Runtime benchmarks** (`criterion`), with these targets:
  - a counter click under 1 ms;
  - a 1,000-item toggle recomposing one row, under 4 ms frame p99 headless;
  - a slot-table operation O(1) amortized;
  - a skipped composable costing one comparison per argument.
- **Compile-time benchmarks** (planned). Compile speed is a design criterion, so it's measured: generate crates with 256, 1,024 and 4,096 composables, branches and lambdas, measure `cargo build` time and macro expansion size, and assert near-linear growth.
- **The edit loop** (planned). A benchmark builds the sample app, touches one composable, and times the incremental rebuild. It guards the 1–3 s target of [08](08-fast-builds.md).

## Android

- **Replayer tests** on the JVM: the Kotlin replayer is fed recorded display lists, with a mock `Canvas` checking the calls.
- **Instrumented tests** (emulator, opt-in in CI): a `ComposeRsView` inside a Kotlin Compose screen, driven with Compose's own `ComposeTestRule`, checking input, the soft keyboard, accessibility nodes and screenshots against the desktop goldens, allowing for platform text differences.
- **The JNI budget:** a test counts crossings per frame, expecting one for the frame and one per input batch, plus text measurements only for changed texts.

## Windows

Real-window tests are planned, opt-in (`COMPOSE_WINDOW_TESTS=1`); today the desktop crate's tests open a window for a smoke test only. They feed input through the window's own event handling and check pixels and frame timing. Posting real OS input needs the Accessibility permission on macOS, which only the user can grant.
