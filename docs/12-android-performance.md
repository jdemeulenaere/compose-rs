# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in jank, per-frame durations and work, startup and memory.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX but `androidx.profileinstaller`, as the Kotlin app, since D360). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring; every round so far used `speed` for both apps (recorded since round 4). That isn't Compose's best case for startup: compiled from its shipped profiles (`-m speed-profile`, as a Play install compiles it, with an app image), Compose's feed starts in 120 ms against 132 ms with `speed` (in D362's session), so compose-rs (107 ms either way) leads by 13 ms rather than 25 (25 in round 11 as well, under `speed`). `measure.py --compile speed-profile` measures that way, with each app's shipped Baseline Profile (D362): Compose's libraries' and the app's own, and the compose-rs library's (D360); see "Compiled from the shipped Baseline Profiles" below.

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it. Since D302, it first checks that the installed apps' native libraries for the device's ABI are those built in the checkout (and stops if not), and records each APK's size and SHA-1, the compilation filter (`--compile`, `speed` by default) and each app's compilation state (`dumpsys package dexopt`) at the start and the end of the run, and stops unless each app starts compiled as asked. With `--compile speed-profile`, each app is reset, given the Baseline Profile its APK ships through androidx.profileinstaller's receiver, and compiled from it (D362). Both apps run androidx.profileinstaller's startup initializer on every cold start, in every mode. A traced run whose trace holds no frame of the app's is run again (D362). Its parsing is tested on captured output: `python3 android/bench/test_measure.py`.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler. Since D343, a warm-up swipe comes first and is left out (the stats start at the first input after it; gfxinfo is reset after it): the display, idle since the app started, resyncs its vsync on the first input, which FrameTimeline counted as jank no app caused. Each run also reports its janky frames by side (the app's deadline, or the compositor's: SurfaceFlinger's deadlines, prediction errors, the display), the frames presented at another vsync than expected (`present_type`, buffer stuffing's late frames included), the `doFrame`s starting more than 2 ms after their vsync (about 1.1 ms normally), and of those the ones that work on the main thread delayed (a slice ending more than 1.2 ms after the vsync: idle-time work, input; the rest woke late), compose-rs's `prefetch` calls, and whether Choreographer entered buffer stuffing recovery (its async track, or a `buffer stuffed` slice): `summarize.py` prints a line per run.
- **The RenderThread's boost**: since round 6 (D370), each traced run also reports how much Android's performance hints boosted the RenderThread. That is the HWUI hint session's boost minimum: the `uclamp.min` that ADPF sets for HWUI's threads, out of 1024, which the power HAL traces as `adpf.<pid>-…-HWUI-…-min`. The report gives its time-weighted mean over the run's frames, the share of frames started under 100 (unboosted), and the `DrawFrame` percentiles in each boost bucket (under 100, 100–199, 200 and more). It also gives the share of the RenderThread's running time on the little cores (those of the lowest capacity, from the device's `cpu_capacity`). A trace without the counter reports the little cores' share only. Since D390, compose-rs's own session boosts the RenderThread too, and the power HAL applies the higher of the two sessions' votes: `render_boost` still reads HWUI's counter alone, so the picture below (rounds 8 to 11) comes from the same traces with both sessions' votes (the round's JSON, `d390_boost`).
- **Isolation**: before each run, measure.py force-stops our other apps (`rs.compose.*` but the one measured): the other benchmark app, left cached by the run before, and the demo, whose animation ran a frame on every vsync in the background before D342.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.
- **Stress** (D400, D401): a much heavier list (`stress_startup`, `stress_scroll`) and a screen of hundreds of concurrent animations (`stress_animation`), measured as above; [15](15-android-stress-benchmark.md) describes them and their results. `--scenarios` runs some scenarios only (all by default), and every traced run lists its slowest frames with their longest slices (`slowest_frames`).

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) boost the threads, onto faster cores at higher frequencies, after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on; durations are what a user sees.

Measurements drift between sessions: Compose's own numbers move by up to 5 to 9 % from one session to the next (its scrolling main-thread work was 827 ms in round 3, 760 ms in round 4, 844 ms in round 5, 843 ms in round 6, 839 ms in round 7, 917 ms in round 8, 842 ms in round 9, 847 ms in round 10, 815 ms in round 11). So the two apps are compared within a session, and changes to compose-rs are measured on their own with interleaved A/B runs ([14](14-performance-opportunities.md)); a difference between rounds smaller than that is drift.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians of 3 runs (startup: of 15). The better side is in bold; ties aren't.

"Round 11" is compose-rs at 613eaf6 (`results/2026-10-23-pixel-8-pro-round-11.json`, `speed`, every scenario: the stress ones' results are in [15](15-android-stress-benchmark.md)), the current results. measure.py's checks passed: the installed native libraries matched those built, and both apps were `speed` at the start and the end. Compose's first scroll run was run again once, its first trace holding no frame of the app's (D362). "Round 10" is compose-rs at b3ef0c4 (`results/2026-10-23-pixel-8-pro-round-10.json`); its numbers are quoted in parentheses. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers are from the round 11 run.

Since round 10:

- **D470–D496: lazy lists compose and place only the items in view**, as Compose's; they placed a viewport's worth more on each side, each item a render node the main thread replays and the RenderThread syncs every frame. Prefetch runs from the first layout on, not only after a scroll. The first layout composes what it shows: what a measurement asks for doubles from round to round, up to 16 rounds a frame (D472, D487, D492), where the stress list's first layout composed 87 items to show 16. Two fixes let prefetch run in idle time it used to skip (D471, D474). What a list can't use of a scroll goes on to its nested-scroll parents (D479, D489, D493), focus searches lay out items beyond a list's bounds one at a time (D477, D483, D486, D495), and lists have scroll semantics for accessibility (D478, D484, D491, D496). Their interleaved A/Bs are in [15](15-android-stress-benchmark.md), "The lazy list's policy"; "Lazy lists place only the items in view" below sums them up.
- **D460, D461**: an item that leaves a list without ever having been measured is disposed rather than kept for reuse, and `Group` is back to 144 bytes. That removed the stress list's extra native heap of round 10 ([15](15-android-stress-benchmark.md), "Memory").
- **D450–D459: lazy grids compose item by item**, with spans and content types, and follow their items' keys. No scenario here has a grid; the `grid` screen both apps gained (D473) is measured in [15](15-android-stress-benchmark.md).
- The library grew by 76 KB (1,632,832 → 1,708,480 bytes), and the Kotlin APK by 33 KB with the `grid` screen.

### Scrolling: frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,792 | 1,791 |
| Janky frames (FrameTimeline), per run | 1, 0, 0 | **0, 0, 0** (round 10: 1, 0, 0) |
| of which the app's | 1, 0, 0 | **0, 0, 0** (1, 0, 0) |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 2, 0, 0 | **0, 0, 0** (128, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 14 (5), 16 (2), 23 (5) | **12 (0), 10 (0), 7 (0)** (12 (0), 3 (0), 7 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / **5** / **7** ms (5 / 6 / 8 ms) |
| Main thread per frame p50 / p90 / p99 | 1.60 / 3.30 / 7.19 ms | **1.16** / **1.68** / **2.74** ms (1.27 / 1.83 / 2.85 ms) |
| RenderThread per frame p50 / p90 / p99 | 2.34 / 3.54 / 5.16 ms | **2.10** / **3.10** / **4.86** ms (2.35 / 3.55 / 4.93 ms) |

Both scroll at 120 Hz. **No frame of compose-rs's was janky** (round 10's one was its RenderThread waiting 10.9 ms in a call to SurfaceFlinger), and none was presented late. Compose missed one deadline, in its first run: its slowest main-thread frame (14.8 ms) composed and measured a newly revealed post inside the frame (`AndroidOwner:measureAndLayout`, 7.8 ms). compose-rs prefetched 353 to 363 times a run; no frame of its laid text out or ran a second round of composition and layout (the round's JSON, `lazy_lists`).

**The main thread's frames are shorter than Compose's** at every percentile: the median by 28 %, the slowest by more than half (p99 2.74 against 7.19 ms; 8.57 ms in round 1). Prefetching composes and lays out the next item between frames (D156), records its paragraphs (D322) and draws and records its groups (D350). So the frame that reveals an item composes nothing, lays no text out and records nothing new. A fling back finds the items it wants composed (D375) and recorded (D380), so its first frames do no more than the others. Against round 10, the p50 and p90 are shorter (1.27 → 1.16 ms, 1.83 → 1.68 ms), as in D470's A/Bs (1.22 to 1.24 → 1.16 to 1.17 ms at p50): with only the items in view placed, each frame replays fewer render nodes (`compose-rs: frame` 0.24 → 0.20 ms a frame). The p99 moved within the runs' spread (2.58, 2.87 and 2.74 ms a run, against 2.81, 3.15 and 2.85 ms). The main thread ran 73 % of its time on the little cores, as in round 10. Compose's p50 and p99 were longer than in round 10 (1.51 → 1.60 ms, 6.74 → 7.19 ms): drift between sessions.

**`doFrame`s starting late**: compose-rs's 7 to 12 a run, none of them waiting for its own main-thread work (as in round 10); Compose's 14 to 23, 12 of them waiting for its work.

**The RenderThread's frames are now shorter than Compose's** at every percentile: p50 2.10 against 2.34 ms (round 10: 2.35 against 2.11 ms). Both apps' frames were boosted about as often this session (52 % unboosted each), and at each boost compose-rs's are now the shorter (next section): its work fell 4 % (804 → 769 ms), as in D470's A/Bs (−5 to −8 %). Compose's p50 rose from 2.11 to 2.34 ms, its frames boosted less often than in round 10. So part of the change is the session: D470's A/Bs moved compose-rs's p50 by −0.09 to +0.12 ms, where this round moved it by −0.25 ms.

### Scrolling: the RenderThread's boost

ADPF boosts a thread after the frames a hint session holding it reports run long against their target. HWUI's session holds the main thread and the RenderThread and reports each frame from the main thread's start to the RenderThread's end against 11.62 ms, so the main thread's long frames earn the boost; compose-rs's own session (D390) held the RenderThread alone in rounds 8 and 9, and holds the main thread too since D440 (round 10); it reports the RenderThread's CPU time per frame against 6.5 ms. The power HAL gives a thread the highest active vote of its sessions. measure.py's `render_boost` reads HWUI's counter alone; the rows below take each frame's boost as the higher of both sessions' votes when it started, from the same traces (the round's JSON, `d390_boost`). Medians of the 3 runs (each bucket's percentiles: of the runs with frames in it). The table describes rather than ranks, so nothing is in bold:

| Scroll | Kotlin Compose | compose-rs (round 10) |
|---|---|---|
| Boost minimum, time-weighted (out of 1024) | 108 (runs: 108 to 146) | 100 (99 to 109) (92) |
| of which HWUI's session's alone | 108 | 9 (20) |
| Frames unboosted (minimum under 100) | 52 % (45 to 55 %) | 52 % (43 to 57 %) (56 %) |
| Frames at 100–199 / at 200 or more | 25 % / 20 % | 42 % / 10 % (36 % / 8 %) |
| RenderThread running time on the little cores | 26 % | 19 % (23 %) |
| Main thread running time on the little cores | 61 % | 73 % (73 %) |
| Unboosted frames: p50 / p90 / p99 | 2.85 / 4.09 / 5.65 ms | 2.63 / 3.51 / 5.29 ms (2.83 / 3.76 / 5.26 ms) |
| Frames at 100–199: p50 / p90 / p99 | 2.09 / 2.86 / 3.84 ms | 1.98 / 2.58 / 3.67 ms (2.09 / 2.89 / 4.21 ms) |
| Frames at 200 or more: p50 / p90 / p99 | 1.59 / 2.08 / 2.73 ms | 1.50 / 1.94 / 2.65 ms (1.57 / 1.98 / 2.56 ms) |

At the same boost, compose-rs's RenderThread frames are now shorter than Compose's: at p50 by 0.22 ms unboosted, 0.11 ms at 100–199 and 0.09 ms at 200 or more (within 0.05 ms of Compose's in round 10). That is D470's less work (769 against 804 ms): fewer placed items to sync and draw. compose-rs's frames were boosted about as in round 10 (52 % unboosted, against 56 %), Compose's less often (52 % against 44 %; 20 % at 200 or more, against 31 %).

The target is a choice: [14](14-performance-opportunities.md) swept 3.5 to 7.5 ms. At 6.5 ms, compose-rs's frames were unboosted as often as Compose's in the sweep's sessions (51 % against 51 %), and more often in the A/B's (62 % against 49 %), round 8's (53 % against 34 %), round 9's (56 % against 47 %) and round 10's (56 % against 44 %), and as often in round 11's (52 % against 52 %): far more boosted than before D390, less often than Compose in four sessions of six, as often in the other two. Lower targets boost every frame (no frame unboosted at 3.5 or 4.5 ms) for 10 to 17 % more CPU power.

**The cost.** In D390's interleaved A/B, the CPU rails drew 2.1 % more power scrolling and 2.3 % more animating, and all the power rails (display included) 0.4 % and 0.5 % more; 6 runs don't resolve it (95 % intervals of about −3 to +7 % and −4 to +9 % for the CPU rails). Compose's CPU rails drew 9 % more than compose-rs's without the session, scrolling. The boosted RenderThread runs at higher frequencies: less CPU time (−9 %) for about the same work (+1.6 %). The power HAL spends 34 ms more CPU per scroll run on the session's reports (+5 %), and the main thread's work rose 2.6 % scrolling and 3.5 % animating (about 6 µs a frame: the clock read and the report call).

**The vote's level, and the main thread (D440).** Under the target, the power HAL's controller is a random walk: about 23 off the vote every 83 ms batch, whatever the vote, and 7.7 added per 100 µs by which a batch's last report exceeds the one before. Its level is set by how much the frames vary from one to the next, not by how far they are under the target: about 100 on the feed, about 250 to 320 on the stress list, and the same mechanism holds Compose's HWUI session at about 130 and 290 ([14](14-performance-opportunities.md), "The main thread in the session"). On the stress list in round 9, with the RenderThread boosted, HWUI's session stopped boosting compose-rs's main thread, which then ran 83 % of its time on the little cores. Since D440 our session holds the main thread too. In its interleaved A/B (12 runs each), the feed's scroll main thread p50 / p90 / p99 went 1.32 / 1.86 / 2.88 → 1.24 / 1.75 / 2.70 ms, the stress list's 1.54 / 2.24 / 3.53 → 1.07 / 1.71 / 2.97 ms, with no change in the RenderThread, the votes or power resolved. Round 10 measured it beside Compose: on the feed, the main thread ran 73 % of its time on the little cores (83 % in round 9) and its p50 / p90 went 1.36 / 1.90 → 1.27 / 1.83 ms; on the stress list, 21 % (83 %), for a p50 of 1.02 ms against Compose's 1.29 ms ([15](15-android-stress-benchmark.md)). In round 11, with less RenderThread work on the stress list (D470), our session voted less there (190 against 295, time-weighted), and the main thread ran 46 % of its time on the little cores, for a p50 of 1.08 ms against Compose's 1.25 ms. Its A/B saw compositor-side janky frames in 4 of 48 runs, against none without it; rounds 10 and 11 had none in any of compose-rs's 12 runs.

**Both threads' work, against 8.33 ms (D500, after round 11).** The controller's vote does settle: where the reports' mean is about 0.4 of the target, for frames varying as ours do. So against 6.5 ms, the RenderThread's time alone held near 2.6 ms on every screen, and the lighter stress list got less boost. Our session now reports both threads' CPU time since the last frame against one 120 Hz interval, which puts that level where the frames' load is: the vote is 53 in the feed's animation, 261 on the feed's scroll and 332 on the stress list. In its interleaved A/B (10 runs each, [14](14-performance-opportunities.md), "The session's target"), the feed's scroll RenderThread p50 / p90 / p99 went 2.33 / 3.23 / 4.77 → 1.46 / 2.27 / 3.29 ms and its main thread 1.15 / 1.64 / 2.74 → 0.81 / 1.28 / 2.20 ms; the stress list's RenderThread 2.49 / 3.59 / 5.15 → 2.06 / 2.89 / 3.93 ms, below round 11's Compose. The animations didn't change. CPU-rail power rose 1.8 % (± 2.5) on the feed's scroll and 2.9 % (± 2.7) on the list's, all rails at most 0.6 %, and twice as many `doFrame`s started more than 2 ms late (none delayed by our work, none janky). The rounds above were measured before it.

In the animation, neither app's main thread has long frames, and HWUI's session rarely boosts either app: Compose's frames were 97 % unboosted (93 to 97 %; round 10: 86 %). compose-rs's own session boosts some of them, though its RenderThread was already the faster: 48 % to 61 % unboosted (median 60 %; round 10: 69 %); in D390's A/B, 74 % (53 to 92 %), for p50 2.63 → 2.54 ms.

### Animation: frames (10 s of the animated header)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,347 | 1,345 |
| Janky frames (FrameTimeline), per run | 2, 0, 0 | 0, 0, 0 (round 10: 0, 0, 0) |
| of which the app's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| of which the compositor's | 2, 0, 0 | 0, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 2, 0, 0 | **0, 0, 0** (0, 0, 1,200) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 10 (2), 14 (2), 9 (1) | **6 (0), 1 (0), 11 (0)** (7 (0), 8 (0), 4 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / **5** / **7** ms (5 / 5 / 7 ms) |
| Main thread per frame p50 / p90 / p99 | 1.59 / 2.27 / 3.10 ms | **1.16** / **1.57** / **2.82** ms (1.19 / 1.69 / 2.95 ms) |
| RenderThread per frame p50 / p90 / p99 | 3.05 / 4.18 / 5.69 ms | **2.39** / **3.15** / **4.45** ms (2.44 / 3.31 / 4.73 ms) |
| RenderThread frames unboosted (the higher of both sessions' votes) | 97 % | 60 % (69 %) |

No frame of either app missed its deadline; Compose's two janky frames, in its first run, were the compositor's prediction errors. No run of either app was presented a vsync late throughout (round 10: two of Compose's and one of compose-rs's, after entering Choreographer's buffer stuffing recovery).

compose-rs's frames are shorter on both threads. The main thread's median is 27 % shorter than Compose's (23 % in round 10, 17 % in round 9, 14 % in round 8, 21 % in round 7). Each animation frame draws again only the repaint boundary whose content changed (D155), the spinner's group alone (D323). The pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The header's three transitions run a task each (D420). The slowest main-thread frames are the system's (vsync requests, waking and waiting for the RenderThread), the same in Compose, so p99 moves least. The RenderThread's frames are 22 % shorter than Compose's at the median and 25 % at p90; the spinner's arc is one `CircleOp`, as in Compose (D320). Against round 10, the main thread's p90 is shorter (1.69 → 1.57 ms) and its p50 and p99 moved within the runs' spread (1.19 → 1.16 ms, 2.95 → 2.82 ms); the RenderThread's p50 moved from 2.44 to 2.39 ms and its work fell 5 % (516 → 492 ms). The animated header is the first item of the feed's lazy list, which now places only the items in view (D470).

### Work per frame

| Scrolling (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 815 ms | **473 ms** (−42 %; round 10: 479 ms) |
| RenderThread work | 853 ms | **769 ms** (−10 %; 804 ms) |
| Main thread CPU time | 4.61 s | **3.09 s** (3.23 s) |
| RenderThread CPU time | 4.60 s | **4.20 s** (4.73 s) |

| Animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 251 ms | **209 ms** (−17 %; round 10: 210 ms) |
| RenderThread work | 532 ms | **492 ms** (−8 %; 516 ms) |
| Main thread CPU time | 2.08 s | **1.52 s** (1.54 s) |
| RenderThread CPU time | 3.86 s | **3.06 s** (3.12 s) |

compose-rs's main thread does 42 % less work than Compose's when scrolling and 17 % less in the animation. Its own work moved within the runs' spread of round 10 (479 → 473 ms, 210 → 209 ms); D470's A/Bs measured 3 to 7 % less on the feed's scroll.

The RenderThread does 10 % less work than Compose's scrolling, and now for less CPU time too (4.20 against 4.60 s; round 10: 4.73 against 4.36 s). Its work fell 4 % from round 10 (804 → 769 ms) and its CPU time 11 %. In the animation, 8 % less work than Compose's (11 % in round 10, when Compose's did 583 ms; 532 ms this round), for 21 % less CPU time.

### Item reuse on the device (D430–D435), rounds 10 and 11

compose-rs's traces time each prefetch step (`compose-rs: prefetch`), and inside it the composition of an item (`prefetch compose`), its measurement (`prefetch measure`, text layout included) and the recording of its paragraphs. Compose's time its own (`compose:lazy:prefetch:compose`, `…:measure`). The table gives, per run, the main thread's work inside those slices from the stats' start (ms on the fastest core at its highest frequency), computed from the traces of rounds 9 to 11 (each round's JSON, `prefetch_slices`). Medians of 3 runs; nothing ranked:

| Feed scroll (10 flings), per run | compose-rs, round 9 | compose-rs, round 10 | compose-rs, round 11 | Compose, round 11 |
|---|---|---|---|---|
| Prefetch steps | 372 | 362 | 361 | |
| Item compositions; work each | 349; 0.096 ms | 347; 0.086 ms (−10 %) | 346; 0.087 ms | 238; 0.251 ms (round 10: 0.255 ms) |
| Composition work | 33.3 ms | 29.7 ms (−11 %) | 30.1 ms | 60.1 ms |
| Measurements; work each | 174; 0.308 ms | 173; 0.310 ms | 173; 0.326 ms | 179; 0.477 ms |
| Paragraphs laid out while prefetching | 1,044 | 911 (−13 %) | 1,038 | |
| Paragraphs recorded | 221 | 194 (−12 %) | 194 | |
| All prefetch work | 93.0 ms | 88.9 ms (−4 %) | 91.5 ms | |
| Main thread on the little cores, while prefetching | 82 to 85 % | 70 to 76 % | 70 to 77 % | |

| Stress list scroll, per run | compose-rs, round 9 | compose-rs, round 10 | compose-rs, round 11 | Compose, round 11 |
|---|---|---|---|---|
| Prefetch steps | 423 | 352 | 354 | |
| Item compositions; work each | 307; 0.163 ms | 347; 0.167 ms | 351; 0.132 ms | 384; 0.262 ms (round 10: 0.258 ms) |
| Composition work | 48.5 ms | 58.1 ms | 46.8 ms | 100.5 ms |
| Measurements; work each | 213; 0.318 ms | 211; 0.304 ms | 217; 0.245 ms | 178; 0.541 ms |
| Paragraphs laid out while prefetching | 1,283 | 670 (−48 %) | 629 | |
| Paragraphs recorded | 265 | 200 (−25 %) | 206 | |
| All prefetch work | 127.6 ms | 134.5 ms | 106.8 ms | |
| Main thread on the little cores, while prefetching | 85 to 87 % | 18 to 19 % | 37 to 57 % | |

In round 10, against round 9:

**On the feed**, composing a prefetched post takes 10 % less work (0.096 → 0.086 ms), and 0.58 against 0.75 ms of wall time. Headless, reuse took 16 % off an item scrolled into view (composition, measurement and drawing together). Fewer paragraphs are laid out (−13 %) and recorded (−12 %): a reused text node whose text and constraints didn't change, such as a tag or a like count that two posts share, most likely keeps its layout. All prefetch work went down 4 %.

**On the stress list**, the per-item numbers can't be read the same way: its main thread moved from the little cores to the mid and big ones (D440), where the work metric reads about a fifth higher for the same code (D440's A/B: 498 → 610 ms of main-thread work, for a third less CPU time). An item's composition did 0.167 ms of work against 0.163 ms, its measurement 0.304 against 0.318 ms, and the prefetch steps took half the wall time (1.14 against 2.18 ms each). Half as many paragraphs were laid out (1,283 → 670): the chips', buttons' and section headers' labels are the same from one item of a kind to the next. Separating reuse's own effect from D440's would take an interleaved A/B on the device.

Compose's prefetched items cost the same as in round 9 on both lists (0.255 and 0.258 ms of work each): its posts were already reused without content types.

**In round 11**, with only the items in view placed (D470), the feed's prefetching did as much as in round 10: as many compositions and measurements, each within a few percent of round 10's work. More paragraphs were laid out while prefetching (911 → 1,038, about round 9's 1,044), for as many recorded; that hasn't been broken down. On the stress list, an item's composition and measurement did a fifth less work (0.167 → 0.132 ms, 0.304 → 0.245 ms), but the main thread ran more of its time on the little cores again (37 to 57 %, against 18 to 19 %), where the work metric reads lower for the same code: our session voted less with less RenderThread work ([15](15-android-stress-benchmark.md)). Compose's prefetched items cost the same as in round 10 (0.251 and 0.262 ms each).

### Lazy lists place only the items in view (D470–D496): the A/Bs before round 11

Interleaved A/Bs, before round 11 measured the result beside Compose (above). The first: main (727d266) against D470–D474, 12 runs each ([15](15-android-stress-benchmark.md), "The lazy list's policy", has both lists and the variants weighed). A list now places only the items in view (it placed a viewport's worth more on each side), prefetches from its first layout on, and its first layout composes what it shows. On the feed's scroll: main thread p50 / p90 / p99 1.23 / 1.76 / 2.72 → 1.16 / 1.68 / 2.73 ms, RenderThread 2.33 / 3.40 / 4.79 → 2.24 / 3.26 / 4.86 ms (p99 within the runs' spread), RenderThread work 810 → 772 ms, no janky frame and no text laid out in a frame either way, native heap after the flings 11.8 → 10.6 MB. Its start: `TotalTime` 108.5 → 104.0 ms, the first frame's own part 6.7 → 5.1 ms; the big screen's unchanged (162 ms). A review then found regressions D470 had hidden (scrolls onto large items out of view cut short, short first frames under a large first item, a viewport's worth composed on focus) and that TalkBack couldn't scroll a list; D475–D478 fix them, and an 8-run A/B against main confirmed the feed's numbers: main thread 1.23 / 1.75 / 2.75 → 1.17 / 1.67 / 2.71 ms, RenderThread 2.31 / 3.38 / 5.08 → 2.35 / 3.23 / 4.72 ms, work −6 % and −7 %. A third review's seven fixes (D479–D485) kept them in another 8-run A/B (main thread 1.22 / 1.75 / 2.75 → 1.16 / 1.66 / 2.73 ms, RenderThread 2.20 / 3.34 / 4.97 → 2.32 / 3.21 / 4.66 ms, work −7 % and −8 %; [15](15-android-stress-benchmark.md)). After a fourth review's five fixes (D486–D491), again: main thread 1.24 / 1.75 / 2.74 → 1.17 / 1.67 / 2.64 ms, RenderThread 2.35 / 3.40 / 4.89 → 2.33 / 3.20 / 4.73 ms, work −6 % and −7 %. A fifth review's fixes (D492–D496) don't touch a steady scroll and had no device A/B. Round 11, beside Compose, has the feed's main thread at 1.16 / 1.68 / 2.74 ms, its RenderThread at 2.10 / 3.10 / 4.86 ms, its work −1 % and −4 % against round 10, and its native heap after the flings at 10.6 MB.

### Startup: launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 131 ms | **106 ms** (−19 %; round 10: 110 ms) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 45.6 ms | **23.3 ms** (23.3 ms) |
| `big`: 200 posts at once | 235 ms | **162 ms** (−31 %; 164 ms) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 173.3 ms | **89.3 ms** (89.7 ms) |

Compose's numbers are under `speed`. Compiled from its shipped profiles (`speed-profile`), its feed starts sooner: 120 against 132 ms in D362's session, where compose-rs's took 107 ms either way; `big` didn't change (228 ms). compose-rs's feed started 4 ms sooner than in round 10 (110 → 106 ms), within `TotalTime`'s noise (a standard deviation of 5 to 6 ms over the 15 starts; compose-rs's feed ranged over 100 to 116 ms, Compose's over 126 to 143 ms); D470's interleaved A/Bs measured −4.5 to +0.5 ms. The first layout now composes what it shows (D472): the feed's first frame's own part (`compose-rs: frame`) took 5.1 ms against 6.2 ms in round 10 (A/Bs: 6.7 → 5.0 ms), laying out 31 paragraphs, though the traversal around it took the same 23.3 ms in this single trace. `big` has no lazy list and didn't change.

In `big`'s first frame (89.3 ms), composing 200 posts takes 15.8 ms and laying them out 34.5 ms. Drawing the display list takes 0.9 ms, and replaying it on the `Canvas` 20.3 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 35.1 ms (`Record View#draw()`). On the feed's traced start, `bindApplication` is shorter too (13.7 against 16.6 ms; 13.2 against 20.4 ms in round 10, one traced start each): there is no Compose runtime to initialize.

### Memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 53.0 MB | **33.9 MB** (−36 %; round 10: 34.1 MB) |
| of which code | 7.4 MB | **2.4 MB** (2.2 MB) |
| of which Java heap | 3.6 MB | **1.3 MB** (1.3 MB) |
| of which native heap | 8.2 MB | 8.0 MB (8.4 MB) |
| of which graphics | 27.9 MB | **17.0 MB** (17.0 MB) |
| PSS after 10 flings | 87.0 MB | **79.4 MB** (80.6 MB) |
| of which Java heap | **3.7 MB** | 4.4 MB (4.7 MB) |
| of which native heap | 12.9 MB | **10.6 MB** (11.8 MB) |
| of which graphics | 56.6 MB | 56.7 MB (56.5 MB) |
| APK (release; decimal MB) | 1.92 MB (minified) | **1.86 MB**, with the 1.71 MB `.so` uncompressed inside (1.79 MB) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). Both apps' code grew with the stress screens (D400) and the `grid` screen (D473): compose-rs's to 2.4 MB, Compose's to 7.4 MB. After scrolling, both are dominated by graphics memory (the render thread's caches: about 56 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (87.0 MB this round, with 3.7 MB of Java heap, against 89.0 MB and 6.0 MB in round 10; up to 112 MB in earlier rounds: it allocates on every frame); compose-rs's stay within about a megabyte and a half (77.7 to 80.5 MB in this round's runs).

Placing only the items in view (D470) took the feed's native heap after 10 flings from 11.8 to 10.6 MB, as in D470's A/Bs (11.8 to 12.0 → 10.5 to 10.6 MB). On the stress list, D460 and D470 together took it from 22.3 to 17.2 MB ([15](15-android-stress-benchmark.md), "Memory").

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots). D410 does the same for paths, which the feed doesn't draw anew; on the stress animation it took compose-rs's memory after 13 s from 405 to 204 MB ([15](15-android-stress-benchmark.md)).

The APK is smaller than Compose's. Since D361, release builds take a release profile for Android (fat LTO, one codegen unit, `panic = "abort"`), stored uncompressed so the library can be mapped, against Compose's minified dex ([14](14-performance-opportunities.md), #4b). bench-rs's APK holds androidx.profileinstaller too since D360 (about 38 KB). Both grew in round 8 with the stress screens (D400: the library by 181 KB, Compose's APK by 147 KB); D390 added 12 KB to the library, D390's refinements, D410–D412 and D420–D424 9 KB more, item reuse (D430–D435) and the main thread in the session (D440) 20 KB more (1,632,832 bytes), and lazy grids item by item (D450–D459), the lists' policy and its fixes (D470–D496) 76 KB more (1,708,480 bytes). Compose's APK grew by 33 KB with the `grid` screen (D473).

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

Compose's numbers are round 11's.

| Gap | Steps |
|---|---|
| Scrolling, janky frames per run (Compose: 1, 0, 0) | 1, 2, 0 in round 1, all the app's; 1, 0, 1 in round 2; 0, 0, 0 in round 3; 1, 3, 1 in round 4, all the compositor's (a vsync resync, a display stall); 0, 0, 0 with the prefetch budget on the real vsync and the warm-up swipe left out (D340, D343, round 5); 2, 0, 0 in round 6, the compositor's; 0, 0, 0 in rounds 7, 8 and 9; 1, 0, 0 in round 10, the app's by FrameTimeline: the RenderThread waited 10.9 ms in a call to SurfaceFlinger; 0, 0, 0 in round 11 |
| Scrolling, main thread p99 (Compose: 7.19 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3; 3.11 ms with prefetched paragraphs recorded while idle (D322, round 4; p90 2.20 → 1.90 ms); 3.04 ms in round 5; 3.05 ms with the items' groups recorded while idle (D350, round 6; p90 1.92 → 1.84 ms); 2.86 ms in round 7 (within round 6's runs' spread), and in round 8; 2.76 ms in round 9; 2.85 ms in round 10, within the runs' spread, with the main thread in the hint session (D440; p90 1.90 → 1.83 ms); 2.74 ms in round 11, within the spread, with only the items in view placed (D470; p90 1.83 → 1.68 ms) |
| Animation, main thread per frame p50 (Compose: 1.59 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3); 1.20 ms with a replay without interpreted calls and records naming nested groups (D321, D323, round 4); 1.21 ms in round 5; 1.18 ms in round 6; 1.21 ms in round 7; 1.33 ms in round 8, whose two runs presented a vsync late were slower (D390's A/B: 1.20 → 1.22 ms); 1.26 ms in round 9; 1.19 ms with the main thread in the hint session (D440, round 10); 1.16 ms in round 11 |
| Animation, RenderThread work (Compose: 532 ms) | 505 ms in round 1; 535 ms in round 2 and 540 ms in round 3, even with Compose's (a square-capped spinner, a stroked path each frame); 517 ms with the round cap (D320, round 4); 501 ms in round 5; 502 ms in round 6; 505 ms in round 7; 556 ms in round 8, boosted (D390's A/B: unchanged, 450 → 444 ms over 9 s); 499 ms in round 9; 516 ms in round 10; 492 ms in round 11 |
| Scrolling, RenderThread work (Compose: 853 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3; 786 ms in round 4; 813 ms in round 5; 794 ms in round 6; 784 ms in round 7; 821 ms in round 8; 810 ms in round 9; 804 ms in round 10; 769 ms with only the items in view placed (D470, round 11) |
| Scrolling, RenderThread p50 (Compose: 2.34 ms) | 2.71 ms in round 5, 2.80 ms in round 6, 2.70 ms in round 7: not compose-rs's work, but how rarely Android boosted it (at the same boost the durations match); 2.31 ms in round 8 with a hint session of its own (D390; Compose's p50 moves with how often its session boosts it: 1.85 to 2.33 ms in round 8's runs); 2.31 ms in round 9, with fewer of its frames boosted to 200 or more (Compose's: 2.04 to 2.17 ms in its runs); 2.35 ms in round 10, boosted as in round 9 (Compose's: 2.01 to 2.16 ms); 2.10 ms in round 11, boosted as often as Compose's and shorter at each boost, with less work (D470; Compose's: 2.08 to 2.35 ms in its runs). Closed in this session |
| Startup, `big` (Compose: 235 ms) | 186 ms in round 1; 178 ms in round 2; 179 ms in round 3; 174 ms with composition linear in its reads (D310, round 4); 163 ms with `BoringLayout` (D330, round 5), and in round 6; 160 ms in round 7; 166 ms in round 8; 163 ms in round 9; 164 ms in round 10; 162 ms in round 11 |
| PSS after 10 flings (Compose: 87.0 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3; 81.2 MB in round 4; 79.5 MB in round 5; 79.7 MB in round 6; 80.0 MB in round 7; 80.2 MB in round 8, and in round 9; 80.6 MB in round 10, with items kept for reuse (D430); 79.4 MB in round 11, with only the items in view placed (D470: native heap 11.8 → 10.6 MB) |
| APK (Compose: 1.92 MB) | 2.4 MiB in round 2; 3.24 MB (3.1 MiB) in round 3, unstripped; 1.99 MB stripped (D301, round 4), and in round 5; 1.55 MB with the release profile (D361, round 6), profileinstaller included; 1.57 MB in round 7; 1.76 MB in round 8, with the stress screens (Compose's grew by 0.15 MB); 1.77 MB in round 9; 1.79 MB in round 10; 1.86 MB in round 11, with lazy grids item by item and the lists' policy (Compose's grew by 0.03 MB with the `grid` screen). Closed |

In round 3, D270 (the display list as a tree) was also measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` unchanged); scrolling, `frame` 0.273 → 0.237 ms. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs and the todo toggle from 263 to 194 µs. The changes of rounds 4 to 8 were measured the same way in [14](14-performance-opportunities.md), round 9's (D410–D412, D420–D424) on the stress screens in [15](15-android-stress-benchmark.md), and round 10's: item reuse (D430–D433) headless in [14](14-performance-opportunities.md) (#19), the main thread in the hint session (D440) interleaved on the device in [14](14-performance-opportunities.md), "The main thread in the session", and round 11's: the lists' policy (D470–D496) interleaved on the device in [15](15-android-stress-benchmark.md), "The lazy list's policy".

## What made the difference

- **Main-thread frames**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270), and a group's record names its nested groups by key, so a nested group's change leaves its parent's recording alone (D323); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose; the replay calls no interpreted framework method (D321). Lazy lists compose and premeasure their next item when the main thread is idle (D156), record its paragraphs (D322) and draw and record its groups then (D350), which takes composition, text layout and recording out of the scrolled frames; they keep the two items behind the scroll composed (D375) and recorded (D380), so a fling back's first frames do no more. An item coming into a list takes the composition and nodes of one of the same content type that left (D430–D435), as in Compose: a prefetched feed post's composition does 10 % less work. A list places only the items in view, as Compose's (D470), so a frame replays fewer render nodes and its first frame composes only what it shows (D472).
- **Jank**: the idle-time budget is counted from the real vsync, so prefetching goes on through Choreographer's buffer stuffing recovery (D340), and an idle-time step starts only if its recent maximum fits with a margin, so it doesn't delay the next frame (D341), scaled by how much slower than expected the idle time's steps so far ran (D381).
- **RenderThread frames**: kept groups are drawn by reference; small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190); the spinner's round cap is drawn as a circle (D320). Without long main-thread frames, HWUI's hint session rarely gets the RenderThread boosted, which lengthened the scroll's frames but not their work; a session of compose-rs's own, reporting the RenderThread's CPU time against 6.5 ms, now gets it boosted far more often (half its scroll frames, against 4 %; Compose's: a half to two thirds), for about 2 % more CPU power (D390). Placing only the items in view (D470) gives it less to sync and draw: 4 % less work on the feed, 18 % less on the stress list, since round 10.
- **Where the main thread runs**: since D440, compose-rs's session holds the main thread too, which HWUI's session stopped boosting once the RenderThread's part of each frame got shorter. On the stress list, the main thread ran 21 % of its time on the little cores in round 10 (83 % in round 9), for a p50 of 1.02 against Compose's 1.29 ms; in round 11, with less work to report after D470 and so a lower vote, 46 %, for a p50 of 1.08 against Compose's 1.25 ms.
- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; composition is linear in its reads (D310), and single lines are laid out with `BoringLayout` (D330). The big screen's first frame takes 52 % of Compose's.
- **Memory and size**: the app's code is a 1.7 MB native library rather than Compose's dex and runtime (5.0 MB less code mapped, 2.3 MB less Java heap), built with LTO (D361); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195).

## Next steps

[14](14-performance-opportunities.md) measures what's left. For frame durations:

- **The scroll's slowest main-thread frames**: since D375 and D380, a fling back's first frames compose, lay out and record nothing either. No frame of compose-rs's was janky in rounds 7 to 9 and 11; round 10's one janky frame was the RenderThread waiting 10.9 ms in a call to SurfaceFlinger, not compose-rs's work. What sets the p99 now (2.74 ms) hasn't been broken down. On the stress list, each round-9 run had 1 to 3 frames of about 10 ms that laid a newly revealed item out inside the frame; rounds 10 and 11 had none (their slowest main-thread frames took 5.1 and 4.4 ms), and in round 11 no frame laid text out on either list ([15](15-android-stress-benchmark.md)).
- **Text layout while prefetching** is still about half of a prefetched item's work, after `BoringLayout` (D330), and most of the big screen's 35 ms of layout. Laying out on worker threads (#11) took the scroll's main p99 from 3.53 to 3.08 ms and the big screen's first-frame work down 16 % in a prototype that isn't production-ready (measured before D330); batching JNI calls alone is capped at about 2.6 ms of the big frame.
- **Late `doFrame`s**: 7 to 12 a scroll run start more than 2 ms after their vsync (round 11; 3 to 12 in round 10, 12 to 21 in round 9), woken late (the `Choreographer#onVsync` message itself comes 2 to 4 ms after the vsync: the scheduler's doing). None waited for compose-rs's idle-time work on the feed in rounds 10 and 11, nor on the stress list in round 10; round 11 had 2 there in 3 runs, its main thread back on the little cores 46 % of its time (round 9: 4 in 3 runs on the feed, 8 to 16 a run on the stress list, whose main thread then ran on the little cores). Those that waited were a prefetch measuring an item on the little cores; D381 halved the idle-time slices running past the vsync, and D440 took the stress list's main thread off the little cores. If they come back, splitting an item's measurement at its paragraphs (so a step can stop at the deadline) would be the next step, at the cost of an interruptible measure.
- **The compositor's jank**: none scrolling in rounds 7 to 9 (round 6: 2, SurfaceFlinger's, in one run); 3 in one animation run of round 9 (two prediction errors, a SurfaceFlinger scheduling miss). In D440's A/B, 4 of the 48 runs with the main thread in the session had 1 to 3 (prediction errors, SurfaceFlinger scheduling, the display HAL), against none of main's 48, and 6 of 48 for the most boosted variant: too few to tell from noise. Round 10, with D440, had none in any measured run of either app (12 runs each, on the feed and the stress screens); the only ones in compose-rs's traces were prediction errors during the warm-up swipe, which the stats leave out by design (D343); one of Compose's traces had three there too (SurfaceFlinger's and the display HAL's). Round 11 had none in compose-rs's measured runs either; Compose's animation had 2 prediction errors in one run. Still watched.
- **The scroll's RenderThread durations** follow ADPF's boost. Since D390, compose-rs's own session boosts the RenderThread; its target is a trade of power for durations, swept in [14](14-performance-opportunities.md) (since D500, both threads' work against 8.33 ms). How often each app ends up boosted varies from session to session (compose-rs's frames unboosted 51 to 62 %, Compose's 34 to 52 %, in the six sessions measured). On the stress list in round 11, less work meant a lower vote, and its RenderThread's p90 and p99 came out longer than Compose's (3.81 / 5.45 against 3.63 / 5.27 ms) though at each boost its frames are shorter ([15](15-android-stress-benchmark.md)): the one place compose-rs was behind Compose's frame durations. D500 bought it back (2.89 / 3.93 ms in its A/B) for +2.9 % (± 2.7) CPU-rail power on that list; the next round will confirm it beside Compose.
- **Item reuse on the stress list**: on the device, its per-item effect couldn't be separated from D440's, which moved the main thread to faster cores in the same round ("Item reuse on the device"); an interleaved A/B would. Round 10's 1.0 to 1.5 MB more native heap on the stress list was the first layout's extra items kept for reuse, now disposed (D460); round 11's native heap there is 14.7 MB after start and 17.2 MB after scrolling.
- **The animation's p99** frames are the system's (vsync requests, the RenderThread's sync), which a `View` pays as Compose does; only drawing without HWUI (option D of [13](13-android-drawing-paths.md), our own Skia) would remove part of them.
- **Smaller items**: caching the display's refresh rate in `prefetch()` (1.9 ms per scroll run); rounded clips through a `Path` (about 15 % of the animation's replay); the kept groups' bookkeeping (D380), a few microseconds in every scrolled frame (the replay's median 77 → 82 µs in its A/B), which listing only changes would cut.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
