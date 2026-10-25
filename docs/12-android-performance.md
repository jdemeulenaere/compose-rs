# 12. Android performance: compose-rs and Jetpack Compose

The same app, written twice with equivalent widgets, measured on the same device: how compose-rs on Android ([07](07-platforms.md), "Android") compares with Jetpack Compose in jank, per-frame durations and work, startup and memory.

## The app

A social feed (`examples/feed` for compose-rs, `android/bench-compose` for Kotlin Compose), widget for widget:

- **`feed`**: a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 posts, keyed. A post is a `Card` holding a 40 dp circular avatar with initials, the author and the time, two sentences of text (3 lines at most, ellipsized), a like `IconButton` with its count, and a tag.
- **`feed_animated`**: the same, with a header card whose indeterminate `CircularProgressIndicator`, pulsing dot (alpha) and turning star (rotation) animate forever.
- **`big`**: 200 posts in a `Column` with `verticalScroll`, all composed at once: the cost of a big screen.

The Kotlin app follows Compose's performance guidance: an `@Immutable` data class, keys on lazy items, animated values read only inside `graphicsLayer { }` (deferred reads, so an animation frame neither recomposes nor relayouts), and a release build minified with R8. The compose-rs app reads its animated values while drawing too (`graphics_layer(|layer| …)`), and is a plain `Activity` holding a `ComposeRsView` (no AndroidX but `androidx.profileinstaller`, as the Kotlin app, since D360). Both apps are compiled ahead of time (`cmd package compile -m speed`) before measuring; every round so far used `speed` for both apps (recorded since round 4). That isn't Compose's best case for startup: compiled from its shipped profiles (`-m speed-profile`, as a Play install compiles it, with an app image), Compose's feed starts in 120 ms against 132 ms with `speed` (in D362's session), so compose-rs (107 ms either way) leads by 13 ms rather than 25 (28 at parity in round 14, under `speed`). `measure.py --compile speed-profile` measures that way, with each app's shipped Baseline Profile (D362): Compose's libraries' and the app's own, and the compose-rs library's (D360); see "Compiled from the shipped Baseline Profiles" below.

Both apps draw the same pixels: a fidelity audit (D570) compared them on the device, bounds and pixels, and brought compose-rs's Material 3 to 1.4.0's tokens (its type scale's letter spacing, its centered line height, the outlined button's, chips' and progress track's colors, D570 to D573). Since, the feed, big, stress and grid screens are pixel-identical at their start; before, every compose-rs text was a few pixels narrower and its glyphs 1 to 3 px lower, which moved line breaks (the stress screen's first rich post fit "UI" on its first line).

The Kotlin app is built with the latest stable Compose (D540): BOM 2026.09.00 (ui, foundation and runtime 1.12.1, material3 1.4.0), `activity-compose` 1.13.0, Kotlin 2.4.21 with its Compose compiler plugin and AGP 9.4.1, compiled against API 37 (Compose 1.12 requires it) with `targetSdk` 36. Rounds 1 to 13 used BOM 2026.06.01 (1.11.4, the same material3) and `activity-compose` 1.12.4. The defaults that matter for the comparison (prefetch and its time budget, compositing, shadows and clips, paths) are read from that version's sources in [15](15-android-stress-benchmark.md), "Compose's defaults".

## Method

`android/bench/measure.py` drives a connected device over adb, interleaving the two apps run by run, and writes a JSON report (`android/bench/results/`); `summarize.py` prints it. Since D302, it first checks that the installed apps' native libraries for the device's ABI are those built in the checkout (and stops if not), and records each APK's size and SHA-1, the compilation filter (`--compile`, `speed` by default) and each app's compilation state (`dumpsys package dexopt`) at the start and the end of the run, and stops unless each app starts compiled as asked. With `--compile speed-profile`, each app is reset, given the Baseline Profile its APK ships through androidx.profileinstaller's receiver, and compiled from it (D362). Both apps run androidx.profileinstaller's startup initializer on every cold start, in every mode. A traced run whose trace holds no frame of the app's is run again (D362). Its parsing is tested on captured output: `python3 android/bench/test_measure.py`.

To run it, install both apps' release builds, then measure; `measure.py` needs a [trace_processor_shell](https://perfetto.dev/docs/analysis/trace-processor) binary:

```bash
android/gradlew -p android :bench-compose:installRelease :bench-rs:installRelease
python3 android/bench/measure.py --trace-processor path/to/trace_processor_shell --out bench-out
python3 android/bench/summarize.py bench-out/report.json
```

`--optimizations parity|all|each|<names>` picks compose-rs's flags (parity, the default, turns them all off), `--scenarios` picks the scenarios (the stress screens: `stress_startup,stress_scroll,stress_animation`, [15](15-android-stress-benchmark.md)), and `--thermal-wait-limit 0 --rotate` waits for a cool device before each run, as the published rounds do.

- **Startup**: 15 cold starts of each screen per app (`am force-stop`, then `am start -W`: `TotalTime`, from the launch to the first frame drawn). One cold start of each is also traced with Perfetto, to break the first frame down.
- **Memory**: `dumpsys meminfo` 5 s after start, and again after scrolling (3 runs). Calling it runs the app's garbage collector, so the first measurement resets what the second counts.
- **Scrolling**: 10 flings through the feed (`input swipe`, 120 ms each, 1.2 s apart), 3 runs per app. Frames and jank come from `dumpsys gfxinfo` and from Perfetto's FrameTimeline (SurfaceFlinger's verdict on each frame). Per-frame durations come from the app's `Choreographer#doFrame` (main thread) and `DrawFrame` (RenderThread) slices, and CPU time from the scheduler. Since D343, a warm-up swipe comes first and is left out (the stats start at the first input after it; gfxinfo is reset after it): the display, idle since the app started, resyncs its vsync on the first input, which FrameTimeline counted as jank no app caused. Each run also reports its janky frames by side (the app's deadline, or the compositor's: SurfaceFlinger's deadlines, prediction errors, the display), the frames presented at another vsync than expected (`present_type`, buffer stuffing's late frames included), the `doFrame`s starting more than 2 ms after their vsync (about 1.1 ms normally), and of those the ones that work on the main thread delayed (a slice ending more than 1.2 ms after the vsync: idle-time work, input; the rest woke late), compose-rs's `prefetch` calls, and whether Choreographer entered buffer stuffing recovery (its async track, or a `buffer stuffed` slice): `summarize.py` prints a line per run.
- **The RenderThread's boost**: since round 6 (D370), each traced run also reports how much Android's performance hints boosted the RenderThread. That is the HWUI hint session's boost minimum: the `uclamp.min` that ADPF sets for HWUI's threads, out of 1024, which the power HAL traces as `adpf.<pid>-…-HWUI-…-min`. The report gives its time-weighted mean over the run's frames, the share of frames started under 100 (unboosted), and the `DrawFrame` percentiles in each boost bucket (under 100, 100–199, 200 and more). It also gives the share of the RenderThread's running time on the little cores (those of the lowest capacity, from the device's `cpu_capacity`). A trace without the counter reports the little cores' share only. Since D390, compose-rs's own session boosts the RenderThread too, and the power HAL applies the higher of the two sessions' votes: `render_boost` still reads HWUI's counter alone, so the pictures below (rounds 8 to 14) come from the same traces with both sessions' votes (the round's JSON, `d390_boost`). At parity compose-rs's session is off, and both apps have HWUI's alone.
- **Isolation**: before each run, measure.py force-stops our other apps (`rs.compose.*` but the one measured): the other benchmark app, left cached by the run before, and the demo, whose animation ran a frame on every vsync in the background before D342.
- **Animation**: 10 s of the animated header, untouched, 3 runs per app, measured as scrolling.
- **compose-rs's optimizations** (D569): `--optimizations parity` (the default) runs bench-rs with none of them (`-e optimizations none`, its hint session off: Compose has none of its own), `all` with every one, `each` bench-rs alone at parity and with each one alone, interleaved run by run ([14](14-performance-opportunities.md), "Backport candidates"). The report records each subject's list and bits.
- **Thermal guard** (D569, D574, D579): before each run, startup round and traced first frame, our apps are stopped and measure.py waits while the thermal status is above 0, the battery above `--battery-limit` or the skin (the highest `TYPE_SKIN` sensor, Pixel's VIRTUAL-SKIN) above `--skin-limit`, however long with `--thermal-wait-limit 0`; each run records its state before and after. Round 14 ran with 34 °C and 35 °C.
- **Stress** (D400, D401): a much heavier list (`stress_startup`, `stress_scroll`) and a screen of hundreds of concurrent animations (`stress_animation`), measured as above; [15](15-android-stress-benchmark.md) describes them and their results. `--scenarios` runs some scenarios only (all by default), and every traced run lists its slowest frames with their longest slices (`slowest_frames`).

Durations and CPU time alone mislead, because they depend on where a thread ran as much as on what it did. The scheduler moves light threads to the little cores, where the same work takes longer, and Android's performance hints (ADPF) boost the threads, onto faster cores at higher frequencies, after long frames. When prefetching (D156) made compose-rs's frames shorter, its threads moved to the little cores: its frames took longer and its CPU time rose while the work they did fell. So "work" scales each running slice by its core's frequency (traced) and capacity: milliseconds on the fastest core at its highest frequency. It's the measure of cost the comparisons below rely on; durations are what a user sees.

Measurements drift between sessions: Compose's own numbers move by up to 5 to 9 % from one session to the next (its scrolling main-thread work was 827 ms in round 3, 760 ms in round 4, 844 ms in round 5, 843 ms in round 6, 839 ms in round 7, 917 ms in round 8, 842 ms in round 9, 847 ms in round 10, 815 ms in round 11, 850 ms in round 12, 854 ms in round 13, 838 and 901 ms in round 14's parity and all sessions, an hour apart). So the two apps are compared within a session, and changes to compose-rs are measured on their own with interleaved A/B runs ([14](14-performance-opportunities.md)); a difference between rounds smaller than that is drift.

compose-rs's frame phases show in the traces as `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay` sections, beside the system's.

## Results

Pixel 8 Pro, Android 16, October 2026. Medians of 3 runs (startup: of 15). The better side is in bold; ties aren't.

**Round 14** is compose-rs at 3ac74a6 against Compose 1.12.1, measured three ways in one session (2026-10-25): every scenario in the first two, the scrolls and animations in the third:

- **Parity** (`results/2026-10-25-pixel-8-pro-parity.json`, `--optimizations parity`): compose-rs does the work Compose does by default (D560–D569): Compose's prefetch rule and time budget, Compose's layers, shadows and clips, the mutable `Path`, Compose's text relayout, and none of its own optimizations, its hint session included (bench-rs's `-e optimizations none`, D568). The two apps' static screens are pixel-identical (D570–D573). These are the headline numbers.
- **All** (`…-all.json`, `--optimizations all`): every one of compose-rs's optimizations on ([07](07-platforms.md), "compose-rs's own optimizations"), against Compose in that session: compose-rs's best case.
- **Each** (`…-each.json`, `--optimizations each`): bench-rs alone, at parity and with each optimization alone, 5 runs a subject: [14](14-performance-opportunities.md), "Backport candidates".

measure.py's checks passed in each: the installed native libraries were those built at 3ac74a6, both apps were `speed` at the start and the end, the compose-rs traces hold its prefetch slices (`compose-rs: prefetch`, 353 to 389 a feed run), and FrameTimeline's janky frames are split by side. Every run, startup round and traced first frame started at thermal status 0, battery ≤ 34 °C and skin ≤ 35 °C (`--battery-limit 34 --skin-limit 35 --thermal-wait-limit 0`, our apps stopped while waiting, D574): the parity round's runs started at battery 27.5 to 31.8 °C and skin 29.2 to 34.9 °C, the all round's at 31.1 to 33.1 °C and 31.6 to 34.9 °C; the guard waited 10 to 30 s before 4 stress-animation runs and one startup round, and no run was throttled or set apart (gfxinfo's medians 5 ms on the feed's screens and 14 to 15 ms in the stress animation, as in round 13). Six traced runs were run again once, their first trace holding no frame of the app's (D362). The stress screens' results are in [15](15-android-stress-benchmark.md).

### At parity: where compose-rs is ahead, level and behind

- **Jank: ahead.** No frame of compose-rs's missed the app's deadline in any of the 12 parity runs (feed and stress list, scrolling and animating); Compose missed 1 on the feed's scroll and 8 on the stress screens (6 scrolling, 2 animating). The stress animation runs near the deadline for both apps: in the each round, 3 of parity's 5 runs there had one janky frame (Compose wasn't in that round); its 15 scroll runs had none. Neither app had a compositor-side janky frame. compose-rs's `doFrame`s started more than 2 ms late 7 to 14 times a feed scroll run, Compose's 23 to 28.
- **The main thread: ahead**, at every percentile but one: on the feed's scroll p50 / p90 / p99 1.28 / 2.25 / 3.47 against 1.51 / 3.26 / 7.05 ms, its p99 half Compose's; animating 1.23 / 1.74 / 3.02 against 1.55 / 2.19 / 3.13 ms. The exception is the stress list's p50 (1.39 against 1.29 ms; [15](15-android-stress-benchmark.md)), its main thread unboosted on the little cores 76 % of its time against Compose's 14 %.
- **The RenderThread: behind on the scrolls, ahead animating the header, level in the stress animation.** Scrolling the feed, 2.50 / 3.51 / 5.27 against 2.11 / 3.40 / 4.91 ms (p50 +18 %), the stress list +19 to +24 % ([15](15-android-stress-benchmark.md)); animating the header 2.58 / 3.27 / 4.67 against 3.01 / 4.10 / 5.65 ms; in the stress animation 7.67 / 8.77 / 10.14 against 7.48 / 8.94 / 10.77 ms. On the scrolls it isn't its work: by the work metric compose-rs's RenderThread does 17 % less than Compose's on the feed's scroll and 32 % less on the stress list, but it runs unboosted. At parity HWUI's session is the only one, for both apps; it boosts after frames that run long against their target from the main thread's start, and compose-rs's main-thread frames are short, so its frames earn little boost: a time-weighted minimum of 38 against Compose's 129 on the feed (84 % of frames unboosted, against 48 %), 67 against 277 on the stress list. At the same boost its frames are as short or shorter ("The RenderThread's boost" below), and with its own session (`performance_hint`, D390/D500; the `all` round) it is ahead at every percentile. Its RenderThread spends more CPU time than Compose's for less work (4.65 against 4.40 s on the feed): the same frames at lower clocks.
- **Startup and memory: ahead.** The feed starts in 107 against 135 ms, `big` in 165 against 229 ms; 33.2 against 52.7 MB of PSS after start, 78.9 against 87.6 MB after 10 flings.
- **Work: ahead**, read with care: the metric reads lower at low clocks, and compose-rs ran less boosted. Main thread −47 % scrolling the feed, −22 % animating; RenderThread −17 % and −12 %.
- **The stress animation's graphics memory and HWUI's workers: behind** ([15](15-android-stress-benchmark.md)): after 13 s, 171.5 against 140.6 MB of graphics (compose-rs's runs 164 to 203 MB, Compose's 138 to 142 MB), its HWUI workers 10.1 against 8.5 s of CPU time (D511).
- **Pausable composition: a bias against compose-rs in round 14, gone since** (D590–D599). In this round compose-rs had none: a prefetch request composed its item in one step, gated on a whole composition's average, where Compose gates one stretch between pause checks and splits the rest over frames. Here the idle time had room: the feed's scrolls composed no item inside a frame (one or no second round of composition and layout a run, laying no text out), and no `doFrame` of compose-rs's was janky. What did run past the vsync were mostly measurements, which Compose doesn't split either: 9 prefetch steps in the 6 scroll runs (3 on the feed, 6 on the stress list) ended 1.3 to 3.6 ms after their vsync and delayed a `doFrame` by 2.1 to 3.8 ms, steps of 3.5 to 8.0 ms, most of each an item's measurement, on a little core (CPU 0), unboosted; the frames were on time (margins 5.7 ms and more). Compose's main thread delayed 17 `doFrame`s of its feed scroll runs and 25 of its stress list runs with its own work. Pausable composition came after this round, and changed nothing beyond the runs' spread ([15](15-android-stress-benchmark.md), "Pausable composition (D598)").

### At parity: scrolling the feed (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,796 | 1,792 |
| Janky frames (FrameTimeline), per run | 0, 1, 0 | **0, 0, 0** |
| of which the app's | 0, 1, 0 | **0, 0, 0** |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 |
| Frames presented at another vsync than expected, per run | 0, 1, 0 | 0, 0, 0 |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 28 (8), 23 (6), 26 (3) | **11 (1), 14 (1), 7 (1)** |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / **6** / **8** ms |
| Main thread per frame p50 / p90 / p99 | 1.51 / 3.26 / 7.05 ms | **1.28** / **2.25** / **3.47** ms |
| RenderThread per frame p50 / p90 / p99 | **2.11** / **3.40** / **4.91** ms | 2.50 / 3.51 / 5.27 ms |

Both scroll at 120 Hz. **Compose's one janky frame** started 11.0 ms after its vsync, behind its own work (a `doFrame` resync running `Recomposer:animation` for 4.4 ms), and finished 6.9 ms after its deadline. Its slowest main-thread frames took 10.3 to 10.6 ms a run. compose-rs's slowest took 4.9 to 6.0 ms, most of it the view system's `traversal`: waiting for the RenderThread's sync (`postAndWait`, up to 3.9 ms) or recording the items a frame reveals (`Record View#draw()`, up to 2.7 ms: at parity nothing records ahead, D564).

**Late `doFrame`s**: compose-rs's 7 to 14 a run started more than 2 ms after their vsync, at most 3.6 ms; none was janky or presented late, the closest finishing 6.3 ms before its deadline (the round's JSON, `late_do_frames`). One a run followed its own work: a prefetch step measuring an item on a little core (5.6 to 6.9 ms) that ended 1.3 to 2.5 ms past the vsync. The others were woken late, the main thread asleep until the vsync's message came. Compose's 23 to 28 a run, up to 11.0 ms late, 3 to 8 a run after its own work.

### At parity: the animated header (10 s)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,329 | 1,340 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 |
| Frames presented at another vsync than expected, per run | 0, 0, 1,201 | 0, 0, 1,203 |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 14 (0), 9 (1), 6 (0) | **6 (0), 4 (0), 17 (0)** |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / **5** / **7** ms |
| Main thread per frame p50 / p90 / p99 | 1.55 / 2.19 / 3.13 ms | **1.23** / **1.74** / **3.02** ms |
| RenderThread per frame p50 / p90 / p99 | 3.01 / 4.10 / 5.65 ms | **2.58** / **3.27** / **4.67** ms |

No frame of either app missed its deadline; one run of each was presented a vsync late throughout, on cadence (Compose's in Choreographer's buffer stuffing recovery; compose-rs's showed none in its trace), as in round 13. **compose-rs is ahead on both threads**: the main thread's p50 21 % shorter, the RenderThread's 14 % at p50 and 20 % at p90. Here neither app is boosted (96 % and 98 % of RenderThread frames unboosted), so the comparison is at the same clocks: what compose-rs keeps by default (each repaint boundary's recording kept and drawn again only when it changed, D155; layer properties set on render nodes, D157; the replay, D321; nested groups by key, D323) is Compose's own design, done natively.

### At parity: the RenderThread's boost

ADPF boosts a thread after the frames a hint session holding it reports run long against their target. At parity, both apps have only HWUI's session, which holds the main thread and the RenderThread and reports each frame from the main thread's start to the RenderThread's end against 11.62 ms; with `performance_hint`, compose-rs's own session holds both threads too and reports their CPU time against 8.33 ms (D390, D440, D500), and the power HAL applies the higher of both sessions' votes. The rows take each frame's boost as the higher of the active votes when it started, from the traces (the rounds' JSONs, `d390_boost`). Medians of the 3 runs (each bucket's percentiles: of the runs with frames in it); nothing ranked:

| Feed scroll | Kotlin Compose | compose-rs, parity | compose-rs, all (Compose in that session) |
|---|---|---|---|
| Boost minimum, time-weighted (out of 1024; runs) | 129 (120 to 198) | 38 (26 to 43) | 302, of which HWUI's session's alone 5 (146) |
| Frames unboosted / at 100–199 / at 200 or more | 48 % / 26 % / 26 % | 84 % / 12 % / 0 % | 1 % / 19 % / 80 % (35 % / 30 % / 31 %) |
| RenderThread running time on the little cores | 24 % | 28 % | 1 % (19 %) |
| Main thread running time on the little cores | 58 % | 81 % | 17 % (52 %) |
| Unboosted frames: p50 / p90 / p99 | 2.79 / 3.98 / 5.39 ms | 2.57 / 3.55 / 5.28 ms | 2.38 / 3.94 / 4.91 ms (2.77 / 3.99 / 5.78 ms) |
| Frames at 100–199: p50 / p90 / p99 | 2.05 / 2.74 / 3.78 ms | 1.93 / 2.61 / 3.85 ms | 1.96 / 2.55 / 3.48 ms (2.07 / 2.73 / 4.34 ms) |
| Frames at 200 or more: p50 / p90 / p99 | 1.57 / 1.99 / 2.60 ms | 1.66 / 1.87 / 2.08 ms | 1.39 / 1.79 / 2.52 ms (1.57 / 2.11 / 2.83 ms) |

**At parity compose-rs's RenderThread is unboosted on 84 % of its frames, Compose's on 48 %**: HWUI's session sees compose-rs's short main-thread frames and votes 38 where it votes 129 for Compose. At each boost level compose-rs's frames are as short or shorter (unboosted p50 2.57 against 2.79 ms; at 100–199 1.93 against 2.05 ms; at 200 or more 1.66 against 1.57 ms at p50 and 2.08 against 2.60 ms at p99), so its RenderThread's longer median is where it ran, not what it did. The same holds on the stress list (67 against 277; [15](15-android-stress-benchmark.md)). Animating, neither is boosted (23 against 8). With its own session (`all`), compose-rs's frames are boosted on 99 % of the frames, Compose's on 65 %, and its RenderThread is ahead at every percentile (next sections). The session's cost is power, which these rounds don't measure: D500's A/B measured +1.8 % (± 2.5) on the CPU rails on this scroll and +2.9 % (± 2.7) on the stress list's; "The session's target" in [14](14-performance-opportunities.md) has the trade.

### At parity: work per frame

| Scrolling the feed (10 flings) | Kotlin Compose | compose-rs, parity | compose-rs, all (Compose in that session) |
|---|---|---|---|
| Main thread work | 838 ms | **445 ms** (−47 %) | 676 ms (−25 %; 901 ms) |
| RenderThread work | 874 ms | **723 ms** (−17 %) | 960 ms (+5 %; 918 ms) |
| Main thread CPU time | 4.51 s | **3.39 s** (−25 %) | 2.35 s (−47 %; 4.43 s) |
| RenderThread CPU time | **4.40 s** | 4.65 s (+6 %) | 2.97 s (−30 %; 4.22 s) |

| The animated header (10 s) | Kotlin Compose | compose-rs, parity | compose-rs, all (Compose in that session) |
|---|---|---|---|
| Main thread work | 256 ms | **201 ms** (−22 %) | 224 ms (−12 %; 253 ms) |
| RenderThread work | 534 ms | **470 ms** (−12 %) | 501 ms (−5 %; 529 ms) |
| Main thread CPU time | 2.08 s | **1.65 s** (−21 %) | 1.67 s (−20 %; 2.09 s) |
| RenderThread CPU time | 3.81 s | **3.27 s** (−14 %) | 3.12 s (−18 %; 3.81 s) |

**Work** is CPU time scaled by each core's frequency and capacity ("Method"). It reads higher for the same code at high frequencies (the metric assumes work scales with the clock; code waiting for memory doesn't), so a less boosted app reads lower: at parity compose-rs's threads are the less boosted ones on the scrolls, at `all` the more boosted ones. The animation, where neither app is boosted at parity, is the cleanest comparison: compose-rs's main thread does 22 % less work, its RenderThread 12 % less. With every optimization on, compose-rs's scroll does more work than at parity by the metric (main 445 → 676 ms, RenderThread 723 → 960 ms) for less CPU time (3.39 → 2.35 s, 4.65 → 2.97 s): the same frames boosted, plus what `draw_prefetched_ahead` records ahead ([14](14-performance-opportunities.md), "Backport candidates", has each flag alone).

### At parity: prefetch, against Compose's

Both lists prefetch by Compose's rule at parity (one item ahead, on scrolls only, Compose's per-content-type time budget: D560, D561). The main thread's work inside the prefetch slices from the stats' start (ms on the fastest core at its highest frequency), per run, medians of 3 (the rounds' JSONs, `prefetch_slices`, `lazy_lists`); compose-rs's own sections (`compose-rs: prefetch`, `prefetch compose`, `prefetch measure`) and Compose's (`compose:lazy:prefetch:compose`, `…:apply`, `…:measure`, `…:nested`). Compose's compositions are pausable (`compose` slices are each a stretch; an apply follows), so their counts aren't comparable one for one. Nothing ranked:

| Feed scroll, per run | compose-rs, parity | compose-rs, all | Compose (parity session) |
|---|---|---|---|
| Prefetch steps | 389 | 357 | |
| Item compositions; work each | 353; 0.080 ms | 355; 0.114 ms | 225 slices; 0.264 ms (apply: 179; 0.123 ms) |
| Composition work | 28.3 ms | 40.7 ms | 59.0 ms (apply 22.2 ms) |
| Measurements; work each; wall time each | 177; 0.300 ms; 2.27 ms | 182; 0.399 ms; 1.20 ms | 179; 0.470 ms; 2.06 ms |
| Paragraphs laid out (all on the main thread, none in a frame) | 1,052 | 1,032 | |
| Paragraphs recorded while prefetching | none | 177 | none |
| All prefetch work | 85.8 ms | 119.9 ms | |
| Main thread on the little cores, while prefetching | 78 to 82 % | 11 to 26 % | 26 to 62 % |
| Second rounds of composition and layout in frames | 1, 0, 1 | 0, 0, 0 | |

compose-rs's composition of a prefetched post does about a third of Compose's work (0.080 against 0.264 ms, plus Compose's apply), its measurement 36 % less (0.300 against 0.470 ms); on the little cores, unboosted, its measurement takes about as long as Compose's in wall time (2.27 against 2.06 ms). No frame of compose-rs's laid text out on the feed. With every optimization on, the steps run on faster cores (the main thread on the little cores 11 to 26 % of its time while prefetching), where the metric reads each one higher (0.114 and 0.399 ms) for half the wall time, and `draw_prefetched_ahead` records each item's paragraphs and groups in the same idle time (177 recordings a run). The stress list's are in [15](15-android-stress-benchmark.md); the rounds before parity in "Item reuse on the device" below.

### At parity: startup

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`, 15 cold starts) | 135 ms (129 to 143) | **107 ms** (103 to 128; −21 %) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 50.0 ms | **25.8 ms** |
| `big`: 200 posts at once | 229 ms (223 to 247) | **165 ms** (152 to 180; −28 %) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 166.5 ms | **85.6 ms** |
| `bindApplication` (the feed's traced start) | 18.9 ms | **12.0 ms** |

Startup doesn't depend on the optimizations: with all of them, 109 and 170 ms (Compose in that session 136 and 231 ms), within `TotalTime`'s noise (standard deviations 4.5 to 8.3 ms over 15 starts). In `big`'s first frame, composing 200 posts took 17.3 ms and laying them out 36.4 ms (1,201 paragraphs, 31.5 ms), replaying the display list on the `Canvas` 13.0 ms (at parity each paragraph is drawn into its layer's recording, D567; with `paragraph_render_nodes`, 21.7 ms, recording each into a node of its own). The feed's first frame's own part (`compose-rs: frame`) took 5.3 ms, laying out 31 paragraphs.

### At parity: memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 52.7 MB | **33.2 MB** (−37 %) |
| of which code | 7.2 MB | **2.1 MB** |
| of which Java heap | 3.6 MB | **1.3 MB** |
| of which native heap | 8.1 MB | **7.8 MB** |
| of which graphics | 28.1 MB | **17.0 MB** |
| PSS after 10 flings | 87.6 MB | **78.9 MB** |
| of which Java heap | **4.9 MB** | 5.2 MB |
| of which native heap | 13.1 MB | **10.1 MB** |
| of which graphics | 56.8 MB | 56.3 MB |
| APK (release; decimal MB) | 1.95 MB (minified) | 1.99 MB, with the 1.82 MB `.so` uncompressed inside |

After start compose-rs holds 37 % less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 11 MB smaller (the icons drawn from masks, D190, as Compose's `VectorPainter` caches them, need no path atlas). After the flings both are dominated by the render thread's caches (56 to 57 MB each); with every optimization on, 79.2 MB (Compose 88.2 MB in that session), its native heap 11.1 MB against 10.1 MB at parity. The APKs are 1,989,482 and 1,948,185 bytes: compose-rs's grew with the resize screen (D553) and the parity work (D560–D579), to 41 KB more than Compose's.

### Every optimization on (`all`)

compose-rs with every optimization on, against Compose measured in the same session, run right after the parity round (`results/2026-10-25-pixel-8-pro-all.json`); the parity column is the parity round's, for reference.

| Feed scroll | Kotlin Compose (this session) | compose-rs, all | compose-rs, parity |
|---|---|---|---|
| Janky frames, per run: the app's; the compositor's | 0, 0, 0; 0, 0, 0 | 0, 0, 0; 0, 0, 0 | 0, 0, 0; 0, 0, 0 |
| `doFrame`s more than 2 ms late, per run (after main-thread work) | 18 (0), 26 (6), 26 (2) | 28 (0), 18 (1), 14 (0) | 11 (1), 14 (1), 7 (1) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / **5** / **6** ms | 5 / 6 / 8 ms |
| Main thread p50 / p90 / p99 | 1.45 / 3.36 / 7.21 ms | **0.87** / **1.35** / **2.37** ms | 1.28 / 2.25 / 3.47 ms |
| RenderThread p50 / p90 / p99 | 2.05 / 3.29 / 5.18 ms | **1.48** / **2.05** / **3.19** ms | 2.50 / 3.51 / 5.27 ms |
| Work: main thread / RenderThread | 901 / **918** ms | **676** / 960 ms | 445 / 723 ms |
| Boost minimum, time-weighted; frames unboosted | 146; 35 % | 302; 1 % | 38; 84 % |

| Animated header | Kotlin Compose (this session) | compose-rs, all | compose-rs, parity |
|---|---|---|---|
| Janky frames, per run | 0, 0, 0 | 0, 0, 0 | 0, 0, 0 |
| Frames presented at another vsync than expected, per run | 1,196, 0, 1,205 | 1,199, 0, 0 | 0, 0, 1,203 |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / **7** ms | 5 / 5 / 7 ms |
| Main thread p50 / p90 / p99 | 1.55 / 2.17 / 3.13 ms | **1.29** / **1.78** / **2.90** ms | 1.23 / 1.74 / 3.02 ms |
| RenderThread p50 / p90 / p99 | 3.04 / 4.10 / 5.57 ms | **2.43** / **3.40** / **4.91** ms | 2.58 / 3.27 / 4.67 ms |
| Work: main thread / RenderThread | 253 / 529 ms | **224** / **501** ms | 201 / 470 ms |
| Boost minimum, time-weighted; frames unboosted | 27; 91 % | 72; 68 % | 8; 98 % |

**With every optimization, compose-rs is ahead of Compose at every percentile on both threads**, on these screens and the stress ones ([15](15-android-stress-benchmark.md)): on the feed's scroll the main thread's p99 is a third of Compose's (2.37 against 7.21 ms) and the RenderThread's p50 28 % shorter (1.48 against 2.05 ms), gfxinfo's p99 6 against 11 ms. Against parity, the scroll's gains are the hint session's (the RenderThread 2.50 → 1.48 ms at p50, boosted on 99 % of its frames) and drawing prefetched items ahead (the main thread's p90 and p99), as [14](14-performance-opportunities.md)'s each round finds one flag at a time. In the animation, `all` and parity are within the runs' spread: the session votes little there (72) and the main thread's frames are short either way. Against round 13 (compose-rs's own defaults then, much the same set), the feed's scroll is where it was (main thread 0.77 / 1.23 / 2.07, RenderThread 1.48 / 2.00 / 2.97 ms then).

## Rounds 1 to 13: compose-rs's own defaults (history)

Until D560 (round 13 included), compose-rs ran its own optimizations by default (its hint session, its prefetch with two items ahead, kept items, D341's budget and drawing ahead, single-drawing fades, paragraph render nodes, the flush layer) and didn't yet draw Compose's layers for clips and shadows (D522–D525) or fade cells offscreen (D521); its texts were narrower before D570. Those rounds compare compose-rs's own defaults with Compose 1.11.4 (rounds 1 to 13) and are kept as history; the numbers above supersede them.

"Round 13" is compose-rs at d39ecb4 (`results/2026-10-24-pixel-8-pro-round-13.json`, `speed`, every scenario: the stress ones' results are in [15](15-android-stress-benchmark.md)). measure.py's checks passed: the installed native libraries matched those built, and both apps were `speed` at the start and the end. Four scroll runs were run again once each (each app's second and third), their first traces holding no frame of the app's (D362). "Round 12" is compose-rs at 4a28979 (`results/2026-10-24-pixel-8-pro-round-12.json`); its numbers are quoted in parentheses. "Round 1" is the version this document first reported (7550e31, `results/2026-10-22-pixel-8-pro-round-2-baseline.json`, measured again with the work metric). Compose's numbers in this part are from the round 13 run.

Round 13 against round 12:

- **D510–D512: HWUI's layers flushed before a frame that drops a path drawn as a path.** Such a frame first draws a cleared 1 × 1 compositing layer, so HWUI flushes its layers before the frame, as Compose's faded cells make it do, and Skia's GPU cache reuses the dropped paths' mask textures in the next frame rather than the one after. Only the stress animation draws new paths every frame; the feed's screens don't, and its A/B found them unchanged. This round confirms the A/B beside Compose: 3 s into the stress animation, compose-rs's graphics memory went from 154.6 to 114.9 MB and its PSS from 184.7 to 144.1 MB, both below Compose's (119.7 and 165.7 MB). After 13 s, both apps' graphics memory had grown by amounts that vary from run to run, in the GPU driver's memory: compose-rs's median was 173.7 against Compose's 146.1 MB in this round's runs, 137.7 against 136.8 MB in 4 untraced runs each after it ([15](15-android-stress-benchmark.md), "Round 13: memory").
- The library didn't change (1,708,640 bytes, the same SHA-1): D510 is in the Kotlin replayer, and the APK kept its size (1,864,482 bytes). The Kotlin app didn't change.
- measure.py now records HWUI's GPU caches (`dumpsys gfxinfo`) in the stress animation, 3 s after start and after the trace (13 s), beside `dumpsys meminfo`.

### Round 13: scrolling the feed (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,796 | 1,794 |
| Janky frames (FrameTimeline), per run | 1, 1, 1 | **0, 0, 1** (round 12: 0, 0, 0) |
| of which the app's | 1, 1, 1 | **0, 0, 1** (0, 0, 0) |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 2, 4, 2 | 0, 0, 59 (0, 0, 0) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 29 (3), 23 (5), 28 (6) | **15 (0), 19 (0), 20 (0)** (22 (0), 13 (0), 17 (1)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 11 ms | 5 / **5** / **6** ms (5 / 5 / 6 ms) |
| Main thread per frame p50 / p90 / p99 | 1.50 / 3.22 / 7.09 ms | **0.77** / **1.23** / **2.07** ms (0.78 / 1.26 / 2.05 ms) |
| RenderThread per frame p50 / p90 / p99 | 2.10 / 3.43 / 5.08 ms | **1.48** / **2.00** / **2.97** ms (1.46 / 2.15 / 3.44 ms) |

Both scroll at 120 Hz. **One frame of compose-rs's was janky, by FrameTimeline, not by its work** (the first in a feed scroll since round 10; gfxinfo counted none). Its `doFrame` started 3.2 ms after its vsync, woken late (the main thread asleep, not runnable, until then), and took 0.79 ms; the RenderThread had queued its buffer 5.1 ms after the vsync (1.35 ms of `DrawFrame`) and the GPU finished it at 7.6 ms. FrameTimeline put the frame's end 16.8 ms after the vsync, 0.22 ms past its deadline, at SurfaceFlinger's wake-up a vsync later. Why SurfaceFlinger's composition 1.2 ms after the GPU finished didn't take the buffer hasn't been traced further. The 59 frames after it, half a second, were presented a vsync late, on cadence (buffer stuffing). Compose missed one deadline in each run; its slowest main-thread frames took 11.9 to 13.1 ms (`animation` 5.1 to 7.4 ms, `AndroidOwner:measureAndLayout` up to 6.3 ms). compose-rs prefetched 348 to 351 times a run; no frame of its laid text out or ran a second round of composition and layout (the round's JSON, `lazy_lists`).

**Both threads' frames are as in round 12**, which D510 doesn't touch on this screen: the main thread's p50 / p90 / p99 0.78 / 1.26 / 2.05 → 0.77 / 1.23 / 2.07 ms, the RenderThread's 1.46 / 2.15 / 3.44 → 1.48 / 2.00 / 2.97 ms (its p99 within round 12's runs' spread, 2.94 to 3.51 ms). The main thread's median is half Compose's, its p99 under a third; the RenderThread's p99 is 42 % shorter than Compose's. gfxinfo's p99 frame time is 6 ms (Compose: 11 ms). Our session holds both threads boosted on most frames (next section): the main thread ran 13 % of its time on the little cores (round 12: 17 %), the RenderThread 2 % (4 %).

**`doFrame`s starting late**: compose-rs's 15 to 20 a run (round 12: 13 to 22); Compose's 23 to 29. Only the one above cost a frame: none of compose-rs's followed its own work on the main thread, the main thread was never runnable waiting for a core between the vsync and the `doFrame` (it was asleep until the vsync's message came: woken late), none but that one was janky or presented late, and the closest of the others finished its frame 7.3 ms before its deadline (the round's JSON, `late_do_frames`). The latest started 5.7 ms late (round 12: 3.6 ms); their `doFrame`s took 0.84 to 0.95 ms at each run's median. Compose's started up to 6.9 ms late, 3 to 6 a run after its own work on the main thread, and one finished 2.2 ms after its deadline (its second run's janky frame).

### Round 13: the RenderThread's boost

ADPF boosts a thread after the frames a hint session holding it reports run long against their target. HWUI's session holds the main thread and the RenderThread and reports each frame from the main thread's start to the RenderThread's end against 11.62 ms, so the main thread's long frames earn the boost. compose-rs's own session (D390) held the RenderThread alone in rounds 8 and 9, and holds the main thread too since D440 (round 10); since D500 (this round) it reports both threads' CPU time since the last frame against 8.33 ms (until round 11: the RenderThread's against 6.5 ms). The power HAL gives a thread the highest active vote of its sessions. measure.py's `render_boost` reads HWUI's counter alone; the rows below take each frame's boost as the higher of both sessions' votes when it started, from the same traces (the round's JSON, `d390_boost`). Medians of the 3 runs (each bucket's percentiles: of the runs with frames in it). The table describes rather than ranks, so nothing is in bold:

| Scroll | Kotlin Compose | compose-rs (round 12) |
|---|---|---|
| Boost minimum, time-weighted (out of 1024) | 134 (runs: 131 to 154) | 308 (307 to 313) (286) |
| of which HWUI's session's alone | 134 | 5 (5) |
| Frames unboosted (minimum under 100) | 43 % (43 % in each run) | 2 % (2 to 4 %) (3 %) |
| Frames at 100–199 / at 200 or more | 25 % / 31 % | 16 % / 82 % (21 % / 76 %) |
| RenderThread running time on the little cores | 23 % | 2 % (4 %) |
| Main thread running time on the little cores | 53 % | 13 % (17 %) |
| Unboosted frames: p50 / p90 / p99 | 2.86 / 3.97 / 5.52 ms | 2.53 / 3.29 / 4.40 ms (2.68 / 3.73 / 5.48 ms) |
| Frames at 100–199: p50 / p90 / p99 | 2.10 / 2.67 / 3.34 ms | 1.83 / 2.37 / 4.09 ms (1.89 / 2.44 / 3.39 ms) |
| Frames at 200 or more: p50 / p90 / p99 | 1.64 / 2.10 / 2.75 ms | 1.43 / 1.78 / 2.45 ms (1.38 / 1.77 / 2.48 ms) |

**compose-rs's frames are boosted far more often than Compose's**: 82 % of them start at 200 or more, against Compose's 31 % (round 12: 76 % against 32 %). Its vote, 308 time-weighted (round 12: 286), is near D500's A/B (261). At each boost level its frames are still the shorter, by 0.2 to 0.3 ms at p50.

**The target, round by round.** At 6.5 ms for the RenderThread alone, compose-rs's frames were unboosted as often as Compose's in [14](14-performance-opportunities.md)'s sweep (51 % against 51 %), and more often in D390's A/B (62 % against 49 %), round 8 (53 % against 34 %), round 9 (56 % against 47 %) and round 10 (56 % against 44 %), as often in round 11 (52 % against 52 %). Against 8.33 ms for both threads: 3 % against 38 % (round 12), 2 % against 43 % (round 13). Lower targets for the RenderThread alone boosted every frame (no frame unboosted at 3.5 or 4.5 ms) for 10 to 17 % more CPU power.

**The cost.** This round doesn't measure power (measure.py's traces have no power rails). D500's interleaved A/B measured the CPU rails at +1.8 % (± 2.5) on the feed's scroll and +2.9 % (± 2.7) on the stress list's, +0.7 % and −1.1 % animating, all rails within +0.6 %. D510's, on the stress animation: +2.0 % (± 2.4) on the CPU rails, all rails +1.1 % ([15](15-android-stress-benchmark.md)). D390's, for the session itself: +2.1 % scrolling and +2.3 % animating on the CPU rails, all rails (display included) +0.4 % and +0.5 %; Compose's CPU rails drew 9 % more than compose-rs's without the session, scrolling. The power HAL spends 34 ms more CPU per scroll run on the session's reports (+5 %). The work metric rises with the boost for the same code (next sections): +27 % on the main thread, +21 % on the RenderThread since round 11, for a third less CPU time.

**The vote's level (D440, D500).** Under the target, a batch of reports moves the power HAL's vote by about −6 × (target − report) + (500 / target) × (the rise over the report before), in milliseconds, whatever the vote. So the vote settles where the reports' mean is about 0.4 of the target, a pull too weak to see in a single run ([14](14-performance-opportunities.md), "The session's target"; D441 had described it as a random walk). Against 6.5 ms for the RenderThread alone, that held its time near 2.6 ms on every screen: in round 11, the stress list's RenderThread, with D470's 18 % less work, got less boost (vote 190 against 295) and came out longer than Compose's at p90 and p99. Against 8.33 ms for both threads, the level follows the frames' load: round 12's votes were 286 on the feed's scroll, 369 on the stress list, 55 in the feed's animation and 472 in the stress animation (A/B: 261, 332, 53, 476), round 13's 308, 341, 54 and 479. Since D440, the session holds the main thread too, which HWUI's session stopped boosting once the RenderThread's part of each frame got shorter (in round 9 the stress list's main thread ran 83 % of its time on the little cores; round 10 21 %, round 11 46 %, rounds 12 and 13 4 %).

In the animation, neither app's main thread has long frames, and HWUI's session rarely boosts either app: Compose's frames were 93 % unboosted (88 to 96 %; round 12: 100 %). compose-rs's session votes little there, its reports light (vote 54, runs 35 to 61; round 12: 55): 80 % of its frames unboosted (75 to 90 %; round 12: 80 %).

### Round 13: the animated header (10 s)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,340 | 1,341 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 (round 12: 0, 0, 0) |
| Frames presented at another vsync than expected, per run | 0, 0, 1,199 | 1,199, 0, 0 (1,205, 0, 1,203) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 6 (1), 8 (2), 10 (2) | **5 (0), 5 (0), 6 (0)** (15 (0), 5 (0), 5 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / **5** / **7** ms (5 / 6 / 11 ms) |
| Main thread per frame p50 / p90 / p99 | 1.58 / 2.27 / 3.33 ms | **1.19** / **1.65** / **2.85** ms (1.20 / 1.66 / 3.03 ms) |
| RenderThread per frame p50 / p90 / p99 | 3.03 / 4.23 / 5.74 ms | **2.61** / **3.30** / **4.96** ms (2.54 / 3.28 / 4.80 ms) |
| RenderThread frames unboosted (the higher of both sessions' votes) | 93 % | 80 % (80 %) |

No frame of either app missed its deadline. **One run of each app was presented a vsync late throughout**, on cadence. Compose's third ran in Choreographer's buffer stuffing recovery after a buffer was stuffed, its slowest RenderThread frame waiting for a buffer (`dequeueBuffer`, 4.7 ms). compose-rs's first showed no recovery in its trace (no recovery slice, no stuffed buffer), so how it fell a vsync behind wasn't seen. That is latency, not dropped frames. gfxinfo's p99 is 7 ms in each of compose-rs's runs, against Compose's 8, 8 and 11 ms. In round 12 it was 11 ms, set by two of compose-rs's runs in buffer stuffing recovery throughout, and that was the one place in the feed's tables where compose-rs was behind. Nothing changed on this screen since: which runs start a vsync behind varies from session to session, for both apps (one of compose-rs's in round 10, none in round 11, two in round 12, one of each app's this round; the stress animation's runs do it in most sessions, both apps'; [15](15-android-stress-benchmark.md)).

compose-rs's frames are shorter on both threads. The main thread's median is 25 % shorter than Compose's (25 % in round 12, 27 % in round 11, 23 % in round 10, 17 % in round 9). Each animation frame draws again only the repaint boundary whose content changed (D155), the spinner's group alone (D323). The pulsing dot's alpha and the star's rotation are set as their render nodes' properties without recording anything (D157), as Compose's `graphicsLayer` does. The header's three transitions run a task each (D420). The slowest main-thread frames are the system's (vsync requests, waking and waiting for the RenderThread), the same in Compose, so p99 moves least. The RenderThread's frames are 14 % shorter than Compose's at the median and 22 % at p90 (round 12: 16 % and 23 %); the spinner's arc is one `CircleOp`, as in Compose (D320).

**Against round 12, both threads' frames are within the runs' spread**: the main thread's p50 / p90 / p99 1.20 / 1.66 / 3.03 → 1.19 / 1.65 / 2.85 ms, the RenderThread's 2.54 / 3.28 / 4.80 → 2.61 / 3.30 / 4.96 ms, for the same work (205 → 201 ms, 495 → 492 ms) and the same vote (55 → 54; 80 % of frames unboosted both times). D510's A/B found this screen unchanged.

### Round 13: work per frame

| Scrolling (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 854 ms | **646 ms** (−24 %; round 12: 601 ms) |
| RenderThread work | **888 ms** | 985 ms (+11 %; 931 ms) |
| Main thread CPU time | 4.35 s | **2.14 s** (2.19 s) |
| RenderThread CPU time | 4.41 s | **2.94 s** (3.01 s) |

| Animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 256 ms | **201 ms** (−21 %; round 12: 205 ms) |
| RenderThread work | 535 ms | **492 ms** (−8 %; 495 ms) |
| Main thread CPU time | 2.08 s | **1.55 s** (1.59 s) |
| RenderThread CPU time | 3.80 s | **3.31 s** (3.20 s) |

**Scrolling, the work metric reads compose-rs's RenderThread 11 % above Compose's** (985 against 888 ms; round 12: 6 % above, round 11: 10 % below), for a third less CPU time (2.94 against 4.41 s). Its main thread's reads 24 % below Compose's (29 % in round 12, 42 % in round 11), for half the CPU time. compose-rs's code did the same as in rounds 11 and 12: no frame laid text out, as many prefetch steps; D500 changed only what the session reports, and D510 nothing on this screen. Against round 12, both threads read 6 to 7 % more work for 2 to 3 % less CPU time, boosted a little more (82 % of the RenderThread's frames at 200 or more, against 76 %). What changed since round 11 is where it ran. Boosted on most frames, both threads run on the mid and big cores at higher frequencies, where the metric (CPU time scaled by each core's frequency and capacity) reads higher for the same code: D500's A/B measured +28 % on the main thread and +18 % on the RenderThread on the feed for the same build otherwise, D440's +22 % for the stress list's main thread, and this round +27 % and +21 %. The metric assumes work scales with frequency; code waiting for memory doesn't, so it overstates work done at high frequencies. Compose's RenderThread was boosted less (31 % of its frames at 200 or more, against 82 %). So the comparison of work between the two apps is now confounded by their boost, as it was the other way in rounds 5 to 7 (when compose-rs ran on the little cores); within a build, against itself, it holds.

In the animation, where our session votes little, compose-rs's main thread does 21 % less work than Compose's and its RenderThread 8 % less, each within 2 % of round 12's.

### Item reuse on the device (D430–D435), rounds 10 to 13

compose-rs's traces time each prefetch step (`compose-rs: prefetch`), and inside it the composition of an item (`prefetch compose`), its measurement (`prefetch measure`, text layout included) and the recording of its paragraphs. Compose's time its own (`compose:lazy:prefetch:compose`, `…:measure`). The table gives, per run, the main thread's work inside those slices from the stats' start (ms on the fastest core at its highest frequency), computed from the traces of rounds 9 to 13 (each round's JSON, `prefetch_slices`). Medians of 3 runs; nothing ranked:

| Feed scroll (10 flings), per run | compose-rs, round 9 | compose-rs, round 10 | compose-rs, round 11 | compose-rs, round 12 | compose-rs, round 13 | Compose, round 13 |
|---|---|---|---|---|---|---|
| Prefetch steps | 372 | 362 | 361 | 352 | 349 | |
| Item compositions; work each | 349; 0.096 ms | 347; 0.086 ms (−10 %) | 346; 0.087 ms | 347; 0.106 ms | 348; 0.111 ms | 233; 0.269 ms (round 12: 0.258 ms) |
| Composition work | 33.3 ms | 29.7 ms (−11 %) | 30.1 ms | 36.9 ms | 38.8 ms | 62.7 ms |
| Measurements; work each | 174; 0.308 ms | 173; 0.310 ms | 173; 0.326 ms | 174; 0.355 ms | 174; 0.396 ms | 179; 0.483 ms |
| Paragraphs laid out while prefetching | 1,044 | 911 (−13 %) | 1,038 | 1,038 | 1,026 | |
| Paragraphs recorded | 221 | 194 (−12 %) | 194 | 174 | 173 | |
| All prefetch work | 93.0 ms | 88.9 ms (−4 %) | 91.5 ms | 104.0 ms | 113.4 ms | |
| Main thread on the little cores, while prefetching | 82 to 85 % | 70 to 76 % | 70 to 77 % | 15 to 20 % | 6 to 20 % | |

| Stress list scroll, per run | compose-rs, round 9 | compose-rs, round 10 | compose-rs, round 11 | compose-rs, round 12 | compose-rs, round 13 | Compose, round 13 |
|---|---|---|---|---|---|---|
| Prefetch steps | 423 | 352 | 354 | 345 | 350 | |
| Item compositions; work each | 307; 0.163 ms | 347; 0.167 ms | 351; 0.132 ms | 347; 0.169 ms | 354; 0.163 ms | 389; 0.265 ms (round 12: 0.284 ms) |
| Composition work | 48.5 ms | 58.1 ms | 46.8 ms | 57.7 ms | 57.8 ms | 103.1 ms |
| Measurements; work each | 213; 0.318 ms | 211; 0.304 ms | 217; 0.245 ms | 217; 0.303 ms | 216; 0.299 ms | 178; 0.561 ms |
| Paragraphs laid out while prefetching | 1,283 | 670 (−48 %) | 629 | 626 | 625 | |
| Paragraphs recorded | 265 | 200 (−25 %) | 206 | 199 | 197 | |
| All prefetch work | 127.6 ms | 134.5 ms | 106.8 ms | 131.1 ms | 130.2 ms | |
| Main thread on the little cores, while prefetching | 85 to 87 % | 18 to 19 % | 37 to 57 % | 3 to 5 % | 3 to 11 % | |

In round 10, against round 9:

**On the feed**, composing a prefetched post takes 10 % less work (0.096 → 0.086 ms), and 0.58 against 0.75 ms of wall time. Headless, reuse took 16 % off an item scrolled into view (composition, measurement and drawing together). Fewer paragraphs are laid out (−13 %) and recorded (−12 %): a reused text node whose text and constraints didn't change, such as a tag or a like count that two posts share, most likely keeps its layout. All prefetch work went down 4 %.

**On the stress list**, the per-item numbers can't be read the same way: its main thread moved from the little cores to the mid and big ones (D440), where the work metric reads about a fifth higher for the same code (D440's A/B: 498 → 610 ms of main-thread work, for a third less CPU time). An item's composition did 0.167 ms of work against 0.163 ms, its measurement 0.304 against 0.318 ms, and the prefetch steps took half the wall time (1.14 against 2.18 ms each). Half as many paragraphs were laid out (1,283 → 670): the chips', buttons' and section headers' labels are the same from one item of a kind to the next. Separating reuse's own effect from D440's would take an interleaved A/B on the device.

Compose's prefetched items cost the same as in round 9 on both lists (0.255 and 0.258 ms of work each): its posts were already reused without content types.

**In round 11**, with only the items in view placed (D470), the feed's prefetching did as much as in round 10: as many compositions and measurements, each within a few percent of round 10's work. More paragraphs were laid out while prefetching (911 → 1,038, about round 9's 1,044), for as many recorded; that hasn't been broken down. On the stress list, an item's composition and measurement did a fifth less work (0.167 → 0.132 ms, 0.304 → 0.245 ms), but the main thread ran more of its time on the little cores again (37 to 57 %, against 18 to 19 %), where the work metric reads lower for the same code: our session voted less with less RenderThread work ([15](15-android-stress-benchmark.md)).

**In round 12**, the prefetching did the same steps (as many compositions and measurements, the same 1,038 and 626 paragraphs laid out), on the mid and big cores (the main thread on the little cores 15 to 20 % of its time on the feed, 3 to 5 % on the stress list) since D500: the work metric reads 9 to 28 % higher for each (a feed post's composition 0.087 → 0.106 ms, the stress list's 0.132 → 0.169 ms, back to round 10's 0.167 ms when its main thread was on the same cores), while the prefetch steps' wall time fell by about two fifths (1.66 → 1.00 ms a step on the feed, 1.35 → 0.93 ms on the stress list). Compose's prefetched items cost about as in round 11 (0.258 and 0.284 ms each).

**In round 13**, D510 touches neither list, and the prefetching did the same steps again (348 and 354 compositions, 1,026 and 625 paragraphs laid out, none inside a frame). On the stress list each step cost as in round 12 (composition 0.163 against 0.169 ms of work, measurement 0.299 against 0.303 ms). On the feed the metric read a measurement 12 % higher (0.355 → 0.396 ms) and a composition 5 % higher (0.106 → 0.111 ms), with the main thread a little less often on the little cores while prefetching (6 to 20 % of its time, against 15 to 20 %); this hasn't been separated from the metric's drift. Compose's prefetched items cost as in round 12 (0.269 and 0.265 ms each).

### Lazy lists place only the items in view (D470–D496): the A/Bs before round 11

Interleaved A/Bs, before round 11 measured the result beside Compose. The first: main (727d266) against D470–D474, 12 runs each ([15](15-android-stress-benchmark.md), "The lazy list's policy", has both lists and the variants weighed). A list now places only the items in view (it placed a viewport's worth more on each side), prefetches from its first layout on, and its first layout composes what it shows. On the feed's scroll: main thread p50 / p90 / p99 1.23 / 1.76 / 2.72 → 1.16 / 1.68 / 2.73 ms, RenderThread 2.33 / 3.40 / 4.79 → 2.24 / 3.26 / 4.86 ms (p99 within the runs' spread), RenderThread work 810 → 772 ms, no janky frame and no text laid out in a frame either way, native heap after the flings 11.8 → 10.6 MB. Its start: `TotalTime` 108.5 → 104.0 ms, the first frame's own part 6.7 → 5.1 ms; the big screen's unchanged (162 ms). A review then found regressions D470 had hidden (scrolls onto large items out of view cut short, short first frames under a large first item, a viewport's worth composed on focus) and that TalkBack couldn't scroll a list; D475–D478 fix them, and an 8-run A/B against main confirmed the feed's numbers: main thread 1.23 / 1.75 / 2.75 → 1.17 / 1.67 / 2.71 ms, RenderThread 2.31 / 3.38 / 5.08 → 2.35 / 3.23 / 4.72 ms, work −6 % and −7 %. A third review's seven fixes (D479–D485) kept them in another 8-run A/B (main thread 1.22 / 1.75 / 2.75 → 1.16 / 1.66 / 2.73 ms, RenderThread 2.20 / 3.34 / 4.97 → 2.32 / 3.21 / 4.66 ms, work −7 % and −8 %; [15](15-android-stress-benchmark.md)). After a fourth review's five fixes (D486–D491), again: main thread 1.24 / 1.75 / 2.74 → 1.17 / 1.67 / 2.64 ms, RenderThread 2.35 / 3.40 / 4.89 → 2.33 / 3.20 / 4.73 ms, work −6 % and −7 %. A fifth review's fixes (D492–D496) don't touch a steady scroll and had no device A/B. Round 11, beside Compose, had the feed's main thread at 1.16 / 1.68 / 2.74 ms, its RenderThread at 2.10 / 3.10 / 4.86 ms, its work −1 % and −4 % against round 10, and its native heap after the flings at 10.6 MB (round 12: 10.5 MB, round 13: 10.4 MB).

### Text across width changes (D550–D553): the resize screen

An ad hoc screen in both apps (`resize`, not a measure.py scenario) lays 56 texts out in a container whose width animates every frame. Compose lays every text out again at each new width, from intrinsics it measured once; compose-rs now does the same (D550: Kotlin keeps each paragraph's intrinsics, `relayout` sends the widths), where it used to send and measure each text anew. 6 interleaved runs: main-thread work 1,853 → 1,094 ms, main thread p50 / p90 / p99 3.28 / 4.39 / 5.66 → 2.49 / 3.21 / 4.69 ms; Compose 3,615 ms and 5.16 / 8.29 / 10.29 ms. With D551's optimization, off by default, the labels that fit keep their paragraphs: 3 layouts a frame instead of 43, main-thread work 527 ms, RenderThread work 815 → 547 ms, at about the same frame times (the hint session lowers the clocks). Details in [14](14-performance-opportunities.md), "Text across width changes". bench-rs's APK is 1,897,018 bytes with the screen (main: 1,875,098), bench-compose's 1,948,185 (1,931,801).

### Round 13: startup, launch to the first frame with content

| | Kotlin Compose | compose-rs |
|---|---|---|
| `feed` (`TotalTime`) | 136 ms | **106 ms** (−22 %; round 12: 109 ms) |
| `feed`: the first frame's main thread (`traversal`, one traced start) | 48.7 ms | **23.7 ms** (24.4 ms) |
| `big`: 200 posts at once | 236 ms | **167 ms** (−29 %; 163 ms) |
| `big`: the first frame's main thread (`traversal`, one traced start) | 173.1 ms | **88.8 ms** (88.9 ms) |

Compose's numbers are under `speed`. Compiled from its shipped profiles (`speed-profile`), its feed starts sooner: 120 against 132 ms in D362's session, where compose-rs's took 107 ms either way; `big` didn't change (228 ms). Nothing since round 11 touches startup (our session opens at the second frame shown; D510 draws only when a frame drops a path): compose-rs's feed started 3 ms sooner than in round 12 (109 → 106 ms) and Compose's 1 ms later (135 → 136 ms), `big` 4 ms later (163 → 167 ms; Compose's 233 → 236 ms), within `TotalTime`'s noise (a standard deviation of 7.0 ms over this round's 15 feed starts for compose-rs, 6.5 ms for Compose; compose-rs's feed ranged over 98 to 122 ms, Compose's over 126 to 153 ms). The feed's first frame's own part (`compose-rs: frame`) took 4.9 ms (round 12: 4.9 ms), laying out 31 paragraphs.

In `big`'s first frame (88.8 ms), composing 200 posts takes 16.1 ms and laying them out 33.7 ms. Drawing the display list takes 0.8 ms, and replaying it on the `Canvas` 20.7 ms (recording each paragraph into its `RenderNode`); the rest is the view system's traversal around them. Compose records its first `big` frame's drawing in 34.9 ms (`Record View#draw()`). On the feed's traced start, `bindApplication` is shorter too (13.5 against 16.9 ms; 12.4 against 17.1 ms in round 12, one traced start each): there is no Compose runtime to initialize.

### Round 13: memory and APK

| | Kotlin Compose | compose-rs |
|---|---|---|
| PSS after start | 51.3 MB | **33.5 MB** (−35 %; round 12: 33.9 MB) |
| of which code | 7.1 MB | **2.0 MB** (2.4 MB) |
| of which Java heap | 3.6 MB | **1.3 MB** (1.3 MB) |
| of which native heap | 8.0 MB | 8.2 MB (8.2 MB) |
| of which graphics | 26.7 MB | **17.0 MB** (17.0 MB) |
| PSS after 10 flings | 89.4 MB | **77.3 MB** (77.7 MB) |
| of which Java heap | 6.5 MB | **4.4 MB** (4.4 MB) |
| of which native heap | 13.2 MB | **10.4 MB** (10.5 MB) |
| of which graphics | 56.5 MB | 55.3 MB (55.1 MB) |
| APK (release; decimal MB) | 1.92 MB (minified) | **1.86 MB**, with the 1.71 MB `.so` uncompressed inside (1.86 MB) |

After start, compose-rs holds a third less: its code is a native library rather than Compose's dex and runtime, and its graphics memory is 10 MB smaller, most likely because the GPU keeps no path atlas or stencil buffer for the icons, drawn from masks (D190; round 1 held 26.2 MB). Both apps' code grew with the stress screens (D400) and the `grid` screen (D473): compose-rs's to 2.4 MB, Compose's to 7.4 MB in round 12. Both read less this round (2.0 and 7.1 MB), compose-rs's library the same and Compose's app unchanged: code PSS counts the mapped pages resident when measured, which varies between sessions. After scrolling, both are dominated by graphics memory (the render thread's caches: about 55 to 57 MB each). The rest depends on when the garbage collector last ran: `dumpsys meminfo` itself collects in the app (`Runtime.gc` in its path; a second call right after reads 10–13 MB less for both apps), so the measurement after scrolling counts what the 10 flings left for the collector. Compose's numbers swing with it (89.4 MB this round, with 6.5 MB of Java heap, against 89.1 MB and 5.8 MB in round 12; its runs 86.3 to 89.8 MB; up to 112 MB in earlier rounds: it allocates on every frame); compose-rs's stay within about two megabytes (77.1 to 79.3 MB in this round's runs).

Placing only the items in view (D470) took the feed's native heap after 10 flings from 11.8 to 10.6 MB in round 11, as in D470's A/Bs (11.8 to 12.0 → 10.5 to 10.6 MB); 10.5 MB in round 12, 10.4 MB this round. On the stress list, D460 and D470 together took it from 22.3 to 17.2 MB ([15](15-android-stress-benchmark.md), "Round 13: memory").

In round 1, compose-rs's native heap after scrolling held 8.9 MB waiting for the collector, 7.2 MB of it render nodes: a dropped `RenderNode` frees its native side (the recording the render thread last drew included) only when collected, and ART collects on Java heap growth (8 MB at least on this device), which compose-rs's frames rarely reach. D195 reuses render nodes through pools (a released paragraph's or gone group's node records the next one, and its old recording is freed the first time the new one is drawn) and shares a paint per text style: the native heap after 10 flings went from 20.1 to 12.1 MB, with no cost to the frame. Rust's own heap is small (about 1.5 MB, the composition of about 15 posts with their slots). D410 does the same for paths, which the feed doesn't draw anew; on the stress animation it took compose-rs's memory after 13 s from 405 to 204 MB ([15](15-android-stress-benchmark.md)).

The APK is smaller than Compose's. Since D361, release builds take a release profile for Android (fat LTO, one codegen unit, `panic = "abort"`), stored uncompressed so the library can be mapped, against Compose's minified dex ([14](14-performance-opportunities.md), #4b). bench-rs's APK holds androidx.profileinstaller too since D360 (about 38 KB). Both grew in round 8 with the stress screens (D400: the library by 181 KB, Compose's APK by 147 KB); D390 added 12 KB to the library, D390's refinements, D410–D412 and D420–D424 9 KB more, item reuse (D430–D435) and the main thread in the session (D440) 20 KB more (1,632,832 bytes), lazy grids item by item (D450–D459), the lists' policy and its fixes (D470–D496) 76 KB more (1,708,480 bytes), and D500 160 bytes (1,708,640 bytes). D510 changed only the Kotlin replayer: the APK kept its size to the byte (1,864,482 bytes). Compose's APK grew by 33 KB with the `grid` screen (D473).

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

History: how compose-rs's own defaults closed each gap over rounds 1 to 13, against Compose 1.11.4; Compose's numbers are round 13's. Several of the steps are optimizations that parity turns off (D560–D569: the hint session, drawing ahead, kept items, D341's budget, single-drawing fades, paragraph nodes); round 14 at parity, above, is where each gap stands with Compose's work, and [14](14-performance-opportunities.md), "Backport candidates", what each of those steps is worth alone.

| Gap | Steps |
|---|---|
| Scrolling, janky frames per run (Compose: 1, 1, 1) | 1, 2, 0 in round 1, all the app's; 1, 0, 1 in round 2; 0, 0, 0 in round 3; 1, 3, 1 in round 4, all the compositor's (a vsync resync, a display stall); 0, 0, 0 with the prefetch budget on the real vsync and the warm-up swipe left out (D340, D343, round 5); 2, 0, 0 in round 6, the compositor's; 0, 0, 0 in rounds 7, 8 and 9; 1, 0, 0 in round 10, the app's by FrameTimeline: the RenderThread waited 10.9 ms in a call to SurfaceFlinger; 0, 0, 0 in rounds 11 and 12; 0, 0, 1 in round 13, the app's by FrameTimeline: a frame its GPU work had finished 9 ms before the deadline, counted 0.2 ms past it |
| Scrolling, main thread p99 (Compose: 7.09 ms) | 8.57 ms in round 1; 3.94 ms with prefetching (D156, round 2); 3.74 ms in round 3; 3.11 ms with prefetched paragraphs recorded while idle (D322, round 4; p90 2.20 → 1.90 ms); 3.04 ms in round 5; 3.05 ms with the items' groups recorded while idle (D350, round 6; p90 1.92 → 1.84 ms); 2.86 ms in round 7 (within round 6's runs' spread), and in round 8; 2.76 ms in round 9; 2.85 ms in round 10, within the runs' spread, with the main thread in the hint session (D440; p90 1.90 → 1.83 ms); 2.74 ms in round 11, within the spread, with only the items in view placed (D470; p90 1.83 → 1.68 ms); 2.05 ms in round 12, with both threads' work in the hint session against 8.33 ms (D500; p90 1.68 → 1.26 ms, p50 1.16 → 0.78 ms); 2.07 ms in round 13 |
| Animation, main thread per frame p50 (Compose: 1.58 ms) | 3.1 ms at first (`results/…-baseline.json`); 2.0 ms with each paragraph recorded once into a `RenderNode`, `Paint` properties set only on change and a layer's alpha folded into its one drawing (D153); 1.66 ms with each repaint boundary's group kept in a render node (D154, round 1); 1.38 ms with retained drawing and layer properties on render nodes (D155, D157, round 2); 1.32 ms with the display list as a tree (D270, round 3); 1.20 ms with a replay without interpreted calls and records naming nested groups (D321, D323, round 4); 1.21 ms in round 5; 1.18 ms in round 6; 1.21 ms in round 7; 1.33 ms in round 8, whose two runs presented a vsync late were slower (D390's A/B: 1.20 → 1.22 ms); 1.26 ms in round 9; 1.19 ms with the main thread in the hint session (D440, round 10); 1.16 ms in round 11; 1.20 ms in round 12, less boosted than in round 11 and two of its runs in buffer stuffing recovery throughout; 1.19 ms in round 13 |
| Animation, RenderThread work (Compose: 535 ms) | 505 ms in round 1; 535 ms in round 2 and 540 ms in round 3, even with Compose's (a square-capped spinner, a stroked path each frame); 517 ms with the round cap (D320, round 4); 501 ms in round 5; 502 ms in round 6; 505 ms in round 7; 556 ms in round 8, boosted (D390's A/B: unchanged, 450 → 444 ms over 9 s); 499 ms in round 9; 516 ms in round 10; 492 ms in round 11; 495 ms in round 12; 492 ms in round 13 |
| Scrolling, RenderThread work (Compose: 888 ms) | 970 ms in round 1; 774 ms with retained groups, icon masks (D190) and no boost (round 2); 779 ms in round 3; 786 ms in round 4; 813 ms in round 5; 794 ms in round 6; 784 ms in round 7; 821 ms in round 8; 810 ms in round 9; 804 ms in round 10; 769 ms with only the items in view placed (D470, round 11); 931 ms in round 12, boosted on most frames (D500), where the metric reads higher for the same code (D500's A/B: +18 %), for 3.01 s of CPU time against Compose's 4.34 s. Reopened by the metric, not by the code ("Round 13: work per frame"); 985 ms in round 13, boosted a little more (82 % of frames at 200 or more), for 2.94 s against Compose's 4.41 s |
| Scrolling, RenderThread p50 (Compose: 2.10 ms) | 2.71 ms in round 5, 2.80 ms in round 6, 2.70 ms in round 7: not compose-rs's work, but how rarely Android boosted it (at the same boost the durations match); 2.31 ms in round 8 with a hint session of its own (D390; Compose's p50 moves with how often its session boosts it: 1.85 to 2.33 ms in round 8's runs); 2.31 ms in round 9, with fewer of its frames boosted to 200 or more (Compose's: 2.04 to 2.17 ms in its runs); 2.35 ms in round 10, boosted as in round 9 (Compose's: 2.01 to 2.16 ms); 2.10 ms in round 11, boosted as often as Compose's and shorter at each boost, with less work (D470; Compose's: 2.08 to 2.35 ms in its runs), closed in that session; 1.46 ms in round 12, both threads' work reported against 8.33 ms (D500; 76 % of its frames boosted to 200 or more; Compose's: 1.86 to 2.24 ms in its runs); 1.48 ms in round 13 (82 %; Compose's: 2.06 to 2.18 ms), its p99 2.97 against Compose's 5.08 ms |
| Startup, `big` (Compose: 236 ms) | 186 ms in round 1; 178 ms in round 2; 179 ms in round 3; 174 ms with composition linear in its reads (D310, round 4); 163 ms with `BoringLayout` (D330, round 5), and in round 6; 160 ms in round 7; 166 ms in round 8; 163 ms in round 9; 164 ms in round 10; 162 ms in round 11; 163 ms in round 12; 167 ms in round 13 |
| PSS after 10 flings (Compose: 89.4 MB) | 87.1 MB before groups, 90.1 MB in round 1; 80.9 MB with render nodes reused (D195, round 2); 80.8 MB in round 3; 81.2 MB in round 4; 79.5 MB in round 5; 79.7 MB in round 6; 80.0 MB in round 7; 80.2 MB in round 8, and in round 9; 80.6 MB in round 10, with items kept for reuse (D430); 79.4 MB in round 11, with only the items in view placed (D470: native heap 11.8 → 10.6 MB); 77.7 MB in round 12; 77.3 MB in round 13 |
| APK (Compose: 1.92 MB) | 2.4 MiB in round 2; 3.24 MB (3.1 MiB) in round 3, unstripped; 1.99 MB stripped (D301, round 4), and in round 5; 1.55 MB with the release profile (D361, round 6), profileinstaller included; 1.57 MB in round 7; 1.76 MB in round 8, with the stress screens (Compose's grew by 0.15 MB); 1.77 MB in round 9; 1.79 MB in round 10; 1.86 MB in round 11, with lazy grids item by item and the lists' policy (Compose's grew by 0.03 MB with the `grid` screen), and in rounds 12 and 13. Closed |

In round 3, D270 (the display list as a tree) was also measured on its own (same device, the APKs before and after it alternated, 3 runs each, averages of the `compose-rs:` sections): in the animation, `frame` 0.252 → 0.217 ms (`draw` 0.086 → 0.061, `serialize` 0.036 → 0.028; `replay` unchanged); scrolling, `frame` 0.273 → 0.237 ms. Headless (`cargo bench --bench ui`), the feed's animation frame went from 2.0 to 1.39 µs and the todo toggle from 263 to 194 µs. The changes of rounds 4 to 8 were measured the same way in [14](14-performance-opportunities.md), round 9's (D410–D412, D420–D424) on the stress screens in [15](15-android-stress-benchmark.md), and round 10's: item reuse (D430–D433) headless in [14](14-performance-opportunities.md) (#19), the main thread in the hint session (D440) interleaved on the device in [14](14-performance-opportunities.md), "The main thread in the session", round 11's: the lists' policy (D470–D496) interleaved on the device in [15](15-android-stress-benchmark.md), "The lazy list's policy", round 12's: the hint session's target (D500) interleaved on the device in [14](14-performance-opportunities.md), "The session's target", and round 13's: HWUI's layers flushed before frames dropping paths (D510) interleaved on the device in [15](15-android-stress-benchmark.md), "HWUI's workers and graphics memory".

## What made the difference

At parity (round 14), compose-rs's lead comes from what it does by default, which is Compose's design done natively: recordings kept per repaint boundary and drawn again only on change (D155), held by reference (D270) and naming nested groups by key (D323), layer properties set on render nodes (D157), a replay without interpreted calls (D321), icons from masks (D190, as Compose's `VectorPainter`), render nodes reused (D195), composition linear in its reads (D310), `BoringLayout` (D330), items reused by content type (D430) and only the items in view placed (D470), and no Kotlin runtime to start. Several of the steps below are optimizations that parity turns off (D569): the hint session (D390, D440, D500: `performance_hint`), drawing ahead (D322, D350: `draw_prefetched_ahead`), kept items (D375, D380: `keep_items_behind`), D340/D341/D381's budget (`prefetch_budget_recent_maximum`), two items ahead (D156: `prefetch_two_ahead`), single-drawing fades and paragraph nodes (D153: `single_drawing_fade`, `paragraph_render_nodes`) and the flush layer (D510); [14](14-performance-opportunities.md), "Backport candidates", measures each alone against parity. The bullets are history: what made the difference in rounds 1 to 13.

- **Main-thread frames**: each repaint boundary keeps its recording and a version, and draws again only when something it drew from changed (D155); a frame holds the kept recordings by reference, without copying them (D270), and a group's record names its nested groups by key, so a nested group's change leaves its parent's recording alone (D323); Android keeps a render node per group by key and version and sets graphics layers' properties on them (D157), so a layer-only change records nothing, as in Compose; the replay calls no interpreted framework method (D321). Lazy lists compose and premeasure their next item when the main thread is idle (D156), record its paragraphs (D322) and draw and record its groups then (D350), which takes composition, text layout and recording out of the scrolled frames; they keep the two items behind the scroll composed (D375) and recorded (D380), so a fling back's first frames do no more. An item coming into a list takes the composition and nodes of one of the same content type that left (D430–D435), as in Compose: a prefetched feed post's composition does 10 % less work. A list places only the items in view, as Compose's (D470), so a frame replays fewer render nodes and its first frame composes only what it shows (D472).
- **Jank**: the idle-time budget is counted from the real vsync, so prefetching goes on through Choreographer's buffer stuffing recovery (D340), and an idle-time step starts only if its recent maximum fits with a margin, so it doesn't delay the next frame (D341), scaled by how much slower than expected the idle time's steps so far ran (D381).
- **RenderThread frames**: kept groups are drawn by reference; small filled paths are drawn from cached alpha masks (one GPU render pass a frame instead of two, no path stencil operations: D190); the spinner's round cap is drawn as a circle (D320). Without long main-thread frames, HWUI's hint session rarely gets the RenderThread boosted, which lengthened the scroll's frames but not their work; a session of compose-rs's own (D390) gets it boosted: reporting the RenderThread's CPU time against 6.5 ms, about half its scroll frames (against 4 % without it), for about 2 % more CPU power; since D500, reporting both threads' CPU time against 8.33 ms, whose vote follows the frames' load, 98 % of them (Compose's: 57 % this round), for +1.8 to +2.9 % more CPU-rail power in its A/B. Placing only the items in view (D470) gives it less to sync and draw: 4 % less work on the feed, 18 % less on the stress list, in round 11.
- **Where the main thread runs**: since D440, compose-rs's session holds the main thread too, which HWUI's session stopped boosting once the RenderThread's part of each frame got shorter. On the stress list, the main thread ran 21 % of its time on the little cores in round 10 (83 % in round 9), for a p50 of 1.02 against Compose's 1.29 ms; in round 11, with less work to report after D470 and so a lower vote, 46 %, for a p50 of 1.08 against Compose's 1.25 ms; in round 12, with both threads' work reported (D500), 4 %, for a p50 of 0.81 against 1.26 ms, and in round 13 4 %, 0.83 against 1.26 ms. On the feed, 13 % (17 % in round 12, 73 % in round 11), for a p50 of 0.77 against Compose's 1.50 ms.
- **Startup and the big screen**: compose-rs composes and lays out natively, without class loading, JIT warm-up or a Kotlin runtime to initialize; composition is linear in its reads (D310), and single lines are laid out with `BoringLayout` (D330). The big screen's first frame takes 52 % of Compose's.
- **Memory and size**: the app's code is a 1.7 MB native library rather than Compose's dex and runtime (5.0 MB less code mapped, 2.3 MB less Java heap), built with LTO (D361); icons drawn from masks need no GPU path atlas (D190); render nodes are reused rather than left to the garbage collector (D195); a frame dropping a path flushes HWUI's layers first, so Skia's GPU cache gives the last frame's mask textures to the next (D510: on the stress animation, graphics memory 3 s after start 154.6 → 114.9 MB, below Compose's 119.7 MB).

## Next steps

[14](14-performance-opportunities.md) measures what's left. For frame durations:

- **Pausable composition: done** (D590–D599): prefetch composes items with a paused composition, as Compose's default; against parity before it, no frame percentile, jank or late `doFrame` moved beyond the runs' spread, the idle times fitting whole compositions on this device ([15](15-android-stress-benchmark.md), "Pausable composition (D598)"). After the review's fixes (D597) and a cheaper per-item construction (D599), rechecked in 4 rounds: still nothing beyond the spread, main-thread work +1 % and +2 % where it was +3 % and +3.5 % ("Pausable composition, after the review").
- **The RenderThread at parity** is behind Compose's on both scrolls (p50 +18 % on the feed, +20 % on the stress list) for less work, because HWUI's session, the only one at parity, boosts it less: compose-rs's short main-thread frames earn little boost (38 against 129 on the feed). Its own session (`performance_hint`) is the fix it has, an app-side choice with a power cost (D500's A/B: +1.8 to +2.9 % on the CPU rails); as a Compose change, HWUI's session could report both threads' work against one interval ([14](14-performance-opportunities.md), "Backport candidates").
- **Prefetch steps past the vsync at parity**: unboosted, an item's measurement runs 3.5 to 8 ms on a little core, and 9 steps in round 14's 6 scroll runs ended 1.3 to 3.6 ms after their vsync, delaying a `doFrame` (none janky). Compose doesn't split a measurement either; D341's recent-maximum estimate (`prefetch_budget_recent_maximum`) was made for this, and splitting a measurement at its paragraphs would be the next step if they cost frames.

The rest is history from rounds 1 to 13 (compose-rs's own defaults):

- **The scroll's slowest main-thread frames**: since D375 and D380, a fling back's first frames compose, lay out and record nothing either. No frame of compose-rs's missed the app's deadline in rounds 7 to 9, 11 and 12; round 10's one janky frame was the RenderThread waiting 10.9 ms in a call to SurfaceFlinger, and round 13's a frame whose GPU work finished 9 ms before its deadline, which FrameTimeline counted 0.2 ms past it when SurfaceFlinger took the buffer a vsync late: neither was compose-rs's work. What sets the p99 now (2.07 ms) hasn't been broken down; the slowest frames (3.3 to 3.4 ms a run in round 13) are mostly the view system's `traversal`, waiting for the RenderThread's sync (`postAndWait`). On the stress list, each round-9 run had 1 to 3 frames of about 10 ms that laid a newly revealed item out inside the frame; rounds 10 to 13 had none (their slowest main-thread frames took 5.1, 4.4, 4.3 and 4.3 ms), and in rounds 11 to 13 no frame laid text out on either list ([15](15-android-stress-benchmark.md)).
- **Text layout while prefetching** is still about half of a prefetched item's work, after `BoringLayout` (D330), and most of the big screen's 35 ms of layout. Laying out on worker threads (#11) took the scroll's main p99 from 3.53 to 3.08 ms and the big screen's first-frame work down 16 % in a prototype that isn't production-ready (measured before D330); batching JNI calls alone is capped at about 2.6 ms of the big frame.
- **Late `doFrame`s**: 15 to 20 a scroll run start more than 2 ms after their vsync in round 13, 13 to 22 in round 12 (7 to 12 in round 11, 3 to 12 in round 10, 12 to 21 in round 9; Compose's 23 to 29 in round 13): D500 nearly doubled them, as its A/B found (89 → 149 in 10 runs). In round 13 one of them, 3.2 ms late, became the feed's one janky frame (above), its own work done 9 ms before the deadline; on the stress list, one followed a prefetch step that ran 8.2 ms on a little core and ended 2.2 ms after the vsync, without cost (the frame was on time). They are woken late: the `Choreographer#onVsync` message comes 2 to 4 ms after the vsync, the main thread asleep until then (never runnable waiting for a core, nor waiting for our work: one of 52 followed main-thread work, against 13 of Compose's 85). They don't cost a frame: none started more than 3.6 ms late, none was janky or presented late, and the closest still finished 8.3 ms before its deadline, their `doFrame`s taking 0.9 ms at the median (1.6 ms at most; the round's JSON, `late_do_frames`; on the stress list: 13 to 15 a run, at most 4.5 ms late, the closest 8.2 ms before its deadline; in round 13, 13 to 23, at most 6.2 ms late, the closest 5.4 ms before its deadline). Their margin is more than any frame of ours takes; they would matter on a screen whose frames take most of an interval. Why the boosted main thread is woken late more often hasn't been traced. Before D440, those that waited for our work were a prefetch measuring an item on the little cores; D381 halved the idle-time slices running past the vsync, and D440 took the stress list's main thread off the little cores. If they come back, splitting an item's measurement at its paragraphs (so a step can stop at the deadline) would be the next step, at the cost of an interruptible measure.
- **The compositor's jank**: none scrolling in rounds 7 to 9 (round 6: 2, SurfaceFlinger's, in one run); 3 in one animation run of round 9 (two prediction errors, a SurfaceFlinger scheduling miss). In D440's A/B, 4 of the 48 runs with the main thread in the session had 1 to 3 (prediction errors, SurfaceFlinger scheduling, the display HAL), against none of main's 48, and 6 of 48 for the most boosted variant: too few to tell from noise. Round 10, with D440, had none in any measured run of either app (12 runs each, on the feed and the stress screens); the only ones in compose-rs's traces were prediction errors during the warm-up swipe, which the stats leave out by design (D343); one of Compose's traces had three there too (SurfaceFlinger's and the display HAL's). Round 11 had none in compose-rs's measured runs either; Compose's animation had 2 prediction errors in one run. Round 12 had 3 in one of compose-rs's stress list runs (2 prediction errors, 1 SurfaceFlinger scheduling), which had entered Choreographer's buffer stuffing recovery during the warm-up swipe, and 2 prediction errors in one of Compose's; D500's A/B had 4 against main's 6 across its scenarios. Round 13 had 2 in one of compose-rs's stress list runs (the display HAL, SurfaceFlinger scheduling), none in Compose's. Still watched.
- **The scroll's RenderThread durations** follow ADPF's boost. Since D390, compose-rs's own session boosts the RenderThread; its target is a trade of power for durations, swept in [14](14-performance-opportunities.md). Against 6.5 ms for the RenderThread alone, how often each app ended up boosted varied from session to session (compose-rs's frames unboosted 51 to 62 %, Compose's 34 to 52 %, in six sessions), and on the stress list in round 11 less work meant a lower vote, its RenderThread's p90 and p99 longer than Compose's (3.81 / 5.45 against 3.63 / 5.27 ms). Since D500, both threads' work against 8.33 ms: round 12 confirms its A/B beside Compose, the feed's RenderThread at 1.46 / 2.15 / 3.44 ms against 2.08 / 3.33 / 4.98 ms, the stress list's at 2.00 / 2.75 / 3.94 ms against 2.59 / 3.56 / 4.88 ms ([15](15-android-stress-benchmark.md)), 3 % and 0 % of compose-rs's frames unboosted; round 13 again, 1.48 / 2.00 / 2.97 against 2.10 / 3.43 / 5.08 ms and 2.12 / 2.90 / 3.86 against 2.53 / 3.53 / 5.03 ms, 2 % and 0 % unboosted. The price is power, which this round doesn't measure: +1.8 % (± 2.5) and +2.9 % (± 2.7) on the CPU rails in the A/B. If power must not move, a 6.0 ms target for the RenderThread alone took a fifth (the feed's p50) to two thirds (the stress list's p90) of the gain, for no power change resolved ([14](14-performance-opportunities.md)). The work metric reads the boosted threads' work higher for the same code (feed RenderThread 769 → 931 ms since round 11, 985 ms in round 13, above Compose's 888 ms), so comparisons of work between the two apps need the same boost; an instructions-retired count (simpleperf) would compare them without it.
- **Item reuse on the stress list**: on the device, its per-item effect couldn't be separated from D440's, which moved the main thread to faster cores in the same round ("Item reuse on the device"); an interleaved A/B would. Round 10's 1.0 to 1.5 MB more native heap on the stress list was the first layout's extra items kept for reuse, now disposed (D460); round 13's native heap there is 14.7 MB after start and 17.2 MB after scrolling (round 12: 14.6 and 17.2 MB). In rounds 12 and 13, with the main thread on the mid and big cores again (D500), a prefetched item's composition read 0.169 and 0.163 ms of work, as round 10's 0.167 ms on the same cores.
- **Buffer stuffing at the animation's start**: two of round 12's three feed animation runs of compose-rs's (one in round 10, none in round 11, none of D500's 10 in its A/B) were presented a vsync late throughout, after slow RenderThread frames (5 to 7.6 ms, traced) as the trace started stuffed a buffer; Choreographer then stays in its recovery for the run. No frame is dropped, but gfxinfo's p99 read 11 ms (Compose: 8 ms). In round 13 one run of each app was (Compose's in recovery, compose-rs's with none traced), and compose-rs's p99 was 7 ms in every run: it isn't specific to compose-rs. Whether tracing's start alone sets it off hasn't been looked at.
- **The animation's p99** frames are the system's (vsync requests, the RenderThread's sync), which a `View` pays as Compose does; only drawing without HWUI (option D of [13](13-android-drawing-paths.md), our own Skia) would remove part of them.
- **Smaller items**: caching the display's refresh rate in `prefetch()` (1.9 ms per scroll run); rounded clips through a `Path` (about 15 % of the animation's replay); the kept groups' bookkeeping (D380), a few microseconds in every scrolled frame (the replay's median 77 → 82 µs in its A/B), which listing only changes would cut.

Flings are comparable by construction: compose-rs's fling follows Android's `OverScroller` spline from the release velocity, the curve of Compose's default fling behaviour on Android.
