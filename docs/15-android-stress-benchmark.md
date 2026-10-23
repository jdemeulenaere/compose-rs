# 15. Android stress benchmark: compose-rs and Jetpack Compose under load

[12](12-android-performance.md) compares the two on a light social feed. This document compares them on two much heavier screens, built the same way in both apps (D400): a list of rich, heterogeneous items, and a screen running hundreds of animations at once. It measures jank, frame durations on both threads, work, memory and startup with the same tools as [12](12-android-performance.md) (D401).

**Short answer** (Pixel 8 Pro, Android 16; round 11 of [12](12-android-performance.md), compose-rs at 613eaf6, whose lazy lists compose and place only the items in view (D470–D496); medians of 3 runs, startup of 15):

- **Scrolling the stress list**, compose-rs had no janky frame; Compose missed 3 app deadlines in 3 runs.
  - compose-rs's main-thread frames are shorter than Compose's at every percentile: p50 1.08 against 1.25 ms, p99 2.75 against 7.61 ms, for half the work (573 against 1,194 ms).
  - Placing only the items in view took a fifth off both threads' work since round 10 (main thread 713 → 573 ms, RenderThread 1,593 → 1,311 ms) and 5 MB off the native heap after scrolling.
  - With less work to report, compose-rs's hint session voted less (190 against 295 in round 10). Its RenderThread's p50 is still a little shorter than Compose's (2.58 against 2.63 ms), but its p90 and p99 are now longer (3.81 / 5.45 against 3.63 / 5.27 ms), though at each boost level its frames are the shorter. Its main thread ran 46 % of its time on the little cores (21 % in round 10).
- **The stress animation** keeps both RenderThreads busy for most of a 120 Hz vsync (8.33 ms): Compose's frames exceed it at p90, compose-rs's at p99.
  - Compose missed 2 deadlines in one run; compose-rs none.
  - compose-rs's frames are shorter on both threads: main thread p50 2.23 against 3.64 ms, RenderThread p50 6.43 against 7.41 ms. Its main thread does half Compose's work since its transitions run one task each (D420).
  - Most of the RenderThread gap is one Compose default: a `graphicsLayer` whose alpha is below 1 draws offscreen (`CompositingStrategy.Auto`). compose-rs fades a layer holding a single drawing without one. With `CompositingStrategy.ModulateAlpha`, Compose's RenderThread matched compose-rs's in its session and did less work.
- **Memory**: on the list, compose-rs holds a fifth less: 91.2 against 113.3 MB after scrolling. Its native heap there is 17.2 MB against 22.3 MB in round 10 (D460 and D470, "Memory" below).
  - In the animation, after 13 s, it held 223 against Compose's 238 MB this round (210 against 196 MB in round 10). Compose's swings with its garbage collector (196 to 247 MB in this round's runs), compose-rs's less (218 to 224 MB).
  - In the first results compose-rs held 405 MB there: its 50 waves are new paths every frame, and each frame's paths, with the masks the GPU cached for them, lived until the garbage collector freed the Kotlin `Path`s. The replayer now rewinds a dropped path at once and rebuilds it for the next, as Compose's one reset `Path` does (D410).
- **Startup** of the stress list: 118 against 143 ms (126 ms in round 10: the first layout composes only what it shows, D472).

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
- **`contentType` on both.** compose-rs reuses items as Compose does since D430, by the same content types (`sticky_header_keyed_with`, `items_keyed_with`). Round 10 was the first measurement with it ("Results" below is round 11); the sections after "Results" predate it, when compose-rs composed each new item afresh where Compose reused the composition of an item of the same kind.
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

compose-rs at 613eaf6: round 11 of [12](12-android-performance.md), which ran every scenario (`results/2026-10-23-pixel-8-pro-round-11.json`, `speed`, `--runs 15`). Round 10 (b3ef0c4, `results/2026-10-23-pixel-8-pro-round-10.json`) is quoted in parentheses. Since round 10, compose-rs gained changes that touch the list, none that touch the animation:

- **D470–D496**: a lazy list composes and places only the items in view, as Compose's (it placed a viewport's worth more on each side, about 7 items here with their carousels' tiles), prefetches from its first layout on, and its first layout composes what it shows. Their interleaved A/Bs, and the review fixes that followed, are in "The lazy list's policy" below.
- **D460, D461**: an item that leaves the list without ever having been measured is disposed rather than kept for reuse ("Memory" below).
- **D450–D459**: lazy grids compose item by item. The carousels are `LazyRow`s, not grids, so this list has none.

The Kotlin app didn't change on these screens. The changes since the first results (921db6b, "First results" below) are D390 (compose-rs's own hint session for the RenderThread), D410–D412 (paths reused, "With paths reused"), D420–D424 (one task per infinite transition, "Infinite transitions, one task each"), D430–D435 (lazy items reused by content type), D440 (the main thread in the hint session) and D470–D496.

measure.py's checks passed: the installed native libraries were those built, and both apps were `speed` at the start and the end. Medians of 3 runs, startup of 15. The better side is in bold; ties aren't.

"Frames presented at another vsync than expected" counts frames shown later than FrameTimeline predicted. In the animation, all of Compose's frames are, in every run, and compose-rs's were in two of this round's three runs (none in round 10's): the main thread and the RenderThread together take longer than a vsync. They are presented a vsync late but on cadence: latency, not dropped frames. gfxinfo's 14 to 15 ms median frame time for both apps says the same.

The RenderThread's boost below is the higher of both sessions' votes when each frame started (HWUI's and, for compose-rs, D390's), from the round's JSON (`d390_boost`); measure.py's `render_boost` reads HWUI's alone.

### Stress scroll: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs (round 10) |
|---|---|---|
| Frames (gfxinfo) | 1,793 | 1,798 |
| Janky frames (FrameTimeline), per run | 2, 0, 1, all the app's | **0, 0, 0** (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 212, 0, 164 | **0, 0, 0** (0, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 30 (6), 28 (8), 22 (4) | **4 (1), 15 (1), 7 (0)** (11 (0), 17 (0), 7 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 8 / 12 ms | 5 / **6** / **8** ms (5 / 6 / 8 ms) |
| Main thread per frame p50 / p90 / p99 | 1.25 / 3.33 / 7.61 ms (max 13.0) | **1.08** / **1.62** / **2.75** ms (max 4.3) (1.02 / 1.63 / 2.86 ms, max 4.8) |
| RenderThread per frame p50 / p90 / p99 | 2.63 / **3.63** / **5.27** ms | **2.58** / 3.81 / 5.45 ms (2.49 / 3.55 / 5.02 ms) |

| The boost (medians; nothing ranked) | Kotlin Compose | compose-rs (round 10) |
|---|---|---|
| RenderThread's boost minimum, time-weighted (out of 1024) | 262 (runs: 229 to 321) | 190 (147 to 256), of which HWUI's session's alone 18 (295, 16) |
| RenderThread frames unboosted / at 100–199 / at 200 or more | 5 % / 17 % / 78 % | 14 % / 40 % / 42 % (4 % / 18 % / 77 %) |
| `DrawFrame` p50: unboosted / 100–199 / 200 or more | 3.43 / 3.09 / 2.50 ms | 3.26 / 2.90 / 2.21 ms (3.33 / 3.03 / 2.40 ms) |
| RenderThread running time on the little cores | 2 % | 6 % (2 %) |
| Main thread running time on the little cores | 15 % (10 to 24 %) | 46 % (35 to 57 %) (21 %, 16 to 22 %) |

**No janky frame of compose-rs's**; Compose missed 3 deadlines. Its slowest frames compose and measure a newly revealed item inside the frame (`AndroidOwner:measureAndLayout`, 6.8 to 8.9 ms, in the fling's `Recomposer:animation`), as in the first results ("Where each app struggles"). compose-rs prefetched 352 to 356 times a run (335 to 356 in round 10). No frame of its laid text out, in any run; each run had 10 frames with a second round of composition and layout (`recompose and layout again`, 1.9 to 2.3 ms a run in all), as in the A/Bs: most likely the sticky header changing.

**Less work on both threads.** Since round 10, the main thread's work fell from 713 to 573 ms (−20 %) and the RenderThread's from 1,593 to 1,311 ms (−18 %), against −13 to −16 % for each in the A/Bs of the lists' policy. The rest of the main thread's drop is most likely where it ran: more of its time on the little cores, where the work metric reads lower for the same code (its CPU time rose, 2.41 → 2.72 s).

**The boost fell with the work.** Our session's vote averaged 190 (147 to 256) against 295 in round 10, and 42 % of the RenderThread's frames started at 200 or more, against 77 %. The vote's level is a random walk set by how much the reported frames vary from one to the next (D441); with less work they vary less, as the A/Bs found (a time-weighted 212 to 230 against 252 to 268). The runs show it: the one voted highest (256) had the shortest RenderThread p50 (2.33 ms), the one voted lowest (147) the longest (2.76 ms). So:
- **The RenderThread's p90 and p99 are now longer than Compose's** (3.81 / 5.45 against 3.63 / 5.27 ms; round 10: 3.55 / 5.02 ms), its p50 still 0.05 ms shorter. At each boost level its frames are shorter than Compose's (p50 3.26 against 3.43 ms unboosted, 2.90 against 3.09 ms at 100–199, 2.21 against 2.50 ms at 200 or more), but Compose's are boosted more often: HWUI's session boosts them after Compose's long main-thread frames (78 % at 200 or more). This is where compose-rs is now behind Compose on these screens: a matter of the boost, not of the work.
- **The main thread**, in the same session, ran 46 % of its time on the little cores against 21 %. Its p50 rose from 1.02 to 1.08 ms, still 14 % shorter than Compose's; its p90 and p99 didn't rise (1.62 / 2.75 against 1.63 / 2.86 ms).
- **2 `doFrame`s waited for compose-rs's idle-time work**, in 2 of 3 runs (none in round 10; 8 to 16 a run in round 9, on the little cores).

### Stress animation: frames (10 s, 3 runs)

| | Kotlin Compose | compose-rs (round 10) |
|---|---|---|
| Frames (FrameTimeline) | 1,196 | 1,203 |
| Janky frames (FrameTimeline), per run | 0, 0, 2, all the app's | **0, 0, 0** (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 1,196, 1,196, 1,197 | 0, 1,205, 1,200 (0, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run | 19, 7, 25 | **3, 1, 2** (7, 4, 6) |
| Frame time p50 / p90 / p99 (gfxinfo) | 15 / 17 / 20 ms | **14** / **16** / **17** ms (14 / 16 / 18 ms) |
| Main thread per frame p50 / p90 / p99 | 3.64 / 5.42 / 8.04 ms (max 13.3) | **2.23** / **2.77** / **3.71** ms (max 4.8) (2.24 / 2.82 / 4.12 ms, max 5.2) |
| RenderThread per frame p50 / p90 / p99 | 7.41 / 9.00 / 10.73 ms | **6.43** / **7.53** / **8.62** ms (6.44 / 7.61 / 8.83 ms) |
| RenderThread's boost minimum, time-weighted; frames unboosted | 713; 0 % | 468 (HWUI's alone 400); 0 % (467 (384); 0 %) |

**Both apps run near their limit**, boosted on every frame. Both RenderThreads take most of a 120 Hz vsync (8.33 ms) at p50, and more at p90 (Compose) or p99 (compose-rs). Compose missed 2 deadlines in its third run (none in round 10); no frame of compose-rs's was janky. Two of compose-rs's runs were presented a vsync late throughout, on cadence (none in round 10, two in round 9), as all of Compose's were.

**compose-rs's frames** moved within the runs' spread of round 10 on both threads (main thread p50 2.24 → 2.23 ms, p99 4.12 → 3.71 ms; RenderThread p50 6.44 → 6.43 ms, p99 8.83 → 8.62 ms): nothing since round 10 touches this screen, which has no lazy list. Compose's main thread takes 63 % longer at the median and twice as long at p99; its RenderThread 15 % longer at the median and 24 % at p99: its faded cells are drawn offscreen ("With `ModulateAlpha`" below).

**compose-rs's slowest RenderThread frames** (9.6 to 23.2 ms, one a run) were again GPU waits (`Vulkan finish frame`, 5.8 to 6.6 ms); the 23.2 ms one spent 7.3 ms drawing, the rest not broken down. None missed a deadline.

### Work

| Stress scroll (10 flings) | Kotlin Compose | compose-rs (round 10) |
|---|---|---|
| Main thread work | 1,194 ms | **573 ms** (−52 %; 713 ms) |
| RenderThread work | 1,594 ms | **1,311 ms** (−18 %; 1,593 ms) |
| Main thread CPU time | 3.88 s | **2.72 s** (2.41 s) |
| RenderThread CPU time | 4.90 s | 4.93 s (4.56 s) |

| Stress animation (10 s) | Kotlin Compose | compose-rs (round 10) |
|---|---|---|
| Main thread work | 2,909 ms | **1,437 ms** (−51 %; 1,440 ms) |
| RenderThread work | 6,009 ms | **3,173 ms** (−47 %; 3,184 ms; Compose with `ModulateAlpha`: 3,286 ms in its session) |
| Main thread CPU time | 4.12 s | **2.64 s** (2.66 s) |
| RenderThread CPU time | 6.94 s | **5.43 s** (5.56 s) |
| HWUI's worker threads (`hwuiTask0`, `hwuiTask1`), CPU time (the second run) | **8.6 s** | 10.9 s (11.0 s) |

**Work** is milliseconds on the fastest core at its highest frequency ([12](12-android-performance.md), "Method"). compose-rs's main thread does half Compose's work on both screens. On the list, its RenderThread does 18 % less work than Compose's, for the same CPU time (4.93 against 4.90 s): less boosted, it runs at lower frequencies. Round 10's per-item prefetch work, and this round's, are in [12](12-android-performance.md), "Item reuse on the device".

**HWUI's two worker threads** still cost compose-rs about 27 % more CPU time than Compose in the animation (10.9 against 8.6 s), as in the first results: reusing paths (D410) didn't change it. They rasterize the 50 stroked waves on the CPU into masks (Skia's software path renderer); why compose-rs's take longer hasn't been found.

### Memory

| | Kotlin Compose | compose-rs (round 10) |
|---|---|---|
| Stress list, PSS 5 s after start | 64.1 MB | **47.3 MB** (50.7 MB) |
| of which Java heap / native heap / code / graphics | 5.3 / 10.9 / 7.4 / 34.5 MB | 1.3 / 14.7 / 2.4 / 23.8 MB (1.3 / 18.1 / 2.3 / 23.8 MB) |
| Stress list, PSS after 10 flings | 113.3 MB | **91.2 MB** (97.8 MB) |
| of which Java heap / native heap / graphics | 14.0 / 22.5 / 63.4 MB | 4.3 / 17.2 / 62.1 MB (5.9 / 22.3 / 62.0 MB) |
| Stress animation, PSS 3 s after start | **167.2 MB** | 180.9 MB (180.4 MB) |
| of which Java heap / native heap / graphics | 13.4 / 20.5 / 119.8 MB | 4.2 / 18.7 / 150.6 MB (4.2 / 18.3 / 150.5 MB) |
| Stress animation, PSS after 13 s | 238.2 MB (runs: 195.6 to 246.9) | **223.3 MB** (218.0 to 224.2) (210.1 MB) |
| of which Java heap / native heap / graphics | 24.0 / 25.8 / 132.1 MB | 8.3 / 23.8 / 183.7 MB (7.8 / 23.9 / 171.3 MB) |

**On the list**, compose-rs holds a quarter less than Compose after start and a fifth less after scrolling. Its native heap is 14.7 MB after start and 17.2 MB after scrolling, against 18.1 and 22.3 MB in round 10: D460 took it back to round 9's (17.1 and 20.8 MB, below), and placing only the items in view (D470) about 3 MB lower, as in its A/Bs (14.4 to 14.7 and 17.2 to 17.4 MB). The Java heap after the flings is 4.3 MB against 5.9 MB.

**Where round 10's extra 1.0 to 1.5 MB went** (D460, measured before D470). The list's first layout has measured only its header (81 px) when it estimates how many items to compose: 87 items, its rows and its carousels' tiles, of which 38 stay; the others leave in the next pass without ever being measured. Until D430 they were disposed. With D430 they went to the reuse pool, 7 per content type: 30 rows with their carousels' first tiles, 531 layout nodes, 3,563 groups and 1,462 slots, about 33 KB a row. The pool then stayed full: scrolling hands each slot leaving to an item coming in the same pass, so nothing else enters it; and the items in it, once reused and measured, kept their paragraphs (the Java heap after the flings was 0.4 MB larger too). Compose composes items as its measurement reaches them, so its pool holds only items that left. A slot whose item leaves unmeasured is now disposed, as before D430. A flag D430 added to every group, written and never read, had also grown `Group` from 144 to 152 bytes; it's gone (D461).

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

**In the animation**, the medians changed places again: compose-rs's 223.3 MB after 13 s is below Compose's 238.2 MB this round, against 210.1 and 195.8 MB in round 10. Neither app changed on this screen. Compose's memory swings with its garbage collector: its runs ranged over 195.6 to 246.9 MB this round (190.6 to 284.7 MB in round 10), its Java heap 24.0 MB at the median against 12.6 MB. compose-rs's runs ranged over 218.0 to 224.2 MB (194.8 to 214.5 MB). compose-rs grows by 42 MB over the 10 s, Compose by 71 MB at the median; compose-rs holds about 50 MB more graphics memory than Compose (183.7 against 132.1 MB; 171.3 MB in round 10): HWUI's scratch surfaces and buffers, whose level varies from session to session ("With paths reused" below).

### Startup

| | Kotlin Compose | compose-rs (round 10) |
|---|---|---|
| `stress` (`TotalTime`, 15 cold starts) | 143 ms (132 to 170) | **118 ms** (111 to 126; −17 %) (126 ms) |
| The first frame's main thread (`traversal`, one traced start) | 58.1 ms | **37.1 ms** (42.7 ms) |
| `bindApplication` | 17.5 ms | **12.6 ms** (12.6 ms) |

compose-rs starts 8 ms sooner than in round 10, as in the A/Bs of D470–D472 (`TotalTime` −7 ms in the first, 125.5 → 118.5 ms). Its first frame composes what it shows: its own part (`compose-rs: frame`) took 6.6 ms against 13.8 ms, 5.0 ms of it in 3 rounds of composing and laying out again, and laid out 29 paragraphs (3.5 ms), where it composed 87 items and laid out 69 paragraphs before D472 (the A/Bs' count). Its replay took 2.9 against 3.9 ms (it makes the bitmaps) and `Record View#draw()` 4.4 against 5.3 ms. Compose's first frame took 58.1 ms in this run's traced start (58.5 ms in round 10's, 64.3 ms in the first results': single traces).

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
- **In round 9** (with D390 as well, which neither build here had), compose-rs's main thread p50 / p90 / p99 was 2.36 / 2.97 / 3.99 ms for 1,444 ms of work, `compose-rs: tasks` 0.36 ms a frame, and the RenderThread's p50 6.34 ms: the gains held, with a p99 below both builds' here. Round 10 measured 2.24 / 2.82 / 4.12 ms, 1,440 ms of work and 0.35 ms of tasks a frame; round 11 ("Results" above) 2.23 / 2.77 / 3.71 ms and 1,437 ms of work.

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
- **Beside Compose (round 10).** The list's main thread p50 / p90 / p99 was 1.02 / 1.63 / 2.86 ms against Compose's 1.29 / 3.32 / 7.76 ms, 21 % of its time on the little cores. No `doFrame` waited for a prefetch step, and no run of compose-rs's had a compositor-side janky frame, on any screen.
- **Round 11 ("Results" above).** With only the items in view placed (D470), the RenderThread has less work, its reports vary less and the vote is lower (190 against 295). The main thread shares it, and ran 46 % of its time on the little cores, for a p50 / p90 / p99 of 1.08 / 1.62 / 2.75 ms against Compose's 1.25 / 3.33 / 7.61 ms; 2 `doFrame`s waited for a prefetch step in 3 runs. Again no compositor-side janky frame of compose-rs's, on any screen.

## The lazy list's policy (D470–D474)

Until D470 a lazy list composed and placed a viewport's worth of items beyond each side of the viewport: about 7 items on this list (and their carousels' tiles), 5 on the feed. Each placed item is a render node the main thread replays and the RenderThread syncs every frame. Its first layout also estimated the items it lacked from the only one it had measured, this list's 81 px header, and composed 87 items (rows and tiles) of which 38 stayed (D460). The changes:

- **D470**: a list composes and places only the items in view, as Compose's; a focus search leaving it asks for the next items one at a time (D477, after a review: D470 first kept a viewport's worth beyond each side while an item held the focus). Prefetch runs from the first layout on, not only after a scroll, so a fling's first frames find the next two items composed. A frame still keeps the two composed items next to those it wants, for a reversal.
- **D472**: while a list knows fewer than 4 item sizes, a measurement lacking items asks for 4 at most (or as many as it placed), and the frame's next round asks for the rest.
- **D471, D474**: two fixes, each of which kept prefetch from running in the idle time after some frames. The list read `layout_info` while measuring, which measured it twice and asked for another frame whenever the average size changed; and a frame that composed in a round asked for another frame. A frame with work pending skips prefetch, so the next frame had to compose its items itself.

**What a viewport's worth bought.** (Results: `results/2026-10-23-pixel-8-pro-lazy-policy.json`.) First, an interleaved A/B on the Pixel 8 Pro, 12 runs each of the feed's and this list's scroll (10 flings), of four variants: main (727d266), and with D471 and D472 a viewport's worth, two items beyond (placed), and none. In-frame composition was the same in all four: no text laid out inside a frame on the feed, and on this list one in 12 runs with two beyond, none otherwise. This list had 10 rounds a run (`recompose and layout again`) in all four, which laid out no text: most likely the sticky header changing (a state the items' composition reads, written as a section's first item scrolls past). Placing fewer items cut the work of both threads (medians of the runs):

| Stress scroll | main | a viewport's worth | two beyond | none (shipped) |
|---|---|---|---|---|
| Main thread work | 677 ms | 678 ms | 639 ms | **589 ms** |
| RenderThread work | 1,543 ms | 1,551 ms | 1,451 ms | **1,345 ms** |
| Main thread p50 / p90 / p99 | 1.05 / 1.66 / 2.93 ms | 1.04 / 1.63 / 2.86 ms | 0.99 / 1.60 / 2.79 ms | 1.01 / 1.55 / 2.76 ms |
| RenderThread p50 / p90 / p99 | 2.56 / 3.59 / 4.97 ms | 2.48 / 3.54 / 5.11 ms | 2.46 / 3.48 / 5.00 ms | 2.46 / 3.51 / 5.04 ms |
| Native heap after the flings | 20.5 MB | 19.7 MB | 18.1 MB | **17.3 MB** |

On the feed the same order held: RenderThread work 829, 811, 789 and 747 ms. There, with none beyond, the RenderThread's p50 was 2.42 ms against main's 2.16 ms in this session, for 10 % less work at lower clocks: its CPU time didn't change (4.39 and 4.51 s), and 21 % of its time was on the little cores against 17 %. Its p90 and p99 were shorter (3.21 / 4.72 against 3.29 / 4.85 ms). The confirming A/B below didn't show the p50 difference (2.33 → 2.24 ms). So a viewport's worth saved no composition in these flings, and none beyond shipped. A review then found what it had hidden, fixed by D475–D478: scrolls onto a large item out of view were cut at an estimated room, first frames under a large first item could be short, focusing a field composed a viewport's worth in a frame, and accessibility couldn't scroll a list ("After the review" below).

**The shipped code against main**: interleaved, 12 runs each, scroll and cold starts (medians of the runs; the better side in bold where the runs' ranges barely overlap or not at all):

| | Feed: main | shipped | Stress list: main | shipped |
|---|---|---|---|---|
| Janky frames, the app's / the compositor's (12 runs) | 0 / 0 | 0 / 0 | 0 / 2 | 0 / 2 |
| Frames presented a vsync late (12 runs) | 0 | 0 | 127 | 3 |
| Main thread p50 / p90 / p99 | 1.23 / 1.76 / 2.72 ms | **1.16** / **1.68** / 2.73 ms | 1.06 / 1.68 / 2.86 ms | 0.98 / **1.53** / 2.70 ms |
| RenderThread p50 / p90 / p99 | 2.33 / 3.40 / 4.79 ms | 2.24 / 3.26 / 4.86 ms | 2.56 / 3.63 / 4.98 ms | 2.51 / 3.56 / 5.09 ms |
| Main thread work | 483 ms | 468 ms | 681 ms | **591 ms** |
| RenderThread work | 810 ms | **772 ms** | 1,556 ms | **1,333 ms** |
| Frames with an in-frame round; text laid out in frames (12 runs) | 0; 0 | 0; 0 | 120; 1 | 120; 1 |
| Prefetch steps composing, a run | 347 | 348 | 345 | 351 |
| Native heap after start / after the flings | 8.5 / 11.8 MB | **8.1** / **10.6** MB | 17.2 / 20.5 MB | **14.4** / **17.3** MB |
| PSS after start / after the flings | 34.3 / 79.2 MB | 33.9 / 77.9 MB | 49.8 / 95.6 MB | **47.0** / **91.4** MB |
| `TotalTime` (24 cold starts) | 108.5 ms | 104.0 ms | 125.5 ms | **118.5 ms** |
| The first frame's `compose-rs: frame` (12 traced starts) | 6.7 ms | **5.1 ms** | 14.5 ms | **7.2 ms** |

The big screen's start didn't change (162 ms). The RenderThread's p99s are 0.07 and 0.11 ms longer, within the runs' spread (4.54 to 5.47 against 4.61 to 5.05 ms on the feed, 4.52 to 5.81 against 4.45 to 5.86 ms here), and were 0.13 ms shorter on the feed and 0.07 ms longer here in the first A/B. Our hint session voted less for the RenderThread, which had less work (here a time-weighted 212 against 252).

The first frame now composes 16 items where it composed 87, and lays out 29 paragraphs where it laid out 69. Its rounds take 5.5 against 12.7 ms. Headless, the list's Rust heap peaks at 2,288 KiB at start, against 6,677 KiB (live: 2,268 against 5,434 KiB), and is 3,160 KiB after 10 flings, against 5,792 KiB.

**After the review (D475–D478).** A review of D470–D474 found three regressions, fixed since:
- **D476**: scrolls past an estimated end are no longer cut there. With only the items in view placed, a `scroll_by` onto a large item out of view, or a fling back to a large first item, stopped short.
- **D475**: first frames are full again. A large first item made the estimate run the 3 rounds out, short of the viewport.
- **D477**: a focus search asks the list for the next item beyond its bounds, as Compose's, instead of a viewport's worth composed while an item holds the focus. Focusing a field in a list composed 16 to 36 items in a frame.

It also found that TalkBack couldn't scroll a list. Lists, grids and scrolling containers now have scroll semantics, and Android exposes them as scrollable with scroll actions; nodes clipped away aren't visible to the user (D478).

Interleaved again against main (727d266), 8 runs each:

| | Feed: main | shipped | Stress list: main | shipped |
|---|---|---|---|---|
| Janky frames, the app's / the compositor's (8 runs) | 0 / 0 | 0 / 2 | 0 / 2 | 0 / 4 |
| Main thread p50 / p90 / p99 | 1.23 / 1.75 / 2.75 ms | **1.17** / **1.67** / 2.71 ms | 1.04 / 1.65 / 2.80 ms | 1.01 / 1.57 / 2.74 ms |
| RenderThread p50 / p90 / p99 | 2.31 / 3.38 / 5.08 ms | 2.35 / **3.23** / **4.72** ms | 2.49 / 3.61 / 5.01 ms | 2.51 / 3.60 / 5.31 ms |
| Main thread / RenderThread work | 490 / 814 ms | **459** / **755** ms | 680 / 1,562 ms | **574** / **1,316** ms |
| Text laid out in frames (8 runs) | 0 | 0 | 0 | 3 |
| Native heap after start / after the flings | 8.5 / 12.0 MB | **8.1** / **10.6** MB | 17.1 / 20.5 MB | **14.4** / **17.2** MB |
| `TotalTime` (16 cold starts) | 108.0 ms | 105.0 ms | 123.0 ms | 121.0 ms |
| The first frame's `compose-rs: frame` (8 traced starts) | 6.7 ms | **5.0 ms** | 14.3 ms | **7.3 ms** |

- **The wins hold:** less work on both threads, less memory, and a first frame half as long.
- **The stress list's RenderThread p99 is 0.30 ms longer.** The runs' ranges overlap (4.68 to 5.53 against 4.50 to 5.42 ms). The earlier A/Bs gave +0.07 and +0.11 ms.
- **It laid out text in 3 frames in 8 runs**, where main laid out none in 8 and 1 in 12 in the first A/B; too few to tell from noise.
- **The compositor's janky frames** (4 against 2) aren't the app's.
- **Startup:** `TotalTime` is within the starts' spread here (−2 and −3 ms).

**After the third review (D479–D485).** A third review found seven more bugs, fixed since:
- **Nested scrolling (D479).** Inside a nested-scroll parent (a `Scaffold`'s app bar connection, or a `LazyColumn` around a carousel), a delta was still cut at the estimated room. Now the list takes the whole delta, and what it couldn't use is offered to the parents once laid out, in the same frame.
- **Emptied lists (D480).** An emptied list took scroll.
- **Cards and dividers (D481).** With 1 px dividers between 300 px cards, the first frame composed 805 items to show 15.
- **Search leftovers (D482).** Rows laid out for a focus search stayed composed.
- **Nested focus searches (D483).** Searches didn't climb from a carousel's row to its list, and a spaced list's Down spun over 200 rows.
- **Accessibility directions (D484).** Accessibility scrolled reversed and right-to-left lists the wrong way.
- **Focus timing (D485).** The row a focus search found showed a frame late.

Interleaved against main (727d266), 8 runs each:

| | Feed: main | shipped | Stress list: main | shipped |
|---|---|---|---|---|
| Janky frames, the app's / the compositor's (8 runs) | 0 / 4 | 0 / 0 | 0 / 0 | 0 / 0 |
| Main thread p50 / p90 / p99 | 1.22 / 1.75 / 2.75 ms | **1.16** / **1.66** / 2.73 ms | 1.07 / 1.68 / 2.88 ms | **0.98** / **1.54** / 2.79 ms |
| RenderThread p50 / p90 / p99 | 2.20 / 3.34 / 4.97 ms | 2.32 / 3.21 / 4.66 ms | 2.62 / 3.68 / 5.02 ms | 2.40 / 3.57 / 5.34 ms |
| Main thread / RenderThread work | 497 / 829 ms | **462** / **759** ms | 689 / 1,552 ms | **595** / **1,348** ms |
| Text laid out in frames (8 runs) | 0 | 0 | 0 | 0 |
| Native heap after start / after the flings | 8.5 / 12.0 MB | **8.1** / **10.6** MB | 17.2 / 20.4 MB | **14.7** / **17.2** MB |
| `TotalTime` (16 cold starts) | 104.5 ms | 105.0 ms | 126.5 ms | **119.5 ms** |
| The first frame's `compose-rs: frame` (8 traced starts) | 6.7 ms | **5.0 ms** | 14.6 ms | **7.2 ms** |

- **Most numbers hold or improve.**
- **The stress list's RenderThread p99 is longer again:** 5.34 against 5.02 ms here, +0.30 ms in the run before. The ranges overlap (4.74 to 5.47 against 4.27 to 5.53 ms) but the shift recurs, so it may be real. The RenderThread does 13 % less work, and its p50 and p90 are shorter; with less work, our hint session votes less for it (D441).
- **Grid fast flings, again (4 runs each, interleaved):**
  - compose-rs composed items in 0 to 3 frames a run, with no janky frame and a main thread p99 of 2.9 to 3.0 ms.
  - Compose laid out new tiles' text in 72 to 118 frames a run, with a p99 of 5.6 to 6.8 ms and 2 app-janky frames.

**After the fourth review (D486–D491).** The fourth review found five bugs, fixed since:
- **A Tab that never ended (D486).** A Tab from a carousel ending in unfocusable tiles waited forever; every focus search now ends within about 25 frames.
- **Over-composition (D487).** A page down on cards, spacers and dividers composed 800 items in a frame. What a frame asks for now doubles round to round, bounded at about twice what it needs.
- **A phantom delta (D488).** An item inserted above during a drag sent one to the app bar.
- **Grids (D490).** A grid dropped what its estimated end left instead of passing it to its parents.
- **Right-to-left accessibility (D491).** Accessibility's forward and backward in right-to-left rows were the opposite of Compose's; the mapping is now Compose's, in Rust.

Interleaved against main (727d266), 8 runs each:
- One of main's feed traces held no frame, so its feed column has 7 runs.
- Neither app had a janky frame in any run.

| | Feed: main | shipped | Stress list: main | shipped |
|---|---|---|---|---|
| Main thread p50 / p90 / p99 | 1.24 / 1.75 / 2.74 ms | **1.17** / **1.67** / 2.64 ms | 1.06 / 1.66 / 2.79 ms | **0.96** / **1.53** / 2.70 ms |
| RenderThread p50 / p90 / p99 | 2.35 / 3.40 / 4.89 ms | 2.33 / 3.20 / 4.73 ms | 2.47 / 3.54 / 4.95 ms | 2.42 / 3.43 / 4.96 ms |
| Main thread / RenderThread work | 490 / 814 ms | **460** / **755** ms | 703 / 1,589 ms | **596** / **1,366** ms |
| Text laid out in frames (runs) | 0 | 0 | 1 | 0 |
| Native heap after start / after the flings | 8.5 / 11.9 MB | **8.1** / **10.5** MB | 17.2 / 20.6 MB | **14.7** / **17.4** MB |
| `TotalTime` (16 cold starts) | 106.5 ms | 105.5 ms | 123.5 ms | **116.5 ms** |
| The first frame's `compose-rs: frame` (8 traced starts) | 6.8 ms | **5.0 ms** | 14.5 ms | **7.0 ms** |

**The stress list's RenderThread p99** came out 0.3 ms longer in the two A/Bs before this one, and equal here (4.96 against 4.95 ms). It's the boost, not more work:
- **Faster at every boost level.** At the same boost, the shipped code's RenderThread frames are faster. In the A/B before this one, p99 was 6.45 against 6.82 ms unboosted, 4.91 against 5.75 ms at 100–199 and 3.94 against 4.33 ms at 200 or more; this one, 6.01 / 4.90 / 3.73 against 6.47 / 5.35 / 4.37 ms.
- **Less boosted.** With 13 to 14 % less RenderThread work, our hint session voted less: 230 against 254, then 226 against 268, time-weighted. Fewer frames started at 200 or more (58 against 69 %, then 60 against 73 %), and more ran unboosted.
- **Still drawn ahead.** Prefetched items are drawn and recorded ahead as before: about 213 draw-ahead steps a run against 210. The replay takes less time (100 against 144 ms a run).
- **Fewer render nodes.** The frame records no more render nodes, it places fewer.

**Grid fast flings (4 runs each, interleaved):**
- compose-rs composed items in 0 to 3 frames a run, with no janky frame and a main thread p99 of 2.7 to 2.9 ms.
- Compose laid out new tiles' text in 99 to 143 frames a run, with a p99 of 6.2 to 6.7 ms and 2 app-janky frames.

The stress list's first frame still composes 16 items (headless 0.25 ms).

**After the fifth review (D492–D496).** The fifth review found five more bugs, fixed since. None touches a steady scroll, so no device A/B was run:
- **Long scrolls (D492).** A `scroll_by` of 160 rows, or a `scroll_to_item` with a large offset, landed over 2 or 3 frames, an item in between shown: a scroll past the composed items placed none, so each round asked for 4. What a round asks for now doubles from the last.
- **Flings handed on (D493).** What a list leaves of a fling goes to its parents as a fling, as Compose's; a fast fling to the top of a list in a `ModalBottomSheet` dragged the sheet down.
- **Overshoot (D494).** The distance an overshoot measures counts items not measured yet at the average size, rather than dropping the rest.
- **Focus (D495).** A focus search waiting for items beyond a list's bounds ends when the focus moved meanwhile from outside it (a click).
- **Accessibility pages (D496).** A scroll action's page is the viewport as shown, its width horizontally and its height vertically.

**In round 11** (613eaf6, beside Compose: "Results" above), the full run kept what the A/Bs measured: on this list, main thread and RenderThread work 573 and 1,311 ms (round 10: 713 and 1,593 ms), native heap 14.7 and 17.2 MB (18.1 and 22.3 MB), `TotalTime` 118 ms (126 ms), the first frame's `compose-rs: frame` 6.6 ms (13.8 ms), and no text laid out in a frame in any run, on either list. As in the A/Bs, our session voted less for a RenderThread with less work, and on this list its p90 and p99 came out longer than Compose's (3.81 / 5.45 against 3.63 / 5.27 ms), though its frames are shorter at each boost level.

**Lazy grids** keep 2 lines composed and placed beyond each side (D456). Their open risk was that a fast fling would outrun that buffer. Checked on the new `grid` screen of both apps (D473): 3,000 tiles in 4 columns, 10 flings of 1,600 px in 60 ms as measure.py's pattern, 3 to 4 runs each, interleaved.
- Before D474, compose-rs composed items in 0 to 11 frames a run (mean 4.3, 6 runs), in runs of consecutive frames at the flings' starts with no prefetch between them. With D474, 0 to 2 a run (mean 1.0, 4 runs).
- No janky frame of compose-rs's; its main thread p99 was 2.6 to 3.0 ms.
- Compose laid out new tiles' text in 79 to 136 of its frames a run (7 runs), with a main thread p99 of 5.8 to 6.9 ms, and had 5 app-janky frames in 10 runs.
- So the grid's beyond and prefetch stay as they were.

## Next steps

- **Paths in the bridge**: done (D410, D411, "With paths reused"). HWUI's worker threads still take about 27 to 30 % more CPU time than Compose's in the animation (10.9 against 8.6 s in round 11, 11.0 against 8.5 s in round 10, 11.2 against 8.6 s in round 9), unchanged by D410; why isn't known.
- **Infinite transitions**: done (D420–D424). The 0.4 ms of tasks a frame left is mostly easing curves and color conversions.
- **The stress list's main thread on the little cores**: done (D440, "The main thread in the hint session"). The main thread joined compose-rs's hint session: in its A/B, on the little cores 79 → 28 % of its time, p50 1.54 → 1.07 ms, `doFrame`s waiting for a prefetch step 58 → 4 in 12 runs, no power change resolved. In round 10, 21 % on the little cores, p50 1.02 ms, no `doFrame` waiting for a prefetch step, and no frame laying a newly revealed item out itself (1 to 3 a run in round 9). In round 11, with less work to report after D470 and so a lower vote, 46 % on the little cores, p50 1.08 ms (Compose 1.25 ms), 2 `doFrame`s waiting for a prefetch step in 3 runs, and still no frame laying an item out itself. If those frames come back, splitting an item's measurement so a step can stop at the deadline ([12](12-android-performance.md), "Next steps") is the fix.
- **Item reuse** (D430–D435): measured on the device for the first time in round 10, in the same build as D440, whose faster cores raise the work metric; a prefetched item's composition did about the same work as in round 9 (0.167 against 0.163 ms) and half as many paragraphs were laid out ([12](12-android-performance.md), "Item reuse on the device"). An interleaved A/B would separate the two; in round 11 an item's composition did 0.132 ms of work, with the main thread back on the little cores 46 % of its time, where the metric reads lower. The list's native heap was 1.0 to 1.5 MB larger than in round 9: items the first layout composed beyond those it wanted, kept for reuse; fixed (D460, "Memory").
- **The RenderThread's boost on the list** varies with the session and, since D470, follows the lower work: our session's vote averaged 324 in round 9, 247 to 257 in D440's A/B, 295 in round 10 and 190 in round 11, for RenderThread p50s of 2.20, 2.49, 2.49 and 2.58 ms (Compose: 2.62, 2.63 and 2.63 ms in rounds 9 to 11). In round 11 its p90 and p99 were longer than Compose's (3.81 / 5.45 against 3.63 / 5.27 ms), though shorter at each boost level: the one place compose-rs is behind on these screens. A lower target, or reporting both threads' CPU time against 8.33 ms (D442's variant: RenderThread p50 2.09 ms here in its A/B, for +2.6 % CPU-rail power), would buy it back with power.
- **The lazy list's policy**: done (D470–D496, "The lazy list's policy"). Only the items in view are placed, prefetch runs from the first layout on, and the first layout composes 16 items where it composed 87. In its first A/B, main thread work −13 %, RenderThread work −14 %, native heap −3 MB, `TotalTime` −7 ms; in round 11 against round 10, −20 %, −18 %, −5 MB and −8 ms. The 10 in-frame rounds a run that remain laid out no text; headless, the only items flings compose in frames are sticky headers, when a scroll back enters the section before (as Compose's would).
- **Shader warm-up**: a pipeline compiled on first use cost compose-rs its one janky scroll frame in the first results (none in rounds 9 to 11). Compose's apps share the same cache behavior; it isn't specific to compose-rs, but it showed here.
