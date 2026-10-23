//! Reusable groups, as Compose's `ReusableContent` and `ReusableContentHost` (D430): content
//! composed in the place of other content reuses its groups and reusable nodes, its remembered
//! values computed anew and its remember observers forgotten; deactivated content keeps its
//! groups and nodes, forgets its observers and runs nothing until reused.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::*;

#[derive(Clone, PartialEq)]
struct Label(String);

impl fmt::Debug for Label {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

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

fn serial() -> u32 {
    SERIAL.with(|serial| {
        let value = serial.get();
        serial.set(value + 1);
        value
    })
}

/// A node tree that logs what the runtime asks of it beyond insertions and moves.
struct Recording {
    tree: NodeTree<Label>,
}

impl Recording {
    fn label(&self, node: NodeRef) -> String {
        self.tree.get(node).map_or_else(|| "?".to_string(), |label| label.0.clone())
    }
}

impl Applier for Recording {
    type Node = Label;
    fn root(&self) -> NodeRef {
        self.tree.root()
    }
    fn insert(&mut self, parent: NodeRef, index: usize, node: Label) -> NodeRef {
        log(format!("insert {}", node.0));
        Applier::insert(&mut self.tree, parent, index, node)
    }
    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        Applier::remove(&mut self.tree, parent, index, count)
    }
    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        Applier::move_nodes(&mut self.tree, parent, from, to, count)
    }
    fn release(&mut self, node: NodeRef) {
        log(format!("release {}", self.label(node)));
        Applier::release(&mut self.tree, node)
    }
    fn reuse(&mut self, node: NodeRef) {
        log(format!("reuse {}", self.label(node)));
    }
    fn deactivate(&mut self, node: NodeRef) {
        log(format!("deactivate {}", self.label(node)));
    }
}

fn new_composition() -> Composition {
    take_log();
    SERIAL.with(|serial| serial.set(0));
    Composition::new(Recording { tree: NodeTree::new() })
}

fn tree(composition: &Composition) -> String {
    composition.with_applier(|recording: &mut Recording| recording.tree.dump())
}

fn node_refs(composition: &Composition) -> Vec<NodeRef> {
    composition.with_applier(|recording: &mut Recording| {
        let root = recording.tree.root();
        recording.tree.children(root).collect()
    })
}

/// Logs when it's remembered, forgotten or abandoned.
struct Observer(String);

impl RememberObserver for Observer {
    fn on_remembered(&self) {
        log(format!("remembered {}", self.0));
    }
    fn on_forgotten(&self) {
        log(format!("forgotten {}", self.0));
    }
    fn on_abandoned(&self) {
        log(format!("abandoned {}", self.0));
    }
}

/// Logs when it's dropped.
struct Token(String);

impl Drop for Token {
    fn drop(&mut self) {
        log(format!("drop {}", self.0));
    }
}

/// A reusable node (as `Layout`'s) labelled `label`, its children `content`'s nodes.
fn ReusableNode(label: &str, content: impl FnOnce()) {
    let composer = current();
    let node = composer.start_reusable_node(key_of("reusable::Node"), || Label(label.to_string()));
    composer.with_applier(|recording: &mut Recording| {
        let value = recording.tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    content();
    composer.end_group();
}

/// A node that isn't reusable (as `ComposeNode`'s): reused content replaces it.
fn PlainNode(label: &str) {
    let composer = current();
    let node = composer.start_node(key_of("reusable::Plain"), || Label(label.to_string()));
    composer.with_applier(|recording: &mut Recording| {
        let value = recording.tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    composer.end_group();
}

/// An item: a remembered token and serial, an observer, and a reusable node showing them.
fn Item(name: &str) {
    let label = name.to_string();
    let serial = remember(serial);
    let token = remember({
        let label = label.clone();
        move || Rc::new(Token(label))
    });
    assert_eq!(token.0, name, "a remembered value of other content");
    remember_observer({
        let label = label.clone();
        move || Observer(label)
    });
    ReusableNode(&format!("{name}#{serial}"), || {});
}

#[test]
fn content_with_another_key_reuses_the_groups_and_nodes_with_values_remembered_anew() {
    let composition = new_composition();
    let name = Rc::new(RefCell::new("a".to_string()));
    let content = name.clone();
    composition.set_content(move || {
        let name = content.borrow().clone();
        ReusableContent(name.clone(), || Item(&name));
    });
    assert_eq!(tree(&composition), "a#0\n");
    assert_eq!(take_log(), ["insert a#0", "remembered a"]);
    let before = node_refs(&composition);
    *name.borrow_mut() = "b".to_string();
    composition.set_content({
        let name = name.clone();
        move || {
            let name = name.borrow().clone();
            ReusableContent(name.clone(), || Item(&name));
        }
    });
    // The node kept and updated; the values remembered anew, the old ones forgotten and
    // dropped at the end of the pass, before the new observer is remembered.
    assert_eq!(tree(&composition), "b#1\n");
    assert_eq!(node_refs(&composition), before, "the node was kept");
    assert_eq!(take_log(), ["reuse a#0", "forgotten a", "drop a", "remembered b"]);
    composition.validate();
    drop(composition);
    assert_eq!(take_log(), ["release b#1", "forgotten b", "drop b"]);
}

#[test]
fn the_same_key_reuses_nothing() {
    let composition = new_composition();
    composition.set_content(|| ReusableContent("a", || Item("a")));
    take_log();
    let before = node_refs(&composition);
    composition.set_content(|| ReusableContent("a", || Item("a")));
    assert_eq!(tree(&composition), "a#0\n");
    assert_eq!(node_refs(&composition), before);
    assert_eq!(take_log(), Vec::<String>::new());
}

#[test]
fn reused_content_replaces_the_nodes_that_are_not_reusable() {
    let composition = new_composition();
    composition.set_content(|| ReusableContent(1, || PlainNode("plain a")));
    assert_eq!(take_log(), ["insert plain a"]);
    composition.set_content(|| ReusableContent(2, || PlainNode("plain b")));
    assert_eq!(tree(&composition), "plain b\n");
    assert_eq!(take_log(), ["insert plain b", "release plain a"]);
    composition.validate();
}

#[test]
fn keyed_groups_inside_reused_content_are_matched_by_their_keys() {
    let composition = new_composition();
    // `inner` keyed content in reused content: kept (and reset) when its key stays, new when
    // it changes.
    let set = |outer: &'static str, inner: &'static [&'static str]| {
        composition.set_content(move || {
            ReusableContent(outer, || {
                for name in inner {
                    key(*name, || Item(name));
                }
            })
        });
    };
    set("one", &["x", "y"]);
    take_log();
    let before = node_refs(&composition);
    set("two", &["y", "z"]);
    let log = take_log();
    // y's group moved and was reused, its value remembered anew; z is new; x is gone.
    assert_eq!(tree(&composition), "y#2\nz#3\n");
    assert_eq!(node_refs(&composition)[0], before[1], "y kept its node");
    assert!(log.contains(&"reuse y#1".to_string()), "{log:?}");
    assert!(log.contains(&"insert z#3".to_string()), "{log:?}");
    assert!(log.contains(&"release x#0".to_string()), "{log:?}");
    for event in ["forgotten x", "forgotten y", "drop x", "drop y", "remembered y", "remembered z"]
    {
        assert!(log.contains(&event.to_string()), "{event} in {log:?}");
    }
    composition.validate();
}

#[test]
fn deactivated_content_keeps_its_nodes_forgets_its_observers_and_runs_nothing() {
    let composition = new_composition();
    let active = mutable_state_of(true);
    let state = mutable_state_of(0);
    let (active_handle, state_handle) = (active.handle(), state.handle());
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    composition.set_content(move || {
        let runs = counted.clone();
        ReusableContentHost(active_handle.get(), move || {
            // A scope of its own, reading the state.
            Reader(state_handle, runs.clone());
        });
    });
    assert_eq!(tree(&composition), "reader 0\n");
    assert_eq!(runs.get(), 1);
    let before = node_refs(&composition);
    take_log();
    active.set(false);
    assert!(composition.recompose());
    // Deactivated: the observer forgotten (the plain value stays), the node kept, unchanged.
    assert_eq!(take_log(), ["deactivate reader 0", "forgotten reader"]);
    assert_eq!(tree(&composition), "reader 0\n");
    assert_eq!(node_refs(&composition), before);
    composition.validate();
    // Its scope was released: the state it read recomposes nothing.
    state.set(1);
    assert!(!composition.recompose());
    assert_eq!(runs.get(), 1);
    // Active again: reused, its value remembered anew.
    active.set(true);
    assert!(composition.recompose());
    assert_eq!(runs.get(), 2);
    assert_eq!(tree(&composition), "reader 1\n");
    assert_eq!(node_refs(&composition), before);
    assert_eq!(take_log(), ["reuse reader 0", "drop reader", "remembered reader"]);
    // And its new scope recomposes it.
    state.set(2);
    assert!(composition.recompose());
    assert_eq!(runs.get(), 3);
    assert_eq!(tree(&composition), "reader 2\n");
    composition.validate();
    drop(composition);
    assert_eq!(take_log(), ["release reader 2", "forgotten reader", "drop reader"]);
}

/// A hand-expanded restartable composable reading `state`.
fn Reader(state: MutableState<i32>, runs: Rc<Cell<u32>>) {
    let composer = current();
    composer.start_restart_group(key_of("reusable::Reader"));
    let changed = composer.update_kept(
        |kept: &(MutableState<i32>, Rc<Cell<u32>>)| kept.0 != state || !Rc::ptr_eq(&kept.1, &runs),
        || (state, runs.clone()),
    );
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        runs.set(runs.get() + 1);
        let value = state.get();
        remember(|| Rc::new(Token("reader".into())));
        remember_observer(|| Observer("reader".into()));
        ReusableNode(&format!("reader {value}"), || {});
    }
    composer.end_restart_group(|kept| {
        let (state, runs) =
            kept.downcast_ref::<(MutableState<i32>, Rc<Cell<u32>>)>().expect("kept").clone();
        Reader(state, runs)
    });
}

/// What a lazy list does: slots in keyed groups, each a host whose content is keyed by the item
/// it holds; the slots kept for reuse come after those holding items.
fn Slots(active: &[(u32, &str)], pooled: &[u32]) {
    for &(slot, item) in active {
        key(slot, || ReusableContentHost(true, || ReusableContent(item, || Item(item))));
    }
    for &slot in pooled {
        key(slot, || ReusableContentHost(false, || {}));
    }
}

#[test]
fn slots_deactivated_while_others_move_keep_their_content() {
    let composition = new_composition();
    let set = |active: &'static [(u32, &'static str)], pooled: &'static [u32]| {
        composition.set_content(move || Slots(active, pooled));
        composition.validate();
    };
    set(&[(1, "a"), (2, "b"), (3, "c"), (4, "d")], &[]);
    take_log();
    // b and c go and d comes, in a's slot: b's and c's slots kept for reuse, the others moved.
    set(&[(4, "d"), (1, "e")], &[2, 3]);
    let log = take_log();
    assert!(!log.iter().any(|event| event.starts_with("release")), "{log:?}");
    assert!(log.contains(&"deactivate b#1".to_string()), "{log:?}");
    assert!(log.contains(&"deactivate c#2".to_string()), "{log:?}");
    assert!(log.contains(&"reuse a#0".to_string()), "{log:?}");
    assert_eq!(tree(&composition), "d#3\ne#4\nb#1\nc#2\n");
    // c's slot reused by f, b's dropped: its node released, nothing else of it forgotten.
    set(&[(4, "d"), (1, "e"), (3, "f")], &[]);
    let log = take_log();
    assert_eq!(tree(&composition), "d#3\ne#4\nf#5\n");
    assert!(log.contains(&"reuse c#2".to_string()), "{log:?}");
    assert!(log.contains(&"release b#1".to_string()), "{log:?}");
    assert!(!log.contains(&"forgotten b".to_string()), "forgotten once: {log:?}");
    assert!(log.contains(&"drop b".to_string()) && log.contains(&"drop c".to_string()));
    drop(composition);
    let log = take_log();
    for item in ["d", "e", "f"] {
        assert!(log.contains(&format!("forgotten {item}")), "{log:?}");
        assert!(log.contains(&format!("drop {item}")), "{log:?}");
    }
    assert!(!log.contains(&"forgotten b".to_string()));
}

#[test]
fn a_deactivated_slot_reused_by_another_item_is_reset_once() {
    let composition = new_composition();
    composition.set_content(|| Slots(&[(1, "a")], &[]));
    composition.set_content(|| Slots(&[], &[1]));
    assert_eq!(take_log(), ["insert a#0", "remembered a", "deactivate a#0", "forgotten a"]);
    composition.set_content(|| Slots(&[(1, "b")], &[]));
    // Forgotten when deactivated, so only dropped now.
    assert_eq!(take_log(), ["reuse a#0", "drop a", "remembered b"]);
    assert_eq!(tree(&composition), "b#1\n");
}
