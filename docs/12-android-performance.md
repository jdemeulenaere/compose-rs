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
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs).
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame work comes from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler. Since time on a slow core isn't more work, "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. The scheduler moves light threads to the little cores, so a frame's duration says as much about where it ran as about what it did.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

The numbers below are the latest run, with repaint-boundary groups kept in render nodes (D154). The run before them (`results/…-before-groups.json`) is quoted where it differs.

## Results

Pixel 8 Pro, Android 16, October 2026 (`results/2026-10-22-pixel-8-pro.json`). Medians.

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 131 ms | **116 ms** (−11 %) |
| `big`: 200 posts at once | 227 ms | **187 ms** (−18 %) |
| `big`: the first frame's main thread (`Choreographer#doFrame`) | 161 ms | **112 ms** |

In compose-rs's first `big` frame (112 ms): composing 200 posts takes 19 ms, laying them out 48 ms (most of it laying out 1,200 paragraphs with `StaticLayout` through JNI), recording the display list 1 ms, and replaying it on the `Canvas` 22 ms (recording each paragraph into its `RenderNode`). `bindApplication` is shorter too (10 ms against 16 ms): there is no Compose runtime to initialize.

### Memory

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 50.0 MB | **42.8 MB** (−14 %) |
| of which code | 6.1 MB | **1.5 MB** |
| of which Java heap | 3.6 MB | **1.3 MB** |
| of which native heap | 7.9 MB | 8.6 MB |
| PSS after 10 flings | **85.4 MB** | 90.2 MB (87.1 MB before groups) |
| APK (release) | 1.7 MB (minified) | 2.4 MB (with the 3 MB `.so` uncompressed inside) |

After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). compose-rs keeps more in its native and Java heaps after scrolling (20 and 7 MB against 13 and 4 MB: render nodes, the bytes of kept groups and text layouts, held by Java objects until the garbage collector runs), but it doesn't grow: over 48 flings both apps stay between 85 and 111 MB, falling back as the collector runs.

### Scrolling (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,759 | 1,750 |
| Janky frames (FrameTimeline) | 0 | 1 (one run; others 0) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 10 ms | 5 / 7 / 13 ms |
| Main thread per frame p50 / p90 / p99 | 1.54 / 3.16 / **6.78** ms | **1.19** / **3.08** / 8.53 ms |
| RenderThread per frame p50 / p90 | 2.13 / 3.47 ms | **1.84** / **3.24** ms |
| Main thread CPU over the run | 4.37 s | **2.97 s** (−32 %) |

Both scroll without jank at 120 Hz. compose-rs's median frame is shorter than Compose's (a scrolled item is drawn from its kept render node: before groups, the median was 1.65 ms) and its main thread works a third less; its slowest frames (p99) are a little longer: the frames that compose new items lay their text out through JNI.

### Animation (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Janky frames | 0 | 0 |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / 8 ms |
| Main thread per frame p50 / p90 | **1.45** / **2.09** ms | 1.66 / 2.19 ms (1.99 / 2.83 before groups) |
| RenderThread per frame p50 | 2.99 ms | **2.87 ms** |
| Main thread CPU over 10 s | **1.94 s** | 2.11 s (2.50 s before groups) |

Here Compose is still a little ahead (0.2 ms a frame). Each animation frame, compose-rs runs its frame in Rust (0.44 ms: the executor 0.07, drawing the whole tree again 0.26, serializing 0.05) and the Kotlin replay 0.34 ms: the posts are drawn from their kept render nodes, and only the header's group, which changes, is recorded again. Compose's frame is cheaper because an animated `graphicsLayer` property is a `RenderNode` property: it changes without re-recording anything. The gap closed in steps: the frame went from 3.1 ms (the baseline run, `results/…-baseline.json`) to 2.0 ms by recording each paragraph once into a `RenderNode`, setting `Paint` properties only when they change, merging consecutive translations, and folding a layer's alpha into its one drawing (the pulsing dot), which removed two offscreen GPU passes a frame; then to 1.66 ms by keeping each repaint boundary's group (lazy items, graphics layers) in a render node while its operations stay the same (D154), which halved the replay.

## What made the difference

- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; the big screen's first frame is about a third faster.
- **Memory**: the app's code is a 3 MB native library rather than Compose's dex and runtime: 4.7 MB less code mapped, 2.3 MB less Java heap.
- **Per-frame work**: compose-rs draws its whole tree into a display list each frame something draws, and the Kotlin replayer records again only the groups whose operations changed; Compose records only what changed, below render nodes. On a frame that only animates a layer's alpha or rotation, Compose does almost nothing on the main thread; while scrolling, compose-rs does less.

## Next steps

- **Per-node draw invalidation**: compose-ui draws the whole tree each frame (0.26 ms of the animation frame); keeping a repaint boundary's operations while nothing it reads changed would skip drawing, serializing and comparing unchanged groups.
- **Layer properties on render nodes**: setting a graphics layer's alpha, rotation and scale as its render node's properties would make a layer-only animation frame skip recording the layer, as Compose's.
- **Text layout**: laying out paragraphs in batches (one JNI crossing for many) would shorten the frames that compose new items.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
