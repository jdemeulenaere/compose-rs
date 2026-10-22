# Differences from Compose

The known differences from Jetpack Compose that an app will run into, distilled from [docs/11](../11-status.md) and the [decisions log](../decisions.md) (D-numbers below). They're grouped by where you'll meet them. Some come from Rust and are there to stay; others are gaps that later work may close.

## The language

- **Call macros instead of named and default arguments.** A composable is called through its macro (`Text!(…)`), which takes positional, then named arguments, fills in defaults and takes a trailing lambda. The function itself takes every argument (D163, D164; [docs/03](../03-composable-macro.md#named-default-and-trailing-arguments-the-call-macro)). A parameter typed `impl Into<T>` converts what it's given; a plain `T` parameter takes only a `T` (D232).
- **Overloads have names of their own**, as Rust has no overloading:
  - a function with optional parameters is `<name>` (Compose's defaults) and `<name>_with` (all of them);
  - overloads differing by a type take a tuple or `impl Into` (`padding((16.dp(), 8.dp()))`);
  - the clickable family takes `ClickableOptions` (D263, D265);
  - composable overloads are named after what they add: `ClickableSurface`, `SelectableSurface`, `AnnotatedText`, `IndeterminateCircularProgressIndicator` (D181);
  - lazy list forms are `items`, `items_keyed`, `items_list`, `items_indexed`, … (D174).
- **Struct update instead of named arguments of constructors.** `ColorScheme { primary, ..base }`, `ButtonColors { container_color, ..ButtonDefaults::button_colors() }`, `KeyboardOptions { ime_action, ..KeyboardOptions::DEFAULT }` (D180, D261). A default that depends on another parameter (an outlined button's border reading `enabled`) is an `Option` whose `None` is Compose's default.
- **Constants in Rust's case, arrangements typed by axis.** `Alignment::CENTER_VERTICALLY`, `Arrangement::SPACE_BETWEEN`. `Row` takes a horizontal arrangement only; mixing kinds in one expression needs `.into()`. There's no `Arrangement.Absolute` (D262). A reversed lazy list's default arrangement stays `TOP` (Compose's becomes `Bottom`): pass `Arrangement::BOTTOM` (D171).
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

- **Writing state during composition panics** (Compose allows it with caveats). `remember_updated_state` is a plain cell rather than state: reading it subscribes nothing.
- **No snapshot isolation.** Writes are seen at once; there's no `Snapshot` API, no `snapshotFlow`, and no Flow integration (`collectAsState`): collect in a `LaunchedEffect` and write state ([docs/05](../05-state.md#snapshots)).
- **Coroutines are Rust futures** on a UI-thread executor, with `delay`, `with_frame_nanos` and cancellation by dropping. There are no dispatchers: blocking work goes through `spawn_background`, a thread pool taking a `Send` closure, and work already running isn't interrupted when its task is cancelled (D231). State handles are `!Send`: everything touching state runs on the UI thread.
- **No subcomposition.** Content Compose composes while measuring (lazy items, `BoxWithConstraints`) is composed for what the measurement asks, then the frame recomposes and measures again, up to three rounds. `BoxWithConstraints` usually composes once, from a hint of its constraints (D142); lazy lists compose ahead between frames (D156). There's no `SubcomposeLayout`.
- **`remember_saveable`** takes no `inputs` or `key`. Values in lazy items scrolled out of the composition aren't kept, and there's no `rememberSaveableStateHolder`; a restored `OwnedState` compares structurally whatever its policy was. The desktop host saves nothing, as a window is never recreated (D223).

## Layout

- **Intrinsics are approximated** by measuring the child once to ask (`max_intrinsic_width`, `max_intrinsic_height`); there are no min intrinsics or per-layout formulas, so `IntrinsicSize.Min` rows can differ (D137).
- **No alignment lines** (`alignBy(FirstBaseline)`, `paddingFrom`): bars place their titles by measured heights (D186).
- **`LayoutCoordinates` is a snapshot** given to `on_globally_positioned` (size, position in the root and window, bounds), not a live object: no `localPositionOf`, no parent coordinates, and a graphics layer's property change alone doesn't report new coordinates (D172).
- **Window insets aren't consumed**: nested paddings of the same insets add up. Sides are physical (no `Start`/`End`); there's no `exclude`, no `WindowInsets(left, top, …)`, no insets animations nor `consumeWindowInsets` (D222). Material's `Scaffold` and bars take their insets as `PaddingValues`, zero by default (D183).
- **Nested scrolling** finds the connections around a scrollable from the pointer's hit path rather than its layout parents, and has no `dispatcher` for custom scroll sources (`NestedScrollChain` is there for them). An outer scrollable takes the scroll an inner one leaves, but not the rest of its fling (D188).

## Lazy lists

- No `animateItem` (placement and fade animations) and no `contentType`: items are reused by key only.
- `LazyVerticalGrid` has no `reverse_layout` or `user_scroll_enabled`, and keeps an item's state by its row rather than its key, so items inserted before others move state between rows (D144).
- A popup in a lazy item composed beyond the viewport shows where the item is placed (D136).

## Drawing and graphics

- **No text in a `DrawScope`**: no `drawText`, no `TextMeasurer`/`rememberTextMeasurer`. No color filters or blend modes, and no `Brush` on `border`.
- **Ripples are static** state layers (8 % hovered, 10 % pressed or focused), not animated ripples. Interactive elevations snap to their value instead of animating (D184).
- **Desktop renders on the CPU** (Skia's raster backend): well under a millisecond a frame up to about 1080p, slower at 4K. A GPU surface is planned (D93).
- **Android shadows are approximated** with a blurred outline rather than the platform's elevation shadows (D152).

## Text and input

- **Text** has no link annotations and no paragraph styles in spans. `TextLayoutResult` is a snapshot of the layout's metrics, without Compose's queries (`getLineForOffset`, `getBoundingBox`) (D233). Text fields take a `String` value and have no `on_text_layout`, and there's no `TextFieldState` or `TextFieldValue` overload.
- **Focus requests are asynchronous**: `FocusManager` and `FocusRequester` requests apply after the current event or at the next frame, so `move_focus` says whether a tree was there to ask, not whether the focus moved. Arrow keys don't move the focus, as on desktop Compose (D230).
- **Semantics** have no custom actions or pane titles, so a bottom sheet's drag handle offers no expand, collapse or dismiss actions to accessibility services.
- **Content descriptions of icons and images are `&'static str`** (`Icon!(icon, Some("Search"))`): a description built at run time goes through `Modifier.semantics` ([Accessibility](kotlin-to-rust.md#accessibility-and-semantics)).
- **Fonts are the system's**: `FontFamily` names a system family (`SansSerif`, `Serif`, `Monospace`, `Cursive`, `Named("…")`); custom font files can't be loaded.

## Navigation and platform

- **No Navigation Compose.** There's no `NavHost`: keep a back stack in a saved list and `match` on its top ([Navigation](kotlin-to-rust.md#navigation)). Screens under the top aren't composed, so their state goes when another is pushed.
- **No predictive back** (`PredictiveBackHandler`, back progress). `BackHandler` takes back on Android and Escape on desktop; dialogs, sheets and menus take it first (D220, D221).
- **Embedding goes one way.** A `ComposeRsView` can be inside a Kotlin UI, but compose-rs content can't host Android views or Kotlin composables (no `AndroidView` in Rust). Kotlin and Rust exchange properties and text events (D226).
- **Overlays stay in the view.** Popups, dialogs, menus, bottom sheets and tooltips are nodes of the app's one tree, drawn over its content (D130), not windows of their own: in a `ComposeRsView` that's part of a Kotlin screen, they're confined to the view's bounds, and a dialog's scrim covers the view, not the screen.
- **No platform locals.** There's no `LocalContext`, `LocalUriHandler`, `LocalClipboardManager`, `LocalHapticFeedback`, `LocalLifecycleOwner` or `ViewModel`. Send Kotlin an event (`LOCAL_HOST_EVENTS`) to open a link, vibrate or start an activity, and pass what Rust needs back as properties (`LOCAL_HOST_PROPS`). Text fields use the system clipboard themselves.
- **No Android resources.** There are no `R.string`, `R.drawable` or resource qualifiers, so no localization through resources: bundle images with `include_bytes!`, and pass localized strings from Kotlin as properties or keep them in Rust.
- **No logging API.** On Android, `println!` goes nowhere, and compose-rs's own debug warnings print to standard error in debug builds only, which Android's build never makes. Log through the `log` facade with an Android backend (the `android_logger` crate) ([Logging](kotlin-to-rust.md#logging-on-android)).
- **Android**: arm64 only, API 29 and later.
- **Desktop windows** don't inherit the application's composition locals, and have no position, placement (maximized, minimized, fullscreen), icon, menu bar, tray or `DialogWindow`; a `Window` can't be composed inside another window's content.
- **No iOS or web.**
- **No tooling**: no `@Preview`, Layout Inspector or Live Edit, and no hot reload. Tests' dumps (`dump_tree`, `dump_semantics`, `dump_display_list`) and `COMPOSE_SCREENSHOT` stand in for inspection; [docs/08](../08-fast-builds.md) keeps rebuilds short.

## Animation and Material

- **Transitions** have no child transitions; `AnimatedContent` keeps at most three states' contents while they animate.
- **Material 3 lacks**, among others:
  - dismissible and permanent navigation drawers, `BottomSheetScaffold` and standard bottom sheets;
  - input chips, date and time pickers, search bars, exposed dropdown menus, multi-choice segmented buttons;
  - the toggleable `Surface` overload, the content overloads of `ExtendedFloatingActionButton` and `BottomAppBar`, and the bottom app bar's scroll behavior.
- **Top app bars' scroll behaviors** have no fling decay before they settle, and dragging the bar itself doesn't collapse it (D186).
- **Icons** are material-icons-core's filled set (D187).
- **No dynamic color**: there's no `dynamicLightColorScheme`/`dynamicDarkColorScheme` reading the wallpaper's colors; pass them from Kotlin as properties if you need them.
- **Missing foundation pieces**: no `HorizontalPager`/`VerticalPager`, no pull to refresh, no `movableContentOf`, no `LazyHorizontalGrid` or staggered grids.
