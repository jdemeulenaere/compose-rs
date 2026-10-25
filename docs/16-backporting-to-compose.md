# 16. Backporting compose-rs's optimizations to Jetpack Compose

compose-rs is a Rust port of Jetpack Compose's design ([01](01-feasibility.md)). On Android it draws through the same public `RenderNode` and `Canvas` APIs as Compose, and HWUI renders both apps the same way ([13](13-android-drawing-paths.md)). Since D560, compose-rs runs by default in a **parity mode**: it does the work Compose 1.12.1 does by default, and each optimization of its own is a flag, off unless an app opts in (D581: the hint session too). Each flag was measured alone against parity on a Pixel 8 Pro, then all of them together ([12](12-android-performance.md), [14](14-performance-opportunities.md), [15](15-android-stress-benchmark.md)).

This document is for engineers working on Jetpack Compose, HWUI or Android graphics who want to decide which of those optimizations to backport, and how. For each candidate it gives what it changes in Compose's terms, the evidence, the mechanism, a sketch of the change against Compose's structure, and how to measure it in Compose. Candidates come in order of measured benefit and confidence.

**A note on names.** The Compose classes, methods and line numbers quoted in [15](15-android-stress-benchmark.md), "Compose's defaults (version 1.12.1)", were read from the 1.12.1 sources jars. Other Compose, HWUI and Skia names below come from our knowledge of their structure. They weren't re-read for this document, so check them against current sources before relying on them. The power HAL's controller terms come from the Pixel power HAL's sources and the device's `powerhint.json` (D441).

## Short answer

| # | Candidate | Where the change goes | Measured alone against parity (5 runs a side; \* = Mann–Whitney p < 0.05) | Confidence |
|---|---|---|---|---|
| 1 | **Report each frame's CPU work (main thread plus RenderThread) to the hint session, against one vsync interval** | HWUI (`HintSessionWrapper`, `CanvasContext`); the platform | Feed scroll: every percentile on both threads −24 to −46 %\*. Stress list: −30 to −41 %\*. Animations: none resolved. CPU-rail power +1.8 % (± 2.5) and +2.9 % (± 2.7) scrolling (D500, 10 runs) | High for frame durations; power is just resolved on one screen |
| 2 | **Record a prefetched item's layers in the idle time** | `LazyLayoutPrefetchState`'s request, after premeasure | Main thread p90 / p99: −13 / −17 %\* (feed), −15 / −23 %\* (stress list). RenderThread and work unchanged | High in compose-rs. Medium for Compose, where creating a layer is tied to placement |
| 3 | **`CompositingStrategy.Auto` fades a layer holding one drawing by its paint** (as `ModulateAlpha`) | `GraphicsLayerV29.updateLayerProperties`, or HWUI | Stress animation: RenderThread work −42 %\*, RenderThread p50 / p99 −16 / −19 %\*, main thread p99 −34 %\*. Alone it also grows HWUI's GPU caches by 31 %\* (see #4). Nothing on the scrolls | High on screens that fade many simple layers; none elsewhere |
| 4 | **Free the dropped path masks' textures before the frame draws** | HWUI / Skia's GPU resource cache | Its own A/B (D510, 8 runs): graphics memory 150.6 → 122.9 MB 3 s into the stress animation. At parity: GPU caches −18 %, not resolved. With #3, it brings the caches back to parity's | Medium: one screen; memory varies run to run |
| 5 | **Keep a paragraph while the width it's laid out at doesn't change** | `ParagraphLayoutCache` / `MultiParagraphLayoutCache.newLayoutWillBeDifferent` | Resize screen (D551, 6 runs, not in the each round): text layouts 51,047 → 3,609, main-thread work −52 % | Medium: exact by construction, but one ad hoc screen |

Candidates 1, 3 and 4 interact with each other through HWUI. Less RenderThread work means less boost (#1 explains why), and removing offscreen layers removes the flush that freed dropped path masks (#3 and #4).

## Reading the numbers

- **Rounds.** Three rounds on 2026-10-25, all with compose-rs at 3ac74a6 against Compose 1.12.1 (material3 1.4.0):
  - **parity:** both apps, 3 traced runs a scenario;
  - **all:** both apps, every compose-rs optimization on;
  - **each:** compose-rs alone, at parity and with each flag alone, 5 runs a subject, interleaved ([Methodology](#methodology)).
- **Effects.** "Alone" means the each round: the change of the flag's median over its runs against parity's median. **\*** marks an exact two-sided Mann–Whitney test at p < 0.05. With 5 runs a side, that means at most 2 of the 25 pairs of runs go the other way. The round ran about 400 tests, so about 1 in 20 of the stars on a flag that changes nothing are expected by chance.
- **Scenarios.** Most numbers come from four of measure.py's scenarios:
  - **feed scroll:** 10 flings through a `LazyColumn` of Material 3 `Card` posts;
  - **animated header:** a spinner, a pulsing dot and a turning star over the feed;
  - **stress list:** a `LazyColumn` of four heterogeneous item kinds, with nested `LazyRow` carousels;
  - **stress animation:** 140 cells, each with its own infinite transition, read in `graphicsLayer { }`, plus 50 stroked sine waves redrawn every frame.
- **Durations.** "Main thread" means `Choreographer#doFrame` durations and "RenderThread" means `DrawFrame` durations, both from Perfetto.
- **Work.** CPU time scaled by each core's frequency and capacity: milliseconds on the fastest core at its highest frequency ([12](12-android-performance.md), "Method"). It reads higher for the same code on boosted cores, so a flag that boosts shows more "work" for less CPU time.
- **Boost.** The RenderThread's `uclamp.min` from ADPF, out of 1024. The power HAL traces it as `adpf.<pid>-…-HWUI-…-min`. It is reported as a mean weighted by time over the run's frames.

## 1. The hint session: report the frame's work, not its span (HWUI, platform)

### What it is

**HWUI today** (`HintSessionWrapper`, `CanvasContext`) has a session holding the main thread, the RenderThread and HWUI's worker threads. Its reports and target are:
- **The report:** each drawn frame's span, from the main thread's start (`FrameStartTime`) to the RenderThread's end, less the time waiting for a buffer or the sync.
- **The target:** 70 % of `FrameDeadline − IntendedVsync` (`debug.hwui.target_cpu_time_percent`), which is 11.62 ms on this device at 120 Hz.
- **Gaps:** HWUI sends `CPU_LOAD_RESET` at the first frame after 100 ms without one.

**compose-rs's own session** (`performance_hint.rs`; D390, D440, D500; the `performance_hint` flag) works differently:
- **The threads:** it holds the RenderThread and the main thread.
- **The report:** at the start of each frame, both threads' CPU time since the last frame. That is two `clock_gettime` calls on the threads' CPU clocks. While frames come every vsync, this is the RenderThread's previous frame plus the main thread's frame and its idle-time work (prefetch).
- **The target:** a fixed 8.33 ms, one 120 Hz interval.
- **Its rules:**
  - frames less than 4 ms apart count once;
  - after a gap of more than 25 ms, the time since starts a new count and isn't reported;
  - the session is opened at the second frame after a view shows and closed when the last view hides.
- **Combining sessions:** the Pixel power HAL gives a thread the highest active vote of the sessions holding it (`SessionTaskMap::getTaskVoteRange`). On the device, the RenderThread's requested `uclamp.min` matched the higher of the two sessions' votes in 63 of 71 samples (D390). So compose-rs's session adds to HWUI's.

The backport is for HWUI to report what compose-rs reports, so that every app gets it without a session of its own.

### The evidence

Each round, 5 runs a side ([14](14-performance-opportunities.md), "Backport candidates"):

| `performance_hint` alone | Feed scroll | Stress list scroll | Animated header | Stress animation |
|---|---|---|---|---|
| Main thread p50 / p90 / p99 | 1.27 / 2.15 / 3.35 → 0.82 / 1.41 / 2.54 ms (−35 / −34 / −24 %\*) | 1.37 / 2.31 / 3.87 → 0.90 / 1.48 / 2.69 ms (−35 / −36 / −30 %\*) | none resolved | none resolved |
| RenderThread p50 / p90 / p99 | 2.61 / 3.49 / 5.20 → 1.42 / 1.98 / 2.91 ms (−46 / −43 / −44 %\*) | 3.15 / 4.69 / 6.30 → 2.02 / 2.76 / 3.85 ms (−36 / −41 / −39 %\*) | none resolved | none resolved |
| Boost, time-weighted | 31 → 313 | 82 → 326 | 10 → 60 | 613 → 641 (HWUI's session already boosts it) |
| Work (main / RenderThread) | +53 / +32 %\* | +43 / +23 %\* | +9 / −1 % | +4 %\* / +0 % |

The work rises because the same frames run at higher clocks (see "Work" above): the threads' CPU time falls.

**Beside Compose** (the parity and all rounds, 3 runs each):

| | Feed scroll RenderThread p50 / p99 | Stress list RenderThread p50 / p99 |
|---|---|---|
| Compose, parity session | 2.11 / 4.91 ms | 2.61 / 5.45 ms |
| compose-rs at parity | 2.50 / 5.27 ms | 3.13 / 6.47 ms |
| compose-rs, all | 1.48 / 3.19 ms | 2.09 / 4.02 ms |
| Compose, all session | 2.05 / 5.18 ms | 2.51 / 5.13 ms |

At parity, compose-rs's RenderThread is the slower one on both scrolls, though it does 17 % (feed) and 32 % (stress list) less work. HWUI's session boosts it little: 84 % of its feed frames start unboosted (Compose's: 48 %), and its time-weighted boost is 38 against Compose's 129 (stress list: 67 against 277). At each boost level, compose-rs's frames are as short or shorter: unboosted p50 2.57 against 2.79 ms, at 100–199 1.93 against 2.05 ms ([12](12-android-performance.md), "The RenderThread's boost").

**In the all round,** the scrolls' RenderThread gains are the hint session's alone: no other flag moves the RenderThread. On the main thread's p90, its gains compound with #2 (feed −40 % with all, against the flags' product of −36 %).

### Why it works

**Less work leads to less boost, and to slower frames.** HWUI's report is a frame's span. When an app's main-thread frames get shorter, its reports get shorter, the controller lowers the vote, and the RenderThread runs on the little cores at low clocks. The same RenderThread work then takes longer. compose-rs met this four times:
- **At parity** (above): less work than Compose, yet a slower RenderThread.
- **D470**, which cut the stress list's RenderThread work by 18 %, made compose-rs's own session (then reporting the RenderThread alone against 6.5 ms) vote less: 295 → 190. Its RenderThread's p90 / p99 came out at 3.81 / 5.45 ms, above Compose's 3.63 / 5.27, though shorter at each boost level.
- **In the each round,** `draw_prefetched_ahead` shortens the frames that reveal items, and HWUI's vote on the feed falls from 31 to 17 (−45 %\*).
- **Also in the each round,** `single_drawing_fade` takes 42 % off the stress animation's RenderThread work, and HWUI's vote falls from 613 to 444 (−28 %\*). HWUI's worker threads then spend more CPU time on the same work (9.44 → 10.89 s, median of 5; every fade run above every parity run), at lower clocks (D511).

So any Compose optimization that shortens the main thread's frames also lowers the RenderThread's boost.

**The controller** (the device's ADPF profile, `REFRESH_120FPS`; D441, D501):
- **The client batches.** The NDK client sends a report over the target at once. Under the target, it sends the first 83 ms of a run of reports, then one batch per 83 ms.
- **The proportional term** uses the batch's last report: 2.0 per 100 µs over the target, 0.6 per 100 µs under it.
- **There is no integral term.**
- **The derivative term** adds 500 / (target in 100 µs) per 100 µs by which the batch's last report exceeds the one before, on rises only.
- **The vote** stays between 2 and 480.
- **The heuristic boost** sets floors of 230–410 once 2 of the last 300 reports exceed 1.2 × the target.

Under the target, a batch therefore changes the vote by about −6 × (T − report) + (500 / T) × (the rise over the report before), in milliseconds, whatever the vote. In compose-rs's traces, the rises average a tenth of the reports (0.097 to 0.103, on the feed and the stress list alike). So the vote settles where the reports' mean is 6T² / (6T + 50): about 0.4 of the target (2.85 ms at 6.5 ms, 4.2 ms at 8.33 ms; D501).

What sets the vote's level is how much the reports vary from frame to frame, against a fixed decay. With HWUI's span against 11.62 ms:
- **Compose's** reports (about 4 ms on the feed) lost 45 a batch to the decay and gained 37 from the derivative: vote 113–137.
- **compose-rs's** (D390 builds) lost 47 and gained 21: vote 18–27.

A fixed target makes the controller hold a duration, not a load. Against 6.5 ms, compose-rs's RenderThread-only reports held near 2.6 ms on every screen, so a lighter screen got less boost, not shorter frames (D500).

**Both threads' CPU time against one interval** puts that level where a frame's load is. In D500's A/B the vote followed the load: 53 in the feed's animation, 261 on the feed's scroll, 332 on the stress list and 476 in the stress animation. The heuristic boost never engaged: 1.2 × 8.33 ms is 10 ms of CPU time, which no frame reached. CPU time rather than a span leaves out waits (the sync, the GPU), which a boost doesn't speed up.

**Alternatives measured and rejected:**
- **Lower fixed targets** for the RenderThread alone. At 3.5 and 4.5 ms no frame was unboosted, for +17 % and +10 % CPU power (D390's sweep). 5.5 ms set off the heuristic boost in the stress animation (+5.1 % ± 3.1 CPU power for no shorter frames).
- **HWUI's own target (11.62 ms) with both threads' time:** feed RenderThread p50 2.67 against 2.38 ms (vote 43).
- **A target scaled to the recent reports:** in the model, unstable. It ran to its lower bound on every scroll (D501), so it wasn't built.
- **A mean over 4 frames against 8.33 ms:** the vote fell to 13–56.

### How to backport it

The change is in HWUI's per-frame report, plus a fixed target:

```cpp
// HintSessionWrapper / CanvasContext, where HWUI reports a drawn frame today (a sketch)
// Today: report(frameEnd - FrameStartTime - waits), target 0.7 * (FrameDeadline - IntendedVsync).
// Proposed: both threads' CPU time since the last report, against one vsync interval.
int64_t ui = threadCpuNanos(uiThreadTid);       // the UI thread's CPU clock, read from the RenderThread
int64_t rt = threadCpuNanos(gettid());          // the RenderThread's own
int64_t work = (ui - mLastUiCpu) + (rt - mLastRtCpu);
mLastUiCpu = ui; mLastRtCpu = rt;
if (sinceLastFrame <= 25ms && work > 0) mSession->reportActualWorkDuration(work);
// Target: 8.33 ms, set once (APerformanceHint_updateTargetWorkDuration).
```

- **Reading another thread's CPU clock.** compose-rs builds the clock id from the thread id, `(~tid << 3) | 6`, which is the kernel's per-thread scheduler clock (`clock_gettime`). `pthread_getcpuclockid` gives the same clock from a `pthread_t`. It is one system call; compose-rs measured about 6 µs a frame of main-thread work for the read and the report together (D390).
- **When to read.** Reading both clocks at the end of each frame and reporting the deltas since the last report sums to the same work over consecutive frames. The main thread's idle-time work (Compose's prefetch) is then counted, as compose-rs counts it.
- **Which threads get the vote.** The main thread must stay in the session. With only the RenderThread boosted, the stress list's main thread ran 79 % of its time on the little cores, and 58 `doFrame`s in 12 runs waited for a prefetch step; with it in the session, 28 % and 4 (D440). HWUI's workers can stay in the session to get the vote. Adding their time to the report would put the vote at its ceiling in the stress animation (about 16 ms of CPU a frame against 8.33; D511).
- **The target.** 8.33 ms was measured at 120 Hz only. compose-rs keeps it at other refresh rates on the reasoning that the frame's work, not its budget, sets the vote's level; this is unmeasured.
- **Rollout.** Behind a system property first, as `debug.hwui.target_cpu_time_percent` is, so that the power cost can be measured per device. The controller is the vendor's: these numbers are the Pixel power HAL's, and another vendor's ADPF implementation may respond differently.
- **What doesn't transfer.** compose-rs runs a second session beside HWUI's because it can't change HWUI. A library can do that too (compose-rs finds the RenderThread by its name in `/proc/self/task`), but the session belongs in HWUI.

### Power and other costs

- **Power** isn't measured in the rounds. The A/Bs measured the CPU rails with an `android.power` data source (rails polled every 250 ms; ± is the 95 % interval of the difference):

  | A/B | Runs | Scrolling | Animating | All rails |
  |---|---|---|---|---|
  | D500, both threads against 8.33 ms | 10 | +1.8 % ± 2.5 (feed), +2.9 % ± 2.7 (stress list) | +0.7 % (feed), −1.1 % (stress) | within +0.6 % |
  | D390, the RenderThread against 6.5 ms | 6 | +2.1 % | +2.3 % | +0.4 to +0.5 % |
  | D442's "c8" (before D470 cut the work) | 12 | +4.0 % ± 2.1 (feed) | | |

  D390's differences were not resolved: their 95 % intervals ran from about −3 to +7 %. What costs is the frequency, not more cycles: the boosted RenderThread ran 9 % less CPU time for about the same work (D390).
- **The power HAL itself** ran 34 ms more CPU a scroll run (+5 %) for compose-rs's session's reports (D390).
- **Late frames.** More `doFrame`s start more than 2 ms after their vsync: 89 → 149 (feed) and 85 → 187 (stress list) in 10 runs. None was delayed by the app's own work, and none was janky (D500). The each round shows the same direction (feed 6 → 16 a run, not resolved). The threads finish earlier and the main thread is woken late; the cause wasn't established.

### How to measure it

- **Perfetto.** The HWUI session's `adpf.<pid>-…-HWUI-…-min` counter, and `DrawFrame` durations bucketed by it (under 100, 100–199, 200 or more). The RenderThread's and main thread's running time on the little cores (sched slices with the cores' `cpu_capacity`). The power HAL's per-session `-pid.pOut`, `-pid.dOut`, `-min` and `-batch_size` counters show the controller at work.
- **Macrobenchmark.** `FrameTimingMetric` (`frameDurationCpuMs` p50 / p90 / p99, `frameOverrunMs`) on a fling. `PowerMetric` on a device with power rails.
- **Screens.** A light `LazyColumn` fling, where Compose's main-thread frames are shortest and the boost lowest. A heavy list. An idle animation, to check that power doesn't move where nothing needs a boost.

## 2. Record a prefetched item's layers in the idle time (`LazyLayoutPrefetchState`)

### What it is

**Compose's prefetch** ([15](15-android-stress-benchmark.md), "Compose's defaults" (b)):
- A request composes the item (pausable), applies it, prefetches nested lazy layouts, and premeasures it (`LazyLayoutPrefetchState.kt:622-744`).
- Nothing places it, so no layer is created and nothing is recorded. A coordinator's render node is made at placement (`NodeCoordinator.placeSelf`; `updateLayerBlock` only `if (layoutNode.isAttached && layerBlock != null)`).
- So the frame that first places the item creates its layers and records them all on the main thread: the card's clip layer, an elevated card's shadow layer, a carousel's clip layer, and every paragraph drawn into them.

**compose-rs's `draw_prefetched_ahead`** (D350, with D322; D564) adds a step to the same idle time, after the measurement:
1. **Draw ahead.** It draws the measured item's unplaced repaint boundary and the boundaries nested in it, each in its own coordinates.
2. **Record ahead.** It records their `RenderNode`s, kept apart until a frame names them.
3. **The placing frame.** That frame draws the item's parent again (a translation and a `drawRenderNode` by reference), not the item, so it records nothing new for the item.
4. **The gate.** The step starts only if its estimate fits 0.5 ms before the next frame. The estimate is the larger of a running average and a recent maximum (D341).
5. **The cap.** Nodes recorded ahead and never placed are dropped all at once at 8 items or 128 groups. A kept recording draws its nested nodes by reference, so none can go alone.

At parity, compose-rs draws each paragraph into its layer's recording, as Compose's `Paragraph.paint` does (D567). So recording ahead alone covers the paragraphs too, as it would in Compose.

### The evidence

**Each round, 5 runs a side:**
- **Feed scroll:** main thread p90 / p99 2.15 / 3.35 → 1.86 / 2.79 ms (−13 / −17 %\*). p50, the RenderThread and main-thread work unchanged (+1 %, +1 %, +0 %).
- **Stress list:** p90 / p99 2.31 / 3.87 → 1.96 / 2.96 ms (−15 / −23 %\*); the rest unchanged.
- **Animated header:** main p90 −5 %\*. Nothing is prefetched there, so this is most likely noise.

The first each round (D574, 3 runs) agreed: −14 to −20 % at p90 and p99 on both scrolls.

**Why the slow frames shrink.** At parity, compose-rs's slowest main-thread frames on the scrolls record the items they reveal (`Record View#draw()`: up to 2.7 ms on the feed, 4.0 ms on the stress list; [12](12-android-performance.md), [15](15-android-stress-benchmark.md)). In D350's own A/B (10 runs a side, interleaved):
- The revealing frames, about 174 a run, went from a 1.80 to a 1.53 ms mean `doFrame` (the replay 0.31 → 0.11 ms).
- Other frames stayed at 1.35–1.36 ms.
- So the revealing frames lost most of their excess over the others: 0.45 → 0.17 ms.

**With the hint session** (the all round), the main thread's p90 gains compound: feed −40 % against the flags' product of −36 %, stress list −42 % against −46 %. At the stress list's p99 they overlap (−36 % against −46 %): a boosted frame records its revealed items faster, which leaves less to save.

The work moves rather than shrinks. Main-thread work is unchanged alone (+0 % feed, −0 % stress list), and prefetched items that are never shown (a fling that reverses) are recorded for nothing.

### How to backport it

Add a fourth step to the request, after premeasure, gated like the others by a per-content-type average:

```kotlin
// LazyLayoutPrefetchState's request (HandleAndRequestImpl.executeRequest), after the measure step. A sketch.
if (isMeasured && !isRecorded) {
    if (!shouldExecute(available, averages.recordTimeNanos)) return true  // "more work": next idle time
    val elapsed = measureNanoTime { precomposedSlotHandle.prerecord() }
    averages.saveRecordTime(elapsed)
    updateElapsedAndAvailableTime(elapsed)
    isRecorded = true
}
```

`prerecord()` would be a new internal API on the precomposed slot (beside `premeasure`). It would:
1. **Place the slot's content at its own origin,** without placing the slot's root in its parent, so that the coordinators' layers are created as placement creates them (`updateLayerBlock`).
2. **Record each dirty layer in the subtree** (`OwnedLayer.updateDisplayList()`, the work `AndroidComposeView.dispatchDraw` does for its dirty layers in a frame).

The frame that then places the item at its offset only moves its layer (a `RenderNode` position). The inner layers keep their size and content, so they aren't invalidated and their display lists are drawn by reference. compose-rs relies on the same property: a layer's recording is in its own coordinates, so placing it elsewhere doesn't invalidate it (D155's `invalidate_position`).

**Correctness conditions and risks:**
- **Placement's callbacks.** The provisional placement must not dispatch `onPlaced`, `onGloballyPositioned` or position-dependent semantics. They should be suppressed, or deferred to the real placement, and the item must not be hit-tested or reported to accessibility before it's placed. This is the main Compose-specific risk: compose-rs separates drawing from placement, Compose doesn't.
- **Staleness is already handled.** A state change between the idle time and the frame invalidates the layer, as for any layer, and the frame records it again. That costs work, but stays correct.
- **Sizes.** If the frame measures the item with other constraints, its layers are resized and recorded again, as today.
- **Bounds.** Layers recorded for items that are never placed must be released with the slot: when the request is cancelled, or the item is disposed or goes to the reuse pool. compose-rs bounds them (8 items, 128 groups).
- **Splitting.** Each layer's recording is a natural pause point, so the step can be split across idle times like pausable composition. compose-rs's step isn't split.
- **Opt-in first.** A `ComposeFoundationFlags` flag, like `isPausableCompositionInPrefetchEnabled`, then the default.

**What doesn't transfer:** compose-rs's records, keyed by group and version, and its byte protocol between Rust and Kotlin. In Compose the layers themselves are the cache.

**Not measured:** how much of Compose's own revealing frames is recording. Its slowest frames on the stress list compose an item inside the frame ("Compose behaviours" below), which this doesn't address.

### How to measure it

- **Perfetto.** In the frames that bring a new item into view: `Record View#draw()` and the `doFrame` duration. A marker slice in the frames that place a new item makes them easy to find (compose-rs's A/B used one). Also the `compose:lazy:prefetch:*` slices, plus a new one for the recording step.
- **Macrobenchmark.** `FrameTimingMetric`'s `frameDurationCpuMs` p90 and p99 on flings.
- **Screens.** A `LazyColumn` whose items have layers of their own (Material 3 `Card`s clip, so each is a layer), and a heterogeneous list with nested `LazyRow`s.

## 3. The single-drawing fade (`CompositingStrategy.Auto`)

### What it is

**Compose's rule** ([15](15-android-stress-benchmark.md), "Compose's defaults" (c); `GraphicsLayerV29.android.kt:244-267`, `:355-361`):
- Under `CompositingStrategy.Auto`, without a blend mode, colour filter or render effect, the node gets `setUseCompositingLayer(false)` and `setHasOverlappingRendering(true)`.
- With an alpha below 1, HWUI then draws the node through an offscreen buffer (a save layer).
- The rule never reads the alpha, nor what the layer holds.

**compose-rs's `single_drawing_fade`** (D153, D521; `compositing_of` in [serialize.rs](../crates/compose-android/src/serialize.rs), `single_drawing` in [display_list.rs](../crates/compose-ui/src/display_list.rs)) gives a layer `ModulateAlpha` when all of these hold:
- its strategy is `Auto` and its alpha is below 1;
- no nested group draws anything;
- its content is exactly one drawing with a paint (a rect, round rect, line, arc or path) among saves, clips, translations and transforms.

A layer holding text, an image or a shadow keeps `Auto`. With `ModulateAlpha`, the node gets `setHasOverlappingRendering(false)`, so HWUI multiplies the drawing's alpha and allocates no buffer. One drawing can't overlap itself, so the pixels are the same, but for rounding. compose-rs's Skia raster test finds the two within 2 of 255 per channel. This flag's output wasn't compared pixel by pixel on the device: the fidelity audit compared the parity screens, and not the animated ones.

### The evidence

**Stress animation, each round, 5 runs a side.** The 132 cells faded below an alpha of 1 lose their offscreen buffers:

| `single_drawing_fade` alone | parity | the flag |
|---|---|---|
| RenderThread work | 6,144 ms | 3,571 ms (−42 %\*) |
| RenderThread p50 / p90 / p99 | 7.62 / 8.80 / 10.53 ms | 6.38 / 7.44 / 8.48 ms (−16 / −15 / −19 %\*) |
| Main thread p90 / p99 (it waits less for the sync) | 4.24 / 6.82 ms | 3.27 / 4.50 ms (−23 / −34 %\*) |
| HWUI's boost, time-weighted | 613 | 444 (−28 %\*) |
| HWUI's workers' CPU time | 9.44 s | 10.89 s (every run above parity's) |
| Graphics memory after 13 s | 163.6 MB | 183.7 MB (+12 %\*) |
| HWUI's GPU caches after 13 s | 109.8 MB | 143.7 MB (+31 %\*; scratch surfaces 63.6 → 80.0 MB) |

The scrolls and the animated header fade nothing that qualifies: no change.

**The same effect in Compose itself:** Compose's own cells were given `ModulateAlpha` explicitly (D402; first results, a separate session, not interleaved). Compose's RenderThread work went from 6,048 to 3,286 ms (−46 %), its p50 / p90 / p99 from 7.52 / 8.84 / 10.49 to 6.22 / 7.03 / 8.21 ms.

**All together:** RenderThread work 5,803 → 3,313 ms (−43 %), main p99 6.03 → 4.08 ms. The RenderThread's p99 went 10.14 → 8.56 ms, against Compose's 11.10 ms in that session. The fade accounts for all of it. The GPU caches' growth goes away with #4 (118.7 → 114.6 MB, −3 %, while Compose's drifted +9 % between the sessions).

### Why it works

An offscreen buffer for an alpha below 1 costs the RenderThread a render target switch, a draw into it and a composite back, per layer per frame. For a layer that holds one drawing, the buffer buys nothing: fading the paint gives the same pixels. On the stress animation that is 132 buffers a frame. The costs are the boost and memory:
- **The boost.** HWUI's session sees the lighter frames and votes less (613 → 444). HWUI's workers, which rasterize the waves' path masks, then run at lower clocks and spend more CPU time for the same work (D511).
- **Memory.** The offscreen layers were what flushed the dropped path masks in early (#4).

### How to backport it

**The Compose part** is a recording-time check in `GraphicsLayerV29`:

```kotlin
// GraphicsLayerV29 (a sketch)
private var holdsOneDrawing = false

override fun record(/* … */ block: DrawScope.() -> Unit) {
    // … the existing recording, through a canvas that counts what it records …
    val one = counter.drawings == 1 && !counter.disqualified
    if (one != holdsOneDrawing) { holdsOneDrawing = one; updateLayerProperties() }
}

private fun updateLayerProperties() {                        // today's :261-267, one case added
    when {
        requiresCompositingLayer() -> renderNode.applyCompositingStrategy(CompositingStrategy.Offscreen)
        compositingStrategy == CompositingStrategy.Auto && holdsOneDrawing ->
            renderNode.applyCompositingStrategy(CompositingStrategy.ModulateAlpha)
        else -> renderNode.applyCompositingStrategy(compositingStrategy)
    }
}
```

The counter counts the draw calls that reach the recording through Compose's canvas. It should disqualify the layer, keeping `Auto`, in these cases:
- **More than one drawing,** including what children without a layer of their own draw into it (a background plus content is two).
- **A child layer** (`drawRenderNode`, `drawLayer`), or a save layer.
- **Any access to `nativeCanvas`.** `Paragraph.paint` and `drawIntoCanvas { it.nativeCanvas… }` draw on the `android.graphics.Canvas` directly, so a wrapper can't count those calls. Text is out anyway: glyphs, decorations and background spans can overlap.
- **A paint whose blend mode isn't `SrcOver`.** Drawn directly, it would blend with the parent's pixels instead of with a transparent buffer.
- **A colour filter or image filter that maps alpha non-linearly.** compose-rs's drawings have no blend modes or colour filters, so it never needed this check; Compose does.
- **A layer with an elevation or an outline shadow.** compose-rs never measured one; keep them out at first.

Images (one `drawImageRect`) can't overlap themselves either, but compose-rs leaves them out. Include them only after a pixel check.

**The HWUI alternative:** HWUI makes the same decision when it draws a node: alpha below 1, overlapping rendering, and a display list of exactly one qualifying draw op and no child nodes. That catches what the Compose counter can't see (text drawn through the native canvas, Views). It needs HWUI to know its display list's op kinds, and it reaches only new platform versions. We haven't read HWUI's drawing code for this, so this is a direction, not a design.

**API surface:** a behaviour change of `Auto` behind a flag (`ComposeUiFlags`, or the platform's equivalent) for a release. Apps that set `ModulateAlpha` already get it.

**Ship it with or after #4.** Without #4, a screen that fades only qualifying layers and redraws stroked paths every frame keeps an extra frame's worth of path masks. In Compose's own D510 session, `ModulateAlpha` on the cells held 162.9 MB of graphics memory 3 s after start against 119.4 MB with `Auto`.

### How to measure it

- **Perfetto:** `DrawFrame` durations and the RenderThread's CPU time. With the `gfx` category, HWUI traces each layer's drawing, so the save layers show.
- **Memory:** `dumpsys gfxinfo`'s GPU caches by kind (scratch surfaces) and `dumpsys meminfo`'s graphics, 3 s and 13 s into the animation.
- **Macrobenchmark:** `FrameTimingMetric` on an animation, not a scroll.
- **Screen:** many layers each fading one shape (`graphicsLayer { alpha = … }` plus one `drawRoundRect`), ideally with stroked paths redrawn every frame, to see #4.

## 4. Free the dropped path masks before the frame draws (HWUI / Skia)

### What it is

This is HWUI or Skia, not Compose. compose-rs's `flush_layers_before_dropped_paths` (D510; `flushNode` in [Replayer.kt](../android/compose-rs/src/main/java/rs/compose/android/Replayer.kt)) is an app-level workaround:
- **When.** A frame drops or redefines a path that a recording drew with `drawPath` (not one drawn from an icon mask, D190).
- **What.** The frame records and draws a 1 × 1 `RenderNode` with `setUseCompositingLayer(true)`, cleared (`drawColor(0, CLEAR)`).
- **Why it works.** Its recording changed, so HWUI renders that layer, and flushes, before drawing the frame.

### Why it works

In D510's reading of the traces, Skia's GPU cache keys the mask it rasterizes for a stroked path (its software path renderer, on HWUI's workers) by the path's contents. It frees the key when the contents die, at the sync that frees the last frame's recording, but it takes those frees in only at a flush:

| Stress animation, 3 s after start (D510 A/B, 8 runs) | Mask textures | Uploads into the last frame's textures | Graphics memory |
|---|---|---|---|
| Compose, `Auto`: the faded cells' offscreen layers flush first | 141 | 82 % | 119.4 MB |
| compose-rs without layers: one flush, at the frame's end | 245 | 0.1 % | 150.6 MB |
| compose-rs with the 1 × 1 layer | 131 | 85 % | 122.9 MB |

Without layers, each frame's 50 masks took the textures of the frame two before, and a frame's worth more masks and scratch surfaces stayed allocated. Compose with `ModulateAlpha` did the same (162.9 MB, 78 % of uploads two frames later). We didn't trace inside Skia: the mechanism is inferred from the upload counts and confirmed by the fix.

### The evidence

- **D510's A/B** (before parity, no offscreen cells; 8 runs a side; [15](15-android-stress-benchmark.md), "HWUI's workers and graphics memory"):
  - graphics memory 150.6 → 122.9 MB 3 s after start, and 154.6 → 123.0 MB after 13 s;
  - PSS 182.5 → 154.8 MB and 197.8 → 165.9 MB;
  - scratch surfaces 79.3 → 60.0 MB.
  - **Its cost:** a second flush and submit a frame (0.19 ms). The RenderThread p50 / p90 / p99 went 7.64 / 8.64 / 9.68 → 7.79 / 8.78 / 9.81 ms, its work +1.8 %. CPU-rail power +2.0 % (± 2.4, not resolved).
  - **Elsewhere:** the feed and the stress list didn't move (5 runs each).
- **At parity alone** (each round): GPU caches after 13 s −18 % (scratch buffers 37.6 → 18.9 MB, a level each run lands on, D510) and graphics −10 %, neither resolved. At parity the cells' offscreen layers already flush. The caches vary by 27 % across parity's own runs. D574's first each round (3 runs) read −33 %.
- **With #3** (the all round): the caches are back to parity's (−3 %), while the fade alone grew them by 31 %.

Graphics memory after 13 s varies from run to run in both apps (Compose's runs 138 to 142 MB, compose-rs's 164 to 203 MB at parity), mostly in the GPU driver's memory, which wasn't traced. Read every 13 s figure with that spread in mind.

### How to backport it

The cleanest fix is in HWUI or Skia: take the cache's pending key invalidations in before the frame's draw, right after the sync that freed the recordings, rather than at the frame's end. Three options:
- **(a)** HWUI asks Skia to process the resource cache's pending messages at the start of a frame's draw. That is the step a flush runs to take the frees in, without the flush itself. We didn't establish which Skia entry point does that.
- **(b)** Skia's software path renderer takes them in before it allocates a mask texture.
- **(c)** HWUI flushes before drawing a frame whose sync freed a recording that drew a path through the mask path. That is what compose-rs's layer forces, at 0.19 ms a frame.

Compose can't easily do (c) itself: it doesn't know when a path's contents die, since the recording owns its copy.

Any of these helps more than the fade's screens. It covers any app whose stroked paths change every frame with no offscreen layer in the frame: a chart or waveform drawn with `drawPath`, or a View app.

### How to measure it

- **Memory:** `dumpsys gfxinfo`'s GPU caches by kind (scratch surfaces, SW path masks) and `dumpsys meminfo`'s graphics, 3 s and 13 s into the animation.
- **Uploads:** the mask textures' uploads into the last frame's textures, counted from the traces.
- **Screen:** a `Canvas` redrawing 50 stroked paths of 48 segments each every frame (the stress animation's waves), with nothing faded offscreen.

## 5. Keep a paragraph while its layout width doesn't change (`ParagraphLayoutCache`)

### What it is

**Compose's rule** (`ParagraphLayoutCache.kt:318-347`, the same in `MultiParagraphLayoutCache.kt:387-414`; quoted in [15](15-android-stress-benchmark.md), "Compose's defaults" (g)):
- Any new `maxWidth` or `minWidth` lays the text out again.
- So does a new `maxHeight` when the paragraph is taller than it, or was cut at its maximum lines.

The new `Paragraph` comes from the cached `ParagraphIntrinsics`, which measured the text's own width once. But a label that fits gets a new layout whenever its container's width changes, though it is laid out at the same width every time.

**compose-rs's `reuse_paragraph_at_same_layout_width`** (D551; `new_layout_will_be_different` and `same_layout_width` in [text.rs](../crates/compose-foundation/src/text.rs)) keeps the paragraph when Compose's `finalMaxWidth` (`LayoutUtils.kt:38-66`) gives the same width for the old and new constraints. A measurer lays a text out from that width alone. So the kept paragraph is the one a new layout would make: its line breaks, alignment, ellipsis and right-to-left layout included.

### The evidence

**The resize screen** (D553; [14](14-performance-opportunities.md), "Text across width changes") is ad hoc, in both apps, not one of measure.py's scenarios: 56 texts in a box whose width animates between half and all of the screen, 1.5 s each way, read while measuring. 6 interleaved runs of 10 s each; means. This A/B ran before parity mode, with compose-rs's hint session on:

| | Compose 1.12.1 | compose-rs, Compose's rule (D550) | compose-rs with D551 |
|---|---|---|---|
| Text layouts (Compose: `TextStringSimpleNode::measure`) | 50,882 (3,418 ms) | 51,047 (1,030 ms) | **3,609** (835 ms) |
| Main-thread work | 3,615 ms | 1,094 ms | **527 ms** (−52 %) |
| RenderThread work | 924 ms | 815 ms | **547 ms** |
| Main thread p50 / p90 / p99 | 5.16 / 8.29 / 10.29 ms | 2.49 / 3.21 / 4.69 ms | 2.67 / 3.36 / 4.33 ms |

- **The layouts left.** 14 times fewer layouts: 3 a frame remain, for the wrapping paragraph, the ellipsized line and the title centred across the width.
- **The RenderThread.** The kept paragraphs' recordings stay valid, so it does a third less.
- **Frame durations** barely move, and p50 rises. With half the work the hint session lowers the clocks (main-thread CPU time 3,098 → 3,272 ms): the saving is work and power, not latency, on this screen.
- **Headless** (Skia, a host resized by 3 px a frame): a grid of labels 96.2 → 9.9 µs a frame, 12 nested levels 46.2 → 4.6 µs.
- **The each round** has no screen whose widths change: no gain there. Its feed p90 +10 %\* stands alone, with no mechanism, and is most likely noise.

### How to backport it

The change is in `newLayoutWillBeDifferent` only. The existing kept path (`ParagraphLayoutCache.kt:166-183`) already re-constrains the size and overflow for the new constraints:

```kotlin
private fun newLayoutWillBeDifferent(constraints: Constraints, layoutDirection: LayoutDirection): Boolean {
    val localParagraph = paragraph ?: return true
    val localParagraphIntrinsics = paragraphIntrinsics ?: return true
    if (localParagraphIntrinsics.hasStaleResolvedFonts) return true
    if (layoutDirection != intrinsicsLayoutDirection) return true
    if (constraints == prevConstraints) return false
    if (constraints.maxWidth != prevConstraints.maxWidth || constraints.minWidth != prevConstraints.minWidth) {
        if (!sameLayoutWidth(localParagraph, localParagraphIntrinsics.maxIntrinsicWidth, prevConstraints, constraints)) {
            return true
        }
    }
    // Unchanged: the height clause.
    if (constraints.maxHeight < localParagraph.height || localParagraph.didExceedMaxLines) return true
    return false
}

private fun sameLayoutWidth(paragraph: Paragraph, own: Float, old: Constraints, new: Constraints): Boolean {
    val widthMatters = softWrap || overflow == TextOverflow.Ellipsis
    val oldLimit = if (widthMatters && old.hasBoundedWidth) old.maxWidth else Constraints.Infinity
    val fit = !paragraph.didExceedMaxLines && own.ceilToIntPx() <= oldLimit
    return fit && finalMaxWidth(old, softWrap, overflow, own) == finalMaxWidth(new, softWrap, overflow, own)
}
```

**The rule, exactly** (from `final_max_width`):
- **The limit.** Width matters when the text wraps or ellipsizes. The limit is then `maxWidth`, if bounded, else unbounded.
- **The layout width.** If `minWidth` equals the limit, the layout width is the limit. Otherwise it is the text's own width, rounded up, kept between `minWidth` and the limit.
- **The decision.** A paragraph is kept when that width is the same for the old and the new constraints. It also needs to have fit: its own width within the old limit, and no line past its maximum.

So:
- **Kept:** a label that fits keeps its paragraph while its container narrows down to the label's own width, or widens.
- **Laid out again:**
  - a text across the whole width (`fillMaxWidth`, so `minWidth == maxWidth`);
  - a text that wraps or is ellipsized;
  - a text held wider than its own width by a `minWidth` that changes.
- **Unchanged:** Compose's height clause, and the `MinLinesConstrainer`'s constraints are the ones compared.

**Why `fit`.** compose-rs's measurers report their own width from a layout. In Compose, `ParagraphIntrinsics.maxIntrinsicWidth` doesn't depend on the layout, so the clause may be unnecessary there, but we haven't shown that. Keep it in a first version.

**What to check:**
- the kept path invalidates drawing when the new size or overflow differs (a non-wrapping text clipped by a narrower `maxWidth`): the size changes, as with Compose's existing height-only case;
- `MultiParagraphLayoutCache` gets the same change.

compose-rs checked its version frame by frame against hosts without it, with three measurers (fixed-advance, SkParagraph with Hebrew, Arabic, emoji, letter spacing and styled runs, and a model of its Android measurer). It compared every paragraph's metrics, baselines and the cursor at every offset.

**API surface.** It is exact by construction, so it could be the default. A `ComposeFoundationFlags` flag for one release would allow an A/B.

**Two simpler rules don't work.** Cranpose's rule (reuse a single line while the new maximum holds its width) is the special case without a minimum width. Applied to every text, it draws centred, end-aligned and full-width texts at their old width (D551).

### How to measure it

- **Perfetto:** the count and duration of `TextStringSimpleNode::measure` slices (what the resize A/B counted in Compose's traces); main-thread CPU time.
- **Microbenchmark:** a box whose width changes every frame around a grid of labels.
- **Screens:** any width that animates or is dragged: `animateContentSize` horizontally, a draggable pane divider, a side sheet pushing content, a window resized on a foldable or in freeform. Expect the gain in work and power more than in frame durations.

## Measured and not worth it

Each round, 5 runs a side on the feed, the header and the stress list. On the stress animation, the seven flags below had 3 runs a side against parity's own 3, which no test can resolve at p < 0.05: none of these flags touches that screen, and its spread there is 16 to 27 %.

| Flag | What it does (Compose's default instead) | Measured | Verdict |
|---|---|---|---|
| `prefetch_two_ahead` | Two items ahead, not one (`LazyListPrefetchStrategy` has no count) | Feed main-thread work −5 %\*, durations within the spread; stress list within the spread | No gain in durations |
| `prefetch_budget_recent_maximum` | D341/D381's estimate: the larger of a running average and a recent maximum, plus a 0.5 ms margin, scaled by how much slower this idle time's earlier steps ran; D340's deadline on the vsync's phase. Compose: the step's average, against the last frame's start plus an interval | Feed p90 +10 %\*, alone and unexplained; 422 to 441 prefetch steps a feed run against 381 to 406. Its earlier A/Bs, against compose-rs's earlier budget: idle-time slices more than 1.2 ms past the vsync 9.0 → 2.0 a run (D341) and 1.67 → 0.92 (D381) | At parity, few steps overran (9 in 6 scroll runs, each frame on time), so there was little to cut |
| `prefetch_at_rest` | Prefetch after a layout at rest too (Compose: on scrolls only) | Within the spread | No gain |
| `keep_items_behind` | Keep two composed items behind the scroll, with their recordings (Compose: none; the experimental `LazyLayoutCacheWindow` keeps a window behind) | Main p50 +5 %\* on both lists, the kept items' bookkeeping each frame. Earlier, on reversals: a reversal's slowest `doFrame` p90 7.26 → 4.64 ms and max 11.44 → 5.32 ms (D375, 16 runs) | A cost on flings that reverse only every fourth swipe; a gain only on reversals |
| `grid_lines_beyond` | Compose and place two grid lines beyond each side (Compose: none beyond) | Within the spread; no grid in the scenarios | Not measured where it applies. Nothing to backport: a buffer Compose doesn't need |
| `paragraph_render_nodes` | Each paragraph recorded once into a render node of its own (Compose: drawn into its layer's recording) | Feed main p50 / p90 / p99 +3 / +11 / +9 %\*, header RenderThread p50 +4 %\*. The big screen's first replay 13.0 → 21.7 ms | A cost: each card is a layer already, and a node per paragraph adds one more to record and draw |

`reuse_paragraph_at_same_layout_width` is in the same position on these screens (nothing resolved), but its own screen shows the gain (#5).

## Compose behaviours worth knowing

Found while matching Compose 1.12.1's defaults ([15](15-android-stress-benchmark.md), "Compose's defaults", with line numbers):

- **Prefetch's per-type averages start at zero.**
  - **Where:** `LazyLayoutPrefetchState.kt:350-362` creates a new content type's `Averages()` with every field 0 (`getOrPut(contentType) { Averages() }`). The comment there speaks of starting from the overall averages; the code doesn't.
  - **Why it matters:** the gate is `available > average` (`:572-579`), so the first item of each new type composes and measures with any time left. The same happens after the measure average is cleared, when a nested prefetch did fewer items than its ideal count (`:737-739`).
  - **Also:** the averages are `current / 4 * 3 + new / 4` in integer nanoseconds, divided first, from a first sample taken as is (`:408-418`).
  - **Not measured:** how often this lets a step run past the deadline in Compose. compose-rs copies it at parity.
- **The prefetch deadline isn't the next vsync.**
  - **The rule:** `AndroidPrefetchScheduler.run` uses `max(frameStartTimeNanos, view.drawingTime) + frameIntervalNs`, and gives the first request no limit after two intervals without a draw (`PrefetchScheduler.android.kt:130-174`, `:154`). `frameIntervalNs` is read once per process.
  - **Why it might matter:** compose-rs's own deadline was the frame time plus an interval. Under Android 16's buffer stuffing recovery, Choreographer hands every callback a frame time an interval early, so that deadline was always past, and prefetching stopped for 10 s, with main p99 at 7.35 ms (D340).
  - **Not measured:** Compose's deadline comes from the same frame times, so it may starve the same way; we didn't measure Compose in recovery. At parity compose-rs keeps Compose's rule. D340's fix (the next vsync on the frame time's phase) is part of `prefetch_budget_recent_maximum`.
- **A measurement isn't split.** Pausable composition splits a composition across frames; nothing splits an item's premeasure. In both apps, the prefetch steps that ran past the vsync were mostly measurements, on a little core, unboosted:
  - compose-rs at parity: 3.5 to 8.0 ms steps, which delayed a `doFrame` by 2.1 to 3.8 ms, on time;
  - Compose's own work delayed 17 `doFrame`s in its 3 feed scroll runs and 25 in its 3 stress-list runs.
- **One item ahead doesn't keep up on a heavy list.** On the stress list, Compose's janky frames (2 a run) compose a newly revealed item inside the frame: `AndroidOwner:measureAndLayout` in `Recomposer:animation`, 9.2 to 12.8 ms. compose-rs, also one item ahead at parity, had none. Its prefetch compositions there did 38.6 ms of work a run, against Compose's 98.9 ms, plus 24.8 ms nested and 32.1 ms of apply ([15](15-android-stress-benchmark.md), "prefetch on the stress list").
- **Three text reuse rules.**
  - `ParagraphLayoutCache` lays out again for any width bound.
  - The legacy `CoreTextField`'s `TextDelegate.canReuse` (`TextLayoutHelper.kt:44-89`) reuses across maximum widths when the text neither wraps nor ellipsizes.
  - `TextMeasurer`'s `TextLayoutCache` keys on the exact `Constraints`.

  #5 generalizes the legacy rule, and makes it exact for text that wraps.
- **`Auto` never reads the alpha.** The offscreen buffer is HWUI's decision, made from `hasOverlappingRendering`. `ModulateAlpha` is opt-in, and on the stress animation it took 46 % off Compose's RenderThread work (#3).
- **HWUI's boost rewards long main-thread frames.** Making Compose's main thread faster will lower its RenderThread's boost, as it did compose-rs's (#1).

## Methodology

**Parity mode** (D560–D569; [15](15-android-stress-benchmark.md), "Parity mode"). compose-rs does Compose 1.12.1's work by default:
- Compose's prefetch strategy, scheduler and per-type budget;
- a layer per `graphicsLayer`, `clip` and `shadow`, with HWUI's elevation shadows (D520–D529);
- a mutable `Path`, reset in place (D530–D535);
- Compose's text relayout rule (D550);
- no hint session of its own (D568; off by default since D581).

What remains different:
- **No pausable composition.** A request composes its item in one step, gated on a whole composition's average.
- **Composition in rounds.** Composition runs in rounds, not inside measure.
- **Scroll deltas.** The strategy hears a frame's deltas summed.
- **Masks.** compose-rs masks any small filled path, as `VectorPainter` caches vectors.

**The fidelity audit** (D570–D573) compared both apps on the device: bounds from `uiautomator dump`, pixels from `screencap`. After Material 3 1.4.0's type scale, line height style and component tokens were matched, the feed, big, stress and grid screens are pixel-identical at their start (the stress screen: 99,173 differing pixels before, 0 after). The animated screens were matched by eye.

**Device and build:**
- **Device:** Pixel 8 Pro (nine cores in three clusters), Android 16, `user` build.
- **Compilation:** both apps `cmd package compile -m speed`.
- **Compose:** BOM 2026.09.00 (ui, foundation, runtime 1.12.1; material3 1.4.0), Kotlin 2.4.21, AGP 9.4.1, a release build minified with R8.
- **compose-rs:** 3ac74a6, release profile (fat LTO, `panic = "abort"`). measure.py checks that the installed native libraries are those built.

**Thermal guards** (D569, D574, D579). Before each run, startup round and traced first frame, measure.py:
- stops the benchmark apps;
- waits while the thermal status is above 0, the battery above 34 °C or the skin (the highest `TYPE_SKIN` sensor) above 35 °C, with no time limit;
- records each run's state before and after.

In the each round the guard waited before 37 of 229 runs: 10 to 20 s, and up to 240 s in the stress animation's supplement. No run was throttled: gfxinfo's median frame time was 5 or 6 ms on the lists and 14 or 15 ms in the stress animation in every run.

**Interleaving and statistics** (D580):
- **The each round** ran 12 subjects (parity and 11 flags), one run each per invocation, in five invocations whose order was rotated by 0, 2, 5, 7 and 10 places. The stress animation had parity and the four flags that can matter there (5 runs). The seven others had 3 runs each against 3 of parity's own, stopped there when the device needed up to 4 minutes of cooling before each run.
- **The tests** are exact two-sided Mann–Whitney tests over the runs, read against the number of tests. A t-test was rejected: 5 runs aren't normal enough to trust one.
- **Parity's own spread** over its 5 runs (max − min over the median) was 2 to 17 % on the feed's percentiles, 3 to 18 % on the stress list's, 6 to 36 % on the animated header's and 7 to 18 % on the stress animation's.
- **The parity and all rounds** were one invocation each, with 3 traced runs a scenario and Compose interleaved, an hour apart. Compose's change between them gives the sessions' drift.
- **Interactions** compare the all round with the parity round, against the product of the starred flags' changes.

**Dynamic clocks.** Every run used the device's governors and ADPF, so durations include how the system clocked each thread. That is the point of #1, but it adds noise everywhere else, and it is why "work" is reported beside durations. No run used fixed clocks: that needs a rooted device, which was deferred.

**Tracing.** Perfetto with atrace's `gfx` category, which costs both apps' RenderThreads time, more on the stress screens. RenderThread durations here are traced durations.

**Data and tools:**
- [`2026-10-25-pixel-8-pro-parity.json`](../android/bench/results/2026-10-25-pixel-8-pro-parity.json), [`…-all.json`](../android/bench/results/2026-10-25-pixel-8-pro-all.json) and [`…-each.json`](../android/bench/results/2026-10-25-pixel-8-pro-each.json). The each round's `changes` holds every median, change and p value.
- The earlier A/Bs: [`…-flush-layers.json`](../android/bench/results/2026-10-24-pixel-8-pro-flush-layers.json) (D510), [`…-resize-text.json`](../android/bench/results/2026-10-24-pixel-8-pro-resize-text.json) (D550, D551), [`…-hint-target.json`](../android/bench/results/2026-10-23-pixel-8-pro-hint-target.json) (D500) and [`…-hint-main-thread.json`](../android/bench/results/2026-10-23-pixel-8-pro-hint-main-thread.json) (D440).
- [`measure.py`](../android/bench/measure.py) (`--optimizations parity|all|each`) and [`summarize.py`](../android/bench/summarize.py).

## See also

- [12](12-android-performance.md): the feed against Compose, at parity and with every optimization.
- [14](14-performance-opportunities.md): "Backport candidates" (the each round's tables, the interactions) and the history of each optimization's A/B.
- [15](15-android-stress-benchmark.md): the stress screens, "Compose's defaults (version 1.12.1)" with source references, and "Parity mode".
- [13](13-android-drawing-paths.md): how compose-rs draws on Android, through the same `RenderNode` and `Canvas` APIs as Compose.
- [decisions.md](decisions.md): D153, D190, D322, D340, D341, D350, D375, D380, D381, D390, D440, D441, D500, D501, D510, D511, D520–D529, D550–D556 and D560–D580.
- The code:
  - [performance_hint.rs](../crates/compose-android/src/performance_hint.rs): #1;
  - [Replayer.kt](../android/compose-rs/src/main/java/rs/compose/android/Replayer.kt): `recordAhead` for #2, `flushNode` for #4, the compositing strategies for #3;
  - [prefetch.rs](../crates/compose-foundation/src/lazy/prefetch.rs): Compose's prefetch at parity, and the prefetch flags;
  - [text.rs](../crates/compose-foundation/src/text.rs): #5.
