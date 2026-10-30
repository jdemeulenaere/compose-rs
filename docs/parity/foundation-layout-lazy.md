# Parity ledger: foundation layout, gestures, scrolling and lazy layouts

What compose-rs lacks, and which of Compose's tests to adapt, for Compose's foundation layouts (`foundation-layout`), gestures, scrolling and lazy layouts. It drives the porting work: one row per Compose test file, the public API compose-rs lacks, the missing features in a suggested order, and notes for the agents adapting the tests.

## Scope

- **Reference:** androidx-main at `d153d87ed76` (2026-10-25), `frameworks/support`, read-only. Paths below are relative to `frameworks/support`.
- **Compose sources read:**
  - `compose/foundation/foundation-layout`: `api/current.txt`, `src/commonMain`, and the tests in `src/androidDeviceTest` and `src/androidHostTest`;
  - `compose/foundation/foundation`: `api/current.txt` (the root package's scroll, overscroll and gesture APIs, and the packages `gestures`, `gestures.snapping`, `lazy`, `lazy.grid`, `lazy.layout`, `lazy.staggeredgrid`, `pager` and `relocation`), with their tests in `src/androidDeviceTest`, `src/androidHostTest` and `integration-tests/lazy-tests/src/androidTest`.
- **Out of scope** (other ledgers): text and selection; clickable, focusable, hoverable, indication, interaction, border, background, Image and Canvas. `ClickableInScrollableViewGroupTest` is clickable's, so it isn't counted here. `relocation/` (bring into view) is included, since scrolling containers are what respond to it.
- **compose-rs read** at `7f744da`:
  - `crates/compose-foundation/src`: `layout/`, `lazy.rs`, `lazy/grid.rs`, `lazy/prefetch.rs`, `scroll.rs`, `gestures.rs` and `fling.rs`;
  - `crates/compose-ui/src`: `modifier.rs`, `layout.rs`, `layout_modifiers.rs`, `pointer_input.rs`, `nested_scroll.rs` and `semantics.rs`;
  - the tests in `crates/compose-foundation/tests/suite`, `crates/compose-ui/tests/suite` and `crates/compose-android/tests`.
- **How the counts were made:** `grep -cE '^\s*@Test\b'` per file.

### Totals

| | Files | `@Test` methods |
|---|---|---|
| Test files in scope | 208 | 3,129 |
| Files with tests (the table rows) | 185 | 3,129 |
| Helper files without tests (listed at the end) | 23 | 0 |

By area: `foundation-layout` 29 files (850 tests); lazy lists, grids and `LazyLayout` (lazy-tests and the host tests) 67 files (1,000 tests); staggered grids 20 files (317 tests); pager 25 files (239 tests); gestures, scrolling, overscroll, anchored drag, snapping and relocation 44 files (723 tests).

### Counts by status

| Status | Files | `@Test` methods |
|---|---|---|
| `covered` | 13 | 132 |
| `ported` | 47 | 733 |
| `port` | 1 | 1 |
| `partial` | 27 | 885 |
| `feature:<name>` | 87 | 1,245 |
| `n/a` | 10 | 133 |
| **Total** | **185** | **3,129** |

These counts are per file, by the file's status. Many `partial` and `port` files hold a few tests that need a missing feature, and those tests are listed in the file's row. `feature:` rows give the main feature first; a row naming several features needs them all.

## Test files

Statuses:

- `covered`: compose-rs's tests already exercise the file's scenarios. The row cites the compose-rs tests and says how fully.
- `ported`: Compose's tests are adapted as Rust tests, which the row cites.
- `port`: the feature exists in compose-rs, and the tests should be adapted.
- `partial`: some of the feature exists. The row says what's missing.
- `feature:<name>`: needs a missing feature, named in [Missing features](#missing-features).
- `n/a: <reason>`: adapting the file wouldn't make compose-rs more robust.

The table's suite references (`lazy_api.rs`, `fling.rs`, …) are files in `crates/compose-foundation/tests/suite/` unless a crate is named.

### foundation-layout

| Path | Tests | Covers | Status |
|---|---|---|---|
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AlignmentLineTest.kt | 14 | `paddingFrom` / `paddingFromBaseline` around alignment lines: offsets, too little space, min constraints | feature:alignment-lines |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/ArrangementTest.kt | 1 | Parameterized table of `Arrangement.arrange` outputs (Start/End/Center/Space*/spacedBy, LTR/RTL) | ported (`port_rows.rs`: its 4 cases) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/AspectRatioTest.kt | 6 | `aspectRatio` sizing (width/height first), zero/negative ratio throwing, intrinsics | partial (`aspect_ratio`, `aspect_ratio_with` exist; `modifiers_api.rs::aspect_ratio_sizes_from_the_width_or_the_height_first`; `testAspectRatioModifier_intrinsicDimensions` ported in `intrinsics.rs`. Missing: check invalid-ratio panics; inspector test n/a) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/BoxTest.kt | 14 | Box alignment, stretch children, RTL, `propagateMinConstraints` (and its changes), child sizes the box, intrinsics, `matchParentSize` | partial (Box, `content_alignment`, `propagate_min_constraints` exist; `foundation.rs::a_box_aligns_its_children`. `testBox_hasCorrectIntrinsicMeasurements` ported in `intrinsics.rs`, without its `matchParentSize` child. Missing: `matchParentSize` (feature:layout-modifiers), with `…withNoAlignedChildren`) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/BoxWithConstraintsTest.kt | 17 | Content composed with the measured constraints, relayout/recomposition interplay, model writes during measure, disposal, Dp accessors, infinity | partial (exists; `grid.rs::box_with_constraints_composes_for_its_constraints`, `review_round6.rs` box tests. compose-rs composes for a hint then again (docs/06), so tests counting steps (`allTheStepsCalledExactlyOnce`, `withConstraintCallbackIsNotExecutedWithInnerRecompositions`) may differ: see Notes) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/ContextualFlowRowColumnTest.kt | 77 | `ContextualFlowRow` / `ContextualFlowColumn` | n/a: the API is `@Deprecated` |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FitInsideTest.kt | 2 | `Modifier.fitInside(RectRulers)` | feature:ruler-fit |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FitOutsideTest.kt | 6 | `Modifier.fitOutside(RectRulers)` per side | feature:ruler-fit |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlexBoxDirectionTest.kt | 73 | FlexBox `justifyContent`, `alignItems`, gaps, wrap, reverse directions, grow/shrink/basis | feature:flexbox |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlexBoxTest.kt | 21 | FlexBox baselines, RTL, invalid grow/shrink, intrinsics, focus order | feature:flexbox (+ alignment-lines, intrinsics) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt | 128 | FlowRow/FlowColumn wrapping, maxItems, maxLines, weights, `fillMaxRowHeight`, arrangements, intrinsics (45), RTL | ported (`flow_layout.rs`, 94 grouped by scenario, D1350–D1353; the 6 `…MaxItemsIn…_MaxLines` intrinsic tests in their first, `Clip`, state only. Not ported: `testColumn_withCustomHorizontal_alignment`, `testColumn_withCustomHorizontalAlignModifier` and `testRow_withCustomVerticalAlignModifier`, custom `Alignment` objects, the ui ledger's alignment-api ([ui-core](ui-core.md)); 31 of the deprecated `overflow` (`…Visible`, `…SeeMore…`, `…Collapse…`, `…reuseMeasurePolicy`, `…measurePolicy_Identical`), n/a) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/GridTest.kt | 79 | The `Grid` layout: track sizes (fixed/%/fr/auto/min-max), gaps, spans, auto placement, areas, RTL | feature:grid-layout |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/IntrinsicTest.kt | 27 | `width/height(IntrinsicSize.Min/Max)`, `requiredWidth/Height(IntrinsicSize)` | ported (`intrinsics.rs`, all 27, grouped by scenario) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/LayoutReuseTest.kt | 9 | Box/Row/Column/Spacer/Flow layouts inside `ReusableContent` re-measure correctly | ported (`layout_reuse.rs`, all 9; 8 failed before D1354: a policy remembered anew measured again) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/OffsetTest.kt | 14 | `offset`, `absoluteOffset`, lambda offsets, RTL, no remeasure on offset change | partial (`offset(x, y)` exists, checked in `rtl.rs`. Missing: `absolute_offset`, `offset { IntOffset }` (feature:layout-modifiers); 4 inspector tests n/a) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt | 19 | padding overloads, negative padding throws, absolutePadding, RTL, intrinsics | partial (padding exists, `modifiers_api.rs::padding_and_size_take_each_overloads_arguments`, `rtl.rs`. `intrinsicMeasurements` ported in `intrinsics.rs`. Missing: `absolute_padding`, `PaddingValues.Absolute`, the negative-padding panic (feature:layout-modifiers); inspector tests n/a) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingValuesTest.kt | 4 | `PaddingValues` zero, `plus`, `minus` (non-negative) | partial (`PaddingValues` exists; `+`/`-` missing: feature:layout-modifiers) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnModifierTest.kt | 8 | Row/Column relayout when a child's align / alignBy / weight modifier changes | partial (align and weight exist; `alignBy` needs feature:alignment-lines) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt | 133 | Row/Column: weights (31), arrangements (49), alignments, rounding, RTL (19), alignBy (8), intrinsics (7), wrap/size interplay | partial (Row/Column/weight/align/arrangements exist; `foundation.rs` has 4 such tests. The 11 intrinsics tests are ported in `intrinsics.rs` (their `alignBy` variants wait on alignment lines). Missing: alignBy/alignByBaseline (feature:alignment-lines), `Arrangement.Absolute` (feature:layout-modifiers); about 100 tests portable now) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SizeTest.kt | 50 | size/width/height/sizeIn/required*/fillMax*/wrapContent*/defaultMinSize; constraints satisfiable or not; intrinsics (20) | partial (most exist; `compose-ui/tests/suite/ui.rs::size_modifiers_follow_compose`, `modifiers_api.rs`. The 19 intrinsics tests and `testModifiers_doNotCauseUnnecessaryRemeasure` are ported in `intrinsics.rs` (with `required_width`/`required_height`, D1203). Missing: `required_size_in/width_in/height_in` (feature:layout-modifiers)) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SpacerTest.kt | 6 | Spacer sized by size/width/height, inside smaller containers | ported (`port_rows.rs`: all 6, one table) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt | 8 | `Modifier.visible(false)`: not drawn, keeps space, not clickable or focusable | feature:layout-modifiers |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsControllerTest.kt | 11 | `imeNestedScroll`: scrolling and flinging the IME open or closed | feature:ime-nested-scroll (Android IME animation controller, D1361) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsDeviceTest.kt | 13 | Insets on a real device: consumption flag on the view, visibility at start, IME padding switching fields | partial (the listener exists, `ComposeRsInsets`, D1358. Run by hand on a Pixel 8 Pro, not as tests: `insetsSetAtStart`, `startVisibilityNavigationBarsVisible`, `startVisibilityStatusBarsVisible`, `startVisibilityImeNotVisible`. Not run: the rest, the consumption flag's, the re-attached view's and `imePadding_updatesCorrectly_whenFocusSwitchesBetweenBasicTextFields`: the keyboard probe was refused on the phone) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsIgnoringVisibilityTest.kt | 11 | `is*Visible`, `*IgnoringVisibility` insets | ported (`window_insets.rs`, 11 grouped in 2) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsListenerUnsetTest.kt | 1 | Insets update after the view's listener is unset during an animation | port (on a device, as WindowInsetsDeviceTest; `@Ignore`d in Compose) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsPaddingTest.kt | 50 | `*Padding` modifiers per inset kind, `consumeWindowInsets`, `recalculateWindowInsets`, IME animation | partial (`window_insets.rs`: 42 of 50, the host's insets standing for the device's. Not ported: the 6 `animateImeInsets*` and `disableConsuming`, the Android listener's, on a device as WindowInsetsDeviceTest; `consumeWindowInsets_movableContent_updateWhileUnattached`, feature:movable-content in the runtime. `assertIsConsumed` is the view's, not checked here) |
| compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/WindowInsetsSizeTest.kt | 16 | `windowInsetsTopHeight` / `BottomHeight` / `StartWidth` / `EndWidth` | ported (`window_insets.rs`, 16 grouped in 4) |
| compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt | 32 | Value insets: fixed, `union`, `exclude`, `add`, `only`, `MutableWindowInsets`, `asPaddingValues(density)`, consumption while unattached | ported (`window_insets.rs` and the unit tests of `layout/window_insets.rs`: 31 of 32; `insetsValues_toString` n/a, the class is internal) |

### Lazy lists (`integration-tests/lazy-tests`, package `lazy.list`)

| Path | Tests | Covers | Status |
|---|---|---|---|
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt | 29 | Main-axis arrangements of short lists, spacedBy, RTL, spacing in wrap content, after scrolls | ported (`lazy_list_arrangements.rs`: 9 tests, all 29; the reversed defaults pass `BOTTOM`/`END` (D171); the negative spacing larger than an item found D873) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnMouseHoverTest.kt | 4 | Hover follows content scrolled by the wheel under a resting mouse | ported (`port_rows.rs`: all 4) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt | 20 | Disposal when data changes or the list leaves, removals, alignment, a fling stopped by a press, nested LazyRow reuse, recomposition order | partial (`lazy_column_row.rs` and `lazy_list.rs`: 12 of 20. Left: `nestedLazyRowChildren…AreReused` (2): a reused node is always measured again (`UiTree::reuse` sets `needs_measure`, where Compose's `onReuse` only reschedules a pending measure; compose-ui's tree, D869), `scrolledAwayItemIsNotDisplayedAnymore` (a pixel capture), `scrollingDeactivatedListIsNotCrashing` (`SubcomposeLayout`: feature), the four `awaitFirstLayout…` (internal: n/a) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTrackpadHoverTest.kt | 8 | Hover during trackpad pans | feature:pointer-input-types (trackpad pan events, ui) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt | 21 | Keyed items keep state through removals, reorders and replacement; keys in `LazyListItemInfo` | ported (`lazy_custom_keys.rs`: 13 tests, all 21; keys compared hashed (D170); they found D867) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListAnimateScrollScope.kt | 1 | `animateScrollToItem` doesn't stop at a sticky header with the same index | ported (`lazy_scroll.rs::a_sticking_header_is_not_an_item_found_in_view`) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListBeyondBoundsAndExtraItemsTest.kt | 2 | Beyond-bounds items placed with `beyondBoundsItemCount` items | feature:beyond-bounds-api |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListBeyondBoundsItemCountTest.kt | 3 | The internal `beyondBoundsItemCount` parameter | n/a: internal parameter, not public API |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListBeyondBoundsTest.kt | 10 | `BeyondBoundsLayout.layout` requests through the modifier local: items laid out per direction | feature:beyond-bounds-api (compose-rs's own mechanism is focus-driven: `lazy.rs` tab tests) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt | 13 | A `LazyLayoutCacheWindow` set up to behave like the old one-item strategy | ported (`lazy_cache_window.rs`: all 13, `parity_*`, in both orientations) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt | 18 | Cache window: initial fill, small scrolls fill the window, kept items not disposed or recomposed, dataset changes | ported (`lazy_cache_window.rs`: 17, in both orientations, `smallScrollBackwardShouldFillEntireWindow` given the body its name asks for, Compose's scrolling nothing; `lookahead_sizeChange_cacheClearedCorrectly` is n/a with `isCacheWindowLookaheadCheckEnabled` off. its backward test found the negative initial offset compose-rs clamped; `force_remeasure` stands for `Remeasurement`, D896) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListFocusMoveCompositionCountTest.kt | 6 | A focus move beyond bounds composes a bounded number of items (various sizes, nesting) | covered (`review_d475.rs::tab_past_unfocusable_rows_finds_the_next_and_leaves_few_composed`, `review_d486.rs`, `lazy.rs::tab_lays_out_the_next_row…`; port the large and different-size cases) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListFocusMoveTest.kt | 9 | 2D and 1D focus moves among visible, just-beyond, far and nested items, with user scroll off | covered (`lazy.rs` tab and move-focus tests, `review_d475.rs`, `review_d479.rs`; the `userScrollEnabled = false` variants are worth porting) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListHeadersTest.kt | 16 | Sticky headers: shown, replaced, pushed, with pinned items, in the content padding, empty header | partial (`lazy_list_headers.rs`: 10 tests porting 13, the focus tests without their pinning, on an item in view. Left: the two `beyondBoundsItemCount = 1` tests (internal) and `withPinnedItem` (needs `LocalPinnableContainer`: feature:pinnable)) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemAppearanceAnimationTest.kt | 10 | `animateItem` fade-in | feature:animate-item |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemDisappearanceAnimationTest.kt | 12 | `animateItem` fade-out, container size during removal | feature:animate-item |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListItemPlacementAnimationTest.kt | 50 | `animateItem` placement on reorders and size changes | feature:animate-item |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt | 16 | `layoutInfo`: visible items, viewport offsets, padding, reverse, orientation, content type, observability | ported (`lazy_layout_info.rs`: 13 tests, all 16 in the four configurations; `snapshotFlow` replaced by a composable reading the info) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListMovableContentTest.kt | 2 | A list in `movableContentOf` keeps its position when moved | feature:movable-content (runtime) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListNestedPrefetchingTest.kt | 6 | Nested lists' items prefetched with the parent's, cancellation, constraints, automatic count | ported (`lazy_nested_prefetch.rs` for 4, the automatic count starting at 2 as no strategy of your own can ask 1; `overridingNestedPrefetchCountIsRespected` and `nestedPrefetchIsMeasuredWithProvidedConstraints` need a strategy of your own: n/a at the tip's window. Scrolled into the window, as it asks later than the one-item strategy) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListPinnableContainerTest.kt | 19 | `LocalPinnableContainer` pinning keeps items composed and placed | feature:pinnable (compose-rs pins the focused item internally: `review_lazy2.rs::a_focused_item_stays_pinned…`) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListPrefetchPrecedenceTest.kt | 4 | The default is the cache window; the flag-off fallback; an explicit window or strategy wins | ported (`lazy_cache_window.rs`: 2; the other 2 are n/a, the fallback flag and the deprecated strategy not being ported, D892) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListPrefetchStrategyTest.kt | 7 | Callbacks of a custom `LazyListPrefetchStrategy` | n/a: `LazyListPrefetchStrategy` is `@Deprecated` at tip |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListPrefetcherTest.kt | 13 | The one-item-ahead prefetch: forward/backward, twice, reverse, padding, sticky header, cancellation | covered (`prefetch_parity.rs`, `prefetch.rs`, at the default window now. At tip this file pins the deprecated strategy, and the cache-window rows supersede it: see Tip differences) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt | 9 | `requestScrollToItem` across data changes | ported (`lazy_request_scroll.rs`: 9 tests; `performScrollToIndex` as `scroll_to_item` until feature:lazy-semantics) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt | 13 | `scrollIndicatorState` offsets and sizes | feature:scroll-indicator-state |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListSlotsReuseTest.kt | 10 | Scrolled-off items kept for reuse (max per type), reuse on scroll back, content types | covered (`lazy_reuse.rs`: 19 tests, including up to seven per type, content types, own slot on scroll back) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt | 94 | The list at large: items, empty/nullable, scrolling, wrap content, fill parent, state restore, nested lists, recomposition counts, RTL, overscroll, keys | partial (62 of 94 ported, `lazy_grid.rs` holding `zIndexOnItemAffectsDrawingOrder` in `tests/suite/lazy_list.rs`, both orientations, each citing its test; they found D863 and D864. Left: the lookahead and placement-animator tests (feature:lookahead, feature:animate-item), `beyondBoundsItemCount` (internal: drop), custom overscroll (feature:overscroll), `semanticsScroll_isAnimated` (D491: not animated), the pixel captures (`zIndex…`, `doesNotClipHorizontalOverdraw`), state restoration, `Modifier.composed`, the nested-fling cancellation test (needs a paused clock driving a fling while another swipe runs), negative counts (`usize`: n/a)) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt | 23 | Content padding: applied, scroll position, items in the padding, to the end, reversed, larger than the parent | ported (`lazy_list_content_padding.rs`: 19 tests, all 23 in both orientations; `overscrollWithContentPadding` is a scroll past the end, no overscroll effect. They found D865) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsIndexedTest.kt | 6 | `itemsIndexed` passes indices and items | covered (`lazy_api.rs::items_over_a_list_get_each_item_and_its_index`) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt | 17 | Reverse layout of columns and rows (RTL too): positions, initial scroll, scroll directions | ported (`lazy_list_reverse_layout.rs`: 7 tests, all 17; they found D866) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyNestedScrollingTest.kt | 9 | Nested scroll dispatch from a list at its edges; wheel in a row inside a column | covered (`review_nested_scroll.rs`, `review_d479.rs`, `review_lazy.rs::the_wheel_delta_a_nested_lazy_list_leaves_scrolls_its_parent`) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyRowTest.kt | 5 | LazyRow vertical alignment, scrolling and layout in RTL | ported (`lazy_column_row.rs`: 5 tests, all 5) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollAccessibilityTest.kt | 9 | Android accessibility scroll actions forward/back/up/down/left/right at the start, middle and end | covered (`compose-android/tests/accessibility_scroll.rs`, `lazy.rs::a_lazy_list_has_scroll_semantics_and_scrolls_by_its_action`) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt | 35 | `scrollToItem`, `animateScrollBy`, `animateScrollToItem`, offsets past the bounds, `canScroll*`, `lastScrolled*` | ported (`lazy_scroll.rs`: 13 tests, all 35 in both orientations; the per-frame spring checked against `animate` run alongside) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt | 8 | Item semantics, `CollectionInfo`, `ScrollToIndex` | feature:lazy-semantics |

### Lazy grids (`lazy.grid`)

| Path | Tests | Covers | Status |
|---|---|---|---|
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyArrangementsTest.kt | 33 | Grid main-axis arrangements, spacing, RTL | ported (`lazy_grid_arrangements.rs`: all 33, both orientations, the arrangement across too) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyCustomKeysTest.kt | 21 | Keyed grid items keep state through edits | covered (`review_round6.rs::a_keyed_grid_keeps_each_items_state_by_its_key`, `lazy_grid_reuse.rs` reorder and insert tests; port the remaining edit cases) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridBeyondBoundsTest.kt | 10 | `BeyondBoundsLayout` requests on grids | feature:beyond-bounds-api |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt | 12 | Grid cache window configured like the old strategy | ported (`lazy_cache_window.rs`: 11, `grid_parity_*`, in both orientations; `prefetchingForwardAndBackwardReverseLayout` waits for grids' `reverseLayout`, feature:grid-parity) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt | 8 | Grid cache window fill and keep | ported (`lazy_cache_window.rs`: all 8, in both orientations) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt | 14 | `LazyGridScope.stickyHeader` | partial (`lazy_grid_headers.rs`: 13 of 14, the focus tests without their pinning, on an item in view. Left: `lazyVerticalGridwithPinnedItem`: feature:pinnable) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemAppearanceAnimationTest.kt | 10 | Grid `animateItem` fade-in | feature:animate-item |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemDisappearanceAnimationTest.kt | 13 | Grid `animateItem` fade-out | feature:animate-item |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt | 51 | Grid `animateItem` placement | feature:animate-item |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt | 17 | Grid `layoutInfo` (rows/columns/spans, viewport) | ported (`lazy_grid_layout_info.rs`: all 17, the four configurations) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridNestedPrefetchingTest.kt | 6 | Nested prefetch from grid items | ported (`lazy_nested_prefetch.rs` for 4, D1461; `overridingNestedPrefetchCountIsRespected` and `nestedPrefetchIsMeasuredWithProvidedConstraints` need a strategy of your own: n/a at the tip's window. As the list's: the rest of those in view compose in a pass after the measurement short of them, where Compose's measurement composes them) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridPinnableContainerTest.kt | 13 | Pinning grid items | feature:pinnable |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridPrefetchPrecedenceTest.kt | 4 | Grid default is the cache window | ported (`lazy_cache_window.rs`: 2; the other 2 are n/a, as the list's) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridPrefetchStrategyTest.kt | 6 | Custom `LazyGridPrefetchStrategy` callbacks | n/a: `@Deprecated` at tip |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridPrefetcherTest.kt | 12 | Grid line prefetch (1.12.1's default strategy) | covered (`prefetch.rs::a_grid_prefetches_its_next_rows`, `prefetch_parity.rs::a_grid_prefetches_the_next_line…`, `review_grid_d450.rs`, at the default window now; deprecated at tip, as the list's) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt | 9 | `requestScrollToItem` on grids | ported (`lazy_request_scroll.rs`: all 9) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt | 11 | Grid `scrollIndicatorState` | feature:scroll-indicator-state (+ grid-parity for the sticky case) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSlotsReuseTest.kt | 10 | Grid slot reuse | covered (`lazy_grid_reuse.rs`: 13 tests) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt | 16 | Spans, line breaks, horizontal spacing, `maxLineSpan` per cell kind | covered (`lazy_grid_reuse.rs::spans_lay_items_out_in_lines_as_compose`, `review_grid_d450.rs` span tests; `lazy_grid.rs`: all of the `crossAxisCellCount` tests, the count of compositions less the first, an item knowing the cells from an estimate until measured (D1451), and `spansCalculationDoesntCrash`) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt | 60 | Grids at large: fixed/adaptive/fixed-size cells, spacing, scrolling, RTL, state, wrap content | partial (`lazy_grid.rs`: 40 of 60, both orientations, with `grid.rs`, `review_grid_d450.rs`, `review_grid_d455.rs`. Left: 16 of lookahead, `customOverscroll` (feature:overscroll), `onlyOneInitialMeasurePass` (rounds, D125), `passingNegativeItemsCountIsNotAllowed` (a `usize`), `recomposingWithNewComposedModifierObjectIsNotCausingRemeasure`: a grid recomposed at its call site has new items, a closure not comparing as Compose compares its content lambda) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt | 24 | Grid content padding | ported (`lazy_grid_content_padding.rs`: 13 tests porting all 24; they found D865 in grids too) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsIndexedTest.kt | 2 | Indexed grid items | covered (`lazy_grid_reuse.rs::the_list_and_indexed_forms_take_keys_spans_and_content_types`) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsReverseLayoutTest.kt | 7 | Grid `reverseLayout` | ported (`lazy_grid_arrangements.rs`: all 7) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyNestedScrollingTest.kt | 4 | Nested scroll from a grid at its edges | ported (`lazy_grid.rs`: all 4, both orientations; `review_d479.rs::a_nested_grid_passing_its_estimated_start_offers_the_rest`) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollAccessibilityTest.kt | 9 | Accessibility scroll actions on grids | ported (`compose-android/tests/accessibility_scroll.rs`: all 9, the eight configurations) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt | 34 | Grid `scrollToItem` / `animateScrollToItem` / `animateScrollBy` | ported (`lazy_grid_scroll.rs`: 11 tests, all 34) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazySemanticsTest.kt | 2 | Grid item semantics (`ScrollToIndex`, collection info) | feature:lazy-semantics |

### LazyLayout (`lazy.layout`, and the host tests)

| Path | Tests | Covers | Status |
|---|---|---|---|
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStateRestorationTest.kt | 6 | Saveable state of items (also nested, custom keys, reorders) restored | ported (`lazy_state_restoration.rs`: all 6, on `LazyColumn`, as `LazyLayout` isn't public: lazy-layout-public) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutTest.kt | 19 | The public `LazyLayout`: item provider, measure scope, `LazyLayoutPrefetchState` scheduling and cancelling, approach | feature:lazy-layout-public |
| compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/lazy/MutableIntervalListTest.kt | 19 | `MutableIntervalList` add and search | feature:lazy-layout-public (compose-rs's private `Items` interval search could take an adapted copy now) |
| compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/lazy/layout/CacheWindowLogicTest.kt | 1 | Lane resize resets the cache-window strategy | ported (`cache_window.rs`, `a_change_of_lanes_starts_the_window_over`) |

### Lazy staggered grids (`lazy.staggeredgrid`)

All 20 files need `feature:staggered-grid`; the animation files also need `animate-item`, the cache window file `cache-window`, and `LazyStaggeredGridInLookaheadTest` needs ui's lookahead.

| Path | Tests | Covers | Status |
|---|---|---|---|
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt | 19 | `animateScrollBy` / `animateScrollToItem` | feature:staggered-grid (+ lazy-state-api) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridArrangementsTest.kt | 7 | Spacing both ways, negative spacing | feature:staggered-grid |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridBeyondBoundsTest.kt | 11 | Beyond-bounds layout | feature:staggered-grid (+ beyond-bounds-api) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt | 15 | Per-lane cache window | feature:staggered-grid (+ cache-window) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt | 11 | Content padding | feature:staggered-grid |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt | 21 | Keys keep state | feature:staggered-grid |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridInLookaheadTest.kt | 13 | Positions under lookahead | feature:staggered-grid (+ ui lookahead) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemAppearanceAnimationTest.kt | 10 | `animateItem` fade-in | feature:staggered-grid (+ animate-item) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemDisappearanceAnimationTest.kt | 12 | `animateItem` fade-out | feature:staggered-grid (+ animate-item) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt | 49 | `animateItem` placement | feature:staggered-grid (+ animate-item) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt | 10 | Lane bookkeeping (internal `LazyStaggeredGridLaneInfo`) | feature:staggered-grid (an internal class: adapt to compose-rs's own lane store) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLayoutInfoTest.kt | 6 | `layoutInfo` | feature:staggered-grid (+ lazy-layout-info) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt | 13 | Pinning | feature:staggered-grid (+ pinnable) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPrefetcherTest.kt | 15 | Prefetch | feature:staggered-grid |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt | 9 | `requestScrollToItem` | feature:staggered-grid (+ lazy-state-api) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt | 12 | Reverse layout | feature:staggered-grid |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt | 9 | `scrollIndicatorState` | feature:staggered-grid (+ scroll-indicator-state) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt | 18 | `scrollToItem` in and out of bounds, lanes | feature:staggered-grid |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridSemanticTest.kt | 2 | Item semantics | feature:staggered-grid (+ lazy-semantics) |
| compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt | 55 | Distribution across lanes, spans (`FullLine`), scrolling, sizes | feature:staggered-grid |

### Pager (`pager`)

All need `feature:pager`. Pager prefetch already uses a cache window in 1.12.1 (`isCacheWindowForPagerEnabled`).

| Path | Tests | Covers | Status |
|---|---|---|---|
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PageCountTest.kt | 5 | Zero pages, `Int.MAX_VALUE` pages | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PageLayoutPositionOnScrollingTest.kt | 2 | Page positions while swiping and dragging | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PageSizeTest.kt | 3 | `PageSize.Fill` / custom / 0 dp | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerAccessibilityTest.kt | 21 | Accessibility paging and scroll actions | feature:pager (+ lazy-semantics) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCacheWindowTest.kt | 7 | Pager cache window | feature:pager (+ cache-window) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerContentPaddingTest.kt | 23 | Content padding | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerContentTest.kt | 3 | Outer and inner modifiers; scrollable while changing pages | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCrossAxisTest.kt | 1 | Wraps content across an infinite cross axis | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerCustomKeyTest.kt | 1 | The key lambda is called once | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerGestureTest.kt | 2 | A low-velocity swipe bounces back; small content | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerLayoutInfoTest.kt | 12 | `layoutInfo.visiblePagesInfo` | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerLazyLayoutScrollScopeTest.kt | 1 | `LazyLayoutScrollScope.calculateDistanceTo` capped | feature:pager (+ lazy-state-api) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedPrefetchingTest.kt | 3 | Nested prefetch in pages | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerNestedScrollContentTest.kt | 10 | Nested scrollables in pages: flings, edges, cross axis | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerOffscreenPageLimitPlacingTest.kt | 4 | `beyondViewportPageCount` placement | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPinnableContainerTest.kt | 13 | Pinning pages | feature:pager (+ pinnable) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerPrefetcherTest.kt | 15 | Page prefetch | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerRequestScrollTest.kt | 8 | `requestScrollToPage` | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt | 10 | `scrollIndicatorState` | feature:pager (+ scroll-indicator-state) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollingTest.kt | 19 | Positional and velocity thresholds, `PagerSnapDistance`, RTL | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerSnapPositionTest.kt | 2 | `snapPosition` vs the max scroll | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateNonGestureScrollingTest.kt | 24 | `PagerState` before attach, `scrollToPage`, offset fractions, restoration | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerStateTest.kt | 24 | `scrollToPage` / `animateScrollToPage` / `currentPage` / `targetPage` / `settledPage` | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerSwipeEdgeTest.kt | 3 | Swiping at the edges | feature:pager |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerTest.kt | 23 | `userScrollEnabled`, mouse wheel, page count, keys, sizes, RTL | feature:pager |

### Gestures, scrolling, overscroll, anchored drag, snapping, relocation

| Path | Tests | Covers | Status |
|---|---|---|---|
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt | 29 | `draggable2D` and `Draggable2DState` | ported (`draggable_2d.rs`, with `draggable.rs` for the node they share: D1458. Left: `gestureState`'s two tests read Compose's internals; the inspector's and `equalInputs`) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableInteropTest.kt | 1 | Velocity is right while a hosting `ComposeView` translates | n/a: Android View-hierarchy interop; compose-rs takes window coordinates from its host view |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt | 71 | `draggable`: axes, start/stop, velocity, reverse, enabled, interaction source, `startDragImmediately`, nested drags, indirect pointer, newest callbacks | partial (`draggable.rs`, 49 tests porting 52 of its 71, on the port of `DragGestureNode` (D1458). Left: the 14 indirect-pointer tests (feature:pointer-input-types), `gestureState`'s two reading Compose's internals, the inspector's, the modifier's equality and a mouse's secondary button (events here don't say which button)) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/LazyListFocusableInteractionTest.kt | 19 | A focused item stays in view when the list's viewport shrinks | partial (focus brings items into view: `review_rtl.rs`, `lazy_api.rs::a_reversed_list_brings_a_focused_item_into_view`; keeping the focused item in view on a viewport resize is missing (feature:keep-focused-in-view); AndroidView cases n/a) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/MouseWheelScrollable2DTest.kt | 2 | Wheel on `scrollable2D`, velocity | feature:scrollable2d |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/OverscrollTest.kt | 40 | `Modifier.overscroll`, `rememberOverscrollEffect`, `LocalOverscrollFactory`, overscroll on scrollables (drag, fling, reverse, nested) | partial (`overscroll.rs`: the 23 on events and the effect's remembering. Left: `rememberOverscrollEffect_defaultValue`, as the default is no effect until the stretch; 7 stretch and 2 glow drawing tests, `zeroSizedEffect`, `notAttachedEffect` and `testOverscrollModifierDrawsOnce` on the real effect, with Android's stretch (D1464); `modifierUndelegatesNodeOnDetach`, `modifierDoesNotAddAlreadyAttachedNode` and `modifierIsProducingEquals` have no meaning for a `modifier()` (D1464); the inspector test is n/a) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollAccessibilityTest.kt | 9 | Accessibility scroll actions on `verticalScroll`/`horizontalScroll` | covered (`compose-android/tests/accessibility_scroll.rs`, `lazy.rs::a_scrolling_column_has_scroll_semantics_and_scrolls_by_its_action`) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollFocusableInteractionTest.kt | 19 | A focused child of a scroll stays in view on resize | partial (as LazyListFocusableInteractionTest: feature:keep-focused-in-view) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt | 42 | `verticalScroll`/`horizontalScroll`: small/large content, `reverseScrolling`, `scrollTo`, `animateScrollTo`, swipes, coercion, RTL, `isScrollInProgress`, nested | partial (`scroll_state.rs`: 12 tests porting 21 (content fitting or not, scroll to the end, reversed, `canScroll*`, viewport size, coercion of `scrollTo`, `animateScrollTo` and a shrinking range, a fling stopped by a touch, semantics enabled or not, swipes each way, the maximum known before placement), in the three configurations; `onMaxValueUpdate_shouldNotGenerateExtraMeasurements` fails (D870: a scroll measures the scroller's node again); `scroll.rs`, `rtl.rs`. Missing: `performScrollTo` (harness), state restoration, overscroll, `scrollIndicatorState` (feature:scroll-indicator-state), inspector tests n/a, the measure-count and resize-before-placement tests; the five intrinsics tests are ported in `intrinsics.rs`) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Scrollable2DInputTest.kt | 22 | `scrollable2D` input: drag, fling, nested | feature:scrollable2d |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Scrollable2DTest.kt | 15 | `scrollable2D` semantics, animations, `isScrollInProgress` | feature:scrollable2d |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableAreaTest.kt | 13 | `Modifier.scrollableArea` clip, direction, overscroll node | feature:scrollable (+ overscroll) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt | 87 | `Modifier.scrollable`: drags, mouse wheel, trackpad, keyboard paging, flings, nested scroll, interaction source, `isScrollInProgress`, `MutatePriority` | partial (`scrollable.rs`, `scrollable_nested.rs`, `mouse_wheel.rs`, `nested_fling.rs`, `review_scrolling.rs`, porting 59 of its tests. Left, 28: keys and semantics through the connections around and `NestedScrollDispatcher`'s (7, feature:nested-scroll-dispatcher, D1459), trackpad and indirect pointers (13, feature:pointer-input-types), a horizontal velocity and the reversed scope (repeating what's there), `offsetsScrollable_velocityCalculationShouldConsiderLocalPositions`, a closer focus outside, disabled semantics, the inspector's and the two of equality) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/StretchOverscrollIntegrationTest.kt | 24 | Android stretch overscroll consumes pulls and flings | feature:overscroll |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/StretchOverscrollScreenshotTest.kt | 4 | Stretch rendering per edge | feature:overscroll (screenshots) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/SystemGestureExclusionTest.kt | 7 | `systemGestureExclusion` rects | feature:system-gesture-exclusion |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/TouchScrollable2DTest.kt | 5 | Touch on `scrollable2D`: interaction source, fling scope, velocity | feature:scrollable2d |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/TrackpadScrollable2DTest.kt | 3 | Trackpad on `scrollable2D` | feature:scrollable2d (+ pointer-input-types) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/TransformableTest.kt | 33 | `transformable`: zoom, pan, rotate, locks, `canPan`, in scrollables, trackpad, `animate*By` | feature:transformable |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/focus/LazyListFocusableInteractionTest.kt | 1 | Scrolling a column up and down with a focused item doesn't crash | ported (`port_rows.rs`: a focusable item, where its is an `AndroidView`) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableDecayAnimationTest.kt | 1 | `animateToWithDecay` | ported (`anchored_draggable.rs`) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt | 18 | Swipes settle by positional and velocity thresholds, disabled, nested | ported (`anchored_draggable.rs`) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableOverscrollTest.kt | 7 | Overscroll gets out-of-bounds deltas and velocity | ported (`overscroll.rs`) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt | 45 | `AnchoredDraggableState`: target, current, settled, progress, `updateAnchors`, `snapTo`/`animateTo`, `confirmValueChange`, saver | ported (`anchored_draggable.rs`) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/AwaitEachGestureTest.kt | 3 | `awaitEachGesture` loops and cancellation | feature:gesture-detectors |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/AwaitLongPressOrCancellationTest.kt | 9 | `awaitLongPressOrCancellation` | feature:gesture-detectors |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/DragGestureDetectorTest.kt | 14 | Drag detectors: normal, cross-axis, two fingers, cancel during and after the slop, priority lock, back and forth, callback order | partial (`detect_drag_gestures` exists: `pointer_input.rs::drags_start_past_the_slop_and_report_their_amounts`, `review_round7.rs`. Missing: horizontal/vertical/orientation-locked/after-long-press variants and `drag`/`awaitDragOrCancellation` (feature:gesture-detectors); multi-pointer events (ui)) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/DragGestureDetectorWhileMovingUIToPopupTest.kt | 2 | A drag start that moves content into a popup doesn't crash | ported (`port_rows.rs`: the Compose-content case, composed anew in the popup as there's no `movableContentOf`; the `AndroidView` case is n/a) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/ForEachGestureTest.kt | 3 | `forEachGesture` | n/a: `forEachGesture` is `@Deprecated` (AwaitEachGestureTest covers its replacement) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/TapGestureDetectorTest.kt | 27 | `detectTapGestures`: tap, double tap, long press, misses, out and in, consumption, `PressGestureScope` | partial (`detect_tap_gestures` exists: `pointer_input.rs::taps_double_taps_and_long_presses_are_told_apart`, `review_round7.rs`. Missing: `PressGestureScope` (`awaitRelease`, `tryAwaitRelease`), `awaitFirstDown(pass)` (feature:gesture-detectors)) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/TransformGestureDetectorTest.kt | 9 | `detectTransformGestures`: pan, zoom, rotation, lock, slop | feature:transformable |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyGridSnapFlingBehaviorTest.kt | 8 | Snap fling on grids by velocity and distance | partial (`snapping.rs`: its 5 of velocity and distance, on `FixedSize` cells. Left: the edges' velocity and the remaining offset, as the list's, tested there) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyGridSnapLayoutInfoProviderTest.kt | 3 | Grid `SnapLayoutInfoProvider.calculateSnapOffset` | ported (`snapping.rs`: all 3, one test) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapFlingBehaviorTest.kt | 9 | Snap fling on lists | partial (`snapping.rs`: 8 of 9. Left: `remainingScrollOffset_shouldFollowAnimationOffsets`, covered by `SnapFlingBehaviorTest`'s) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/LazyListSnapLayoutInfoProviderTest.kt | 3 | List `SnapLayoutInfoProvider` | ported (`snapping.rs`: all 3, one test, with a header and without) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehaviorTest.kt | 17 | `snapFlingBehavior` approach and snap phases, remaining offset, velocity | partial (`snapping.rs`: 15 of 17. Left: `findClosestOffset`'s 2, of the test's own provider) |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/relocation/BringIntoViewRequesterViewIntegrationTest.kt | 4 | `bringIntoView` reaching Android's `requestRectangleOnScreen` | n/a: Android View interop |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/relocation/BringIntoViewResponderTest.kt | 20 | `BringIntoViewRequester` propagating up through responders, interrupting requests | feature:bring-into-view |
| compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/relocation/BringIntoViewScrollableInteractionTest.kt | 24 | Scrollables scroll requested rects into view (leading, trailing, center, bigger than the parent) | feature:bring-into-view (compose-rs scrolls a focused node into view, `ModifierNode::bring_into_view`: some cases can be checked through focus now) |
| compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt | 8 | `MutatorMutex` priorities, `tryMutate`, `tryLock` | ported (`mutator_mutex.rs`: 7 tests for the 8; `tryLock`/`unlock` are internal, checked through `try_mutate`; an interrupted mutation returns `Err(MutationInterrupted)`, D850) |
| compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/gestures/BringIntoViewRequestPriorityQueueTest.kt | 15 | The internal request queue of `ContentInViewNode` | n/a: internal class (port its rules with feature:bring-into-view if compose-rs grows the same queue) |
| compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/gestures/DraggableAnchorsTest.kt | 11 | `DraggableAnchors` builder, closest anchor, min/max, equality | ported (`anchored_draggable.rs`; `equality_differentObject` compares with a string, which Rust doesn't compile) |
| compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/gestures/DraggableIsAnyChangedToDownTest.kt | 10 | The internal `isAnyChangedToDown` helper | n/a: internal helper |
| compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/gestures/UpdatableAnimationStateTest.kt | 7 | The internal animation state behind bring-into-view | n/a: internal class |

### Helper files without tests (23)

`compose/foundation/foundation-layout/…/layout/`: `LayoutTest.kt`, `TestActivity.kt`, `WindowInsetsActivity.kt`. `integration-tests/lazy-tests/…/foundation/`: `AutoTestFrameClock.kt`, `BaseLazyLayoutTestWithOrientation.kt`, `ScrollableUtils.kt`, `lazy/BeyondBoundsTestUtils.kt`, `lazy/grid/BaseLazyGridTestWithOrientation.kt`, `lazy/layout/TestPrefetchScheduler.kt`, `lazy/list/BaseLazyListTestWithOrientation.kt`, `lazy/staggeredgrid/BaseLazyStaggeredGridWithOrientation.kt`. `foundation/src/androidDeviceTest/…/foundation/`: `AbstractScrollable2DTest.kt`, `BaseLazyLayoutTestWithOrientation.kt`, `anchoredDraggable/AnchoredDragScopeAsScrollScope.kt`, `anchoredDraggable/AnchoredDraggableBackwardsCompatibleTest.kt`, `anchoredDraggable/AnchoredDraggableTestValue.kt`, `lazy/grid/BaseLazyGridTestWithOrientation.kt`, `lazy/list/BaseLazyListTestWithOrientation.kt`, `lazy/list/PlacedUtils.kt`, `pager/BasePagerTest.kt`, `pager/SingleParamBasePagerTest.kt`, `pager/TestPrefetchScheduler.kt`, `relocation/FakeScrollable.kt`. The base classes hold the orientation-aware position assertions the tests use (`assertMainAxisStartPositionInRootIsEqualTo` and friends): see the notes.

## Tip differences to adopt

The user's direction: compose-rs's parity mode follows androidx-main's tip, not 1.12.1, and the Compose benchmark app will move to tip-of-tree Compose. Items 1 to 4 are adopted (D892 to D899: `crates/compose-foundation/src/lazy/cache_window.rs`, `prefetch.rs`), sticky headers and the stale bound as the tip's (D895, D896); until then compose-rs mirrored 1.12.1's lazy prefetch, which the benchmark app still runs (docs/15, "Compose's defaults (version 1.12.1)" (a), (b), (f)). The differences below are from a diff of foundation 1.12.1's sources jar, from the local Gradle cache (`foundation-android-1.12.1-sources.jar`), against the tip. Paths are under `compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/`.

1. **Lists and grids prefetch with a cache window by default, not the one-item strategy.**
   - At tip, `LazyColumn`/`LazyRow` take `cacheWindow: LazyLayoutCacheWindow = LazyListDefaults.cacheWindow(state)` (`lazy/LazyDsl.kt:349`, `:481`).
   - `LazyList` uses `LazyListCacheWindowStrategy(cacheWindow)` when the window is the default and `ComposeFoundationFlags.isPreferDefaultCacheWindowOverPrefetchStrategyLazyList` is true. That is its default (`lazy/LazyList.kt:105-120`, `ComposeFoundationFlags.kt:290`).
   - The old `LazyListPrefetchStrategy()` remains only when the state was built with the now-deprecated `LazyListState(prefetchStrategy = …)`, or when the flag is off.
   - Grids do the same (`isPreferDefaultCacheWindowOverPrefetchStrategyLazyGrid`, `:300`; `lazy/grid/LazyGridDefaults.kt`).
   - Staggered grids use a cache window too (`isUsingCacheWindowInStaggeredGrids`, `:113`).
   - Tests that pin it: `LazyListPrefetchPrecedenceTest.usesDefaultLayoutCacheWindow` and `usesDefaultPrefetchStrategyWhenFeatureFlagDisabled`, and the grid's `LazyGridPrefetchPrecedenceTest`.
2. **The default window is dynamic.** `DefaultLazyLayoutCacheWindow` (`lazy/layout/LazyLayoutCacheWindow.kt:164-187`, with `isUsingDynamicDefaultCacheWindowInLists`/`InGrids`/`InStaggeredGrids` true, `ComposeFoundationFlags.kt:325-353`):
   - **Ahead:** the visible items' average size plus the spacing (`LazyListLayoutInfo.visibleItemsAverageSize`, `lazy/LazyListLayoutInfo.kt:90-95`), coerced into 10 % to 50 % of the viewport.
   - **Behind:** 0.
   - **No non-scroll caching:** `isNonScrollCachingEnabled = false`, so nothing is prefetched after the first layout at rest.
   - With the flag off, the ahead window is a fixed 10 % of the viewport.
   - So small items prefetch several ahead (10 % of the viewport's worth), while 1.12.1 prefetched one.
3. **The window's rules differ from the strategy's** (`lazy/layout/CacheWindowLogic.kt`, `MultiLaneCacheWindow` since `isMultiLaneCacheWindowEnabled = true`, `ComposeFoundationFlags.kt:103`):
   - **Filling:** a scroll grows the ahead window's remaining space by `|delta|`, up to the window less the last visible item's overflow past the viewport. Items are requested one at a time, the next scheduled when the previous request finishes (`scheduleNextItemIfNeeded`, `:751-806`), until the window is filled.
   - **Keeping:** items already prefetched inside the window stay. They aren't cancelled for not being "the next one", as 1.12.1's `onVisibleItemsUpdated` cancelled them.
   - **Turning:** a direction change resets the window from the visible edge (`changedScrollDirection`, in `onPrefetchForward` from `:454`).
   - **Releasing:** items behind the (zero) behind window are cancelled or removed (`onKeepAround`, `removeOutOfBoundsItems`, `:571-740`).
   - **Urgency:** a request is urgent when it's for the item right after the last visible one and `|delta| >= ` that item's lane's overflow past the viewport end (`:490-496`). 1.12.1 required `lastItem.offset + size + spacing - viewportEndOffset < -delta`: the spacing and the strict comparison differ.
   - **Dataset changes:** a change of item count or size, or of a visible item's key, refills or resets the ahead bounds (`onVisibleItemsUpdated`, `:188-286`).
   - Pinned by `LazyListCacheWindowTest` (18), `LazyGridCacheWindowTest` (8), `LazyStaggeredGridCacheWindowTest` (15), `CacheWindowLogicTest` (1) and `PagerCacheWindowTest` (7). `LazyListCacheWindowParityTest` and `LazyGridCacheWindowParityTest` pin a window configured to mimic the old strategy.
4. **Pausable composition in prefetch is always on.** The flag `isPausableCompositionInPrefetchEnabled` is gone, and so is the non-pausable path (`performFullComposition` and `compositionTimeNanos`): compare `lazy/layout/LazyLayoutPrefetchState.kt:617-624` with 1.12.1's `:623-641`. compose-rs's flag and its whole compositions went with it (D892).
5. **Nested prefetch** is unchanged in substance: the count comes from `realizedNestedPrefetchCount`, else 2 (`DefaultNestedPrefetchCount`, `lazy/LazyListCacheWindowStrategy.kt:157`), as compose-rs does now (D599). `LazyListNestedPrefetchingTest.automaticNestedPrefetchingBasedOnNumberOfVisibleItems` pins it.
6. **APIs that changed status.** These are now `@Deprecated`:
   - `LazyListPrefetchStrategy`, `LazyGridPrefetchStrategy` and their scopes;
   - `LazyListState(prefetchStrategy)` and `LazyListState(cacheWindow, …)`;
   - `rememberLazyListState(prefetchStrategy | cacheWindow)`;
   - `PrefetchScheduler`, `PrefetchRequest` and `PrefetchRequestScope`.

   These are now public, non-experimental API:
   - `LazyLayoutCacheWindow` with its two factories;
   - `LazyListDefaults.cacheWindow`, `LazyGridDefaults.cacheWindow` and `LazyStaggeredGridDefaults.cacheWindow`;
   - `LazyLayoutPrefetchState()`, `schedulePrecomposition` and `schedulePrecompositionAndPremeasure`.

   compose-rs's `PrefetchScheduler`/`PrefetchRequest` (`compose_ui::platform`, docs/06) are its own host plumbing, mirroring a now-deprecated API: fine as plumbing.
7. **Draggable velocity.** `isDraggableVelocityTrackerFixEnabled` was `false` in 1.12.1 and is `true` at tip (`ComposeFoundationFlags.kt:228`, `gestures/Draggable.kt`). It tracks velocity per pointer and adds the events that don't move at the gesture's start. `isDraggableInitialPassConsumptionFixEnabled` (`:206`: a drag that starts immediately consumes in the initial pass) and `isDraggableZeroDeltaConsumptionEnabled` (`:258`: zero-delta moves consumed once a drag started) are new and on. compose-rs's draggable and scroll velocity (`crates/compose-foundation/src/fling.rs`) should be checked against these. DraggableTest's velocity tests pin them.
8. **Lazy measurement** itself (`lazy/LazyListMeasure.kt`) changed only around the item animator (`isLazyListItemAnimatorVisibleBoundsFixEnabled`, item animations always run through `onMeasured`) and in passing the prefetch state. Content padding, sticky headers and the scroll consumption that compose-rs mirrors (D649 and the rows it revises) are unchanged.

The benchmarks (docs/12, docs/15, docs/16): rounds from D892 on compare compose-rs at tip against the Kotlin app's Compose 1.12.1, until the app moves to tip-of-tree (D705). What parity mode prefetches changed (items 1 to 4), so the device's numbers need taking again. compose-rs's own `PREFETCH_TWO_AHEAD`, `KEEP_ITEMS_BEHIND` and `PREFETCH_AT_REST` are parameters of the default window now, and leave a window the app gives alone (D893).

## Public API compose-rs lacks

From `compose/foundation/foundation-layout/api/current.txt` and `compose/foundation/foundation/api/current.txt`, non-deprecated, experimental included (marked *exp*). `restricted_current.txt` adds nothing public. Grouped by the feature that would add them; compose-rs names follow Compose's in snake_case.

- **layout-modifiers:**
  - `requiredSize(w, h)`, `requiredSizeIn`, `requiredWidthIn`, `requiredHeightIn` (`required_width`/`required_height` came with intrinsics, D1203);
  - `absolutePadding`, `absoluteOffset(x, y)`, `offset { IntOffset }` and `absoluteOffset { IntOffset }`;
  - after the placement pass's coordinates (PL-2), `offset { }`, the scroll modifier's placing (`scroll.rs`, its `measure`), sheets and swipe-to-dismiss move to `layout_with`: what they read should place and not measure, as `OffsetTest`'s no-remeasure case asks, and it's a real gain for scrolling;
  - `PaddingValues.Absolute`, `PaddingValues.plus`/`minus`, `calculateStartPadding`/`calculateEndPadding` (check compose-rs's `PaddingValues` for the `calculate*` methods);
  - `BoxScope.matchParentSize`, `Arrangement.Absolute` (Left, Right, Center, SpaceBetween/Around/Evenly, `spacedBy`, `aligned`), `Modifier.visible`.
- **intrinsics:** done (D1200–D1205): `IntrinsicSize`, `Modifier.width/height(IntrinsicSize)`, `requiredWidth/requiredHeight(IntrinsicSize)`, and Compose's intrinsics on every layout here. Flow layouts, `FlexBox` and `Grid` bring theirs.
- **alignment-lines:** `paddingFrom` (Dp and TextUnit), `paddingFromBaseline`, `RowScope.alignBy` (line or lambda), `alignByBaseline`, `ColumnScope.alignBy`. Until then, `RichTooltip` (D1551) takes a first baseline at 80 % of its style's line: `padding_to_baseline` in `crates/compose-material3/src/tooltip.rs`, and its test in `suite/tooltip.rs`.
- **flow-layout:** done (D1350–D1353; `flow_layout.rs`): `FlowRow`, `FlowColumn` (`maxItemsInEachRow`, `maxLines`, `itemVerticalAlignment`), `FlowRowScope.fillMaxRowHeight` (*exp*), `FlowColumnScope.fillMaxColumnWidth` (*exp*). The `overflow` overloads and `ContextualFlow*` are deprecated, and `FlowRowOverflowScope`/`FlowColumnOverflowScope` go with them: not here.
- **flexbox:** `FlexBox`, `FlexBoxConfig`/`FlexBoxConfigScope`, `FlexConfig`/`FlexConfigScope`, `FlexBoxScope.flex`, `FlexDirection`, `FlexWrap`, `FlexJustifyContent`, `FlexAlignItems`, `FlexAlignSelf`, `FlexAlignContent`, `FlexBasis`. They are stable at tip, added 2025-12-08, after 1.12.
- **grid-layout:** `Grid`, `GridConfigurationScope` (`row`/`column` by Dp, % or `Fr`, `gap`s, `area`, `minmax`), `GridScope.gridItem` (index, area, ranges), `GridTrackSize` (Fixed, Flex, MinMax, Percentage, Auto, MinContent, MaxContent), `Fr`, `GridFlow`, `columns`/`rows`. Stable, added 2025-12-09. Don't confuse it with lazy grids.
- **ruler-fit:** `Modifier.fitInside(RectRulers)`, `fitOutside(RectRulers)`.
- **window-insets:** done (D1355–D1358, D1361; `window_insets.rs`): fixed insets, `exclude`, `add`, `WindowInsetsSides` start/end, `consumeWindowInsets`, `onConsumedWindowInsetsChanged`, `recalculateWindowInsets`, the size modifiers, every padding shorthand and kind, `cutoutPath`, visibility, `asPaddingValues(density)`, `MutableWindowInsets` (*exp*), `AbstractComposeView.consumeWindowInsets` (`ComposeRsView.consumeWindowInsets`).
- **ime-nested-scroll:** `imeNestedScroll` (*exp*), `rememberWindowInsetsConnection`: the keyboard moved by a nested scroll through `WindowInsetsAnimationController` (D1361).
- **mutator-mutex:** `MutatorMutex` (`mutate`, `mutateWith`, `tryMutate`), `MutatePriority` (runtime's enum).
- **scrollable:**
  - state and modifiers: `ScrollableState` (`scroll(priority) {}`, `dispatchRawDelta`, `isScrollInProgress`, `canScroll*`, `lastScrolled*`, `scrollIndicatorState`), `ScrollableState(consume)`/`rememberScrollableState`, `ScrollScope`, `Modifier.scrollable` (both overloads), `Modifier.scrollableArea`;
  - flings: `FlingBehavior`, `TargetedFlingBehavior`, `ScrollableDefaults` (`flingBehavior()`, `reverseDirection`);
  - extensions: `scrollBy`, `animateScrollBy`, `stopScroll`;
  - `BringIntoViewSpec` with `LocalBringIntoViewSpec`, `clipScrollableContainer`, `checkScrollableContainerConstraints`.
- **scroll-state-api:**
  - `ScrollState`: `animateScrollTo`, `scroll {}`, suspend `scrollTo` that returns what it consumed, `canScrollForward`/`Backward`, `isScrollInProgress`, `lastScrolled*`, `interactionSource`, `scrollIndicatorState`, `Saver`;
  - the parameters `enabled`, `flingBehavior`, `reverseScrolling` and `overscrollEffect` of `verticalScroll`/`horizontalScroll`.
- **scroll-indicator-state:** `ScrollIndicatorState` (`scrollOffset`, `contentSize`, `viewportSize`) on `ScrollState`, the lazy states and `PagerState`.
- **scrollable2d:** `Scrollable2DState`, `rememberScrollable2DState`, `Scroll2DScope`, `Modifier.scrollable2D`, and the `Offset` forms of `scrollBy`/`animateScrollBy`/`stopScroll`.
- **overscroll:** Done (D1464) but for `rememberPlatformOverscrollFactory`, Android's glow and stretch, and drawing the effect `verticalScroll` gets from the local factory (it gets the events, D1465): `OverscrollEffect` (`applyToScroll`, `applyToFling`, `isInProgress`, `node`), `OverscrollFactory`, `LocalOverscrollFactory`, `rememberOverscrollEffect`, `Modifier.overscroll`, `withoutVisualEffect`, `withoutEventHandling`, `rememberPlatformOverscrollFactory` (Android glow and stretch).
- **draggable:** done (D1458).
- **gesture-detectors:**
  - `awaitEachGesture`, `awaitFirstDown(requireUnconsumed, pass)`, `waitForUpOrCancellation(pass)`;
  - the drag detectors: `detectDragGestures(orientationLock, onDragStart(down, slop, offset), …)`, `detectHorizontalDragGestures`, `detectVerticalDragGestures`, `detectDragGesturesAfterLongPress`;
  - the slop and drag primitives: `awaitTouchSlopOrCancellation` (as public API), `awaitHorizontal/VerticalTouchSlopOrCancellation`, `awaitHorizontal/VerticalPointerSlopOrCancellation`, `awaitDragOrCancellation` (and its horizontal and vertical forms), `drag`, `horizontalDrag`, `verticalDrag`, `awaitLongPressOrCancellation`;
  - `PressGestureScope` (`awaitRelease`, `tryAwaitRelease`) for `onPress`, and `GestureCancellationException`.
  - compose-rs puts its detectors in `compose_ui::PointerInputScope`, where Compose puts them in foundation's `gestures` package. Re-export them from `compose::foundation::gestures`.
- **transformable:**
  - `detectTransformGestures`, `calculatePan`, `calculateZoom`, `calculateRotation`, `calculateCentroid`, `calculateCentroidSize`;
  - `Modifier.transformable` (with and without `canPan`), `TransformableState`, `rememberTransformableState`, `TransformScope`;
  - `animateBy`, `animatePanBy`, `animateRotateBy`, `animateZoomBy`, `panBy`, `rotateBy`, `zoomBy`, `stopTransformation`.
- **anchored-draggable:** Done (D1463, `overscrollEffect` D1464) but for the deprecated constructors and `settle(velocity)`: `AnchoredDraggableState` (both constructors, `anchoredDrag`, `dispatchRawDelta`, `progress(from, to)`, `requireOffset`, `settle(spec)`, `updateAnchors`, `currentValue`, `targetValue`, `settledValue`, `offset`, `lastVelocity`, `isAnimationRunning`, `Saver()`), `DraggableAnchors {}`, `DraggableAnchorsConfig`, `Modifier.anchoredDraggable` (both overloads), `AnchoredDragScope`, `AnchoredDraggableDefaults` (`flingBehavior`, `PositionalThreshold`, `SnapAnimationSpec`, `DecayAnimationSpec`), `animateTo`, `animateToWithDecay`, `snapTo`, `forEach`.
- **snapping:** Done (D1462): `SnapLayoutInfoProvider` (interface, and for `LazyListState`/`LazyGridState`), `SnapPosition` (Start, Center, End), `snapFlingBehavior`, `rememberSnapFlingBehavior` (for a provider, a `LazyListState` or a `LazyGridState`).
- **bring-into-view:** `BringIntoViewRequester()`, `Modifier.bringIntoViewRequester`, `bringIntoView(rect)`. compose-rs only brings focused nodes into view, through its internal `ModifierNode::bring_into_view`.
- **lazy-layout-info:** done (D859).
- **lazy-state-api:** on `LazyListState` and `LazyGridState`:
  - `animateScrollToItem`, `requestScrollToItem`, `scroll(priority) {}`, `dispatchRawDelta`;
  - `isScrollInProgress`, `interactionSource`, `lastScrolledForward`/`Backward`;
  - `LazyLayoutScrollScope` (and `LazyLayoutScrollScope(state, scrollScope)` for each state), the `Saver` as a value.
- **lazy-list-params:** done: `LazyColumn`/`LazyRow`'s `flingBehavior` and `overscrollEffect` (D1464). `reverseLayout`, `userScrollEnabled`, `contentPadding` and `cacheWindow` exist.
- **grid-parity:** done but for `LazyGridItemScope`'s `animateItem` (with animate-item), D1451; `overscrollEffect` D1464.
- **cache-window:** done for lists and grids (D892). Left: `LazyStaggeredGridDefaults.cacheWindow(state)`, with staggered grids.
- **lazy-semantics:** items in lazy layouts expose `CollectionInfo`/`CollectionItemInfo`, `ScrollToIndex`, `IndexForKey` and `scrollByOffset` (ui semantics properties), so `performScrollToIndex`/`performScrollToKey` work.
- **pinnable:** (Started, D1282: `LOCAL_PINNABLE_CONTAINER`, `PinnableContainer` and `PinnedHandle` in ui, lazy items pinned one at a time by what's focusable; the rest of this waits.) `LocalPinnableContainer`/`PinnableContainer` (ui) honoured by lazy items, `LazyLayoutPinnableItem`, `LazyLayoutPinnedItemList`.
- **beyond-bounds-api:** ui's `BeyondBoundsLayout` (`ModifierLocalBeyondBoundsLayout`, `layout(direction) {}`) answered by lazy layouts. compose-rs answers focus searches only.
- **animate-item:** `LazyItemScope.animateItem(fadeInSpec, placementSpec, fadeOutSpec)`, `LazyGridItemScope.animateItem`, `LazyStaggeredGridItemScope.animateItem`.
- **lazy-layout-public:**
  - layout: `LazyLayout(itemProvider, modifier, prefetchState, measurePolicy)`, `LazyLayoutItemProvider`, `LazyLayoutMeasurePolicy`, `LazyLayoutMeasureScope.compose(index)`;
  - prefetch: `LazyLayoutPrefetchState` (`schedulePrecomposition`, `schedulePrecompositionAndPremeasure`, `PrefetchHandle.cancel`/`markAsUrgent`, `PrefetchResultScope`), the `onNestedPrefetch` constructor (*exp*), `NestedPrefetchScope` (*exp*);
  - content: `LazyLayoutIntervalContent`, `IntervalList`/`MutableIntervalList`, `LazyLayoutKeyIndexMap`, `getDefaultLazyLayoutKey`.
- **staggered-grid:** `LazyVerticalStaggeredGrid`, `LazyHorizontalStaggeredGrid`, `StaggeredGridCells` (Fixed, Adaptive, FixedSize), `StaggeredGridItemSpan` (FullLine, SingleLane), `LazyStaggeredGridScope` (`item`, `items`, list and indexed forms), `LazyStaggeredGridState` (`rememberLazyStaggeredGridState`, all the scroll API, `layoutInfo`), `LazyStaggeredGridItemInfo`/`LayoutInfo`, `LazyStaggeredGridItemScope`.
- **pager:**
  - composables: `HorizontalPager`, `VerticalPager`;
  - state: `PagerState` (abstract) and `PagerState(…, pageCount)`, `rememberPagerState`;
  - `PageSize` (Fill, Fixed), `PagerDefaults` (`flingBehavior`, `pageNestedScrollConnection`, `bringIntoViewSpec`, `BeyondViewportPageCount`), `PagerSnapDistance.atMost`;
  - info and scopes: `PagerLayoutInfo`, `PageInfo`, `PagerScope`.
- **keep-focused-in-view:** no API. Compose's scrollables keep a focused child in view when their viewport shrinks (`ContentInViewNode`).
- **system-gesture-exclusion:** `Modifier.systemGestureExclusion()`, with a rect lambda (Android).
- **flags:** `ComposeFoundationFlags` (*exp*). compose-rs has none now: the pausable-prefetch flag it mirrored went as the tip's did (D892).

What compose-rs has in this area: `Box`, `Row`, `Column`, `Spacer`, `BoxWithConstraints`; the arrangements (no `Absolute`); padding, size, fill, wrap, `aspect_ratio`, `default_min_size`, `offset(x, y)`, `weight`, `align`; window insets (`window_insets.rs`: every kind, consumption, sizes); `vertical_scroll`/`horizontal_scroll` with `ScrollState` (value, max, viewport, `scroll_to`); `draggable` (axis, reverse, callbacks); `detect_tap_gestures`, `detect_drag_gestures`, `await_first_down`, `wait_for_up_or_cancellation`, `await_touch_slop_or_cancellation`; nested scroll; `LazyColumn`/`LazyRow` (keys, content types, sticky headers, content padding, reverse layout, `user_scroll_enabled`, `fill_parent_max_*`, saveable item state, reuse, prefetch, beyond-bounds focus search); `LazyVerticalGrid`/`LazyHorizontalGrid` (Fixed/Adaptive, spans, content types, padding); Compose Desktop's scrollbars and tooltips.

## Missing features

Each entry gives the Compose API, what it does, the test files that need it (by file name), its dependencies, its size (S < 1 day, M a few days, L 1–2 weeks, XL multi-week), and whether it's experimental in Compose.

1. **cache-window** (tip parity). **Done for lists and grids** (D892 to D899).
   - API: `LazyLayoutCacheWindow` and its factories, `LazyListDefaults`/`LazyGridDefaults.cacheWindow`, the `cache_window` parameter.
   - Does: the dynamic default window replaces the one-item strategy as the parity mode's prefetch (Tip differences 1–4); the old strategy isn't kept, being deprecated at tip.
   - Tests: LazyListCacheWindowTest, LazyListCacheWindowParityTest, LazyListPrefetchPrecedenceTest, LazyGridCacheWindowTest, LazyGridCacheWindowParityTest, LazyGridPrefetchPrecedenceTest, CacheWindowLogicTest are ported. Later, with their features: PagerCacheWindowTest and LazyStaggeredGridCacheWindowTest.
2. **mutator-mutex.**
   - API: `MutatorMutex`, `MutatePriority`.
   - Does: one mutation at a time, a higher or equal priority cancelling the running one, as scrolls and drags take turns.
   - Tests: MutatorMutexTest; used by scrollable, draggable, anchored-draggable and pager.
   - Depends on: runtime's coroutine cancellation (exists: `Job`).
   - Size: S. Experimental: no.
   - **Done** (D850): `compose_foundation::{MutatorMutex, MutatePriority, MutationInterrupted}`.
3. **scrollable.**
   - API: `ScrollableState`, `Modifier.scrollable`, `scrollableArea`, `ScrollScope`, `FlingBehavior`, `ScrollableDefaults`, `scrollBy`/`animateScrollBy`/`stopScroll`, keyboard Page Up/Down and arrow scrolling.
   - Does: the public scrolling primitive that `vertical_scroll` and the lazy lists build on. compose-rs's `DragScroller` and the scroll nodes already do drags, wheel, flings and nested scroll, privately.
   - Tests: ScrollableTest, ScrollableAreaTest, and most scroll-state and lazy-state tests.
   - Depends on: mutator-mutex; interaction (`DragInteraction`, misc ledger) for `interactionSource`.
   - Size: L. Experimental: no.
   - **Mostly done** (D851, D852, D855): `ScrollableState`, `ScrollableStateExt` (`scroll`, `scroll_by`, `animate_scroll_by`, `stop_scroll`), `ScrollScope`, `FlingBehavior`, `DefaultFlingBehavior`, `ScrollableDefaults` (`fling_behavior`, `reverse_direction`), `scrollable_state`/`remember_scrollable_state`, `Modifier.scrollable`/`scrollable_with`. Left: `scrollableArea`, keyboard paging, `TargetedFlingBehavior`, `BringIntoViewSpec`, `clipScrollableContainer`, `checkScrollableContainerConstraints`, the overscroll overload.
4. **scroll-state-api.**
   - API: the rest of `ScrollState`, and `verticalScroll`'s `enabled`/`flingBehavior`/`reverseScrolling` parameters.
   - Tests: ScrollTest (rest).
   - Depends on: scrollable.
   - Size: S–M. Experimental: no.
   - **Mostly done** (D853): `scroll_to`, `animate_scroll_to`, `scroll {}`, `can_scroll_*`, `is_scroll_in_progress`, `last_scrolled_*`, `interaction_source`, `vertical_scroll_with`/`horizontal_scroll_with` (`enabled`, `fling_behavior`, `reverse_scrolling`). Left: `scrollIndicatorState`, the `Saver` as a value, the `overscrollEffect` overload.
5. **lazy-layout-info.**
   - API: `layoutInfo` with item infos on list and grid states.
   - Does: what most of Compose's lazy tests assert with.
   - Tests: LazyListLayoutInfoTest, LazyGridLayoutInfoTest, parts of LazyListHeadersTest, LazyCustomKeysTest and LazyListTest; snapping and animate-item tests.
   - Depends on: none.
   - Size: M. Experimental: no.
   - **Done** (D859): `layout_info()` on both states, `LazyListLayoutInfo`/`LazyListItemInfo`, `LazyGridLayoutInfo`/`LazyGridItemInfo` (keys and content types as their hashes).
6. **lazy-state-api.**
   - API: `animateScrollToItem`, `requestScrollToItem`, `scroll {}`, `dispatchRawDelta`, `isScrollInProgress`, `interactionSource`, `lastScrolled*`, `LazyLayoutScrollScope`.
   - Tests: LazyScrollTest (list, grid), LazyListRequestScrollTest, LazyGridRequestScrollTest, LazyListAnimateScrollScope, parts of LazyListTest and LazyColumnTest.
   - Depends on: scrollable, lazy-layout-info.
   - Size: M. Experimental: no.
   - **Done** (D853, D858, D860, D861): `animate_scroll_to_item`, `request_scroll_to_item`, `scroll_to_item` (async), the `ScrollableState` API, `interaction_source`, `LazyLayoutScrollScope` and `lazy_layout_scroll_scope`. Left: the `Saver` as a value.
7. **layout-modifiers.**
   - API: `required*`, `absolutePadding`/`absoluteOffset`, lambda offsets, `PaddingValues.Absolute`/`+`/`-`, `matchParentSize`, `Arrangement.Absolute`, `visible`, negative-padding checks.
   - Tests: VisibleTest, parts of OffsetTest, PaddingTest, PaddingValuesTest, SizeTest, BoxTest and RowColumnTest.
   - Depends on: none (`visible` needs a node that skips drawing, hit testing and focus).
   - Size: M. Experimental: no.
8. **grid-parity.** **Done** but for `animateItem`, which comes with its feature (D1451); `overscrollEffect` D1464.
   - API: grids' `reverse_layout`, `sticky_header`, `GridCells::FixedSize`, `GridCells::Custom` for `calculateCrossAxisCellSizes`, the cross-axis arrangement, `LazyGridItemScope::cross_axis_cell_count`, `LazyGridItemInfo::UNKNOWN_ROW`/`UNKNOWN_COLUMN`.
   - Tests: LazyGridsReverseLayoutTest, LazyGridHeadersTest, LazyGridTest, LazyArrangementsTest (grid), LazyGridSpanTest, LazyGridsContentPaddingTest, LazyGridLayoutInfoTest, LazyGridRequestScrollTest.
9. **draggable.** Done (D1458).
   - API: the full `draggable` signature, `DraggableState.drag`, `draggable2D`.
   - Tests: DraggableTest (rest), Draggable2DTest.
   - Depends on: mutator-mutex; interaction (misc ledger); Tip differences 7.
   - Size: M. Experimental: no.
10. **gesture-detectors.**
    - API: `awaitEachGesture`, `PressGestureScope`, the drag-detector variants and primitives, `awaitLongPressOrCancellation`.
    - Tests: AwaitEachGestureTest, AwaitLongPressOrCancellationTest, DragGestureDetectorTest (rest), TapGestureDetectorTest (rest).
    - Depends on: ui's pointer events carrying every pointer's change at once (two-finger tests; ui ledger).
    - Size: M. Experimental: no.
11. **scroll-indicator-state.**
    - API: `ScrollIndicatorState`.
    - Tests: LazyListScrollIndicatorTest, LazyGridScrollIndicatorTest; later the staggered and pager ones.
    - Depends on: lazy-layout-info. compose-rs's `ScrollbarAdapter` estimates the same values.
    - Size: S. Experimental: no.
12. **lazy-semantics.**
    - API: collection info, `ScrollToIndex`, `IndexForKey`, `scrollByOffset`.
    - Tests: LazySemanticsTest (list and grid), PagerAccessibilityTest.
    - Depends on: ui semantics properties and the test harness's `perform_scroll_to_index`/`key`.
    - Size: M. Experimental: no.
13. **keep-focused-in-view.**
    - Does: a scrollable whose viewport shrinks (an IME, a resize) scrolls its focused child back into view, as `ContentInViewNode.onRemeasured`.
    - Tests: ScrollFocusableInteractionTest, LazyListFocusableInteractionTest.
    - Depends on: none.
    - Size: M. Experimental: no.
14. **bring-into-view.**
    - API: `BringIntoViewRequester`, `bringIntoViewRequester`, `BringIntoViewSpec`, `LocalBringIntoViewSpec`.
    - Tests: BringIntoViewResponderTest, BringIntoViewScrollableInteractionTest.
    - Depends on: scrollable.
    - Size: M. Experimental: no.
15. **snapping.** Done (D1462).
    - API: `SnapLayoutInfoProvider`, `SnapPosition`, `snapFlingBehavior`, `rememberSnapFlingBehavior`, `TargetedFlingBehavior`.
    - Tests: the five files in `gestures/snapping`.
    - Depends on: scrollable (`FlingBehavior`), lazy-layout-info, animation's decay specs (`DecayAnimationSpec`, `exponentialDecay`, `splineBasedDecay`: animation ledger).
    - Size: M. Experimental: no.
16. **anchored-draggable.** Done (D1463); material3's sheets and swipe-to-dismiss are on it (D1466).
    - API: the `anchoredDraggable` family (see above).
    - Tests: the four `anchoredDraggable` files, DraggableAnchorsTest.
    - Depends on: draggable, snapping (`TargetedFlingBehavior`), the decay specs (animation), overscroll (for its overscroll tests).
    - Size: M–L. Experimental: no.
17. **overscroll.** The events are done (D1464): the effect, its factory, the parameters. Left: Android's stretch and glow (a `RenderEffect` or `EdgeEffect` in the replayer), with StretchOverscrollIntegrationTest, StretchOverscrollScreenshotTest and the drawing tests of OverscrollTest.
    - API: `OverscrollEffect`, `overscroll`, `rememberOverscrollEffect`, `LocalOverscrollFactory`, `rememberPlatformOverscrollFactory`; the `overscrollEffect` parameters.
    - Does: edge effects; Android's stretch.
    - Tests: OverscrollTest, StretchOverscrollIntegrationTest, StretchOverscrollScreenshotTest, AnchoredDraggableOverscrollTest, ScrollableAreaTest, parts of LazyListTest and LazyListsContentPaddingTest.
    - Depends on: scrollable; a stretch effect on Android (a `RenderEffect` or `EdgeEffect` in the replayer: graphics, android).
    - Size: L. Experimental: no.
18. **intrinsics.** Done (PL-3, D1200–D1205; `crates/compose-foundation/tests/suite/intrinsics.rs`).
    - API: `IntrinsicSize`, the intrinsic modifiers, min and max intrinsics on every layout.
    - Tests: IntrinsicTest, parts of RowColumnTest, SizeTest, BoxTest, AspectRatioTest, PaddingTest and ScrollTest are ported; FlowRowColumnTest's and FlexBoxTest's come with those layouts.
19. **alignment-lines.**
    - API: `paddingFrom`, `paddingFromBaseline`, `alignBy`, `alignByBaseline`.
    - Tests: AlignmentLineTest, RowColumnModifierTest, parts of RowColumnTest and FlexBoxTest.
    - Depends on: ui's alignment lines (`AlignmentLine`, `FirstBaseline`/`LastBaseline` in measure results, text reporting baselines; ui and text ledgers).
    - Size: L. Experimental: no.
20. **flow-layout.** Done (D1350–D1353; `crates/compose-foundation/tests/suite/flow_layout.rs`).
    - API: `FlowRow`, `FlowColumn` and their scopes.
    - Tests: FlowRowColumnTest, 2 of LayoutReuseTest.
    - Depends on: intrinsics for 45 tests (done).
    - Size: L. Experimental: no (`fillMaxRowHeight` and the overflow scopes are).
21. **transformable.**
    - API: `detectTransformGestures`, the `calculate*` helpers, `transformable`, `TransformableState`.
    - Tests: TransformableTest, TransformGestureDetectorTest.
    - Depends on: ui's multi-pointer events and trackpad gestures (pointer-input-types), mutator-mutex.
    - Size: M. Experimental: no.
22. **scrollable2d.**
    - API: `scrollable2D` and its state.
    - Tests: Scrollable2DTest, Scrollable2DInputTest, TouchScrollable2DTest, MouseWheelScrollable2DTest, TrackpadScrollable2DTest.
    - Depends on: scrollable, nested scroll in 2D (exists).
    - Size: M. Experimental: no.
23. **pinnable.**
    - API: `LocalPinnableContainer` honoured by lazy items, `LazyLayoutPinnableItem`.
    - Tests: the four `*PinnableContainerTest` files.
    - Depends on: ui's `PinnableContainer`. compose-rs pins the focused item internally; generalize it.
    - Size: M. Experimental: no.
24. **beyond-bounds-api.**
    - API: ui's `BeyondBoundsLayout`, answered by lazy layouts.
    - Tests: LazyListBeyondBoundsTest, LazyListBeyondBoundsAndExtraItemsTest, LazyGridBeyondBoundsTest, LazyStaggeredGridBeyondBoundsTest.
    - Depends on: ui modifier locals. compose-rs's `lay_out_beyond_bounds` is the mechanism, focus-only.
    - Size: M. Experimental: no.
25. **animate-item.**
    - API: `animateItem` for lists and grids (staggered with 27).
    - Does: fades items in and out and animates moves, keeping removed items composed while they fade.
    - Tests: the 3 list, 3 grid and 3 staggered animation files.
    - Depends on: lazy-layout-info (tests), animation (exists); ui lookahead for the lookahead variants (ui ledger).
    - Size: L. Experimental: no.
26. **pager.**
    - API: `HorizontalPager`, `VerticalPager`, `PagerState` and the rest of the pager API.
    - Tests: all 25 pager files.
    - Depends on: scrollable, lazy-state-api, snapping, cache-window, bring-into-view, pinnable, lazy-semantics, scroll-indicator-state. Built on compose-rs's lazy list internals.
    - Size: XL. Experimental: no.
27. **staggered-grid.**
    - API: `LazyVerticalStaggeredGrid`, `LazyHorizontalStaggeredGrid` and their API.
    - Tests: all 20 staggered files.
    - Depends on: lazy-layout-info, lazy-state-api, cache-window (staggered's default), animate-item, pinnable, beyond-bounds-api, ui lookahead.
    - Size: L–XL. Experimental: no.
28. **window-insets.**
    Done (D1355–D1358; `crates/compose-foundation/tests/suite/window_insets.rs`, the Android listener in `ComposeRsInsets.kt`).
    - API: the insets API above.
    - Tests: WindowInsetsTest, WindowInsetsPaddingTest (42 of 50), WindowInsetsSizeTest, WindowInsetsIgnoringVisibilityTest. On a device, to run: WindowInsetsDeviceTest, WindowInsetsListenerUnsetTest. `imeNestedScroll` and WindowInsetsControllerTest are **ime-nested-scroll** (D1361).
    - Size: L. Experimental: partly (`MutableWindowInsets`).
29. **flexbox.**
    - API: `FlexBox` and its configs.
    - Tests: FlexBoxTest, FlexBoxDirectionTest.
    - Depends on: alignment-lines (baseline tests), intrinsics (done).
    - Size: L. Experimental: no (new at tip).
30. **grid-layout.**
    - API: `Grid` and its configuration.
    - Tests: GridTest.
    - Depends on: intrinsics (done; content-based tracks need min/max content sizes).
    - Size: L–XL. Experimental: no (new at tip).
31. **ruler-fit.**
    - API: `fitInside`, `fitOutside`.
    - Tests: FitInsideTest, FitOutsideTest.
    - Depends on: ui's rulers (`RectRulers`, `WindowInsetsRulers`; ui ledger).
    - Size: M. Experimental: no.
32. **lazy-layout-public.**
    - API: `LazyLayout` and its provider, measure scope and prefetch-state API, `MutableIntervalList`.
    - Tests: LazyLayoutTest, MutableIntervalListTest.
    - Depends on: compose-rs composes lazy items in rounds outside measurement (docs/06), where `LazyLayoutMeasureScope.compose(index)` composes inside it. A public `LazyLayout` needs either subcomposition during measure or an API shaped around rounds: a design decision first.
    - Size: XL. Experimental: partly (`NestedPrefetchScope`, the `onNestedPrefetch` constructor).
33. **system-gesture-exclusion.**
    - API: `Modifier.systemGestureExclusion`.
    - Tests: SystemGestureExclusionTest.
    - Depends on: compose-android setting `View.setSystemGestureExclusionRects`.
    - Size: S. Experimental: no.
34. **pointer-input-types** (ui ledger's).
    - Does: trackpad pan and scale gestures, indirect pointer events.
    - Tests: LazyColumnTrackpadHoverTest, TrackpadScrollable2DTest, indirect-pointer tests in DraggableTest and ScrollableTest.
    - Size: M (ui).
35. **movable-content** (runtime ledger's).
    - API: `movableContentOf`.
    - Tests: LazyListMovableContentTest.

**lazy-list-params** (`flingBehavior` and `overscrollEffect` on `LazyColumn`/`LazyRow`) has no entry of its own: each parameter comes with the feature it carries (scrollable, overscroll). Size S each.

### Suggested order

1. **cache-window** (L): parity follows tip, and the benchmark app is moving to tip, so this goes first. It needs nothing else, and its tests assert composition and disposal, which the harness can already observe.
2. **mutator-mutex** (S), then **scrollable** (L), then **scroll-state-api** (S–M): the foundation the scroll, lazy, snapping and pager work stands on.
3. **lazy-layout-info** (M), then **lazy-state-api** (M): they unlock adapting most lazy tests (LazyListTest, LazyScrollTest, the headers, keys and padding files).
4. **layout-modifiers** (M) and **grid-parity** (M): small and self-contained, they finish Row/Column/Box/Size and the grids.
5. **draggable** (M), then **gesture-detectors** (M), then **scroll-indicator-state** (S).
6. **lazy-semantics** (M), **keep-focused-in-view** (M), **bring-into-view** (M).
7. **snapping** (M), then **anchored-draggable** (M–L), then **overscroll** (L).
8. **intrinsics** (L, done) and **alignment-lines** (L), when the ui work it depends on lands; then **flow-layout** (L, done).
9. **transformable** (M) and **scrollable2d** (M), after ui's multi-pointer events.
10. **pinnable** (M), **beyond-bounds-api** (M), **animate-item** (L).
11. **pager** (XL), then **staggered-grid** (L–XL).
12. **window-insets** (L, done), **flexbox** (L), **grid-layout** (L–XL), **ruler-fit** (M), **system-gesture-exclusion** (S).
13. **lazy-layout-public** (XL), after a design decision on composing during measurement.

## Notes for the porting agents

### How Compose's test patterns map to `compose::test`

- **The rule.**
  - `createComposeRule()` + `setContent {}` is `compose_test(|| …)`, or `ComposeTestRule::sized(IntSize, …)` for a root of a given size at density 1, or `compose_test_with(size, density, measurer, …)`.
  - Compose's lazy tests size things in Dp computed from pixels (`with(rule.density) { 100.toDp() }`). At density 1 the numbers are the same in pixels; keep density 1 unless the test is about density.
- **Clock and idling.**
  - `mainClock.autoAdvance = false` is `ComposeTestRule::with_manual_clock` or `set_auto_advance(false)`.
  - `mainClock.advanceTimeBy(ms)` is `advance_time_by(Duration)`, and `advanceTimeByFrame()` is `advance_time_by_frame()`. A frame is 16 ms, as Compose's test clock.
  - `waitForIdle`/`runOnIdle {}` is `wait_for_idle()` before and after plain code.
- **Finders and assertions.**
  - `onNodeWithTag(tag)` is `on_node_with_tag(tag)`, and `onNodeWithText` is `on_node_with_text`.
  - `assertExists`/`assertDoesNotExist` exist. `on_all_nodes(matcher, index)` and `count_nodes` cover `onAllNodes…[i]` and `assertCountEquals`.
- **State-driven scrolls.**
  - `runBlocking { state.scrollToItem(i, o) }` is `scroll_support::run(&mut ui, async move { state.scroll_to_item(i, o).await })`: `scroll_to_item` is `async`, as Compose's suspend function, and returns after the layout (D853). `state.requestScrollToItem(i, o)` is `request_scroll_to_item(i, o)`, which writes the position at once (D861).
  - `runBlocking { state.scrollBy(x) }` is `run(&mut ui, async move { state.scroll_by(x).await })` (`ScrollableStateExt::scroll_by`, `async`, returning the pixels consumed); `state.dispatchRawDelta(x)` is `dispatch_raw_delta(x)`, synchronous. A `scroll_by(x)` not awaited is a dropped future, and the workspace denies `unused_must_use`, so it doesn't compile.
- **Gestures.**
  - `performTouchInput { down/moveBy/up }` has no DSL. Build `PointerEvent::press/moved/release(…).at_time(ns).with_pointer(PointerId, PointerType::Touch)` and send them with `host().send_pointer` (see `fling.rs::swipe_up`).
  - Compose's `swipeUp()`/`swipeWithVelocity` interpolate moves over a duration: write the same number of samples, at the same times, or the velocity differs.
  - `performMouseInput { scroll(d) }` is `PointerEvent::scroll(position, Offset)`. compose-test's are pixels (`WheelStep::Pixels`, the default): the units a port's `scroll(d)` means times the factor it asserts (D1450).
- **Touch slop.** Compose tests read `LocalViewConfiguration.current.touchSlop`. compose-rs's is 8 dp (`PointerInputScope::touch_slop`, `review_lazy2.rs::the_touch_slop_is_in_dp`): add the slop to drag distances as Compose's tests do.
- **Keys.** `performKeyInput { pressKey(Key.PageDown) }` is `press_key(Key::PageDown)` or `perform_key_press`.
- **Accessibility.** `performSemanticsAction(SemanticsActions.ScrollBy)` is `perform_semantics_action_with(&SemanticsActions::SCROLL_BY, |scroll| { scroll(x, y); })`. Android's `AccessibilityNodeInfo` checks go in `compose-android/tests` (see `accessibility_scroll.rs`).
- **Prefetch.**
  - Compose's `TestPrefetchScheduler` (`waitForPrefetch()`) is `host().prefetch(deadline)` / `prefetch_with(deadline, true)`.
  - Composition is observed with a `DisposableEffect` keyed by index, as `prefetch_parity.rs` does. Compose's prefetch tests use `onNodeWithTag(i).assertExists()` with `assertIsNotPlaced()`: compose-rs has no `assert_is_not_placed`, so check composition through the effect.
  - Set `use_optimizations(Optimizations::NONE)` for any test of Compose's own behaviour (parity), since the rule's default is the recommended set (D610).
- **Parameterized tests.** Orientation, reverse layout and RTL become a helper taking the parameters, or a small macro emitting one `#[test]` per case. Name each case after the Compose parameters.
- **Citations.** Cite the Compose test in a doc comment on each adapted test (`/// Compose: LazyListTest.lazyListScrollToShowItems123`), rewritten from the scenario, never copied.

### What the harness lacks (add as needed, in `crates/compose-test`)

> Since the harness's milestone 1 (D900–D905), compose-test has Compose's clock model (rounding, frames on the test clock, `advance_time_until`, `wait_until`, the infinite-animation policy), bounds and displayed assertions, matchers over the tree and selectors (`on_children`, `on_parent`, collections), the touch, mouse (primary button) and key injection scopes, `perform_scroll_to`, `capture_to_image` with `assert_pixels`, and `print_to_string`; `perform_click` is a touch click. The notes in this section and the mapping above that call these missing are out of date: docs/09 lists the API, and [ui-text-and-test](ui-text-and-test.md) what is still missing. Existing tests that relied on the old timing gained a frame where they wait for a recomposition (D900).

- **Position and size assertions.** compose-test has them (`assert_top_position_in_root_is_equal_to`, `get_unclipped_bounds_in_root`, …). The lazy base classes' orientation-aware forms are `Lazy::main_start`, `cross_start`, `main_size` and `cross_size` in `tests/suite/lazy_support.rs`, on `get_unclipped_bounds_in_root`, returning the value to compare.
- **Display assertions.** compose-test's `is_displayed`, `assert_is_displayed` and `assert_is_not_displayed`; `assertIsNotPlaced` is still to add.
- **Touch input.** compose-test's `perform_touch_input` and `perform_mouse_input`; `scroll_support::{Touch, swipe}` are those in the root's coordinates, for a finger across calls.
- **Semantic scroll actions.** `performScrollToIndex`, `performScrollToKey` and `performScrollTo` (they need feature:lazy-semantics).
- **Tree navigation.** `onChildren()`, `onParent()`, `onChildAt`.
- **Screenshots.** `captureToImage`: compose-rs can render to PNG through Skia (`draw_snapshot`, docs/06), which is enough for the stretch screenshots once overscroll exists.
- **Not needed.** Inspector tests (`testInspectableParameter`, `modifierInspectorInfo`): compose-rs has no inspector; those test methods are n/a inside their files (about 20 in foundation-layout, a few in OverscrollTest, ScrollableAreaTest and Scrollable2DTest).

### compose-rs behaviour that likely differs from Compose

- **A focus search's jump.** Tab past rows it has to lay out lands on the row found in one frame (D485), where Compose's bring-into-view animates to it over frames. So the window's bound before the scroll can't apply there: a search's end reads the bounds as they are (`Prefetch::search_ended`, D896), which would hold 56 items in `review_d475.rs`'s Tab past 50 rows otherwise. For the focus workstream to close, by animating the bring-into-view; `search_ended` goes with it.
- **`ScrollState::scroll_to`** clamps at once, as Compose's (D853), and is `async`; a jump outside a task is `dispatch_raw_delta`. The lazy states' synchronous jump is `request_scroll_to_item`, `scroll_to_item` being `async`. Tests port `runBlocking { state.scrollTo(…) }` with `scroll_support::run`.
- **Velocity tracking** is Compose's `VelocityTracker1D` (Lsq2, 20 samples, the 40 ms cut-off; D862) since this workstream. Left: tip's draggable tracking per pointer and counting the first still events (Tip differences 7).

  Caught by `DraggableTest`'s velocity tests (`draggable_velocityProxy`…), `ScrollableTest`'s fling tests, `Scrollable2DInputTest`, `TouchScrollable2DTest.scrollable_assertVelocityCalculationIsSimilarInsideOutsideVelocityTracker`, and the anchored-drag velocity-threshold tests.
- **Negative padding** doesn't panic. Compose's `padding` requires non-negative values (`PaddingTest.negative*Padding_throws`), and `PaddingValues.minus` coerces to non-negative (`PaddingValuesTest.minusPaddingValues_mustBeNonNegative`).
- **Out-of-range grid spans** are clamped where Compose throws (documented in `lazy/grid.rs`). `LazyGridSpanTest` cases that expect an exception should become clamping tests that cite the deviation.
- **`BoxWithConstraints` composes** first for a hinted constraint, then again if the real one differs. Compose subcomposes once, during measure. Tests counting compositions or steps (`BoxWithConstraintsTest.allTheStepsCalledExactlyOnce`, `withConstraintCallbackIsNotExecutedWithInnerRecompositions`, `updateConstraintsRecomposingWithConstraints`) may see an extra composition when the hint is wrong.
- **Lazy lists and grids have no intrinsics** and panic when asked, with Compose's `SubcomposeLayout` message (D1204), though compose-rs doesn't subcompose them: the same behaviour, for another reason.
- **Window insets aren't consumed** (fixed, D1355): nested paddings of the same insets added up. `WindowInsetsPaddingTest.consumedInsets`, `consumedPadding` and `withConsumedWindowInsets` are ported.
- **Lazy items are composed in rounds** within a frame, not during measurement. Tests that count recompositions of items or callers (`LazyColumnTest.recompositionOrder`, `LazyListTest` composition-count tests) can see different orders. The count of items composed per frame should match (D487, D492). Compare against `review_lazy.rs::scrolling_one_row_composes_only_the_rows_coming_into_range`.
- **Lazy lists compose no `beyondBoundsItemCount` items** (Compose's is internal and 0 by default, the same). With compose-rs's recommended optimizations, items are kept behind and two are prefetched ahead. Lazy tests about what's composed must set `Optimizations::NONE` to match Compose.
- **Grids compose `BEYOND` (2) lines beyond each side** only with the `GRID_LINES_BEYOND` optimization. Compose places none, so set `NONE` for `LazyGridTest`/`LazyGridPrefetcherTest` ports.
- **Prefetch at tip** is compose-rs's parity mode (D892): `LazyListPrefetcherTest`'s ports (`prefetch_parity.rs`) run at the default window, and that file pins the deprecated strategy in Compose.
- **`detect_tap_gestures`' `on_press`** is a plain callback. Compose's gets a `PressGestureScope` whose `tryAwaitRelease` reports whether the press was released or cancelled. Caught by `TapGestureDetectorTest`'s press tests.
- **Pointer events carry one change each.** A Compose `PointerEvent` carries every pointer's change at once. Two-finger tests (`DragGestureDetectorTest.twoFingerDrag_*`, `TransformGestureDetectorTest`, `TransformableTest`) need the ui change first.
- **The gesture detectors live in `compose_ui::PointerInputScope`**, not foundation. A port in compose-foundation's suite imports them from `compose_ui`; re-exporting them under `compose::foundation::gestures` would match Compose's package.
