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

Measurements drift between sessions: Compose's own numbers move by up to 5 to 8 % from one session to the next (its scrolling main-thread work was 827 ms in round 3, 760 ms in round 4). So the two apps are compared within a session, and changes to compose-rs are measured on their own with interleaved A/B runs ([14](14-performance-opportunities.md)); a difference between rounds smaller than that is drift.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians of 3 runs (startup: of 15). The better side is in bold; ties aren't.

"Round 4" is compose-rs at 84a79be (`results/2026-10-22-pixel-8-pro-round-4.json`), the current results. It is the first run with measure.py's checks (D300–D302): the installed native libraries matched those built, both apps were `speed` at the start and the end, and the report records the APKs' sizes. "Round 3" is compose-rs at f79a7ab (`results/2026-10-22-pixel-8-pro-round-3.json`); its numbers are quoted in parentheses where they show the change. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers are from the round 4 run.

Since round 3:

- **D301**: the native library is stripped (AGP skipped it silently without a pinned NDK): the APK goes from 3.24 to 1.99 MB.
- **D310**: the runtime's state readers and composition locals: a read searches a cell's latest readers only, and a local walks provider groups only, so composing a big screen is linear in its reads.
- **D320**: progress indicators have round caps by default, as Material 3 1.4's. The benchmark now draws the same spinner as its Compose twin: compose-rs's square-capped arc was a stroked path each frame where Compose drew a circle, which is why round 3 found the animation's RenderThread even with Compose's.
- **D321**: the replay calls no framework method the boot image leaves to the interpreter (floats read as their int bits, one reused buffer, sizes tracked in Kotlin), and frame awaiters don't wake the host.
- **D322**: the paragraphs a prefetch laid out are recorded into their render nodes in the same idle time, so the frame that reveals an item draws them instead of recording them: the scroll's p90 and p99.
- **D323**: a group's record names its nested groups by key, so the spinner's frame records the spinner's group only, not the header card around it.

### Scrolling: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,763 | 1,758 |
| Janky frames (FrameTimeline), per run | **2, 0, 0** | 1, 3, 1 (round 3: 0, 0, 0) |
| of which the app missed its deadline | 2, 0, 0 | **0, 0, 0** |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 10 ms | 5 / **6** / **8** ms |
| Main thread per frame p50 / p90 / p99 | 1.47 / 3.11 / 6.21 ms | **1.23** / **1.90** / **3.11** ms (1.25 / 2.20 / 3.74 ms) |
| RenderThread per frame p50 / p90 / p99 | **2.24** / **3.52** / 5.19 ms | 2.74 / 3.70 / 5.17 ms (2.67 / 3.73 / 5.35 ms) |

Both scroll at 120 Hz. Compose's 2 janky frames were missed app deadlines. None of compose-rs's 5 was: 3 were prediction errors and 2 SurfaceFlinger CPU deadlines missed, on the compositor's side; in round 3 it was Compose that had those (two prediction errors and a SurfaceFlinger scheduling delay) and compose-rs none. Three of the five came in one run, which differed from the other two throughout (main thread p99 7.35 ms, gfxinfo p99 12 ms, RenderThread p50 2.14 ms: its threads ran on faster cores); the medians set it aside. In that run, after a display stall, Choreographer's buffer stuffing recovery shifted every frame time back an interval, which left prefetching no idle time for 10 s; D340 fixed that, and a run of the full benchmark since (with D330 and D340–D343) had no janky frame of compose-rs's ([14](14-performance-opportunities.md), "Jank").

The main thread's frames are shorter than Compose's at every percentile, the slowest by half (p99 3.11 against 6.21 ms; 8.57 ms in round 1). Prefetching composes and lays out the next item between frames (D156), and since D322 records its paragraphs too, so the frame that reveals it composes nothing, lays no text out and records no paragraph: its p90 and p99 went from 2.20 and 3.74 ms to 1.90 and 3.11 ms.

The RenderThread's frames take longer than Compose's at the median (2.74 against 2.24 ms) though it does less work (below): without long frames, the scheduler keeps it on the little cores and ADPF doesn't raise their frequency (see "Method"). Its slowest frames are even with Compose's (p99 5.17 against 5.19 ms).

### Animation: frames (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,341 | 1,350 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / **7** ms |
| Main thread per frame p50 / p90 / p99 | 1.47 / 2.20 / 3.08 ms | **1.20** / **1.79** / **2.98** ms (round 3: 1.32 / 1.95 / 3.12 ms) |
| RenderThread per frame p50 / p90 / p99 | 2.98 / 4.00 / 5.32 ms | **2.63** / **3.56** / **5.16** ms (2.92 / 3.86 / 5.54 ms) |

Neither app had a janky frame. compose-rs's frames are shorter on both threads now. The main thread's median is 18 % shorter than Compose's: each animation frame draws again only the repaint boundary whose content changed (D155), and since D323 that is the spinner's group alone, not the header card around it; the pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The slowest main-thread frames are the system's (vsync requests, waking and waiting for the RenderThread), the same in Compose, so p99 moves least. The RenderThread's p50 and p90 went from even to 12 and 11 % shorter than Compose's with the round cap (D320): the spinner's arc is one `CircleOp`, as in Compose, instead of a stroked path and a second GPU render pass each frame.

### Work per frame

| Scrolling (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 760 ms | **477 ms** (−37 %; round 3: 477 ms) |
| RenderThread work | 834 ms | **786 ms** (−6 %; 779 ms) |
| Main thread CPU time | 4.16 s | **3.18 s** (3.25 s) |
| RenderThread CPU time | **4.48 s** | 4.96 s (4.89 s) |

| Animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 247 ms | **190 ms** (−23 %; round 3: 211 ms) |
| RenderThread work | 536 ms | **517 ms** (−3 %; 540 ms) |
| Main thread CPU time | 1.96 s | **1.53 s** (1.66 s) |
| RenderThread CPU time | 3.76 s | **3.32 s** (3.64 s) |

compose-rs's main thread does 37 % less work than Compose's when scrolling, 23 % less in the animation. The animation's main-thread work fell by 10 % since round 3: the replay without interpreted calls (D321, −5 % measured on its own) and records that name nested groups (D323, −3 %). The scroll's is unchanged: D322 moves the paragraphs' recording from the revealing frames to idle time without adding work (473.1 → 473.9 ms measured on its own); Compose's own scroll work was lower this session (757 to 832 ms across its runs, 827 ms in round 3), hence −37 % against round 3's −42 %.

The RenderThread does less work than Compose's in both scenarios. In the animation it was the one measure where compose-rs wasn't ahead (540 against 535 ms in round 3), because of the spinner's square cap; with the round cap it does 517 ms against 536 (503 to 520 ms across its runs, 528 to 552 ms for Compose's). Scrolling, its CPU time is higher than Compose's for less work, the little cores again.

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 128 ms | **107 ms** (−16 %; round 3: 107 ms) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 43.9 ms | **29.1 ms** (24.4 ms) |
| `big`: 200 posts at once | 222 ms | **174 ms** (−22 %; 179 ms) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 162.1 ms | **104.6 ms** (110.1 ms) |

Compose's numbers are under `speed`. With a profile (`speed-profile`), its feed starts sooner: 113.5 ms against 129 in another session, where compose-rs's took 105.5 ms; `big` didn't change (229 against 227 ms) ([14](14-performance-opportunities.md)). The feed's traced first frame is longer than round 3's, but its `compose-rs:` sections took the same time (7.2 ms against 7.3): the difference is in the view system around them, in one trace.

In compose-rs's first `big` frame (104.6 ms): composing 200 posts takes 18.1 ms (22.2 ms in round 3: D310 made it linear in its reads), laying them out 46 ms (most of it laying out 1,200 paragraphs with `StaticLayout` through JNI), drawing and serializing the display list 1 ms, and replaying it on the `Canvas` 22 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 36 ms (`Record View#draw()`). `bindApplication` is shorter too (9 to 10 ms against 15 ms): there is no Compose runtime to initialize.

### Memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 51.5 MB | **33.2 MB** (−36 %; round 3: 33.2 MB) |
| of which code | 6.1 MB | **1.8 MB** (1.9 MB) |
| of which Java heap | 3.6 MB | **1.4 MB** |
| of which native heap | 8.0 MB | 8.0 MB (8.1 MB) |
| of which graphics | 28.1 MB | **17.0 MB** |
| PSS after 10 flings | 85.7 MB | **81.2 MB** (80.8 MB) |
| of which Java heap | **4.5 MB** | 6.3 MB (6.1 MB) |
| of which native heap | **12.4 MB** | 12.7 MB (12.5 MB) |
| of which graphics | 56.8 MB | **55.3 MB** |
| APK (release; decimal MB) | **1.74 MB** (minified) | 1.99 MB, with the 1.89 MB `.so` uncompressed inside (round 3: 3.24 MB, unstripped) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (84.9 to 87.3 MB in this run, up to 112 MB in earlier ones: its Java heap holds 4 to 20 MB of garbage, as it allocates on every frame); compose-rs's stay within 80.9 to 82.8 MB.

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots).

The APK is still larger than Compose's: the library is 1.89 MB stored uncompressed (so it can be mapped), against Compose's minified dex. A release profile for Android (fat LTO, one codegen unit, `panic = "abort"`) would make it 1.50 MB ([14](14-performance-opportunities.md), #4b), with a behaviour change and longer builds.

### How the gaps closed

Compose's numbers are round 4's.

| Gap | Steps |
|---|---|
| Animation, main thread per frame p50 (Compose: 1.47 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3); 1.20 ms with a replay without interpreted calls and records naming nested groups (D321, D323, round 4) |
| Animation, RenderThread work (Compose: 536 ms) | 505 ms in round 1; 535 ms in round 2 and 540 ms in round 3, even with Compose's (a square-capped spinner, a stroked path each frame); 517 ms with the round cap (D320, round 4) |
| Scrolling, main thread p99 (Compose: 6.21 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3; 3.11 ms with prefetched paragraphs recorded while idle (D322, round 4; p90 2.20 → 1.90 ms) |
| Scrolling, RenderThread work (Compose: 834 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3; 786 ms in round 4 |
| PSS after 10 flings (Compose: 85.7 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3; 81.2 MB in round 4 |
| APK (Compose: 1.74 MB) | 2.4 MiB in round 2; 3.24 MB (3.1 MiB) in round 3, unstripped; 1.99 MB stripped (D301, round 4). Not closed |

In round 3, D270 (the display list as a tree) was also measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` unchanged); scrolling, `frame` 0.273 → 0.237 ms. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs and the todo toggle from 263 to 194 µs. The round 4 changes were measured the same way in [14](14-performance-opportunities.md).

## What made the difference

- **Main-thread frames**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270), and a group's record names its nested groups by key, so a nested group's change leaves its parent's recording alone (D323); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose; the replay calls no interpreted framework method (D321). Lazy lists compose and premeasure their next item when the main thread is idle (D156) and record its paragraphs then (D322), which takes composition, text layout and paragraph recording out of the scrolled frames.
- **RenderThread frames**: kept groups are drawn by reference; small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190); the spinner's round cap is drawn as a circle (D320); and without long frames Android no longer boosts the cores, which lengthens the scroll's frames but not its work.
- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize, and composition is linear in its reads (D310); the big screen's first frame is more than a third shorter.
- **Memory**: the app's code is a 1.9 MB native library rather than Compose's dex and runtime (4.3 MB less code mapped, 2.2 MB less Java heap); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).

## Next steps

[14](14-performance-opportunities.md) measures what's left. For frame durations:

- **The scroll's slowest main-thread frames** still reveal an item: prefetching composes and measures it and records its paragraphs while idle, but the revealing frame records the item's group. Recording that ahead too would need Rust to draw unplaced items (M–L, about 0.15 ms per revealing frame).
- **Text layout while prefetching**: text through JNI is about half of a prefetched item's work, and most of the big screen's 46 ms of layout. Laying out a single left-to-right line that fits as a `BoringLayout`, as Compose's `TextLayout` does, instead of measuring its width and building a `StaticLayout`, is in progress (D330). Laying out on worker threads (#11) took the scroll's main p99 from 3.53 to 3.08 ms and the big screen's first-frame work down 16 % in a prototype that isn't production-ready; batching JNI calls alone is capped at about 2.6 ms of the big frame.
- **The scroll's RenderThread durations** are longer than Compose's for less work: they follow the cores the scheduler picks, not compose-rs's cost.
- **The animation's p99** frames are the system's (vsync requests, the RenderThread's sync), which a `View` pays as Compose does; only drawing without HWUI (option D of [13](13-android-drawing-paths.md), our own Skia) would remove part of them.
- **Smaller items**: caching the display's refresh rate in `prefetch()` (1.9 ms per scroll run); rounded clips through a `Path` (about 15 % of the animation's replay); a release profile for the library (Rust's frame sections −4 to −8 %, main work −3 to −4 %, about the noise).

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
