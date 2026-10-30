# Parity ledger: UI input, focus, semantics, accessibility and platform hosts

This ledger maps Jetpack Compose UI's tests and public API in input, focus, semantics, accessibility, autofill, content capture, drag and drop and the platform hosts onto compose-rs. It says which Compose tests compose-rs already covers, which can be ported now, and which need a missing feature. It drives the porting work: section 2 is the per-file list, section 4 the features to build, section 5 what porting agents need to know.

## 1. Scope and totals

- **Reference:** the androidx checkout at commit `d153d87ed76` (2026-10-25), module `compose/ui/ui` (paths below are relative to `frameworks/support`). Its tests live in two source sets: `src/androidDeviceTest` (instrumented) and `src/androidHostTest` (JVM). There is no `commonTest`.
- **In this area:** `focus/`, `input/` (pointer, key, nested scroll, rotary, indirect, focus-aware events, and the Android IME bridge under `input/` and `text/input/`), `semantics/`, `accessibility/`, the accessibility and host tests at the package root, `autofill/`, `contentcapture/`, `draganddrop/`, `platform/` (but its font, inspector, `LayoutId` and recycled-layer tests), `owners/`, `viewinterop/`, `window/` (popups and dialogs are platform windows in Compose), `scrollcapture/`, `relocation/`, `adaptive/`, `hapticfeedback/`, `gesture/` (velocity-tracking math) and the hit-testing tests in `node/`.
- **Left to other areas:** layout, drawing, graphics, text measurement, `modifier/` and `node/` (other than hit testing), `spatial/`, `res/`, `tooling/`. Shared: `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/LayoutNodeTest.kt` (90 methods, of which 31 `hitTest_*` test this area's hit testing; not counted here, owned by the layout ledger).
- **The text input rows** (`input/` IME classes, `text/input/`, `text/TextActionModeCallbackTest.kt`) are the platform host's input-method bridge; the text area may claim them too.
- **Totals:** 257 Kotlin files: 239 test classes with 3,525 `@Test` methods (counted with `grep -cE "^\s*@(Test|org\.junit\.Test)\b"`), and 18 helpers without tests (listed after the table). 28 classes are parameterized (directions, input modes, node types), so they run more cases than methods: the two-dimensional focus tests alone are 205 methods and 820 cases.

| Status | Files | `@Test` methods |
|---|---|---|
| `covered` | 5 | 39 |
| `port` | 15 | 302 |
| `partial` | 36 | 1,185 |
| `feature:*` | 133 | 1,543 |
| `n/a` | 50 | 456 |
| **Total** | **239** | **3,525** |

By feature (rows whose status is that feature; `partial` rows needing it are in section 4):

| Feature | Files | Methods |
|---|---|---|
| host-view-interop | 23 | 294 |
| autofill | 11 | 166 |
| indirect-pointer-input | 7 | 123 |
| focus-hierarchy | 6 | 97 |
| velocity-tracker | 6 | 90 |
| focus-properties | 9 | 79 |
| platform-text-input | 7 | 76 |
| focus-requester-api | 8 | 62 |
| pointer-icon-api | 1 | 59 |
| pointer-event-model | 3 | 45 |
| scroll-capture | 3 | 44 |
| semantics-tree-api | 2 | 21 |
| media-query | 2 | 42 |
| focus-search-tree | 3 | 39 |
| nested-scroll-dispatcher | 1 | 34 |
| clipboard-api | 2 | 30 |
| platform-window-modifiers | 4 | 25 |
| content-capture | 1 | 22 |
| window-info | 4 | 22 |
| hit-test-extensions | 5 | 21 |
| popup-dialog-properties | 4 | 19 |
| android-accessibility-parity | 3 | 18 |
| semantics-properties | 1 | 17 |
| sound-effects | 3 | 16 |
| rotary-input | 1 | 14 |
| input-mode | 3 | 13 |
| key-focus-navigation | 1 | 13 |
| drag-and-drop | 1 | 12 |
| view-configuration | 2 | 9 |
| platform-locals | 4 | 9 |
| key-input-api | 1 | 6 |
| text-toolbar | 1 | 6 |

**Reading the status.** `covered`: compose-rs tests already exercise it. `port`: the feature exists, adapt the tests. `partial`: some exists; the detail says what is missing and which feature brings it. `feature:<name>`: needs that missing feature (section 4). `n/a`: no compose-rs analogue, with the reason.

**compose-rs test paths are abbreviated:** `ui/x.rs` is `crates/compose-ui/tests/suite/x.rs`, `fnd/x.rs` is `crates/compose-foundation/tests/suite/x.rs`, `m3/x.rs` is `crates/compose-material3/tests/suite/x.rs`, `and/x.rs` is `crates/compose-android/tests/x.rs`, `and-src/x.rs` is `crates/compose-android/src/x.rs` (unit tests), `dsk/x.rs` is `crates/compose-desktop/tests/x.rs`.

## 2. Compose test files

### Focus (54 files, 761 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CancelFocusMoveTest.kt` | 11 | `focusProperties { next = FocusRequester.Cancel }` and friends stop moves per direction | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CaptureFocusTest.kt` | 8 | `captureFocus` and the resulting `FocusState` | `feature:focus-requester-api` | Needs focus-hierarchy states too. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ClearFocusExitTest.kt` | 11 | `clearFocus` runs `onExit` on ancestors, which can cancel or redirect it | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CombinedFocusModifierNodeTest.kt` | 7 | One node delegating to focus target, requester and properties nodes; `canFocus` changes | `feature:focus-hierarchy` | Public focus node API (`FocusTargetModifierNode`, `FocusRequesterModifierNode`, `FocusPropertiesModifierNode`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ComposeViewKeyEventInteropTest.kt` | 5 | A `ComposeView` consumes a key only when focus moved; Back leaving to the root isn't consumed | `feature:host-view-interop` | compose-rs always consumes an unconsumed Tab, even with nothing to focus (see notes). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ComposeWithinAndroidViewsInteropTest.kt` | 2 | Focus moves between composables in embedded cousin views | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CustomFocusRectTest.kt` | 8 | `focusProperties { focusRect }` and ancestors' rects | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/CustomFocusTraversalTest.kt` | 15 | `focusProperties` custom next/previous/up/down/left/right/start/end/enter/exit, outermost wins, state-driven changes | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/DeactivatedFocusNodeTest.kt` | 4 | Focus events of deactivated (`canFocus = false`) nodes | `feature:focus-hierarchy` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusAggregationTest.kt` | 14 | A parent's `FocusState` aggregates its children's (hasFocus, isFocused, isCaptured) | `partial` | `hasFocus` aggregation exists and is tested (`ui/review_d.rs` `on_focus_changed_observes_a_focused_child`); `isFocused`/`isCaptured` need focus-hierarchy. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedCountTest.kt` | 8 | How often `onFocusChanged` is called: initially, on gain, loss, removal, re-request | `port` | Reported on attach and on change (D85); partly covered by `ui/review_d.rs` (`on_focus_changed_reports_the_initial_state`, `losing_the_focus_target_from_the_chain_notifies_on_focus_changed`). Assert `hasFocus` values until focus-hierarchy brings `FocusState`. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusChangedTest.kt` | 19 | `FocusState` transitions on `requestFocus` for each state; interaction barriers block focus search and keys | `feature:focus-hierarchy` | The barrier tests use the internal `interactionBarrier()`; their scenario (content under a modal blocks focus and keys) maps onto compose-rs's modal overlays (`fnd/review_overlay.rs`, `fnd/review_overlay2.rs`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusEventCountTest.kt` | 25 | How often `onFocusEvent` is called, with delegating nodes and `focusProperties` changes | `feature:focus-hierarchy` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusListenerTest.kt` | 5 | The focus owner's listener sees previous/current focused nodes | n/a: internal `FocusListener` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusManagerCompositionLocalTest.kt` | 10 | `LocalFocusManager.clearFocus`: whole hierarchy cleared, captured focus and `force`, inactive root, nested views | `partial` | `clear_focus` exists (`fnd/forms.rs` `the_focus_manager_clears_the_focus`, `fnd/review_forms.rs`); captured focus/`force` need focus-requester-api; nested `ComposeView` cases are host-view-interop. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRequesterTest.kt` | 23 | `requestFocus` on nodes without a target, targets below, first child, deactivated parents with implicit enter, removed children | `partial` | Focusing the node's target or the first one below exists (D90; `fnd/focus_requester.rs`, `fnd/review_round12.rs`, `fnd/review_reuse_d434.rs`); deactivated parents and enter need focus-hierarchy and focus-properties. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerConfigurationChangeTest.kt` | 1 | `focusRestorer` restores after activity recreation | `feature:focus-requester-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusRestorerTest.kt` | 13 | `Modifier.focusRestorer(fallback)` restores the last focused child, in lazy lists too | `feature:focus-requester-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusSearchNonPlacedItemsTest.kt` | 15 | Focus moves skip items that aren't placed, in every direction | `port` | `focus_order` skips unplaced nodes (`crates/compose-ui/src/focus.rs`); the `Enter` case needs focus-properties (`FocusDirection::Enter`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusSoundInteropTest.kt` | 3 | No Compose sound when Tab crosses `ComposeView`s | `feature:sound-effects` | Also host-view-interop. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusSoundTest.kt` | 8 | D-pad, cluster and Tab navigation play the navigation sound | `feature:sound-effects` | Needs key-focus-navigation. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt` | 49 | Focus states as focus targets are added, removed and reordered in focused and inactive hierarchies | `partial` | Losing a removed target and keeping focus when one is inserted before it are tested (`ui/ui.rs` `removing_the_focused_node_clears_focus`, `ui/review_d.rs`); parent states (`ActiveParent`, deactivated) need focus-hierarchy. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetModifierNodeTest.kt` | 17 | `FocusTargetModifierNode(focusability, onFocusChange)`, `Focusability` in touch and keyboard modes, focused rect | `feature:focus-hierarchy` | Needs input-mode for `SystemDefined`. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTransactionsTest.kt` | 5 | Re-entrant `requestFocus` from `onFocusChanged`, cancelling a take of focus | `partial` | Requests from callbacks are queued until the tree is free (D148), so re-entrancy is portable now; cancelling through `onEnter` needs focus-properties. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusViewInteropTest.kt` | 22 | `getFocusedRect` and focus moves between `ComposeView`s and Views | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FreeFocusTest.kt` | 5 | `freeFocus` from each state | `feature:focus-requester-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/InitialFocusTest.kt` | 7 | The owner assigns initial focus to the top-left item in keyboard mode | `feature:input-mode` | Also host-view-interop (owner focus). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/KeyEventToFocusDirectionTest.kt` | 13 | Arrow, page, Tab, D-pad center, Enter, Back and Escape keys map to focus directions | `feature:key-focus-navigation` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchNextTest.kt` | 21 | `moveFocus(Next)` order: initial focus, deactivated items, parents before children, wrap-around | `partial` | Tab order and wrap exist (`ui/ui.rs` `tab_moves_focus_through_clickables_and_enter_clicks`, `fnd/review_overlay2.rs`, `fnd/review_round12.rs`); deactivated items need focus-properties (`canFocus`), parent/child order across nested targets needs focus-search-tree; `moveFocus`'s returned value differs (notes). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OneDimensionalFocusSearchPreviousTest.kt` | 24 | `moveFocus(Previous)` order, mirror of the above | `partial` | As the row above; `fnd/review_forms.rs` `previous_from_no_focus_goes_to_the_last`. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/OwnerFocusTest.kt` | 10 | The Android view's focus and window focus drive Compose's root focus | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestChildFocusTest.kt` | 18 | `requestFocusForChildInLocalBounds/RootBounds` pick the child overlapping a rect | `feature:focus-requester-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusEnterExitTest.kt` | 7 | Which `onEnter`/`onExit` properties can stop a focus change | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusEnterTest.kt` | 13 | Gaining focus runs ancestors' `onEnter`, which can cancel or redirect | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusExitTest.kt` | 1 | Redirecting an exit calls `onExit` | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RequestFocusTest.kt` | 25 | `requestFocus` from every state and relation, with a direction, and the event sequence | `feature:focus-hierarchy` | The single-node cases are portable now (`fnd/semantics_actions.rs` `request_focus_focuses_the_node`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/RestoreFocusTest.kt` | 5 | `saveFocusedChild`/`restoreFocusedChild` | `feature:focus-requester-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ReusedFocusRequesterCaptureFocusTest.kt` | 6 | `captureFocus` on a requester attached to several targets | `feature:focus-requester-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ReusedFocusRequesterFreeFocusTest.kt` | 6 | `freeFocus` on a requester attached to several targets | `feature:focus-requester-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/ReusedFocusRequesterTest.kt` | 3 | One requester on several targets: the last one requested keeps focus | `port` | A requester on several nodes exists, but compose-rs focuses the first attached where Compose ends on the last (notes). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalEnterTest.kt` | 10 | `moveFocus(Enter)` into children: top-left child, deactivated children, custom enter | `feature:focus-search-tree` | Needs `FocusDirection::Enter` (focus-properties). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalExitTest.kt` | 21 | `moveFocus(Exit)` to parents; 2D moves leaving a parent | `feature:focus-search-tree` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitEnterTest.kt` | 5 | Implicit enter through custom `enter` during 2D moves | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalImplicitExitTest.kt` | 8 | Implicit exit through custom `exit` during 2D moves | `feature:focus-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalInitialFocusTest.kt` | 5 | 2D moves with nothing focused pick the first item from the corner | `port` | Implemented (`crates/compose-ui/src/focus_manager.rs` `two_dimensional_search_in`); deactivated items map to `focusable_with(false)`. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalTest.kt` | 8 | 2D moves in hierarchies: siblings before closer ancestors, leaving parents, deactivated parents | `feature:focus-search-tree` | compose-rs searches all focus targets flat (except lazy-list groups, D483). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalThreeItemsTest.kt` | 71 | Candidate choice among three items: in-beam beats closer, valid beats closer | `port` | Flat sibling boxes at absolute offsets; the beam and weighted-distance rules exist (`fnd/forms.rs` `the_focus_manager_moves_the_focus_in_two_dimensions`, `fnd/review_forms.rs`). Parameterized over 4 directions (284 cases). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/TwoDimensionalFocusTraversalTwoItemsTest.kt` | 134 | Whether one candidate is valid in each direction: overlapping, adjacent, common boundaries | `port` | As above; parameterized over 4 directions (536 cases). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/WrapAroundFocusTest.kt` | 10 | `Next`/`Previous` wrap around rows, or not (the owner's internal wrap flag), in touch and keyboard modes | `port` | Tab wraps (`fnd/review_overlay2.rs` `tab_from_the_last_focusable_of_a_dialog_wraps_to_its_first`): the wrapping half ports now; the non-wrapping half is how an embedded view lets focus leave (host-view-interop). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/InputModeTest.kt` | 1 | Switching to touch mode programmatically | `feature:input-mode` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/focus/FocusAwareEventPropagationTest.kt` | 13 | Focus-aware events (keys, rotary, soft-keyboard interception) go from the focused node up, preview pass first | `partial` | Key bubbling exists (`ui/ui.rs` `key_events_bubble_from_the_focused_node_until_consumed`, `ui/review_d.rs` `on_key_event_after_focusable_receives_keys`); preview pass and interception need key-input-api, rotary needs rotary-input. Parameterized over 3 node types. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/focus/FocusChangedModifierTest.kt` | 1 | Inspector value of `onFocusChanged` | n/a: `InspectorInfo` (tooling) has no analogue |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/focus/FocusEventModifierTest.kt` | 1 | Inspector value of `onFocusEvent` | n/a: `InspectorInfo` (tooling) has no analogue |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/focus/FocusRequesterModifierTest.kt` | 1 | Inspector value of `focusRequester` | n/a: `InspectorInfo` (tooling) has no analogue |  |

### Key input (6 files, 36 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/key/AndroidProcessKeyInputTest.kt` | 1 | An Android `KeyEvent` reaches `onKeyEvent` | `port` | `ComposeRsNative.key` → `UiHost::send_key` (`crates/compose-android/src/bridge.rs`); no test of that path yet. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/key/HardwareKeyInputTest.kt` | 6 | Preview and soft-keyboard interception order for hardware keys | `feature:key-input-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/key/KeyTest.kt` | 2 | Android key codes ↔ Compose `Key` | `partial` | The bridge maps 14 keys and characters (`bridge.rs` `Java_rs_compose_android_ComposeRsNative_key`); Compose's full `Key` set and `nativeKeyCode` need key-input-api. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/key/MetaKeyTest.kt` | 8 | `isAltPressed`/`isCtrlPressed`/`isMetaPressed`/`isShiftPressed` from the meta state | `port` | `KeyModifiers` and the bridge's meta mapping exist; untested. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/key/ProcessKeyInputTest.kt` | 17 | `onKeyEvent`/`onPreviewKeyEvent` order, parents and children, updates, up without down ignored, unknown type | `partial` | `on_key_event` bubbling and updates are tested (`ui/ui.rs`, `ui/review_d.rs`, `fnd/review_f.rs`); preview, up-without-down filtering and `KeyEventType::Unknown` need key-input-api. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/key/KeyInputModifierTest.kt` | 2 | Inspector values of `onKeyEvent`/`onPreviewKeyEvent` | n/a: `InspectorInfo` (tooling) has no analogue |  |

### Pointer input and hit testing (33 files, 718 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/ClickNotPlacedChildTest.kt` | 1 | A child first not placed, placed later, takes clicks | `port` | Unplaced nodes aren't hit (`crates/compose-ui/src/dispatch.rs` `hit_node`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/InputInLayerTest.kt` | 1 | A reused graphics layer's transform is reset for input | `feature:hit-test-extensions` | Hit testing ignores layer transforms (D121). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/AndroidPointerInputTest.kt` | 72 | `AndroidComposeView` touch, hover, scroll and stylus dispatch; minimum touch targets; overlapping and z-ordered siblings; dynamic modifiers | `partial` | Portable now: hover enter/exit/move, press/release cycles, scroll, overlapping siblings and `z_index` (`fnd/modifiers_api.rs` `z_index_draws_and_hits_a_sibling_over_the_others`, `ui/ui.rs`, `fnd/hover.rs`, `fnd/review_round6.rs`). Needs pointer-event-model (classification, trackpad pan/scale, stylus hover, buttons), hit-test-extensions (minimum touch target, layer transforms), host-view-interop (`requestDisallowInterceptTouchEvent`, return values). Invalid-coordinate and `MotionEvent` cases are n/a. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/ClipPointerInputTest.kt` | 9 | Clipped content isn't hit: `clipToBounds`, rounded rects, generic shapes; interaction barriers | `partial` | Clip hit testing follows the outline (D101, D523; `fnd/scroll.rs` `content_scrolled_out_of_the_viewport_is_not_hit`); generic shapes need `GenericShape` outlines (drawing area, D529); barrier tests use the internal `interactionBarrier()`. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt` | 80 | The hit path tree: merging paths per pointer, dispatch order per pass, cancel, removal mid-stream | `partial` | Internal `HitPathTracker`; its scenarios hold through the public API: single-pointer captures and passes exist (`crates/compose-ui/src/dispatch.rs`, `ui/review_d.rs` `a_second_pointer_does_not_complete_the_first_pointers_click`); events split across pointers need pointer-event-model. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/LayerTouchTransformTest.kt` | 1 | A press inside a scaled, rotated, translated layer is hit | `feature:hit-test-extensions` | Hit testing ignores layer transforms (D121). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MotionEventAdapterTest.kt` | 44 | `MotionEvent` → pointer input events: ids, tool types, pointer down/up, hover, historical | `partial` | `ComposeRsView.onTouchEvent` converts touches (ids, pointer down/up, historical samples as separate events, cancel); untested. Every pointer is typed touch (no tool types, buttons), hover and mouse events aren't forwarded, historical changes aren't `historical`: pointer-event-model. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MotionEventSpyTest.kt` | 2 | `Modifier.motionEventSpy` sees raw `MotionEvent`s | n/a: hands Android `MotionEvent`s to content; compose-rs content has no `MotionEvent` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MouseEventTest.kt` | 6 | Synthesized Enter/Exit, relayout under the cursor, no synthesized events after up or hover exit | `port` | Hover and re-hit after layout exist (`fnd/review_round6.rs` `scrolling_under_a_resting_mouse_moves_the_hover`, `passes_hover_moves_at_the_release_of_a_drag`, `a_hovered_clickable_whose_modifiers_change_still_hears_exit`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerIconTest.kt` | 59 | `pointerHoverIcon` with `overrideDescendants` across parents, children and siblings, dynamic changes | `partial` | The innermost hovered node's icon wins (`fnd/hover.rs` `a_text_field_shows_an_i_beam`); `overrideDescendants` needs pointer-icon-api. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputDensityTest.kt` | 6 | Density changes restart `pointerInput`; NaN/infinite densities | `feature:view-configuration` | compose-rs reads the density per event and never restarts the block for it. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt` | 48 | Event processing: hit tests over nested and overlapping nodes, offsets, clipping, cancel, removal mid-stream, results, buttons, meta state | `partial` | Single-pointer hit testing, passes and captures exist (`ui/ui.rs`, `ui/review_d.rs`); multi-pointer events, buttons and meta state need pointer-event-model; process results map to host return values (host-view-interop). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputViewConfigurationTest.kt` | 3 | `LocalViewConfiguration` changes restart `pointerInput` | `feature:view-configuration` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInteropFilterAndroidViewHookupTest.kt` | 12 | An `AndroidView` gets its `MotionEvent`s through the filter | n/a: Android `AndroidView` (Views inside composable content) has no analogue: embedding goes one way (D226, docs/guide/differences.md) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInteropFilterAndroidViewOffsetsTest.kt` | 2 | `AndroidView` click offsets | n/a: Android `AndroidView` (Views inside composable content) has no analogue: embedding goes one way (D226, docs/guide/differences.md) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInteropFilterComposeHookupTest.kt` | 10 | `pointerInteropFilter` in composable content | n/a: `pointerInteropFilter` hands Android `MotionEvent`s to content; `pointer_input` covers its uses |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInteropFilterTest.kt` | 89 | `pointerInteropFilter`'s `MotionEvent` dispatch, cancel and disallow-intercept | n/a: `pointerInteropFilter` hands Android `MotionEvent`s to content |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInteropUtilsTest.kt` | 18 | Pointer events → `MotionEvent` | n/a: `MotionEvent` reconstruction for `pointerInteropFilter` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/RequestUnbufferedDispatchTest.kt` | 3 | `View.requestUnbufferedDispatch` for raw `MotionEvent` access | n/a: raw `MotionEvent` access only |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/RestrictedSizeTest.kt` | 5 | Pointer positions in the measured size when constraints shrink it; minimum touch target | `partial` | Positions are in the measured size (`layers[].measured`), portable; the minimum touch target cases need hit-test-extensions. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/StylusEventTest.kt` | 1 | Stylus hover exit before re-hover after relayout | `feature:pointer-event-model` | No stylus hover: Android forwards no hover events to content (only to touch exploration) and types every pointer as touch; desktop's hover is the mouse's. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/StylusHoverIconTest.kt` | 59 | `stylusHoverIcon` across overlapping nodes | `feature:pointer-icon-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/SuspendingPointerInputFilterCoroutineJobTest.kt` | 1 | The `pointerInput` job stays active after a cancel event | `feature:pointer-event-model` | compose-rs cancels and restarts the block on cancel (notes). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/SuspendingPointerInputFilterTest.kt` | 31 | `pointerInput` coroutine: awaiting events, synthetic cancel events, keys, block replacement, timeouts, delegation | `partial` | Awaiting events, key restarts, block replacement without restart and `with_timeout` (OrNull) exist (`fnd/pointer_input.rs`, `fnd/review_round7.rs` `passes_callbacks_removing_the_node_or_changing_the_key`); synthetic cancel, `awaitPointerEventScope`, multiple keys, `withTimeout` throwing and `SuspendingPointerInputModifierNode` delegation need pointer-event-model. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/util/PlatformVelocityTrackerTest.kt` | 7 | The platform velocity tracker: positions, changes with history, duplicate times, maximum, reset, gaps | `feature:velocity-tracker` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/SharePointerInputWithSiblingTest.kt` | 5 | `sharePointerInputWithSiblings` lets stacked siblings both get events | `feature:hit-test-extensions` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt` | 21 | The least-squares polynomial fit behind Lsq2 | `feature:velocity-tracker` | Internal function, but a Rust Lsq2 needs the same fit. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/PointerInputTest.kt` | 43 | `PointerInputChange` helpers: `changedToDown/Up(IgnoreConsumed)`, `positionChange(d)(IgnoreConsumed)`, consumption | `feature:pointer-event-model` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt` | 36 | `VelocityTracker1D`: Lsq2 and impulse, differential data, linear and stopped motion | `feature:velocity-tracker` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt` | 10 | `VelocityTracker`: expected values, maximum velocity, the 40 ms gap, few samples | `feature:velocity-tracker` | compose-rs's internal tracker (`crates/compose-foundation/src/fling.rs`) is a linear fit over 100 ms with no 40 ms gap rule (notes). |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/HitTestResultTest.kt` | 19 | The internal hit result list: hits, clipped hits, minimum-touch-target speculative hits | n/a: internal `HitTestResult`; the behaviour is ported from `AndroidPointerInputTest` and `RestrictedSizeTest` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/HitTestTouchBoundsExpansionTest.kt` | 10 | `TouchBoundsExpansion` widens hit bounds (stylus only, RTL, out of parent, overlaps) | `feature:hit-test-extensions` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/TouchBoundsExpansionTest.kt` | 4 | `TouchBoundsExpansion` construction and limits | `feature:hit-test-extensions` |  |

### Nested scroll (1 file, 34 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/nestedscroll/NestedScrollModifierTest.kt` | 34 | Dispatch order and deltas through `NestedScrollDispatcher` chains, removal and swaps, scope, mouse vs touch flings | `feature:nested-scroll-dispatcher` | Connections exist (D188, `crates/compose-ui/src/nested_scroll.rs`); the 6 tests driven by real scrollables are portable now and partly covered (`fnd/review_nested_scroll.rs`); the 28 others dispatch by hand through a `NestedScrollDispatcher`. |

### Rotary and indirect input (8 files, 137 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/indirect/DelegatedIndirectPointerAndFocusEventTests.kt` | 10 | Indirect pointer streams are cancelled when focus moves | `feature:indirect-pointer-input` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/indirect/IndirectPointerEventNavigationSystemTests.kt` | 53 | Touch-navigation swipes move focus along the primary axis | `feature:indirect-pointer-input` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/indirect/IndirectPointerEventTest.kt` | 20 | Touch-navigation `MotionEvent`s become `IndirectPointerEvent`s to the focused node and its ancestors | `feature:indirect-pointer-input` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/indirect/IndirectPointerNavigationGestureDetectorTest.kt` | 14 | The navigation gesture detector's swipe thresholds | `feature:indirect-pointer-input` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/rotary/RotaryScrollEventTest.kt` | 14 | Rotary crown events reach the focused node with device id and time; preview; view interop | `feature:rotary-input` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/IndirectPointerEventWithInputDeviceMockTest.kt` | 15 | The primary axis from the input device's motion ranges | `feature:indirect-pointer-input` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/indirect/IndirectPointerInputChangeTest.kt` | 10 | `IndirectPointerInputChange` properties, consume, copy, historical | `feature:indirect-pointer-input` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/indirect/IndirectPointerNavigationGestureDetectorHostTest.kt` | 1 | The detector clears pending messages on dispose | `feature:indirect-pointer-input` |  |

### Semantics (7 files, 142 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/MergedSemanticsConfigurationTest.kt` | 11 | The merged configuration of a node: merging, clearing, deep hierarchies, nested mergers | `covered` | Ported through the merged and unmerged trees: `crates/compose-foundation/tests/suite/semantics_properties.rs` (`a_single_modifier_merges_to_itself` … `merging_goes_deep_and_stops_at_mergers_and_clearers`); the merged texts in the children's order, as `SemanticsNode`'s (the test's `SemanticsInfo.mergedSemanticsConfiguration` walks them last first). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsInfoTest.kt` | 9 | `SemanticsInfo` of layout nodes, deactivated nodes, transparency | `feature:semantics-tree-api` | Transparent (alpha 0) nodes are still in compose-rs's semantics. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsListenerTest.kt` | 13 | The owner's semantics listeners fire on property, text and focus changes | n/a: internal `SemanticsListener`; compose-rs's per-frame invalidation (D630) is tested by `and/accessibility_state.rs` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsModifierNodeTest.kt` | 22 | `applySemantics` is lazy, cached, re-run on `invalidateSemantics` and observed reads | `covered` | Ported (`crates/compose-foundation/tests/suite/semantics_modifier_node.rs`, D1257, D1260): kept per node, invalidated, observed reads, deactivated nodes, invalidations while computing, the bounds' flags; compose-rs computes when next asked where Compose computes at the invalidation, so the counts are the reads'. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsPropertiesAndroidTest.kt` | 1 | The credential-request semantics property | `feature:autofill` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTests.kt` | 74 | Semantics tree: merging, clearing, outer over inner, updates on add/remove, action merging, z order, bounds, ids, text substitution, visibility of merged children | `partial` | Merging, clearing and override rules are tested (`fnd/review_semantics.rs`, `fnd/review_round5.rs`, `fnd/review_round3.rs`, `ui/ui.rs`); pane title, traversal properties, action collapsing and merging and the merge policies are ported (`crates/compose-foundation/tests/suite/semantics_properties.rs`, D1250); `testRegenerateSemanticsId` and `testSemanticsUpdatedWhenReused` are ported (`crates/compose-foundation/tests/suite/semantics_invalidation.rs`, D1262); content type needs autofill, text substitution needs `BasicText`'s substitution actions; z-sorted children, bounds in parent, fake nodes need semantics-tree-api. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/semantics/SemanticsHostApiTest.kt` | 12 | The experimental host API: place order, merged/unmerged queries, accessibility children, bounds, visibility, hit tests, uncovered nodes | `feature:semantics-tree-api` | Experimental (`ExperimentalComposeUiHostApi`). |

### Accessibility (9 files, 354 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AccessibilityIteratorsTest.kt` | 14 | Text-granularity iterators (character, word, line, paragraph, page) a screen reader moves through | `feature:android-accessibility-parity` | Needs text movement granularities on the Android bridge and text layout queries (`getLineForOffset`…, text area). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAccessibilityTest.kt` | 188 | `AccessibilityNodeInfo` built from semantics: classes per role, traversal order, actions, events, semantics hit testing, pane titles, extras, view interop | `partial` | `Accessibility.kt` + `crates/compose-android/src/accessibility.rs` map text, content description, roles, toggle/selected, focus, click, progress, scroll (and its actions), password, error, dialog pane title, visibility; tests: `and/accessibility_scroll.rs`, `and/accessibility_state.rs`, `and-src/accessibility.rs`. Missing: traversal sorting and groups (`isTraversalGroup`, `traversalIndex`), live regions, pane appear/disappear events, custom actions, text selection/set-text actions, extra data (character locations, test tags as resource ids), sensitive data, transparent-node pruning, uncovered-node pruning, fake nodes, explore-by-touch overlay rules (feature:android-accessibility-parity). View-interop cases are n/a. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityDelegateCompatTest.kt` | 105 | The delegate's node properties, importance, class names, live regions, shape and rendering extras, paste action, text and password events, z drawing order, hint text | `partial` | Covered: role/class mapping, disabled, toggles, progress, text field editable/hint, error (`and-src/accessibility.rs`, `and/accessibility_state.rs`, `dsk/review_accessibility.rs` on desktop). Missing: importance rules, `accessibilityClassName`, live regions, shape/rendering extras, text and selection events, paste action, hide-from-accessibility propagation, drawing order, bounds through layer transforms (feature:android-accessibility-parity, semantics-properties). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ShowOnScreenAccessibilityTest.kt` | 1 | `ACTION_SHOW_ON_SCREEN` without a scrollable parent | `feature:android-accessibility-parity` | Depends on bring-into-view-api. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/accessibility/AccessibilityTouchModeTest.kt` | 5 | An accessibility focus action puts the view in keyboard mode | `feature:input-mode` | Also android-accessibility-parity. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/accessibility/CollectionInfoTest.kt` | 17 | `CollectionInfo`/`CollectionItemInfo` from lazy lists and `selectableGroup` | `feature:semantics-properties` | Then android-accessibility-parity for the node info. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/accessibility/ScrollingTest.kt` | 14 | Scroll events from state observation, `canScroll` of the host view, scroll viewport length | `partial` | `TYPE_VIEW_SCROLLED` and scroll actions exist (`and/accessibility_scroll.rs`, `and/accessibility_state.rs`, D478/D491). Missing: `View.canScrollHorizontally/Vertically` on `ComposeRsView` (host-view-interop), `GetScrollViewportLength` (semantics-properties). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/accessibility/WindowContentChangeTest.kt` | 7 | `TYPE_WINDOW_CONTENT_CHANGED` (undefined change type) when properties or action labels change | `partial` | compose-rs sends one `CONTENT_CHANGE_TYPE_SUBTREE` per frame that recomposed or measured (D630, `and/accessibility_state.rs`); per-property change types and action labels need semantics-properties. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/AndroidComposeViewAccessibilityTraversalTest.kt` | 3 | `traversalIndex` respected among merged descendants; `findViewByAccessibilityIdTraversal` | `feature:android-accessibility-parity` | Needs semantics-properties (`traversalIndex`). |

### Autofill and content capture (17 files, 246 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AndroidAssistTest.kt` | 1 | The assist structure (`ViewStructure`) is populated from semantics | `feature:autofill` | Shares the `ViewStructure` population with autofill. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/AndroidAutoFillTest.kt` | 5 | `LocalAutofill`/`AutofillTree` on Android | n/a: deprecated API (`Autofill`, `AutofillTree`, `AutofillNode`) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/AndroidAutofillManagerTest.kt` | 47 | `AutofillManager` commit/cancel and notifications as autofillable nodes appear, change and disappear | `feature:autofill` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/AndroidCredentialViewStructureTest.kt` | 1 | A credential request populates the `ViewStructure` | `feature:autofill` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/AndroidFillableDataTest.kt` | 18 | `FillableData` typed accessors (text, toggle, list, date) | `feature:autofill` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/MixedAutofillTest.kt` | 8 | Deprecated and semantics autofill side by side | n/a: exercises the deprecated `AutofillTree` path with the new one |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/PerformAndroidAutofillManagerTest.kt` | 43 | `ViewStructure` population from semantics (content type, roles, visibility, focus…) and filling | `feature:autofill` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/TextFieldStateSemanticAutofillTest.kt` | 2 | Credentials autofilled into `BasicTextField(TextFieldState)` | `feature:autofill` | Also needs foundation's `TextFieldState` field (text area). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/TextFieldsSemanticAutofillTest.kt` | 6 | Credentials autofilled into foundation and Material fields, with highlight | `feature:autofill` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/contentcapture/ContentCaptureTest.kt` | 22 | Content capture session events as semantics nodes appear, disappear and scroll | `feature:content-capture` | Experimental (`ContentCaptureManager`). |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/autofill/AndroidAutofillTypeTest.kt` | 36 | `AutofillType` to Android hints | n/a: deprecated API (`AutofillType`) |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/autofill/AndroidContentDataTypeTest.kt` | 5 | `ContentDataType` to Android autofill types | `feature:autofill` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/autofill/AndroidContentTypeTest.kt` | 36 | `ContentType` to Android autofill hints | `feature:autofill` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/autofill/AndroidPerformAutofillTest.kt` | 2 | `AutofillTree.performAutofill` | n/a: deprecated API (`AutofillTree`) |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/autofill/AndroidPopulateViewStructureTest.kt` | 3 | `ViewStructure` from an `AutofillTree` | n/a: deprecated API (`AutofillTree`) |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/autofill/AutofillNodeTest.kt` | 5 | `AutofillNode` ids and requests | n/a: deprecated API (`AutofillNode`) |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/autofill/AutofillUtilsTest.kt` | 6 | Autofill text truncated at the platform's maximum, surrogate pairs kept whole | `feature:autofill` |  |

### Drag and drop (1 file, 12 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draganddrop/AndroidDragAndDropTest.kt` | 12 | `DragEvent`s reach `DragAndDropTarget` nodes: start, enter, move, exit, drop, end, nesting, moves | `feature:drag-and-drop` |  |

### Platform hosts and services (54 files, 376 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ComposeViewContextMemoryLeakTest.kt` | 3 | `ComposeViewContext` doesn't leak views | n/a: `ComposeViewContext` (sharing a composition context between Views) has no analogue; compose-rs's leak checks are its allocation tests |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/FrameRateTest.kt` | 8 | `Modifier.preferredFrameRate` votes reach `View.setRequestedFrameRate` | `feature:platform-window-modifiers` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/KeepScreenOnModifierTest.kt` | 6 | `Modifier.keepScreenOn` sets and clears the window flag as nodes come and go | `feature:platform-window-modifiers` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/MemoryLeakTest.kt` | 7 | Disposed compositions and owner views are garbage collected | n/a: JVM garbage-collection checks; compose-rs checks the same with its counting allocator (`crates/compose-ui/tests/allocations.rs`, `ui/ui.rs` `dropping_the_host_frees_every_handler_and_state`) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/RecyclerViewIntegrationTest.kt` | 1 | Recycled `ComposeView`s in a `RecyclerView` are remeasured and redrawn | `feature:host-view-interop` | Analogue: a `ComposeRsView` recycled by a `RecyclerView`; needs an instrumented Android test harness (none exists). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/SensitiveContentModifierTest.kt` | 9 | `Modifier.sensitiveContent` sets `View.setContentSensitivity` | `feature:platform-window-modifiers` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ViewIntegrationTest.kt` | 5 | Drawing and invalidation of detached layout nodes inside `AndroidComposeView`, drawing outside a zero-sized view, view transitions | n/a: `AndroidComposeView`'s View drawing internals; `ComposeRsView` draws through its replayer, tested by `and/retained_groups.rs` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/adaptive/MediaQueryTest.kt` | 19 | `mediaQuery`/`derivedMediaQuery` recompose only when the result changes | `feature:media-query` | Experimental (`ExperimentalMediaQueryApi`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/owners/LifecycleOwnerInAppCompatActivityTest.kt` | 2 | `LocalLifecycleOwner` in an `AppCompatActivity` | n/a: no `LifecycleOwner`/lifecycles in compose-rs (docs/guide/differences.md), and `LocalLifecycleOwner` is deprecated in ui |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/owners/LifecycleOwnerInComponentActivityTest.kt` | 2 | `LocalLifecycleOwner` in a `ComponentActivity` | n/a: no lifecycles in compose-rs; deprecated in ui |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/owners/LifecycleOwnerInFragmentTest.kt` | 2 | `LocalLifecycleOwner` in a `Fragment` | n/a: no lifecycles in compose-rs; deprecated in ui |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/owners/SavedStateRegistryOwnerInAppCompatActivityTest.kt` | 2 | `SavedStateRegistryOwner` available | n/a: AndroidX saved-state owners; compose-rs saves through `UiHost::save_state` (D223) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/owners/SavedStateRegistryOwnerInComponentActivityTest.kt` | 2 | `SavedStateRegistryOwner` available | n/a: AndroidX saved-state owners (D223) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/owners/SavedStateRegistryOwnerInFragmentTest.kt` | 1 | `SavedStateRegistryOwner` available | n/a: AndroidX saved-state owners (D223) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/ActivityRetainTest.kt` | 6 | `retain` survives activity recreation | n/a: runtime's `retain` API (runtime area); the host side is Android activity recreation |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidClipboardIntegrationTest.kt` | 1 | `setText` and `getClipEntry` see each other | `feature:clipboard-api` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidClipboardTest.kt` | 29 | `AnnotatedString` span styles encoded into and out of the clipboard | `feature:clipboard-api` | compose-rs's clipboard is plain text (D281). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidComposeViewScreenCoordinatesTest.kt` | 9 | Positions on screen, `screenToLocal`, transforms to screen | `feature:host-view-interop` | `LayoutCoordinates` itself is the layout area. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidComposeViewTest.kt` | 2 | Frame rate votes reach `setRequestedFrameRate` only when needed | `feature:platform-window-modifiers` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidComposeViewsInRecyclerViewTest.kt` | 12 | `ComposeView`s in `RecyclerView` pools are disposed as the pool, adapter or visibility change | `feature:host-view-interop` | Composition strategies for a `ComposeRsView` in a pool. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidCompositionLocalTest.kt` | 9 | Android composition locals in popups and dialogs: configuration equality, `LocalWindow`, `LocalResources`, locales | `partial` | Popups share the composition's locals (overlays, D130) and `LOCAL_CONFIGURATION` follows the host (`fnd/platform.rs` `dark_theme_and_the_configuration_follow_the_host`); `LocalLocale`/`LocaleList` need platform-locals; `LocalWindow`/`LocalResources`/`LocalContext` are n/a (D282). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidUiDispatcherTest.kt` | 3 | The Choreographer-driven `AndroidUiDispatcher` | n/a: compose-rs's executor is driven by its host (runtime area) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidViewCompatTest.kt` | 27 | `AndroidView` measure specs, layout and drawing | n/a: Android `AndroidView` (Views inside composable content) has no analogue: embedding goes one way (D226, docs/guide/differences.md) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidViewsHandlerDelayedCreationTest.kt` | 1 | The `AndroidView` holder's late creation | n/a: Android `AndroidView` (Views inside composable content) has no analogue: embedding goes one way (D226, docs/guide/differences.md) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidViewsHandlerLookupTest.kt` | 3 | Semantics ids to `AndroidView`s | n/a: Android `AndroidView` (Views inside composable content) has no analogue: embedding goes one way (D226, docs/guide/differences.md) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AppCompatActivityLocaleTest.kt` | 3 | Per-app locales reach `LocalConfiguration` | `feature:platform-locals` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/ComposeViewContextResourceCacheTest.kt` | 1 | A shared `ComposeViewContext` resolves drawables per configuration | n/a: `ComposeViewContext` and Android resources have no analogue |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/ComposeViewOverlayTest.kt` | 4 | `ComposeView` moved to a `ViewOverlay`, created during transitions | n/a: Android view-overlay and fragment-transition mechanics of `AndroidComposeView` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/ComposeViewSavedStateSizeTest.kt` | 2 | A `ComposeView` with nothing saveable saves nothing; layers don't grow saved state | `port` | `UiHost::save_state` (D223; `fnd/platform.rs` `remember_saveable_comes_back_in_a_new_host`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/ComposeViewTest.kt` | 32 | `ComposeView` with `ComposeViewContext`: sharing, detaching, recomposing, transition groups | n/a: `ComposeView`/`ComposeViewContext` View-host plumbing; `ComposeRsView` has its own lifecycle |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/ConfigChangeActivityLocaleTest.kt` | 3 | Locales after a configuration change reach `LocalConfiguration` | `feature:platform-locals` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/DispatchGenericMotionEventTest.kt` | 7 | Generic scroll events: focus moves from unknown sources, scrolling returns whether it scrolled | `partial` | `ComposeRsView.onGenericMotionEvent` sends wheel scrolls; focus moves from unknown-source scrolls and the consumed return value need key-focus-navigation and host-view-interop. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/DisposableSaveableStateRegistryActivityTest.kt` | 1 | Duplicate view ids: the first saves and restores, the second resets | n/a: Android view-id keyed `Bundle` registries; compose-rs saves per host (D223) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/DisposableSaveableStateRegistryTest.kt` | 10 | The `Bundle`-backed registry: what can be saved, shared state arrays | `partial` | `SavedState` encodes booleans, numbers, strings and lists and rejects the rest (`fnd/platform.rs` `saved_state_bytes_round_trip_and_reject_garbage`); `Parcelable`/`Bundle` cases are n/a. Overlaps the runtime area. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/FragmentRetainTest.kt` | 4 | `retain` scoped to fragments | n/a: runtime's `retain` API (runtime area); fragments have no analogue |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/LocalResourcesTest.kt` | 1 | `LocalResources` invalidates on configuration change | n/a: no Android resources in compose-rs (docs/guide/differences.md) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/LocalSoftwareKeyboardControllerTest.kt` | 3 | `LocalSoftwareKeyboardController` hides and shows the keyboard, safely before a session | `covered` | `fnd/forms.rs` `the_keyboard_controller_asks_the_host_to_show_or_hide_the_keyboard`, `and/text_input.rs` `done_hides_the_keyboard_and_the_controller_shows_it`; fully, bar the delegation to the deprecated `TextInputService`. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/SoundEffectOnInteractionTest.kt` | 5 | `LocalSoundEffect` default, disabling, nesting, dead-object errors | `feature:sound-effects` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/TaskDispatchersCompositionLocalTest.kt` | 2 | `LocalTaskDispatchers` default and override | `feature:platform-locals` | compose-rs has `spawn_background` (D231) but no `TaskDispatchers` local. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/WindowInfoCompositionLocalTest.kt` | 12 | `LocalWindowInfo`: window focus with popups and dialogs, keyboard modifiers, container size | `feature:window-info` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/WindowInfoTest.kt` | 1 | `isWindowFocused` in a fragment | `feature:window-info` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/WindowRecomposerTest.kt` | 6 | The window recomposer's lifecycle | n/a: Android window recomposer; compose-rs's host drives recomposition |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/WrapperTest.kt` | 5 | `ComponentActivity.setContent` wrapper | n/a: `Activity.setContent` wrapper; `ComposeRsView` is created by the app |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/relocation/BringIntoViewModifierNodeTest.kt` | 16 | `BringIntoViewModifierNode` and `bringIntoView(bounds)`: propagation, nearest handler, rect translation, interruption | `partial` | Focus brings nodes into view through `ModifierNode::bring_into_view` (`fnd/lazy_api.rs` `a_reversed_list_brings_a_focused_item_into_view`, `fnd/review_rtl.rs`); the public node, suspending requests and interruption need bring-into-view-api. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/scrollcapture/ScrollCaptureDrawTest.kt` | 14 | Long-screenshot capture draws scrolled content | `feature:scroll-capture` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/scrollcapture/ScrollCaptureIntegrationTest.kt` | 11 | Which scrollables scroll capture finds | `feature:scroll-capture` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/scrollcapture/ScrollCaptureTest.kt` | 19 | Scroll capture target search: largest, deepest, clipped, hidden, transparent | `feature:scroll-capture` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/adaptive/MediaQueryIntegrationTest.kt` | 23 | `UiMediaScope` values from the device: window size, camera, viewing distance, pointer precision, keyboard kind, posture | `feature:media-query` | Experimental. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/hapticfeedback/HapticFeedbackTest.kt` | 2 | `HapticFeedbackType` constants map to Android's | `covered` | `and-src/services.rs` `haptic_codes_match_the_views` (the Kotlin side of the mapping is untested). |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/platform/ClipboardTest.kt` | 3 | `hasText` default derived from `getText` | `port` | `ClipboardManager::has_text` (`crates/compose-ui/src/clipboard.rs`). |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/platform/ComposeViewContextAccessibilityTest.kt` | 5 | Accessibility listeners registered on attach, re-registered on reattach, first frame with accessibility on | `partial` | `Accessibility.kt` `attach`/`detach` and the stale-on-change rule (D630; `and/accessibility_state.rs`); prewarming and shared contexts are n/a. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/platform/CrossWindowBlurTest.kt` | 7 | `WindowInfo.isCrossWindowBlurEnabled` registration and updates | `feature:window-info` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/platform/TaskDispatchersTest.kt` | 1 | Android task dispatchers | `feature:platform-locals` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/platform/WindowInfoTest.kt` | 2 | `WindowInfo.keyboardModifiers` shared and observable | `feature:window-info` |  |

### Popups and dialogs (window) (13 files, 163 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/DialogScreenshotTest.kt` | 2 | A dialog window's elevation shadow | n/a: platform dialog window decoration; compose-rs dialogs are overlays in the view (D130) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/DialogSecureFlagTest.kt` | 4 | `SecureFlagPolicy` on dialog windows | `feature:popup-dialog-properties` | Overlays have no window of their own (D130): the policy would apply to the host window. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/DialogTest.kt` | 47 | `Dialog`: content, dismissal by outside click, back and Escape, back handlers, locals, sizes, platform width, insets, window type, blur and scrim | `partial` | Dismissal, back/Escape, back handlers, modality, locals, centering are tested (`fnd/window.rs` `a_dialog_is_modal_takes_the_focus_and_dismisses_on_escape`, `fnd/review_overlay.rs`, `fnd/review_overlay2.rs`, `fnd/platform.rs`, `fnd/platform_review.rs`); `usePlatformDefaultWidth`, `decorFitsSystemWindows`, window title/type, blur and scrim alpha need popup-dialog-properties. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/DialogWithInsetsTest.kt` | 3 | Dialogs and window insets | `feature:popup-dialog-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/DialogWithNoStatusBarTest.kt` | 7 | Full-screen dialogs match the container size | `feature:popup-dialog-properties` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/PopupAlignmentTest.kt` | 17 | `Popup(alignment, offset)` positions, left to right and right to left | `port` | `AlignmentOffsetPositionProvider` exists (`fnd/review_overlay.rs` `a_popup_is_anchored_inside_its_parents_padding`). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/PopupDismissTest.kt` | 1 | A click outside dismisses a popup | `covered` | `fnd/review_overlay.rs` `a_popup_is_dismissed_on_the_press_outside_it`, `fnd/window.rs`; fully. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/PopupLayoutTest.kt` | 10 | Popup position updates with coordinates, state reads and detaching; clipping to the visible frame | `partial` | Popups are placed after each layout (`fnd/review_overlay.rs` `passes_a_popup_follows_its_anchor_when_it_scrolls`); `clippingEnabled` needs popup-dialog-properties. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/PopupSecureFlagTest.kt` | 5 | `SecureFlagPolicy` on popup windows | `feature:popup-dialog-properties` | As `DialogSecureFlagTest`. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/PopupTest.kt` | 36 | `Popup`: showing, size, dismissal by tap, back and Escape, locals, layout direction, nested popups, flags, window token | `partial` | Dismissal, back, nesting, locals tested (`fnd/review_overlay.rs`, `fnd/review_overlay2.rs`, `fnd/platform.rs` `only_a_focusable_popup_takes_back`, `fnd/window.rs`); window flags, blur, scrim, `LocalView`, lifecycle owners are n/a or popup-dialog-properties. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/PositionInWindowTest.kt` | 5 | `positionInWindow` with scrolled decor views and moving popups | n/a: Android window and decor-view offsets; `LayoutCoordinates` is the layout area |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/window/PopupPositionProviderTest.kt` | 18 | `AlignmentOffsetPositionProvider` for every alignment, both directions | `port` | `crates/compose-ui/src/window.rs` `AlignmentOffsetPositionProvider`. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/window/PopupWindowTokenTest.kt` | 8 | Which window token a popup window uses | n/a: Android window tokens; overlays have no window (D130) |  |

### View interop (20 files, 307 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/AndroidViewTest.kt` | 57 | `AndroidView`: creation, update, release, modifiers, accessibility delegate | n/a: Android `AndroidView` (Views inside composable content) has no analogue: embedding goes one way (D226, docs/guide/differences.md) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/ComposeViewTest.kt` | 25 | `ComposeView` in Views: composition strategies, paddings, bounds in window, `canScrollVertically`, `addView` refused | `feature:host-view-interop` | Analogue: `ComposeRsView` in a View tree. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/EditTextInteropTest.kt` | 1 | A hardware key reaches an `EditText` inside an `AndroidView` | n/a: Android `AndroidView` (Views inside composable content) has no analogue: embedding goes one way (D226, docs/guide/differences.md) |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusSearchBackwardInteropTest.kt` | 25 | Backward focus search across composables and Views in layouts | `feature:host-view-interop` | Cases with Views inside Compose (`AndroidView`) are n/a; those with a `ComposeRsView` among Views apply. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusSearchDownInteropTest.kt` | 26 | Down focus search across composables and Views | `feature:host-view-interop` | As above. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusSearchForwardInteropTest.kt` | 26 | Forward focus search across composables and Views | `feature:host-view-interop` | As above. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusSearchInteropTest.kt` | 2 | Focus search with a single composable or View | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusSearchLeftInteropTest.kt` | 25 | Left focus search across composables and Views | `feature:host-view-interop` | As above. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusSearchRightInteropTest.kt` | 25 | Right focus search across composables and Views | `feature:host-view-interop` | As above. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusSearchUpInteropTest.kt` | 26 | Up focus search across composables and Views | `feature:host-view-interop` | As above. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusWappingTest.kt` | 2 | Forward and backward focus wrap across Views and Compose | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/FocusWrapperTest.kt` | 2 | The host view's focus vs the focused composable | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/MixedFocusChangeTest.kt` | 3 | Focus leaving `RecyclerView`s and beams between Views and Compose | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/NestedScrollInteropConnectionTest.kt` | 20 | Compose scrollables inside a `NestedScrollingParent` View (`rememberNestedScrollInteropConnection`) | `feature:host-view-interop` | Needs nested-scroll-dispatcher. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/NestedScrollInteropThreeFoldTest.kt` | 7 | Compose-View-Compose and View-Compose-View nested scroll chains | `feature:host-view-interop` | The Views-inside-Compose half is n/a. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/NestedScrollInteropViewHolderTest.kt` | 8 | Nested scroll deltas and flings between a View holder and Compose | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/PoolingContainerComposeTest.kt` | 4 | Compositions disposed when released from a pooling container | `feature:host-view-interop` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/VelocityTrackingListParityTest.kt` | 8 | Compose lists fling as far as `RecyclerView` for the same gestures | `feature:velocity-tracker` | Also foundation's fling (foundation area). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/VelocityTrackingParityTest.kt` | 8 | Compose's tracked velocity matches the framework's | `feature:velocity-tracker` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/ViewParentFocusSearchTest.kt` | 7 | `ViewParent.focusSearch` into and between `ComposeView`s | `feature:host-view-interop` |  |

### Platform text input (overlaps the text area) (16 files, 239 methods)

| Path | `@Test` | Covers | Status | Detail |
|---|---|---|---|---|
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/CursorAnchorInfoBuilderTest.kt` | 25 | `CursorAnchorInfo` for the input method: selection, composition, insertion marker, character and line bounds | `feature:platform-text-input` | Overlaps the text area. compose-rs's Android IME has no cursor anchor info. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/EditorInfoTest.kt` | 48 | `EditorInfo` from `ImeOptions`: input types, actions, capitalization, auto-correct, initial selection, locales | `partial` | Input type, action, capitalization, auto-correct and password map as Compose's (`and-src/input_method.rs` tests, `and/text_input.rs`); missing: initial selection/surrounding text, hint locales, `PlatformImeOptions`, `showKeyboardOnFocus` (platform-text-input, text area). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt` | 31 | Compose's `InputConnection`: surrounding text, selection, composition, batch edits, key events, cursor updates | `partial` | `ComposeRsInputConnection` (`ComposeRsView.kt`) commits text, sets composing text, deletes surrounding text and performs editor actions; it exposes no text (no `getTextBeforeCursor`, selection, batch edits) (platform-text-input). No Kotlin unit tests exist; port as Rust tests of the bridge or add a JVM test harness. |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidCursorAnchorInfoTest.kt` | 5 | `requestCursorUpdates` modes | `feature:platform-text-input` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidEmojiTest.kt` | 1 | EmojiCompat's `EditorInfo` extras | n/a: depends on AndroidX EmojiCompat, which compose-rs's Android text stack doesn't use |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidOnStateUpdateTest.kt` | 8 | `restartInput` vs `updateSelection` as the field's state changes | `partial` | compose-rs restarts the input method on focus changes and resets (`UiHost::take_input_method_reset`, `and/text_input.rs`); no `updateSelection` (platform-text-input). |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/AndroidPlatformTextInputSessionTest.kt` | 7 | The keyboard shown and hidden as nested text input sessions start, cancel and interrupt | `feature:platform-text-input` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/InterceptPlatformTextInputTest.kt` | 13 | `InterceptPlatformTextInput` wraps requests, `EditorInfo` and connections | `feature:platform-text-input` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/NullableInputConnectionWrapperTest.kt` | 9 | The internal connection wrapper's delegation and disposal | n/a: internal implementation detail |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/PlatformTextInputMethodTestOverrideTest.kt` | 2 | A test override handles the input method | `feature:platform-text-input` |  |
| `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/PlatformTextInputViewIntegrationTest.kt` | 21 | `PlatformTextInputModifierNode` sessions and the view's `onCreateInputConnection` | `feature:platform-text-input` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/SessionMutexTest.kt` | 16 | The coroutine mutex behind text input sessions | n/a: internal implementation detail (`SessionMutex`) |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionInactiveTest.kt` | 19 | A closed `InputConnection` ignores every call | `partial` | As `RecordingInputConnectionTest`. |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionUpdateTextFieldValueTest.kt` | 3 | Extracted-text updates to the input method | `feature:platform-text-input` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/text/TextActionModeCallbackTest.kt` | 6 | The text toolbar's `ActionMode` menu items | `feature:text-toolbar` |  |
| `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/text/input/TextInputServiceAndroidCommandDebouncingTest.kt` | 25 | Show/hide/start/stop keyboard commands debounced per frame | `partial` | The host coalesces the keyboard requests of a frame (`UiHost::take_software_keyboard_request`, last request wins; `and/text_input.rs`); the start/stop-input pairs follow `textInput` changes in `ComposeRsView.kt`, untested. |

**Helpers without `@Test` methods** (fixtures, not ported on their own): `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/autofill/FakeViewStructure.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTestUtils.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/gesture/Utils.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/indirect/IndirectPointerInputModifier.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/indirect/util/FocusableAndIndirectPointerInputModifier.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HandwritingTestUtils.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/TestUtils.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/AndroidOwnerExtraAssertionsRule.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/scrollcapture/ScrollCaptureTester.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/scrollcapture/ScrollExpecter.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/semantics/SemanticsTestUtils.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/text/input/TestInputMethodRequest.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/InvalidatedTextView.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/viewinterop/NestedScrollInteropTestHelper.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/ActivityWithInsets.kt`, `compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/window/PopupTestUtils.kt`, `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/autofill/FakeViewStructure.kt`, `compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/TestUtils.kt`.
## 3. The public API surface compose-rs lacks

From `compose/ui/ui/api/current.txt` (deprecated entries left out; `restricted_current.txt` only to tell what is internal). Each group names the feature of section 4 that brings it. "Partly" says what compose-rs has.

### `androidx.compose.ui.focus`

- **focus-hierarchy:** `FocusState` (`isFocused`, `hasFocus`, `isCaptured`; compose-rs's `on_focus_changed` passes `hasFocus` only, D85), `Modifier.onFocusChanged(FocusState)` (partly), `Modifier.onFocusEvent`, `FocusEventModifierNode`, `Modifier.focusTarget()` (compose-rs has `focusable`/`focusable_with` and the `FocusTargetModifierNode` trait, D76), `FocusTargetModifierNode(focusability, onFocusChange)` factory with `focusState`, `focusability`, `requestFocus(direction)`, `getFocusedRect`, `Focusability` (`Always`, `Never`, `SystemDefined`).
- **focus-properties:** `Modifier.focusProperties`, `FocusProperties` (`canFocus`, `next`, `previous`, `up`, `down`, `left`, `right`, `start`, `end`, `onEnter`, `onExit`, `focusRect`, `UnsetFocusRect`; `enter`/`exit` getters are `@ExperimentalComposeUiApi`), `FocusEnterExitScope` (`requestedFocusDirection`, `cancelFocusChange`), `FocusPropertiesModifierNode`, `invalidateFocusProperties`, `FocusRequester.Default`, `FocusRequester.Cancel`, `FocusDirection.Enter`, `FocusDirection.Exit` (compose-rs's `FocusDirection` has the other six).
- **focus-requester-api:** `FocusRequester.captureFocus`, `freeFocus`, `saveFocusedChild`, `restoreFocusedChild`, `requestFocus(focusDirection): Boolean` (compose-rs's `request_focus()` returns whether the requester is attached, and acts later, D148), `FocusRequester.createRefs()`, `FocusRequesterModifierNode` and its `requestFocus`/`captureFocus`/`freeFocus`/`saveFocusedChild`/`restoreFocusedChild`, `Modifier.focusRestorer(fallback)`, `requestFocusForChildInLocalBounds`, `requestFocusForChildInRootBounds`, `FocusManager.moveFocus`'s result (compose-rs returns whether a tree was asked, D230), `clearFocus(force)` with captured focus (partly: `force` is ignored).

### `androidx.compose.ui.input.key`

- **key-input-api:** `Modifier.onPreviewKeyEvent`, `KeyInputModifierNode.onPreKeyEvent` (compose-rs's `KeyInputModifierNode` has `on_key_event` and text/IME hooks), `Key`'s full set of about 300 keys with `keyCode`, `Key(nativeKeyCode)` and `nativeKeyCode` (compose-rs has 15 named keys plus `Character(char)` and `Other(u32)`), `KeyEventType.Unknown`, `KeyEvent.utf16CodePoint`, `isAltPressed`/`isCtrlPressed`/`isMetaPressed`/`isShiftPressed` (compose-rs: `KeyModifiers` fields), `KeyEvent.nativeKeyEvent`, `SoftKeyboardInterceptionModifierNode`, `Modifier.onInterceptKeyBeforeSoftKeyboard`, `Modifier.onPreInterceptKeyBeforeSoftKeyboard`.

### `androidx.compose.ui.input.rotary`, `androidx.compose.ui.input.indirect`, `androidx.compose.ui.input`

- **rotary-input:** `Modifier.onRotaryScrollEvent`, `Modifier.onPreRotaryScrollEvent`, `RotaryScrollEvent` (`verticalScrollPixels`, `horizontalScrollPixels`, `uptimeMillis`, `inputDeviceId`), `RotaryInputModifierNode`.
- **indirect-pointer-input:** `IndirectPointerEvent` (`changes`, `type`, `primaryDirectionalMotionAxis`, `nativeEvent`), `IndirectPointerEventType`, `IndirectPointerEventPrimaryDirectionalMotionAxis`, `IndirectPointerInputChange`, `IndirectPointerInputModifierNode`, the test-only `IndirectPointerEvent(...)` factory.
- **input-mode:** `InputMode` (`Touch`, `Keyboard`), `InputModeManager` (`inputMode`, `requestInputMode`), `LocalInputModeManager`.

### `androidx.compose.ui.input.pointer` and `.util`

- **pointer-event-model:** `PointerEvent` as a list of `changes` with `type`, `buttons` (`PointerButtons`, `isPrimaryPressed`…, `indexOfFirstPressed`…), `keyboardModifiers` (`PointerKeyboardModifiers`, `isCtrlPressed`…), `classification`, `motionEvent` (compose-rs's `PointerEvent` is one pointer's change); `PointerInputChange`'s `pressed`/`previousPressed`, `previousUptimeMillis`, `pressure`, `type`, `historical` (`HistoricalChange`), `scaleFactor`, `panOffset`, `copy` (compose-rs has `kind`, `id`, `position`, `previous_position`, `pressed`, `scroll_delta`, `uptime_millis`, consume); the helpers `changedToDown`, `changedToDownIgnoreConsumed`, `changedToUp`, `changedToUpIgnoreConsumed`, `positionChange` (present), `positionChangeIgnoreConsumed`, `positionChanged`, `positionChangedIgnoreConsumed`, `isOutOfBounds(size, extendedTouchPadding)` (partly); `PointerType.Eraser`/`Unknown`; `PointerEventType.Unknown`, `PanStart`/`PanMove`/`PanEnd`, `ScaleStart`/`ScaleChange`/`ScaleEnd`; `AwaitPointerEventScope` (`currentEvent`, `size`, `viewConfiguration`, `extendedTouchPadding`, `withTimeout` throwing `PointerEventTimeoutCancellationException`, `withTimeoutOrNull` (present as `with_timeout`)); `PointerInputScope.awaitPointerEventScope`, `interceptOutOfBoundsChildEvents`; `pointerInput(key1, key2)` and `pointerInput(keys)`; `PointerInputEventHandler`; `SuspendingPointerInputModifierNode` (`resetPointerInputHandler`, `pointerInputEventHandler`); `PointerInputFilter`/`PointerInputModifier` (legacy, not deprecated).
- **hit-test-extensions:** `PointerInputModifierNode.sharePointerInputWithSiblings`, `interceptOutOfBoundsChildEvents`, `touchBoundsExpansion`; `TouchBoundsExpansion`, `DpTouchBoundsExpansion` (`Absolute`, `roundToTouchBoundsExpansion`); hit testing through graphics-layer transforms and minimum touch targets (behaviour, not API).
- **pointer-icon-api:** `Modifier.pointerHoverIcon(icon, overrideDescendants)` (compose-rs has `pointer_hover_icon(icon)`), `Modifier.stylusHoverIcon`, `PointerIcon(android.view.PointerIcon)`, `PointerIcon(type)`.
- **velocity-tracker:** `VelocityTracker` (`addPosition`, `calculateVelocity(maximumVelocity)`, `resetTracking`, `addPointerInputChange`), `VelocityTracker1D(isDataDifferential)` (compose-rs has a `pub(crate)` linear tracker in foundation's `fling.rs`).

### `androidx.compose.ui.input.nestedscroll`

- **nested-scroll-dispatcher:** `NestedScrollDispatcher` (`dispatchPreScroll`, `dispatchPostScroll`, `dispatchPreFling`, `dispatchPostFling`, `coroutineScope`), `Modifier.nestedScroll(connection, dispatcher)` (compose-rs takes the connection only), `nestedScrollModifierNode(connection, dispatcher)`, suspending `onPreFling`/`onPostFling` with `Velocity` (compose-rs's are synchronous with `Offset`). `NestedScrollConnection` and `NestedScrollSource.UserInput`/`SideEffect` are present (D188).

### `androidx.compose.ui.semantics` and `androidx.compose.ui.node` (semantics parts)

- **semantics-properties:** `SemanticsPropertyKey<T>` (name, merge policy; `accessibilityExtraKey` is host-experimental), `SemanticsPropertyReceiver`, `SemanticsConfiguration` as a typed map (`get`, `getOrNull`, `contains`, iteration; `isMergingSemanticsOfDescendants`, `isClearingSemantics`) — compose-rs's `SemanticsConfiguration` is a fixed struct; `AccessibilityAction(label, action)`, `CustomAccessibilityAction`, `SemanticsActions.CustomActions`. Properties compose-rs lacks: `heading`, `paneTitle`, `liveRegion` (`LiveRegionMode`), `traversalIndex`, `isTraversalGroup`, `collectionInfo`/`collectionItemInfo`, `hideFromAccessibility`, `hintText`, `maxTextLength`, `imeAction`, `inputText`, `textSelectionRange`, `textCompositionRange`, `textSubstitution`, `isShowingTextSubstitution`, `inputTextSuggestionState`, `isEditable` (partly: `editable`), `isSensitiveData`, `shape`, `backgroundColor`, `popup()`/`IsPopup`, `indexForKey`, `textEntryKey`, `LinkTestMarker`, `contentType`/`contentDataType`/`fillableData` (autofill), `accessibilityClassName`, `testTagsAsResourceId`, `password(isPasswordObfuscated)` (partly), `Role.Carousel`, `Role.ValuePicker`, `ProgressBarRangeInfo.Indeterminate`, `ScrollAxisRange`'s lambdas (compose-rs stores values). Actions compose-rs lacks: `expand`, `collapse`, `dismiss`, `pageUp`/`pageDown`/`pageLeft`/`pageRight`, `scrollToIndex`, `scrollByOffset`, `setText`, `setSelection`, `insertTextAtCursor`, `copyText`, `cutText`, `pasteText`, `onImeAction`, `getTextLayoutResult`, `setTextSubstitution`/`showTextSubstitution`/`clearTextSubstitution`, `getScrollViewportLength`, `onFillData`, labels on every action (compose-rs has `OnClick`, `OnLongClick`, `RequestFocus`, `SetProgress`, `ScrollBy` as an enum to perform; the configuration only flags `clickable`/`long_clickable`).
- **semantics-tree-api:** `SemanticsOwner` (`rootSemanticsNode`, `unmergedRootSemanticsNode`; host-experimental `get(id)`, `getLayoutInfo`, `hitTest`, `forEachUncoveredSemanticsNode`), `SemanticsNode`'s `id`, `parent`, `isRoot`, `config`, `children` sorted by placement and z-index, `boundsInRoot`, `boundsInWindow`, `positionInRoot`, `positionInWindow`, `positionOnScreen`, `size`, `touchBoundsInRoot`, `layoutInfo`, `root` (compose-rs's `platform::SemanticsNode` is a snapshot with `id`, `bounds`, `rtl`, `config`, `children` in tree order); host-experimental `mergedConfig`, `unmergedConfig`, `getMergedOrNull`, `forEachMergedProperty`, `collectAccessibilityChildren`, `isHidden`, `isImportantForAccessibility`, `SemanticsPropertyProvider`, `LayoutInfo.semanticsConfiguration`; `SemanticsModifierNode.shouldMergeDescendantSemantics`, `shouldClearDescendantSemantics`, `invalidateSemantics()`; `RootForTest.semanticsOwner`, `sendKeyEvent`, `sendIndirectPointerEvent`, `updateSemanticsForTest`.

### `androidx.compose.ui.autofill`, `androidx.compose.ui.contentcapture`, `androidx.compose.ui.draganddrop`

- **autofill:** `ContentType` (all 36 constants, `plus`, `ContentType(hint)`), `Modifier.contentType`, `ContentDataType` (`Text`, `List`, `Date`, `Toggle`, `None`, `ContentDataType(int)`, `dataType`), `FillableData` (typed values, `createFromText`/`Boolean`/`ListIndex`/`DateMillis`/`AutofillValue`, `toAutofillValue`), `AutofillManager` (`commit`, `cancel`), `LocalAutofillManager`, `SemanticsProperties.ContentType`/`ContentDataType`/`FillableData`, `SemanticsActions.OnFillData`, `CredentialRequestData`.
- **content-capture:** `ContentCaptureManager.isEnabled` (`@ExperimentalComposeUiApi`).
- **drag-and-drop:** `DragAndDropSourceModifierNode(onStartTransfer)` (`requestDragAndDropTransfer`, `isRequestDragAndDropTransferRequired`), `DragAndDropStartTransferScope.startDragAndDropTransfer`, `DragAndDropTargetModifierNode(shouldStartDragAndDrop, target)`, `DragAndDropTarget` (`onStarted`, `onEntered`, `onMoved`, `onExited`, `onChanged`, `onDrop`, `onEnded`), `DragAndDropEvent` (`mimeTypes`, `toAndroidDragEvent`), `DragAndDropTransferData` (`clipData`, `localState`, `flags`).

### `androidx.compose.ui.platform`, `androidx.compose.ui.relocation`, `androidx.compose.ui` (host-facing parts)

- **view-configuration:** `ViewConfiguration` (`longPressTimeoutMillis`, `doubleTapTimeoutMillis`, `doubleTapMinTimeMillis`, `touchSlop`, `handwritingSlop`, `minimumTouchTargetSize`, `maximumFlingVelocity`, `handwritingGestureLineMargin`), `LocalViewConfiguration`, `AndroidViewConfiguration`. compose-rs has them all (D907: compose-ui's `ViewConfiguration`, Android's values at the density, the host's through `UiHost::set_view_configuration` and `LOCAL_VIEW_CONFIGURATION`, read by the gesture detectors and scrolling); left: `pointer_input` blocks restarting when it changes, and a subtree's own (`WithViewConfiguration`).
- **window-info:** `WindowInfo` (`isWindowFocused`, `keyboardModifiers`, `containerSize`, `containerDpSize`), `LocalWindowInfo`, `WindowInfo.isCrossWindowBlurEnabled` (Android).
- **sound-effects:** `SoundEffect`, `LocalSoundEffect`, the sound-on-interaction extension.
- **platform-locals:** `LocalAccessibilityManager` (`AccessibilityManager.calculateRecommendedTimeoutMillis`), `LocalLocale`, `LocalLocaleList`, `LocalCursorBlinkEnabled`, `LocalScrollCaptureInProgress`, `LocalTaskDispatchers` (`TaskDispatchers`).
- **text-toolbar:** `TextToolbar` (`showMenu`, `hide`, `status`), `TextToolbarStatus`, `LocalTextToolbar`.
- **clipboard-api:** `Clipboard` (`getClipEntry`, `setClipEntry`, `nativeClipboard`), `ClipEntry`, `ClipMetadata`, `LocalClipboard`, `toClipEntry`/`toClipMetadata`, `ClipEntry.firstUriOrNull` (experimental). Compose deprecates `ClipboardManager`/`LocalClipboardManager`, the API compose-rs's `ClipboardManager`/`LOCAL_CLIPBOARD_MANAGER` follow (D281).
- **platform-text-input:** `PlatformTextInputModifierNode`, `establishTextInputSession`, `PlatformTextInputSession`, `PlatformTextInputSessionScope`, `PlatformTextInputMethodRequest`, `PlatformTextInputInterceptor`, `InterceptPlatformTextInput` (compose-rs's text fields talk to the host through `KeyInputModifierNode` and `ImeOptions`, D234).
- **bring-into-view-api:** `BringIntoViewModifierNode` (`bringChildIntoView`), `DelegatableNode.bringIntoView(bounds)` (suspending) (compose-rs: `ModifierNode::bring_into_view`, synchronous, driven by focus).
- **scroll-capture:** Android scroll capture (no public type beyond `LocalScrollCaptureInProgress`; behaviour of `AndroidComposeView`).
- **platform-window-modifiers:** `Modifier.keepScreenOn()`, `Modifier.sensitiveContent(isContentSensitive)`, `Modifier.preferredFrameRate(frameRate)`/`preferredFrameRate(FrameRateCategory)`, `FrameRateCategory`.
- **host-view-interop:** `rememberNestedScrollInteropConnection` (Compose inside a `NestedScrollingParent`), the host view's focus search, key consumption, `canScroll*`, `requestDisallowInterceptTouchEvent` behaviour (no further public types: the View is compose-rs's `ComposeRsView`).
- **media-query** (`@ExperimentalMediaQueryApi`): `UiMediaScope` (`windowWidth`/`windowHeight`, `windowPosture`, `pointerPrecision`, `keyboardKind`, `viewingDistance`, `hasCamera`, `hasMicrophone`, `FoldState`, `FoldOrientation`, `WindowFold`), `mediaQuery`, `derivedMediaQuery`.

### `androidx.compose.ui.window`

- **popup-dialog-properties:** `PopupProperties`' `securePolicy`, `excludeFromSystemGesture`, `clippingEnabled`, `usePlatformDefaultWidth`, `windowType`, `windowToken`, `blurBehindRadius`, `scrimAlpha`, the `flags` overload; `DialogProperties`' `securePolicy`, `usePlatformDefaultWidth`, `decorFitsSystemWindows`, `windowTitle`, `windowType`, `windowToken`, `windowShape`, `backgroundBlurRadius`, `blurBehindRadius`, `scrimAlpha`; `SecureFlagPolicy`; `DialogWindowProvider`. compose-rs has `focusable`, `dismiss_on_back_press`, `dismiss_on_click_outside` (popups) and the two dismiss flags (dialogs); most of the rest only means something for a platform window, which compose-rs's overlays aren't (D130).

### Out of scope by design (no feature planned)

Kept out because compose-rs content isn't an Android View tree (D226, D282, docs/guide/differences.md): `AndroidView`/`AndroidViewBinding`-style hosting of Views inside Rust content, `Modifier.pointerInteropFilter`, `Modifier.motionEventSpy`, `RequestDisallowInterceptTouchEvent`, `AbstractComposeView`, `ComposeView`, `ComposeViewContext`, `ViewCompositionStrategy`, `ViewRootForTest`, `ViewRootForInspector`, `LocalView`, `LocalContext`, `LocalResources`, `LocalWindow`, the Android `LocalConfiguration` type (compose-rs has its own `Configuration`), `AndroidUiDispatcher`, `AndroidUiFrameClock`, `createLifecycleAwareWindowRecomposer`, `WindowRecomposerPolicy` (internal), `ComposeUiHost`/`ComposeUiSharedResources` (`@ExperimentalComposeUiHostApi`: compose-rs's `UiHost` is its host API), `ComposeUiFlags`/`AndroidComposeUiFlags` (Compose's own feature flags), `InspectableValue`/`InspectorInfo` (tooling), and the deprecated autofill types.

## 4. Missing features

Sizes: S under a day, M a few days, L one to two weeks, XL several weeks. "Tests" lists the section 2 rows the feature unblocks (as their status or as what a `partial` row misses).

### focus-hierarchy (L)
- **Compose API:** `FocusState`, `onFocusChanged(FocusState)`, `onFocusEvent`, `FocusEventModifierNode`, `Modifier.focusTarget()`, `FocusTargetModifierNode` factory (`Focusability`, `onFocusChange(previous, current)`, `focusState`, `requestFocus(direction)`, `getFocusedRect`).
- **What:** Compose's focus tree: nested focus targets with the states Active, ActiveParent, Captured, Inactive; a target that can't focus (deactivated) stays in the tree and passes requests to its children; focus events reach every event node between targets. compose-rs keeps one focused node and reports `hasFocus` only (D76, D85).
- **Tests:** CombinedFocusModifierNodeTest, DeactivatedFocusNodeTest, FocusChangedTest, FocusEventCountTest, FocusTargetModifierNodeTest, RequestFocusTest; and the missing parts of FocusAggregationTest, FocusChangedCountTest, FocusRequesterTest, FocusTargetAttachDetachTest.
- **Depends on:** input-mode (for `Focusability.SystemDefined`). Foundation's `focusable`/`focusGroup` would move onto it (foundation area).
- **Experimental:** no.

### focus-properties (L)
- **Compose API:** `Modifier.focusProperties`, `FocusProperties`, `FocusEnterExitScope`, `FocusPropertiesModifierNode`, `invalidateFocusProperties`, `FocusRequester.Default`/`Cancel`, `FocusDirection.Enter`/`Exit`.
- **What:** per-target `canFocus`, custom destinations per direction (or cancel), `onEnter`/`onExit` that can cancel or redirect a change, a custom focus rect.
- **Tests:** CancelFocusMoveTest, ClearFocusExitTest, CustomFocusRectTest, CustomFocusTraversalTest, RequestFocusEnterExitTest, RequestFocusEnterTest, RequestFocusExitTest, TwoDimensionalFocusTraversalImplicitEnterTest, TwoDimensionalFocusTraversalImplicitExitTest; parts of FocusRequesterTest, FocusTransactionsTest, OneDimensionalFocusSearch{Next,Previous}Test, FocusSearchNonPlacedItemsTest.
- **Depends on:** focus-hierarchy.
- **Experimental:** the `enter`/`exit` properties are (their setters deprecated); the rest is stable.

### focus-requester-api (M)
- **Compose API:** `captureFocus`, `freeFocus`, `saveFocusedChild`, `restoreFocusedChild`, `requestFocus(direction): Boolean`, `createRefs`, `FocusRequesterModifierNode` functions, `Modifier.focusRestorer`, `requestFocusForChildInLocalBounds`/`RootBounds`, `clearFocus(force)` honouring capture, `moveFocus`'s result.
- **What:** capture and its effect on clear/request, saved children, restoring focus into a group (lazy lists), and synchronous results. The results need a decision on D148/D230 (requests apply when the tree is free): apply them at once when called outside dispatch, or return a pending result.
- **Tests:** CaptureFocusTest, FreeFocusTest, FocusRestorerTest, FocusRestorerConfigurationChangeTest, RequestChildFocusTest, RestoreFocusTest, ReusedFocusRequesterCaptureFocusTest, ReusedFocusRequesterFreeFocusTest; parts of FocusManagerCompositionLocalTest.
- **Depends on:** focus-hierarchy; saved state for the restorer across recreation (present, D223).
- **Experimental:** no (the deprecated `focusRestorer(onRestoreFailed)` overload was experimental).

### focus-search-tree (M)
- **Compose API:** behaviour of `moveFocus` (`Next`, `Previous`, `Up`…, `Enter`, `Exit`) over the focus tree.
- **What:** Compose's one-dimensional order (a focusable parent before its children, children of deactivated parents) and two-dimensional search among the focused node's siblings first, then out of its parent; `Enter` picks the top-left child, `Exit` the parent. compose-rs searches all targets flat, with lazy lists as the only groups (D483).
- **Tests:** TwoDimensionalFocusTraversalEnterTest, TwoDimensionalFocusTraversalExitTest, TwoDimensionalFocusTraversalTest; parts of OneDimensionalFocusSearch{Next,Previous}Test.
- **Depends on:** focus-hierarchy, focus-properties.
- **Experimental:** no.

### key-input-api (M)
- **Compose API:** `onPreviewKeyEvent`, `KeyInputModifierNode.onPreKeyEvent`, the full `Key` set with `keyCode`/`nativeKeyCode`, `KeyEventType.Unknown`, `utf16CodePoint`, `nativeKeyEvent`, soft-keyboard interception modifiers and node.
- **What:** a preview pass from the root down to the focused node before the bubbling pass; key identity separate from the typed character (compose-rs's `Key::Character('A')` with Shift is not `Key.A`); key-ups without their down dropped; both hosts mapping every key.
- **Tests:** HardwareKeyInputTest; parts of ProcessKeyInputTest, KeyTest, FocusAwareEventPropagationTest.
- **Depends on:** none. Foundation's text field and shortcuts would move to `Key` constants (text area).
- **Experimental:** no.

### key-focus-navigation (S)
- **Compose API:** behaviour (Compose's `getFocusDirection`): arrows and D-pad to `Up`/`Down`/`Left`/`Right`, Tab/Shift+Tab, D-pad center/Enter/NumPadEnter to `Enter`, Back/Escape to `Exit`, PageUp/PageDown.
- **What:** unconsumed navigation keys move focus on Android as Compose does (compose-rs moves only on Tab, D230, following desktop Compose); desktop keeps Tab only.
- **Tests:** KeyEventToFocusDirectionTest; FocusSoundTest; part of DispatchGenericMotionEventTest.
- **Depends on:** focus-properties (`Enter`/`Exit`), input-mode.
- **Experimental:** no.

### input-mode (S)
- **Compose API:** `InputMode`, `InputModeManager`, `LocalInputModeManager`.
- **What:** touch vs keyboard mode from the host (Android's touch mode; desktop: keyboard), switching on input, gating `Focusability.SystemDefined` (clickables focusable only in keyboard mode on Android) and initial focus.
- **Tests:** InputModeTest, AccessibilityTouchModeTest, InitialFocusTest; parts of FocusTargetModifierNodeTest.
- **Depends on:** none.
- **Experimental:** no.

### pointer-event-model (L)
- **Compose API:** listed in section 3 (multi-change `PointerEvent`, full `PointerInputChange`, helpers, buttons, keyboard modifiers, `HistoricalChange`, `AwaitPointerEventScope`, multi-key `pointerInput`, `SuspendingPointerInputModifierNode`, pan/scale event types).
- **What:** one event per platform batch carrying every pointer's change, as Compose; the block sees `PointerEvent`s, not single changes (compose-rs: D75, one change per event); a cancelled gesture delivers synthetic up events to the running block instead of restarting it; desktop sends every mouse button and modifiers; Android passes historical samples as `historical`, stylus hover, classification and trackpad pan/scale.
- **Tests:** PointerInputTest (host), StylusEventTest, SuspendingPointerInputFilterCoroutineJobTest; parts of SuspendingPointerInputFilterTest, PointerInputEventProcessorTest, HitPathTrackerTest, AndroidPointerInputTest, MotionEventAdapterTest.
- **Depends on:** none in ui; foundation's detectors and `draggable`/`scrollable` must move to it (foundation area), and multi-touch detectors (`detectTransformGestures`) need it.
- **Experimental:** one `PointerInputChange.copy` overload.

### hit-test-extensions (M)
- **Compose API:** `sharePointerInputWithSiblings`, `interceptOutOfBoundsChildEvents`, `touchBoundsExpansion`, `TouchBoundsExpansion`, `DpTouchBoundsExpansion`; behaviour: minimum touch target expansion, layer-transformed hit tests, generic-shape clips.
- **What:** a pointer node smaller than `minimumTouchTargetSize` (48 dp) is hit around it when nothing is hit directly; stylus touch bounds expansion; siblings sharing events; hit testing through `graphics_layer` transforms (revising D121).
- **Tests:** HitTestTouchBoundsExpansionTest, TouchBoundsExpansionTest, SharePointerInputWithSiblingTest, LayerTouchTransformTest, InputInLayerTest; parts of AndroidPointerInputTest, RestrictedSizeTest, ClipPointerInputTest.
- **Depends on:** view-configuration (`minimumTouchTargetSize`); `GenericShape` outlines (drawing area, D529) for generic clips.
- **Experimental:** no.

### pointer-icon-api (S)
- **Compose API:** `pointerHoverIcon(icon, overrideDescendants)`, `stylusHoverIcon`, `PointerIcon(platform)`.
- **What:** a parent's icon overriding its descendants'; stylus hover icons; Android `PointerIcon`s.
- **Tests:** StylusHoverIconTest; part of PointerIconTest.
- **Depends on:** pointer-event-model (stylus hover), hit-test-extensions (`touchBoundsExpansion`).
- **Experimental:** no.

### velocity-tracker (M)
- **Compose API:** `VelocityTracker`, `VelocityTracker1D`, `addPointerInputChange`.
- **What:** public trackers in compose-ui with Compose's Lsq2 (second-degree least squares) and impulse strategies, differential data, maximum velocity, the 40 ms "pointer stopped" rule; historical samples. compose-rs's is a crate-private linear fit (`compose-foundation/src/fling.rs`).
- **Tests:** VelocityTrackerTest, VelocityTracker1DTest, PlatformVelocityTrackerTest, PolyFitLeastSquaresTest, VelocityTrackingParityTest, VelocityTrackingListParityTest.
- **Depends on:** pointer-event-model (historical changes); foundation's fling adopts it (foundation area).
- **Experimental:** no.

### view-configuration (S)
- **Compose API:** `ViewConfiguration`, `LocalViewConfiguration`.
- **What:** the host provides touch slop, timeouts, minimum touch target and maximum fling velocity (Android's `ViewConfiguration`); `pointer_input` blocks restart when the density or configuration they read changes.
- **Status:** partly done (D907): `ViewConfiguration` (every field, the handwriting ones included) and `LOCAL_VIEW_CONFIGURATION`, the host's, read by the detectors (`PointerInputScope::view_configuration`), the clickable family (D1062), scrolling's slop and fling bounds and compose-test. Left: the restart of `pointer_input` blocks when the configuration or the density changes (a running block keeps its scope's first values until its keys change), and a subtree's configuration, which needs modifier nodes reading composition locals.
- **Tests:** PointerInputViewConfigurationTest, PointerInputDensityTest.
- **Depends on:** none.
- **Experimental:** no.

### nested-scroll-dispatcher (M)
- **Compose API:** `NestedScrollDispatcher`, `nestedScroll(connection, dispatcher)`, `nestedScrollModifierNode`, suspending fling callbacks with `Velocity`.
- **What:** a chain of nested-scroll nodes through layout ancestors (not the pointer's hit path, D188), dispatchers any code can drive (custom scrollables, animations), a coroutine scope tied to the node.
- **Tests:** NestedScrollModifierTest; the interop rows in host-view-interop.
- **Depends on:** foundation's scrollables move to dispatchers (foundation area); a `Velocity` unit type (layout/unit area, if absent).
- **Experimental:** no.

### rotary-input (S)
- **Compose API:** `onRotaryScrollEvent`, `onPreRotaryScrollEvent`, `RotaryScrollEvent`, `RotaryInputModifierNode`.
- **What:** rotary crown events from Android's `onGenericMotionEvent` (`SOURCE_ROTARY_ENCODER`) to the focused node, preview first then bubbling.
- **Tests:** RotaryScrollEventTest; part of FocusAwareEventPropagationTest.
- **Depends on:** key-input-api (the shared focus-aware dispatch with a preview pass).
- **Experimental:** no.

### indirect-pointer-input (M)
- **Compose API:** the `androidx.compose.ui.input.indirect` package.
- **What:** touch-navigation devices (touchpads without a cursor): events to the focused node, cancellation when focus moves, a navigation gesture moving focus along the primary axis.
- **Tests:** DelegatedIndirectPointerAndFocusEventTests, IndirectPointerEventNavigationSystemTests, IndirectPointerEventTest, IndirectPointerNavigationGestureDetectorTest, IndirectPointerEventWithInputDeviceMockTest, IndirectPointerInputChangeTest, IndirectPointerNavigationGestureDetectorHostTest.
- **Depends on:** pointer-event-model (`PointerId`, `HistoricalChange`), focus-hierarchy.
- **Experimental:** no.

### semantics-properties (XL)
- **Compose API:** `SemanticsPropertyKey`, `SemanticsPropertyReceiver`, typed `SemanticsConfiguration`, `AccessibilityAction`, `CustomAccessibilityAction`, and the properties and actions listed in section 3.
- **What:** semantics as an open set of keyed properties with merge policies and labelled actions, as Compose's, replacing the fixed `SemanticsConfiguration` struct and the `SemanticsAction` enum; every compose-rs component and both accessibility bridges move to it; test matchers on keys.
- **Status:** done (D1250–D1254): the keys, both sets but the deprecated, autofill and `TextRange` keys, the receiver, actions as closures, custom actions, every component and both bridges moved, the key matchers. Left: components setting the new properties (lazy lists' and `selectable_group`'s `CollectionInfo`, `BasicText`'s text substitution, headings, live regions), which CollectionInfoTest needs.
- **Tests:** CollectionInfoTest; most of what SemanticsTests, WindowContentChangeTest, AndroidAccessibilityTest and AndroidComposeViewAccessibilityDelegateCompatTest miss; every Compose test using `SemanticsMatcher.expectValue` in other areas.
- **Depends on:** none in ui; text's `TextRange`/`AnnotatedString` (present) for text properties; foundation and Material components set the new properties (their areas).
- **Experimental:** no (the host-facing `accessibilityExtraKey` is `ExperimentalComposeUiHostApi`).

### semantics-tree-api (L)
- **Compose API:** `SemanticsOwner`, the public `SemanticsNode`, `SemanticsModifierNode` merge/clear flags and `invalidateSemantics`, the `ExperimentalComposeUiHostApi` queries, `RootForTest`.
- **What:** a cached semantics tree updated on invalidation, stable ids, children sorted by placement and z-index, bounds in root/window/screen, touch bounds, fake nodes, transparency hiding nodes, uncovered-node queries and hit tests for accessibility.
- **Tests:** SemanticsInfoTest, SemanticsModifierNodeTest, SemanticsHostApiTest; parts of SemanticsTests, AndroidAccessibilityTest.
- **Depends on:** semantics-properties.
- **Experimental:** the host queries are.

### android-accessibility-parity (XL)
- **Compose API:** none new (behaviour of `AndroidComposeViewAccessibilityDelegateCompat`).
- **What:** `Accessibility.kt` reaching the delegate's behaviour: traversal order (sorting by bounds, traversal groups and indices, `traversalBefore/After`), live regions, pane appear/disappear events, custom and labelled actions, text granularity movement and selection, set-text, extra data (character locations, test tags as resource ids), state and text change events with the right change types, sensitive data, transparent and covered nodes pruned, explore-by-touch overlay rules, fake nodes, `showOnScreen`. Move decisions now in Kotlin (class names, action lists) into Rust so they can be tested headless, as `scroll_actions` is.
- **Tests:** AccessibilityIteratorsTest, ShowOnScreenAccessibilityTest, AndroidComposeViewAccessibilityTraversalTest; the rest of AndroidAccessibilityTest, AndroidComposeViewAccessibilityDelegateCompatTest, ScrollingTest, WindowContentChangeTest, AccessibilityTouchModeTest.
- **Depends on:** semantics-properties, semantics-tree-api, bring-into-view-api, input-mode; text layout queries (`getLineForOffset`, bounding boxes; text area, docs/11).
- **Experimental:** no.

### autofill (L)
- **Compose API:** the `androidx.compose.ui.autofill` package's non-deprecated types, `Modifier.contentType`, `LocalAutofillManager`, the autofill semantics.
- **What:** Android: populate `ViewStructure` for autofill and assist from semantics, fill through `onFillData`, commit/cancel sessions as nodes come and go, credential requests; desktop: none.
- **Tests:** AndroidAutofillManagerTest, AndroidCredentialViewStructureTest, AndroidFillableDataTest, PerformAndroidAutofillManagerTest, TextFieldStateSemanticAutofillTest, TextFieldsSemanticAutofillTest, AndroidContentDataTypeTest, AndroidContentTypeTest, AutofillUtilsTest, AndroidAssistTest, SemanticsPropertiesAndroidTest.
- **Depends on:** semantics-properties, semantics-tree-api; foundation text fields (and `TextFieldState`, text area).
- **Experimental:** no.

### content-capture (M)
- **Compose API:** `ContentCaptureManager.isEnabled`.
- **What:** Android content capture sessions told of semantics nodes appearing, disappearing and scrolling.
- **Tests:** ContentCaptureTest.
- **Depends on:** semantics-tree-api.
- **Experimental:** yes (`ExperimentalComposeUiApi`).

### drag-and-drop (L)
- **Compose API:** the `androidx.compose.ui.draganddrop` package's non-deprecated types.
- **What:** source and target nodes, a transfer started from a gesture with a drawn decoration, events dispatched to interested targets by position; Android `DragEvent` and `startDragAndDrop`; desktop drag-and-drop where winit offers it (files).
- **Tests:** AndroidDragAndDropTest.
- **Depends on:** hit testing as present; foundation's `dragAndDropSource`/`dragAndDropTarget` (foundation area) build on it.
- **Experimental:** no.

### bring-into-view-api (M)
- **Compose API:** `BringIntoViewModifierNode`, `DelegatableNode.bringIntoView(bounds)`.
- **What:** a public, suspending request propagated to the nearest responder with rect translation and interruption, replacing the synchronous `ModifierNode::bring_into_view` focus path.
- **Tests:** part of BringIntoViewModifierNodeTest; ShowOnScreenAccessibilityTest.
- **Depends on:** none (foundation's `BringIntoViewRequester`/`Responder` are the foundation area).
- **Experimental:** no.

### window-info (S)
- **Compose API:** `WindowInfo`, `LocalWindowInfo`, `isCrossWindowBlurEnabled`.
- **What:** window focus (false while a dialog or focusable popup shows, as Compose's separate windows), keyboard modifiers as state, container size; both hosts.
- **Tests:** WindowInfoCompositionLocalTest, WindowInfoTest (both), CrossWindowBlurTest.
- **Depends on:** none.
- **Experimental:** no.

### sound-effects (S)
- **Compose API:** `SoundEffect`, `LocalSoundEffect`.
- **What:** navigation and click sounds through Android's `View.playSoundEffect`; nothing on desktop.
- **Tests:** SoundEffectOnInteractionTest, FocusSoundTest, FocusSoundInteropTest.
- **Depends on:** key-focus-navigation.
- **Experimental:** no.

### platform-locals (S)
- **Compose API:** `LocalAccessibilityManager`, `LocalLocale`, `LocalLocaleList`, `LocalCursorBlinkEnabled`, `LocalScrollCaptureInProgress`, `LocalTaskDispatchers`.
- **What:** the remaining platform composition locals; locales from the host configuration.
- **Tests:** AppCompatActivityLocaleTest, ConfigChangeActivityLocaleTest, TaskDispatchersCompositionLocalTest, TaskDispatchersTest; part of AndroidCompositionLocalTest.
- **Depends on:** a `Locale` type (text area).
- **Experimental:** no.

### text-toolbar (M)
- **Compose API:** `TextToolbar`, `TextToolbarStatus`, `LocalTextToolbar`.
- **What:** the floating copy/paste toolbar for selections (Android `ActionMode`); desktop context menu.
- **Tests:** TextActionModeCallbackTest.
- **Depends on:** text selection (text area).
- **Experimental:** no.

### clipboard-api (M)
- **Compose API:** `Clipboard`, `ClipEntry`, `ClipMetadata`, `LocalClipboard`.
- **What:** the suspending clip-entry clipboard Compose now recommends, with `AnnotatedString` spans encoded on Android, alongside or replacing the plain-text `ClipboardManager` (deprecated in Compose, D281).
- **Tests:** AndroidClipboardTest, AndroidClipboardIntegrationTest; ClipboardTest is portable now.
- **Depends on:** `AnnotatedString` spans (present; text area for encoding).
- **Experimental:** `ClipEntry.firstUriOrNull` only.

### host-view-interop (L)
- **Compose API:** `rememberNestedScrollInteropConnection`; behaviour of the host view.
- **What:** `ComposeRsView` as a well-behaved View among Views: focus search into, out of and through it (`focusSearch`, `onRequestFocusInDescendants`, `getFocusedRect`), keys not consumed when focus can't move (Tab and arrows leave the view), view and window focus driving the root focus, `requestDisallowInterceptTouchEvent` when content consumes a move, `canScrollHorizontally/Vertically`, nested scrolling with `NestedScrollingParent`/`Child` Views, disposal strategies in pooling containers (`RecyclerView`), screen coordinates. Needs an instrumented Android test harness: docs/09 describes one, but the repository has no Android test source set yet.
- **Tests:** the 23 `feature:host-view-interop` rows (focus, viewinterop, platform), and parts of FocusManagerCompositionLocalTest, AndroidPointerInputTest, PointerInputEventProcessorTest, DispatchGenericMotionEventTest, ScrollingTest.
- **Depends on:** focus-hierarchy, nested-scroll-dispatcher, key-focus-navigation.
- **Experimental:** no.

### platform-text-input (L)
- **Compose API:** the `PlatformTextInput*` types and `InterceptPlatformTextInput`.
- **What:** text input sessions any node can open, interceptable, with a full Android `InputConnection` (surrounding text, selection, batch edits, cursor anchor info, extracted text). Overlaps the text area's text fields.
- **Tests:** AndroidPlatformTextInputSessionTest, InterceptPlatformTextInputTest, PlatformTextInputMethodTestOverrideTest, PlatformTextInputViewIntegrationTest, CursorAnchorInfoBuilderTest, TextInputServiceAndroidCursorAnchorInfoTest, RecordingInputConnectionUpdateTextFieldValueTest; parts of EditorInfoTest, RecordingInputConnectionTest, RecordingInputConnectionInactiveTest, TextInputServiceAndroidOnStateUpdateTest.
- **Depends on:** text area (field state, layout queries).
- **Experimental:** no.

### scroll-capture (M)
- **Compose API:** behaviour of `AndroidComposeView` (Android 12+ long screenshots), `LocalScrollCaptureInProgress`.
- **What:** find the largest, deepest vertical scroll container from semantics and capture it by scrolling and drawing.
- **Tests:** ScrollCaptureTest, ScrollCaptureIntegrationTest, ScrollCaptureDrawTest.
- **Depends on:** semantics-tree-api, semantics-properties (`scrollByOffset`), drawing a node offscreen (drawing area).
- **Experimental:** no.

### platform-window-modifiers (S)
- **Compose API:** `keepScreenOn`, `sensitiveContent`, `preferredFrameRate`, `FrameRateCategory`.
- **What:** modifiers setting properties of the host window/view while their nodes exist (Android `keepScreenOn`, `setContentSensitivity`, `setRequestedFrameRate`; desktop: keep-awake where available, otherwise nothing).
- **Tests:** KeepScreenOnModifierTest, SensitiveContentModifierTest, FrameRateTest, AndroidComposeViewTest.
- **Depends on:** none.
- **Experimental:** no.

### popup-dialog-properties (M)
- **Compose API:** the `PopupProperties`/`DialogProperties` fields of section 3, `SecureFlagPolicy`, `DialogWindowProvider`.
- **What:** the window properties that still mean something for overlays (`usePlatformDefaultWidth`, `decorFitsSystemWindows`/insets, `clippingEnabled`, `excludeFromSystemGesture`, `scrimAlpha`), and a decision for the window-only ones (secure flag on the host window, blur, window type and token, title).
- **Tests:** DialogSecureFlagTest, PopupSecureFlagTest, DialogWithInsetsTest, DialogWithNoStatusBarTest; parts of DialogTest, PopupTest, PopupLayoutTest.
- **Depends on:** window insets (present, D222; consumption is the layout area).
- **Experimental:** no.

### media-query (M)
- **Compose API:** `UiMediaScope`, `mediaQuery`, `derivedMediaQuery`.
- **What:** recomposition-minimal queries over window size, posture, pointer precision, keyboard kind, viewing distance and device features.
- **Tests:** MediaQueryTest, MediaQueryIntegrationTest.
- **Depends on:** window-info.
- **Experimental:** yes (`ExperimentalMediaQueryApi`).

### Suggested order

1. **view-configuration** (S), **input-mode** (S), **window-info** (S): small, unblock others.
2. **key-input-api** (M): preview pass and real `Key`s fix two behaviour differences (up without down, Shift changing the key) and unblock rotary and soft-keyboard interception.
3. **focus-hierarchy** (L), then **focus-properties** (L), **focus-requester-api** (M), **focus-search-tree** (M), **key-focus-navigation** (S): the largest block of Compose tests (41 files, 682 methods, about 1,600 cases once parameterized) becomes portable; the 2D geometry tests can be ported first, now.
4. **pointer-event-model** (L), **hit-test-extensions** (M), **velocity-tracker** (M), **pointer-icon-api** (S): coordinate with the foundation agent, whose detectors and scrollables move onto them.
5. **nested-scroll-dispatcher** (M), **bring-into-view-api** (M): also coordinated with foundation.
6. **semantics-properties** (XL), then **semantics-tree-api** (L): every area's tests assert semantics, so this unblocks ports elsewhere too.
7. **android-accessibility-parity** (XL), **rotary-input** (S), **sound-effects** (S), **platform-locals** (S), **platform-window-modifiers** (S), **clipboard-api** (M), **popup-dialog-properties** (M).
8. **host-view-interop** (L), together with an instrumented Android test harness.
9. **autofill** (L), **content-capture** (M), **scroll-capture** (M), **drag-and-drop** (L), **indirect-pointer-input** (M), **platform-text-input** (L, with the text area), **text-toolbar** (M), **media-query** (M).

## 5. Notes for the porting agents

### Compose's test patterns in `compose::test`

| Compose | compose-rs |
|---|---|
| `rule.setContent { … }`, `setFocusableContent` | `compose_test(\|\| …)`, `ComposeTestRule::sized(size, \|\| …)`; there is no window focus to grant |
| `rule.runOnIdle { … }`, `waitForIdle()` | code after an action (actions run frames synchronously), `ui.wait_for_idle()` |
| `rule.mainClock.autoAdvance = false`, `advanceTimeBy` | `ComposeTestRule::with_manual_clock`, `advance_time_by`, `advance_time_by_frame` |
| `onNodeWithTag/Text/ContentDescription`, `onAllNodes…[i]` | `on_node_with_tag/text/content_description`, `on_all_nodes(matcher, i)`, `count_nodes`; `on_node_unmerged` for `useUnmergedTree = true` |
| `performClick`, `performSemanticsAction(OnClick)`, `requestFocus()`, `performTextInput`, `performKeyPress` | `perform_click`, `perform_semantics_action(&SemanticsActions::ON_CLICK)`, `request_focus`, `perform_text_input`, `perform_key_press`; `ui.press_key`, `ui.type_text` |
| `assertIsFocused`, `assertIsEnabled`, `assertIsOn`, `assertTextEquals`, `assertExists` | the same in snake case |
| `LocalFocusManager.current.moveFocus(d)` inside content | capture `LOCAL_FOCUS_MANAGER.current()` in a `thread_local!` or a `Rc<Cell<…>>` the test reads, then `wait_for_idle()` before asserting (the move is queued, D230) |
| `FocusRequester()` + `requestFocus()` in `runOnIdle` | `FocusRequester::new()` (a `Copy` handle) created in the test, `request_focus()`, then `wait_for_idle()` |
| `Modifier.onFocusChanged { state = it }` | `on_focus_changed(move \|has_focus\| …)`: a `bool` (`hasFocus`) until focus-hierarchy |
| `performTouchInput { down; moveTo; up }` | `ui.host().send_pointer(PointerEvent::press(at))`, `PointerEvent::moved(at, true)`, `PointerEvent::release(at)`, `.with_pointer(PointerId(n), PointerType::Touch)`, `.at_time(nanos)`; see `crates/compose-foundation/tests/suite/pointer_input.rs` |
| `performMouseInput { enter; moveTo; scroll }` | `send_pointer(PointerEvent::moved(at, false))` (mouse by default), `PointerEvent::scroll(at, delta)`, `host().send_pointer_left()` |
| `rule.onRoot().performKeyPress(KeyEvent(…))` | `ui.host().send_key(KeyEvent { key, kind, modifiers })` for modifiers or a lone key-up |
| `@RunWith(Parameterized::class)` | a loop over the parameters inside one `#[test]`, or one `#[test]` per parameter through a small `macro_rules!`; name the failing parameter in the assertion message |
| `AccessibilityNodeInfo` from `createAccessibilityNodeInfo` | the record `compose_android::accessibility::write_tree` writes, parsed as in `crates/compose-android/tests/accessibility_state.rs`; what `Accessibility.kt` decides in Kotlin (class names, the action list) has no headless test |

### What the harness lacks

> Since the harness's milestone 1 (D900–D905), compose-test has Compose's clock model (rounding, frames on the test clock, `advance_time_until`, `wait_until`, the infinite-animation policy), bounds and displayed assertions, matchers over the tree and selectors (`on_children`, `on_parent`, collections), the touch, mouse (primary button) and key injection scopes, `perform_scroll_to`, `capture_to_image` with `assert_pixels`, and `print_to_string`; `perform_click` is a touch click. The notes in this section and the mapping above that call these missing are out of date: docs/09 lists the API, and [ui-text-and-test](ui-text-and-test.md) what is still missing. Existing tests that relied on the old timing gained a frame where they wait for a recomposition (D900).

- **Input injection.** No `perform_touch_input`/`perform_mouse_input`/`perform_key_input` scopes: Compose's `down(pointerId, position)`, `moveBy`, `up`, `click`, `doubleClick`, `longClick`, `swipe(start, end, duration)`, `swipeUp/Down/Left/Right`, `pinch`, multi-pointer `down(1, …)`, `eventPeriodMillis` with the clock advanced per event, `percentOffset`/`center`/`topLeft` in node coordinates; mouse `press(MouseButton.Secondary)`, `scroll(delta, ScrollWheel.Horizontal)`, `enter`/`exit`; key `keyDown`/`keyUp`/`withKeyDown(Key.ShiftLeft)`/`pressKey(repeat)`; `performRotaryScrollInput`; `performIndirectTouchInput`. Every port writes its own `press`/`drag_to`/`release` helpers today; a shared injection module in `compose-test` (node-relative positions, pointer ids, event times on the manual clock) would serve all the pointer and key ports.
- **Semantics matchers and tree navigation.** Present: `has_text`, `has_text_containing`, `has_test_tag`, `has_content_description`, `has_role`, `has_click_action`, `is_enabled`, `is_not_enabled`, `is_focused`, `is_focusable`, `and`/`or`. Missing: `SemanticsMatcher.expectValue(key, value)`/`keyIsDefined`/`keyNotDefined` (need semantics-properties), `isNotFocused`, `isSelected`, `isToggleable`, `isHeading`, `isDialog`, `isPopup`, `hasScrollAction`, `hasSetTextAction`, `hasProgressBarRangeInfo`, `hasStateDescription`, `hasParent`, `hasAnyAncestor`, `hasAnyChild`, `hasAnyDescendant`, `hasAnySibling`, `isRoot`; `onRoot`, `onChildren`, `onChildAt`, `onParent`, `onSiblings`, `onAncestors`, `fetchSemanticsNodes`, `assertCountEquals`, `printToLog` (`dump_semantics` stands in).
- **Assertions.** Missing: `assertIsNotFocused`, `assertIsSelected`/`NotSelected`, `assertIsToggleable`, `assertHasClickAction`/`NoClickAction`, `assertIsDisplayed`/`NotDisplayed`, `assertContentDescriptionEquals`/`Contains`, `assertTextContains`, `assertValueEquals`, `assertRangeInfoEquals`, bounds assertions (`assertPositionInRootIsEqualTo`, `assertWidthIsEqualTo`, `getUnclippedBoundsInRoot`, `getBoundsInRoot`).
- **Input mode and window focus.** Compose's rules take `ComposeUiTestConfig(inputMode = …)` and grant window focus (`setFocusableContent`); many focus tests run in both modes. Needs input-mode, then a rule option.
- **Host-level checks.** Tests of hosts (key mapping in `bridge.rs`, `ComposeRsView`'s touch conversion, `Accessibility.kt`) have no JVM or instrumented harness (docs/09 describes instrumented tests; none exist yet). Until then, port the Rust half (`UiHost` returns, records, `ImeOptions`) and leave the Kotlin half noted.

### Behaviour that may differ from Compose (check when porting)

1. **Tab is always consumed** (`crates/compose-ui/src/dispatch.rs` `dispatch_key`: an unconsumed Tab key-down moves focus and returns consumed, even with nothing focusable). Compose doesn't consume a key that moved nothing, so an embedding View can take focus out. Caught by `ComposeViewKeyEventInteropTest.composeView_doesNotConsumesKeyEvent_ifTheContentIsNotFocusable` and `composeView_doesNotConsumesKeyEvent_ifFocusIsNotMoved`. In a `ComposeRsView` inside a Kotlin screen, Tab never leaves the view.
2. **Key-ups without their key-down are delivered.** Compose drops them. Caught by `ProcessKeyInputTest.onKeyEvent_ignoresUpWithoutDown` and `onPreviewKeyEvent_ignoresUpWithoutDown`.
3. **Shift changes the key.** Android and desktop send `Key::Character` of the typed character, so Shift+A is `Character('A')`, where Compose's `Key.A` is the same key with `isShiftPressed`. Caught by `KeyTest.androidKeyCode_to_composeKey`, `MetaKeyTest.shiftIsPressed`.
4. **A `FocusRequester` on several targets focuses the first attached; Compose ends on the last** (it requests each in turn). Caught by `ReusedFocusRequesterTest.twoComponents` and `focusRequesterUsedWithThreeComponent`.
5. **`move_focus`/`request_focus` return whether they were queued, not whether focus moved** (D148, D230). Caught by `OneDimensionalFocusSearchNextTest.moveFocus_noFocusableItem` (expects `false`) and every 1D/2D search test asserting the returned value; port them by checking the focus after `wait_for_idle()`.
6. **Arrow and D-pad keys don't move focus on Android** (D230 follows desktop Compose); Compose on Android does. Caught by `KeyEventToFocusDirectionTest.left`/`right`/`up`/`down`, `FocusSoundTest.dpadNavigation_playsSoundEffect`.
7. **Clickables are focusable in touch mode on Android** (compose-rs follows desktop: enabled clickables are always focus targets, D76); Compose's are `Focusability.SystemDefined`. Caught by `FocusTargetModifierNodeTest.requestFocus_focusabilitySystemDefined_touchMode`, `InitialFocusTest.itemFocusedOnAppearing` (touch mode).
8. **Hit testing ignores graphics-layer transforms** (D121) and has **no minimum-touch-target expansion**. Caught by `LayerTouchTransformTest.testTransformTouchEventConsumed`, `InputInLayerTest.reusedLayerIsReset`, `AndroidPointerInputTest.inputOnNestedBoxesLargerThanMinTouchPlacedViaGraphicsLayer_simpleInput_properlyTriggers`, `RestrictedSizeTest.clippedTouchInMinimumTouchTarget`, `AndroidPointerInputTest.unclippedTakesPrecedenceWithMinimumTouchTarget`.
9. **A cancelled gesture restarts the `pointer_input` block** (`PointerInputDriver::stop`); Compose keeps the coroutine and sends it synthetic up events (consumed), so code after `awaitPointerEvent` still runs. Caught by `SuspendingPointerInputFilterTest.testSyntheticCancelEvent`, `testSyntheticMouseCancelEvent`, `SuspendingPointerInputFilterCoroutineJobTest.isPointerInputJobStillActive_cancelPointerEvent_assertsTrue`.
10. **One change per pointer event.** A second finger's press is its own event, so a block never sees two changes in one `PointerEvent` (D75). Caught by `PointerInputEventProcessorTest.process_2DownOn2DifferentPointerNodes_hitAndDispatchInfoAreCorrect` and the multi-pointer `HitPathTrackerTest` cases.
11. **Desktop sends only the left mouse button**, and pointer events carry no keyboard modifiers (docs/06 notes no Shift+click). Caught by `PointerInputEventProcessorTest.buttonsPressed`, `metaState`.
12. **The velocity tracker is a linear least-squares fit over 100 ms with no 40 ms "stopped" cut-off** (`crates/compose-foundation/src/fling.rs`); Compose uses Lsq2 (or the framework tracker) and discards samples before a 40 ms gap, so a drag that pauses before release flings in compose-rs and not in Compose. Caught by `VelocityTrackerTest.calculateVelocity_gapOf40MillisecondsInPositions_positionsAfterGapIgnored`, `PlatformVelocityTrackerTest.longGapBetweenEvents_resetsTracking`.
13. **Density changes don't restart `pointer_input` blocks**: the density is read per event. Caught by `PointerInputDensityTest.compositionLocalDensityChangeRestartsPointerInputOverload1`.
14. **Semantics children are in composition order**, not sorted by placement and z-index. Caught by `SemanticsTests.testChildrenAreZSorted`, `AndroidComposeViewAccessibilityDelegateCompatTest.drawingOrder_customZIndex`, the `AndroidAccessibilityTest.testSortedAccessibilityNodeInfo_*` cases.
15. **Transparent nodes stay in semantics and accessibility.** Compose hides alpha-0 content. Caught by `SemanticsInfoTest.transparent`, `AndroidAccessibilityTest.testTransparentNode_withAlphaModifier_notAccessible`.
16. **Every frame that recomposed or measured sends one subtree content change** (D630); Compose sends per-node change types (state description, text, pane title…) and only when properties change. Caught by `WindowContentChangeTest.sendWindowContentChangeUndefinedEventByDefault_onlyOnce_whenMultiplePropertiesChange`, `AndroidAccessibilityTest.sendStateChangeEvent_whenStateChange`.
17. **Nested-scroll connections are found through the pointer's hit path**, not layout ancestors (D188, docs/11). A connection on an ancestor that isn't on the hit path (a sibling layer placed over) is skipped, and scrolls not started by a pointer have no chain. Caught by `NestedScrollModifierTest.nestedScroll_twoNodes_hierarchyDispatch` and the dispatcher cases.
18. **`ComposeRsView` takes every touch and no hover.** `onTouchEvent` returns `true` whatever the content does with it, and hover `MotionEvent`s reach content only for touch exploration; every Android pointer is typed touch. Compose returns `false` when no pointer node was hit and dispatches mouse and stylus hover. Caught by `AndroidPointerInputTest.dispatchTouchEvent_noPointerInputModifiers_returnsFalse`, `dispatchHoverEnter`, `dispatchHoverExit`, `MotionEventAdapterTest.convertToolType`.
19. **Overlays aren't windows** (D130), so when window-info is added its `isWindowFocused` must model them: Compose's main window loses focus while a dialog shows and the dialog's content sees its own focused window. Caught by `WindowInfoCompositionLocalTest.mainWindowIsNotFocused_whenDialogIsVisible`, `windowIsFocused_whenDialogIsDismissed`.
