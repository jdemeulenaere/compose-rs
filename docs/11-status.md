# 11. Status and known differences

What compose-rs covers of Jetpack Compose today, and where it knowingly differs. The design documents ([02](02-architecture.md) to [09](09-testing.md)) describe how each part works; the decisions log ([decisions.md](decisions.md)) records why.

## Platforms

| | Status |
|---|---|
| **Desktop** (macOS, Windows, Linux) | Runs: `application` with `Window`s (or one window with `run_app`), Skia rendering on the CPU raster backend presented with softbuffer, SkParagraph text, input with IME, hover and pointer icons, the system clipboard, AccessKit for screen readers, HiDPI, right-to-left. |
| **Android** (arm64, API 29+) | Runs: `ComposeRsView`, a plain `View` embeddable in Kotlin Compose (`AndroidView`), drawing with the system `Canvas` (hardware-accelerated: the GPU), text by `StaticLayout`, touch, hardware keys, the soft keyboard, the clipboard ([07](07-platforms.md)). `android/` builds the library and the demo app with Gradle; cargo builds the Rust side. Accessibility through an `AccessibilityNodeProvider`; shadows are approximated (a blurred outline); [12](12-android-performance.md) measures it against Jetpack Compose. |
| **iOS, web** | Not started. |

## Covered

- **Runtime:** the slot table, restart scopes and skipping, `remember`, `key`, state (`mutable_state_of`, `derived_state_of`, state lists), `CompositionLocal`, `SideEffect`, `DisposableEffect`, `LaunchedEffect`, `produce_state`, `remember_updated_state`, coroutine scopes on a UI-thread executor (`delay`, `with_frame_nanos`), and a manual clock for tests.
- **Layout:** `Layout` and measure policies, `Box`, `BoxWithConstraints`, `Row`, `Column`, `Spacer`, arrangements and alignments, weights, size and padding modifiers (`aspect_ratio`, `wrap_content_*`, `width_in`, `height_in`, `default_min_size`, `Modifier.layout`), `z_index`, `on_size_changed` and `on_globally_positioned`, scrolling, `LazyColumn` and `LazyRow` (lists of items, keys, `fill_parent_max_*`, `reverse_layout`, `user_scroll_enabled`, sticky headers), `LazyVerticalGrid`, scrollbars, right-to-left layout.
- **Drawing:** shapes, borders, clips, graphics layers, paths and `ImageVector`, images, arcs, circles and ovals, stroke caps, shadows, `Canvas`, `draw_behind`, `draw_with_content`, `draw_with_cache`, scoped transforms and clips (`rotate`, `scale`, `translate`, `inset`, `clip_rect`, `clip_path`), brushes (linear, radial and sweep gradients: `brush_paint`, `background_brush`).
- **Text:** `BasicText`, `SelectionContainer` and `DisableSelection`, `BasicTextField` (selection, IME, undo, Compose Desktop's key mappings), `TextStyle` (weight, style, family, letter spacing, decoration, alignment, line height), overflow, `AnnotatedString` spans.
- **Input:** clicks and toggles, `combined_clickable` (double and long clicks), `selectable` and `selectable_group`, `hoverable`, `FocusRequester`, keyboard options and actions, password fields, pointer events with passes, hover, pointer icons, `pointer_input` with Compose's gesture detectors, `draggable`, keys and focus (Tab order, focus requests, `bring_into_view`), semantics with actions.
- **Windows:** `Popup`, `Dialog`, `TooltipArea`.
- **Animation:** `Animatable`, tweens, springs, keyframes, repeatables, `animate_*_as_state`, infinite transitions, `AnimatedVisibility`, `Crossfade`, `AnimatedContent`, `update_transition`, `animate_content_size`.
- **Material 3:** the theme (colors, typography, shapes, tonal elevation), `Text`, `Surface`, buttons (filled, tonal, elevated, outlined, text, icon, FAB), icons, `Checkbox`, `Switch`, `RadioButton`, `Slider`, `TextField` and `OutlinedTextField`, cards, dividers, `ListItem`, `TopAppBar`, `Scaffold`, `NavigationBar`, `ModalNavigationDrawer` (with `ModalDrawerSheet` and `NavigationDrawerItem`), `TabRow`, `AlertDialog`, `DropdownMenu`, `Snackbar`, progress indicators, `PlainTooltip`, chips and badges.
- **Testing:** `ComposeTestRule` with Compose's finders, actions and assertions, event scripts with transcripts, layout and display-list dumps.

## Known differences

- **No subcomposition.** Content that Compose composes while measuring (lazy items, `BoxWithConstraints`) is composed for what the measurement asks, then the frame recomposes and measures again (up to three rounds). `BoxWithConstraints` first composes for a hint of its constraints, so it usually composes once (D142). A lazy list that scrolled composes and measures its next items between frames (prefetch, D156), so the frames reaching them usually compose nothing.
- **Intrinsics** are approximated by measuring a child once to ask (`max_intrinsic_width`, `max_intrinsic_height`), where Compose has each layout's own formulas (D137).
- **State writes during composition** panic; `remember_updated_state` is a plain cell rather than state.
- **`combined_clickable`** has no interaction source parameter; `LayoutCoordinates` is a snapshot handed to `on_globally_positioned`, not a live object (no `localPositionOf`, no parent coordinates), and a graphics layer's property change alone (no layout) doesn't report new coordinates (D172).
- **Ripples** are static state layers (8 % hovered, 10 % pressed or focused), not animated ripples; a `Switch`'s is on its touch target rather than around its thumb.
- **Rendering** is on the CPU: fast at window sizes up to about 1080p (well under a millisecond to render a screen), slower at 4K. A GPU surface is planned (D93).
- **Drawing** has no `drawText` in a `DrawScope` (nor `TextMeasurer`/`rememberTextMeasurer`: drawing has no access to the platform's text measurer, and paragraphs drawn there would need a per-node cache), no color filters or blend modes, and no `Brush` on `border`. **On Android**, gradients paint as their average color and `clip_path` doesn't clip yet (D175, D176).
- **Windows** don't inherit the application's composition locals, and have no position, placement (maximized, minimized, fullscreen), icon, menu bar, tray, or `DialogWindow`; a `Window` can't be composed inside another window's content.
- **Lazy lists** have no `animate_item` (item placement and fade animations), no `content_type` (items are reused by key only); a reversed list's default arrangement stays `TOP` (Compose's becomes `Bottom`, D171); `LazyVerticalGrid` has no `reverse_layout` or `user_scroll_enabled` yet. **Grids** keep an item's state by its row rather than its key (D144); a popup in a lazy item composed beyond the viewport shows (D136).
- **Text** has no link annotations, no paragraph styles in spans, and text fields have no prefix, suffix, `max_lines` or `min_lines`.
- **Animation** transitions have no child transitions; `AnimatedContent` keeps at most three states' contents while they animate.
- **Material** lacks, among others, dismissible and permanent navigation drawers, navigation rails, bottom sheets, input chips, date and time pickers, segmented buttons and search bars.
- **Saving state** across process death (`rememberSaveable`) has no desktop counterpart and isn't provided.
