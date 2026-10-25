# 17. Navigation latency: an expensive screen never seen before

[12](12-android-performance.md) and [15](15-android-stress-benchmark.md) measure scrolling, animations and cold starts. This document measures the latency a user feels when an app that is already running and warm navigates to a new, expensive screen: the tap, then the first frame of a full screen that has never been composed, laid out or drawn, with content no cache has seen. compose-rs (main 598200e with the navigation screens at parity, ed3171c's for `recommended` and `all`) against Jetpack Compose 1.12.1 on a Pixel 8 Pro, Android 16, 2026-10-25 (D602–D605).

**Short answer.** At parity, from the tap to the display showing the new screen, compose-rs takes **82.8 / 88.9 / 108.6 ms** (p50 / p90 / max over 60 traced navigations) against Compose's **96.7 / 107.7 / 118.1 ms**: 14 % less at the median, 17 % at p90. Untraced, the heavy frame from its vsync to its completion (framestats, 300 navigations each) takes 70.1 / 74.9 / 89.6 ms against 82.9 / 91.7 / 112.6 ms.

- **Both apps spend half the frame laying out text**: about 240 paragraphs, all through Android's text stack on the main thread (compose-rs 39.1 ms, Compose 43.1 ms at p50).
- **compose-rs's lead is in drawing and layout**: replaying its display list into render nodes takes 6.2 ms where Compose records its layers in 14.6 ms, and its layout outside text takes 3.1 ms against 9.2 ms.
- **Its composition takes longer in wall time** (21.3 against 16.2 ms) though its main thread does 40 % less work over the frame (32.2 against 53.7 ms of work on the fastest core at its top frequency). The heavy frame starts on a little core in compose-rs: about 9 ms of each composition run there before the scheduler moves the thread, where Compose's frame runs on the mid and big cores from its start.
- **Every optimization on changes nothing here**: `recommended` 82.6 / 90.3 ms, `all` 84.2 / 90.4 ms, against Compose's 99.7 / 107.4 ms in that session. The hint session reports after frames, so the first heavy frame gets no boost from it.
- Every heavy frame misses its deadline in both apps (it takes 8 to 10 vsyncs); no other frame around it is janky.

## The screens

`examples/feed/src/navigation.rs` (compose-rs) and `android/bench-compose/…/Navigation.kt` (Kotlin Compose), widget for widget, exported as `navigation` in both apps (D602):

- **The launcher**: a `Scaffold` with a `TopAppBar` ("Navigation") over a 320 × 240 dp `Button`, centered, reading "Open screen n" and "n − 1 visits". It's light, and the button holds the screen's center, where measure.py taps.
- **The switch**: the button increments a visit number (`seed`) and shows the heavy screen, a plain `if` on a state: no `NavHost`, no transition animation, so the first frame after the tap is the heavy screen's first frame. Back (the back key, through each app's `BackHandler`, or the top bar's back button) shows the launcher again, disposing the heavy screen.
- **The heavy screen**: a `Scaffold` with a `TopAppBar` ("Screen n", a back `IconButton`) over a `Column` with `verticalScroll` of 50 items, all composed, measured and placed at once (not a lazy list), five kinds in turn:
  - **Profile**: an `ElevatedCard` with a 48 dp circular avatar filled with a two-color linear gradient and initials, a name and a handle with a follower count, a like `IconButton`, a paragraph of two or three sentences (several lines, no line limit), and an `AssistChip` with a leading icon, a `FilterChip` and a `SuggestionChip`.
  - **Article**: a `Card` with a title (title large, one or two lines), a byline, an `AnnotatedString` paragraph with bold, italic, colored medium-weight, underlined and highlighted spans, and four icons with counts.
  - **Stats**: an `OutlinedCard` with a label, a large value (headline small) and its change, a `Canvas` bar chart of 12 rounded bars, a determinate `LinearProgressIndicator` (no gap, no stop dot, as the stress screen's), a `Button`, an `OutlinedButton` and a percentage.
  - **Gallery**: a title and an icon over three 120 × 90 dp `Image`s (`ContentScale.Crop`, rounded 12 dp clip; the stress screen's procedural bitmaps, made once) and a two-line caption.
  - **Banner**: a 140 dp box with a 6 dp `shadow` and a rounded 16 dp clip, filled with a three-color gradient, a `Canvas` sparkline (a stroked 24-point `Path`, remembered and reset, and dots), a title and a line of text over it, and two icons in a corner.
- **New content every visit**: every string, count and color comes from the visit's number and the item's index, through the same integer hash in both apps (`mix`, `pick`: lowbias32, unsigned arithmetic in Rust, `Int` with `ushr` in Kotlin; the Rust tests pin its values, and both apps showed the same text on the device). Words are two to four syllables drawn from 24, so names, titles and paragraphs are new words each visit: no paragraph, text layout or word cache (Android's minikin caches words process-wide, for both apps) is warmed by an earlier visit, and no composition is reused. Each item's content is computed once in its composition (`remember(seed, index)` in Kotlin), the same algorithm in both.
- **A marker**: the heavy screen's composition starts with an empty trace section, `navigation: heavy screen` (`android.os.Trace` in Kotlin, compose-rs's tracer in Rust), by which measure.py finds the frame.

The heavy screen holds about 950 words in 241 texts (both apps lay out 241 paragraphs in its first frame); the Pixel 8 Pro shows the top bar and the first four or five items, and about ten screens' worth lie below. At 150 items, the first size tried, Compose's first frame took about 190 ms and compose-rs's 125 ms; 50 puts Compose's in the 30–80 ms range asked for, measured untraced (82.9 ms at the median; its doFrame 85.6 ms traced).

**Fairness** follows [15](15-android-stress-benchmark.md), "What differs, and why it's fair": the same viewport (Kotlin's content inside the scaffold's padding, as compose-rs's view inside the system bars), Compose's best practices (keys, `remember` for the item and its `AnnotatedString`, a remembered `Path`, stable immutable items), both apps compiled `speed`, the light theme in both. Both first frames are **pixel-identical**: `screencap` of each app's heavy screen of visit 1, below the status bar, 0 differing pixels (the scrolled pair differed by its fling's distance only); so are the launcher and the launcher after back ("Open screen 2", proving back works in both). Bounds weren't compared: compose-rs's `uiautomator dump` after the switch still listed the launcher's nodes (see "Surprises").

## Method

measure.py's `navigation` scenario (D603), run only when named (`--scenarios navigation`; `all` keeps the rounds' scenarios):

- **A run**: the app is stopped, started on the launcher (`-e screen navigation`, bench-rs at parity with `-e optimizations none`), left 3 s, then a warm-up navigation (tap, 1 s, back, 1 s) left out of the stats, then 20 navigations (`--navigations`): `dumpsys gfxinfo framestats` (the last frame before the tap), `input tap` at the screen's center, 1 s for the frames to settle, `framestats` again, `input keyevent KEYCODE_BACK`, 1 s.
- **Runs**: `--traced-runs` traced runs, then `--runs` untraced ones, of each subject, interleaved run by run (compose-rs first with `--rotate 1`); before each run our apps are stopped and the thermal guard waits for a cool device (D569, D574, D579), as in the other scenarios.
- **The first frame** of each navigation, in the trace: the main thread's `Choreographer#doFrame` holding the `navigation: heavy screen` section. Its vsync id (in the doFrame's name) gives the RenderThread's `DrawFrames <vsync id>`, FrameTimeline's actual and expected slices, and the GPU completion thread's wait after that `DrawFrames` (before the next one).
- **The input**: the last `deliverInputEvent` on the main thread before that doFrame, the tap's up, whose click switches the screen. The latency runs from its delivery. `input tap` stamps its down and up with the same time and injects the up only once the app has handled the down, so the up's own `eventTimeNano` is the down's; measure.py places it on the trace's clock (CLOCK_MONOTONIC to BOOTTIME by the trace's clock snapshot) and reports it apart, "from the down", which includes the injection of the up after the app handled the down (8.6 ms at the median for compose-rs, 9.6 ms for Compose).
- **Input to present**: from that delivery to FrameTimeline's actual present of the frame (the end of its actual slice: when the display showed it). Also from the input: the doFrame's start and end, the `DrawFrames`' end, the GPU's completion; the frame's lateness (actual present less expected present) and FrameTimeline's jank type.
- **The breakdown** of the doFrame's main thread (`frame_phases`): each slice's own time counts in the phase of its nearest slice, itself or an ancestor, that names one, so the phases sum to the doFrame. Compose 1.12.1 traces `Compose:recompose`, `Compose:applyChanges` and their kin (composition, also when `Scaffold` subcomposes inside a measure pass), `AndroidOwner:onMeasure`, `onLayout` and `measureAndLayout` (layout), `TextStringSimpleNode::measure`, `TextAnnotatedStringNode:measure` and `TextLayout:initLayout` (text layout, the text node's measurement included), and `AndroidOwner:draw` (drawing into its layers' recordings); compose-rs traces `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay`; the view system `Record View#draw()` and `postAndWait` (the main thread waiting for the RenderThread's sync). Also the main thread's states, cores and work (D603: CPU time scaled by each core's capacity and frequency, as [12](12-android-performance.md#method)'s) while the doFrame ran, and the RenderThread's while its `DrawFrames` ran.
- **Untraced**, every run: the heavy frame from framestats, the slowest unflagged frame after the tap, from its intended vsync to its completion (`FrameCompleted`), by phase (the Choreographer's callbacks, which hold compose-rs's whole frame and Compose's recomposition; the traversal, which holds Compose's measure and layout; drawing; the RenderThread). The device didn't report `DisplayPresentTime` for these frames.
- **Reported**: p50, p90 and the max of each, per subject, over the navigations.

What the latency is made of: the wait for the next vsync after the click (4 ms at the median in both apps, at 120 Hz), the doFrame, the RenderThread (sync, GPU commands), the GPU, and the display: the frame is presented about when the GPU finishes, at the next refresh.

## Results

Two invocations of measure.py in one session, every run starting at thermal status 0 with no wait (`--battery-limit 34 --skin-limit 35 --thermal-wait-limit 0 --rotate 1`); `results/2026-10-25-pixel-8-pro-navigation.json` holds both (D604):

- **Parity**: 3 traced and 15 untraced runs of each app, 20 navigations each: 60 traced and 300 untraced navigations a side. Battery 28.4 to 31.8 °C, skin 29.7 to 31.9 °C before the runs.
- **Recommended and all**: `--subject recommended --subject all` beside Compose, 2 traced and 4 untraced runs each (40 and 80 navigations), shortened to free the phone. Battery 27.9 to 28.3 °C, skin 29.4 to 29.7 °C.

### At parity

| p50 / p90 / max, ms | Kotlin Compose | compose-rs |
|---|---|---|
| **Tap (up) to present** | 96.7 / 107.7 / 118.1 | **82.8** / **88.9** / **108.6** |
| From the down (`input tap`'s) | 106.3 / 116.7 / 125.1 | **92.1** / **97.5** / **115.9** |
| doFrame (main thread) | 85.6 / 95.3 / 101.2 | **73.6** / **79.5** / **86.5** |
| DrawFrames (RenderThread) | 5.5 / 8.5 / 35.0 | **4.3** / **5.5** / **33.7** |
| GPU wait after it | 1.8 / 2.2 / 8.2 | 1.7 / 2.1 / 9.3 |
| Main thread work | 53.7 / 56.7 / 58.6 | **32.2** / **33.4** / **38.0** |
| Late (actual less expected present) | 78.3 / 87.6 / 94.5 | **63.9** / **69.7** / **91.5** |
| Untraced: vsync to frame completed (framestats, 300) | 82.9 / 91.7 / 112.6 | **70.1** / **74.9** / **89.6** |

From the tap's up, at the median: the doFrame starts after 4.0 ms in both; the main thread ends at 77.3 ms in compose-rs (88.6 ms in Compose), the RenderThread at 80.9 ms (95.4), the GPU at 82.9 ms (96.5), and the display shows it at 82.8 ms (96.7). Every heavy frame is FrameTimeline's "App Deadline Missed" in both apps (one of Compose's 60 also "Buffer Stuffing"); no other frame from the tap to 100 ms after the present is janky in either.

**The main thread by phase** (p50, ms; the phases sum to the doFrame):

| Phase | Kotlin Compose | compose-rs |
|---|---|---|
| Composition | **16.2** | 21.3 |
| Layout (text apart) | 9.2 | **3.1** |
| Text layout | 43.1 | **39.1** |
| Drawing and recording | 14.6 (`AndroidOwner:draw`) | **6.9** (draw 0.7, serialize 0.2, replay 6.2) |
| Waiting for the RenderThread's sync | 1.8 | 1.5 |
| Other | 0.4 | 0.8 |

**Where the main thread ran** (p50, ms of the doFrame): compose-rs 10.2 on the little cores, 49.4 on the mid ones, 11.3 on the big one; Compose none, 53.8 and 31.1. Its RenderThread ran on the mid cores in both (4.2 and 5.0 ms).

### Every optimization on

| p50 / p90 / max, ms | Kotlin Compose (this session) | compose-rs, recommended | compose-rs, all |
|---|---|---|---|
| Tap (up) to present | 99.7 / 107.4 / 121.1 | **82.6** / **90.3** / 107.2 | 84.2 / 90.4 / **101.1** |
| doFrame | 89.4 / 96.1 / 106.3 | **73.7** / **79.9** / 98.3 | 76.8 / 81.0 / **96.3** |
| DrawFrames | 6.4 / 10.1 / 11.5 | **4.2** / 5.8 / 6.9 | 4.5 / **5.6** / **6.0** |
| Main thread work | 54.7 / 56.9 / 58.6 | **31.8** / **34.8** / **44.2** | 33.8 / 36.6 / 46.0 |
| Untraced: vsync to completed (80) | 81.1 / 90.4 / 107.1 | **69.2** / **74.1** / **80.0** | 72.8 / 78.9 / 89.7 |
| Main thread on little / mid / big cores (p50) | 0 / 55.4 / 30.1 | 11.1 / 51.0 / 10.2 | 8.6 / 52.0 / 12.8 |

Both sets are within the runs' spread of parity (82.8 / 88.9 ms). None of their flags touches a first frame: `performance_hint`'s session reports each frame's CPU time after it, so the first long frame is what earns the boost, for the frames after; `draw_prefetched_ahead` and the prefetch flags work in lazy lists only; `single_drawing_fade` and the flush layer concern faded layers and dropped paths; `reuse_paragraph_at_same_layout_width` relayouts. With `all`, `paragraph_render_nodes` records each paragraph into a render node of its own, which shows as more drawing (framestats' draw 10.3 against 7.0 ms) and replay (7.6 against 6.2 ms).

## Where the time goes

**Text dominates both.** About 240 paragraphs are laid out on the main thread in the frame, by Android's text stack in both apps: compose-rs's `TextMeasurer` through JNI (38 `StaticLayout`s, the single lines `BoringLayout`s, D330, whose `isBoring` with fallback line spacing measures each line's bounds: 795 `calculateBounds`, 5.2 ms, in the slowest frame), Compose's `TextStringSimpleNode` and `TextAnnotatedStringNode` (241 `StaticLayout`s). 39 against 43 ms: compose-rs is 9 % faster on the same text, on slower cores.

**compose-rs's lead is drawing and layout.** Compose records its layers' display lists on the main thread (`AndroidOwner:draw`, 14.6 ms, on the big core) and walks its layout nodes' modifier chains in measure and placement (9.2 ms, Scaffold's subcomposition apart). compose-rs draws into its own display list (0.7 ms), serializes it (0.2 ms) and replays it into render nodes in Kotlin (6.2 ms), and lays out its nodes in 3.1 ms. Of the doFrame's 12.0 ms lead at the median, drawing and layout give 13.8 ms and text 4.0 ms; composition takes 5.1 ms back.

**compose-rs's composition takes longer in wall time, for less work.** 21.3 against 16.2 ms. Compose's composition here is mostly `Scaffold`'s subcomposition inside its measure pass, on the mid and big cores. compose-rs's composition is the frame's first phase, and the frame starts on a little core: about 9 ms of each composition ran on a little core (the slices' running time, from the traces), then the scheduler moved the thread to a mid core. Its main thread does 40 % less work over the whole frame (32.2 against 53.7 ms), but runs 10 ms of it on the little cores and 11 on the big one, where Compose's runs none on the little cores and 31 ms on the big one. The likely cause is the scheduler, which places a thread by its recent utilization: compose-rs's main thread had little to do on the launcher and in the tap's handling, and was still on a little core when the heavy frame began; Compose's, which does more per input and per frame, had been placed on faster cores already. (The traces kept per navigation, in the results, hold the cores of the doFrame only; what ran on the main thread before it wasn't recorded.)

**The RenderThread and the GPU are a small part** in both: 4 to 6 ms of `DrawFrames` (both on mid cores) and under 2 ms of GPU after it, for a frame whose display lists are all new.

## Opportunities

Recorded in [14](14-performance-opportunities.md), "Android: a new screen's first frame":

- **Start a heavy frame on fast cores.** compose-rs knows, before the frame, that it composes a lot (a state change invalidating a large group, or content inserted). An ADPF load-up hint for that frame (`APerformanceHint_notifyWorkloadIncrease` on API 36, or `sendHint(CPU_LOAD_UP)` on its session) could move the main thread off the little core before composition; worth up to the 9 ms composition spends there, and a few more of the frame's.
- **Text off the main thread or ahead of it.** Half of both frames is paragraph layout. compose-rs measures text through an interface it owns (`TextMeasurer`), so it could lay out a screen's paragraphs in parallel on worker threads (Android builds `StaticLayout`s and `PrecomputedText` off the main thread), which Compose's `BasicText` doesn't do.
- **`BoringLayout`'s bounds**: 5 ms of `calculateBounds` for compose-rs's single lines; `isBoring` without fallback line spacing (when the font needs none) would skip them.

## Surprises

- The composition's wall time misleads: compose-rs's is slower here for less work, because of where the scheduler put the thread, and the cause is the light input handling before it.
- Neither optimization set moves the first frame; the hint session can only help the frames after.
- compose-rs's accessibility tree went stale across the switch: `uiautomator dump` after the tap still listed the launcher's nodes ("Open screen 1"), on a screen showing the heavy one. compose-rs sends a semantics change only when an accessibility service is enabled at the frame (`ComposeRsView.frame`), and `uiautomator` connects only for its dump, so it likely read the tree cached before. A service that connects later reads stale nodes until the next change; not fixed here.

## Headless

`benchmarks/benches/ui.rs`, `navigation: the heavy screen's first frame and serialize` (D605): a `UiHost` the size of the Pixel 8 Pro's screen (1008 × 2244 px at 2.25) at parity, the launcher shown; each iteration taps the button, times the frame composing, laying out and drawing the heavy screen of a new visit and what Android's bridge does after it (`write_display_list`, the resources collected and taken), then goes back untimed. Text is measured by the fixed measurer, so it times compose-rs's own work without Android's text layout. On an M-series Mac: 1.25 ms (criterion's median; p99 1.65 ms over 30 frames). On the device the same work, text apart, is 25.3 ms of the frame's wall time at the median (composition, layout, draw, serialize), 31.5 ms with the Kotlin replay.

```bash
cargo bench -p compose-benchmarks --bench ui -- navigation
python3 android/bench/measure.py --trace-processor TP --out OUT --scenarios navigation --runs 15 --rotate 1 \
    --battery-limit 34 --skin-limit 35 --thermal-wait-limit 0
```
