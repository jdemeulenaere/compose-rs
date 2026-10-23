# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in jank, per-frame durations and work, startup and memory.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX but `androidx.profileinstaller`, as the Kotlin app, since D360). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring; every round so far used `speed` for both apps (recorded since round 4). That isn't Compose's best case for startup: compiled from its shipped profiles (`-m speed-profile`, as a Play install compiles it, with an app image), Compose's feed starts in 120 ms against 132 ms with `speed` (in D362's session), so compose-rs (107 ms either way) leads by 13 ms rather than 25 (25 in round 8 too, under `speed`). `measure.py --compile speed-profile` measures that way, with each app's shipped Baseline Profile (D362): Compose's libraries' and the app's own, and the compose-rs library's (D360); see "Compiled from the shipped Baseline Profiles" below.

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it. Since D302, it first checks that the installed apps' native libraries for the device's ABI are those built in the checkout (and stops if not), and records each APK's size and SHA-1, the compilation filter (`--compile`, `speed` by default) and each app's compilation state (`dumpsys package dexopt`) at the start and the end of the run, and stops unless each app starts compiled as asked. With `--compile speed-profile`, each app is reset, given the Baseline Profile its APK ships through androidx.profileinstaller's receiver, and compiled from it (D362). Both apps run androidx.profileinstaller's startup initializer on every cold start, in every mode. A traced run whose trace holds no frame of the app's is run again (D362). Its parsing is tested on captured output: `python3 android/bench/test_measure.py`.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler. Since D343, a warm-up swipe comes first and is left out (the stats start at the first input after it; gfxinfo is reset after it): the display, idle since the app started, resyncs its vsync on the first input, which FrameTimeline counted as jank no app caused. Each run also reports its janky frames by side (the app's deadline, or the compositor's: SurfaceFlinger's deadlines, prediction errors, the display), the frames presented at another vsync than expected (`present_type`, buffer stuffing's late frames included), the `doFrame`s starting more than 2 ms after their vsync (about 1.1 ms normally), and of those the ones that work on the main thread delayed (a slice ending more than 1.2 ms after the vsync: idle-time work, input; the rest woke late), compose-rs's `prefetch` calls, and whether Choreographer entered buffer stuffing recovery (its async track, or a `buffer stuffed` slice): `summarize.py` prints a line per run.
- **The RenderThread's boost**: since round 6 (D370), each traced run also reports how much Android's performance hints boosted the RenderThread. That is the HWUI hint session's boost minimum: the `uclamp.min` that ADPF sets for HWUI's threads, out of 1024, which the power HAL traces as `adpf.<pid>-…-HWUI-…-min`. The report gives its time-weighted mean over the run's frames, the share of frames started under 100 (unboosted), and the `DrawFrame` percentiles in each boost bucket (under 100, 100–199, 200 and more). It also gives the share of the RenderThread's running time on the little cores (those of the lowest capacity, from the device's `cpu_capacity`). A trace without the counter reports the little cores' share only. Since D390, compose-rs's own session boosts the RenderThread too, and the power HAL applies the higher of the two sessions' votes: `render_boost` still reads HWUI's counter alone, so round 8's picture below comes from the same traces with both sessions' votes (the round's JSON, `d390_boost`).
- **Isolation**: before each run, measure.py force-stops our other apps (`rs.compose.*` but the one measured): the other benchmark app, left cached by the run before, and the demo, whose animation ran a frame on every vsync in the background before D342.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.
- **Stress** (D400, D401): a much heavier list (`stress_startup`, `stress_scroll`) and a screen of hundreds of concurrent animations (`stress_animation`), measured as above; [15](15-android-stress-benchmark.md) describes them and their results. `--scenarios` runs some scenarios only (all by default), and every traced run lists its slowest frames with their longest slices (`slowest_frames`).

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) boost the threads, onto faster cores at higher frequencies, after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on; durations are what a user sees.

Measurements drift between sessions: Compose's own numbers move by up to 5 to 9 % from one session to the next (its scrolling main-thread work was 827 ms in round 3, 760 ms in round 4, 844 ms in round 5, 843 ms in round 6, 839 ms in round 7, 917 ms in round 8). So the two apps are compared within a session, and changes to compose-rs are measured on their own with interleaved A/B runs ([14](14-performance-opportunities.md)); a difference between rounds smaller than that is drift.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians of 3 runs (startup: of 15). The better side is in bold; ties aren't.

"Round 8" is compose-rs with D390, on main 0e0c86c (`results/2026-10-23-pixel-8-pro-round-8.json`, `speed`; `--scenarios startup,scroll,animation`: the stress scenarios, [15](15-android-stress-benchmark.md), weren't run again), the current results. measure.py's checks passed: the installed native libraries matched those built, and both apps were `speed` at the start and the end. "Round 7" is compose-rs at 2dd698c (`results/2026-10-23-pixel-8-pro-round-7.json`); its numbers are quoted in parentheses where they show the change. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers are from the round 8 run. Both apps hold the stress screens since 0e0c86c (D400), which their feed screens don't use: they grow the APKs only.

Since round 7:

- **D390**: while a `ComposeRsView` shows (API 33+), the RenderThread is in a performance hint session of compose-rs's own, which reports the RenderThread's CPU time per frame against 6.5 ms, so Android boosts it about as often as HWUI's session boosts Compose's (below, "Scrolling: the RenderThread's boost"). Measured on its own, interleaved with main and Compose (6 runs each): the scroll's RenderThread p50 / p99 2.70 → 2.50 / 5.36 → 4.92 ms, for 2.2 % more energy on the CPU rails (within the runs' spread) ([14](14-performance-opportunities.md), "The scroll's RenderThread"). `ComposeRsView.isPerformanceHintEnabled = false` turns it off.

### Scrolling: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,793 | 1,793 |
| Janky frames (FrameTimeline), per run | 0, 1, 0 | **0, 0, 0** (round 7: 0, 0, 0) |
| of which the app's | 0, 1, 0 | **0, 0, 0** (0, 0, 0) |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 0, 47, 0 | 0, 14, 0 (0, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 34 (7), 21 (4), 20 (5) | **8 (0), 10 (0), 32 (0)** (14 (0), 12 (0), 11 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 10 ms | 5 / **6** / **8** ms (5 / 6 / 8 ms) |
| Main thread per frame p50 / p90 / p99 | 1.45 / 3.03 / 6.88 ms | **1.34** / **1.85** / **2.86** ms (1.29 / 1.94 / 2.86 ms) |
| RenderThread per frame p50 / p90 / p99 | **1.98** / **3.21** / **4.85** ms | 2.31 / 3.37 / 4.94 ms (2.70 / 3.76 / 5.44 ms) |

Both scroll at 120 Hz. No frame of compose-rs's was janky in any run. Compose missed the app's deadline once, in its second run (a 15.4 ms `doFrame`, mostly its recomposer's animation step), with 47 frames presented at another vsync around it; compose-rs's second run had 14 such frames, none janky. No run of either app entered buffer stuffing recovery, and compose-rs prefetched 363 to 387 times a run.

The main thread's frames are shorter than Compose's at every percentile, the slowest by more than half (p99 2.86 against 6.88 ms; 8.57 ms in round 1). Prefetching composes and lays out the next item between frames (D156), records its paragraphs (D322) and draws and records its groups (D350). So the frame that reveals an item composes nothing, lays no text out and records nothing new. A fling back finds the items it wants composed (D375) and recorded (D380), so its first frames do no more than the others. The p99 is round 7's; the p50 (1.29 → 1.34 ms) and p90 (1.94 → 1.85 ms) moved within the runs' spread (in D390's A/B, main's p50 / p90 / p99 were 1.28 / 1.85 / 2.85 ms and D390's 1.31 / 1.85 / 2.79).

No `doFrame` waited for compose-rs's main-thread work in any run, as in round 7, against 4 to 7 a run of Compose's. compose-rs's `doFrame`s started late less often than Compose's in two runs of three (8 and 10 against 20 to 34; 32 in the third); the scheduler woke all of them late.

The RenderThread's frames are 0.39 ms shorter at the median than in round 7 (2.70 → 2.31 ms) and 0.50 ms at p99 (5.44 → 4.94), with D390's boost; they still take longer than Compose's (p50 1.98 ms), which its HWUI session boosted more in this run than in round 7 (next section).

### Scrolling: the RenderThread's boost

ADPF boosts a thread after the frames a hint session holding it reports run long against their target. HWUI's session holds the main thread and the RenderThread and reports each frame from the main thread's start to the RenderThread's end against 11.62 ms, so the main thread's long frames earn the boost; compose-rs's own session (D390) holds the RenderThread alone and reports its CPU time per frame against 6.5 ms. The power HAL gives a thread the highest active vote of its sessions. measure.py's `render_boost` reads HWUI's counter alone; the rows below take each frame's boost as the higher of both sessions' votes when it started, from the same traces (the round's JSON, `d390_boost`). Medians of the 3 runs (each bucket's percentiles: of the runs with frames in it). The table describes rather than ranks, so nothing is in bold:

| Scroll | Kotlin Compose | compose-rs (round 7: HWUI's session only) |
|---|---|---|
| Boost minimum, time-weighted (out of 1024) | 169 (runs: 103 to 186) | 112 (107 to 119) (25) |
| of which HWUI's session's alone | 169 | 10 |
| Frames unboosted (minimum under 100) | 34 % (28 to 62 %) | 53 % (44 to 57 %) (96 %) |
| Frames at 100–199 / at 200 or more | 18 % / 42 % | 28 % / 16 % (4 % / 0 %) |
| RenderThread running time on the little cores | 18 % | 19 % (29 %) |
| Unboosted frames: p50 / p90 / p99 | 2.85 / 3.92 / 5.69 ms | 2.74 / 3.81 / 5.31 ms (2.73 / 3.83 / 5.46 ms) |
| Frames at 100–199: p50 / p90 / p99 | 2.08 / 2.76 / 3.66 ms | 2.12 / 2.87 / 3.97 ms (2.12 / 2.83 / 3.59 ms) |
| Frames at 200 or more: p50 / p90 / p99 | 1.62 / 2.07 / 2.82 ms | 1.81 / 2.22 / 2.72 ms (1.79 / 2.40 / 4.42 ms) |

At the same boost, the two apps' RenderThread frames take the same time, as before: each bucket's p50 within 0.2 ms. What changed is how often compose-rs's is boosted: 53 % of its frames run unboosted, against 96 % in round 7. HWUI's own session boosts it even less than before (a time-weighted 10 against 25): its frames, shorter, cross HWUI's target less often. Compose was boosted more in this run (34 % unboosted) than in round 7 (43 %) or D390's A/B (49 %): its share moves from run to run with its long main-thread frames (28 to 62 % here), so its RenderThread p50 does too (1.85 to 2.33 ms here).

The target is a choice: [14](14-performance-opportunities.md) swept 3.5 to 7.5 ms. At 6.5 ms, compose-rs's frames were unboosted as often as Compose's in the sweep's session (51 % against 49–51 %) and somewhat more often in the A/B's (62 % against 49 %) and this run's (53 % against 34 %). Lower targets boost every frame (no frame unboosted at 3.5 or 4.5 ms) for 10 to 17 % more CPU energy.

**The cost.** In D390's interleaved A/B, the CPU rails took 2.2 % more energy scrolling and 2.0 % more animating, within the runs' spread, and all the power rails (display included) 0.5 % and 0.2 % more; Compose's CPU rails took 8 % more than compose-rs's without the session. The boosted RenderThread runs at higher frequencies: less CPU time (−9 %) for about the same work (+1.6 %). The power HAL spends 34 ms more CPU per scroll run on the session's reports (+5 %).

In the animation, neither app's main thread has long frames, and compose-rs's RenderThread is rarely over the target: Compose's frames were 96 % unboosted, compose-rs's 49 % to 90 % (median 63 %), boosted most in the two runs presented a vsync late throughout (next section). In D390's A/B, 74 % (53 to 92 %).

### Animation: frames (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,332 | 1,341 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 (round 7: 0, 0, 0) |
| Frames presented at another vsync than expected, per run | **0, 0, 0** | 0, 1,204, 1,205 (0, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 9 (1), 2 (0), 6 (1) | 5 (0), 12 (0), 17 (0) (7 (0), 7 (0), 4 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / 8 ms (5 / 6 / 7 ms) |
| Main thread per frame p50 / p90 / p99 | 1.55 / 2.22 / 3.06 ms | **1.33** / **1.82** / **2.95** ms (1.21 / 1.67 / 2.88 ms) |
| RenderThread per frame p50 / p90 / p99 | 2.99 / 4.18 / 5.43 ms | **2.47** / **3.37** / **5.29** ms (2.66 / 3.47 / 5.17 ms) |
| RenderThread frames unboosted (the higher of both sessions' votes) | 96 % | 63 % (99 %) |

No frame of either app was janky. In compose-rs's second and third runs, all frames but one were presented a vsync late, on cadence: the second run entered buffer stuffing recovery, the third didn't (its frames were late from the trace's start). Compose's first run did the same in round 7, and none of D390's 6 animation runs of compose-rs (nor main's) did, so it isn't attributed to D390; those two runs' main-thread frames were longer (p90 2.14 and 1.82 against 1.70 ms in the first run).

compose-rs's frames are shorter on both threads. The main thread's median is 14 % shorter than Compose's (21 % in round 7, whose 1.21 ms D390's A/B reproduced: 1.20 ms on main, 1.22 ms with D390). Each animation frame draws again only the repaint boundary whose content changed (D155), the spinner's group alone (D323). The pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The slowest main-thread frames are the system's (vsync requests, waking and waiting for the RenderThread), the same in Compose, so p99 moves least. The RenderThread's frames are 17 % shorter at the median and 19 % at p90, more often boosted than Compose's; the spinner's arc is one `CircleOp`, as in Compose (D320).

### Work per frame

| Scrolling (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 917 ms | **454 ms** (−50 %; round 7: 442 ms) |
| RenderThread work | 911 ms | **821 ms** (−10 %; 784 ms) |
| Main thread CPU time | 4.26 s | **3.57 s** (3.38 s) |
| RenderThread CPU time | 4.10 s | 4.52 s (5.06 s) |

| Animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 250 ms | **198 ms** (−21 %; round 7: 192 ms) |
| RenderThread work | **523 ms** | 556 ms (+6 %; 505 ms) |
| Main thread CPU time | 2.05 s | **1.80 s** (1.58 s) |
| RenderThread CPU time | 3.81 s | **3.07 s** (3.37 s) |

compose-rs's main thread does half Compose's work when scrolling, 21 % less in the animation. Compose's scroll work (917 ms) is above its usual range (839 ms in round 7): drift between sessions, which moves both apps.

The RenderThread does 10 % less work than Compose's scrolling, for 10 % more CPU time, now that it is boosted more often (round 7: 5.06 s). In the animation its work rose to 556 ms (505 in round 7), above Compose's: the two runs presented late were boosted most (their RenderThread work 566 and 556 ms, against 486 in the first). In D390's A/B, the animation's RenderThread work didn't change (450 → 444 ms on its 9 s window), nor did the scroll's (676 → 687 ms on 13 s).

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 133 ms | **108 ms** (−19 %; round 7: 105 ms) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 46.1 ms | **25.6 ms** (24.9 ms) |
| `big`: 200 posts at once | 231 ms | **166 ms** (−28 %; 160 ms) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 167.0 ms | **91.7 ms** (90.6 ms) |

Compose's numbers are under `speed`. Compiled from its shipped profiles (`speed-profile`), its feed starts sooner: 120 against 132 ms in D362's session, where compose-rs's took 107 ms either way; `big` didn't change (228 ms). Both apps are within `TotalTime`'s noise of round 7 (a standard deviation of 4 to 11 ms over the 15 starts). D390 opens its session at the first frame drawn while shown: the RenderThread found among the process's threads, and a session created through the system (not traced apart; startup is within its noise).

In `big`'s first frame (91.7 ms), composing 200 posts takes 17.1 ms and laying them out 34.5 ms. Drawing and serializing the display list take 0.9 ms, and replaying it on the `Canvas` 20.4 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 35.1 ms (`Record View#draw()`). On the feed's traced start, `bindApplication` is shorter too (11.8 against 18.8 ms): there is no Compose runtime to initialize.

### Memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 52.8 MB | **34.2 MB** (−35 %; round 7: 33.6 MB) |
| of which code | 7.3 MB | **2.3 MB** (2.1 MB) |
| of which Java heap | 3.6 MB | **1.3 MB** (1.3 MB) |
| of which native heap | 8.1 MB | 8.5 MB (8.1 MB) |
| of which graphics | 27.9 MB | **17.0 MB** (17.0 MB) |
| PSS after 10 flings | 89.2 MB | **80.2 MB** (80.0 MB) |
| of which Java heap | 5.8 MB | **5.1 MB** (5.1 MB) |
| of which native heap | 13.2 MB | **12.1 MB** (12.1 MB) |
| of which graphics | 56.6 MB | 55.5 MB (55.3 MB) |
| APK (release; decimal MB) | 1.89 MB (minified) | **1.76 MB**, with the 1.60 MB `.so` uncompressed inside (1.57 MB) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). Both apps' code grew with the stress screens (D400). After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (up to 112 MB in earlier rounds: it allocates on every frame); compose-rs's stay within about a megabyte.

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots).

The APK is smaller than Compose's. Since D361, release builds take a release profile for Android (fat LTO, one codegen unit, `panic = "abort"`), stored uncompressed so the library can be mapped, against Compose's minified dex ([14](14-performance-opportunities.md), #4b). bench-rs's APK holds androidx.profileinstaller too since D360 (about 38 KB). Both grew since round 7 with the stress screens (D400: the library by 181 KB, Compose's APK by 147 KB); D390 adds 12 KB to the library.

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

Compose's numbers are round 8's.

| Gap | Steps |
|---|---|
| Scrolling, janky frames per run (Compose: 0, 1, 0) | 1, 2, 0 in round 1, all the app's; 1, 0, 1 in round 2; 0, 0, 0 in round 3; 1, 3, 1 in round 4, all the compositor's (a vsync resync, a display stall); 0, 0, 0 with the prefetch budget on the real vsync and the warm-up swipe left out (D340, D343, round 5); 2, 0, 0 in round 6, the compositor's; 0, 0, 0 in rounds 7 and 8 |
| Scrolling, main thread p99 (Compose: 6.88 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3; 3.11 ms with prefetched paragraphs recorded while idle (D322, round 4; p90 2.20 → 1.90 ms); 3.04 ms in round 5; 3.05 ms with the items' groups recorded while idle (D350, round 6; p90 1.92 → 1.84 ms); 2.86 ms in round 7 (within round 6's runs' spread), and in round 8 |
| Animation, main thread per frame p50 (Compose: 1.55 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3); 1.20 ms with a replay without interpreted calls and records naming nested groups (D321, D323, round 4); 1.21 ms in round 5; 1.18 ms in round 6; 1.21 ms in round 7; 1.33 ms in round 8, whose two runs presented a vsync late were slower (D390's A/B: 1.20 → 1.22 ms) |
| Animation, RenderThread work (Compose: 523 ms) | 505 ms in round 1; 535 ms in round 2 and 540 ms in round 3, even with Compose's (a square-capped spinner, a stroked path each frame); 517 ms with the round cap (D320, round 4); 501 ms in round 5; 502 ms in round 6; 505 ms in round 7; 556 ms in round 8, boosted (D390's A/B: unchanged, 450 → 444 ms over 9 s) |
| Scrolling, RenderThread work (Compose: 911 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3; 786 ms in round 4; 813 ms in round 5; 794 ms in round 6; 784 ms in round 7; 821 ms in round 8 |
| Scrolling, RenderThread p50 (Compose: 1.98 ms) | 2.71 ms in round 5, 2.80 ms in round 6, 2.70 ms in round 7: not compose-rs's work, but how rarely Android boosted it (at the same boost the durations match); 2.31 ms in round 8 with a hint session of its own (D390; Compose's p50 moves with how often its session boosts it: 1.85 to 2.33 ms in round 8's runs) |
| Startup, `big` (Compose: 231 ms) | 186 ms in round 1; 178 ms in round 2; 179 ms in round 3; 174 ms with composition linear in its reads (D310, round 4); 163 ms with `BoringLayout` (D330, round 5), and in round 6; 160 ms in round 7; 166 ms in round 8 |
| PSS after 10 flings (Compose: 89.2 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3; 81.2 MB in round 4; 79.5 MB in round 5; 79.7 MB in round 6; 80.0 MB in round 7; 80.2 MB in round 8 |
| APK (Compose: 1.89 MB) | 2.4 MiB in round 2; 3.24 MB (3.1 MiB) in round 3, unstripped; 1.99 MB stripped (D301, round 4), and in round 5; 1.55 MB with the release profile (D361, round 6), profileinstaller included; 1.57 MB in round 7; 1.76 MB in round 8, with the stress screens (Compose's grew by 0.15 MB). Closed |

In round 3, D270 (the display list as a tree) was also measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` unchanged); scrolling, `frame` 0.273 → 0.237 ms. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs and the todo toggle from 263 to 194 µs. The changes of rounds 4 to 8 were measured the same way in [14](14-performance-opportunities.md).

## What made the difference

- **Main-thread frames**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270), and a group's record names its nested groups by key, so a nested group's change leaves its parent's recording alone (D323); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose; the replay calls no interpreted framework method (D321). Lazy lists compose and premeasure their next item when the main thread is idle (D156), record its paragraphs (D322) and draw and record its groups then (D350), which takes composition, text layout and recording out of the scrolled frames; they keep the two items behind the scroll composed (D375) and recorded (D380), so a fling back's first frames do no more.
- **Jank**: the idle-time budget is counted from the real vsync, so prefetching goes on through Choreographer's buffer stuffing recovery (D340), and an idle-time step starts only if its recent maximum fits with a margin, so it doesn't delay the next frame (D341), scaled by how much slower than expected the idle time's steps so far ran (D381).
- **RenderThread frames**: kept groups are drawn by reference; small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190); the spinner's round cap is drawn as a circle (D320). Without long main-thread frames, HWUI's hint session rarely gets the RenderThread boosted, which lengthened the scroll's frames but not their work; a session of compose-rs's own, reporting the RenderThread's CPU time against 6.5 ms, now gets it boosted about as often as Compose's (D390), for about 2 % more CPU energy.
- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; composition is linear in its reads (D310), and single lines are laid out with `BoringLayout` (D330). The big screen's first frame takes 53 % of Compose's.
- **Memory and size**: the app's code is a 1.4 MB native library rather than Compose's dex and runtime (4.4 MB less code mapped, 2.3 MB less Java heap), built with LTO (D361); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).

## Next steps

[14](14-performance-opportunities.md) measures what's left. For frame durations:

- **The scroll's slowest main-thread frames**: since D375 and D380, a fling back's first frames compose, lay out and record nothing either, and no frame of compose-rs's was janky in rounds 7 and 8. What sets the p99 now (2.86 ms) hasn't been broken down.
- **Text layout while prefetching** is still about half of a prefetched item's work, after `BoringLayout` (D330), and most of the big screen's 35 ms of layout. Laying out on worker threads (#11) took the scroll's main p99 from 3.53 to 3.08 ms and the big screen's first-frame work down 16 % in a prototype that isn't production-ready (measured before D330); batching JNI calls alone is capped at about 2.6 ms of the big frame.
- **Late `doFrame`s**: 11 to 14 a scroll run start more than 2 ms after their vsync, all woken late (the `Choreographer#onVsync` message itself comes 2 to 4 ms after the vsync: the scheduler's doing). The ones that waited for compose-rs's idle-time work were a prefetch measuring an item on the little cores; D381 halved the idle-time slices running past the vsync, and those left overrun within a step whose start gave no sign. Splitting an item's measurement at its paragraphs (so a step can stop at the deadline) would be the next step, at the cost of an interruptible measure.
- **The compositor's jank**: none in rounds 7 and 8 (round 6: 2, SurfaceFlinger's, in one run). Watched in the next rounds.
- **The scroll's RenderThread durations** follow ADPF's boost. Since D390, compose-rs's own session boosts the RenderThread; its 6.5 ms target is a trade of power for durations, swept in [14](14-performance-opportunities.md). How often each app ends up boosted varies from session to session (compose-rs's frames unboosted 51 to 62 %, Compose's 34 to 51 %, in the three sessions measured).
- **The animation's p99** frames are the system's (vsync requests, the RenderThread's sync), which a `View` pays as Compose does; only drawing without HWUI (option D of [13](13-android-drawing-paths.md), our own Skia) would remove part of them.
- **Smaller items**: caching the display's refresh rate in `prefetch()` (1.9 ms per scroll run); rounded clips through a `Path` (about 15 % of the animation's replay); the kept groups' bookkeeping (D380), a few microseconds in every scrolled frame (the replay's median 77 → 82 µs in its A/B), which listing only changes would cut.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
