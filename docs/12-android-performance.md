# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in jank, per-frame durations and work, startup and memory.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring; every round so far used `speed` for both apps (recorded since round 4). That isn't Compose's best case for startup: with a profile (`-m speed-profile`, as a Baseline or Cloud Profile gives, with an app image), Compose's feed starts in 113.5 ms against 129 ms with `speed`, so compose-rs (105.5 ms) leads by 8 ms rather than 23.5 ([14](14-performance-opportunities.md), "Compilation modes"; compose-rs's startup doesn't depend on the mode). `measure.py --compile speed-profile` measures that way.

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it. Since D302, it first checks that the installed apps' native libraries for the device's ABI are those built in the checkout (and stops if not), and records each APK's size and SHA-1, the compilation filter (`--compile`, `speed` by default) and each app's compilation state (`dumpsys package dexopt`) at the start and the end of the run. Its parsing is tested on captured output: `python3 android/bench/test_measure.py`.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler. Since D343, a warm-up swipe comes first and is left out (the stats start at the first input after it; gfxinfo is reset after it): the display, idle since the app started, resyncs its vsync on the first input, which FrameTimeline counted as jank no app caused. Each run also reports its janky frames by side (the app's deadline, or the compositor's: SurfaceFlinger's deadlines, prediction errors, the display), the frames presented at another vsync than expected (`present_type`, buffer stuffing's late frames included), the `doFrame`s starting more than 2 ms after their vsync (about 1.1 ms normally), and of those the ones that work on the main thread delayed (a slice ending more than 1.2 ms after the vsync: idle-time work, input; the rest woke late), compose-rs's `prefetch` calls, and whether Choreographer entered buffer stuffing recovery (its async track, or a `buffer stuffed` slice): `summarize.py` prints a line per run.
- **Isolation**: before each run, measure.py force-stops our other apps (`rs.compose.*` but the one measured): the other benchmark app, left cached by the run before, and the demo, whose animation ran a frame on every vsync in the background before D342.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) raise the cores' frequencies after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on; durations are what a user sees.

Measurements drift between sessions: Compose's own numbers move by up to 5 to 8 % from one session to the next (its scrolling main-thread work was 827 ms in round 3, 760 ms in round 4, 844 ms in round 5). So the two apps are compared within a session, and changes to compose-rs are measured on their own with interleaved A/B runs ([14](14-performance-opportunities.md)); a difference between rounds smaller than that is drift.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians of 3 runs (startup: of 15). The better side is in bold; ties aren't.

"Round 5" is compose-rs at 6f31c6e (`results/2026-10-22-pixel-8-pro-round-5.json`), the current results. It is the first run with D343's measure.py: our other apps stopped before each run, the scroll's warm-up swipe left out of the stats, and janky frames split by side. measure.py's checks (D300–D302) passed: the installed native libraries matched those built, and both apps were `speed` at the start and the end. "Round 4" is compose-rs at 84a79be (`results/2026-10-22-pixel-8-pro-round-4.json`); its numbers are quoted in parentheses where they show the change. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers are from the round 5 run.

**Jank counts aren't comparable one for one with earlier rounds.** Round 5's scroll leaves out the warm-up swipe, and with it the frame where the display resyncs its vsync after being idle, which FrameTimeline counted in every earlier round.

Since round 4:

- **D330**: single-line texts are laid out with `BoringLayout`, as Compose's `TextLayout` does, instead of measuring their width and building a `StaticLayout`; about 5 in 6 paragraphs take it. Measured on its own: the big screen's first-frame text layout −29 %, the scroll's main-thread work −6 % ([14](14-performance-opportunities.md), #14).
- **D340**: the prefetch budget runs to the real vsync, on the frame time's phase, so it survives Choreographer's buffer stuffing recovery, which shifts every frame time back an interval. Round 4's slow run lost its prefetching that way.
- **D341**: a prefetch step starts only if the larger of its running average and its recent maximum fits, with 0.5 ms to spare: fewer frames start late behind idle-time work.
- **D342**: a hidden view asks for no frame. The demo's animation ran a frame on every vsync in the background.
- **D343**: measure.py's hygiene (see "Method").

### Scrolling: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,788 | 1,777 |
| Janky frames (FrameTimeline), per run | 0, 1, 0 | **0, 0, 0** (round 4: 1, 3, 1) |
| of which the app's | 0, 1, 0 | **0, 0, 0** (0, 0, 0) |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 (1, 3, 1) |
| Frames presented at another vsync than expected, per run | 0, 2, 0 | **0, 0, 0** |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 28 (7), 32 (9), 22 (4) | **10 (0), 10 (0), 9 (0)** |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / **6** / **8** ms (5 / 6 / 8 ms) |
| Main thread per frame p50 / p90 / p99 | 1.56 / 3.22 / 7.03 ms | **1.30** / **1.92** / **3.04** ms (1.23 / 1.90 / 3.11 ms) |
| RenderThread per frame p50 / p90 / p99 | **2.13** / **3.56** / **5.17** ms | 2.71 / 3.81 / 5.68 ms (2.74 / 3.70 / 5.17 ms) |

Both scroll at 120 Hz. compose-rs had no janky frame in any run, on either side. Compose's one was a missed app deadline, and two of its frames were presented a vsync late. No run of either app entered buffer stuffing recovery, and compose-rs prefetched 365 to 370 times a run.

Round 4's 5 janky frames of compose-rs's were all on the compositor's side ([14](14-performance-opportunities.md), "Jank"):

- **The vsync resync.** Each run had one prediction error on its first fling: the display, idle since the app started, resyncs its vsync on the first input. measure.py now leaves a warm-up swipe out of the stats (D343), so round 5 doesn't count that frame.
- **A display stall.** One run also had 2 SurfaceFlinger CPU deadlines missed when the display driver stalled. Choreographer then entered buffer stuffing recovery and shifted every frame time back an interval for the rest of the run. The prefetch budget, counted from that time, was always past: that run made 69 prefetches against 318, made 120 compositions in frames, and its main thread's p99 was 7.35 ms. D340 counts the budget from the real vsync. Simulated recovery now prefetches as if nothing were shifted (main p99 3.19 against 7.36 ms).

The main thread's frames are shorter than Compose's at every percentile, the slowest by more than half (p99 3.04 against 7.03 ms; 8.57 ms in round 1). Prefetching composes and lays out the next item between frames (D156) and records its paragraphs (D322). So the frame that reveals an item composes nothing, lays no text out and records no paragraph. The percentiles are round 4's within the runs' spread: D330 takes work out of prefetching, in idle time, not out of frames.

`doFrame`s start late less often than Compose's (9 or 10 a run against 22 to 32), and none behind compose-rs's own main-thread work: since D341, an idle-time step that may not fit before the vsync waits. The rest woke late, which is the scheduler's doing. Before D341, 0 to 6 a run were delayed by idle work (3.3 on average, against 1.0 with D341's first version, interleaved: [14](14-performance-opportunities.md), "Jank").

The RenderThread's frames take longer than Compose's (p50 2.71 against 2.13 ms) though it does less work (below): without long frames, the scheduler keeps it on the little cores and ADPF doesn't raise their frequency (see "Method"). Its p99 rose since round 4 (5.17 → 5.68 ms); the interleaved A/Bs of D330 and of D340–D342 each moved it by less (5.33 → 5.49, 5.27 → 5.55 ms), within their runs' spread. Frame times are unaffected: gfxinfo's p99 is 8 ms, against Compose's 11.

### Animation: frames (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,350 | 1,348 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 |
| Frames presented at another vsync than expected, per run | 1,204, 0, 1,207 | **0, 0, 0** |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 4 (0), 5 (1), 10 (3) | 6 (0), 8 (1), 4 (0) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / **7** ms |
| Main thread per frame p50 / p90 / p99 | 1.56 / 2.25 / 3.28 ms | **1.21** / **1.69** / **2.76** ms (round 4: 1.20 / 1.79 / 2.98 ms) |
| RenderThread per frame p50 / p90 / p99 | 2.90 / 4.10 / 5.81 ms | **2.64** / **3.38** / **5.03** ms (2.63 / 3.56 / 5.16 ms) |

Neither app had a janky frame. In two of Compose's runs, every frame was presented a vsync later than expected, on cadence: a frame more latency, without a stutter, which FrameTimeline doesn't count as jank (the cause wasn't investigated).

compose-rs's frames are shorter on both threads. The main thread's median is 22 % shorter than Compose's. Each animation frame draws again only the repaint boundary whose content changed (D155), the spinner's group alone (D323). The pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The slowest main-thread frames are the system's (vsync requests, waking and waiting for the RenderThread), the same in Compose, so p99 moves least. The RenderThread's frames are 9 % shorter at the median, 18 % at p90: the spinner's arc is one `CircleOp`, as in Compose (D320). The changes since round 4 don't touch the animation; its numbers moved within the drift between sessions.

### Work per frame

| Scrolling (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 844 ms | **452 ms** (−46 %; round 4: 477 ms) |
| RenderThread work | 878 ms | **813 ms** (−7 %; 786 ms) |
| Main thread CPU time | 4.55 s | **3.45 s** (3.18 s) |
| RenderThread CPU time | **4.47 s** | 5.05 s (4.96 s) |

| Animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 258 ms | **190 ms** (−26 %; round 4: 190 ms) |
| RenderThread work | 544 ms | **501 ms** (−8 %; 517 ms) |
| Main thread CPU time | 2.13 s | **1.56 s** (1.53 s) |
| RenderThread CPU time | 3.68 s | **3.34 s** (3.32 s) |

compose-rs's main thread does 46 % less work than Compose's when scrolling, 26 % less in the animation. The scroll's fell by 5 % since round 4 (477 → 452 ms), as `BoringLayout` (D330) measured on its own (−6.4 %): text layout through JNI is about half of a prefetched item's work. Compose's own scroll work was higher this session (811 to 882 ms across its runs, 760 ms in round 4), hence −46 % against round 4's −37 %.

The RenderThread does less work than Compose's in both scenarios. Scrolling, its CPU time is higher than Compose's for less work: the little cores again. Its scroll work rose since round 4 (786 → 813 ms), as Compose's did (834 → 878 ms).

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 131 ms | **110 ms** (−16 %; round 4: 107 ms) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 44.2 ms | **25.4 ms** (29.1 ms) |
| `big`: 200 posts at once | 226 ms | **163 ms** (−28 %; 174 ms) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 165.9 ms | **92.1 ms** (104.6 ms) |

Compose's numbers are under `speed`. With a profile (`speed-profile`), its feed starts sooner: 113.5 ms against 129 in another session, where compose-rs's took 105.5 ms; `big` didn't change (229 against 227 ms) ([14](14-performance-opportunities.md)). The feed's 3 ms against round 4 are within `TotalTime`'s noise (a standard deviation of 3 to 7 ms).

`big` starts 11 ms sooner than in round 4, with `BoringLayout` (D330). In its first frame (92.1 ms), composing 200 posts takes 18.6 ms, laying them out 35.0 ms (46.0 ms in round 4), of which 30.1 ms is text (41.2 ms: 1,200 paragraphs through JNI, 1,000 of them now single lines), drawing and serializing the display list 1 ms, and replaying it on the `Canvas` 20.5 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 36.6 ms (`Record View#draw()`). `bindApplication` is shorter too (12 to 14 ms against 16 to 17 ms): there is no Compose runtime to initialize.

### Memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 51.8 MB | **33.7 MB** (−35 %; round 4: 33.2 MB) |
| of which code | 6.5 MB | **2.4 MB** (1.8 MB) |
| of which Java heap | 3.6 MB | **1.3 MB** (1.4 MB) |
| of which native heap | **7.9 MB** | 8.1 MB (8.0 MB) |
| of which graphics | 27.9 MB | **17.0 MB** (17.0 MB) |
| PSS after 10 flings | 90.0 MB | **79.5 MB** (81.2 MB) |
| of which Java heap | 7.4 MB | **5.1 MB** (6.3 MB) |
| of which native heap | 13.2 MB | **11.8 MB** (12.7 MB) |
| of which graphics | 56.6 MB | **55.1 MB** (55.3 MB) |
| APK (release; decimal MB) | **1.74 MB** (minified) | 1.99 MB, with the 1.89 MB `.so` uncompressed inside (1.99 MB) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). Both apps' code rose since round 4 (6.1 → 6.5 MB, 1.8 → 2.4 MB), most likely because PSS divides a shared page among the processes that map it, and measure.py now stops the other benchmark app (D343). After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (89.1 to 93.8 MB in this run, its Java heap 6.5 to 10.9 MB, and up to 112 MB in earlier ones: it allocates on every frame); compose-rs's stay within 79.1 to 80.6 MB.

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots).

The APK is still larger than Compose's: the library is 1.89 MB stored uncompressed (so it can be mapped), against Compose's minified dex. A release profile for Android (fat LTO, one codegen unit, `panic = "abort"`) would make it 1.50 MB ([14](14-performance-opportunities.md), #4b), with a behaviour change and longer builds.

### How the gaps closed

Compose's numbers are round 5's.

| Gap | Steps |
|---|---|
| Scrolling, janky frames per run (Compose: 0, 1, 0) | 1, 2, 0 in round 1, all the app's; 1, 0, 1 in round 2; 0, 0, 0 in round 3; 1, 3, 1 in round 4, all the compositor's (a vsync resync, a display stall); 0, 0, 0 with the prefetch budget on the real vsync and the warm-up swipe left out (D340, D343, round 5) |
| Scrolling, main thread p99 (Compose: 7.03 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3; 3.11 ms with prefetched paragraphs recorded while idle (D322, round 4; p90 2.20 → 1.90 ms); 3.04 ms in round 5 |
| Animation, main thread per frame p50 (Compose: 1.56 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3); 1.20 ms with a replay without interpreted calls and records naming nested groups (D321, D323, round 4); 1.21 ms in round 5 |
| Animation, RenderThread work (Compose: 544 ms) | 505 ms in round 1; 535 ms in round 2 and 540 ms in round 3, even with Compose's (a square-capped spinner, a stroked path each frame); 517 ms with the round cap (D320, round 4); 501 ms in round 5 |
| Scrolling, RenderThread work (Compose: 878 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3; 786 ms in round 4; 813 ms in round 5 |
| Startup, `big` (Compose: 226 ms) | 186 ms in round 1; 178 ms in round 2; 179 ms in round 3; 174 ms with composition linear in its reads (D310, round 4); 163 ms with `BoringLayout` (D330, round 5) |
| PSS after 10 flings (Compose: 90.0 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3; 81.2 MB in round 4; 79.5 MB in round 5 |
| APK (Compose: 1.74 MB) | 2.4 MiB in round 2; 3.24 MB (3.1 MiB) in round 3, unstripped; 1.99 MB stripped (D301, round 4), and in round 5. Not closed |

In round 3, D270 (the display list as a tree) was also measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` unchanged); scrolling, `frame` 0.273 → 0.237 ms. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs and the todo toggle from 263 to 194 µs. The round 4 and round 5 changes were measured the same way in [14](14-performance-opportunities.md).

## What made the difference

- **Main-thread frames**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270), and a group's record names its nested groups by key, so a nested group's change leaves its parent's recording alone (D323); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose; the replay calls no interpreted framework method (D321). Lazy lists compose and premeasure their next item when the main thread is idle (D156) and record its paragraphs then (D322), which takes composition, text layout and paragraph recording out of the scrolled frames.
- **Jank**: the idle-time budget is counted from the real vsync, so prefetching goes on through Choreographer's buffer stuffing recovery (D340), and an idle-time step starts only if its recent maximum fits with a margin, so it doesn't delay the next frame (D341).
- **RenderThread frames**: kept groups are drawn by reference; small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190); the spinner's round cap is drawn as a circle (D320); and without long frames Android no longer boosts the cores, which lengthens the scroll's frames but not its work.
- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; composition is linear in its reads (D310), and single lines are laid out with `BoringLayout` (D330). The big screen's first frame is almost half Compose's.
- **Memory**: the app's code is a 1.9 MB native library rather than Compose's dex and runtime (4.1 MB less code mapped, 2.3 MB less Java heap); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).

## Next steps

[14](14-performance-opportunities.md) measures what's left. For frame durations:

- **The scroll's slowest main-thread frames** still reveal an item: prefetching composes and measures it and records its paragraphs while idle, but the revealing frame records the item's group. Recording that ahead too would need Rust to draw unplaced items (M–L, about 0.15 ms per revealing frame).
- **Text layout while prefetching** is still about half of a prefetched item's work, after `BoringLayout` (D330), and most of the big screen's 35 ms of layout. Laying out on worker threads (#11) took the scroll's main p99 from 3.53 to 3.08 ms and the big screen's first-frame work down 16 % in a prototype that isn't production-ready (measured before D330); batching JNI calls alone is capped at about 2.6 ms of the big frame.
- **Late `doFrame`s**: 9 or 10 a scroll run start more than 2 ms after their vsync. None now waits for compose-rs's idle work (D341); they woke late, which is the scheduler's.
- **The scroll's RenderThread durations** are longer than Compose's for less work: they follow the cores the scheduler picks, not compose-rs's cost. Its p99 (5.68 against 5.17 ms in round 4) is to be watched in the next round.
- **The animation's p99** frames are the system's (vsync requests, the RenderThread's sync), which a `View` pays as Compose does; only drawing without HWUI (option D of [13](13-android-drawing-paths.md), our own Skia) would remove part of them.
- **Smaller items**: caching the display's refresh rate in `prefetch()` (1.9 ms per scroll run); rounded clips through a `Path` (about 15 % of the animation's replay); a release profile for the library (Rust's frame sections −4 to −8 %, main work −3 to −4 %, about the noise).

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
