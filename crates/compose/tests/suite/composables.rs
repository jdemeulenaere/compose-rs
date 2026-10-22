//! Phase A's runtime scenarios, with composables written normally: `#[composable]` functions.
//! Only `Leaf`, which emits a node, is a hand-written primitive (as Compose's `ComposeNode`).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use compose::runtime::platform::*;
use compose::runtime::*;

// ---- Test support --------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct Label(String);

impl fmt::Debug for Label {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type Tree = NodeTree<Label>;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SERIAL: Cell<u32> = const { Cell::new(0) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn next_serial() -> u32 {
    SERIAL.with(|serial| serial.replace(serial.get() + 1))
}

fn new_composition() -> Composition {
    take_log();
    SERIAL.with(|serial| serial.set(0));
    Composition::new(Tree::new())
}

fn tree(composition: &Composition) -> String {
    composition.with_applier(|tree: &mut Tree| tree.dump())
}

struct Token(String);

impl Token {
    fn new(name: impl Into<String>) -> Rc<Token> {
        let name = name.into();
        log(format!("create {name}"));
        Rc::new(Token(name))
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        log(format!("drop {}", self.0));
    }
}

/// Emits a leaf node: a hand-written primitive, as Compose's `ComposeNode`.
fn Leaf(label: &str) {
    let composer = current();
    let node = composer.start_node(key_of("composables::Leaf"), || Label(label.to_string()));
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    composer.end_group();
}

// ---- Composables ---------------------------------------------------------------------------

#[composable]
fn Text(text: String) {
    log(format!("Text({text})"));
    Leaf(&text);
}

#[composable]
fn Reader(name: &'static str, state: MutableState<i32>) {
    log(name);
    Leaf(&format!("{name}={}", state.get()));
}

#[composable]
fn Row(id: u32) {
    log(format!("Row({id})"));
    let serial = remember(next_serial);
    Leaf(&format!("{id}:t{serial}"));
}

// ---- Scenarios -----------------------------------------------------------------------------

#[test]
fn initial_composition_and_reader_scopes() {
    #[composable]
    fn Screen(a: MutableState<i32>, b: MutableState<i32>) {
        log("Screen");
        Reader("a", a);
        Reader("b", b);
        Text("c".into());
    }
    let composition = new_composition();
    let (a, b) = (mutable_state_of(0), mutable_state_of(0));
    let (a_handle, b_handle) = (a.handle(), b.handle());
    composition.set_content(move || Screen(a_handle, b_handle));
    assert_eq!(tree(&composition), "a=0\nb=0\nc\n");
    assert_eq!(take_log(), ["Screen", "a", "b", "Text(c)"]);
    a.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["a"]);
    assert_eq!(tree(&composition), "a=1\nb=0\nc\n");
    composition.validate();
}

#[test]
fn unchanged_arguments_skip_and_changed_ones_recompose() {
    #[composable]
    fn Parent(x: MutableState<i32>) {
        log("Parent");
        Text("constant".into());
        Text(format!("x={}", x.get()));
    }
    let composition = new_composition();
    let x = mutable_state_of(0);
    let handle = x.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    x.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["Parent", "Text(x=1)"]);
    assert_eq!(tree(&composition), "constant\nx=1\n");
}

#[test]
fn a_scope_recomposes_alone_below_a_skipped_parent() {
    #[composable]
    fn Wrapper(state: MutableState<i32>) {
        log("Wrapper");
        Leaf("before");
        Reader("r", state);
        Leaf("after");
    }
    let composition = new_composition();
    let state = mutable_state_of(0);
    let handle = state.handle();
    composition.set_content(move || Wrapper(handle));
    take_log();
    let groups = composition.group_count();
    for value in 1..=5 {
        state.set(value);
        composition.recompose();
        assert_eq!(take_log(), ["r"]);
        assert_eq!(composition.group_count(), groups);
    }
    assert_eq!(tree(&composition), "before\nr=5\nafter\n");
    composition.validate();
}

#[test]
fn branches_get_region_groups_automatically() {
    #[composable]
    fn Holder(flag: MutableState<bool>) {
        if flag.get() {
            let token = remember(|| Token::new(format!("on{}", next_serial())));
            Leaf(&token.0);
        } else {
            Leaf("off");
        }
    }
    let composition = new_composition();
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    composition.set_content(move || Holder(handle));
    flag.set(false);
    composition.recompose();
    assert_eq!(tree(&composition), "off\n");
    flag.set(true);
    composition.recompose();
    assert_eq!(tree(&composition), "on1\n");
    assert_eq!(take_log(), ["create on0", "drop on0", "create on1"]);
    composition.validate();
}

#[test]
fn match_arms_get_region_groups() {
    #[derive(Clone, Copy, PartialEq, ComposeArg)]
    enum Mode {
        First,
        Second,
    }
    #[composable]
    fn Switch(mode: MutableState<Mode>) {
        match mode.get() {
            Mode::First => {
                let serial = remember(next_serial);
                Leaf(&format!("first{serial}"));
            }
            Mode::Second => {
                let serial = remember(next_serial);
                Leaf(&format!("second{serial}"));
            }
        }
    }
    let composition = new_composition();
    let mode = mutable_state_of(Mode::First);
    let handle = mode.handle();
    composition.set_content(move || Switch(handle));
    mode.set(Mode::Second);
    composition.recompose();
    mode.set(Mode::First);
    composition.recompose();
    // Each arm has its own state: switching back creates a new one.
    assert_eq!(tree(&composition), "first2\n");
}

#[test]
fn keyed_items_keep_their_state_when_moved() {
    #[composable]
    fn Column(list: MutableState<Vec<u32>>) {
        list.with(|ids| {
            for &id in ids {
                key(id, || Row(id));
            }
        })
    }
    let composition = new_composition();
    let list = mutable_state_of(vec![1u32, 2, 3]);
    let handle = list.handle();
    composition.set_content(move || Column(handle));
    take_log();
    list.set(vec![3, 1, 2]);
    composition.recompose();
    assert_eq!(tree(&composition), "3:t2\n1:t0\n2:t1\n");
    assert!(take_log().is_empty(), "moved rows skip");
    list.set(vec![2, 4]);
    composition.recompose();
    assert_eq!(tree(&composition), "2:t1\n4:t3\n");
    composition.validate();
}

#[test]
fn unkeyed_items_match_by_position() {
    #[composable]
    fn Column(list: MutableState<Vec<u32>>) {
        for id in list.get() {
            Row(id);
        }
    }
    let composition = new_composition();
    let list = mutable_state_of(vec![1u32, 2, 3]);
    let handle = list.handle();
    composition.set_content(move || Column(handle));
    list.set(vec![0, 1, 2, 3]);
    composition.recompose();
    assert_eq!(tree(&composition), "0:t0\n1:t1\n2:t2\n3:t3\n");
}

#[test]
fn a_scope_whose_node_count_changes_keeps_siblings_in_place() {
    #[composable]
    fn Many(count: MutableState<i32>) {
        for index in 0..count.get() {
            Leaf(&format!("item{index}"));
        }
    }
    let composition = new_composition();
    let count = mutable_state_of(1);
    let handle = count.handle();
    composition.set_content(move || {
        Leaf("first");
        Many(handle);
        Leaf("last");
    });
    count.set(3);
    composition.recompose();
    assert_eq!(tree(&composition), "first\nitem0\nitem1\nitem2\nlast\n");
    count.set(0);
    composition.recompose();
    assert_eq!(tree(&composition), "first\nlast\n");
    composition.validate();
}

#[test]
fn a_removed_scope_never_runs_and_none_runs_twice() {
    #[composable]
    fn Parent(show: MutableState<bool>, value: MutableState<i32>) {
        log("Parent");
        if show.get() {
            Reader("child", value);
        }
        Child(value.get(), value);
    }
    #[composable]
    fn Child(copy: i32, value: MutableState<i32>) {
        log(format!("Child {copy} {}", value.get()));
    }
    let composition = new_composition();
    let show = mutable_state_of(true);
    let value = mutable_state_of(0);
    let (show_handle, value_handle) = (show.handle(), value.handle());
    composition.set_content(move || Parent(show_handle, value_handle));
    take_log();
    value.set(1);
    show.set(false);
    composition.recompose();
    assert_eq!(take_log(), ["Parent", "Child 1 1"]);
}

#[test]
fn early_exits_keep_groups_balanced() {
    #[composable]
    fn Early(stop: MutableState<i32>) {
        Leaf("start");
        if stop.get() == 1 {
            Leaf("one");
            return;
        }
        for index in 0..3 {
            if index == stop.get() - 2 {
                Leaf("skip");
                continue;
            }
            if index == 2 && stop.get() == 5 {
                break;
            }
            Leaf(&format!("loop{index}"));
        }
        Leaf("end");
    }
    let composition = new_composition();
    let stop = mutable_state_of(0);
    let handle = stop.handle();
    composition.set_content(move || Early(handle));
    assert_eq!(tree(&composition), "start\nloop0\nloop1\nloop2\nend\n");
    for (value, expected) in [
        (1, "start\none\n"),
        (2, "start\nskip\nloop1\nloop2\nend\n"),
        (5, "start\nloop0\nloop1\nend\n"),
        (4, "start\nloop0\nloop1\nskip\nend\n"),
        (3, "start\nloop0\nskip\nloop2\nend\n"),
        (0, "start\nloop0\nloop1\nloop2\nend\n"),
    ] {
        stop.set(value);
        composition.recompose();
        assert_eq!(tree(&composition), expected, "stop = {value}");
        composition.validate();
    }
}

#[test]
fn non_restartable_composables_return_values_and_propagate_errors() {
    #[composable]
    fn Parse(text: &str) -> Result<i32, std::num::ParseIntError> {
        let value: i32 = text.parse()?;
        Leaf(&format!("parsed {value}"));
        Ok(value)
    }
    #[composable]
    fn Screen(text: MutableState<String>) {
        let result = text.with(|text| Parse(text));
        Leaf(&format!("{result:?}"));
    }
    let composition = new_composition();
    let text = mutable_state_of(String::from("12"));
    let handle = text.handle();
    composition.set_content(move || Screen(handle));
    assert_eq!(tree(&composition), "parsed 12\nOk(12)\n");
    text.set(String::from("x"));
    composition.recompose();
    assert!(tree(&composition).starts_with("Err("));
    text.set(String::from("7"));
    composition.recompose();
    assert_eq!(tree(&composition), "parsed 7\nOk(7)\n");
    composition.validate();
}

#[test]
fn derived_compose_args_skip_by_field() {
    #[derive(Clone, ComposeArg)]
    struct Todo {
        id: u64,
        title: Rc<str>,
        done: MutableState<bool>,
    }
    #[composable]
    fn TodoRow(todo: Todo) {
        log(format!("TodoRow({})", todo.id));
        Leaf(&format!("{}{}", todo.title, if todo.done.get() { " ✓" } else { "" }));
    }
    #[composable]
    fn TodoList(todos: MutableState<Vec<Todo>>) {
        todos.with(|todos| {
            for todo in todos {
                key(todo.id, || TodoRow(todo.clone()));
            }
        });
    }
    let composition = new_composition();
    let owners: Vec<OwnedState<bool>> = (0..3).map(|_| mutable_state_of(false)).collect();
    let todos: Vec<Todo> = owners
        .iter()
        .enumerate()
        .map(|(index, done)| Todo {
            id: index as u64,
            title: Rc::from(format!("t{index}")),
            done: done.handle(),
        })
        .collect();
    let list = mutable_state_of_with(todos, never_equal_policy());
    let handle = list.handle();
    composition.set_content(move || TodoList(handle));
    take_log();
    owners[1].set(true);
    composition.recompose();
    assert_eq!(take_log(), ["TodoRow(1)"]);
    // A rebuilt but equal list: every row skips, as each field is the same.
    list.edit(|todos| *todos = todos.clone());
    composition.recompose();
    assert!(take_log().is_empty());
    assert_eq!(tree(&composition), "t0\nt1 ✓\nt2\n");
}

#[test]
fn generic_and_pattern_parameters() {
    #[composable]
    fn Pair((first, second): (i32, i32)) {
        Leaf(&format!("{first},{second}"));
    }
    #[composable]
    fn Items<T: ComposeArg + Clone + fmt::Display>(items: Rc<Vec<T>>) {
        for item in items.iter() {
            Leaf(&item.to_string());
        }
    }
    let composition = new_composition();
    let pair = mutable_state_of((1, 2));
    let handle = pair.handle();
    composition.set_content(move || {
        Pair(handle.get());
        Items(Rc::new(vec!["a", "b"]));
    });
    pair.set((3, 4));
    composition.recompose();
    assert_eq!(tree(&composition), "3,4\na\nb\n");
}

#[test]
fn effects_and_observers() {
    struct Observer(&'static str);
    impl RememberObserver for Observer {
        fn on_remembered(&self) {
            log(format!("remembered {}", self.0));
        }
        fn on_forgotten(&self) {
            log(format!("forgotten {}", self.0));
        }
    }
    #[composable]
    fn Effects(value: MutableState<i32>) {
        let current = value.get();
        if current < 2 {
            remember_observer(|| Observer("low"));
        }
        SideEffect(move || log(format!("effect {current}")));
        DisposableEffect(current / 2, move |scope| {
            log(format!("start {}", current / 2));
            scope.on_dispose(move || log(format!("stop {}", current / 2)))
        });
    }
    let composition = new_composition();
    let value = mutable_state_of(0);
    let handle = value.handle();
    composition.set_content(move || Effects(handle));
    assert_eq!(take_log(), ["remembered low", "start 0", "effect 0"]);
    value.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["effect 1"]);
    value.set(2);
    composition.recompose();
    assert_eq!(take_log(), ["forgotten low", "stop 0", "start 1", "effect 2"]);
    drop(composition);
    assert_eq!(take_log(), ["stop 1"]);
}

thread_local! {
    static COMPARISONS: Cell<u32> = const { Cell::new(0) };
}

/// An argument that counts its comparisons.
#[derive(Clone)]
struct Counted(u32);

impl ComposeArg for Counted {
    fn same(&self, other: &Self) -> bool {
        COMPARISONS.with(|count| count.set(count.get() + 1));
        self.0 == other.0
    }
}

#[test]
fn a_skipped_composable_costs_one_comparison_per_argument() {
    #[composable]
    fn ThreeArguments(a: Counted, b: Counted, c: Counted) {
        log("ThreeArguments");
    }
    #[composable]
    fn Parent(trigger: MutableState<i32>) {
        trigger.get();
        ThreeArguments(Counted(1), Counted(2), Counted(3));
    }
    let composition = new_composition();
    let trigger = mutable_state_of(0);
    let handle = trigger.handle();
    composition.set_content(move || Parent(handle));
    take_log();
    COMPARISONS.with(|count| count.set(0));
    trigger.set(1);
    composition.recompose();
    assert!(take_log().is_empty(), "the child skips");
    assert_eq!(COMPARISONS.with(Cell::get), 3);
}
