# 15. Android stress benchmark: compose-rs and Jetpack Compose under load

[12](12-android-performance.md) compares the two on a light social feed. This document compares them on two much heavier screens, built the same way in both apps (D400): a list of rich, heterogeneous items, and a screen running hundreds of animations at once. It measures jank, frame durations on both threads, work, memory and startup with the same tools as [12](12-android-performance.md) (D401).

**Short answer** (Pixel 8 Pro, Android 16; round 14 of [12](12-android-performance.md), compose-rs at 3ac74a6 against Compose 1.12.1; medians of 3 runs, startup of 15). At **parity**, compose-rs does the work Compose does by default and none of its own optimizations, its hint session included ("Parity mode"):

- **Scrolling the stress list**, no frame of compose-rs's missed the app's deadline; Compose missed 6 in 3 runs (2 a run), building a newly revealed item inside the frame.
  - The main thread is ahead at p90 and p99 (2.36 / 4.11 against 3.14 / 7.95 ms; its slowest frame 7.5 against 15.4 ms) and behind at p50 (1.39 against 1.29 ms).
  - The RenderThread is behind at every percentile: 3.13 / 4.60 / 6.47 against 2.61 / 3.71 / 5.45 ms (+19 to +24 %), for 32 % less work. HWUI's session, the only one at parity, boosts it little: compose-rs's main-thread frames are short, so its frames earn a time-weighted boost minimum of 67 against Compose's 277, and 71 % of them run unboosted (Compose's 8 %). At each boost level its frames are the shorter.
  - By the work metric, the main thread does 61 % less work than Compose's (475 against 1,214 ms), the RenderThread 32 % less (1,119 against 1,648 ms); the metric reads lower at the low clocks compose-rs ran at.
- **The stress animation** keeps both RenderThreads busy for most of a 120 Hz vsync (8.33 ms).
  - No frame of compose-rs's missed a deadline in the parity round's 3 runs; Compose missed 2 in one run. On this screen both apps are near the deadline: in the each round, parity's 5 runs had one janky frame in 3 of them (Compose wasn't in that round).
  - The main thread is ahead: 2.37 / 4.01 / 6.03 against 3.70 / 5.36 / 8.03 ms, for half Compose's work (D420's one task per transition).
  - The RenderThread is level: 7.67 / 8.77 / 10.14 against 7.48 / 8.94 / 10.77 ms (p50 +3 %, p99 −6 %), for 5 % less work. At parity the faded cells composite offscreen as Compose's (`CompositingStrategy.Auto`, D521); with `single_drawing_fade` (a layer holding one drawing faded without a buffer, as `ModulateAlpha`) its RenderThread does 42 % less work ([14](14-performance-opportunities.md), "Backport candidates").
- **Memory**: on the list, compose-rs holds a fifth less after scrolling (90.4 against 114.3 MB). In the animation it holds less 3 s after start (152.2 against 168.0 MB of PSS, the same graphics memory: 119.8 against 119.4 MB) and after 13 s (216.0 against 255.5 MB of PSS), but its graphics memory then is the larger (171.5 against 140.6 MB at the median; its runs 164 to 203 MB, Compose's 138 to 142 MB), and HWUI's workers spend 18 % more CPU time for it (10.1 against 8.5 s, D511).
- **Startup** of the stress list: 116 against 148 ms.

With **every optimization on** (`all`, the same session), compose-rs is ahead at every percentile on both threads on both screens: the list's RenderThread 2.09 / 2.87 / 4.02 against 2.51 / 3.58 / 5.13 ms, its main thread 0.94 / 1.36 / 2.64 against 1.24 / 3.24 / 8.41 ms; the animation's RenderThread 6.41 / 7.43 / 8.56 against 7.44 / 8.91 / 11.10 ms, for 46 % less work.

## The screens

`examples/feed/src/stress.rs` (compose-rs) and `android/bench-compose/…/Stress.kt` (Kotlin Compose) are written widget for widget.

**`stress`** is a Material 3 `Scaffold` with a `TopAppBar` over a `LazyColumn` of 1,000 keyed items, with a sticky header ("Section n") before every 20. An item is one of four kinds, by its index modulo 4:

- **A carousel:** a title and an icon over a nested `LazyRow` of 12 keyed tiles. A tile is a 120 × 90 dp `Image` (`ContentScale.Crop`, rounded 12 dp clip) with a caption.
  - The images are 8 procedural 240 × 180 bitmaps: a diagonal gradient between two palette colors with lighter rings, computed by the same integer arithmetic on both sides.
  - They are made once, on first use.
- **A rich post:** an `ElevatedCard` holding:
  - a 44 dp circular avatar filled with a linear gradient, with a number;
  - the author, the time and a `MoreVert` icon;
  - an `AnnotatedString` of 4 lines at most, with bold, italic, colored medium-weight, underlined and highlighted (background) spans;
  - a row of 4 icons with counts.
- **An action card:** an `OutlinedCard` holding a title and an `AssistChip` with a leading icon, a `FilterChip` and a `SuggestionChip`. Below them, a determinate `LinearProgressIndicator`, then a `Button`, an `OutlinedButton` and a percentage.
- **A banner:** a 120 dp box with a 6 dp `shadow` and a rounded 16 dp clip, filled with a three-color linear gradient. A title and a line of text sit over it, and three icons in a corner.

Each item has several times a feed post's nodes. A carousel composes and lays out its own lazy list.

**`stress_animation`** is a `Scaffold` and `TopAppBar` over a column of five parts:

- **A ticker:** a text changing every frame ("Frame n · t ms"), written from a `withFrameMillis` loop and read in composition. It recomposes every frame.
- **A grid of 140 cells (10 × 14), each with its own infinite transition.**
  - Each cell animates a rotation (linear, restarting), a scale and an alpha (reversing), and a color, each with its own duration by index.
  - All four are read while drawing: `graphicsLayer { rotationZ; scaleX; scaleY; alpha }` and `drawBehind { drawRoundRect(color) }` (compose-rs: `graphics_layer` and `draw_behind`).
  - That makes 560 animated values.
- **Six bars** whose widths animate.
  - Each width is read while measuring (`Modifier.layout`), so it relayouts without recomposing.
  - Each bar has a percentage beside it, from `derivedStateOf`, which recomposes when the integer changes.
- **A card that grows to four lines and shrinks back to one every second.** It toggles from a `delay(1000)` loop, with `animateContentSize(tween(500))` on its column. This is a layout animation that moves what's below it.
- **A 140 dp `Canvas` of 50 sine waves**, each a 48-segment stroked path with its own amplitude, frequency and speed. The waves are recomputed from an infinite transition read while drawing.

The two apps' screens are pixel-identical at their start since the fidelity audit (D570–D573, "What differs" below); the animated screens' layouts and line breaks match by eye. Before it, side by side at the top of the list, mid-scroll with a header stuck and in the animation, they differed in Material version details:

- Kotlin's `OutlinedButton` label was `onSurfaceVariant` where compose-rs's was `primary`, and its chip and card outlines lighter (D572 took Material 3 1.4.0's tokens).
- compose-rs's shadows were a blurred outline before D524; since, the elevated card's and the banner's are HWUI's elevation shadows, as Kotlin's, and the cards' rounded clips the same (compared side by side after D522-D525, with the feed, the grid and the stress animation).
- compose-rs sits below the status bar, as on the feed.

## What differs, and why it's fair

D400 records the choices. A fresh reviewer checked the two implementations before measuring:

- **The same viewport.** Kotlin's list takes the scaffold's padding as a modifier (`Modifier.padding(padding)`), not as `contentPadding`. The edge-to-edge Kotlin app would otherwise compose, lay out and draw items under the bars, about 110 dp more of the list. A sticky header would also slide under the top bar. The feed in [12](12-android-performance.md) keeps `contentPadding`.
- **Compose's best practices.** These are what a skilled Compose developer writes:
  - keys on both lazy lists, and `contentType` on the outer one;
  - stable parameters, and `remember(index)` for the `AnnotatedString`;
  - deferred reads in `graphicsLayer { }`, `drawBehind`, `Modifier.layout` and `Canvas`;
  - `derivedStateOf` for the percentages, and one `Path` remembered and reset for each wave.
- **`contentType` on both.** compose-rs reuses items as Compose does since D430, by the same content types (`sticky_header_keyed_with`, `items_keyed_with`). Round 10 was the first measurement with it; the sections after "Round 13 (history)" predate it, when compose-rs composed each new item afresh where Compose reused the composition of an item of the same kind.
- **The progress indicator.** Both apps' `LinearProgressIndicator`s are given no gap (`gapSize = 0.dp`, `gap_size = 0.dp()`), and Kotlin's an empty `drawStopIndicator`: a line, then the track from where it ends, rather than Material 3 1.4's two segments and stop dot (compose-rs draws no stop dot). Before D572, compose-rs's track ran under the whole line.
- **The same pixels (D570).** A fidelity audit compared both apps on the device, each screen at its start: bounds from `uiautomator dump`, pixels from `screencap`. It found compose-rs's texts narrower (no letter spacing in its Material typography: the rich post fit "how UI" on its first line, where Kotlin wraps "UI"), its glyphs 1 to 3 px lower (its line height wasn't centered as Material's), and the 1.2 colors of the outlined button's label and outline, the chips' outlines and the progress track, with the chips' outlines clipped at the corners. Since D570 to D573 (Material 3 1.4.0's type scale, `LineHeightStyle`, its component tokens), the stress, feed, big and grid screens are pixel-identical (the stress screen: 99,173 differing pixels before, 0 after). The results before D570 laid out compose-rs's narrower texts, so some of its lines broke later (the first rich post's).
- **The alpha of the grid's cells.** Both keep the default `CompositingStrategy.Auto`, under which HWUI draws a layer faded below 1 offscreen (`setHasOverlappingRendering(true)`), since D521.
  - Before D521, compose-rs faded a layer whose content is one drawing without an offscreen buffer (`setHasOverlappingRendering(false)`, D153 and D157), as `ModulateAlpha` would: the results below that predate D521 have it. It is now `Optimization::SINGLE_DRAWING_FADE`, off by default (`-e optimizations single_drawing_fade` on bench-rs measures it, D520). Since, compose-rs's RenderThread does the stress animation's work Compose's does: 5.8 s against 3.2 s before and 6.0 s for Compose, p50 7.55 ms against 6.46 (Compose 7.40), D527.
  - `ModulateAlpha`, Compose's opt-in for the same thing, was measured separately (D402, "With `ModulateAlpha`" below).
- **Infinite transitions.** In the first results below, compose-rs ran one `Animatable` and one coroutine per animated value (`animate_value`). That made 567 tasks a frame, and the ticker's. Compose runs one frame loop per transition (147) and ticks its animations together. This was a real cost of the library, measured as such. Since D420, compose-rs's transitions run one task each too ("Infinite transitions, one task each" below).
- **One `Path`, reset for each wave**, in both apps since D533: Kotlin's `remember { Path() }` and `path.reset()`, compose-rs's `remember(Path::new)` and `path.reset()` (D530). Kotlin's `drawPath` records each wave's contents into the node's recording, as compose-rs's display list records each wave's `PathData`. On Android compose-rs's 50 contents keep 50 ids, each defined again in place each frame, so the replayer refills the same 50 `android.graphics.Path`s, defining no new one and dropping none (D532); a frame redefining a path drawn as a path draws a 1 × 1 layer so HWUI flushes before drawing it, which frees the old contents' masks in time for the new ones (D510, D534). Before D533, compose-rs made a new `Path` per wave per frame (a drawn path was immutable, D412): 50 paths defined and 50 dropped a frame; in the first results below, the replayer made an `android.graphics.Path` for each, the bridge's cost, measured as such (see "Round 13: memory"), then rebuilt the dropped ones (D410).
- **Clips and shadows.** Both make a render node (a graphics layer) for each `clip` and each `shadow`, and HWUI draws the shadow from the elevation, since D522-D525: a card is a node clipping to its outline (`setClipToOutline`), an elevated card adds a 1 dp elevation node, the banner's `shadow(6.dp, shape, clip = true)` is one node with an elevation and a clip.
  - Before, compose-rs clipped inline in its drawing and drew an approximate shadow with a `BlurMaskFilter`: the results below that predate D522 have it.
- **The theme.** Both apps use the light theme. The Kotlin app always does; compose-rs follows the system's, which was light.

## Compose's defaults (version 1.12.1)

What the Kotlin app gets by default, read from the sources jars of Compose BOM 2026.09.00 (foundation, ui, ui-graphics 1.12.1; material3 1.4.0), since D540. File names are under `androidx/compose/…` in each jar, `commonMain` unless marked `android`. Everything in (a), (b) and (f) is the same in 1.11.4 (rounds 1 to 13), whose prefetch files differ only in the Robolectric check; in (c) and (d), 1.12 adds layer outsets, which nothing here uses.

**(a) No cache window on lists; one item ahead.** `LazyColumn`'s `state = rememberLazyListState()` (`LazyDsl.kt:330`) builds `LazyListState(index, offset)`, which passes `LazyListPrefetchStrategy()` (`LazyListState.kt:185-188`); a `LazyLayoutCacheWindow` only comes through the opt-in constructor `LazyListState(cacheWindow, …)` (`:170-177`, `LazyListCacheWindowStrategy`). The cache window is the default only for `Pager` (`ComposeFoundationFlags.isCacheWindowForPagerEnabled = true`, `ComposeFoundationFlags.kt:97`). The default strategy (`LazyListPrefetchStrategy.kt:137-255`) prefetches **one item ahead and none behind**: `visibleItemsInfo.last().index + 1` scrolling forward (`delta < 0`), `first().index - 1` backward (`:236-242`). It cancels its request when the direction turns (`:163-169`) or when, after a layout, the index it holds isn't the next one any more (`:244-254`), and marks it urgent when the same delta again would reach the item: `lastItem.offset + lastItem.size + spacing - viewportEndOffset < -delta` (`:174-182`; backward `viewportStartOffset - firstItem.offset < delta`).

**(b) Prefetch composes and measures; it doesn't place or draw; nested prefetch is on.** The list's request is `schedulePrecompositionAndPremeasure(index, lastMeasureResult.childConstraints, …)` (`LazyListState.kt:336-340`), high priority once the list has scrolled (`executeRequestsInHighPriorityMode = true` in `onScroll`, `:510`). A request composes the item (pausable composition by default: `isPausableCompositionInPrefetchEnabled = true`, `ComposeFoundationFlags.kt:89`), applies it, prefetches nested lazy layouts, then premeasures every placeable with those constraints (`LazyLayoutPrefetchState.kt:622-744`, `performMeasure` `:803-817`). Nothing places it, so no layer is created and nothing records: a coordinator's render node is made at placement (`NodeCoordinator.kt`, `placeSelf` `:412-448`, `updateLayerBlock` only `if (layoutNode.isAttached && layerBlock != null)` `:563-583`). Nested prefetch is on by default: `LazyListPrefetchStrategy(nestedPrefetchItemCount: Int = 2)` (`LazyListPrefetchStrategy.kt:128`); a prefetched item's nested lists (found through `traversablePrefetchState`, `LazyLayoutPrefetchState.kt:819-833`) each *precompose* (no premeasure) `firstVisibleItemIndex + i` for i below the count (`LazyListPrefetchStrategy.kt:218-228`). The count is 2 until the parent has measured one item of that content type; then it's the parent's average for that type of the smallest nested list's visible item count (`idealNestedPrefetchCount = result.visibleItemsInfo.size`, `LazyListState.kt:609`; `LazyLayoutPrefetchState.kt:726-740`, `:913-920`). So a carousel prefetched on the stress list precomposes its row's first tiles too.

**(c) `CompositingStrategy.Auto` never asks for a compositing layer itself.** On API 29+ a layer is a `GraphicsLayerV29` around a `RenderNode` (`AndroidComposeView.android.kt:2127-2135`, through `GraphicsLayerOwnerLayer`). Its rule (`android/…/graphics/layer/GraphicsLayerV29.android.kt`, ui-graphics):

```kotlin
private fun updateLayerProperties() {                       // :261-267
    if (requiresCompositingLayer()) renderNode.applyCompositingStrategy(CompositingStrategy.Offscreen)
    else renderNode.applyCompositingStrategy(compositingStrategy)
}
private fun requiresCompositingLayer(): Boolean =            // :355-361
    compositingStrategy == CompositingStrategy.Offscreen || requiresLayerPaint() || renderEffect != null
private fun requiresLayerPaint(): Boolean = blendMode != BlendMode.SrcOver || colorFilter != null
// applyCompositingStrategy, :244-259:
//   Offscreen:     setUseCompositingLayer(true, layerPaint);  setHasOverlappingRendering(true)
//   ModulateAlpha: setUseCompositingLayer(false, layerPaint); setHasOverlappingRendering(false)
//   else (Auto):   setUseCompositingLayer(false, layerPaint); setHasOverlappingRendering(true)
```

So under `Auto`, without a blend mode, color filter or render effect, the node has no compositing layer and `hasOverlappingRendering = true`, and the alpha goes to `renderNode.alpha` (`:65-69`). The offscreen buffer for an alpha below 1 is then HWUI's choice: a node with overlapping rendering and alpha below 1 is drawn through a save layer, which is what the grid's faded cells pay. The rule never reads the alpha.

**(d) `shadow` and `clip` are render node properties; Material's surfaces add two layers.** `Modifier.clip(shape)` is `graphicsLayer(shape = shape, clip = true)` (`ui/draw/Clip.kt:32`). `Modifier.shadow(elevation, shape, clip = elevation > 0.dp, ambientColor = DefaultShadowColor, spotColor = DefaultShadowColor)` adds a layer when `elevation > 0.dp || clip` (`ui/draw/Shadow.kt:110-122`) whose block sets `shadowElevation = elevation.toPx()`, `shape`, `clip`, `ambientShadowColor`, `spotShadowColor` (`:871-877`); `DefaultShadowColor = Color.Black` (`GraphicsLayerScope.kt:37`). On the node (`GraphicsLayerV29.android.kt`): `renderNode.elevation = shadowElevation`, `renderNode.ambientShadowColor` / `spotShadowColor = value.toArgb()` (`:126-142`), and the outline: a layer needs one when `clip || shadowElevation > 0f` (`android/…/AndroidGraphicsLayer.android.kt:634-681`). A rectangle or a rounded rectangle with equal corners becomes `android.graphics.Outline.setRoundRect` (`GraphicsLayer.kt:413-437`), any other shape `Outline.setPath` (API 30+, `AndroidGraphicsLayer.android.kt:697-716`); the outline's alpha is the layer's. With an outline, `clip` sets `renderNode.clipToOutline`, without one `clipToBounds` (`GraphicsLayerV29.android.kt:217-228`); a path outline that can't clip (`!canClip()`) is clipped with `clipPath` in the recording instead. A layer with elevation is drawn into its parent's canvas between `enableZ()` and `disableZ()` (`AndroidGraphicsLayer.android.kt:570-573`, `:608-610`); HWUI casts the shadow from the node's elevation and outline. material3 1.4.0's `Surface` (`material3/Surface.kt:456-475`) is `graphicsLayer(shadowElevation, shape, clip = false)` when `shadowElevation > 0f`, then `border(border, shape)`, `background(color, shape)`, and `clip(shape)` around the content. `Card`, `ElevatedCard` and `OutlinedCard` are that `Surface` with `CardDefaults.shape` (`RoundedCornerShape(12.dp)`), no tonal elevation, and shadow elevations 0, 1 and 0 dp (`FilledCardTokens`, `ElevatedCardTokens`, `OutlinedCardTokens`: `ContainerElevation`; `Card.kt:80-97`). So every card is a clipping render node with a round-rect outline around its content, its background drawn outside it as a rounded rectangle; the `ElevatedCard` adds a non-clipping node with a 1 dp elevation; the `OutlinedCard` draws its 1 dp border, no shadow. The banner's `shadow(6.dp, shape, clip = true)` is one node: elevation, outline and `clipToOutline`.

**(e) A `Path` is an `android.graphics.Path`.** `Path()` is `AndroidPath()` (`android/…/graphics/AndroidPath.android.kt:26`), which wraps `val internalPath: PlatformPath = PlatformPath()` (`:43`, `PlatformPath` being `android.graphics.Path`); `moveTo` and `lineTo` forward to it (`:67-77`), `reset()` is `internalPath.reset()` and `rewind()` `internalPath.rewind()` (`:194-200`). `drawPath(path, color, style = Stroke(…))` configures a paint and calls `canvas.drawPath(path, paint)` (`drawscope/CanvasDrawScope.kt:430-441`), which is `internalCanvas.drawPath(path.asAndroidPath(), paint.nativePaint)` (`android/…/AndroidCanvas.android.kt:194-196`) on the RenderNode's recording canvas. The waves' one remembered `Path` is reset and refilled for each wave: the recording keeps each wave's contents (Skia's paths share their data until written), as D512 found.

**(f) The prefetch time budget.** The request's steps, their averages, and the deadline:

- *Averages.* Per content type (`itemProvider.getContentType(index)`, `null` without one), in each lazy layout's own `PrefetchMetrics` (`LazyLayoutPrefetchState.kt:343-368`): `compositionTimeNanos` (whole composition, used only with pausable composition off), `resumeTimeNanos` (one stretch of pausable composition, up to its next pause check), `pauseTimeNanos` (stopping it), `applyTimeNanos`, `measureTimeNanos`, and `nestedPrefetchCount` (`:370-382`). Each is a weighted running average, `current / 4 * 3 + new / 4` in integer nanoseconds (divided first), and the first sample is taken as is (`if (current == 0L) new`, `:408-418`); the nested count is `(current * 3 + new) / 4` from its first sample (`:420-426`). A content type seen for the first time starts at zero for everything (`getOrPut(contentType) { Averages() }`, `:350-362`: the comment speaks of starting from the overall averages; the code doesn't). The measure average is cleared when the last nested prefetch did fewer items than the ideal count (`:737-739`).
- *The gate.* Each step runs only if `available > required`, `required` being the step's average, or 0 for an urgent request (`shouldExecute`, `:572-579`). The steps of `executeRequest` (`:622-744`), with `available` read once from the scheduler and decreased by each step's elapsed time (`resetAvailableTimeTo`, `updateElapsedAndAvailableTime`, `:585-598`): compose, if `available > resume + pause` averages; apply, if `> apply`; resolve nested states, if `> 0`; the nested requests, each through the same rule (urgent if the parent is, `:856-911`); measure, if `> measure`.
- *Split, not skipped.* A step that doesn't fit returns "more work" (`return true`): the request stays in the queue (`peek`, not `poll`) and continues from that step after the next frame; steps done stay done. Pausable composition (`performPausableComposition`, `:748-786`) checks at each of the runtime's pause points: it saves the time since the last check as a resume sample, and pauses when `available` is no longer above `resume + pause`; the time to stop is saved as a pause sample, and the next frame resumes the same composition. The nested requests are split one request at a time. Measurement is not split.
- *The deadline* (`android/…/lazy/layout/PrefetchScheduler.android.kt`, `AndroidPrefetchScheduler`). Requests are posted to the main thread's handler (`view.post(this)`, `:208-214`), so they run after the frame that scheduled them. In `run()` (`:130-174`): `nextFrameTimeNs = maxOf(frameStartTimeNanos, viewDrawTimeNanos) + frameIntervalNs`, where `viewDrawTimeNanos` is `view.drawingTime` in nanoseconds (the last drawn frame's Choreographer frame time, which `ViewRootImpl` stores in milliseconds) and `frameStartTimeNanos` the `frameTimeNanos` of the last Choreographer callback the scheduler itself posted (`doFrame`, `:201-206`); `availableTimeNanos = max(0, nextFrameTimeNs - System.nanoTime())` (`:248-253`). `frameIntervalNs = 1_000_000_000 / refreshRate`, read once per process from `view.display.refreshRate` when at least 30, else 60 Hz (`:264-278`; 8.33 ms at 120 Hz). So the deadline is the last frame's start plus one interval, about the next vsync; it is not computed from Choreographer's idle time or the next vsync's timestamp. If `System.nanoTime() > viewDrawTimeNanos + 2 * frameIntervalNs` (no draw for two frames), the first request run gets `Long.MAX_VALUE`, no limit (`isFrameIdle`, `:154`, reset after one request, `:189`). It runs requests while each completes and time remains (`runRequest`, `:176-194`); the first that can't finish (or no time left) posts a Choreographer frame callback, whose `doFrame` posts `run()` again after that frame. The queue is a `PriorityQueue` by priority only, high first (`:108-109`).
- *Items ahead*: one (a), and two nested items to start with (b).

**(g) A text keeps its paragraph only for the same width bounds** (since D550; `foundation` 1.12.1, `text/modifiers/`). `BasicText` is `TextStringSimpleNode` (a `String`) or `TextAnnotatedStringNode` (an `AnnotatedString`), whose `measure` calls `layoutCache.layoutWithConstraints(constraints, layoutDirection)` and, when that returns true, `invalidateLayer()` (`TextStringSimpleNode.kt:385-408`). Both caches decide the same way (`ParagraphLayoutCache.kt:318-347`; `MultiParagraphLayoutCache.kt:387-414`, the same lines over its `TextLayoutResult`):

```kotlin
private fun newLayoutWillBeDifferent(constraints: Constraints, layoutDirection: LayoutDirection): Boolean {
    val localParagraph = paragraph ?: return true
    val localParagraphIntrinsics = paragraphIntrinsics ?: return true
    if (localParagraphIntrinsics.hasStaleResolvedFonts) return true      // async typeface changes
    if (layoutDirection != intrinsicsLayoutDirection) return true
    if (constraints == prevConstraints) return false                   // identical constraints
    if (constraints.maxWidth != prevConstraints.maxWidth) return true
    if (constraints.minWidth != prevConstraints.minWidth) return true
    // if we get here width won't change, height may be clipped
    if (constraints.maxHeight < localParagraph.height || localParagraph.didExceedMaxLines) {
        return true                                                    // vertical clip changes
    }
    return false                                                       // breaks can't change, height can't change
}
```

So any new `maxWidth` or `minWidth` lays the text out again: a single line growing past its width, or shrinking but staying wider than it, gets a new `Paragraph` all the same. Alignment, `softWrap`, `overflow`, `maxLines` and `minWidth` play no part in the decision; only a height-only change looks at the paragraph (taller than the new `maxHeight`, or cut at `maxLines`). Kept, it only updates its size and overflow: `layoutSize = finalConstraints.constrain(IntSize(min(maxIntrinsicWidth, paragraph.width).ceilToIntPx(), paragraph.height.ceilToIntPx()))` (`ParagraphLayoutCache.kt:166-183`; `MultiParagraphLayoutCache.kt:190-200` makes a new `TextLayoutResult` around the same `MultiParagraph`). With `minLines > 1`, `MinLinesConstrainer` raises the constraints' `minHeight` first (`:159-164`). A new text, font resolver, overflow, soft wrap or line count clears everything (`update` → `markDirty`, `:241-259`, `:349-358`), as a new density does (`:69-84`); so does a new style, but only when its layout attributes differ: the node calls `updateLayoutRelatedArgs`, whose `changed = !this.style.hasSameLayoutAffectingAttributes(style)` (`TextStringSimpleNode.kt:191-201`; ui-text's `TextStyle.hasSameLayoutAffectingAttributes`: the paragraph style and the span style's size, weight, style, synthesis, family, feature settings, letter spacing, baseline shift, geometric transform, locales, background and platform style). A new color, decoration, shadow or draw style only redraws (`updateDraw`, `hasSameDrawAffectingAttributes`, `:172-180`): the kept paragraph is painted with them (`localParagraph.paint(canvas, color, shadow, drawStyle, textDecoration)`, `:479-505`).

What a new layout costs: `layoutText` makes a new `Paragraph` from the cache's `ParagraphIntrinsics`, kept across constraint changes (`setLayoutDirection`, `:266-289`) at `finalConstraints(constraints, softWrap, overflow, localParagraphIntrinsics.maxIntrinsicWidth)` (`:297-312`), which reads the text's own width every time. That width is Compose's `finalMaxWidth` (`LayoutUtils.kt:38-66`): `constraints.maxWidth` if `softWrap || overflow.isEllipsis` and bounded, else `Infinity`; that if `minWidth` equals it, else `maxIntrinsicWidth.ceilToIntPx().coerceIn(constraints.minWidth, maxWidth)`. On Android the intrinsics hold the text with its spans, the paint and a `LayoutIntrinsics` (ui-text, `AndroidParagraphIntrinsics.layoutIntrinsics`, passed to each `TextLayout`), which measures the boring metrics (`BoringLayout.isBoring`) and `maxIntrinsicWidth` (`boringMetrics?.width`, else `ceil(Layout.getDesiredWidth(…))`, + 0.5 with letter spacing) once and caches them (`_boringMetrics`, `_maxIntrinsicWidth`): a new width builds a `BoringLayout` from the cached metrics or a `StaticLayout`, without measuring the text's width again. An `Ellipsis` paragraph taller than `constraints.maxHeight` is laid out again with as many lines as fit (`AndroidParagraph`, `numberOfLinesThatFitMaxHeight`), which is what the height clause above protects.

Two other caches differ, and `BasicText` uses neither. `TextMeasurer`'s `TextLayoutCache` (ui-text) keys an LRU by `CacheTextLayoutInput`, whose `equals` compares the text, the style's layout attributes, placeholders, max lines, soft wrap, overflow, density, direction, font resolver and the exact `Constraints` (read from the 1.12.1 class: the sources jar isn't in the Gradle cache). The legacy `CoreTextField`'s `TextDelegate` reuses a `TextLayoutResult` through `canReuse` (`TextLayoutHelper.kt:44-89`): the same `minWidth`, then `if (!(softWrap || overflow == TextOverflow.Ellipsis)) return true` ("If width does not matter, we can result the same layout"), else the same `maxWidth` and `maxHeight`.

## Method

measure.py's stress scenarios (D401), beside [12](12-android-performance.md)'s, which are unchanged:

- **`stress_startup`:** cold starts of `stress` (`am start -W`: `TotalTime`), `--runs` times, interleaved, and one traced start for the first frame's breakdown.
- **`stress_scroll`:** `scroll`'s method on `stress`. That is the memory 5 s after start, then a warm-up swipe left out, 10 flings 1.2 s apart, the memory after them, and 3 runs per app, interleaved.
- **`stress_animation`:** `animation`'s method on `stress_animation`: 10 s untouched, 3 s after start, 3 runs per app. Each run also records the memory before the trace (3 s after start) and after it (13 s), and since round 13 HWUI's GPU caches at both points (`dumpsys gfxinfo`: Skia's cache by kind, its total and purgeable part; `gpu_caches_mb`).

`--scenarios stress_startup,stress_scroll,stress_animation` runs these alone; the report holds the scenarios run (`"scenarios"`), and `summarize.py` prints what's there.

The stress traces get a 384 MB buffer, against 64 MB for the feed's. HWUI traces every drawing operation of the animation's 140 layers, about 110,000 slices a second. A 64 MB ring buffer wrapped within 5 s and lost the process table scanned at the start, and with it every frame.

Every traced run now also lists its 5 slowest `doFrame`s and `DrawFrame`s, with the 8 longest slices inside each (`slowest_frames`). That is where the analysis below starts.

That tracing (atrace's `gfx` category) costs both RenderThreads time. On these screens it costs more than on the feed, so the RenderThread's durations here are traced durations; both apps pay it.

## Results

Round 14 of [12](12-android-performance.md): compose-rs at 3ac74a6 against Compose 1.12.1, every scenario, at parity (`results/2026-10-25-pixel-8-pro-parity.json`) and with every optimization on (`…-all.json`), then each optimization alone against parity (`…-each.json`; [14](14-performance-opportunities.md), "Backport candidates", and "Parity mode" below). measure.py's checks passed: the installed native libraries were those built, both apps were `speed` at the start and the end, the compose-rs traces hold its prefetch slices (377 to 384 `compose-rs: prefetch` steps a stress list run at parity), and FrameTimeline's janky frames are split by side. Every run started at thermal status 0, battery ≤ 34 °C and skin ≤ 35 °C (no time limit, our apps stopped while waiting): the stress animation's runs started at battery 28.9 to 33.0 °C and skin 30.3 to 34.9 °C and ended at most at 33.4 °C and 36.6 °C; the guard waited 20 to 30 s before 4 of its 12 runs. No run was throttled: gfxinfo's median frame time was 14 to 15 ms in every animation run, as in round 13, and none ran at a thermal status above 0. Medians of 3 runs, startup of 15. The better side is in bold; ties aren't.

"Frames presented at another vsync than expected" counts frames shown later than FrameTimeline predicted. In the animation both apps' main thread and RenderThread together take longer than a vsync, and most runs are presented a vsync late throughout, on cadence: latency, not dropped frames; gfxinfo's 14 to 15 ms median frame time for both apps says the same.

The RenderThread's boost is the higher of the active sessions' votes when each frame started: HWUI's alone at parity, HWUI's and compose-rs's own (D390) with `all` (the rounds' JSONs, `d390_boost`).

### At parity: stress scroll (10 flings, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,804 | 1,785 |
| Janky frames (FrameTimeline), per run | 2, 2, 2 | **0, 0, 0** |
| of which the app's | 2, 2, 2 | **0, 0, 0** |
| of which the compositor's | 0, 0, 0 | 0, 0, 0 |
| Frames presented at another vsync than expected, per run | 52, 16, 123 | **0, 0, 0** |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 31 (11), 37 (6), 26 (8) | **15 (5), 4 (1), 11 (0)** |
| Frame time p50 / p90 / p99 (gfxinfo) | **5** / 8 / 13 ms | 6 / **7** / **9** ms |
| Main thread per frame p50 / p90 / p99 | **1.29** / 3.14 / 7.95 ms (max 15.4) | 1.39 / **2.36** / **4.11** ms (max 7.5) |
| RenderThread per frame p50 / p90 / p99 | **2.61** / **3.71** / **5.45** ms | 3.13 / 4.60 / 6.47 ms |

| The boost (medians; nothing ranked) | Kotlin Compose | compose-rs, parity | compose-rs, all (Compose in that session) |
|---|---|---|---|
| RenderThread's boost minimum, time-weighted (out of 1024; runs) | 277 (239 to 286) | 67 (58 to 75) | 356, of which HWUI's session's alone 10 (285) |
| RenderThread frames unboosted / at 100–199 / at 200 or more | 8 % / 17 % / 76 % | 71 % / 22 % / 4 % | 0 % / 5 % / 95 % (3 % / 17 % / 80 %) |
| `DrawFrame` p50: unboosted / 100–199 / 200 or more | 3.23 / 3.09 / 2.50 ms | 3.38 / 2.81 / 2.15 ms | none / 2.69 / 2.07 ms (3.48 / 3.03 / 2.42 ms) |
| RenderThread running time on the little cores | 4 % | 19 % | 1 % (2 %) |
| Main thread running time on the little cores | 14 % (11 to 16 %) | 76 % (72 to 79 %) | 3 % (15 %) |

**No frame of compose-rs's missed the app's deadline; Compose missed 2 in each run.** Its slowest frames build a newly revealed item inside the frame (`AndroidOwner:measureAndLayout` in the fling's `Recomposer:animation`, 9.2 to 12.8 ms; the frames 13.6 to 15.4 ms), as in every round; its runs then presented 16 to 123 frames a vsync late. compose-rs prefetched 377 to 384 times a run; one frame in 3 runs laid a text out (1.6 ms, the second run), and each run had 11 or 12 frames with a second round of composition and layout (2.9 to 3.9 ms a run in all), most likely the sticky header changing, as in rounds 11 to 13.

**The main thread is ahead at p90 and p99 and behind at p50.** compose-rs's slowest main-thread frames (5.3 to 7.5 ms) wait for the RenderThread's sync (`postAndWait`, up to 6.5 ms) or record the items the frame reveals (`Record View#draw()`, up to 4.0 ms: at parity nothing is recorded ahead, D564). Its p50 (1.39 against 1.29 ms) is where it runs: 76 % of its time on the little cores, against Compose's 14 %.

**The RenderThread is behind at every percentile, for 32 % less work.** HWUI's session reports each frame from the main thread's start to the RenderThread's end against 11.62 ms, and compose-rs's main-thread frames are short, so its frames are rarely reported long and rarely boosted: 71 % of them start unboosted against Compose's 8 %. At each boost level compose-rs's `DrawFrame`s are the shorter (2.81 against 3.09 ms at 100–199, 2.15 against 2.50 ms at 200 or more), but most of its frames are at the lowest level, where they take 3.38 ms. Its RenderThread's CPU time is 20 % above Compose's (5.96 against 4.95 s) for that less work: the same frames at lower clocks. With its own session (`all`), compose-rs is boosted on every frame and its RenderThread is the shorter at every percentile (below).

**`doFrame`s starting late**: 4 to 15 a run, none janky, the latest 3.8 ms late, the closest finishing 5.7 ms before its deadline (the round's JSON, `late_do_frames`). Six followed a prefetch step that ran past the vsync: steps of 3.5 to 8.0 ms, most of each an item's measurement (4.6 to 7.7 ms) on a little core, unboosted, ending 1.5 to 3.6 ms after the vsync. A measurement isn't split in Compose either; a composition isn't split here (no pausable composition), and the longest steps' compositions took up to 4.1 ms. Compose's: 26 to 37 a run, up to 8.3 ms late, 6 to 11 a run after its own work, the closest finishing 0.3 ms before its deadline.

### At parity: stress animation (10 s, 3 runs)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,452 | 1,460 |
| Janky frames (FrameTimeline), per run | 0, 2, 0 | **0, 0, 0** |
| Frames presented at another vsync than expected, per run | 1,203, 1,069, 1,197 | 0, 1,203, 1,199 |
| `doFrame`s starting more than 2 ms late, per run | 4, 13, 16 | **1, 0, 0** |
| Frame time p50 / p90 / p99 (gfxinfo) | 15 / 17 / 20 ms | **14** / **16** / **17** ms |
| Main thread per frame p50 / p90 / p99 | 3.70 / 5.36 / 8.03 ms (max 10.7) | **2.37** / **4.01** / **6.03** ms (max 8.5) |
| RenderThread per frame p50 / p90 / p99 | **7.48** / 8.94 / 10.77 ms | 7.67 / **8.77** / **10.14** ms |
| RenderThread's boost minimum, time-weighted; frames unboosted | 719; 0 % | 556; 0 % |

**Both apps run near their limit**, boosted on every frame. Both RenderThreads take most of a 120 Hz vsync (8.33 ms) at p50 and more at p90. Compose's second run missed 2 deadlines; no frame of compose-rs's did, though in the each round parity's 5 runs had one janky frame in 3 of them ("Each optimization alone (round 14)"). Two of compose-rs's runs and all of Compose's were presented a vsync late throughout, on cadence.

**The RenderThread is level with Compose's**: 3 % longer at p50, 2 % and 6 % shorter at p90 and p99, for 5 % less work (5,803 against 6,118 ms). At parity compose-rs draws what Compose draws: the 132 faded cells offscreen (D521), each card and clip a render node (D522–D525), the waves one reset `Path` (D530–D535), every paragraph into its layer's recording (D567), and no flush layer (D566). Its slowest RenderThread frames (11.3 to 13.6 ms, one a run) wait for the GPU (`Vulkan finish frame`, 4.2 to 4.7 ms) and flush the layers (3.4 to 4.4 ms); Compose's (13.6 to 13.9 ms) the same (its layers' flush 5.0 to 5.6 ms), or wait in `setFrameTimelineInfo` (6.1 ms).

**The main thread is ahead**, a third shorter at p50 and a quarter at p99, for half the work: compose-rs's transitions run one task each (D420), its cells' properties are set on render nodes without recording (D157), and its slowest frames (7.1 to 8.5 ms) wait for the RenderThread's sync (`postAndWait`, 5.2 to 6.8 ms).

### At parity: work

| Stress scroll (10 flings) | Kotlin Compose | compose-rs, parity | compose-rs, all (Compose in that session) |
|---|---|---|---|
| Main thread work | 1,214 ms | **475 ms** (−61 %) | 816 ms (−35 %; 1,251 ms) |
| RenderThread work | 1,648 ms | **1,119 ms** (−32 %) | 1,562 ms (−6 %; 1,664 ms) |
| Main thread CPU time | 3.94 s | **3.45 s** (−12 %) | 2.27 s (−42 %; 3.89 s) |
| RenderThread CPU time | **4.95 s** | 5.96 s (+20 %) | 3.84 s (−20 %; 4.80 s) |

| Stress animation (10 s) | Kotlin Compose | compose-rs, parity | compose-rs, all (Compose in that session) |
|---|---|---|---|
| Main thread work | 2,998 ms | **1,559 ms** (−48 %) | 1,564 ms (−48 %; 3,007 ms) |
| RenderThread work | 6,118 ms | **5,803 ms** (−5 %) | 3,313 ms (−46 %; 6,128 ms) |
| Main thread CPU time | 4.22 s | **2.53 s** (−40 %) | 2.82 s (−34 %; 4.27 s) |
| RenderThread CPU time | 7.06 s | **6.21 s** (−12 %) | 5.46 s (−23 %; 7.10 s) |
| HWUI's worker threads (`hwuiTask0`, `hwuiTask1`), CPU time | **8.5 s** | 10.1 s | 10.6 s (8.5 s) |

**Work** is milliseconds on the fastest core at its highest frequency ([12](12-android-performance.md), "Method"); it reads lower at low clocks, and on the list compose-rs's threads ran less boosted than Compose's at parity, more boosted with `all`. In the animation both apps are boosted on every frame, so the comparison holds at parity: compose-rs's main thread does half Compose's work, its RenderThread 5 % less. With `all`, `single_drawing_fade` takes the RenderThread's offscreen layers out (5,803 → 3,313 ms; [14](14-performance-opportunities.md) has it alone).

**HWUI's two worker threads** cost compose-rs 18 % more CPU time than Compose (10.1 against 8.5 s; 10.6 s with `all`), as in every round since the first results: they rasterize the 50 stroked waves on the CPU into masks (Skia's software path renderer), the same work in both apps, at lower clocks for compose-rs (D511, "HWUI's workers and graphics memory").

### At parity: prefetch on the stress list

As [12](12-android-performance.md)'s feed table, per run, medians of 3 (the rounds' JSONs, `prefetch_slices`, `lazy_lists`); Compose's compositions are pausable (its `compose` slices are each a stretch; an apply follows), and its nested prefetch (a carousel's first tiles) is a slice of its own; compose-rs's item compositions include the carousels' tiles. Nothing ranked:

| Stress list scroll, per run | compose-rs, parity | compose-rs, all | Compose (parity session) |
|---|---|---|---|
| Prefetch steps | 382 | 358 | |
| Compositions; work each | 639; 0.061 ms | 678; 0.093 ms | 384 slices; 0.261 ms, nested 55; 0.451 ms (apply: 341; 0.094 ms) |
| Composition work | 38.6 ms | 63.5 ms | 98.9 ms, nested 24.8 ms (apply 32.1 ms) |
| Measurements; work each; wall time each | 215; 0.180 ms; 1.29 ms | 322; 0.221 ms; 0.55 ms | 177; 0.548 ms; 1.42 ms |
| Paragraphs laid out (main thread) | 583 | 625 | |
| Paragraphs recorded while prefetching | none | 201 | none |
| All prefetch work | 82.3 ms | 144.2 ms | |
| Main thread on the little cores, while prefetching | 68 to 79 % | 2 to 10 % | 4 to 16 % |
| Second rounds of composition and layout in frames | 11, 12, 12 | 10, 10, 10 | |

### At parity: memory

| | Kotlin Compose | compose-rs |
|---|---|---|
| Stress list, PSS 5 s after start | 63.1 MB | **45.0 MB** |
| of which Java heap / native heap / code / graphics | 5.3 / 10.6 / 7.2 / 34.1 MB | 1.3 / 13.0 / 2.1 / 23.6 MB |
| Stress list, PSS after 10 flings | 114.3 MB | **90.4 MB** |
| of which Java heap / native heap / graphics | 15.3 / 22.6 / 63.1 MB | 5.1 / 15.7 / 62.2 MB |
| Stress animation, PSS 3 s after start | 168.0 MB (167.7 to 187.6) | **152.2 MB** (151.9 to 154.3) |
| of which Java heap / native heap / graphics | 13.7 / 21.8 / 119.4 MB | 4.8 / 20.4 / 119.8 MB |
| HWUI's GPU caches 3 s after start, total (`dumpsys gfxinfo`) | 89.7 MB | 89.8 MB |
| Stress animation, PSS after 13 s | 255.5 MB (217.6 to 259.6) | **216.0 MB** (206.3 to 245.5) |
| of which Java heap / native heap / graphics | 76.7 / 26.1 / **140.6** MB | 6.9 / 28.2 / 171.5 MB |
| HWUI's GPU caches after 13 s, total | **109.6 MB** (108.6 to 109.6) | 118.7 MB (100.4 to 128.3) |

**On the list** compose-rs holds 29 % less after start and 21 % less after scrolling; its native heap is 13.0 MB after start and 15.7 MB after scrolling (round 13, with kept items: 14.7 and 17.2 MB; with `all` this round 14.2 and 17.4 MB).

**In the animation, 3 s after start** both apps hold the same graphics memory (119.8 against 119.4 MB) and Skia caches (89.8 against 89.7 MB): without D510's flush layer, the frame's offscreen layers flush the dropped masks in, as Compose's faded cells do for it. **After 13 s** compose-rs's graphics memory has grown more (164.2, 171.5 and 203.0 MB in its runs, against Compose's 137.6 to 141.6 MB), and its Skia caches by 9 MB more at the median; its PSS is still below Compose's (216.0 against 255.5 MB), Compose's Java heap holding 76.7 MB by then (ours 6.9 MB). `flush_layers_before_dropped_paths` takes compose-rs's caches after 13 s down ("Parity mode", "Each optimization alone (round 14)"). How much the GPU driver's memory grows varies from run to run in both apps, as round 13 found (with `all` this round: 131.7 to 178.3 MB for compose-rs, 149.5 to 163.8 MB for Compose).

### At parity: startup

| | Kotlin Compose | compose-rs |
|---|---|---|
| `stress` (`TotalTime`, 15 cold starts) | 148 ms (142 to 173) | **116 ms** (110 to 128; −22 %) |
| The first frame's main thread (`traversal`, one traced start) | 57.3 ms | **34.6 ms** |
| `bindApplication` | 19.1 ms | **12.7 ms** |

compose-rs's first frame's own part (`compose-rs: frame`) took 7.2 ms, 5.5 ms of it in 3 rounds of composing and laying out again, laying out 29 paragraphs (3.9 ms); its replay 2.9 ms (it makes the bitmaps) and `Record View#draw()` 4.0 ms. With `all`, 117 ms (Compose 147 ms in that session): startup doesn't depend on the optimizations.

### Every optimization on (`all`)

compose-rs with every optimization on, against Compose in the same session, run right after the parity round; the parity column is the parity round's.

| Stress scroll | Kotlin Compose (this session) | compose-rs, all | compose-rs, parity |
|---|---|---|---|
| Janky frames, per run: the app's; the compositor's | 2, 3, 2; 0, 2, 0 | **0, 0, 0; 0, 0, 0** | 0, 0, 0; 0, 0, 0 |
| Frames presented at another vsync than expected, per run | 4, 107, 4 | **0, 0, 0** | 0, 0, 0 |
| `doFrame`s more than 2 ms late, per run (after main-thread work) | 27 (7), 20 (7), 29 (10) | **20 (0), 26 (1), 14 (0)** | 15 (5), 4 (1), 11 (0) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 8 / 13 ms | 5 / **5** / **7** ms | 6 / 7 / 9 ms |
| Main thread p50 / p90 / p99 | 1.24 / 3.24 / 8.41 ms | **0.94** / **1.36** / **2.64** ms | 1.39 / 2.36 / 4.11 ms |
| RenderThread p50 / p90 / p99 | 2.51 / 3.58 / 5.13 ms | **2.09** / **2.87** / **4.02** ms | 3.13 / 4.60 / 6.47 ms |
| Work: main thread / RenderThread | 1,251 / 1,664 ms | **816** / **1,562** ms | 475 / 1,119 ms |
| Boost minimum, time-weighted; frames unboosted | 285; 3 % | 356; 0 % | 67; 71 % |

| Stress animation | Kotlin Compose (this session) | compose-rs, all | compose-rs, parity |
|---|---|---|---|
| Janky frames, per run (all the app's) | 1, 0, 0 | 1, 0, 0 | 0, 0, 0 |
| Frames presented at another vsync than expected, per run | 1,202, 1,201, 1,204 | 440, 0, 1,205 | 0, 1,203, 1,199 |
| Frame time p50 / p90 / p99 (gfxinfo) | 15 / 17 / 20 ms | 15 / **16** / **18** ms | 14 / 16 / 17 ms |
| Main thread p50 / p90 / p99 | 3.79 / 5.62 / 8.67 ms | **2.41** / **2.97** / **4.08** ms | 2.37 / 4.01 / 6.03 ms |
| RenderThread p50 / p90 / p99 | 7.44 / 8.91 / 11.10 ms | **6.41** / **7.43** / **8.56** ms | 7.67 / 8.77 / 10.14 ms |
| Work: main thread / RenderThread | 3,007 / 6,128 ms | **1,564** / **3,313** ms | 1,559 / 5,803 ms |
| Graphics memory 3 s after start / after 13 s | **119.4** / 154.1 MB | 131.6 / **144.0** MB | 119.8 / 171.5 MB |
| Boost minimum, time-weighted (of which HWUI's alone) | 719 | 482 (427) | 556 |

**With every optimization, compose-rs is ahead at every percentile on both threads on both screens.** On the list the hint session boosts every frame (vote 356, none unboosted), and the RenderThread goes from 3.13 / 4.60 / 6.47 ms at parity to 2.09 / 2.87 / 4.02 ms, against Compose's 2.51 / 3.58 / 5.13 ms; the main thread's p99 is a third of Compose's (2.64 against 8.41 ms). In the animation, `all` takes the RenderThread's work down 43 % against parity (5,803 → 3,313 ms; `single_drawing_fade`'s, mostly) and its p99 from 10.14 to 8.56 ms (Compose: 11.10 ms), and the main thread waits less for its sync (p90 4.01 → 2.97 ms). compose-rs's one janky frame there (its first run) was a GPU wait (`Vulkan finish frame`, 5.6 ms in a 9.7 ms `DrawFrame`); Compose had one too. Its graphics memory 3 s after start varied from run to run (114.7, 131.6 and 178.3 MB), its PSS after 13 s was below Compose's (184.8 against 232.2 MB).

## Round 13 (history)

Round 13 of [12](12-android-performance.md), compose-rs at d39ecb4 with its own defaults then (its hint session, drawing ahead, kept items, two ahead, D341's budget, single-drawing fades, paragraph render nodes, the flush layer), against Compose 1.11.4; superseded by round 14 above, kept as history. The short answer it gave was: on the list, compose-rs's frames shorter at every percentile on both threads (main p50 0.83 against 1.26 ms, RenderThread p50 / p90 / p99 2.12 / 2.90 / 3.86 against 2.53 / 3.53 / 5.03 ms), boosted by its session (vote 341); in the animation, main p50 2.26 against 3.72 ms and RenderThread p50 6.39 against 7.42 ms, most of the RenderThread gap being the single-drawing fade; 91.2 against 112.1 MB after scrolling, 144.1 against 165.7 MB 3 s into the animation; startup 119 against 147 ms.

compose-rs at d39ecb4: round 13 of [12](12-android-performance.md), which ran every scenario (`results/2026-10-24-pixel-8-pro-round-13.json`, `speed`, `--runs 15`). Round 12 (4a28979, `results/2026-10-24-pixel-8-pro-round-12.json`) is quoted in parentheses. Since round 12, compose-rs gained one change, which touches the stress animation only:

- **D510–D512**: a frame that drops a path drawn as a path first draws a cleared 1 × 1 compositing layer, so HWUI flushes its layers before the frame and Skia's GPU cache reuses the dropped paths' mask textures. Its interleaved A/B, traced with the power rails, is in "HWUI's workers and graphics memory" below; this round confirms it beside Compose without them.

The Kotlin app didn't change on these screens. The changes since the first results (921db6b, "First results" below) are D390 (compose-rs's own hint session for the RenderThread), D410–D412 (paths reused, "With paths reused"), D420–D424 (one task per infinite transition, "Infinite transitions, one task each"), D430–D435 (lazy items reused by content type), D440 (the main thread in the hint session), D470–D496 (lazy lists place only the items in view, "The lazy list's policy"), D500 (both threads' work in the session, "The hint session's target") and D510.

measure.py's checks passed: the installed native libraries were those built, and both apps were `speed` at the start and the end. Medians of 3 runs, startup of 15. The better side is in bold; ties aren't.

"Frames presented at another vsync than expected" counts frames shown later than FrameTimeline predicted. In the animation, all of Compose's frames are, in every run, and compose-rs's were in two of this round's three runs (two in round 12 too): the main thread and the RenderThread together take longer than a vsync. They are presented a vsync late but on cadence: latency, not dropped frames. gfxinfo's 14 to 15 ms median frame time for both apps says the same.

The RenderThread's boost below is the higher of both sessions' votes when each frame started (HWUI's and, for compose-rs, D390's), from the round's JSON (`d390_boost`); measure.py's `render_boost` reads HWUI's alone.

### Round 13: stress scroll, frames (10 flings, 3 runs)

| | Kotlin Compose | compose-rs (round 12) |
|---|---|---|
| Frames (gfxinfo) | 1,798 | 1,795 |
| Janky frames (FrameTimeline), per run | 3, 1, 1 | **2, 0, 0** (0, 0, 3) |
| of which the app's | 3, 1, 1 | **0, 0, 0** (0, 0, 0) |
| of which the compositor's | 0, 0, 0 | 2, 0, 0 (0, 0, 3) |
| Frames presented at another vsync than expected, per run | 211, 2, 4 | 135, 0, 0 (0, 0, 3) |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 32 (8), 26 (7), 37 (12) | **19 (2), 13 (0), 23 (0)** (13 (0), 15 (0), 14 (0)) |
| Frame time p50 / p90 / p99 (gfxinfo) | 5 / 7 / 12 ms | 5 / **5** / **7** ms (5 / 5 / 7 ms) |
| Main thread per frame p50 / p90 / p99 | 1.26 / 3.23 / 8.01 ms (max 14.5) | **0.83** / **1.24** / **2.39** ms (max 3.9) (0.81 / 1.23 / 2.52 ms, max 3.8) |
| RenderThread per frame p50 / p90 / p99 | 2.53 / 3.53 / 5.03 ms | **2.12** / **2.90** / **3.86** ms (2.00 / 2.75 / 3.94 ms) |

| The boost (medians; nothing ranked) | Kotlin Compose | compose-rs (round 12) |
|---|---|---|
| RenderThread's boost minimum, time-weighted (out of 1024) | 309 (runs: 271 to 333) | 341 (327 to 361), of which HWUI's session's alone 6 (369, 5) |
| RenderThread frames unboosted / at 100–199 / at 200 or more | 2 % / 14 % / 85 % | 0 % / 5 % / 95 % (0 % / 5 % / 95 %) |
| `DrawFrame` p50: unboosted / 100–199 / 200 or more | 3.58 / 3.14 / 2.45 ms | none / 2.75 / 2.09 ms (3.14 / 2.74 / 1.98 ms) |
| RenderThread running time on the little cores | 2 % | 0 % (0 %) |
| Main thread running time on the little cores | 13 % (10 to 20 %) | 4 % (4 to 11 %) (4 %, 3 to 5 %) |

**No frame of compose-rs's missed the app's deadline**; Compose missed 5, in all three runs. Its slowest frames compose and measure a newly revealed item inside the frame (`AndroidOwner:measureAndLayout`, 9.6 and 6.0 ms in the first and third runs' slowest, in the fling's `Recomposer:animation`), as in the first results ("Where each app struggles"); its first run then presented 211 frames late. compose-rs prefetched 348 to 353 times a run (337 to 356 in round 12). No frame of its laid text out, in any run; each run had 10 frames with a second round of composition and layout (`recompose and layout again`, 1.4 to 1.9 ms a run in all), as in rounds 11 and 12: most likely the sticky header changing.

**compose-rs's first run had 2 compositor-side janky frames** (the display HAL, a SurfaceFlinger scheduling miss) and then presented 135 frames a vsync late, on cadence. Round 12 had 3 in one run (prediction errors and a SurfaceFlinger scheduling miss), D500's A/B none on this list in 10 runs; none of Compose's this round. Its other two runs entered Choreographer's buffer stuffing recovery and presented no frame late.

**Both threads' frames are as in round 12**, which D510 doesn't touch on this list: the RenderThread's p50 / p90 / p99 2.00 / 2.75 / 3.94 → 2.12 / 2.90 / 3.86 ms, the main thread's 0.81 / 1.23 / 2.52 → 0.83 / 1.24 / 2.39 ms, both shorter than Compose's at every percentile (2.53 / 3.53 / 5.03 and 1.26 / 3.23 / 8.01 ms). Our session's vote was 341 (round 12: 369; D500's A/B: 332), with 95 % of the RenderThread's frames starting at 200 or more both times; at that level its frames' p50 is 2.09 against Compose's 2.45 ms. The RenderThread's p50 rose 0.12 ms with the lower vote, within the runs' spread over rounds (1.91 to 2.13 ms across round 12's runs and this round's).

**`doFrame`s starting late**: 13 to 23 a run (13 to 15 in round 12), none janky, the latest 6.2 ms late, the closest finishing 5.4 ms before its deadline; the main thread was asleep, never runnable, between the vsync and the `doFrame` (the round's JSON, `late_do_frames`). Two in the first run followed work on the main thread: one the system's (`MSG_CHECK_INVALIDATION_IDLE`), one a prefetch step that measured an item for 8.2 ms on a little core (6.3 ms of it measuring, 5.3 ms laying text out) and ended 2.2 ms after the vsync; that frame was on time. Compose's: 26 to 37 a run, up to 10.5 ms late, 7 to 12 a run after its own work, the closest finishing 1.5 ms before its deadline.

### Round 13: stress animation, frames (10 s, 3 runs)

| | Kotlin Compose | compose-rs (round 12) |
|---|---|---|
| Frames (FrameTimeline) | 1,205 | 1,203 |
| Janky frames (FrameTimeline), per run | 0, 0, 0 | 0, 0, 0 (0, 0, 0) |
| Frames presented at another vsync than expected, per run | 1,205, 1,202, 1,206 | 1,201, 1,207, 0 (1,205, 0, 1,196) |
| `doFrame`s starting more than 2 ms late, per run | 9, 8, 11 | **4, 3, 0** (6, 6, 3) |
| Frame time p50 / p90 / p99 (gfxinfo) | 15 / 17 / 19 ms | **14** / **16** / **18** ms (14 / 16 / 18 ms) |
| Main thread per frame p50 / p90 / p99 | 3.72 / 5.44 / 7.86 ms (max 11.6) | **2.26** / **2.84** / **3.88** ms (max 5.8) (2.25 / 2.81 / 3.80 ms, max 6.2) |
| RenderThread per frame p50 / p90 / p99 | 7.42 / 8.87 / 10.80 ms | **6.39** / **7.54** / **8.49** ms (6.36 / 7.38 / 8.53 ms) |
| RenderThread's boost minimum, time-weighted; frames unboosted | 715; 0 % | 479 (HWUI's alone 416); 0 % (472 (426); 0 %) |

**Both apps run near their limit**, boosted on every frame. Both RenderThreads take most of a 120 Hz vsync (8.33 ms) at p50, and more at p90 (Compose) or p99 (compose-rs). No frame of either app missed its deadline. Two of compose-rs's runs were presented a vsync late throughout, on cadence, as in round 12, and all of Compose's were.

**compose-rs's frames** moved within the runs' spread of round 12 on both threads (main thread p50 2.25 → 2.26 ms, p99 3.80 → 3.88 ms; RenderThread p50 6.36 → 6.39 ms, p90 7.38 → 7.54 ms, p99 8.53 → 8.49 ms). D510's second flush a frame shows a little at p90, as its A/B measured (+0.13 to +0.15 ms at each percentile there, traced with the power rails). Compose's main thread takes 65 % longer at the median and twice as long at p99; its RenderThread 16 % longer at the median and 27 % at p99: its faded cells are drawn offscreen ("With `ModulateAlpha`" below).

**compose-rs's slowest RenderThread frames** (9.5 to 13.6 ms, one a run) were again GPU waits (`Vulkan finish frame`, 6.5 to 8.4 ms); its slowest main-thread frame (7.4 ms) waited for the RenderThread's sync (`postAndWait`, 5.7 ms). None missed a deadline. Compose's slowest RenderThread frames took 13.7 to 18.7 ms (its layers' flush 5.9 to 6.5 ms, or a GPU wait of 15.0 ms).

### Round 13: work

| Stress scroll (10 flings) | Kotlin Compose | compose-rs (round 12) |
|---|---|---|
| Main thread work | 1,314 ms | **710 ms** (−46 %; 714 ms) |
| RenderThread work | 1,706 ms | **1,541 ms** (−10 %; 1,535 ms) |
| Main thread CPU time | 3.84 s | **2.09 s** (2.08 s) |
| RenderThread CPU time | 4.75 s | **3.95 s** (3.77 s) |

| Stress animation (10 s) | Kotlin Compose | compose-rs (round 12) |
|---|---|---|
| Main thread work | 3,028 ms | **1,493 ms** (−51 %; 1,462 ms) |
| RenderThread work | 6,049 ms | **3,362 ms** (−44 %; 3,172 ms; Compose with `ModulateAlpha`: 3,286 ms in its session) |
| Main thread CPU time | 4.32 s | **2.67 s** (2.66 s) |
| RenderThread CPU time | 6.98 s | **5.57 s** (5.47 s) |
| HWUI's worker threads (`hwuiTask0`, `hwuiTask1`), CPU time | **8.6 s** (runs: 8.55 to 8.60 s) | 10.8 s (10.58 to 10.81 s) (10.8 s, the second run) |

**Work** is milliseconds on the fastest core at its highest frequency ([12](12-android-performance.md), "Method"). compose-rs's main thread does half Compose's work in the animation and 46 % less on the list (44 % in round 12). On the list, its RenderThread does 10 % less work than Compose's, as in round 12, for 17 % less CPU time; both threads did what they did in round 12 (the same prefetch steps and paragraphs, none laid out in a frame: [12](12-android-performance.md), "Item reuse on the device"). Boosted, both apps' threads run at high frequencies, where the metric reads more for the same code (D500's A/B on this list: +18 % and +12 %), and both were boosted about as often (95 % of compose-rs's RenderThread frames at 200 or more, 85 % of Compose's).

**In the animation, compose-rs's RenderThread did 6 % more work than in round 12** (3,172 → 3,362 ms) for 2 % more CPU time (5.47 → 5.57 s): D510's second flush and submit a frame, which its A/B put at +1.8 % of work (traced with the power rails, which lengthen the RenderThread's frames, so the shares differ), with drift between sessions on top; Compose's own RenderThread work moved by 0.05 % (6,046 → 6,049 ms). It still does 44 % less than Compose's.

**HWUI's two worker threads** still cost compose-rs about 26 % more CPU time than Compose in the animation (10.8 against 8.6 s, the medians of all three runs this round; 10.8 against 8.9 s in round 12's second run), as in the first results: reusing paths (D410) didn't change it. They rasterize the 50 stroked waves on the CPU into masks (Skia's software path renderer), the same work in both apps (5.9 against 6.1 s by the work metric), at lower clocks for compose-rs: HWUI's session holds them, and the mid cores' frequency follows Compose's heavier main thread and RenderThread (D511, "HWUI's workers and graphics memory").

### Round 13: memory

| | Kotlin Compose | compose-rs (round 12) |
|---|---|---|
| Stress list, PSS 5 s after start | 63.4 MB | **46.9 MB** (47.2 MB) |
| of which Java heap / native heap / code / graphics | 5.3 / 10.7 / 7.1 / 34.3 MB | 1.3 / 14.7 / 2.0 / 23.8 MB (1.3 / 14.6 / 2.4 / 23.8 MB) |
| Stress list, PSS after 10 flings | 112.1 MB | **91.2 MB** (91.4 MB) |
| of which Java heap / native heap / graphics | 12.7 / 22.2 / 63.3 MB | 4.7 / 17.2 / 62.1 MB (4.3 / 17.2 / 62.3 MB) |
| Stress animation, PSS 3 s after start | 165.7 MB | **144.1 MB** (184.7 MB) |
| of which Java heap / native heap / graphics | 12.9 / 20.0 / 119.7 MB | 4.2 / 17.9 / 114.9 MB (4.3 / 18.4 / 154.6 MB) |
| HWUI's GPU caches 3 s after start: total; scratch surfaces; scratch buffers (`dumpsys gfxinfo`) | 89.7; 62.7; 18.9 MB | 96.0; 60.0; 28.1 MB (not recorded) |
| Stress animation, PSS after 13 s | **209.7 MB** (runs: 202.4 to 251.4) | 213.2 MB (171.7 to 251.2) (222.4 MB) |
| of which Java heap / native heap / graphics | 39.4 / 26.8 / 146.1 MB | 8.5 / 25.3 / 173.7 MB (7.9 / 23.2 / 183.7 MB) |
| HWUI's GPU caches after 13 s, total | 91.8 MB | 96.5 MB |

**On the list**, compose-rs holds a quarter less than Compose after start and a fifth less after scrolling, as in rounds 11 and 12: D510 changes nothing here. Its native heap is 14.6 MB after start and 17.2 MB after scrolling, against 18.1 and 22.3 MB in round 10: D460 took it back to round 9's (17.1 and 20.8 MB, below), and placing only the items in view (D470) about 3 MB lower, as in its A/Bs (14.4 to 14.7 and 17.2 to 17.4 MB).

**Where round 10's extra 1.0 to 1.5 MB went** (D460, measured before D470). The list's first layout has measured only its header (81 px) when it estimates how many items to compose: 87 items, its rows and its carousels' tiles, of which 38 stay; the others leave in the next pass without ever being measured. Until D430 they were disposed. With D430 they went to the reuse pool, 7 per content type: 30 rows with their carousels' first tiles, 531 layout nodes, 3,563 groups and 1,462 slots, about 33 KB a row. The pool then stayed full: scrolling hands each slot leaving to an item coming in the same pass, so nothing else enters it; and the items in it, once reused and measured, kept their paragraphs (the Java heap after the flings was 0.4 MB larger too). Compose composes items as its measurement reaches them, so its pool holds only items that left. A slot whose item leaves unmeasured is now disposed, as before D430. A flag D430 added to every group, written and never read, had also grown `Group` from 144 to 152 bytes; it's gone (D461).

The Rust heap, headless (a counting allocator; the stress screen at 1008 × 2244 px and 2.25, a fixed text measurer; after start, and after 10 flings of 1,400 px):

| | before D430 (7cf5338) | round 10 (aaa2035) | D460, D461 |
|---|---|---|---|
| After start | 5,412.6 KiB, 357 nodes, 2,474 groups | 6,405.2 KiB, 888 nodes, 6,097 groups | 5,428.0 KiB, 360 nodes, 2,497 groups |
| After 10 flings | 5,886.5 KiB, 516 nodes, 3,585 groups | 7,008.4 KiB, 1,080 nodes, 7,441 groups | 5,791.6 KiB, 516 nodes, 3,585 groups |

What's left of reuse's cost after start, 15 KiB: one more group per composed item (its slot's reusable group), and the one item the first layout measured and then dropped (the second section's header), kept. After the flings the pool is empty: every item that left was taken by one coming in, in the same pass. On the device, main (aaa2035) against D460 and D461, 6 runs each in two alternating installs, the stress list's `stress_scroll` memory steps (medians, MB):

| | main | D460, D461 |
|---|---|---|
| Native heap after start | 18.1 | 17.1 |
| Native heap after 10 flings | 22.3 | 20.4 |
| Java heap after 10 flings | 5.0 | 4.6 |
| PSS after start / after 10 flings | 50.7 / 97.1 | 49.7 / 94.8 |

No range overlaps; the native heap is back to round 9's (17.1 and 20.8 MB).

**In the animation, 3 s after start, D510 holds**: compose-rs's graphics memory went from 154.6 MB in round 12 to 114.9 MB (runs: 114.9, 114.9, 122.9), below Compose's 119.7 MB (119.4 to 127.6), and its PSS from 184.7 to 144.1 MB, 22 MB below Compose's 165.7 MB. Its A/B, traced with the power rails, had measured 150.6 → 122.9 MB of graphics. Skia's own caches hold about as much in both apps (96.0 against 89.7 MB; scratch surfaces 60.0 against 62.7 MB). The 9 MB more scratch buffers the A/B left unexplained (28.1 against 18.8 MB) is a level either app lands on: 18.8 or 28.1 MB, run by run, in both (compose-rs's runs 18.8, 28.1, 28.1 MB; Compose's 28.0, 18.8, 18.9 MB).

**After 13 s, both apps' graphics memory grows by an amount that varies from run to run**, more than the medians can separate: compose-rs's 131.7 to 208.7 MB (median 173.7), Compose's 132.2 to 153.0 MB (146.1), so PSS was 213.2 against Compose's 209.7 MB at the median (171.7 to 251.2 against 202.4 to 251.4 MB). Compose's swings with its garbage collector too (its Java heap 8.3 to 65.1 MB, ours 7.4 to 8.6 MB). Not all of the growth is in Skia's caches: compose-rs's run at 208.7 MB held 96.3 MB there, as at 3 s, while in others Skia's caches grew by up to 37 MB of scratch buffers. An untraced probe after the round (4 runs each, the same two points; the round's JSON, `stress_animation_memory_probe`) put it in the GPU driver's memory (`GL mtrack`; the buffers' `EGL mtrack` stays at 44.1 MB in both apps), and found both apps alike: graphics 114.7 against 119.6 MB at 3 s and 137.7 against 136.8 MB after 13 s (compose-rs's runs 123.0 to 208.7, Compose's 128.7 to 182.4 MB), PSS 144.1 against 166.7 MB and 177.0 against 202.9 MB. D510's A/B had compose-rs at 123.0 MB after 13 s (8 runs, traced with the power rails). What the driver allocates in some runs hasn't been traced.

In round 12, compose-rs's 222.4 MB after 13 s was below Compose's 275.3 MB, as in round 11 (223.3 against 238.2 MB; in round 10 the medians were the other way, 210.1 against 195.8 MB): Compose's memory swings with its garbage collector (239.0 to 287.5 MB in round 12's runs, its Java heap 75.1 MB at the median). 3 s after start compose-rs held 17 MB more than Compose then (184.7 against 167.3 MB), its graphics memory 35 MB more (154.6 against 119.4 MB): a frame's worth more of the masks of the 50 waves, which Skia's GPU cache freed only at the end of the frame's one flush (D510, "HWUI's workers and graphics memory").

### Round 13: startup

| | Kotlin Compose | compose-rs (round 12) |
|---|---|---|
| `stress` (`TotalTime`, 15 cold starts) | 147 ms (135 to 157) | **119 ms** (111 to 136; −19 %) (118 ms) |
| The first frame's main thread (`traversal`, one traced start) | 58.5 ms | **35.1 ms** (36.7 ms) |
| `bindApplication` | 17.5 ms | **12.9 ms** (12.0 ms) |

compose-rs starts as in round 12: nothing since touches its start (our session opens at the second frame shown; the list drops no path). Its first frame's own part (`compose-rs: frame`) took 7.3 ms (round 12: 7.2 ms; before D472, 13.8 ms), 5.4 ms of it in 3 rounds of composing and laying out again, and laid out 29 paragraphs (4.1 ms). Its replay took 3.0 ms (it makes the bitmaps) and `Record View#draw()` 4.6 ms. Compose's first frame took 58.5 ms in this run's traced start (57.8 ms in round 12's, 64.3 ms in the first results': single traces).

## First results (921db6b)

These were the current results before round 9, kept as history: compose-rs at 921db6b with D400–D402, before D390, D410 and D420. The run is `results/2026-10-23-pixel-8-pro-stress.json`: `speed`, `--runs 15`, the stress scenarios only. The RenderThread boosts below are HWUI's session's alone.

### Stress scroll: frames (10 flings, 3 runs) (first results)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (gfxinfo) | 1,792 | 1,793 |
| Janky frames (FrameTimeline), per run | 0, 2, 1, all the app's | 1, 0, 0, the app's |
| Frames presented at another vsync than expected, per run | 0, 53, 43 | **3, 0, 0** |
| `doFrame`s starting more than 2 ms late, per run (of which after main-thread work) | 21 (6), 34 (8), 29 (9) | **13 (2), 10 (0), 9 (0)** |
| Frame time p50 / p90 / p99 (gfxinfo) | **5** / 8 / 12 ms | 6 / **7** / **9** ms |
| Main thread per frame p50 / p90 / p99 | **1.31** / 3.18 / 8.00 ms (max 12.7) | 1.41 / **2.05** / **3.24** ms (max 5.6) |
| RenderThread per frame p50 / p90 / p99 | **2.60** / **3.60** / **5.14** ms | 3.04 / 4.62 / 6.60 ms |

**Main thread.**
- Compose's median main-thread frame is a little shorter. Its slowest are 2.5 times as long as compose-rs's.
- Its three janky frames were all the app's. Each composed and measured a newly revealed item inside the frame ("Where each app struggles").
- compose-rs prefetched 356 to 370 times a run: it composes, measures and records the next item between frames (D156, D322, D350), carousels included.

**compose-rs's one janky frame.** It was a 21.4 ms `DrawFrame` in its first run. 12.5 ms of it went to `CreateGraphicsPipeline` for a `CircularRRectOp`: Skia compiling a GPU pipeline the first time it drew that shape. HWUI stores compiled pipelines (`ShaderCache::store`), and no later run had one.

**RenderThread.** compose-rs's is slower at every percentile, for less work (below). As on the feed ([12](12-android-performance.md), "the RenderThread's boost"), that is the boost:

| HWUI boost (medians) | Kotlin Compose | compose-rs |
|---|---|---|
| Boost minimum, time-weighted (out of 1024) | 283 | 114 |
| Frames unboosted / at 100–199 / at 200 or more | 5 % / 19 % / 75 % | 49 % / 30 % / 25 % |
| `DrawFrame` p50: unboosted / 100–199 / 200 or more | 3.35 / 3.08 / 2.52 ms | 3.45 / 3.05 / 2.37 ms |
| RenderThread running time on the little cores | 4 % | 14 % |

At the same boost, the two RenderThreads' frames take the same time. Compose's long main-thread frames earn it a boost three quarters of the time; compose-rs's half its frames run unboosted.

### Stress animation: frames (10 s, 3 runs) (first results)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Frames (FrameTimeline) | 1,198 | 1,200 |
| Janky frames (FrameTimeline), per run | 1, 0, 20, all the app's | **0, 0, 0** |
| Frames presented at another vsync than expected, per run | 925, 1,202, 1,164 | 1,199, 0, 1,206 |
| `doFrame`s starting more than 2 ms late, per run | 1, 7, 237 | 1, 2, 17 |
| Frame time p50 / p90 / p99 (gfxinfo) | 15 / 17 / 19 ms | 15 / 17 / 19 ms |
| Main thread per frame p50 / p90 / p99 | 3.68 / 5.07 / 7.18 ms (max 12.2) | **3.20** / **3.98** / **5.24** ms (max 8.6) |
| RenderThread per frame p50 / p90 / p99 | 7.52 / 8.84 / 10.49 ms | **6.16** / **7.20** / **8.60** ms |
| HWUI boost minimum, time-weighted; frames unboosted | 704; 0 % | 450; 0 % |

**Both apps run near their limit.** Both are boosted on every frame. Both RenderThreads take most of a 120 Hz vsync (8.33 ms) at p50, and more at p90 (Compose) or p99 (compose-rs).

**Compose's third run.**
- Its main thread started 237 `doFrame`s late (none after its own work: the scheduler woke it late) and missed 20 deadlines.
- Its main thread's p50 was 5.14 ms, against 3.59 and 3.68 ms in the other runs.
- Nothing in the trace explains it beyond more time waiting to run (24 against 5 ms runnable). It is the median run's jank that counts: 1 against 0.

#### With `ModulateAlpha` (D402)

A second session (`results/2026-10-23-pixel-8-pro-stress-modulate-alpha.json`, `stress_animation` only) measured both apps again. Kotlin's cells were given `compositingStrategy = CompositingStrategy.ModulateAlpha`, Compose's opt-in for fading a layer's drawings one by one without an offscreen buffer. compose-rs is unchanged.

| | Compose, `Auto` (first session) | Compose, `ModulateAlpha` | compose-rs (second session) |
|---|---|---|---|
| Janky frames, per run | 1, 0, 20 | 1, 1, 0 | 1, 1, 1 |
| Main thread p50 / p90 / p99 | 3.68 / 5.07 / 7.18 ms | 3.89 / 5.04 / 6.65 ms | 3.26 / 4.04 / 5.84 ms |
| RenderThread p50 / p90 / p99 | 7.52 / 8.84 / 10.49 ms | 6.22 / 7.03 / 8.21 ms | 6.36 / 7.42 / 9.28 ms |
| Main thread, RenderThread work | 2,897, 6,048 ms | 2,795, 3,286 ms | 2,134, 4,178 ms |

**With `ModulateAlpha`:**
- Compose's RenderThread does 46 % less work, a fifth less than compose-rs's, and its frames match compose-rs's.
- Its main thread keeps doing a third more work than compose-rs's.

**compose-rs's janky frames in this session** (one a run) were RenderThread frames of 12 to 16 ms, of which 9 to 14 ms was `Vulkan finish frame`: waiting for the GPU. This happened in the first session too, without missing a deadline (slowest `DrawFrame`s 11.2 to 15.4 ms).

So the stress animation janks rarely in either app, and which one depends on the session.

### Work (first results)

| Stress scroll (10 flings) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 1,230 ms | **576 ms** (−53 %) |
| RenderThread work | 1,656 ms | **1,379 ms** (−17 %) |
| Main thread CPU time | 3.89 s | **3.29 s** |
| RenderThread CPU time | **4.90 s** | 5.82 s |

| Stress animation (10 s) | Kotlin Compose | compose-rs |
|---|---|---|
| Main thread work | 2,897 ms | **2,163 ms** (−25 %) |
| RenderThread work | 6,048 ms | **4,399 ms** (−27 %; Compose with `ModulateAlpha`: 3,286 ms) |
| Main thread CPU time | 4.27 s | **3.90 s** |
| RenderThread CPU time | 7.16 s | **5.66 s** |
| HWUI's worker threads (`hwuiTask0`, `hwuiTask1`), CPU time (the second run) | **8.7 s** | 10.9 s |

**Work** is milliseconds on the fastest core at its highest frequency ([12](12-android-performance.md), "Method"). compose-rs's main thread does about half Compose's work scrolling and three quarters in the animation.

**HWUI's two worker threads** run in the animation (scrolling, they hardly run), where both apps keep them busy most of the time. They rasterize the 50 stroked waves on the CPU into masks (Skia's software path renderer). That work is the canvas's, and it costs compose-rs a quarter more: its masks go into new GPU images each frame rather than recycled ones (next section; since D410, they're freed a frame later).

### Memory (first results)

| | Kotlin Compose | compose-rs |
|---|---|---|
| Stress list, PSS 5 s after start | 63.6 MB | **49.7 MB** |
| of which Java heap / native heap / code / graphics | 5.3 / 10.8 / 7.3 / 34.3 MB | 1.3 / 17.0 / 2.3 / 23.8 MB |
| Stress list, PSS after 10 flings | 110.8 MB | **97.9 MB** |
| of which Java heap / native heap / graphics | 12.1 / 21.8 / 63.3 MB | 6.5 / 21.0 / 62.4 MB |
| Stress animation, PSS 3 s after start | **166.9 MB** | 289.4 MB |
| of which Java heap / native heap / graphics | 12.3 / 21.8 / 119.4 MB | 7.8 / 59.6 / 210.8 MB |
| Stress animation, PSS after 13 s | **251.0 MB** | 405.4 MB |
| of which Java heap / native heap / graphics | 30.0 / 27.0 / 132.2 MB | 24.9 / 143.6 / 218.9 MB |

**On the list**, compose-rs holds less after start (less code, less Java heap, less graphics memory) and slightly less after scrolling. Both apps' graphics memory roughly doubles over the flings: the render thread's caches, and the 8 bitmaps' textures.

**In the animation, compose-rs holds 60 % more.** Over a minute of it (sampled at 5, 20, 40 and 60 s, outside measure.py) it moved between 320 and 450 MB: it doesn't leak, but it waits for the garbage collector. Compose stayed between 184 and 224 MB.

**Where it goes** (`dumpsys gfxinfo` after the minute):

- **GPU memory.** HWUI's GPU caches held 146.6 MB for compose-rs, of which 82.6 MB were Skia's software path masks (518 entries), against 103.7 MB and 8.0 MB (50 entries) for Compose.
  - A cached mask lives as long as its path.
  - Kotlin's one reset `Path` frees each wave's mask when it changes.
  - compose-rs's replayer makes a new `android.graphics.Path` for each path each frame, and drops it when the display list does. Its native path, and the mask cached for it, live until the collector finalizes the Kotlin object.
- **Native heap.** The 60 to 144 MB are those paths' native memory, waiting for the same collection. A `Path`'s native size isn't counted against the Java heap, so the collector runs late.
- **New GPU images.** Since the dead paths' masks keep their textures, Skia allocates 50 new GPU images a frame (`allocateImageMemory`: 50 a frame, 0.19 ms) where Compose re-uploads into textures it recycles.

The fix belonged to the Android bridge, and is the next section.

### With paths reused (D410, D411)

The replayer now:

- rewinds a dropped path's `android.graphics.Path` at once, and keeps it (64 at most) for the next definition, which rebuilds it, as Compose's one reset `Path`;
- reads a frame's drops before its definitions (Rust writes them first), so the 50 waves reuse the last frame's 50 `Path`s in the same frame.

A recording keeps its own copy of a path's contents, so rewinding changes nothing drawn. HWUI keys its cached mask by those contents' generation: it's freed when the last recording drawing them is, a frame later, not at a garbage collection.

D190's masks, for small filled paths (the waves are stroked: none is made for them), aren't made for paths that are new every frame either (D411).

Keeping a wave's path identity across frames instead, a resource id per draw site redefined in place, was not done (D412). It would save only the 5-byte drop, and a group's key and version would no longer name one drawing.

The stress animation at 0e0c86c with D410 and D411, against 0e0c86c, in one session (`dumpsys meminfo` every 5 s for 60 s, each app started alone):

| PSS over 60 s | Kotlin Compose | compose-rs before | compose-rs after |
|---|---|---|---|
| Total, range | 189–241 MB | 322–334 MB | **186–230 MB** (228 MB from 15 s; in a later session, 191–195 MB) |
| Java heap / native heap / graphics at 60 s | 35.0 / 26.7 / 165.7 MB | 14.1 / 87.8 / 224.8 MB | 6.4 / 22.0 / 192.0 MB |
| HWUI's software path masks at 60 s (`gfxinfo`) | 50 entries, 8.0 MB | 519 entries, 82.9 MB | 50 entries, 8.0 MB |
| Total GPU memory at 60 s | 103.1 MB | 146.6 MB | 125.7 MB |

compose-rs's memory no longer waits on the collector: it is flat after 15 s, where Compose's moves with its Java heap. Its level depends on how much scratch memory HWUI kept: 192 MB of graphics in this session, 158 MB in the later one. It holds about 26 MB more graphics memory than Compose at 60 s, mostly Skia's scratch surfaces and buffers (118 against 95 MB), which both apps' HWUI keeps for reuse.

measure.py's `stress_animation` (3 runs a side; compose-rs before and after in two runs of it, back to back):

| | compose-rs before | compose-rs after |
|---|---|---|
| PSS 3 s after start / after 13 s | 293.1 / 397.2 MB | **181.9 / 201.8 MB** |
| Janky frames (FrameTimeline), per run | 2, 1, 1 | **0, 1, 0** |
| Main thread per frame p50 / p90 / p99 | 3.93 / 5.11 / 7.21 ms | **3.62 / 4.67 / 6.12** ms |
| RenderThread per frame p50 / p90 / p99 | 7.64 / 8.50 / 9.76 ms | 7.55 / 8.55 / 9.46 ms |
| Main thread / RenderThread CPU time | 4.29 / 7.52 s | **3.73 / 6.28 s** |
| Main thread / RenderThread work | 2,115 / 3,909 ms | **1,933 / 3,200 ms** (−9 %, −18 %) |

Compose's runs in that session were slow: 233 to 330 janky frames a run, both threads' p50 above 9.5 ms, unlike its runs above. They aren't compared here. Its memory was 167–169 MB after start and 215–240 MB after 13 s.

The other scenarios didn't change: compose-rs before and after, in another session (measure.py's `scroll`, `animation` and `stress_scroll`, 3 runs a side each):

| compose-rs, before → after | Main thread p50 / p90 / p99 | RenderThread p50 / p90 / p99 | Janky frames | PSS after scrolling |
|---|---|---|---|---|
| Feed, 10 flings | 1.30 / 1.87 / 2.90 → 1.30 / 1.92 / 2.86 ms | 2.76 / 3.69 / 5.19 → 2.79 / 3.75 / 5.30 ms | 0 → 0 | 80.8 → 79.9 MB |
| Feed, animated header | 1.22 / 1.83 / 3.23 → 1.21 / 1.79 / 2.93 ms | 2.65 / 3.60 / 5.02 → 2.68 / 3.56 / 5.27 ms | 0 → 0 | |
| Stress list, 10 flings | 1.54 / 2.23 / 3.46 → 1.51 / 2.17 / 3.31 ms | 3.27 / 4.93 / 7.06 → 3.33 / 4.88 / 6.69 ms | 0 → 1 (one run) | 93.4 → 96.9 MB |

### Startup (first results)

| | Kotlin Compose | compose-rs |
|---|---|---|
| `stress` (`TotalTime`, 15 cold starts) | 143 ms (134 to 155) | **126 ms** (117 to 138; −12 %) |
| The first frame's main thread (`traversal`, one traced start) | 64.3 ms | **44.4 ms** |
| `bindApplication` | 19.2 ms | **12.3 ms** |

**Compose's first frame:**
- `Compose:initializeView` 8.5 ms;
- a recomposition, 3.2 ms;
- a measure of 20.8 ms, which composes and measures the visible items;
- the window's relayout, 4.5 ms;
- drawing, 8.2 ms.

The rest is untraced: class loading and the interpreter.

**compose-rs's first frame:**
- 14.1 ms of its own (`compose-rs: frame`), most of it two rounds of composing and laying out again (10.8 and 1.7 ms). The outer list composes its items, then each carousel composes its tiles: compose-rs has no subcomposition ([11](11-status.md)).
- 3.9 ms of replay, which makes the bitmaps;
- 5.4 ms of `Record View#draw()`;
- 6.2 ms waiting for the RenderThread's first frame (`postAndWait`);
- 4.7 ms of window relayout.

## Where each app struggles

From the traces of the first results (921db6b), before D390, D410 and D420; their effects are in "Round 13 (history)" above. The slowest 1 % of each app's frames on each thread, pooled over the 3 runs, with the slices inside them averaged per frame (`slowest_frames` gives the slowest 5 of each run).

**Compose, scrolling, main thread** (54 frames of 8.0 ms or more, 9.4 ms on average):
- `AndroidOwner:measureAndLayout` takes 4.5 ms a frame: Compose composes and measures a newly revealed item inside the layout pass, nested `LazyRow` included (`TextStringSimpleNode::measure` 1.1 ms, `Compose:recompose` 0.9 ms).
- `Record View#draw()` takes 3.4 ms.
- `Recomposer:animation` (the fling's frame) takes 3.5 ms.
- These are the frames that miss: Compose prefetches between frames too, but an item it hasn't prefetched in time is composed in the frame that reveals it.

**compose-rs, scrolling, main thread** (54 frames of 3.2 ms or more, 4.0 ms on average):
- Its own frame takes 0.5 ms of that.
- The rest is waiting for the RenderThread to take the frame (`postAndWait`, 2.2 ms), input (0.4 ms) and the view system.
- Its slowest main-thread frames are the RenderThread's.

**Both, scrolling, RenderThread** (Compose 6.5 ms on average, compose-rs 7.6 ms):
- Most of each is `flush commands` (2.9 and 3.4 ms): Skia submitting to and waiting for the GPU, and `queueBuffer` (1.9 ms).
- compose-rs's slowest are mostly unboosted frames (above). Its single 21 ms frame compiled a pipeline.

**The animation, main thread, all frames** (per frame):
- **Compose** runs its 147 transitions' frame loops in `Recomposer:animation` (1.19 ms). It measures and lays out (0.86 ms: the ticker and the percentages' text, 3.75 paragraphs a frame, 0.6 ms) and records (1.63 ms).
- **compose-rs** runs its 568 tasks (`compose-rs: tasks`, 1.12 ms: a task per animated value, and the ticker's), then lays out (0.40 ms, of which text 0.32 ms), draws (0.31 ms) and replays (0.68 ms). That comes to 2.03 ms for its whole frame, plus `Record View#draw()`.
- **Their slowest 1 %** (Compose 8.9 ms on average, compose-rs 7.0 ms) mostly wait for the RenderThread (`postAndWait`: 5.6 and 2.6 ms). compose-rs's also have 1.7 ms of tasks.

**The animation, RenderThread, all frames** (per frame):
- **Compose:**
  - Its `Drawing` takes 7.3 ms, of which `flush layers` takes 1.4 ms: 132 `alpha caused saveLayer` a frame, and 73 render passes (`OpsTask`s) a frame instead of one. Each cell faded below 1 is drawn offscreen and then composited.
  - This is the 46 % of RenderThread work that `ModulateAlpha` saves.
- **compose-rs:**
  - Its `Drawing` takes 6.0 ms, in one render pass.
  - 4.8 ms of it is `flush commands`, waiting for the GPU (Compose: 4.0 ms), which the 50 new mask images a frame add to.
- **Both apps' slowest RenderThread frames** (Compose 11.6 ms, compose-rs 9.8 ms on average) are those GPU waits: `Vulkan finish frame`, 4.3 and 7.4 ms.

**In short:**
- **Compose struggles on the main thread when content arrives.** Its slowest scroll frames compose and measure new items, and its animation frames are dearer per animated value.
- **compose-rs struggles where it hands drawing to Android:**
  - its paths, new every frame, cost memory, mask rasterization and GPU allocations (until D410: "With paths reused");
  - its RenderThread ran unboosted because its main thread is short (until D390: in round 9 the RenderThread was boosted almost every frame, and the main thread ran on the little cores instead, until D440 put it in the same session);
  - like Compose, it waits for the GPU on the heaviest frames.

## Infinite transitions, one task each (D420–D424)

compose-rs's infinite transitions now work as Compose's: one task per transition sets all its values each frame (D420). The executor's cost per task resumed by a frame went down too (D421):
- every task waiting for a frame used to wake the host (on Android a JNI upcall each);
- two scans of the awaiter list per task made resuming them quadratic.

Animated colors no longer convert to Oklab to compare (D422). `update_transition`'s values keep a task each (D423).

**Headless** (`benchmarks/benches/animation.rs`, D424; an M-series Mac, frames 16 ms apart on a phone-sized host, p50):

| | Before | After |
|---|---|---|
| Resuming 568 tasks waiting for a frame (568 closure calls: 0.43 µs) | 169 µs | 21 µs |
| 140 infinite transitions of 4 values, nothing reading them | 211 µs | 44 µs |
| The same, drawn as the stress cells | 250 µs | 76 µs |
| `stress_animation` (fixed text measurer) | 284 µs | 107 µs |

Most of the 140 transitions' remaining time is the values' own math: the easing curves (a cubic Bézier solved per value per frame) and the colors' conversions from Oklab.

**On the Pixel 8 Pro**: the same session, `speed`, 3 runs a side, before (0e0c86c) then after, with the same Kotlin APK measured beside each. The runs are `results/2026-10-23-pixel-8-pro-infinite-transitions-{before,after}.json`.

| `stress_animation`, compose-rs (medians of 3 runs) | Before | After |
|---|---|---|
| Main thread per frame p50 / p90 / p99 | 3.18 / 3.82 / 5.08 ms | **2.53** / **3.37** / 5.44 ms |
| Main thread work | 2,139 ms | **1,550 ms** (−28 %) |
| Main thread CPU | 3,845 ms | **2,946 ms** |
| `compose-rs: tasks` per frame, per run | 1.13, 1.13, 1.13 ms | **0.36, 0.41, 0.42 ms** |
| `compose-rs: frame` per frame, per run | 2.03, 2.02, 2.06 ms | **1.25, 1.37, 1.36 ms** |
| Janky frames (FrameTimeline), per run | 0, 0, 1 | 0, 0, 1 |
| RenderThread per frame p50 / p90 / p99 | 6.11 / 7.04 / 8.20 ms | 7.14 / 8.18 / 9.37 ms |
| RenderThread work | 4,221 ms | 3,850 ms |

- The p99 moved within the runs' spread (before 5.08, 4.70, 5.74 ms; after 3.97, 6.17, 5.44 ms). These frames wait for the RenderThread (`postAndWait`), not for tasks.
- The RenderThread did less work but took longer. The device was slower in the second half of the session: Kotlin's RenderThread p50 went from 7.40 to 7.80 ms with the same APK, and its third run had 292 janky frames against 1. Nothing in this change touches the RenderThread's input; the drawing is the same.
- **The feed's animation** (`animation`, docs/12) didn't move: compose-rs's main thread p50 / p90 / p99 1.21 / 1.79 / 2.91 ms before and 1.21 / 1.80 / 3.15 ms after, with 187 ms of work both times and no janky frames. Its header has three animated values.
- **In round 9** (with D390 as well, which neither build here had), compose-rs's main thread p50 / p90 / p99 was 2.36 / 2.97 / 3.99 ms for 1,444 ms of work, `compose-rs: tasks` 0.36 ms a frame, and the RenderThread's p50 6.34 ms: the gains held, with a p99 below both builds' here. Round 10 measured 2.24 / 2.82 / 4.12 ms, 1,440 ms of work and 0.35 ms of tasks a frame; round 11 2.23 / 2.77 / 3.71 ms and 1,437 ms of work; round 12 ("Round 13 (history)" above) 2.25 / 2.81 / 3.80 ms and 1,462 ms.

## The main thread in the hint session (D440)

Round 9 left compose-rs's main thread on the little cores on this list: with D390 boosting the RenderThread, the RenderThread's part of each frame got shorter, HWUI's session (which reports the whole frame and boosts both threads) hardly boosted any more, and the main thread ran 83 % of its time on the little cores, for a p50 of 1.67 ms against 1.41 ms before D390. Two findings ([14](14-performance-opportunities.md), "The main thread in the session"):
- **Why the vote sits near 320 here, and near 100 on the feed.** Under the target, the power HAL's controller is a random walk. About every 83 ms it takes about 23 off the vote, whatever the vote, and adds 7.7 for each 100 µs by which the last report of the batch exceeds the one before; only rises count. This list's frames vary more from one to the next than the feed's. The kicks average 26 to 29 a batch here against 20 to 21 there, so the vote drifts up here and down there. HWUI's session works the same way, which is why Compose's main thread is boosted on this list too (its vote about 290).
- **The fix: the main thread joins compose-rs's session.** It gets the same vote as the RenderThread; the reports don't change.

Interleaved on the device, 12 runs each against main (18272a3), the variants rotating each round (medians of the runs' percentiles; power: means over the measured 13 s and 9 s, the difference ± its 95 % interval):

| | Stress scroll: main | with the main thread (D440) | Stress animation: main | with the main thread (D440) |
|---|---|---|---|---|
| Main thread per frame p50 / p90 / p99 | 1.54 / 2.24 / 3.53 ms | **1.07** / **1.71** / **2.97** ms | 2.36 / 3.00 / 4.10 ms | 2.26 / 2.83 / 3.80 ms |
| RenderThread per frame p50 / p90 / p99 | 2.39 / 3.46 / 5.01 ms | 2.49 / 3.51 / 4.83 ms | 6.36 / 7.43 / 8.50 ms | 6.38 / 7.43 / 8.43 ms |
| Janky frames, the app's / the compositor's (12 runs) | 0 / 0 | 0 / 2 (one run) | 1 / 0 | 1 / 0 |
| `doFrame`s delayed by main-thread work (on the list, prefetch steps; 12 runs) | 58 (1 to 10 a run) | **4** (0 or 1) | 1 | 0 |
| Main thread on the little cores | 79 % | 28 % | 0 % | 0 % |
| Main thread CPU time (work) | 3,529 (498) ms | 2,368 (610) ms | 2,515 (1,295) ms | 2,409 (1,318) ms |
| Power: CPU rails | 474 ± 14 mW | −0.7 % ± 2.2 | 1,837 ± 61 mW | −0.8 % ± 2.8 |
| Power: all rails | 1,813 ± 35 mW | −0.6 % ± 1.6 | 3,663 ± 87 mW | −0.4 % ± 1.9 |

- **The list's main thread.** Its median is now shorter than Compose's in round 9 (1.07 against 1.26 ms). The runs' p50s range over 1.02 to 1.23 ms against main's 1.51 to 1.61, and their p99s over 2.69 to 3.05 against 3.16 to 3.90 ms.
- **Prefetch steps.** Almost no frame waits for one any more.
- **Power.** No change was resolved.
- **The animation.** It moves little: both sessions already vote about 430 to 465 there, and both threads run on the mid and big cores.
- **The compositor.** One run had 2 compositor-side janky frames and presented 72 frames a vsync late, on cadence. In all, 4 of the 48 runs with the main thread had such frames (1 to 3), against none of main's 48: too few to tell from noise.
- **Not chosen.** Reporting both threads' CPU time against 8.33 ms gave shorter frames still: list main thread p50 0.94 ms, RenderThread 2.09 ms. It cost +2.6 % (± 2.3) CPU-rail power here and +4.0 % (± 2.1) on the feed (D442).
- **Beside Compose (round 10).** The list's main thread p50 / p90 / p99 was 1.02 / 1.63 / 2.86 ms against Compose's 1.29 / 3.32 / 7.76 ms, 21 % of its time on the little cores. No `doFrame` waited for a prefetch step, and no run of compose-rs's had a compositor-side janky frame, on any screen.
- **Round 11** (round 11 of [12](12-android-performance.md)). With only the items in view placed (D470), the RenderThread has less work, its reports vary less and the vote is lower (190 against 295). The main thread shares it, and ran 46 % of its time on the little cores, for a p50 / p90 / p99 of 1.08 / 1.62 / 2.75 ms against Compose's 1.25 / 3.33 / 7.61 ms; 2 `doFrame`s waited for a prefetch step in 3 runs. Again no compositor-side janky frame of compose-rs's, on any screen.
- **Round 12 ("Round 13 (history)" above).** With both threads' work reported against 8.33 ms (D500), the vote is 369 and the main thread runs 4 % of its time on the little cores, for a p50 / p90 / p99 of 0.81 / 1.23 / 2.52 ms against Compose's 1.26 / 3.20 / 8.05 ms; no `doFrame` waited for a prefetch step. One run had 3 compositor-side janky frames, in Choreographer's buffer stuffing recovery from its warm-up on.

## The lazy list's policy (D470–D474)

Until D470 a lazy list composed and placed a viewport's worth of items beyond each side of the viewport: about 7 items on this list (and their carousels' tiles), 5 on the feed. Each placed item is a render node the main thread replays and the RenderThread syncs every frame. Its first layout also estimated the items it lacked from the only one it had measured, this list's 81 px header, and composed 87 items (rows and tiles) of which 38 stayed (D460). The changes:

- **D470**: a list composes and places only the items in view, as Compose's; a focus search leaving it asks for the next items one at a time (D477, after a review: D470 first kept a viewport's worth beyond each side while an item held the focus). Prefetch runs from the first layout on, not only after a scroll, so a fling's first frames find the next two items composed. A frame still keeps the two composed items next to those it wants, for a reversal.
- **D472**: while a list knows fewer than 4 item sizes, a measurement lacking items asks for 4 at most (or as many as it placed), and the frame's next round asks for the rest.
- **D471, D474**: two fixes, each of which kept prefetch from running in the idle time after some frames. The list read `layout_info` while measuring, which measured it twice and asked for another frame whenever the average size changed; and a frame that composed in a round asked for another frame. A frame with work pending skips prefetch, so the next frame had to compose its items itself.

**What a viewport's worth bought.** (Results: `results/2026-10-23-pixel-8-pro-lazy-policy.json`.) First, an interleaved A/B on the Pixel 8 Pro, 12 runs each of the feed's and this list's scroll (10 flings), of four variants: main (727d266), and with D471 and D472 a viewport's worth, two items beyond (placed), and none. In-frame composition was the same in all four: no text laid out inside a frame on the feed, and on this list one in 12 runs with two beyond, none otherwise. This list had 10 rounds a run (`recompose and layout again`) in all four, which laid out no text: most likely the sticky header changing (a state the items' composition reads, written as a section's first item scrolls past). Placing fewer items cut the work of both threads (medians of the runs):

| Stress scroll | main | a viewport's worth | two beyond | none (shipped) |
|---|---|---|---|---|
| Main thread work | 677 ms | 678 ms | 639 ms | **589 ms** |
| RenderThread work | 1,543 ms | 1,551 ms | 1,451 ms | **1,345 ms** |
| Main thread p50 / p90 / p99 | 1.05 / 1.66 / 2.93 ms | 1.04 / 1.63 / 2.86 ms | 0.99 / 1.60 / 2.79 ms | 1.01 / 1.55 / 2.76 ms |
| RenderThread p50 / p90 / p99 | 2.56 / 3.59 / 4.97 ms | 2.48 / 3.54 / 5.11 ms | 2.46 / 3.48 / 5.00 ms | 2.46 / 3.51 / 5.04 ms |
| Native heap after the flings | 20.5 MB | 19.7 MB | 18.1 MB | **17.3 MB** |

On the feed the same order held: RenderThread work 829, 811, 789 and 747 ms. There, with none beyond, the RenderThread's p50 was 2.42 ms against main's 2.16 ms in this session, for 10 % less work at lower clocks: its CPU time didn't change (4.39 and 4.51 s), and 21 % of its time was on the little cores against 17 %. Its p90 and p99 were shorter (3.21 / 4.72 against 3.29 / 4.85 ms). The confirming A/B below didn't show the p50 difference (2.33 → 2.24 ms). So a viewport's worth saved no composition in these flings, and none beyond shipped. A review then found what it had hidden, fixed by D475–D478: scrolls onto a large item out of view were cut at an estimated room, first frames under a large first item could be short, focusing a field composed a viewport's worth in a frame, and accessibility couldn't scroll a list ("After the review" below).

**The shipped code against main**: interleaved, 12 runs each, scroll and cold starts (medians of the runs; the better side in bold where the runs' ranges barely overlap or not at all):

| | Feed: main | shipped | Stress list: main | shipped |
|---|---|---|---|---|
| Janky frames, the app's / the compositor's (12 runs) | 0 / 0 | 0 / 0 | 0 / 2 | 0 / 2 |
| Frames presented a vsync late (12 runs) | 0 | 0 | 127 | 3 |
| Main thread p50 / p90 / p99 | 1.23 / 1.76 / 2.72 ms | **1.16** / **1.68** / 2.73 ms | 1.06 / 1.68 / 2.86 ms | 0.98 / **1.53** / 2.70 ms |
| RenderThread p50 / p90 / p99 | 2.33 / 3.40 / 4.79 ms | 2.24 / 3.26 / 4.86 ms | 2.56 / 3.63 / 4.98 ms | 2.51 / 3.56 / 5.09 ms |
| Main thread work | 483 ms | 468 ms | 681 ms | **591 ms** |
| RenderThread work | 810 ms | **772 ms** | 1,556 ms | **1,333 ms** |
| Frames with an in-frame round; text laid out in frames (12 runs) | 0; 0 | 0; 0 | 120; 1 | 120; 1 |
| Prefetch steps composing, a run | 347 | 348 | 345 | 351 |
| Native heap after start / after the flings | 8.5 / 11.8 MB | **8.1** / **10.6** MB | 17.2 / 20.5 MB | **14.4** / **17.3** MB |
| PSS after start / after the flings | 34.3 / 79.2 MB | 33.9 / 77.9 MB | 49.8 / 95.6 MB | **47.0** / **91.4** MB |
| `TotalTime` (24 cold starts) | 108.5 ms | 104.0 ms | 125.5 ms | **118.5 ms** |
| The first frame's `compose-rs: frame` (12 traced starts) | 6.7 ms | **5.1 ms** | 14.5 ms | **7.2 ms** |

The big screen's start didn't change (162 ms). The RenderThread's p99s are 0.07 and 0.11 ms longer, within the runs' spread (4.54 to 5.47 against 4.61 to 5.05 ms on the feed, 4.52 to 5.81 against 4.45 to 5.86 ms here), and were 0.13 ms shorter on the feed and 0.07 ms longer here in the first A/B. Our hint session voted less for the RenderThread, which had less work (here a time-weighted 212 against 252).

The first frame now composes 16 items where it composed 87, and lays out 29 paragraphs where it laid out 69. Its rounds take 5.5 against 12.7 ms. Headless, the list's Rust heap peaks at 2,288 KiB at start, against 6,677 KiB (live: 2,268 against 5,434 KiB), and is 3,160 KiB after 10 flings, against 5,792 KiB.

**After the review (D475–D478).** A review of D470–D474 found three regressions, fixed since:
- **D476**: scrolls past an estimated end are no longer cut there. With only the items in view placed, a `scroll_by` onto a large item out of view, or a fling back to a large first item, stopped short.
- **D475**: first frames are full again. A large first item made the estimate run the 3 rounds out, short of the viewport.
- **D477**: a focus search asks the list for the next item beyond its bounds, as Compose's, instead of a viewport's worth composed while an item holds the focus. Focusing a field in a list composed 16 to 36 items in a frame.

It also found that TalkBack couldn't scroll a list. Lists, grids and scrolling containers now have scroll semantics, and Android exposes them as scrollable with scroll actions; nodes clipped away aren't visible to the user (D478).

Interleaved again against main (727d266), 8 runs each:

| | Feed: main | shipped | Stress list: main | shipped |
|---|---|---|---|---|
| Janky frames, the app's / the compositor's (8 runs) | 0 / 0 | 0 / 2 | 0 / 2 | 0 / 4 |
| Main thread p50 / p90 / p99 | 1.23 / 1.75 / 2.75 ms | **1.17** / **1.67** / 2.71 ms | 1.04 / 1.65 / 2.80 ms | 1.01 / 1.57 / 2.74 ms |
| RenderThread p50 / p90 / p99 | 2.31 / 3.38 / 5.08 ms | 2.35 / **3.23** / **4.72** ms | 2.49 / 3.61 / 5.01 ms | 2.51 / 3.60 / 5.31 ms |
| Main thread / RenderThread work | 490 / 814 ms | **459** / **755** ms | 680 / 1,562 ms | **574** / **1,316** ms |
| Text laid out in frames (8 runs) | 0 | 0 | 0 | 3 |
| Native heap after start / after the flings | 8.5 / 12.0 MB | **8.1** / **10.6** MB | 17.1 / 20.5 MB | **14.4** / **17.2** MB |
| `TotalTime` (16 cold starts) | 108.0 ms | 105.0 ms | 123.0 ms | 121.0 ms |
| The first frame's `compose-rs: frame` (8 traced starts) | 6.7 ms | **5.0 ms** | 14.3 ms | **7.3 ms** |

- **The wins hold:** less work on both threads, less memory, and a first frame half as long.
- **The stress list's RenderThread p99 is 0.30 ms longer.** The runs' ranges overlap (4.68 to 5.53 against 4.50 to 5.42 ms). The earlier A/Bs gave +0.07 and +0.11 ms.
- **It laid out text in 3 frames in 8 runs**, where main laid out none in 8 and 1 in 12 in the first A/B; too few to tell from noise.
- **The compositor's janky frames** (4 against 2) aren't the app's.
- **Startup:** `TotalTime` is within the starts' spread here (−2 and −3 ms).

**After the third review (D479–D485).** A third review found seven more bugs, fixed since:
- **Nested scrolling (D479).** Inside a nested-scroll parent (a `Scaffold`'s app bar connection, or a `LazyColumn` around a carousel), a delta was still cut at the estimated room. Now the list takes the whole delta, and what it couldn't use is offered to the parents once laid out, in the same frame.
- **Emptied lists (D480).** An emptied list took scroll.
- **Cards and dividers (D481).** With 1 px dividers between 300 px cards, the first frame composed 805 items to show 15.
- **Search leftovers (D482).** Rows laid out for a focus search stayed composed.
- **Nested focus searches (D483).** Searches didn't climb from a carousel's row to its list, and a spaced list's Down spun over 200 rows.
- **Accessibility directions (D484).** Accessibility scrolled reversed and right-to-left lists the wrong way.
- **Focus timing (D485).** The row a focus search found showed a frame late.

Interleaved against main (727d266), 8 runs each:

| | Feed: main | shipped | Stress list: main | shipped |
|---|---|---|---|---|
| Janky frames, the app's / the compositor's (8 runs) | 0 / 4 | 0 / 0 | 0 / 0 | 0 / 0 |
| Main thread p50 / p90 / p99 | 1.22 / 1.75 / 2.75 ms | **1.16** / **1.66** / 2.73 ms | 1.07 / 1.68 / 2.88 ms | **0.98** / **1.54** / 2.79 ms |
| RenderThread p50 / p90 / p99 | 2.20 / 3.34 / 4.97 ms | 2.32 / 3.21 / 4.66 ms | 2.62 / 3.68 / 5.02 ms | 2.40 / 3.57 / 5.34 ms |
| Main thread / RenderThread work | 497 / 829 ms | **462** / **759** ms | 689 / 1,552 ms | **595** / **1,348** ms |
| Text laid out in frames (8 runs) | 0 | 0 | 0 | 0 |
| Native heap after start / after the flings | 8.5 / 12.0 MB | **8.1** / **10.6** MB | 17.2 / 20.4 MB | **14.7** / **17.2** MB |
| `TotalTime` (16 cold starts) | 104.5 ms | 105.0 ms | 126.5 ms | **119.5 ms** |
| The first frame's `compose-rs: frame` (8 traced starts) | 6.7 ms | **5.0 ms** | 14.6 ms | **7.2 ms** |

- **Most numbers hold or improve.**
- **The stress list's RenderThread p99 is longer again:** 5.34 against 5.02 ms here, +0.30 ms in the run before. The ranges overlap (4.74 to 5.47 against 4.27 to 5.53 ms) but the shift recurs, so it may be real. The RenderThread does 13 % less work, and its p50 and p90 are shorter; with less work, our hint session votes less for it (D441).
- **Grid fast flings, again (4 runs each, interleaved):**
  - compose-rs composed items in 0 to 3 frames a run, with no janky frame and a main thread p99 of 2.9 to 3.0 ms.
  - Compose laid out new tiles' text in 72 to 118 frames a run, with a p99 of 5.6 to 6.8 ms and 2 app-janky frames.

**After the fourth review (D486–D491).** The fourth review found five bugs, fixed since:
- **A Tab that never ended (D486).** A Tab from a carousel ending in unfocusable tiles waited forever; every focus search now ends within about 25 frames.
- **Over-composition (D487).** A page down on cards, spacers and dividers composed 800 items in a frame. What a frame asks for now doubles round to round, bounded at about twice what it needs.
- **A phantom delta (D488).** An item inserted above during a drag sent one to the app bar.
- **Grids (D490).** A grid dropped what its estimated end left instead of passing it to its parents.
- **Right-to-left accessibility (D491).** Accessibility's forward and backward in right-to-left rows were the opposite of Compose's; the mapping is now Compose's, in Rust.

Interleaved against main (727d266), 8 runs each:
- One of main's feed traces held no frame, so its feed column has 7 runs.
- Neither app had a janky frame in any run.

| | Feed: main | shipped | Stress list: main | shipped |
|---|---|---|---|---|
| Main thread p50 / p90 / p99 | 1.24 / 1.75 / 2.74 ms | **1.17** / **1.67** / 2.64 ms | 1.06 / 1.66 / 2.79 ms | **0.96** / **1.53** / 2.70 ms |
| RenderThread p50 / p90 / p99 | 2.35 / 3.40 / 4.89 ms | 2.33 / 3.20 / 4.73 ms | 2.47 / 3.54 / 4.95 ms | 2.42 / 3.43 / 4.96 ms |
| Main thread / RenderThread work | 490 / 814 ms | **460** / **755** ms | 703 / 1,589 ms | **596** / **1,366** ms |
| Text laid out in frames (runs) | 0 | 0 | 1 | 0 |
| Native heap after start / after the flings | 8.5 / 11.9 MB | **8.1** / **10.5** MB | 17.2 / 20.6 MB | **14.7** / **17.4** MB |
| `TotalTime` (16 cold starts) | 106.5 ms | 105.5 ms | 123.5 ms | **116.5 ms** |
| The first frame's `compose-rs: frame` (8 traced starts) | 6.8 ms | **5.0 ms** | 14.5 ms | **7.0 ms** |

**The stress list's RenderThread p99** came out 0.3 ms longer in the two A/Bs before this one, and equal here (4.96 against 4.95 ms). It's the boost, not more work:
- **Faster at every boost level.** At the same boost, the shipped code's RenderThread frames are faster. In the A/B before this one, p99 was 6.45 against 6.82 ms unboosted, 4.91 against 5.75 ms at 100–199 and 3.94 against 4.33 ms at 200 or more; this one, 6.01 / 4.90 / 3.73 against 6.47 / 5.35 / 4.37 ms.
- **Less boosted.** With 13 to 14 % less RenderThread work, our hint session voted less: 230 against 254, then 226 against 268, time-weighted. Fewer frames started at 200 or more (58 against 69 %, then 60 against 73 %), and more ran unboosted.
- **Still drawn ahead.** Prefetched items are drawn and recorded ahead as before: about 213 draw-ahead steps a run against 210. The replay takes less time (100 against 144 ms a run).
- **Fewer render nodes.** The frame records no more render nodes, it places fewer.

**Grid fast flings (4 runs each, interleaved):**
- compose-rs composed items in 0 to 3 frames a run, with no janky frame and a main thread p99 of 2.7 to 2.9 ms.
- Compose laid out new tiles' text in 99 to 143 frames a run, with a p99 of 6.2 to 6.7 ms and 2 app-janky frames.

The stress list's first frame still composes 16 items (headless 0.25 ms).

**After the fifth review (D492–D496).** The fifth review found five more bugs, fixed since. None touches a steady scroll, so no device A/B was run:
- **Long scrolls (D492).** A `scroll_by` of 160 rows, or a `scroll_to_item` with a large offset, landed over 2 or 3 frames, an item in between shown: a scroll past the composed items placed none, so each round asked for 4. What a round asks for now doubles from the last.
- **Flings handed on (D493).** What a list leaves of a fling goes to its parents as a fling, as Compose's; a fast fling to the top of a list in a `ModalBottomSheet` dragged the sheet down.
- **Overshoot (D494).** The distance an overshoot measures counts items not measured yet at the average size, rather than dropping the rest.
- **Focus (D495).** A focus search waiting for items beyond a list's bounds ends when the focus moved meanwhile from outside it (a click).
- **Accessibility pages (D496).** A scroll action's page is the viewport as shown, its width horizontally and its height vertically.

**In round 11** (613eaf6, beside Compose: round 11 of [12](12-android-performance.md)), the full run kept what the A/Bs measured: on this list, main thread and RenderThread work 573 and 1,311 ms (round 10: 713 and 1,593 ms), native heap 14.7 and 17.2 MB (18.1 and 22.3 MB), `TotalTime` 118 ms (126 ms), the first frame's `compose-rs: frame` 6.6 ms (13.8 ms), and no text laid out in a frame in any run, on either list. As in the A/Bs, our session voted less for a RenderThread with less work, and on this list its p90 and p99 came out longer than Compose's (3.81 / 5.45 against 3.63 / 5.27 ms), though its frames are shorter at each boost level.

**Lazy grids** keep 2 lines composed and placed beyond each side (D456). Their open risk was that a fast fling would outrun that buffer. Checked on the new `grid` screen of both apps (D473): 3,000 tiles in 4 columns, 10 flings of 1,600 px in 60 ms as measure.py's pattern, 3 to 4 runs each, interleaved.
- Before D474, compose-rs composed items in 0 to 11 frames a run (mean 4.3, 6 runs), in runs of consecutive frames at the flings' starts with no prefetch between them. With D474, 0 to 2 a run (mean 1.0, 4 runs).
- No janky frame of compose-rs's; its main thread p99 was 2.6 to 3.0 ms.
- Compose laid out new tiles' text in 79 to 136 of its frames a run (7 runs), with a main thread p99 of 5.8 to 6.9 ms, and had 5 app-janky frames in 10 runs.
- So the grid's beyond and prefetch stay as they were.

## The hint session's target (D500)

In round 11, with D470's 18 % less RenderThread work, our session's vote on the list fell (295 → 190) and its RenderThread's p90 / p99 came out longer than Compose's (3.81 / 5.45 against 3.63 / 5.27 ms). A closed-loop model of the power HAL's controller ([14](14-performance-opportunities.md), "The session's target") explains it: under the target, the vote settles where the reports' mean is about 0.4 of the target, so the RenderThread's time alone held near 2.6 ms against 6.5 ms whatever the work. Reporting both threads' CPU time since the last frame against 8.33 ms puts that level where the frames' load is. Interleaved A/B, 10 runs each, against main and against 6.0 and 5.5 ms targets for the RenderThread alone (all variants in [14](14-performance-opportunities.md)):

| Stress screens | main (6.5 ms) | 6.0 ms | 5.5 ms | **both threads, 8.33 ms (D500)** |
|---|---|---|---|---|
| Scroll: RenderThread p50 / p90 / p99 | 2.49 / 3.59 / 5.15 ms | 2.24 / 3.14 / 4.44 ms | 2.11 / 2.91 / 4.02 ms | **2.06 / 2.89 / 3.93 ms** |
| Scroll: main thread p50 / p90 / p99 | 0.99 / 1.56 / 2.65 ms | 0.89 / 1.41 / 2.64 ms | 0.83 / 1.30 / 2.63 ms | **0.83 / 1.27 / 2.60 ms** |
| Scroll: our session's vote; frames at 200 or more | 214; 53 % | 262; 73 % | 320; 86 % | 332; 91 % |
| Scroll: main thread on the little cores | 38 % | 20 % | 12 % | 7 % |
| Scroll: janky frames (app / compositor) | 0 / 2 | 0 / 0 | 0 / 0 | 0 / 0 |
| Scroll: power, CPU rails | 452 ± 13 mW | +1.2 % ± 3.3 | +4.1 % ± 2.7 | +2.9 % ± 2.7 |
| Animation: RenderThread p50 / p90 / p99 | 6.35 / 7.41 / 8.56 ms | 6.37 / 7.41 / 8.39 ms | 6.26 / 7.31 / 8.33 ms | 6.35 / 7.31 / 8.41 ms |
| Animation: our session's vote | 467 | 487 | 549 | 476 |
| Animation: power, CPU rails | 1,861 ± 66 mW | 0.0 % ± 2.9 | +5.1 % ± 3.1 | −1.1 % ± 3.0 |

The 5.5 ms target sets off the heuristic boost in the stress animation, whose RenderThread frames take 4.8 ms of CPU time: +5.1 % CPU-rail power there for no shorter frames. Both threads against 8.33 ms never cross 1.2 × the target (10 ms), and its vote in the animation stays main's. `doFrame`s starting more than 2 ms late went 85 → 187 on the list in 10 runs, none more delayed by our work (5 → 2).

**Round 12** (4a28979, beside Compose: "Round 13 (history)" above) confirms it. On the list, the RenderThread's p50 / p90 / p99 is 2.00 / 2.75 / 3.94 ms against Compose's 2.59 / 3.56 / 4.88 ms (round 11: 2.58 / 3.81 / 5.45 ms), the main thread's 0.81 / 1.23 / 2.52 ms against 1.26 / 3.20 / 8.05 ms; the vote 369 (A/B: 332), 95 % of the RenderThread's frames at 200 or more, the main thread on the little cores 4 % of its time. In the animation, nothing moved (vote 472, RenderThread p50 6.36 ms). The late `doFrame`s: 13 to 15 a run on the list (round 11: 4 to 15), at most 4.5 ms late, none janky, the closest finishing 8.2 ms before its deadline. Power isn't measured in the round.

## HWUI's workers and graphics memory (D510–D512)

Two gaps were left in the stress animation, both off the frame path: HWUI's two worker threads (`hwuiTask0`, `hwuiTask1`) took about 20 % more CPU time than Compose's (10.8 against 8.9 s in round 12), and 3 s after start the process held 35 MB more graphics memory (154.6 against 119.4 MB). Traced with the sched, frequency and atrace view, gfx and hal categories, one run of each app at b790106, then an interleaved A/B.

**The workers do the same work, at lower clocks** (D511). Both apps' workers rasterize the 50 stroked waves on the CPU into masks, each frame (Skia's software path renderer, which HWUI gives its workers through the GPU context's executor); nothing else runs there. HWUI emits no slice on them, so this comes from their running time and the RenderThread's waits:

| Stress animation, 10 s | compose-rs | Kotlin Compose | Compose, `ModulateAlpha` |
|---|---|---|---|
| Workers' CPU time | 11.2 s | 8.8 s | 10.2 s |
| Workers' work (ms on the fastest core at its highest frequency) | 5,920 ms | 6,058 ms | 5,933 ms |
| Their mean frequency | 1.69 GHz | 2.30 GHz | 1.90 GHz |
| Their time on the mid / big cores | 73 / 27 % | 96 / 4 % | 82 / 18 % |
| The RenderThread asleep in `flush commands`, woken by them, per frame | 1.6 ms | 1.4 ms | |

- **Same work:** 4.41 against 4.43 ms of work a frame in the A/B below. The masks are the same: 50 a frame, 168,000 pixels on average, 49 sizes.
- **Lower clocks:** HWUI's hint session holds the workers with the main thread and the RenderThread (`dumpsys performance_hint`); ours holds the RenderThread and the main thread. The mid cores' frequency follows the heaviest threads on them, and Compose's main thread and RenderThread do twice our work there (its session's vote 719 against our 472). With `ModulateAlpha`, its RenderThread's work halved, Compose's workers ran slower too.
- **Not changed:** with the workers in our session (one traced run), they took 10.6 s at 1.85 GHz. Higher clocks cost power, and the frames already meet their deadlines. Adding the workers' time to our reports would put the vote at its ceiling (D511).

**A frame's worth of masks more** (D510). Skia's GPU cache keys a stroked path's mask by the path's contents and frees the key when they die, at the sync that frees the last frame's recording. It takes those frees in only at a flush, though.
- **Compose** draws its faded cells into layers each frame, and the layers' flush comes before the frame's own. So the last frame's masks are freed by then, and 82 % of each frame's mask uploads go into the textures the frame before used.
- **compose-rs** had no layer: one flush a frame, at its end, after its masks took their textures. So each frame took the textures of the frame two before (78 % of uploads, none the frame before), and kept 245 mask textures where Compose keeps 141, with HWUI's scratch surfaces at 79 against 63 MB.
- **Compose without layers** (`ModulateAlpha`) did the same: graphics 162.9 MB 3 s after start, 78 % of uploads two frames later, scratch surfaces 79.7 MB.
- **The fix:** a frame dropping a path drawn as a path now also draws a 1 × 1 cleared layer, so HWUI flushes the layers first. In a first run, 84 % of uploads went into the last frame's textures, and graphics memory was 123.5 MB.

A path kept per draw site and reset in place, as Compose's (D412), wouldn't have helped (D512). A variant releasing each new `Path`'s contents as soon as it was recorded changed nothing (78 % two frames later). A mask's key goes when the recording that drew it does.

**Interleaved A/B** (main b790106 against D510, and Compose, 8 runs each of the stress animation; medians; traced with the power rails and `sched_waking`, which lengthen the RenderThread's frames; `results/2026-10-24-pixel-8-pro-flush-layers.json`):

| Stress animation | main | D510 | Kotlin Compose |
|---|---|---|---|
| PSS 3 s after start | 182.5 MB | **154.8 MB** | 167.8 MB |
| PSS after 13 s | 197.8 MB | **165.9 MB** | 178.7 MB (165.1 to 319.2) |
| Graphics 3 s after start / after 13 s | 150.6 / 154.6 MB | 122.9 / 123.0 MB | 119.4 / 119.8 MB |
| HWUI's GPU caches 3 s after start: total; scratch surfaces; scratch buffers | 115.4; 79.3; 28.1 MB | 96.1; 60.0; 28.1 MB | 89.5; 62.6; 18.8 MB |
| Mask textures; uploads into the last frame's | 245; 0.1 % | 131; 85 % | 141; 82 % |
| Main thread per frame p50 / p90 / p99 | 2.78 / 3.95 / 5.33 ms | 2.88 / 4.19 / 5.49 ms | (see below) |
| RenderThread per frame p50 / p90 / p99 | 7.64 / 8.64 / 9.68 ms | 7.79 / 8.78 / 9.81 ms | |
| RenderThread work | 3,003 ms | 3,056 ms (+1.8 %) | |
| Workers' CPU time; work | 11.2 s; 5,322 ms | 11.4 s; 5,339 ms | |
| Janky frames (the app's), in 8 runs | 2 | 4 | |
| Power: CPU rails; all rails | 1,623 ± 38; 3,397 ± 38 mW | 1,656 ± 42; 3,434 ± 43 mW | |

- **Memory:** 28 MB less graphics memory, and 28 to 32 MB less PSS. compose-rs now holds less than Compose 3 s after start, not 15 MB more.
- **Left:** 9 MB of scratch buffers (28.1 against 18.8 MB) that the flush didn't change. Round 13 found it is a level either app lands on, run by run (below).
- **Traded:** a second flush and submit a frame (0.19 ms): the RenderThread +0.15 ms at p50 and +1.8 % work. CPU-rail power +2.0 % (± 2.4 %, not resolved), all rails +1.1 %. Each run's janky frames were 0 or 1 in both builds.
- **Compose's runs** in this session were slow, as in the first results' "With paths reused": 215 to 338 janky frames a run, both threads' p50 above 10 ms, 914 frames a run against 1,205. Their frames and power aren't compared here; their memory matches round 12's.

**Elsewhere nothing moved.** Only a frame dropping a path drawn as a path draws the layer. 5 runs each, main against D510:

| | main | D510 |
|---|---|---|
| Feed scroll: main thread / RenderThread p50 / p90 / p99 | 0.80 / 1.28 / 2.15; 1.48 / 2.25 / 3.23 ms | 0.78 / 1.27 / 2.15; 1.44 / 2.17 / 3.27 ms |
| Feed animation | 1.18 / 1.70 / 2.95; 2.49 / 3.24 / 4.62 ms | 1.16 / 1.63 / 2.90; 2.50 / 3.19 / 4.76 ms |
| Stress scroll | 0.84 / 1.31 / 2.64; 2.07 / 2.85 / 3.87 ms | 0.84 / 1.31 / 2.54; 2.08 / 2.88 / 4.03 ms |
| CPU rails: feed scroll, animation, stress scroll | 451, 430, 498 mW | 438, 433, 501 mW |

No janky frame of the app's in either build, except one in a D510 stress-scroll run.

**Round 13, beside Compose** (d39ecb4, measure.py without the power rails, 3 runs each; "Round 13: memory" above). 3 s after start, compose-rs's graphics memory was 114.9 MB against Compose's 119.7 MB (round 12: 154.6 against 119.4 MB) and its PSS 144.1 against 165.7 MB; an untraced probe of 4 runs each had 114.7 against 119.6 MB. Skia's caches held 96.0 against 89.7 MB: scratch surfaces 60.0 against 62.7 MB, scratch buffers 28.1 against 18.9 MB at the median, but each app's runs land on 18.8 or 28.1 MB (compose-rs's 18.8, 28.1, 28.1; Compose's 28.0, 18.8, 18.9 MB), so the A/B's 9 MB is that level, not compose-rs's. After 13 s, both apps' graphics memory grew by an amount that varies from run to run, mostly in the GPU driver's memory (`GL mtrack`): compose-rs's runs 131.7 to 208.7 MB, Compose's 132.2 to 153.0 MB (the probe: 123.0 to 208.7 against 128.7 to 182.4 MB, medians 137.7 against 136.8 MB). The RenderThread's frames were within round 12's spread (p50 / p90 / p99 6.36 / 7.38 / 8.53 → 6.39 / 7.54 / 8.49 ms) and its work 6 % higher (3,172 → 3,362 ms, CPU time +2 %), still 44 % below Compose's; no frame missed its deadline.

## A mutable `Path` (D530–D535)

compose-rs's `Path` is now Compose's: mutable and shared, remembered and reset in a drawing block (D530). The stress animation's waves remember one `Path` and reset it for each wave, as Stress.kt's (D533). Drawing it records each wave's contents, as Kotlin's `drawPath` records a copy into the node's recording (D531). On Android, the serializer defines the 50 contents under the same 50 ids each frame, and the replayer refills the same 50 `android.graphics.Path`s: after the first frame, no new id and no drop (D532).

**Headless** (`benchmarks/benches/animation.rs` on the Mac, criterion means; main b847ddd against D530–D535, one run each):

| `stress_animation` | main | D530 |
|---|---|---|
| A frame | 109.5 µs | 105.1 µs (−4 %) |
| A frame, then written for Kotlin and taken with its resource changes | 132.0 µs (p99 183 µs) | 122.6 µs (−7 %; p99 134 µs) |
| Bytes Kotlin takes a frame; of them resource changes | 47,285; 22,800 | 47,235; 22,750 |

The frame no longer copies each wave into a new `Arc<Path>` (a `&Path` was copied at each draw) nor grows a new one per wave: a reset keeps the storage. The bytes lose the 50 drops (5 bytes each) and gain a step count (4 bytes) per definition (D535). The device ran the first version of D530–D535; the review's fixes after it (D531, D532, D535) don't change the waves' flow: the same 50 ids redefined each frame.

**On the device** (Pixel 8 Pro; main b847ddd against D530–D535, and D530–D535 without D510's flush layer, and Compose, 6 runs each of the stress animation, interleaved; medians; traced as measure.py traces, without the power rails; `results/2026-10-24-pixel-8-pro-mutable-path.json`):

| Stress animation | main | D530 | D530, no flush layer | Kotlin Compose |
|---|---|---|---|---|
| Main thread per frame p50 / p90 / p99 | 2.30 / 2.96 / 4.01 ms | 2.30 / 2.93 / 4.09 ms | 2.32 / 2.95 / 3.97 ms | 3.82 / 5.35 / 7.65 ms |
| RenderThread per frame p50 / p90 / p99 | 6.38 / 7.47 / 8.63 ms | 6.39 / 7.48 / 8.41 ms | 6.34 / 7.32 / 8.40 ms | 7.44 / 8.77 / 10.61 ms |
| Main thread / RenderThread work | 1,544 / 3,587 ms | 1,535 / 3,566 ms | 1,548 / 3,575 ms | 3,068 / 6,339 ms |
| HWUI's workers: CPU time; work | 9.53 s; 5,371 ms | 9.62 s; 5,377 ms | 9.69 s; 5,382 ms | 7.77 s; 5,466 ms |
| Graphics 3 s after start / after 13 s | 114.8 / 127.2 MB | 114.7 / 123.1 MB | 150.6 / 158.8 MB | 119.6 / 119.9 MB |
| PSS 3 s after start / after 13 s | 145.6 / 164.4 MB | 144.6 / 163.6 MB | 179.9 / 201.0 MB | 167.8 / 209.9 MB |
| HWUI's GPU caches 3 s after start / after 13 s | 87.1 / 96.6 MB | 91.5 / 96.4 MB | 115.2 / 120.9 MB | 89.5 / 91.2 MB |
| Mask textures; uploads into the last frame's | 131; 84 % | 131; 85 % | 245; 0.2 % | 143; 84 % |
| Janky frames (the app's), in 6 runs | 0 | 1 | 0 | 2 |

- **Nothing moved on the device**, within the runs' spread: both threads' frames, their work, the workers' time, memory. HWUI does the same work either way: Kotlin's `reset` and our refill both give each wave's recording new native contents (a recording keeps its own copy), as before with a new `Path` per wave (D512). After 13 s, graphics memory still grows by an amount that varies run to run (main 114.7 to 191.4 MB, D530 114.7 to 161.1 MB), as round 13 found.
- **D510's flush layer is still needed** (D534). Without it, each frame's 50 masks take the textures of the frame two before (0.2 % of uploads into the last frame's, against 85 %), and the process holds 36 MB more graphics memory (150.6 against 114.7 MB 3 s after start), as main did before D510. Kotlin needs none because its faded cells' layers make HWUI flush first each frame; with `ModulateAlpha` it held as much as compose-rs without the layer (D510).

**Elsewhere nothing moved.** The feed's scroll, 4 runs each, main against D530: main thread p50 / p90 / p99 0.82 / 1.30 / 2.09 → 0.80 / 1.27 / 2.06 ms, RenderThread 1.48 / 2.21 / 3.34 → 1.46 / 2.22 / 3.29 ms, work 603 / 905 → 597 / 913 ms; no janky frame in either. Its icons are fixed contents (`Arc<PathData>`), defined once as before.

## Parity mode (D560–D569, D574)

Since D560, compose-rs does by default the work Compose 1.12.1 does: lazy lists and grids prefetch as Compose's strategy and scheduler ("Compose's defaults" (a), (b), (f)), and every compose-rs-only behaviour is an optimization flag, off by default where it changes the work or the rendering ([07](07-platforms.md), "compose-rs's own optimizations"). measure.py runs bench-rs at parity by default (`--optimizations parity`: `-e optimizations none`, the hint session off too, D568), and each flag alone against it with `--optimizations each` (D569), the candidates for Compose in [14](14-performance-opportunities.md), "Backport candidates".

What's still not the same, structurally:

- **No pausable composition.** A request composes its item in one step, where Compose pauses at the runtime's pause points when out of time and resumes after the next frame. So the gate differs too, against compose-rs: Compose starts a composition when the time left exceeds the average of one stretch between pause checks plus a pause (`resumeTimeNanos + pauseTimeNanos`), compose-rs only when it exceeds the average of a whole composition, apply included (Compose's apply is a step of its own, gated by its own average). A long item's composition starts later here, in a longer idle time, where Compose's would start and be split. This goes when compose-rs has pausable composition (D578).
- **Composition in rounds, not in measure.** compose-rs's layout asks for the items it lacks by an estimate of their size: as many as cover the room at the sizes it knows (D576, backward as forward, a grid's lines too), a few at a time while it knows few sizes, doubling from round to round (D472, D487; a grid's first layout as a list's since D578, where it composed 15 lines for 6). With sizes that vary, a frame may still compose more than it places, up to about twice: the next item stays composed (it's the one a scroll that way requests), the others go in the next idle time, disposed (never measured), where Compose composes only what its measurement reaches. Items that leave need a composition to be deactivated: a frame composing anyway deactivates them in the same composition; a frame with nothing to compose leaves them composed, unplaced and undrawn, until the next idle time with any time left, whose first request deactivates them, whatever else waits (D575), where Compose's measurement deactivates them without composing.
- **Deltas summed.** The strategy hears a frame's scroll deltas summed, Compose's `onScroll` each delta; and one queue in order, Compose's a priority queue (one list's requests either way).
- **Small filled paths.** compose-rs masks any small filled path, where Compose's `VectorPainter` caches vectors only (no screen here has another small filled path; D567).

### Each optimization alone (round 14)

`results/2026-10-25-pixel-8-pro-each.json`: bench-rs at parity and with each flag alone, interleaved run by run, 5 runs a subject in five invocations whose order was rotated; [14](14-performance-opportunities.md), "Backport candidates", has the method, the feed's tables, the summary and the interactions with `all`. The change of each median against parity's; **\*** marks an exact Mann–Whitney test over the runs at p < 0.05 (with about 400 tests in the round, a lone star with nothing beside it is most likely noise).

**Stress scroll** (5 runs a subject)

| | main p50 | p90 | p99 | RenderThread p50 | p90 | p99 | work main | work RenderThread | boost | app janky |
|---|---|---|---|---|---|---|---|---|---|---|
| parity | 1.37 | 2.31 | 3.87 | 3.15 | 4.69 | 6.30 | 490 | 1,119 | 82 | 0 |
| `single_drawing_fade` | +0 % | +1 % | +2 % | +2 % | +1 % | -0 % | -2 % | +1 % | 63 | 1 |
| `performance_hint` | -35 %* | -36 %* | -30 %* | -36 %* | -41 %* | -39 %* | +43 %* | +23 %* | 326 | 0 |
| `reuse_paragraph_at_same_layout_width` | +1 % | +1 % | +2 % | -0 % | +0 % | +1 % | -0 % | -0 % | 78 | 0 |
| `prefetch_two_ahead` | +1 % | -4 % | +1 % | +2 % | -1 % | -1 % | +0 % | -1 % | 71 | 0 |
| `prefetch_budget_recent_maximum` | +1 % | -2 % | -0 % | +1 % | -0 % | +1 % | -3 % | -1 % | 69 | 1 |
| `prefetch_at_rest` | +2 % | +2 % | +4 % | +3 % | +1 % | -1 % | -5 % | -2 % | 59 | 0 |
| `keep_items_behind` | +5 %* | +0 % | +0 % | -2 % | -0 % | -0 % | +2 % | +1 % | 77 | 1 |
| `draw_prefetched_ahead` | -0 % | -15 %* | -23 %* | -0 % | -0 % | -1 % | -0 % | -1 % | 65 | 0 |
| `grid_lines_beyond` | +1 % | +1 % | +4 % | -0 % | +0 % | +2 % | -1 % | +0 % | 74 | 1 |
| `flush_layers_before_dropped_paths` | +1 % | -2 % | +1 % | -0 % | +0 % | -2 % | -0 % | -1 % | 70 | 0 |
| `paragraph_render_nodes` | +2 % | +2 % | +3 % | +3 % | +1 % | +2 % | -2 % | +1 % | 73 | 1 |

**Stress animation** (5 runs a subject; the supplement's, 3)

| | main p50 | p90 | p99 | RenderThread p50 | p90 | p99 | work main | work RenderThread | graphics after 13 s | GPU caches after 13 s | boost | app janky |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| parity | 2.43 | 4.24 | 6.82 | 7.62 | 8.80 | 10.53 | 1,626 | 6,144 | 164 | 110 | 613 | 3 |
| `single_drawing_fade` | +4 % | -23 %* | -34 %* | -16 %* | -15 %* | -19 %* | -2 %* | -42 %* | +12 %* | +31 %* | 444 | 1 |
| `flush_layers_before_dropped_paths` | +0 % | -5 % | +6 % | -1 % | +0 % | -3 % | +2 % | +1 % | -10 % | -18 % | 621 | 2 |
| `paragraph_render_nodes` | +4 % | +2 % | -3 % | +1 % | +0 % | -3 % | -2 %* | -0 % | -6 % | +9 % | 566 | 5 |
| `performance_hint` | +0 % | -2 % | +5 % | -1 % | +0 % | +0 % | +4 %* | +0 % | -14 %* | -9 % | 641 | 6 |
| parity (the supplement's) | 2.40 | 3.93 | 6.15 | 7.69 | 8.78 | 10.23 | 1,562 | 6,106 | 163 | 110 | 566 | 2 |
| `reuse_paragraph_at_same_layout_width` | -0 % | +5 % | +0 % | -0 % | -0 % | -4 % | -1 % | -1 % | +0 % | +8 % | 553 | 3 |
| `prefetch_two_ahead` | +10 % | +14 % | +1 % | +2 % | +1 % | +2 % | +1 % | +1 % | +1 % | +9 % | 539 | 1 |
| `prefetch_budget_recent_maximum` | +2 % | +7 % | +11 % | -1 % | +1 % | +5 % | +3 % | +1 % | -13 % | -2 % | 602 | 2 |
| `prefetch_at_rest` | +1 % | +8 % | +7 % | -2 % | +0 % | -0 % | +5 % | +1 % | +15 % | +34 % | 618 | 1 |
| `keep_items_behind` | +2 % | +7 % | +6 % | -1 % | +0 % | +2 % | +3 % | +1 % | +13 % | +16 % | 588 | 2 |
| `draw_prefetched_ahead` | +0 % | +3 % | +20 % | +1 % | +1 % | -1 % | +2 % | -0 % | +15 % | -1 % | 574 | 3 |
| `grid_lines_beyond` | +1 % | +5 % | +1 % | +0 % | +1 % | -3 % | +0 % | -1 % | +7 % | +8 % | 541 | 1 |

What each one does on these screens:

- **`performance_hint`** is the largest on the list: every percentile on both threads −30 to −41 %, the RenderThread's boost minimum from 82 to 326, the work metric +23 % and +43 % (the same frames at higher clocks). It is why round 13's compose-rs (which had it) was ahead of Compose on the list's RenderThread, and why compose-rs at parity is behind. In the stress animation HWUI's session already boosts every frame (613 at parity), and the frames don't change; its graphics memory after 13 s read 14 % lower, in a measure that varies by about a fifth from run to run.
- **`draw_prefetched_ahead`**: the list's main-thread p90 / p99 −15 / −23 %: the frames that reveal an item draw nodes recorded in the idle time (D350) instead of recording them (`Record View#draw()`, up to 4 ms at parity).
- **`single_drawing_fade`**: in the stress animation, RenderThread work −42 %, its p50 / p90 / p99 −16 / −15 / −19 % (the 132 faded cells without their offscreen layers, as `ModulateAlpha`), the main thread's p90 / p99 −23 / −34 % (it waits less for the RenderThread's sync). It costs memory alone: graphics after 13 s +12 %, HWUI's GPU caches +31 %, since Skia frees the dropped wave masks at a layers' flush and the cells' layers were the frame's flush; with the flush layer as well (`all`), the caches are back to parity's (−3 %; [14](14-performance-opportunities.md), "Interactions").
- **`flush_layers_before_dropped_paths`**: at parity, the GPU caches after 13 s −18 % and graphics −10 %, not resolved in 5 runs (the caches vary by 27 % across parity's runs; D574's 3 runs read −33 %): with the cells drawn offscreen, the frame's own layers flush the dropped masks already, and D510's layer adds a second flush. Its use is with `single_drawing_fade`.
- **`paragraph_render_nodes`** and **`keep_items_behind`**: within the spread on the stress screens but the kept items' main p50 +5 %\* on the list (their bookkeeping each frame, D380).
- **The others** are within the runs' spread on both stress screens; on the stress animation the seven others (the supplement: 3 runs a subject, against its own 3 of parity, which no test over 3 runs a side resolves at p < 0.05) moved its frames by at most +14 % at p90 and +20 % at p99 (`prefetch_two_ahead`'s and `draw_prefetched_ahead`'s main thread) and its memory after 13 s by −13 to +34 %, about parity's own spread there (main p90 18 %, p99 16 %, GPU caches 27 %): none of them touches that screen (no list, no width that changes).

The stress animation's janky frames: parity had one in 3 of its 5 runs, each flag 1 to 6 over its runs (one or two frames in a run; on this screen both apps' frames are near the deadline: the parity round had none of compose-rs's in 3 runs and 2 of Compose's).

### A short parity run against round 13 (history)

Superseded by round 14 ("Results" above, and "Each optimization alone (round 14)").

`results/2026-10-24-pixel-8-pro-parity-short.json`: compose-rs at 637b3ad (D560–D569) with no optimization, `--runs 5` (startup), 3 traced runs of each scroll and animation scenario, `speed`. Every run started cool (thermal status 0, battery 29.9–31.7 °C; no wait). Round 13 (`results/2026-10-24-pixel-8-pro-round-13.json`, compose-rs at d39ecb4 with its hint session, draw-ahead, kept items, two ahead, D341's budget and D153's single-drawing fade and paragraph nodes; before D520–D529's layers) in parentheses. Medians of 3 runs, ms.

| | Compose | compose-rs, parity (round 13) |
|---|---|---|
| Feed scroll main p50 / p90 / p99 | 1.46 / 3.11 / 6.95 (1.50 / 3.22 / 7.09) | 1.33 / 2.30 / 3.49 (0.77 / 1.23 / 2.07) |
| Feed scroll RenderThread p50 / p90 / p99 | 2.01 / 3.34 / 5.00 (2.10 / 3.43 / 5.08) | 2.59 / 3.53 / 5.16 (1.48 / 2.00 / 2.97) |
| Feed scroll work main / RenderThread | 890 / 930 (854 / 888) | 438 / 728 (646 / 985) |
| Stress scroll main p50 / p90 / p99 | 1.29 / 3.22 / 7.96 (1.26 / 3.23 / 8.01) | 1.38 / 2.43 / 3.99 (0.83 / 1.24 / 2.39) |
| Stress scroll RenderThread p50 / p90 / p99 | 2.67 / 3.65 / 5.54 (2.53 / 3.53 / 5.03) | 3.10 / 4.68 / 6.57 (2.12 / 2.90 / 3.86) |
| Stress scroll work main / RenderThread | 1,216 / 1,623 (1,314 / 1,706) | 483 / 1,147 (710 / 1,541) |
| Scroll janky frames, the app's (feed + stress, 3 runs each) | 2 + 6 (3 + 5) | 0 + 0 (1 + 0) |
| Feed animation main p99 / RenderThread p99 | 3.35 / 5.95 (3.33 / 5.74) | 2.88 / 5.20 (2.85 / 4.96) |
| Stress animation RenderThread p50 / p90 / p99 | 7.47 / 8.90 / 10.80 (7.42 / 8.87 / 10.80) | 7.63 / 8.75 / 10.17 (6.39 / 7.54 / 8.49) |
| Stress animation work main / RenderThread | 2,924 / 5,994 (3,028 / 6,049) | 1,511 / 5,755 (1,493 / 3,362) |
| Stress animation graphics memory after 13 s (MB) | 165.3 (146.1) | 149.5 (173.7) |
| Startup `TotalTime` feed / big / stress (medians of 5) | 135 / 233 / 148 (136 / 236 / 147) | 114 / 156 / 119 (106 / 167 / 119) |

What changed, for compose-rs:

- **Durations rose, work fell, on the scrolls.** Without its own hint session, compose-rs's threads get only HWUI's boost, which follows the main thread's frames, short here: the RenderThread's time-weighted boost minimum was 37–49 on the feed and 67–86 on the stress list, against Compose's 91–192 and 252–277 (D370's picture; D390's reason for the session). At lower clocks the same frames take longer and the work metric (time on the fastest core at its highest frequency) falls. Its main thread's p99 stays at half Compose's (3.49 against 6.95 ms on the feed, 3.99 against 7.96 on the stress list), with no janky frame of the app's; its RenderThread is now slower than Compose's on the stress list (p99 6.57 against 5.54 ms), with D522's render node per clip and layer as Compose's and no boost of its own.
- **The stress animation's RenderThread does Compose's work** (5.8 s against Compose's 6.0; round 13 3.4 s before D521's offscreen fades), and its p50 is Compose's (7.63 against 7.47 ms). Its graphics memory after 13 s is lower than Compose's (149.5 against 165.3 MB) without D510's flush layer: the frame's offscreen layers flush the dropped masks in, as D510 found Compose's do.
- **Startup and memory** didn't move beyond the runs' spread.

### Each optimization alone: the first round, 3 runs (history)

Superseded by "Each optimization alone (round 14)" above, which agrees with it on every flag that stands out.

`results/2026-10-24-pixel-8-pro-parity-each.json`: bench-rs at parity and with each flag alone, interleaved run by run, 3 runs each (`--optimizations each`). The change of each median against parity's (the first row, ms or ms of work); 3 runs a subject, so differences under about 5 % are within the runs' spread.

**Feed scroll** (3 runs a subject)

| | main p50 | p90 | p99 | RenderThread p50 | p90 | p99 | work main | work RenderThread | app janky |
|---|---|---|---|---|---|---|---|---|---|
| parity | 1.28 | 2.27 | 3.40 | 2.63 | 3.42 | 4.78 | 428 | 714 | 0 |
| `single_drawing_fade` | +2 % | -4 % | +5 % | -1 % | +1 % | +0 % | +3 % | +3 % | 0 |
| `performance_hint` | -34 % | -36 % | -28 % | -46 % | -42 % | -38 % | +56 % | +33 % | 0 |
| `reuse_paragraph_at_same_layout_width` | +2 % | -2 % | +0 % | -1 % | +3 % | +5 % | +3 % | +3 % | 0 |
| `prefetch_two_ahead` | +1 % | -5 % | +1 % | -3 % | +0 % | +3 % | -1 % | +0 % | 0 |
| `prefetch_budget_recent_maximum` | +2 % | -0 % | -4 % | -0 % | +2 % | +3 % | -1 % | -1 % | 0 |
| `prefetch_at_rest` | +2 % | +0 % | -6 % | -1 % | +0 % | +3 % | -1 % | -1 % | 0 |
| `keep_items_behind` | +3 % | -2 % | +2 % | +1 % | +4 % | +4 % | +0 % | -0 % | 0 |
| `draw_prefetched_ahead` | +3 % | -16 % | -14 % | -1 % | +1 % | +7 % | +4 % | +0 % | 0 |
| `grid_lines_beyond` | +2 % | +0 % | +5 % | -2 % | +6 % | +9 % | +0 % | +1 % | 0 |
| `flush_layers_before_dropped_paths` | +3 % | +1 % | +9 % | +0 % | +1 % | +6 % | -0 % | -1 % | 0 |
| `paragraph_render_nodes` | +5 % | +5 % | +10 % | -1 % | +6 % | +7 % | +3 % | +4 % | 0 |

**Stress scroll** (3 runs a subject)

| | main p50 | p90 | p99 | RenderThread p50 | p90 | p99 | work main | work RenderThread | app janky |
|---|---|---|---|---|---|---|---|---|---|
| parity | 1.43 | 2.46 | 4.06 | 3.20 | 4.68 | 6.42 | 478 | 1,141 | 0 |
| `single_drawing_fade` | -3 % | -0 % | +4 % | -1 % | +2 % | +1 % | +1 % | +1 % | 0 |
| `performance_hint` | -36 % | -36 % | -35 % | -37 % | -40 % | -36 % | +49 % | +26 % | 0 |
| `reuse_paragraph_at_same_layout_width` | -1 % | +1 % | -4 % | -5 % | +2 % | +0 % | +0 % | +2 % | 0 |
| `prefetch_two_ahead` | -3 % | -2 % | -2 % | -4 % | -1 % | -1 % | +5 % | +4 % | 0 |
| `prefetch_budget_recent_maximum` | -2 % | +1 % | +1 % | +0 % | +2 % | +3 % | -0 % | -0 % | 0 |
| `prefetch_at_rest` | -3 % | -3 % | -0 % | -4 % | -0 % | +0 % | +2 % | +1 % | 0 |
| `keep_items_behind` | +1 % | -0 % | -1 % | -3 % | -0 % | +1 % | +5 % | +4 % | 0 |
| `draw_prefetched_ahead` | -3 % | -16 % | -20 % | -1 % | +2 % | +5 % | +1 % | +2 % | 0 |
| `grid_lines_beyond` | -3 % | +2 % | -1 % | +2 % | +0 % | -1 % | +1 % | +0 % | 0 |
| `flush_layers_before_dropped_paths` | -3 % | -3 % | -2 % | -1 % | +4 % | -1 % | +2 % | -0 % | 0 |
| `paragraph_render_nodes` | -1 % | -1 % | -1 % | -2 % | +1 % | +2 % | +3 % | +6 % | 0 |

**Stress animation** (3 runs a subject)

| | main p50 | p90 | p99 | RenderThread p50 | p90 | p99 | work main | work RenderThread | app janky | graphics after 13 s (MB) | GPU caches after (MB) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| parity | 2.50 | 4.04 | 6.04 | 7.71 | 8.80 | 10.16 | 1,582 | 6,123 | 2 | 185.9 | 146.5 |
| `single_drawing_fade` | -0 % | -20 % | -23 % | -17 % | -15 % | -13 % | -2 % | -42 % | 0 | -12 % | -15 % |
| `flush_layers_before_dropped_paths` | -3 % | -3 % | +6 % | -1 % | -0 % | +4 % | +1 % | +1 % | 3 | -14 % | -33 % |
| `paragraph_render_nodes` | -6 % | +3 % | +12 % | -2 % | -1 % | -1 % | +1 % | +1 % | 1 | -5 % | -12 % |
| `performance_hint` | -3 % | -1 % | +18 % | -2 % | +0 % | +5 % | +5 % | +1 % | 3 | -21 % | -25 % |

Taken before main's text fidelity changes (D570–D573: letter spacing, line heights, colour tokens), which change both sides of each comparison alike; the full round follows. The scrolls' runs come from one `each` invocation (every run started at battery 29.5–33.7 °C, thermal status 0, no wait). Its stress-animation runs were throttled and aren't used (kept apart in the JSON as `stress_animation_throttled`): the thermal guard waited with the last run's animation still on screen, so the device kept heating (battery 39 °C, skin 39.6 °C, thermal status 1; gfxinfo p50 15–23 ms against 14–15). measure.py now stops our apps before waiting, and can wait on the skin temperature too (D574). The stress animation was then measured again for parity and the four flags that can matter there, in three invocations of one run per subject, each run started at thermal status 0, battery ≤ 34 °C and skin ≤ 35 °C (no time limit), the subjects' order rotated; all twelve would have taken more than the 90 minutes allowed at about 4 minutes of cooling a run. The other flags do nothing on that screen (no list, no text changing width).

What each one does here:

- **`performance_hint`** is the largest: on the scrolls every percentile on both threads falls by a third or more (feed RenderThread p50 2.63 → 1.41 ms), the work metric rising (+26 to +56 %: the same frames at higher clocks spend more of their time waiting on memory, D440). It's what round 13's compose-rs had, and why its durations were shorter than parity's. On the stress animation it changes nothing measurable: HWUI's session already boosts its long frames.
- **`draw_prefetched_ahead`**: main-thread p90 and p99 −14 to −20 % on both scrolls (the frames revealing an item draw nodes recorded ahead, D350), RenderThread unchanged, work within the spread.
- **`single_drawing_fade`**: on the stress animation, RenderThread work −42 % and p50 / p99 −17 / −13 % (its 132 faded cells skip their offscreen layers, D521), main thread p90 / p99 −20 / −23 % (it waits less for the RenderThread's sync), graphics memory −12 %. Nothing on the scrolls, which fade nothing.
- **`flush_layers_before_dropped_paths`**: on the stress animation, HWUI's GPU caches after 13 s −33 % (146.5 → 98 MB), graphics memory −14 %, durations within the spread: with the cells offscreen at parity, D510's flush still frees the dropped masks' textures earlier than the frame's own layers do. Graphics memory varies between subjects by up to about 20 % in 3 runs (the hint session's −21 % is that), so only the GPU caches' drop stands out.
- **The others** (`reuse_paragraph_at_same_layout_width`, `prefetch_two_ahead`, `prefetch_budget_recent_maximum`, `prefetch_at_rest`, `keep_items_behind`, `grid_lines_beyond`, `paragraph_render_nodes`) are within 3 runs' spread on these flings: each is for a case these runs have little of (a width changing, D551; a reversal's first frame, D375/D380; overruns past the vsync, D341/D381, which need more runs to count; grids, D456). `paragraph_render_nodes` no longer helps: each card is a render node of its own since D522, so a paragraph's node saves little recording.


## Next steps

- **Pausable composition** (next): the one structural difference left at parity ("Parity mode"). In round 14 it cost no frame on the list (no janky frame of compose-rs's, one text laid out in a frame in 3 runs); the steps past the vsync there were measurements, which Compose doesn't split either, run unboosted on a little core.
- **The list's RenderThread at parity** is behind Compose's (p50 3.13 against 2.61 ms) for 32 % less work: HWUI's session boosts compose-rs's frames little (67 against 277), its main-thread frames being short. `performance_hint` closes it and more (2.09 against 2.51 ms with `all`); for Compose, the same report in HWUI's session ([14](14-performance-opportunities.md), "Backport candidates").
- **The stress animation's graphics memory after 13 s** at parity is above Compose's (171.5 against 140.6 MB at the median, compose-rs's runs 164 to 203 MB), with 9 MB more in Skia's caches; how much the GPU driver's memory grows varies run to run in both apps and hasn't been traced. HWUI's workers' 18 % more CPU time (D511) is unchanged.

Earlier items (rounds 1 to 13):

- **Paths in the bridge**: done (D410, D411, "With paths reused"); since D530, a `Path` is reset in place as Compose's, the waves one remembered `Path` as Kotlin's, refilled in the same `android.graphics.Path`s (D532), with no change on the device and D510's flush still needed (D534, "A mutable `Path`"). HWUI's worker threads take about 21 to 30 % more CPU time than Compose's in the animation (10.8 against 8.9 s in round 12, 10.9 against 8.6 s in round 11, 11.0 against 8.5 s in round 10, 11.2 against 8.6 s in round 9) for the same work, at lower clocks: not changed (D511). In round 13, 10.8 against 8.6 s. The graphics memory compose-rs held more than Compose 3 s after start was a frame's worth of the waves' masks: fixed (D510; "HWUI's workers and graphics memory"), confirmed beside Compose in round 13 (114.9 against 119.7 MB); the 9 MB of scratch buffers left is a level both apps land on, run by run. After 13 s both apps' graphics memory has grown by up to 90 MB, in the GPU driver's memory more than in Skia's caches, differently each run: not traced.
- **Infinite transitions**: done (D420–D424). The 0.4 ms of tasks a frame left is mostly easing curves and color conversions.
- **The stress list's main thread on the little cores**: done (D440, "The main thread in the hint session"). The main thread joined compose-rs's hint session: in its A/B, on the little cores 79 → 28 % of its time, p50 1.54 → 1.07 ms, `doFrame`s waiting for a prefetch step 58 → 4 in 12 runs, no power change resolved. In round 10, 21 % on the little cores, p50 1.02 ms, no `doFrame` waiting for a prefetch step, and no frame laying a newly revealed item out itself (1 to 3 a run in round 9). In round 11, with less work to report after D470 and so a lower vote, 46 % on the little cores, p50 1.08 ms (Compose 1.25 ms), 2 `doFrame`s waiting for a prefetch step in 3 runs, and still no frame laying an item out itself. In round 12, with both threads' work reported (D500), 4 % on the little cores, p50 0.81 ms (Compose 1.26 ms), no `doFrame` waiting for a prefetch step. In round 13, 4 % (4 to 11 %), p50 0.83 ms (Compose 1.26 ms); one `doFrame` in 3 runs started 2.3 ms late after a prefetch step that ran 8.2 ms on a little core, on time. If those frames come back, splitting an item's measurement so a step can stop at the deadline ([12](12-android-performance.md), "Next steps") is the fix.
- **Item reuse** (D430–D435): measured on the device for the first time in round 10, in the same build as D440, whose faster cores raise the work metric; a prefetched item's composition did about the same work as in round 9 (0.167 against 0.163 ms) and half as many paragraphs were laid out ([12](12-android-performance.md), "Item reuse on the device"). An interleaved A/B would separate the two; in round 11 an item's composition did 0.132 ms of work, with the main thread back on the little cores 46 % of its time, where the metric reads lower, and in rounds 12 and 13 0.169 and 0.163 ms, on the mid and big cores again (D500). The list's native heap was 1.0 to 1.5 MB larger than in round 9: items the first layout composed beyond those it wanted, kept for reuse; fixed (D460, "Memory").
- **The RenderThread's boost on the list** varies with the session and, since D470, follows the lower work: our session's vote averaged 324 in round 9, 247 to 257 in D440's A/B, 295 in round 10 and 190 in round 11, for RenderThread p50s of 2.20, 2.49, 2.49 and 2.58 ms (Compose: 2.62, 2.63 and 2.63 ms in rounds 9 to 11). In round 11 its p90 and p99 were longer than Compose's (3.81 / 5.45 against 3.63 / 5.27 ms), though shorter at each boost level: the one place compose-rs is behind on these screens. Done since (D500, "The hint session's target"): the session reports both threads' CPU time against 8.33 ms, whose vote follows the load (332 on the list, 476 in the stress animation); in its A/B the list's RenderThread p50 / p90 / p99 went 2.49 / 3.59 / 5.15 → 2.06 / 2.89 / 3.93 ms, for +2.9 % (± 2.7) CPU-rail power. Round 12, beside Compose: 2.00 / 2.75 / 3.94 against 2.59 / 3.56 / 4.88 ms (vote 369): shorter at every percentile again; round 13: 2.12 / 2.90 / 3.86 against 2.53 / 3.53 / 5.03 ms (vote 341). The vote now rises with the load, so a heavier list would be boosted more, up to the ceiling the stress animation already sits at.
- **The lazy list's policy**: done (D470–D496, "The lazy list's policy"). Only the items in view are placed, prefetch runs from the first layout on, and the first layout composes 16 items where it composed 87. In its first A/B, main thread work −13 %, RenderThread work −14 %, native heap −3 MB, `TotalTime` −7 ms; in round 11 against round 10, −20 %, −18 %, −5 MB and −8 ms; round 12 kept the native heap and `TotalTime` (17.2 MB, 118 ms), its work reading higher on boosted cores, and round 13 too (17.2 MB, 119 ms; work 710 and 1,541 ms). The 10 in-frame rounds a run that remain laid out no text; headless, the only items flings compose in frames are sticky headers, when a scroll back enters the section before (as Compose's would).
- **Shader warm-up**: a pipeline compiled on first use cost compose-rs its one janky scroll frame in the first results (none of the app's in rounds 9 to 13). Compose's apps share the same cache behavior; it isn't specific to compose-rs, but it showed here.
