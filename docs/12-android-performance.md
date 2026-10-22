# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in startup, memory, jank and per-frame work.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring; rounds 1 to 3 used `speed` for both apps. That isn't Compose's best case for startup: with a profile (`-m speed-profile`, as a Baseline or Cloud Profile gives, with an app image), Compose's feed starts in 113.5 ms against 129 ms with `speed`, so compose-rs (105.5 ms) leads by 8 ms rather than 23.5 ([14](14-performance-opportunities.md), "Compilation modes"; compose-rs's startup doesn't depend on the mode). `measure.py --compile speed-profile` measures that way.

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it. Since D302, it first checks that the installed apps' native libraries for the device's ABI are those built in the checkout (and stops if not), and records each APK's size and SHA-1, the compilation filter (`--compile`, `speed` by default) and each app's compilation state (`dumpsys package dexopt`) at the start and the end of the run. Its parsing is tested on captured output: `python3 android/bench/test_measure.py`.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) raise the cores' frequencies after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians. "Round 3" is compose-rs at f79a7ab (`results/2026-10-22-pixel-8-pro-round-3.json`), the current results. "Round 2" is compose-rs at 097759b (`results/2026-10-22-pixel-8-pro-round-2.json`); its numbers are quoted in parentheses where they show the change. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers are from the round 3 run; they differ from its round 2 run by a few percent.

Since round 2:

- **D251**: a paint that draws nothing (a transparent color, no shader) records nothing, and top app bars are repaint boundaries. Material 3's top app bar had made the animated frame about 10 % slower after round 2 (a transparent background and its clip drawn, sent and replayed every frame); this brought it back.
- **D270**: the display list is a tree: a frame holds each kept group's recording by reference instead of copying its operations into one list each frame. Rust's frame is about 14 % shorter on the device, about 30 % headless (below).
- **D240**: state holders (`LazyListState`, `ScrollState`, …) are `Copy` handles.
- **D188**: nested scrolling: scrollables dispatch each delta to the connections around them.
- Features with no performance intent: forms, platform APIs, logging (the APK grew from 2.4 to 3.1 MB).

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 129 ms | **107 ms** (−17 %; round 2: 110 ms) |
| `feed`: the first frame's main thread (`traversal`) | 44 ms | **24 ms** (25 ms) |
| `big`: 200 posts at once | 225 ms | **179 ms** (−20 %; 178 ms) |
| `big`: the first frame's main thread (`traversal`) | 164 ms | **110 ms** (106 ms) |

Compose's numbers are under `speed`. With a profile (`speed-profile`), its feed starts sooner: 113.5 ms against 129 in another session, where compose-rs's took 105.5 ms; `big` didn't change (229 against 227 ms) ([14](14-performance-opportunities.md)).

In compose-rs's first `big` frame (110 ms): composing 200 posts takes 22 ms, laying them out 46 ms (most of it laying out 1,200 paragraphs with `StaticLayout` through JNI), drawing and serializing the display list 1 ms, and replaying it on the `Canvas` 24 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 36 ms (`Record View#draw()`). `bindApplication` is shorter too (10 ms against 16 to 19 ms): there is no Compose runtime to initialize.

### Memory

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 51.3 MB | **33.2 MB** (−35 %; round 2: 32.7 MB) |
| of which code | 6.1 MB | **1.9 MB** (1.6 MB) |
| of which Java heap | 3.6 MB | **1.4 MB** |
| of which native heap | **8.0 MB** | 8.1 MB (7.9 MB) |
| of which graphics | 27.9 MB | **17.0 MB** |
| PSS after 10 flings | 85.8 MB | **80.8 MB** (80.9 MB) |
| of which Java heap | **4.6 MB** | 6.1 MB |
| of which native heap | 12.5 MB | 12.5 MB (12.1 MB) |
| of which graphics | 56.7 MB | **55.3 MB** |
| APK (release) | 1.7 MB (minified) | 3.1 MB (with the 3.0 MB `.so` uncompressed inside, unstripped; 2.4 MB in round 2). Since D301 the library is stripped: 1.9 MB, with a 1.8 MB `.so` |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). The code mapped grew by 0.3 MB with the library. After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (85 to 90 MB in this run, up to 112 MB in earlier ones: its Java heap holds 4 to 20 MB of garbage, as it allocates on every frame); compose-rs's stay within 80.6 to 81.7 MB.

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots).

### Scrolling (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,770 | 1,768 |
| Janky frames (FrameTimeline), per run | 0, 1, 3 | **0, 0, 0** (round 2: 1, 0, 1) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 10 ms | 5 / **6** / **8** ms |
| Main thread per frame p50 / p90 / p99 | 1.44 / 3.08 / 6.50 ms | **1.25** / **2.20** / **3.74** ms (1.34 / 2.32 / 3.94 ms) |
| RenderThread per frame p50 / p90 | **2.13** / **3.43** ms | 2.67 / 3.73 ms (2.76 / 3.88 ms) |
| Main thread CPU time | 4.06 s | **3.25 s** (3.44 s) |
| RenderThread CPU time | **4.26 s** | 4.89 s (5.08 s) |
| Main thread work | 827 ms | **477 ms** (−42 %; 475 ms) |
| RenderThread work | 862 ms | **779 ms** (−10 %; 774 ms) |

Both scroll at 120 Hz. compose-rs had no janky frame in the three runs; Compose had 4 (one missed app deadline, two prediction errors, one SurfaceFlinger scheduling delay). compose-rs's main thread does 42 % less work than Compose's, and its slowest frames are well under Compose's (p99 3.7 ms against 6.5 ms; 8.6 ms in round 1): prefetching composes and lays out the next item between frames (D156), so the frame that reveals it composes nothing and lays no text out. Since round 2 its frames are a little shorter and its CPU time lower, for the same work (477 against 475 ms; 458 to 501 ms across this round's runs): D270's saving, 0.036 ms of a frame's `compose-rs:` sections, is within the spread between runs.

The RenderThread's frames take longer than Compose's (p50 2.67 against 2.13 ms) though it does 10 % less work: without long frames, the scheduler keeps it on the little cores and ADPF doesn't raise their frequency (see "Method"). Round 1 showed the opposite: shorter frames than Compose's but more work (970 ms against 843 ms), its long frames bringing the boost. Kept groups record nothing, and icons are drawn from masks (one GPU render pass a frame instead of two, no path stencil operations).

### Animation (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / 8 ms |
| Main thread per frame p50 / p90 / p99 | 1.48 / 2.13 / **3.06** ms | **1.32** / **1.95** / 3.12 ms (round 2: 1.38 / 2.03 / 3.04 ms) |
| RenderThread per frame p50 / p90 | 2.97 / 3.99 ms | **2.92** / **3.86** ms (2.93 / 3.85 ms) |
| Main thread CPU over 10 s | 1.96 s | **1.66 s** (1.74 s) |
| RenderThread CPU over 10 s | 3.72 s | **3.64 s** (3.67 s) |
| Main thread work | 251 ms | **211 ms** (−16 %; 217 ms) |
| RenderThread work | 535 ms | 540 ms (535 ms) |

compose-rs's main thread does 16 % less work than Compose's. Each animation frame draws again only the repaint boundary whose content changed (D155), not the whole tree: the progress indicator draws a new arc each frame, so the header's group is drawn and recorded again (about 0.7 KB of display list), while the pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The RenderThread's work is even with Compose's: 540 against 535 ms, within the spread between runs (535 to 542 ms for compose-rs, 528 to 544 ms for Compose). It is the one measure of work where compose-rs isn't ahead.

D270's effect, measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` 0.198 → 0.200, unchanged: the bytes are the same); scrolling, `frame` 0.273 → 0.237 ms (`draw` 0.073 → 0.046, `serialize` 0.031 → 0.025). Skia draws a group's content where its boundary is, and the serializer copies a kept group's record without reading its operations. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs (4.9 to 2.0 µs four phones tall) and the todo toggle from 263 to 194 µs.

### How the gaps closed

| Gap | Steps |
|---|---|
| Animation, main thread per frame (Compose: 1.48 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3) |
| Scrolling, main thread p99 (Compose: 6.50 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3 |
| Scrolling, RenderThread work (Compose: 862 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3 |
| PSS after 10 flings (Compose: 85.8 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3 |

## What made the difference

- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; the big screen's first frame is about a third faster.
- **Memory**: the app's code is a 3 MB native library rather than Compose's dex and runtime (4.2 MB less code mapped, 2.2 MB less Java heap); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).
- **Per-frame work on the main thread**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose. Lazy lists compose and premeasure their next item when the main thread is idle (D156), which takes composition and text layout out of the scrolled frames.
- **Per-frame work on the RenderThread**: kept groups are drawn by reference, small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190), and without long frames Android no longer boosts the cores.

## Next steps

[14](14-performance-opportunities.md) measures these three, and what else is left (the animation's RenderThread gap was mostly the spinner's square cap).

- **The animation's RenderThread**: even with Compose's (540 ms against 535 ms of work), where round 1 did 505 ms; comparing the rounds' `DrawFrame` slices would show what a frame draws more of now.
- **Text layout in batches**: laying out paragraphs in batches (one JNI crossing for many) would shorten the big screen's first frame (46 ms of layout, most of it text) and the prefetch of each item.
- **Recording prefetched items ahead**: prefetching composes and measures an item between frames, but the frame that reveals it still records its group and its paragraphs' render nodes; recording them while idle too would leave that frame only drawing render nodes.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
