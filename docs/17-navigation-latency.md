# 17. Navigation latency: an expensive screen never seen before

[12](12-android-performance.md) and [15](15-android-stress-benchmark.md) measure scrolling, animations and cold starts. This document measures the latency a user feels when an app that is already running and warm navigates to a new, expensive screen: the tap, then the first frame of a full screen that has never been composed, laid out or drawn, with content no cache has seen. compose-rs against Jetpack Compose 1.12.1 on a Pixel 8 Pro (D602–D60x).

RESULTS_SUMMARY

## The screens

`examples/feed/src/navigation.rs` (compose-rs) and `android/bench-compose/…/Navigation.kt` (Kotlin Compose), widget for widget, exported as `navigation` in both apps:

- **The launcher**: a `Scaffold` with a `TopAppBar` ("Navigation") over a 320 × 240 dp `Button`, centered, reading "Open screen n" and "n − 1 visits". It's light: the frames before the tap cost little in either app.
- **The switch**: the button increments a visit number (`seed`) and shows the heavy screen, a plain `if` on a state: no `NavHost`, no transition animation, so the first frame after the tap is the heavy screen's first frame. Back (the back key, through each app's `BackHandler`, or the top bar's back button) shows the launcher again, disposing the heavy screen.
- **The heavy screen**: a `Scaffold` with a `TopAppBar` ("Screen n", a back `IconButton`) over a `Column` with `verticalScroll` of 150 items, all composed, measured and placed at once (not a lazy list), five kinds in turn:
  - **Profile**: an `ElevatedCard` with a 48 dp circular avatar filled with a two-color linear gradient and initials, a name and a handle with a follower count, a like `IconButton`, a paragraph of two or three sentences (several lines, no line limit), and an `AssistChip` with a leading icon, a `FilterChip` and a `SuggestionChip`.
  - **Article**: a `Card` with a title (title large, one or two lines), a byline, an `AnnotatedString` paragraph with bold, italic, colored medium-weight, underlined and highlighted spans, and four icons with counts.
  - **Stats**: an `OutlinedCard` with a label, a large value (headline small) and its change, a `Canvas` bar chart of 12 rounded bars, a determinate `LinearProgressIndicator` (no gap, no stop dot, as the stress screen's), a `Button`, an `OutlinedButton` and a percentage.
  - **Gallery**: a title and an icon over three 120 × 90 dp `Image`s (`ContentScale.Crop`, rounded 12 dp clip; the stress screen's procedural bitmaps, made once) and a two-line caption.
  - **Banner**: a 140 dp box with a 6 dp `shadow` and a rounded 16 dp clip, filled with a three-color gradient, a `Canvas` sparkline (a stroked 24-point `Path`, remembered and reset, and dots), a title and a line of text over it, and two icons in a corner.
- **New content every visit**: every string, count and color comes from the visit's number and the item's index, through the same integer hash in both apps (`mix`, `pick`: lowbias32, unsigned arithmetic in Rust, `Int` with `ushr` in Kotlin; the Rust tests pin its values). Words are two to four syllables drawn from 24, so names, titles and paragraphs are new words each visit: no paragraph, text layout or word cache (Android's minikin caches words, process-wide, for both apps) is warmed by an earlier visit, and no composition is reused. Each item's content is computed once in its composition (`remember(seed, index)` in Kotlin), the same algorithm in both.
- **A marker**: the heavy screen's composition starts with an empty trace section, `navigation: heavy screen` (`android.os.Trace` in Kotlin, compose-rs's tracer in Rust), by which measure.py finds the frame.

Content: 30 of each kind, about 1,600 words, 450 texts and 150 paragraphs of several lines; on the Pixel 8 Pro, the screen shows the top bar and the first four or five items, and about 33 screens' worth lie below.

FIDELITY

## Method

measure.py's `navigation` scenario (D603), run only when named (`--scenarios navigation`; `all` keeps the rounds' scenarios):

- **A run**: the app is stopped, started on the launcher (`-e screen navigation`, bench-rs at parity with `-e optimizations none`), left 3 s, then a warm-up navigation (tap, 1 s, back, 1 s) left out of the stats, then 20 navigations (`--navigations`): `dumpsys gfxinfo framestats` (the last frame before the tap), `input tap` at the screen's center (the launcher's button), 1 s for the frames to settle, `framestats` again, `input keyevent KEYCODE_BACK`, 1 s.
- **Runs**: 3 traced runs (`--traced-runs`), then `--runs` untraced ones, of each app, interleaved run by run (compose-rs first with `--rotate 1`); before each run our apps are stopped and the thermal guard waits for a cool device (D569, D574, D579), as the other scenarios.
- **The first frame** of each navigation, in the trace: the main thread's `Choreographer#doFrame` holding the `navigation: heavy screen` section. Its vsync id (in the doFrame's name) gives the RenderThread's `DrawFrames <vsync id>`, FrameTimeline's actual and expected slices, and the GPU completion thread's wait after that `DrawFrames`.
- **The input**: the last `deliverInputEvent` on the main thread before that doFrame (the tap's up; the click runs on it), placed at the event's own time (its name's `eventTimeNano`, CLOCK_MONOTONIC, converted to the trace's BOOTTIME by the trace's clock snapshot), so the latency includes the system's dispatch to the app.
- **Input to present**: from that event to FrameTimeline's actual present of the frame (the end of its actual slice: when the display showed it). Also from the input: the doFrame's start (the wait for the next vsync), its end, the `DrawFrames`' end, the GPU's completion; and the frame's lateness (actual present less expected present) and FrameTimeline's jank type.
- **The breakdown** of the doFrame's main thread (`frame_phases`): each slice's own time counts in the phase of its nearest slice, itself or an ancestor, that names one, so the phases sum to the doFrame. Compose 1.12.1 traces `Compose:recompose` and `Compose:applyChanges` (composition, also when subcomposing inside a measure pass, as `Scaffold` does), `AndroidOwner:measureAndLayout` (layout, text included: Compose doesn't trace text apart), `AndroidOwner:draw` (drawing into its layers' recordings); compose-rs traces `compose-rs: recompose`, `layout`, `text layout`, `draw`, `serialize` and `replay`; the view system `Record View#draw()` and `postAndWait` (the main thread waiting for the RenderThread's sync). Also the main thread's states and cores while the doFrame ran, and the RenderThread's cores while its `DrawFrames` ran.
- **Untraced**, every run (traced or not): the heavy frame from framestats, the slowest unflagged frame after the tap's last one, from its intended vsync to its completion (`FrameCompleted`), the GPU's and the present's, and by phase (the Choreographer's callbacks, the traversal, drawing, the RenderThread).
- **Reported**: p50, p90 and the max of each, per app, over the navigations.

What the latency is made of: up to one vsync (8.3 ms at 120 Hz) between the tap and the next frame's start, by chance; the doFrame (compose, lay out, draw, record); the RenderThread (sync, GPU commands); the GPU; and the wait for the next display refresh after the buffer is queued (SurfaceFlinger latches it at a vsync and presents it at the next).

RESULTS

## Headless

`benchmarks/benches/ui.rs`, `navigation: the heavy screen's first frame and serialize` (D605): a `UiHost` the size of the Pixel 8 Pro's screen (1008 × 2244 px at 2.25) at parity, the launcher shown; each iteration taps the button, times the frame composing, laying out and drawing the heavy screen of a new visit and what Android's bridge does after it (`write_display_list`, the resources collected and taken), then goes back untimed. Text is measured by the fixed measurer, so it times compose-rs's own work without Android's text layout. On an M-series Mac: HEADLESS.

```bash
cargo bench -p compose-benchmarks --bench ui -- navigation
```
