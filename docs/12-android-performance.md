# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in jank, per-frame durations and work, startup and memory.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX but `androidx.profileinstaller`, as the Kotlin app, since D360). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring; every round so far used `speed` for both apps (recorded since round 4). That isn't Compose's best case for startup: compiled from its shipped profiles (`-m speed-profile`, as a Play install compiles it, with an app image), Compose's feed starts in 120 ms against 132 ms with `speed` (in D362's session), so compose-rs (107 ms either way) leads by 13 ms rather than 25 (21 in round 10, under `speed`). `measure.py --compile speed-profile` measures that way, with each app's shipped Baseline Profile (D362): Compose's libraries' and the app's own, and the compose-rs library's (D360); see "Compiled from the shipped Baseline Profiles" below.

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it. Since D302, it first checks that the installed apps' native libraries for the device's ABI are those built in the checkout (and stops if not), and records each APK's size and SHA-1, the compilation filter (`--compile`, `speed` by default) and each app's compilation state (`dumpsys package dexopt`) at the start and the end of the run, and stops unless each app starts compiled as asked. With `--compile speed-profile`, each app is reset, given the Baseline Profile its APK ships through androidx.profileinstaller's receiver, and compiled from it (D362). Both apps run androidx.profileinstaller's startup initializer on every cold start, in every mode. A traced run whose trace holds no frame of the app's is run again (D362). Its parsing is tested on captured output: `python3 android/bench/test_measure.py`.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler. Since D343, a warm-up swipe comes first and is left out (the stats start at the first input after it; gfxinfo is reset after it): the display, idle since the app started, resyncs its vsync on the first input, which FrameTimeline counted as jank no app caused. Each run also reports its janky frames by side (the app's deadline, or the compositor's: SurfaceFlinger's deadlines, prediction errors, the display), the frames presented at another vsync than expected (`present_type`, buffer stuffing's late frames included), the `doFrame`s starting more than 2 ms after their vsync (about 1.1 ms normally), and of those the ones that work on the main thread delayed (a slice ending more than 1.2 ms after the vsync: idle-time work, input; the rest woke late), compose-rs's `prefetch` calls, and whether Choreographer entered buffer stuffing recovery (its async track, or a `buffer stuffed` slice): `summarize.py` prints a line per run.
- **The RenderThread's boost**: since round 6 (D370), each traced run also reports how much Android's performance hints boosted the RenderThread. That is the HWUI hint session's boost minimum: the `uclamp.min` that ADPF sets for HWUI's threads, out of 1024, which the power HAL traces as `adpf.<pid>-…-HWUI-…-min`. The report gives its time-weighted mean over the run's frames, the share of frames started under 100 (unboosted), and the `DrawFrame` percentiles in each boost bucket (under 100, 100–199, 200 and more). It also gives the share of the RenderThread's running time on the little cores (those of the lowest capacity, from the device's `cpu_capacity`). A trace without the counter reports the little cores' share only. Since D390, compose-rs's own session boosts the RenderThread too, and the power HAL applies the higher of the two sessions' votes: `render_boost` still reads HWUI's counter alone, so the picture below (rounds 8 to 10) comes from the same traces with both sessions' votes (the round's JSON, `d390_boost`).
- **Isolation**: before each run, measure.py force-stops our other apps (`rs.compose.*` but the one measured): the other benchmark app, left cached by the run before, and the demo, whose animation ran a frame on every vsync in the background before D342.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.
- **Stress** (D400, D401): a much heavier list (`stress_startup`, `stress_scroll`) and a screen of hundreds of concurrent animations (`stress_animation`), measured as above; [15](15-android-stress-benchmark.md) describes them and their results. `--scenarios` runs some scenarios only (all by default), and every traced run lists its slowest frames with their longest slices (`slowest_frames`).

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) boost the threads, onto faster cores at higher frequencies, after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on; durations are what a user sees.

Measurements drift between sessions: Compose's own numbers move by up to 5 to 9 % from one session to the next (its scrolling main-thread work was 827 ms in round 3, 760 ms in round 4, 844 ms in round 5, 843 ms in round 6, 839 ms in round 7, 917 ms in round 8, 842 ms in round 9, 847 ms in round 10). So the two apps are compared within a session, and changes to compose-rs are measured on their own with interleaved A/B runs ([14](14-performance-opportunities.md)); a difference between rounds smaller than that is drift.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians of 3 runs (startup: of 15). The better side is in bold; ties aren't.

"Round 10" is compose-rs at b3ef0c4 (`results/2026-10-23-pixel-8-pro-round-10.json`, `speed`, every scenario: the stress ones' results are in [15](15-android-stress-benchmark.md)), the current results. measure.py's checks passed: the installed native libraries matched those built, and both apps were `speed` at the start and the end. Each of Compose's three scroll runs was run again once, its first trace holding no frame of the app's (D362). "Round 9" is compose-rs at c57a8e1 (`results/2026-10-23-pixel-8-pro-round-9.json`); its numbers are quoted in parentheses. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers are from the round 10 run.

Since round 9:

- **D430–D435: lazy items reused by content type**, as Compose's `ReusableContent` and `LazyLayoutItemReusePolicy`. An item coming into the list takes the composition and nodes of an item of the same content type that left (up to 7 kept per type), so composing it recomputes its `remember`s and runs its bodies, but makes no group or node anew where it can reuse the old one. The feed's posts and header, and the stress list's headers and four kinds of items, pass content types. Headless, an item scrolled into the feed went from 41.0 to 34.5 µs (−16 %), into the stress list from 21.3 to 19.1 µs (−10 %) ([14](14-performance-opportunities.md), #19). "Item reuse on the device" below has what the traces show.
- **The Kotlin feed passes `contentType` too** (`"post"`, and `"header"` for the animated header), so both apps stay the same. Compose's lazy layouts already reused posts without it: items without a content type are compatible with one another. So the change only keeps the header apart, and Compose's numbers didn't move beyond the runs' spread (its scroll main-thread work 842 → 847 ms; a prefetched post's composition 0.25 ms of work in both rounds).
- **D440–D442: the main thread joins compose-rs's performance hint session** (D390's, which held the RenderThread alone), so it gets the same vote. The reports don't change. In its interleaved A/B, the stress list's main thread p50 went from 1.54 to 1.07 ms; the feed's scroll from 1.32 to 1.24 ms ("Scrolling: the RenderThread's boost" below).

### Scrolling: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,783 | 1,792 |
| Janky frames (FrameTimeline), per run | **0, 0, 0** | 1, 0, 0 (round 9: 0, 0, 0) |
| of which the app's | **0, 0, 0** | 1, 0, 0 (0, 0, 0) |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | **0, 0, 0** | 128, 0, 0 (0, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 16 (4), 31 (6), 23 (5) | **12 (0), 3 (0), 7 (0)** (16 (3), 12 (0), 21 (1)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / **6** / **8** ms (5 / 6 / 7 ms) |
| Main thread per frame p50 / p90 / p99 | 1.51 / 3.19 / 6.74 ms | **1.27** / **1.83** / **2.85** ms (1.36 / 1.90 / 2.76 ms) |
| RenderThread per frame p50 / p90 / p99 | **2.11** / **3.41** / 4.98 ms | 2.35 / 3.55 / 4.93 ms (2.31 / 3.33 / 5.13 ms) |

Both scroll at 120 Hz. **compose-rs had one janky frame**, in its first run: its first janky scroll frame since round 6; FrameTimeline counts it as the app's (`App Deadline Missed`). Its RenderThread frame took 16.2 ms, 10.9 ms of it in `dispatchSetFrameTimelineInfo`: a call to SurfaceFlinger (`setFrameTimelineInfo`), in which the RenderThread slept for 10.7 ms. The main thread waited for it (`postAndWait`, 10.8 ms), so that frame was the run's slowest on both threads (11.9 ms on the main thread). compose-rs's own work in it was a normal frame's. Round 9 had a 5.3 ms wait in the same call, not janky. After it, the run's frames were presented a vsync late, on cadence, for about a second: with 17 such frames at the start of the run, after a SurfaceFlinger miss in the warm-up swipe, they make the run's 128. No frame of Compose's was janky or presented late. compose-rs prefetched 354 to 364 times a run.

**The main thread's frames are shorter than Compose's** at every percentile: the median by 16 %, the slowest by more than half (p99 2.85 against 6.74 ms; 8.57 ms in round 1). Prefetching composes and lays out the next item between frames (D156), records its paragraphs (D322) and draws and records its groups (D350). So the frame that reveals an item composes nothing, lays no text out and records nothing new. A fling back finds the items it wants composed (D375) and recorded (D380), so its first frames do no more than the others. Against round 9, the p50 and p90 are shorter (1.36 → 1.27 ms, 1.90 → 1.83 ms), as in D440's A/B: the main thread now ran 73 % of its time on the little cores, against 83 %. The p99 moved within the runs' spread (2.81, 3.15 and 2.85 ms a run, against 2.76, 2.76 and 2.94 ms).

**`doFrame`s starting late**: compose-rs's 3 to 12 a run, none of them waiting for its own main-thread work (round 9: 12 to 21, 4 of them waiting); Compose's 16 to 31, 15 of them waiting for its work.

**The RenderThread's frames** take about as long as in round 9 at the median (2.35 against 2.31 ms), 0.24 ms longer than Compose's (2.11 ms). Its p90 is 0.14 ms longer than Compose's, its p99 the same (4.93 against 4.98 ms). How often each app's session boosted it decides most of this (next section).

### Scrolling: the RenderThread's boost

ADPF boosts a thread after the frames a hint session holding it reports run long against their target. HWUI's session holds the main thread and the RenderThread and reports each frame from the main thread's start to the RenderThread's end against 11.62 ms, so the main thread's long frames earn the boost; compose-rs's own session (D390) held the RenderThread alone in rounds 8 and 9, and holds the main thread too since D440 (round 10); it reports the RenderThread's CPU time per frame against 6.5 ms. The power HAL gives a thread the highest active vote of its sessions. measure.py's `render_boost` reads HWUI's counter alone; the rows below take each frame's boost as the higher of both sessions' votes when it started, from the same traces (the round's JSON, `d390_boost`). Medians of the 3 runs (each bucket's percentiles: of the runs with frames in it). The table describes rather than ranks, so nothing is in bold:

| Scroll | Kotlin Compose | compose-rs (round 9) |
|---|---|---|
| Boost minimum, time-weighted (out of 1024) | 138 (runs: 132 to 155) | 92 (73 to 113) (93) |
| of which HWUI's session's alone | 138 | 20 (24) |
| Frames unboosted (minimum under 100) | 44 % (44 to 46 %) | 56 % (47 to 71 %) (56 %) |
| Frames at 100–199 / at 200 or more | 24 % / 31 % | 36 % / 8 % (38 % / 6 %) |
| RenderThread running time on the little cores | 22 % | 23 % (20 %) |
| Main thread running time on the little cores | 55 % | 73 % (83 %) |
| Unboosted frames: p50 / p90 / p99 | 2.81 / 3.99 / 5.44 ms | 2.83 / 3.76 / 5.26 ms (2.72 / 3.75 / 5.49 ms) |
| Frames at 100–199: p50 / p90 / p99 | 2.08 / 2.78 / 3.45 ms | 2.09 / 2.89 / 4.21 ms (2.10 / 2.74 / 4.04 ms) |
| Frames at 200 or more: p50 / p90 / p99 | 1.54 / 2.05 / 2.56 ms | 1.57 / 1.98 / 2.56 ms (1.70 / 2.37 / 2.82 ms) |

At the same boost, the two apps' RenderThread frames take the same time, as before: each bucket's p50 within 0.05 ms. compose-rs's frames were boosted as in round 9 (56 % unboosted, 8 % at 200 or more), as D440's A/B found: the main thread's joining the session changed neither its reports nor its vote. Compose's were boosted a little more often than in round 9 (44 % unboosted, against 47 %; 31 % at 200 or more, against 26 %). So compose-rs's RenderThread p50 is still the longer, by the share of its frames at 200 or more.

The target is a choice: [14](14-performance-opportunities.md) swept 3.5 to 7.5 ms. At 6.5 ms, compose-rs's frames were unboosted as often as Compose's in the sweep's sessions (51 % against 51 %), and more often in the A/B's (62 % against 49 %), round 8's (53 % against 34 %), round 9's (56 % against 47 %) and round 10's (56 % against 44 %): far more boosted than before D390, less than Compose in four sessions of five. Lower targets boost every frame (no frame unboosted at 3.5 or 4.5 ms) for 10 to 17 % more CPU power.

**The cost.** In D390's interleaved A/B, the CPU rails drew 2.1 % more power scrolling and 2.3 % more animating, and all the power rails (display included) 0.4 % and 0.5 % more; 6 runs don't resolve it (95 % intervals of about −3 to +7 % and −4 to +9 % for the CPU rails). Compose's CPU rails drew 9 % more than compose-rs's without the session, scrolling. The boosted RenderThread runs at higher frequencies: less CPU time (−9 %) for about the same work (+1.6 %). The power HAL spends 34 ms more CPU per scroll run on the session's reports (+5 %), and the main thread's work rose 2.6 % scrolling and 3.5 % animating (about 6 µs a frame: the clock read and the report call).

**The vote's level, and the main thread (D440).** Under the target, the power HAL's controller is a random walk: about 23 off the vote every 83 ms batch, whatever the vote, and 7.7 added per 100 µs by which a batch's last report exceeds the one before. Its level is set by how much the frames vary from one to the next, not by how far they are under the target: about 100 on the feed, about 250 to 320 on the stress list, and the same mechanism holds Compose's HWUI session at about 130 and 290 ([14](14-performance-opportunities.md), "The main thread in the session"). On the stress list in round 9, with the RenderThread boosted, HWUI's session stopped boosting compose-rs's main thread, which then ran 83 % of its time on the little cores. Since D440 our session holds the main thread too. In its interleaved A/B (12 runs each), the feed's scroll main thread p50 / p90 / p99 went 1.32 / 1.86 / 2.88 → 1.24 / 1.75 / 2.70 ms, the stress list's 1.54 / 2.24 / 3.53 → 1.07 / 1.71 / 2.97 ms, with no change in the RenderThread, the votes or power resolved. Round 10 measured it beside Compose: on the feed, the main thread ran 73 % of its time on the little cores (83 % in round 9) and its p50 / p90 went 1.36 / 1.90 → 1.27 / 1.83 ms; on the stress list, 21 % (83 %), for a p50 of 1.02 ms against Compose's 1.29 ms ([15](15-android-stress-benchmark.md)). Its A/B saw compositor-side janky frames in 4 of 48 runs, against none without it; round 10 had none in any of compose-rs's 12 runs.

In the animation, neither app's main thread has long frames, and HWUI's session rarely boosts either app: Compose's frames were 86 % unboosted (62 to 96 %). compose-rs's own session boosts some of them, though its RenderThread was already the faster: 55 % to 71 % unboosted (median 69 %; round 9: 80 %); in D390's A/B, 74 % (53 to 92 %), for p50 2.63 → 2.54 ms.

### Animation: frames (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,344 | 1,327 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 (round 9: 0, 0, 3) |
| of which the app's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 (0, 0, 3) |
| Frames presented at another vsync than expected, per run | 0, 1,204, 1,196 | 0, 0, 1,200 (1,199, 0, 3) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 9 (2), 11 (2), 19 (0) | **7 (0), 8 (0), 4 (0)** (3 (0), 5 (0), 11 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 9 ms | 5 / **5** / **7** ms (5 / 6 / 7 ms) |
| Main thread per frame p50 / p90 / p99 | 1.54 / 2.13 / 3.11 ms | **1.19** / **1.69** / **2.95** ms (1.26 / 1.80 / 2.83 ms) |
| RenderThread per frame p50 / p90 / p99 | 2.71 / 4.13 / 5.66 ms | **2.44** / **3.31** / **4.73** ms (2.61 / 3.34 / 4.87 ms) |
| RenderThread frames unboosted (the higher of both sessions' votes) | 86 % | 69 % (80 %) |

No frame of either app was janky. Some runs had all their frames presented a vsync late, on cadence, after entering Choreographer's buffer stuffing recovery: two of Compose's and one of compose-rs's. In round 9, one run of each was presented late throughout without entering it.

compose-rs's frames are shorter on both threads. The main thread's median is 23 % shorter than Compose's (17 % in round 9, 14 % in round 8, 21 % in round 7). Each animation frame draws again only the repaint boundary whose content changed (D155), the spinner's group alone (D323). The pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The header's three transitions run a task each (D420). The slowest main-thread frames are the system's (vsync requests, waking and waiting for the RenderThread), the same in Compose, so p99 moves least. The RenderThread's frames are 10 % shorter than Compose's at the median and 20 % at p90; the spinner's arc is one `CircleOp`, as in Compose (D320). Against round 9, compose-rs's main thread p50 and the RenderThread's p50 are shorter (1.26 → 1.19 ms, 2.61 → 2.44 ms), as in D440's A/B (1.22 → 1.19 ms and 2.54 → 2.45 ms), with more of the RenderThread's frames boosted (69 % unboosted, against 80 %).

### Work per frame

| Scrolling (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 847 ms | **479 ms** (−43 %; round 9: 467 ms) |
| RenderThread work | 887 ms | **804 ms** (−9 %; 810 ms) |
| Main thread CPU time | 4.38 s | **3.23 s** (3.64 s) |
| RenderThread CPU time | **4.36 s** | 4.73 s (4.53 s) |

| Animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 263 ms | **210 ms** (−20 %; round 9: 199 ms) |
| RenderThread work | 583 ms | **516 ms** (−11 %; 499 ms) |
| Main thread CPU time | 2.09 s | **1.54 s** (1.63 s) |
| RenderThread CPU time | 3.61 s | **3.12 s** (3.29 s) |

compose-rs's main thread does 43 % less work than Compose's when scrolling and 20 % less in the animation. Its own work rose 3 % scrolling (467 → 479 ms) and 6 % animating (199 → 210 ms), for 11 % and 6 % less CPU time: the main thread, now in compose-rs's session, ran at higher frequencies, where the work metric reads higher for the same code (D440's A/B: +5 % on the feed). Item reuse took about 4 ms off the prefetching (next section), less than that.

The RenderThread does 9 % less work than Compose's scrolling, for 8 % more CPU time. In the animation, 11 % less, for 14 % less CPU time; Compose's RenderThread work rose from 532 to 583 ms, its one run boosted most (25 % of its frames at 200 or more) doing the most.

### Item reuse on the device (D430–D435)

compose-rs's traces time each prefetch step (`compose-rs: prefetch`), and inside it the composition of an item (`prefetch compose`), its measurement (`prefetch measure`, text layout included) and the recording of its paragraphs. Compose's time its own (`compose:lazy:prefetch:compose`, `…:measure`). The table gives, per run, the main thread's work inside those slices from the stats' start (ms on the fastest core at its highest frequency), computed from the traces of rounds 9 and 10 (the round's JSON, `prefetch_slices`). Medians of 3 runs; nothing ranked:

| Feed scroll (10 flings), per run | compose-rs, round 9 | compose-rs, round 10 | Compose, round 10 |
|---|---|---|---|
| Prefetch steps | 372 | 362 | |
| Item compositions; work each | 349; 0.096 ms | 347; 0.086 ms (−10 %) | 228; 0.255 ms (round 9: 0.245 ms) |
| Composition work | 33.3 ms | 29.7 ms (−11 %) | 56.8 ms |
| Measurements; work each | 174; 0.308 ms | 173; 0.310 ms | 179; 0.483 ms |
| Paragraphs laid out while prefetching | 1,044 | 911 (−13 %) | |
| Paragraphs recorded | 221 | 194 (−12 %) | |
| All prefetch work | 93.0 ms | 88.9 ms (−4 %) | |
| Main thread on the little cores, while prefetching | 82 to 85 % | 70 to 76 % | |

| Stress list scroll, per run | compose-rs, round 9 | compose-rs, round 10 | Compose, round 10 |
|---|---|---|---|
| Prefetch steps | 423 | 352 | |
| Item compositions; work each | 307; 0.163 ms | 347; 0.167 ms | 387; 0.258 ms (round 9: 0.254 ms) |
| Composition work | 48.5 ms | 58.1 ms | 99.9 ms |
| Measurements; work each | 213; 0.318 ms | 211; 0.304 ms | 178; 0.544 ms |
| Paragraphs laid out while prefetching | 1,283 | 670 (−48 %) | |
| Paragraphs recorded | 265 | 200 (−25 %) | |
| All prefetch work | 127.6 ms | 134.5 ms | |
| Main thread on the little cores, while prefetching | 85 to 87 % | 18 to 19 % | |

**On the feed**, composing a prefetched post takes 10 % less work (0.096 → 0.086 ms), and 0.58 against 0.75 ms of wall time. Headless, reuse took 16 % off an item scrolled into view (composition, measurement and drawing together). Fewer paragraphs are laid out (−13 %) and recorded (−12 %): a reused text node whose text and constraints didn't change, such as a tag or a like count that two posts share, most likely keeps its layout. All prefetch work went down 4 %.

**On the stress list**, the per-item numbers can't be read the same way: its main thread moved from the little cores to the mid and big ones (D440), where the work metric reads about a fifth higher for the same code (D440's A/B: 498 → 610 ms of main-thread work, for a third less CPU time). An item's composition did 0.167 ms of work against 0.163 ms, its measurement 0.304 against 0.318 ms, and the prefetch steps took half the wall time (1.14 against 2.18 ms each). Half as many paragraphs were laid out (1,283 → 670): the chips', buttons' and section headers' labels are the same from one item of a kind to the next. Separating reuse's own effect from D440's would take an interleaved A/B on the device.

Compose's prefetched items cost the same as in round 9 on both lists (0.255 and 0.258 ms of work each): its posts were already reused without content types.

### Lazy lists place only the items in view (D470–D474), after round 10

Not a round: an interleaved A/B of main (727d266) against D470–D474, 12 runs each ([15](15-android-stress-benchmark.md), "The lazy list's policy", has both lists and the variants weighed). A list now places only the items in view (it placed a viewport's worth more on each side), prefetches from its first layout on, and its first layout composes what it shows. On the feed's scroll: main thread p50 / p90 / p99 1.23 / 1.76 / 2.72 → 1.16 / 1.68 / 2.73 ms, RenderThread 2.33 / 3.40 / 4.79 → 2.24 / 3.26 / 4.86 ms (p99 within the runs' spread), RenderThread work 810 → 772 ms, no janky frame and no text laid out in a frame either way, native heap after the flings 11.8 → 10.6 MB. Its start: `TotalTime` 108.5 → 104.0 ms, the first frame's own part 6.7 → 5.1 ms; the big screen's unchanged (162 ms). A review then found regressions D470 had hidden (scrolls onto large items out of view cut short, short first frames under a large first item, a viewport's worth composed on focus) and that TalkBack couldn't scroll a list; D475–D478 fix them, and an 8-run A/B against main confirmed the feed's numbers: main thread 1.23 / 1.75 / 2.75 → 1.17 / 1.67 / 2.71 ms, RenderThread 2.31 / 3.38 / 5.08 → 2.35 / 3.23 / 4.72 ms, work −6 % and −7 %.

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 131 ms | **110 ms** (−16 %; round 9: 107 ms) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 44.8 ms | **23.3 ms** (26.3 ms) |
| `big`: 200 posts at once | 230 ms | **164 ms** (−29 %; 163 ms) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 169.0 ms | **89.7 ms** (89.8 ms) |

Compose's numbers are under `speed`. Compiled from its shipped profiles (`speed-profile`), its feed starts sooner: 120 against 132 ms in D362's session, where compose-rs's took 107 ms either way; `big` didn't change (228 ms). Both apps are within `TotalTime`'s noise of round 9 (a standard deviation of 5 to 8 ms over the 15 starts; compose-rs's feed ranged over 104 to 134 ms, Compose's over 125 to 143 ms). Neither change since round 9 runs at startup: there is nothing to reuse in a first frame, and the hint session opens at the second frame.

In `big`'s first frame (89.7 ms), composing 200 posts takes 17.0 ms and laying them out 34.5 ms. Drawing the display list takes 1.0 ms, and replaying it on the `Canvas` 20.8 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 36.8 ms (`Record View#draw()`). On the feed's traced start, `bindApplication` is shorter too (13.2 against 20.4 ms; 15.0 against 18.3 ms in round 9, one traced start each): there is no Compose runtime to initialize.

### Memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 52.8 MB | **34.1 MB** (−35 %; round 9: 34.1 MB) |
| of which code | 7.2 MB | **2.2 MB** (2.2 MB) |
| of which Java heap | 3.6 MB | **1.3 MB** (1.3 MB) |
| of which native heap | 8.1 MB | 8.4 MB (8.5 MB) |
| of which graphics | 27.9 MB | **17.0 MB** (17.0 MB) |
| PSS after 10 flings | 89.0 MB | **80.6 MB** (80.2 MB) |
| of which Java heap | 6.0 MB | **4.7 MB** (5.1 MB) |
| of which native heap | 13.1 MB | **11.8 MB** (11.9 MB) |
| of which graphics | 56.6 MB | 56.5 MB (55.3 MB) |
| APK (release; decimal MB) | 1.89 MB (minified) | **1.79 MB**, with the 1.63 MB `.so` uncompressed inside (1.77 MB) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). Both apps' code grew with the stress screens (D400). After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (89.0 MB this round, with 6.0 MB of Java heap, against 86.8 MB and 3.6 MB in round 9; up to 112 MB in earlier rounds: it allocates on every frame); compose-rs's stay within about a megabyte (79.2 to 81.2 MB in this round's runs).

The items kept for reuse (D430: up to 7 of each content type, composed, with their nodes) didn't show on the feed: its native heap after 10 flings is round 9's within 0.1 MB. On the stress list, compose-rs's native heap was 1.0 MB larger after start and 1.5 MB larger after scrolling, in every run ([15](15-android-stress-benchmark.md), "Memory").

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots). D410 does the same for paths, which the feed doesn't draw anew; on the stress animation it took compose-rs's memory after 13 s from 405 to 204 MB ([15](15-android-stress-benchmark.md)).

The APK is smaller than Compose's. Since D361, release builds take a release profile for Android (fat LTO, one codegen unit, `panic = "abort"`), stored uncompressed so the library can be mapped, against Compose's minified dex ([14](14-performance-opportunities.md), #4b). bench-rs's APK holds androidx.profileinstaller too since D360 (about 38 KB). Both grew in round 8 with the stress screens (D400: the library by 181 KB, Compose's APK by 147 KB); D390 added 12 KB to the library, D390's refinements, D410–D412 and D420–D424 9 KB more, and item reuse (D430–D435) and the main thread in the session (D440) 20 KB more (1,632,832 bytes).

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

Compose's numbers are round 10's.

| Gap | Steps |
|---|---|
| Scrolling, janky frames per run (Compose: 0, 0, 0) | 1, 2, 0 in round 1, all the app's; 1, 0, 1 in round 2; 0, 0, 0 in round 3; 1, 3, 1 in round 4, all the compositor's (a vsync resync, a display stall); 0, 0, 0 with the prefetch budget on the real vsync and the warm-up swipe left out (D340, D343, round 5); 2, 0, 0 in round 6, the compositor's; 0, 0, 0 in rounds 7, 8 and 9; 1, 0, 0 in round 10, the app's by FrameTimeline: the RenderThread waited 10.9 ms in a call to SurfaceFlinger |
| Scrolling, main thread p99 (Compose: 6.74 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3; 3.11 ms with prefetched paragraphs recorded while idle (D322, round 4; p90 2.20 → 1.90 ms); 3.04 ms in round 5; 3.05 ms with the items' groups recorded while idle (D350, round 6; p90 1.92 → 1.84 ms); 2.86 ms in round 7 (within round 6's runs' spread), and in round 8; 2.76 ms in round 9; 2.85 ms in round 10, within the runs' spread, with the main thread in the hint session (D440; p90 1.90 → 1.83 ms) |
| Animation, main thread per frame p50 (Compose: 1.54 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3); 1.20 ms with a replay without interpreted calls and records naming nested groups (D321, D323, round 4); 1.21 ms in round 5; 1.18 ms in round 6; 1.21 ms in round 7; 1.33 ms in round 8, whose two runs presented a vsync late were slower (D390's A/B: 1.20 → 1.22 ms); 1.26 ms in round 9; 1.19 ms with the main thread in the hint session (D440, round 10) |
| Animation, RenderThread work (Compose: 583 ms) | 505 ms in round 1; 535 ms in round 2 and 540 ms in round 3, even with Compose's (a square-capped spinner, a stroked path each frame); 517 ms with the round cap (D320, round 4); 501 ms in round 5; 502 ms in round 6; 505 ms in round 7; 556 ms in round 8, boosted (D390's A/B: unchanged, 450 → 444 ms over 9 s); 499 ms in round 9; 516 ms in round 10 |
| Scrolling, RenderThread work (Compose: 887 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3; 786 ms in round 4; 813 ms in round 5; 794 ms in round 6; 784 ms in round 7; 821 ms in round 8; 810 ms in round 9; 804 ms in round 10 |
| Scrolling, RenderThread p50 (Compose: 2.11 ms) | 2.71 ms in round 5, 2.80 ms in round 6, 2.70 ms in round 7: not compose-rs's work, but how rarely Android boosted it (at the same boost the durations match); 2.31 ms in round 8 with a hint session of its own (D390; Compose's p50 moves with how often its session boosts it: 1.85 to 2.33 ms in round 8's runs); 2.31 ms in round 9, with fewer of its frames boosted to 200 or more (Compose's: 2.04 to 2.17 ms in its runs); 2.35 ms in round 10, boosted as in round 9 (Compose's: 2.01 to 2.16 ms) |
| Startup, `big` (Compose: 230 ms) | 186 ms in round 1; 178 ms in round 2; 179 ms in round 3; 174 ms with composition linear in its reads (D310, round 4); 163 ms with `BoringLayout` (D330, round 5), and in round 6; 160 ms in round 7; 166 ms in round 8; 163 ms in round 9; 164 ms in round 10 |
| PSS after 10 flings (Compose: 89.0 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3; 81.2 MB in round 4; 79.5 MB in round 5; 79.7 MB in round 6; 80.0 MB in round 7; 80.2 MB in round 8, and in round 9; 80.6 MB in round 10, with items kept for reuse (D430) |
| APK (Compose: 1.89 MB) | 2.4 MiB in round 2; 3.24 MB (3.1 MiB) in round 3, unstripped; 1.99 MB stripped (D301, round 4), and in round 5; 1.55 MB with the release profile (D361, round 6), profileinstaller included; 1.57 MB in round 7; 1.76 MB in round 8, with the stress screens (Compose's grew by 0.15 MB); 1.77 MB in round 9; 1.79 MB in round 10. Closed |

In round 3, D270 (the display list as a tree) was also measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` unchanged); scrolling, `frame` 0.273 → 0.237 ms. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs and the todo toggle from 263 to 194 µs. The changes of rounds 4 to 8 were measured the same way in [14](14-performance-opportunities.md), round 9's (D410–D412, D420–D424) on the stress screens in [15](15-android-stress-benchmark.md), and round 10's: item reuse (D430–D433) headless in [14](14-performance-opportunities.md) (#19), the main thread in the hint session (D440) interleaved on the device in [14](14-performance-opportunities.md), "The main thread in the session".

## What made the difference

- **Main-thread frames**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270), and a group's record names its nested groups by key, so a nested group's change leaves its parent's recording alone (D323); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose; the replay calls no interpreted framework method (D321). Lazy lists compose and premeasure their next item when the main thread is idle (D156), record its paragraphs (D322) and draw and record its groups then (D350), which takes composition, text layout and recording out of the scrolled frames; they keep the two items behind the scroll composed (D375) and recorded (D380), so a fling back's first frames do no more. An item coming into a list takes the composition and nodes of one of the same content type that left (D430–D435), as in Compose: a prefetched feed post's composition does 10 % less work, and fewer paragraphs are laid out again.
- **Jank**: the idle-time budget is counted from the real vsync, so prefetching goes on through Choreographer's buffer stuffing recovery (D340), and an idle-time step starts only if its recent maximum fits with a margin, so it doesn't delay the next frame (D341), scaled by how much slower than expected the idle time's steps so far ran (D381).
- **RenderThread frames**: kept groups are drawn by reference; small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190); the spinner's round cap is drawn as a circle (D320). Without long main-thread frames, HWUI's hint session rarely gets the RenderThread boosted, which lengthened the scroll's frames but not their work; a session of compose-rs's own, reporting the RenderThread's CPU time against 6.5 ms, now gets it boosted far more often (half its scroll frames, against 4 %; Compose's: a half to two thirds), for about 2 % more CPU power (D390).
- **Where the main thread runs**: since D440, compose-rs's session holds the main thread too, which HWUI's session stopped boosting once the RenderThread's part of each frame got shorter. On the stress list, the main thread ran 21 % of its time on the little cores in round 10 (83 % in round 9), for a p50 of 1.02 against Compose's 1.29 ms.
- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; composition is linear in its reads (D310), and single lines are laid out with `BoringLayout` (D330). The big screen's first frame takes 53 % of Compose's.
- **Memory and size**: the app's code is a 1.6 MB native library rather than Compose's dex and runtime (5.0 MB less code mapped, 2.3 MB less Java heap), built with LTO (D361); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).

## Next steps

[14](14-performance-opportunities.md) measures what's left. For frame durations:

- **The scroll's slowest main-thread frames**: since D375 and D380, a fling back's first frames compose, lay out and record nothing either. No frame of compose-rs's was janky in rounds 7 to 9; round 10's one janky frame was the RenderThread waiting 10.9 ms in a call to SurfaceFlinger, not compose-rs's work. What sets the p99 now (2.85 ms) hasn't been broken down. On the stress list, each round-9 run had 1 to 3 frames of about 10 ms that laid a newly revealed item out inside the frame; round 10's had none (its slowest main-thread frame took 5.1 ms), with the main thread off the little cores ([15](15-android-stress-benchmark.md)).
- **Text layout while prefetching** is still about half of a prefetched item's work, after `BoringLayout` (D330), and most of the big screen's 35 ms of layout. Laying out on worker threads (#11) took the scroll's main p99 from 3.53 to 3.08 ms and the big screen's first-frame work down 16 % in a prototype that isn't production-ready (measured before D330); batching JNI calls alone is capped at about 2.6 ms of the big frame.
- **Late `doFrame`s**: 3 to 12 a scroll run start more than 2 ms after their vsync (round 10; 12 to 21 in round 9), woken late (the `Choreographer#onVsync` message itself comes 2 to 4 ms after the vsync: the scheduler's doing). None waited for compose-rs's idle-time work in round 10, on the feed or on the stress list (round 9: 4 in 3 runs on the feed, 8 to 16 a run on the stress list, whose main thread then ran on the little cores). Those that waited were a prefetch measuring an item on the little cores; D381 halved the idle-time slices running past the vsync, and D440 took the stress list's main thread off the little cores. If they come back, splitting an item's measurement at its paragraphs (so a step can stop at the deadline) would be the next step, at the cost of an interruptible measure.
- **The compositor's jank**: none scrolling in rounds 7 to 9 (round 6: 2, SurfaceFlinger's, in one run); 3 in one animation run of round 9 (two prediction errors, a SurfaceFlinger scheduling miss). In D440's A/B, 4 of the 48 runs with the main thread in the session had 1 to 3 (prediction errors, SurfaceFlinger scheduling, the display HAL), against none of main's 48, and 6 of 48 for the most boosted variant: too few to tell from noise. Round 10, with D440, had none in any measured run of either app (12 runs each, on the feed and the stress screens); the only ones in compose-rs's traces were prediction errors during the warm-up swipe, which the stats leave out by design (D343); one of Compose's traces had three there too (SurfaceFlinger's and the display HAL's). Still watched.
- **The scroll's RenderThread durations** follow ADPF's boost. Since D390, compose-rs's own session boosts the RenderThread; its 6.5 ms target is a trade of power for durations, swept in [14](14-performance-opportunities.md). How often each app ends up boosted varies from session to session (compose-rs's frames unboosted 51 to 62 %, Compose's 34 to 51 %, in the five sessions measured).
- **Item reuse on the stress list**: on the device, its per-item effect couldn't be separated from D440's, which moved the main thread to faster cores in the same round ("Item reuse on the device"); an interleaved A/B would. The items kept for reuse may be why compose-rs's native heap on the stress list is 1.0 MB larger after start and 1.5 MB after scrolling; that wasn't broken down.
- **The animation's p99** frames are the system's (vsync requests, the RenderThread's sync), which a `View` pays as Compose does; only drawing without HWUI (option D of [13](13-android-drawing-paths.md), our own Skia) would remove part of them.
- **Smaller items**: caching the display's refresh rate in `prefetch()` (1.9 ms per scroll run); rounded clips through a `Path` (about 15 % of the animation's replay); the kept groups' bookkeeping (D380), a few microseconds in every scrolled frame (the replay's median 77 → 82 µs in its A/B), which listing only changes would cut.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
