# 15. Android stress benchmark: compose-rs and Jetpack Compose under load

[12](12-android-performance.md) compares the two on a light social feed. This document compares them on two much heavier screens, built the same way in both apps (D400): a list of rich, heterogeneous items, and a screen running hundreds of animations at once. It measures jank, frame durations on both threads, work, memory and startup with the same tools as [12](12-android-performance.md) (D401).

**Short answer** (Pixel 8 Pro, Android 16; round 10 of [12](12-android-performance.md), compose-rs at b3ef0c4 with lazy items reused by content type (D430) and the main thread in its own hint session (D440); medians of 3 runs, startup of 15):

- **Scrolling the stress list**, neither app janks much: Compose missed 4 app deadlines in 3 runs, compose-rs none.
  - compose-rs's main-thread frames are now shorter than Compose's at every percentile: p50 1.02 against 1.29 ms, p99 2.86 against 7.76 ms, for 43 % less work. Since D440 its main thread is in compose-rs's hint session and runs 21 % of its time on the little cores (83 % in round 9, when its p50 was 1.67 ms).
  - Its RenderThread is a little faster: p50 2.49 against 2.63 ms. In round 9 it was 2.20 ms: compose-rs's session voted higher in that session.
- **The stress animation** keeps both RenderThreads busy for most of a 120 Hz vsync (8.33 ms): Compose's frames exceed it at p90, compose-rs's at p99.
  - Neither app had a janky frame. In earlier sessions, each had a few, depending on the session.
  - compose-rs's frames are shorter on both threads: main thread p50 2.24 against 3.59 ms, RenderThread p50 6.44 against 7.39 ms. Its main thread does half Compose's work since its transitions run one task each (D420).
  - Most of the RenderThread gap is one Compose default: a `graphicsLayer` whose alpha is below 1 draws offscreen (`CompositingStrategy.Auto`). compose-rs fades a layer holding a single drawing without one. With `CompositingStrategy.ModulateAlpha`, Compose's RenderThread matched compose-rs's in its session and did less work.
- **Memory**: on the list, compose-rs holds less: 97.8 against 109.3 MB after scrolling.
  - Round 10's 1.0 to 1.5 MB more native heap than round 9 was items the list's first layout composed beyond those it wanted, kept for reuse since D430; they're disposed again (D460): native heap 18.1 → 17.1 MB after start and 22.3 → 20.4 MB after scrolling, PSS 97.1 → 94.8 MB ("Memory" below).
  - In the animation, after 13 s, it held 210 against Compose's 196 MB this round (204 against 226 MB in round 9). Compose's swings with its garbage collector (191 to 285 MB in this round's runs), compose-rs's less (195 to 215 MB).
  - In the first results compose-rs held 405 MB there: its 50 waves are new paths every frame, and each frame's paths, with the masks the GPU cached for them, lived until the garbage collector freed the Kotlin `Path`s. The replayer now rewinds a dropped path at once and rebuilds it for the next, as Compose's one reset `Path` does (D410).
- **Startup** of the stress list: 126 against 145 ms.

## The screens

`examples/feed/src/stress.rs` (compose-rs) and `android/bench-compose/…/Stress.kt` (Kotlin Compose) are written widget for widget.

**`stress`** is a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 keyed items, with a sticky header ("Section n") before every 20. An item is one of four kinds, by its index modulo 4:

- **A carousel:** a title and an icon over a nested `LazyRow` of 12 keyed tiles. A tile is a 120 × 90 dp `Image` (`ContentScale.Crop`, rounded 12 dp clip) with a caption.
  - The images are 8 procedural 240 × 180 bitmaps: a diagonal gradient between two palette colors with lighter rings, computed by the same integer arithmetic on both sides.
  - They are made once, on first use.
- **A rich post:** an `ElevatedCard` holding:
  - a 44 dp circular avatar filled with a linear gradient, with a number;
  - the author, the time and a `MoreVert` icon;
  - an `AnnotatedString` of 4 lines at most, with bold, italic, colored medium-weight, underlined and highlighted (background) spans;
  - a row of 4 icons with counts.
- **An action card:** an `OutlinedCard` holding a title and an `AssistChip` with a leading icon, a `FilterChip` and a `SuggestionChip`. Below them, a determinate `LinearProgressIndicator`, then a `Button`, an `OutlinedButton` and a percentage.
- **A banner:** a 120 dp box with a 6 dp `shadow` and a rounded 16 dp clip, filled with a three-color linear gradient. A title and a line of text sit over it, and three icons in a corner.

Each item has several times a feed post's nodes. A carousel composes and lays out its own lazy list.

**`stress_animation`** is a `Scaffold` and `TopAppBar` over a column of five parts:

- **A ticker:** a text changing every frame ("Frame n · t ms"), written from a `withFrameMillis` loop and read in composition. It recomposes every frame.
- **A grid of 140 cells (10 × 14), each with its own infinite transition.**
  - Each cell animates a rotation (linear, restarting), a scale and an alpha (reversing), and a color, each with its own duration by index.
  - All four are read while drawing: `graphicsLayer { rotationZ; scaleX; scaleY; alpha }` and `drawBehind { drawRoundRect(color) }` (compose-rs: `graphics_layer` and `draw_behind`).
  - That makes 560 animated values.
- **Six bars** whose widths animate.
  - Each width is read while measuring (`Modifier.layout`), so it relayouts without recomposing.
  - Each bar has a percentage beside it, from `derivedStateOf`, which recomposes when the integer changes.
- **A card that grows to four lines and shrinks back to one every second.** It toggles from a `delay(1000)` loop, with `animateContentSize(tween(500))` on its column. This is a layout animation that moves what's below it.
- **A 140 dp `Canvas` of 50 sine waves**, each a 48-segment stroked path with its own amplitude, frequency and speed. The waves are recomputed from an infinite transition read while drawing.

The two apps' screenshots, side by side, match except for Material version details. These were compared at the top of the list, mid-scroll with a header stuck, and in the animation:

- Kotlin's `OutlinedButton` label is `onSurfaceVariant` where compose-rs's is `primary`.
- Kotlin's chip and card outlines are lighter.
- compose-rs's shadow is a blurred outline ([11](11-status.md)).
- compose-rs sits below the status bar, as on the feed.

## What differs, and why it's fair

D400 records the choices. A fresh reviewer checked the two implementations before measuring:

- **The same viewport.** Kotlin's list takes the scaffold's padding as a modifier (`Modifier.padding(padding)`), not as `contentPadding`. The edge-to-edge Kotlin app would otherwise compose, lay out and draw items under the bars, about 110 dp more of the list. A sticky header would also slide under the top bar. The feed in [12](12-android-performance.md) keeps `contentPadding`.
- **Compose's best practices.** These are what a skilled Compose developer writes:
  - keys on both lazy lists, and `contentType` on the outer one;
  - stable parameters, and `remember(index)` for the `AnnotatedString`;
  - deferred reads in `graphicsLayer { }`, `drawBehind`, `Modifier.layout` and `Canvas`;
  - `derivedStateOf` for the percentages, and one `Path` remembered and reset for each wave.
- **`contentType` on both.** compose-rs reuses items as Compose does since D430, by the same content types (`sticky_header_keyed_with`, `items_keyed_with`). "Results" below (round 10) is the first measurement with it; the sections after it predate it, when compose-rs composed each new item afresh where Compose reused the composition of an item of the same kind.
- **The progress indicator.** Kotlin's `LinearProgressIndicator` is given `gapSize = 0.dp` and an empty `drawStopIndicator`. That draws a track and a line, as compose-rs's does, rather than Material 3 1.4's two segments and stop dot.
- **The alpha of the grid's cells.** Kotlin keeps the default `CompositingStrategy.Auto`, under which HWUI draws a layer faded below 1 offscreen.
  - compose-rs fades a layer whose content is one drawing without an offscreen buffer (`setHasOverlappingRendering(false)`, D153 and D157).
  - `ModulateAlpha`, Compose's opt-in for the same thing, was measured separately (D402, "With `ModulateAlpha`" below).
- **Infinite transitions.** In the first results below, compose-rs ran one `Animatable` and one coroutine per animated value (`animate_value`). That made 567 tasks a frame, and the ticker's. Compose runs one frame loop per transition (147) and ticks its animations together. This was a real cost of the library, measured as such. Since D420, compose-rs's transitions run one task each too ("Infinite transitions, one task each" below).
- **Paths drawn anew each frame.** compose-rs's `Canvas` makes a new `Path` per wave per frame. There is no way to reuse one: a drawn path is a shared, immutable resource (`Arc<Path>`, D412). So the display list defines 50 paths and drops 50 a frame. Kotlin resets one `Path`. In the first results below, the replayer made an `android.graphics.Path` for each, the bridge's cost, measured as such (see "Memory"); it now rebuilds the dropped ones (D410).
- **Clips and shadows.**
  - Compose makes a render node (a graphics layer) for each non-rectangular `clip` and each `shadow`, and HWUI draws the shadow from the elevation.
  - compose-rs clips inline in its drawing, and draws its approximate shadow with a `BlurMaskFilter`.
- **The theme.** Both apps use the light theme. The Kotlin app always does; compose-rs follows the system's, which was light.

## Method

measure.py's stress scenarios (D401), beside [12](12-android-performance.md)'s, which are unchanged:

- **`stress_startup`:** cold starts of `stress` (`am start -W`: `TotalTime`), `--runs` times, interleaved, and one traced start for the first frame's breakdown.
- **`stress_scroll`:** `scroll`'s method on `stress`. That is the memory 5 s after start, then a warm-up swipe left out, 10 flings 1.2 s apart, the memory after them, and 3 runs per app, interleaved.
- **`stress_animation`:** `animation`'s method on `stress_animation`: 10 s untouched, 3 s after start, 3 runs per app. Each run also records the memory before the trace (3 s after start) and after it (13 s).

`--scenarios stress_startup,stress_scroll,stress_animation` runs these alone; the report holds the scenarios run (`"scenarios"`), and `summarize.py` prints what's there.

The stress traces get a 384 MB buffer, against 64 MB for the feed's. HWUI traces every drawing operation of the animation's 140 layers, about 110,000 slices a second. A 64 MB ring buffer wrapped within 5 s and lost the process table scanned at the start, and with it every frame.

Every traced run now also lists its 5 slowest `doFrame`s and `DrawFrame`s, with the 8 longest slices inside each (`slowest_frames`). That is where the analysis below starts.

That tracing (atrace's `gfx` category) costs both RenderThreads time. On these screens it costs more than on the feed, so the RenderThread's durations here are traced durations; both apps pay it.

## Results

compose-rs at b3ef0c4: round 10 of [12](12-android-performance.md), which ran every scenario (`results/2026-10-23-pixel-8-pro-round-10.json`, `speed`, `--runs 15`). Round 9 (c57a8e1, `results/2026-10-23-pixel-8-pro-round-9.json`) is quoted in parentheses. Since round 9, compose-rs gained two changes that touch these screens:

- **D430–D435**: lazy items reused by content type, as Compose's `ReusableContent` and `LazyLayoutItemReusePolicy`. The list's headers and its four kinds of items pass content types, as the Kotlin list did already; a carousel's inner list keeps its slots when its item is reused, so its tiles are reused too (D434). [12](12-android-performance.md), "Item reuse on the device", has the prefetch work it changed.
- **D440–D442**: the main thread joins compose-rs's hint session ("The main thread in the hint session" below).

The Kotlin app didn't change on these screens. The changes since the first results (921db6b, "First results" below) are D390 (compose-rs's own hint session for the RenderThread), D410–D412 (paths reused, "With paths reused") and D420–D424 (one task per infinite transition, "Infinite transitions, one task each").

measure.py's checks passed: the installed native libraries were those built, and both apps were `speed` at the start and the end. Medians of 3 runs, startup of 15. The better side is in bold; ties aren't.

"Frames presented at another vsync than expected" counts frames shown later than FrameTimeline predicted. In the animation, all of Compose's frames are, in every run, and compose-rs's were in two of round 9's three runs (none in round 10): the main thread and the RenderThread together take longer than a vsync. They are presented a vsync late but on cadence: latency, not dropped frames. gfxinfo's 14 to 15 ms median frame time for both apps says the same.

The RenderThread's boost below is the higher of both sessions' votes when each frame started (HWUI's and, for compose-rs, D390's), from the round's JSON (`d390_boost`); measure.py's `render_boost` reads HWUI's alone.

### Stress scroll: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs (round 9) |
|---|---|---|
| Frames (gfxinfo) | 1,804 | 1,811 |
| Janky frames (FrameTimeline), per run | 1, 1, 2, all the app's | **0, 0, 0** (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 115, 2, 330 | **0, 0, 0** (0, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 31 (8), 29 (6), 26 (5) | **11 (0), 17 (0), 7 (0)** (45 (16), 27 (10), 25 (8)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 8 / 12 ms | 5 / **6** / **8** ms (6 / 7 / 8 ms) |
| Main thread per frame p50 / p90 / p99 | 1.29 / 3.32 / 7.76 ms (max 13.7) | **1.02** / **1.63** / **2.86** ms (max 4.8) (1.67 / 2.42 / 3.60 ms, max 10.6) |
| RenderThread per frame p50 / p90 / p99 | 2.63 / 3.68 / 5.31 ms | **2.49** / **3.55** / **5.02** ms (2.20 / 3.08 / 4.22 ms) |

| The boost (medians; nothing ranked) | Kotlin Compose | compose-rs (round 9) |
|---|---|---|
| RenderThread's boost minimum, time-weighted (out of 1024) | 282 | 295 (runs: 272 to 305), of which HWUI's session's alone 16 (324, 16) |
| RenderThread frames unboosted / at 100–199 / at 200 or more | 5 % / 17 % / 79 % | 4 % / 18 % / 77 % (1 % / 4 % / 96 %) |
| `DrawFrame` p50: unboosted / 100–199 / 200 or more | 3.38 / 3.06 / 2.49 ms | 3.33 / 3.03 / 2.40 ms (3.11 / 3.05 / 2.18 ms) |
| RenderThread running time on the little cores | 3 % | 2 % (1 %) |
| Main thread running time on the little cores | 13 % (10 to 15 %) | 21 % (16 to 22 %) (83 %, 81 to 87 %) |

**No janky frame of compose-rs's**; Compose missed 4 deadlines. Its slowest frames compose and measure a newly revealed item inside the frame (`AndroidOwner:measureAndLayout`, 6 to 8 ms, in the fling's `Recomposer:animation`), as in the first results ("Where each app struggles"); its third run's slowest (45.7 ms) waited for its RenderThread. compose-rs prefetched 335 to 356 times a run (417 to 456 in round 9).

**The main thread is off the little cores.** Since D440 it is in compose-rs's session, with the RenderThread's vote, and runs 21 % of its time on the little cores, against 83 % in round 9. Its p50 / p90 / p99 went from 1.67 / 2.42 / 3.60 to 1.02 / 1.63 / 2.86 ms, as in D440's A/B (1.07 / 1.71 / 2.97 ms), and are now shorter than Compose's at every percentile: the median by 21 %, the p99 by 63 %. Its CPU time fell 45 % (4.37 → 2.41 s) while its work rose 24 % (574 → 713 ms): at higher frequencies the work metric reads higher for the same code, as in D440's A/B (498 → 610 ms). Two effects of round 9's little cores are gone:
- **No `doFrame` waited for compose-rs's idle-time work** (8 to 16 a run in round 9): prefetch steps run in half the time (1.14 against 2.18 ms each).
- **No frame laid out a newly revealed item itself.** Round 9's runs had 1 to 3 frames of about 10 ms doing so; round 10's slowest main-thread frames took 4.7 to 5.1 ms.

**The RenderThread was boosted less than in round 9.** Our session's vote averaged 295 (272 to 305) against 324, and 77 % of the RenderThread's frames started at 200 or more, against 96 %. The vote's level is a random walk set by how much the frames vary from one to the next (D441), and it differs between sessions: 247 to 257 in D440's A/B. So the RenderThread's p50 went from 2.20 to 2.49 ms (2.49 ms in D440's A/B as well), still 0.14 ms shorter than Compose's. At the same boost, both apps' frames take about the same time (p50 2.40 against 2.49 ms at 200 or more).

### Stress animation: frames (10 s, 3 runs)

| | Kotlin Compose | compose-rs (round 9) |
|---|---|---|
| Frames (FrameTimeline) | 1,199 | 1,206 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 1,206, 1,197, 1,199 | **0, 0, 0** (0, 1,203, 1,197) |
| `doFrame`s starting more than 2 ms late, per run | 10, 13, 7 | **7, 4, 6** (1, 1, 3) |
| Frame time p50 / p90 / p99 (gfxinfo) | 15 / 17 / 19 ms | **14** / **16** / **18** ms (15 / 16 / 18 ms) |
| Main thread per frame p50 / p90 / p99 | 3.59 / 5.32 / 8.16 ms (max 9.9) | **2.24** / **2.82** / **4.12** ms (max 5.2) (2.36 / 2.97 / 3.99 ms, max 6.7) |
| RenderThread per frame p50 / p90 / p99 | 7.39 / 8.88 / 10.82 ms | **6.44** / **7.61** / **8.83** ms (6.34 / 7.42 / 8.41 ms) |
| RenderThread's boost minimum, time-weighted; frames unboosted | 719; 0 % | 467 (HWUI's alone 384); 0 % (469 (429); 0 %) |

**Both apps run near their limit**, boosted on every frame. Both RenderThreads take most of a 120 Hz vsync (8.33 ms) at p50, and more at p90 (Compose) or p99 (compose-rs). Neither app had a janky frame (Compose had one a run in round 9). No frame of compose-rs's was presented late this round; all of Compose's were.

**compose-rs's main thread** moved within the runs' spread of round 9 (p50 2.36 → 2.24 ms, p99 3.99 → 4.12 ms): neither change since round 9 touches this screen, which has no lazy list and whose main thread already ran on the mid and big cores. Its frame's own part (`compose-rs: frame`) takes 1.19 ms (round 9: 1.26 ms): 0.35 ms of tasks, 0.37 ms of layout, 0.29 ms of drawing and 0.53 ms of replay. Compose's main thread takes 60 % longer at the median and twice as long at p99.

**compose-rs's RenderThread** takes about as long as in round 9 (p50 6.34 → 6.44 ms, p99 8.41 → 8.83 ms; 8.66 to 8.89 ms in this round's runs), for 3 % less work (3,280 → 3,184 ms). HWUI's session voted less than in round 9 (384 against 429), ours about the same (467 against 469 for the higher of both). Compose's frames are 15 % longer at the median and 23 % at p99: its faded cells are drawn offscreen ("With `ModulateAlpha`" below).

**compose-rs's slowest RenderThread frames** (10.6 to 15.1 ms, one a run) were again GPU waits (`Vulkan finish frame`, 6.3 to 13.2 ms), as in round 9; none missed a deadline.

### Work

| Stress scroll (10 flings) | Kotlin Compose | compose-rs (round 9) |
|---|---|---|
| Main thread work | 1,244 ms | **713 ms** (−43 %; 574 ms) |
| RenderThread work | 1,679 ms | **1,593 ms** (−5 %; 1,527 ms) |
| Main thread CPU time | 3.96 s | **2.41 s** (4.37 s) |
| RenderThread CPU time | 4.93 s | **4.56 s** (4.18 s) |

| Stress animation (10 s) | Kotlin Compose | compose-rs (round 9) |
|---|---|---|
| Main thread work | 2,881 ms | **1,440 ms** (−50 %; 1,444 ms) |
| RenderThread work | 6,042 ms | **3,184 ms** (−47 %; 3,280 ms; Compose with `ModulateAlpha`: 3,286 ms in its session) |
| Main thread CPU time | 4.07 s | **2.66 s** (2.78 s) |
| RenderThread CPU time | 7.00 s | **5.56 s** (5.56 s) |
| HWUI's worker threads (`hwuiTask0`, `hwuiTask1`), CPU time (the second run) | **8.5 s** | 11.0 s (11.2 s) |

**Work** is milliseconds on the fastest core at its highest frequency ([12](12-android-performance.md), "Method"). compose-rs's main thread does 43 % less work than Compose's scrolling the list, and half animating. On the list, its work rose from 574 to 713 ms while its CPU time fell from 4.37 to 2.41 s: the same code at higher frequencies (D440, above). How much item reuse saved can't be told apart from that on the device: a prefetched item's composition did about the same work as in round 9 (0.167 against 0.163 ms), on faster cores where the metric reads about a fifth higher ([12](12-android-performance.md), "Item reuse on the device").

**HWUI's two worker threads** still cost compose-rs about 30 % more CPU time than Compose in the animation (11.0 against 8.5 s), as in the first results: reusing paths (D410) didn't change it. They rasterize the 50 stroked waves on the CPU into masks (Skia's software path renderer); why compose-rs's take longer hasn't been found.

### Memory

| | Kotlin Compose | compose-rs (round 9) |
|---|---|---|
| Stress list, PSS 5 s after start | 63.2 MB | **50.7 MB** (49.6 MB) |
| of which Java heap / native heap / code / graphics | 5.3 / 10.6 / 7.2 / 34.1 MB | 1.3 / 18.1 / 2.3 / 23.8 MB (1.3 / 17.1 / 2.2 / 23.8 MB) |
| Stress list, PSS after 10 flings | 109.3 MB | **97.8 MB** (97.0 MB) |
| of which Java heap / native heap / graphics | 11.1 / 21.5 / 63.3 MB | 5.9 / 22.3 / 62.0 MB (6.4 / 20.8 / 62.2 MB) |
| Stress animation, PSS 3 s after start | **167.3 MB** | 180.4 MB (181.4 MB) |
| of which Java heap / native heap / graphics | 13.1 / 21.5 / 119.5 MB | 4.2 / 18.3 / 150.5 MB (4.2 / 19.2 / 150.6 MB) |
| Stress animation, PSS after 13 s | 195.8 MB (runs: 190.6 to 284.7) | 210.1 MB (194.8 to 214.5) (203.9 MB) |
| of which Java heap / native heap / graphics | 12.6 / 27.8 / 141.8 MB | 7.8 / 23.9 / 171.3 MB (7.0 / 23.0 / 162.9 MB) |

**On the list**, compose-rs holds less after start (less code, less Java heap, less graphics memory) and after scrolling, as before. Its native heap is 1.0 MB larger than in round 9 after start (17.1 → 18.1 MB) and 1.5 MB larger after scrolling (20.8 → 22.3 MB), in all three runs alike.

**Where the 1.0 to 1.5 MB went** (D460). The list's first layout has measured only its header (81 px) when it estimates how many items to compose: 87 items, its rows and its carousels' tiles, of which 38 stay; the others leave in the next pass without ever being measured. Until D430 they were disposed. With D430 they went to the reuse pool, 7 per content type: 30 rows with their carousels' first tiles, 531 layout nodes, 3,563 groups and 1,462 slots, about 33 KB a row. The pool then stayed full: scrolling hands each slot leaving to an item coming in the same pass, so nothing else enters it; and the items in it, once reused and measured, kept their paragraphs (the Java heap after the flings was 0.4 MB larger too). Compose composes items as its measurement reaches them, so its pool holds only items that left. A slot whose item leaves unmeasured is now disposed, as before D430. A flag D430 added to every group, written and never read, had also grown `Group` from 144 to 152 bytes; it's gone (D461).

The Rust heap, headless (a counting allocator; the stress screen at 1008 × 2244 px and 2.25, a fixed text measurer; after start, and after 10 flings of 1,400 px):

| | before D430 (7cf5338) | round 10 (aaa2035) | D460, D461 |
|---|---|---|---|
| After start | 5,412.6 KiB, 357 nodes, 2,474 groups | 6,405.2 KiB, 888 nodes, 6,097 groups | 5,428.0 KiB, 360 nodes, 2,497 groups |
| After 10 flings | 5,886.5 KiB, 516 nodes, 3,585 groups | 7,008.4 KiB, 1,080 nodes, 7,441 groups | 5,791.6 KiB, 516 nodes, 3,585 groups |

What's left of reuse's cost after start, 15 KiB: one more group per composed item (its slot's reusable group), and the one item the first layout measured and then dropped (the second section's header), kept. After the flings the pool is empty: every item that left was taken by one coming in, in the same pass. On the device, main (aaa2035) against D460 and D461, 6 runs each in two alternating installs, the stress list's `stress_scroll` memory steps (medians, MB):

| | main | D460, D461 |
|---|---|---|
| Native heap after start | 18.1 | 17.1 |
| Native heap after 10 flings | 22.3 | 20.4 |
| Java heap after 10 flings | 5.0 | 4.6 |
| PSS after start / after 10 flings | 50.7 / 97.1 | 49.7 / 94.8 |

No range overlaps; the native heap is back to round 9's (17.1 and 20.8 MB).

**In the animation**, the medians changed places: Compose's 195.8 MB after 13 s is below compose-rs's 210.1 MB this round, against 226.2 and 203.9 MB in round 9. Neither app changed on this screen. Compose's memory swings with its garbage collector: its runs ranged over 190.6 to 284.7 MB this round (211.0 to 264.0 MB in round 9), and its Java heap was 12.6 MB at the median here against 44.9 MB in round 9's. compose-rs's runs ranged over 194.8 to 214.5 MB (192.3 to 214.6 MB). compose-rs grows by 30 MB over the 10 s, Compose by 29 MB at the median; compose-rs holds about 30 MB more graphics memory than Compose: HWUI's scratch surfaces and buffers ("With paths reused" below).

### Startup

| | Kotlin Compose | compose-rs (round 9) |
|---|---|---|
| `stress` (`TotalTime`, 15 cold starts) | 145 ms (136 to 152) | **126 ms** (120 to 134; −13 %) (126 ms) |
| The first frame's main thread (`traversal`, one traced start) | 58.5 ms | **42.7 ms** (44.9 ms) |
| `bindApplication` | 17.0 ms | **12.6 ms** (13.1 ms) |

compose-rs's start didn't change: its first frame is 13.8 ms of its own (14.0 ms), 3.9 ms of replay and 5.3 ms of `Record View#draw()`, broken down in "First results" below. Compose's first frame took 58.5 ms in this run's traced start (56.7 ms in round 9's, 64.3 ms in the first results': single traces).

## First results (921db6b)

These were the current results before round 9, kept as history: compose-rs at 921db6b with D400–D402, before D390, D410 and D420. The run is `results/2026-10-23-pixel-8-pro-stress.json`: `speed`, `--runs 15`, the stress scenarios only. The RenderThread boosts below are HWUI's session's alone.

### Stress scroll: frames (10 flings, 3 runs) (first results)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,792 | 1,793 |
| Janky frames (FrameTimeline), per run | 0, 2, 1, all the app's | 1, 0, 0, the app's |
| Frames presented at another vsync than expected, per run | 0, 53, 43 | **3, 0, 0** |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 21 (6), 34 (8), 29 (9) | **13 (2), 10 (0), 9 (0)** |
| Frame time p50 / p90 / p99 (gfxinfo) | **5** / 8 / 12 ms | 6 / **7** / **9** ms |
| Main thread per frame p50 / p90 / p99 | **1.31** / 3.18 / 8.00 ms (max 12.7) | 1.41 / **2.05** / **3.24** ms (max 5.6) |
| RenderThread per frame p50 / p90 / p99 | **2.60** / **3.60** / **5.14** ms | 3.04 / 4.62 / 6.60 ms |

**Main thread.**
- Compose's median main-thread frame is a little shorter. Its slowest are 2.5 times as long as compose-rs's.
- Its three janky frames were all the app's. Each composed and measured a newly revealed item inside the frame ("Where each app struggles").
- compose-rs prefetched 356 to 370 times a run: it composes, measures and records the next item between frames (D156, D322, D350), carousels included.

**compose-rs's one janky frame.** It was a 21.4 ms `DrawFrame` in its first run. 12.5 ms of it went to `CreateGraphicsPipeline` for a `CircularRRectOp`: Skia compiling a GPU pipeline the first time it drew that shape. HWUI stores compiled pipelines (`ShaderCache::store`), and no later run had one.

**RenderThread.** compose-rs's is slower at every percentile, for less work (below). As on the feed ([12](12-android-performance.md), "the RenderThread's boost"), that is the boost:

| HWUI boost (medians) | Kotlin Compose | compose-rs |
|---|---|---|
| Boost minimum, time-weighted (out of 1024) | 283 | 114 |
| Frames unboosted / at 100–199 / at 200 or more | 5 % / 19 % / 75 % | 49 % / 30 % / 25 % |
| `DrawFrame` p50: unboosted / 100–199 / 200 or more | 3.35 / 3.08 / 2.52 ms | 3.45 / 3.05 / 2.37 ms |
| RenderThread running time on the little cores | 4 % | 14 % |

At the same boost, the two RenderThreads' frames take the same time. Compose's long main-thread frames earn it a boost three quarters of the time; compose-rs's half its frames run unboosted.

### Stress animation: frames (10 s, 3 runs) (first results)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (FrameTimeline) | 1,198 | 1,200 |
| Janky frames (FrameTimeline), per run | 1, 0, 20, all the app's | **0, 0, 0** |
| Frames presented at another vsync than expected, per run | 925, 1,202, 1,164 | 1,199, 0, 1,206 |
| `doFrame`s starting more than 2 ms late, per run | 1, 7, 237 | 1, 2, 17 |
| Frame time p50 / p90 / p99 (gfxinfo) | 15 / 17 / 19 ms | 15 / 17 / 19 ms |
| Main thread per frame p50 / p90 / p99 | 3.68 / 5.07 / 7.18 ms (max 12.2) | **3.20** / **3.98** / **5.24** ms (max 8.6) |
| RenderThread per frame p50 / p90 / p99 | 7.52 / 8.84 / 10.49 ms | **6.16** / **7.20** / **8.60** ms |
| HWUI boost minimum, time-weighted; frames unboosted | 704; 0 % | 450; 0 % |

**Both apps run near their limit.** Both are boosted on every frame. Both RenderThreads take most of a 120 Hz vsync (8.33 ms) at p50, and more at p90 (Compose) or p99 (compose-rs).

**Compose's third run.**
- Its main thread started 237 `doFrame`s late (none after its own work: the scheduler woke it late) and missed 20 deadlines.
- Its main thread's p50 was 5.14 ms, against 3.59 and 3.68 ms in the other runs.
- Nothing in the trace explains it beyond more time waiting to run (24 against 5 ms runnable). It is the median run's jank that counts: 1 against 0.

#### With `ModulateAlpha` (D402)

A second session (`results/2026-10-23-pixel-8-pro-stress-modulate-alpha.json`, `stress_animation` only) measured both apps again. Kotlin's cells were given `compositingStrategy = CompositingStrategy.ModulateAlpha`, Compose's opt-in for fading a layer's drawings one by one without an offscreen buffer. compose-rs is unchanged.

| | Compose, `Auto` (first session) | Compose, `ModulateAlpha` | compose-rs (second session) |
|---|---|---|---|
| Janky frames, per run | 1, 0, 20 | 1, 1, 0 | 1, 1, 1 |
| Main thread p50 / p90 / p99 | 3.68 / 5.07 / 7.18 ms | 3.89 / 5.04 / 6.65 ms | 3.26 / 4.04 / 5.84 ms |
| RenderThread p50 / p90 / p99 | 7.52 / 8.84 / 10.49 ms | 6.22 / 7.03 / 8.21 ms | 6.36 / 7.42 / 9.28 ms |
| Main thread, RenderThread work | 2,897, 6,048 ms | 2,795, 3,286 ms | 2,134, 4,178 ms |

**With `ModulateAlpha`:**
- Compose's RenderThread does 46 % less work, a fifth less than compose-rs's, and its frames match compose-rs's.
- Its main thread keeps doing a third more work than compose-rs's.

**compose-rs's janky frames in this session** (one a run) were RenderThread frames of 12 to 16 ms, of which 9 to 14 ms was `Vulkan finish frame`: waiting for the GPU. This happened in the first session too, without missing a deadline (slowest `DrawFrame`s 11.2 to 15.4 ms).

So the stress animation janks rarely in either app, and which one depends on the session.

### Work (first results)

| Stress scroll (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 1,230 ms | **576 ms** (−53 %) |
| RenderThread work | 1,656 ms | **1,379 ms** (−17 %) |
| Main thread CPU time | 3.89 s | **3.29 s** |
| RenderThread CPU time | **4.90 s** | 5.82 s |

| Stress animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 2,897 ms | **2,163 ms** (−25 %) |
| RenderThread work | 6,048 ms | **4,399 ms** (−27 %; Compose with `ModulateAlpha`: 3,286 ms) |
| Main thread CPU time | 4.27 s | **3.90 s** |
| RenderThread CPU time | 7.16 s | **5.66 s** |
| HWUI's worker threads (`hwuiTask0`, `hwuiTask1`), CPU time (the second run) | **8.7 s** | 10.9 s |

**Work** is milliseconds on the fastest core at its highest frequency ([12](12-android-performance.md), "Method"). compose-rs's main thread does about half Compose's work scrolling and three quarters in the animation.

**HWUI's two worker threads** run in the animation (scrolling, they hardly run), where both apps keep them busy most of the time. They rasterize the 50 stroked waves on the CPU into masks (Skia's software path renderer). That work is the canvas's, and it costs compose-rs a quarter more: its masks go into new GPU images each frame rather than recycled ones (next section; since D410, they're freed a frame later).

### Memory (first results)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Stress list, PSS 5 s after start | 63.6 MB | **49.7 MB** |
| of which Java heap / native heap / code / graphics | 5.3 / 10.8 / 7.3 / 34.3 MB | 1.3 / 17.0 / 2.3 / 23.8 MB |
| Stress list, PSS after 10 flings | 110.8 MB | **97.9 MB** |
| of which Java heap / native heap / graphics | 12.1 / 21.8 / 63.3 MB | 6.5 / 21.0 / 62.4 MB |
| Stress animation, PSS 3 s after start | **166.9 MB** | 289.4 MB |
| of which Java heap / native heap / graphics | 12.3 / 21.8 / 119.4 MB | 7.8 / 59.6 / 210.8 MB |
| Stress animation, PSS after 13 s | **251.0 MB** | 405.4 MB |
| of which Java heap / native heap / graphics | 30.0 / 27.0 / 132.2 MB | 24.9 / 143.6 / 218.9 MB |

**On the list**, compose-rs holds less after start (less code, less Java heap, less graphics memory) and slightly less after scrolling. Both apps' graphics memory roughly doubles over the flings: the render thread's caches, and the 8 bitmaps' textures.

**In the animation, compose-rs holds 60 % more.** Over a minute of it (sampled at 5, 20, 40 and 60 s, outside measure.py) it moved between 320 and 450 MB: it doesn't leak, but it waits for the garbage collector. Compose stayed between 184 and 224 MB.

**Where it goes** (`dumpsys gfxinfo` after the minute):

- **GPU memory.** HWUI's GPU caches held 146.6 MB for compose-rs, of which 82.6 MB were Skia's software path masks (518 entries), against 103.7 MB and 8.0 MB (50 entries) for Compose.
  - A cached mask lives as long as its path.
  - Kotlin's one reset `Path` frees each wave's mask when it changes.
  - compose-rs's replayer makes a new `android.graphics.Path` for each path each frame, and drops it when the display list does. Its native path, and the mask cached for it, live until the collector finalizes the Kotlin object.
- **Native heap.** The 60 to 144 MB are those paths' native memory, waiting for the same collection. A `Path`'s native size isn't counted against the Java heap, so the collector runs late.
- **New GPU images.** Since the dead paths' masks keep their textures, Skia allocates 50 new GPU images a frame (`allocateImageMemory`: 50 a frame, 0.19 ms) where Compose re-uploads into textures it recycles.

The fix belonged to the Android bridge, and is the next section.

### With paths reused (D410, D411)

The replayer now:

- rewinds a dropped path's `android.graphics.Path` at once, and keeps it (64 at most) for the next definition, which rebuilds it, as Compose's one reset `Path`;
- reads a frame's drops before its definitions (Rust writes them first), so the 50 waves reuse the last frame's 50 `Path`s in the same frame.

A recording keeps its own copy of a path's contents, so rewinding changes nothing drawn. HWUI keys its cached mask by those contents' generation: it's freed when the last recording drawing them is, a frame later, not at a garbage collection.

D190's masks, for small filled paths (the waves are stroked: none is made for them), aren't made for paths that are new every frame either (D411).

Keeping a wave's path identity across frames instead, a resource id per draw site redefined in place, was not done (D412). It would save only the 5-byte drop, and a group's key and version would no longer name one drawing.

The stress animation at 0e0c86c with D410 and D411, against 0e0c86c, in one session (`dumpsys meminfo` every 5 s for 60 s, each app started alone):

| PSS over 60 s | Kotlin Compose | compose-rs before | compose-rs after |
|---|---|---|---|
| Total, range | 189–241 MB | 322–334 MB | **186–230 MB** (228 MB from 15 s; in a later session, 191–195 MB) |
| Java heap / native heap / graphics at 60 s | 35.0 / 26.7 / 165.7 MB | 14.1 / 87.8 / 224.8 MB | 6.4 / 22.0 / 192.0 MB |
| HWUI's software path masks at 60 s (`gfxinfo`) | 50 entries, 8.0 MB | 519 entries, 82.9 MB | 50 entries, 8.0 MB |
| Total GPU memory at 60 s | 103.1 MB | 146.6 MB | 125.7 MB |

compose-rs's memory no longer waits on the collector: it is flat after 15 s, where Compose's moves with its Java heap. Its level depends on how much scratch memory HWUI kept: 192 MB of graphics in this session, 158 MB in the later one. It holds about 26 MB more graphics memory than Compose at 60 s, mostly Skia's scratch surfaces and buffers (118 against 95 MB), which both apps' HWUI keeps for reuse.

measure.py's `stress_animation` (3 runs a side; compose-rs before and after in two runs of it, back to back):

| | compose-rs before | compose-rs after |
|---|---|---|
| PSS 3 s after start / after 13 s | 293.1 / 397.2 MB | **181.9 / 201.8 MB** |
| Janky frames (FrameTimeline), per run | 2, 1, 1 | **0, 1, 0** |
| Main thread per frame p50 / p90 / p99 | 3.93 / 5.11 / 7.21 ms | **3.62 / 4.67 / 6.12** ms |
| RenderThread per frame p50 / p90 / p99 | 7.64 / 8.50 / 9.76 ms | 7.55 / 8.55 / 9.46 ms |
| Main thread / RenderThread CPU time | 4.29 / 7.52 s | **3.73 / 6.28 s** |
| Main thread / RenderThread work | 2,115 / 3,909 ms | **1,933 / 3,200 ms** (−9 %, −18 %) |

Compose's runs in that session were slow: 233 to 330 janky frames a run, both threads' p50 above 9.5 ms, unlike its runs above. They aren't compared here. Its memory was 167–169 MB after start and 215–240 MB after 13 s.

The other scenarios didn't change: compose-rs before and after, in another session (measure.py's `scroll`, `animation` and `stress_scroll`, 3 runs a side each):

| compose-rs, before → after | Main thread p50 / p90 / p99 | RenderThread p50 / p90 / p99 | Janky frames | PSS after scrolling |
|---|---|---|---|---|
| Feed, 10 flings | 1.30 / 1.87 / 2.90 → 1.30 / 1.92 / 2.86 ms | 2.76 / 3.69 / 5.19 → 2.79 / 3.75 / 5.30 ms | 0 → 0 | 80.8 → 79.9 MB |
| Feed, animated header | 1.22 / 1.83 / 3.23 → 1.21 / 1.79 / 2.93 ms | 2.65 / 3.60 / 5.02 → 2.68 / 3.56 / 5.27 ms | 0 → 0 | |
| Stress list, 10 flings | 1.54 / 2.23 / 3.46 → 1.51 / 2.17 / 3.31 ms | 3.27 / 4.93 / 7.06 → 3.33 / 4.88 / 6.69 ms | 0 → 1 (one run) | 93.4 → 96.9 MB |

### Startup (first results)

| | Kotlin Compose | compose-rs |
|---|---|---|
| `stress` (`TotalTime`, 15 cold starts) | 143 ms (134 to 155) | **126 ms** (117 to 138; −12 %) |
| The first frame's main thread (`traversal`, one traced start) | 64.3 ms | **44.4 ms** |
| `bindApplication` | 19.2 ms | **12.3 ms** |

**Compose's first frame:**
- `Compose:initializeView` 8.5 ms;
- a recomposition, 3.2 ms;
- a measure of 20.8 ms, which composes and measures the visible items;
- the window's relayout, 4.5 ms;
- drawing, 8.2 ms.

The rest is untraced: class loading and the interpreter.

**compose-rs's first frame:**
- 14.1 ms of its own (`compose-rs: frame`), most of it two rounds of composing and laying out again (10.8 and 1.7 ms). The outer list composes its items, then each carousel composes its tiles: compose-rs has no subcomposition ([11](11-status.md)).
- 3.9 ms of replay, which makes the bitmaps;
- 5.4 ms of `Record View#draw()`;
- 6.2 ms waiting for the RenderThread's first frame (`postAndWait`);
- 4.7 ms of window relayout.

## Where each app struggles

From the traces of the first results (921db6b), before D390, D410 and D420; their effects are in "Results" above. The slowest 1 % of each app's frames on each thread, pooled over the 3 runs, with the slices inside them averaged per frame (`slowest_frames` gives the slowest 5 of each run).

**Compose, scrolling, main thread** (54 frames of 8.0 ms or more, 9.4 ms on average):
- `AndroidOwner:measureAndLayout` takes 4.5 ms a frame: Compose composes and measures a newly revealed item inside the layout pass, nested `LazyRow` included (`TextStringSimpleNode::measure` 1.1 ms, `Compose:recompose` 0.9 ms).
- `Record View#draw()` takes 3.4 ms.
- `Recomposer:animation` (the fling's frame) takes 3.5 ms.
- These are the frames that miss: Compose prefetches between frames too, but an item it hasn't prefetched in time is composed in the frame that reveals it.

**compose-rs, scrolling, main thread** (54 frames of 3.2 ms or more, 4.0 ms on average):
- Its own frame takes 0.5 ms of that.
- The rest is waiting for the RenderThread to take the frame (`postAndWait`, 2.2 ms), input (0.4 ms) and the view system.
- Its slowest main-thread frames are the RenderThread's.

**Both, scrolling, RenderThread** (Compose 6.5 ms on average, compose-rs 7.6 ms):
- Most of each is `flush commands` (2.9 and 3.4 ms): Skia submitting to and waiting for the GPU, and `queueBuffer` (1.9 ms).
- compose-rs's slowest are mostly unboosted frames (above). Its single 21 ms frame compiled a pipeline.

**The animation, main thread, all frames** (per frame):
- **Compose** runs its 147 transitions' frame loops in `Recomposer:animation` (1.19 ms). It measures and lays out (0.86 ms: the ticker and the percentages' text, 3.75 paragraphs a frame, 0.6 ms) and records (1.63 ms).
- **compose-rs** runs its 568 tasks (`compose-rs: tasks`, 1.12 ms: a task per animated value, and the ticker's), then lays out (0.40 ms, of which text 0.32 ms), draws (0.31 ms) and replays (0.68 ms). That comes to 2.03 ms for its whole frame, plus `Record View#draw()`.
- **Their slowest 1 %** (Compose 8.9 ms on average, compose-rs 7.0 ms) mostly wait for the RenderThread (`postAndWait`: 5.6 and 2.6 ms). compose-rs's also have 1.7 ms of tasks.

**The animation, RenderThread, all frames** (per frame):
- **Compose:**
  - Its `Drawing` takes 7.3 ms, of which `flush layers` takes 1.4 ms: 132 `alpha caused saveLayer` a frame, and 73 render passes (`OpsTask`s) a frame instead of one. Each cell faded below 1 is drawn offscreen and then composited.
  - This is the 46 % of RenderThread work that `ModulateAlpha` saves.
- **compose-rs:**
  - Its `Drawing` takes 6.0 ms, in one render pass.
  - 4.8 ms of it is `flush commands`, waiting for the GPU (Compose: 4.0 ms), which the 50 new mask images a frame add to.
- **Both apps' slowest RenderThread frames** (Compose 11.6 ms, compose-rs 9.8 ms on average) are those GPU waits: `Vulkan finish frame`, 4.3 and 7.4 ms.

**In short:**
- **Compose struggles on the main thread when content arrives.** Its slowest scroll frames compose and measure new items, and its animation frames are dearer per animated value.
- **compose-rs struggles where it hands drawing to Android:**
  - its paths, new every frame, cost memory, mask rasterization and GPU allocations (until D410: "With paths reused");
  - its RenderThread ran unboosted because its main thread is short (until D390: in round 9 the RenderThread was boosted almost every frame, and the main thread ran on the little cores instead, until D440 put it in the same session);
  - like Compose, it waits for the GPU on the heaviest frames.

## Infinite transitions, one task each (D420–D424)

compose-rs's infinite transitions now work as Compose's: one task per transition sets all its values each frame (D420). The executor's cost per task resumed by a frame went down too (D421):
- every task waiting for a frame used to wake the host (on Android a JNI upcall each);
- two scans of the awaiter list per task made resuming them quadratic.

Animated colors no longer convert to Oklab to compare (D422). `update_transition`'s values keep a task each (D423).

**Headless** (`benchmarks/benches/animation.rs`, D424; an M-series Mac, frames 16 ms apart on a phone-sized host, p50):

| | Before | After |
|---|---|---|
| Resuming 568 tasks waiting for a frame (568 closure calls: 0.43 µs) | 169 µs | 21 µs |
| 140 infinite transitions of 4 values, nothing reading them | 211 µs | 44 µs |
| The same, drawn as the stress cells | 250 µs | 76 µs |
| `stress_animation` (fixed text measurer) | 284 µs | 107 µs |

Most of the 140 transitions' remaining time is the values' own math: the easing curves (a cubic Bézier solved per value per frame) and the colors' conversions from Oklab.

**On the Pixel 8 Pro**: the same session, `speed`, 3 runs a side, before (0e0c86c) then after, with the same Kotlin APK measured beside each. The runs are `results/2026-10-23-pixel-8-pro-infinite-transitions-{before,after}.json`.

| `stress_animation`, compose-rs (medians of 3 runs) | Before | After |
|---|---|---|
| Main thread per frame p50 / p90 / p99 | 3.18 / 3.82 / 5.08 ms | **2.53** / **3.37** / 5.44 ms |
| Main thread work | 2,139 ms | **1,550 ms** (−28 %) |
| Main thread CPU | 3,845 ms | **2,946 ms** |
| `compose-rs: tasks` per frame, per run | 1.13, 1.13, 1.13 ms | **0.36, 0.41, 0.42 ms** |
| `compose-rs: frame` per frame, per run | 2.03, 2.02, 2.06 ms | **1.25, 1.37, 1.36 ms** |
| Janky frames (FrameTimeline), per run | 0, 0, 1 | 0, 0, 1 |
| RenderThread per frame p50 / p90 / p99 | 6.11 / 7.04 / 8.20 ms | 7.14 / 8.18 / 9.37 ms |
| RenderThread work | 4,221 ms | 3,850 ms |

- The p99 moved within the runs' spread (before 5.08, 4.70, 5.74 ms; after 3.97, 6.17, 5.44 ms). These frames wait for the RenderThread (`postAndWait`), not for tasks.
- The RenderThread did less work but took longer. The device was slower in the second half of the session: Kotlin's RenderThread p50 went from 7.40 to 7.80 ms with the same APK, and its third run had 292 janky frames against 1. Nothing in this change touches the RenderThread's input; the drawing is the same.
- **The feed's animation** (`animation`, docs/12) didn't move: compose-rs's main thread p50 / p90 / p99 1.21 / 1.79 / 2.91 ms before and 1.21 / 1.80 / 3.15 ms after, with 187 ms of work both times and no janky frames. Its header has three animated values.
- **In round 9** (with D390 as well, which neither build here had), compose-rs's main thread p50 / p90 / p99 was 2.36 / 2.97 / 3.99 ms for 1,444 ms of work, `compose-rs: tasks` 0.36 ms a frame, and the RenderThread's p50 6.34 ms: the gains held, with a p99 below both builds' here. Round 10 ("Results" above) measured 2.24 / 2.82 / 4.12 ms, 1,440 ms of work and 0.35 ms of tasks a frame.

## The main thread in the hint session (D440)

Round 9 left compose-rs's main thread on the little cores on this list: with D390 boosting the RenderThread, the RenderThread's part of each frame got shorter, HWUI's session (which reports the whole frame and boosts both threads) hardly boosted any more, and the main thread ran 83 % of its time on the little cores, for a p50 of 1.67 ms against 1.41 ms before D390. Two findings ([14](14-performance-opportunities.md), "The main thread in the session"):
- **Why the vote sits near 320 here, and near 100 on the feed.** Under the target, the power HAL's controller is a random walk. About every 83 ms it takes about 23 off the vote, whatever the vote, and adds 7.7 for each 100 µs by which the last report of the batch exceeds the one before; only rises count. This list's frames vary more from one to the next than the feed's. The kicks average 26 to 29 a batch here against 20 to 21 there, so the vote drifts up here and down there. HWUI's session works the same way, which is why Compose's main thread is boosted on this list too (its vote about 290).
- **The fix: the main thread joins compose-rs's session.** It gets the same vote as the RenderThread; the reports don't change.

Interleaved on the device, 12 runs each against main (18272a3), the variants rotating each round (medians of the runs' percentiles; power: means over the measured 13 s and 9 s, the difference ± its 95 % interval):

| | Stress scroll: main | with the main thread (D440) | Stress animation: main | with the main thread (D440) |
|---|---|---|---|---|
| Main thread per frame p50 / p90 / p99 | 1.54 / 2.24 / 3.53 ms | **1.07** / **1.71** / **2.97** ms | 2.36 / 3.00 / 4.10 ms | 2.26 / 2.83 / 3.80 ms |
| RenderThread per frame p50 / p90 / p99 | 2.39 / 3.46 / 5.01 ms | 2.49 / 3.51 / 4.83 ms | 6.36 / 7.43 / 8.50 ms | 6.38 / 7.43 / 8.43 ms |
| Janky frames, the app's / the compositor's (12 runs) | 0 / 0 | 0 / 2 (one run) | 1 / 0 | 1 / 0 |
| `doFrame`s delayed by main-thread work (on the list, prefetch steps; 12 runs) | 58 (1 to 10 a run) | **4** (0 or 1) | 1 | 0 |
| Main thread on the little cores | 79 % | 28 % | 0 % | 0 % |
| Main thread CPU time (work) | 3,529 (498) ms | 2,368 (610) ms | 2,515 (1,295) ms | 2,409 (1,318) ms |
| Power: CPU rails | 474 ± 14 mW | −0.7 % ± 2.2 | 1,837 ± 61 mW | −0.8 % ± 2.8 |
| Power: all rails | 1,813 ± 35 mW | −0.6 % ± 1.6 | 3,663 ± 87 mW | −0.4 % ± 1.9 |

- **The list's main thread.** Its median is now shorter than Compose's in round 9 (1.07 against 1.26 ms). The runs' p50s range over 1.02 to 1.23 ms against main's 1.51 to 1.61, and their p99s over 2.69 to 3.05 against 3.16 to 3.90 ms.
- **Prefetch steps.** Almost no frame waits for one any more.
- **Power.** No change was resolved.
- **The animation.** It moves little: both sessions already vote about 430 to 465 there, and both threads run on the mid and big cores.
- **The compositor.** One run had 2 compositor-side janky frames and presented 72 frames a vsync late, on cadence. In all, 4 of the 48 runs with the main thread had such frames (1 to 3), against none of main's 48: too few to tell from noise.
- **Not chosen.** Reporting both threads' CPU time against 8.33 ms gave shorter frames still: list main thread p50 0.94 ms, RenderThread 2.09 ms. It cost +2.6 % (± 2.3) CPU-rail power here and +4.0 % (± 2.1) on the feed (D442).
- **Beside Compose (round 10, "Results" above).** The list's main thread p50 / p90 / p99 was 1.02 / 1.63 / 2.86 ms against Compose's 1.29 / 3.32 / 7.76 ms, 21 % of its time on the little cores. No `doFrame` waited for a prefetch step, and no run of compose-rs's had a compositor-side janky frame, on any screen.

## Next steps

- **Paths in the bridge**: done (D410, D411, "With paths reused"). HWUI's worker threads still take about 30 % more CPU time than Compose's in the animation (11.0 against 8.5 s in round 10, 11.2 against 8.6 s in round 9), unchanged by D410; why isn't known.
- **Infinite transitions**: done (D420–D424). The 0.4 ms of tasks a frame left is mostly easing curves and color conversions.
- **The stress list's main thread on the little cores**: done (D440, "The main thread in the hint session"). The main thread joined compose-rs's hint session: in its A/B, on the little cores 79 → 28 % of its time, p50 1.54 → 1.07 ms, `doFrame`s waiting for a prefetch step 58 → 4 in 12 runs, no power change resolved. In round 10, 21 % on the little cores, p50 1.02 ms, no `doFrame` waiting for a prefetch step, and no frame laying a newly revealed item out itself (1 to 3 a run in round 9). If those frames come back, splitting an item's measurement so a step can stop at the deadline ([12](12-android-performance.md), "Next steps") is the fix.
- **Item reuse** (D430–D435): measured on the device for the first time in round 10, in the same build as D440, whose faster cores raise the work metric; a prefetched item's composition did about the same work as in round 9 (0.167 against 0.163 ms) and half as many paragraphs were laid out ([12](12-android-performance.md), "Item reuse on the device"). An interleaved A/B would separate the two. The list's native heap was 1.0 to 1.5 MB larger than in round 9: items the first layout composed beyond those it wanted, kept for reuse; fixed (D460, "Memory").
- **The RenderThread's boost on the list** varies with the session: our session's vote averaged 324 in round 9, 247 to 257 in D440's A/B and 295 in round 10, for RenderThread p50s of 2.20, 2.49 and 2.49 ms (Compose: 2.62 and 2.63 ms in rounds 9 and 10).
- **Shader warm-up**: a pipeline compiled on first use cost compose-rs its one janky scroll frame in the first results (none in rounds 9 and 10). Compose's apps share the same cache behavior; it isn't specific to compose-rs, but it showed here.
