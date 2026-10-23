# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in jank, per-frame durations and work, startup and memory.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX but `androidx.profileinstaller`, as the Kotlin app, since D360). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring; every round so far used `speed` for both apps (recorded since round 4). That isn't Compose's best case for startup: compiled from its shipped profiles (`-m speed-profile`, as a Play install compiles it, with an app image), Compose's feed starts in 120 ms against 132 ms with `speed` (in D362's session), so compose-rs (107 ms either way) leads by 13 ms rather than 25 (28 in round 9, under `speed`). `measure.py --compile speed-profile` measures that way, with each app's shipped Baseline Profile (D362): Compose's libraries' and the app's own, and the compose-rs library's (D360); see "Compiled from the shipped Baseline Profiles" below.

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it. Since D302, it first checks that the installed apps' native libraries for the device's ABI are those built in the checkout (and stops if not), and records each APK's size and SHA-1, the compilation filter (`--compile`, `speed` by default) and each app's compilation state (`dumpsys package dexopt`) at the start and the end of the run, and stops unless each app starts compiled as asked. With `--compile speed-profile`, each app is reset, given the Baseline Profile its APK ships through androidx.profileinstaller's receiver, and compiled from it (D362). Both apps run androidx.profileinstaller's startup initializer on every cold start, in every mode. A traced run whose trace holds no frame of the app's is run again (D362). Its parsing is tested on captured output: `python3 android/bench/test_measure.py`.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler. Since D343, a warm-up swipe comes first and is left out (the stats start at the first input after it; gfxinfo is reset after it): the display, idle since the app started, resyncs its vsync on the first input, which FrameTimeline counted as jank no app caused. Each run also reports its janky frames by side (the app's deadline, or the compositor's: SurfaceFlinger's deadlines, prediction errors, the display), the frames presented at another vsync than expected (`present_type`, buffer stuffing's late frames included), the `doFrame`s starting more than 2 ms after their vsync (about 1.1 ms normally), and of those the ones that work on the main thread delayed (a slice ending more than 1.2 ms after the vsync: idle-time work, input; the rest woke late), compose-rs's `prefetch` calls, and whether Choreographer entered buffer stuffing recovery (its async track, or a `buffer stuffed` slice): `summarize.py` prints a line per run.
- **The RenderThread's boost**: since round 6 (D370), each traced run also reports how much Android's performance hints boosted the RenderThread. That is the HWUI hint session's boost minimum: the `uclamp.min` that ADPF sets for HWUI's threads, out of 1024, which the power HAL traces as `adpf.<pid>-…-HWUI-…-min`. The report gives its time-weighted mean over the run's frames, the share of frames started under 100 (unboosted), and the `DrawFrame` percentiles in each boost bucket (under 100, 100–199, 200 and more). It also gives the share of the RenderThread's running time on the little cores (those of the lowest capacity, from the device's `cpu_capacity`). A trace without the counter reports the little cores' share only. Since D390, compose-rs's own session boosts the RenderThread too, and the power HAL applies the higher of the two sessions' votes: `render_boost` still reads HWUI's counter alone, so the picture below (rounds 8 and 9) comes from the same traces with both sessions' votes (the round's JSON, `d390_boost`).
- **Isolation**: before each run, measure.py force-stops our other apps (`rs.compose.*` but the one measured): the other benchmark app, left cached by the run before, and the demo, whose animation ran a frame on every vsync in the background before D342.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.
- **Stress** (D400, D401): a much heavier list (`stress_startup`, `stress_scroll`) and a screen of hundreds of concurrent animations (`stress_animation`), measured as above; [15](15-android-stress-benchmark.md) describes them and their results. `--scenarios` runs some scenarios only (all by default), and every traced run lists its slowest frames with their longest slices (`slowest_frames`).

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) boost the threads, onto faster cores at higher frequencies, after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on; durations are what a user sees.

Measurements drift between sessions: Compose's own numbers move by up to 5 to 9 % from one session to the next (its scrolling main-thread work was 827 ms in round 3, 760 ms in round 4, 844 ms in round 5, 843 ms in round 6, 839 ms in round 7, 917 ms in round 8, 842 ms in round 9). So the two apps are compared within a session, and changes to compose-rs are measured on their own with interleaved A/B runs ([14](14-performance-opportunities.md)); a difference between rounds smaller than that is drift.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians of 3 runs (startup: of 15). The better side is in bold; ties aren't.

"Round 9" is compose-rs at c57a8e1 (`results/2026-10-23-pixel-8-pro-round-9.json`, `speed`, every scenario: the stress ones' results are in [15](15-android-stress-benchmark.md)), the current results. measure.py's checks passed: the installed native libraries matched those built, and both apps were `speed` at the start and the end. "Round 8" is compose-rs with D390 as first built, on 0e0c86c (`results/2026-10-23-pixel-8-pro-round-8.json`); its numbers are quoted in parentheses where they show the change. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers are from the round 9 run.

Since round 8:

- **D390's refinements after review**: the session opens at the second compose-rs frame after a view shows, and a gap of more than 25 ms starts a new count. In an interleaved A/B they boosted slightly less than round 8's build, within the runs' spread ([14](14-performance-opportunities.md), "The scroll's RenderThread").
- **D410–D412**: the Android replayer rewinds a dropped path's `android.graphics.Path` and rebuilds it for the next definition, rather than leaving it to the garbage collector, and makes no mask for a path that is new every frame. The feed draws no such paths; the stress animation's 50 waves are ([15](15-android-stress-benchmark.md), "With paths reused").
- **D420–D424**: an infinite transition runs one task that sets all its values each frame, as in Compose, rather than a task per value, and resuming the tasks waiting for a frame costs a poll each, without waking the host. The feed's animated header has three transitions; the stress animation has 147 ([15](15-android-stress-benchmark.md), "Infinite transitions, one task each").

None of them was expected to change the feed's numbers, and none did beyond the runs' spread.

### Scrolling: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,801 | 1,792 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 (round 8: 0, 0, 0) |
| of which the app's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 0, 0, 0 | 0, 0, 0 (0, 14, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 10 (0), 20 (3), 10 (3) | 16 (3), 12 (0), 21 (1) (8 (0), 10 (0), 32 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 10 ms | 5 / **6** / **7** ms (5 / 6 / 8 ms) |
| Main thread per frame p50 / p90 / p99 | 1.52 / 3.24 / 6.55 ms | **1.36** / **1.90** / **2.76** ms (1.34 / 1.85 / 2.86 ms) |
| RenderThread per frame p50 / p90 / p99 | **2.12** / 3.41 / 5.16 ms | 2.31 / 3.33 / 5.13 ms (2.31 / 3.37 / 4.94 ms) |

Both scroll at 120 Hz. Neither app had a janky frame in any run, and no frame of either was presented at another vsync than expected. compose-rs's third run entered buffer stuffing recovery (1,767 recovery slices, no frame stuffed) without a frame presented late. compose-rs prefetched 368 to 378 times a run.

The main thread's frames are shorter than Compose's at every percentile, the slowest by more than half (p99 2.76 against 6.55 ms; 8.57 ms in round 1). Prefetching composes and lays out the next item between frames (D156), records its paragraphs (D322) and draws and records its groups (D350). So the frame that reveals an item composes nothing, lays no text out and records nothing new. A fling back finds the items it wants composed (D375) and recorded (D380), so its first frames do no more than the others. The three percentiles moved within the runs' spread from round 8 (p99 per run: 2.76, 2.76 and 2.94 ms).

`doFrame`s starting late: compose-rs's 12 to 21 a run, Compose's 10 to 20. 4 of compose-rs's, in two runs, waited for its own main-thread work (round 8: none, nor in round 7); 6 of Compose's did. D381's A/B counted 3 such frames in 12 runs, so the count moves from session to session; it is watched ("Next steps").

The RenderThread's frames take as long as in round 8 at the median (2.31 ms), and 0.19 ms longer than Compose's (2.12 ms). At p90 and p99 the two apps match (3.33 against 3.41 ms, 5.13 against 5.16 ms). Compose was boosted less often than in round 8 (next section), so its p50 rose from 1.98 to 2.12 ms.

### Scrolling: the RenderThread's boost

ADPF boosts a thread after the frames a hint session holding it reports run long against their target. HWUI's session holds the main thread and the RenderThread and reports each frame from the main thread's start to the RenderThread's end against 11.62 ms, so the main thread's long frames earn the boost; compose-rs's own session (D390) held the RenderThread alone in rounds 8 and 9, and holds the main thread too since D440; it reports the RenderThread's CPU time per frame against 6.5 ms. The power HAL gives a thread the highest active vote of its sessions. measure.py's `render_boost` reads HWUI's counter alone; the rows below take each frame's boost as the higher of both sessions' votes when it started, from the same traces (the round's JSON, `d390_boost`). Medians of the 3 runs (each bucket's percentiles: of the runs with frames in it). The table describes rather than ranks, so nothing is in bold:

| Scroll | Kotlin Compose | compose-rs (round 8) |
|---|---|---|
| Boost minimum, time-weighted (out of 1024) | 128 (runs: 120 to 146) | 93 (70 to 111) (112) |
| of which HWUI's session's alone | 128 | 24 (10) |
| Frames unboosted (minimum under 100) | 47 % (42 to 50 %) | 56 % (47 to 70 %) (53 %) |
| Frames at 100–199 / at 200 or more | 26 % / 26 % | 38 % / 6 % (28 % / 16 %) |
| RenderThread running time on the little cores | 21 % | 20 % (19 %) |
| Unboosted frames: p50 / p90 / p99 | 2.80 / 4.02 / 5.74 ms | 2.72 / 3.75 / 5.49 ms (2.74 / 3.81 / 5.31 ms) |
| Frames at 100–199: p50 / p90 / p99 | 2.01 / 2.71 / 3.76 ms | 2.10 / 2.74 / 4.04 ms (2.12 / 2.87 / 3.97 ms) |
| Frames at 200 or more: p50 / p90 / p99 | 1.60 / 2.05 / 2.59 ms | 1.70 / 2.37 / 2.82 ms (1.81 / 2.22 / 2.72 ms) |

At the same boost, the two apps' RenderThread frames take the same time, as before: each bucket's p50 within 0.1 ms. compose-rs's frames were unboosted about as often as in round 8 (56 % against 53 %), but fewer of its boosted ones reached 200 (6 % against 16 %), as with the refined session in its A/B. Compose's were boosted less often than in round 8 (47 % unboosted, against 34 %), so the two apps' shares are closer than in round 8, and compose-rs's p50 is still the longer, by the share of its frames at 200 or more.

The target is a choice: [14](14-performance-opportunities.md) swept 3.5 to 7.5 ms. At 6.5 ms, compose-rs's frames were unboosted as often as Compose's in the sweep's sessions (51 % against 51 %), and more often in the A/B's (62 % against 49 %), round 8's (53 % against 34 %) and round 9's (56 % against 47 %): far more boosted than before D390, less than Compose in three sessions of four. Lower targets boost every frame (no frame unboosted at 3.5 or 4.5 ms) for 10 to 17 % more CPU power.

**The cost.** In D390's interleaved A/B, the CPU rails drew 2.1 % more power scrolling and 2.3 % more animating, and all the power rails (display included) 0.4 % and 0.5 % more; 6 runs don't resolve it (95 % intervals of about −3 to +7 % and −4 to +9 % for the CPU rails). Compose's CPU rails drew 9 % more than compose-rs's without the session, scrolling. The boosted RenderThread runs at higher frequencies: less CPU time (−9 %) for about the same work (+1.6 %). The power HAL spends 34 ms more CPU per scroll run on the session's reports (+5 %), and the main thread's work rose 2.6 % scrolling and 3.5 % animating (about 6 µs a frame: the clock read and the report call).

**The vote's level, and the main thread (D440, after round 9).** Under the target, the power HAL's controller is a random walk: about 23 off the vote every 83 ms batch, whatever the vote, and 7.7 added per 100 µs by which a batch's last report exceeds the one before. Its level is set by how much the frames vary from one to the next, not by how far they are under the target: about 100 on the feed, about 320 on the stress list, and the same mechanism holds Compose's HWUI session at about 130 and 290 in round 9 ([14](14-performance-opportunities.md), "The main thread in the session"). On the stress list, with the RenderThread boosted, HWUI's session stopped boosting compose-rs's main thread, which then ran 79 % of its time on the little cores. Since D440 our session holds the main thread too. In an interleaved A/B (12 runs each), the feed's scroll main thread p50 / p90 / p99 went 1.32 / 1.86 / 2.88 → 1.24 / 1.75 / 2.70 ms, the stress list's 1.54 / 2.24 / 3.53 → 1.07 / 1.71 / 2.97 ms. The RenderThread and the votes didn't change, and the CPU rails' power moved by +1.0 % (± 1.8) on the feed and −0.7 % (± 2.2) on the list, within the runs' spread. The next round will measure it beside Compose.

In the animation, neither app's main thread has long frames, and HWUI's session rarely boosts either app: Compose's frames were 92 % unboosted. compose-rs's own session boosts some of them, though its RenderThread was already the faster: 70 % to 83 % unboosted (median 80 %; round 8: 63 %, boosted most in its two runs presented a vsync late throughout); in D390's A/B, 74 % (53 to 92 %), for p50 2.63 → 2.54 ms.

### Animation: frames (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,351 | 1,328 |
| Janky frames (FrameTimeline), per run | **0, 0, 0** | 0, 0, 3 (round 8: 0, 0, 0) |
| of which the app's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| of which the compositor's | **0, 0, 0** | 0, 0, 3 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 0, 1,204, 0 | 1,199, 0, 3 (0, 1,204, 1,205) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 6 (0), 10 (2), 5 (1) | 3 (0), 5 (0), 11 (0) (5 (0), 12 (0), 17 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / **7** ms (5 / 6 / 8 ms) |
| Main thread per frame p50 / p90 / p99 | 1.52 / 2.10 / 3.08 ms | **1.26** / **1.80** / **2.83** ms (1.33 / 1.82 / 2.95 ms) |
| RenderThread per frame p50 / p90 / p99 | 2.97 / 4.03 / 5.63 ms | **2.61** / **3.34** / **4.87** ms (2.47 / 3.37 / 5.29 ms) |
| RenderThread frames unboosted (the higher of both sessions' votes) | 92 % | 80 % (63 %) |

No frame of either app missed the app's deadline. compose-rs's third run had 3 janky frames, all the compositor's: two prediction errors and one SurfaceFlinger scheduling miss, none of them in the app's frames. One run of each app had all its frames presented a vsync late, on cadence (compose-rs's first, Compose's second), as two of compose-rs's did in round 8 and one of Compose's in round 7; neither entered buffer stuffing recovery.

compose-rs's frames are shorter on both threads. The main thread's median is 17 % shorter than Compose's (14 % in round 8, whose two late runs were slower; 21 % in round 7). Each animation frame draws again only the repaint boundary whose content changed (D155), the spinner's group alone (D323). The pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The header's three transitions now run a task each (D420), which [15](15-android-stress-benchmark.md) measured as no change on this screen. The slowest main-thread frames are the system's (vsync requests, waking and waiting for the RenderThread), the same in Compose, so p99 moves least. The RenderThread's frames are 12 % shorter than Compose's at the median and 17 % at p90; the spinner's arc is one `CircleOp`, as in Compose (D320).

### Work per frame

| Scrolling (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 842 ms | **467 ms** (−45 %; round 8: 454 ms) |
| RenderThread work | 881 ms | **810 ms** (−8 %; 821 ms) |
| Main thread CPU time | 4.38 s | **3.64 s** (3.57 s) |
| RenderThread CPU time | **4.40 s** | 4.53 s (4.52 s) |

| Animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 254 ms | **199 ms** (−22 %; round 8: 198 ms) |
| RenderThread work | 532 ms | **499 ms** (−6 %; 556 ms) |
| Main thread CPU time | 2.05 s | **1.63 s** (1.80 s) |
| RenderThread CPU time | 3.79 s | **3.29 s** (3.07 s) |

compose-rs's main thread does 45 % less work than Compose's when scrolling and 22 % less in the animation. In round 8 the scroll's gap was 50 %, against a Compose run above its usual range (917 ms); this round's 842 ms is within it. compose-rs's own scroll work is round 8's within 3 %.

The RenderThread does 8 % less work than Compose's scrolling, for 3 % more CPU time. In the animation its work is back to 499 ms (556 ms in round 8, whose two late runs were boosted most), 6 % less than Compose's, for 13 % less CPU time.

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 135 ms | **107 ms** (−21 %; round 8: 108 ms) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 45.0 ms | **26.3 ms** (25.6 ms) |
| `big`: 200 posts at once | 227 ms | **163 ms** (−28 %; 166 ms) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 169.5 ms | **89.8 ms** (91.7 ms) |

Compose's numbers are under `speed`. Compiled from its shipped profiles (`speed-profile`), its feed starts sooner: 120 against 132 ms in D362's session, where compose-rs's took 107 ms either way; `big` didn't change (228 ms). Both apps are within `TotalTime`'s noise of round 8 (a standard deviation of 4 to 11 ms over the 15 starts). This is the first round with D390's session opened at the second compose-rs frame after a view shows rather than in the first (traced as `compose-rs: hint session`); it moved neither start beyond that noise.

In `big`'s first frame (89.8 ms), composing 200 posts takes 16.8 ms and laying them out 34.3 ms. Drawing and serializing the display list take 0.9 ms, and replaying it on the `Canvas` 20.6 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 35.7 ms (`Record View#draw()`). On the feed's traced start, `bindApplication` is shorter too (15.0 against 18.3 ms; 11.8 against 18.8 ms in round 8, one traced start each): there is no Compose runtime to initialize.

### Memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 52.9 MB | **34.1 MB** (−36 %; round 8: 34.2 MB) |
| of which code | 7.2 MB | **2.2 MB** (2.3 MB) |
| of which Java heap | 3.6 MB | **1.3 MB** (1.3 MB) |
| of which native heap | 8.2 MB | 8.5 MB (8.5 MB) |
| of which graphics | 28.1 MB | **17.0 MB** (17.0 MB) |
| PSS after 10 flings | 86.8 MB | **80.2 MB** (80.2 MB) |
| of which Java heap | **3.6 MB** | 5.1 MB (5.1 MB) |
| of which native heap | 12.9 MB | **11.9 MB** (12.1 MB) |
| of which graphics | 56.8 MB | 55.3 MB (55.5 MB) |
| APK (release; decimal MB) | 1.89 MB (minified) | **1.77 MB**, with the 1.61 MB `.so` uncompressed inside (1.76 MB) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). Both apps' code grew with the stress screens (D400). After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (86.8 MB this round, with 3.6 MB of Java heap against 5.8 MB in round 8; up to 112 MB in earlier rounds: it allocates on every frame); compose-rs's stay within about a megabyte.

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots). D410 does the same for paths, which the feed doesn't draw anew; on the stress animation it took compose-rs's memory after 13 s from 405 to 204 MB ([15](15-android-stress-benchmark.md)).

The APK is smaller than Compose's. Since D361, release builds take a release profile for Android (fat LTO, one codegen unit, `panic = "abort"`), stored uncompressed so the library can be mapped, against Compose's minified dex ([14](14-performance-opportunities.md), #4b). bench-rs's APK holds androidx.profileinstaller too since D360 (about 38 KB). Both grew in round 8 with the stress screens (D400: the library by 181 KB, Compose's APK by 147 KB); D390 added 12 KB to the library, and D390's refinements, D410–D412 and D420–D424 9 KB more (1,613,192 bytes).

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

Compose's numbers are round 9's.

| Gap | Steps |
|---|---|
| Scrolling, janky frames per run (Compose: 0, 0, 0) | 1, 2, 0 in round 1, all the app's; 1, 0, 1 in round 2; 0, 0, 0 in round 3; 1, 3, 1 in round 4, all the compositor's (a vsync resync, a display stall); 0, 0, 0 with the prefetch budget on the real vsync and the warm-up swipe left out (D340, D343, round 5); 2, 0, 0 in round 6, the compositor's; 0, 0, 0 in rounds 7, 8 and 9 |
| Scrolling, main thread p99 (Compose: 6.55 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3; 3.11 ms with prefetched paragraphs recorded while idle (D322, round 4; p90 2.20 → 1.90 ms); 3.04 ms in round 5; 3.05 ms with the items' groups recorded while idle (D350, round 6; p90 1.92 → 1.84 ms); 2.86 ms in round 7 (within round 6's runs' spread), and in round 8; 2.76 ms in round 9 |
| Animation, main thread per frame p50 (Compose: 1.52 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3); 1.20 ms with a replay without interpreted calls and records naming nested groups (D321, D323, round 4); 1.21 ms in round 5; 1.18 ms in round 6; 1.21 ms in round 7; 1.33 ms in round 8, whose two runs presented a vsync late were slower (D390's A/B: 1.20 → 1.22 ms); 1.26 ms in round 9 |
| Animation, RenderThread work (Compose: 532 ms) | 505 ms in round 1; 535 ms in round 2 and 540 ms in round 3, even with Compose's (a square-capped spinner, a stroked path each frame); 517 ms with the round cap (D320, round 4); 501 ms in round 5; 502 ms in round 6; 505 ms in round 7; 556 ms in round 8, boosted (D390's A/B: unchanged, 450 → 444 ms over 9 s); 499 ms in round 9 |
| Scrolling, RenderThread work (Compose: 881 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3; 786 ms in round 4; 813 ms in round 5; 794 ms in round 6; 784 ms in round 7; 821 ms in round 8; 810 ms in round 9 |
| Scrolling, RenderThread p50 (Compose: 2.12 ms) | 2.71 ms in round 5, 2.80 ms in round 6, 2.70 ms in round 7: not compose-rs's work, but how rarely Android boosted it (at the same boost the durations match); 2.31 ms in round 8 with a hint session of its own (D390; Compose's p50 moves with how often its session boosts it: 1.85 to 2.33 ms in round 8's runs); 2.31 ms in round 9, with fewer of its frames boosted to 200 or more (Compose's: 2.04 to 2.17 ms in its runs) |
| Startup, `big` (Compose: 227 ms) | 186 ms in round 1; 178 ms in round 2; 179 ms in round 3; 174 ms with composition linear in its reads (D310, round 4); 163 ms with `BoringLayout` (D330, round 5), and in round 6; 160 ms in round 7; 166 ms in round 8; 163 ms in round 9 |
| PSS after 10 flings (Compose: 86.8 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3; 81.2 MB in round 4; 79.5 MB in round 5; 79.7 MB in round 6; 80.0 MB in round 7; 80.2 MB in round 8, and in round 9 |
| APK (Compose: 1.89 MB) | 2.4 MiB in round 2; 3.24 MB (3.1 MiB) in round 3, unstripped; 1.99 MB stripped (D301, round 4), and in round 5; 1.55 MB with the release profile (D361, round 6), profileinstaller included; 1.57 MB in round 7; 1.76 MB in round 8, with the stress screens (Compose's grew by 0.15 MB); 1.77 MB in round 9. Closed |

In round 3, D270 (the display list as a tree) was also measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` unchanged); scrolling, `frame` 0.273 → 0.237 ms. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs and the todo toggle from 263 to 194 µs. The changes of rounds 4 to 8 were measured the same way in [14](14-performance-opportunities.md), and round 9's (D410–D412, D420–D424) on the stress screens in [15](15-android-stress-benchmark.md).

## What made the difference

- **Main-thread frames**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270), and a group's record names its nested groups by key, so a nested group's change leaves its parent's recording alone (D323); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose; the replay calls no interpreted framework method (D321). Lazy lists compose and premeasure their next item when the main thread is idle (D156), record its paragraphs (D322) and draw and record its groups then (D350), which takes composition, text layout and recording out of the scrolled frames; they keep the two items behind the scroll composed (D375) and recorded (D380), so a fling back's first frames do no more.
- **Jank**: the idle-time budget is counted from the real vsync, so prefetching goes on through Choreographer's buffer stuffing recovery (D340), and an idle-time step starts only if its recent maximum fits with a margin, so it doesn't delay the next frame (D341), scaled by how much slower than expected the idle time's steps so far ran (D381).
- **RenderThread frames**: kept groups are drawn by reference; small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190); the spinner's round cap is drawn as a circle (D320). Without long main-thread frames, HWUI's hint session rarely gets the RenderThread boosted, which lengthened the scroll's frames but not their work; a session of compose-rs's own, reporting the RenderThread's CPU time against 6.5 ms, now gets it boosted far more often (half its scroll frames, against 4 %; Compose's: a half to two thirds), for about 2 % more CPU power (D390).
- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; composition is linear in its reads (D310), and single lines are laid out with `BoringLayout` (D330). The big screen's first frame takes 53 % of Compose's.
- **Memory and size**: the app's code is a 1.6 MB native library rather than Compose's dex and runtime (5.0 MB less code mapped, 2.3 MB less Java heap), built with LTO (D361); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).

## Next steps

[14](14-performance-opportunities.md) measures what's left. For frame durations:

- **The scroll's slowest main-thread frames**: since D375 and D380, a fling back's first frames compose, lay out and record nothing either, and no frame of compose-rs's was janky in rounds 7 to 9. What sets the p99 now (2.76 ms) hasn't been broken down. On the stress list, each round-9 run had 1 to 3 frames of about 10 ms that laid a newly revealed item out inside the frame ([15](15-android-stress-benchmark.md)), none janky.
- **Text layout while prefetching** is still about half of a prefetched item's work, after `BoringLayout` (D330), and most of the big screen's 35 ms of layout. Laying out on worker threads (#11) took the scroll's main p99 from 3.53 to 3.08 ms and the big screen's first-frame work down 16 % in a prototype that isn't production-ready (measured before D330); batching JNI calls alone is capped at about 2.6 ms of the big frame.
- **Late `doFrame`s**: 12 to 21 a scroll run start more than 2 ms after their vsync (round 9), most woken late (the `Choreographer#onVsync` message itself comes 2 to 4 ms after the vsync: the scheduler's doing). 4 in 3 runs waited for compose-rs's idle-time work (none in rounds 7 and 8), and 8 to 16 a run on the stress list, whose main thread now runs on the little cores more ([15](15-android-stress-benchmark.md)). The ones that waited for compose-rs's idle-time work were a prefetch measuring an item on the little cores; D381 halved the idle-time slices running past the vsync, and those left overrun within a step whose start gave no sign. Splitting an item's measurement at its paragraphs (so a step can stop at the deadline) would be the next step, at the cost of an interruptible measure.
- **The compositor's jank**: none scrolling in rounds 7 to 9 (round 6: 2, SurfaceFlinger's, in one run); 3 in one animation run of round 9 (two prediction errors, a SurfaceFlinger scheduling miss). In D440's A/B, 4 of the 48 runs with the main thread in the session had 1 to 3 (prediction errors, SurfaceFlinger scheduling, the display HAL), against none of main's 48, and 6 of 48 for the most boosted variant: too few to tell from noise. Watched in the next rounds.
- **The scroll's RenderThread durations** follow ADPF's boost. Since D390, compose-rs's own session boosts the RenderThread; its 6.5 ms target is a trade of power for durations, swept in [14](14-performance-opportunities.md). How often each app ends up boosted varies from session to session (compose-rs's frames unboosted 51 to 62 %, Compose's 34 to 51 %, in the four sessions measured).
- **The animation's p99** frames are the system's (vsync requests, the RenderThread's sync), which a `View` pays as Compose does; only drawing without HWUI (option D of [13](13-android-drawing-paths.md), our own Skia) would remove part of them.
- **Smaller items**: caching the display's refresh rate in `prefetch()` (1.9 ms per scroll run); rounded clips through a `Path` (about 15 % of the animation's replay); the kept groups' bookkeeping (D380), a few microseconds in every scrolled frame (the replay's median 77 → 82 µs in its A/B), which listing only changes would cut.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
