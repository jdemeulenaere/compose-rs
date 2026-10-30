# Differences from Compose

The known differences from Jetpack Compose that an app will run into, distilled from [docs/11](../11-status.md) and the [decisions log](../decisions.md) (D-numbers below). They're grouped by where you'll meet them. Some come from Rust and are there to stay; others are gaps that later work may close.

## The language

- **Call macros instead of named and default arguments.** A composable is called through its macro (`Text!(…)`), which takes positional, then named arguments, fills in defaults and takes a trailing lambda. The function itself takes every argument (D163, D164; [docs/03](../03-composable-macro.md#named-default-and-trailing-arguments-the-call-macro)). A parameter typed `impl Into<T>` converts what it's given; a plain `T` parameter takes only a `T` (D232).
- **Overloads have names of their own**, as Rust has no overloading:
  - a function with optional parameters is `<name>` (Compose's defaults) and `<name>_with` (all of them);
  - overloads differing by a type take a tuple or `impl Into` (`padding((16.dp(), 8.dp()))`);
  - the clickable family takes `ClickableOptions` (D263, D265);
  - composable overloads are named after what they add: `ClickableSurface`, `SelectableSurface`, `ClickableListItem`, `SelectableListItem`, `ToggleableListItem`, `AnnotatedText`, `IndeterminateCircularProgressIndicator` (D181);
  - lazy list forms are `items`, `items_keyed`, `items_list`, `items_indexed`, … (D174);
  - a composable extension of a type is a method taking every parameter: `transition.AnimatedVisibility(…)`, `transition.AnimatedContent(…)` (D812).
- **Struct update instead of named arguments of constructors.** `ColorScheme { primary, ..base }`, `ButtonColors { container_color, ..ButtonDefaults::button_colors() }`, `KeyboardOptions { ime_action, ..KeyboardOptions::DEFAULT }` (D180, D261). A default that depends on another parameter (an outlined button's border reading `enabled`) is an `Option` whose `None` is Compose's default.
- **Constants in Rust's case, arrangements typed by axis.** `Alignment::CENTER_VERTICALLY`, `Arrangement::SPACE_BETWEEN`. `Row` takes a horizontal arrangement only; mixing kinds in one expression needs `.into()`. There's no `Arrangement.Absolute` (D262).
- **Values aren't references.** Inside a composable's lambdas, the transform clones a capture where Rust would move it (D158, D159), but a composable's top-level code is plain Rust: a value given to two calls, or moved into a `LaunchedEffect`'s `async move` before a later use, needs `.clone()`. Lambda captures must be `Clone` (D115).
- **State holders are `Copy` handles** (D240, [docs/05](../05-state.md)):
  - `DrawerState`, `LazyListState`, `SnackbarHostState`, `CoroutineScope` and the other holders are copied, never cloned;
  - made outside a composition, a holder's `new` returns its owner (`Owned<H>`), which must be kept: `*DrawerState::new(…)` is a handle stale from the start;
  - a holder used after its owner went does nothing, with a warning in debug builds, but a `MutableState` used after its cell was freed panics.
- **Borrowed parameters make a composable non-restartable**, and callbacks can't take references (`impl Fn(&str)`): take owned values ([docs/03](../03-composable-macro.md#borrowed-parameters)).
- **Skipping compares cheaply, never deeply.** `Vec` isn't a `ComposeArg`; pass an `ImmutableList` (compared by identity), an `Rc`, or mark a short field `#[compose_arg(eq)]` (D160). There's no stability inference and no `@Stable`/`@Immutable`.
- **Content lambdas have no restart scope of their own.** State read in a content block recomposes the composable that invokes the block (usually a layout doing little), where Compose recomposes the lambda alone ([docs/03](../03-composable-macro.md#composable-lambdas-content)).
- **Root content is plain Rust.** The transform runs on `#[composable]` functions only: give `run_app`, `application` and `compose_test` a composable function, or a closure that only calls one. Calls, branches and lambdas written in the closure itself get no memoization and no groups.
- **Composables are free functions**: not methods, not `async`, no `&mut` parameters.

## Runtime and state

- **No snapshot isolation.** Writes are seen at once; there's no `Snapshot` API, no `snapshotFlow`, and no Flow integration (`collectAsState`): collect in a `LaunchedEffect` and write state ([docs/05](../05-state.md#snapshots)).
- **Coroutines are Rust futures** on a UI-thread executor, with `delay`, `with_frame_nanos` and cancellation by dropping. There are no dispatchers: blocking work goes through `spawn_background`, a thread pool taking a `Send` closure, and work already running isn't interrupted when its task is cancelled (D231). State handles are `!Send`: everything touching state runs on the UI thread.
- **No subcomposition.** Content Compose composes while measuring (lazy items, `BoxWithConstraints`) is composed for what the measurement asks, then the frame recomposes and measures again, up to three rounds. `BoxWithConstraints` usually composes once, from a hint of its constraints (D142); lazy lists compose ahead between frames (D156). There's no `SubcomposeLayout`.
- **`SaveableStateProvider` matches by position**, as Compose's (it's a `ReusableContent(key)`, D430): content with another key in its place reuses its composition, its values computed anew. Providers in a loop need a `key(…)` each to move with their keys, as in Compose.
- **`remember_saveable`** takes no `inputs` or `key`. A `SaveableStateProvider`'s values are found by their place within it, so content coming back under another parent finds them (D290); a restored `OwnedState` compares structurally whatever its policy was. The desktop host saves nothing, as a window is never recreated (D223).

## Layout

- **Intrinsics** are Compose's (D1200), except that the layouts Compose builds on `SubcomposeLayout` (lazy lists and grids, `BoxWithConstraints`) panic when asked, as Compose's, though compose-rs doesn't subcompose them; and a policy can't call the default intrinsics from its own (Kotlin's `super`).
- **No alignment lines** (`alignBy(FirstBaseline)`, `paddingFrom`): bars place their titles by measured heights (D186).
- **`LayoutCoordinates` is a snapshot** given to `on_globally_positioned` (size, position in the root and window, bounds), not a live object: no `localPositionOf`, no parent coordinates, and a graphics layer's property change alone doesn't report new coordinates (D172).
- **Window insets aren't consumed**: nested paddings of the same insets add up. Sides are physical (no `Start`/`End`); there's no `exclude`, no `WindowInsets(left, top, …)`, no insets animations nor `consumeWindowInsets` (D222). Material's `Scaffold` and bars take their insets as `PaddingValues`, zero by default (D183).
- **`Path`** is a shared reference (`Rc`), as Kotlin's: a clone is the same path, and it stays on its thread; its contents (`Path::data`, an `Arc<PathData>`) can go anywhere. On desktop `op` is computed by Skia when it's called, as Compose Desktop's; on Android the path keeps the operation for the replayer, which computes it, so `op` returns `true` and the path's definition carries its operands (D535).
- **Nested scrolling** finds the connections around a scrollable from the pointer's hit path rather than its layout parents, and has no `dispatcher` for custom scroll sources (`NestedScrollChain` is there for them). An outer scrollable takes the scroll an inner one leaves, but not the rest of its fling (D188).
- **The mouse wheel isn't animated.** A turn applies whole, in its event, where Compose's animates over up to 100 ms and keeps `isScrollInProgress` true for at least 50 ms (D879).

## Lazy lists

- No `animateItem` (placement and fade animations). `contentType` is given with the `_with` forms (`items_keyed_with(count, key, content_type, content)`, D432), and items reuse the composition and nodes of those that left as in Compose (D430), and a reused node's modifier nodes are reset, detached and attached again as Compose's (`on_reset`, `on_detach`, `on_attach`), after the pass's element updates rather than before them (D434): a custom node keeps its resources while its item is kept for reuse, giving them back in `on_release` once dropped (D1153).
- `LazyVerticalGrid` and `LazyHorizontalGrid` have no `reverse_layout` and no sticky headers. Their items are composed one by one and keep their state by key, as in Compose (D450); the content of the `items` forms gets no item scope (Compose's `LazyGridItemScope` only has `animateItem`), and the forms taking a span are named apart (`_with_span`, D454).
- A lazy list places only the items in view, as Compose (D470); a focus search leaving it lays out the next items beyond its bounds one at a time, as Compose's, but over frames: the moves wait for the next layout (D477). A popup in an item laid out beyond the bounds for a search shows where the item is placed while it is (D136).

## Drawing and graphics

- **No text in a `DrawScope`**: no `drawText`, no `TextMeasurer`/`rememberTextMeasurer`. No color filters or blend modes.
- **Ripples are static** state layers (8 % hovered, 10 % pressed or focused), not animated ripples. Material's components draw them; a plain `clickable` draws `LOCAL_INDICATION`'s (foundation's debug overlay unless provided), read where its node is, as Compose's (D1056); clickables take the focus in touch mode too (Compose's don't: there's no input mode yet).
- **Desktop renders on the CPU** (Skia's raster backend): well under a millisecond a frame up to about 1080p, slower at 4K. A GPU surface is planned (D93).
- **Graphics layers, clips and shadows are Compose's**: each `graphics_layer`, `clip`, `clip_to_bounds`, `alpha` and `shadow` is a layer (a render node on Android, its shadow the platform's elevation shadow, D522-D525). Desktop draws shadows with Skia's shadow utilities, lit as Compose Desktop's. No `rotation_x`/`rotation_y`, camera distance, render effects, color filters or blend modes on a layer, and shapes are rectangles and rounded rectangles (no generic outline). compose-rs's own optimizations beyond Compose's rendering are flags set per view (`ComposeRsView.optimizations`, `WindowOptions::optimizations`, D520): by default the `recommended` ones, which include `single_drawing_fade` (a layer holding one drawing fades its paint, as `ModulateAlpha`, where Compose's `Auto` composites offscreen: the same pixels, up to rounding) (D610); `none` draws exactly as Compose does.
- **Interactive elevations animate and recompose**: a button's or card's shadow follows its press, hover and focus as Compose's does, animated and read in composition (D526).

## Text and input

- **Text** lays out an `AnnotatedString`'s span styles: its paragraph styles, links (not clickable yet), text-to-speech annotations and bullets are kept but not laid out or drawn, and the measurers don't draw a style's shadow, baseline shift, geometric transform, font features, locales, draw style or indent (docs/11). `TextLayoutResult` has Compose's queries (`get_line_for_offset`, `get_bounding_box`, …, D1105), through one paragraph for the whole text (`multi_paragraph`), with byte offsets. Text fields take a `String` value and have no `on_text_layout`, and there's no `TextFieldState` or `TextFieldValue` overload.
- **Focus requests are asynchronous**: `FocusManager` and `FocusRequester` requests apply after the current event or at the next frame, so `move_focus` says whether a tree was there to ask, not whether the focus moved. Arrow keys don't move the focus, as on desktop Compose (D230).
- **Semantics** have no custom actions or pane titles, so a bottom sheet's drag handle offers no expand, collapse or dismiss actions to accessibility services.
- **Content descriptions of icons and images are `&'static str`** (`Icon!(icon, Some("Search"))`): a description built at run time goes through `Modifier.semantics` ([Accessibility](kotlin-to-rust.md#accessibility-and-semantics)).
- **Line heights** spread as Compose's `LineHeightStyle` in its `Fixed` mode only: there are no `Tight` and `Minimum` modes, and no `includeFontPadding` (a style without a line height style takes `LineHeightStyle::DEFAULT`, as Compose's without font padding, D571). On the desktop, Skia centers or scales the extra height; other alignments are proportional there.
- **Fonts are the system's**: `FontFamily` names a system family (`SansSerif`, `Serif`, `Monospace`, `Cursive`, `Named("…")`); custom font files can't be loaded.

## Navigation and platform

- **A smaller Navigation Compose** ([Navigation](kotlin-to-rust.md#navigation)). Destinations are values of the app's own type (an enum), not routes, compared with `==`: `launch_single_top` keeps the top entry only when it's equal, where Compose compares routes and replaces the arguments. `remember_nav_controller` takes the start destination, and the start destination is never popped. There are no nested graphs, deep links, dialog destinations, per-entry `ViewModel`s or lifecycles, nor `saveState`/`restoreState`; `NavHost` has no transition by default (Compose fades) and the same one both ways (D291).
- **No predictive back** (`PredictiveBackHandler`, back progress). `BackHandler` takes back on Android and Escape on desktop; dialogs, sheets and menus take it first (D220, D221).
- **Embedding goes one way.** A `ComposeRsView` can be inside a Kotlin UI, but compose-rs content can't host Android views or Kotlin composables (no `AndroidView` in Rust). Kotlin and Rust exchange properties and text events (D226).
- **Overlays stay in the view.** Popups, dialogs, menus, bottom sheets and tooltips are nodes of the app's one tree, drawn over its content (D130), not windows of their own: in a `ComposeRsView` that's part of a Kotlin screen, they're confined to the view's bounds, and a dialog's scrim covers the view, not the screen.
- **Few platform locals.** `LOCAL_URI_HANDLER`, `LOCAL_CLIPBOARD_MANAGER` (plain text) and `LOCAL_HAPTIC_FEEDBACK` are Compose's; there's no `LocalContext`, `LocalLifecycleOwner` or `ViewModel`. Send Kotlin an event (`LOCAL_HOST_EVENTS`) to share, start an activity or ask for a permission, and pass what Rust needs back as properties (`LOCAL_HOST_PROPS`) ([Android interop](kotlin-to-rust.md#android-interop)).
- **Links are checked.** The platform's `UriHandler` opens only `http`, `https`, `mailto` and `tel` URIs, refusing others with a warning, where Compose's opens any; `PlatformUriHandler::current().with_schemes(…)` widens it for a subtree (D283).
- **No Android resources.** There are no `R.string`, `R.drawable` or resource qualifiers, so no localization through resources: bundle images with `include_bytes!`, and pass localized strings from Kotlin as properties or keep them in Rust.
- **Logging is the `log` crate's** (`compose::log::info!`): logcat on Android, standard error on desktop; `println!` goes nowhere on Android ([Logging](kotlin-to-rust.md#logging)).
- **Android**: arm64 only, API 29 and later.
- **Desktop windows** don't inherit the application's composition locals, and have no position, placement (maximized, minimized, fullscreen), icon, menu bar, tray or `DialogWindow`; a `Window` can't be composed inside another window's content.
- **No iOS or web.**
- **No tooling**: no `@Preview`, Layout Inspector or Live Edit, and no hot reload. Tests' dumps (`dump_tree`, `dump_semantics`, `dump_display_list`) and `COMPOSE_SCREENSHOT` stand in for inspection; [docs/08](../08-fast-builds.md) keeps rebuilds short.

## Animation and Material

- **Material 3 lacks**, among others:
  - dismissible and permanent navigation drawers, `BottomSheetScaffold` and standard bottom sheets;
  - input chips, date and time pickers, search bars, exposed dropdown menus, multi-choice segmented buttons;
  - the content overloads of `ExtendedFloatingActionButton` and `BottomAppBar`, and the bottom app bar's scroll behavior.
- **The theme** is three composition locals (`LOCAL_COLOR_SCHEME`, `LOCAL_TYPOGRAPHY`, `LOCAL_SHAPES`), not one: `MaterialTheme::values()` stands for `MaterialTheme.LocalMaterialTheme.current`. The color scheme, the typography and the shapes are read as `Rc`s, compared by value, standing for Kotlin's shared objects (D1021, D1106, D1107). `Typography(fontFamily, …)` is `Typography::from_font_family(family)` with struct update, then `with_default_font_family(family)` for the styles given without one (D1008).
- **Progress indicators** take their progress as a lambda, as Compose's non-deprecated overloads (the `Float` ones are gone); `drawStopIndicator` is a `DrawStopIndicator` value, as a composable's callbacks take their arguments by value (D1003).
- **Checkboxes**' stroke overload is two optional parameters, `checkmark_stroke` and `outline_stroke`; `CheckboxDefaults.colors(checkedColor, …)` is `CheckboxDefaults::colors_with` (D1009).
- **Sliders** take a `SliderState`, as Compose's non-deprecated overload (the value overload is gone); `SliderDefaults::track_with` stands for `Track`'s parameters past the colors, its `drawStopIndicator` and `drawTick` `DrawSliderStopIndicator` and `DrawSliderTick` values. No `RangeSlider` or `VerticalSlider` yet (D1006).
- **Top app bars' scroll behaviors** have no fling decay before they settle, and dragging the bar itself doesn't collapse it (D186).
- **A `SeekableTransitionState` call can end a frame before Compose's**, as a task waiting for a frame resumes before that frame's composition (D821, D817; a runtime parity item). One interrupted by another cancels its task, as Compose's `CancellationException` does (D823).
- **Icons** are material-icons-core's filled set (D187).
- **No dynamic color**: there's no `dynamicLightColorScheme`/`dynamicDarkColorScheme` reading the wallpaper's colors; pass them from Kotlin as properties if you need them.
- **Missing foundation pieces**: no `HorizontalPager`/`VerticalPager`, no pull to refresh, no `movableContentOf`, no staggered grids (`LazyVerticalStaggeredGrid`), no `GridCells.FixedSize`.
