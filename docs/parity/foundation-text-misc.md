# Parity ledger: foundation text, selection and miscellaneous APIs

What Compose's `compose/foundation/foundation` tests and public API in this area cover, and where compose-rs stands against each. It drives the porting work: each row says whether its tests can be adapted now (`port`), are already exercised (`covered`), wait for a missing feature (`feature:<name>`, defined in [Missing features](#4-missing-features)), or don't apply (`n/a`).

## 1. Scope and totals

- **Reference:** androidx-main at `d153d87ed76` (2026-10-25), `frameworks/support/compose/foundation/foundation`: the test source sets `src/androidDeviceTest` and `src/androidHostTest` (the module has no `commonTest`), and the API in `api/current.txt` (`api/restricted_current.txt` is the same file with a different surface tag).
- **In this area:** every test file outside the gestures, scrolling and lazy areas: the `text/**`, `textfield/**`, `selection/**`, `shape/**`, `style/**`, `content/**`, `contextmenu/**`, `draganddrop/**` and `relocation/**` directories, the host-side `text/**`, `shape/**`, `style/**`, `contextmenu/**` and `MutatorMutexTest`, and the root-package device tests for clickable, focusable, hoverable, indication, interactions, border, background, image, canvas, marquee, magnifier, tooltip, external surfaces, progress semantics, system gesture exclusion and keep-clear rects. Left to the gestures/scrolling/lazy ledgers: `gestures/**`, `lazy/**`, `pager/**`, `anchoredDraggable/**`, `focus/LazyListFocusableInteractionTest`, and the root `Scroll*`, `Scrollable*`, `*Draggable*`, `*Overscroll*`, `Transformable*`, `*Scrollable2D*`, `LazyListFocusableInteractionTest` files. Shared with them: `MutatorMutexTest` and `relocation/BringIntoViewScrollableInteractionTest` (listed here).
- **compose-rs side:** `crates/compose-foundation` (`text.rs`, `text_field.rs`, `text_input.rs`, `image.rs`, `shape.rs`, `tooltip.rs`), `crates/compose-ui` (`modifier.rs`, `nodes.rs` for clickable/toggleable/combined clickable/hoverable/focusable/border/background, `interaction.rs`, `text_selection.rs`, `graphics.rs` for shapes), `crates/compose-android/src/input_method.rs`, and their tests.

**Totals:** 305 test files with 4,670 `@Test` methods (counted with `grep -cE '^\s*@(Test|ParameterizedTest)\b'`, so abstract bases such as `NodeInvalidationTestParent` count their own methods once). Another 48 files in these directories are helpers without tests and aren't listed.

| Status | Files | @Test |
|---|---:|---:|
| `covered` | 11 | 59 |
| `port` | 16 | 116 |
| `partial` | 25 | 671 |
| `feature:<name>` | 178 | 2,836 |
| `n/a` | 75 | 988 |
| **Total** | **305** | **4,670** |

Of the `n/a` tests, 571 (48 files) test the **deprecated** value-based `BasicTextField` and its internals (see below) or `ClickableText`/`onFocusedBoundsChanged`, 389 (23 files) test Compose-internal data structures or helpers whose behaviour other files check through public API, and 28 (4 files) test Compose's own test infrastructure or compare a golden image. Three `feature:` rows (`textfield/TextFieldContextMenuTest`, `textfield/TextFieldTextContextMenuBuilderTest`, `textfield/TextFieldToolbarTest`) mix the legacy and state-based fields: their 41 legacy (`btf1`) tests are n/a too.

By feature, the `feature:` rows wait on: `text-field-state` 697 tests (30 files), `android-ime` 316 (17), `text-context-menu` 294 (21), `touch-text-selection` 227 (10), `style-api` 159 (9), `secure-text-field` 106 (5), `border-parity` 92 (4), `output-transformation` 87 (6), `handwriting` 84 (6), `magnifier` 78 (5), `polygon-shapes` 64 (4), `text-field-styles` 64 (3), `text-links` 56 (3), `shapes` 50 (5), `bring-into-view-requester` 48 (3), `undo-parity` 46 (4), `content-receiver` 40 (4), `selection-state` 34, `input-transformation` 34 (3), `drag-and-drop` 31 (4), `clickable-parity` 26 (4), `basic-marquee` 23, `android-external-surface` 20 (2), `hover-icon-override` 18, `android-window-rects` 16 (2), `text-auto-size` 15, `focusable-parity` 14, `smart-selection` 13 (4), `background-text-measurement` 11, `cursor-blink` 11 (2), `indication` 10, `basic-tooltip` 8, `mutator-mutex` 8, `inline-content` 6, `dead-keys` 6, `ui-text-substitution` 6 (2), `interactions` 5, `ui-text-async-fonts` 4, `text-selection-colors` 4, `basic-text-drawing` 3, `progress-semantics` 2. The `partial` rows add more demand on `interactions`, `indication`, `clickable-parity`, `shapes` and `image-painter`: `ClickableTest`, `CombinedClickableTest`, `ToggleableTest`, `SelectableTest` and `FocusableTest` alone are 361 tests, most of them about interactions and indication.

**The value-based `BasicTextField` is deprecated.** At this commit both `BasicTextField(value: String, onValueChange, …)` and `BasicTextField(value: TextFieldValue, …)` are `@Deprecated("Use the BasicTextField(state: TextFieldState, ...) overload instead …")` (`BasicTextField.kt` lines 799 and 970). compose-rs's only text field is the `String` one (D96). So its tests (`textfield/**`, `text/CoreTextField*`, the `Legacy*` IME tests and the legacy internals) are `n/a` here, and the state-based field's tests (`text/input/**`) wait on `text-field-state`, even where compose-rs's field already does what they check (the rows say so). `KeyboardActions`, `KeyboardOptions` and `VisualTransformation` aren't deprecated themselves, but in foundation only the deprecated field takes `KeyboardActions` and `VisualTransformation`.

## 2. Test files

Paths are relative to `frameworks/support`. A status cites the compose-rs tests that already exercise the scenario (paths under `crates/compose-foundation/tests/suite/` unless another crate is named) and says what's missing.

### Foundation root (`androidDeviceTest`, misc modifiers): 33 files, 606 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/AdvancedBorderRendererTest.kt` | 40 | `BorderRenderer.drawBorder` over triangle, generic-path and non-simple rounded outlines; alpha tint; outline bounds with offset origins | feature:border-parity — `BorderRenderer` is public and absent; generic outlines need `shapes` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/AdvancedBorderTest.kt` | 30 | `Modifier.border` on triangle/`GenericShape`/non-simple rounded outlines, fill area, outline bounds | feature:border-parity — needs `shapes` (generic outlines) and brush borders |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/AndroidEmbeddedExternalSurfaceTest.kt` | 10 | `AndroidEmbeddedExternalSurface` (TextureView): surface create/change/destroy, reuse in a lazy list, opacity, transform | feature:android-external-surface |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/AndroidExternalSurfaceTest.kt` | 10 | `AndroidExternalSurface` (SurfaceView): lifecycle, z-order, secure, opacity | feature:android-external-surface |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BackgroundTest.kt` | 23 | `background` with color or brush in rectangle/circle; changing color, brush, alpha, shape; observable custom shapes; RTL; equality | port — exists (`background`, `background_with`); seen only incidentally in `compose-ui/tests/suite/ui.rs` (`modifier_order_decides_what_is_drawn_where`) and `drawing_api.rs`. The 2 observable-shape cases need `shapes`; the 2 inspectable-value cases are n/a (no inspector) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BasicMarqueeTest.kt` | 23 | `Modifier.basicMarquee`: iterations, delays, spacing, velocity, RTL, `WhileFocused` mode, cancellation | feature:basic-marquee |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BasicTooltipTest.kt` | 8 | `BasicTooltipBox`: default gestures (long press, hover), constraints propagation, anchor/popup semantics | feature:basic-tooltip |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderRendererTest.kt` | 13 | `BorderRenderer`: solid/gradient/shader brushes, fill when too thick, negative/unspecified/zero-size draws nothing, hairline = 1 px, fractional widths round up, inset scopes | feature:border-parity |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderScreenshotTest.kt` | 1 | golden screenshot of a set of borders | n/a: golden-image comparison; the same borders are asserted pixel-wise by `BorderTest`/`BorderRendererTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderSemanticsTest.kt` | 9 | `border` sets the `Shape` semantics property (for accessibility focus outlines), offset by padding, invalidated on shape change | feature:border-parity — needs a `shape` semantics property (ui area) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderTest.kt` | 8 | border with color, brush and shader brush; thicker than the layout fills; width < 0 draws nothing; zero size; RTL | partial — color borders exist (`compose-ui/tests/suite/review_d.rs` `a_thick_border_stays_inside_the_node`); brush borders and `BorderStroke(width, brush)` missing; negative widths and the fill threshold differ (see notes) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CanvasTest.kt` | 5 | `Canvas` draws, `contentDescription` overload semantics, zero size, exact sizes | partial — `Canvas(modifier, on_draw)` exists (`draw_modifiers.rs` `a_canvas_redraws_what_it_read_without_recomposing`); the `contentDescription` overload is missing |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableInScrollableViewGroupTest.kt` | 3 | press indication delayed when Compose sits in a scrollable Android `ViewGroup`, not otherwise | feature:clickable-parity — scrollable-container press delay, with the host telling whether its View parents scroll |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableIndirectPointerInputTest.kt` | 6 | clicks from indirect pointers (touchpads/XR controllers) on the focused clickable; cancel on focus loss | feature:clickable-parity — needs ui's indirect pointer events |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableParameterizedKeyInputTest.kt` | 9 | click by Enter/NumPadEnter/DPadCenter/Space; focus lost while pressed cancels; press/cancel interactions; key repeat | partial — Enter/NumPadEnter/Space clicks exist (`compose-ui/tests/suite/ui.rs` `tab_moves_focus_through_clickables_and_enter_clicks`, `compose-ui/tests/suite/review_d.rs` `enter_and_space_held_together_each_click`); missing `DPadCenter` and the `PressInteraction` stream (`interactions`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableSoundTest.kt` | 9 | click sound effect (Android `playSoundEffect`) on pointer, mouse, key and accessibility clicks; not on long click | feature:clickable-parity — needs a host sound-effect hook (ui area) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt` | 142 | the clickable modifier: semantics, clicks, touch/keyboard-mode focus, press/release/cancel interactions with and without scrollable containers, hover, dynamic pointer-input ordering, `LocalIndication` and `IndicationNodeFactory` creation rules, equality, minimum touch target | partial — clicks, disabled, merge, focus, hover and press-to-source exist (`compose-ui/tests/suite/ui.rs` 8 tests, `modifiers_api.rs` `the_clickable_family_reports_presses_to_its_interaction_source`, `review_round5.rs`, `review_round12.rs`); about 100 scenarios need `interactions`, `indication` and `clickable-parity` (labels, press delay, touch mode) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableWithDynamicConfigChangesTest.kt` | 1 | clicks still work after the host View is removed and re-added with a recreated Recomposer | port — as a compose-android host test (detach and re-attach a `ComposeRsView`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableIndirectPointerInputTest.kt` | 8 | click/long/double click from indirect pointers | feature:clickable-parity — needs ui's indirect pointer events |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableParameterizedKeyInputTest.kt` | 17 | key click, long click by holding a key (no haptics), double click by keys, focus loss cancels, interactions | partial — key clicks exist; holding a key to long-click, key double clicks, `DPadCenter` and interactions are missing |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt` | 103 | `combinedClickable`: semantics incl. long-click label, click/long/double timing, haptics on/off, consumption after a long click, interactions, indication rules, equality | partial — click, long, double, haptics and indication-while-pressed exist (`modifiers_api.rs` 4 tests, `platform_services.rs` 2 tests); missing `onClickLabel`/`onLongClickLabel`, `hapticFeedbackEnabled`, interactions, indication factories |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusGroupTest.kt` | 14 | `Modifier.focusGroup`: not focusable itself, made focusable by `focusProperties`, 1D/2D search entering and leaving groups | feature:focusable-parity — needs ui focus groups and focus properties |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt` | 28 | `focusable`: semantics, touch vs keyboard mode, focus interactions and their reset, pinning in lazy layouts, bring-into-view on focus, clearing on removal/reuse/movable content, click-outside clears focus | partial — focusable, semantics, reuse and removal exist (`compose-ui/tests/suite/review_d.rs` focus tests, `semantics_actions.rs`, `review_reuse_d434.rs` `focus_skips_deactivated_content_and_comes_back_with_it`); missing `interactionSource` parameter, touch mode, auto-clear on outside click |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusedBoundsChangedTest.kt` | 1 | `onFocusedBoundsChanged` is a no-op | n/a: deprecated API (`onFocusedBoundsChanged`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/HoverableTest.kt` | 9 | `hoverable` reports enter/exit, reset when disposed/reused/moved/disabled, no hover when disabled, equality | port — exists (`modifiers_api.rs` `hoverable_reports_the_mouse_over_it`, `review_round6.rs` `a_hovered_clickable_whose_modifiers_change_still_hears_exit`); assertions on the `HoverInteraction` list need `interactions`, the state-level ones port now |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ImageTest.kt` | 13 | `Image` from bitmap (subsection, stretching, scaling, alignment), from vector, crop, `painterResource`, semantics, no-intrinsic-size painter, zero size, `FilterQuality.None`, WebP | partial — bitmap `Image` exists (`image.rs` 5 tests, `review_image.rs` 5 tests); missing `Image(painter)`, `Image(imageVector)`, `colorFilter`, `filterQuality` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/IndicationTest.kt` | 10 | `Modifier.indication` with `Indication` and `IndicationNodeFactory`: draw called, updates, reuse and recreation rules | feature:indication |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/InteractionSourceTest.kt` | 7 | emitting interactions in order; `collectIsDragged/Focused/PressedAsState` with nested interactions | partial — `is_pressed`/`is_focused`/`is_hovered` states exist; missing the `interactions` stream, `emit`/`try_emit`, drag |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/MagnifierTest.kt` | 21 | `Modifier.magnifier`: supported/unsupported, recreated on density/config change, zoom updates, content/center updates | feature:magnifier |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/PlatformMagnifierTest.kt` | 7 | Android platform magnifier wrapper: show, zoom, center, size | feature:magnifier |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/PreferKeepClearTest.kt` | 9 | `preferKeepClear` rects reach the View, removed with the modifier, multiple rects | feature:android-window-rects |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ProgressSemanticsTest.kt` | 2 | `progressSemantics()` (indeterminate) and `(value, range, steps)` | feature:progress-semantics |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/SystemGestureExclusionTest.kt` | 7 | `systemGestureExclusion` rects reach the View, removed with the modifier, multiple rects | feature:android-window-rects |

### content: 3 files, 29 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/content/ReceiveContentTest.kt` | 16 | `contentReceiver` configurations merged bottom to top, `onReceive` chains to parents, drag-and-drop permissions | feature:content-receiver |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/content/TransferableContentTest.kt` | 8 | `TransferableContent.hasMediaType`, plain text reading, `consume` | feature:content-receiver |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/content/internal/DragAndDropRequestPermissionTest.kt` | 5 | drop asks for URI permissions only when needed | feature:content-receiver |

### contextmenu: 4 files, 46 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/contextmenu/ContextMenuAreaTest.kt` | 10 | (internal) context menu area: right click opens a popup at the pointer, left click doesn't, item click closes | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/contextmenu/ContextMenuGestureTest.kt` | 2 | (internal) right-click gesture detection order with other pointer input | feature:text-context-menu — needs pointer buttons (ui area) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/contextmenu/ContextMenuUiTest.kt` | 30 | (internal) context menu items: enabled/disabled, sizes, icons, semantics, colors | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/contextmenu/ContextMenuWithDialogTest.kt` | 4 | text field toolbar/context menu inside a dialog doesn't crash | feature:text-context-menu |

### draganddrop: 3 files, 8 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/draganddrop/AndroidDragAndDropIntegrationTest.kt` | 4 | plain-text drag and drop by touch long press and by mouse | feature:drag-and-drop |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/draganddrop/DragAndDropSourceTest.kt` | 1 | a drag source doesn't block child invalidations | feature:drag-and-drop |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/draganddrop/DragDropTargetTest.kt` | 3 | changing a drop target updates the modifier; `shouldStartDragAndDrop` is the latest | feature:drag-and-drop |

### relocation: 3 files, 48 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/relocation/BringIntoViewRequesterViewIntegrationTest.kt` | 4 | `bringIntoView` reaches `View.requestRectangleOnScreen`, and back | feature:bring-into-view-requester — plus the Android host bridge |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/relocation/BringIntoViewResponderTest.kt` | 20 | `BringIntoViewRequester` through responders: rects, child sizes, propagation, interruption of overlapping requests | feature:bring-into-view-requester |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/relocation/BringIntoViewScrollableInteractionTest.kt` | 24 | a requester inside scrollables scrolls the least needed (leading, trailing, centred, bigger than the viewport) | feature:bring-into-view-requester — the scrolling side exists for focus (`compose-foundation/src/scroll.rs` `bring_into_view`); shared with the gestures/scrolling ledger |

### selection: 2 files, 88 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/SelectableTest.kt` | 33 | `selectable`: semantics, clicks, disabled, interactions with/without scrollable container, keys (Enter, NumPadEnter, DPadCenter), indication equality rules | partial — exists (`modifiers_api.rs` `selectable_and_selectable_group_set_their_semantics`); missing `DPadCenter`, interactions, indication factories |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt` | 55 | `toggleable` and `triStateToggleable`: semantics, autofill, toggling, interactions, minimum touch target, keys, indication equality | partial — `toggleable` exists (`compose-ui/tests/suite/ui.rs` `toggleable_calls_back_with_the_new_value`, `semantics_actions.rs`); missing `triStateToggleable`, interactions, `DPadCenter`, autofill (ui area) |

### shape: 10 files, 117 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteCutCornerShapeTest.kt` | 9 | `AbsoluteCutCornerShape` outlines, equality, `copy` | feature:shapes |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/AbsoluteRoundedCornerShapeTest.kt` | 10 | `AbsoluteRoundedCornerShape` outlines (no RTL mirroring), equality, `copy` | feature:shapes |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerBasedShapeTest.kt` | 7 | `CornerBasedShape`: corners scaled to the minimum dimension per side, 100 %, `copy` defaults, layout direction | feature:shapes — compose-rs clamps each corner to half the shorter side instead (see notes) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CornerSizeTest.kt` | 6 | `CornerSize` in px, dp, percent, zero; equality | partial — `CornerSize::Dp` and `Percent` exist; px sizes and `ZeroCornerSize` missing |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/CutCornerShapeTest.kt` | 10 | `CutCornerShape` outlines incl. RTL, equality, `copy` | feature:shapes |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/MorphPolygonShapeTest.kt` | 13 | `MorphPolygonShape` between polygons: progress clamping, caching, size changes, conversions | feature:polygon-shapes |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/PolygonShapeIntegrationTest.kt` | 3 | demo polygon shapes resolve and morph at several sizes | feature:polygon-shapes |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/PolygonShapeTest.kt` | 28 | `PolygonShape` builders (regular, star, pill, pill star, custom vertices, rounding) | feature:polygon-shapes |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/RoundedCornerShapeTest.kt` | 11 | `RoundedCornerShape` outlines incl. RTL, rectangle for zero corners, equality, `copy`, not equal to a cut shape | partial — exists (`compose-ui/src/graphics.rs` `rounded_corners_are_clamped_to_half_the_shorter_side`); `copy` and the cut-shape comparison need `shapes` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/shape/TransformedPolygonShapeTest.kt` | 20 | `PolygonShape.transform`/`scaleToFit` | feature:polygon-shapes |

### style: 4 files, 100 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/style/StyleEquivalenceTests.kt` | 18 | styles draw the same as the equivalent modifiers (background, border, padding, shape, alpha, size, color filter) | feature:style-api |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/style/StyleLayoutTest.kt` | 29 | style layout properties (padding, sizes, min/max, fill, fractions) | feature:style-api |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/style/StyleResolverNodeTest.kt` | 8 | style resolver node reuse and resolver swaps | feature:style-api |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/style/StyleUxTaskTests.kt` | 45 | end-to-end style tasks (states, animation, nesting) | feature:style-api |

### text (BasicText, legacy field internals, keyboard): 49 files, 322 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextBrushTest.kt` | 3 | toggling a solid, predefined-shader or custom-shader `Brush` in a `BasicText` style redraws | feature:basic-text-drawing — `TextStyle` has no `brush` yet (ui-text area) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextDensityTest.kt` | 3 | a density change lays out simple, multi-paragraph and selectable text again | covered — partly: `text_reuse.rs` `a_new_density_or_direction_lays_the_text_out_anew_even_with_the_optimization` (string text); the annotated and selectable cases are left |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextGraphicsLayerTest.kt` | 2 | text modifiers don't add a graphics layer (string and annotated) | port — assert on the display list that a `BasicText` is no repaint boundary |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextHoverTest.kt` | 18 | pointer icon over selectable and non-selectable text inside boxes setting their own icon, with and without `overrideDescendants` | feature:hover-icon-override — selectable text shows no I-beam today, and `pointer_hover_icon` has no `override_descendants` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextIntrinsicWidthWrappingRegressionTest.kt` | 2 | text measured at its intrinsic width doesn't wrap (regression) | port — D104's `finalMaxWidth` rounding is what it checks |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextLayoutTest.kt` | 2 | 2^14+ character texts lay out without throwing (string and annotated) | port |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextLinkInteropTest.kt` | 1 | links in an `AndroidView` measured several times don't recompose forever | feature:text-links |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextLinkTest.kt` | 45 | `LinkAnnotation` in `BasicText`: focus order, clicks inside/outside/between links, RTL, styles per state, semantics, `LocalUriHandler` | feature:text-links |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextMinMaxLinesTest.kt` | 9 | `minLines`/`maxLines` change the height, introduced later take effect, validation | covered — partly: `forms.rs` `a_text_is_at_least_min_lines_high_and_reports_its_layout`, `review_forms.rs` `an_empty_text_with_min_lines_is_that_high` and `min_lines_above_max_lines_is_rejected_as_compose_does`, `foundation.rs` `basic_text_wraps_and_limits_its_lines`; the "introduced later" and monotonic cases are left |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextMinSizeTest.kt` | 2 | a smaller minimum size constraint shrinks the text (string and annotated) | covered — partly: `review_e.rs` `a_text_shrinks_when_its_minimum_height_goes_away` (string only) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextPrefetchTest.kt` | 11 | text layout prefetched on a background executor (`LocalBackgroundTextMeasurementExecutor`) during composition | feature:background-text-measurement |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextScreenshotTest.kt` | 4 | goldens: centred ellipsis margin; multi-style weight/family/style | port — as Skia raster checks (`compose-skia/tests/suite/skia.rs` `annotated_text_draws_each_span_in_its_style` is close), not goldens |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextSemanticsTest.kt` | 2 | semantics text follows a changed string/annotated string | port |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextUnexpectedWrappingRegressionTest.kt` | 1 | a text doesn't wrap unexpectedly at its measured width (regression) | port |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/ClickableTextTest.kt` | 2 | `ClickableText` click callback and its update | n/a: deprecated API (`ClickableText`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldFocusTest.kt` | 4 | legacy field: input session disposed on removal/focus clear; tap to focus | n/a: deprecated value-based `BasicTextField`; the state-based `BasicTextFieldTest` has the same scenarios |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldHandwritingBoundsTest.kt` | 2 | legacy field: stylus handwriting bounds | n/a: deprecated value-based `BasicTextField`; see `BasicTextFieldHandwritingBoundsTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldHandwritingGestureTest.kt` | 58 | legacy field: handwriting gestures | n/a: deprecated value-based `BasicTextField`; see `BasicTextFieldHandwritingGestureTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldHandwritingTest.kt` | 13 | legacy field: stylus handwriting start rules | n/a: deprecated value-based `BasicTextField`; see `BasicTextFieldHandwritingTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldHighlightTest.kt` | 1 | legacy field: autofill highlight colour | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldHoverTest.kt` | 6 | legacy field: pointer icons | n/a: deprecated value-based `BasicTextField`; see `BasicTextFieldHoverTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldInputServiceIntegrationTest.kt` | 16 | legacy field: IME options and keyboard show/hide with focus | n/a: deprecated value-based `BasicTextField`; see `BasicTextFieldImmIntegrationTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldKeyboardScrollableInteractionTest.kt` | 1 | legacy field: keyboard opening scrolls the field into view | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldPlatformSelectionBehaviorsTest.kt` | 3 | legacy field: smart selection | n/a: deprecated value-based `BasicTextField`; see `TextFieldPlatformSelectionBehaviorsTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldSelectionOnBackTest.kt` | 2 | legacy field: back clears the selection | n/a: deprecated value-based `BasicTextField`; see `TextFieldSelectionOnBackTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextFieldSoftWrapTest.kt` | 2 | legacy field: `softWrap` false takes the max intrinsic width | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/CoreTextInlineContentTest.kt` | 6 | `BasicText` inline content: placeholder size changes, placement in RTL layouts and bidi text | feature:inline-content |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/DeadKeyCombinerTest.kt` | 6 | hardware-keyboard dead keys combine with the next key | feature:dead-keys |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/DefaultKeyboardActionsTest.kt` | 3 | no action given runs the default; `defaultKeyboardAction` from a custom one; a different default than the key pressed | partial — default actions exist (`forms.rs` `the_default_actions_move_the_focus_and_hide_the_keyboard`, `text_input.rs` `next_without_an_action_moves_focus_on`); `KeyboardActionScope.defaultKeyboardAction` missing (`keyboard-options-parity`). Written against the deprecated field; port against the state-based one |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/DetectDownAndDragGesturesWithObserverInitializationTest.kt` | 1 | (internal) text drag-gesture helper records press and drag interactions | n/a: internal gesture helper; field drags are checked by `TextFieldDoubleTapTest`/`TextFieldLongPressTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/DrawPhaseAttributesToggleTest.kt` | 3 | draw-only style changes (color, decoration) redraw without relayout in `BasicText`, annotated text and a field | covered — partly: `text_reuse.rs` `a_paint_only_style_change_keeps_the_layout_and_draws_it_again_as_compose` (string text); the annotated and field cases are left |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/FontScalingScreenshotTest.kt` | 8 | non-linear font scaling (2×) with sp/em line heights and `drawText` | partial — linear font scale exists; Android 14's non-linear `FontScaleConverter` (ui-unit) and `drawText` (ui-text) are other areas |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/IntrinsicTextAsyncFontTest.kt` | 4 | intrinsic rows measure again when an async font loads | feature:ui-text-async-fonts — compose-rs resolves system fonts synchronously |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/KeyEventHelpersTest.kt` | 3 | (internal) back key down/up helper | n/a: internal helper; behaviour in `TextFieldSelectionOnBackTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/KeyboardActionsTest.kt` | 2 | the action for the IME action runs; a custom `Done` doesn't hide the keyboard | covered — `text_input.rs` `enter_in_a_single_line_field_performs_its_action`, `forms.rs` `the_input_methods_action_key_runs_the_fields_action` (on compose-rs's value-based field) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/MinLinesMemoryLeakTest.kt` | 1 | the min-lines constrainer doesn't leak | port — check the per-style line-height cache stays bounded |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/PlatformSelectionBehaviorCommonTestCases.kt` | 5 | (abstract) smart selection via the text classifier on long press/double click | feature:smart-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/PointerMoveDetectorTest.kt` | 3 | (internal) pointer move detector: moves, first pointer, hand-off | n/a: internal helper |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextDelegateIntegrationTest.kt` | 8 | (internal, legacy) `TextDelegate`: intrinsics, relayout, ellipsis | n/a: internal legacy delegate; `ParagraphLayoutCacheTest` has the same scenarios |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextDelegateWidthWithLetterSpacingTest.kt` | 4 | (internal, legacy) `TextDelegate` width with letter spacing and line height | n/a: internal legacy delegate; see `MultiParagraphLayoutCacheWidthWithLetterSpacingTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextFieldDelegateIntegrationTest.kt` | 7 | (internal, legacy) field drawing of selection/highlight, height constraints | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextFieldInteractionsTest.kt` | 7 | legacy field: press, focus, drag interactions | n/a: deprecated value-based `BasicTextField`; see `TextFieldInteractionSourcePressTest`, `TextFieldFocusTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextLayoutDirectionTest.kt` | 1 | legacy field gets the layout direction | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextLayoutResultIntegrationTest.kt` | 11 | (internal, legacy) `TextDelegate` layout results: sizes, positions, overflow | n/a: internal legacy delegate; see `text/modifiers/TextLayoutResultIntegrationTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextLayoutTest.kt` | 6 | `BasicText` layout: size, intrinsics, baselines (unconstrained, zero width), `onTextLayout`, min width change with end alignment | partial — size and `on_text_layout` exist (`review_forms.rs` 3 tests); alignment lines (`FirstBaseline`/`LastBaseline`, layout area) missing |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextOverflowTest.kt` | 2 | `TextOverflow.Visible` paints beyond the bounds (single and multi paragraph) | port — `TextOverflow::Visible` exists, untested |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextPreparedSelectionTest.kt` | 10 | (internal, legacy) cursor movements: characters LTR/RTL/bidi, words, paragraphs, lines | n/a: legacy `TextPreparedSelection`; the state-based `TextFieldKeyEventTest` has the scenarios |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextStyleInvalidationTest.kt` | 1 | which style attribute changes lay out, draw or do nothing | covered — `text_reuse.rs` `layout_affecting_attributes_are_composes` and `a_paint_only_style_change_keeps_the_layout_and_draws_it_again_as_compose` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/TextUsingModifierMinMaxLinesTest.kt` | 5 | min/max lines bound the height by the line height; validation | covered — partly: `forms.rs`, `review_forms.rs` (as `BasicTextMinMaxLinesTest`) |

### text/contextmenu: 8 files, 75 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/contextmenu/internal/AndroidTextContextMenuToolbarProviderTest.kt` | 13 | Android floating-toolbar provider: items, cancellation, replacement, closing | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/contextmenu/internal/DefaultTextContextMenuDropdownProviderTest.kt` | 14 | default dropdown provider: items, cancellation, replacement, click off | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/contextmenu/internal/PlatformDefaultTextContextMenuProvidersTest.kt` | 4 | default providers created only when none is provided | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/contextmenu/modifier/TextContextMenuGesturesModifierTest.kt` | 6 | right click (not tap or left click) opens the menu at the right position | feature:text-context-menu — needs pointer buttons (ui area) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/contextmenu/modifier/TextContextMenuModifierTraversalTest.kt` | 7 | `appendTextContextMenuComponents`/`filterTextContextMenuComponents` traversal order | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/contextmenu/modifier/TextContextMenuToolbarHandlerTest.kt` | 10 | toolbar show/hide through the provider, attach/detach | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/contextmenu/provider/BasicTextContextMenuProviderTest.kt` | 9 | context menu provider: session close, cancellation, anchor removal and moves | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/contextmenu/test/ContextMenuFlagFlipperRunnerTest.kt` | 12 | Compose's JUnit runner flipping the new-context-menu flag | n/a: tests Compose's own test runner |

### text/input (state-based BasicTextField): 33 files, 636 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicSecureTextFieldTest.kt` | 44 | `BasicSecureTextField`: password semantics, obfuscation modes (`RevealLastTyped`, hidden, visible), last typed char revealed then hidden, no copy/cut, IME options | feature:secure-text-field — compose-rs's `PasswordVisualTransformation` covers masking and no copy/cut on the value-based field (`text_input.rs` 2 tests) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldAnnotatedOutputTransformationTest.kt` | 3 | an output transformation adding styles is applied to the layout | feature:output-transformation — with `text-field-styles` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldDrawPhaseToggleTest.kt` | 4 | color/brush/shadow/decoration changes redraw without relayout | feature:text-field-state |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldHandwritingBoundsTest.kt` | 2 | stylus inside the editor bounds focuses and starts handwriting; overlapping editors | feature:handwriting |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldHandwritingGestureTest.kt` | 60 | handwriting gestures: select, delete, insert, join, remove space, preview | feature:handwriting |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldHandwritingTest.kt` | 13 | stylus handwriting starts only on stylus moves, not for disabled/read-only fields | feature:handwriting |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldImeSelectionChangesTest.kt` | 4 | IME select-all, set selection, set composing text, send key event update the state | feature:android-ime |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldImmIntegrationTest.kt` | 23 | the input method manager follows focus and window focus; restarts on state/filter changes | feature:android-ime |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldLayoutPhaseToggleTest.kt` | 3 | font weight, alignment and min-width changes relayout | feature:text-field-state |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldSemanticsTest.kt` | 40 | field semantics: editable text, SetText/InsertTextAtCursor/SetSelection, copy/cut/paste actions, IME action, transformations, content types, password | feature:text-field-state — needs ui semantics text actions; compose-rs has only `editable_text`/`password`/`editable` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldSendKeyEventTest.kt` | 2 | IME key events don't restart input | feature:android-ime |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldStyledTextTest.kt` | 14 | styled text in the state (span and paragraph styles) laid out, ranges follow edits, with transformations | feature:text-field-styles |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/BasicTextFieldTest.kt` | 69 | the state-based field: rendering, state updates, relayout rules, keyboard show/hide, filters on every input path, change tracking, keyboard options merging, context-menu actions, default width, composition underline, long text | feature:text-field-state — many scenarios already hold for the value-based field (`foundation.rs`, `review_e.rs`, `forms.rs`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/CommonTextFieldKeyEventTest.kt` | 40 | arrows, numpad, Page Up/Down, Home/End with and without Shift; Shift/Ctrl+Insert; Shift/Ctrl+Enter newline | feature:text-field-state — the value-based field has most moves (`review_f*.rs`); Page Up/Down, numpad keys and Insert are missing |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/HandwritingDetectorTest.kt` | 7 | `handwritingDetector` delegates stylus handwriting, extended bounds, hover icon | feature:handwriting |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/HandwritingHandlerTest.kt` | 1 | `handwritingHandler` accepts delegation on focus | feature:handwriting |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/HandwritingHoverIconTest.kt` | 1 | stylus hover shows the handwriting icon | feature:handwriting |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/HeightInLinesModifierCalculationTest.kt` | 1 | line-limit height calculation | feature:text-field-state — the value-based field's `min_lines`/`max_lines` heights exist (`forms.rs` `a_multi_line_field_is_between_min_lines_and_max_lines_high`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/HeightInLinesModifierTest.kt` | 21 | `TextFieldLineLimits.MultiLine` heights: short/long text, validation, async fonts, density and direction changes | feature:text-field-state |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/RememberTextFieldStateTest.kt` | 4 | `rememberTextFieldState` initial text/selection, restore after recreation incl. styles | feature:text-field-state |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldCodepointTransformationTest.kt` | 31 | (internal) codepoint transformation rendering, offset mapping, toggling, with state | feature:secure-text-field — the masking path; single-line uses it too |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldCursorTest.kt` | 26 | cursor drawing LTR/RTL, coerced into view, brush, blinking (and reduced motion), hidden while typing/selecting/unfocused/read-only, drag-and-drop cursor | feature:text-field-state — with `cursor-blink` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldDecoratorTest.kt` | 9 | `TextFieldDecorator`: focus, semantics, clicks and long clicks on the decoration; unplaced inner field still takes input | feature:text-field-state — the value-based `decoration_box` behaves so (D102) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldDragAndDropTest.kt` | 23 | dropping text or other content into a field, selection follows the drag, hover interactions | feature:drag-and-drop — with `content-receiver` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldFocusTest.kt` | 35 | focus: request, focus interactions, focus bounds (cursor, selection), text input started from effects, D-pad and Tab navigation | feature:text-field-state — with `interactions` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldKeyEventTest.kt` | 60 | hardware keys: typing, copy/paste/cut, line/word/paragraph/page navigation, deletions, Tab/Enter rules, undo/redo, key-up propagation | feature:text-field-state — most scenarios already hold for the value-based field (`foundation.rs`, `review_e.rs`, `review_f.rs`, `review_f2.rs`, `review_f3.rs`); page navigation is missing |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldKeyboardActionsTest.kt` | 14 | `KeyboardActionHandler`: semantics and IME actions, defaults for Next/Previous/Done, overriding and requesting the default, Enter in single/multi line | feature:text-field-state — the value-based field's actions exist (`text_input.rs`, `forms.rs`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldOutputTransformationGesturesIntegrationTest.kt` | 3 | clicks around output replacements/insertions put the cursor at their edges | feature:output-transformation |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldOutputTransformationHardwareKeysIntegrationTest.kt` | 20 | cursor movement by keys across output replacements/insertions in state and semantics offsets | feature:output-transformation |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldReceiveContentTest.kt` | 11 | `contentReceiver` on fields: commitContent, EditorInfo MIME types, delegation to parents | feature:content-receiver |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldScrollTest.kt` | 34 | field scrolling: horizontal/vertical, viewport and content sizes, swipes, restore, follow the cursor when focused, RTL, nested scrolling, inside scrollable containers | feature:text-field-state — cursor-in-view scrolling exists on the value-based field (`review_f.rs` `a_single_line_field_keeps_its_cursor_in_view`, `review_f2.rs` `a_multi_line_field_keeps_its_cursor_in_view`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldSingleLineHeightTest.kt` | 6 | single-line height follows tall text; maxLines=1 vs single line | feature:text-field-state |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldSizeModifierTest.kt` | 8 | field size: single line fixed height, multi-line clamped, async fonts, parent constraints, empty text minimum | feature:text-field-state |

### text/input/internal: 18 files, 332 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/AndroidTextInputSessionTest.kt` | 7 | Android text input session: EditorInfo, updates, editor actions, extracted text | feature:android-ime |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/BasicTextFieldHoverTest.kt` | 6 | pointer icons over the field with boxes setting their own icon | feature:text-field-state — the I-beam exists on the value-based field (`hover.rs` `a_text_field_shows_an_i_beam`); overriding needs `hover-icon-override` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/ClipboardPasteStateTest.kt` | 3 | paste availability tracked without reading the clip | feature:text-context-menu — needs `ClipEntry`-style clipboard metadata (ui area) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/ComposeInputMethodManagerTest.kt` | 2 | each `restartInput` creates a new input connection | feature:android-ime |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/CursorAnchorInfoBuilderTest.kt` | 23 | `CursorAnchorInfo` built from the layout: selection, composition, insertion marker, character bounds | feature:android-ime |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/DragAndDropTestUtilsTest.kt` | 3 | Compose's drag-and-drop test utilities | n/a: tests Compose's test utilities |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/EditorInfoTest.kt` | 57 | `EditorInfo` from `KeyboardOptions`: input types, IME actions, flags, hint locales, initial selection, MIME types | partial — `compose-android/src/input_method.rs` maps types, actions, capitalization and auto-correct (9 tests); missing initial selection/surrounding text, hint locales, `showKeyboardOnFocus`, content MIME types |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/LegacyCursorAnchorInfoBuilderTest.kt` | 25 | legacy `CursorAnchorInfo` builder | n/a: deprecated value-based `BasicTextField`'s IME path; see `CursorAnchorInfoBuilderTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/LegacyEditorInfoTest.kt` | 48 | legacy `EditorInfo` table | n/a: deprecated value-based `BasicTextField`'s IME path; see `EditorInfoTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/LegacyTextInputMethodRequestCursorAnchorInfoTest.kt` | 5 | legacy cursor updates requests | n/a: deprecated value-based `BasicTextField`'s IME path |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/LegacyTextInputMethodRequestEmojiTest.kt` | 1 | legacy EmojiCompat EditorInfo | n/a: deprecated value-based `BasicTextField`'s IME path |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/LegacyTextInputMethodRequestOnStateUpdateTest.kt` | 8 | legacy restartInput/updateSelection rules | n/a: deprecated value-based `BasicTextField`'s IME path |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/OutputTransformationPerGlyphTest.kt` | 21 | (internal) per-glyph replacement (emoji clusters, mixed content) used for masking | feature:secure-text-field |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/RecordingInputConnectionTest.kt` | 32 | legacy `InputConnection`: text around the cursor, batch edits, composing, deletions, key events | n/a: deprecated value-based `BasicTextField`'s IME path; see `StatelessInputConnectionTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/SingleLineOutputTransformationTest.kt` | 4 | (internal) single line shows `\n` as a space and `\r` as zero-width space | feature:text-field-state — compose-rs's single-line field drops typed or pasted `\n` instead, and draws a value's `\n` and `\r` as they are (see notes) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/StatelessInputConnectionTest.kt` | 37 | state-based `InputConnection`: text around the cursor, batch commits, commitContent, suggestion spans, composing spans | feature:android-ime |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/TextFieldLayoutStateCacheTest.kt` | 45 | (internal) when the field's layout cache invalidates: text, transformations, style, constraints, density, direction, line limits | feature:text-field-state — the value-based field's node has its own rules; port at the behaviour level |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/TextInputServiceAndroidCursorAnchorInfoTest.kt` | 5 | legacy `TextInputServiceAndroid` cursor updates | n/a: deprecated value-based `BasicTextField`'s IME path |

### text/input/internal/selection: 16 files, 274 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/PressDownTest.kt` | 5 | (internal) press-down detection: consumed, long press, move out | n/a: internal gesture helper; behaviours in `TextFieldClickToMoveCursorTest`/`TextFieldLongPressTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldClickToMoveCursorTest.kt` | 11 | a click places the cursor: empty field, LTR/RTL text and layouts, empty regions, scrolled edges, read-only, in a dialog | feature:text-field-state — exists on the value-based field (`foundation.rs` `a_tap_focuses_a_text_field_and_places_the_cursor`, `review_e.rs` `text_typed_right_after_a_tap_goes_where_the_tap_put_the_cursor`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldConfigChangeAppCompatActivityLocaleTest.kt` | 1 | toolbar strings follow a locale change | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldCursorHandleTest.kt` | 36 | touch cursor handle: position LTR/RTL, touch size, drag, visibility after typing/scrolling | feature:touch-text-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldDoubleTapTest.kt` | 25 | double tap selects a word, drags extend by words in LTR/RTL, scrolled/decorated fields, handles hidden while scrolling | feature:text-field-state — double-click word selection and word drags exist for the mouse (`review_round12.rs`, `review_f3.rs`); the handle cases need `touch-text-selection` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldInteractionSourcePressTest.kt` | 5 | press/release/cancel interactions from the field and its decoration | feature:interactions — with `text-field-state` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldLongPressTest.kt` | 31 | long press: focus after release, cursor at the end of empty regions, word selection, toolbar, drag to extend | feature:touch-text-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldMagnifierTest.kt` | 19 | magnifier follows the dragged cursor/handles within the line, hides far past it | feature:magnifier — with `touch-text-selection` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldPlatformSelectionBehaviorsTest.kt` | 4 | smart selection with output and codepoint transformations | feature:smart-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldSelectionHandlesTest.kt` | 32 | selection handles: shown when focused by touch, hidden in keyboard mode, positions, drags, crossing, scrolling | feature:touch-text-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldSelectionOnBackTest.kt` | 4 | back clears the selection and isn't propagated; with a cursor handle it isn't consumed | feature:text-field-state — compose-rs's field doesn't take back/Escape to collapse its selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldShiftClickTest.kt` | 9 | Shift+click extends the selection: LTR/RTL, empty regions, read-only | feature:text-field-state — needs keyboard modifiers on pointer events (ui area; none today, as 06-ui says) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldTextContextMenuToolbarTest.kt` | 41 | toolbar (new context menu) shown on handle tap, long press, double tap; hidden on edits, keys, scroll | feature:text-context-menu — with `touch-text-selection` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldTextToolbarTest.kt` | 40 | `TextToolbar` shown on handle tap, long press, double tap; hidden on edits, keys, scroll | feature:touch-text-selection — needs ui's `TextToolbar` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/TextFieldTripleTapTest.kt` | 7 | triple tap selects the paragraph, drags extend by paragraphs | feature:text-field-state — the value-based field's triple click selects the hard line (`review_f3.rs` `a_triple_click_selects_the_line`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/selection/gesture/TextFieldScrolledSelectionGestureTest.kt` | 4 | long-press and handle drags in a scrolled field | feature:touch-text-selection |

### text/input/internal/undo: 1 file, 15 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/internal/undo/BasicTextFieldUndoTest.kt` | 15 | undo/redo of IME inserts, merging rules, selection restored, paste/cut never merge, programmatic edits, `clearHistory` | feature:undo-parity — compose-rs has undo on the value-based field (`review_f2.rs` `command_z_undoes_typing`, D118: 5 s grouping, not Compose's merge rules) |

### text/modifiers (BasicText nodes): 13 files, 119 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/BasicTextSemanticsTest.kt` | 3 | semantics text follows changes; inline content semantics merge | port — the inline-content case needs `inline-content` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/LayoutUtilsKtTest.kt` | 9 | (internal) `finalConstraints`, `finalMaxWidth`, `finalMaxLines` (ellipsis without soft wrap forces one line) | covered — partly: `compose-foundation/src/text.rs` `the_final_width_is_composes_final_max_width`, `review_text_draw.rs` `an_ellipsized_text_without_soft_wrap_is_one_line`; the huge-constraint and negative-lines cases are left |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/MinLinesConstrainerTest.kt` | 3 | (internal) min-lines heights recomputed on a style change, reused otherwise | port — compose-rs keeps one- and two-line heights per style and density (D233) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/MultiParagraphLayoutCacheTest.kt` | 37 | annotated text's layout cache: intrinsics, min-lines intrinsics, relayout for height, density, font scale, ellipsis, constraints, direction | port — the rules exist (D550); `text_reuse.rs` (7 tests) and `review_d550.rs` (6 tests) cover string text and a few cases |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/MultiParagraphLayoutCacheWidthWithLetterSpacingTest.kt` | 4 | annotated text width with letter spacing and line height | port |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/NodeInvalidationTestParent.kt` | 11 | (abstract) which `BasicText` parameter changes invalidate layout, draw or semantics | partial — the style rules are checked (`text_reuse.rs`); the `color` lambda and brush cases need `basic-text-drawing` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/ParagraphLayoutCacheTest.kt` | 20 | string text's layout cache: intrinsics, relayout for height, density, font scale, ellipsis, max lines, constraints | port — the rules exist (D550); `text_reuse.rs` and `review_d550.rs` cover part of them |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/SelectionControllerTest.kt` | 2 | selectable text's highlight is clipped unless `TextOverflow.Visible` | covered — partly: `review_round14.rs` `the_highlight_of_a_clipped_text_is_clipped_too`; the `Visible` case is left |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/TextAnnotatedStringContentCaptureInvalidationTest.kt` | 4 | content capture and translation invalidated only by text changes | feature:ui-text-substitution |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/TextAnnotatedStringNodeTest.kt` | 7 | annotated text node: bullet/draw annotations invalidation, draw while detached, huge constraints, movable content, text substitution | partial — the crash-safety and movable-content cases port now; bullets (ui-text) and text substitution are other areas |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/TextLayoutResultIntegrationTest.kt` | 11 | `TextLayoutResult` from `BasicText`: size, positions for offsets, overflow by lines or height, visual overflow | partial — the snapshot's size and overflow flags exist (`review_forms.rs`); queries (`getCursorRect`, `getBoundingBox`, …) are ui-text's |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/TextStringContentCaptureTest.kt` | 2 | content capture/translation for string text, with inline content | feature:ui-text-substitution |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/modifiers/TextStringSimpleNodeTest.kt` | 6 | string text node: draw while detached, huge constraints, movable content, async text resolution, text substitution | partial — as `TextAnnotatedStringNodeTest` |

### text/selection: 17 files, 241 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/AbstractSelectionMagnifierTests.kt` | 28 | (abstract) magnifier during selection drags: centre, hidden when empty or without handles, bidi, empty lines | feature:magnifier — with `touch-text-selection` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/MultiWidgetSelectionDelegateTest.kt` | 58 | (internal) per-text selection delegate: handle positions LTR/RTL/bidi, text, line ranges, bounds | feature:touch-text-selection — compose-rs's `Selectable` (D151) has no handle positions |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/PlatformSelectionBehaviorsImplTest.kt` | 2 | text classification results preload icons | feature:smart-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionContainerContextMenuBuilderTest.kt` | 7 | SelectionContainer context-menu items (copy, select all) for touch and mouse | feature:text-context-menu |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionContainerContextMenuTest.kt` | 13 | SelectionContainer right-click menu: appears, dismisses, copy and select all | feature:text-context-menu — needs pointer buttons (ui area) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionContainerFocusTest.kt` | 4 | a tap cancels; selecting in another container cancels the old one; leaving the composition hides the toolbar | partial — tap-to-cancel exists (`selection.rs` `a_selection_is_highlighted_and_a_click_clears_it`); the cross-container and toolbar cases need `touch-text-selection` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionContainerMagnifierTest.kt` | 3 | magnifier on overflowed text and RTL empty lines | feature:magnifier |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionContainerPlatformSelectionsBehaviorTest.kt` | 2 | smart selection across several texts | feature:smart-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionContainerPointerTest.kt` | 21 | mouse and trackpad selection: drags across texts, double click, clicks clear, losing focus clears, buttons inside, starting between texts | covered — partly: `selection.rs` (5 tests), `review_round14.rs` (8 tests) cover drags, double clicks, clearing and focus loss; starting between texts and the trackpad variants are left |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionContainerTest.kt` | 38 | SelectionContainer: long-press word selection (LTR/RTL), drags across texts, overflowed/zero-size texts, copy key, buttons inside, auto-scroll while dragging outside, lazy ordering | partial — mouse drags, copy and `DisableSelection` exist (`selection.rs`, `review_round14.rs`); touch long press, handles and auto-scroll are missing |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionCopyTest.kt` | 2 | copy puts the selection in the clipboard; editing the text drops the selection and copies nothing | covered — `selection.rs` `a_drag_selects_across_texts_and_copy_takes_it`, `review_round14.rs` `a_selected_text_that_changes_drops_the_selection`; port the clipboard-unchanged assertion |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionHandlePopupPositionTest.kt` | 6 | selection handle popups placed for LTR/RTL; semantics positions | feature:touch-text-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionHandlesTest.kt` | 9 | handle icons, visibility, semantics, direction | feature:touch-text-selection |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/SelectionStateTest.kt` | 34 | `SelectionState`: saved across recreation, selected texts, select all, clear, extend by word, with gestures and handles | feature:selection-state |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/TextFieldMagnifierTest.kt` | 8 | legacy field: magnifier on cursor drags | n/a: deprecated value-based `BasicTextField`; see `text/input/internal/selection/TextFieldMagnifierTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/TextFieldVisualTransformationMagnifierTest.kt` | 2 | legacy field with a visual transformation: magnifier | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/TextSelectionColorsScreenshotTest.kt` | 4 | default and custom selection colors for text and a field | feature:text-selection-colors — the field half uses the legacy field; port it with the state-based one |

### textfield (legacy value-based field): 17 files, 298 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/HardwareKeyboardTest.kt` | 41 | legacy field: hardware keys | n/a: deprecated value-based `BasicTextField`; the state-based `TextFieldKeyEventTest` has the scenarios |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/HeightInLinesModifierCalculationTest.kt` | 1 | legacy field: line height calculation | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/HeightInLinesModifierTest.kt` | 22 | legacy field: min/max lines | n/a: deprecated value-based `BasicTextField`; see `text/input/HeightInLinesModifierTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldContextMenuTest.kt` | 50 | right-click context menu on fields (cut, copy, paste, select all, autofill), legacy (btf1) and state-based (btf2) halves | feature:text-context-menu — the 25 btf2 tests; the 25 btf1 tests are n/a (deprecated field) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldCursorTest.kt` | 15 | legacy field: cursor | n/a: deprecated value-based `BasicTextField`; see `text/input/TextFieldCursorTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldDefaultWidthTest.kt` | 6 | legacy field: default width | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldFocusCustomDialogTest.kt` | 2 | legacy field: keyboard in an Android dialog | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldFocusTest.kt` | 20 | legacy field: focus | n/a: deprecated value-based `BasicTextField`; see `text/input/TextFieldFocusTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldOnValueChangeTextFieldValueTest.kt` | 6 | legacy field: `onValueChange` called once per IME command | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldScrollTest.kt` | 18 | legacy field: scrolling | n/a: deprecated value-based `BasicTextField`; see `text/input/TextFieldScrollTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldSelectionTest.kt` | 25 | legacy field: handles and toolbar | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldTest.kt` | 52 | legacy field: general behaviour | n/a: deprecated value-based `BasicTextField`; see `text/input/BasicTextFieldTest` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldTextContextMenuBuilderTest.kt` | 30 | context-menu items by clipboard and selection, btf1 and btf2 halves | feature:text-context-menu — the 15 btf2 tests; the btf1 half is n/a (deprecated field) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldToolbarTest.kt` | 2 | a decoration not calling the inner field doesn't crash on long press (btf1 and btf2) | feature:touch-text-selection — the btf2 test; the btf1 one is n/a |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldUndoTest.kt` | 2 | legacy field: undo/redo with Ctrl+Shift+Z and Ctrl+Y | n/a: deprecated value-based `BasicTextField` (compose-rs's field does both: `review_f2.rs` `command_z_undoes_typing`) |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldVisualTransformationCursorTest.kt` | 4 | legacy field: long press with visual transformation | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/textfield/TextFieldVisualTransformationSelectionBoundsTest.kt` | 2 | legacy field: offset-map checks in the first composition | n/a: deprecated value-based `BasicTextField` |

### Host tests (`androidHostTest`): 71 files, 1316 tests

| File (relative to `frameworks/support`) | @Test | Covers | Status |
|---|---:|---|---|
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt` | 8 | `MutatorMutex`: newer mutators cancel older by priority, `tryMutate`, try-lock | feature:mutator-mutex |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/contextmenu/ContextMenuPopupPositionProviderTest.kt` | 18 | (internal) context menu popup positioning in the window, LTR/RTL, affinities | feature:text-context-menu |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/contextmenu/ContextMenuStateTest.kt` | 7 | (internal) context menu state open/closed and equality | feature:text-context-menu |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/shape/ShapeInterpolationTests.kt` | 14 | lerp between rounded, cut, absolute and rectangle shapes | feature:shapes |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/style/CommonStyleTest.kt` | 7 | combining `CommonStyle`s | feature:style-api |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/style/StyleCollectorTest.kt` | 26 | style collection: custom properties, states, layers, animation | feature:style-api |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/style/StylePropertyTest.kt` | 10 | style properties of each type, animated | feature:style-api |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/style/StyleResolverNodeTest.kt` | 3 | style resolver element equality and binding | feature:style-api |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/style/StyleStateTest.kt` | 13 | `StyleState` predefined and custom states, observation | feature:style-api |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/KeyboardOptionsTest.kt` | 7 | `KeyboardOptions.toImeOptions`, unspecified values, `merge`/`fillUnspecifiedValuesWith` | partial — `to_ime_options` exists (`compose-android/src/input_method.rs` tests); unspecified values and `merge` are missing (`keyboard-options-parity`) |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/LegacyTextFieldStateTest.kt` | 1 | legacy field state: layout result is snapshot state | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/LinkStateInteractionSourceObserverTest.kt` | 10 | (internal) link state from focus/hover/press interactions | feature:text-links — with `interactions` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/TextAutoSizeTest.kt` | 15 | `TextAutoSize.StepBased` validation, equality and font size search | feature:text-auto-size |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/TextDelegateTest.kt` | 7 | (internal, legacy) `TextDelegate` construction and intrinsics before layout | n/a: internal legacy delegate |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/TextFieldBringIntoViewTest.kt` | 4 | legacy field: focused rect notified to the input service | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/TextFieldDelegateTest.kt` | 8 | legacy field delegate: cursor offset, focus/blur, composition decoration | n/a: deprecated value-based `BasicTextField` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/TextFieldScrollerPositionTest.kt` | 10 | legacy field scroller position coercion | n/a: deprecated value-based `BasicTextField` (the state-based field scrolls with a `ScrollState`) |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/TextLayoutHelperTest.kt` | 16 | (internal, legacy) `TextLayoutResult.canReuse` rules | n/a: internal to the legacy `TextDelegate`; the node caches' rules are in `ParagraphLayoutCacheTest` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/UndoManagerTest.kt` | 5 | legacy field's undo manager | n/a: deprecated value-based `BasicTextField`; see `text/input/internal/undo/*` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/ValidatingOffsetMappingKtTest.kt` | 5 | legacy `VisualTransformation` offset-mapping validation | n/a: only the deprecated value-based field takes a `VisualTransformation` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/ValidatingOffsetMappingTest.kt` | 7 | legacy offset-mapping validation filter | n/a: only the deprecated value-based field takes a `VisualTransformation` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/contextmenu/builder/TextContextMenuBuilderScopeTest.kt` | 15 | `TextContextMenuBuilderScope`: items, separators collapsed, icons, clicks | feature:text-context-menu |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/contextmenu/test/ContextMenuFlagFlipperRunnerTest.kt` | 12 | Compose's JUnit runner flipping the new-context-menu flag | n/a: tests Compose's own test runner |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/AllCapsTransformationTest.kt` | 5 | `InputTransformation.allCaps`: keyboard option, new characters upper-cased, locale | feature:input-transformation |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/ExpandPolicyTest.kt` | 5 | `ExpandPolicy` of tracked style ranges | feature:text-field-styles |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/InputTransformationTest.kt` | 12 | chained `InputTransformation`s: equality, keyboard options merging, semantics | feature:input-transformation |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/MaxLengthTransformationTest.kt` | 17 | `maxLengthTrim`/`maxLengthReject`: semantics, truncation, rejection | feature:input-transformation |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/PasswordInputTransformationTest.kt` | 7 | (internal) password reveal rules by input source and obfuscation mode | feature:secure-text-field |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt` | 83 | `TextFieldBuffer`: selection adjusted by edits, replace/insert/delete/append, cursor placement, changes, `revertAllChanges` | feature:text-field-state |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt` | 13 | (internal) text with selection and composition: validation, coercion, equality | feature:text-field-state |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldStateSaverTest.kt` | 3 | `TextFieldState.Saver` saves text, selection, undo and styles | feature:text-field-state |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldStateTest.kt` | 63 | `TextFieldState`: initial selection, `edit` re-entrancy and exceptions, invalidation, snapshot behaviour, helpers | feature:text-field-state |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/BatchEditTest.kt` | 66 | IME batch edits: commits, composing, deletions, selections combined | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt` | 26 | `ChangeList` tracking of inserts, deletes and replaces in any order | feature:text-field-state — `TextFieldBuffer.changes` is public |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/CodepointTransformationTest.kt` | 3 | (internal) codepoint transformation offsets | feature:secure-text-field |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/CommitTextCommandTest.kt` | 16 | IME commitText semantics on cursor, selection and composition | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/CursorAnimationDurationTest.kt` | 2 | (internal) cursor blink never/forever durations | feature:cursor-blink |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/CursorAnimationStateTest.kt` | 9 | (internal) cursor blink animation start, suspend, cancel | feature:cursor-blink |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/DeleteSurroundingTextCommandTest.kt` | 28 | IME deleteSurroundingText around cursor, selection, composition | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/DeleteSurroundingTextInCodePointsCommandTest.kt` | 26 | IME deleteSurroundingTextInCodePoints with surrogates | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/FinishComposingTextCommandTest.kt` | 2 | IME finishComposingText | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/GapBufferTest.kt` | 81 | (internal) gap buffer data structure | n/a: internal data structure; compose-rs edits a `String`, and `TextFieldBufferTest` covers the behaviour |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/IntIntervalTreeMapTest.kt` | 11 | (internal) interval tree remapping for styled text | n/a: internal data structure; `TextStyleBufferTest` and `BasicTextFieldStyledTextTest` cover the behaviour |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/IntIntervalTreeTest.kt` | 36 | (internal) interval tree | n/a: internal data structure |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/IntervalTest.kt` | 7 | (internal) packed interval flags | n/a: internal data structure |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/MathUtilsTest.kt` | 3 | (internal) closest-rect search | n/a: internal helper |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/OffsetMappingCalculatorTest.kt` | 37 | offset mapping through edits (insert, delete, replace) both ways | feature:output-transformation |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/SetComposingRegionCommandTest.kt` | 15 | IME setComposingRegion incl. out-of-range and output transformations | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/SetComposingTextCommandTest.kt` | 15 | IME setComposingText semantics | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/SetSelectionCommandTest.kt` | 8 | IME setSelection semantics and coercion | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/TextFieldBufferDeleteFromImeRangeTest.kt` | 8 | deleting an IME range and adjusting a target range | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/TextFieldBufferUseFromImeTest.kt` | 34 | buffer used by the IME: insert, delete, selection, composition coercion | feature:android-ime |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/TextFieldStateInternalBufferTest.kt` | 24 | state's internal buffer: commands, new values, observers | feature:text-field-state |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/TextStyleBufferTest.kt` | 45 | (internal) style ranges under edits: overlaps, expansion, order, equality | feature:text-field-styles — port as behaviour of `TextFieldBuffer.addStyle` under edits |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ToCharArrayTest.kt` | 6 | (internal) `toCharArray` bounds | n/a: internal helper |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/TransformedTextFieldStateTest.kt` | 18 | output transformations: presented text, composing annotations, mapping insertions/deletions/replacements | feature:output-transformation |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/TransformedTextSelectionMovementTest.kt` | 6 | cursor movement around output replacements, insertions, deletions | feature:output-transformation |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/undo/TextUndoTest.kt` | 20 | undo merge rules for insertions, deletions, replaces | feature:undo-parity |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/undo/UndoManagerSaverTest.kt` | 1 | undo history saved and restored | feature:undo-parity |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/undo/UndoManagerTest.kt` | 10 | (internal) undo manager capacity and stacks | feature:undo-parity |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectableInfoTest.kt` | 16 | (internal) selectable info: crossed status, recompute rules | n/a: internal data of Compose's selection layout; compose-rs's selection is the tree's (D151); behaviour in `SelectionContainerTest` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectionAdjustmentTest.kt` | 76 | selection adjustments: none, character, word, paragraph, character-with-word-accelerate | partial — character and word exist (drags, double clicks); paragraph differs (whole text, see notes); the touch handles' accelerate mode is missing (`touch-text-selection`) |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectionLayoutStartSlot2DTest.kt` | 2 | (internal) start slot when a mouse selection starts between texts | n/a: internal; behaviour in `SelectionContainerPointerTest` (`mouseSelectionStartBetweenSelectables*`) |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectionLayoutTest.kt` | 87 | (internal) selection layout data structure | n/a: internal data structure of Compose's selection |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectionManagerGetSelectedRegionRectTest.kt` | 9 | the selected region's rect (to place the toolbar) | feature:touch-text-selection |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectionManagerTest.kt` | 50 | (internal) selection manager with mocked selectables: updates, haptics, merging, copy, select all | n/a: mock-driven internals; behaviours in `SelectionContainerTest`, `SelectionStateTest` |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectionModeTest.kt` | 16 | (internal) geometric `isSelected` by vertical/horizontal mode | n/a: internal; compose-rs orders texts by placement (D151, see notes) |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectionRegistrarImplTest.kt` | 8 | (internal) registrar subscribe/unsubscribe and sorting by position | n/a: internal; compose-rs finds selectables in the tree (D151); the ordering difference is in the notes |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/SelectionTest.kt` | 6 | (internal) `Selection` data class merge and ranges | n/a: internal data class |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/StringHelpersTest.kt` | 2 | (internal) paragraph start/end | n/a: internal helper; triple-click tests cover it |
| `compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/selection/TextFieldSelectionManagerTest.kt` | 45 | legacy field selection manager | n/a: deprecated value-based `BasicTextField` |

## 3. Public API surface compose-rs lacks

From `api/current.txt`, every public, non-deprecated API in this area that compose-rs lacks or only partly has, by package, with the feature it belongs to. Experimental APIs are marked *(experimental)*. Left out: deprecated APIs (the value-based `BasicTextField` overloads, `ClickableText`, `excludeFromSystemGesture`, `onFocusedBoundsChanged`, `BringIntoViewResponder`/`bringIntoViewResponder`, `IndicationInstance`, `Indication.rememberUpdatedInstance`, `KeyboardOptions(autoCorrect = …)`, `InputTransformation.maxLength`, the suspending `dragAndDropSource` overloads, `LocalAutofillHighlightColor`, the `@Deprecated` style scope interfaces) and the scrolling/gestures/lazy APIs of the root package (`ScrollState`, `scrollableArea`, overscroll, `ScrollIndicatorState`, `clipScrollableContainer`, `checkScrollableContainerConstraints`, `rememberPlatformOverscrollFactory`, `AndroidComposeFoundationFlags`).

### `androidx.compose.foundation`

| API | compose-rs | Feature |
|---|---|---|
| `AndroidExternalSurface`, `AndroidEmbeddedExternalSurface`, `AndroidExternalSurfaceScope`, `AndroidExternalSurfaceZOrder`, `SurfaceScope`, `SurfaceCoroutineScope` | missing | `android-external-surface` |
| `Modifier.background(color, shape)`, `background(brush, shape, alpha)` | present (`background`, `background_with`; `alpha` as `brush.with_alpha`) | — |
| `Modifier.basicMarquee(iterations, animationMode, repeatDelayMillis, initialDelayMillis, spacing, velocity)`, `MarqueeSpacing` (+ `fractionOfContainer`), `MarqueeAnimationMode`, `MarqueeDefaults` | missing | `basic-marquee` |
| `BasicTooltipBox`, `BasicTooltipState`, `rememberBasicTooltipState`, `BasicTooltipDefaults` *(experimental)* | missing (`TooltipArea` is Compose Desktop's) | `basic-tooltip` |
| `Modifier.border(BorderStroke, shape)`, `border(width, brush, shape)`, `BorderStroke(width, brush)`, `BorderRenderer` | partial: `border(width, color)`, `border_with(width, color, shape)`; `BorderStroke` holds a color only | `border-parity` |
| `Canvas(modifier, contentDescription, onDraw)` | missing (the overload without a description exists) | `canvas-content-description` |
| `Modifier.clickable(interactionSource, indication, enabled, onClickLabel, role, onClick)` and `clickable(enabled, onClickLabel, role, interactionSource, onClick)` | partial: no `onClickLabel`; `indication` is a color layer, not Compose's `Indication`; no default `LocalIndication` | `clickable-parity`, `indication` |
| `Modifier.combinedClickable(…, onLongClickLabel, onLongClick, onDoubleClick, hapticFeedbackEnabled, …)` | partial: no labels, no `hapticFeedbackEnabled` (always on) | `clickable-parity` |
| `ComposeFoundationFlags` *(experimental)* | one flag (`pausable_composition_in_prefetch`); this area's flags (`isNewContextMenuEnabled`, `isSmartSelectionEnabled`, `isSelectionAutoScrollEnabled`, `isMouseSelectionBetweenTextEnabled`, `isBasicTextFieldStyledTextEnabled`, `isLinkMinimumTouchTargetSizeZeroEnabled`, `isNewBorderImplementationEnabled`, `isBorderMatchesOutlineBoundsFixEnabled`, `isRoundedRectangleBorderRenderingFixEnabled`, `isInheritedTextStyleEnabled`, `isTextFieldWaitWindowFocusForInputSessionEnabled`, the `isBasicTextField*` optimizations) come with the features they switch | each feature |
| `Modifier.focusGroup()`, `focusable(enabled, interactionSource)` | partial: `focusable()`, `focusable_with(enabled)`; no interaction source, no focus group | `focusable-parity` |
| `Modifier.hoverable(interactionSource, enabled)` | present | — |
| `Image(bitmap, …, colorFilter, filterQuality)`, `Image(painter, …)`, `Image(imageVector, …)` | partial: `Image(bitmap, content_description, modifier, alignment, content_scale, alpha)` | `image-painter` |
| `Indication`, `IndicationNodeFactory`, `Modifier.indication(interactionSource, indication)`, `LocalIndication` | missing (`Indication` is a struct of a color and a radius) | `indication` |
| `Modifier.magnifier(sourceCenter, magnifierCenter, onSizeChanged, zoom, size, cornerRadius, elevation, clip)` | missing | `magnifier` |
| `MutatorMutex` (`mutate`, `mutateWith`, `tryMutate`), `MutatePriority` | missing | `mutator-mutex` |
| `Modifier.preferKeepClear()`, `preferKeepClear(rectProvider)` | missing | `android-window-rects` |
| `Modifier.progressSemantics()`, `progressSemantics(value, valueRange, steps)` | missing in foundation (Material has a private one) | `progress-semantics` |
| `Modifier.systemGestureExclusion()`, `systemGestureExclusion(exclusion)` | missing | `android-window-rects` |
| `isSystemInDarkTheme()` | present | — |

### `androidx.compose.foundation.content` *(experimental)*

`MediaType` (`All`, `HtmlText`, `Image`, `PlainText`, `Text`), `PlatformTransferableContent`, `Modifier.contentReceiver`, `ReceiveContentListener`, `TransferableContent` (+ `Source`), `TransferableContent.consume`, `hasMediaType`: all missing (`content-receiver`). `MediaType` itself isn't marked experimental.

### `androidx.compose.foundation.draganddrop`

`Modifier.dragAndDropSource(transferData)`, `dragAndDropSource(drawDragDecoration, transferData)`, `Modifier.dragAndDropTarget(shouldStartDragAndDrop, target)`: missing (`drag-and-drop`).

### `androidx.compose.foundation.interaction`

| API | compose-rs | Feature |
|---|---|---|
| `Interaction`, `InteractionSource.interactions`, `MutableInteractionSource.emit`/`tryEmit`, `MutableInteractionSource()` | partial: `MutableInteractionSource` holds three boolean states that the clickable family writes; no interactions, no stream, no emitting | `interactions` |
| `PressInteraction.Press/Release/Cancel`, `FocusInteraction.Focus/Unfocus`, `HoverInteraction.Enter/Exit`, `DragInteraction.Start/Stop/Cancel` | missing | `interactions` |
| `collectIsPressedAsState`, `collectIsFocusedAsState`, `collectIsHoveredAsState` | present as `is_pressed()`, `is_focused()`, `is_hovered()` (state reads) | — |
| `collectIsDraggedAsState` | missing | `interactions` |

### `androidx.compose.foundation.relocation`

`BringIntoViewRequester()`, `BringIntoViewRequester.bringIntoView(rect)`, `Modifier.bringIntoViewRequester`: missing; only the responder side exists, internally (`ModifierNode::bring_into_view`, used for focus) (`bring-into-view-requester`).

### `androidx.compose.foundation.selection`

`selectableGroup` and `selectable(selected, enabled, role, interactionSource, onClick)` present (through `ClickableOptions`); `toggleable` present; `triStateToggleable` (both overloads) missing (`tri-state-toggleable`); the interaction/indication overloads as `clickable`'s.

### `androidx.compose.foundation.shape`

| API | compose-rs | Feature |
|---|---|---|
| `RoundedCornerShape` (dp, px, percent, per-corner overloads), `CircleShape` | partial: `RoundedCornerShape(impl RoundedCorners)` takes a `Dp`, an `i32` percent, a `CornerSize` or four of them; no px overloads, no `copy` | `shapes` |
| `CornerSize(dp/px/percent)`, `ZeroCornerSize`, `CornerSize.toPx` | partial: `CornerSize::Dp`, `CornerSize::Percent`, `to_px` | `shapes` |
| `CornerBasedShape` (`copy`, `createOutline`, `lerp`), `CutCornerShape`, `AbsoluteRoundedCornerShape`, `AbsoluteCutCornerShape`, `GenericShape`, a user-implemented `Shape` | missing: `Shape` is a closed enum (`Rectangle`, `Rounded`) | `shapes` |
| `PolygonShape` (+ `circle`, `pill`, `pillStar`, `rectangle`, `absoluteRectangle`, `regularPolygon`, `star`, `copy`, `transform`), `PolygonShapeGeometry`, `PolygonShapeScope`, `PolygonShapeTransformScope`, `CornerRounding`, `MorphPolygonShape`, `toPolygonShape` | missing | `polygon-shapes` |

### `androidx.compose.foundation.style` *(experimental, `ExperimentalFoundationStyleApi`)*

`Modifier.appearance`/`placement`/`graphicsLayer(layerRequired, block)` and their `StyleResolver` forms (`styleAppearance`, `stylePlacement`, `styleGraphicsLayer`, `styleBorderPadding`, `styleContentPadding`, `styleExternalPadding`, `styleResolver`), `CommonStyle` (+ combinators, `then`), `CustomStyle`, `CustomStyleScope`, `CommonStyleScope`, `AnimateStyleScope`, `StyleState`/`MutableStyleState`, `StyleStateKey`, `StyleProperty`/`ProvidableStyleProperty`, `StyleResolver`, `NestedStyleKey`, `Breadth`, `Fill`, `Shadows`, `AppearanceScope`, `PlacementModifierScope`: all missing (`style-api`). `Modifier.styleable` and the `Style`/`*Scope` interfaces of the first design are deprecated.

### `androidx.compose.foundation.text`

| API | compose-rs | Feature |
|---|---|---|
| `BasicText(text: String, modifier, style, onTextLayout, overflow, softWrap, maxLines, minLines, color, autoSize)` | partial: no `color: ColorProducer`, no `autoSize` | `basic-text-drawing`, `text-auto-size` |
| `BasicText(text: AnnotatedString, …, inlineContent, color, autoSize)` | partial (`BasicAnnotatedText`): no `inlineContent`, `color`, `autoSize` | `inline-content`, `basic-text-drawing`, `text-auto-size` |
| `InlineTextContent`, `AnnotatedString.Builder.appendInlineContent` | missing | `inline-content` |
| `TextAutoSize` (`StepBased`), `TextAutoSizeDefaults`, `text.modifiers.TextAutoSizeLayoutScope` | missing | `text-auto-size` |
| `LocalBackgroundTextMeasurementExecutor` | missing | `background-text-measurement` |
| `BasicTextField(state: TextFieldState, modifier, enabled, readOnly, inputTransformation, textStyle, keyboardOptions, onKeyboardAction, lineLimits, onTextLayout, interactionSource, cursorBrush, outputTransformation, decorator, scrollState)` | missing (only the deprecated value-based shape exists) | `text-field-state` |
| `BasicSecureTextField(…)`, `LocalTextFieldContentObserverRegistrationExecutor` | missing (`PasswordVisualTransformation` on the value-based field) | `secure-text-field` |
| `KeyboardOptions(capitalization, autoCorrectEnabled: Boolean?, keyboardType, imeAction, platformImeOptions, showKeyboardOnFocus, hintLocales)`, `copy`, `merge`, `KeyboardOptions.Default` | partial: the first four fields, `auto_correct_enabled` not nullable; no `Unspecified` values in `KeyboardType`, `ImeAction`, `KeyboardCapitalization` | `keyboard-options-parity` |
| `KeyboardActions(onDone, …)` taking `KeyboardActionScope.() -> Unit`, `KeyboardActionScope.defaultKeyboardAction`, `KeyboardActions(onAny)`, `KeyboardActions.Default` | partial: `KeyboardActions::new().on_done(…)` with plain closures | `keyboard-options-parity` |
| `LocalAutofillHighlightBrush` | missing | `autofill-highlight` |

### `androidx.compose.foundation.text.contextmenu.*`

`builder.TextContextMenuBuilderScope` (`item`, `separator`), `data.TextContextMenuData`/`TextContextMenuItem`/`TextContextMenuSeparator`/`TextContextMenuComponent`/`TextContextMenuKeys`/`TextContextMenuSession`/`ProcessTextKey`, `modifier.appendTextContextMenuComponents`/`filterTextContextMenuComponents`, `provider.TextContextMenuProvider`/`TextContextMenuDataProvider`/`LocalTextContextMenuDropdownProvider`/`LocalTextContextMenuToolbarProvider`: all missing (`text-context-menu`).

### `androidx.compose.foundation.text.handwriting`

`Modifier.handwritingDetector(callback)`, `Modifier.handwritingHandler()`: missing (`handwriting`).

### `androidx.compose.foundation.text.input`

| API | Feature |
|---|---|
| `TextFieldState` (`text`, `selection`, `composition`, `edit`, `undoState`, `textStyles`, `Saver`), `rememberTextFieldState`, `clearText`, `setTextAndPlaceCursorAtEnd`, `setTextAndSelectAll`, `toTextFieldBuffer` | `text-field-state` |
| `TextFieldBuffer` (`replace`, `append`, `insert`, `delete`, `charAt`, `length`, `asCharSequence`, `selection`, `hasSelection`, `originalText`, `originalSelection`, `placeCursorBeforeCharAt`/`AfterCharAt`, `placeCursorAtEnd`, `selectAll`, `revertAllChanges`, `changes`, `ChangeList`, `forEachChange`, `forEachChangeReversed`) | `text-field-state` |
| `TextFieldLineLimits` (`SingleLine`, `MultiLine(min, max)`, `Default`), `TextFieldDecorator`, `KeyboardActionHandler` | `text-field-state` |
| `UndoState` (`undo`, `redo`, `clearHistory`, `canUndo`, `canRedo`) | `undo-parity` |
| `InputTransformation` (`transformInput`, `keyboardOptions`, `applySemantics`), `then`, `allCaps`, `byValue`, `maxLengthTrim`, `maxLengthReject` | `input-transformation` |
| `OutputTransformation` | `output-transformation` |
| `TextObfuscationMode` (`RevealLastTyped`, `Hidden`, `Visible`, `System`) | `secure-text-field` |
| `TextFieldBuffer.addStyle`/`removeStyle`/`getSpanStyles`/`getParagraphStyles`, `TrackedRange`, `ExpandPolicy`, `TextFieldTextStyles` | `text-field-styles` |

### `androidx.compose.foundation.text.selection`

| API | compose-rs | Feature |
|---|---|---|
| `SelectionContainer(modifier, content)`, `DisableSelection` | present | — |
| `SelectionContainer(state, modifier, content)`, `SelectionState` (`select`, `selectAll`, `clear`, `extendSelectionByWord`, `selectedTexts`, `getSelectableTexts`, `Saver`), `rememberSelectionState` | missing | `selection-state` |
| `TextSelectionColors`, `LocalTextSelectionColors` | missing (one constant color, `#664286F4`) | `text-selection-colors` |
| `LocalTextClassifierCoroutineContext` | missing | `smart-selection` |

APIs with no test in this module that still need porting: `GenericShape`, `MarqueeSpacing.fractionOfContainer`, `BasicTooltipDefaults`, `MediaType`, `TextFieldBuffer.forEachChangeReversed`, `InputTransformation.byValue`, `LocalAutofillHighlightBrush`, `LocalTextFieldContentObserverRegistrationExecutor`, `LocalTextClassifierCoroutineContext`: each is in the feature named above.

## 4. Missing features

Sizes: S under a day, M a few days, L a week or two, XL multi-week. "Other area" dependencies belong to other ledgers (ui, ui-text, runtime, gestures) and are named so those ledgers can match them.

### interactions
- **Compose API:** `Interaction`, `InteractionSource.interactions`, `MutableInteractionSource.emit`/`tryEmit`, `PressInteraction`, `FocusInteraction`, `HoverInteraction`, `DragInteraction`, `collectIsDraggedAsState`.
- **What:** a stream of typed interactions (press with its position, release/cancel naming their press, focus/unfocus, hover enter/exit, drag start/stop/cancel) that components emit and indications or apps collect; the `collectIs*AsState` reads become collectors of it. compose-rs's three booleans become derived state.
- **Tests:** `InteractionSourceTest`, `text/input/internal/selection/TextFieldInteractionSourcePressTest`, and most of `ClickableTest`, `CombinedClickableTest`, `*ParameterizedKeyInputTest`, `SelectableTest`, `ToggleableTest`, `FocusableTest`, `HoverableTest`, `text/input/TextFieldFocusTest`, `LinkStateInteractionSourceObserverTest`.
- **Depends on:** a multi-subscriber async stream in compose-runtime (other area: runtime's `Flow`/`SharedFlow` equivalent); the draggable in compose-foundation emitting `DragInteraction` (gestures area).
- **Size:** M. **Experimental:** no.

### indication
- **Compose API:** `Indication`, `IndicationNodeFactory`, `Modifier.indication`, `LocalIndication`; the clickable family's `indication: Indication?` with lazily created interaction sources.
- **What:** an indication is a factory of a draw modifier node that collects an interaction source; clickables use `LocalIndication.current` by default (foundation's default draws a press/hover/focus overlay, Material provides its ripple); `null` draws nothing. Equality rules decide when the node is recreated.
- **Tests:** `IndicationTest`, the `localIndication_*`, `indication_*`, `indicationNodeFactory_*` and `*_equality` tests of `ClickableTest`, `CombinedClickableTest`, `SelectableTest`, `ToggleableTest`.
- **Depends on:** `interactions`; ui's node delegation (a modifier node hosting another node: other area, ui `DelegatingNode`); Material's ripple moves onto it (material area).
- **Size:** M. **Experimental:** no.

### clickable-parity
- **Compose API:** `onClickLabel`, `onLongClickLabel`, `hapticFeedbackEnabled`; behaviour: press delayed in scrollable containers (`isComposeRootInScrollableContainer`, Compose's tap indication delay), `Key.DirectionCenter` clicks, holding a key long-clicks and repeated keys double-click in `combinedClickable`, touch-mode focusability, indirect pointer clicks, click sounds.
- **Tests:** `ClickableTest`, `CombinedClickableTest`, `ClickableInScrollableViewGroupTest`, `ClickableIndirectPointerInputTest`, `CombinedClickableIndirectPointerInputTest`, `ClickableSoundTest`, `*ParameterizedKeyInputTest`.
- **Depends on:** `interactions`; other areas: ui semantics action labels, ui indirect pointer events, ui `InputModeManager` (touch vs keyboard mode), a `DirectionCenter` key, a host sound-effect hook, the host telling whether an Android parent View scrolls.
- **Size:** M. **Experimental:** no.

### tri-state-toggleable
- **Compose API:** `Modifier.triStateToggleable(state, enabled, role, interactionSource, onClick)` and the indication overload.
- **What:** a toggleable whose semantics state is `ToggleableState` (`On`, `Off`, `Indeterminate`), clicking calls `onClick`. `ToggleableState` exists.
- **Tests:** `selection/ToggleableTest`.
- **Depends on:** nothing new (interactions/indication for its interaction tests).
- **Size:** S. **Experimental:** no.

### focusable-parity
- **Compose API:** `Modifier.focusable(enabled, interactionSource)`, `Modifier.focusGroup()`.
- **What:** focusable emits `FocusInteraction`s and resets them on disable/detach; a focus group makes its children searched together (1D and 2D) and isn't focusable itself unless `focusProperties` says so; clicks outside clear the focus by default.
- **Tests:** `FocusableTest`, `FocusGroupTest`.
- **Depends on:** `interactions`; other area: ui focus properties, focus groups and touch mode.
- **Size:** M. **Experimental:** no.

### mutator-mutex
- **Compose API:** `MutatorMutex` (`mutate`, `mutateWith`, `tryMutate`), `MutatePriority`.
- **What:** mutual exclusion of suspending mutators by priority, a newer one cancelling an older one of lower or equal priority.
- **Tests:** `MutatorMutexTest` (shared with the gestures ledger, whose scroll APIs take a `MutatePriority`).
- **Depends on:** compose-runtime task cancellation (exists).
- **Size:** S. **Experimental:** no.

### shapes
- **Compose API:** `Shape` as an open interface (`createOutline(size, layoutDirection, density)`), `CornerBasedShape` (`copy`, `lerp`), `CutCornerShape`, `AbsoluteRoundedCornerShape`, `AbsoluteCutCornerShape`, `GenericShape`, `CornerSize` in px, `ZeroCornerSize`, the px/percent overloads of every corner shape.
- **What:** shapes compose-rs can't express today: cut corners, left/right (not start/end) corners, any path; corner sizes resolved as Compose's (start pair and end pair scaled down together only when they don't fit; negative sizes rejected). Everything that takes a `Shape` (background, border, clip, graphics layer, shadow, Material's surfaces) must accept a generic outline.
- **Tests:** `shape/*CornerShapeTest`, `CornerBasedShapeTest`, `CornerSizeTest`, `ShapeInterpolationTests`; the generic-outline tests of `AdvancedBorderTest`, `AdvancedBorderRendererTest`, `BackgroundTest`, `BorderSemanticsTest`.
- **Depends on:** other area: ui graphics `Outline::Generic(Path)` (clips to paths exist), `Interpolatable`.
- **Size:** L (the `Shape` enum is matched across compose-ui, the renderers and Material). **Experimental:** no.

### polygon-shapes
- **Compose API:** `PolygonShape` and its builders, `PolygonShapeGeometry`, `PolygonShapeScope`, `PolygonShapeTransformScope`, `CornerRounding`, `MorphPolygonShape`, `toPolygonShape`.
- **What:** rounded polygons, stars and pills as shapes, transformable, morphing between each other.
- **Tests:** `PolygonShapeTest`, `MorphPolygonShapeTest`, `PolygonShapeIntegrationTest`, `TransformedPolygonShapeTest`.
- **Depends on:** `shapes`; a port of `androidx.graphics.shapes` (`RoundedPolygon`, `Morph`, its cubics and feature mapping).
- **Size:** XL. **Experimental:** no.

### border-parity
- **Compose API:** `Modifier.border(BorderStroke, shape)`, `border(width, brush, shape)`, `BorderStroke(width, brush)`, `BorderRenderer`.
- **What:** brush borders, borders over generic outlines, the public renderer, Compose's width rules (negative or unspecified widths draw nothing; hairline 1 px; fractional widths rounded up; filled when the stroke exceeds half the smaller side), and the border's shape exposed in semantics.
- **Tests:** `BorderTest`, `BorderRendererTest`, `AdvancedBorderTest`, `AdvancedBorderRendererTest`, `BorderSemanticsTest`.
- **Depends on:** `shapes`; other area: a `shape` semantics property (ui).
- **Size:** M. **Experimental:** no.

### image-painter
- **Compose API:** `Image(painter, …)`, `Image(imageVector, …)`, `colorFilter`, `filterQuality` on all overloads.
- **What:** images from any painter (bitmap subsections, vectors, resources), tinted and filtered.
- **Tests:** `ImageTest`.
- **Depends on:** other area: ui graphics `Painter` (`BitmapPainter`, `VectorPainter`, `painterResource`), `ColorFilter`, `FilterQuality`.
- **Size:** M. **Experimental:** no.

### canvas-content-description
- **Compose API:** `Canvas(modifier, contentDescription, onDraw)`.
- **Tests:** `CanvasTest`.
- **Size:** S. **Experimental:** no.

### progress-semantics
- **Compose API:** `Modifier.progressSemantics()`, `progressSemantics(value, valueRange, steps)`.
- **What:** progress-bar semantics, indeterminate or within a range; Material's progress indicators use it.
- **Tests:** `ProgressSemanticsTest`.
- **Depends on:** `ProgressBarRangeInfo` (exists: `current`, `range`, `steps`) gaining Compose's `Indeterminate` value.
- **Size:** S. **Experimental:** no.

### android-window-rects
- **Compose API:** `Modifier.systemGestureExclusion()`/`(exclusion)`, `Modifier.preferKeepClear()`/`(rectProvider)`.
- **What:** rects in window coordinates handed to the host View (`setSystemGestureExclusionRects`, `setPreferKeepClearRects`), updated as layouts move, removed with the modifier; no-ops on desktop.
- **Tests:** `SystemGestureExclusionTest`, `PreferKeepClearTest`.
- **Depends on:** a compose-android host call per frame (global positions exist).
- **Size:** S. **Experimental:** no.

### basic-marquee
- **Compose API:** `Modifier.basicMarquee`, `MarqueeSpacing`, `MarqueeAnimationMode`, `MarqueeDefaults`.
- **What:** content wider than its bounds scrolls horizontally in a loop (iterations, delays, spacing, velocity, RTL), immediately or while focused.
- **Tests:** `BasicMarqueeTest`.
- **Depends on:** animation (exists), focus (exists), graphics layers (exist).
- **Size:** M. **Experimental:** no.

### magnifier
- **Compose API:** `Modifier.magnifier(…)`.
- **What:** Android's platform magnifier (API 28+) over a source point; unsupported platforms (desktop) make it a no-op, as Compose's.
- **Tests:** `MagnifierTest`, `PlatformMagnifierTest`, the magnifier files under `text/selection` and `text/input/internal/selection`.
- **Depends on:** a compose-android bridge to `android.widget.Magnifier`; `touch-text-selection` for the text uses.
- **Size:** M. **Experimental:** no.

### bring-into-view-requester
- **Compose API:** `BringIntoViewRequester`, `bringIntoView(rect)`, `Modifier.bringIntoViewRequester`.
- **What:** application code asks the scrollable ancestors to show a node or a rect of it, each scrolling the least needed, newer requests interrupting older ones they don't contain; on Android it continues to `View.requestRectangleOnScreen` past the root.
- **Tests:** `relocation/*`.
- **Depends on:** ui's responder side (exists as `ModifierNode::bring_into_view`); the gestures area's `BringIntoViewSpec`; the Android host bridge.
- **Size:** M. **Experimental:** no.

### basic-tooltip
- **Compose API:** `BasicTooltipBox`, `BasicTooltipState`, `rememberBasicTooltipState`, `BasicTooltipDefaults`.
- **What:** a tooltip popup shown on long press or hover, one at a time (global mutator mutex), dismissed after a duration unless persistent; Material's tooltips build on it.
- **Tests:** `BasicTooltipTest`.
- **Depends on:** `mutator-mutex`; popups, long press and hover (exist).
- **Size:** M. **Experimental:** yes.

### android-external-surface
- **Compose API:** `AndroidExternalSurface`, `AndroidEmbeddedExternalSurface`, their scopes and `AndroidExternalSurfaceZOrder`.
- **What:** a `SurfaceView` or `TextureView` placed where the composable is, with surface callbacks run in a coroutine scope.
- **Tests:** `AndroidExternalSurfaceTest`, `AndroidEmbeddedExternalSurfaceTest`.
- **Depends on:** compose-android hosting child Views positioned by layout (no View interop exists yet: other area, ui `AndroidView`).
- **Size:** L. **Experimental:** no.

### content-receiver
- **Compose API:** `Modifier.contentReceiver`, `ReceiveContentListener`, `TransferableContent`, `MediaType`, `PlatformTransferableContent`, `consume`, `hasMediaType`.
- **What:** rich content (images, URIs) from the keyboard, the clipboard or a drop delivered to the nearest receivers, bottom up.
- **Tests:** `content/*`, `text/input/TextFieldReceiveContentTest`.
- **Depends on:** `text-field-state`, `android-ime` (`commitContent`), `drag-and-drop`; other area: ui `Clipboard`/`ClipEntry`.
- **Size:** L. **Experimental:** yes.

### drag-and-drop
- **Compose API:** `Modifier.dragAndDropSource`, `Modifier.dragAndDropTarget`.
- **Tests:** `draganddrop/*`, `text/input/TextFieldDragAndDropTest`.
- **Depends on:** other area: ui's drag-and-drop infrastructure and the platforms' drag sessions.
- **Size:** L. **Experimental:** no.

### style-api
- **Compose API:** the `androidx.compose.foundation.style` package.
- **What:** declarative, state-aware, animatable styles applied through a `StyleResolver` (`Modifier.appearance`, `placement`, `styleAppearance`, …), equivalent to background, border, padding, size, shape, alpha, text style modifiers.
- **Tests:** `style/*` (device and host).
- **Depends on:** `shapes`, `border-parity`, `interactions` (style states such as pressed and hovered).
- **Size:** XL. **Experimental:** yes (`ExperimentalFoundationStyleApi`).

### text-field-state
- **Compose API:** `TextFieldState`, `TextFieldBuffer` (+ `ChangeList`), `rememberTextFieldState`, `TextFieldState.Saver`, `BasicTextField(state, …)`, `TextFieldLineLimits`, `TextFieldDecorator`, `KeyboardActionHandler`, the field's `onTextLayout`, `interactionSource`, `cursorBrush`, `scrollState`.
- **What:** the non-deprecated text field: state hoisted in a `TextFieldState` edited through `edit { }` (selection and composition in the state; the cursor at the end of a new state's text), line limits instead of `singleLine`/`maxLines`/`minLines` (a single line shows new lines as spaces), a decorator instead of `decorationBox`, a keyboard action handler with a default action, scrolling through a `ScrollState`, the full set of semantics actions (set text, insert at cursor, set selection, copy, cut, paste, IME action), a default width of 10 "H" characters, and the field's layout cache rules. compose-rs's `TextFieldNode` (keys, mouse selection, IME composition, undo, scrolling, cursor) is the place to start; the value-based field can stay as a wrapper.
- **Tests:** 30 files under `text/input/**` (see the table), most importantly `BasicTextFieldTest`, `TextFieldKeyEventTest`, `CommonTextFieldKeyEventTest`, `BasicTextFieldSemanticsTest`, `TextFieldScrollTest`, `TextFieldFocusTest`, `TextFieldStateTest`, `TextFieldBufferTest`, `ChangeTrackerTest`.
- **Depends on:** other areas: ui semantics text actions and properties (`SetText`, `InsertTextAtCursor`, `SetSelection`, `CopyText`, `CutText`, `PasteText`, `PerformImeAction`, `GetTextLayoutResult`, `TextSelectionRange`, `ContentType`), ui `TextRange`, keyboard modifiers on pointer events (Shift+click), Page Up/Down and Insert handling; this ledger's `keyboard-options-parity`, `cursor-blink`, `interactions`.
- **Size:** XL. **Experimental:** no.

### keyboard-options-parity
- **Compose API:** `KeyboardOptions` (`autoCorrectEnabled: Boolean?`, `platformImeOptions`, `showKeyboardOnFocus`, `hintLocales`, `copy`, `merge`, `Default`), `KeyboardType.Unspecified`, `ImeAction.Unspecified`, `KeyboardCapitalization.Unspecified`, `KeyboardActionScope.defaultKeyboardAction`, `KeyboardActions(onAny)`.
- **Tests:** `KeyboardOptionsTest`, `DefaultKeyboardActionsTest`, `EditorInfoTest`, `InputTransformationTest` (keyboard options merging).
- **Depends on:** other area: ui `text.input` types (`LocaleList`, `PlatformImeOptions`, the `Unspecified` values).
- **Size:** S. **Experimental:** no.

### cursor-blink
- **What:** the cursor blinks (500 ms on, 500 ms off, as `CursorAnimationState`), stays on while typing or moving, stops while the window is unfocused, and is solid when animations are disabled.
- **Tests:** `CursorAnimationStateTest`, `CursorAnimationDurationTest`, `text/input/TextFieldCursorTest`.
- **Size:** S. **Experimental:** no (no public API).

### undo-parity
- **Compose API:** `UndoState` (`undo`, `redo`, `clearHistory`, `canUndo`, `canRedo`), undo saved with the state.
- **What:** Compose's merge rules: insertions merge only when continuing from the previous one's end, deletions merge only in the same direction and sharing a boundary, new lines, paste and cut never merge, a group lasts at most 5 s from its first edit; programmatic edits enter the history.
- **Tests:** `BasicTextFieldUndoTest`, `TextUndoTest`, `UndoManagerTest`, `UndoManagerSaverTest`.
- **Depends on:** `text-field-state`.
- **Size:** M. **Experimental:** no.

### input-transformation
- **Compose API:** `InputTransformation` (+ `keyboardOptions`, `applySemantics`), `then`, `allCaps`, `byValue`, `maxLengthTrim`, `maxLengthReject`.
- **What:** every edit, from any source (keys, IME, semantics, paste), filtered or changed before it reaches the state; chained transformations merge their keyboard options.
- **Tests:** `InputTransformationTest`, `AllCapsTransformationTest`, `MaxLengthTransformationTest`, the filter tests of `BasicTextFieldTest`/`BasicTextFieldSemanticsTest`.
- **Depends on:** `text-field-state`.
- **Size:** M. **Experimental:** no.

### output-transformation
- **Compose API:** `OutputTransformation`.
- **What:** the text shown differs from the state (insertions, replacements); offsets map both ways through the changes, the cursor can't land inside a replacement, clicks and arrows move around them.
- **Tests:** `TextFieldOutputTransformation*IntegrationTest`, `TransformedTextFieldStateTest`, `TransformedTextSelectionMovementTest`, `OffsetMappingCalculatorTest`, `BasicTextFieldAnnotatedOutputTransformationTest`.
- **Depends on:** `text-field-state`.
- **Size:** L. **Experimental:** no.

### secure-text-field
- **Compose API:** `BasicSecureTextField`, `TextObfuscationMode`, `LocalTextFieldContentObserverRegistrationExecutor`.
- **What:** a password field masking per glyph, revealing the last typed character briefly (following the system's "show passwords" setting for touch and hardware input), without copy or cut, with password semantics and keyboard.
- **Tests:** `BasicSecureTextFieldTest`, `PasswordInputTransformationTest`, `TextFieldCodepointTransformationTest`, `CodepointTransformationTest`, `OutputTransformationPerGlyphTest`.
- **Depends on:** `text-field-state`, `input-transformation`.
- **Size:** M. **Experimental:** no.

### text-field-styles
- **Compose API:** `TextFieldBuffer.addStyle`/`removeStyle`/`getSpanStyles`/`getParagraphStyles`, `TrackedRange`, `ExpandPolicy`, `TextFieldState.textStyles`, `TextFieldTextStyles`.
- **What:** span and paragraph styles kept in the state, their ranges following edits by their expand policy, saved with the state.
- **Tests:** `BasicTextFieldStyledTextTest`, `TextStyleBufferTest`, `ExpandPolicyTest`.
- **Depends on:** `text-field-state`; other area: ui-text paragraph styles in `AnnotatedString`.
- **Size:** L. **Experimental:** no (behind `ComposeFoundationFlags.isBasicTextFieldStyledTextEnabled`).

### touch-text-selection
- **What:** touch selection in `SelectionContainer` and fields: long press selects a word (and places the cursor in an empty region), selection and cursor handles (drawn in popups, at least 48 dp to touch, dragged with word acceleration), the text toolbar shown after a selection or a handle tap and hidden on edits, scrolls and keys, back clearing the selection, auto-scroll while dragging past a scrollable's edge, haptics on selection change.
- **Tests:** `TextFieldCursorHandleTest`, `TextFieldSelectionHandlesTest`, `TextFieldLongPressTest`, `TextFieldTextToolbarTest`, `TextFieldScrolledSelectionGestureTest`, `SelectionHandlesTest`, `SelectionHandlePopupPositionTest`, `MultiWidgetSelectionDelegateTest`, `SelectionManagerGetSelectedRegionRectTest`, `TextFieldToolbarTest`, and the touch parts of `SelectionContainerTest`, `SelectionContainerFocusTest`, `TextFieldDoubleTapTest`.
- **Depends on:** other areas: ui `TextToolbar`, ui-text `TextLayoutResult` queries (bounding boxes, line queries); `magnifier`; `text-field-state`; `selection-state`.
- **Size:** XL. **Experimental:** no (no new API beyond the toolbar's ui types).

### selection-state
- **Compose API:** `SelectionState`, `rememberSelectionState`, `SelectionContainer(state, …)`.
- **What:** application access to a container's selection: select ranges, select all, clear, extend by word, read the selected texts, save across recreation.
- **Tests:** `SelectionStateTest`.
- **Depends on:** compose-rs's selection (exists, D151).
- **Size:** M. **Experimental:** no.

### text-selection-colors
- **Compose API:** `TextSelectionColors`, `LocalTextSelectionColors`.
- **Tests:** `TextSelectionColorsScreenshotTest`.
- **Size:** S. **Experimental:** no.

### text-context-menu
- **Compose API:** `text.contextmenu.*` (builder, data, modifier, provider packages); internally the dropdown context menu (`ContextMenuArea`, its state, popup positioning and items) and the Android floating-toolbar provider.
- **What:** right click (or the toolbar on touch) on a field or a selection shows cut/copy/paste/select all/autofill items, extensible and filterable by modifiers, through replaceable providers.
- **Tests:** `contextmenu/*`, `text/contextmenu/**`, `SelectionContainerContextMenu*Test`, `TextFieldTextContextMenuToolbarTest`, `TextFieldConfigChangeAppCompatActivityLocaleTest`, `ClipboardPasteStateTest`, the btf2 halves of `textfield/TextFieldContextMenuTest` and `TextFieldTextContextMenuBuilderTest`.
- **Depends on:** other areas: pointer buttons on pointer events (right click), ui clipboard metadata; `touch-text-selection` for the toolbar; popups (exist).
- **Size:** L. **Experimental:** no.

### smart-selection
- **Compose API:** `LocalTextClassifierCoroutineContext`.
- **What:** on Android, a long press or double tap asks the `TextClassifier` for a suggested range (an address, a phone number) and may add an action item.
- **Tests:** `PlatformSelectionBehaviorCommonTestCases`, `PlatformSelectionBehaviorsImplTest`, `SelectionContainerPlatformSelectionsBehaviorTest`, `TextFieldPlatformSelectionBehaviorsTest`.
- **Depends on:** `touch-text-selection`; a compose-android bridge to `TextClassifier`.
- **Size:** M. **Experimental:** no (behind `isSmartSelectionEnabled`).

### hover-icon-override
- **What:** selectable text (in a `SelectionContainer`) shows the I-beam, and an ancestor's `pointerHoverIcon(icon, overrideDescendants = true)` wins over its descendants'.
- **Tests:** `BasicTextHoverTest`, `text/input/internal/BasicTextFieldHoverTest`.
- **Depends on:** other area: ui `pointer_hover_icon`'s `override_descendants`.
- **Size:** S. **Experimental:** no.

### android-ime
- **What:** Compose's `InputConnection` contract on Android: text before/after the cursor, selected and extracted text, batch edits, `commitText`/`setComposingText`/`setComposingRegion`/`setSelection`/`deleteSurroundingText(InCodePoints)`/`finishComposingText` semantics, `CursorAnchorInfo`, `EditorInfo` initial selection and hints, when to restart input or update the selection. compose-rs's `ComposeRsInputConnection` is a `BaseInputConnection` without a model of the text; the command semantics should move into Rust so they're host-tested.
- **Tests:** `StatelessInputConnectionTest`, `AndroidTextInputSessionTest`, `CursorAnchorInfoBuilderTest`, `ComposeInputMethodManagerTest`, `BasicTextFieldImmIntegrationTest`, `BasicTextFieldImeSelectionChangesTest`, `BasicTextFieldSendKeyEventTest`, the `*CommandTest`s, `BatchEditTest`, `TextFieldBufferUseFromImeTest`, `TextFieldBufferDeleteFromImeRangeTest`.
- **Depends on:** `text-field-state` (composition in the state, unlike D117), `keyboard-options-parity`.
- **Size:** L. **Experimental:** no (no public API).

### handwriting
- **Compose API:** `Modifier.handwritingDetector`, `Modifier.handwritingHandler`.
- **What:** stylus handwriting into fields (Android 14+): starting on stylus moves, delegation from detectors to handlers, handwriting gestures (select, delete, insert, join, remove space) with previews, the handwriting hover icon.
- **Tests:** `BasicTextFieldHandwriting*Test`, `HandwritingDetectorTest`, `HandwritingHandlerTest`, `HandwritingHoverIconTest`.
- **Depends on:** `android-ime`, `text-field-state`; stylus pointer events (exist as `PointerType::Stylus`).
- **Size:** L. **Experimental:** no.

### dead-keys
- **What:** a hardware dead key (an accent) combines with the next key into one character in a field, as Compose's `DeadKeyCombiner` on Android.
- **Tests:** `DeadKeyCombinerTest`.
- **Depends on:** Android key events carrying their unicode and dead-key flags.
- **Size:** S. **Experimental:** no.

### text-links
- **What:** `LinkAnnotation.Url` and `LinkAnnotation.Clickable` in `BasicText`: each link focusable and clickable (opening through `LOCAL_URI_HANDLER`), styled by state (`TextLinkStyles` for focused/hovered/pressed, from interactions), with a minimum touch target and link semantics.
- **Tests:** `BasicTextLinkTest`, `BasicTextLinkInteropTest`, `LinkStateInteractionSourceObserverTest`.
- **Depends on:** other area: ui-text `LinkAnnotation`/`TextLinkStyles` and `TextLayoutResult.getPathForRange`; `interactions`.
- **Size:** L. **Experimental:** no.

### inline-content
- **Compose API:** `InlineTextContent`, `appendInlineContent`, `BasicText(…, inlineContent)`.
- **What:** composables laid out in placeholders inside the text, in LTR, RTL and bidi text.
- **Tests:** `CoreTextInlineContentTest`, one case of `text/modifiers/BasicTextSemanticsTest`.
- **Depends on:** other area: ui-text `Placeholder` support in both measurers (SkParagraph and `StaticLayout` replacement spans).
- **Size:** L. **Experimental:** no.

### text-auto-size
- **Compose API:** `TextAutoSize.StepBased`, `TextAutoSizeDefaults`, `TextAutoSizeLayoutScope`, `BasicText(…, autoSize)`.
- **What:** the largest font size (by steps) at which the text fits its constraints.
- **Tests:** `TextAutoSizeTest`.
- **Size:** M. **Experimental:** no.

### basic-text-drawing
- **Compose API:** `BasicText(…, color: ColorProducer)`; brush/shadow/draw-style changes drawn without relayout.
- **Tests:** `BasicTextBrushTest`, the color-lambda and brush cases of `NodeInvalidationTestParent`.
- **Depends on:** other area: ui-text `TextStyle.brush`, `shadow`, `drawStyle`.
- **Size:** S (plus the ui-text work). **Experimental:** no.

### background-text-measurement
- **Compose API:** `LocalBackgroundTextMeasurementExecutor`.
- **What:** text layouts prefetched on a background executor during composition, used by the measurement if ready.
- **Tests:** `BasicTextPrefetchTest`.
- **Depends on:** a thread-safe measurer path (compose-rs measures on the UI thread; Android's measurer calls into Kotlin).
- **Size:** M. **Experimental:** no.

### autofill-highlight
- **Compose API:** `LocalAutofillHighlightBrush`.
- **Tests:** none in this module (the legacy `CoreTextFieldHighlightTest` is n/a).
- **Depends on:** other area: ui autofill.
- **Size:** S. **Experimental:** no.

### Other areas' features named in statuses

- **ui-text-substitution** (ui semantics/Android translation): `SetTextSubstitution`/`ShowTextSubstitution` and content capture, for `TextStringContentCaptureTest` and `TextAnnotatedStringContentCaptureInvalidationTest`.
- **ui-text-async-fonts** (ui-text): font families resolved asynchronously (downloadable fonts), for `IntrinsicTextAsyncFontTest` and the async-font cases of the field size tests.

### Suggested order

1. `interactions` (M), then `indication` (M): they gate most of the 400-odd clickable/toggleable/selectable/focusable/hoverable tests, and Material's ripple should move onto them before more components copy today's color layer.
2. `tri-state-toggleable` (S), `clickable-parity` (M), `focusable-parity` (M), `mutator-mutex` (S), `progress-semantics` (S), `canvas-content-description` (S).
3. `shapes` (L), then `border-parity` (M); `image-painter` (M) once ui has `Painter`.
4. `text-field-state` (XL) with `keyboard-options-parity` (S) and `cursor-blink` (S): the largest block of tests, and the non-deprecated field. Then `undo-parity` (M), `input-transformation` (M), `output-transformation` (L), `secure-text-field` (M), `text-field-styles` (L).
5. `text-selection-colors` (S), `hover-icon-override` (S), `selection-state` (M).
6. `touch-text-selection` (XL) with `magnifier` (M), then `text-context-menu` (L), `smart-selection` (M).
7. `android-ime` (L), then `handwriting` (L) and `dead-keys` (S).
8. `text-links` (L), `inline-content` (L), `text-auto-size` (M), `basic-text-drawing` (S), as ui-text lands what they need; `background-text-measurement` (M).
9. `bring-into-view-requester` (M), `basic-marquee` (M), `basic-tooltip` (M), `android-window-rects` (S).
10. `drag-and-drop` (L), `content-receiver` (L), `autofill-highlight` (S).
11. `polygon-shapes` (XL), `android-external-surface` (L), `style-api` (XL, experimental and still changing: 42 of its scope interfaces are already deprecated).

## 5. Notes for the porting agents

### Mapping Compose's test patterns to `compose::test`

| Compose | compose-rs |
|---|---|
| `rule.setContent { … }` | `compose_test(\|\| …)` (400×800 at density 1, fixed-advance text), `ComposeTestRule::sized`, `compose_test_with(size, density, measurer, …)` (the Skia measurer for real text), `ComposeTestRule::with_manual_clock` for `mainClock.autoAdvance = false` |
| `onNodeWithTag/Text/ContentDescription`, `onNode(matcher)`, `onAllNodes(…)[i]`, `useUnmergedTree = true` | `on_node_with_tag/text/content_description`, `on_node(matcher)`, `on_all_nodes(matcher, i)`, `on_node_unmerged`; matchers `has_text`, `has_test_tag`, `has_role`, `has_click_action`, `is_enabled`, `is_focused`, `is_focusable` |
| `performClick()` | `perform_click()`: a press and release at the centre **with a mouse pointer** (`PointerEvent::press` defaults to `PointerType::Mouse`); Compose's is a touch. Send touch events yourself where hover or touch-only behaviour matters |
| `performTouchInput { down; moveTo; up; longClick(); doubleClick(); swipe… }` | no DSL: `ui.host().send_pointer(PointerEvent::press(p).with_pointer(PointerId(n), PointerType::Touch))`, `PointerEvent::moved(p, true)`, `PointerEvent::release(p)`, with `ui.advance_time_by(…)` between (long press 400 ms, double tap 40–300 ms, as Compose's test `ViewConfiguration`) |
| `performMouseInput { moveTo; enter; exit; press; scroll; rightClick() }` | `PointerEvent::moved(p, false)` for hover (exit through `send_pointer_left`), `PointerEvent::scroll`; no buttons, so no right click |
| `performKeyInput { pressKey(…); withKeyDown(Key.ShiftLeft) { … } }` | `ui.press_key(key)` (no modifiers) or `ui.host().send_key(KeyEvent { key, kind, modifiers: KeyModifiers { shift: true, .. } })` |
| `performTextInput`, `performImeAction`, `performSemanticsAction(OnClick/RequestFocus/OnLongClick)` | `perform_text_input`, `ui.host().perform_ime_action(action)`, `perform_semantics_action(SemanticsAction::…)` |
| `performTextReplacement`, `performTextClearance`, `performTextInputSelection`, `performSemanticsAction(SetText/SetSelection/CopyText/…)` | missing (need the ui semantics text actions) |
| `mainClock.advanceTimeBy`, `advanceTimeByFrame`, `waitForIdle`, `runOnIdle` | `advance_time_by`, `advance_time_by_frame`, `wait_for_idle` (actions already wait); `runOnIdle` is plain code after an action |
| `assertIsOn/Off`, `assertIsEnabled/NotEnabled`, `assertIsFocused`, `assertTextEquals`, `assertExists/DoesNotExist` | the same names in snake case |
| `assertIsSelected`, `assertIsToggleable`, `assertHasClickAction`, `assertEditableTextEquals`, `assertTextSelectionRange`, `assertContentDescriptionEquals`, `assertWidthIsEqualTo`/`assertPositionInRootIsEqualTo`/`getUnclippedBoundsInRoot` | missing as assertions: read `fetch_semantics_node()` (`config.selected`, `config.toggleable_state`, `config.clickable`, `config.editable_text`, the node's bounds) |
| `captureToImage().assertPixels/assertShape`, screenshot goldens | no capture in `compose::test`: assert on `dump_display_list()` / `draw_snapshot`, or render with Skia's raster backend as `crates/compose-skia/tests/suite/*` does for pixel checks |
| `interactionSource.interactions.collect { interactions.add(it) }` in a `LaunchedEffect` | not possible until `interactions`; meanwhile assert `source.is_pressed()`/`is_focused()`/`is_hovered()` |
| `LocalClipboard`, `LocalUriHandler`, `LocalHapticFeedback` fakes | `CompositionLocalProvider(LOCAL_CLIPBOARD_MANAGER.provides(…))`, `platform::MemoryClipboard`; `LOCAL_URI_HANDLER` and `LOCAL_HAPTIC_FEEDBACK` likewise |
| `LocalTextToolbar` fake, `InputMethodInterceptor`/`FakeInputMethodManager`, `InputModeManager` (touch vs keyboard mode) | missing: no text toolbar, no Rust-side input connection (the host exposes `text_input_options()`, `take_software_keyboard_request()`, `send_preedit`, `send_text`), no input mode |
| `createAndroidComposeRule<Activity>`, `AndroidView`, View hierarchy assertions | only through compose-android's host tests (`crates/compose-android/tests`, Rust with the Kotlin side modelled); there are no instrumented tests |
| `@RunWith(Parameterized::class)` (key types, LTR/RTL, flag flippers) | one Rust test per parameter, or a loop over the cases inside one test naming the case in its assertion messages |

Cite the Compose test in each adapted test's doc comment (`/// Compose: ClickableTest.click_consumedWhenDisabled`), rewrite the scenario from scratch, and keep compose-rs's default optimizations unless the test is about Compose's own rendering behaviour (`use_optimizations(Optimizations::NONE)` before composing, for parity tests, as D610 says).

### What the harness lacks, by priority

1. Pointer buttons and keyboard modifiers on `PointerEvent` (right click for every context-menu test, Shift+click for `TextFieldShiftClickTest`): a ui-area change, then `perform_mouse_input`-style helpers.
2. A touch/mouse gesture DSL (`down`, `move_to`, `up`, `long_click`, `double_click`, `swipe`) on `SemanticsNodeInteraction`, defaulting to touch as Compose's.
3. Semantics assertions and text actions listed above (`assert_is_selected`, `assert_editable_text_equals`, `assert_text_selection_range`, `perform_text_replacement`, `perform_text_clearance`, `perform_text_input_selection`).
4. An interaction recorder once `interactions` exists (`record_interactions(&source) -> Rc<RefCell<Vec<Interaction>>>`).
5. A pixel capture (`capture_to_image()` through Skia's raster backend, available when the test crate depends on compose-skia), for the border/background/selection-color tests.
6. Fakes for the text toolbar and the input connection, and a touch/keyboard input mode switch.

### Behaviour that probably differs from Compose

- **No default indication.** `ClickableOptions::DEFAULT.indication` is `Indication::NONE`, and the clickable node draws its own 8 %/10 % state layers; Compose's clickables draw `LocalIndication.current` unless given `null`. Caught by `ClickableTest.localIndication_interactionSource_eagerlyCreated` and the other `localIndication_*` tests.
- **Press shown at once inside scrollables.** Compose delays a clickable's press (interaction and indication) inside a scrollable container and drops it if the gesture becomes a scroll; compose-rs presses on down. `ClickableTest.interactionSource_scrollableContainer`, `interactionSource_immediateDrag_scrollableContainer`, `ClickableInScrollableViewGroupTest.clickable_scrollableViewGroup`.
- **Always focusable.** compose-rs's clickables and focusables take focus as on desktop; on Android in touch mode Compose's don't. `ClickableTest.requestFocus_touchMode`, `FocusableTest.requestFocus_touchMode`.
- **Corner sizes.** compose-rs clamps each corner to half the shorter side; Compose scales the two start corners together, and the two end corners together, only when their sum exceeds the shorter side, and throws on negative sizes. A 40 px top-start with a 0 px bottom-start on a 50 px-high box is 40 px in Compose, 25 px in compose-rs. `CornerBasedShapeTest.overSizedCornerSizesShouldProportionallyScale`, `topCornersSizesAreNotLargerThenMinDimension`.
- **Border widths.** A negative width gives a negative `ceil`ed stroke instead of drawing nothing, and the shape is filled already when twice the width equals the smaller side (Compose fills only beyond it). `BorderTest.border_lessThanZero_doesNothing` and `BorderRendererTest.drawBorder_negativeWidth_drawsNothing` catch the first; `BorderTest.border_biggerThanLayout_fills` checks only the beyond case, so the equal case needs a test of its own.
- **Triple click in `SelectionContainer` selects the whole text**, Compose's selects the paragraph (`SelectionAdjustment.Paragraph`, `SelectionGestures.kt`). The field already selects the paragraph. `SelectionAdjustmentTest` (paragraph cases).
- **Selection order is placement order** (D151); Compose sorts selectables by position in the container (top to bottom, then start to end). They differ where placement order isn't visual order (a `Box`'s stacking, `z_index`, reversed layouts). `SelectionRegistrarImplTest.sort_vertical_oriented`, `SelectionContainerTest.selectionRegistrar_sortsLazySelectablesCorrectly`.
- **No touch selection.** A long press on text or in a field doesn't select (no handles, toolbar or magnifier). `SelectionContainerTest.long_press_select_a_word`, `TextFieldLongPressTest`.
- **The cursor doesn't blink.** `TextFieldCursorTest.cursorBlinkingAnimation`.
- **No default field width.** compose-rs's field is as wide as its text plus the cursor; Compose's is at least 10 "H" characters wide (`DefaultWidthCharCount`). `BasicTextFieldTest.textField_textAlignCenter_defaultWidth`. Material's 280 dp minimum hides it there.
- **Single-line new lines.** compose-rs's single-line field drops typed or pasted `\n` (not `\r`), and draws a `value` containing either as is; Compose's state-based single-line field keeps the text and shows `\n` as a space, `\r` as a zero-width space. `SingleLineOutputTransformationTest`.
- **Initial cursor.** The value-based field starts at the start (as `TextFieldValue`'s default), but `TextFieldState(initialText)` puts the cursor at the end. `TextFieldStateTest.customInitialTextAndDefaultSelection`. Ported field tests must not assume the old start.
- **Undo grouping** (D118): compose-rs joins typing within 5 s of the last typed text and clears the history when the application changes the value; Compose merges by edit type and direction within 5 s of a group's first edit (deletions too, new lines never) and records programmatic edits. `TextUndoTest`, `BasicTextFieldUndoTest.programmaticEdits_contributeToUndoHistory`, `undoMerges_deletes`.
- **Back doesn't collapse a field's selection.** `TextFieldSelectionOnBackTest.whenBackPressed_andReleased_textFieldClearsSelection`.
- **Keys the field ignores:** Page Up/Down, numpad navigation keys, Shift+Insert/Ctrl+Insert, `DirectionCenter`. `CommonTextFieldKeyEventTest.textField_pageUp`, `textField_shiftInsert`, `TextFieldKeyEventTest.textField_pageNavigationDown`.
- **API shape:** `Image`'s `content_description` is `Option<&'static str>`, where other text parameters take `impl Into<SharedString>` (06-ui's rule): a dynamic description can't be passed.
