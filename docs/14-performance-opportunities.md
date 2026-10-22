# 14. Performance opportunities: what's left, measured

Where compose-rs can still get faster, smaller or quicker to build, after round 3 of [12](12-android-performance.md). Each opportunity was measured with a prototype where feasible, or estimated from profiles with the reasoning shown. Small and negative results are reported too. This is an analysis, not a change: nothing here is merged. The prototypes are kept on `perf/*` branches, each with its raw measurements and a `README.txt` of exact commands under `perf-results/<branch>/` (see "Branches" below).

**Short answer.**
- **Desktop** has the one big gain left. A Metal surface takes 70 % off the app's CPU time per animated frame in a 2560 × 1324 window (6.8 → 2.0 ms). At that size, a click's time to present goes from 16.4 ms (median of 3; 7.0–16.9) to 1.3 ms. Today's CPU path spends most of each frame not rasterizing but presenting: Core Animation converts the frame's colour space on the CPU.
- **Android** has several cheap gains, each a few percent to 10 % of a frame or startup metric, and no big one:
  - a round cap on the spinner: −8 % of the animation's RenderThread work;
  - a replay without interpreted framework calls: −8 % of its main-thread work;
  - two runtime fixes that remove a quadratic scan when composing big screens: −39 % of the big screen's first frame headless;
  - a release profile: the APK goes from 3.2 to 1.5 MB (−54 %).
  - **Together**, measured with the full benchmark ("Combined" below; the two sessions ran one after the other, not interleaved): the animation's main-thread work goes from 210 to 196 ms and its RenderThread work from 547 to 492 ms (Compose: 248–260 and 534–544). `big` starts in 166 ms instead of 181 (Compose: 222–224), and the APK is 1.5 MB. An interleaved A/B of the frame changes alone agrees: 209.7 → 194.0 ms and 546.6 → 496.5 ms.
- **Text** on Android is 84 % platform work that any approach pays. Batching JNI calls would save at most 2.6 ms of the big screen's 110 ms first frame.
- **Build times:** the edit loop is at its floor (about 0.45 s). Clean debug builds can lose 27 %.

## Ranking

Gain over cost, best first.
- **"Measured"** means a prototype measured against main (9f80586) on the same device or machine, alternated run by run. A/B results are means over the runs, with [min–max]. Full-benchmark results are medians, as in [12](12-android-performance.md).
- **"Estimated"** means from profiles, with the reasoning in the section named.
- **Sizes** are decimal megabytes: main's APK is 3.24 MB here and "3.1 MB" (MiB) in docs/12.

| # | Opportunity | Gain (metric) | Evidence | Cost, risk | Behaviour or API change | Branch |
|---|---|---|---|---|---|---|
| 1 | **Desktop: GPU surface** (Skia on Metal; D3D or GL elsewhere) | App CPU per animated frame at 2560 × 1324: 6.79 → 2.04 ms (−70 %); click → present 16.4 [7.0–16.9] → 1.30 [1.14–1.55] ms. WindowServer's CPU rises 45 → 52 % | measured, 3 runs each | M per platform: a raster fallback, resizing, device loss, unsafe code in compose-desktop; the prototype has no colour management | none in the API; colours need the layer's colour space set (not yet measured) | `perf/desktop-metal` |
| 2 | **Android: the spinner's round cap** (`IndeterminateCircularProgressIndicator`, as Material 3 1.4) | Animation RenderThread work −44 ms (544.4 → 500.1, −8.1 %) | measured, 3 runs | S, none | the default cap's look, on every platform (Compose's own default); the determinate indicator keeps `Butt` | `perf/frame-roundcap` |
| 3 | **Android: replay without interpreted calls** (floats as int bits, one reused buffer, sizes tracked in Kotlin, no host wake for frame awaiters) | Animation main work −16.8 ms (213.9 → 197.1, −7.9 %); scroll −12 ms (−2.6 %, ranges overlap) | measured, 3 runs | S, low | internal JNI signatures; the executor skips the host wake for `dispatch_frame`'s wakes on every host (a thread-local flag, not reset if a waker panics) | `perf/frame-replay` |
| 4 | **Android: release profile and stripping** (fat LTO, one codegen unit, `panic = "abort"`; AGP strips once `ndkVersion` is set) | APK 3.24 → 1.50 MB (−54 %, below Compose's 1.74 MB); code PSS −0.3 MB; Rust frame section −4 to −8 %, main work −3 to −4 % (about the noise); startup unchanged | measured | S. Builds: clean 6.9 → 9.2 s; a material3 edit rebuilds the Android library in 4.7 s instead of 1.9 | a panic in `spawn_background` work aborts the process instead of reaching the awaiting task | `perf/startup-release-profile` |
| 5 | **Runtime: state readers and composition locals** (a read searches a cell's 8 latest readers; a local read walks provider groups only) | Headless big-screen first frame −39 % (7.04 → 4.32 ms), composition −44 %; feed first frame −20 %. On the device, with #2–#4 and #7 (one traced start): big composition 22.1 → 16.5 ms | measured headless, 3–5 rounds | S/M, low | none (a cell can hold duplicate readers until they're stale) | `perf/runtime-compose` |
| 6 | **Build: build-only dependencies at opt-level 0** in the dev profile | Clean debug build of todo 11.8 → 8.6 s (−27 %), workspace `test --no-run` 16.1 → 12.6 s (−22 %); edit loop unchanged | measured, 3 runs | S (Cargo.toml; the list needs upkeep) | none | `perf/build-dev-profile` |
| 7 | **Android: startup warm-ups** (an ALPHA_8 bitmap's texture upload, and two StaticLayouts on a thread, when the first `ComposeRsView` is made) | TotalTime feed −4.0 ms (se 1.1, −4 %), big −2.2 ms (se 1.2, −1 %); the first `doFrame` 26.0 → 21.5 ms | measured, n = 36 | S; `activityResume` grows by about 1 ms | none | `perf/startup-text-warmup` |
| 8 | **Desktop: present without the colour conversion** (tag the image with the window's colour space; properly, an IOSurface-backed layer) | Present 5.37 → 0.09 ms at 2560 × 1324; app CPU per frame 6.79 → 3.64 ms | measured (tagging, with the alpha pass also off), estimated (IOSurface) | S–M (a softbuffer fork or our own layer code) | tagging turns off colour management | `perf/desktop-present` |
| 9 | **Android: a Baseline Profile for the Kotlin glue** | For installs that aren't fully compiled (Play), at most: replay +25–30 % and main work +5 % recovered; startup unchanged | measured (`-m verify` against `speed`) | S | none | `perf/startup-compilation-modes` |
| 10 | **Android: records that reference nested groups by key**, so a parent's version doesn't change with a nested group's content | Animation main work −5 to −10 ms (−3 to −5 %) on top of #3 | estimated ("Nested groups") | M–L, medium | none | — |
| 11 | **Android: text widths and paragraph recording on worker threads** | Big first frame main work 94.0 → 79.0 ms (−16 %), TotalTime 182 → 174 ms; scroll main work −19 ms, p99 3.53 → 3.08 ms; costs 50 ms (big) and 440 ms (scroll) of worker CPU | measured, n = 10 and 4 | M–L, threading risk; the prototype isn't production-ready (see "Android: text and the big screen") | none | `perf/text-combined` |
| 12 | **Android: fewer Java allocations per frame** | Per animation frame 17.2 → 4.2 Java objects; −0.48 MB allocated over start and 10 flings; Java heap PSS after flings 5.40 → 4.86 MB (single runs); no frame-time gain | measured | S | none | `perf/startup-java-allocs` |
| 13 | **Memory: exact or chunked growth** of the slot table's groups and the node arena | Big screen's Rust heap −1.7 MB of 17.9 (its PSS is 69.8 MB) | estimated (heapprofd) | S–M | none | — |

Small, negative or not worth doing: see "What isn't worth doing".

## Method

- **Android:** Pixel 8 Pro, Android 16 (`user` build), apps compiled with `cmd package compile -m speed -f` unless stated. Each prototype was A/B tested against main (9f80586), with APKs alternated run by run, each installed APK checked by SHA-1, and each APK's packaged library checked.
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

**JNI crossings** are few: about 6 per animation frame (`frame()`, `takeDrawing()`, `tick()` from a posted message, one `wake()` upcall per woken frame awaiter, the `NewDirectByteBuffer` constructor). Scrolling adds `touch()` per input sample, `requestFrame` upcalls, one `prefetch()` per idle slot with about 6 text-layout upcalls per item, and release upcalls. Their cost is small: text's JNI overhead is about 5 % of text layout.

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

**Ceiling.** If the replay, serializing and taking the drawing all cost nothing, the animation's main work would drop by about 24 ms (12 %) after this prototype. About 40 % of what's left is `Canvas` and `RenderNode` calls that any path through HWUI makes, so the realistic remaining gain on this path is about 12 ms (6 %), #10 included. (Option D of [13](13-android-drawing-paths.md), our own Skia, would also drop HWUI's recording and sync, so this doesn't bound it.) docs/13's "about 0.2 ms a frame" for the replay was one-third interpreter overhead.

Smaller items left:
- `display?.refreshRate` in `prefetch()` (1.9 sampled ms per scroll run; cache it: S);
- serializing copies each group's bytes twice (under 1 %);
- rounded clips go through `Path.addRoundRect` + `clipPath` (about 15 % of the animation's replay); public `Canvas` has no `clipRRect`, and a `RenderNode` outline clip would be the alternative.

### Nested groups (#10, estimated)

Making the spinner a repaint boundary of its own (`perf/frame-spinner-boundary`) gained nothing: main work 209.7 → 209.4 ms. A group's version changes whenever a nested group's content changes (D155, D157: a record holds its nested records' bytes). So the header card around the spinner is still serialized and recorded again in Kotlin every frame, plus one more group.

With records that reference nested groups by key, the header's re-recording would go away. Before #3, that re-recording was 16.4 of the replay's 31.3 sampled ms (the arc itself about 1 ms), which would have meant −12 to −15 ms of the animation's main work. #3 has since cut the replay by a third (194 → 128 µs a frame), much of it in the same re-recording, and the two don't add: on top of #3 the estimate is −5 to −10 ms (3 to 5 %), within the 12 ms ceiling above. This revisits D155's rejected alternative now that D270 made the display list a tree. Cost M–L: serialize.rs and `Replayer.kt`.

### Prefetch

The "about 2.1 ms per item" quoted for prefetching is the wall-clock time of a `prefetch()` call (2.0 ms on average, 351 calls per run), mostly spent at low clock speeds: the big cores sit at about 550 MHz between frames. The work is 0.58 ms per item (about 182 items in 10 flings, 1.9 calls each). Of that work:
- text layout through JNI 52 % (StaticLayout's build 25 %; `getDesiredWidth`'s callees run interpreted, 20 %);
- composition 26 %;
- applying and disposing the items behind 10 %;
- other layout about 5 %;
- `update_focus_listeners` 3.6 %.

The frames that reveal an item (about 175 per run) are the slow ones: replay 1.04 ms against 0.09, `doFrame` 2.42 against 1.24 ms. Recording each paragraph's render node while prefetching (`perf/frame-eager-text`, docs/12's third next step) cuts the main thread's p90 and p99 by 15 % each (2.17 → 1.84 ms, 3.53 → 3.00 ms). But it raises total main work by 4 % (460 → 480 ms): prefetching's work rose by 33 ms to save 18 ms in frames. Some prefetched paragraphs are never shown; the rest of the difference is unexplained (the investigation's guess: the same recording costs more on the little cores, where idle-time work runs). It moves work to shorten the slowest frames; it doesn't reduce work. Recording the item's group ahead too would need Rust to draw unplaced items (M–L), for about 0.15 ms more per revealing frame.

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

compose-rs's `speed-profile` row is really `verify`: its training runs left no profile worth compiling. Its startup doesn't depend on ART's compilation.

Its frames do. Under `verify` (the glue run by the JIT and the interpreter), the replay is 25–30 % slower and main work 5 % higher: 216.9 → 228.3 ms animating, 459 → 484 ms scrolling. A Baseline Profile for `rs.compose.android.*` (#9), shipped with the library, would give Play installs the `speed` numbers. That gain is an upper bound: on a build whose JIT compiles the hot glue sooner, `verify` would be closer to `speed`.

docs/12 calls `-m speed` "Compose's best case". For the feed's startup it isn't: with a profile, Compose starts in 113.5 ms (its app image), against 129 ms with `speed`. compose-rs still starts first (105.5 ms), by 8 ms rather than the 23.5 ms of the same session's `speed` row.

**The library (#4).** The workspace has no `[profile.release]`, and the bench app sets no `ndkVersion`, so the Android Gradle plugin silently skips stripping. Today's APK therefore ships 1.25 MB of symbol tables, which are never mapped. The prototype adds `[profile.android]` (fat LTO, one codegen unit, `panic = "abort"`) and sets `ndkVersion`, so AGP strips and the unstripped copy stays in `build/intermediates` for profilers.

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
| Rebuild after an edit to feed / to compose-material3 | 0.40 / 1.9 s | 0.51 / 4.7 s |

The build costs are measured on `perf/combined` (`perf-results/perf-combined/build-cost.txt`). Debug Android builds keep today's profile.

Other options:
- `opt-level = "s"`: on top of the prototype, 45 KB smaller (APK 1.497 → 1.452 MB) and frames 11–13 % slower (0.204 → 0.230 ms, 0.227 → 0.251 ms). `"s"` or `"z"` alone, against stripping only: 0.14–0.18 MB smaller, frames about 8 % slower.
- Thin LTO: byte-identical to no LTO.
- Fat LTO adds nothing over one codegen unit.
- `panic = "abort"` is a behaviour change:
  - **Where it changes nothing:** a panic in an `extern "system"` JNI function already aborts the process (Rust 1.81 and later). compose-ui's `measure.rs` catches a panic only to restore state before resuming it.
  - **Where it changes behaviour:** compose-runtime's `background.rs` catches the panics of `spawn_background` work and of its pool threads. Today such a panic is handed to the awaiting task, or dropped if the task was cancelled. With abort, the panic kills the process. Taking the profile without abort keeps that behaviour, and gives up about 0.35 MB of the library (stripped: 1.88 → 1.54 MB from abort alone: `.gcc_except_table` −165 KB, `.eh_frame` −41 KB, `.text` −135 KB).

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

Exact or chunked growth of the groups and the arena would save about 1.7 MB (#13), and avoid a transient 2× copy when they grow. Boxing a layout node's cold fields would save about 1 MB (M). A slimmer `Group` or kept arguments: M–L. At most 3–5 MB of 69.8.

**Graphics memory** is HWUI's caches; the app has no lever on it.

## Runtime core (headless)

**Composing a big screen had a quadratic scan (#5).** (Only the 200-post screen was measured, not the scaling.)
- Every layout node reads `LOCAL_LAYOUT_DIRECTION`, so that local's state cell gathers thousands of readers.
  - Each new read searched all of them (`register_read`, crates/compose-runtime/src/state.rs): 23 % of the big screen's first frame.
  - Each local read also walked 60–90 ancestor groups to find its provider (crates/compose-runtime/src/local.rs).
- The fixes:
  - search a cell's 8 latest readers only. A scope found further back is recorded again. The copies are harmless (invalidating a scope twice does nothing more), and they're pruned once the scope recomposes; until then a scope that reads one cell many times, with other readers in between, holds a copy per read;
  - give frames and groups a link to the nearest provider group.
- A local read with its provider 60 groups up goes 133 → 20.5 ns.

| Probe scenario (median of 5 round medians, one session) | main | readers | locals | both |
|---|---|---|---|---|
| big first frame (200 posts) | 7,039 µs | 5,116 | 6,210 | 4,317 (−39 %) |
| of which recompose | 6,365 | | | 3,596 (−44 %) |
| feed first frame | 287 | 273 | 238 | 228 (−21 %) |
| animation frame, feed drag, lazy scroll | 1.46, 2.12, 18.1 | | | unchanged |

A second session, main against both fixes plus a tighter walk over clean siblings (`perf/runtime-compose`, 3 rounds), gave 7,051 → 4,318 µs (big) and 280 → 227 µs (feed). Criterion's counter click went 244 → 252 ns (+3 %: a frame grows from 56 to 64 bytes).

All 1,044 tests of the runtime, UI, foundation, Material, animation, feed and todo crates pass. On the device, composing the big screen takes 22 ms. The device is 3.45× the Mac's warm run, but only 1.5× its first, cold run (`perf/text-analysis`), so the headless −44 % says little about the device. The full benchmark ("Combined") measured 22.1 → 16.5 ms in one traced start, with the release profile also in.

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
- **Frame rates differ:** main runs 142 fps at 2560 × 1324, Metal 122 and tagged 129. So the CPU percentages are partly fewer frames; milliseconds per frame are the fair comparison.
- **WindowServer** takes 40–52 % of a core for any window animating at 120–144 Hz. Metal raises it a little: 45 → 52 % at 2560 × 1324.
- **Clicks** are timed from the injected click to the return of the present call, excluding the OS's input delivery. Metal's present returns before the GPU finishes; waiting for the GPU, a click takes 1.4 ms.

Turning a 48 dp spinner in a 2560 × 1324 window keeps a core busy. At 4K and 2× the CPU path would cost about 15–17 ms a frame (rasterizing 1.5 ms warm, copying 0.9 ms, presenting about 13 ms): too slow for 60 Hz. docs/11's "fast up to about 1080p" holds for Skia's rasterizing alone (0.5–0.85 ms) but not for the frame.

- **GPU (#1).** `perf/desktop-metal` adds `COMPOSE_GPU=1`: Skia's Ganesh backend on a `CAMetalLayer`, about 130 lines (crates/compose-desktop/src/metal.rs). Recording and flushing cost 0.56–0.97 ms of CPU whatever the window's size, and the GPU is barely loaded (waiting for it, a gallery frame is 0.86 ms).
  - **First frame:** +4–6 ms with Metal's shader cache warm, 12–159 ms cold.
  - **Missing:** the layer's colour space (sRGB, so the compositor converts on the GPU), a raster fallback, screenshots by GPU readback (the prototype has one), live resizing and device loss. Unsafe code in compose-desktop (today `forbid(unsafe_code)`). skia-safe's `metal` feature's prebuilt binaries (they download).
  - **Other platforms:** Windows needs D3D or GL, Linux GL or Vulkan: M per platform.
  - **Compose Desktop (Skiko) does the same:** Metal on macOS, OpenGL on Linux and Direct3D on Windows, falling back to software (`SkikoProperties.kt`).
- **Present without conversion (#8).** Tagging the image with the window's colour space (a patched softbuffer, `perf/desktop-present`) removes the conversion, but also colour management. What's left is softbuffer's new 13.5 MB buffer every frame (2.2 ms); reusing buffers was worse (copy-on-write faults). The proper CPU path is an IOSurface-backed layer drawn into directly (estimated present about 0.1 ms; M, macOS only). Dropping the alpha mask pass saves 0.2 ms at 3.4 MP (S).
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
- The fix (#6) lists those crates at opt-level 0, keeping syn, quote and proc-macro2 at 1: at 0 they slow every `#[composable]` expansion (material3's 67 → 115 ms).
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

Two corrections to docs/08:
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

With these changes, compose-rs's animation does about 25 % less main-thread work than Compose's and 10 % less RenderThread work (previously even). The interleaved A/B of #2 and #3 alone (`perf/frame-combined`, 3 runs each) agrees: main work 209.7 → 194.0 ms, RenderThread work 546.6 → 496.5 ms, with Compose at 250–253 and 537–545 ms right after. The big screen starts 15 ms sooner.
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
| Recording prefetched paragraphs ahead (docs/12's next step) | p99 −15 %, but total main work +4 % (frame-eager-text) |
| The spinner as a repaint boundary alone | no change (frame-spinner-boundary); needs #10 |
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

- **Gradle can package a stale Rust library.** The `buildRust` task declares no outputs; twice during this work an APK held the previous build's `.so`. Every measured APK's library was checked (sha of the packaged `.so`). The fix is to declare the task's outputs and feed `jniLibs` from them.
- **No JIT for framework methods on this build:** compiled with `-m speed`, neither app showed JIT-compiled code (Android 16, `husky_beta` canary build), so framework methods missing from the boot image stayed interpreted. On builds where the JIT compiles hot framework methods, #3's gain would be smaller. Under `-m verify` the app's own glue does run JIT code (#9).
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
| `perf/desktop-present` | a6567da | #8 (patched softbuffer) |
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
