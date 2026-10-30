# Design: focus and semantics on par with Compose's

The spec for the [ui-input ledger](../ui-input.md)'s `focus-hierarchy`, `focus-properties`, `focus-requester-api`, `focus-search-tree` (Part A) and `semantics-properties`, `semantics-tree-api`, and the Rust half of `android-accessibility-parity` (Part B), with what the test harness's matchers ([ui-text-and-test](../ui-text-and-test.md)'s `semantics-matchers`) and the platform bridges need. It builds on [modifier-nodes.md](modifier-nodes.md) (kinds, `NodeContext`, `NodeHandle`, delegation, traversal) and, for focus rectangles and bounds, [placement-and-lookahead.md](placement-and-lookahead.md) (live coordinates).

Compose paths are relative to `frameworks/support/compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/` at androidx-main `d153d87ed76`.

# Part A: focus

## A.1 What Compose does

**One active node, a derived tree of states.** `FocusOwnerImpl` (`focus/FocusOwnerImpl.kt`) holds `activeFocusTargetNode` and `isFocusCaptured` (`:581-596`), and a root `FocusTargetNode(focusability = Never)` on the root layout node (`:94-116`). Every other state is derived (`FocusTargetNode.focusState`, `focus/FocusTargetNode.kt:81-96`): the active node is `Active` (or `Captured`), each focus target among its ancestors `ActiveParent`, the rest `Inactive` (`focus/FocusState.kt:58-90`). The "focus tree" is the `FocusTarget`-kind nodes found by `visitAncestors`/`visitChildren`; there is no separate structure.

**Targets and their properties.** `Modifier.focusTarget()` and `FocusTargetModifierNode(focusability, onFocusChange)` (`focus/FocusTargetModifierNode.kt:31-139`) make targets; `Focusability` is `Always`, `SystemDefined` (focusable unless the input mode is touch, read through `LocalInputModeManager`) or `Never` (`focus/Focusability.kt:33-77`). A target's properties are folded from the `FocusPropertiesModifierNode`s before it in its chain and in its ancestors up to the next target above (`fetchFocusProperties`, `FocusTargetNode.kt:248-256`): `canFocus`, `next`, `previous`, `up`, `down`, `left`, `right`, `start`, `end` (each a `FocusRequester`, `Default` meaning "the search decides", `Cancel` meaning "don't move"), `onEnter`/`onExit` (a `FocusEnterExitScope` that may `cancelFocusChange()` or redirect by requesting focus elsewhere), `focusRect` (`focus/FocusProperties.kt:32-246`). `Modifier.focusProperties { }` is the element. Foundation's `Modifier.focusGroup()` is `focusProperties { canFocus = false }.focusTarget()`.

**Events.** `FocusEventModifierNode.onFocusEvent(state)` (`focus/FocusEventModifierNode.kt:30-60`) is told the state of the targets it "covers": `dispatchFocusCallbacks` (`FocusTargetNode.kt:370-381`) calls the target's `onFocusChange(previous, current)` and then every event node from the target up to the next target (`visitSelfAndAncestors(FocusEvent, untilType = FocusTarget)`), stopping if a callback moved the focus. An event node's aggregate state is its first non-inactive target below (`getFocusState`). `Modifier.onFocusChanged` (only on change) and `onFocusEvent` (every event) are its elements. Invalidations (a target or event node attached, detached or changed) are batched by `FocusInvalidationManager` (`focus/FocusInvalidationManager.kt:31-118`) and resolved after the changes are applied: event nodes get the state the active node gives them (`Active`/`Captured` below the first target above the active one, `ActiveParent` above, `Inactive` elsewhere).

**Requesting focus** (`focus/FocusTransactions.kt:49-187`, `performRequestFocus`): refused if an interaction barrier (a modal) occludes the target; the previous active node's ancestors are diffed with the new one's; the previous node is cleared (its `onExit`s may refuse); focus is granted; `onFocusChange`/events go to the previous node (`Active → Inactive`), its no-longer-parent ancestors (`ActiveParent → Inactive`), the new node's new parents (`→ ActiveParent`, top-down), then the node (`→ Active`), each step aborting if a callback moved the focus; finally each new ancestor saves the path (`saveFocusedChild`). A target that can't focus passes the request to the child an `Enter` would pick (`requestFocus(direction)`, `FocusTargetNode.kt:107-117`, `findChildCorrespondingToFocusEnter`). `captureFocus`/`freeFocus` (`:189-228`) toggle `Captured`: while captured, other requests and non-forced clears are refused.

**`FocusRequester`** (`focus/FocusRequester.kt:42-280`) is attached to the `FocusRequesterModifierNode`s of `Modifier.focusRequester`; `requestFocus(direction): Boolean` visits each attached node's first targets below (`visitChildren(FocusTarget)`) and requests each in turn, so with several nodes the **last** one keeps the focus (`findFocusTarget`, `:262-279`); `captureFocus`, `freeFocus`, `saveFocusedChild`, `restoreFocusedChild` stop at the first that succeeds; `Default`, `Cancel` and the internal `Redirect` are sentinels; `createRefs()`. `Modifier.focusRestorer(fallback)` (`focus/FocusRestorer.kt:37-164`) restores the saved child on `Enter` (keyed by the child's composite key hash, saved across recreation through the saveable registry). `requestFocusForChildInRootBounds/LocalBounds` (`focus/RequestChildFocus.kt:40-110`).

**Search.** `FocusOwner.moveFocus(direction): Boolean` (`FocusOwnerImpl.kt:264-339`): the active target's custom destination first (`customFocusSearch`, `focus/FocusTraversal.kt:54-111`: `next`…`end`, `start`/`end` mapped by layout direction, `Enter`/`Exit` through `onEnter`/`onExit`), then the root's `focusSearch` (`FocusTraversal.kt:113-149`):

- **One-dimensional** (`Next`, `Previous`; `focus/OneDimensionalFocusSearch.kt:41-272`): recursive over targets: from the active parent chain, search the focused child's later siblings (forward: a target before its children; backward: children before their parent), siblings sorted by the **placement order** of their layout ancestors under the lowest common ancestor (`FocusableChildrenComparator`, `:225-272`); deactivated (`canFocus = false`) targets are skipped but their children searched; finding nothing, the parent's beyond-bounds layout generates more (`generateAndSearchChildren`, `:106-131`); wrapping around clears focus and takes it again from the start (`FocusOwnerImpl.kt:314-322`). Ineligible targets (unplaced, detached, occluded) are skipped (`isEligibleForFocusSearch`, `FocusTraversal.kt:160-164`).
- **Two-dimensional** (`Up`, `Down`, `Left`, `Right`; `focus/TwoDimensionalFocusSearch.kt:51-429`): among the **siblings of the focused target under its parent target first** (best candidate by beam and weighted distance, `isBetterCandidate`/`beamBeats`, `:269-418`), then out of the parent; a candidate that can't focus is entered (its own children searched); `Enter` searches the active node's children from its top left (`findChildCorrespondingToFocusEnter`, `:120-158`), `Exit` moves to the nearest focusable parent.

**`FocusManager`** (`focus/FocusManager.kt:22-48`): `clearFocus(force)` (runs `onExit`s; refused when captured unless forced) and `moveFocus(direction): Boolean`.

## A.2 Where compose-rs stands

`UiTree.focus: Option<Focus { node, index }>` (`crates/compose-ui/src/focus.rs:14-19`): one focused target (a chain index); `FocusTargetModifierNode` (`crates/compose-ui/src/modifier.rs:1054-1063`) has `can_focus`, `on_focus_changed(bool)` and compose-rs's `on_focus_tap`, implemented by `ClickableNode`, `FocusableNode` (`nodes.rs:697-709`, `1094-1127`), the text field, the slider and the selection container. `on_focus_changed` listeners report `hasFocus` only, by walking **the whole tree** on each change (`update_focus_listeners`, `focus.rs:268-308`, D85). Tab order is a flat pre-order list of targets (`focus_order`, `focus.rs:336-360`) skipping unplaced nodes, restricted to a modal overlay when one shows; two-dimensional search is Compose's geometry over all targets flat, with lazy lists as the only groups (`focus_manager.rs:170-289`, D483); beyond-bounds layout for lazy lists is asynchronous (D477). `FocusRequester` is a `Copy` id in a thread-local registry (`focus_requester.rs:60-123`, D240); requests are queued and applied when the tree is free (D148), focusing the **first** attached node; `FocusManager` likewise (D230), returning whether a tree was asked. No `FocusState`, `focusProperties`, capture, saved children, `Enter`/`Exit`, `onFocusEvent`, `focusTarget()`, `Focusability`.

The gap is narrower than it looks: Compose too keeps **one** active node and derives the rest; what compose-rs lacks is the derived states, the per-target properties, the transactions and the tree-scoped searches.

## A.3 Design

### A.3.1 Kinds, nodes and the owner

```rust
// Kinds (modifier-nodes §3.1): FOCUS_TARGET, FOCUS_PROPERTIES, FOCUS_EVENT, FOCUS_REQUESTER.

/// As `FocusState`.
pub trait FocusState { fn is_focused(&self) -> bool; fn has_focus(&self) -> bool; fn is_captured(&self) -> bool; }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusStateImpl { Active, ActiveParent, Captured, Inactive }   // implements FocusState

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Focusability(u8);   // ALWAYS, SYSTEM_DEFINED, NEVER

/// A focus target, as Compose's `FocusTargetModifierNode`: the per-target data and callback;
/// the logic (states, transactions, searches) is the tree's.
pub trait FocusTargetModifierNode {
    fn focusability(&self) -> Focusability { Focusability::ALWAYS }            // replaces can_focus
    fn on_focus_changed(&mut self, previous: FocusStateImpl, current: FocusStateImpl, cx: &mut NodeContext<'_>) {}
    fn on_focus_tap(&mut self, position: Offset, cx: &mut NodeContext<'_>) {}  // compose-rs's (text fields)
}
/// Compose's factory: a target to delegate to (modifier-nodes §3.7).
pub struct FocusTargetNode { focusability: Focusability, on_focus_change: Option<Callback<dyn Fn(FocusStateImpl, FocusStateImpl)>> }
pub fn focus_target_modifier_node(focusability: Focusability, on_focus_change: Option<…>) -> FocusTargetNode;

pub trait FocusPropertiesModifierNode { fn apply_focus_properties(&self, properties: &mut FocusProperties); }
pub trait FocusEventModifierNode { fn on_focus_event(&mut self, state: FocusStateImpl, cx: &mut NodeContext<'_>); }
pub trait FocusRequesterModifierNode {}   // marker; its functions are on NodeContext (A.3.4)

/// The tree's focus state (Compose's FocusOwnerImpl), replacing `UiTree.focus`.
pub(crate) struct FocusOwner {
    active: Option<FocusTargetRef>,        // (NodeId, EntryId, DelegatePath): stable across chain diffs
    captured: bool,
    invalidated_targets: SmallVec<[FocusTargetRef; 4]>,
    invalidated_events: SmallVec<[EntryRef; 4]>,
    owner_invalidated: bool,
}
```

- **The root target** is implicit: a target "above" the root layout node with `Focusability::NEVER`, so searches and `clear_focus` have a root as Compose's (`rootFocusNode`), without a real chain entry.
- **States are derived**, as Compose's: `focus_state(target)` is `Active`/`Captured` if it is the active one, `ActiveParent` if the active one is below it (`visit_ancestors(FOCUS_TARGET)` from the active one), else `Inactive`. Asked for one node, it costs the active node's depth.
- **Properties** are folded on demand: `fetch_focus_properties(target)` starts from `can_focus = focusability.can_focus(input mode)` and applies each `FOCUS_PROPERTIES` entry before the target in its chain and up its ancestors until the next target (`visit_ancestors` with a stop at `FOCUS_TARGET`, modifier-nodes §3.8). `Modifier.focus_properties(|properties| …)` is its element; `FocusProperties` is a struct of Compose's fields (`next`…`end` as `FocusRequester`, with `FocusRequester::DEFAULT`, `CANCEL` constants and a private `REDIRECT`; `on_enter`/`on_exit` as `Callback<dyn Fn(&mut FocusEnterExitScope)>`; `focus_rect: Rect`, `UNSET_FOCUS_RECT`). `invalidate_focus_properties` (and auto-invalidation of a `focus_properties` update that sets `can_focus`, `NodeKind.kt:394-399`) re-checks the active target: one that can no longer focus loses it.
- **`Focusability::SYSTEM_DEFINED`** reads the input mode (ui-input's `input-mode` feature: `LOCAL_INPUT_MODE_MANAGER`, read through `current_value_of`, modifier-nodes §3.9).
- **Modal overlays as interaction barriers.** compose-rs's dialogs and focusable popups are overlays in one tree (D130), where Compose's are windows with their own owners. While a modal overlay shows, targets outside it are ineligible and requests to them fail (Compose's `isOccludedByInteractionBarrier`, `FocusTraversal.kt:192-252`); searches start from the overlay's subtree as a root, which is what `focus_order` does today (`focus.rs:336-369`) and what `FocusChangedTest`'s barrier cases test.

### A.3.2 Transactions

`UiTree::request_focus_target(target, direction) -> bool` is `performRequestFocus` step by step (§A.1): the barrier check; if the target can't focus, the `Enter` child (A.3.5); the ancestor diff (`visit_ancestors(FOCUS_TARGET)` of the previous and the new active nodes); `prepare_to_clear` on the previous one (its and its ancestors' `on_exit`, refused while captured unless forced); the grant; then the callbacks in Compose's order, each followed by "did a callback move the focus? then stop and report false". `clear_focus(force)`, `capture_focus`, `free_focus` likewise.

**Callbacks** (`on_focus_changed` of targets, `on_focus_event` of event nodes, `on_enter`/`on_exit` scopes) run inside the transaction with a `NodeContext`, as Compose runs them synchronously, so `FocusTransactionsTest`'s re-entrant requests work; their application callbacks (the `on_focus_changed(|state| …)` lambda) are queued as actions and run after, as today (`Action::CallWithBool`, D85), so application code never runs while the tree is borrowed. A request made from inside a callback (re-entrancy) is executed at once on the tree, nested, as Compose's.

**Events** go up from the target to the next target above (`dispatch_focus_callbacks`), so a focus change costs the depth of the two nodes, not the tree: `update_focus_listeners`' whole-tree walk (`focus.rs:268-308`) goes. Attach, detach and modifier changes of targets and event nodes are recorded in `FocusOwner`'s invalidation lists and resolved at the end of the pass (modifier-nodes §3.5's queue), as `FocusInvalidationManager.invalidateNodes` (`:57-108`): a detached active target clears the focus; event nodes get their state from the active node's position.

`on_focus_changed(|state: FocusState| …)` becomes Compose's (a `FocusState`, called on change); `on_focus_event(|state| …)` is added (every event). The `bool` form goes (8 call sites in the repository).

### A.3.3 Results: synchronous when the tree is free

D148 and D230 queue focus requests and return "a tree was asked", because the requests often come from callbacks running while the tree is borrowed. The design keeps the queue for that case and answers synchronously otherwise:

- `FocusRequester::request_focus(direction)`, `capture_focus`, `free_focus`, `save_focused_child`, `restore_focused_child`, `FocusManager::move_focus`, `clear_focus`: if the requester's tree is free (borrowable through the registry: an effect, a test, a task, an action run after dispatch), the request is performed now and its real result returned; if the tree is busy (called from inside a pass, which only node code can do, and node code has `NodeContext` versions that act at once), it is queued as today and returns `true` if attached. Application code never sees the queued form in practice; `OneDimensionalFocusSearchNextTest.moveFocus_noFocusableItem` and the searches asserting `moveFocus`' result become portable.
- **Several nodes**: `request_focus` requests each attached node's targets in attachment order, as `findFocusTarget`, so the last one wins (divergence 4 in ui-input §5 ends); `capture_focus`/`free_focus`/`save`/`restore` stop at the first success.
- `FocusRequester::create_refs()` returns a tuple of new requesters (Compose's destructuring factory); `DEFAULT` and `CANCEL` are constants (ids 0 and 1).

### A.3.4 Requester nodes, restoration, child requests

`Modifier.focus_requester(requester)` keeps its node (`focus_requester.rs:146-197`), now of kind `FOCUS_REQUESTER` with the registry attaching on `on_attach`/`on_detach` (modifier-nodes §3.5) instead of `refresh_chain` and `set_requesters_attached` (`tree.rs:849-854`, `1103-1123`). A node implementing `FocusRequesterModifierNode` gets `NodeContext::request_focus(direction) -> bool`, `capture_focus`, `free_focus`, `save_focused_child`, `restore_focused_child` acting on the targets below it.

`save_focused_child` stores, on the target, the **composite key hash** of the focused child's layout node (`previouslyFocusedChildHash`, `FocusTargetNode.kt:149`): `LayoutNode` gains `composite_key_hash: u64`, from the runtime's `current_composite_key_hash` at emission (one read per emission; the runtime has it). `Modifier.focus_restorer(fallback)` is a properties node whose `on_enter` restores the saved child or requests `fallback`, and registers the hash in the saveable registry so it survives recreation (`FocusRestorerConfigurationChangeTest`). Lazy lists keep the restored child composed through pinning (compose-rs pins the focused item, docs/06).

`NodeContext::request_focus_for_child_in_root_bounds(rect)` / `…_in_local_bounds` pick the child target overlapping `rect` most (`RequestChildFocus.kt`).

### A.3.5 Searches over the target tree

`move_focus(direction)`: the active target's custom destination (`next`…`end`, `Enter`/`Exit` through the scopes; `CANCEL` → `false`; a redirect → `true`), then:

- **`Next`/`Previous`**: Compose's recursive search (`OneDimensionalFocusSearch.kt:51-188`) over targets found with `visit_children(FOCUS_TARGET)`, siblings sorted by the placement order of their layout ancestors under the common ancestor (compose-rs's `place_order`, `tree.rs:177`), deactivated targets entered, ineligible targets (not placed, deactivated, outside a modal) skipped; at the end, wrap around by clearing and taking focus from the root, as `FocusOwnerImpl.kt:314-322`. This replaces `focus_order`'s flat list, `tab_target`'s clipping heuristic (`reachable_from`, `focus.rs:529-556`: Compose skips only unplaced items, which lazy lists' items beyond the viewport are in Compose but, with compose-rs's `keep_items_behind`/beyond-bounds items placed, are not; the beyond-bounds items placed for a search stay eligible as in Compose) and `focus_order`'s overlay rules (now the barrier rule).
- **`Up`/`Down`/`Left`/`Right`**: Compose's tree-scoped search (`TwoDimensionalFocusSearch.kt:51-267`): siblings under the parent target first, then outward; candidates that can't focus entered; the geometry (`is_candidate`, `beam_beats`, `weighted_distance`) moves from `focus_manager.rs:292-367`, where it matches Compose's already. Focus rectangles come from live coordinates and `focus_rect` properties (`fetchFocusRect`, `FocusTargetNode.kt:267-284`).
- **`Enter`/`Exit`**: the active target's children from its top left (or bottom right, by direction) / the nearest focusable parent target.
- **Beyond bounds**: where Compose calls `searchBeyondBounds` (`generateAndSearchChildren`), compose-rs's D477 mechanism plugs in unchanged: the search asks the lazy list (`lay_out_beyond_bounds`), waits for the next layout, and resumes (`BeyondSearch`, `focus.rs:574-583`). The ledger's `beyond-bounds-layout` feature later makes the provider node public (`BeyondBoundsLayoutProviderModifierNode`) and can make it synchronous with `remeasure_now` (modifier-nodes §3.4) as Compose's.
- **Key mapping**: Tab/Shift-Tab, arrows and D-pad keys to directions belong to ui-input's `key-focus-navigation`; Tab stays consumed only when the focus moved (ending ui-input §5 divergence 1).

### A.3.6 Foundation and Material

`focusable(enabled, interaction_source)` becomes a delegating node of a `FocusTargetNode` plus semantics (foundation's `focusable-parity`); `focus_group()` is `focus_properties(can_focus = false)` + a target; `clickable` delegates to `focusable` as Compose's (modifier-nodes MN-5); the text field, slider and selection container implement `FocusTargetModifierNode` with the new callback.

## A.4 Cost

| Path | Change | Estimate |
|---|---|---|
| A focus change | Ancestor walks instead of a whole-tree walk for listeners | Cheaper: O(depth) instead of O(nodes) (3,615 layout nodes on the 200-post screen, docs/14) |
| Tab | A recursive search over targets (sorting siblings by placement) instead of a flat pre-order list | Comparable for flat screens; cheaper on big ones (no full list) |
| Frames without focus changes | None (states derived on demand) | — |
| Node emission | `composite_key_hash` read (A.3.4) | A field copy from the composer's current key |
| Hit tests, draw | None | — |

## A.5 Migration

| Step | Content | Compose tests it makes portable |
|---|---|---|
| FO-1 | Kinds and `FocusOwner` (active target as `FocusTargetRef`), derived `FocusStateImpl`, `Focusability`, `focus_target()`, `FocusTargetNode` factory, `FocusEventModifierNode`, `on_focus_event`, `on_focus_changed(FocusState)`, transactions and their callback order, invalidation at pass end, barriers for modal overlays (needs MN-3, MN-6) | `FocusChangedTest`, `FocusEventCountTest`, `FocusTargetModifierNodeTest` (`SystemDefined` after input-mode), `DeactivatedFocusNodeTest`, `RequestFocusTest`, the rest of `FocusAggregationTest`, `FocusChangedCountTest`, `FocusTargetAttachDetachTest`, `FocusRequesterTest` |
| FO-2 | `FocusProperties`, `focus_properties`, `FocusPropertiesModifierNode`, `DEFAULT`/`CANCEL`, `Enter`/`Exit` directions and scopes, custom destinations, `focus_rect` | `CancelFocusMoveTest`, `ClearFocusExitTest`, `CustomFocusRectTest`, `CustomFocusTraversalTest`, `RequestFocusEnterExitTest`, `RequestFocusEnterTest`, `RequestFocusExitTest`, `TwoDimensionalFocusTraversalImplicitEnterTest`, `TwoDimensionalFocusTraversalImplicitExitTest`, the rest of `FocusTransactionsTest` |
| FO-3 | Synchronous results (A.3.3), several nodes, capture/free, save/restore, `focus_restorer`, child-in-bounds requests, `create_refs`, `clear_focus(force)` honouring capture | `CaptureFocusTest`, `FreeFocusTest`, `FocusRestorerTest`, `FocusRestorerConfigurationChangeTest`, `RequestChildFocusTest`, `RestoreFocusTest`, `ReusedFocusRequesterTest`, `ReusedFocusRequesterCaptureFocusTest`, `ReusedFocusRequesterFreeFocusTest`, the rest of `FocusManagerCompositionLocalTest` |
| FO-4 | Tree-scoped 1D and 2D searches, `Enter`/`Exit` searches, wrap-around, D477 at Compose's hook | `TwoDimensionalFocusTraversalTest`, `TwoDimensionalFocusTraversalEnterTest`, `TwoDimensionalFocusTraversalExitTest`, `OneDimensionalFocusSearchNextTest`, `OneDimensionalFocusSearchPreviousTest`, `FocusSearchNonPlacedItemsTest` (`Enter`), `WrapAroundFocusTest`; with foundation: `FocusGroupTest` |
| FO-5 (foundation) | `focusable`, `focus_group`, `clickable` as delegating nodes | Foundation's focusable and clickable focus tests |

The geometry-only 2D tests (`TwoDimensionalFocusTraversalTwoItemsTest`, `…ThreeItemsTest`, `…InitialFocusTest`) are portable before FO-1 (the ledger's `port` rows) and should be, as regression guards for FO-4.

## A.6 What the implementation settled

FO-1 to FO-5 are in `focus.rs`, `focus_properties.rs`, `focus_search.rs`, `focus_requester.rs`, `input_mode.rs` (D1267-D1279). Where they differ from the text above: `FocusState` is one enum (D1267); a block's `requester.request_focus()` acts as the block returns, its scope's at once (D1270); `focusable`, `clickable` and the fields are targets themselves, not delegating to a `FocusTargetNode` (D1274); callbacks take an `EventContext`, which derefs to `NodeContext`.

# Part B: semantics

## B.1 What Compose does

**Keys with merge policies.** `SemanticsPropertyKey<T>(name, mergePolicy)` (`semantics/SemanticsProperties.kt:493-590`): the default policy is `parentValue ?: childValue`; `ContentDescription` and `Text` concatenate (`:43-49`, `:244-251`), `PaneTitle`, `IsPopup`, `IsDialog` refuse to merge (throw), `InvisibleToUser`, `HideFromAccessibility`, `TraversalIndex`, `TestTag`, `Role`, `Shape` keep the parent's (`:120-170`, `:217-240`, `:315-330`), actions merge label and action separately (`ActionPropertyKey`, `:633-645`), `CustomActions` concatenate (`:461-466`); `AccessibilityKey` marks keys important for accessibility (`:588-596`).

**The sets.** `SemanticsProperties` (`:41-336`): `ContentDescription`, `StateDescription`, `ProgressBarRangeInfo`, `PaneTitle`, `SelectableGroup`, `CollectionInfo`, `CollectionItemInfo`, `Heading`, `TextEntryKey`, `Disabled`, `LiveRegion`, `Focused`, `IsContainer` (deprecated), `IsTraversalGroup`, `IsSensitiveData`, `InvisibleToUser` (deprecated), `HideFromAccessibility`, `ContentType`, `ContentDataType`, `FillableData`, `TraversalIndex`, `HintText`, `HorizontalScrollAxisRange`, `VerticalScrollAxisRange`, `IsPopup`, `IsDialog`, `Role`, `TestTag`, `LinkTestMarker`, `Text`, `TextSubstitution`, `IsShowingTextSubstitution`, `InputText`, `EditableText`, `TextSelectionRange`, `TextCompositionRange`, `ImeAction`, `Selected`, `ToggleableState`, `InputTextSuggestionState`, `Password`, `IsPasswordObfuscated`, `Error`, `IndexForKey`, `IsEditable`, `MaxTextLength`, `BackgroundColor`, `Shape`. `SemanticsActions` (`:338-490`): `GetTextLayoutResult`, `OnClick`, `OnLongClick`, `ScrollBy`, `ScrollByOffset`, `ScrollToIndex`, `OnAutofillText` (deprecated), `OnFillData`, `SetProgress`, `SetSelection`, `SetText`, `SetTextSubstitution`, `ShowTextSubstitution`, `ClearTextSubstitution`, `InsertTextAtCursor`, `OnImeAction`, `PerformImeAction` (deprecated), `CopyText`, `CutText`, `PasteText`, `Expand`, `Collapse`, `Dismiss`, `RequestFocus`, `CustomActions`, `PageUp`, `PageLeft`, `PageDown`, `PageRight`, `GetScrollViewportLength`. `AccessibilityAction<T : Function<Boolean>>(label, action)` (`:609-630`) and `CustomAccessibilityAction(label, action)` (`:654-680`). `SemanticsPropertyReceiver` setters and functions (`:850-1960`: `contentDescription = …`, `heading()`, `onClick(label) { … }`, `isTraversalGroup = …`, `traversalIndex = …`, …).

**Configurations.** `SemanticsConfiguration` (`semantics/SemanticsConfiguration.kt:111-334`) is a map from keys to values with `isMergingSemanticsOfDescendants` and `isClearingSemantics`; `mergeChild` (`:229-239`) applies each key's policy; `collapsePeer` (`:251-272`) folds a node's own modifiers (outer wins, action labels merged). Each layout node computes its configuration from its `SemanticsModifierNode`s tail to head under read observation (`LayoutNode.calculateSemanticsConfiguration`, `node/LayoutNode.kt:474-495`), caches it, and recomputes it on `invalidateSemantics` (`:436-458`) or when a read changes; `SemanticsModifierNode` has `shouldMergeDescendantSemantics`, `shouldClearDescendantSemantics`, `isImportantForBounds`, `applySemantics` (`node/SemanticsModifierNode.kt:40-128`).

**Trees.** `SemanticsOwner` (`semantics/SemanticsOwner.kt:43-232`): `rootSemanticsNode` (merged), `unmergedRootSemanticsNode`, `get(id)`, `getLayoutInfo`, `hitTest` (host-experimental), `getAllSemanticsNodes`. `SemanticsNode` (`semantics/SemanticsNode.kt:50-940`) wraps a layout node: `id` (the layout node's `semanticsId`, renewed on reuse, `LayoutNode.kt:1463-1465`), `config` (merged when merging), `unmergedConfig`, `children` (one layer of semantics nodes below, in **z order** (`zSortedChildren`, `:512-531`); a merging node's only the merging descendants'; a clearing node none), `parent`, `boundsInRoot`, `boundsInWindow`, `positionInRoot`, `positionInWindow`, `positionOnScreen`, `size`, `touchBoundsInRoot` (minimum touch target, `:267-327`), `layoutInfo`, `isRoot`, fake nodes for a merging node's own content description and role (`emitFakeNodes`, `:873-931`), `isHidden` (transparent: alpha 0 on the path) and `isImportantForAccessibility` (`SemanticsOwner.kt:208-232`).

**Traversal order.** Accessibility orders nodes by geometry (`semantics/SemanticsSort.kt:36-282`): rows of overlapping vertical extent top to bottom, each row left to right (right to left in RTL), then by `traversalIndex`, with `isTraversalGroup` nodes ordering their subtree as a unit; the Android delegate turns it into `traversalBefore`/`traversalAfter`.

## B.2 Where compose-rs stands

`SemanticsConfiguration` is a fixed struct of 30 fields (`crates/compose-ui/src/semantics.rs:88-130`) with hand-written `merge_child` and `apply` (`:140-225`); actions are an enum performed through `SemanticsModifierNode::perform_action` (`semantics.rs:36-51`, `modifier.rs:1041-1052`); `Modifier.semantics(|config| …)` runs its block at composition and keeps the result (`modifier.rs:394-410`). `UiTree::semantics_tree(merged)` (`semantics.rs:416-547`) builds a snapshot tree on every call, children in composition order, nothing cached, ids the `NodeId`s (the same across reuse). Consumers: the test harness's matchers over `&SemanticsConfiguration` (`crates/compose-test/src/matcher.rs`), `perform_semantics_action` (`crates/compose-ui/src/dispatch.rs:403-458`), Android's bridge (`crates/compose-android/src/accessibility.rs`: a byte record per node with fixed flags, written for `Accessibility.kt`, `android/compose-rs/src/main/java/rs/compose/android/Accessibility.kt:199-290`, which chooses class names and actions in Kotlin), desktop's AccessKit tree (`crates/compose-desktop/src/accessibility.rs:12-140`), and the frame's subtree-changed notification (D630).

## B.3 Design: keys, values, configurations

```rust
/// As `SemanticsPropertyKey<T>`: declared as a `static`, compared by address.
pub struct SemanticsPropertyKey<T: SemanticsValue> {
    name: &'static str,
    merge_policy: fn(parent: Option<&T>, child: &T) -> Option<T>,
    important_for_accessibility: bool,
}
impl<T: SemanticsValue> SemanticsPropertyKey<T> {
    pub const fn new(name: &'static str) -> Self;                                   // parent ?: child
    pub const fn with_merge_policy(name: &'static str, policy: fn(Option<&T>, &T) -> Option<T>) -> Self;
    pub fn merge(&self, parent: Option<&T>, child: &T) -> Option<T>;
    pub fn name(&self) -> &'static str;
}

/// A value semantics can hold without boxing the common types.
pub trait SemanticsValue: Clone + PartialEq + 'static {
    fn into_slot(self) -> ValueSlot;
    fn from_slot(slot: &ValueSlot) -> Option<&Self>;
}
#[doc(hidden)]
pub enum ValueSlot {
    Unit, Bool(bool), Float(f32), Int(i32), String(SharedString), Strings(SmallVec<[SharedString; 1]>),
    AnnotatedString(AnnotatedString), AnnotatedStrings(SmallVec<[AnnotatedString; 1]>), TextRange(TextRange),
    Role(Role), ToggleableState(ToggleableState), ProgressBarRangeInfo(ProgressBarRangeInfo),
    ScrollAxisRange(ScrollAxisRange), CollectionInfo(CollectionInfo), CollectionItemInfo(CollectionItemInfo),
    LiveRegion(LiveRegionMode), ImeAction(ImeAction),
    Action(AccessibilityAction<dyn Fn() -> bool>),         // and one variant per action signature below
    CustomActions(SmallVec<[CustomAccessibilityAction; 1]>),
    Other(Rc<dyn Any>),                                    // application keys of other types
}

/// As `SemanticsConfiguration`: a small map of keys to values, inline up to four.
#[derive(Clone, Default, PartialEq)]
pub struct SemanticsConfiguration {
    entries: SmallVec<[(KeyId, ValueSlot); 4]>,             // KeyId: the key's address
    pub is_merging_semantics_of_descendants: bool,
    pub is_clearing_semantics: bool,
}
impl SemanticsConfiguration {
    pub fn get<T: SemanticsValue>(&self, key: &'static SemanticsPropertyKey<T>) -> Option<&T>;  // Compose's getOrNull
    pub fn contains<T: SemanticsValue>(&self, key: &'static SemanticsPropertyKey<T>) -> bool;
    pub fn set<T: SemanticsValue>(&mut self, key: &'static SemanticsPropertyKey<T>, value: T);
    pub fn for_each_property(&self, f: impl FnMut(&dyn AnySemanticsPropertyKey, &ValueSlot));
    pub(crate) fn merge_child(&mut self, child: &SemanticsConfiguration);   // each key's policy
    pub(crate) fn collapse_peer(&mut self, peer: &SemanticsConfiguration);  // outer wins, action labels merged
}
```

- **The key sets** are two modules of statics with Compose's names in SCREAMING_CASE (`SemanticsProperties::CONTENT_DESCRIPTION`, `SemanticsActions::ON_CLICK`) and Compose's policies (§B.1); compose-rs's two extras stay as compose-rs keys: `PART_OF_PARENT` (the text field's inner node, until the text field has Compose's single semantics node) and the dialog/popup roots. Deprecated keys (`IsContainer`, `InvisibleToUser`, `OnAutofillText`, `PerformImeAction`) are left out. Autofill keys (`ContentType`, `ContentDataType`, `FillableData`, `OnFillData`) come with ui-input's `autofill`.
- **Actions**: `AccessibilityAction<F: ?Sized> { pub label: Option<SharedString>, pub action: Option<Rc<F>> }` with `F` one of Compose's signatures: `dyn Fn() -> bool` (click, long click, focus, expand, collapse, dismiss, copy, cut, paste, IME action, pages), `dyn Fn(f32, f32) -> bool` (`ScrollBy`), `dyn Fn(i32) -> bool` (`ScrollToIndex`), `dyn Fn(f32) -> bool` (`SetProgress`), `dyn Fn(i32, i32, bool) -> bool` (`SetSelection`), `dyn Fn(AnnotatedString) -> bool` (`SetText`, `SetTextSubstitution`, `InsertTextAtCursor`), `dyn Fn(bool) -> bool` (`ShowTextSubstitution`), `dyn Fn(&mut Vec<TextLayoutResult>) -> bool` (`GetTextLayoutResult`), `dyn Fn() -> f32` (`GetScrollViewportLength`); `ScrollByOffset`'s suspending function becomes `dyn Fn(Offset) -> Pin<Box<dyn Future<Output = Offset>>>`. Equality is by label and `Rc::ptr_eq`. `CustomAccessibilityAction { label, action: Rc<dyn Fn() -> bool> }`.
- **The receiver**: `SemanticsConfiguration` gets Compose's `SemanticsPropertyReceiver` API as methods: setters for Compose's `var`s (`set_content_description`, `set_state_description`, `set_traversal_index`, `set_is_traversal_group`, `set_role`, `set_test_tag`, `set_selected`, `set_toggleable_state`, …) and Compose's functions under their names (`heading()`, `disabled()`, `password()`, `dialog()`, `popup()`, `selectable_group()`, `hide_from_accessibility()`, `on_click(label, action)`, `scroll_by(label, action)`, `set_progress(label, action)`, `custom_actions(…)`, …). Today's field writes (48 sites in foundation and Material) move to them mechanically.
- **Actions are performed by calling them**, as Compose: `SemanticsModifierNode::perform_action` and the `SemanticsAction` enum go; a node that offers an action stores its closure in the configuration (built once per node update: `ClickableNode` keeps an `Rc` built in `update`, not per semantics computation). Performing an action clones the `Rc` out of the configuration, releases the tree, and calls it, so it runs as application code; actions that need the tree (focus) go through handles (modifier-nodes §3.3), and `RequestFocus` is the focus target's `request_focus` (A.3.3).

## B.4 Design: nodes, caching and the trees

- **`SemanticsModifierNode`** becomes Compose's: `fn should_merge_descendant_semantics(&self) -> bool { false }`, `fn should_clear_descendant_semantics(&self) -> bool { false }`, `fn is_important_for_bounds(&self) -> bool { true }`, `fn apply_semantics(&self, config: &mut SemanticsConfiguration)` (today's signature), and `NodeContext::invalidate_semantics()`.
- **Caching per layout node**: `LayoutNode.semantics: Option<Box<SemanticsCache>>` (`config: Rc<SemanticsConfiguration>`, `dirty: bool`), allocated for nodes of kind `SEMANTICS`. It is computed **lazily**, when a tree is asked (a test, an accessibility read), tail to head over the node's semantics entries as `calculateSemanticsConfiguration`, under a new observation kind (`Observation::Semantics`, so a state read in `apply_semantics` dirties it; placement doc §3.4 takes the observation kind to three bits); `invalidate_semantics`, auto-invalidation of semantics entries (modifier-nodes §3.6), attach, detach and reuse mark it dirty. Compose computes eagerly at invalidation; lazy is the same answer at no cost when nothing asks (accessibility off, no test).
- **`Modifier.semantics(|config| …)`** keeps running its block at composition and storing the configuration (its skipping rule, `modifier.rs:394-410`). Compose runs the block in `applySemantics`, so a state it reads invalidates semantics only; in compose-rs it recomposes the caller. A known difference, kept: making the block lazy would make the element compare by closure identity and stop skipping.
- **Ids**: `LayoutNode.semantics_id: i32` from a tree counter, renewed when the node is reused (as `LayoutNode.onReuse`), so accessibility never sees one id for two contents; `SemanticsOwner::get(id)` resolves through a map built when a tree is asked (the walk that builds it touches every semantics node anyway). The Android bridge's `virtual_id` (`accessibility.rs:51-53`, the node index) becomes the semantics id.
- **`SemanticsOwner` and `SemanticsNode`** as Compose's, as handles computing on access (Compose's `SemanticsNode` is also created per access):

```rust
pub struct SemanticsOwner { tree: TreeKey }   // UiTree::semantics_owner(), UiHost::semantics_owner()
impl SemanticsOwner {
    pub fn root_semantics_node(&self) -> SemanticsNode;
    pub fn unmerged_root_semantics_node(&self) -> SemanticsNode;
    pub fn get(&self, id: i32) -> Option<SemanticsNode>;                  // host-experimental in Compose
    pub fn get_all_semantics_nodes(&self, merging_enabled: bool, skip_deactivated: bool) -> Vec<SemanticsNode>;
    pub fn hit_test(&self, position: Offset, f: impl FnMut(&LayoutInfo) -> bool) -> Option<LayoutInfo>;  // experimental
}
#[derive(Clone, Copy)]
pub struct SemanticsNode { tree: TreeKey, node: NodeId, merging_enabled: bool, fake: Option<FakeKind> }
impl SemanticsNode {
    pub fn id(&self) -> i32;
    pub fn config(&self) -> Rc<SemanticsConfiguration>;                  // merged when merging is enabled
    pub fn unmerged_config(&self) -> Rc<SemanticsConfiguration>;
    pub fn children(&self) -> Vec<SemanticsNode>;                        // z order (placement_order)
    pub fn parent(&self) -> Option<SemanticsNode>;
    pub fn is_root(&self) -> bool;
    pub fn bounds_in_root(&self) -> Rect;  pub fn bounds_in_window(&self) -> Rect;
    pub fn position_in_root(&self) -> Offset; pub fn position_in_window(&self) -> Offset; pub fn position_on_screen(&self) -> Offset;
    pub fn size(&self) -> IntSize;
    pub fn touch_bounds_in_root(&self) -> Rect;                          // minimum touch target
    pub fn layout_info(&self) -> LayoutInfo;                             // ledger 3.26
    pub fn is_merging_semantics_of_descendants(&self) -> bool;
    pub fn is_hidden(&self) -> bool;                                     // alpha 0 on the path
    pub fn is_important_for_accessibility(&self) -> bool;
    pub fn snapshot(&self) -> SemanticsSnapshot;                         // today's owned tree, for bridges and dumps
}
```

Queries go through the registry (modifier-nodes §3.3) outside passes. The merged configuration of a merging node is computed on access by merging its subtree (`mergeConfig`, `SemanticsNode.kt:450-490`), each key by its policy; `children` follow `getChildren` (`:576-597`): a clearing node has none, a merging node only its merging descendants, otherwise one layer of semantics nodes below in z order, deactivated and unplaced nodes left out, overlays as roots of their own (compose-rs's popups and dialogs, `semantics.rs:425-433`). Bounds use the live coordinates of the placement doc and the bounds-giving layer (`findCoordinatorToGetBounds`: the outermost merging modifier's, today's rule `semantics.rs:404-410`). Fake nodes (`emitFakeNodes`) are emitted only for `replaced_children`/accessibility, as Compose's. `SemanticsSnapshot` is today's owned `SemanticsNode` struct renamed (`id`, `bounds`, `rtl`, `config`, `children`), for code that wants a tree to walk without the registry.

- **Children in z order and transparency** end ui-input §5 divergences 14 and 15.
- **Traversal order**: `SemanticsNode::sorted_for_accessibility(&self) -> Vec<SemanticsNode>`, a port of `SemanticsSort.kt` (rows by vertical overlap, left to right or right to left, `traversal_index`, `is_traversal_group` subtrees), in Rust so it is tested headless; Android writes `traversal_before`/`traversal_after`, desktop orders AccessKit children by it.

## B.5 Consumers

**Test harness** (`crates/compose-test`):
- `Matcher` takes a `&SemanticsNode` (the handle) instead of a configuration, so tree matchers work: `has_parent`, `has_any_ancestor`, `has_any_child`, `has_any_descendant`, `has_any_sibling`, `is_root`. Key matchers: `expect_value(&KEY, value)`, `key_is_defined(&KEY)`, `key_not_defined(&KEY)`. The existing matchers (`has_text`, `has_role`, `is_focused`, …) are rewritten over keys with Compose's exact definitions (`hasText` over `Text` and `EditableText`, `isFocused` over `Focused`, `hasClickAction` over `OnClick`).
- Interactions: `perform_semantics_action(&KEY)` for `() -> bool` actions and `perform_semantics_action_with(&KEY, |action| action(args))` for the others (Compose's two overloads), `perform_custom_accessibility_action_with_label`; `fetch_semantics_node()` returns the handle; `on_root`, `on_children`, `on_parent`, `on_siblings`, `on_ancestors`, `fetch_semantics_nodes`. `SemanticsAction::…` uses (31 in the repository) move to keys.
- These are ui-text-and-test's `semantics-matchers` and harness features; this design gives them the keys and the handle.

**Android** (`crates/compose-android/src/accessibility.rs`, `Accessibility.kt`):
- **Record v2**: per node, the semantics id, parent, bounds (clipped, as today), then tagged fields (tag byte, payload) for what Kotlin sets on `AccessibilityNodeInfo`: the **class name** and **role description** chosen in Rust (today's Kotlin `when`, `Accessibility.kt:220-233`, moved, plus Compose's rules from `AndroidComposeViewAccessibilityDelegateCompat`), text, content description, hint, state description, error, pane title, heading, checkable/checked/selected, enabled, focusable/focused, editable, password, live region, collection info, range info, the **action list** (Android action ids with labels, custom actions with their own ids), traversal before/after, test tag as resource id when `testTagsAsResourceId`, text selection. Kotlin copies tags it knows and skips others, so the record grows without breaking the view.
- **Performing actions**: `performAction(host, id, action, int, float, string)` maps Android actions to keys in Rust (`accessibility::perform`), the scroll mapping of D478/D491/D496 included, and calls the action as §B.3.
- **Change events**: when a service is on, the bridge keeps the last snapshot and diffs per node (text, state description, selection, pane appearance, …) into Compose's change types, in Rust (`accessibility::changes(previous, current)`), replacing D630's one subtree change per frame (ui-input §5 divergence 16).
- These are the Rust half of ui-input's `android-accessibility-parity` (XL); the Kotlin half shrinks to copying fields.

**Desktop** (`crates/compose-desktop/src/accessibility.rs`): the AccessKit mapping reads keys instead of fields, orders children by `sorted_for_accessibility`, and offers the new actions AccessKit has (expand, collapse, set value, scroll, custom actions).

## B.6 Cost

| Path | Change | Estimate |
|---|---|---|
| Frames | Marking semantics dirty on invalidation (a flag) | ~0 |
| A semantics query (a test assertion, an accessibility refresh) | Cached configurations instead of re-applying every modifier; handles instead of a cloned snapshot tree | Cheaper after the first query; the first costs about today's build |
| Configuration storage | `SmallVec<[(KeyId, ValueSlot); 4]>`, inline values for common types; actions are `Rc`s kept by nodes | No allocation for nodes with up to four properties; merged configurations of merging nodes allocate when they spill |
| Node emission | `semantics_id` from a counter, only for `SEMANTICS` nodes | A counter increment |
| Android with accessibility on | Per-node diff instead of a subtree event | More work per changed frame (a snapshot diff), fewer events; only while a service is on |

## B.7 Migration

| Step | Content | Compose tests it makes portable |
|---|---|---|
| SE-1 | Keys, `ValueSlot`, typed `SemanticsConfiguration` with policies, the receiver API, `AccessibilityAction`/`CustomAccessibilityAction`, actions as closures; every component, both bridges and the harness moved over (no new behaviour) | `MergedSemanticsConfigurationTest`; the property cases of `SemanticsTests` (pane title, content type, action labels, text substitution, traversal properties); `CollectionInfoTest` (with foundation's lazy lists and `selectable_group` setting `CollectionInfo`); the harness's `expect_value` users across areas |
| SE-2 | `SemanticsModifierNode` flags, per-node cache with `Observation::Semantics` and `invalidate_semantics`, semantics ids renewed on reuse | `SemanticsModifierNodeTest` |
| SE-3 | `SemanticsOwner`, `SemanticsNode` handles, z-ordered children, bounds from live coordinates, transparency, fake nodes, touch bounds, the host-experimental queries | `SemanticsInfoTest`, `SemanticsHostApiTest`, the tree cases of `SemanticsTests` (z order, bounds in parent, ids, fake nodes) |
| SE-4 | Harness: matchers over handles, key matchers, tree navigation, generic actions | ui-text-and-test's `semantics-matchers` rows: `HasParentTest`, `HasAnyAncestorTest`, `HasAnyChildTest`, `HasAnyDescendantTest`, `HasAnySiblingTest`, `CallSemanticsActionTest`, `CustomAccessibilityActionsTest` |
| SE-5 | Traversal order (`SemanticsSort` port), Android record v2 with class names and actions in Rust, per-node change events, desktop ordering | `AndroidComposeViewAccessibilityTraversalTest`, the Rust-testable parts of `AndroidAccessibilityTest` and `AndroidComposeViewAccessibilityDelegateCompatTest` (class names, actions, traversal, transparency), `WindowContentChangeTest` |

## Order, dependencies and parallelism

- **Focus** needs modifier-nodes MN-3 (contexts, pass-end queue), MN-5 (delegation, for `focusable`/`clickable`/`FocusTargetNode`) and MN-6 (visitors); its rectangles and child-in-bounds requests want the placement doc's PL-2 (live coordinates) but can start from today's `layer_origin` rectangles. ui-input's `input-mode` before `SystemDefined`.
- **Semantics** SE-1 needs nothing new and can start now (it is a refactor of `semantics.rs`, the components' semantics blocks, the bridges and the harness). SE-2 needs MN-1 (kinds) and the observation-key change; SE-3 wants PL-2 for bounds.
- In `crates/compose-ui`, semantics (`semantics.rs`, `dispatch.rs`'s action path) and focus (`focus.rs`, `focus_manager.rs`, `focus_requester.rs`) touch disjoint files except `nodes.rs` (built-in nodes' semantics and focus targets) and `modifier.rs` (traits): SE-1 can run beside the modifier-node track; focus FO-1 should wait for MN-3 and MN-5.

## Decisions to record

Numbers to be assigned from the workstream's block.

- **Focus results are synchronous when the tree is free**, queued only from inside passes, revising D148 and D230; a requester on several nodes ends on the last, as Compose.
- **Focus listeners are notified by ancestor walks** (Compose's dispatch), replacing D85's whole-tree walk; `on_focus_changed` takes a `FocusState`.
- **Modal overlays are Compose's interaction barriers** for focus.
- **Semantics as typed keys with merge policies and inline values**, replacing the fixed struct and the action enum; actions are closures called outside the tree.
- **Semantics configurations are cached per node and computed lazily**; ids renewed on reuse.
- **`Modifier.semantics` keeps its eager block** (a known difference: its state reads recompose the caller).
- **Accessibility decisions move to Rust** (class names, actions, traversal, change events); the Kotlin side copies.

## Open questions for the user

1. Synchronous focus results (A.3.3) change D148/D230's contract that requests act "once the tree is free": accept, given that application code then gets Compose's return values?
2. `on_focus_changed` changes from a `bool` to Compose's `FocusState` (a breaking change for apps): accept now, or keep a `bool` convenience beside it?
3. Semantics keys as statics with an inline value enum (§B.3) instead of boxed `dyn Any` values: the enum is closed to application value types (they go through `Other(Rc<dyn Any>)`, one allocation). Accept?

## The orchestrator's answers to the open questions (2026-10-26)

Taken while the user was away, by D700's rule:

1. **Synchronous focus request results** when the tree is free (D148/D230 revised): accepted, as Compose's.
2. **`on_focus_changed` takes `FocusState`** outright, as Compose's `onFocusChanged`; no `bool` convenience.
3. **Semantics values** inline in a small enum, application types through one `Rc<dyn Any>` each: accepted.
4. **Decision block:** D1250–D1299.
