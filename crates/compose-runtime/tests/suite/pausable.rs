//! Pausable composition (D590-D596), as Compose 1.12.1's `PausableComposition`: content in a
//! `PausableContent` inserted or reused in a paused composition pauses where the callback says
//! (each restartable composable's start, inserting or reusing), resumes from there, and nothing
//! of it is applied (nodes attached, observers remembered, effects run) before `apply`. A
//! randomized test checks that a composition paused at random points, with state written
//! between its resumes, ends as the same composition composed at once.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::*;

type Tree = NodeTree<String>;

thread_local! {
    static EPOCH: Cell<u32> = const { Cell::new(0) };
    static SHOW_BORN: Cell<bool> = const { Cell::new(false) };
    static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn event(entry: String) {
    EVENTS.with(|events| events.borrow_mut().push(entry));
}

fn take_events() -> Vec<String> {
    EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

/// The events but the bodies run (which a paused composition runs in another order).
fn lifecycle(events: &[String]) -> Vec<String> {
    events.iter().filter(|entry| !entry.starts_with("run ")).cloned().collect()
}

/// A remember observer that logs its callbacks.
struct Logged(String);

impl RememberObserver for Logged {
    fn on_remembered(&self) {
        event(format!("remember {}", self.0));
    }
    fn on_forgotten(&self) {
        event(format!("forget {}", self.0));
    }
    fn on_abandoned(&self) {
        event(format!("abandon {}", self.0));
    }
}

/// An item: a restartable composable that remembers an observer, may emit a node (its label
/// shows its value), run a side effect, hold its children in pausable content of their own
/// (nested lists), and composes its children by key.
struct Spec {
    id: u32,
    node: bool,
    reusable_node: bool,
    effect: bool,
    nested: bool,
    value: OwnedState<u32>,
    branch: OwnedState<bool>,
    children: OwnedState<Vec<Rc<Spec>>>,
}

impl Spec {
    fn new(id: u32) -> Spec {
        Spec {
            id,
            node: true,
            reusable_node: true,
            effect: false,
            nested: false,
            value: mutable_state_of(0),
            branch: mutable_state_of(false),
            children: mutable_state_of_with(Vec::new(), never_equal_policy()),
        }
    }
}

const ITEM_KEY: u64 = key_of("pausable::Item");
const LEAF_KEY: u64 = key_of("pausable::Leaf");

fn Node(reusable: bool, label: String, content: impl FnOnce()) {
    let composer = current();
    let node = if reusable {
        composer.start_reusable_node(key_of("pausable::Node"), || label.clone())
    } else {
        composer.start_node(key_of("pausable::Node"), || label.clone())
    };
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("a live node");
        if *value != label {
            *value = label;
        }
    });
    content();
    composer.end_group();
}

/// A restartable leaf of no arguments: a pause point that `skipping` asks at.
fn Leaf() {
    let composer = current();
    composer.start_restart_group(LEAF_KEY);
    if composer.skipping() {
        composer.skip_to_group_end();
    } else {
        event("run leaf".to_string());
        let token = remember_observer(|| Logged("leaf".to_string()));
        drop(token);
        Node(false, "leaf".to_string(), || {});
    }
    composer.end_restart_group(|_| Leaf());
}

/// `#[composable] fn Item(spec: Rc<Spec>)`, expanded by hand.
fn Item(spec: Rc<Spec>) {
    let composer = current();
    composer.start_restart_group(ITEM_KEY);
    let changed =
        composer.update_kept(|kept: &(Rc<Spec>,)| !Rc::ptr_eq(&kept.0, &spec), || (spec.clone(),));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        event(format!("run {}", spec.id));
        let observer = remember_observer(|| Logged(format!("o{}", spec.id)));
        drop(observer);
        let value = spec.value.get();
        // The epoch it was remembered in, shown when `SHOW_BORN` (review tests).
        let born = remember(|| EPOCH.get());
        let shown_born = SHOW_BORN.get();
        if spec.effect {
            let id = spec.id;
            SideEffect(move || event(format!("effect {id}")));
        }
        let children = || {
            if spec.branch.get() {
                composer.start_replace_group(key_of("pausable::branch"));
                Leaf();
                composer.end_group();
            }
            spec.children.with(|children| {
                for child in children {
                    key(child.id, || Item(child.clone()));
                }
            });
        };
        let content = || {
            if spec.nested {
                PausableContent(children);
            } else {
                children();
            }
        };
        if spec.node {
            let label = if shown_born {
                format!("{}:{value}@{born}", spec.id)
            } else {
                format!("{}:{value}", spec.id)
            };
            Node(spec.reusable_node, label, content);
        } else {
            content();
        }
    }
    composer.end_restart_group(|kept| {
        let (spec,) = kept.downcast_ref::<(Rc<Spec>,)>().expect("kept arguments").clone();
        Item(spec)
    });
}

/// What a test composes: a root node, and in it, while `shown`, the items in pausable content
/// (in reusable content of `reuse_key`, which reuses them when it changes).
struct Scene {
    composition: Composition,
    shown: OwnedState<bool>,
    reuse_key: OwnedState<u32>,
    /// Read by the content: a write runs it again (as a parent composing its child).
    tick: OwnedState<u32>,
    items: Vec<Rc<Spec>>,
}

impl Scene {
    fn new(items: Vec<Rc<Spec>>) -> Scene {
        Scene::new_with(items, None)
    }

    /// `new`, the items in a `ReusableContentHost` active while `active` (review 2).
    fn new_with(items: Vec<Rc<Spec>>, active: Option<MutableState<bool>>) -> Scene {
        let composition = Composition::new(Tree::new());
        let shown = mutable_state_of(false);
        let reuse_key = mutable_state_of(0);
        let tick = mutable_state_of(0);
        let (shown_handle, reuse_handle, tick_handle) =
            (shown.handle(), reuse_key.handle(), tick.handle());
        let roots = items.clone();
        composition.set_content(move || {
            tick_handle.get();
            let roots = roots.clone();
            Node(true, "root".to_string(), || {
                if shown_handle.get() {
                    let body = || {
                        ReusableContent(reuse_handle.get(), || {
                            PausableContent(|| {
                                for item in &roots {
                                    key(item.id, || Item(item.clone()));
                                }
                            });
                        });
                    };
                    match active {
                        Some(active) => ReusableContentHost(active.get(), body),
                        None => body(),
                    }
                }
            });
        });
        Scene { composition, shown, reuse_key, tick, items }
    }

    fn tree(&self) -> String {
        self.composition.with_applier(|tree: &mut Tree| tree.dump()).replace('"', "")
    }

    /// The tree's shape, without labels (a reused node shows its new value at once).
    fn shape(&self) -> String {
        let tree = self.tree();
        let lines: Vec<String> = tree
            .lines()
            .map(|line| format!("{}*", &line[..line.len() - line.trim_start().len()]))
            .collect();
        lines.join("/")
    }

    fn node_count(&self) -> usize {
        self.composition.with_applier(|tree: &mut Tree| tree.len())
    }

    /// The slot table without its node handles (two trees give different ones).
    fn groups(&self) -> String {
        let dump = self.composition.dump();
        dump.lines()
            .map(|line| match line.find(" node=") {
                Some(at) => {
                    let rest = &line[at + 6..];
                    let end = rest.find(' ').map_or(line.len(), |end| at + 6 + end);
                    format!("{}{}", &line[..at], &line[end..])
                }
                None => line.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn chain(ids: &[u32]) -> Vec<Rc<Spec>> {
    ids.iter().map(|&id| Rc::new(Spec::new(id))).collect()
}

fn with_children(id: u32, children: Vec<Rc<Spec>>) -> Rc<Spec> {
    let spec = Spec::new(id);
    spec.children.set(children);
    Rc::new(spec)
}

/// A callback answering from a list (then "go on"), counting its calls.
fn scripted(answers: &[bool]) -> (impl Fn() -> bool + 'static, Rc<Cell<usize>>) {
    let answers = answers.to_vec();
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let callback = move || {
        let call = counter.get();
        counter.set(call + 1);
        answers.get(call).copied().unwrap_or(false)
    };
    (callback, calls)
}

#[test]
fn a_paused_composition_pauses_at_each_restartable_composable_it_is_told_to() {
    // 1 { 2 { 3 }, 4 }, each a node.
    let items = vec![with_children(1, vec![with_children(2, chain(&[3])), Rc::new(Spec::new(4))])];
    let scene = Scene::new(items);
    let before = scene.tree();
    assert_eq!(before, "root\n");
    take_events();
    scene.shown.set(true);
    let paused = scene.composition.recompose_pausable();
    assert!(!paused.is_complete());
    // Paused at item 1, the first pause point in the pausable content.
    let (callback, calls) = scripted(&[true]);
    assert!(!paused.resume(callback));
    assert_eq!(calls.get(), 1);
    assert_eq!(take_events(), Vec::<String>::new(), "item 1 paused before its body");
    scene.composition.validate();
    // Resumed, 1 runs from its start (no pause point there: it's resuming), then 2 and 4 are
    // asked: 2 pauses, 4 runs.
    let (callback, calls) = scripted(&[true, false]);
    assert!(!paused.resume(callback));
    assert_eq!(calls.get(), 2);
    assert_eq!(take_events(), ["run 1", "run 4"]);
    // 2 resumes; 3 is asked and runs.
    let (callback, calls) = scripted(&[false]);
    assert!(paused.resume(callback));
    assert_eq!(calls.get(), 1);
    assert_eq!(take_events(), ["run 2", "run 3"]);
    assert!(paused.is_complete() && !paused.is_applied());
    assert_eq!(scene.tree(), before, "nothing attached before apply");
    scene.composition.validate();
    paused.apply();
    assert!(paused.is_applied());
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n      3:0\n    4:0\n");
    // Observers in tree order, as without the pauses.
    assert_eq!(take_events(), ["remember o1", "remember o2", "remember o3", "remember o4"]);
    scene.composition.validate();
}

#[test]
fn a_composable_of_no_arguments_pauses_at_skipping() {
    let items = vec![Rc::new(Spec::new(1))];
    items[0].branch.set(true);
    let scene = Scene::new(items);
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    // Item 1 runs, the leaf (`skipping`'s pause point) pauses.
    let (callback, calls) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    assert_eq!(calls.get(), 2);
    assert_eq!(take_events(), ["run 1"]);
    assert!(paused.resume(|| true), "a resumed scope doesn't pause at its own start");
    assert_eq!(take_events(), ["run leaf"]);
    paused.apply();
    assert_eq!(scene.tree(), "root\n  1:0\n    leaf\n");
    assert_eq!(take_events(), ["remember o1", "remember leaf"]);
}

#[test]
fn a_composition_that_never_pauses_completes_in_one_resume() {
    let items = vec![with_children(1, chain(&[2, 3]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    let paused = scene.composition.recompose_pausable();
    assert!(paused.resume(|| false));
    assert_eq!(scene.tree(), "root\n", "composed, not applied");
    paused.apply();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n    3:0\n");
    // A paused composition with nothing to compose is complete at once.
    let idle = scene.composition.recompose_pausable();
    assert!(idle.resume(|| true));
    idle.apply();
    assert!(idle.is_applied());
}

#[test]
fn observers_and_effects_run_at_apply() {
    let first = Spec { effect: true, ..Spec::new(1) };
    let second = Spec { effect: true, ..Spec::new(2) };
    let scene = Scene::new(vec![Rc::new(first), Rc::new(second)]);
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    assert!(paused.resume(|| false));
    assert_eq!(take_events(), ["run 1", "run 2"], "no observer remembered, no effect run");
    paused.apply();
    assert_eq!(take_events(), ["remember o1", "remember o2", "effect 1", "effect 2"]);
    // From then on, they're the composition's: a write recomposes item 2 and runs its effect.
    scene.items[1].value.set(5);
    scene.composition.recompose();
    assert_eq!(take_events(), ["run 2", "effect 2"]);
    assert_eq!(scene.tree(), "root\n  1:0\n  2:5\n");
}

#[test]
fn cancelling_discards_the_paused_content() {
    let items = vec![with_children(1, chain(&[2, 3])), Rc::new(Spec::new(4))];
    let scene = Scene::new(items);
    let live_nodes = scene.node_count();
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, false, true, true]);
    assert!(!paused.resume(callback));
    assert_eq!(take_events(), ["run 1", "run 2"]);
    paused.cancel();
    assert!(paused.is_cancelled());
    // Its observers abandoned, its nodes released, the tree as it was.
    let mut events = take_events();
    events.sort();
    assert_eq!(events, ["abandon o1", "abandon o2"]);
    assert_eq!(scene.node_count(), live_nodes);
    assert_eq!(scene.tree(), "root\n");
    scene.composition.validate();
    // The composable around it is invalidated: it composes it again.
    assert!(scene.composition.has_invalidations());
    scene.composition.recompose();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n    3:0\n  4:0\n");
    scene.composition.validate();
    // Cancelling again, or after applying, does nothing.
    paused.cancel();
    let applied = scene.composition.recompose_pausable();
    assert!(applied.resume(|| false));
    applied.apply();
    applied.cancel();
    assert!(applied.is_applied());
}

#[test]
fn state_written_while_paused_is_composed_by_the_next_resume() {
    let items = vec![with_children(1, chain(&[2])), Rc::new(Spec::new(3))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    // 1 runs, 2 and 3 pause.
    let (callback, _) = scripted(&[false, true, true]);
    assert!(!paused.resume(callback));
    assert_eq!(take_events(), ["run 1"]);
    // Item 1 read its value: the write invalidates it inside the paused content. No pass of the
    // composition recomposes it: the next resume does.
    scene.items[0].value.set(7);
    assert!(!scene.composition.has_invalidations());
    assert!(!scene.composition.recompose());
    assert_eq!(take_events(), Vec::<String>::new());
    assert!(paused.resume(|| false));
    let mut events = take_events();
    events.sort();
    assert_eq!(events, ["run 1", "run 2", "run 3"]);
    paused.apply();
    assert_eq!(scene.tree(), "root\n  1:7\n    2:0\n  3:0\n");
    scene.composition.validate();
}

#[test]
fn state_written_once_complete_makes_it_incomplete() {
    let scene = Scene::new(vec![Rc::new(Spec::new(1))]);
    scene.shown.set(true);
    let paused = scene.composition.recompose_pausable();
    assert!(paused.resume(|| false));
    scene.items[0].value.set(2);
    compose_runtime::snapshots::Snapshot::send_apply_notifications();
    assert!(!paused.is_complete(), "invalidated before it was applied");
    take_events();
    assert!(paused.resume(|| false));
    assert_eq!(take_events(), ["run 1"]);
    paused.apply();
    assert_eq!(scene.tree(), "root\n  1:2\n");
}

/// A pass reaching paused content (a body around it ran) composes in it what its body changes
/// (here, reused content: everything), recorded for the paused composition's `apply` (D592).
#[test]
fn a_pass_reaching_paused_content_composes_into_it() {
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[true]);
    assert!(!paused.resume(callback));
    // The reusable content around it changes key: its body runs around the paused content.
    scene.reuse_key.set(1);
    scene.composition.recompose();
    assert!(!paused.is_applied(), "still the paused composition's");
    assert!(paused.resume(|| true), "nothing left to compose");
    paused.apply();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n");
    scene.composition.validate();
    let events = take_events();
    assert!(events.contains(&"remember o1".to_string()), "{events:?}");
}

#[test]
fn a_pass_beside_paused_content_leaves_it_paused() {
    // A sibling composable outside the pausable content, in the same node, recomposes and
    // inserts a node before it.
    let composition = Composition::new(Tree::new());
    let before = mutable_state_of(false);
    let shown = mutable_state_of(false);
    let (before_handle, shown_handle) = (before.handle(), shown.handle());
    let item = Rc::new(Spec::new(1));
    let item_for_content = item.clone();
    composition.set_content(move || {
        let item = item_for_content.clone();
        Node(true, "root".to_string(), || {
            Sibling(before_handle);
            if shown_handle.get() {
                PausableContent(|| Item(item.clone()));
            }
            Node(false, "after".to_string(), || {});
        });
    });
    shown.set(true);
    let paused = composition.recompose_pausable();
    assert!(!paused.resume(|| true));
    before.set(true);
    composition.recompose();
    assert!(!paused.is_complete(), "the sibling's pass ran around nothing paused");
    let dump = || composition.with_applier(|tree: &mut Tree| tree.dump()).replace('"', "");
    assert_eq!(dump(), "root\n  before\n  after\n");
    composition.validate();
    assert!(paused.resume(|| false));
    paused.apply();
    // Attached where it belongs, after the node the sibling inserted meanwhile.
    assert_eq!(dump(), "root\n  before\n  1:0\n  after\n");
    composition.validate();
    before.set(false);
    composition.recompose();
    assert_eq!(dump(), "root\n  1:0\n  after\n");
    composition.validate();
    drop(item);
}

thread_local! {
    static SIBLING_STATE: Cell<Option<MutableState<bool>>> = const { Cell::new(None) };
}

/// A restartable composable reading `before`: a node "before" while it's true.
fn Sibling(before: MutableState<bool>) {
    let composer = current();
    composer.start_restart_group(key_of("pausable::Sibling"));
    let changed =
        composer.update_kept(|kept: &(MutableState<bool>,)| kept.0 != before, || (before,));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else if before.get() {
        Node(false, "before".to_string(), || {});
    }
    composer.end_restart_group(|kept| {
        let (before,) = *kept.downcast_ref::<(MutableState<bool>,)>().expect("kept");
        Sibling(before)
    });
}

#[test]
fn disposing_a_paused_composition_abandons_its_observers_and_releases_its_nodes() {
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    drop(scene);
    assert_eq!(lifecycle(&take_events()), ["abandon o1"]);
    assert!(paused.is_cancelled());
    assert_eq!(live_scope_count(), 0);
}

#[test]
fn reused_content_pauses_and_resumes_its_reuse() {
    let items = vec![with_children(1, chain(&[2, 3]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    scene.composition.recompose();
    let nodes = scene.node_count();
    take_events();
    // Another reuse key: the content is reused, paused at item 2.
    scene.reuse_key.set(1);
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true, false]);
    assert!(!paused.resume(callback));
    assert_eq!(take_events(), ["run 1", "run 3"]);
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n    3:0\n", "reused nodes kept in place");
    scene.composition.validate();
    assert!(paused.resume(|| true));
    assert_eq!(take_events(), ["run 2"]);
    paused.apply();
    // The reused observers forgotten, the new ones remembered, at apply.
    assert_eq!(
        take_events(),
        ["forget o3", "forget o2", "forget o1", "remember o1", "remember o2", "remember o3",]
    );
    assert_eq!(scene.node_count(), nodes, "the reusable nodes were kept");
    scene.composition.validate();
}

#[test]
fn nested_pausable_content_is_part_of_the_paused_content_around_it() {
    let inner = Spec { nested: true, ..Spec::new(1) };
    inner.children.set(chain(&[2, 3]));
    let scene = Scene::new(vec![Rc::new(inner)]);
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true, true]);
    assert!(!paused.resume(callback));
    assert!(paused.resume(|| false));
    assert_eq!(scene.tree(), "root\n");
    paused.apply();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n    3:0\n");
    scene.composition.validate();
}

// ---- Randomized: paused at random points, the same as composed at once ---------------------

/// A small deterministic generator (xorshift).
struct Random(Cell<u64>);

impl Random {
    fn new(seed: u64) -> Random {
        Random(Cell::new(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1))
    }
    fn next(&self) -> u64 {
        let mut x = self.0.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0.set(x);
        x
    }
    fn below(&self, bound: u64) -> u64 {
        self.next() % bound
    }
    fn chance(&self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// A random forest of items, the same for the same seed.
fn forest(seed: u64) -> Vec<Rc<Spec>> {
    let random = Random::new(seed);
    let mut next_id = 1;
    fn build(random: &Random, next_id: &mut u32, depth: u32) -> Rc<Spec> {
        let id = *next_id;
        *next_id += 1;
        let spec = Spec {
            node: random.chance(70),
            reusable_node: random.chance(60),
            effect: random.chance(30),
            nested: random.chance(15),
            ..Spec::new(id)
        };
        spec.branch.set(random.chance(30));
        spec.value.set(random.below(3) as u32);
        let count = if depth >= 3 { 0 } else { random.below(5) };
        let children = (0..count).map(|_| build(random, next_id, depth + 1)).collect();
        spec.children.set(children);
        Rc::new(spec)
    }
    let roots = 1 + random.below(4);
    (0..roots).map(|_| build(&random, &mut next_id, 0)).collect()
}

fn all_items(items: &[Rc<Spec>]) -> Vec<Rc<Spec>> {
    let mut all = Vec::new();
    let mut stack: Vec<Rc<Spec>> = items.iter().rev().cloned().collect();
    while let Some(item) = stack.pop() {
        stack.extend(item.children.get().iter().rev().cloned());
        all.push(item);
    }
    all
}

/// Composes the same forest in two compositions, one at once and one paused at random (with
/// values, branches and children changed between its resumes when `writes`, cancelled at a random point when `cancel`
/// and composed again), reused first when `reuse`; then checks both the same, and that they stay
/// so through changes.
fn compare(seed: u64, reuse: bool, writes: bool, cancel: bool) -> (usize, usize) {
    let reference = Scene::new(forest(seed));
    let paused_scene = Scene::new(forest(seed));
    let random = Random::new(seed ^ 0x5a5a);
    if reuse {
        for scene in [&reference, &paused_scene] {
            scene.shown.set(true);
            scene.composition.recompose();
            scene.reuse_key.set(1);
        }
    } else {
        for scene in [&reference, &paused_scene] {
            scene.shown.set(true);
        }
    }
    take_events();
    reference.composition.recompose();
    let reference_events = take_events();
    let before = paused_scene.shape();
    let paused = paused_scene.composition.recompose_pausable();
    let pause_percent = 20 + random.below(60);
    let pauses = Rc::new(Random::new(seed ^ 0x77));
    let mut resumes = 0;
    let mut written = Vec::new();
    let mut cancelled = false;
    loop {
        let pauses = pauses.clone();
        if paused.resume(move || pauses.chance(pause_percent)) {
            break;
        }
        resumes += 1;
        if cancel && random.chance(40) {
            paused.cancel();
            cancelled = true;
            paused_scene.composition.validate();
            break;
        }
        assert!(resumes < 10_000, "seed {seed}: no progress");
        assert_eq!(paused_scene.shape(), before, "seed {seed}: applied before apply");
        paused_scene.composition.validate();
        if writes && random.chance(50) {
            // A value, a branch or the children changed, in both (the reference composes them
            // at the end).
            let (pick, kind, value) =
                (random.below(1000) as usize, random.below(3), random.below(5) as u32);
            for scene in [&paused_scene, &reference] {
                let items = all_items(&scene.items);
                let item = &items[pick % items.len()];
                match kind {
                    0 => item.value.set(value),
                    1 => item.branch.set(!item.branch.get()),
                    _ => {
                        let mut children = item.children.get();
                        children.reverse();
                        if children.len() > 2 {
                            children.remove(1);
                        }
                        item.children.set(children);
                    }
                }
            }
            written.push(pick);
            // A pass meanwhile leaves the paused content alone.
            paused_scene.composition.recompose();
        }
    }
    paused_scene.composition.validate();
    if cancelled {
        // Composed again by the composable around it, then the same as the reference.
        paused_scene.tick.set(1);
        paused_scene.composition.recompose();
        reference.tick.set(1);
    } else {
        paused.apply();
    }
    paused_scene.composition.validate();
    let paused_events = take_events();
    if written.is_empty() && !cancelled {
        assert_eq!(
            lifecycle(&paused_events),
            lifecycle(&reference_events),
            "seed {seed}: observers and effects in another order"
        );
    }
    // What was written while paused, written to the reference too.
    let written_count = written.len();
    reference.composition.recompose();
    take_events();
    assert_eq!(paused_scene.tree(), reference.tree(), "seed {seed}");
    assert_eq!(paused_scene.groups(), reference.groups(), "seed {seed}");
    // Changes from then on, in both: the paused composition's content is the composition's.
    for step in 0..6 {
        let pick = random.below(1000);
        for scene in [&reference, &paused_scene] {
            let items = all_items(&scene.items);
            let item = &items[(pick as usize) % items.len()];
            match step % 3 {
                0 => item.value.set(item.value.get() + 1),
                1 => item.branch.set(!item.branch.get()),
                _ => {
                    let mut children = item.children.get();
                    children.reverse();
                    if children.len() > 1 {
                        children.pop();
                    }
                    item.children.set(children);
                }
            }
            scene.composition.recompose();
            scene.composition.validate();
        }
        take_events();
        assert_eq!(paused_scene.tree(), reference.tree(), "seed {seed}, step {step}");
    }
    (resumes, written_count)
}

#[test]
fn a_composition_paused_at_random_points_matches_one_composed_at_once() {
    let resumes: usize = (0..300).map(|seed| compare(seed, false, false, false).0).sum();
    assert!(resumes > 800, "{resumes} resumes");
}

#[test]
fn reused_content_paused_at_random_points_matches_reuse_at_once() {
    let resumes: usize = (0..300).map(|seed| compare(seed, true, false, false).0).sum();
    assert!(resumes > 800, "{resumes} resumes");
}

#[test]
fn state_written_between_random_pauses_ends_as_written() {
    let writes: usize = (0..300).map(|seed| compare(seed, seed % 2 == 0, true, false).1).sum();
    assert!(writes > 300, "{writes} writes");
}

#[test]
fn a_composition_cancelled_at_a_random_point_and_composed_again_matches() {
    for seed in 0..300 {
        compare(seed, seed % 2 == 0, seed % 3 == 0, true);
    }
}

#[test]
fn cancelling_reused_content_removes_it_and_forgets_its_old_values() {
    let items = vec![with_children(1, chain(&[2, 3]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    scene.composition.recompose();
    let nodes = scene.node_count();
    take_events();
    scene.reuse_key.set(1);
    let paused = scene.composition.recompose_pausable();
    // Item 1 reused, 2 and 3 paused.
    let (callback, _) = scripted(&[false, true, true]);
    assert!(!paused.resume(callback));
    paused.cancel();
    let mut events = lifecycle(&take_events());
    events.sort();
    // Item 1's new observer was never remembered; the old ones are forgotten.
    assert_eq!(events, ["abandon o1", "forget o1", "forget o2", "forget o3"]);
    assert_eq!(scene.tree(), "root\n");
    assert_eq!(scene.node_count(), nodes - 3);
    scene.composition.validate();
    scene.tick.set(1);
    scene.composition.recompose();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n    3:0\n");
    assert_eq!(scene.node_count(), nodes);
    scene.composition.validate();
}

#[test]
fn disposing_paused_reused_content_releases_everything() {
    let items = vec![with_children(1, chain(&[2, 3]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    scene.composition.recompose();
    scene.reuse_key.set(1);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, false, true]);
    assert!(!paused.resume(callback));
    drop(scene);
    let mut events = lifecycle(&take_events());
    events.sort();
    assert_eq!(events, ["abandon o1", "abandon o2", "forget o1", "forget o2", "forget o3"]);
    assert_eq!(live_scope_count(), 0);
}

#[test]
#[should_panic(expected = "paused composition was used out of order")]
fn one_paused_composition_at_a_time() {
    let scene = Scene::new(vec![Rc::new(Spec::new(1))]);
    let _first = scene.composition.recompose_pausable();
    let _second = scene.composition.recompose_pausable();
}

#[test]
#[should_panic(expected = "paused composition was used out of order")]
fn applying_before_complete_is_a_misuse() {
    let scene = Scene::new(vec![Rc::new(Spec::new(1))]);
    scene.shown.set(true);
    let paused = scene.composition.recompose_pausable();
    assert!(!paused.resume(|| true));
    paused.apply();
}

#[test]
fn cancelling_during_a_layout_waits_for_the_next_pass() {
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    // While the applier is in use (as by a layout), the content can't be discarded yet.
    scene.composition.with_applier(|_: &mut Tree| paused.cancel());
    assert!(paused.is_cancelled());
    assert_eq!(lifecycle(&take_events()), Vec::<String>::new());
    // The next pass discards it first.
    scene.tick.set(1);
    scene.composition.recompose();
    assert_eq!(lifecycle(&take_events()), ["abandon o1", "remember o1", "remember o2"]);
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n");
    scene.composition.validate();
}

#[test]
fn content_outside_pausable_content_composes_and_applies_with_the_first_pass() {
    // Beyond its pausable content the pass is an ordinary one, applied at its end, as the rest
    // of the item around Compose's subcomposition (its slot's node) is.
    let composition = Composition::new(Tree::new());
    let shown = mutable_state_of(false);
    let handle = shown.handle();
    let inner = Rc::new(Spec::new(2));
    let inner_for_content = inner.clone();
    composition.set_content(move || {
        let inner = inner_for_content.clone();
        if handle.get() {
            Node(true, "outside".to_string(), || PausableContent(|| Item(inner.clone())));
        }
    });
    shown.set(true);
    take_events();
    let paused = composition.recompose_pausable();
    assert!(!paused.resume(|| true));
    let dump = || composition.with_applier(|tree: &mut Tree| tree.dump()).replace('"', "");
    assert_eq!(dump(), "outside\n");
    assert!(paused.resume(|| false));
    paused.apply();
    assert_eq!(dump(), "outside\n  2:0\n");
    assert_eq!(lifecycle(&take_events()), ["remember o2"]);
    composition.validate();
}

// ---- Review (adversarial) -------------------------------------------------------------------

#[test]
fn review_a_paused_reusing_scope_reached_by_its_parent_still_reuses() {
    // 1 { 2 }, composed, then reused (another reuse key) in a paused composition: 1 runs,
    // 2 pauses while reusing (its group keeps the old item's content).
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    scene.composition.recompose();
    take_events();
    scene.reuse_key.set(1);
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    assert_eq!(take_events(), ["run 1"]);
    // A write invalidating item 1: the next resume runs 1's body again, which reaches 2 (paused,
    // reusing) through its parent rather than through the walk. Compose's `addRecomposeScope`
    // resumes a paused scope met this way too, in reuse if it paused reusing.
    scene.items[0].value.set(9);
    assert!(paused.resume(|| false));
    paused.apply();
    let mut events = lifecycle(&take_events());
    events.sort();
    // Reused content's values are forgotten and new ones remembered, as composed at once.
    assert_eq!(events, ["forget o1", "forget o2", "remember o1", "remember o2"]);
    scene.composition.validate();
}

#[test]
fn review_a_scope_resumed_through_its_parent_is_no_longer_paused() {
    // Plain insertion: 2 pauses, 1 is invalidated, its body composes 2. 2's scope stays marked
    // paused (with its segment), and a later paused composition reusing the content takes it
    // for a paused scope of its own (item 2 runs a side effect, which goes to its segment).
    let items = vec![with_children(1, vec![Rc::new(Spec { effect: true, ..Spec::new(2) })])];
    let scene = Scene::new(items);
    scene.shown.set(true);
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    scene.items[0].value.set(9);
    assert!(paused.resume(|| false));
    paused.apply();
    scene.composition.validate();
    // Reused in a new paused composition, nothing pausing; then a write invalidates item 2.
    scene.reuse_key.set(1);
    let second = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, false]);
    assert!(second.resume(callback));
    scene.items[0].children.get()[0].value.set(4);
    take_events();
    assert!(second.resume(|| false));
    assert_eq!(take_events(), ["run 2"]);
    second.apply();
    take_events();
    scene.composition.validate();
    assert_eq!(scene.tree(), "root\n  1:9\n    2:4\n");
}

thread_local! {
    static OWN_COMPOSITION: RefCell<Option<std::rc::Weak<Composition>>> =
        const { RefCell::new(None) };
}

/// A restartable composable reading `trigger`: once it's set, its side effect writes `tick`
/// (read by the content around the paused content) and recomposes its own composition at once,
/// while that composition is still applying its pass's end.
fn Trigger(trigger: MutableState<u32>, tick: MutableState<u32>) {
    let composer = current();
    composer.start_restart_group(key_of("pausable::Trigger"));
    let changed = composer.update_kept(
        |kept: &(MutableState<u32>, MutableState<u32>)| kept.0 != trigger || kept.1 != tick,
        || (trigger, tick),
    );
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else if trigger.get() > 0 {
        SideEffect(move || {
            tick.set(tick.get() + 1);
            let own =
                OWN_COMPOSITION.with(|own| own.borrow().as_ref().and_then(|own| own.upgrade()));
            if let Some(own) = own {
                own.recompose();
            }
        });
    }
    composer.end_restart_group(|kept| {
        let (trigger, tick) =
            *kept.downcast_ref::<(MutableState<u32>, MutableState<u32>)>().expect("kept");
        Trigger(trigger, tick)
    });
}

#[test]
fn review_recomposing_while_finishing_around_paused_content_returns() {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let composition = Rc::new(Composition::new(Tree::new()));
        OWN_COMPOSITION.with(|own| *own.borrow_mut() = Some(Rc::downgrade(&composition)));
        let shown = mutable_state_of(false);
        let trigger = mutable_state_of(0u32);
        let tick = mutable_state_of(0u32);
        let (shown_handle, trigger_handle, tick_handle) =
            (shown.handle(), trigger.handle(), tick.handle());
        let item = Rc::new(Spec::new(1));
        let item_for_content = item.clone();
        composition.set_content(move || {
            tick_handle.get();
            let item = item_for_content.clone();
            Node(true, "root".to_string(), || {
                Trigger(trigger_handle, tick_handle);
                if shown_handle.get() {
                    PausableContent(|| Item(item.clone()));
                }
            });
        });
        shown.set(true);
        let paused = composition.recompose_pausable();
        assert!(!paused.resume(|| true));
        // A pass beside the paused content, whose effect invalidates the content around it and
        // recomposes at once (as a frame requester may): that recomposition must return.
        trigger.set(1);
        composition.recompose();
        let _ = sender.send(());
        drop(paused);
        drop(item);
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("recompose() looped forever in complete_if_enclosed");
}

#[test]
fn review_cancelling_from_the_callback_stays_cancelled() {
    let items = vec![with_children(1, chain(&[2, 3]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    take_events();
    let paused = Rc::new(scene.composition.recompose_pausable());
    let weak = Rc::downgrade(&paused);
    // Cancelled at the first pause point, from the callback (during the pass): told to go on.
    let cancelled = Cell::new(false);
    let complete = paused.resume(move || {
        if !cancelled.replace(true) {
            weak.upgrade().expect("alive").cancel();
        }
        false
    });
    assert!(paused.is_cancelled(), "complete: {complete}, applied: {}", paused.is_applied());
    assert!(!complete);
}

/// Composes the same forest twice, one paused at random with changes *around* the paused
/// content between its resumes (the content around it composed again, its reusable content's
/// key changed, it hidden and shown) and inside it, and checks both the same at the end. Item
/// labels show the epoch their `remember` ran in (the reuse key's changes), so content kept
/// rather than reused shows.
fn compare_outer(seed: u64) {
    SHOW_BORN.set(true);
    EPOCH.set(0);
    let reference = Scene::new(forest(seed));
    let scene = Scene::new(forest(seed));
    let random = Random::new(seed ^ 0x1234);
    let reuse = seed.is_multiple_of(2);
    for each in [&reference, &scene] {
        each.shown.set(true);
        if reuse {
            each.composition.recompose();
        }
    }
    if reuse {
        EPOCH.set(1);
        for each in [&reference, &scene] {
            each.reuse_key.set(1);
        }
    }
    reference.composition.recompose();
    let paused = scene.composition.recompose_pausable();
    let pauses = Rc::new(Random::new(seed ^ 0x99));
    let mut steps = 0;
    loop {
        let answers = pauses.clone();
        if paused.resume(move || answers.chance(50)) {
            paused.apply();
            break;
        }
        steps += 1;
        assert!(steps < 10_000, "seed {seed}: no progress");
        scene.composition.validate();
        if seed % 3 == 1 && random.chance(15) {
            paused.cancel();
            scene.composition.validate();
            scene.tick.set(scene.tick.get() + 1);
            reference.tick.set(reference.tick.get() + 1);
            for each in [&scene, &reference] {
                each.composition.recompose();
                each.composition.validate();
            }
            break;
        }
        if random.chance(50) {
            let (kind, pick) = (random.below(5), random.below(1000) as usize);
            if kind == 1 {
                EPOCH.set(EPOCH.get() + 1);
            }
            for each in [&scene, &reference] {
                match kind {
                    0 => each.tick.set(each.tick.get() + 1),
                    1 => each.reuse_key.set(each.reuse_key.get() + 1),
                    2 => each.shown.set(!each.shown.get()),
                    _ => {
                        let items = all_items(&each.items);
                        let item = &items[pick % items.len()];
                        item.value.set(item.value.get() + 1);
                    }
                }
                each.composition.recompose();
                each.composition.validate();
            }
            if paused.is_applied() || paused.is_cancelled() {
                break;
            }
        }
    }
    for each in [&scene, &reference] {
        each.shown.set(true);
        each.composition.recompose();
        each.composition.validate();
    }
    assert_eq!(scene.tree(), reference.tree(), "seed {seed}");
    assert_eq!(scene.groups(), reference.groups(), "seed {seed}");
    take_events();
    SHOW_BORN.set(false);
}

#[test]
fn review_changes_around_paused_content_between_resumes_end_as_composed_at_once() {
    // Fails at seed 128 (a paused scope composed by its parent's body, not in reuse) and, with
    // that fixed, at seed 874 (a walk inside a skipped paused group resets the reuse marker).
    let seeds: u64 = std::env::var("OUTER_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(1000);
    for seed in 0..seeds {
        compare_outer(seed);
    }
}

#[test]
fn review_a_walk_inside_a_paused_group_keeps_the_reuse_around_it() {
    // Reused: 1 { 2 { 3 { 4 (with a leaf), 6 }, 5 } }. Labels show the epoch each item's
    // `remember` ran in: reused at epoch 1, every item should show @1.
    SHOW_BORN.set(true);
    EPOCH.set(0);
    let third = Spec::new(3);
    let fourth = Spec::new(4);
    fourth.branch.set(true);
    third.children.set(vec![Rc::new(fourth), Rc::new(Spec::new(6))]);
    let second = Spec::new(2);
    second.children.set(vec![Rc::new(third), Rc::new(Spec::new(5))]);
    let first = Spec::new(1);
    first.children.set(vec![Rc::new(second)]);
    let scene = Scene::new(vec![Rc::new(first)]);
    scene.shown.set(true);
    scene.composition.recompose();
    EPOCH.set(1);
    scene.reuse_key.set(1);
    let paused = scene.composition.recompose_pausable();
    // 1 runs, 2 pauses (reusing; its content kept as it was, its scopes live).
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    // Item 4, in 2's kept content, is written.
    let second = scene.items[0].children.get()[0].clone();
    let third = second.children.get()[0].clone();
    let fourth = third.children.get()[0].clone();
    fourth.value.set(1);
    // 2 resumes (reusing): 3 pauses; skipped while dirty, the walk inside it restarts 4 (asked:
    // runs), whose leaf pauses (reusing); 5 runs.
    let (callback, _) = scripted(&[true, false, true, false]);
    assert!(!paused.resume(callback));
    // 3 resumes (reusing): 4 pauses; skipped while dirty, the walk inside it restarts the leaf,
    // which sets the reuse marker at its own depth and clears it as it ends: 6, after 4 in 3,
    // composes as if not reused (it skips, keeping what it remembered at epoch 0).
    let (callback, _) = scripted(&[true]);
    assert!(!paused.resume(callback));
    while !paused.resume(|| false) {}
    paused.apply();
    let tree = scene.tree();
    SHOW_BORN.set(false);
    assert!(!tree.contains("@0"), "content kept rather than reused:\n{tree}");
}

// ---- After the review: passes around paused content, requests, reuse at apply ---------------

#[test]
fn a_pass_removing_paused_content_discards_it_without_composing_it() {
    let items = vec![with_children(1, chain(&[2, 3]))];
    let scene = Scene::new(items);
    let nodes = scene.node_count();
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true, true]);
    assert!(!paused.resume(callback));
    assert_eq!(take_events(), ["run 1"]);
    // Hidden: the pass removing it composes nothing of it (D593).
    scene.shown.set(false);
    scene.composition.recompose();
    let events = take_events();
    assert!(!events.iter().any(|event| event.starts_with("run ")), "{events:?}");
    assert_eq!(lifecycle(&events), ["abandon o1"]);
    assert!(paused.is_cancelled());
    assert_eq!(scene.node_count(), nodes, "what it made released");
    scene.composition.validate();
    // Shown again: composed anew.
    scene.shown.set(true);
    scene.composition.recompose();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n    3:0\n");
    scene.composition.validate();
}

#[test]
fn a_pass_removing_reused_paused_content_releases_its_old_nodes_once() {
    let items = vec![with_children(1, chain(&[2, 3]))];
    let scene = Scene::new(items);
    let empty = scene.node_count();
    scene.shown.set(true);
    scene.composition.recompose();
    scene.reuse_key.set(1);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true, false]);
    assert!(!paused.resume(callback));
    scene.shown.set(false);
    scene.composition.recompose();
    assert!(paused.is_cancelled());
    assert_eq!(scene.node_count(), empty);
    assert_eq!(scene.tree(), "root\n");
    let mut events = lifecycle(&take_events());
    events.sort();
    assert_eq!(events, ["abandon o1", "abandon o3", "forget o1", "forget o2", "forget o3"]);
    scene.composition.validate();
}

/// Paused content in a `ReusableContentHost` deactivated meanwhile: discarded, not composed.
#[test]
fn deactivating_paused_content_discards_it() {
    let composition = Composition::new(Tree::new());
    let active = mutable_state_of(false);
    let active_handle = active.handle();
    let item = with_children(1, chain(&[2]));
    let content_item = item.clone();
    composition.set_content(move || {
        let item = content_item.clone();
        Node(true, "root".to_string(), || {
            ReusableContentHost(active_handle.get(), || {
                Node(true, "host".to_string(), || PausableContent(|| Item(item.clone())));
            });
        });
    });
    let dump = || composition.with_applier(|tree: &mut Tree| tree.dump()).replace('"', "");
    let nodes = || composition.with_applier(|tree: &mut Tree| tree.len());
    active.set(true);
    take_events();
    let paused = composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    assert_eq!(take_events(), ["run 1"]);
    assert_eq!(dump(), "root\n  host\n");
    let made = nodes();
    active.set(false);
    composition.recompose();
    let events = take_events();
    assert!(!events.iter().any(|event| event.starts_with("run ")), "{events:?}");
    assert_eq!(lifecycle(&events), ["abandon o1"]);
    assert!(paused.is_cancelled());
    assert!(nodes() < made, "what it made released");
    composition.validate();
    // Active again: composed anew in the reused host.
    active.set(true);
    composition.recompose();
    assert_eq!(dump(), "root\n  host\n    1:0\n      2:0\n");
    composition.validate();
}

#[test]
fn a_completion_asked_for_is_done_before_the_next_pass() {
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    let paused = scene.composition.recompose_pausable();
    assert!(!paused.resume(|| true));
    // A layout needing it asks (it can't compose itself); the next pass completes it first.
    paused.request_completion();
    assert!(!paused.is_complete());
    scene.tick.set(1);
    scene.composition.recompose();
    assert!(paused.is_applied());
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n");
    scene.composition.validate();
}

#[test]
fn dropping_a_paused_composition_cancels_it() {
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    let nodes = scene.node_count();
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    drop(paused);
    assert_eq!(lifecycle(&take_events()), ["abandon o1"]);
    assert_eq!(scene.node_count(), nodes);
    // The next paused composition isn't held up by it.
    let next = scene.composition.recompose_pausable();
    assert!(next.resume(|| false));
    next.apply();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n");
    scene.composition.validate();
}

/// A node tree that logs the nodes reset for reuse.
struct ReuseLog(Tree);

impl Applier for ReuseLog {
    type Node = String;
    fn root(&self) -> NodeRef {
        self.0.root()
    }
    fn insert(&mut self, parent: NodeRef, index: usize, node: String) -> NodeRef {
        Applier::insert(&mut self.0, parent, index, node)
    }
    fn create(&mut self, node: String) -> NodeRef {
        Applier::create(&mut self.0, node)
    }
    fn attach(&mut self, parent: NodeRef, index: usize, node: NodeRef) {
        Applier::attach(&mut self.0, parent, index, node)
    }
    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        Applier::remove(&mut self.0, parent, index, count)
    }
    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        Applier::move_nodes(&mut self.0, parent, from, to, count)
    }
    fn release(&mut self, node: NodeRef) {
        Applier::release(&mut self.0, node)
    }
    fn reuse(&mut self, node: NodeRef) {
        let label = self.0.get(node).cloned().unwrap_or_default();
        event(format!("reuse {label}"));
    }
}

fn LogNode(label: String, content: impl FnOnce()) {
    let composer = current();
    let node = composer.start_reusable_node(key_of("pausable::LogNode"), || label.clone());
    composer.with_applier(|tree: &mut ReuseLog| {
        let value = tree.0.get_mut(node).expect("live");
        *value = label.clone();
    });
    content();
    composer.end_group();
}

/// A node reused in paused content is reset as the paused composition is applied, as Compose's
/// recorded `reuse` (its focus requesters attached only then, D591).
#[test]
fn a_node_reused_in_paused_content_is_reset_at_apply() {
    let composition = Composition::new(ReuseLog(Tree::new()));
    let key = mutable_state_of(0);
    let key_handle = key.handle();
    composition.set_content(move || {
        LogNode("root".to_string(), || {
            ReusableContent(key_handle.get(), || {
                PausableContent(|| LogNode(format!("item {}", key_handle.get()), || {}));
            });
        });
    });
    take_events();
    key.set(1);
    let paused = composition.recompose_pausable();
    assert!(paused.resume(|| false));
    assert_eq!(take_events(), Vec::<String>::new(), "not reset before apply");
    paused.apply();
    assert_eq!(take_events(), ["reuse item 1"]);
    composition.validate();
}

/// A pass through paused content (the body around it ran, the composables in it skipping with
/// their arguments) composes nothing of what's paused: that's its resumes' (D592).
#[test]
fn a_pass_through_paused_content_leaves_what_is_paused() {
    let items = vec![with_children(1, chain(&[2, 3]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    take_events();
    let paused = scene.composition.recompose_pausable();
    let (callback, _) = scripted(&[false, true, true]);
    assert!(!paused.resume(callback));
    assert_eq!(take_events(), ["run 1"]);
    // The content around it composes again: it reaches the paused content's group.
    scene.tick.set(1);
    scene.composition.recompose();
    assert_eq!(take_events(), Vec::<String>::new());
    assert!(!paused.is_complete());
    assert_eq!(scene.tree(), "root\n");
    scene.composition.validate();
    assert!(paused.resume(|| false));
    assert_eq!(take_events(), ["run 2", "run 3"]);
    paused.apply();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n    3:0\n");
    scene.composition.validate();
}

/// What a layout keeps of a paused composition to know whether its content is there unapplied:
/// paused, complete, or cancelled with the removal waiting (the applier in use), it is; not
/// composed yet, complete without paused content, applied, or cancelled and removed, it isn't,
/// the paused composition held or not (D599).
#[test]
fn a_watch_says_whether_paused_content_is_there_unapplied() {
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    let paused = scene.composition.recompose_pausable();
    let watch = paused.watch();
    assert!(!watch.holds_unapplied_content(), "not composed yet: none there");
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    assert!(watch.holds_unapplied_content(), "paused");
    // Dropped during a layout: cancelled, the removal waits for the next pass.
    scene.composition.with_applier(|_: &mut Tree| drop(paused));
    assert!(watch.holds_unapplied_content(), "removal waiting");
    scene.tick.set(1);
    scene.composition.recompose();
    assert!(!watch.holds_unapplied_content(), "removed");
    // Complete without paused content: none there.
    scene.tick.set(2);
    let next = scene.composition.recompose_pausable();
    let watch = next.watch();
    assert!(next.resume(|| false));
    assert!(!watch.holds_unapplied_content(), "complete without paused content");
    next.apply();
    // Complete, then applied.
    scene.shown.set(false);
    scene.composition.recompose();
    scene.shown.set(true);
    let next = scene.composition.recompose_pausable();
    let watch = next.watch();
    assert!(next.resume(|| false));
    assert!(watch.holds_unapplied_content(), "complete, not applied");
    next.apply();
    assert!(!watch.holds_unapplied_content(), "applied");
    scene.composition.validate();
}

// ---- Second review: completion on request, cancels during a layout, drops ------------------

/// Tallies remember observers' events: each label's live count.
#[derive(Default)]
struct Tally {
    live: std::collections::BTreeMap<String, i64>,
}

impl Tally {
    fn add(&mut self, seed: u64, who: &str, events: Vec<String>) {
        for entry in events {
            if let Some(label) = entry.strip_prefix("remember ") {
                *self.live.entry(label.to_string()).or_default() += 1;
            } else if let Some(label) = entry.strip_prefix("forget ") {
                let count = self.live.entry(label.to_string()).or_default();
                *count -= 1;
                assert!(*count >= 0, "seed {seed}: {who} forgot {label} never remembered");
            }
        }
    }
    fn live(&self) -> Vec<(String, i64)> {
        self.live.iter().filter(|(_, count)| **count != 0).map(|(l, c)| (l.clone(), *c)).collect()
    }
}

/// As `compare_outer`, with completions asked for (from a layout, the applier in use, or not),
/// cancels during a layout, and the paused composition dropped, at random points; checks that a
/// completion asked for is done by the next pass, the watch, and the observers left remembered.
fn compare_outer2(seed: u64) {
    SHOW_BORN.set(true);
    EPOCH.set(0);
    take_events();
    let hosted = seed % 3 == 2;
    let (active_ref, active_ours) = (mutable_state_of(true), mutable_state_of(true));
    let reference = Scene::new_with(forest(seed), hosted.then(|| active_ref.handle()));
    let scene = Scene::new_with(forest(seed), hosted.then(|| active_ours.handle()));
    let (mut ours, mut theirs) = (Tally::default(), Tally::default());
    let random = Random::new(seed ^ 0xabcd);
    let reuse = seed.is_multiple_of(2);
    for (each, tally) in [(&reference, &mut theirs), (&scene, &mut ours)] {
        each.shown.set(true);
        if reuse {
            each.composition.recompose();
        }
        tally.add(seed, "setup", take_events());
    }
    if reuse {
        EPOCH.set(1);
        for each in [&reference, &scene] {
            each.reuse_key.set(1);
        }
    }
    reference.composition.recompose();
    theirs.add(seed, "reference", take_events());
    let mut paused = Some(scene.composition.recompose_pausable());
    let watch = paused.as_ref().unwrap().watch();
    let pauses = Rc::new(Random::new(seed ^ 0x4242));
    let mut steps = 0;
    let mut started = false;
    loop {
        steps += 1;
        assert!(steps < 10_000, "seed {seed}: no progress");
        let Some(current) = paused.as_ref() else { break };
        if current.is_applied() || current.is_cancelled() {
            break;
        }
        let mut action = random.below(10);
        // A request resumes its paused composition as it makes it (D595); it may be dropped
        // or cancelled before that, though.
        if !started && action != 5 {
            action = 0;
        }
        if std::env::var("OUTER2_TRACE").is_ok() {
            eprintln!(
                "seed {seed} step {steps}: action {action} state complete={} tree={:?}",
                current.is_complete(),
                scene.tree()
            );
        }
        match action {
            0..=3 => {
                let answers = pauses.clone();
                started = true;
                if current.resume(move || answers.chance(50)) {
                    current.apply();
                }
                ours.add(seed, "resume", take_events());
            }
            4 => {
                // A layout needing it asks, then the round's pass.
                if random.chance(50) {
                    scene.composition.with_applier(|_: &mut Tree| current.request_completion());
                } else {
                    current.request_completion();
                }
                let has_content = watch.holds_unapplied_content();
                scene.tick.set(scene.tick.get() + 1);
                reference.tick.set(reference.tick.get() + 1);
                scene.composition.recompose();
                ours.add(seed, "completion", take_events());
                reference.composition.recompose();
                theirs.add(seed, "reference", take_events());
                scene.composition.validate();
                if has_content {
                    COMPLETIONS.set(COMPLETIONS.get() + 1);
                    assert!(
                        current.is_applied() || current.is_cancelled(),
                        "seed {seed}: a completion asked for wasn't done by the next pass ({})",
                        if current.is_complete() { "complete" } else { "incomplete" }
                    );
                    assert!(
                        !watch.holds_unapplied_content(),
                        "seed {seed}: watch after completion"
                    );
                }
            }
            5 => {
                // Cancelled or dropped, during a layout or not.
                let during_layout = random.chance(50);
                let drop_it = random.chance(50);
                let taken = paused.take().unwrap();
                let act = RefCell::new(Some(move || {
                    if drop_it {
                        drop(taken);
                    } else {
                        taken.cancel();
                    }
                }));
                if during_layout {
                    scene
                        .composition
                        .with_applier(|_: &mut Tree| (act.borrow_mut().take().unwrap())());
                } else {
                    (act.borrow_mut().take().unwrap())();
                }
                ours.add(seed, "cancel", take_events());
                scene.tick.set(scene.tick.get() + 1);
                reference.tick.set(reference.tick.get() + 1);
                scene.composition.recompose();
                ours.add(seed, "after cancel", take_events());
                reference.composition.recompose();
                theirs.add(seed, "reference", take_events());
                scene.composition.validate();
                assert!(
                    !watch.holds_unapplied_content(),
                    "seed {seed}: watch after a cancel and a pass"
                );
                break;
            }
            _ => {
                let (kind, pick) = (random.below(6), random.below(1000) as usize);
                if kind == 1 {
                    EPOCH.set(EPOCH.get() + 1);
                }
                for (each, tally, who, active) in [
                    (&scene, &mut ours, "change", &active_ours),
                    (&reference, &mut theirs, "reference", &active_ref),
                ] {
                    match kind {
                        0 => each.tick.set(each.tick.get() + 1),
                        1 => each.reuse_key.set(each.reuse_key.get() + 1),
                        2 => each.shown.set(!each.shown.get()),
                        5 if hosted => active.set(!active.get()),
                        _ => {
                            let items = all_items(&each.items);
                            let item = &items[pick % items.len()];
                            item.value.set(item.value.get() + 1);
                        }
                    }
                    each.composition.recompose();
                    tally.add(seed, who, take_events());
                    each.composition.validate();
                }
            }
        }
    }
    drop(paused);
    ours.add(seed, "drop", take_events());
    active_ours.set(true);
    active_ref.set(true);
    for (each, tally) in [(&scene, &mut ours), (&reference, &mut theirs)] {
        each.shown.set(true);
        each.tick.set(each.tick.get() + 1);
        each.composition.recompose();
        tally.add(seed, "end", take_events());
        each.composition.validate();
    }
    assert!(!watch.holds_unapplied_content(), "seed {seed}: watch at the end");
    assert_eq!(scene.tree(), reference.tree(), "seed {seed}");
    assert_eq!(scene.groups(), reference.groups(), "seed {seed}");
    assert_eq!(ours.live(), theirs.live(), "seed {seed}: observers remembered");
    // A next paused composition can be made.
    let next = scene.composition.recompose_pausable();
    assert!(next.resume(|| false));
    next.apply();
    take_events();
    SHOW_BORN.set(false);
}

#[test]
fn review2_completions_cancels_and_drops_at_random_points_end_as_composed_at_once() {
    let seeds: u64 =
        std::env::var("OUTER2_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(1000);
    let first: u64 = std::env::var("OUTER2_FIRST").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    for seed in first..first + seeds {
        compare_outer2(seed);
    }
    let completions = COMPLETIONS.get();
    assert!(completions > seeds / 10, "{completions} completions with content");
}

thread_local! {
    static COMPLETIONS: Cell<u64> = const { Cell::new(0) };
}

/// A paused composition dropped (so cancelled, D597) before its first resume: no region to
/// remove, but `cancel` marked the composition finishing and returned without the guard that
/// ends it. The composition never composes again, and the runtime stays busy.
#[test]
fn review2_dropping_a_paused_composition_before_its_first_resume_leaves_the_composition_usable() {
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    let paused = scene.composition.recompose_pausable();
    drop(paused);
    scene.tick.set(1);
    scene.composition.recompose();
    assert_eq!(scene.tree(), "root\n  1:0\n    2:0\n", "composed after the drop");
    scene.composition.validate();
}

/// The same with a complete paused composition that holds no paused content (its first pass
/// inserted no `PausableContent`: the request's item was composed already), cancelled before
/// it's applied (a request cleaned up, its apply step short of time).
#[test]
fn review2_cancelling_a_complete_paused_composition_without_regions_leaves_the_composition_usable()
{
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    scene.composition.recompose();
    // A write outside the pausable content: the paused composition's first pass composes it,
    // and makes no region.
    scene.tick.set(1);
    let paused = scene.composition.recompose_pausable();
    assert!(paused.resume(|| true), "complete: nothing pausable");
    paused.cancel();
    scene.items[0].value.set(5);
    scene.composition.recompose();
    assert_eq!(scene.tree(), "root\n  1:5\n    2:0\n", "composed after the cancel");
    scene.composition.validate();
}

/// A complete paused composition holding no paused content (its first pass inserted no
/// `PausableContent`: a request for an item composed already, a pinned or sticky one): its
/// watch says its content is there unapplied, and a completion asked for does nothing
/// (`run_requests` needs live regions), so a layout wanting its item lacks it, and asks for a
/// round, at every frame until the request's apply step runs, which no idle time comes for while
/// frames keep being asked for.
#[test]
fn review2_a_completion_of_a_complete_paused_composition_without_regions_applies_it() {
    let items = vec![with_children(1, chain(&[2]))];
    let scene = Scene::new(items);
    scene.shown.set(true);
    scene.composition.recompose();
    scene.tick.set(1);
    let paused = scene.composition.recompose_pausable();
    let watch = paused.watch();
    assert!(paused.resume(|| true), "complete: nothing pausable");
    paused.request_completion();
    scene.tick.set(2);
    scene.composition.recompose();
    assert!(!watch.holds_unapplied_content(), "the watch after a completion and a pass");
    assert!(paused.is_applied(), "applied by the completion asked for");
}

/// A paused composition for a target pauses only that target's pausable content: another one
/// its first pass inserts (another list's item, composed for writes of its own) is ordinary
/// content, applied at the pass's end (D597).
#[test]
fn a_paused_composition_for_a_target_pauses_only_its_content() {
    let composition = Composition::new(Tree::new());
    let shown = mutable_state_of(false);
    let shown_handle = shown.handle();
    let (mine, other) = (with_children(1, chain(&[2])), with_children(3, chain(&[4])));
    composition.set_content(move || {
        let (mine, other) = (mine.clone(), other.clone());
        Node(true, "root".to_string(), || {
            if shown_handle.get() {
                PausableContentFor(7, || Item(mine.clone()));
                PausableContentFor(8, || Item(other.clone()));
            }
        });
    });
    let dump = || composition.with_applier(|tree: &mut Tree| tree.dump()).replace('"', "");
    shown.set(true);
    let paused = composition.recompose_pausable_for(7);
    let (callback, _) = scripted(&[false, true]);
    assert!(!paused.resume(callback));
    assert_eq!(dump(), "root\n  3:0\n    4:0\n", "the other target's content applied at once");
    assert!(paused.resume(|| false));
    paused.apply();
    assert_eq!(dump(), "root\n  1:0\n    2:0\n  3:0\n    4:0\n");
    composition.validate();
}
