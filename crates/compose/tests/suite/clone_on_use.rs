//! An API audit's findings: code an app writes every day that needed manual `.clone()`s or
//! failed with confusing errors (docs/03, "Captures a lambda would move"; the call macro rules).
//!
//! - A lambda the macro memoizes is `Fn` and built from clones of its captures (D115): it can't
//!   give a capture away. A capture given by itself as an argument (`Label!(name)`,
//!   `Show(name)`, `sink.push(name)`) is passed as a clone, and a `move` closure or an
//!   `async move` block in a closure gets clones of the closure's captures it names.
//! - A local given to a call macro by name and captured by a lambda of the same call is passed
//!   as a clone, so the lambda can still capture it whatever the parameters' order.
//! - Strings and lists in data models: `&String` converts into `SharedString`,
//!   `ImmutableList<T>` is a `ComposeArg`, and `#[compose_arg(eq)]` compares a field by value.
//! - Call macros: an optional callback takes a callback or a closure given by name; a closure
//!   binds to the trailing parameter only once the required ones before it are given; the
//!   trailing content may follow named arguments.

#![allow(non_snake_case, dead_code)]
// A statement macro wrapped with its clones keeps its attributes on the block.
#![deny(unused_attributes)]

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use compose::runtime::*;
use compose::ui::SharedString;

// ---- Test support --------------------------------------------------------------------------

type Task = Pin<Box<dyn Future<Output = ()>>>;

thread_local! {
    static TASKS: RefCell<Vec<Task>> = const { RefCell::new(Vec::new()) };
    static CLICKS: RefCell<Vec<Callback>> = const { RefCell::new(Vec::new()) };
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// Runs every launched task once (they finish at once).
fn run_tasks() {
    let tasks = TASKS.with(|tasks| std::mem::take(&mut *tasks.borrow_mut()));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    for mut task in tasks {
        assert!(task.as_mut().poll(&mut context).is_ready());
    }
}

/// Clicks every button composed so far, in order.
fn click_all() {
    let clicks = CLICKS.with(|clicks| std::mem::take(&mut *clicks.borrow_mut()));
    for click in clicks {
        click();
    }
}

/// As `CoroutineScope`: launches futures.
#[derive(Clone, Copy, ComposeArg)]
struct Scope;

impl Scope {
    fn launch(&self, future: impl Future<Output = ()> + 'static) {
        TASKS.with(|tasks| tasks.borrow_mut().push(Box::pin(future)));
    }
}

/// An `Rc`-like holder, as `DrawerState`: `Clone`, not `Copy`.
#[derive(Clone, ComposeArg)]
struct Drawer(Rc<Cell<u32>>);

impl Drawer {
    async fn open(&self) {
        self.0.set(self.0.get() + 1);
    }
}

/// Not `Clone`: what a lambda makes for itself is still moved.
struct Token;

fn consume(_: Token) {}

#[composable]
fn Button(on_click: impl Fn() + 'static, content: impl Fn() + 'static) {
    CLICKS.with(|clicks| clicks.borrow_mut().push(on_click.clone()));
    content();
}

#[composable]
fn Section(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Label(#[format] text: String) {
    log(text);
}

#[composable]
fn Show(text: String) {
    log(format!("show {text}"));
}

fn composition() -> Composition {
    take_log();
    CLICKS.with(|clicks| clicks.borrow_mut().clear());
    Composition::new(NodeTree::<()>::new())
}

// ---- Captures a lambda would move ----------------------------------------------------------

/// The audit's case: an `async move` in a click handler names an `Rc`-like capture.
#[composable]
fn Menu(drawer: Drawer, scope: Scope) {
    Button!(
        on_click = move || {
            scope.launch(async move {
                drawer.open().await;
            });
        },
        {
            Label!("Menu");
        }
    );
}

#[test]
fn an_async_move_block_in_a_lambda_gets_clones_of_its_captures() {
    let composition = composition();
    let drawer = Drawer(Rc::new(Cell::new(0)));
    let shown = drawer.clone();
    composition.set_content(move || Menu(shown.clone(), Scope));
    let clicks = CLICKS.with(|clicks| clicks.borrow().clone());
    for click in clicks.iter().chain(&clicks) {
        click();
    }
    run_tasks();
    assert_eq!(drawer.0.get(), 2);
}

/// A lazy list's item: its key, index and content.
type ListItem = (u64, usize, Rc<dyn Fn(usize)>);

/// As a lazy list's scope: item closures are plain `Fn` closures, not memoized.
struct ListScope {
    items: RefCell<Vec<ListItem>>,
}

impl ListScope {
    fn items_keyed(
        &self,
        count: usize,
        key: impl Fn(usize) -> u64 + 'static,
        content: impl Fn(usize) + 'static,
    ) {
        let content: Rc<dyn Fn(usize)> = Rc::new(content);
        for index in 0..count {
            self.items.borrow_mut().push((key(index), index, content.clone()));
        }
    }

    fn item(&self, content: impl Fn() + 'static) {
        self.items.borrow_mut().push((0, 0, Rc::new(move |_| content())));
    }
}

#[composable]
fn List(content: impl Fn(Rc<ListScope>) + 'static) {
    let scope = Rc::new(ListScope { items: RefCell::new(Vec::new()) });
    content(scope.clone());
    for (key, index, item) in scope.items.borrow().iter() {
        log(format!("key {key}"));
        item(*index);
    }
}

/// The audit's other case: a captured `Rc` and `String` used by item closures.
#[composable]
fn Cards(shown: Rc<Vec<u32>>, title: String) {
    List!(move |scope: Rc<ListScope>| {
        scope.item(move || Label!(title));
        scope.items_keyed(
            shown.len(),
            move |index| u64::from(shown[index]),
            move |index| {
                Label!("{title} {}", shown[index]);
            },
        );
    });
}

#[test]
fn item_closures_in_a_lambda_get_clones_of_its_captures() {
    let composition = composition();
    composition.set_content(|| Cards(Rc::new(vec![7, 8]), "card".into()));
    assert_eq!(take_log(), ["key 0", "card", "key 7", "card 7", "key 8", "card 8"]);
}

/// A capture given by itself as an argument, to a call macro, a function or a method.
#[composable]
fn Greeting(name: String, sink: Rc<RefCell<Vec<String>>>) {
    Section!({
        #[allow(unused_braces)]
        Label!(name);
        Show(name);
        sink.borrow_mut().push(name);
    });
    Label!("Goodbye {name}");
}

#[test]
fn a_capture_given_as_an_argument_is_cloned() {
    let composition = composition();
    let sink = Rc::new(RefCell::new(Vec::new()));
    let handle = sink.clone();
    composition.set_content(move || Greeting("Ada".into(), handle.clone()));
    assert_eq!(take_log(), ["Ada", "show Ada", "Goodbye Ada"]);
    assert_eq!(*sink.borrow(), ["Ada"]);
}

#[composable]
fn Holder(value: Rc<String>, content: impl Fn() + 'static) {
    log(format!("holds {value}"));
    content();
}

/// A local given by name and captured by a lambda of the same call: the value comes first, so
/// it would move before the lambda captures it.
#[composable]
fn Holders(data: Rc<String>) {
    Holder!(value = data, {
        Label!("{}", data);
    });
    Holder!(data, {
        Label!("again {}", data);
    });
}

#[test]
fn a_local_given_to_a_call_and_captured_by_its_lambda_is_cloned_for_the_value() {
    let composition = composition();
    let data = Rc::new(String::from("x"));
    composition.set_content(move || Holders(data.clone()));
    assert_eq!(take_log(), ["holds x", "x", "holds x", "again x"]);
}

/// `callback(…)` in a lambda: its `async move` gets clones too, and the callback is passed on by
/// name and called from another lambda.
#[composable]
fn Drawers(drawer: Drawer, scope: Scope) {
    Section!({
        let close = callback(move || {
            scope.launch(async move { drawer.open().await });
        });
        Button!(on_click = close, {
            Label!("Close");
        });
        Button!(on_click = move || close(), {
            Label!("Close again");
        });
    });
}

#[test]
fn a_callback_made_in_a_lambda_clones_into_its_async_block() {
    let composition = composition();
    let drawer = Drawer(Rc::new(Cell::new(0)));
    let shown = drawer.clone();
    composition.set_content(move || Drawers(shown.clone(), Scope));
    click_all();
    run_tasks();
    assert_eq!(drawer.0.get(), 2);
}

fn remember_opened(drawer: Drawer) -> u32 {
    drawer.0.get()
}

/// What a lambda makes for itself is moved, not cloned (`Token` isn't `Clone`), and a capture
/// given to a `remember_…` function is cloned like any argument.
#[composable]
fn Owned(drawer: Drawer, scope: Scope) {
    Button!(
        on_click = move || {
            let token = Token;
            scope.launch(async move { consume(token) });
            let token = Token;
            let take = move || consume(token);
            take();
        },
        {
            let opened = remember_opened(drawer);
            Label!("opened {opened}");
        }
    );
}

#[test]
fn what_a_lambda_makes_is_moved() {
    let composition = composition();
    composition.set_content(|| Owned(Drawer(Rc::new(Cell::new(3))), Scope));
    click_all();
    run_tasks();
    assert_eq!(take_log(), ["opened 3"]);
}

/// Memoization is unchanged: a lambda whose captures are the same is the same callback.
#[composable]
fn Parent(name: MutableState<String>, ticks: MutableState<u32>) {
    let current = name.get();
    log(format!("parent {}", ticks.get()));
    Section!({
        Show(current);
    });
}

#[test]
fn cloned_captures_keep_lambdas_memoized() {
    let composition = composition();
    let name = mutable_state_of(String::from("a"));
    let ticks = mutable_state_of(0);
    let (name_handle, ticks_handle) = (name.handle(), ticks.handle());
    composition.set_content(move || Parent(name_handle, ticks_handle));
    assert_eq!(take_log(), ["parent 0", "show a"]);
    ticks.set(1);
    composition.recompose();
    // The parent ran again; its content lambda is the same, so `Section` skipped.
    assert_eq!(take_log(), ["parent 1"]);
    name.set(String::from("b"));
    composition.recompose();
    assert_eq!(take_log(), ["parent 1", "show b"]);
}

// ---- Strings and lists in data models ------------------------------------------------------

#[composable]
fn Caption(#[format] text: SharedString) {
    log(text.to_string());
}

#[composable]
fn Captions(name: String) {
    Caption!(&name);
    Section!({
        Caption!(name);
    });
    Caption!(name);
}

#[test]
fn a_string_reference_converts_into_a_shared_string() {
    let composition = composition();
    composition.set_content(|| Captions("Ada".into()));
    assert_eq!(take_log(), ["Ada", "Ada", "Ada"]);
    let text: Rc<str> = Rc::from("shared");
    assert_eq!(SharedString::from(&text).as_str(), "shared");
    let shared = SharedString::from(text.clone());
    assert_eq!(SharedString::from(&shared).as_str(), "shared");
}

#[derive(Clone, ComposeArg)]
struct Item {
    title: String,
    tags: ImmutableList<String>,
    #[compose_arg(eq)]
    scores: Vec<u32>,
}

#[composable]
fn ItemRow(item: Item) {
    log(format!("{}: {} ({:?})", item.title, item.tags.join(", "), item.scores));
}

#[test]
fn data_models_hold_lists() {
    let composition = composition();
    let tags = ImmutableList::from(vec![String::from("a"), String::from("b")]);
    let item = mutable_state_of_with(
        Item { title: "x".into(), tags: tags.clone(), scores: vec![1] },
        NeverEqualPolicy,
    );
    let handle = item.handle();
    composition.set_content(move || ItemRow(handle.get()));
    assert_eq!(take_log(), ["x: a, b ([1])"]);
    // The same list and equal scores: skipped.
    item.set(Item { title: "x".into(), tags: tags.clone(), scores: vec![1] });
    composition.recompose();
    assert!(take_log().is_empty());
    // Equal contents in another list: compared by identity, so changed.
    let tags = vec![String::from("a"), String::from("b")];
    item.set(Item { title: "x".into(), tags: tags.into(), scores: vec![1] });
    composition.recompose();
    assert_eq!(take_log(), ["x: a, b ([1])"]);
}

// ---- Call macros ---------------------------------------------------------------------------

#[composable]
fn Card(
    #[default(None)] on_click: Option<impl Fn() + 'static>,
    #[default(true)] enabled: bool,
    content: impl Fn() + 'static,
) {
    if let Some(on_click) = &on_click {
        CLICKS.with(|clicks| clicks.borrow_mut().push(on_click.clone()));
    }
    log(format!("card {enabled}"));
    content();
}

/// Forwarding a callback into an optional one, as Kotlin passes a lambda for a nullable one.
#[composable]
fn ClickableCard(on_click: impl Fn() + 'static) {
    Card!(on_click = on_click, {
        Label!("forwarded");
    });
    let local = move || log("local clicked");
    Card!(on_click = local, enabled = false, {
        Label!("local");
    });
}

#[test]
fn an_optional_callback_takes_a_callback_or_a_closure() {
    let composition = composition();
    composition.set_content(|| ClickableCard(callback(|| log("clicked"))));
    assert_eq!(take_log(), ["card true", "forwarded", "card false", "local"]);
    click_all();
    assert_eq!(take_log(), ["clicked", "local clicked"]);
}

#[composable]
fn Field(
    value: String,
    on_value_change: impl Fn(String) + 'static,
    #[default(true)] enabled: bool,
    #[default(|inner: Callback| inner())] decoration_box: impl Fn(Callback) + 'static,
) {
    log(format!("field {value} {enabled}"));
    on_value_change(format!("{value}!"));
    decoration_box(callback(|| log("inner")));
}

/// A closure binds to the trailing parameter only once the required ones before it are given:
/// here it is `on_value_change`, given positionally.
#[composable]
fn Fields(value: MutableState<String>) {
    Field!(value.get(), move |text| log(format!("set {text}")));
    Field!(value.get(), move |text| log(format!("changed to {text}")), |inner| {
        log("decorated");
        inner();
    });
}

#[test]
fn a_closure_is_trailing_only_once_the_required_parameters_are_given() {
    let composition = composition();
    let value = mutable_state_of(String::from("a"));
    let handle = value.handle();
    composition.set_content(move || Fields(handle));
    assert_eq!(
        take_log(),
        ["field a true", "set a!", "inner", "field a true", "changed to a!", "decorated", "inner"]
    );
}

#[composable]
fn Layout(#[default(1)] modifier: u32, measure_policy: u32, content: impl Fn() + 'static) {
    log(format!("layout {modifier} {measure_policy}"));
    content();
}

/// The trailing content may follow named arguments, as a block or a closure, and may come
/// before named arguments that follow it.
#[composable]
fn Trailing(content: impl Fn() + 'static) {
    Layout!(modifier = 2, measure_policy = 3, move || content());
    Layout!(measure_policy = 4, { Label!("block") });
    Card!(on_click = || log("unused"), { Label!("middle") }, enabled = false);
}

#[test]
fn the_trailing_content_may_follow_named_arguments() {
    let composition = composition();
    composition.set_content(|| Trailing(callback(|| log("content"))));
    assert_eq!(
        take_log(),
        ["layout 2 3", "content", "layout 1 4", "block", "card false", "middle"]
    );
}
