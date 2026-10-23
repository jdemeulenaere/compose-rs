# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in jank, per-frame durations and work, startup and memory.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX but `androidx.profileinstaller`, as the Kotlin app, since D360). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring; every round so far used `speed` for both apps (recorded since round 4). That isn't Compose's best case for startup: compiled from its shipped profiles (`-m speed-profile`, as a Play install compiles it, with an app image), Compose's feed starts in 120 ms against 132 ms with `speed` (in D362's session), so compose-rs (107 ms either way) leads by 13 ms rather than 25 (28 in round 6, under `speed`). `measure.py --compile speed-profile` measures that way, with each app's shipped Baseline Profile (D362): Compose's libraries' and the app's own, and the compose-rs library's (D360); see "Compiled from the shipped Baseline Profiles" below.

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it. Since D302, it first checks that the installed apps' native libraries for the device's ABI are those built in the checkout (and stops if not), and records each APK's size and SHA-1, the compilation filter (`--compile`, `speed` by default) and each app's compilation state (`dumpsys package dexopt`) at the start and the end of the run, and stops unless each app starts compiled as asked. With `--compile speed-profile`, each app is reset, given the Baseline Profile its APK ships through androidx.profileinstaller's receiver, and compiled from it (D362). Both apps run androidx.profileinstaller's startup initializer on every cold start, in every mode. A traced run whose trace holds no frame of the app's is run again (D362). Its parsing is tested on captured output: `python3 android/bench/test_measure.py`.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler. Since D343, a warm-up swipe comes first and is left out (the stats start at the first input after it; gfxinfo is reset after it): the display, idle since the app started, resyncs its vsync on the first input, which FrameTimeline counted as jank no app caused. Each run also reports its janky frames by side (the app's deadline, or the compositor's: SurfaceFlinger's deadlines, prediction errors, the display), the frames presented at another vsync than expected (`present_type`, buffer stuffing's late frames included), the `doFrame`s starting more than 2 ms after their vsync (about 1.1 ms normally), and of those the ones that work on the main thread delayed (a slice ending more than 1.2 ms after the vsync: idle-time work, input; the rest woke late), compose-rs's `prefetch` calls, and whether Choreographer entered buffer stuffing recovery (its async track, or a `buffer stuffed` slice): `summarize.py` prints a line per run.
- **The RenderThread's boost**: since round 6 (D370), each traced run also reports how much Android's performance hints boosted the RenderThread. That is the HWUI hint session's boost minimum: the `uclamp.min` that ADPF sets for HWUI's threads, out of 1024, which the power HAL traces as `adpf.<pid>-…-HWUI-…-min`. The report gives its time-weighted mean over the run's frames, the share of frames started under 100 (unboosted), and the `DrawFrame` percentiles in each boost bucket (under 100, 100–199, 200 and more). It also gives the share of the RenderThread's running time on the little cores (those of the lowest capacity, from the device's `cpu_capacity`). A trace without the counter reports the little cores' share only.
- **Isolation**: before each run, measure.py force-stops our other apps (`rs.compose.*` but the one measured): the other benchmark app, left cached by the run before, and the demo, whose animation ran a frame on every vsync in the background before D342.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) boost the threads, onto faster cores at higher frequencies, after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on; durations are what a user sees.

Measurements drift between sessions: Compose's own numbers move by up to 5 to 8 % from one session to the next (its scrolling main-thread work was 827 ms in round 3, 760 ms in round 4, 844 ms in round 5, 843 ms in round 6). So the two apps are compared within a session, and changes to compose-rs are measured on their own with interleaved A/B runs ([14](14-performance-opportunities.md)); a difference between rounds smaller than that is drift.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians of 3 runs (startup: of 15). The better side is in bold; ties aren't.

"Round 6" is compose-rs at e12d389 (`results/2026-10-23-pixel-8-pro-round-6.json`, `speed`), the current results. It is the first full run with D350 and D360–D362 together, and the first with the RenderThread's boost picture. measure.py's checks passed: the installed native libraries matched those built, and both apps were `speed` at the start and the end. "Round 5" is compose-rs at 6f31c6e (`results/2026-10-22-pixel-8-pro-round-5.json`); its numbers are quoted in parentheses where they show the change. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers are from the round 6 run.

Since round 5:

- **D350**: a prefetched item's groups are drawn and recorded ahead, in the idle time after prefetching and recording its paragraphs, so the frame that reveals it records nothing new. Measured on its own, interleaved with round 5's code (10 runs each): the revealing frames' `doFrame` 1.80 → 1.53 ms on average (replay 0.31 → 0.11 ms), the scroll's main p90 1.94 → 1.86 ms; p99 and work within the runs' spread ([14](14-performance-opportunities.md), "Prefetch"; a full run: `results/2026-10-23-pixel-8-pro-draw-ahead.json`).
- **D360**: the compose-rs library ships a Baseline Profile for its Kotlin glue, which every app's APK merges; bench-rs takes androidx.profileinstaller, as bench-compose does. It matters to installs that aren't fully compiled (below).
- **D361**: release builds of the library take a release profile (fat LTO, one codegen unit, `panic = "abort"`; debug builds abort too, without LTO). The library is 1.41 MB instead of 1.89, the APK 1.55 MB instead of 1.99. A panic now aborts where it happens, after the panic hook logs it.
- **D362**: `measure.py --compile speed-profile` compiles each app from the Baseline Profile its APK ships, as a Play install does, and a traced run whose trace holds no frame runs again.

### Scrolling: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,770 | 1,771 |
| Janky frames (FrameTimeline), per run | **0, 0, 0** | 2, 0, 0 (round 5: 0, 0, 0) |
| of which the app's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| of which the compositor's | **0, 0, 0** | 2, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | **0, 0, 0** | 7, 0, 0 (0, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 29 (9), 20 (4), 16 (5) | **16 (2), 5 (1), 20 (3)** (10 (0), 10 (0), 9 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / **6** / **8** ms (5 / 6 / 8 ms) |
| Main thread per frame p50 / p90 / p99 | 1.57 / 3.26 / 6.84 ms | **1.28** / **1.84** / **3.05** ms (1.30 / 1.92 / 3.04 ms) |
| RenderThread per frame p50 / p90 / p99 | **2.21** / **3.49** / **5.03** ms | 2.80 / 3.74 / 5.47 ms (2.71 / 3.81 / 5.68 ms) |

Both scroll at 120 Hz. No frame of either app missed the app's deadline. compose-rs's 2 janky frames, in one run, were the compositor's: a SurfaceFlinger CPU deadline missed and a SurfaceFlinger scheduling delay, with 7 frames presented at another vsync around them. No run of either app entered buffer stuffing recovery, and compose-rs prefetched 360 to 367 times a run.

The main thread's frames are shorter than Compose's at every percentile, the slowest by more than half (p99 3.05 against 6.84 ms; 8.57 ms in round 1). Prefetching composes and lays out the next item between frames (D156), records its paragraphs (D322) and, since D350, draws and records its groups. So the frame that reveals an item composes nothing, lays no text out and records nothing new. The p90 fell since round 5 (1.92 → 1.84 ms), as D350 measured on its own; the p99 moved within the runs' spread.

`doFrame`s start late more often than in round 5 (5 to 20 a run, a median of 16, against 9 or 10) and, by the median, less often than Compose's (16 against 20; 16 to 29 a run). The scheduler woke most of them late. 1 to 3 a run waited for main-thread work, against none in round 5 and 4 to 9 of Compose's. Which work it was hasn't been looked at: the `speed-profile` run before D350 had 0 to 2 a run too (`…-android-builds-speed-profile.json`), so D350's idle-time steps aren't clearly the cause.

The RenderThread's frames take longer than Compose's (p50 2.80 against 2.21 ms) though it does less work (below). That is the boost Android gives each app, not compose-rs's cost (next section).

### Scrolling: the RenderThread's boost

ADPF boosts the RenderThread after the frames HWUI reports to it run long against their target. Those frames include the main thread's. Medians of the 3 runs (each bucket's percentiles: of the runs with frames in it). The table describes rather than ranks, so nothing is in bold:

| Scroll | Kotlin Compose | compose-rs |
|---|---|---|
| HWUI boost minimum, time-weighted (out of 1024) | 130 (runs: 87 to 193) | 21 (18 to 26) |
| Frames unboosted (minimum under 100) | 46 % (27 to 64 %) | 96 % (92 to 97 %) |
| Frames at 100–199 / at 200 or more | 26 % / 28 % | 4 % / 0.4 % |
| RenderThread running time on the little cores | 25 % | 28 % |
| Unboosted frames: p50 / p90 / p99 | 2.86 / 3.96 / 5.34 ms | 2.82 / 3.79 / 5.48 ms |
| Frames at 100–199: p50 / p90 / p99 | 2.07 / 2.77 / 3.59 ms | 2.08 / 2.91 / 3.86 ms |
| Frames at 200 or more: p50 / p90 / p99 | 1.50 / 2.02 / 2.82 ms | 1.62 / 1.95 / 2.01 ms (23 frames in all) |

At the same boost, the two apps' RenderThread frames take the same time: the unboosted frames' percentiles are within 0.2 ms of each other, the 100–199 bucket's within 0.3 ms. The difference is how often each app is boosted. Compose's main thread has long frames (p99 6.84 ms) and its frames are boosted half the time; compose-rs's main thread has few, and 96 % of its RenderThread frames run unboosted. A boosted frame runs at higher frequencies and mostly off the little cores (in [14](14-performance-opportunities.md)'s sessions, 0 to 12 % of boosted frames' running time was on them). The two apps' share of running time on the little cores is about the same overall (25 against 28 %), because Compose's unboosted frames ran more on them (35 to 40 % against compose-rs's 28 to 33 % there); the boost shows in the frequencies and the bigger cores, which the report doesn't break down. Compose's own runs show it: its RenderThread's p50 was 2.46, 1.87 and 2.21 ms in runs whose frames were 64 %, 27 % and 46 % unboosted.

[14](14-performance-opportunities.md) ("The scroll's RenderThread") investigated this over five sessions' traces. compose-rs's unboosted p99 differs from Compose's by +0.08 ms on average. Weighted so that each boost bucket holds Compose's share of frames, compose-rs's round 5 frames have a p50 / p90 / p99 of 2.16 / 3.35 / 5.27 ms, against Compose's round 5 2.13 / 3.56 / 5.17. The scroll's RenderThread gap is where the work runs, not how much there is.

Requesting a boost ourselves (an `APerformanceHint` session of compose-rs's own, reporting longer frames) was considered and not done. It would spend power on RenderThread durations no user sees: frame times and jank are already better than Compose's without it.

In the animation, neither app's main thread has long frames: 95 % of both apps' RenderThread frames are unboosted. There, compose-rs's RenderThread is the faster one (below), partly by placement too: Compose's ran on the little cores 40 % of its running time, compose-rs's 24 %.

### Animation: frames (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,349 | 1,351 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 |
| Frames presented at another vsync than expected, per run | 0, 0, 0 | 0, 0, 0 |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 4 (0), 9 (1), 7 (0) | 6 (0), 14 (0), 3 (0) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / **7** ms |
| Main thread per frame p50 / p90 / p99 | 1.52 / 2.17 / 3.00 ms | **1.18** / **1.69** / 2.91 ms (round 5: 1.21 / 1.69 / 2.76 ms) |
| RenderThread per frame p50 / p90 / p99 | 3.00 / 4.01 / 5.51 ms | **2.67** / **3.44** / **5.11** ms (2.64 / 3.38 / 5.03 ms) |
| RenderThread frames unboosted | 95 % | 95 % |

Neither app had a janky frame, or a frame presented at another vsync than expected. (In round 5, two of Compose's runs had every frame presented a vsync late, on cadence; not this time.)

compose-rs's frames are shorter on both threads. The main thread's median is 22 % shorter than Compose's. Each animation frame draws again only the repaint boundary whose content changed (D155), the spinner's group alone (D323). The pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The slowest main-thread frames are the system's (vsync requests, waking and waiting for the RenderThread), the same in Compose, so p99 moves least. The RenderThread's frames are 11 % shorter at the median, 14 % at p90, as often unboosted as Compose's, for 4 % less work: the spinner's arc is one `CircleOp`, as in Compose (D320), and the rest of the difference is where it ran (less on the little cores, above). D350 doesn't touch the animation, and D361's release profile moved its Rust frame section by less than the runs' spread; its numbers moved within the drift between sessions.

### Work per frame

| Scrolling (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 843 ms | **439 ms** (−48 %; round 5: 452 ms) |
| RenderThread work | 866 ms | **794 ms** (−8 %; 813 ms) |
| Main thread CPU time | 4.54 s | **3.41 s** (3.45 s) |
| RenderThread CPU time | **4.51 s** | 5.23 s (5.05 s) |

| Animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 252 ms | **192 ms** (−24 %; round 5: 190 ms) |
| RenderThread work | 525 ms | **502 ms** (−4 %; 501 ms) |
| Main thread CPU time | 2.02 s | **1.54 s** (1.56 s) |
| RenderThread CPU time | 3.81 s | **3.37 s** (3.34 s) |

compose-rs's main thread does 48 % less work than Compose's when scrolling, 24 % less in the animation. The scroll's fell by 3 % since round 5 (452 → 439 ms), within the drift between sessions. D350 moves recording from frames into idle time rather than removing it.

The RenderThread does less work than Compose's in both scenarios. Scrolling, its CPU time is higher than Compose's for less work, because it is rarely boosted (above).

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 135 ms | **107 ms** (−21 %; round 5: 110 ms) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 44.4 ms | **24.5 ms** (25.4 ms) |
| `big`: 200 posts at once | 231 ms | **163 ms** (−29 %; 163 ms) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 164.4 ms | **91.0 ms** (92.1 ms) |

Compose's numbers are under `speed`. Compiled from its shipped profiles (`speed-profile`), its feed starts sooner: 120 against 132 ms in the session below, where compose-rs's took 107 ms either way; `big` didn't change (228 ms). Both apps are within `TotalTime`'s noise of round 5 (a standard deviation of 4 to 11 ms over the 15 starts).

In `big`'s first frame (91.0 ms), composing 200 posts takes 17.4 ms and laying them out 34.7 ms, of which 30.1 ms is text (1,200 paragraphs through JNI, 1,000 of them single lines laid out with `BoringLayout`, D330). Drawing and serializing the display list take 1 ms, and replaying it on the `Canvas` 21.0 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 35.1 ms (`Record View#draw()`). On the feed's traced start, `bindApplication` is shorter too (11.8 against 17.5 ms): there is no Compose runtime to initialize.

### Memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 52.1 MB | **33.6 MB** (−36 %; round 5: 33.7 MB) |
| of which code | 6.5 MB | **2.1 MB** (2.4 MB) |
| of which Java heap | 3.6 MB | **1.3 MB** (1.3 MB) |
| of which native heap | 8.1 MB | 8.1 MB (8.1 MB) |
| of which graphics | 28.1 MB | **17.0 MB** (17.0 MB) |
| PSS after 10 flings | 88.7 MB | **79.7 MB** (79.5 MB) |
| of which Java heap | 6.0 MB | **5.1 MB** (5.1 MB) |
| of which native heap | 13.1 MB | **11.9 MB** (11.8 MB) |
| of which graphics | 56.8 MB | 55.3 MB (55.1 MB) |
| APK (release; decimal MB) | 1.74 MB (minified) | **1.55 MB**, with the 1.41 MB `.so` uncompressed inside (1.99 MB) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). Its code fell since round 5 (2.4 → 2.1 MB) with the smaller library (D361). After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (88.4 to 90.0 MB in this run, its Java heap 5.9 to 7.3 MB, and up to 112 MB in earlier ones: it allocates on every frame); compose-rs's stay within 79.3 to 81.0 MB.

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots).

The APK is now smaller than Compose's. Since D361, release builds take a release profile for Android (fat LTO, one codegen unit, `panic = "abort"`): the library is 1.41 MB, stored uncompressed so it can be mapped, against Compose's minified dex ([14](14-performance-opportunities.md), #4b). bench-rs's APK holds androidx.profileinstaller too since D360 (about 38 KB).

### Compiled from the shipped Baseline Profiles (D360–D362)

A full run of each mode after D360 and D361, before D350 was merged: compose-rs at fcf048e, earlier on the day of round 6. The `speed` run is `results/2026-10-23-pixel-8-pro-android-builds.json`. The `speed-profile` run (`…-android-builds-speed-profile.json`, `--compile speed-profile`) compiled each app from the profile it ships. "Round 5" is compose-rs before both changes. Medians of 3 runs (startup: of 15). The table compares compilation modes rather than the two apps, so nothing is in bold:

| | compose-rs, round 5 | compose-rs, `speed` | compose-rs, `speed-profile` | Compose, `speed` | Compose, `speed-profile` |
|---|---|---|---|---|---|
| Scroll: janky frames, per run | 0, 0, 0 | 0, 0, 0 | 0, 0, 0 | 0, 0, 1 | 0, 0, 1 |
| Scroll: main per frame p50 / p90 / p99 | 1.30 / 1.92 / 3.04 ms | 1.28 / 1.98 / 3.17 ms | 1.29 / 1.87 / 2.90 ms | 1.49 / 3.09 / 6.80 ms | 1.57 / 3.25 / 6.87 ms |
| Scroll: RenderThread p50 / p90 | 2.71 / 3.81 ms | 2.75 / 3.81 ms | 2.69 / 3.79 ms | 2.04 / 3.31 ms | 2.04 / 3.36 ms |
| Scroll: main, RenderThread work | 452, 813 ms | 439, 783 ms | 443, 812 ms | 867, 894 ms | 981, 883 ms |
| Scroll: frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / 8 ms | 5 / 6 / 8 ms | 5 / 7 / 11 ms | 5 / 7 / 11 ms |
| Animation: janky frames, per run | 0, 0, 0 | 0, 0, 0 | 0, 2, 0 (prediction errors) | 0, 0, 0 | 0, 0, 0 |
| Animation: main per frame p50 / p90 / p99 | 1.21 / 1.69 / 2.76 ms | 1.18 / 1.71 / 2.90 ms | 1.19 / 1.72 / 2.92 ms | 1.55 / 2.16 / 3.07 ms | 1.74 / 2.43 / 3.36 ms |
| Animation: RenderThread p50 / p90 | 2.64 / 3.38 ms | 2.67 / 3.44 ms | 2.64 / 3.40 ms | 2.98 / 4.13 ms | 3.02 / 4.16 ms |
| Animation: main, RenderThread work | 190, 501 ms | 190, 493 ms | 194, 514 ms | 255, 531 ms | 298, 531 ms |
| Startup feed / big | 110 / 163 ms | 107 / 158 ms | 107 / 164 ms | 132 / 228 ms | 120 / 228 ms |
| PSS after start, of which code | 33.7, 2.4 MB | 33.6, 2.1 MB | 33.6, 2.0 MB | 52.1, 6.5 MB | 50.7, 5.0 MB |
| APK | 1.99 MB | 1.54 MB | | 1.74 MB | |

No frame of compose-rs's missed its deadline in either run; the two janky animation frames under `speed-profile` were the compositor's prediction errors. Under `speed`, compose-rs's numbers are within the runs' spread of round 5, with less code mapped (the smaller library) and an APK now smaller than Compose's. Compiled from the shipped profiles, compose-rs runs as under `speed`: its glue is all in its profile. Compose starts its feed sooner that way (120 against 132 ms; its app image), but its frames do more work, as what Compose's profiles leave out runs in the JIT: main work +13 % scrolling and +17 % animating, the animation's main-thread p50 1.74 ms against 1.55. compose-rs's lead is then 13 ms at the feed's start, 64 ms at the big screen's, and less than half Compose's main-thread work scrolling.

In both `speed-profile` runs made (the first with fcf048e's measure.py), Compose's first scroll trace held no frame of its process; measure.py now runs such a run again (it did once here) and summarize.py leaves out one left in an older report.

### How the gaps closed

Compose's numbers are round 6's.

| Gap | Steps |
|---|---|
| Scrolling, janky frames per run (Compose: 0, 0, 0) | 1, 2, 0 in round 1, all the app's; 1, 0, 1 in round 2; 0, 0, 0 in round 3; 1, 3, 1 in round 4, all the compositor's (a vsync resync, a display stall); 0, 0, 0 with the prefetch budget on the real vsync and the warm-up swipe left out (D340, D343, round 5); 2, 0, 0 in round 6, the compositor's |
| Scrolling, main thread p99 (Compose: 6.84 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3; 3.11 ms with prefetched paragraphs recorded while idle (D322, round 4; p90 2.20 → 1.90 ms); 3.04 ms in round 5; 3.05 ms with the items' groups recorded while idle (D350, round 6; p90 1.92 → 1.84 ms) |
| Animation, main thread per frame p50 (Compose: 1.52 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3); 1.20 ms with a replay without interpreted calls and records naming nested groups (D321, D323, round 4); 1.21 ms in round 5; 1.18 ms in round 6 |
| Animation, RenderThread work (Compose: 525 ms) | 505 ms in round 1; 535 ms in round 2 and 540 ms in round 3, even with Compose's (a square-capped spinner, a stroked path each frame); 517 ms with the round cap (D320, round 4); 501 ms in round 5; 502 ms in round 6 |
| Scrolling, RenderThread work (Compose: 866 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3; 786 ms in round 4; 813 ms in round 5; 794 ms in round 6 |
| Scrolling, RenderThread p50 (Compose: 2.21 ms) | 2.71 ms in round 5, 2.80 ms in round 6: not closed, and not compose-rs's work. At the same boost the durations match ("Scrolling: the RenderThread's boost") |
| Startup, `big` (Compose: 231 ms) | 186 ms in round 1; 178 ms in round 2; 179 ms in round 3; 174 ms with composition linear in its reads (D310, round 4); 163 ms with `BoringLayout` (D330, round 5), and in round 6 |
| PSS after 10 flings (Compose: 88.7 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3; 81.2 MB in round 4; 79.5 MB in round 5; 79.7 MB in round 6 |
| APK (Compose: 1.74 MB) | 2.4 MiB in round 2; 3.24 MB (3.1 MiB) in round 3, unstripped; 1.99 MB stripped (D301, round 4), and in round 5; 1.55 MB with the release profile (D361, round 6), profileinstaller included. Closed |

In round 3, D270 (the display list as a tree) was also measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` unchanged); scrolling, `frame` 0.273 → 0.237 ms. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs and the todo toggle from 263 to 194 µs. The changes of rounds 4 to 6 were measured the same way in [14](14-performance-opportunities.md).

## What made the difference

- **Main-thread frames**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270), and a group's record names its nested groups by key, so a nested group's change leaves its parent's recording alone (D323); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose; the replay calls no interpreted framework method (D321). Lazy lists compose and premeasure their next item when the main thread is idle (D156), record its paragraphs (D322) and draw and record its groups then (D350), which takes composition, text layout and recording out of the scrolled frames.
- **Jank**: the idle-time budget is counted from the real vsync, so prefetching goes on through Choreographer's buffer stuffing recovery (D340), and an idle-time step starts only if its recent maximum fits with a margin, so it doesn't delay the next frame (D341).
- **RenderThread frames**: kept groups are drawn by reference; small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190); the spinner's round cap is drawn as a circle (D320). Without long main-thread frames, Android rarely boosts the RenderThread, which lengthens the scroll's frames but not their work.
- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; composition is linear in its reads (D310), and single lines are laid out with `BoringLayout` (D330). The big screen's first frame takes 55 % of Compose's.
- **Memory and size**: the app's code is a 1.4 MB native library rather than Compose's dex and runtime (4.4 MB less code mapped, 2.3 MB less Java heap), built with LTO (D361); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).

## Next steps

[14](14-performance-opportunities.md) measures what's left. For frame durations:

- **The scroll's slowest main-thread frames** compose or lay out what prefetching didn't reach: a fling back reveals items prefetching ahead in the other direction doesn't cover (D350's full run had one janky frame of the app's that way). Since D350, the frames that reveal a prefetched item record nothing new; the p99 (3.05 ms) is set by the rest.
- **Text layout while prefetching** is still about half of a prefetched item's work, after `BoringLayout` (D330), and most of the big screen's 35 ms of layout. Laying out on worker threads (#11) took the scroll's main p99 from 3.53 to 3.08 ms and the big screen's first-frame work down 16 % in a prototype that isn't production-ready (measured before D330); batching JNI calls alone is capped at about 2.6 ms of the big frame.
- **Late `doFrame`s**: 5 to 20 a scroll run start more than 2 ms after their vsync. Most woke late: the `Choreographer#onVsync` message itself comes 2 to 4 ms after the vsync, which is the scheduler's doing. 1 to 3 a run waited for main-thread work, against none in round 5 (0 to 2 in the `speed-profile` run before D350): which work is to be traced.
- **The compositor's jank**: round 6's 2 janky frames were SurfaceFlinger's (a CPU deadline missed, a scheduling delay), in one run. Watched in the next rounds, as in round 4.
- **The scroll's RenderThread durations** follow ADPF's boost, which compose-rs's short main-thread frames rarely earn; at the same boost they match Compose's. Not pursued: an own `APerformanceHint` session would spend power for durations no user sees.
- **The animation's p99** frames are the system's (vsync requests, the RenderThread's sync), which a `View` pays as Compose does; only drawing without HWUI (option D of [13](13-android-drawing-paths.md), our own Skia) would remove part of them.
- **Smaller items**: caching the display's refresh rate in `prefetch()` (1.9 ms per scroll run); rounded clips through a `Path` (about 15 % of the animation's replay).

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
