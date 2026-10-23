# 14. Performance opportunities: what's left, measured

Where compose-rs can still get faster, smaller or quicker to build, after round 3 of [12](12-android-performance.md). Each opportunity was measured with a prototype where feasible, or estimated from profiles with the reasoning shown. Small and negative results are reported too. This began as an analysis: #2, #3, #10 and the prefetched paragraphs have since been done (D320–D323), each section saying so, and three jank problems round 4's traces showed were fixed (D340–D342, "Jank" below). Since then, the build and benchmark fixes below were made: stripping (#4a, D301), the release profile (#4b, D361) and the Baseline Profile (#8, D360), the stale library (D300), benchmark hygiene (D302), build-only dependencies at opt-level 0 (#6, D303), and the corrections to docs/08 (D304) and docs/12; each is marked **Fixed** where it's discussed. The prototypes are kept on `perf/*` branches, each with its raw measurements and a `README.txt` of exact commands under `perf-results/<branch>/` (see "Branches" below).

**Short answer.**
- **Desktop** has the one big gain left. A Metal surface takes 70 % off the app's CPU time per animated frame in a 2560 × 1324 window (6.8 → 2.0 ms), and 37 % off the app's and the window server's together (10.0 → 6.3 ms a frame). At that size, a click's time to present goes from 16.4 ms (median of 3; 7.0–16.9) to 1.3 ms. Today's CPU path spends most of each frame not rasterizing but presenting: Core Animation converts the frame's colour space on the CPU.
- **Android** has several cheap gains, each a few percent to 10 % of a frame or startup metric, and no big one:
  - a round cap on the spinner: −8 % of the animation's RenderThread work (done: D320);
  - a replay without interpreted framework calls: −8 % of its main-thread work (done: D321);
  - two runtime fixes that remove a quadratic scan when composing big screens: −39 % of the big screen's first frame headless;
  - stripping the library, which the build silently skipped: the APK goes from 3.2 to 2.0 MB at no cost (**fixed**: D301); with a release profile (LTO, `panic = "abort"`), 1.5 MB (**done**: D361, 1.54 MB with profileinstaller).
  - **Together**, measured with the full benchmark ("Combined" below; the two sessions ran one after the other, not interleaved): the animation's main-thread work goes from 210 to 196 ms and its RenderThread work from 547 to 492 ms (Compose: 248–260 and 534–544). `big` starts in 166 ms instead of 181 (Compose: 222–224), and the APK is 1.5 MB. An interleaved A/B of the frame changes alone (#2, #3 and the spinner boundary) agrees: 209.7 → 194.0 ms and 546.6 → 496.5 ms.
- **Text** on Android is 84 % platform work that any approach pays. Batching JNI calls would save at most 2.6 ms of the big screen's 110 ms first frame. Laying out single lines with `BoringLayout`, as Compose does (**done**: D330), takes 29 % off text layout: the big screen's first `doFrame` goes from 104.8 to 90.3 ms.
- **Build times:** the edit loop is at its floor (about 0.45 s). Clean debug builds can lose 27 % (**fixed**: D303).

## Ranking

Gain over cost, best first.
- **"Measured"** means a prototype measured against main (9f80586) on the same device or machine, alternated run by run.
- **Averages:** Android A/B results are means over the runs, with [min–max]. The full benchmark's, the desktop's (3 runs) and the build times are medians, as in [12](12-android-performance.md). The runtime's are medians of round medians.
- **"Estimated"** means from profiles, with the reasoning in the section named.
- **Sizes** are decimal megabytes: main's APK is 3.24 MB here and "3.1 MB" (MiB) in docs/12.

| # | Opportunity | Gain (metric) | Evidence | Cost, risk | Behaviour or API change | Branch |
|---|---|---|---|---|---|---|
| 1 | **Desktop: GPU surface** (Skia on Metal; D3D or GL elsewhere) | App CPU per animated frame at 2560 × 1324: 6.79 → 2.04 ms (−70 %); click → present 16.4 [7.0–16.9] → 1.30 [1.14–1.55] ms. WindowServer's CPU per frame rises 3.2 → 4.2 ms, so app and WindowServer together go 10.0 → 6.3 ms (−37 %) | measured, 3 runs each (medians) | M per platform: a raster fallback, resizing, device loss, unsafe code in compose-desktop; the prototype has no colour management | none in the API; colours need the layer's colour space set (cost not measured); its lower frame rate (122 against 142 fps) is unexplained | `perf/desktop-metal` |
| 2 | **Android: the spinner's round cap** (`IndeterminateCircularProgressIndicator`, as Material 3 1.4). **Done** (D320) | Animation RenderThread work −44 ms (544.4 → 500.1, −8.1 %); as shipped, −51 ms (−9.4 %), RenderThread p99 5.64 → 5.20 ms | measured, 3 runs; as shipped, 4 | S, none | the default cap's look, on every platform: every indicator's default is `Round`, as Material 3 1.4's (determinate and linear too) | `perf/frame-roundcap` |
| 3 | **Android: replay without interpreted calls** (floats as int bits, one reused buffer, sizes tracked in Kotlin, no host wake for frame awaiters). **Done** (D321) | Animation main work −16.8 ms (213.9 → 197.1, −7.9 %); scroll −12 ms (−2.6 %, ranges overlap). As shipped: −11 ms (−5.2 %) and −20 ms (−4.1 %); animation main p50 1.37 → 1.26 ms | measured, 3 runs, on a build whose JIT compiles no framework methods (smaller where it does); as shipped, 4 | S, low | internal JNI signatures; the executor skips the host wake for `dispatch_frame`'s wakes on every host (per executor, reset by a guard, also on a waker's panic) | `perf/frame-replay` |
| 4 | **Android: stripping, then a release profile.** (a) AGP strips the library once `ndkVersion` is set (**fixed**: D301); (b) fat LTO, one codegen unit, `panic = "abort"` (**done**: D361, release variants only; debug ones abort too, without LTO) | (a) APK 3.24 → 1.99 MB (−39 %); (b) → 1.50 MB (−54 % in all, below Compose's 1.74 MB), code PSS −0.3 MB, Rust frame section −4 to −8 %, main work −3 to −4 % (about the noise); startup unchanged. As shipped: library 1.89 → 1.40 MB, APK 2.03 → 1.54 MB; Rust frame section −1.5 to −3 %, main work and frame durations unchanged | measured; as shipped, 3 runs | (a) none, but `ndkVersion` pins one NDK every developer needs. (b) S; release builds only: clean 4.7 → 7.4 s, a material3 edit rebuilds the library in 5.1 s instead of 2.1 (debug builds: 4.4 and 1.8 s) | (b) more panics abort the process: those of cancelled background tasks, and in drops and wakes on the pool's threads; Android only | `perf/startup-release-profile` |
| 5 | **Runtime: state readers and composition locals** (a read searches a cell's 8 latest readers; a local read walks provider groups only) | Headless big-screen first frame −39 % (7.04 → 4.32 ms), composition −44 %; feed first frame −21 %. On the device, with #2–#4 and #7 (one traced start): big composition 22.1 → 16.5 ms | measured headless, 3–5 rounds | S/M, low | none (the prototype's copies of readers are now compacted: D310) | `perf/runtime-compose`; made production-ready as D310 |
| 6 | **Build: build-only dependencies at opt-level 0** in the dev profile (**fixed**: D303) | Clean debug build of todo 11.8 → 8.6 s (median; −27 %, means −24 %), workspace `test --no-run` 16.1 → 12.6 s (−22 %); edit loop unchanged | measured, 3 runs | S (Cargo.toml; the list needs upkeep) | none | `perf/build-dev-profile` |
| 7 | **Android: startup warm-ups** (an ALPHA_8 bitmap's texture upload, and two StaticLayouts on a thread, when the first `ComposeRsView` is made) | TotalTime feed −4.0 ms (se 1.1, −4 %); big −2.2 ms (se 1.2, −1 %, not significant); the first `doFrame` 26.0 → 21.5 ms | measured, n = 36 | S; `activityResume` grows by about 1 ms | none | `perf/startup-text-warmup` |
| 8 | **Android: a Baseline Profile for the Kotlin glue**. **Done** (D360) | For installs that aren't fully compiled (Play), at most: replay +25–30 % and main work +5 % recovered; startup unchanged. As shipped (`verify` → `speed-profile` from the profile, `speed`): replay 155 → 99 µs (93) animating, 172 → 110 µs (108) scrolling; main work −5.6 % and −5.8 %, within 2 % of `speed`; main p90 1.86 → 1.76 ms (1.61) animating | measured (`-m verify` against `speed`); as shipped, 3 runs | S | none; apps that aren't installed from Play need androidx.profileinstaller for it | `perf/startup-compilation-modes` |
| 9 | **Desktop: present without the colour conversion** (tag the image with the window's colour space; properly, an IOSurface-backed layer) | Present 5.37 → 0.09 ms at 2560 × 1324; app CPU per frame 6.79 → 3.64 ms | measured (tagging, with the alpha pass also off), estimated (IOSurface) | S–M (a softbuffer fork or our own layer code); with #1, it matters mainly as the raster fallback | tagging turns off colour management, so it can't ship as measured | `perf/desktop-present` |
| 10 | **Android: records that reference nested groups by key**, so a parent's version doesn't change with a nested group's content. **Done** (D323) | Animation main work −5 to −10 ms (−3 to −5 %) on top of #3; as shipped (with the indeterminate indicators as repaint boundaries), −6.6 ms (−3.3 %), main p50 / p90 −6 / −7 %, p99 unchanged | estimated ("Nested groups"); as shipped, measured, 4 runs | M–L, medium | none (the indeterminate indicators are repaint boundaries of their own) | — |
| 11 | **Android: text widths and paragraph recording on worker threads** | Big first frame main work 94.0 → 79.0 ms (−16 %), TotalTime 182 → 174 ms; scroll main work −19 ms, p99 3.53 → 3.08 ms; costs 50 ms (big) and 440 ms (scroll) of worker CPU | measured, n = 10 and 4 | M–L, threading risk; the prototype isn't production-ready (see "Android: text and the big screen") | none | `perf/text-combined` |
| 12 | **Android: fewer Java allocations per frame** | Per animation frame 17.2 → 4.2 Java objects; −0.48 MB allocated over start and 10 flings; Java heap PSS after flings 5.40 → 4.86 MB (single runs); no frame-time gain | measured | S | none | `perf/startup-java-allocs` |
| 14 | **Android: `BoringLayout` for single lines** (Compose's `TextLayout` choice). **Done** (D330) | Big first frame: text layout 41.3 → 29.3 ms (−29 %), `doFrame` 104.8 → 90.3 ms (−14 %), TotalTime 167 → 158 ms; scroll main work 460 → 431 ms (−6.4 %), frame percentiles unchanged | measured, interleaved: 12 traced and 18 untraced starts, 4 scroll runs each | S, low (checked against `StaticLayout` on API 36 only) | letter-spaced texts at their own width are 1 px wider, as Compose's | — |
| 15 | **Android: a prefetched item's groups drawn and recorded ahead** (D350). **Done** | Revealing frames' `doFrame` 1.80 → 1.53 ms (replay 0.31 → 0.11 ms); scroll main p90 1.94 → 1.86 ms (−4 %), p99 and work within the spread | measured, interleaved, 10 runs each | M, low (a key and version are the same bytes either way; render nodes recorded ahead are kept apart, dropped together) | none (`UiHost::draw_ahead` for platforms recording groups) | — |
| 16 | **Android: kept items' groups kept recorded** (D380). **Done** | The slowest replay in a reversal's first 100 ms 1.17 → 0.18 ms, its slowest `doFrame` 3.37 → 2.96 ms (p90 4.65 → 3.70); percentiles and work within the spread | measured, interleaved, 12 runs each | S–M, low (a node at alpha 0 keeps HWUI from dropping their recordings; 8 items at most) | none | — |
| 17 | **Android: an idle time's slow step slows the next ones' estimates** (D381). **Done** | Idle-time slices past the vsync 12.4 → 6.9 a run, more than 1.2 ms past 1.67 → 0.92; `doFrame`s delayed by our work 5 → 3 in 12 runs | measured, interleaved, 12 runs each | S, low (replaying D375's traces, 14 % of measurements wait for the next idle time instead of 2.6 %; on the device, as many measurements a run, none in frames) | none | — |
| 18 | **Android: a performance hint session of our own for the RenderThread** (D390). **Done**, on the user's decision | Scroll RenderThread p50 / p99 2.70 → 2.50 / 5.36 → 4.92 ms (Compose: 2.19 / 5.03); frames unboosted 93 → 62 % (Compose 49 %); CPU-rail power +2.1 % scrolling, +2.3 % animating (not resolved by 6 runs), all rails +0.4 %; main-thread work +3 % | measured, interleaved with main and Compose, 6 runs each; targets swept 3.5–7.5 ms | S, low (API 33+, looked up at run time; nothing below) | `ComposeRsView.isPerformanceHintEnabled` (on by default) | — |
| 19 | **Lazy item reuse by content type** (Compose's `ReusableContent` and `LazyLayoutItemReusePolicy`, D430–D433; grids item by item, D450–D457). **Done** | Headless, an item scrolled into view: feed 41.0 → 34.5 µs (−16 %), stress list 21.3 → 19.1 µs (−10 %, its carousels' tiles reused too since D434); frames composing nothing −12 % (feed), unchanged (stress); the 100,000-item list's frame 17.8 → 16.1 µs. Lazy grids reuse item by item since D450, as Compose's (they composed rows, reused among themselves): on the bench's photo grid (3 columns of clickable tiles, no content types; min of 5, interleaved against main f15537d), a tile scrolled into view 11.0 → 11.6 µs (+5 %), a 200 px frame 17.6 → 17.9 µs (+2 %), a pixel's frame 7.9 → 6.2 µs (−22 %). The first version cost +29 %, nearly all avoidable: it composed and placed a viewport's worth of lines beyond each side (about 22 lines, 66 tiles, where 7 lines show), and walked every size for each average; two lines beyond and running sums (D456) leave three times the lazy items of a row's grid, not Compose's cost (its grid places only what it measures). Lists gained the running sums too: feed 34.8 → 34.0 µs an item, its pixel frame 2.42 → 2.13 µs; stress 18.9 → 18.6 µs, 3.50 → 3.33 µs. On the device (round 10 of [12](12-android-performance.md), against round 9's traces): a prefetched feed post's composition 0.096 → 0.086 ms of work (−10 %), paragraphs laid out while prefetching −13 % (stress list −48 %), all prefetch work −4 % on the feed; on the stress list not separable from D440's faster cores (an item's composition 0.163 → 0.167 ms of work, on cores where the metric reads about a fifth higher); feed memory unchanged, stress list native heap +1.0 to +1.5 MB: the first layout's extra items kept for reuse, now disposed (D460, "Android: memory") | measured headless (`benches/lazy.rs`), interleaved, min of 5; on the device, round 10 against round 9 (not interleaved) | M, medium (the runtime's reuse; modifier nodes reset by `on_reset` rather than attached anew) | `_with` forms take content types (grids' `_with_span` forms spans too, D454); `SaveableStateProvider` matches by position (a `ReusableContent`) | — |
| 20 | **Android: the main thread in our hint session** (D440). **Done** | Stress scroll main thread p50 / p90 / p99 1.54 / 2.24 / 3.53 → 1.07 / 1.71 / 2.97 ms, on the little cores 79 → 28 % of its time, `doFrame`s delayed by prefetch steps 58 → 4 in 12 runs; feed scroll 1.32 / 1.86 / 2.88 → 1.24 / 1.75 / 2.70 ms; RenderThread and votes unchanged; CPU-rail power −1.8 to +1.0 % (± 1.8 to 3.7, not resolved). Round 10 of [12](12-android-performance.md), beside Compose: stress list main p50 / p99 1.02 / 2.86 ms against Compose's 1.29 / 7.76 ms, on the little cores 21 % of its time; no compositor-side janky frame in its 12 runs. Round 11, after D470 cut the RenderThread's work and with it the vote (190 against 295): 1.08 / 2.75 ms against Compose's 1.25 / 7.61 ms, on the little cores 46 % of its time; since D500 (#23) the session reports both threads' work against 8.33 ms, and in its A/B the list's main thread ran 7 % of its time there | measured, interleaved, 12 runs each of four scenarios, against three other variants | S, low | none | — |
| 21 | **A first layout that composes only what it lays out** (D472). **Done** | Knowing fewer than 4 item sizes, a measurement lacking items asks for 4 at most (or as many as it placed), the frame's next round for the rest: the stress list's first frame composes 16 items where it composed 87 (rows and carousels' tiles, 38 of which stayed), in 3 rounds where it took 2. On the Pixel 8 Pro, with D470 (interleaved, 12 traced and 24 untraced starts each): its first frame's `compose-rs: frame` 14.5 → 7.2 ms, paragraphs laid out 69 → 29, `TotalTime` 125.5 → 118.5 ms; the feed's 6.7 → 5.1 ms and 108.5 → 104.0 ms; big unchanged. Headless, the stress list's Rust heap peak at start 6,677 → 2,288 KiB (live 5,434 → 2,268 KiB). Round 11 of [12](12-android-performance.md), beside Compose: first frame's `compose-rs: frame` 13.8 → 6.6 ms, `TotalTime` 126 → 118 ms (Compose: 143 ms) against round 10 | measured, interleaved; headless | S, low | none | — |
| 22 | **Lazy lists place only the items in view**, prefetching from the first layout on (D470), with focus searches laying items out beyond the bounds on demand (D477) and scrolls not cut at an estimated end (D476); and two fixes that let prefetch run after frames it skipped (D471, D474). **Done** | Stress scroll: main thread work 681 → 591 ms (−13 %), RenderThread work 1,556 → 1,333 ms (−14 %), main thread p50 / p90 / p99 1.06 / 1.68 / 2.86 → 0.98 / 1.53 / 2.70 ms, RenderThread 2.56 / 3.63 / 4.98 → 2.51 / 3.56 / 5.09 ms (p99 within the spread), native heap after the flings 20.5 → 17.3 MB, PSS 95.6 → 91.4 MB; feed: work −3 % and −5 %, main thread 1.23 / 1.76 / 2.72 → 1.16 / 1.68 / 2.73 ms, native heap 11.8 → 10.6 MB; no janky frame of the app's in either, no more text laid out in frames (none on the feed, 1 in 12 runs on the stress list). A viewport's worth beyond and two items beyond, measured in the same session, did no better on any percentile, and composed no less in frames. A grid's fast flings composed in-frame in 1.0 frames a run against 4.3 (D474). Round 11 of [12](12-android-performance.md) against round 10: stress list main / RenderThread work 713 / 1,593 → 573 / 1,311 ms (Compose: 1,194 / 1,594 ms), native heap after the flings 22.3 → 17.2 MB, no text laid out in a frame on either list; feed RenderThread p50 2.35 → 2.10 ms (Compose 2.34). With less work our session votes less (stress list 295 → 190), and that list's RenderThread p90 / p99 came out longer than Compose's (3.81 / 5.45 against 3.63 / 5.27 ms), though shorter at each boost level | measured, interleaved, 12 runs each of 2 and of 4 variants ([15](15-android-stress-benchmark.md), "The lazy list's policy") | S, low | focus moves as before, a frame later past the rows in view (D477); scroll semantics for accessibility (D478) | — |
| 23 | **Android: both threads' work in our hint session, against one 120 Hz interval** (D500). **Done** | Feed scroll RenderThread p50 / p90 / p99 2.33 / 3.23 / 4.77 → 1.46 / 2.27 / 3.29 ms, main thread 1.15 / 1.64 / 2.74 → 0.81 / 1.28 / 2.20 ms; stress scroll RenderThread 2.49 / 3.59 / 5.15 → 2.06 / 2.89 / 3.93 ms (round 11's Compose: 2.63 / 3.63 / 5.27), main thread 0.99 / 1.56 / 2.65 → 0.83 / 1.27 / 2.60 ms; animations unchanged; the vote follows the load (53 animating the feed, 261 scrolling it, 332 on the stress list). CPU-rail power +1.8 % (± 2.5) and +2.9 % (± 2.7) scrolling, +0.7 % and −1.1 % animating; all rails within +0.6 %; `doFrame`s starting more than 2 ms late 89 → 149 and 85 → 187 in 10 runs, none delayed by our work | measured, interleaved, 10 runs each of four scenarios, against 6.0 and 5.5 ms targets; modelled closed loop first | S, low | none | — |
| 13 | **Memory: exact or chunked growth** of the slot table's groups and the node arena | Big screen's Rust heap −1.7 MB allocated of 17.9 (unused capacity, so maybe not resident: the PSS saving is unmeasured; its PSS is 69.8 MB); no transient 2× copy when growing | estimated (heapprofd) | S–M | none | — |

Small, negative or not worth doing: see "What isn't worth doing".

## Method

- **Android:** Pixel 8 Pro, Android 16 (`user` build), apps compiled with `cmd package compile -m speed -f` unless stated. Each prototype was A/B tested against main (9f80586), with APKs alternated run by run, each installed APK checked by SHA-1, and each APK's packaged library checked (measure.py now does both: D302).
  - **Work metric:** CPU time scaled by each core's frequency and capacity ([12](12-android-performance.md), "Method"), from Perfetto traces, plus `compose-rs:` section averages.
  - **Scenarios:** measure.py's: 10 s of the animated feed untouched, 10 flings, cold starts.
  - **Profiles:** simpleperf, symbolized with the unstripped library.
- **Headless:** Apple M4 Pro (14 cores), criterion and a probe binary with a counting allocator (`perf/runtime-probe`). Timing runs held a machine-wide lock that also waited for builds to finish. One stray ~30 s build overlapped the runtime investigation's first baseline run (not used) and ~9 s of the desktop runs (not enough to change their averages).
- **Desktop:** a 2560 × 1440 external display at scale 1 (no Retina display), 144 Hz. Real windows up to 2560 × 1324 px; 2× and 4K from a headless renderer bench.
- **Noise:** run-to-run work varies by ±3–4 % (scroll more); TotalTime's standard deviation is 3–7 ms, so startup effects under ~2 ms needed n ≥ 36. Some single-trace figures (a first frame's phases, the system's share of a frame) are marked as such.

## Android: the frame path

### Where a frame's time goes (main 9f80586)

Work per 10 s of the animated feed, or per 10 flings. Sections are `compose-rs:` and system Perfetto slices; time is attributed to the deepest slice (`perf/frame-replay`, `perf-results/…/an-*.txt`).

| Main thread | Animation | Scroll |
|---|---|---|
| Total work | 209–214 ms (Compose 250–266) | 448–469 ms (Compose 842) |
| Rust frame (tasks, recompose, layout, draw, serialize) | 34 (16 %): tasks 10, draw 9.5, serialize 4.3, layout 1.8, recompose 1 | 58: layout 15, tasks 13, draw 11, serialize 6 |
| JNI and the Kotlin replay | 30 (14 %) + take drawing 3 | 47 |
| HWUI recording around `onDraw` (self) | 11 | — |
| Prefetch (compose and lay out the next item when idle) | — | 105 (23 %), of which text layout 61 |
| System (vsync request, RenderThread sync and wake, Looper, traversal) | about 130 (61 %), the same in Compose | about 165: vsync requests 40, RenderThread sync 34, input 17, Looper 75 |

| RenderThread | Animation | Scroll |
|---|---|---|
| Total work | 535–557 ms | 760–791 ms (Compose 862–872) |
| `queueBuffer`, dequeue (the window's, the same in Compose) | about 250 | |
| Drawing (Skia, GPU command flush 196) | about 450 inclusive | |
| `syncFrameState` | 17.6 (Compose 23.5) | 32.7 (Compose 45) |

So about 60 % of the animation's main thread is the system's per-frame cost, which a `View` can't avoid and Compose pays equally: the binder `requestNextVsync` triggered by `invalidate()` (26 ms), waking and waiting for the RenderThread (22 ms), Looper and epoll (44 ms). compose-rs's own part (Rust, replay, HWUI recording) is about 78 ms of the 210.

**JNI crossings** are few: about 6 per animation frame (`frame()`, `takeDrawing()`, `tick()` from a posted message, one `wake()` upcall per frame for the tasks waiting for the next (one per awaiter until D421), the `NewDirectByteBuffer` constructor). Scrolling adds `touch()` per input sample, `requestFrame` upcalls, one `prefetch()` per idle slot with about 6 text-layout upcalls per item, and release upcalls. Their cost is small: text's JNI overhead is about 5 % of text layout.

**Render nodes** show no extra cost: the RenderThread's sync is cheaper than Compose's in both scenarios (17.6 against 23.5 ms; scrolling, from one trace each, 32.7 against 45 ms). Compose's scroll figure (842 ms) is also one trace.

### The animation's RenderThread: a square cap (#2)

docs/12 found the animation's RenderThread work "even with Compose's" (540 against 535 ms). Most of that is one default:
- compose-rs's `IndeterminateCircularProgressIndicator` (crates/compose-material3/src/progress.rs) defaults to `StrokeCap::Square`, as older Material 3 did.
- The Kotlin twin's Material 3 1.4.0 (checked with `javap`) uses `StrokeCap.Round`.
- Skia's `CircleOp` doesn't take stroked arcs with square caps, so compose-rs's arc becomes a stroked path every frame. That costs:
  - path building (11 against 1.8 sampled ms);
  - two `PathStencilCoverOp`s and two `DrawAtlasPathOp`s per frame;
  - a second GPU render pass per frame (`AtlasRenderTask`, 23.7 sampled ms).

With a round cap (`perf/frame-roundcap`, 3 runs each): RenderThread work 544.4 [534–557] → 500.1 [496–508] ms (−8.1 %), RenderThread p50 2.95 → 2.76 ms, main thread unchanged. Round 1 (7550e31), measured the same day, did 514.7 ms. Today's frame draws a little more than round 1's (four atlas text ops instead of two, the four icon-mask textures of D190, a longer flush), and the round cap more than makes up for it. It's a parity fix too: the benchmark compared different arcs.

**Done (D320).** Material 3 1.4.0's `ProgressIndicatorDefaults` (checked with `javap`) has `Round` for all three caps (`LinearStrokeCap`, `CircularDeterminateStrokeCap`, `CircularIndeterminateStrokeCap`), and every overload taking a cap defaults to them, so all four compose-rs indicators now default to `Round`, not the spinner alone (the table's "the determinate indicator keeps `Butt`" was wrong). The Kotlin twin's spinner (`CircularProgressIndicator(Modifier.size(24.dp))`) already drew a round arc, so the benchmark apps needed no change. Measured against main, 4 runs each, interleaved (means [min–max]):

| Animation | main | round cap |
|---|---|---|
| RenderThread work | 542.7 [541.0–546.6] ms | 491.9 [485.0–507.0] ms (−9.4 %) |
| RenderThread per frame (`DrawFrame`) p50 / p90 / p99 | 2.94 / 3.88 / 5.64 ms | 2.73 / 3.65 / 5.20 ms |
| Main thread work | 214.3 [208.5–219.0] ms | 211.3 [208.8–214.0] ms |
| Main thread per frame (`doFrame`) p50 / p90 / p99 | 1.34 / 1.88 / 2.95 ms | 1.37 / 1.95 / 3.02 ms (ranges overlap) |
| Janky frames (FrameTimeline) | 0 | 0 |

Each frame's two `PathStencilCoverOp`s and two `DrawAtlasPathOp`s became two `CircleOp`s. The scroll (no spinner in its feed) didn't change: RenderThread work 765.9 → 764.4 ms, main p99 4.00 → 3.81 ms (ranges overlap), 1 → 0.5 janky frames a run.

### The replay: interpreted framework calls (#3)

With both apps compiled `-m speed`, neither had any JIT-compiled code in its profiles on this build. So framework methods that aren't compiled in the boot image run in ART's interpreter (nterp), whatever the app's own compilation. Checked against `boot.oat`'s symbols, the replay called several of them on every operation:
- `DirectByteBuffer.getFloat`, for every float of the display list (`getInt`, `getLong` and `get` are compiled);
- `Canvas.isHardwareAccelerated`, per group and per paragraph;
- `RenderNode.getWidth`/`getHeight`, `setPosition`, `setPivotY`, `setRotationZ`, `setClipRect`;
- `putFloat` in `TextMeasurer`.

About a third of the replay was interpreter time. The prototype (`perf/frame-replay`) changes:
- floats are read as `Float.fromBits(getInt())`;
- `isHardwareAccelerated` is read once a frame;
- node sizes are tracked in Kotlin;
- a layer's properties are read in one bulk get and diffed;
- one `DirectByteBuffer` is reused (`takeDrawing` returns a length; a new buffer is asked for only when the Rust `Vec` moved);
- the executor (compose-runtime's executor.rs, so every host) doesn't call the host's wake handler for wakes from `dispatch_frame`, through a thread-local flag; a production version should reset the flag if a waker panics.

| | Animation | Scroll |
|---|---|---|
| Main work | 213.9 → 197.1 ms (−7.9 %) | 460 → 448 ms (−2.6 %, ranges overlap) |
| `compose-rs: replay` | 194 → 128 µs | 193 → 153 µs |
| `Record View#draw()` | 262 → 186 µs | 262 → 211 µs |

**Done (D321).** As prototyped, with two fixes:
- The executor's flag is per executor (in the state its wakers share), not per thread: a frame awaiter that wakes another thread's task wakes that thread's host. A guard sets it around the awaiters' wakes and resets it when dropped, so a waker that panics leaves it reset, and then wakes the host if tasks are ready (the frame's `run_ready` won't run); the awaiters after a panicking one, which were dropped unwoken before, wait for the next frame. A task another thread wakes while it's set needs no wake either: the dispatching thread polls the ready queue after clearing it. Three tests cover the skipped wake, the panic and the other thread (the prototype's thread-local fails the last two).
- A group's tracked size starts at -1, not 0: a node from the pool keeps its last group's size, so a new group of size 0 would have kept it.

Measured against the round cap (D320), 4 runs each, interleaved (means [min–max]):

| | Animation: before | after | Scroll: before | after |
|---|---|---|---|---|
| Main work | 211.8 [207.2–215.4] ms | 200.8 [197.0–207.5] ms (−5.2 %) | 486.5 [477.4–501.9] ms | 466.4 [453.2–474.1] ms (−4.1 %) |
| Main per frame p50 / p90 / p99 | 1.37 / 1.90 / 2.90 ms | 1.26 / 1.82 / 2.93 ms | 1.31 / 2.42 / 3.74 ms | 1.29 / 2.39 / 3.67 ms |
| RenderThread work | 487.5 ms | 497.5 ms (ranges overlap) | 762.3 ms | 775.0 ms (ranges overlap) |
| RenderThread per frame p50 / p90 / p99 | 2.72 / 3.58 / 4.99 ms | 2.69 / 3.54 / 5.00 ms | 2.68 / 3.74 / 5.30 ms | 2.77 / 3.77 / 5.37 ms |
| Janky frames a run | 0 | 0.25 | 1.0 | 1.25 |
| `compose-rs: replay`, `take drawing` | 237, 20 µs | 152, 9 µs | 230, 19 µs | 194, 10 µs |
| `Record View#draw()`, `doFrame` | 313, 1,457 µs | 215, 1,360 µs | 302, 1,496 µs | 257, 1,479 µs |

The frames' medians and work drop; their p99s don't move: the slowest animation frames are the system's (vsync requests, the RenderThread's sync), and the slowest scroll frames reveal an item ("Prefetch" below).

**Ceiling.** If the replay, serializing and taking the drawing all cost nothing, the animation's main work would drop by about 24 ms (12 %) after this prototype. About 40 % of what's left is `Canvas` and `RenderNode` calls that any path through HWUI makes, so the realistic remaining gain on this path is about 12 ms (6 %), #10 included. (Option D of [13](13-android-drawing-paths.md), our own Skia, would also drop HWUI's recording and sync, so this doesn't bound it.) docs/13's "about 0.2 ms a frame" for the replay was one-third interpreter overhead.

Smaller items left:
- `display?.refreshRate` in `prefetch()` (1.9 sampled ms per scroll run; cache it: S);
- serializing copies each group's bytes twice (under 1 %);
- rounded clips go through `Path.addRoundRect` + `clipPath` (about 15 % of the animation's replay); public `Canvas` has no `clipRRect`, and a `RenderNode` outline clip would be the alternative.

### Nested groups (#10, done)

Making the spinner a repaint boundary of its own (`perf/frame-spinner-boundary`) gained nothing: main work 209.7 → 209.4 ms. A group's version changes whenever a nested group's content changes (D155, D157: a record holds its nested records' bytes). So the header card around the spinner is still serialized and recorded again in Kotlin every frame, plus one more group.

With records that reference nested groups by key, the header's re-recording would go away. Before #3, that re-recording was 16.4 of the replay's 31.3 sampled ms (the arc itself about 1 ms), which would have meant −12 to −15 ms of the animation's main work. #3 has since cut the replay by a third (194 → 128 µs a frame), much of it in the same re-recording, and the two don't add: on top of #3 the estimate is −5 to −10 ms (3 to 5 %), within the 12 ms ceiling above. This revisits D155's rejected alternative now that D270 made the display list a tree. Cost M–L: serialize.rs and `Replayer.kt`.

**Done (D323).** Every group's record now comes before the frame's operations, outer before inner, and names the groups drawn in it with a `GROUP_REF` (their key). A group whose display-list version changed is written again, and if its bytes are the ones cached (only a nested group changed) its record keeps the version it was cached with, so Kotlin keeps its recording and draws the nested group's node by reference. Kotlin records each group from its own record and checks each node's recording on its own; a group with no record in a frame is gone. The indeterminate indicators (circular and linear) became repaint boundaries of their own, so the header around the spinner keeps its bytes. In the feed's animation (`examples/feed/tests/serialized_frames.rs`), each frame now writes 2 records (the spinner's and the header's, the latter unchanged) and Kotlin records 1 group, instead of the header's chain. Measured against D322, 4 runs each, interleaved (means [min–max]):

| Animation | before | after |
|---|---|---|
| Main work | 197.9 [195.9–199.9] ms | 191.3 [189.2–195.4] ms (−3.3 %) |
| Main per frame p50 / p90 / p99 | 1.27 / 1.81 / 2.91 ms | 1.19 / 1.68 / 2.89 ms |
| RenderThread work | 489.8 ms | 492.8 ms (ranges overlap) |
| RenderThread per frame p50 / p90 / p99 | 2.70 / 3.53 / 5.00 ms | 2.67 / 3.45 / 4.88 ms |
| Janky frames a run | 0 | 0.25 |
| `compose-rs: replay`, `Record View#draw()`, `serialize` | 155, 221, 30.5 µs | 97, 159, 32.4 µs |

The scroll (no spinner) is unchanged: in a second session of 5 runs each, main work 466.7 → 469.4 ms, main p50 / p90 / p99 1.27 / 1.91 / 3.04 → 1.30 / 1.94 / 3.06 ms, replay 103 → 108 µs (one more lookup per group and frame: its record and its reference); the first session's 4 runs had 473.7 → 483.6 ms and p99 3.04 → 3.17 ms, with overlapping ranges. The animation's p99 frames are the system's (vsync, the RenderThread's sync), which this doesn't touch.

### Prefetch

The "about 2.1 ms per item" quoted for prefetching is the wall-clock time of a `prefetch()` call (2.0 ms on average, 351 calls per run), mostly spent at low clock speeds: the big cores sit at about 550 MHz between frames. The work is 0.58 ms per item (about 182 items in 10 flings, 1.9 calls each). Of that work:
- text layout through JNI 52 % (StaticLayout's build 25 %; `getDesiredWidth`'s callees run interpreted, 20 %);
- composition 26 %;
- applying and disposing the items behind 10 %;
- other layout about 5 %;
- `update_focus_listeners` 3.6 %.

The frames that reveal an item (about 175 per run) are the slow ones: replay 1.04 ms against 0.09, `doFrame` 2.42 against 1.24 ms. Recording each paragraph's render node while prefetching (`perf/frame-eager-text`, docs/12's third next step) cuts the main thread's p90 and p99 by 15 % each (2.17 → 1.84 ms, 3.53 → 3.00 ms). But it raises total main work by 4 % (460 → 480 ms): prefetching's work rose by 33 ms to save 18 ms in frames. Some prefetched paragraphs are never shown; the rest of the difference is unexplained (the investigation's guess: the same recording costs more on the little cores, where idle-time work runs). It moves work to shorten the slowest frames; it doesn't reduce work. Recording the item's group ahead too would need Rust to draw unplaced items (M–L), for about 0.15 ms more per revealing frame. (Done since: D350, below.)

**Done, without the extra work (D322).** The prototype recorded each paragraph inside its layout call. The production version notes the paragraphs laid out during a `prefetch` call and records them after it returns, oldest first, while a recording (a running average of the last ones, 30 µs before any, each counting for at most 1 ms) fits before the next frame's deadline, the one Rust's prefetch works to, and at least one if the deadline isn't past (so a slow recording can't stall them). It skips the paragraphs released or laid out again since (a text measured twice is two paragraphs, the first released), the ones a frame already recorded, and empty ones. What doesn't fit waits for the next idle time, before any more prefetching. Measured against D321, 4 runs each, interleaved with the prototype on top of D321 (means [min–max]):

| Scroll | before | prototype | shipped |
|---|---|---|---|
| Main per frame p50 / p90 / p99 | 1.26 / 2.28 / 3.63 ms | 1.30 / 2.01 / 3.13 ms | 1.28 / **1.94 / 3.05** ms (−15 %, −16 %) |
| Main work | 473.1 [464.4–479.6] ms | 481.6 [475.4–484.2] ms (+1.8 %) | 473.9 [465.2–481.3] ms (+0.2 %) |
| RenderThread work | 775.3 ms | 775.5 ms | 776.3 ms |
| RenderThread per frame p50 / p90 / p99 | 2.73 / 3.69 / 5.49 ms | 2.80 / 3.83 / 5.44 ms | 2.78 / 3.73 / 5.46 ms |
| Janky frames a run | 0.75 | 0.5 | 1.0 |
| `compose-rs: replay`, `Record View#draw()` | 188, 250 µs | 105, 171 µs | 104, 168 µs |

The animation (no prefetching) is unchanged: main work 198.1 → 198.1 ms, p99 2.90 → 3.02 ms (ranges overlap). Janky frames stay at about one a run either way, within the runs' spread.

**The item's groups too (D350).** After the paragraphs, the same idle time draws each item a prefetch measured: Rust draws its unplaced repaint boundary (its group and the nested ones, in their own coordinates) and writes their records, cached as written ahead, and the replayer records their render nodes, kept apart until a frame names them. Placing the item draws its parent again, not it, so the frame placing it shares the recording, copies the records with their versions, and Kotlin keeps the nodes: that frame draws nothing new for the item and records nothing. A step (Rust's draw and serialization, Kotlin's recording) starts only if its estimate (D341's: average and recent maximum) fits 0.5 ms before the next frame; one that doesn't fit an idle time of its own gives up the items waiting, which the frames then draw. Measured against main (round 5's code), interleaved, 10 runs each (4 with the first version, 6 after the review's fixes; means [min–max]), both APKs carrying a marker slice in each frame that places a group the last didn't draw (the revealing frames):

| Scroll (measure.py's scroll run) | main | D350 |
|---|---|---|
| Revealing frames (about 174 a run): `doFrame` mean | 1.80 ms | **1.53 ms** |
| of which the replay | 0.31 ms | **0.11 ms** |
| of which `compose-rs: frame` | 0.39 ms | 0.32 ms |
| Revealing frames' `doFrame` p90 (first and second batch) | 2.45, 2.47 ms | **2.10, 2.12** ms |
| Revealing frames' `doFrame` p99 (each batch about 1,000 frames) | 5.21, 3.74 ms | 3.24, 4.22 ms |
| Other frames' `doFrame` mean | 1.35 ms | 1.36 ms |
| Main per frame p50 / p90 / p99 | 1.28 / 1.94 [1.84–2.05] / 3.00 [2.80–3.19] ms | 1.28 / **1.86** [1.81–1.94] / 2.96 [2.77–3.05] ms |
| RenderThread per frame p50 / p90 / p99 | 2.77 / 3.73 / 5.31 ms | 2.76 / 3.75 / 5.33 ms |
| Main, RenderThread work | 445.0 [437.4–454.0], 776.3 [767.5–788.3] ms | 450.7 [434.5–466.8], 782.2 [766.5–805.8] ms |
| Janky frames (FrameTimeline), the app's | 0 in 10 runs | 0 in 10 runs |
| the compositor's | 2 (one run) | 3 (one run) |
| `doFrame`s more than 2 ms late, a run | 8.3 [6–12] | 10.7 [6–16] |
| of which after main-thread work | 0.6 | 0.6 |
| Idle-time slices more than 1.2 ms past the vsync, a run | 2.3 | 2.3 |
| `prefetch` calls, a run | 363 | 361 |

The revealing frames lose 0.27 ms (−15 %), most of their excess over the other frames (0.45 → 0.17 ms), more than the 0.15 ms estimated: besides the group's recording, the replay no longer draws the paragraphs' nodes into it, and the Rust frame no longer draws the item. The main thread's p90 falls 4 %, in both batches; the p99, which more and more is the frames that compose or lay out in the frame (a fling back, which prefetching ahead in the other direction didn't reach), and the work move within the runs' spread. The late `doFrame`s are no more often behind compose-rs's idle work: on both sides each follows a `Choreographer#onVsync` that itself ran 2 to 4 ms after the vsync (the thread woke late). The full benchmark of D350 (`results/2026-10-23-pixel-8-pro-draw-ahead.json`) matches round 5 apart from it: scroll main p50 / p90 / p99 1.28 / 1.91 / 3.01 ms (1.30 / 1.92 / 3.04), work 454 and 783 ms (452, 813), one janky frame of the app's in 3 runs (a fling back composing items in its 12.9 ms frame), 12 to 20 late `doFrame`s a run (Compose's 14 to 28 in the same session), animation unchanged (no prefetching), startup 108 / 165 ms, PSS after the flings 79.5 MB.

**A fling back (D375).** The full benchmark's one janky frame of the app's after D350 was a fling reversing direction: its first frame moving the first visible item back by one composed and measured an item inside its 12.9 ms frame (`compose-rs: recompose and layout again`). The list composes the items in view and a viewport's worth beyond each side; prefetching (D156) composed two more ahead in the scroll direction but disposed every item behind the wanted range, so the item the first frame back wanted was always gone, and nothing in that frame could prefetch it: the touch that stops a fling scrolls nothing, and the next frame already carries the reversed drag. Compose's `LazyListPrefetchStrategy` turns with the scroll deltas too (and cancels a request going the other way), so it has the same first frame; its prefetch-free mitigation is the slot reuse pool, and Compose 1.9's experimental `LazyLayoutCacheWindow` keeps a "behind" window composed. Here the list now keeps the two composed items next to the wanted ones on the side behind the scroll (as many as it prefetches ahead): a frame that composes keeps them, prefetch disposes only the ones beyond. The first frame back finds them composed and measured, and after a reversal prefetching has nothing to compose until the list has moved an item. Measured against main (e12d389), interleaved, 16 runs each (means [min–max]):

| Scroll (measure.py's scroll run) | main | D375 |
|---|---|---|
| Frames composing items inline, in 16 runs | 4 (all on reversals; `doFrame` 4.2–11.4 ms) | **0** |
| The slowest `doFrame` in each reversal's first 100 ms: mean, p90, max | 4.07, 7.26, 11.44 ms | **3.46, 4.64, 5.32 ms** |
| the same, the other flings | 2.75, 3.54, 5.10 ms | 2.84, 3.79, 8.10 ms |
| Main per frame p50 / p90 / p99 | 1.26 / 1.87 [1.79–1.94] / 2.96 [2.77–3.23] ms | 1.26 / 1.87 [1.71–1.95] / 2.96 [2.68–3.25] ms |
| RenderThread per frame p50 / p90 / p99 | 2.78 / 3.75 / 5.35 ms | 2.76 / 3.75 / 5.31 ms |
| Main, RenderThread work | 437.8 [426.9–452.2], 778.1 [764.1–793.3] ms | 440.4 [423.0–466.0], 783.9 [763.8–801.4] ms |
| Prefetch compositions, measurements, wall time a run | 385, 201, 784 ms | 378, 193, 765 ms |
| Janky frames (FrameTimeline), the app's / the compositor's | 0 / 0 | 0 / 5 (two runs) |
| `doFrame`s more than 2 ms late, a run (after main-thread work) | 11.5 (0.9) | 10.9 (0.6) |

The inline compositions on reversals go, and with them the slowest reversal frames (p90 −36 %, max −54 %). Work moves within the runs' spread: prefetching composes and measures about 8 items fewer a run (those kept across the reversals, 19 ms less wall time and 16 ms less text layout), and the replay takes 4.6 ms more: a kept item isn't placed, so the drawing's records and Kotlin's render nodes let it go, and the frame placing it again writes and records it (a reversal frame's replay is 1.6–2 ms; drawing kept items ahead, D350, would need Rust to re-emit a clean boundary's groups, a follow-up, done since: D380, below). The compositor's janky frames are SurfaceFlinger's, as main's in D350's runs.

**Kept items' groups (D380).** After D375, a kept item (composed behind the wanted range, not placed) lost its records and render nodes, and the frame placing it again on a reversal wrote and recorded it: 1.6–2 ms of replay in that frame. Its recording is still valid (its key and version haven't changed), so now each frame lists the groups of the repaint boundaries an earlier frame drew that are composed, not placed and unchanged, nested ones included (`DisplayList::kept`, 8 items at most, for a platform that asks: Android). The serializer names them in a `KEPT` section and keeps their records cached, without writing them, and the replayer keeps their nodes, layers included. That alone wasn't enough: HWUI drops the recording of a node no recording draws once the render thread syncs the frame, and on the device such a build met 173–174 groups a run whose version matched but whose recording was gone (traced with a temporary marker), and recorded them again (replay 315–319 ms a run against 170 ms with the node below, 2 runs each). So the replayer draws the outermost kept nodes from a node of its own at alpha 0, which the view's recording draws and the render thread skips: none was lost then. A kept item changed while unplaced (it, or a group nested in it) is no longer listed: nothing runs for it until placed, as in Compose. Measured against D375 (26c780c), interleaved, 12 runs each (means [min–max]; the reversals are the 4 flings back a run):

| Scroll (measure.py's scroll run) | D375 | D380 |
|---|---|---|
| The slowest replay in each reversal's first 100 ms: mean, p90, max | 1.17, 1.75, 2.29 ms | **0.18, 0.25, 0.46 ms** |
| The slowest `doFrame` in each reversal's first 100 ms: mean, p90, max | 3.37, 4.65, 4.96 ms | **2.96, 3.70, 4.54 ms** |
| Replays over 0.5 ms, a run | 8.0 (8.0 ms) | **0.4** (0.4 ms) |
| Main per frame p50 / p90 / p99 | 1.27 / 1.88 [1.79–1.98] / 2.98 [2.79–3.22] ms | 1.29 / 1.90 [1.81–1.99] / 2.94 [2.80–3.14] ms |
| RenderThread per frame p50 / p90 / p99 | 2.79 / 3.80 / 5.42 ms | 2.80 / 3.77 / 5.30 ms |
| Main, RenderThread work | 434.2 [421.4–448.7], 779.1 [762.6–790.3] ms | 437.6 [425.3–450.5], 775.3 [753.0–793.9] ms |
| `compose-rs: replay`, `draw`, `serialize`, a run | 177.3, 85.3, 62.2 ms | 181.6, 91.9, 64.5 ms |
| Janky frames (FrameTimeline), the app's / the compositor's | 0 / 0 | 0 / 0 |
| `doFrame`s more than 2 ms late, a run (after main-thread work) | 9.9 (0.42) | 10.0 (0.33) |

The reversals' slowest replay falls by 85 % on average (p90 1.75 → 0.25 ms) and their slowest frame by 12 % on average (p90 −20 %). The other frames pay a few microseconds each (noting the kept boundaries, listing their groups, looking their nodes up): the replay takes 4.3 ms more a run in all though the reversals' replays lost 7.6 ms, the Rust draw 6.6 ms and serializing 2.3 ms more; the main thread's work (+3.4 ms, +0.8 %) and percentiles move within the runs' spread. The measured build listed a kept item whatever its nested groups did; the shipped one drops it when one changed (none does in the feed) and lists nothing unless the platform asks.

### The four together (D320–D323): the full benchmark

`measure.py --runs 15`, the branch then main (152a608), back to back, Compose alternated run by run in each session (means of the 3 traced runs; startup medians):

| | main | D320–D323 | Compose (the two sessions) |
|---|---|---|---|
| Animation: main per frame p50 / p90 / p99 | 1.35 / 2.06 / 3.16 ms | 1.18 / 1.75 / 2.90 ms | 1.48–1.50 / 2.18–2.20 / 3.15–3.39 ms |
| Animation: RenderThread per frame p50 / p90 / p99 | 2.94 / 4.02 / 5.65 ms | 2.66 / 3.59 / 5.17 ms | 2.95 / 4.07–4.09 / 5.56–5.70 ms |
| Animation: main, RenderThread work | 213, 552 ms | 192, 505 ms | 256–257, 546–551 ms |
| Scroll: main per frame p50 / p90 / p99 | 1.27 / 2.22 / 3.87 ms | 1.23 / 1.88 / 2.89 ms | 1.36–1.50 / 2.93–3.23 / 6.11–6.71 ms |
| Scroll: RenderThread per frame p50 / p90 / p99 | 2.71 / 3.76 / 5.36 ms | 2.64 / 3.81 / 5.59 ms | 2.04–2.24 / 3.32–3.48 / 4.90–5.24 ms |
| Scroll: main, RenderThread work | 478, 779 ms | 484, 809 ms | 823–873, 864–903 ms |
| Janky frames (3 runs) | animation 0, scroll 1 | 0, 0 | 0, 1 and 0, 1 |
| Startup feed / big | 105 / 179 ms | 107 / 177 ms | 127–130 / 220–221 ms |

The scroll's RenderThread rose in this pair of sessions (779 → 809 ms, p99 5.36 → 5.59), but not in the interleaved A/Bs of each change, where it stayed within the runs' spread; Compose's moved by as much between the sessions (864 → 903 ms).

### Jank: buffer stuffing, idle work past the vsync, hidden views (D340–D343)

Round 4's traces showed three problems no average shows.

- **Buffer stuffing recovery stopped prefetching (D340).** After a display stall, Android 16's Choreographer enters "buffer stuffing recovery" and hands every frame callback its vsync's time less an interval until the app goes idle, which a fling never does. The idle-time budget was the frame time plus an interval, so it was always past: the view re-posted a vsync callback every frame and never called Rust. In the round 4 run that hit it, there were no prefetches for 10 s and 120 compositions in frames (main p99 7.35 ms against about 3.1). The budget now runs to the vsync after the last one at or before the callback's start, on the frame time's phase.
- **Idle work ran past the vsync (D341).** About 1 % of the idle-time slices ended more than 1.2 ms past the vsync. Each was an item's measurement (its six text layouts, the first one cold, 4–6 ms) started with 3–4 ms left, because the running average (about 2.2 ms) said it fit. A step now needs the larger of the average and the recent maximum (fading by an eighth per step), plus 0.5 ms. A miss lowers the estimate only when the step was its idle time's first, and slowly once the step was timed a few times: two reviews found that lowering it by a quarter on every miss let the measurements start after a composition, or after two or three short idle times.
- **A hidden view kept animating (D342).** The demo, in the background, ran a frame on every vsync (about 1 ms each) without drawing.

Measured on the Pixel 8 Pro, with main at 0757e61 (with D330) and the branch alternated run by run, 4 runs each (means [min–max]):

| Scroll (measure.py's scroll run, D343) | main | branch |
|---|---|---|
| Idle-time slices ending past the vsync, a run | 38.8 [37–40] | **16.5** [13–19] |
| of which more than 1.2 ms past | 9.0 [6–11] | **2.0** [1–3] |
| The latest past the vsync | 1.6–3.2 ms | 1.4–2.7 ms |
| `doFrame`s more than 2 ms late after main-thread work, a run | 3.3 [0–6] | **1.0** [1–1] |
| `doFrame`s more than 2 ms late (most woke late) | 17.5 [15–19] | 15.0 [9–20] |
| `prefetch` calls | 352 | 365 |
| Main per frame p50 / p90 / p99 | 1.28 / 1.94 / 3.05 ms | 1.28 / 1.98 / 3.16 ms |
| RenderThread per frame p50 / p90 / p99 | 2.77 / 3.80 / 5.27 ms | 2.78 / 3.83 / 5.55 ms |
| Main, RenderThread work | 441.8, 780.5 ms | 436.0, 776.1 ms |
| Janky frames (FrameTimeline) | 0, 1 (the app's), 0, 0 | 0, 0, 0, 0 |

Fewer idle-time slices run past the vsync, by less, and fewer frames start late because of them. The percentiles' differences are within the runs' spread (main p99 2.88–3.26 against 3.05–3.35 ms; the earlier versions of the branch measured 2.96 and 3.01 against main's 3.03 and 3.02 in the same way), and the work is the same. More measurements wait for a whole idle time, so some may be done in a frame instead; the slowest frames would show it, and they move by less than their spread.

**Buffer stuffing, simulated.** None of these runs entered recovery, so it was simulated: both APKs were built with every frame callback's time shifted back an interval, as recovery does (a temporary patch, not committed), 2 runs each (the branch before the reviews' estimate changes, which don't touch the budget):

| | main, shifted | branch, shifted |
|---|---|---|
| `prefetch` calls | 0, 0 | 363, 365 |
| Main per frame p90 / p99 | 2.67 / 7.36 ms | 2.00 / 3.19 ms |
| Main work | 484.3 ms | 433.4 ms |

main reproduces the round 4 run (p99 7.35 ms, no prefetch); the branch prefetches as if nothing were shifted. The pure function behind it (`frame_time.rs`) is tested with shifted, late and on-time callbacks.

**The hidden view** was traced with the demo's gallery shown for 4 s, on the home screen for 10 s, then shown again: 120 frames a second, none while hidden, 120 a second again, drawn.

**The full benchmark** (`measure.py --runs 15`, the branch with D330: `results/2026-10-22-pixel-8-pro-round-5.json`, round 5 of [12](12-android-performance.md)), against round 4 (e3069cb). Its scroll leaves the warm-up swipe out (D343), and both apps were measured with our other apps stopped:

| compose-rs | round 4 | this run | Compose, this run |
|---|---|---|---|
| Scroll: janky frames, per run | 1, 3, 1 | **0, 0, 0** | 0, 1, 0 (the app's) |
| Scroll: main per frame p50 / p90 / p99 | 1.23 / 1.90 / 3.11 ms | 1.30 / 1.92 / 3.04 ms | 1.56 / 3.22 / 7.03 ms |
| Scroll: RenderThread per frame p50 / p90 | 2.74 / 3.70 ms | 2.71 / 3.81 ms | 2.13 / 3.56 ms |
| Scroll: main, RenderThread work | 477, 786 ms | 452, 813 ms | 844, 878 ms |
| Scroll: frame time p50 / p90 / p99 (gfxinfo) | 5 / 6 / 8 ms | 5 / 6 / 8 ms | 5 / 7 / 11 ms |
| Animation: main per frame p50 / p90 / p99 | 1.20 / 1.79 / 2.98 ms | 1.21 / 1.69 / 2.76 ms | 1.56 / 2.25 / 3.28 ms |
| Animation: RenderThread p50 / p90 | 2.63 / 3.56 ms | 2.64 / 3.38 ms | 2.90 / 4.10 ms |
| Animation: main, RenderThread work | 190, 517 ms | 190, 501 ms | 258, 544 ms |
| Startup feed / big | 107 / 174 ms | 110 / 163 ms | 131 / 226 ms |
| PSS after 10 flings | 81.2 MB | 79.5 MB | 90.0 MB |

compose-rs had no janky frame in any run, and no run of either app entered buffer stuffing recovery. Two of Compose's animation runs had every frame presented a vsync late, on cadence, which FrameTimeline doesn't count as jank. The round 4 run that entered recovery (p99 7.35 ms) set nothing in round 4's medians; the scroll's lower main work and `big`'s startup owe more to D330 (BoringLayout), merged in between, than to these changes. measure.py now leaves out the warm-up swipe, so the jank counts aren't comparable one for one. Two earlier full runs of the branch, before the reviews' fixes, had no janky frame of compose-rs's either, apart from 3 of the compositor's in one run.

**Idle work after a slow step (D381).** Round 6 had 1 to 3 `doFrame`s a run delayed by main-thread work, round 5 none. In the 32 traces of D375's A/B (both sides), the work past the vsync before each such `doFrame` was always a `prefetch` call that composed an item and then measured it in one idle time (6.3–8.0 ms slices, started 4–5 ms before the vsync), never D350's recording of an item's groups or D322's of its paragraphs (the `record paragraphs` slices after them are the 10 µs check that finds the deadline past). Of those runs' 6,300 measurements, 46 ended more than 1.2 ms past the vsync. All ran mostly on the little cores (70 % of their running time or more); 43 followed the item's composition in the same idle time, which had itself taken a median 2.0 times its estimate (41 of the 43 more than it). They took 3.9–8.2 ms (median 5.1) against an estimate of a median 2.4 ms, with 2.5–4.9 ms left. D341's estimate can't foresee it: which core runs the idle time is the scheduler's choice. But the composition just before showed it, so within an idle time a list's steps after one that ran slower than its estimate now need their estimate scaled by the largest such ratio (4 at most). Replaying the traces' steps with D341's estimates, a simplified form of the rule (each measurement scaled by the composition just before it, uncapped) would have started 8 of the 42 overrunning measurements D341 started, deferring 14 % of the measurements to the next idle time instead of 2.6 % (the shipped rule, taking the largest ratio so far and scaling compositions too, defers at least as many). The interleaved A/Bs since D350 counted 0.25 to 0.9 such `doFrame`s a run on either side, so round 5's none and round 6's 6 in 3 runs were both at the edges of what that rate gives. Measured against D380, interleaved, 12 runs each (means [min–max]):

| Scroll (measure.py's scroll run) | D380 | D381 |
|---|---|---|
| Idle-time slices ending past the vsync, a run | 12.4 [7–16] | **6.9** [3–12] |
| of which more than 1.2 ms past | 1.67 [0–4] | **0.92** [0–2] |
| The latest past the vsync | 1.70 [0.80–2.75] ms | 1.39 [0.79–2.16] ms |
| `doFrame`s more than 2 ms late after main-thread work, in 12 runs | 5 | 3 |
| `doFrame`s more than 2 ms late, a run | 11.1 | 10.3 |
| `prefetch` calls; prefetch measurements, a run | 357; 173.5 | 361; 174.0 |
| Text layouts and compositions inside frames | 0, 0 | 0, 0 |
| Main per frame p50 / p90 / p99 | 1.30 / 1.92 [1.86–2.03] / 2.93 [2.77–3.05] ms | 1.29 / 1.90 [1.84–2.00] / 2.91 [2.65–3.20] ms |
| RenderThread per frame p50 / p90 / p99 | 2.78 / 3.80 / 5.48 ms | 2.76 / 3.74 / 5.25 ms |
| Main, RenderThread work | 440.5 [433.3–448.3], 782.0 [759.1–802.4] ms | 434.9 [425.1–444.5], 769.3 [756.7–783.7] ms |
| Janky frames (FrameTimeline), the app's / the compositor's | 0 / 2 (one run) | 0 / 0 |

The idle-time slices past the vsync halve, as do those more than 1.2 ms past; no measurement moved into a frame (the frames lay no text out), and prefetching kept up (as many measurements a run). Of the 3 `doFrame`s still delayed, two followed a composition and a measurement in one idle time, and one a measurement alone that took 6.5 ms: each step's own overrun, which no estimate made before it starts can see. The percentiles and the work move within the runs' spread. So do the reversals' frames, which were worse this time (their slowest `doFrame`'s p90 3.59 → 4.01 ms, max 4.98 → 5.45 ms; replays over 0.5 ms 0.2 → 1.1 a run, the slowest 0.35 → 0.90 ms), probably noise: D381 only defers prefetch steps, and no frame of either build composed or laid out text. The measured build had no upper bound on the ratio; the shipped one caps it at 4 (the largest seen before a measurement that overran was 4.1).

### The scroll's RenderThread: the boost, not the work

Since round 2, the scroll's RenderThread frames have taken longer than Compose's (round 5: p50 2.71 against 2.13 ms) while doing less work (813 against 878 ms). The scroll traces of five full sessions (3 runs per app each: round 5 and the two full runs of D340–D343 before it, and D320–D323's two sessions) were split by the boost Android's performance hints (ADPF) gave the RenderThread when each frame started. That boost is the HWUI hint session's minimum utilization (`uclamp.min`, out of 1024: the `adpf.<pid>-…-HWUI-0-min` counter in the traces). ADPF raises it when the frames HWUI reports run long against their target. Those are the main thread's frames as well as the RenderThread's.

| Scroll, five sessions (range of the sessions) | Kotlin Compose | compose-rs |
|---|---|---|
| Frames unboosted (minimum under 100) | 41–51 % | 86–93 % |
| Frames at 100–199 | 24–29 % | 6–12 % |
| Frames at 200 or more | 21–34 % | 1–3 % |
| Unboosted frames' duration p50 / p90 / p99 | 2.73–2.84 / 3.81–4.02 / 5.47–5.63 ms | 2.74–2.82 / 3.80–3.92 / 5.41–5.83 ms |
| Unboosted frames' work, median | 393–400 µs | 389–401 µs |
| Boosted frames (100–199) duration p50 | 2.05–2.08 ms | 2.07–2.17 ms |

At the same boost, the two apps' RenderThread frames take the same time for the same work. The unboosted p99 differs by +0.08 ms on average over the five sessions (compose-rs's against Compose's: −0.08 to +0.23 ms). Compose's frames are boosted half the time, and boosted frames run on the mid and big cores at higher frequencies. compose-rs's main thread has fewer long frames, so ADPF rarely boosts it. Weighted so that each boost bucket holds Compose's share of frames, compose-rs's round 5 scroll frames have a p50 / p90 / p99 of 2.16 / 3.35 / 5.27 ms, against Compose's 2.13 / 3.56 / 5.17 (unweighted: 2.71 / 3.81 / 5.68). In both apps, about 19 in 20 of the slowest 5 % of frames are unboosted and ran mostly on little cores. measure.py reports the picture for each run since round 6 ([12](12-android-performance.md), "Scrolling: the RenderThread's boost"): there, 96 % of compose-rs's scroll frames were unboosted against 46 % of Compose's, and the unboosted and 100–199 buckets' percentiles matched within 0.3 ms.

**Asking for a boost ourselves: done (D390).** Round 7 left this as "not pursued": the boost would cost power for durations no user sees. The user chose to give compose-rs a boost comparable to Compose's. While a `ComposeRsView` shows (API 33+, `isPerformanceHintEnabled`, on by default), the RenderThread is in a performance hint session of compose-rs's own: the NDK's `APerformanceHint`, looked up at run time, one session per process, opened at the second frame after a view shows (not on the first traversal's path at startup). Each compose-rs frame reads the RenderThread's CPU time (one `clock_gettime` of its thread CPU clock, at the start of `frame`) and reports what it used since the last frame against a 6.5 ms target. While compose-rs makes a frame every vsync, that is the frame before's work (the RenderThread ends well before the next vsync), plus whatever else it drew in between (other views in the window, its own animations); after more than 25 ms without a frame, the time since starts a new count, unreported. The main thread wasn't in the session at first: its frames were already shorter than Compose's, and boosting it seemed to cost power for nothing a user sees. The stress list showed otherwise, and it is now (D440, "The main thread in the session").

- **How the power HAL combines sessions.** The Pixel power HAL keeps each session's votes (`CPU_VOTE_DEFAULT`, the PID's output, and the load hints' `CPU_LOAD_*`, each with a timeout) and sets a thread's `uclamp.min` to the highest active vote of all the sessions holding it (`SessionTaskMap::getTaskVoteRange`). On the device, a scroll sampling the RenderThread's requested `uclamp.min` (`/proc/<pid>/task/<tid>/sched`) every ~170 ms: 63 of 71 samples equal the higher of the two sessions' votes, where HWUI's alone matched 29 and ours alone 51 (the sessions differed at 55; the misses are the sampling's lag). So our session only adds: HWUI's still applies, and wins when it is higher.
- **How it reacts.** The device's ADPF profile (`powerhint.json`, `REFRESH_120FPS`): a PID with no integral term, a proportional gain of 2.0 over the target and 0.6 under it per 100 µs of error, a derivative gain of 500 on rising errors only, the minimum between 2 and 480 (with floors of 230–410 and ceilings of 480–722 when the session counts frames over 1.2 × the target: "heuristic boost"), each vote lasting max(15 × target, 167 ms). Creating a session votes 480 (`CPU_LOAD_RESET`) for about 49 ms at our target, so each show starts with a short full boost. The NDK client sends every report over the target at once; of a run of reports under it, all those of its first 83 ms, then one per 83 ms. So a frame over the target after a short one raises the minimum by 150–250 at once, and it then decays by 0.6 × the margin per report sent. HWUI's session reports the whole frame, from the main thread's start to the RenderThread's end, against 11.62 ms (frame deadline less intended vsync): Compose's long main-thread frames cross it; compose-rs's rarely do.
- **Why reading the CPU clock.** `Window.addOnFrameMetricsAvailableListener` gives the RenderThread's wall time per frame (`SYNC` + `COMMAND_ISSUE` + `SWAP_BUFFERS`), but costs a JNI upcall and a `Handler` post on the RenderThread for every frame, a thread of ours woken per frame, a JNI call to report, and finding the view's `Window` (none for a dialog's). The thread CPU clock costs one system call on the main thread, inside the `frame` call already made, and measures the work a boost speeds up (not waits).

**The target.** Swept on the scroll, each against Compose in its session (2–5 runs each; medians; CPU power: the CPU rails' mean over the measured 13 s, against main's in sweep 1's session, 2 runs each, so only differences of 5 % or more say anything):

| Target | Unboosted frames | Boost minimum, time-weighted | RenderThread p50 / p99 | CPU power against main |
|---|---|---|---|---|
| none (main) | 91–95 % | 23–27 | 2.74 / 5.21 ms | |
| 3.5 ms | 0 % | 499 | 1.10 / 2.68 ms | +17 % |
| 4.5 ms | 0 % | 392 | 1.32 / 2.82 ms | +10 % |
| 6.0 ms | 34 % | 169 | 2.09 / 4.57 ms | +4 % |
| **6.5 ms** | 51 % | 127 | 2.30 / 4.99 ms | (+2 % in the A/B below) |
| 7.0 ms | 63 % | 83 | 2.49 / 5.08 ms | – |
| 7.5 ms | 80 % | 54 | 2.65 / 5.25 ms | +3 % |
| Kotlin Compose (HWUI's session), the sweeps' sessions | 51 % | 124 | 2.22 / 5.09 ms | +11 % |

At 3.5 and 4.5 ms the minimum never drops (the heuristic boost's floors), the RenderThread leaves the little cores altogether, and the main thread's p90 / p99 get worse (2.27 / 3.46 against 1.88 / 2.88 ms at 3.5 ms: more `doFrame`s start late). 6.5 ms is the target whose boost was closest to Compose's in its session (51 % of frames unboosted against Compose's 51 %). From session to session, though, its unboosted share was 51 % (the sweep), 62 % (the A/B below), 53 % (round 8 in [12](12-android-performance.md)) and 56 % (round 9), and Compose's 51 %, 49 %, 34 % and 47 %: compose-rs's RenderThread is now boosted far more often than before, and less often than Compose's in three sessions of four.

**Interleaved A/B** (main 0e0c86c, the change, and Compose in the same session; 6 runs each, the order rotating; percentiles: medians of the runs; CPU, work and power: means over the measured window, 13 s scrolling, 9 s animating, ± the runs' standard deviation; power rails polled every 250 ms by an `android.power` data source added to measure.py's trace config). It measured the change before three refinements made after review (the session opened at the second frame rather than the first, the 25 ms gap, two views' frames counted once within 4 ms rather than 2); 4 runs each of both builds, interleaved, then showed the refined one boosted slightly less, within the runs' spread (below).

| Scroll | main | own session (D390) | Kotlin Compose |
|---|---|---|---|
| RenderThread per frame p50 / p90 / p99 | 2.70 / 3.67 / 5.36 ms | 2.50 / **3.39** / **4.92** ms | **2.19** / 3.49 / 5.03 ms |
| Main thread per frame p50 / p90 / p99 | 1.28 / 1.85 / 2.85 ms | 1.31 / 1.85 / 2.79 ms | 1.56 / 3.10 / 6.84 ms |
| Frames unboosted (the highest vote under 100) | 93 % (84–97) | 62 % (34–76) | 49 % (31–54) |
| Boost minimum, time-weighted | 24 | 85 (HWUI's alone: 12) | 128 |
| Unboosted / 100–199 / 200+ frames' p50 | 2.74 / 2.06 / 1.72 ms | 2.75 / 2.05 / 1.71 ms | 2.85 / 2.10 / 1.56 ms |
| RenderThread on the little cores | 30 % | 23 % | 24 % |
| RenderThread's mean frequency on the mid cores | 548 MHz | 602 MHz | 740 MHz |
| Janky frames (FrameTimeline), all runs | 2 (the compositor's) | 5 (the compositor's) | 2 (the app's) |
| RenderThread CPU time (work) | 4,418 (676) ms | 4,016 (687) ms | 3,793 (761) ms |
| Main thread CPU time (work) | 2,921 ± 48 (385 ± 8) ms | 3,027 ± 87 (395 ± 7) ms | 3,842 (745) ms |
| The app's CPU time (work), all threads | 9,170 (1,297) ms | 8,839 (1,315) ms | 9,477 (1,768) ms |
| The power HAL's CPU time | 649 ms | 683 ms | 642 ms |
| Power: CPU rails | 420 ± 9 mW | 429 ± 20 mW (+2.1 %) | 457 ± 11 mW |
| Power: all rails (display included) | 1,653 ± 16 mW | 1,660 ± 29 mW (+0.4 %) | 1,691 ± 19 mW |

| Animation | main | own session (D390) | Kotlin Compose |
|---|---|---|---|
| RenderThread per frame p50 / p90 / p99 | 2.63 / 3.43 / 4.97 ms | 2.54 / 3.26 / 4.60 ms | 3.03 / 4.11 / 5.49 ms |
| Main thread per frame p50 / p90 / p99 | 1.20 / 1.75 / 2.94 ms | 1.22 / 1.69 / 2.72 ms | 1.53 / 2.15 / 3.03 ms |
| Frames unboosted | 96 % | 74 % (53–92) | 96 % |
| RenderThread CPU time (work) | 2,993 (450) ms | 2,855 (444) ms | 3,426 (474) ms |
| Main thread work | 172 ± 3 ms | 178 ± 2 ms | 227 ms |
| Power: CPU rails | 400 ± 16 mW | 409 ± 25 mW (+2.3 %) | 391 ± 9 mW |
| Power: all rails | 1,621 ± 22 mW | 1,629 ± 29 mW (+0.5 %) | 1,608 ± 13 mW |

- **Durations.** The scroll's RenderThread p50 is 0.20 ms shorter (2.70 → 2.50), its p99 0.44 ms (5.36 → 4.92, below Compose's 5.03). The gap to Compose's p50 closes by about 40 % (0.51 → 0.31 ms) in this session, where our session boosted less often than Compose's HWUI session (62 % of frames unboosted against 49 %); in the sweep's sessions, at 51 % against 51 %, the p50 was 2.30 against Compose's 2.22 ms. Each boost bucket's durations are the same as before: frames are shorter because more of them are boosted, as at the same boost they already matched. Main-thread percentiles and gfxinfo's frame times (5 / 6 / 7 ms) are unchanged; no frame of compose-rs's missed the app's deadline on either side.
- **Power.** The CPU rails draw 2.1 % more power scrolling and 2.3 % more animating, and all rails 0.4 % and 0.5 % more. Six runs don't resolve it: the 95 % intervals are about −3 to +7 % (scroll) and −4 to +9 % (animation) for the CPU rails. Compose's CPU rails drew 9 % more than main's scrolling and 2 % less animating. The boosted RenderThread runs less CPU time (−9 % scrolling: at higher frequencies) for about the same work (+1.6 %): what costs is the frequency, not more cycles. Lower targets cost more: +10 % at 4.5 ms, +17 % at 3.5 ms.
- **CPU elsewhere.** The power HAL runs 34 ms more CPU per scroll run (+5 %) for our session's reports (about 200 a run reach it; the others are batched in the client). The main thread's work rose 2.6 % scrolling (385 → 395 ms) and 3.5 % animating (172 → 178 ms), the same way in both, so likely real: the clock read and the report call (an NDK mutex, trace counters while traced, at times a message to the HAL), about 6 µs a frame of work (milliseconds on the fastest core at its highest frequency; its CPU time rose 3.6 %). Opening the session (finding the RenderThread, `createSession`, a call into the system) is traced as `compose-rs: hint session`, at the second frame shown.
- **Animation.** The RenderThread is rarely over 6.5 ms, but the boost isn't nothing: 74 % of frames unboosted against 96 % (Compose's: 96 %), p50 2.63 → 2.54 ms, p99 4.97 → 4.60 ms, where compose-rs's RenderThread was already faster than Compose's.
- **The refinements** (opening at the second frame, the 25 ms gap, 4 ms for two views), against the build measured above, interleaved, 4 runs each: scroll frames unboosted 49 % (35–63) against 41 % (31–62), time-weighted minimum 112 against 132, RenderThread p50 / p99 2.30 / 4.83 against 2.14 / 4.72 ms, main-thread work 399 against 406 ms; animation 65 % against 69 % unboosted, RenderThread p50 2.45 against 2.50 ms. The refined build was boosted slightly less on every measure, within the runs' spread; no janky frame of the app's.
- **The stress list** ([15](15-android-stress-benchmark.md), round 9: the first measurement of the stress screens with the session). Its reports almost never exceed 6.5 ms (2.5 ms a frame on average), yet the session's vote averages about 320 there, against about 100 on the feed, and 96 % of the RenderThread's frames start at 200 or more: its p50 went from 3.04 to 2.20 ms (Compose: 2.62 ms). The cost fell on the main thread: with the RenderThread's part of each frame shorter, HWUI's session, which boosted both threads, hardly boosts any more (a time-weighted 16, against 114), and the main thread ran 83 % of its time on the little cores. For the same work (576 → 574 ms), its p50 went from 1.41 to 1.67 ms and its p99 from 3.24 to 3.60 ms, and 8 to 16 `doFrame`s a run waited for a prefetch step (0 to 2 before). None was janky. On the feed, the main thread already ran about 80 % of its time on the little cores without D390, so nothing changed there. Fixed by D440 (next section).

### The main thread in the session (D440)

**Why the vote sits near 320 on the stress list.** The power HAL traces each session's controller terms for every batch of reports it gets (`adpf.<pid>-…-pid.pOut`, `-pid.dOut`, `-min`, `-batch_size`, `-hboost.jankyLevel`), so round 9's traces show it directly. The device's ADPF profile (`REFRESH_120FPS`) and the sources (AOSP's `HintSessionWrapper.cpp`, `CanvasContext.cpp` and `performance_hint.cpp`; the Pixel power HAL's `PowerHintSession.cpp`):

- **The client batches.** The NDK sends a report over the target at once. Under the target, it sends those of the first 83 ms of a run, then one batch per 83 ms (`ReportingRateLimitNs`). On the scroll that is about 200 batches a run of about 10 reports each.
- **The controller sees two reports a batch.** The proportional term uses the batch's last report only (`SamplingWindow_P` 1): 2.0 per 100 µs over the target, 0.6 per 100 µs under it. There is no integral term (`PID_I` 0). The derivative term uses the difference between the batch's last two reports (`SamplingWindow_D` 1), on rises only (`PID_Du` 0), with a gain of 500 / (target in 100 µs): 7.7 per 100 µs of rise at 6.5 ms. The sum is added to the vote, which stays between 2 and 480.
- **So under the target, the vote is a random walk.** With reports of about 2.5 ms against 6.5 ms, the proportional term takes about 23 off each batch, whatever the vote. The derivative term adds 7.7 per 100 µs by which the batch's last report exceeds the one before. On the feed it added 20 to 21 a batch on average (45 % of batches, 43 to 49 when it did); on the stress list 26 to 29 (44 to 51 %, 51 to 62 when it did: its frames vary more, with four kinds of items and carousels). The net is −2 to −4 a batch on the feed and +3 to +6 on the stress list. Over 200 batches, the feed's vote drifts down to the floor (it sat at 10 or less 16 to 29 % of the time; time-weighted 85 to 108), and the list's up toward the ceiling (never at 10 or less, at 470 or more 3 to 11 % of the time; time-weighted 309 to 335). What sets the level is the frame-to-frame noise against a fixed decay. How far the reports are under the target hardly matters: nothing pulls the vote back toward a level.
- **Not the other suspects.** The heuristic boost (floors of 230 to 410 once 2 of the last 300 reports exceed 1.2 × the target) never engaged: its janky level stayed 0. The gap rule only drops reports. The cadence (one report a frame, batched by the client) is HWUI's too.
- **HWUI's session works the same way.** It holds the main thread, the RenderThread and HWUI's worker threads, and reports each drawn frame's time from the main thread's start (`FrameStartTime`) to the RenderThread's end, less the time waiting for a buffer or the sync, against `FrameDeadline − IntendedVsync` (11.62 ms here). For Compose, whose reports are about 4 ms, it took 45 a batch and added 37 on the feed (vote 113 to 137), and took 40 and added 48 on the stress list (250 to 305). For compose-rs with D390, it took 47 and added 21 (vote 18 to 27). So Compose's main thread is boosted on the list too, by the same mechanism.

So no target or report formula makes the vote follow the load. A higher target lowers the derivative's gain and raises the decay, so the vote falls; a lower one raises it; a mean over frames removes the derivative's kicks, and the vote falls to the floor (variant d below). What can be chosen cleanly is which threads get the vote. (Revised by D500, "The session's target" below: the vote does settle, where the reports' mean is about 0.4 of the target, and reporting both threads' time puts that level where the frames' load is.)

**The variants**, interleaved on the device (Pixel 8 Pro, `speed`): 12 runs each of the feed's scroll and animation and the stress scroll and animation, the four variants rotating each round, the session's other rules (second frame, 25 ms gap, 4 ms) unchanged:
- **a**: main (18272a3): the RenderThread alone, its CPU time against 6.5 ms.
- **b**: the main thread (the one calling `frame`) in the session too, the same reports.
- **c8**: both threads, reporting the main thread's CPU time since the last frame plus the RenderThread's, against 8.33 ms: one 120 Hz interval, for the two threads' work for a frame (the main thread's about 2 ms on the feed, prefetching included, and the RenderThread's 2.5 ms).
- **c10**: the same against 10 ms, the target whose feed boost was expected to come closest to D390's (between c8's and the 11.62 ms of the screening).

Two more were screened, 3 runs each of both scrolls against a, b and c8, and dropped. With HWUI's target (11.62 ms), the feed's RenderThread p50 was 2.67 ms against a's 2.38 (vote 43, 78 % of frames unboosted). With both threads' time averaged over 4 frames against 8.33 ms, the vote was 13 and 56 and the RenderThread p50 2.72 and 3.32 ms. Medians of the 12 runs' percentiles; work, CPU time and power are means over 13 s scrolling and 9 s animating from the measured start (power ± the runs' standard deviation, the difference ± its 95 % interval); janky frames are FrameTimeline's, all runs. The raw runs are `results/2026-10-23-pixel-8-pro-hint-main-thread.json`.

| Stress scroll | a (main) | **b** | c8 | c10 |
|---|---|---|---|---|
| Main thread per frame p50 / p90 / p99 | 1.54 / 2.24 / 3.53 ms | **1.07 / 1.71 / 2.97 ms** | 0.94 / 1.42 / 2.75 ms | 1.10 / 1.76 / 2.89 ms |
| Main thread p50, range of the runs | 1.51–1.61 ms | 1.02–1.23 ms | 0.92–0.96 ms | 1.05–1.17 ms |
| RenderThread per frame p50 / p90 / p99 | 2.39 / 3.46 / 5.01 ms | 2.49 / 3.51 / 4.83 ms | 2.09 / 2.96 / 4.11 ms | 2.44 / 3.57 / 5.32 ms |
| Janky frames, the app's / the compositor's | 0 / 0 | 0 / 2 (one run) | 0 / 5 (two runs) | 0 / 3 (one run) |
| `doFrame`s delayed by main-thread work (prefetch steps) | 58 (1–10 a run) | 4 (0–1) | 2 | 7 |
| Main thread on the little cores | 79 % | 28 % | 3 % | 31 % |
| Our session's vote, time-weighted (HWUI's) | 257 (23) | 247 (18) | 368 (10) | 244 (21) |
| Main thread CPU time (work) | 3,529 (498) ms | 2,368 (610) ms | 1,996 (722) ms | 2,385 (612) ms |
| RenderThread work | 1,267 ms | 1,269 ms | 1,408 ms | 1,258 ms |
| Power: CPU rails | 474 ± 14 mW | −0.7 % ± 2.2 | +2.6 % ± 2.3 | −0.9 % ± 2.1 |
| Power: all rails | 1,813 ± 35 mW | −0.6 % ± 1.6 | −0.1 % ± 1.5 | −1.0 % ± 1.5 |

| Feed scroll | a (main) | **b** | c8 | c10 |
|---|---|---|---|---|
| Main thread per frame p50 / p90 / p99 | 1.32 / 1.86 / 2.88 ms | **1.24 / 1.75 / 2.70 ms** | 0.84 / 1.33 / 2.16 ms | 1.12 / 1.65 / 2.73 ms |
| RenderThread per frame p50 / p90 / p99 | 2.38 / 3.39 / 4.97 ms | 2.37 / 3.37 / 4.91 ms | 1.54 / 2.21 / 3.30 ms | 2.00 / 3.13 / 4.75 ms |
| Janky frames, the app's / the compositor's | 0 / 0 | 0 / 4 (two runs) | 0 / 8 (three runs) | 0 / 0 |
| `doFrame`s starting more than 2 ms late (by main-thread work) | 119 (1) | 89 (1) | 191 (1) | 109 (3) |
| Main thread / RenderThread on the little cores | 78 % / 22 % | 69 % / 22 % | 20 % / 2 % | 53 % / 16 % |
| Our session's vote, time-weighted (HWUI's) | 87 (20) | 94 (15) | 294 (4) | 151 (17) |
| Main thread work / RenderThread work | 404 / 694 ms | 424 / 693 ms | 562 / 856 ms | 468 / 739 ms |
| Power: CPU rails | 433 ± 8 mW | +1.0 % ± 1.8 | +4.0 % ± 2.1 | +1.8 % ± 1.4 |
| Power: all rails | 1,754 ± 40 mW | +0.4 % ± 1.7 | +0.0 % ± 1.6 | +0.3 % ± 1.7 |

| Animations | a (main) | **b** | c8 | c10 |
|---|---|---|---|---|
| Feed: main thread p50 / p90 / p99 | 1.22 / 1.72 / 2.90 ms | 1.19 / 1.65 / 2.88 ms | 1.21 / 1.71 / 2.95 ms | 1.22 / 1.77 / 3.01 ms |
| Feed: RenderThread p50 / p90 / p99 | 2.54 / 3.27 / 4.71 ms | 2.45 / 3.18 / 4.60 ms | 2.57 / 3.31 / 4.74 ms | 2.65 / 3.42 / 4.95 ms |
| Feed: janky frames (app / compositor) | 0 / 0 | 0 / 2 (one run) | 0 / 0 | 0 / 0 |
| Feed: power, CPU rails | 405 ± 21 mW | −1.8 % ± 3.7 | +1.4 % ± 4.3 | +4.6 % ± 8.5 |
| Stress: main thread p50 / p90 / p99 | 2.36 / 3.00 / 4.10 ms | 2.26 / 2.83 / 3.80 ms | 2.25 / 2.80 / 3.89 ms | 2.33 / 2.95 / 4.07 ms |
| Stress: RenderThread p50 / p90 / p99 | 6.36 / 7.43 / 8.50 ms | 6.38 / 7.43 / 8.43 ms | 6.41 / 7.50 / 8.52 ms | 6.37 / 7.46 / 8.55 ms |
| Stress: janky frames (app / compositor) | 1 / 0 | 1 / 0 | 0 / 1 | 0 / 0 |
| Stress: power, CPU rails | 1,837 ± 61 mW | −0.8 % ± 2.8 | +0.8 % ± 2.8 | −1.3 % ± 2.5 |

**Chosen: b** (D440). It gives the main thread the vote our session already makes and changes nothing else:
- **The stress list's main thread.** It runs 28 % of its time on the little cores instead of 79 %. Its p50 / p90 / p99 go from 1.54 / 2.24 / 3.53 to 1.07 / 1.71 / 2.97 ms (the runs' ranges don't overlap, at p50 or p99), below Compose's 1.26 ms p50 of round 9. The `doFrame`s delayed by prefetch steps drop from 58 to 4 in 12 runs.
- **The feed.** Main thread p50 / p99 1.32 → 1.24 / 2.88 → 2.70 ms; the animation's slightly too.
- **Unchanged.** The RenderThread's percentiles and the votes (time-weighted 87 → 94 on the feed, 257 → 247 on the list) don't move: the reports are the same.
- **Power.** No change resolved by 12 runs: CPU rails +1.0 % (± 1.8) scrolling the feed, −0.7 % (± 2.2) the list, −1.8 % and −0.8 % animating; all rails within ± 1 %. The main thread's work rises (404 → 424 ms on the feed, 498 → 610 ms on the list) while its CPU time falls by a third on the list (3.53 → 2.37 s). The work metric scales time by the core's frequency, and at a higher frequency the same code likely spends more cycles waiting for memory.

**Not chosen: c8.** It has the shortest frames by far (feed RenderThread p50 2.38 → 1.54 ms, main thread 1.32 → 0.84 ms), since its vote stays high (294 on the feed, 368 on the list). But it costs power: CPU rails +4.0 % (± 2.1) on the feed's scroll and +2.6 % (± 2.3) on the list's. It also starts more `doFrame`s late (191 against 119 on the feed; none was delayed by our work), as D390's lower targets did. Power is a guard here, and every frame of a's already meets its deadline. c10 boosts the feed's RenderThread more than a (2.00 ms p50, vote 151) for +1.8 % (± 1.4) CPU-rail power, and the list's main thread a little less than b. On the list's RenderThread it is worse at p99 (5.32 against 5.01 ms), and in the feed's animation, where its vote is 19, worse on both threads.

**The compositor's janky frames.** No frame missed the app's deadline in any variant, except one in each of a's and b's stress animations. FrameTimeline counted compositor-side janky frames in 4 of b's 48 runs (1 to 3 each: `Prediction Error`, `SurfaceFlinger Scheduling`, `Display HAL`, `SurfaceFlinger CPU Deadline Missed`), in 6 of c8's, in 2 of c10's and in none of a's. D390's A/B counted 5 against main's 2. The counts are too small to tell noise from an effect of the app's threads finishing earlier. Two of b's runs with them presented 55 and 72 frames a vsync late, on cadence (Android's buffer stuffing recovery didn't start). This is watched in the next rounds. Round 10 of [12](12-android-performance.md), with D440, had none in any measured run of compose-rs's (12 runs: the feed's and the stress screens' scroll and animation, 3 each), nor of Compose's.

### The session's target (D500)

Round 11 ([12](12-android-performance.md), [15](15-android-stress-benchmark.md)) showed the cost of a fixed target. D470 cut the stress list's RenderThread work by 18 %, and our session's vote fell with it (time-weighted 295 → 190; frames starting at 200 or more 77 → 42 %). Its RenderThread's p90 / p99 came out at 3.81 / 5.45 ms, against Compose's 3.63 / 5.27 ms, though at each boost level its frames were the shorter. Its main thread ran 46 % of its time on the little cores (21 % in round 10). The less work we did, the less we were boosted, and the slower each unit of work ran.

**The model.** D441's terms, run closed loop: the NDK client's batching, the HAL's P term on each batch's last report and D term on its rises, and the heuristic boost's floors once 2 of the last 300 reports pass 1.2 × the target. Each traced report's CPU time is rescaled to the modelled vote by the traces' own speed at each vote (work over CPU time, by bins of 50). It ran on 8 traced runs of main, 2 of each scenario; the predictions are in the A/B's results JSON (`model`).

- **Where the vote settles.** Under the target, a batch changes the vote by about −6 × (target − report) + (500 / target) × (the rise over the report before), in milliseconds. That doesn't depend on the vote. The rises average a tenth of the reports, on the feed and the stress list alike (0.097 to 0.103). So the vote stops moving where the reports' mean is 6T² / (6T + 50): 2.85 ms at 6.5 ms, 2.5 ms at 6.0, 2.2 ms at 5.5, 4.2 ms at 8.33. That is about 0.4 of the target.
- **A fixed target is a duration controller.** Above that level the vote rises, and boosted frames get shorter until they are back at it; below it the vote falls. Against 6.5 ms, the RenderThread's time alone held near 2.6 ms on every screen: a lighter screen got less boost, not shorter frames. It isn't a random walk with nothing pulling it back, as D441 described it: the speed's response to the vote pulls it back, weakly.
- **A target scaled to the recent work is unstable.** Scale the target to the reports (their p90 × 1.3 or × 1.6, updated every 120 frames with `APerformanceHint_updateTargetWorkDuration`) and both terms scale with them except the D term, whose gain is 500 / target: as the vote rises, the reports and the target fall together, the decay shrinks and the rises' kicks grow. Modelled, the target ran to its 3 ms bound on every scroll (vote 395 to 465, as at a fixed 3.5 ms, which D390 measured at +17 % CPU power). Not built.
- **HWUI's session**, for comparison (`HintSessionWrapper`, `CanvasContext`): it reports the whole frame, from the main thread's start to the RenderThread's end, less the time waiting for a buffer or the sync, against 70 % of `FrameDeadline − IntendedVsync` (`debug.hwui.target_cpu_time_percent`: 11.62 ms here), and sends `CPU_LOAD_RESET` at the first frame after 100 ms without one. Its reports are a frame's work, so its vote's level follows the frame's load. Reporting both threads' time against one interval does the same with CPU time.
- **Candidates.** Fixed targets of 6.0 and 5.5 ms for the RenderThread's time, and both threads' time against 8.33 ms (D442's c8, before D470 +4.0 % CPU-rail power on the feed). Modelled votes on the feed's scroll: 113–131, 183–208 and 253–321 (main: 100–114); on the stress list 269–320, 320–364 and 297–342 (main: 203–246). At 5.5 ms the stress animation's frames (4.8 ms of the RenderThread's CPU time) cross 1.2 × the target often enough for the heuristic boost: vote 563–653 against 471–473, for no shorter frames, since the RenderThread is already at the ceiling there.

**The A/B.** Interleaved on the device (Pixel 8 Pro, `speed`): 10 runs each of the feed's scroll and animation and the stress scroll and animation, the four variants rotating each round, every other rule unchanged (both threads in the session, the second frame, the 25 ms gap, 4 ms). a: main d978afa (the RenderThread's time against 6.5 ms); b: against 6.0 ms; c: against 5.5 ms; d: both threads' time against 8.33 ms. Percentiles are medians of the runs; work and CPU time are means over 13 s scrolling and 9 s animating from the measured start. Power is the CPU rails' mean over that window (an `android.power` data source, rails polled every 250 ms, as D390 and D440), ± the runs' standard deviation, the difference ± its 95 % interval. Runs whose other processes did more than 1.1 × the scenario's median work are left out of the power figures: 11 of 160, most of them b's rounds 5 and 6 and d's round 4, each with background activity (system_server's or other processes' work up, the app's own unchanged); with them, b's feed scroll reads +1.5 % (± 5.5) and d's feed animation +10.5 % (± 19.5). The janky frames are FrameTimeline's, all runs. The raw runs are `results/2026-10-23-pixel-8-pro-hint-target.json`.

| Feed scroll | a (main) | b (6.0 ms) | c (5.5 ms) | **d** (both, 8.33 ms) |
|---|---|---|---|---|
| RenderThread per frame p50 / p90 / p99 | 2.33 / 3.23 / 4.77 ms | 2.15 / 3.07 / 4.54 ms | 1.86 / 2.79 / 4.09 ms | **1.46 / 2.27 / 3.29 ms** |
| Main thread per frame p50 / p90 / p99 | 1.15 / 1.64 / 2.74 ms | 1.13 / 1.59 / 2.60 ms | 1.01 / 1.50 / 2.49 ms | **0.81 / 1.28 / 2.20 ms** |
| Janky frames, the app's / the compositor's | 1 / 2 | 1 / 0 | 0 / 5 | 0 / 2 |
| `doFrame`s more than 2 ms late (delayed by our work) | 89 (2) | 102 (1) | 79 (1) | 149 (0) |
| Our session's vote, time-weighted; frames at 200 or more | 78; 10 % | 108; 15 % | 162; 29 % | 261; 66 % |
| Main thread / RenderThread on the little cores | 69 % / 22 % | 66 % / 19 % | 54 % / 12 % | 27 % / 4 % |
| Main thread work / RenderThread work | 398 / 654 ms | 409 / 662 ms | 438 / 686 ms | 508 / 773 ms |
| Power: CPU rails | 441 ± 13 mW | −2.3 % ± 2.4 | −1.6 % ± 2.5 | +1.8 % ± 2.5 |
| Power: all rails | 1,649 ± 17 mW | −1.0 % ± 1.1 | −1.0 % ± 1.1 | +0.2 % ± 0.8 |

| Stress scroll | a (main) | b (6.0 ms) | c (5.5 ms) | **d** (both, 8.33 ms) |
|---|---|---|---|---|
| RenderThread per frame p50 / p90 / p99 | 2.49 / 3.59 / 5.15 ms | 2.24 / 3.14 / 4.44 ms | 2.11 / 2.91 / 4.02 ms | **2.06 / 2.89 / 3.93 ms** |
| Main thread per frame p50 / p90 / p99 | 0.99 / 1.56 / 2.65 ms | 0.89 / 1.41 / 2.64 ms | 0.83 / 1.30 / 2.63 ms | **0.83 / 1.27 / 2.60 ms** |
| Janky frames, the app's / the compositor's | 0 / 2 | 0 / 0 | 0 / 0 | 0 / 0 |
| `doFrame`s more than 2 ms late (delayed by our work) | 85 (5) | 114 (1) | 192 (3) | 187 (2) |
| Our session's vote; frames at 200 or more | 214; 53 % | 262; 73 % | 320; 86 % | 332; 91 % |
| Main thread on the little cores | 38 % | 20 % | 12 % | 7 % |
| Main thread work / RenderThread work | 514 / 1,165 ms | 544 / 1,214 ms | 592 / 1,277 ms | 609 / 1,304 ms |
| Power: CPU rails | 452 ± 13 mW | +1.2 % ± 3.3 | +4.1 % ± 2.7 | +2.9 % ± 2.7 |
| Power: all rails | 1,682 ± 25 mW | +0.5 % ± 1.7 | +1.2 % ± 1.2 | +0.6 % ± 1.5 |

| Animations | a (main) | b (6.0 ms) | c (5.5 ms) | **d** (both, 8.33 ms) |
|---|---|---|---|---|
| Feed: RenderThread p50 / p90 / p99 | 2.46 / 3.16 / 4.57 ms | 2.36 / 3.07 / 4.38 ms | 2.10 / 2.90 / 4.05 ms | 2.50 / 3.21 / 4.61 ms |
| Feed: main thread p50 / p90 / p99 | 1.14 / 1.61 / 2.83 ms | 1.12 / 1.56 / 2.73 ms | 1.07 / 1.50 / 2.60 ms | 1.16 / 1.65 / 2.86 ms |
| Feed: our session's vote | 54 | 80 | 120 | 53 |
| Feed: power, CPU rails | 399 ± 18 mW | −1.6 % ± 3.9 | +2.9 % ± 3.7 | +0.7 % ± 3.4 |
| Stress: RenderThread p50 / p90 / p99 | 6.35 / 7.41 / 8.56 ms | 6.37 / 7.41 / 8.39 ms | 6.26 / 7.31 / 8.33 ms | 6.35 / 7.31 / 8.41 ms |
| Stress: main thread p50 / p90 / p99 | 2.28 / 2.91 / 3.99 ms | 2.25 / 2.81 / 3.96 ms | 2.17 / 2.68 / 3.74 ms | 2.28 / 2.85 / 3.93 ms |
| Stress: janky frames (app / compositor) | 1 / 2 | 1 / 0 | 0 / 0 | 1 / 2 |
| Stress: our session's vote | 467 | 487 | 549 | 476 |
| Stress: power, CPU rails | 1,861 ± 66 mW | 0.0 % ± 2.9 | +5.1 % ± 3.1 | −1.1 % ± 3.0 |

**Chosen: d** (D500). Both threads' CPU time since the last frame, against 8.33 ms.
- **The scrolls.** The feed's RenderThread p50 / p90 / p99 go from 2.33 / 3.23 / 4.77 to 1.46 / 2.27 / 3.29 ms and its main thread's from 1.15 / 1.64 / 2.74 to 0.81 / 1.28 / 2.20 ms. The stress list's RenderThread goes from 2.49 / 3.59 / 5.15 to 2.06 / 2.89 / 3.93 ms, below round 11's Compose (2.63 / 3.63 / 5.27 ms), and 91 % of its frames start at 200 or more (round 10: 77 %). Its main thread runs 7 % of its time on the little cores.
- **The vote follows the load.** 53 in the feed's animation (as main's 54: its frames are light, the main thread's part small), 261 on the feed's scroll, 332 on the stress list, 476 in the stress animation (as main's: at the ceiling already). The heuristic boost never engages: 1.2 × 8.33 ms is 10 ms of CPU time, which no frame reaches.
- **Traded.** CPU-rail power +1.8 % (± 2.5) on the feed's scroll and +2.9 % (± 2.7) on the stress list's, the second just resolved; before D470, D442 measured +4.0 % (± 2.1) and +2.6 % (± 2.3). Animating, +0.7 % and −1.1 %. All rails within +0.6 %. The main thread's work rises (+28 % on the feed, +18 % on the list) for less CPU time (−23 %, −17 %): the work metric scales time by frequency. And more `doFrame`s start more than 2 ms after their vsync: 89 → 149 on the feed, 85 → 187 on the list in 10 runs (D442: 119 → 191). None is delayed by our work (2 → 0, 5 → 2); they are woken late, as the lower targets' were in D390's sweep, and none was janky.
- **Jank.** No janky frame of the app's while scrolling (main: one, on the feed), one in the stress animation as main's; 4 compositor-side janky frames against main's 6.

**Not chosen: b** (6.0 ms). The cheapest gain: stress list RenderThread p90 / p99 3.59 / 5.15 → 3.14 / 4.44 ms and main thread p50 0.99 → 0.89 ms, the feed's frames 0.1 to 0.2 ms shorter, the feed's animation's slightly too, with no power change resolved anywhere (−2.3 to +1.2 %). d's gains on the scrolls are 2 to 4 times larger for at most 3 % more on the CPU rails, and the user's priority is frame durations: b is what to fall back to if power must not move. **Not chosen: c** (5.5 ms). Its RenderThread's time alone needs a target close to its frames, and then the stress animation's frames trigger the heuristic boost: +5.1 % (± 3.1) CPU-rail power there, +4.1 % (± 2.7) on the stress list, for durations no better than d's on the list.

## Android: text and the big screen

The `big` screen's first frame (110 ms of main thread) lays out 1,201 paragraphs. Each paragraph is laid out exactly once: no intrinsics and no second measure. Each costs about 35 µs: one JNI call, one `NewStringUTF`, one direct buffer, one `getDesiredWidth` and one `StaticLayout`. simpleperf (3 cold starts, dwarf call graphs) splits the samples:

| Part | ms | Who pays it |
|---|---|---|
| `getDesiredWidth` (each word's first shaping) | 11.9 | the platform; Compose computes this width too |
| StaticLayout's measure (mostly cache hits) | 5.9 | the platform |
| Line breaking (ICU word breaker, greedy breaker) | 6.4 | the platform |
| StaticLayout objects, paints and the rest of the build | 5.0 | the platform |
| JNI: the call 1.2, `NewStringUTF` 1.0, the direct buffer 0.4 | 2.6 | compose-rs |
| Rust encoding and bookkeeping | 0.8 | compose-rs |
| Kotlin glue (method body, spans, paint lookup, maps, results) | 2.3 | compose-rs |

About 84 % is platform work. Compose's profile of its own big screen shows twice as much in the same calls (`getDesiredWidth` 24.1, `StaticLayout.<init>` 42.4, `Layout.draw` 26.3 ms). Of the replay's 24 ms, 18.1 is paragraphs: `Layout.draw` 12.3, render node creation 3.4 (the pool is empty at start), `drawRenderNode` 0.9.

| Prototype | Result (big first frame, n = 10 each) | Verdict |
|---|---|---|
| A: one `PrecomputedText` per paragraph (`perf/text-precomputed`) | text layout 42.3 → 48.0 ms, replay 23.5 → 26.0, TotalTime 182 → 188.5 | worse: it keeps glyph pieces, and drawing from it is slower |
| E: lay out at the constraint first, skip `getDesiredWidth` when a line wrapped (`perf/text-wrap-first`) | text layout 42.4 → 42.0 | flat: the first shaping moves into StaticLayout's measure |
| C: record paragraphs' render nodes on 3 worker threads as they're laid out (`perf/text-async-record`) | replay 23.3 → 6.1, but text layout 42.9 → 61.5 (contention on minikin's cache, the allocator) | a loss |
| C2: the same, started when the replay begins | replay 23.1 → 19.5, traversal −2.9 ms; 33 ms of worker CPU | small |
| D: composition hints each text in batches of 16 per JNI call; 3 workers compute widths (`perf/text-desired-hints`) | text layout 42.2 → 32.7, recompose +3.4, traversal 111.6 → 105.0, TotalTime 185.5 → 172 | a gain, M |
| **F**: D with the main thread only copying batches, plus C2 (`perf/text-combined`) | traversal 111.8 → 101.3, main work 94.0 → 79.0 (−16 %), TotalTime 182 → 174; scroll main work 470 → 451, p99 3.53 → 3.08 ms | the best, M–L, +50 ms (big) and +440 ms (scroll) of worker CPU |

Every prototype's screenshots match the baseline pixel for pixel.
- **Batching alone** (one JNI crossing for many paragraphs, docs/12's next step) is capped by the JNI share: about 2.6 ms of the big frame and about 3 ms per scroll run.
- **A layout cache keyed by (text, style, width)** pays only because this benchmark repeats its texts (271 distinct of 1,201 paragraphs); F's width map benefits from that too, so expect less with real text.
- **F is a prototype, not production code.**
  - Its threading is unproven: it records render nodes and runs `Layout.draw` off the main thread with shared `TextPaint`s.
  - Its recordings bypass the render node pool (D195).
  - The typefaces map became concurrent, and the workers' paints duplicate the main thread's.
  - Its width map is never trimmed.
  - Hints aren't deduplicated in Rust, and D's hints for texts whose constraints leave no choice are never consumed (a leak).

### `BoringLayout` for single lines (#14, done: D330)

Compose's `TextLayout` lays out a text with `BoringLayout` when `BoringLayout.isBoring` returns metrics (one line: no new line, tab, emoji, character that could be right to left, paragraph span, or right-to-left direction) and the line fits its width; the metrics' width is the text's own width, so `getDesiredWidth` isn't called. `TextMeasurer.kt` now does the same on API 33+, with two differences from Compose. A line height is applied to the boring metrics as `LineHeightSpan.Standard` would apply it to the line (Compose's own span rules these texts out, and every Material 3 text has a line height). Below API 33, `BoringLayout` can't use fallback line spacing, so it isn't used. Letter-spaced texts get Compose's half pixel (b/173574230). Without it, a `StaticLayout` can break or ellipsize a line at its own measured width: the check below found 31 such cases among 3,769, and the old code had that bug.

How many paragraphs take it: about 5 in 6. In the feed, 917 of the first 1,100 paragraphs (start and 10 flings); on the big screen, 1,000 of 1,200 (the rest are wrapped bodies).

**Same results.** A debuggable app checks each `BoringLayout` against the `StaticLayout` it replaces, in everything Rust reads: size, line count, baselines, ellipsis, each offset's cursor position, and hit tests at and between them. On a difference it logs it and keeps the `StaticLayout`. On the Pixel 8 Pro, an instrumented build also compared software-drawn pixels:
- every feed and big-screen paragraph matched;
- so did 3,769 random cases: Latin, CJK, Devanagari, Thai, Greek and Cyrillic text; runs with other sizes and backgrounds; underline and strike-through; letter spacing of ±; line heights of 7–100 px; every alignment and justify; ellipsis; 1, 2 or unlimited lines; and widths at +0, +1 and +37 px.

Screenshots of the feed and the big screen are byte-identical to the baseline's. Not checked: API 33–35, where the debug check is the safeguard.

Measured against 84a79be. APKs alternated, each install checked by SHA-1 and compiled `speed`. Results are means [min–max]; startup values are medians.

| | before | after |
|---|---|---|
| Big first frame: `compose-rs: text layout` (1,201 paragraphs) | 41.3 ms | **29.3 ms** (−29 %) |
| Big first frame: `doFrame` (main work) | 104.8 (87.5) ms | **90.3 (73.0) ms** (−14 %) |
| Big first frame: `compose-rs: replay` | 21.9 ms | 19.8 ms |
| TotalTime big / feed (18 starts each) | 167 / 101 ms | **158** / 102 ms |
| Scroll: main work (4 runs) | 460.3 [447.4–471.4] ms | **430.9** [407.9–439.6] ms (−6.4 %) |
| Scroll: `compose-rs: text layout` per paragraph, `prefetch` per call | 383, 2,053 µs | 270, 1,703 µs |
| Scroll: main per frame p50 / p90 / p99 | 1.23 / 1.80 / 2.92 ms | 1.23 / 1.79 / 2.95 ms |
| Scroll: RenderThread work | 789.0 [762.3–807.6] ms | 778.8 [751.5–797.5] ms |
| Scroll: RenderThread per frame p50 / p90 / p99 | 2.74 / 3.77 / 5.33 ms | 2.75 / 3.73 / 5.49 ms |
| Janky frames a run | 0.5 | 0.75 |

Scrolling lays out text while prefetching, in idle time. So the scroll's gain is in total main-thread work, not in frame percentiles; RenderThread and jank stay within the runs' spread.

A wrapped paragraph at a fixed width now pays `isBoring`'s measure before its `StaticLayout`, as in Compose. Minikin's word cache makes the second measure cheap, and the big screen's 200 bodies are included in the numbers above.

## Android: startup

A cold start of the feed (one traced start of each app; ms after the process is created; traced starts are 15–20 ms slower than untraced):

| Phase | compose-rs | Compose |
|---|---|---|
| system_server, launch to fork | ~39 | ~45 |
| fork to `bindApplication` | 16.3 | 16.4 |
| `bindApplication` | 10.5 | 15.5 |
| `activityStart` (of which `performCreate`: dlopen 0.7, `JNI_OnLoad` and the view's init ~1) | 13.2 | 13.9 |
| `activityResume` | 10.0 | 9.9 |
| first `doFrame` | 26.9 | 44.3 |
| of which the app's work | Rust frame 7.3 (55 paragraphs' text 5.3), replay 2.0 | `initializeView` 8.2, recompose 2.6, measure 11.1, draw 7.2 |
| of which waiting for the RenderThread's first frames | 10.8 (a 4 ms first texture upload) | 7.6 |

The app owns about 2 ms of activity creation and the first frame's work. The rest is the system's, and it's the same for both apps.
- **Class loading:** 38 classes in 1.4 ms (Compose: 1,422 in 12.5 ms).
- **JNI lookups:** 1 `FindClass`, 10 method IDs and about 10 lazy native-method resolutions, under 0.2 ms together. `RegisterNatives` isn't worth it.
- **Lazy initialization:** nothing in the startup path takes over 0.3 ms.

**Warm-ups (#7).** Two warm-ups in the first `ComposeRsView`'s constructor: an ALPHA_8 bitmap's `prepareToDraw()` (the RenderThread's first texture upload, which Compose also pays but overlaps with its main-thread draw), and two StaticLayouts laid out on a thread (font files, shape plans). TotalTime against main in the same sessions, pooled over sessions (`perf/startup-text-warmup`):

| | Feed TotalTime | Big TotalTime | n |
|---|---|---|---|
| texture warm-up | −2.1 ms (se 1.1) | −2.5 ms (se 1.3) | 36 each |
| texture and text warm-ups | −4.0 ms (se 1.1) | −2.2 ms (se 1.2) | 36 each |
| both warm-ups and the release profile (#4), another session (`perf/startup-combined`) | −3.1 ms (se 2.1) | −4.1 ms (se 2.0) | 21 each |

With both warm-ups, the first `doFrame` goes from 26.0 to 21.5–21.7 ms and the Rust frame section from 7.37 to 6.33–6.40 ms. TotalTime gains less, because `activityResume` grows by about 1 ms: creating the window's renderer waits for the busy RenderThread. Posting the texture warm-up at attach time instead was worse (big +1.3 ms).

**Compilation modes** (`perf/startup-compilation-modes`, TotalTime medians, n = 12 each):

| Mode | compose-rs feed | compose-rs big | Compose feed | Compose big |
|---|---|---|---|---|
| `verify` (no profile) | 102.0 | 178.0 | 136.0 | 390.0 |
| `speed-profile` | 105.5 | 181.0 | **113.5** | 229.0 |
| `speed` | 105.5 | 179.5 | 129.0 | 227.0 |

compose-rs's `speed-profile` row is really `verify`: its training runs left no profile (the profile saver hadn't written one by the `am force-stop`; see the Baseline Profile below). Its startup doesn't depend on ART's compilation.

Its frames do. Under `verify` (the glue run by the JIT and the interpreter), the replay is 25–30 % slower and main work 5 % higher: 216.9 → 228.3 ms animating, 459 → 484 ms scrolling. A Baseline Profile for `rs.compose.android.*` (#8), shipped with the library, would give Play installs the `speed` numbers. That gain is an upper bound: on a build whose JIT compiles the hot glue sooner, `verify` would be closer to `speed`.

docs/12 called `-m speed` "Compose's best case" (**fixed**: docs/12 now cites this; `measure.py --compile speed-profile` measures with a profile, D302, since D362 each APK's own Baseline Profile rather than a trained one). For the feed's startup it isn't: with a profile, Compose starts in 113.5 ms (its app image), against 129 ms with `speed`. compose-rs still starts first (105.5 ms), by 8 ms rather than the 23.5 ms of the same session's `speed` row.

**The Baseline Profile (#8, done: D360).** The library ships `android/compose-rs/src/main/baseline-prof.txt`, which the Android Gradle plugin merges into each app's `assets/dexopt/baseline.prof`: every class of `rs.compose.android` and every method marked hot, startup and post-startup, as wildcards (AndroidX's form). Neither the Macrobenchmark nor the Baseline Profile Gradle plugin is in the offline Gradle cache, so the rules are written by hand and checked three ways:
- **What ART records.** After the benchmark's screens from a reset (`pm dump-profiles --dump-classes-and-methods`, which flushes the running app's profile: the profile saver hadn't written one before `am force-stop`, which is why the earlier training left compose-rs at `verify`), about 120 of the glue's methods ran, but ART marked only 9 hot (the replay's, `frame`, `TextMeasurer.layout`); `speed-profile` compiles only hot methods, so a profile of what ran would leave most of the frame path to the interpreter. The wildcard marks them all, and the rest (input, back, accessibility).
- **What the APK holds.** profgen's dump of bench-rs's `baseline.prof` lists 370 hot methods, the lambdas and API outlines R8 synthesizes included, under their obfuscated names.
- **What ART compiles.** After the profile is installed (androidx.profileinstaller's broadcast) and `cmd package compile -m speed-profile -f`, the reference profile holds the same 370, and running the screens again adds no method of the package, only profileinstaller's own, run once.

The Kotlin standard library's few calls from the glue (`coerceIn`, `Pair`) aren't listed. bench-compose gets Compose's libraries' profiles (5,514 hot methods merged) but none for its own composables, so it has its own profile too (`rs/compose/bench/compose/**`: 29 more hot methods after R8), and so does bench-rs's activity. Sideloaded installs need androidx.profileinstaller to install a profile: bench-compose has it, and bench-rs now takes it too (+31 KB of dex, +38 KB of APK). The library doesn't depend on it, as it takes no AndroidX: apps get it through AndroidX (`androidx.activity` depends on it) or add it, and Play installs need none.

Interleaved A/B on bench-rs, 3 rounds (`verify` and `speed`: the APK without the profile; `speed-profile`: with it, reset, the profile installed, compiled), means [min–max]; frame durations: medians of the runs; TotalTime: medians of 15:

| | `verify` (no profile) | `speed-profile` (Baseline Profile) | `speed` |
|---|---|---|---|
| Animation: `compose-rs: replay` | 155.5 µs [153–159] | **98.7 µs** [97–100] | 93.0 µs [83–98] |
| Scroll: `compose-rs: replay` | 171.7 µs [167–175] | **110.0 µs** [106–114] | 108.3 µs [105–112] |
| Animation: main work | 206.3 ms [203–209] | **194.7 ms** [193–197] | 198.7 ms [193–210] |
| Scroll: main work | 476.3 ms [468–486] | **448.9 ms** [432–459] | 446.8 ms [444–451] |
| Animation: main p50 / p90 / p99 | 1.30 / 1.86 / 2.96 ms | 1.23 / 1.76 / 2.90 ms | 1.19 / 1.61 / 2.68 ms |
| Scroll: main p50 / p90 / p99 | 1.36 / 2.04 / 3.11 ms | 1.32 / 1.98 / 3.23 ms | 1.27 / 1.87 / 3.01 ms |
| RenderThread p50 / p90, animation; scroll | 2.68 / 3.44; 2.77 / 3.84 ms | 2.69 / 3.55; 2.71 / 3.94 ms | 2.68 / 3.44; 2.75 / 3.71 ms |
| Frame time p50 / p90 / p99 (gfxinfo), animation; scroll | 5 / 6 / 7; 5 / 6 / 8 ms | 5 / 6 / 7; 5 / 6 / 8 ms | 5 / 5 / 7; 5 / 6 / 8 ms |
| TotalTime feed / big | 107 / 167 ms | 107 / 161 ms | 110 / 165 ms |

With the profile, the replay runs within 2–6 % of `speed` and the main thread does the same work (`speed`'s own runs spread wider than the difference); main-thread percentiles recover most of the way, the rest within the runs' spread. The RenderThread doesn't run the glue and doesn't change. Startup doesn't depend on the mode, as before, and profileinstaller's startup cost doesn't show: a fourth variant of the same A/B, the profiled APK under `speed`, started in 107 / 163 ms. No janky frame in the animation; the scroll had 2 in all under `speed-profile` and under `speed`, none of them the app's deadline.

**The library (#4).** The workspace has no `[profile.release]`, and the bench app set no `ndkVersion`, so the Android Gradle plugin silently skipped stripping (**fixed**: D301 pins `ndkVersion`; bench-rs's APK is now 1,985,087 bytes, its `.so` 1,883,512). Main's APK therefore shipped 1.25 MB of symbol tables, which are never mapped. The prototype adds `[profile.android]` (fat LTO, one codegen unit, `panic = "abort"`) and sets `ndkVersion`, so AGP strips and the unstripped copy stays in `build/intermediates` for profilers.

| | main | prototype |
|---|---|---|
| APK | 3.24 MB | 1.50 MB (Compose's: 1.74 MB) |
| `.so` packaged (gzip) | 3.13 MB (1.01) | 1.40 MB (0.66) |
| `.text` | 1.23 MB | 1.00 MB |
| Code PSS after start | 2,052 KB | 1,741 KB |
| `compose-rs: frame`, animation / scroll (3 rounds, means) | 0.223 / 0.236 ms | 0.204 / 0.227 ms |
| Main work, animation / scroll (same runs) | 212.2 / 442.3 ms | 205.2 / 423.3 ms (−3.3 / −4.3 %, about the noise) |
| TotalTime feed / big, means | 107.9 / 180.3 ms | 104.6 / 175.3 ms (n = 15, session prof1); −0.8 / +0.8 ms (se 2, n = 21, session comb1): no consistent change |
| Clean build of the Android library (M4 Pro) | 6.9 s | 9.2 s |
| Rebuild after an edit to feed / to compose-material3 (medians of 3) | 0.40 / 1.9 s | 0.52 / 4.6 s |

The build costs are measured on `perf/combined` (`perf-results/perf-combined/build-cost.txt`). As prototyped, `build-rust.sh` always builds with the new profile, so every Android build pays them, debug builds and the demo app included. A production version would choose the profile per Gradle variant (done: D361, below). Stripping alone (setting `ndkVersion`) takes the APK to 1.99 MB with no build cost, but pins the NDK version every developer must install (build-rust.sh took the newest; since D301 it prefers the pinned one).

Other options:
- `opt-level = "s"`: on top of the prototype, 45 KB smaller (APK 1.497 → 1.452 MB) and frames 11–13 % slower (0.204 → 0.230 ms, 0.227 → 0.251 ms). `"s"` or `"z"` alone, against stripping only: 0.14–0.18 MB smaller, frames about 8 % slower.
- Thin LTO: byte-identical to no LTO.
- Fat LTO adds nothing over one codegen unit.
- `panic = "abort"` is a behaviour change:
  - **Where it changes nothing:** a panic in an `extern "system"` JNI function already aborts the process (Rust 1.81 and later). compose-ui's `measure.rs` catches a panic only to restore state before resuming it.
  - **Where it changes behaviour:** compose-runtime's `background.rs` catches the panics of `spawn_background` work and of its pool threads. Today a panic in awaited work is handed to the awaiting task. On Android that task runs inside a JNI call, so the panic already ends in an abort, but on the main thread rather than the worker's. Panics that today are swallowed would now abort the process: those of cancelled tasks' work, and those in drops and wakes on the pool's threads. The profile is Android's only; desktop keeps unwinding.
  - **Without abort:** fat LTO and one codegen unit alone give a 1.68 MB library instead of 1.40, so an APK of about 1.78 MB rather than 1.50, just above Compose's 1.74.

**As shipped (#4b, done: D361).** Release variants build the library with `[profile.android-release]` (fat LTO, one codegen unit, `panic = "abort"`), debug variants with `[profile.android]` (the release profile with `panic = "abort"`): `build-rust.sh` takes the profile Gradle passes per variant. Interleaved A/B on bench-rs with the Baseline Profile, 3 rounds, `speed`, means [min–max] (the variants ran in the same order each round, and the RenderThread's work grew with the position, 486 → 503 → 539 ms animating, with no Rust code on that thread):

| | release profile | fat LTO, one codegen unit | and `panic = "abort"` |
|---|---|---|---|
| Library / APK | 1.89 / 2.03 MB | 1.68 / 1.82 MB | **1.40 / 1.54 MB** |
| Animation: `compose-rs: frame` | 227.7 µs [226–231] | 222.9 µs [217–235] | 224.4 µs [217–233] |
| Scroll: `compose-rs: frame` | 268.2 µs [267–270] | 262.5 µs [261–265] | 261.0 µs [256–264] |
| Animation; scroll: main work | 191.7; 438.1 ms | 190.2; 437.0 ms | 196.3 (one run 205); 447.4 ms |
| Animation: main p50 / p90 / p99 | 1.21 / 1.77 / 2.99 ms | 1.18 / 1.70 / 2.78 ms | 1.22 / 1.82 / 2.95 ms |
| Scroll: main p50 / p90 / p99 | 1.28 / 1.85 / 2.92 ms | 1.28 / 1.90 / 2.99 ms | 1.29 / 1.93 / 2.98 ms |
| TotalTime feed / big | 108 / 163 ms | 106 / 166 ms | 108 / 162 ms |

The Rust frame is 1.5–3 % shorter, less than the prototype's 4–8 % (since then the frame lost its text layout to `BoringLayout` and prefetching, and the replay is Kotlin's); main work and frame durations don't change beyond the runs' spread, and neither does startup. `panic = "abort"` adds nothing measurable to the speed: it is 0.28 MB of the 0.49 MB. It's in both profiles so that debug builds treat a panic as release builds do, and it costs debug builds nothing.

`panic = "abort"`, checked on the device with a panic in background work nobody awaits (a temporary test app): logcat shows `compose-rs: panic: panicked at …` from the panic hook (D280), then the process aborts on the worker thread (`Fatal signal 6 (SIGABRT)` in `compose-backgro…`), the tombstone's abort message the panic's. With unwinding that panic was swallowed.

Build times of the library (`build-rust.sh`, M4 Pro, medians of 3; incremental: after touching a file, as release builds aren't incremental):

| Profile | Clean | Edit to feed | Edit to compose-material3 |
|---|---|---|---|
| `release` (before; every variant) | 4.7 s | 0.43 s | 2.1 s |
| `android` (debug variants) | 4.4 s | 0.40 s | 1.8 s |
| `android-release` (release variants) | 7.4 s | 0.53 s | 5.1 s |

Through Gradle, `:bench-rs:assembleDebug` after an edit to feed takes 1.05 s, to compose-material3 2.4 s; `assembleRelease` 1.1 and 5.5 s.

The full benchmark with D360 and D361, under `speed` and compiled from the shipped profiles: [12](12-android-performance.md), "Compiled from the shipped Baseline Profiles".

## Android: memory

- **The Java heap after flings** (6.1 against Compose's 4.6 MB) is garbage waiting for a collection, not live data.
  - The live heap is about 2.65 MB. 4.5 MB is allocated during the 10 flings, with one concurrent collection (13–17 ms on `HeapTaskDaemon`, off the frame path).
  - 63 % of it is `StaticLayout` and `MeasuredParagraph` internals. The glue's own share: boxed `Long` group keys 0.41 MB, a `DirectByteBuffer` per frame 0.21 MB, iterators 0.09 MB.
  - The prototype (#12, `perf/startup-java-allocs`) removes most of them. Per animation frame: in `onDraw`, 6.0 objects (176 B) → 0; between draws, 11.2 objects (522 B) → 4.2 (314 B, mostly the framework's). Java allocated over start and 10 flings: 7.09 → 6.61 MB. No frame-time gain, as expected.
- **Native heap during scroll** (12.5 MB):
  - Rust 1.0 → 1.9 MB;
  - about 1.5 MB held by Java garbage (StaticLayout paints 0.77, `MeasuredText` 0.36, line breakers, render nodes);
  - the GPU driver about 4 MB, HWUI's recordings about 3 MB.
- **The big screen:** Rust heap 17.9 MB (49k blocks) for 23,138 groups and 3,615 layout nodes, about 88 KB per post; native PSS 40.2 MB, total PSS 69.8 MB. Compose's big screen, measured the same way: 102.4 MB, with 31–32 MB of Java heap. heapprofd's breakdown of the Rust part:

| What | Size | Note |
|---|---|---|
| Slot table's groups `Vec` | 4.72 MB | 32,768 × 144 B capacity for 23,138 groups: 1.39 MB of doubling slack |
| `KeptArguments::set` (each restart group's arguments) | 4.17 MB | about 21 KB per post |
| Node arena | 2.88 MB | 4,096 × 704 B per `LayoutNode`; 0.34 MB slack |
| `PendingLambda` callbacks | 1.23 MB | |
| Remembered slots | 0.67 MB | |
| Display-list shapes | 0.59 MB | |
| HWUI (not Rust): paragraph render node recordings | 4.9 MB | |

Exact or chunked growth of the groups and the arena would save about 1.7 MB of allocation (#13; unused capacity, so the resident saving may be smaller and is unmeasured), and avoid a transient 2× copy when they grow. Boxing a layout node's cold fields would save about 1 MB (M). A slimmer `Group` or kept arguments: M–L. At most 3–5 MB of 69.8.

- **The stress list's reuse pool** (D460, D461). Round 10's stress list held 1.0 MB more native heap than round 9 after start, and 1.5 MB more after the flings. The list's first layout estimates the items it lacks from its 81 px header alone, composes 87 items, and keeps 38; until D430 the others were disposed, but D430 kept them for reuse, 7 per content type: 30 rows, 531 nodes, 3,563 groups, about 33 KB a row, a pool that then stayed full (an item leaving goes to one coming in the same pass). A slot whose item leaves unmeasured is disposed again (D460), and a group flag that was never read, which grew every `Group` from 144 to 152 bytes, is gone (D461). Headless Rust heap after start 6,405 → 5,428 KiB (5,413 before D430), after 10 flings 7,008 → 5,792 KiB (5,887); on the device, native heap 18.1 → 17.1 MB after start and 22.3 → 20.4 MB after the flings, Java heap 5.0 → 4.6 MB, PSS 97.1 → 94.8 MB (6 runs each, alternating installs; [15](15-android-stress-benchmark.md), "Memory"). What reuse still costs is a group per composed item. The overshoot is gone too (#21, D472): with D470, the stress list's native heap after start is 14.4 MB and 17.3 MB after the flings (main: 17.2 and 20.5 MB; [15](15-android-stress-benchmark.md), "The lazy list's policy"). Round 11 measured 14.7 and 17.2 MB, against Compose's 10.9 and 22.5 MB.

**Graphics memory** is HWUI's caches; the app has no lever on it.

## Runtime core (headless)

**Composing a big screen had a quadratic scan (#5).** (Only the 200-post screen was measured, not the scaling.)
- Every layout node reads `LOCAL_LAYOUT_DIRECTION`, so that local's state cell gathers thousands of readers.
  - Each new read searched all of them (`register_read`, crates/compose-runtime/src/state.rs): 23 % of the big screen's first frame.
  - Each local read also walked 60–90 ancestor groups to find its provider (crates/compose-runtime/src/local.rs).
- The prototype's fixes (`perf/runtime-compose`; what ships differs, see "Made production-ready" below):
  - search a cell's 8 latest readers only. A scope found further back is recorded again. The copies are harmless (invalidating a scope twice does nothing more), and they're pruned once the scope recomposes; until then a scope that reads one cell many times, with other readers in between, holds a copy per read;
  - give frames and groups a link to the nearest provider group.
- A local read with its provider 60 groups up goes 133 → 20.5 ns.

| Probe scenario (median of 5 round medians, one session) | main | readers | locals | both |
|---|---|---|---|---|
| big first frame (200 posts) | 7,039 µs | 5,116 | 6,210 | 4,317 (−39 %) |
| of which recompose | 6,365 | | | 3,596 (−44 %) |
| feed first frame | 287 | 273 | 238 | 228 (−21 %) |
| animation frame, feed drag, lazy scroll | 1.46, 2.12, 18.1 | | | unchanged |

A second session, main against both fixes plus a tighter walk over clean siblings (`perf/runtime-compose`, 3 rounds), gave 7,051 → 4,318 µs (big) and 280 → 227 µs (feed). Criterion's counter click went 244 → 252 ns (+3 %; the prototype attributed it to a frame growing from 56 to 64 bytes, but D310 keeps frames at 56 bytes and measures the same +3 %).

All 1,044 tests of the runtime, UI, foundation, Material, animation, feed and todo crates pass. On the device, composing the big screen takes 22 ms. The device is 3.45× the Mac's warm run, but only 1.5× its first, cold run (`perf/text-analysis`), so the headless −44 % says little about the device. The full benchmark ("Combined") measured 22.1 → 16.5 ms in one traced start, with the release profile also in.

**Made production-ready (D310).** Two changes to the prototype:
- A cell's copies of readers are bounded. When its readers fill their room, a compaction drops the stale ones and the copies, then doubles the room only if more than half is still in use. The room holds less than four records per scope reading the cell at once (four to start with), at a constant amortized cost per read. The prototype kept copies until they went stale: their number grew with reads rather than scopes, and without bound for a scope whose epoch doesn't move.
- The nearest provider is recorded in each group when it is allocated (a group's ancestors never change), not in each frame: `Frame` stays 56 bytes and `Group` 144.

Measured with criterion: new benches `first frame, feed` and `first frame, big screen` in `ui` (as the probe's), and three in `runtime`. Main (152a608) against D310, 3 interleaved rounds (ABBA), medians of the rounds' estimates:

| Bench | main | D310 |
|---|---|---|
| first frame, big screen | 7.73 ms | 4.91 ms (−36 %) |
| first frame, feed | 294 µs | 247 µs (−16 %) |
| 1,000 local reads, the provider 60 groups up (recompose) | 145 µs | 26.5 µs (−82 %) |
| initial composition of 2,000 readers of one state | 9.82 ms | 0.335 ms (−97 %) |
| write a state 2,000 readers read (recompose all) | 1.78 ms | 0.228 ms (−87 %) |
| toggle 1 of 1,000 rows (runtime) | 2.82 µs | 2.57 µs (−9 %, the walk's tight loop) |
| counter click (runtime) | 248 ns | 255 ns (+3 %) |
| others (initial composition of rows, keyed reorders, ui frames) | | within −5 to +2.4 % |

A first session with the prototype's frame link gave the same picture (big screen 7.37 → 4.63 ms, −37 %; counter click +2.9 %), so the counter click's +3 % doesn't come from the frame's size.

**Per-operation costs** (one scope recomposed doing 1,000 of each):

| Operation | Cost |
|---|---|
| `remember` hit | 6.2 ns |
| replaceable group | 17.6 ns |
| keyed (movable) group | 19 ns |
| reused leaf node and its update | 25 ns |
| skipped restartable child | 32 ns |
| state read in composition | 6.3–6.9 ns |
| state read outside composition | 5.3 ns |
| initial composition | about 60 ns a group, 300 ns a row |

After the fixes, the big screen's profile is diffuse: memmove 12.7 %, malloc and free about 13 %, `start_group` 4.1 %, `SlotTable::alloc` 4.0 %. Collapsing the per-operation thread-local and composition lookups and shrinking `Group` (144 bytes; Compose's groups are about 20 bytes and a slot 8) would be an L-sized job for perhaps 10–15 % of composition.

**Allocations** (counted, deterministic):

| Scenario | Allocations | Note |
|---|---|---|
| feed first frame | 3,329 (1.6 MB) | |
| big first frame | 55,902 (28.7 MB) | about 17 MB of it `Vec` regrowth |
| animation frame | 8 (320 B) | all in the executor: a new `Arc<TaskWaker>` per poll, an `Rc<Cell>` per frame await, a `Vec` per `run_ready`; avoidable (S) |
| lazy scroll frame, 2 items in and 2 out | 124 | |
| todo toggle | 40 | |

Steady-state recomposition allocates nothing (compose-ui's allocation test): modifier chains aren't rebuilt. `Text!("literal")` expands to `format!` and allocates twice per (re)composition (`format_args!(…).as_str()` would make it static); there are few literal texts per screen.

**No big gains are left in steady frames.**
- Headless, the animation frame is 1.46 µs and a feed drag frame 2.1 µs. The device's `compose-rs: frame` is 217 µs.
- The device-to-Mac ratio that holds for big composition (3.45×) doesn't hold for frames of a few microseconds. What the device spends there is fixed per-frame overhead (ATrace sections, the bridge, cold caches, little cores), not the runtime's algorithms.

Other findings, with no device impact on today's screens:
- **Reversing 1,000 keyed rows** takes 1.34 ms and is quadratic: matching scans siblings linearly, where Compose builds a key map on the first mismatch (M).
- **The todo toggle's 195 µs** is mostly three hit tests over all ~7,000 nodes. `hit_node` doesn't cull by bounds; a phone with touch input tests about 160 nodes once.
- **Observed reads in layout and draw** (`register_observed_read`) still scan a cell's observed list linearly: the same quadratic pattern if many nodes read one cell while measuring or drawing. It didn't show in these scenarios.

## Desktop

What compose-desktop does today (crates/compose-desktop/src/lib.rs, crates/compose-skia/src/render.rs):
- **When it renders:** only when invalidated, so idle is idle (0.0 % CPU, no frames). Animation frames are paced by `about_to_wait` at the display's interval, without a display link: 125–142 fps on a 144 Hz display, not aligned to vsync.
- **What it renders:** the whole window every frame. No damage regions, and no per-group cache (D270's groups are drawn by reference).
- **How it presents:**
  - the Skia surface is copied into softbuffer's buffer, then a second full pass masks the alpha;
  - on macOS, softbuffer allocates a new zeroed buffer every frame and presents it as a DeviceRGB `CGImage` set as a layer's contents;
  - at the commit, Core Animation converts that image to the display's colour space on the CPU: 80 % of the main thread's samples at 2560 × 1324, about 1.6 ns a pixel.

Per animated frame of the gallery (its spinner), and per click in the todo app (1,000 todos), medians of 3 runs:

| Scenario | Variant | App CPU | ms of CPU per frame | Raster | Copy and mask | Present | Click → present |
|---|---|---|---|---|---|---|---|
| Gallery, 420 × 760 | main | 32.1 % | 2.55 | 0.53 | 0.10 | 1.77 | |
| | colour space tagged | 19.5 % | 1.58 | 0.62 | 0.08 | 0.25 | |
| | Metal | 19.5 % | 1.62 | 0.56 | 0 | 0.02 | |
| Gallery, 2560 × 1324 | main | **96.5 %** | **6.79** | 0.84 | 0.42 | **5.37** | |
| | colour space tagged | 46.9 % | 3.64 | 1.06 | 2.24 (new buffer, page faults) | 0.09 | |
| | Metal | **24.9 %** | **2.04** | 0.80 | 0 | 0.03 | |
| Todo click, 1280 × 800 | main / tagged / Metal | | | 1.18 / 1.18 / 0.74 | 0.58 / 1.53 / 0 | 5.72 / 0.23 / 0.04 | **7.72 / 3.28 / 0.97** |
| Todo click, 2560 × 1324 | main / tagged / Metal | | | 2.28 / 2.56 / 0.97 | 2.07 / 5.12 / 0 | 11.27 / 0.34 / 0.05 | **16.44 / 8.62 / 1.30** |

Notes on this table:
- **The "colour space tagged" variant** also skips the alpha mask pass (`COMPOSE_PERF_NO_MASK=1`), which is worth 0.2 ms at this size.
- **Frame rates differ:** main runs 142 fps at 2560 × 1324, Metal 122 and tagged 129. So the CPU percentages are partly fewer frames; milliseconds per frame are the fair comparison. Why Metal paces lower (its drawables, or dropped frames) wasn't investigated.
- **WindowServer** takes 40–52 % of a core for any window animating at 120–144 Hz. Per frame, Metal raises it: 3.2 → 4.2 ms at 2560 × 1324 (45 → 52 % of a core at fewer frames). The app and WindowServer together go from 10.0 to 6.3 ms a frame (−37 %).
- **Clicks** are timed from the injected click to the return of the present call, excluding the OS's input delivery. Metal's present returns before the GPU finishes; waiting for the GPU, a click takes 1.4 ms.

Turning a 48 dp spinner in a 2560 × 1324 window keeps a core busy. At 4K and 2× the CPU path would cost about 15–17 ms a frame (rasterizing 1.5 ms warm, copying 0.9 ms, presenting about 13 ms): too slow for 60 Hz. docs/11's "fast up to about 1080p" holds for Skia's rasterizing alone (0.5–0.85 ms) but not for the frame.

- **GPU (#1).** `perf/desktop-metal` adds `COMPOSE_GPU=1`: Skia's Ganesh backend on a `CAMetalLayer`, about 130 lines (crates/compose-desktop/src/metal.rs). Recording and flushing cost 0.56–0.97 ms of CPU whatever the window's size, and the GPU is barely loaded (waiting for it, a gallery frame is 0.86 ms).
  - **First frame:** +4–6 ms with Metal's shader cache warm, 12–159 ms cold.
  - **Missing:** the layer's colour space (sRGB, so the compositor converts on the GPU), a raster fallback, screenshots by GPU readback (the prototype has one), live resizing and device loss. Unsafe code in compose-desktop (today `forbid(unsafe_code)`). skia-safe's `metal` feature's prebuilt binaries (they download).
  - **Other platforms:** Windows needs D3D or GL, Linux GL or Vulkan: M per platform.
  - **Compose Desktop (Skiko) does the same:** Metal on macOS, OpenGL on Linux and Direct3D on Windows, falling back to software (`SkikoProperties.kt`).
- **Present without conversion (#9).** Tagging the image with the window's colour space (a patched softbuffer, `perf/desktop-present`) removes the conversion, but also colour management. What's left is softbuffer's new 13.5 MB buffer every frame (2.2 ms); reusing buffers was worse (copy-on-write faults). The proper CPU path is an IOSurface-backed layer drawn into directly (estimated present about 0.1 ms; M, macOS only). Dropping the alpha mask pass saves 0.2 ms at 3.4 MP (S).
- **Damage tracking (estimated).**
  - Rasterizing only the changed pixels would take the spinner frame from 837 to about 29 µs at 2× 1280 × 800, and the todo toggle from 506 to about 24 µs (a lower bound; groups' bounds would be larger, about 40–60 µs).
  - On today's present path that's at most 0.8 of about 7 ms: copying and presenting are full-frame anyway.
  - It pays only with a present that takes partial updates (an IOSurface with buffer age, Wayland, X11 or Win32 damage), where a spinner frame would cost about 0.15–0.3 ms. On the GPU path, not worth it.
  - Cost M–L: device-space bounds per group, a diff per key, buffer-age bookkeeping, and stale pixels as the failure mode.
- **Caching groups as `SkPicture`s or raster layers: no.** Walking the whole list costs 9.5–18 µs of 180–1,506 µs (1–5 %), and blitting a cached layer costs about what drawing it does. Clearing the surface alone is 25 % of rasterizing the spinner frame at 2× and 4K, and 37–48 % of the todo toggle's; only damage tracking avoids it.

## Build times

Clean builds (seconds, 3 runs, `CARGO_INCREMENTAL=0`, Skia's binaries from a local copy):

| Build | main | build-only crates at opt-level 0 (#6) |
|---|---|---|
| `cargo build -p todo` (debug) | 11.8 | 8.6–9.8 |
| `cargo build -p gallery` (debug) | 12.0 | 8.8–9.0 |
| `cargo test --no-run` (workspace) | 16.1 | 12.6 |
| `--release -p todo` | 8.4 | |
| feed for Android (`--release --target aarch64-linux-android`) | 6.1–6.5 | |

**A clean debug build is slower than a release build.** `[profile.dev.package."*"] opt-level = 1` also applies to build-script dependencies, and it outranks `[profile.dev.build-override]`. So bindgen, regex, clang-sys and cc compile optimized in debug builds only, on the critical path: proc-macro2 → syn (6.1 s) → prettyplease → bindgen (5.1 s) → skia-bindings' build script → skia-safe → the app.
- bindgen is compiled although Skia's binaries are prebuilt: it's a non-optional build dependency of skia-bindings, whose script then runs in 0.3 s.
- The fix (#6, **applied**: D303) lists those crates at opt-level 0, keeping syn, quote and proc-macro2 at 1: at 0 they slow every `#[composable]` expansion (material3's 67 → 115 ms).
- The list needs upkeep when skia-bindings' build dependencies change.
- compose-rs's own crates are small in clean builds: compose-ui 1.1 s, compose-macros 1.2, material3 0.85 in debug; compose-ui 2.4 s in release.

**The edit loop is at its floor.** The README's "about 0.5 s" holds:

| Edit | Incremental rebuild |
|---|---|
| a `Text` in examples/todo | 0.43–0.46 s |
| examples/gallery | 0.58–0.66 s |
| compose-material3's `Checkbox`, then todo | 0.71–0.84 s |
| feed for Android, release | 0.375 s |
| material3 for Android | 1.78 s |

Of the todo edit's 0.45 s:
- cargo itself: 0.08 s (a no-op build costs the same);
- the todo library: 0.13–0.16 s (expansion 15 ms, type checking 13, borrow checking 11, code generation about 40, incremental bookkeeping about 45);
- the 11-line binary: 0.20–0.22 s, of which Apple's linker is 0.12 s.

Macro expansion is 8–13 % of a crate's clean compilation, but 23 % of an incremental compose-material3 edit (70 of 310 ms), as expansion is redone in full on every edit: caching expansions, or smaller ones, is a possible further gain for framework edits, not measured. A `call!` averages 1.8 KB in gallery, mostly default arguments written out. The macro crate's own optimization level doesn't matter (67 → 65 ms at opt-level 3).

**Monomorphization has no hot spot.**
- Drop glue is 4–5 % of each crate's LLVM IR.
- Callback memoization (`lambda_probe`, `resolve_lazy_*`, `store_callback`) is 16–20 % of an app crate's, already instantiated per signature rather than per lambda. Halving it would save about 2 % of a gallery edit.

Two corrections to docs/08 (**made**: D304):
- Its `compose/dynamic` feature doesn't exist. A prototype (`perf/build-dynamic`: a `compose-dylib` crate, as `bevy_dylib`) saves 50–80 ms on app edits (todo 0.45 → 0.39 s) and costs about 0.5 s on framework edits (a 39 MB dylib to relink). docs/08's premise that relinking dominates an untuned rebuild ("5–15 s untuned, 1–3 s tuned", restarting the app included) doesn't hold on macOS, where Apple's linker links the static 28 MB binary in 0.12 s.
- Call macros aren't purely declarative: `Name!` forwards to the `call!` proc macro.

## Combined: the cheap Android and runtime changes together

`perf/combined` merges #2, #3 (with the spinner boundary, neutral in work, +7 µs of replay), #4, #5 and #7 onto main without conflicts; the crates' tests pass. It leaves out #11 (worker threads, not production-ready) and #12 (it conflicts with #3 and gains no frame time). Measured with the full benchmark (`measure.py --runs 15`) on the same afternoon: the combined APK first, then main's. Each session alternates compose-rs and Compose run by run, but the two sessions weren't interleaved with each other. Compose's own numbers moved by up to 5 % between them, so differences under about 5 % are within drift. Medians, and `compose-rs:` section averages over the 3 traces of each scenario (`perf-results/perf-combined/`):

| | main | combined | Compose (the two sessions) |
|---|---|---|---|
| Startup `feed` (TotalTime) | 107 ms | 104 ms | 126 / 130 ms |
| Startup `big` | 181 ms | **166 ms** (−8 %) | 224 / 222 ms |
| `big` first frame: traversal, composition, layout (one traced start) | 113.2, 22.1, 49.9 ms | 99.2, 16.5, 44.8 ms | 160–164 ms traversal |
| PSS after start, of which code | 33.3, 1.9 MB | 33.3, 1.6 MB | 51.3–51.6 MB |
| PSS after 10 flings, of which Java heap | 80.7, 6.1 MB | 80.4, 5.9 MB | 84.9–85.7 MB |
| Scrolling: main work | 483 ms | 442 ms (−8 %) | 867 / 826 ms |
| Scrolling: RenderThread work | 789 ms | 773 ms | 900 / 864 ms |
| Scrolling: main p50 / p90 / p99 | 1.27 / 2.24 / 3.86 ms | 1.22 / 2.22 / 3.50 ms | 1.33–1.42 / 2.97–2.99 / 5.97–6.38 ms |
| Animation: main work | 210 ms | **196 ms** (−7 %) | 248 / 260 ms |
| Animation: RenderThread work | 547 ms | **492 ms** (−10 %) | 534 / 544 ms |
| Animation: main p50 / p90 / p99 | 1.33 / 2.11 / 3.07 ms | 1.22 / 1.81 / 2.93 ms | 1.48–1.50 / 2.04–2.28 / 3.00–3.30 ms |
| Animation sections: frame, draw, replay, `Record View#draw()` | 230, 66, 218, 290 µs | 205, 53, 146, 208 µs | |
| APK | 3.24 MB | **1.50 MB** | 1.74 MB |

With these changes, compose-rs's animation does about 25 % less main-thread work than Compose's and 10 % less RenderThread work (previously even). The interleaved A/B of #2, #3 and the spinner boundary alone (`perf/frame-combined`, 3 runs each) agrees: main work 209.7 → 194.0 ms, RenderThread work 546.6 → 496.5 ms, with Compose at 250–253 and 537–545 ms right after. The big screen starts 15 ms sooner.
- **Composition** went from 22.1 to 16.5 ms (−25 %) in the one traced start of each session, short of the −44 % measured headless. Two things blur the attribution. The release profile is in too: fat LTO and one codegen unit took 4–8 % off the Rust frame on their own. And the headless gain is a warm-run figure: the device's composition is 1.5× the Mac's cold first run, but 3.45× its warm one. On the device, 7.6 of composition's 21 sampled ms are malloc, realloc and memcpy (the text investigation's profile), which the fixes don't touch.
- **Layout** (49.9 → 44.8 ms) is most likely the text warm-up and the release profile; it's one traced start too.
- **Scrolling's −8 %** is at the edge of the drift between sessions. With interleaved runs, #3 alone measured −2.6 % and the release profile −4.3 %.

## What isn't worth doing

| Idea | Why not (evidence) |
|---|---|
| Batching text layout's JNI calls (docs/12's next step), on its own | the JNI share is 2.6 ms of the big frame, about 3 ms per scroll run |
| `PrecomputedText` per paragraph | big first frame +7 ms (text-precomputed) |
| Skipping `getDesiredWidth` when a line wraps | flat: the shaping moves (text-wrap-first) |
| Recording paragraphs on workers while laying out | contention: text layout +18 ms (text-async-record, C) |
| The spinner as a repaint boundary alone | no change (frame-spinner-boundary); with #10, done (D323) |
| Dropping the per-paragraph render node | at most 4.5 ms of the big frame, and the animation frames that re-record groups would pay (D153) |
| `RegisterNatives`, lazier initialization at startup | under 0.2 ms and 0.3 ms |
| `opt-level = "s"`/`"z"` for Android | on top of #4, 45 KB smaller and frames 11–13 % slower |
| Thin LTO | byte-identical output |
| Optimizing steady-state runtime frames | 1.5–2.1 µs headless; the device's frame cost is fixed overhead |
| Hit-test culling | helps the todo benchmark (about −100 µs), not a phone |
| Caching desktop groups as `SkPicture`s or layers | 1–5 % of rasterizing |
| Damage tracking with today's softbuffer present | at most 12 % of the frame |
| lld instead of Apple's linker | 0.235 s against 0.12 s |
| The proc macro at opt-level 3 | 67 → 65 ms of expansion |
| Thinning monomorphization | about 2 % of an edit |
| Avoiding the system's per-frame costs on Android | about 60 % of the animation's main thread, the same in Compose; inherent to a `View` |

## Caveats

- **Gradle could package a stale Rust library.** The `buildRust` task declared no outputs; twice during this work an APK held the previous build's `.so`. Every measured APK's library was checked (sha of the packaged `.so`). **Fixed** (D300): the task's output directory is each variant's generated `jniLibs`, and measure.py checks the installed libraries against the built ones (D302).
- **No JIT for framework methods on this build:** compiled with `-m speed`, neither app showed JIT-compiled code (Android 16, `husky_beta` canary build), so framework methods missing from the boot image stayed interpreted. On builds where the JIT compiles hot framework methods, #3's gain would be smaller. Under `-m verify` the app's own glue does run JIT code (#8).
- **Benchmark text repeats** (271 distinct paragraphs of 1,201), which favours any width cache (#11).
- **Desktop** was measured on a 1× display: 2× and 4K come from the headless renderer bench.
- **Cold starts** use `am force-stop` only, so the library stays in the page cache; reading it from storage isn't measured.

## Branches

All from main 9f80586; each holds `perf-results/<branch>/README.txt` with the exact commands, raw JSON and logs. None is pushed.

| Branch | Head | Contents |
|---|---|---|
| `perf/frame-roundcap` | 1b70b63 | #2 |
| `perf/frame-replay` | bfa3317 | #3; baseline profiles and per-slice work attribution |
| `perf/frame-eager-text` | 4fa362d | paragraphs recorded while prefetching |
| `perf/frame-spinner-boundary` | 3913f21 | the spinner as a repaint boundary (no gain) |
| `perf/frame-combined` | 34a22a8 | replay, round cap and spinner boundary together |
| `perf/text-analysis` | c9ba57e | profiles of both apps' big frames, scripts, a headless big-frame bench |
| `perf/text-precomputed` | f4a2545 | A |
| `perf/text-wrap-first` | 3ee8912 | E |
| `perf/text-async-record` | 4255797 | C (bf672a0) and C2 (3a3f9a6) |
| `perf/text-desired-hints` | 20cd610 | D |
| `perf/text-combined` | b6eda9c | F (#11) |
| `perf/startup-texture-warmup` | 5ffcaf9 | texture warm-up |
| `perf/startup-text-warmup` | dcb22bf | both warm-ups (#7); start timelines |
| `perf/startup-warmup-attach` | ee16137 | warm-up at attach (worse) |
| `perf/startup-release-profile` | a091410 | #4 |
| `perf/startup-combined` | 99b723e | warm-ups and release profile |
| `perf/startup-compilation-modes` | 54fb4c5 | `verify`, `speed-profile`, `speed` |
| `perf/startup-java-allocs` | 7112360 | #12 |
| `perf/startup-java-allocs-probe`, `perf/startup-memory-probe` | 1bd0280, 876466c | allocation probes, heapprofd and ART profiles, the big screen's census |
| `perf/runtime-probe` | f222eef | probe binary, micro benchmarks, baseline profiles |
| `perf/runtime-readers`, `perf/runtime-locals` | 55e28c5, c664718 | each fix alone |
| `perf/runtime-compose` | 468fb5d | #5 (both, and the sibling walk) |
| `perf/desktop-frame-timing` | a985015 | frame phase timings, a headless renderer bench |
| `perf/desktop-present` | a6567da | #9 (patched softbuffer) |
| `perf/desktop-metal` | 0933d2e | #1 |
| `perf/build-dev-profile` | 2780f70 | #6; clean and incremental timings, llvm-lines |
| `perf/build-dynamic` | 41c3384 | the `dynamic` feature of docs/08 |
| `perf/combined` | f5dcf3e | #2–#5 and #7 together, with the full benchmark |

Reproducing a device A/B: build each APK (`android/gradlew -p android :bench-rs:assembleRelease` in the branch's worktree), check the packaged `.so`, then run the branch's script (for example `perf-results/perf-frame-combined/scripts/ab.py`), which alternates the APKs, checks each install's SHA-1, compiles with `-m speed` and traces each run with measure.py's configuration. The full benchmark:

```bash
python3 android/bench/measure.py --trace-processor path/to/trace_processor_shell --out out --runs 15
python3 android/bench/summarize.py out/report.json
```

Headless: `cargo run --profile bench -p compose-benchmarks --example probe -- report big_first feed_first anim` (on `perf/runtime-probe` and its descendants); `cargo bench -p compose-benchmarks --bench desktop_render` (on `perf/desktop-frame-timing`).

## See also

- [12](12-android-performance.md): the comparison with Jetpack Compose these numbers start from.
- [13](13-android-drawing-paths.md): drawing on Android, and option D (our own Skia), which would also remove HWUI's recording that #3's ceiling keeps.
- [08](08-fast-builds.md): the build loop.
