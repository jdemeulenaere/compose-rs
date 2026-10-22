#[composable]
pub fn Button(
    on_click: impl Fn() + 'static,
    #[default(Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    content: impl Fn() + 'static,
) {
    Surface(modifier, enabled, content);
}

#[composable]
fn Counter() {
    let count = remember(|| mutable_state_of(0));
    Button!(on_click = move || count.update(|c| c + 1), enabled = count.get() > 0, {
        Text!("Count: {}", count.get());
    });
    Toolbar(callback(move || count.set(0)));
}

#[composable]
fn Menu(drawer: DrawerState, title: String, shown: Rc<Vec<u32>>) {
    let scope = remember_coroutine_scope();
    IconButton!(on_click = move || {
        scope.launch(async move { drawer.open().await });
    }, {
        Text!(title);
    });
    LazyColumn!(state = drawer, move |list: LazyListScope| {
        list.items(shown.len(), move |index| Text!("{title} {}", shown[index]));
        Show(drawer);
    });
}
