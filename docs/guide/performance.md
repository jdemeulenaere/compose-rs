# Performance

compose-rs does the same three phases as Compose in each frame: composition, layout, drawing. Most of Compose's performance advice applies as is: read state as late as possible, key lazy items, keep arguments stable so composables skip. This page says what compose-rs does to keep frames short, what defeats it, and how to measure. [docs/12](../12-android-performance.md) measures it against Jetpack Compose on a Pixel 8 Pro.

## What's fast, and why

### Skipping

Every restartable composable can skip, as with Compose's strong skipping: when each argument is `same` as last time, its body doesn't run.

`ComposeArg::same` is always cheap:

- shared data (`Rc`, state handles, callbacks, `ImmutableList`) compares by identity;
- small values (numbers, short strings, small enums and `#[derive(ComposeArg)]` structs) compare by value;
- nothing compares a collection element by element.

So a skip costs one comparison per argument ([State](kotlin-to-rust.md#data-models-and-skipping)).

Lambdas keep their identity across passes. The transform remembers each lambda in a composable's calls and compares its captures; when they're all `same`, the call gets the lambda it got last time, and the callee can skip. This covers callbacks (`on_click = move || …`), content blocks and the callback methods of modifiers (`clickable`, `on_size_changed`, …).

### Fine-grained recomposition

A state read subscribes the scope that reads it, and a write recomposes those scopes only. A read inside a content block (`Button!(…, { Text!("{}", count.get()) })`) subscribes the composable that invokes the block (one inside `Button`), not the composable that wrote the block. Toggling one of 1,000 todos recomposes one row (`examples/todo`, and its test `toggling_one_item_recomposes_one_row`).

### Reads while laying out and drawing

State read while measuring (in a `Layout`'s policy or `Modifier.layout`) subscribes the node's layout, and state read while drawing (in `Canvas`, `draw_behind`, `draw_with_cache`'s drawing block) subscribes its drawing. A change then measures or draws again, without composing. This is Compose's deferred reads, and the most effective change to make to a screen whose state changes every frame:

<!-- snippet: src/performance.rs#read_late -->
```rust
#[composable]
pub fn Article(paragraphs: u32) {
    let scroll = remember_scroll_state!();
    Box!(modifier = Modifier.fill_max_size(), {
        Column!(modifier = Modifier.vertical_scroll(scroll), {
            for index in 0..paragraphs {
                Text!("Paragraph {}", index, modifier = Modifier.padding(16.dp()));
            }
        });
        // Give the child the holder (a Copy handle), not `scroll.value()`: read here, the value
        // would recompose `Article`, its whole column included, at each scrolled pixel.
        ProgressBar!(scroll);
    });
}

#[composable]
fn ProgressBar(scroll: ScrollState) {
    let color = MaterialTheme::color_scheme().primary;
    Box!(
        modifier = Modifier.fill_max_width().height(4.dp()).draw_behind(move |scope| {
            // Read while drawing: a scroll draws this bar again, and recomposes nothing.
            let fraction = scroll.value() as f32 / scroll.max_value().max(1) as f32;
            let size = scope.size();
            scope.draw_rect(
                Rect::new(0.0, 0.0, size.width * fraction, size.height),
                Paint::fill(color),
            );
        })
    );
}
```

A `graphics_layer` block is read while drawing too, and its properties (alpha, scale, rotation, translation, clip) apply to the layer's drawing without drawing it again. On Android each layer is a render node whose properties are set on it: an animation of a layer's properties records nothing, as Compose's `graphicsLayer { }` does.

<!-- snippet: src/performance.rs#deferred -->
```rust
#[composable]
pub fn LiveDot() {
    let transition = remember_infinite_transition("Live");
    let alpha = transition.animate_float(
        0.2,
        1.0,
        infinite_repeatable(tween(800).easing(LINEAR_EASING), RepeatMode::Reverse),
    );
    Box!(
        modifier = Modifier
            .size(12.dp())
            // Read while drawing, as `graphicsLayer { alpha = … }`: an animation frame sets the
            // layer's alpha, and nothing recomposes, measures or draws again.
            .graphics_layer(move |layer| layer.alpha = alpha.get())
            .background_with(Color::RED, CIRCLE_SHAPE)
            .test_tag("dot")
    );
    // Not `Modifier.alpha(alpha.get())`: a read in composition recomposes this scope each frame.
}
```

### Retained drawing and repaint boundaries

A frame doesn't record the whole screen's drawing again. Drawing is kept per repaint boundary, and a boundary draws again only when something in it changed: state its drawing read, a node in it measured or moved, a child added or removed. Graphics layers, lazy list items and top app bars are boundaries, and `Modifier.repaint_boundary()` makes one.

What the platform does with the kept drawing differs. On Android each boundary is a `RenderNode`, kept across frames and drawn by reference, so the GPU replays what didn't change. On desktop the CPU renderer rasterizes the whole window again for each frame that drew anything; what boundaries save there is the recording.

A boundary is worth adding around something that draws often (a progress indicator, a ticking clock) in an area that doesn't:

<!-- snippet: src/performance.rs#repaint_boundary -->
```rust
#[composable]
pub fn Syncing(items: u32) {
    Column!({
        Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
            // The spinner draws anew every frame; a boundary around it keeps the rest of the
            // screen's drawing (and, on Android, its render node) as it was.
            IndeterminateCircularProgressIndicator!(
                modifier = Modifier.size(16.dp()).repaint_boundary()
            );
            Text!("Syncing {} items", items);
        });
        for index in 0..items {
            Text!("Item {}", index);
        }
    });
}
```

Boundaries aren't free, so don't add one to every component. On Android each is a render node, and nested boundaries are nested render nodes: D251 made the top app bars boundaries, but kept other Material surfaces plain clips, as making every surface a boundary would nest a render node inside each lazy item's.

### Paths

A `Path` is mutable and shared, as Compose's: a clone is the same path. Drawing it records its contents as they are then (shared, nothing copied), and the platform keeps a path by identity: on Android, a path is defined to the replayer, which builds an `android.graphics.Path` for it, and defined again under the same id when it changes, the replayer refilling the same `android.graphics.Path` (D530–D532). As in Compose, changing a path draws nothing again: a drawing keeps what it drew until it runs again for its own reasons (state it read). So, as in Compose:

- **build a path that doesn't change once** and `remember` it: it's defined once;
- **remember a path that changes every frame, and reset it in the drawing block** (`remember(Path::new)`, then `path.reset()` and the new shape), as Compose's `remember { Path() }`. One path can draw several shapes in a block, reset between them: each drawing keeps the shape it drew.

<!-- snippet: src/performance.rs#paths -->
```rust
#[composable]
pub fn Heartbeat() {
    // A shape that doesn't change: built once and remembered, so the platform keeps it (Android
    // defines it once).
    let heart = remember(|| Path::from_svg("M12,21 L3,12 Q3,4 12,8 Q21,4 21,12 Z").unwrap());
    // A shape that changes every frame: one path, remembered and reset in the drawing block, as
    // Compose's `remember { Path() }` and `path.reset()` (Android refills its native path).
    let trace = remember(Path::new);
    let transition = remember_infinite_transition("Heartbeat");
    let phase = transition.animate_float(
        0.0,
        1.0,
        infinite_repeatable(tween(1000).easing(LINEAR_EASING), RepeatMode::Restart),
    );
    Canvas(Modifier.size(48.dp()).test_tag("heartbeat"), move |scope| {
        scope.draw_path_scaled(&heart, Offset::ZERO, (2.0, 2.0), Paint::fill(Color::RED));
        let size = scope.size();
        trace.reset();
        for point in 0..=24 {
            let along = point as f32 / 24.0;
            let wave = (std::f32::consts::TAU * (along + phase.get())).sin();
            let (x, y) = (size.width * along, size.height * (0.5 - 0.25 * wave));
            if point == 0 {
                trace.move_to(x, y);
            } else {
                trace.line_to(x, y);
            }
        }
        scope.draw_path(&trace, Paint::stroke(Color::WHITE, 2.0));
    });
}
```

### Lazy lists and prefetch

`LazyColumn`, `LazyRow`, `LazyVerticalGrid` and `LazyHorizontalGrid` compose and lay out only the items in view.

While a list scrolls, it prefetches between frames, as Compose's prefetch does: when the main thread is idle before the next frame, it composes and measures the next item in the scroll direction, so the frame that shows the item has nothing left to compose. On the Pixel 8 Pro this took the main thread's time per frame while scrolling, at the 99th percentile, from 8.6 ms to 3.9 ms ([D156](../decisions/lazy-and-prefetch.md#d156), [docs/12](../12-android-performance.md)). On Android, compose-rs's default optimizations, the `recommended` set ([README](../../README.md#parity-mode-and-the-optimizations), [D610](../decisions/android-host-and-performance.md#d610)), also draw and record the prefetched items in the same idle time (`draw_prefetched_ahead`), so that frame records nothing either; `none` turns them off, for Compose's work exactly.

Keys (`items_keyed`, `items_list_keyed`) keep an item's state and composition with the item when items before it come or go, so the unchanged items skip and the scroll position follows the first visible item.

Content types (`items_keyed_with(count, key, content_type, content)` and the other `_with` forms, Compose's `contentType`) let an item coming into view reuse the composition and nodes of an item of its type that left, as Compose's lazy layouts do: up to 7 items of each type are kept for reuse, their effects disposed, and the item reusing one runs its composables with its own remembered values, updating the nodes in place rather than creating them (D430). Give items that look alike the same content type and different layouts different ones: an item reusing one of another layout replaces most of its nodes, and the forms without a content type all share one. Scrolling the feed through 300 items headless, an item costs 16 % less (41.0 → 34.5 µs). Grids reuse item by item too, as Compose's: their `_with` forms take a content type (`items_keyed_with(count, key, content_type, content)`, and `_with_span` a span too), and a header over a whole line reuses another header, a cell another cell (D450). With the `grid_lines_beyond` optimization (not in the default set), a grid composes two lines beyond each side of its viewport (D456, D565).

### No garbage collection

State, slots and nodes are owned, and freed when they leave the composition. A steady frame allocates little, so there are no collector pauses. Strings shown as text are `SharedString`s, which share static strings and `Rc<str>`s without copying.

## What to avoid

- **Reading fast-changing state in composition.** A scroll offset, an animated value or a pointer position read in a composable's body recomposes that body every frame. Read it in a drawing, layout or `graphics_layer` block, or turn it into a coarser fact with `derived_state_of` (`first_visible_item_index() > 0`).
- **Rebuilding shared data each pass.** `Rc::new(…)` or `ImmutableList::from(vec)` in a composable's body is a new identity at each pass, so every composable given it runs again. Keep such data in state or `remember` it, and pass the same `Rc`.
- **Passing collections by value.** `Vec` isn't a `ComposeArg`, so a composable taking one doesn't compile as restartable. Use an `ImmutableList`, an `Rc`, or `#[compose_arg(eq)]` on a short field of a model.
- **Unkeyed items that move.** Without keys, an item's state goes with its index: inserting at the top recomposes every item below.
- **New measure policies and arrangements each pass.** `Layout!(measure_policy = measure_policy(…))` in a composable's body makes a new policy each time it runs, so the `Layout` can't skip. `remember` it, or `remember_keyed` it on what it uses ([Custom layouts](kotlin-to-rust.md#custom-layouts)).
- **Drawing and layout lambdas in scopes that recompose often.** The lambdas of `draw_behind`, `draw_with_content`, `draw_with_cache`, `layout`, `graphics_layer` and `Canvas` aren't memoized: each run of the composable around them gives the node a new lambda, which draws or measures it again. Read state inside the lambda rather than capturing values read in composition, and keep such nodes in small composables.
- **Large composables reading state at the top.** The reading scope is the unit of recomposition. Move a read into the smallest composable that needs it, or pass the state handle (or a holder such as `LazyListState`) down instead of its value.
- **Work in composition.** A composable's body can run on any frame. Compute derived values with `remember_keyed(keys, || …)` or `derived_state_of`, and do I/O in a `LaunchedEffect` with `spawn_background`.
- **Composable calls in a root closure.** `run_app`, `application` and `compose_test` take root content that the transform doesn't see. Give them a `#[composable]` function, or a closure that only calls one (`move || TodoAppWith(items)`): calls, branches and lambdas written in the closure itself get no memoization and no groups.
- **Measuring debug builds.** Profile release builds (`cargo run --release`). On Android, profile the release variant: its Rust library takes fat LTO and one codegen unit (`android-release`), the debug variant's doesn't (`android`, otherwise the release profile).

## Profiling

### Recomposition counts

The test rule counts how many times each composable's body ran, which is the first thing to check when a screen does more work than expected:

<!-- snippet: tests/suite/testing.rs#recompositions -->
```rust
#[test]
fn a_click_recomposes_only_the_button_content() {
    let mut ui = compose_test(state::Counter);
    let before = ui.recomposition_counts();
    ui.on_node_with_text("Clicked 0 times").perform_click();
    let ran = ui.recomposition_counts().since(&before);
    // The text reads the count inside the button's content: `Counter` itself skips.
    assert_eq!(ran.scopes_named("Counter"), 0, "{:?}", ran.names());
}
```

`recomposition_counts().names()` lists every composable that ran, with its count. `compose::test` also re-exports the runtime's counters: `bodies_run`, `live_state_count` (state cells alive, for leaks) and `recomposition_counts`.

### Frame timings on desktop

`COMPOSE_FRAME_STATS=1 cargo run --release -p gallery` prints a line per frame: whether it recomposed (and how many bodies ran), measured and drew, and the time spent composing (with layout and drawing), rendering with Skia, presenting, and from the input that caused the frame to its presentation.

### Perfetto on Android

compose-rs marks its frame phases as trace sections, which show in Perfetto and Android Studio's system trace beside the system's own (`Choreographer#doFrame`, `DrawFrame`):

| Section | Thread | What |
|---|---|---|
| `compose-rs: frame` | main | the whole Rust side of a frame |
| `compose-rs: tasks` | main | tasks and effects due this frame, frame callbacks (animations) |
| `compose-rs: recompose` | main | recomposition |
| `compose-rs: layout` | main | measuring and placing |
| `compose-rs: text layout` | main | laying out paragraphs with `StaticLayout` |
| `compose-rs: recompose and layout again` | main | content composed for what a layout asked (lazy items, `BoxWithConstraints`) |
| `compose-rs: draw` | main | drawing the boundaries that changed |
| `compose-rs: serialize` | main | encoding the frame's drawing for the Kotlin side |
| `compose-rs: take drawing` | main | handing the frame's drawing to the Kotlin side in `onDraw` |
| `compose-rs: replay` | main | the Kotlin side recording it into render nodes (`onDraw`) |
| `compose-rs: prefetch` | main | lazy list items composed and measured between frames |

Record a trace of a release build with Perfetto's UI (record on the device, with the `gfx` and `view` atrace categories and the app's package for its sections), or with `android/bench/measure.py`, which drives the benchmark apps and summarizes frames, jank, CPU time and memory. [docs/12](../12-android-performance.md) explains the method, including why CPU time misleads on big.LITTLE cores and what to compare instead.

### Benchmarks

`cargo bench -p compose-benchmarks` runs criterion benchmarks of the runtime (a counter click, a toggle among 1,000 rows, skipping 1,000 children, keyed moves) and of frames (a click to a headless frame, a toggle of one of 1,000 todos, the same rendered with Skia at 2×). `benchmarks/benches/ui.rs` is a template for an app's own: it drives the app through a `ComposeTestRule` and times each frame from the event to the end of drawing.
