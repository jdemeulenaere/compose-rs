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
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame work comes from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026 (`results/2026-10-22-pixel-8-pro.json`). Medians.

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 131 ms | **118 ms** (−10 %) |
| `big`: 200 posts at once | 225 ms | **186 ms** (−17 %) |
| `big`: the first frame's main thread (`Choreographer#doFrame`) | 163 ms | **111 ms** |

In compose-rs's first `big` frame (111 ms): composing 200 posts takes 19 ms, laying them out 47 ms (42 ms of it laying out 1,200 paragraphs with `StaticLayout` through JNI), recording the display list 1 ms, and replaying it on the `Canvas` 22 ms (recording each paragraph into its `RenderNode`). `bindApplication` is shorter too (10 ms against 15 ms): there is no Compose runtime to initialize.

### Memory

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 51.6 MB | **42.7 MB** (−17 %) |
| of which code | 6.1 MB | **1.4 MB** |
| of which Java heap | 3.6 MB | **1.3 MB** |
| of which native heap | 8.0 MB | 8.5 MB |
| PSS after 10 flings | 87.5 MB | 87.1 MB |
| APK (release) | 1.7 MB (minified) | 2.4 MB (with the 3 MB `.so` uncompressed inside) |

After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). compose-rs keeps more in its native heap after scrolling (18 MB against 13 MB: render nodes and text layouts held by Java objects until the garbage collector runs), but it doesn't grow: over 48 flings both apps stay between 85 and 111 MB, falling back as the collector runs.

### Scrolling (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,758 | 1,754 |
| Janky frames (FrameTimeline) | 0 | 1 (one run; others 0) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / 7 / 13 ms |
| Main thread per frame p50 / p90 / p99 | 1.50 / 3.09 / 6.62 ms | 1.65 / 3.44 / 8.78 ms |
| RenderThread per frame p50 / p90 | 2.06 / 3.40 ms | 1.94 / 3.38 ms |
| Main thread CPU over the run | 4.27 s | **3.57 s** (−16 %) |

Both scroll without jank at 120 Hz. compose-rs's median frame is close to Compose's and its total main-thread CPU lower; its slowest frames (p99) are a little longer: the frames that compose new items lay their text out through JNI.

### Animation (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Janky frames | 0 | 0 |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 6 / 7 / 9 ms |
| Main thread per frame p50 / p90 | **1.48** / 2.22 ms | 1.99 / 2.83 ms |
| RenderThread per frame p50 | 2.98 ms | 3.11 ms |
| Main thread CPU over 10 s | **1.99 s** | 2.50 s |

Here Compose is ahead. Each animation frame, compose-rs runs its frame in Rust (0.4 ms: the executor, drawing the tree again, serializing) and the Kotlin replay records the display list on the view's canvas again (0.7 ms, about 100 operations). Compose's frame is cheaper because an animated `graphicsLayer` property is a `RenderNode` property: it changes without re-recording anything, and the spinner's re-recording is cheap. A first pass already narrowed the gap: the frame went from 3.1 ms (the baseline run, `results/…-baseline.json`) to 2.0 ms by recording each paragraph once into a `RenderNode`, setting `Paint` properties only when they change, merging consecutive translations, and folding a layer's alpha into its one drawing (the pulsing dot), which removed two offscreen GPU passes a frame.

## What made the difference

- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; the big screen's first frame is about a third faster.
- **Memory**: the app's code is a 3 MB native library rather than Compose's dex and runtime: 4.7 MB less code mapped, 2.3 MB less Java heap.
- **Per-frame work**: compose-rs records the whole display list each frame something draws, and replays it through JNI and the Kotlin replayer; Compose records only what changed, below render nodes. On a frame that only animates a layer's alpha or rotation, Compose does almost nothing on the main thread.

## Next steps

- **Render nodes for graphics layers**: replaying a graphics layer into a `RenderNode` of its own and setting its alpha, rotation and scale as node properties would make a layer-only animation frame skip the replay, as Compose's; with per-node draw invalidation in compose-ui, unchanged subtrees wouldn't be recorded again either.
- **Text layout**: laying out paragraphs in batches (one JNI crossing for many) would shorten the frames that compose new items.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
