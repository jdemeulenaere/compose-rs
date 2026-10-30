# From Kotlin to Rust

Jetpack Compose code next to its compose-rs equivalent. Each Rust sample is quoted from [`examples/guide`](../../examples/guide), where it compiles and has a test. The Kotlin is the usual Compose code for the same thing.

- [Names](#names)
- [Composables](#composables)
- [State](#state)
- [Side effects](#side-effects)
- [Layouts](#layouts)
- [Drawing](#drawing)
- [Theming and Material 3](#theming-and-material-3)
- [Text and text fields](#text-and-text-fields)
- [Dialogs](#dialogs)
- [Selectable lists](#selectable-lists)
- [Accessibility and semantics](#accessibility-and-semantics)
- [Gestures](#gestures)
- [Images](#images)
- [Animation](#animation)
- [Navigation](#navigation)
- [Android interop](#android-interop)
- [Logging](#logging)
- [Testing](#testing)

## Names

compose-rs keeps Compose's names, parameter names, parameter order and defaults, written in Rust's case conventions:

| Kotlin | compose-rs |
|---|---|
| `androidx.compose.runtime`, `.ui`, `.foundation`, `.foundation.layout`, `.material3`, `.animation`; `androidx.navigation.compose` | `compose::runtime`, `compose::ui`, `compose::foundation`, `compose::foundation::layout`, `compose::material3`, `compose::animation`; `compose::navigation` |
| `@Composable fun Greeting(…)` | `#[composable] fn Greeting(…)`, called `Greeting!(…)` |
| `rememberLazyListState()`, `mutableStateOf(0)` | `remember_lazy_list_state!()`, `mutable_state_of(0)` |
| `Modifier.fillMaxWidth().padding(16.dp)` | `Modifier.fill_max_width().padding(16.dp())` |
| `Alignment.CenterVertically`, `Arrangement.SpaceBetween`, `Color.Red` | `Alignment::CENTER_VERTICALLY`, `Arrangement::SPACE_BETWEEN`, `Color::RED` |
| `MaterialTheme.colorScheme.primary` | `MaterialTheme::color_scheme().primary` |
| `LocalDensity.current` | `LOCAL_DENSITY.current()` |
| `16.dp`, `14.sp`, `1.5.em` | `16.dp()`, `14.sp()`, `1.5.em()` |

A file usually starts with a wildcard import of each module it uses, as the samples do:

- `use compose::runtime::*;` (`remember`, state, effects, `#[composable]`),
- `use compose::ui::*;` (`Modifier`, units, colors, drawing),
- `use compose::foundation::*;` and `use compose::foundation::layout::*;` (lazy lists, scrolling, text fields; `Row`, `Column`, `Box`),
- `use compose::material3::*;`, `use compose::animation::*;` and `use compose::navigation::*;` as needed.

Each crate's `platform` module holds what hosts and toolkits use; apps don't need it, and it isn't part of the stable surface.

## Composables

```kotlin
/** Greets [name]. */
@Composable
fun Greeting(name: String, modifier: Modifier = Modifier, excited: Boolean = false) {
    Text("Hello, $name${if (excited) "!" else "."}", modifier = modifier)
}
```

<!-- snippet: src/composables.rs#greeting -->
```rust
/// Greets `name`: a documented composable's rustdoc shows its call form and defaults.
#[composable]
pub fn Greeting(
    name: impl Into<SharedString>,
    #[default(Modifier)] modifier: Modifier,
    #[default(false)] excited: bool,
) {
    let mark = if excited { "!" } else { "." };
    Text!("Hello, {}{}", name, mark, modifier = modifier);
}
```

A composable is a free function marked `#[composable]`. Default values are `#[default(…)]` attributes, evaluated where the composable is called.

A parameter typed `impl Into<T>` takes anything that converts: `"Ada"` for a `SharedString`, the reference-counted string compose-rs uses for text. A plain `SharedString` parameter takes only a `SharedString`. An `Option<impl Into<T>>` parameter, as Compose's `contentDescription: String?`, takes `None`, `Some` of anything that converts (`Some("Close")`, `Some(format!("{count} new"))`) or an `Option` of it.

`#[composable]` turns `impl Into<T>` and `impl Fn(…)` parameters into concrete types, so the composable stays a single non-generic function.

### Calls

Rust has no named or default arguments, so `#[composable]` also generates a call macro with the composable's name. The macro binds arguments positionally, then by name, fills in the defaults, and accepts a trailing lambda:

```kotlin
Column {
    Greeting("Ada")
    Greeting("Grace", excited = true)
    Greeting(name = "Linus", modifier = Modifier.padding(8.dp))
}
```

<!-- snippet: src/composables.rs#calls -->
```rust
#[composable]
pub fn Greetings() {
    Column!({
        Greeting!("Ada"); // the defaults for the rest
        Greeting!("Grace", excited = true); // positional, then named
        Greeting!(name = "Linus", modifier = Modifier.padding(8.dp()));
        // The function itself, every argument given: what the call macro expands to.
        Greeting(SharedString::from("Barbara"), Modifier, false);
    });
}
```

`use` a composable and its macro comes with it (`use compose::material3::*` imports `Text` and `Text!`). A composable whose first parameter is text, as `Text`, takes format arguments: `Text!("Count: {}", count)` and `Text!("{count} unread")`. A documented composable's rustdoc ends with its call form, which lists the defaults:

```text
Button!(on_click, modifier = Modifier, enabled = true, …, { content })
```

### Content lambdas

A composable lambda parameter is `impl Fn() + 'static`. A final block in the call is the trailing lambda, and a block given by name is a lambda too:

```kotlin
@Composable
fun Section(title: String, content: @Composable () -> Unit) {
    Column(Modifier.padding(8.dp)) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        content()
    }
}

@Composable
fun Fruit() {
    Section("Fruit") {
        Text("Apple")
        Text("Pear")
    }
    Section(title = "Vegetables", content = { Text("Leek") })
}
```

<!-- snippet: src/composables.rs#content -->
```rust
/// A titled section: `content` is a composable lambda, as Kotlin's `content: @Composable () -> Unit`.
#[composable]
pub fn Section(title: impl Into<SharedString>, content: impl Fn() + 'static) {
    Column!(modifier = Modifier.padding(8.dp()), {
        Text!(title, style = MaterialTheme::typography().title_medium.clone());
        content();
    });
}

#[composable]
pub fn Fruit() {
    // A final block is the trailing lambda.
    Section!("Fruit", {
        Text!("Apple");
        Text!("Pear");
    });
    // A slot given by name takes a block too.
    Section!(
        title = "Vegetables",
        content = {
            Text!("Leek");
        }
    );
}
```

Kotlin's receiver scopes (`RowScope`, `BoxScope`, `LazyListScope`) are arguments of the lambda: `Row!(|row| { … Modifier.weight(row, 1.0) … })`. A block where the scope isn't needed ignores it.

### What composables can't do

- **Methods and `async`.** A composable is a free function: not a method, not `async` (launch work with `LaunchedEffect`).
- **Borrowed parameters.** A composable keeps its arguments to compare them on the next pass, and Rust can't keep a borrow. A parameter with a reference type (`&str`, `&Todo`) makes the composable non-restartable: it can't skip and runs again with its caller (Compose's `@NonRestartableComposable`). Pass owned or shared values (`SharedString`, `Rc<Todo>`) instead.
- **Uncomparable parameters.** Every parameter of a restartable composable implements `ComposeArg`, the cheap comparison that decides skipping ([State](#data-models-and-skipping)); the compiler says which one doesn't.
- **Values returned.** A composable may return a value (`remember_*` functions do), which makes it non-restartable, as in Compose.

## State

```kotlin
@Composable
fun Counter() {
    var count by remember { mutableStateOf(0) }
    Button(onClick = { count++ }) {
        Text("Clicked $count times")
    }
}
```

<!-- snippet: src/state.rs#counter -->
```rust
#[composable]
pub fn Counter() {
    // The slot keeps the state; `count` is a Copy handle, captured by `move` closures.
    let count = remember(|| mutable_state_of(0));
    Button!(on_click = move || count.update(|c| c + 1), {
        Text!("Clicked {} times", count.get());
    });
}
```

`remember(|| mutable_state_of(0))` keeps the state in the composition and returns a `MutableState<i32>`, a `Copy` handle: closures capture it with `move` and no cloning. Its methods:

- `get()` reads a copy, and subscribes the reading scope;
- `with(|v| …)` reads by reference, for a `String` or a `Vec` that shouldn't be cloned;
- `set(v)` writes, recomposing the readers unless the value is equal;
- `update(|v| …)` writes from the old value;
- `edit(|v| …)` changes it in place, and always recomposes the readers.

Writing state while composing panics: write from handlers, effects and tasks.

`mutable_state_list_of()` and `mutable_state_list_from(items)` are `mutableStateListOf`, with `push`, `insert`, `remove`, `get`, `len`, `with(|list| …)` and `edit(|list| …)`; `mutable_state_map_of` and `mutable_state_set_of` are `mutableStateMapOf` and `mutableStateSetOf`.

### `rememberSaveable`

```kotlin
var text by rememberSaveable { mutableStateOf("") }
TextField(text, onValueChange = { text = it })
```

<!-- snippet: src/state.rs#saveable -->
```rust
#[composable]
pub fn Draft() {
    // Saved with the Android view's instance state: kept across the activity's recreation.
    let text = remember_saveable(|| mutable_state_of(String::new()));
    TextField!(text.get(), move |value| text.set(value), modifier = Modifier.test_tag("draft"));
}
```

Saved values implement `Saveable`: numbers, `bool`, `char`, `String`, `Option`s, `Vec`s and tuples of them. An app's own types derive it (`#[derive(Saveable)]`), implement it ([Navigation](#navigation) saves an enum), or go through a `saver(save, restore)` with `remember_saveable_with`. The state holders' `remember_*` functions (`remember_lazy_list_state!()`, `remember_scroll_state!()`) save their positions, as Compose's do.

### `derivedStateOf`

```kotlin
val listState = rememberLazyListState()
val scrolled by remember { derivedStateOf { listState.firstVisibleItemIndex > 0 } }
```

<!-- snippet: src/state.rs#derived -->
```rust
#[composable]
pub fn LongList() {
    let list = remember_lazy_list_state!();
    let scope = remember_coroutine_scope();
    // Recomposes its readers when the answer changes, not on every scrolled pixel.
    let scrolled = remember(move || derived_state_of(move || list.first_visible_item_index() > 0));
    Box!(modifier = Modifier.fill_max_size(), {
        LazyColumn!(modifier = Modifier.fill_max_size(), state = list, |scope: LazyListScope| {
            scope.items(1_000, |index| Text!("Item {}", index));
        });
        if scrolled.get() {
            SmallFloatingActionButton!(
                on_click = move || {
                    scope.launch(async move {
                        let _ = list.scroll_to_item(0, 0).await;
                    });
                },
                {
                    Icon!(icons::filled::keyboard_arrow_up(), Some("Back to the top"));
                }
            );
        }
    });
}
```

The calculation's reads subscribe the derived state, not the composable; the composable that reads `scrolled.get()` recomposes only when the result changes.

### State hoisting

Hoisting works as in Compose: a stateless composable takes the value and a callback, and its caller owns the state.

```kotlin
@Composable
fun Stepper(value: Int, onValueChange: (Int) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        TextButton(onClick = { onValueChange(value - 1) }) { Text("-") }
        Text("$value")
        TextButton(onClick = { onValueChange(value + 1) }) { Text("+") }
    }
}

@Composable
fun Quantity() {
    var quantity by rememberSaveable { mutableStateOf(1) }
    Stepper(quantity, onValueChange = { quantity = it.coerceAtLeast(0) })
}
```

<!-- snippet: src/state.rs#hoisting -->
```rust
/// Stateless: the value comes in, changes go out.
#[composable]
pub fn Stepper(value: i32, on_value_change: impl Fn(i32) + 'static) {
    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
        TextButton!(on_click = move || on_value_change(value - 1), {
            Text!("-");
        });
        Text!("{}", value);
        TextButton!(on_click = move || on_value_change(value + 1), {
            Text!("+");
        });
    });
}

/// Stateful: owns the state, hoisted out of `Stepper`.
#[composable]
pub fn Quantity() {
    let quantity = remember_saveable(|| mutable_state_of(1));
    Stepper!(quantity.get(), move |value: i32| quantity.set(value.max(0)));
}
```

A callback parameter is `impl Fn(A) + 'static`, and arguments are passed by value (`impl Fn(String)`, as `(String) -> Unit`); a callback taking a reference isn't supported.

### State holders are `Copy` handles

`DrawerState`, `SheetState`, `SnackbarHostState`, `TopAppBarState`, `SwipeToDismissBoxState`, `LazyListState`, `LazyGridState`, `ScrollState`, `CoroutineScope` and `FocusRequester` are `Copy` handles, as state is, so a handler or a task captures them as Kotlin captures a reference:

```kotlin
val drawerState = rememberDrawerState(DrawerValue.Closed)
val scope = rememberCoroutineScope()
ModalNavigationDrawer(drawerContent = { ModalDrawerSheet { Text("Settings") } }, drawerState = drawerState) {
    Button(onClick = { scope.launch { drawerState.open() } }) { Text("Open the menu") }
}
```

<!-- snippet: src/state.rs#holders -->
```rust
#[composable]
pub fn Menu() {
    let drawer = remember_drawer_state!(DrawerValue::Closed); // a Copy handle, as state
    let scope = remember_coroutine_scope(); // Copy too
    ModalNavigationDrawer!(
        {
            ModalDrawerSheet!({
                Text!("Settings", modifier = Modifier.padding(16.dp()));
            });
        },
        drawer_state = drawer,
        {
            Button!(
                on_click = move || {
                    scope.launch(async move { drawer.open().await });
                },
                {
                    Text!("Open the menu");
                }
            );
        }
    );
}
```

Outside a composition (a test, an app's model), `DrawerState::new(…)` and the other holders' constructors return the holder's owner, `Owned<DrawerState>`, which frees the holder when dropped. Keep the owner, and copy the handle out of it (`*owner`) for closures.

`let state = *DrawerState::new(…);` compiles but drops the owner at once: the handle then does nothing (with a warning in debug builds). `FocusRequester` is the exception: it owns nothing, and `FocusRequester::new()` returns the handle itself. See [D240](../decisions/runtime.md#d240).

### Values a lambda would move

Kotlin passes references, so a value can be used by several lambdas and by the code after them. In Rust, a closure that uses a value by itself (`Text!(name)`) moves it. Inside the lambdas of a composable, the transform clones such captures instead (D158): a capture given by itself to a function, a method or a call macro, or named in a `move` closure or an `async move` block inside the lambda.

```kotlin
@Composable
fun Profile(name: String, onOpen: (String) -> Unit) {
    Card {
        Text(name)
        TextButton(onClick = { onOpen(name) }) { Text("Open") }
    }
    Text(name)
}
```

<!-- snippet: src/state.rs#clone_on_use -->
```rust
#[composable]
pub fn Profile(name: SharedString, on_open: impl Fn(SharedString) + 'static) {
    Card!({
        Text!(name); // in a lambda, a capture given by itself is cloned
        TextButton!(on_click = move || on_open(name), {
            Text!("Open");
        });
    });
    // Top-level code is plain Rust: this moves `name`, so it's its last use.
    Text!(name);
}
```

Only the lambda's captures are cloned, not values the lambda makes itself: a local declared inside a lambda moves into a closure as in plain Rust ([Lazy lists](#lazy-lists) clones one for that reason). At a composable's top level the code is plain Rust too: a value given to a call or moved into a `LaunchedEffect`'s `async move` block is moved, so use it last or `.clone()` it. Captures must be `Clone`; wrap data that isn't in an `Rc`.

### Data models and skipping

A composable skips when every argument is the same as last time. "The same" is `ComposeArg::same`, which is always cheap: shared data (`Rc`, state handles, callbacks) compares by identity, as a JVM reference does, and small values (numbers, short strings, small enums and structs) by value. Collections are never compared element by element, so `Vec` isn't a `ComposeArg`.

```kotlin
@Immutable
data class Contact(val id: Long, val name: String, val tags: List<String>)

@Composable
fun Contacts(contacts: ImmutableList<Contact>) {
    Column {
        for (contact in contacts) {
            key(contact.id) { ContactRow(contact) }
        }
    }
}
```

<!-- snippet: src/state.rs#model -->
```rust
/// A contact: `#[derive(ComposeArg)]` compares field by field (numbers and short strings by
/// value, `Rc`s and state handles by identity).
#[derive(Clone, PartialEq, ComposeArg)]
pub struct Contact {
    pub id: u64,
    pub name: SharedString,
    /// A short list, compared by value (`PartialEq`).
    #[compose_arg(eq)]
    pub tags: Vec<String>,
}

/// Skips when given the same list (by identity): keep the list in state, don't rebuild it.
#[composable]
pub fn Contacts(contacts: ImmutableList<Contact>) {
    Column!({
        for contact in contacts.iter() {
            key(contact.id, || ContactRow!(contact.clone()));
        }
    });
}

/// Skips when its contact is the same.
#[composable]
pub fn ContactRow(contact: Contact) {
    Text!("{} ({})", contact.name, contact.tags.join(", "));
}

#[composable]
pub fn ContactsScreen() {
    let contacts = remember(|| {
        mutable_state_of(ImmutableList::from(vec![
            Contact { id: 1, name: "Ada".into(), tags: vec!["math".into()] },
            Contact { id: 2, name: "Grace".into(), tags: vec!["navy".into(), "cobol".into()] },
        ]))
    });
    Contacts!(contacts.get());
}
```

- `#[derive(ComposeArg)]` compares a struct field by field. `#[compose_arg(eq)]` compares a field with `PartialEq`, for a collection the model knows to be short.
- `ImmutableList<T>` (an `Rc<[T]>`, as kotlinx's `ImmutableList`) compares by identity: a list rebuilt with the same contents is a change, so keep lists in state or `remember` them rather than rebuilding them each pass. Large data goes in an `Rc`.
- State can live inside data: a `Todo` with an `OwnedState<bool>` field, in an `Rc<Todo>`, recomposes only its own row when toggled ([examples/todo](../../examples/todo/src/lib.rs)).

## Side effects

### `LaunchedEffect`

```kotlin
@Composable
fun Countdown(from: Int) {
    var left by remember { mutableStateOf(from) }
    LaunchedEffect(from) {
        left = from
        while (left > 0) {
            delay(1.seconds)
            left--
        }
    }
    Text("$left s")
}
```

<!-- snippet: src/effects.rs#launched -->
```rust
#[composable]
pub fn Countdown(from: u32) {
    let left = remember(|| mutable_state_of(from));
    // Restarted (the old task cancelled) whenever `from` changes; cancelled when this leaves.
    LaunchedEffect(from, async move {
        left.set(from);
        while left.get() > 0 {
            delay(Duration::from_secs(1)).await;
            left.update(|seconds| seconds - 1);
        }
    });
    Text!("{} s", left.get());
}
```

The block is a Rust future, run as a task on the UI thread's executor. It suspends at each `.await`: on `delay`, on `with_frame_nanos` (the next frame), on `spawn_background`, or on any other future. Keys are any `ComposeArg` (a tuple for several, `()` for `Unit`). Cancelling the task drops the future, so it stops at the `.await` it was waiting on.

### `DisposableEffect`

```kotlin
DisposableEffect(topic) {
    val id = bus.subscribe(topic)
    onDispose { bus.unsubscribe(id) }
}
```

<!-- snippet: src/effects.rs#disposable -->
```rust
#[composable]
pub fn Subscription(topic: SharedString, bus: Rc<Bus>) {
    // Subscribes after the first composition, and again (unsubscribing first) for a new topic.
    DisposableEffect(topic.clone(), move |scope| {
        let id = bus.subscribe(&topic);
        scope.on_dispose(move || bus.unsubscribe(id))
    });
}
```

### `SideEffect`

```kotlin
Text(screen)
SideEffect { analytics.screen = screen }
```

<!-- snippet: src/effects.rs#side_effect -->
```rust
#[composable]
pub fn Tracked(screen: SharedString, analytics: Rc<Analytics>) {
    Text!(screen.clone());
    // After every composition of this scope that completes.
    SideEffect(move || *analytics.screen.borrow_mut() = screen.to_string());
}
```

`SideEffect(key1, key2) { … }` is `KeyedSideEffect((key1, key2), move || …)`: it runs only when the keys changed.

`SideEffect`, `KeyedSideEffect`, `DisposableEffect` and `LaunchedEffect` take plain closures and futures, not lambdas the transform clones into: what they use is moved, so clone what's used after them.

### `rememberCoroutineScope`

```kotlin
val scope = rememberCoroutineScope()
val snackbars = remember { SnackbarHostState() }
Button(onClick = { scope.launch { snackbars.showSnackbar("Saved") } }) { Text("Save") }
```

<!-- snippet: src/effects.rs#scope -->
```rust
#[composable]
pub fn Saver() {
    let scope = remember_coroutine_scope();
    let snackbars = remember(SnackbarHostState::new);
    Column!({
        Button!(
            on_click = move || {
                // Launched from a handler; cancelled when `Saver` leaves the composition.
                scope.launch(async move {
                    snackbars.show_snackbar("Saved").await;
                });
            },
            {
                Text!("Save");
            }
        );
        SnackbarHost!(snackbars);
    });
}
```

`produce_state(initial, key, |scope| async move { … })` and `remember_updated_state(value)` are `produceState` and `rememberUpdatedState`: `scope.set(…)` writes the state, `scope.await_dispose(|| …).await` is `awaitDispose`, and `produce_state_with(initial, key, policy, …)` takes a mutation policy.

### Blocking work: `spawn_background`

There's no `withContext(Dispatchers.IO)`. A task awaits `spawn_background(work)`, which runs a `Send` closure on a pool of threads and resumes the task on the UI thread with its result. State handles aren't `Send`: read what the work needs before, and write its result after.

```kotlin
var size by remember { mutableStateOf<Long?>(null) }
LaunchedEffect(path) {
    size = withContext(Dispatchers.IO) { File(path).length() }
}
```

<!-- snippet: src/effects.rs#background -->
```rust
#[composable]
pub fn FileSize(path: String) {
    let size = remember(|| mutable_state_of(None::<u64>));
    LaunchedEffect(path.clone(), async move {
        // Blocking work runs on a thread pool (`Dispatchers.IO`); the closure is `Send`, so
        // it takes plain data, not state handles. The task resumes on the UI thread.
        let bytes = spawn_background(move || std::fs::metadata(path).ok().map(|file| file.len()));
        size.set(bytes.await);
    });
    match size.get() {
        Some(bytes) => Text!("{} bytes", bytes),
        None => IndeterminateCircularProgressIndicator!(),
    }
}
```

Cancelling the task drops the result; blocking work already running isn't interrupted.

An async client built on tokio (an HTTP client, a database driver) needs a tokio runtime: build one at startup on threads of its own, and keep its `Handle`. A task on the UI thread then awaits its work in one of two ways:

- `handle.spawn(async move { client.get(url).send().await })` returns a tokio `JoinHandle`, which is a future any executor can await: `let response = join.await;` in a `LaunchedEffect`. A `tokio::sync::oneshot` receiver works the same way. The executor's wakers work from any thread, so the runtime's threads wake the UI task.
- `spawn_background(move || handle.block_on(request))` runs it on the background pool, which holds a pool thread while the request runs.

Either way, what comes back must be `Send`, and the UI task writes it to state.

## Layouts

### `Row`, `Column`, `Box`

```kotlin
@Composable
fun Author(name: String, online: Boolean) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(40.dp), contentAlignment = Alignment.BottomEnd) {
            Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.primary, CircleShape))
            if (online) Box(Modifier.size(10.dp).background(Color.Green, CircleShape))
        }
        Text(name, Modifier.weight(1f))
        TextButton(onClick = {}) { Text("Follow") }
    }
}
```

<!-- snippet: src/layouts.rs#row_column_box -->
```rust
#[composable]
pub fn Author(name: SharedString, online: bool) {
    Row!(
        modifier = Modifier.fill_max_width().padding((16.dp(), 8.dp())),
        horizontal_arrangement = Arrangement::spaced_by(12.dp()),
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        |row| {
            Box!(modifier = Modifier.size(40.dp()), content_alignment = Alignment::BOTTOM_END, {
                Box!(
                    modifier = Modifier
                        .fill_max_size()
                        .background_with(MaterialTheme::color_scheme().primary, CIRCLE_SHAPE)
                );
                if online {
                    Box!(
                        modifier =
                            Modifier.size(10.dp()).background_with(Color::GREEN, CIRCLE_SHAPE)
                    );
                }
            });
            // Receiver-scoped modifiers take the scope: Kotlin's `Modifier.weight(1f)` in a `RowScope`.
            Text!(name, modifier = Modifier.weight(row, 1.0));
            TextButton!(on_click = || {}, {
                Text!("Follow");
            });
        }
    );
}
```

`Spacer!(modifier = Modifier.height(8.dp()))`, `BoxWithConstraints`, `Modifier.vertical_scroll(remember_scroll_state!())` and `LazyVerticalGrid` are there too.

### Modifiers

`Modifier` is a value; its methods chain as Kotlin's extension functions do. The names follow one rule (D263, D265):

- **One method per Compose name.** Overloads that differ by an argument's type take `impl Into<…>`, with a tuple standing for several arguments: `padding(8.dp())`, `padding((16.dp(), 8.dp()))` (horizontal, vertical), `padding((start, top, end, bottom))`, `size(48.dp())`, `size((48.dp(), 24.dp()))`.
- **Optional parameters**: the short method takes Compose's defaults for them, and `<name>_with` takes all of them, in Compose's order: `fill_max_width()` and `fill_max_width_with(0.5)`, `background(color)` and `background_with(color, shape)`, `border(width, color)` and `border_with(width, color, shape)`, `shadow(elevation)` and `shadow_with(…)`.
- **The clickable family** (`clickable`, `toggleable`, `tri_state_toggleable`, `selectable`, `combined_clickable`) takes its optional parameters (`enabled`, `role`, `on_click_label`, `indication`, `interaction_source`, and `combined_clickable`'s `on_long_click_label` and `haptic_feedback_enabled`) as a `ClickableOptions` struct, built by struct update from `ClickableOptions::DEFAULT`; its `indication` defaults to `Indication::LOCAL` (`LocalIndication.current`), and `Indication::NONE` is Kotlin's `indication = null`.

```kotlin
Modifier.padding(horizontal = 16.dp, vertical = 8.dp)
Modifier.fillMaxWidth(0.5f)
Modifier.background(Color.Red, RoundedCornerShape(8.dp))
Modifier.border(1.dp, Color.Black, RoundedCornerShape(8.dp))
Modifier.clickable(enabled = !selected, role = Role.Button, onClick = onClick)
```

<!-- snippet: src/layouts.rs#modifiers -->
```rust
#[composable]
pub fn ModifierForms(selected: bool, on_click: impl Fn() + 'static) {
    let shape = RoundedCornerShape(8.dp());
    Column!({
        // One method per Compose name; a tuple for an overload taking several values.
        Box!(modifier = Modifier.padding(8.dp())); // padding(all)
        Box!(modifier = Modifier.padding((16.dp(), 8.dp()))); // padding(horizontal, vertical)
        Box!(modifier = Modifier.padding((1.dp(), 2.dp(), 3.dp(), 4.dp()))); // start, top, end, bottom
        Box!(modifier = Modifier.size(48.dp()).size((48.dp(), 24.dp()))); // size(size), size(width, height)
        // Compose's optional parameters: the short form takes their defaults, `_with` takes them all.
        Box!(modifier = Modifier.fill_max_width_with(0.5)); // fillMaxWidth(fraction = 0.5f)
        Box!(modifier = Modifier.background(Color::RED)); // background(color)
        Box!(modifier = Modifier.background_with(Color::RED, shape)); // background(color, shape)
        Box!(modifier = Modifier.border_with(1.dp(), Color::BLACK, shape));
        // The clickable family's optional parameters are a struct.
        Box!(
            modifier = Modifier.size(48.dp()).clickable_with(
                ClickableOptions {
                    enabled: !selected,
                    role: Some(Role::Button),
                    ..ClickableOptions::DEFAULT
                },
                on_click,
            )
        );
    });
}
```

Order matters as in Compose: each modifier wraps the ones after it.

Lambdas given to a modifier's callback methods (`clickable`, `on_size_changed`, `on_focus_changed`, …) are memoized like a composable's callback arguments when the chain starts at `Modifier`, a `modifier` parameter or a local modifier, so the chain stays the same across passes. Drawing and layout lambdas (`draw_behind`, `layout`, `graphics_layer`) aren't: a pass that runs the call again gives the node a new lambda ([Performance](performance.md#what-to-avoid)).

### Arrangements and alignments

`Arrangement` and `Alignment` are namespaces of constants in Rust's constant case. Arrangements are typed by axis, as Compose's: `Arrangement::START` is horizontal, `Arrangement::TOP` vertical, and `CENTER`, `SPACE_BETWEEN`, `SPACE_EVENLY`, `SPACE_AROUND` and `spaced_by(…)` either. `spaced_by` and `aligned` take a tuple for their two-argument overloads.

```kotlin
Row(horizontalArrangement = if (compact) Arrangement.End else Arrangement.spacedBy(8.dp)) { … }
Column(
    verticalArrangement = Arrangement.spacedBy(4.dp, Alignment.Bottom),
    horizontalAlignment = Alignment.CenterHorizontally,
) { … }
```

<!-- snippet: src/layouts.rs#arrangements -->
```rust
#[composable]
pub fn Toolbar(compact: bool) {
    Row!(
        modifier = Modifier.fill_max_width(),
        // Compose's `Arrangement.SpaceBetween`, in Rust's constant case.
        horizontal_arrangement = Arrangement::SPACE_BETWEEN,
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        {
            Text!("Inbox");
            Row!(
                // `END` is horizontal only, `spaced_by` either way: `.into()` makes them one type.
                horizontal_arrangement =
                    if compact { Arrangement::END } else { Arrangement::spaced_by(8.dp()).into() },
                {
                    Text!("Edit");
                    Text!("Share");
                }
            );
        }
    );
    Column!(
        vertical_arrangement = Arrangement::spaced_by((4.dp(), Alignment::BOTTOM)),
        horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
        {
            Text!("One");
            Text!("Two");
        }
    );
}
```

A custom arrangement implements `HorizontalArrangementPolicy` or `VerticalArrangementPolicy` and is wrapped with `HorizontalArrangement::custom(policy)`.

### Lazy lists

```kotlin
LazyColumn(
    Modifier.fillMaxSize(),
    state = state,
    contentPadding = PaddingValues(8.dp),
    verticalArrangement = Arrangement.spacedBy(4.dp),
) {
    for (day in days) {
        stickyHeader(key = day.title, contentType = "day") {
            Text(day.title, Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface))
        }
        items(day.messages, key = { it.id }, contentType = { "message" }) { message ->
            Text(message.text)
        }
    }
}
```

<!-- snippet: src/layouts.rs#lazy -->
```rust
#[derive(Clone, PartialEq)]
pub struct Message {
    pub id: u64,
    pub text: SharedString,
}

#[derive(Clone, PartialEq)]
pub struct Day {
    pub title: SharedString,
    pub messages: ImmutableList<Message>,
}

#[composable]
pub fn Messages(days: ImmutableList<Day>, state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.fill_max_size(),
        state = state,
        content_padding = PaddingValues::from(8.dp()),
        vertical_arrangement = Arrangement::spaced_by(4.dp()),
        |scope: LazyListScope| {
            for day in days.iter() {
                // `title` is made here, not captured: it moves into the closure as in plain
                // Rust (D158 clones only the lambda's captures), hence the clone for the key.
                let title = day.title.clone();
                // Content types, as Compose's `contentType`: an item reuses the composition and
                // nodes of one of its type that left (D430).
                scope.sticky_header_keyed_with(title.clone(), "day", move |_item| {
                    Text!(
                        title.clone(),
                        modifier = Modifier
                            .fill_max_width()
                            .background(MaterialTheme::color_scheme().surface)
                    );
                });
                // Keys keep an item's state (and the scroll position) when items move.
                scope.items_list_keyed_with(
                    day.messages.clone(),
                    |message| message.id,
                    |_message| "message",
                    |_item, message| {
                        Text!(&message.text);
                    },
                );
            }
        }
    );
}
```

The content is a closure taking the `LazyListScope`. Its methods are named apart because Rust has no overloads (D174), and each has a `_with` form taking Compose's `contentType` after the key (D432):

| Kotlin | compose-rs |
|---|---|
| `item { … }`, `item(key) { … }` | `scope.item(…)`, `scope.item_keyed(key, …)` |
| `items(count) { index -> … }`, with `key = { index -> … }` | `scope.items(count, …)`, `scope.items_keyed(count, key, …)` |
| `items(list) { item -> … }`, with `key = { item -> … }` | `scope.items_list(list, \|item_scope, item\| …)`, `scope.items_list_keyed(list, key, …)` |
| `itemsIndexed(list) { index, item -> … }` | `scope.items_indexed(list, …)`, `scope.items_indexed_keyed(list, key, …)` |
| `stickyHeader { … }` | `scope.sticky_header(…)`, `scope.sticky_header_keyed(key, …)` |
| `contentType = …` (on any of them) | `scope.item_with(content_type, …)`, `scope.items_keyed_with(count, key, content_type, …)`, … |
| `Modifier.fillParentMaxWidth()` | `Modifier.fill_parent_max_width(&item_scope)` |

The list forms take anything that converts to an `ImmutableList` (a `Vec`, a slice, an `ImmutableList`, shared without copying). Keys are any `Hash + PartialEq` value; debug builds panic on a duplicate key, as Compose throws. A content type is any `Hash` value: as in Compose, an item coming into view reuses the composition and nodes of an item of its content type that left (those without one, Compose's `null`, reuse each other's), its remembered values computed anew ([Performance](performance.md)). Items have no `animateItem` ([Differences](differences.md#lazy-lists)).

### Lazy grids

```kotlin
LazyVerticalGrid(
    columns = GridCells.Adaptive(minSize = 96.dp),
    modifier = Modifier.fillMaxSize(),
    verticalArrangement = Arrangement.spacedBy(4.dp),
    horizontalArrangement = Arrangement.spacedBy(4.dp),
) {
    for (album in albums) {
        item(key = album.title, span = { GridItemSpan(maxLineSpan) }, contentType = "title") {
            Text(album.title)
        }
        items(album.photos, key = { it.id }, contentType = { "photo" }) { photo ->
            Box(Modifier.aspectRatio(1f).background(photo.color))
        }
    }
}
```

<!-- snippet: src/layouts.rs#grid -->
```rust
#[derive(Clone, PartialEq)]
pub struct Photo {
    pub id: u64,
    pub color: Color,
}

#[derive(Clone, PartialEq)]
pub struct Album {
    pub title: SharedString,
    pub photos: ImmutableList<Photo>,
}

#[composable]
pub fn Albums(albums: ImmutableList<Album>) {
    LazyVerticalGrid!(
        GridCells::Adaptive(96.dp()),
        modifier = Modifier.fill_max_size(),
        vertical_arrangement = Arrangement::spaced_by(4.dp()),
        horizontal_arrangement = Arrangement::spaced_by(4.dp()),
        |scope: LazyGridScope| {
            for album in albums.iter() {
                let title = album.title.clone();
                // A title over a whole line, as `span = { GridItemSpan(maxLineSpan) }`.
                scope.item_keyed_with_span(
                    title.clone(),
                    |line| GridItemSpan(line.max_line_span()),
                    "title",
                    move || {
                        Text!(title.clone());
                    },
                );
                // A content type alone (`_with`): each photo takes one cell, the default.
                scope.items_list_keyed_with(
                    album.photos.clone(),
                    |photo| photo.id,
                    |_| "photo",
                    |photo| {
                        Box!(modifier = Modifier.aspect_ratio(1.0).background(photo.color));
                    },
                );
            }
        }
    );
}
```

`LazyVerticalGrid` and `LazyHorizontalGrid` take a `LazyGridScope` with the list's forms (`item`, `items`, `items_keyed`, `items_list`, `items_indexed`, …), whose `_with` forms take Compose's `contentType` after the key, as a list's (`items_keyed_with(count, key, content_type, content)`), and whose `_with_span` forms take Compose's `span` and `contentType`, in Compose's order (`items_keyed_with_span(count, key, span, content_type, content)`, D454). A span is a closure returning a `GridItemSpan` from the `LazyGridItemSpanScope` (`max_line_span()`, `max_current_line_span()`) and the item: `|_, _| GridItemSpan(1)` is Compose's default. Give spans only where items take more than a cell: with spans, which items a line holds is found by asking the spans from the first item, again each time the grid's content changes. The content gets no item scope (Compose's `LazyGridItemScope` only has `animateItem`). Each item is composed on its own and reused by content type, as in a lazy list (D450).

### Custom layouts

```kotlin
@Composable
fun Wrap(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Layout(content, modifier) { measurables, constraints ->
        // measure, then layout(width, height) { place }
    }
}
```

<!-- snippet: src/layouts.rs#custom_layout -->
```rust
/// Its children left to right, wrapping to a new line when one doesn't fit: a minimal FlowRow.
#[composable]
pub fn Wrap(#[default(Modifier)] modifier: Modifier, content: impl Fn() + 'static) {
    // Remembered: a new policy each pass would be a changed argument, so `Layout` couldn't skip.
    let policy = remember(|| {
        measure_policy(|scope, measurables, constraints| {
            let (mut x, mut y, mut line_height, mut width) = (0, 0, 0, 0);
            for &measurable in measurables {
                let placeable = scope.measure(measurable, constraints.loosen());
                if x > 0 && x + placeable.width > constraints.max_width {
                    (x, y, line_height) = (0, y + line_height, 0);
                }
                scope.place(&placeable, x, y);
                x += placeable.width;
                width = width.max(x);
                line_height = line_height.max(placeable.height);
            }
            scope.layout(width, y + line_height)
        })
    });
    Layout!(modifier = modifier, measure_policy = policy, content = content);
}
```

`measure_policy(closure)` makes a policy of a closure, as Kotlin's lambda converts to `MeasurePolicy`. Measuring and placing go through the scope:

- `scope.measure(measurable, constraints)` returns a `Placeable`;
- `scope.place(&placeable, x, y)` places it (`place_relative` mirrors it in right-to-left layouts);
- `scope.layout(width, height)` returns the result, after the placements rather than around them;
- `scope.density()` converts `Dp` to pixels.

Intrinsics are methods of the policy with Compose's defaults, and the scope asks a child's: Kotlin's `measurable.maxIntrinsicWidth(height)` is `scope.max_intrinsic_width(measurable, height)`, and `Modifier.width(IntrinsicSize.Max)` is `Modifier.width(IntrinsicSize::Max)`:

<!-- snippet: src/layouts.rs#intrinsics -->
```rust
/// Its first child in a square as wide as the child's maximum intrinsic width: that width is
/// its minimum intrinsic width too.
pub struct Square;

impl MeasurePolicy for Square {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let side = scope.max_intrinsic_width(measurables[0], constraints.max_height);
        let placeable = scope.measure(measurables[0], Constraints::fixed(side, side));
        scope.place(&placeable, 0, 0);
        scope.layout(side, side)
    }

    fn min_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        scope.max_intrinsic_width(measurables[0], height)
    }
}
```

### `Modifier.layout`

A Kotlin extension function on `Modifier` is a trait implemented for `Modifier` (as foundation's `ScrollModifiers` and `LazyItemModifiers` are):

```kotlin
fun Modifier.indent(level: Int) = layout { measurable, constraints ->
    val indent = 16.dp.roundToPx() * level
    val placeable = measurable.measure(constraints.offset(horizontal = -indent))
    layout(placeable.width + indent, placeable.height) { placeable.place(indent, 0) }
}
```

<!-- snippet: src/layouts.rs#layout_modifier -->
```rust
/// `Modifier.indent(level)`, as a Kotlin extension function: a trait on `Modifier`.
pub trait IndentModifier {
    fn indent(self, level: u32) -> Modifier;
}

impl IndentModifier for Modifier {
    fn indent(self, level: u32) -> Modifier {
        self.layout(move |scope, measurable, constraints| {
            let indent = scope.density().round_to_px(16.dp()) * level as i32;
            let inner =
                Constraints { max_width: (constraints.max_width - indent).max(0), ..constraints };
            let placeable = scope.measure(measurable, inner.loosen());
            scope.place(&placeable, indent, 0);
            scope.layout(placeable.width + indent, placeable.height)
        })
    }
}
```

For a modifier with state of its own, implement `ModifierNodeElement` (Compose's `ModifierNodeElement<N>`: its `type Node`, `create`, `update`, and `same` for `equals`) and `ModifierNode` (Compose's `Modifier.Node`), and add it with `Modifier.then_custom(element)`; there's no `Modifier.composed`.

## Drawing

### `Canvas`

```kotlin
@Composable
fun Gauge(fraction: Float) {
    val track = MaterialTheme.colorScheme.surfaceVariant
    val progress = MaterialTheme.colorScheme.primary
    Canvas(Modifier.size(96.dp)) {
        val stroke = 8.dp.toPx()
        val style = Stroke(stroke, cap = StrokeCap.Round)
        val inset = Size(size.width - stroke, size.height - stroke)
        drawArc(track, 135f, 270f, false, Offset(stroke / 2, stroke / 2), inset, style = style)
        drawArc(progress, 135f, 270f * fraction, false, Offset(stroke / 2, stroke / 2), inset, style = style)
    }
}
```

<!-- snippet: src/drawing.rs#canvas -->
```rust
#[composable]
pub fn Gauge(fraction: f32) {
    let track = MaterialTheme::color_scheme().surface_variant;
    let progress = MaterialTheme::color_scheme().primary;
    Canvas(Modifier.size(96.dp()).test_tag("gauge"), move |scope| {
        let stroke = scope.density().to_px(8.dp());
        let size = scope.size();
        let oval = Rect::new(
            stroke / 2.0,
            stroke / 2.0,
            size.width - stroke / 2.0,
            size.height - stroke / 2.0,
        );
        scope.draw_arc(oval, 135.0, 270.0, Paint::stroke_with(track, stroke, StrokeCap::Round));
        let sweep = 270.0 * fraction.clamp(0.0, 1.0);
        scope.draw_arc(oval, 135.0, sweep, Paint::stroke_with(progress, stroke, StrokeCap::Round));
    });
}
```

`Canvas` is a plain function, called without `!`: its lambda takes a borrowed `DrawScope`, which a composable's parameters can't name.

- `DrawScope`'s calls take the geometry first and a `Paint` last, where Compose takes the color or brush first: `Paint::fill(color)`, `Paint::stroke(color, width)`, `Paint::stroke_with(color, width, cap)`, and `.with_alpha(a)` on either.
- Transforms and clips are scoped by a closure: `scope.rotate(degrees, pivot, |scope| …)`, `translate`, `scale`, `inset`, `clip_rect`, `clip_path`.
- There's no `drawText` ([Differences](differences.md#drawing-and-graphics)).

### `drawBehind` and `drawWithContent`

```kotlin
Text(label, Modifier.drawBehind { drawRoundRect(color, cornerRadius = CornerRadius(size.height / 2)) }.padding(8.dp, 2.dp))

Box(Modifier.drawWithContent {
    drawContent()
    drawRect(Color.Black.copy(alpha = 0.3f))
}) { content() }
```

<!-- snippet: src/drawing.rs#draw_behind -->
```rust
#[composable]
pub fn Badge(label: SharedString) {
    let color = MaterialTheme::color_scheme().error_container;
    Text!(
        label,
        modifier = Modifier
            .draw_behind(move |scope| {
                let radius = scope.size().height / 2.0;
                scope.draw_round_rect(scope.bounds(), [radius; 4], Paint::fill(color));
            })
            .padding((8.dp(), 2.dp()))
    );
}

#[composable]
pub fn Dimmed(content: impl Fn() + 'static) {
    Box!(
        modifier = Modifier.draw_with_content(|scope| {
            scope.draw_content(); // the content first, then a scrim over it
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(Color::BLACK.with_alpha(0.3)));
        }),
        {
            content();
        }
    );
}
```

### `drawWithCache`

```kotlin
Box(Modifier.size(120.dp, 48.dp).drawWithCache {
    val path = Path().apply {
        moveTo(0f, 0f)
        lineTo(size.width, 0f)
        lineTo(size.width - size.height / 2, size.height / 2)
        lineTo(size.width, size.height)
        lineTo(0f, size.height)
        close()
    }
    val brush = Brush.horizontalGradient(listOf(Color(0xFF6750A4), Color(0xFF7D5260)))
    onDrawBehind { drawPath(path, brush) }
})
```

<!-- snippet: src/drawing.rs#draw_with_cache -->
```rust
#[composable]
pub fn Ribbon() {
    Box!(
        modifier = Modifier.size((120.dp(), 48.dp())).draw_with_cache(|cache| {
            // Runs again only for a new size (or density, direction, or state it read).
            let size = cache.size();
            let path = Path::new();
            path.move_to(0.0, 0.0);
            path.line_to(size.width, 0.0);
            path.line_to(size.width - size.height / 2.0, size.height / 2.0);
            path.line_to(size.width, size.height);
            path.line_to(0.0, size.height);
            path.close();
            let brush = Brush::horizontal_gradient([
                Color::from_argb(0xFF6750A4),
                Color::from_argb(0xFF7D5260),
            ]);
            // Runs at each draw, with what the cache built.
            cache.on_draw_behind(move |scope| {
                let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
                scope.draw_path(&path, paint);
            })
        })
    );
}
```

As in Compose, the cache block runs again for a new size, density or layout direction, or when state it read changes; the drawing block runs at each draw, and state it reads draws again without rebuilding.

### Brushes

```kotlin
Modifier.background(Brush.horizontalGradient(listOf(Color.Red, Color.Blue)))
Modifier.background(Brush.radialGradient(listOf(Color.White, Color.Black)), CircleShape, alpha = 0.5f)
Modifier.background(Brush.linearGradient(0f to Color.Red, 0.3f to Color.Yellow, 1f to Color.Blue))
```

<!-- snippet: src/drawing.rs#brushes -->
```rust
#[composable]
pub fn Gradients() {
    Column!(vertical_arrangement = Arrangement::spaced_by(8.dp()), {
        // A brush where Compose takes one: `background(brush)`, `background(brush, shape)`.
        Box!(
            modifier = Modifier
                .size((96.dp(), 32.dp()))
                .background(Brush::horizontal_gradient([Color::RED, Color::BLUE]))
        );
        Box!(
            modifier = Modifier.size(64.dp()).background_with(
                Brush::radial_gradient([Color::WHITE, Color::BLACK]).with_alpha(0.5),
                CIRCLE_SHAPE
            )
        );
        // Stops and every parameter: `Brush.linearGradient(0f to red, 1f to blue, start, end, tileMode)`.
        Box!(
            modifier = Modifier.size(64.dp()).background(Brush::linear_gradient_with(
                [(0.0, Color::RED), (0.3, Color::YELLOW), (1.0, Color::BLUE)],
                Offset::ZERO,
                Offset::INFINITE,
                TileMode::Clamp,
            ))
        );
    });
}
```

In a `DrawScope`, `scope.brush_paint(&brush, alpha, style)` makes a paint of a brush. `background_with(brush, shape)` has no `alpha` parameter: `brush.with_alpha(alpha)` gives the same pixels.

### Graphics layers, clips and shadows

```kotlin
Box(Modifier.size(64.dp).shadow(6.dp, RoundedCornerShape(12.dp), clip = true).background(Color.White))
Box(
    Modifier.size(64.dp)
        .graphicsLayer { alpha = 0.5f; compositingStrategy = CompositingStrategy.ModulateAlpha }
        .background(Color.Red)
)
Box(Modifier.size(40.dp).clip(CircleShape).background(Color.Blue))
```

<!-- snippet: src/drawing.rs#layers -->
```rust
#[composable]
pub fn Layers() {
    let shape = RoundedCornerShape(12.dp());
    Column!(vertical_arrangement = Arrangement::spaced_by(8.dp()), {
        // `Modifier.shadow(6.dp, shape, clip = true)`: a layer casting a shadow of `shape` (a
        // render node's elevation on Android) and clipping its content to it.
        Box!(
            modifier = Modifier
                .size(64.dp())
                .shadow_with(6.dp(), shape, true, DEFAULT_SHADOW_COLOR, DEFAULT_SHADOW_COLOR)
                .background(Color::WHITE)
        );
        // `graphicsLayer { alpha = 0.5f; compositingStrategy = CompositingStrategy.ModulateAlpha }`:
        // each drawing faded, no offscreen buffer.
        Box!(
            modifier = Modifier
                .size(64.dp())
                .graphics_layer(|layer| {
                    layer.alpha = 0.5;
                    layer.compositing_strategy = CompositingStrategy::ModulateAlpha;
                })
                .background(Color::RED)
        );
        // `clip(CircleShape)`: a layer clipping to the circle.
        Box!(modifier = Modifier.size(40.dp()).clip(CIRCLE_SHAPE).background(Color::BLUE));
    });
}
```

As in Compose, `clip`, `clip_to_bounds`, `shadow` and `alpha` are graphics layers: each is a layer of its own (a render node on Android), whose properties change without drawing its content again. `GraphicsLayerScope` has Compose's properties and defaults: `alpha`, `scale_x`/`scale_y`, `rotation_z`, `translation_x`/`translation_y`, `transform_origin`, `shadow_elevation` (pixels: `layer.density.to_px(6.dp())`), `ambient_shadow_color`, `spot_shadow_color`, `shape`, `clip` and `compositing_strategy`, and reads `size` and `density`. `CompositingStrategy::Auto` (the default) draws the content offscreen only for an alpha below 1, `Offscreen` always, `ModulateAlpha` never (each drawing faded: overlapping drawings show through each other). Android draws a layer's shadow from its elevation, as Compose's; desktop draws it with Skia's shadow utilities, lit as Compose Desktop's.

## Theming and Material 3

```kotlin
@Composable
fun AppTheme(content: @Composable () -> Unit) {
    val base = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()
    MaterialTheme(
        colorScheme = base.copy(primary = Brand),
        typography = Typography().run { copy(titleLarge = titleLarge.copy(fontWeight = FontWeight.Bold)) },
        content = content,
    )
}
```

<!-- snippet: src/theming.rs#theme -->
```rust
const BRAND: Color = Color::from_argb(0xFF006A6A);

#[composable]
pub fn AppTheme(content: impl Fn() + 'static) {
    let base = if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    // Struct update syntax where Kotlin names `lightColorScheme(primary = …)`'s arguments.
    let color_scheme = ColorScheme { primary: BRAND, ..base };
    let typography = Typography::default(); // Compose's `Typography()`
    let typography = Typography {
        title_large: TextStyle { font_weight: Some(FontWeight::BOLD), ..typography.title_large },
        ..typography
    };
    MaterialTheme!(color_scheme = color_scheme, typography = typography, content = content);
}
```

Where Kotlin calls a constructor or `copy` with named arguments (`lightColorScheme(primary = …)`, `ButtonDefaults.buttonColors(containerColor = …)`, `TextStyle(fontWeight = …)`), Rust uses struct update syntax from the default: `ColorScheme { primary, ..base }`, `ButtonColors { container_color, ..ButtonDefaults::button_colors() }`, `TextStyle { font_weight, ..TextStyle::DEFAULT }`.

<!-- snippet: src/theming.rs#theme_reads -->
```rust
#[composable]
pub fn Banner(text: SharedString) {
    // `MaterialTheme.colorScheme`, `.typography`, `.shapes`: reads of composition locals.
    let colors = MaterialTheme::color_scheme();
    Surface!(color = colors.primary_container, shape = MaterialTheme::shapes().medium.clone(), {
        Text!(
            text,
            modifier = Modifier.padding(16.dp()),
            style = MaterialTheme::typography().title_large.clone()
        );
    });
}
```

`MaterialTheme::color_scheme()`, `MaterialTheme::typography()` and `MaterialTheme::shapes()` are shared, as Kotlin's objects are: an `Rc`, so a read copies nothing; a composable takes one style from the typography with `.clone()`, and `*MaterialTheme::color_scheme()` copies the scheme where one is needed by value. `MaterialTheme` takes the three as values, or the `Rc`s of what it keeps of the theme around it, and, as Compose, compares them by value: a theme composable that builds an equal one each pass recomposes no reader (D1021, D1106, D1107).

### Composition locals

```kotlin
val LocalSpacing = compositionLocalOf { 8.dp }

CompositionLocalProvider(LocalSpacing provides 16.dp) {
    Column(verticalArrangement = Arrangement.spacedBy(LocalSpacing.current)) { … }
}
```

<!-- snippet: src/theming.rs#local -->
```rust
/// A composition local of the app's own: `compositionLocalOf { 8.dp }`.
pub static LOCAL_SPACING: CompositionLocal<Dp> = CompositionLocal::new("LocalSpacing", || Dp(8.0));

#[composable]
pub fn Spaced() {
    CompositionLocalProvider(LOCAL_SPACING.provides(16.dp()), || {
        Column!(vertical_arrangement = Arrangement::spaced_by(LOCAL_SPACING.current()), {
            Text!("One");
            Text!("Two");
        });
    });
}
```

A local is a `static`, named by its name and type together. `CompositionLocalProvider` is a plain function taking one value, or a tuple of them (`(LOCAL_A.provides(a), LOCAL_B.provides(b))`, Compose's varargs), and `with_composition_local(values, || …)` returns what its content returns. `staticCompositionLocalOf` is `CompositionLocal::new_static`, `compositionLocalWithComputedDefaultOf { … }` is `CompositionLocal::with_computed_default(name, |scope| scope.current_value(&OTHER) …)`, and `provides_default`, `provides_computed`, `current_composition_local_context()` are Compose's `providesDefault`, `providesComputed`, `currentCompositionLocalContext`.

### Components

Material's components take Compose's parameters, in Compose's order:

```kotlin
Scaffold(
    topBar = {
        TopAppBar(title = { Text("Notes") }, actions = {
            IconButton(onClick = { notes = 0 }) { Icon(Icons.Filled.Delete, "Delete all") }
        })
    },
    snackbarHost = { SnackbarHost(snackbars) },
    floatingActionButton = {
        FloatingActionButton(onClick = { … }) { Icon(Icons.Filled.Add, "Add a note") }
    },
) { padding -> … }
```

<!-- snippet: src/theming.rs#scaffold -->
```rust
#[composable]
pub fn Notes() {
    let notes = remember_saveable(|| mutable_state_of(0u32));
    let snackbars = remember(SnackbarHostState::new);
    let scope = remember_coroutine_scope();
    AppTheme!({
        Scaffold!(
            top_bar = {
                TopAppBar!(
                    { Text!("Notes") },
                    actions = |_row| {
                        IconButton!(on_click = move || notes.set(0), {
                            Icon!(icons::filled::delete(), Some("Delete all"));
                        });
                    }
                );
            },
            snackbar_host = {
                SnackbarHost!(snackbars);
            },
            floating_action_button = {
                FloatingActionButton!(
                    on_click = move || {
                        notes.update(|count| count + 1);
                        scope.launch(async move {
                            snackbars.show_snackbar("Note added").await;
                        });
                    },
                    {
                        Icon!(icons::filled::add(), Some("Add a note"));
                    }
                );
            },
            |padding: PaddingValues| {
                Column!(modifier = Modifier.padding(padding).padding(16.dp()), {
                    Text!("{} notes", notes.get());
                    // Colors, as other `…Defaults`, change by struct update.
                    Button!(
                        on_click = move || notes.set(0),
                        enabled = notes.get() > 0,
                        colors = ButtonColors {
                            container_color: MaterialTheme::color_scheme().error,
                            ..ButtonDefaults::button_colors()
                        },
                        {
                            Text!("Clear");
                        }
                    );
                });
            }
        );
    });
}
```

- A slot given by name takes a block (`top_bar = { … }`); an optional slot (`Option<impl Fn()>`) takes `Some(|| …)` or a block (`label = Some(|| Text!("Name"))`).
- Icons are `icons::filled::add()` and the rest of material-icons-core's filled set (`Icons.Default` is `icons::default`).
- Overloads Kotlin distinguishes by parameters have their own names: `Surface(onClick = …)` is `ClickableSurface`, `Surface(selected = …, onClick = …)` is `SelectableSurface`, `Text(AnnotatedString)` is `AnnotatedText`, an indeterminate `CircularProgressIndicator()` is `IndeterminateCircularProgressIndicator`.
- `TopAppBarDefaults::enter_always_scroll_behavior()` and its siblings take no arguments, and a bar's scroll behavior is connected with `Modifier.nested_scroll(behavior.nested_scroll_connection())`, as in Compose.

## Text and text fields

```kotlin
Column {
    Text(title, style = MaterialTheme.typography.headlineSmall, maxLines = 1)
    Text(summary, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 3, overflow = TextOverflow.Ellipsis)
    val count = 3
    Text("$count unread", fontWeight = FontWeight.Bold)
}
```

<!-- snippet: src/text.rs#text -->
```rust
#[composable]
pub fn Headline(title: SharedString, summary: SharedString) {
    Column!({
        Text!(title, style = MaterialTheme::typography().headline_small.clone(), max_lines = 1);
        Text!(
            summary,
            color = MaterialTheme::color_scheme().on_surface_variant,
            max_lines = 3,
            overflow = TextOverflow::Ellipsis,
        );
        // Format arguments, as `format!`: inline names too.
        let count = 3;
        Text!("{count} unread", font_weight = Some(FontWeight::BOLD));
    });
}
```

Styled text is built as with `buildAnnotatedString`:

<!-- snippet: src/text.rs#annotated -->
```rust
#[composable]
pub fn Terms() {
    // `buildAnnotatedString { append(…); withStyle(…) { append(…) } }`
    let text = AnnotatedString::build(|text| {
        text.append("By joining, you accept the ");
        let bold = SpanStyle { font_weight: Some(FontWeight::BOLD), ..SpanStyle::default() };
        text.with_style(bold, |text| {
            text.append("rules");
        });
        text.append(".");
    });
    AnnotatedText!(text); // Material's `Text(AnnotatedString)`
}
```

Selection is opt-in, as in Compose: `SelectionContainer!({ … })` and `DisableSelection!({ … })`.

### Text fields, the keyboard and focus

```kotlin
@Composable
fun JoinNetwork(onJoin: (String, String) -> Unit) {
    var network by rememberSaveable { mutableStateOf("") }
    var passphrase by remember { mutableStateOf("") }
    val networkField = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    val join = {
        focusManager.clearFocus()
        onJoin(network, passphrase)
    }
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            network, { network = it },
            Modifier.focusRequester(networkField),
            label = { Text("Network") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
        )
        OutlinedTextField(
            passphrase, { passphrase = it },
            label = { Text("Passphrase") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { join() }),
        )
        Button(onClick = join, enabled = network.isNotEmpty()) { Text("Join") }
    }
    LaunchedEffect(Unit) { networkField.requestFocus() }
}
```

<!-- snippet: src/text.rs#fields -->
```rust
#[composable]
pub fn JoinNetwork(on_join: impl Fn(String, String) + 'static) {
    let network = remember_saveable(|| mutable_state_of(String::new()));
    let passphrase = remember(|| mutable_state_of(String::new()));
    let network_field = remember(FocusRequester::new);
    let focus_manager = LOCAL_FOCUS_MANAGER.current();
    let join = callback(move || {
        focus_manager.clear_focus(false); // hides the soft keyboard with the focus
        on_join(network.get(), passphrase.get());
    });
    Column!(
        modifier = Modifier.padding(16.dp()),
        vertical_arrangement = Arrangement::spaced_by(8.dp()),
        {
            OutlinedTextField!(
                network.get(),
                move |value| network.set(value),
                modifier = Modifier.focus_requester(network_field).test_tag("network"),
                label = Some(|| Text!("Network")),
                single_line = true,
                // `Next`'s default action moves the focus to the next field.
                keyboard_options =
                    KeyboardOptions { ime_action: ImeAction::Next, ..KeyboardOptions::DEFAULT },
            );
            OutlinedTextField!(
                passphrase.get(),
                move |value| passphrase.set(value),
                modifier = Modifier.test_tag("passphrase"),
                label = Some(|| Text!("Passphrase")),
                single_line = true,
                visual_transformation = PasswordVisualTransformation::new(),
                keyboard_options = KeyboardOptions {
                    keyboard_type: KeyboardType::Password,
                    ime_action: ImeAction::Done,
                    ..KeyboardOptions::DEFAULT
                },
                keyboard_actions = KeyboardActions::new().on_done(move || join()),
            );
            Button!(on_click = join, enabled = !network.get().is_empty(), {
                Text!("Join");
            });
        }
    );
    // Focus the first field once shown, as `LaunchedEffect(Unit) { requester.requestFocus() }`.
    LaunchedEffect((), async move {
        network_field.request_focus();
    });
}
```

- Text fields take the value and a callback with the new value (`TextField(value, onValueChange)`), as Compose's `String` overloads; there's no `TextFieldState` or `TextFieldValue` overload.
- `KeyboardOptions` is built by struct update from `KeyboardOptions::DEFAULT`, and `KeyboardActions` with `KeyboardActions::new().on_done(…)` and its siblings. Without an action, `Next` and `Previous` move the focus and `Done` hides the keyboard, as in Compose. On Android the soft keyboard follows the options (type, action key, capitalization, auto-correction); on desktop the action is Enter.
- `callback(closure)` makes a memoized callback in a composable's body, to pass on or call from several places (`join` above).
- `LOCAL_FOCUS_MANAGER.current()` moves (`move_focus(FocusDirection::Next)`) and clears the focus, `LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current()` (an `Option`) shows and hides the keyboard, and `Modifier.on_focus_changed(|state: FocusState| …)`, `on_focus_event`, `focusable()`, `focus_target()`, `focus_group()`, `focus_restorer(fallback)` and `on_key_event(…)` are Compose's. `Modifier.focus_properties(|properties| { properties.can_focus = false; properties.next = other; properties.set_on_enter(|scope| scope.cancel_focus_change()); })` is `focusProperties`. Requests have Compose's result where the tree is free (a callback, an effect); in composition or a modifier node's code they wait for it, and in an `on_enter` or `on_exit` block they're done as it returns (`scope.request_focus(requester)` acts at once). `requestFocus(direction)` is `request_focus_with(direction)`.

## Dialogs

```kotlin
var confirming by remember { mutableStateOf(false) }
TextButton(onClick = { confirming = true }) { Text("Delete…") }
if (confirming) {
    AlertDialog(
        onDismissRequest = { confirming = false },
        confirmButton = { TextButton(onClick = { confirming = false; onDelete() }) { Text("Delete") } },
        dismissButton = { TextButton(onClick = { confirming = false }) { Text("Cancel") } },
        title = { Text("Delete the note?") },
        text = { Text("It can't be restored.") },
    )
}
```

<!-- snippet: src/components.rs#dialog -->
```rust
#[composable]
pub fn DeleteNote(on_delete: impl Fn() + 'static) {
    let confirming = remember(|| mutable_state_of(false));
    TextButton!(on_click = move || confirming.set(true), {
        Text!("Delete…");
    });
    // In the composition while it shows, as in Compose.
    if confirming.get() {
        AlertDialog!(
            on_dismiss_request = move || confirming.set(false),
            confirm_button = {
                TextButton!(
                    on_click = move || {
                        confirming.set(false);
                        on_delete();
                    },
                    {
                        Text!("Delete");
                    }
                );
            },
            dismiss_button = {
                TextButton!(on_click = move || confirming.set(false), {
                    Text!("Cancel");
                });
            },
            title = {
                Text!("Delete the note?");
            },
            text = {
                Text!("It can't be restored.");
            },
        );
    }
}
```

A dialog shows while it's in the composition, as in Compose. Back, Escape and a tap on the scrim call `on_dismiss_request`. `Dialog!` (compose-ui) shows any content as a dialog, `Popup!` places content over the window, and `DropdownMenu!`, `ModalBottomSheet!` and `PlainTooltip!` (in a `TooltipArea!`) are Material's. All of them are drawn inside the window: on Android, inside the `ComposeRsView` ([Differences](differences.md#navigation-and-platform)).

## Selectable lists

```kotlin
Column(Modifier.selectableGroup()) {
    sizes.forEachIndexed { index, size ->
        ListItem(
            selected = index == selected,
            onClick = { onSelect(index) },
            leadingContent = { RadioButton(index == selected, onClick = null) },
        ) { Text(size) }
    }
}
```

<!-- snippet: src/components.rs#selectable -->
```rust
const SIZES: [&str; 3] = ["Small", "Medium", "Large"];

#[composable]
pub fn SizePicker(selected: usize, on_select: impl Fn(usize) + 'static) {
    // One group for accessibility: "1 of 3"-style navigation between the options.
    Column!(modifier = Modifier.selectable_group(), {
        for (index, size) in SIZES.into_iter().enumerate() {
            // The whole item selects, as a radio button.
            SelectableListItem!(
                index == selected,
                move || on_select(index),
                // `None`: the item handles the click, not the button.
                leading_content = Some(move || RadioButton!(index == selected, None::<fn()>)),
                {
                    Text!(size);
                }
            );
        }
    });
}
```

An optional callback (`onClick = null`) is `None`; a bare `None` needs its type (`None::<fn()>`), as Rust can't infer it. Kotlin's `ListItem` overloads are named apart: `ListItem!`, `ClickableListItem!`, `SelectableListItem!` (`selected`, as here) and `ToggleableListItem!` (`checked`, with `Checkbox!(checked, None::<fn(bool)>)` as its leading content for a checkable row).

## Accessibility and semantics

compose-rs builds Compose's semantics tree, which tests query and the platforms' screen readers read: AccessKit on desktop, an `AccessibilityNodeProvider` on Android. Components set their semantics as Compose's do; `Modifier.semantics`, `semantics_with(merge_descendants, …)` and `clear_and_set_semantics` add or replace them:

```kotlin
Row(Modifier.clearAndSetSemantics { contentDescription = "$stars of 5 stars" }) {
    repeat(5) { index -> Icon(Icons.Filled.Star, null, tint = if (index < stars) primary else outlineVariant) }
}

Row(Modifier.fillMaxWidth().toggleable(on, role = Role.Switch, onValueChange = onChange)) {
    Text(label, Modifier.weight(1f))
    Switch(on, onCheckedChange = null)
}
```

<!-- snippet: src/accessibility.rs#semantics -->
```rust
#[composable]
pub fn Rating(stars: u32) {
    let colors = MaterialTheme::color_scheme();
    Row!(
        // One description for the row instead of five unlabeled icons.
        modifier = Modifier.clear_and_set_semantics(move |config| {
            config.set_content_description(format!("{stars} of 5 stars"));
        }),
        {
            for index in 0..5 {
                let tint = if index < stars { colors.primary } else { colors.outline_variant };
                Icon!(icons::filled::star(), None, tint = tint);
            }
        }
    );
}

#[composable]
pub fn Setting(label: SharedString, on: bool, on_change: impl Fn(bool) + 'static) {
    Row!(
        // A row read as one control, toggled as a switch: `Modifier.toggleable(role = Switch)`.
        modifier = Modifier.fill_max_width().toggleable_with(
            on,
            ClickableOptions { role: Some(Role::Switch), ..ClickableOptions::DEFAULT },
            on_change,
        ),
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        |row| {
            Text!(label, modifier = Modifier.weight(row, 1.0));
            Switch!(on, None::<fn(bool)>);
        }
    );
}
```

The semantics block gets a `SemanticsConfiguration` whose fields are Compose's properties (`content_description`, `role`, `state_description`, `selected`, `disabled`, `test_tag`, …). `Icon!` and `Image!` take their content description as a `&'static str`: for a description built at run time, set it with `Modifier.semantics`. There are no custom accessibility actions or pane titles yet.

## Gestures

`Modifier.pointer_input(key, |scope| async move { … })` runs a coroutine over the pointer events, as `Modifier.pointerInput(key) { … }`, with Compose's detectors: `detect_tap_gestures` and `detect_drag_gestures`, configured by builders (`TapGestures::default().on_double_tap(…)`), and `await_pointer_event` for your own.

```kotlin
Box(Modifier.size(160.dp).background(Color.LightGray).pointerInput(Unit) {
    detectTapGestures(onDoubleTap = { liked = !liked })
}, contentAlignment = Alignment.Center) {
    if (liked) Icon(Icons.Filled.Favorite, "Liked")
}
```

<!-- snippet: src/gestures.rs#tap -->
```rust
#[composable]
pub fn Photo() {
    let liked = remember(|| mutable_state_of(false));
    Box!(
        modifier = Modifier.size(160.dp()).background(Color::LIGHT_GRAY).pointer_input(
            (),
            move |scope| async move {
                // `detectTapGestures(onDoubleTap = { … })`, in a coroutine per pointer input.
                let gestures =
                    TapGestures::default().on_double_tap(move |_| liked.set(!liked.get()));
                scope.detect_tap_gestures(gestures).await;
            }
        ),
        content_alignment = Alignment::CENTER,
        {
            if liked.get() {
                Icon!(icons::filled::favorite(), Some("Liked"));
            }
        }
    );
}
```

`Modifier.draggable` takes a `DraggableState` and every one of Compose's parameters:

```kotlin
var offset by remember { mutableFloatStateOf(0f) }
Card(Modifier.fillMaxWidth()
    .graphicsLayer { translationX = offset }
    .draggable(
        rememberDraggableState { delta -> offset += delta },
        Orientation.Horizontal,
        onDragStopped = { if (abs(offset) > 150f) onDismiss(); offset = 0f },
    )) { Text("Swipe me away", Modifier.padding(16.dp)) }
```

<!-- snippet: src/gestures.rs#drag -->
```rust
#[composable]
pub fn SwipeAway(on_dismiss: impl Fn() + 'static) {
    let offset = remember(|| mutable_state_of(0.0f32));
    let state = remember_draggable_state!(move |delta: f32| offset.update(|x| x + delta));
    Card!(
        modifier = Modifier
            .fill_max_width()
            // Read while drawing: dragging moves the card without recomposing it.
            .graphics_layer(move |layer| layer.translation_x = offset.get())
            .draggable(
                state,
                Orientation::Horizontal,
                true,
                false,
                |_start| {},
                move |_velocity| {
                    if offset.get().abs() > 150.0 {
                        on_dismiss();
                    }
                    offset.set(0.0);
                }
            ),
        {
            Text!("Swipe me away", modifier = Modifier.padding(16.dp()));
        }
    );
}
```

Material's `SwipeToDismissBox!` does this with anchors and animation. `Modifier.combined_clickable` handles double clicks and long presses, `Modifier.vertical_scroll` and `horizontal_scroll` scroll, and `Modifier.nested_scroll` takes part in nested scrolling.

## Images

```kotlin
Image(
    ImageBitmap.imageResource(R.drawable.avatar), "Avatar",
    Modifier.size(48.dp).clip(CircleShape), contentScale = ContentScale.Crop,
)
```

<!-- snippet: src/images.rs#image -->
```rust
#[composable]
pub fn Avatar() {
    // Bundled in the library and decoded once where it's composed (`ImageBitmap.imageResource`).
    // `None` when the platform can't decode it (and in headless tests, which have no decoder).
    let bitmap = remember_image_resource(include_bytes!("checkers.png"));
    let modifier = Modifier.size(48.dp()).clip(CIRCLE_SHAPE);
    if let Some(bitmap) = bitmap {
        Image!(bitmap, Some("Avatar"), modifier = modifier, content_scale = ContentScale::Crop);
    } else {
        Box!(modifier = modifier.background(MaterialTheme::color_scheme().primary_container));
    }
}
```

There are no Android resources: an image is bundled in the Rust library with `include_bytes!`, decoded by the platform (`BitmapFactory` on Android, Skia on desktop). `decode_image_bytes(&bytes)` decodes bytes from elsewhere (a file, the network), returning an `Option<ImageBitmap>`. Vector icons are `ImageVector`s, drawn by `Icon!`.

## Animation

`compose::animation` has Compose's animation APIs, with its specs (`tween(ms)`, `spring()`, `keyframes`, `infinite_repeatable`) and easings:

```kotlin
var expanded by rememberSaveable { mutableStateOf(false) }
val rotation by animateFloatAsState(if (expanded) 180f else 0f)
Card(Modifier.fillMaxWidth().clickable { expanded = !expanded }) {
    Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(title, Modifier.weight(1f))
        Icon(Icons.Filled.KeyboardArrowDown, "Expand", Modifier.graphicsLayer { rotationZ = rotation })
    }
    AnimatedVisibility(expanded) { Text(body, Modifier.padding(16.dp)) }
}
```

<!-- snippet: src/animation.rs#as_state -->
```rust
#[composable]
pub fn Expandable(title: SharedString, body: SharedString) {
    let expanded = remember_saveable(|| mutable_state_of(false));
    // `animateFloatAsState(if (expanded) 180f else 0f)`: a `State<f32>` moving to its target.
    let rotation = animate_float_as_state!(if expanded.get() { 180.0 } else { 0.0 });
    Card!(modifier = Modifier.fill_max_width().clickable(move || expanded.set(!expanded.get())), {
        Row!(
            modifier = Modifier.padding(16.dp()),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            |row| {
                Text!(title, modifier = Modifier.weight(row, 1.0));
                Icon!(
                    icons::filled::keyboard_arrow_down(),
                    Some("Expand"),
                    // Read while drawing: the animation's frames don't recompose.
                    modifier =
                        Modifier.graphics_layer(move |layer| layer.rotation_z = rotation.get())
                );
            }
        );
        // Fades and expands in, shrinks and fades out, as Compose's defaults.
        AnimatedVisibility!(expanded.get(), {
            Text!(body, modifier = Modifier.padding(16.dp()));
        });
    });
}
```

```kotlin
Crossfade(loading) { loading ->
    if (loading) CircularProgressIndicator() else Text("Loaded")
}
```

<!-- snippet: src/animation.rs#crossfade -->
```rust
#[composable]
pub fn Loading(loading: bool) {
    Crossfade!(loading, |loading: bool| {
        if loading {
            IndeterminateCircularProgressIndicator!();
        } else {
            Text!("Loaded");
        }
    });
}
```

`animate_dp_as_state!`, `animate_color_as_state!` and `animate_int_as_state!` animate other values; `AnimatedContent!`, `update_transition`, `Modifier.animate_content_size()` and `Animatable` (driven from a `LaunchedEffect`) are there too. `remember_infinite_transition` runs forever ([Performance](performance.md#reads-while-laying-out-and-drawing) shows one). Tests drive animations with the test clock ([Testing](#testing)).

## Navigation

`compose::navigation` is a small Navigation Compose: a `NavController` holds a back stack of destinations, and a `NavHost` shows the top one. Destinations are values of the app's own type, an enum typically, rather than routes: a `match` takes the place of the graph builder, and an enum variant's fields are its arguments.

```kotlin
@Serializable object Inbox
@Serializable data class Message(val id: Int)
@Serializable object Settings

val navController = rememberNavController()
NavHost(navController, startDestination = Inbox) {
    composable<Inbox> {
        Inbox(onOpen = { id -> navController.navigate(Message(id)) },
              onSettings = { navController.navigate(Settings) })
    }
    composable<Message> { entry -> MessageScreen(entry.toRoute<Message>().id) }
    composable<Settings> { SettingsScreen() }
}
```

<!-- snippet: src/navigation.rs#screens -->
```rust
/// The app's destinations: Navigation Compose's routes, as an enum (`ComposeArg`, so a
/// composable can take one).
#[derive(Clone, Copy, PartialEq, Debug, ComposeArg)]
pub enum Screen {
    Inbox,
    Message(u32),
    Settings,
}

/// Saved as a number, so the back stack survives the activity's recreation.
impl Saveable for Screen {
    fn save(&self) -> SavedValue {
        SavedValue::Int(match *self {
            Screen::Inbox => -1,
            Screen::Settings => -2,
            Screen::Message(id) => i64::from(id),
        })
    }

    fn restore(saved: &SavedValue) -> Option<Screen> {
        match *saved {
            SavedValue::Int(-1) => Some(Screen::Inbox),
            SavedValue::Int(-2) => Some(Screen::Settings),
            SavedValue::Int(id) => u32::try_from(id).ok().map(Screen::Message),
            _ => None,
        }
    }
}
```

<!-- snippet: src/navigation.rs#app -->
```rust
#[composable]
pub fn App() {
    // The back stack, starting at the inbox: saved, so it survives the activity's recreation.
    let nav_controller = remember_nav_controller(Screen::Inbox);
    // Shows the top entry; back pops the stack, and leaves the app at the inbox.
    NavHost!(nav_controller, move |screen: Screen| match screen {
        Screen::Inbox => Inbox!(
            on_open = move |id: u32| nav_controller.navigate(Screen::Message(id)),
            on_settings = move || nav_controller.navigate(Screen::Settings)
        ),
        Screen::Message(id) => MessageScreen!(id),
        Screen::Settings => SettingsScreen!(),
    });
}
```

- **The back stack.** `nav_controller.navigate(destination)` pushes; `pop_back_stack()` pops (and returns `false` at the start destination, which stays); `pop_back_stack_to(&destination, inclusive)` is `popBackStack(route, inclusive)`. `navigate_with(destination, NavOptions { pop_up_to: Some(Screen::Inbox), launch_single_top: true, ..NavOptions::default() })` is `navigate(route) { popUpTo(Inbox); launchSingleTop = true }`. `current_destination()`, `current_back_stack_entry()`, `previous_back_stack_entry()` and `current_back_stack()` are state reads: what reads them recomposes when the stack changes. Navigate from event handlers, not while composing.
- **Saved state.** `remember_nav_controller(start_destination)` saves the stack, so destinations implement `Saveable`. Each entry's content is in a `SaveableStateProvider` of its own: its `remember_saveable` values (its lists' scroll positions too) are kept while it's on the stack, come back when it shows again, survive the process's death with the stack, and go when it's popped. `remember` values go when another entry shows, as in Compose.
- **Back.** The `NavHost` pops on back while there's an entry under the top one; at the start destination, back leaves the app (or goes to the Kotlin UI around it). A screen's own `BackHandler` goes first.
- **Transitions.** None by default: `NavHost!(nav_controller, enter_transition = fade_in(), exit_transition = fade_out(), …)` fades, as Compose's default does; any `EnterTransition` and `ExitTransition` go. A screen fading out takes no back: its `BackHandler`s are disabled (`LOCAL_BACK_HANDLERS_ENABLED`), as Navigation Compose's entries that aren't resumed.
- **Navigate from handlers and effects.** `navigate` writes state, which composition can't (debug builds panic); a redirect goes in a `LaunchedEffect`:

<!-- snippet: src/navigation.rs#redirect -->
```rust
/// Sends the user to `to` as soon as it shows, in place of itself. Navigating writes state, so
/// it goes in an effect or an event handler, never in a composable's body (debug builds panic).
#[composable]
pub fn Redirect(nav_controller: NavController<Screen>, to: Screen) {
    LaunchedEffect((), async move {
        nav_controller.navigate_with(
            to,
            NavOptions { pop_up_to: Some(Screen::Inbox), ..NavOptions::default() },
        );
    });
}
```


What's different from Navigation Compose:

- `remember_nav_controller` takes the start destination (`NavHost` takes it in Compose), so the stack is never empty.
- Entry ids are random 64-bit numbers rather than UUID strings; destinations that a composable takes implement `ComposeArg` (`#[derive(ComposeArg)]`).
- Destinations are compared with `==`: `launch_single_top` keeps the top entry only when it's equal (Compose's compares the route and replaces the arguments), and `pop_up_to` finds the topmost entry equal to it.
- There are no nested graphs, deep links, dialog destinations, per-entry `ViewModel`s or lifecycles, and no `saveState`/`restoreState` for bottom navigation's tabs.

Without a `NavHost`, a back stack is a `remember_saveable(|| mutable_state_of(vec![Screen::Inbox]))`, a `match` on its last entry and a `BackHandler` that pops; but then a screen under the top loses its state. `remember_saveable_state_holder()` (`compose::runtime`, Compose's `rememberSaveableStateHolder`) keeps it for content of your own: `holder.SaveableStateProvider(key, || …)` around each screen or tab, and `holder.remove_state(key)` when one is gone for good.

Back goes to the innermost enabled `BackHandler`: those in dialogs, bottom sheets and menus first, then the one composed last. While none is enabled, back leaves the app. Escape is back on desktop.

## Android interop

The Rust app reads what Kotlin passes through composition locals the host provides:

```kotlin
val view = ComposeRsView(context, "embedded")
view.setProp("user", "ada")              // also Double, Int and Boolean values
view.setOnEventListener { name, payload ->
    if (name == "sign_out") signOut()
}
```

<!-- snippet: src/interop.rs#embedded -->
```rust
#[composable]
pub fn Embedded() {
    // What Kotlin passes with `ComposeRsView.setProp`, and the listener of `setOnEventListener`.
    let props = LOCAL_HOST_PROPS.current();
    let events = LOCAL_HOST_EVENTS.current();
    // A read of a property recomposes this scope when Kotlin sets it again.
    let user = props.string("user");
    let color_scheme =
        if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    MaterialTheme!(color_scheme = color_scheme, {
        // The view reports the window's insets: keep the content clear of the system bars.
        Surface!(modifier = Modifier.fill_max_size(), {
            Column!(modifier = Modifier.safe_drawing_padding().padding(16.dp()), {
                Text!("Signed in as {}", user.as_deref().unwrap_or("nobody"));
                Button!(
                    on_click = move || {
                        events.send("sign_out", "");
                    },
                    {
                        Text!("Sign out");
                    }
                );
            });
        });
    });
}
```

- **Properties** (`LOCAL_HOST_PROPS`) are text, numbers or booleans by name (`props.string(…)`, `number(…)`, `boolean(…)`). Reading one subscribes the reader, so a new value from Kotlin recomposes it.
- **Events** (`LOCAL_HOST_EVENTS`) are a name and a text payload, posted to the listener on the main thread. For anything richer, encode it as text (JSON) or call into your own JNI.
- **Platform services** are Compose's locals: `LOCAL_URI_HANDLER` (`open_uri`), `LOCAL_CLIPBOARD_MANAGER` (`get_text`, `set_text`: the clipboard text fields use) and `LOCAL_HAPTIC_FEEDBACK` (`perform_haptic_feedback`, nothing on desktop). Their values are `Rc<dyn …>`, so a test provides fakes with `CompositionLocalProvider`. `combined_clickable`'s long click vibrates, as Compose's.
- **Links are checked**: the platform's handler opens only `http`, `https`, `mailto` and `tel` URIs (no spaces, quotes or backslashes), and logs a warning for the rest, where Compose opens anything. Widen it for a subtree with `PlatformUriHandler::current().with_schemes(…)`. A link from an untrusted source (a server, a message) still needs the app's own validation.
- **There's no `LocalContext`**, lifecycle owner or `ViewModel`. For anything else Kotlin does (sharing, starting an activity, asking for a permission), send it an event (`events.send("share", &text)`) from a callback; its listener has the activity, runs after the call into Rust returns, and answers with properties (`view.setProp("granted", true)`), which recompose their readers.
- **Back**: `BackHandler`s get the system back while one is enabled; the view intercepts back only then, through the AndroidX `OnBackPressedDispatcher` when the activity has one (so it orders with Kotlin's `BackHandler`s), else the platform's.
- **Insets**: the view reports the window's insets (system bars, the keyboard, cutouts). `Modifier.safe_drawing_padding()`, `system_bars_padding()`, `ime_padding()`, `window_insets_padding(WindowInsets::…)`, `consume_window_insets` and the rest are Compose's, and consume what they pad. Material's `Scaffold` and bars default to Compose's insets (`content_window_insets = WindowInsets::safe_drawing()` to change them).
- **Insets when embedded**: inside Kotlin Compose's `AndroidView`, the view gets the window's insets less its distance from the window's edges, as any embedded view does, so what the Kotlin UI around it already keeps it clear of (`Modifier.safeDrawingPadding()` on a parent) isn't padded again in Rust. The first dispatch comes before anything is laid out, whole; `AndroidView` dispatches them reduced during the same traversal's layout, right after the view's first composition, and the view makes its frame again before that traversal draws, so the whole insets are never shown (D1365). Under a plain parent that pads by the insets without consuming them, the view gets them whole and its content pads a second time: consume them in that parent. `ComposeRsView.consumeWindowInsets` is about what's after the view, not inside it: it returns the insets consumed, so later siblings don't get them.
- **Dark theme and configuration**: `is_system_in_dark_theme()` and `LOCAL_CONFIGURATION` follow the activity's configuration.
- **Saved state**: `remember_saveable` values (and the saveable state holders) are saved with the view's instance state, and restored when the activity is recreated, after a rotation or the process's death.

<!-- snippet: src/interop.rs#services -->
```rust
#[composable]
pub fn Links(url: String) {
    // `LocalUriHandler.current`: Android opens the link with an `ACTION_VIEW` intent, desktop
    // with the OS (http, https, mailto and tel only, unless widened).
    let uri_handler = LOCAL_URI_HANDLER.current();
    let clipboard = LOCAL_CLIPBOARD_MANAGER.current();
    let haptics = LOCAL_HAPTIC_FEEDBACK.current();
    Row!({
        TextButton!(on_click = move || uri_handler.open_uri(&url), {
            Text!("Open");
        });
        TextButton!(
            on_click = move || {
                clipboard.set_text("compose-rs");
                haptics.perform_haptic_feedback(HapticFeedbackType::Confirm);
            },
            {
                Text!("Copy");
            }
        );
    });
}
```

An app's own scheme (a deep link into itself) opens where a subtree allows it:

<!-- snippet: src/interop.rs#deep_links -->
```rust
#[composable]
pub fn AppLinks() {
    // The platform's handler, opening the app's own scheme too in this subtree.
    let handler: Rc<dyn UriHandler> =
        remember(|| Rc::new(PlatformUriHandler::current().with_schemes(&["https", "myapp"])));
    CompositionLocalProvider(LOCAL_URI_HANDLER.provides(handler), || {
        Links("myapp://inbox".to_string());
    });
}
```

<!-- snippet: src/interop.rs#configuration -->
```rust
#[composable]
pub fn Adaptive() {
    // `LocalConfiguration.current`: the window's size in dp, orientation, font scale, night mode.
    let configuration = LOCAL_CONFIGURATION.current();
    if configuration.screen_width_dp >= 600.0 {
        Text!("Two panes");
    } else {
        Text!("One pane");
    }
}
```

## Logging

`println!` and `eprintln!` go nowhere on Android: a process's standard output and error aren't connected to logcat. Log through the `log` crate instead, re-exported as `compose::log`, as `Log.i` and `Log.w` in Kotlin:

<!-- snippet: src/interop.rs#logging -->
```rust
pub fn load(items: &[&str]) {
    // `Log.i(TAG, …)`: logcat on Android (tagged `guide`, the crate), stderr on desktop.
    compose::log::info!("loaded {} items", items.len());
    if items.is_empty() {
        compose::log::warn!("nothing to show");
    }
}
```

- **Android**: `JNI_OnLoad` (`export_apps!`) installs a logcat logger, each record tagged with the crate that logged it: `adb logcat -s guide compose_runtime`. To use a logger of your own (`android_logger` with its own tags), start `export_apps!` with `logger = false;` and install it in a `JNI_OnLoad` of yours that calls `compose_android::on_load_with` (`#[unsafe(no_mangle)] pub unsafe extern "system" fn JNI_OnLoad(vm: *mut compose_android::jni::sys::JavaVM, _: *mut std::ffi::c_void) -> i32`, the VM from `compose_android::jni::JavaVM::from_raw(vm)`).
- **Desktop and tests**: records go to standard error (in a test, its captured output). `COMPOSE_LOG` filters them as `RUST_LOG` does: `COMPOSE_LOG=warn,my_app=debug`. A logger set before `run_app` (`env_logger`) replaces it.
- **Levels**: `debug` and above in debug builds, `info` and above in release ones.
- compose-rs's own warnings (a state holder used after it was freed, D240) come through it too. A panic's message goes to logcat under the tag `compose-rs` before the app aborts.

## Testing

`compose::test` (the facade's `test` feature, for dev-dependencies) is `androidx.compose.ui.test`: a rule over a headless host, with finders through the semantics tree, actions that send real input events, and assertions.

```kotlin
@get:Rule val rule = createComposeRule()

@Test fun openingAMessageAndGoingBack() {
    rule.setContent { App() }
    rule.onNodeWithText("Message 3").performClick()
    rule.onNodeWithContentDescription("Star").performClick().assertIsOn()
    Espresso.pressBack()
    rule.onNodeWithText("Message 3").assertExists()
}
```

<!-- snippet: tests/suite/testing.rs#rule -->
```rust
#[test]
fn opening_a_message_and_going_back() {
    let mut ui = compose_test(navigation::App); // `createComposeRule()` + `setContent { App() }`
    ui.on_node_with_text("Message 3").perform_click();
    ui.on_node_with_content_description("Star").perform_click().assert_is_on();
    assert_eq!(ui.count_nodes(&has_role(Role::Checkbox)), 1); // a toggle button is a checkbox
    ui.host().on_back_pressed(); // the system back
    ui.wait_for_idle();
    ui.on_node_with_text("Message 3").assert_exists();
    ui.on_node(has_text("Settings").and(has_click_action())).assert_is_enabled();
}
```

Compose's test API, in snake case:

| Compose | compose-rs |
|---|---|
| `onNodeWithText`, `onNodeWithTag`, `onNodeWithContentDescription`, `onNode(matcher)`, `onRoot()`, `useUnmergedTree = true` | `on_node_with_text`, `on_node_with_tag` (`Modifier.test_tag`), `on_node_with_content_description`, `on_node(matcher)`, `on_root()`; `on_node_unmerged`, `on_node_with_tag_unmerged`… |
| `onAllNodes(matcher)[i]`, `onFirst()`, `onChildren()`, `onParent()`, `assertCountEquals(n)` | `on_all_nodes(matcher).get(i)`, `on_first()`, `on_children()`, `on_parent()`, `assert_count_equals(n)` |
| `hasText("a") and isEnabled()`, `hasAnyAncestor(hasTestTag("list"))` | `has_text("a") & is_enabled()`, `has_any_ancestor(has_test_tag("list"))` |
| `performClick()`, `performTouchInput { swipeUp() }`, `performMouseInput { scroll(1f) }`, `performKeyInput { pressKey(Key.Enter) }` | `perform_click()`, `perform_touch_input(\|touch\| touch.swipe_up())`, `perform_mouse_input(\|mouse\| mouse.scroll(1.0, ScrollWheel::Vertical))`, `perform_key_input(\|keys\| keys.press_key(Key::Enter))` |
| `assertIsDisplayed()`, `assertWidthIsEqualTo(48.dp)`, `assertPositionInRootIsEqualTo(8.dp, 16.dp)` | `assert_is_displayed()`, `assert_width_is_equal_to(48.dp())`, `assert_position_in_root_is_equal_to(8.dp(), 16.dp())` |
| `captureToImage().assertPixels { Color.Red }` | `capture_to_image().assert_pixels(None, \|_\| Some(Color::RED))`, after `set_capture_renderer(compose_skia::raster_colors)` (the test crate depends on compose-skia) |
| `mainClock.advanceTimeBy(100)`, `waitUntil { … }`, `runOnIdle { … }` | `advance_time_by(Duration::from_millis(100))`, `wait_until(\|ui\| …)`, `run_on_idle(\|ui\| …)` |
| `runComposeUiTest { setContent { App() }; … }` | `run_compose_ui_test(\|ui\| { ui.set_content(App); … })` |
| `StateRestorationTester(rule).emulateSavedInstanceStateRestore()` | `StateRestorationTester::new()`, `tester.set_content(&mut ui, App)`, `tester.emulate_save_and_restore(&mut ui)` |

Also `perform_text_input`, `perform_key_press`, `perform_semantics_action`, `perform_scroll_to`, `request_focus`, and on the rule `type_text`, `press_key`. Dumps for debugging: `print_to_string`, `dump_tree()`, `dump_semantics()`, `dump_display_list()`. [docs/09](../09-testing.md) lists the whole API.

Recomposition counts check what an event recomposed, which Compose tests can't easily do:

<!-- snippet: tests/suite/testing.rs#recompositions -->
```rust
#[test]
fn a_click_recomposes_only_the_button_content() {
    let mut ui = compose_test(state::Counter);
    let before = ui.recomposition_counts();
    ui.on_node_with_text("Clicked 0 times").perform_click();
    let ran = ui.recomposition_counts().since(&before);
    // The text reads the count inside the button's content: `Counter` itself skips.
    assert_eq!(ran.scopes_named("Counter"), 0, "{:?}", ran.names());
}
```

The rule's clock is Compose's test clock: it starts at 0, frames come 16 ms after something needs one (a state change recomposes at the next frame), and waiting for idle advances it a frame at a time while frames are needed, as `mainClock.autoAdvance`, but not past `delay`s; a rule made with `with_manual_clock` advances only when told:

<!-- snippet: tests/suite/testing.rs#clock -->
```rust
#[test]
fn the_countdown_counts_down_on_the_test_clock() {
    // `mainClock.autoAdvance = false`: time moves only when the test says.
    let mut ui = ComposeTestRule::with_manual_clock(|| effects::Countdown(3));
    ui.on_node_with_text("3 s").assert_exists();
    // The delay ends at 1 s; the frame that shows the change comes 16 ms later, at 1,016 ms, as
    // Compose's. `advance_time_by` rounds up to whole frames: 1 s is 1,008 ms, a frame short.
    ui.advance_time_by(Duration::from_secs(1));
    ui.on_node_with_text("3 s").assert_exists();
    ui.advance_time_by_frame();
    ui.on_node_with_text("2 s").assert_exists();
    // `compose_test`'s clock advances by itself while frames are needed, but not past delays.
    let mut ui = compose_test(|| effects::Countdown(3));
    ui.advance_time_by(Duration::from_secs(3));
    ui.on_node_with_text("0 s").assert_exists();
}
```

`ui.host()` is the headless `UiHost` the rule drives: the core every platform wraps. It takes window-level input and lifecycle calls (`on_back_pressed`, `save_state` and `restore_state`, `props()`, `set_configuration`, `perform_ime_action`, `send_pointer`, `resize`). It's in `compose::ui::platform`, outside the stable surface.

<!-- snippet: tests/suite/testing.rs#host -->
```rust
#[test]
fn the_back_stack_survives_recreation() {
    // The headless host the test rule wraps: what an Android view or a desktop window drives.
    let mut ui = compose_test(navigation::App);
    ui.on_node_with_text("Message 3").perform_click();
    // What the Android view saves in its instance state when the activity goes away…
    let saved = ui.host().save_state();
    drop(ui);
    // …and gives a new composition when it's recreated.
    let host = UiHost::headless(400, 800);
    host.restore_state(saved);
    let mut ui = ComposeTestRule::with_host(host, navigation::App);
    ui.on_node_with_content_description("Star").assert_exists(); // still on the message
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_text("Message 3").assert_exists(); // and back goes to the inbox
}
```

Event scripts replay a session from a text file and compare its transcript (each command and what it printed) with an expected file, as the examples' tests do (`examples/counter/tests/counter.script`):

<!-- snippet: tests/navigation.script -->
```text
# The inbox, then a message: its screen replaces the inbox.
assert "Settings"
click "Message 3"
assert_not "Settings"
semantics
```

<!-- snippet: tests/suite/testing.rs#script -->
```rust
#[test]
fn the_session_matches_its_transcript() {
    let mut ui = compose_test(navigation::App);
    // Runs tests/navigation.script and compares the transcript with navigation.script.expected
    // (`UPDATE_SCRIPTS=1 cargo test` writes it).
    check_script(&mut ui, concat!(env!("CARGO_MANIFEST_DIR"), "/tests/navigation.script"));
}
```

The commands are `click "text"` or `click tag:name`, `type tag:name "text"`, `key Tab`, `assert "text"`, `assert_not "text"`, `frame`, `dump`, `semantics` and `display`. On Android, the Kotlin side can be tested with Compose's own test rule around the `ComposeRsView`; the Rust side is tested headless, as above.
