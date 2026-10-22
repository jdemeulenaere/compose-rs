# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in startup, memory, jank and per-frame work.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring, Compose's best case.

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) raise the cores' frequencies after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians. "Round 2" is compose-rs at 097759b (`results/2026-10-22-pixel-8-pro-round-2.json`). "Round 1" is the version this document first reported (7550e31), measured again in the same session with the work metric (`results/2026-10-22-pixel-8-pro-round-2-baseline.json`); its numbers are quoted in parentheses where they show the change. Compose's numbers are from the round 2 run; they differ from its round 1 run by a few percent.

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 128 ms | **110 ms** (−14 %; round 1: 114 ms) |
| `feed`: the first frame's main thread (`traversal`) | 44 ms | **25 ms** (33 ms) |
| `big`: 200 posts at once | 226 ms | **178 ms** (−21 %; 186 ms) |
| `big`: the first frame's main thread (`traversal`) | 169 ms | **106 ms** (112 ms) |

In compose-rs's first `big` frame (106 ms): composing 200 posts takes 20 ms, laying them out 46 ms (42 ms of it laying out 1,200 paragraphs with `StaticLayout` through JNI), drawing and serializing the display list 1 ms, and replaying it on the `Canvas` 23 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 37 ms (`Record View#draw()`). `bindApplication` is shorter too (10 ms against 16 ms): there is no Compose runtime to initialize.

### Memory

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 51.4 MB | **32.7 MB** (−36 %; round 1: 42.5 MB) |
| of which code | 6.1 MB | **1.6 MB** |
| of which Java heap | 3.6 MB | **1.3 MB** |
| of which native heap | 8.1 MB | **7.9 MB** (8.5 MB) |
| of which graphics | 28.1 MB | **17.0 MB** (26.2 MB) |
| PSS after 10 flings | 85.6 MB | **80.9 MB** (90.1 MB) |
| of which Java heap | **3.9 MB** | 6.1 MB (7.1 MB) |
| of which native heap | 12.6 MB | **12.1 MB** (20.1 MB) |
| of which graphics | 56.8 MB | 56.3 MB |
| APK (release) | 1.7 MB (minified) | 2.4 MB (with the 3 MB `.so` uncompressed inside) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory fell by 9 MB since round 1, most likely because the GPU no longer keeps the path atlas and stencil buffer it drew the icons with (D190). After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (85 to 88 MB in this run, up to 112 MB in earlier ones: its Java heap holds 4 to 20 MB of garbage, as it allocates on every frame).

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots).

### Scrolling (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,761 | 1,747 |
| Janky frames (FrameTimeline), per run | 0, 3, 4 | **1, 0, 1** |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / **6** / **8** ms (round 1: 5 / 7 / 12 ms) |
| Main thread per frame p50 / p90 / p99 | 1.46 / 3.18 / 6.67 ms | **1.34** / **2.32** / **3.94** ms (1.12 / 3.11 / 8.57 ms) |
| RenderThread per frame p50 / p90 | **2.20** / **3.37** ms | 2.76 / 3.88 ms (1.82 / 3.06 ms) |
| Main thread CPU time | 4.17 s | **3.44 s** (2.86 s) |
| RenderThread CPU time | **4.33 s** | 5.08 s (3.64 s) |
| Main thread work | 803 ms | **475 ms** (−41 %; 667 ms) |
| RenderThread work | 851 ms | **774 ms** (−9 %; 970 ms) |

Both scroll at 120 Hz with a few janky frames in 1,700: Compose's were missed app deadlines (5) and display or SurfaceFlinger delays (2), compose-rs's one missed deadline and one prediction error. compose-rs's main thread does 41 % less work than Compose's, and its slowest frames are now well under Compose's (p99 3.9 ms against 6.7 ms; 8.6 ms in round 1): prefetching composes and lays out the next item between frames, so the frame that reveals it composes nothing and lays no text out. Its median frame and its CPU time look longer than in round 1, though it does 29 % less work: without long frames, the scheduler keeps the main thread and the RenderThread on the little cores.

The RenderThread shows the same effect from the other side. In round 1 its frames were shorter than Compose's (1.82 ms against 2.21 ms in that run) but it did more work (970 ms against 843 ms): its long frames brought the ADPF boost, and at a raised frequency the same frame takes more cycles (memory doesn't speed up with the core). Now its frames take longer than Compose's and it does 9 % less work: kept groups record nothing again, icons are drawn from masks (one GPU render pass a frame instead of two, no path stencil operations), and the boost no longer comes.

### Animation (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Janky frames (FrameTimeline), per run | 1, 0, 0 | **0, 0, 0** |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / 8 ms |
| Main thread per frame p50 / p90 / p99 | 1.47 / 2.12 / 3.15 ms | **1.38** / **2.03** / **3.04** ms (round 1: 1.66 / 2.38 / 3.34 ms) |
| RenderThread per frame p50 / p90 | 2.94 / 3.98 ms | 2.93 / **3.85** ms (2.88 / 3.99 ms) |
| Main thread CPU over 10 s | 1.95 s | **1.74 s** (2.09 s) |
| RenderThread CPU over 10 s | 3.72 s | **3.67 s** (3.68 s) |
| Main thread work | 249 ms | **217 ms** (−13 %; 251 ms) |
| RenderThread work | **533 ms** | 535 ms (505 ms) |

compose-rs's main thread is now ahead of Compose's (in round 1 it was 0.2 ms a frame behind). Each animation frame draws again only the repaint boundary whose content changed (D155), not the whole tree: the progress indicator draws a new arc each frame, so the header's group is drawn and recorded again (about 0.7 KB of display list), while the pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The RenderThread does as much work as Compose's, slightly more than in round 1.

### How the gaps closed

| Gap | Steps |
|---|---|
| Animation, main thread per frame (Compose: 1.47 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157) |
| Scrolling, main thread p99 (Compose: 6.67 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156) |
| Scrolling, RenderThread work (Compose: 851 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost |
| PSS after 10 flings (Compose: 85.6 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195) |

## What made the difference

- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; the big screen's first frame is about a third faster.
- **Memory**: the app's code is a 3 MB native library rather than Compose's dex and runtime (4.5 MB less code mapped, 2.3 MB less Java heap); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).
- **Per-frame work on the main thread**: each repaint boundary keeps its operations and a version, and draws again only when something it drew from changed (D155); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose. Lazy lists compose and premeasure their next item when the main thread is idle (D156), which takes composition and text layout out of the scrolled frames.
- **Per-frame work on the RenderThread**: kept groups are drawn by reference, small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190), and without long frames Android no longer boosts the cores.

## Next steps

- **The animation's RenderThread**: it is even with Compose's (535 ms against 533 ms of work) and did a little less in round 1 (505 ms); comparing the two rounds' `DrawFrame` slices would show what a frame draws more of now.
- **Text layout in batches**: laying out paragraphs in batches (one JNI crossing for many) would shorten the big screen's first frame (42 ms of text layout) and the prefetch of each item.
- **Recording prefetched items ahead**: prefetching composes and measures an item between frames, but the frame that reveals it still records its group and its paragraphs' render nodes; recording them while idle too would leave that frame only drawing render nodes.
- **Gradients and path clips on Android**: the serializer skips `ClipPath` and paints a gradient's average color (D175); the replayer needs a clip-path record and the display list's shaders.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
