//! Regression tests from the adversarial review of the phase A runtime: each one demonstrated a
//! finding, fixed since. The comment on each test describes the defect it guards against.

#![allow(non_snake_case)]

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::*;

// ---- Test support --------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct Label(String);

impl fmt::Debug for Label {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type Tree = NodeTree<Label>;

fn tree(composition: &Composition) -> String {
    composition.with_applier(|tree: &mut Tree| tree.dump())
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => payload.downcast_ref::<&str>().map(|s| s.to_string()).unwrap_or_default(),
    }
}

/// A non-restartable composable emitting one leaf node.
fn Leaf(label: &str) {
    let composer = current();
    let node = composer.start_node(key_of("review::Leaf"), || Label(label.to_string()));
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    composer.end_group();
}

/// A restartable composable whose body is a plain function of its arguments: the expansion's
/// shape, with the body passed in.
fn restartable<K: ComposeArg + Clone>(key: u64, args: K, body: fn(K)) {
    let composer = current();
    composer.start_restart_group(key);
    let changed = composer
        .update_kept(|kept: &(u64, K, fn(K))| !kept.1.same(&args), || (key, args.clone(), body));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        body(args);
    }
    composer.end_restart_group(|kept| {
        let (key, args, body) = kept.downcast_ref::<(u64, K, fn(K))>().expect("kept").clone();
        restartable(key, args, body)
    });
}

/// `#[composable] fn Reader(name, state)`: shows the state.
fn Reader(name: &'static str, state: MutableState<i32>) {
    restartable(key_of("review::Reader"), (name, state), |(name, state)| {
        Leaf(&format!("{name}={}", state.get()));
    });
}

// ---- Finding 1: stale-reader pruning drops the reads of a scope that is skipping ------------

thread_local! {
    static SHARED: Cell<Option<(MutableState<i32>, MutableState<bool>)>> = const { Cell::new(None) };
}

fn shared() -> (MutableState<i32>, MutableState<bool>) {
    SHARED.with(Cell::get).expect("shared states")
}

/// `#[composable] fn Late()`: reads `flag`, and `a` only when the flag is set.
fn Late() {
    restartable(key_of("review::Late"), (), |()| {
        let (a, flag) = shared();
        if flag.get() {
            Leaf(&format!("late={}", a.get()));
        }
    });
}

/// `#[composable] fn Parent()`: reads `a` itself, and has four other readers of `a` below it.
fn Parent() {
    restartable(key_of("review::Parent"), (), |()| {
        let (a, _) = shared();
        Leaf(&format!("parent={}", a.get()));
        Reader("d1", a);
        Reader("d2", a);
        Reader("d3", a);
        Late();
    });
}

/// Expected: `Parent` read `a` in its latest run, so writing `a` recomposes it.
///
/// Actual: `Parent` is started by its parent (its epoch is bumped to e+1), finds its arguments
/// unchanged and skips, walking into its dirty child `Late`. `Late` reads `a` for the first time;
/// `a`'s reader vector is at capacity (4: Parent, d1, d2, d3), so `register_read` prunes readers
/// that are not current — and Parent's reader, recorded at epoch e, is "not current" during this
/// window. Parent's `end_group` then restores its epoch to e, but its read of `a` is gone, so the
/// next write of `a` misses it: the tree keeps showing `parent=0`. (state.rs `register_read`,
/// composer.rs `start_group`/`end_group` epoch save/restore.) The same window also lets
/// `invalidate_readers` prune a skipping scope for a write made during composition (D40 allows it
/// in release builds).
#[test]
fn a_skipped_scope_loses_its_reads_when_a_child_prunes_stale_readers() {
    let a = mutable_state_of(0);
    let flag = mutable_state_of(false);
    let outer = mutable_state_of(0);
    SHARED.with(|shared| shared.set(Some((a.handle(), flag.handle()))));
    let composition = Composition::new(Tree::new());
    let outer_handle = outer.handle();
    composition.set_content(move || {
        // The content reads `outer`, so writing it re-runs the content, which starts `Parent`
        // again (it skips: no arguments changed).
        Leaf(&format!("outer={}", outer_handle.get()));
        Parent();
    });
    assert_eq!(tree(&composition), "outer=0\nparent=0\nd1=0\nd2=0\nd3=0\n");

    // One pass: the content re-runs, Parent skips, and Late (invalid) starts reading `a`.
    outer.set(1);
    flag.set(true);
    composition.recompose();
    assert_eq!(tree(&composition), "outer=1\nparent=0\nd1=0\nd2=0\nd3=0\nlate=0\n");

    a.set(7);
    composition.recompose();
    assert_eq!(
        tree(&composition),
        "outer=1\nparent=7\nd1=7\nd2=7\nd3=7\nlate=7\n",
        "Parent read `a` in its latest run and must recompose when `a` changes"
    );
}

// ---- Finding 2: effects and observers run while the pass is still "active" ------------------

/// Expected: `SideEffect` runs "after a successful pass" (docs/04), so it may compose another
/// composition, as Compose's `Popup`/`Dialog` do from an effect (`setContent` composes the popup
/// synchronously inside the parent's apply phase).
///
/// Actual: `Pass::finish` keeps `runtime.active` set while it runs `apply_deferred` (releases,
/// `on_forgotten`, drops, `on_remembered`, side effects), so `set_content` on another composition
/// panics with "a composition pass is already running on this thread", and the panic poisons the
/// parent composition. The same happens from `DisposableEffect`, `on_remembered`/`on_forgotten`,
/// a `Drop` of a removed value, and a frame requester that recomposes synchronously after a
/// side-effect write. (composition.rs `Pass::finish`, `Pass::begin`.)
#[test]
fn a_side_effect_can_compose_another_composition() {
    let popup: Rc<RefCell<Option<Composition>>> = Rc::new(RefCell::new(None));
    let composition = Composition::new(Tree::new());
    let slot = popup.clone();
    composition.set_content(move || {
        Leaf("main");
        let slot = slot.clone();
        SideEffect(move || {
            if slot.borrow().is_none() {
                let child = Composition::new(Tree::new());
                child.set_content(|| Leaf("popup"));
                *slot.borrow_mut() = Some(child);
            }
        });
    });
    assert_eq!(tree(&composition), "main\n");
    let popup = popup.borrow();
    assert_eq!(tree(popup.as_ref().expect("popup composed")), "popup\n");
}

/// Expected: a host whose frame requester recomposes synchronously (a test host, an immediate
/// scheduler) works when a side effect writes state, as it does when a handler writes it.
///
/// Actual: the write invalidates a scope, `request_frames` calls the requester at once, and the
/// requester's `recompose()` panics with "a composition pass is already running", because the
/// side effect runs while the writing composition's pass is still active.
#[test]
fn a_synchronous_frame_requester_survives_a_side_effect_write() {
    let composition = Rc::new(Composition::new(Tree::new()));
    let count = mutable_state_of(0);
    let handle = count.handle();
    let weak = Rc::downgrade(&composition);
    composition.set_frame_requester(move || {
        if let Some(composition) = weak.upgrade() {
            composition.recompose();
        }
    });
    composition.set_content(move || {
        let value = handle.get();
        Leaf(&format!("count={value}"));
        SideEffect(move || {
            if value < 2 {
                handle.set(value + 1);
            }
        });
    });
    assert_eq!(tree(&composition), "count=2\n");
}

// ---- Finding 6: an applier that writes state panics, contrary to the `Applier` docs ----------

/// An applier that counts its insertions in a state cell, e.g. for a semantics or focus owner.
struct CountingApplier {
    tree: Tree,
    inserted: MutableState<u32>,
}

impl Applier for CountingApplier {
    type Node = Label;

    fn root(&self) -> NodeRef {
        self.tree.root()
    }

    fn insert(&mut self, parent: NodeRef, index: usize, node: Label) -> NodeRef {
        self.inserted.update(|count| count + 1);
        Applier::insert(&mut self.tree, parent, index, node)
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        Applier::remove(&mut self.tree, parent, index, count)
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        Applier::move_nodes(&mut self.tree, parent, from, to, count)
    }

    fn release(&mut self, node: NodeRef) {
        Applier::release(&mut self.tree, node)
    }
}

/// Insertions and moves are applied during composition (D31), so an applier is composable code:
/// it may read state, but writing it then is a misuse (debug panic). Nodes it releases at the end
/// of the pass drop outside composition, and may write state.
#[cfg(debug_assertions)]
#[test]
fn an_applier_writing_state_during_composition_is_a_misuse() {
    let inserted = mutable_state_of(0u32);
    let composition =
        Composition::new(CountingApplier { tree: Tree::new(), inserted: inserted.handle() });
    let result = catch_unwind(AssertUnwindSafe(|| {
        composition.set_content(|| {
            let composer = current();
            composer.start_node(key_of("review::CountedLeaf"), || Label("x".into()));
            composer.end_group();
        })
    }));
    assert!(panic_message(result.expect_err("a misuse")).contains("during composition"));
}

// ---- Finding 8: a stale handle's panic loses the creation site once its slot is reused -------

/// Expected (rule R3, docs/05): using a freed handle panics "naming the cell's creation site".
///
/// Actual: the arena slot is reused LIFO by the very next `mutable_state_of`, which overwrites
/// `created_at`; `Cells::entry` then sees a generation two ahead and reports no creation site
/// ("state used at … was freed"). In a running app, cells are created constantly, so the
/// creation site is almost never reported. (state.rs `Cells::entry`, `Cells::alloc`.)
#[test]
fn a_stale_handle_names_its_creation_site_after_the_slot_is_reused() {
    let (stale, created_line) = {
        let owner = mutable_state_of(1); // the creation site to report
        (owner.handle(), line!() - 1)
    }; // no composition on this thread: freed at once
    let _reuser = mutable_state_of(2); // reuses the freed slot
    let message = panic_message(
        catch_unwind(AssertUnwindSafe(|| {
            stale.get();
        }))
        .expect_err("a stale handle panics"),
    );
    assert!(
        message.contains(&format!("review.rs:{created_line}:")),
        "the panic should name where the stale cell was created: {message}"
    );
}

// ---- A differential fuzz: recomposition must equal a pure render of the data ----------------
//
// Shapes the model test doesn't exercise: state cells shared by many scopes, content lambdas
// read in a child's scope (D10), groups contributing several nodes without a node of their own,
// branches made of replace groups with different keys and node counts, remember_keyed lambdas.

struct FItem {
    id: u64,
    keyed: bool,
    kind: usize,
    reads: Vec<usize>,
    branch_read: usize,
    branch: OwnedState<bool>,
    children: OwnedState<Vec<Rc<FItem>>>,
}

impl FItem {
    fn new(random: &mut Random, id: u64) -> Rc<FItem> {
        let reads = (0..random.below(3)).map(|_| random.below(SHARED_COUNT)).collect();
        Rc::new(FItem {
            id,
            keyed: random.chance(50),
            kind: random.below(3),
            reads,
            branch_read: random.below(SHARED_COUNT),
            branch: mutable_state_of(random.chance(50)),
            children: mutable_state_of_with(Vec::new(), never_equal_policy()),
        })
    }

    fn copy(&self) -> Rc<FItem> {
        Rc::new(FItem {
            id: self.id,
            keyed: self.keyed,
            kind: self.kind,
            reads: self.reads.clone(),
            branch_read: self.branch_read,
            branch: mutable_state_of(self.branch.get()),
            children: mutable_state_of_with(self.children.get(), never_equal_policy()),
        })
    }
}

const SHARED_COUNT: usize = 4;

thread_local! {
    static FUZZ_SHARED: RefCell<Vec<MutableState<u32>>> = const { RefCell::new(Vec::new()) };
}

fn fuzz_shared(index: usize) -> MutableState<u32> {
    FUZZ_SHARED.with(|shared| shared.borrow()[index])
}

fn item_label(item: &FItem) -> String {
    let mut label = format!("i{}", item.id);
    for &read in &item.reads {
        label += &format!(":{}", fuzz_shared(read).get());
    }
    label
}

const FUZZ_KINDS: [u64; 3] =
    [key_of("review::FItem0"), key_of("review::FItem1"), key_of("review::FItem2")];

fn FuzzNode(label: String, content: impl FnOnce()) {
    let composer = current();
    let node = composer.start_node(key_of("review::FuzzNode"), || Label(label.clone()));
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label;
        }
    });
    content();
    composer.end_group();
}

fn branch_part(item: &FItem) {
    let composer = current();
    if item.branch.get() {
        composer.start_replace_group(key_of("review::branch::then"));
        Leaf(&format!("b{}", fuzz_shared(item.branch_read).get()));
        composer.end_group();
    } else {
        composer.start_replace_group(key_of("review::branch::else"));
        Leaf("x");
        Leaf("y");
        composer.end_group();
    }
}

fn children_part(item: &FItem) {
    let composer = current();
    composer.start_replace_group(key_of("review::children"));
    item.children.with(|children| {
        for child in children {
            if child.keyed {
                key(child.id, || FuzzItem(child.clone()));
            } else {
                FuzzItem(child.clone());
            }
        }
    });
    composer.end_group();
}

/// `#[composable] fn Wrapper(content: Callback)`: a node holding a content lambda, which reads
/// state in Wrapper's scope (D10).
fn Wrapper(content: Rc<dyn Fn()>) {
    let composer = current();
    composer.start_restart_group(key_of("review::Wrapper"));
    let changed = composer.update_kept(
        |kept: &(Rc<dyn Fn()>,)| !Rc::ptr_eq(&kept.0, &content),
        || (content.clone(),),
    );
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        FuzzNode("W".into(), || content());
    }
    composer.end_restart_group(|kept| {
        let (content,) = kept.downcast_ref::<(Rc<dyn Fn()>,)>().expect("kept").clone();
        Wrapper(content)
    });
}

fn FuzzItem(item: Rc<FItem>) {
    let composer = current();
    composer.start_restart_group(FUZZ_KINDS[item.kind]);
    let changed =
        composer.update_kept(|kept: &(Rc<FItem>,)| !kept.0.same(&item), || (item.clone(),));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        match item.kind {
            0 => FuzzNode(item_label(&item), || {
                branch_part(&item);
                children_part(&item);
            }),
            1 => {
                Leaf(&item_label(&item));
                branch_part(&item);
                children_part(&item);
            }
            _ => {
                // A memoized content lambda, as `remember_lambda` would make it.
                let captured = item.clone();
                let content: Rc<dyn Fn()> = remember_keyed(item.clone(), move || {
                    let item = captured.clone();
                    Rc::new(move || {
                        Leaf(&item_label(&item));
                        branch_part(&item);
                        children_part(&item);
                    }) as Rc<dyn Fn()>
                });
                Wrapper(content);
            }
        }
    }
    composer.end_restart_group(|kept| {
        let (item,) = kept.downcast_ref::<(Rc<FItem>,)>().expect("kept").clone();
        FuzzItem(item)
    });
}

fn render(item: &FItem, depth: usize, out: &mut String) {
    let line = |depth: usize, text: &str, out: &mut String| {
        out.push_str(&"  ".repeat(depth));
        out.push_str(text);
        out.push('\n');
    };
    let rest = |depth: usize, out: &mut String| {
        if item.branch.get() {
            line(depth, &format!("b{}", fuzz_shared(item.branch_read).get()), out);
        } else {
            line(depth, "x", out);
            line(depth, "y", out);
        }
        for child in item.children.get() {
            render(&child, depth, out);
        }
    };
    match item.kind {
        0 => {
            line(depth, &item_label(item), out);
            rest(depth + 1, out);
        }
        1 => {
            line(depth, &item_label(item), out);
            rest(depth, out);
        }
        _ => {
            line(depth, "W", out);
            line(depth + 1, &item_label(item), out);
            rest(depth + 1, out);
        }
    }
}

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

fn all_fuzz_items(item: &Rc<FItem>, out: &mut Vec<Rc<FItem>>) {
    out.push(item.clone());
    for child in item.children.get() {
        all_fuzz_items(&child, out);
    }
}

fn fuzz_mutate(random: &mut Random, root: &Rc<FItem>, next_id: &mut u64) {
    let mut items = Vec::new();
    all_fuzz_items(root, &mut items);
    let target = items[random.below(items.len())].clone();
    let len = target.children.with(Vec::len);
    match random.below(100) {
        0..=29 => {
            *next_id += 1;
            let child = FItem::new(random, *next_id);
            let position = random.below(len + 1);
            target.children.edit(|c| c.insert(position, child));
        }
        30..=41 if len > 0 => {
            let position = random.below(len);
            target.children.edit(|c| c.remove(position));
        }
        42..=53 if len > 1 => {
            let (from, to) = (random.below(len), random.below(len));
            target.children.edit(|c| {
                let child = c.remove(from);
                c.insert(to, child);
            });
        }
        54..=74 => fuzz_shared(random.below(SHARED_COUNT)).set(random.below(3) as u32),
        75..=89 => target.branch.set(!target.branch.get()),
        _ if len > 0 => {
            let position = random.below(len);
            let copy = target.children.with(|c| c[position].copy());
            target.children.edit(|c| c[position] = copy);
        }
        _ => {}
    }
}

/// Grows every shared cell's reader vector far beyond what a run needs, so the capacity-triggered
/// pruning of finding 1 never runs: the fuzz then checks everything else.
fn warm_reader_capacity() {
    let warm = Composition::new(Tree::new());
    warm.set_content(|| {
        for index in 0..2000u32 {
            key(index, || {
                restartable(key_of("review::warm"), (), |()| {
                    for shared in 0..SHARED_COUNT {
                        fuzz_shared(shared).get();
                    }
                })
            });
        }
    });
    drop(warm);
    // A write prunes the dead readers; the capacity stays.
    for shared in 0..SHARED_COUNT {
        let state = fuzz_shared(shared);
        state.set(state.get() + 1);
        state.set(0);
    }
}

fn fuzz_seed(seed: u64, steps: usize, warm: bool) -> Result<(), String> {
    let (states_before, scopes_before) = (live_state_count(), live_scope_count());
    let owners: Vec<OwnedState<u32>> = (0..SHARED_COUNT).map(|_| mutable_state_of(0)).collect();
    FUZZ_SHARED.with(|shared| *shared.borrow_mut() = owners.iter().map(|o| o.handle()).collect());
    if warm {
        warm_reader_capacity();
    }
    let mut random = Random(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut next_id = 0;
    let root = FItem::new(&mut random, 0);
    let composition = Composition::new(Tree::new());
    let content_root = root.clone();
    composition.set_content(move || FuzzItem(content_root.clone()));
    for step in 0..steps {
        let mut expected = String::new();
        render(&root, 0, &mut expected);
        let actual = tree(&composition);
        if actual != expected {
            return Err(format!(
                "seed {seed} step {step}:\nexpected:\n{expected}\nactual:\n{actual}"
            ));
        }
        composition.validate();
        for _ in 0..=random.below(3) {
            fuzz_mutate(&mut random, &root, &mut next_id);
        }
        composition.recompose();
    }
    drop(composition);
    drop(root);
    drop(owners);
    let (states, scopes) = (live_state_count(), live_scope_count());
    if (states, scopes) != (states_before, scopes_before) {
        return Err(format!(
            "seed {seed}: {states} state cells and {scopes} scopes live after dropping \
             everything, {states_before} and {scopes_before} before"
        ));
    }
    Ok(())
}

// ---- Finding 9: a SideEffect allocates on every pass that runs it --------------------------

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { std::alloc::System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { std::alloc::System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations_in(f: impl FnOnce()) -> u64 {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

/// `#[composable] fn Effectful(count: MutableState<u64>)`: shows the count and reports it from a
/// `SideEffect`, as Compose code commonly does (e.g. to sync a value to a non-Compose object).
fn Effectful(count: MutableState<u64>) {
    restartable(key_of("review::Effectful"), count, |count| {
        let value = count.get();
        let reported = remember(|| Rc::new(Cell::new(0u64)));
        SideEffect(move || reported.set(value));
    });
}

/// Expected (docs/04, phase A exit criterion): "a steady-state pass allocates nothing: a
/// recomposition with no structural change".
///
/// Actual: `Composer::record_side_effect` boxes every effect (`Box<dyn FnOnce()>`), so any scope
/// that calls `SideEffect` with a capturing closure allocates once per pass that runs it. The
/// reusable `side_effects` vector is fine; the box is not. (composer.rs `record_side_effect`,
/// effects.rs `SideEffect`.) Compose allocates a lambda here too, so this may be acceptable, but
/// then the docs' claim needs the exception.
#[test]
fn a_side_effect_in_a_steady_state_pass_allocates_nothing() {
    let count = mutable_state_of(0u64);
    let handle = count.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || Effectful(handle));
    for round in 1..5 {
        count.set(round);
        composition.recompose();
    }
    let allocated = allocations_in(|| {
        count.set(100);
        assert!(composition.recompose());
    });
    assert_eq!(allocated, 0, "a recomposition with no structural change");
}

// ---- Finding 4: disposing the last composition frees cells while its values still drop ------

/// A remembered helper whose `Drop` writes a state remembered before it in the same group, e.g.
/// a controller marking itself detached. "`Drop` covers the common case" (docs/04).
struct Detaching(MutableState<bool>);

impl Drop for Detaching {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

fn Attached() {
    restartable(key_of("review::Attached"), (), |()| {
        let attached = remember(|| mutable_state_of(true));
        let _controller = remember(|| Rc::new(Detaching(attached)));
        Leaf("attached");
    });
}

/// Expected (R2): a retired cell stays live until the end of the next pass, or until the
/// disposal that retired it has finished; values of one group can rely on each other in `Drop`,
/// as they do when the group is removed by a pass or when another composition is still alive.
///
/// Actual: `Disposal::take` removes the composition from the arena first, so while its values
/// drop, `compositions.live()` is 0 and `OwnedState::drop` frees the cell at once (state.rs).
/// Values drop in slot order, so `Detaching::drop` then writes a freed cell and panics ("state
/// … was used … after it was freed") out of `Composition::drop` — an abort if it was already
/// unwinding. With a second composition on the thread, the same code works.
#[test]
fn dropping_the_last_composition_keeps_cells_alive_while_its_values_drop() {
    let composition = Composition::new(Tree::new());
    composition.set_content(Attached);
    // Works while another composition is alive:
    let other = Composition::new(Tree::new());
    let first = Composition::new(Tree::new());
    first.set_content(Attached);
    drop(first);
    drop(other);
    // Fails for the last one:
    drop(composition);
}

// ---- Finding 5: removing the first keyed item moves every other item ------------------------

thread_local! {
    static MOVES: Cell<usize> = const { Cell::new(0) };
}

/// A `NodeTree` that counts `move_nodes` calls.
struct MoveCounting(Tree);

impl Applier for MoveCounting {
    type Node = Label;

    fn root(&self) -> NodeRef {
        self.0.root()
    }

    fn insert(&mut self, parent: NodeRef, index: usize, node: Label) -> NodeRef {
        Applier::insert(&mut self.0, parent, index, node)
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        Applier::remove(&mut self.0, parent, index, count)
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        MOVES.with(|moves| moves.set(moves.get() + 1));
        Applier::move_nodes(&mut self.0, parent, from, to, count)
    }

    fn release(&mut self, node: NodeRef) {
        Applier::release(&mut self.0, node)
    }
}

fn CountedLeaf(label: String) {
    let composer = current();
    composer.start_node(key_of("review::CountedLeaf"), || Label(label));
    composer.end_group();
}

/// Expected: removing the first of N keyed rows (a queue, a chat log, a sliding window) costs
/// one node removal, as it does for the last row; at worst one coalesced move, as Compose's
/// `recordMoveNode` coalesces adjacent moves.
///
/// Actual: matching (composer.rs `start_group`) finds each remaining row one past the cursor,
/// which still points at the removed row, and moves it in front of the cursor with its own
/// `Applier::move_nodes` call: N-1 moves to the applier (each one a layout invalidation in a UI
/// tree) before the removed row is finally removed at the parent's end.
#[test]
fn removing_the_first_keyed_item_moves_no_other_item() {
    let rows = mutable_state_of_with((0..100u32).collect::<Vec<_>>(), never_equal_policy());
    let handle = rows.handle();
    let composition = Composition::new(MoveCounting(Tree::new()));
    composition.set_content(move || {
        handle.with(|rows| {
            for &row in rows {
                key(row, || CountedLeaf(format!("row{row}")));
            }
        })
    });
    MOVES.with(|moves| moves.set(0));
    rows.edit(|rows| {
        rows.remove(0);
    });
    composition.recompose();
    let moves = MOVES.with(Cell::get);
    assert!(moves <= 1, "removing the first row made {moves} applier moves");
}

// ---- Finding 3: a Composer captured by a content lambda goes stale --------------------------

/// Expected: the expansion of docs/03 and sketches/counter.expanded.rs binds `let c = current();`
/// once per composable and uses `c` for the region groups it inserts, including inside the
/// content lambdas it memoizes (`remember_lambda`). D10 says such a lambda is re-run by the
/// restart of the composable that invokes it, in a later pass. That must work.
///
/// Actual: `Composer` carries the id of the pass it was obtained in, and every method checks it
/// (composer.rs `check_composing`), so the memoized lambda's captured `c` panics with "a
/// Composer from an earlier pass was used" the first time the invoking composable restarts.
/// Phase B then has to call `current()` (a TLS lookup and a runtime borrow) inside every closure,
/// or count `c` as a capture (which is not `ComposeArg`, so the lambda would never be memoized and
/// its callee would never skip). Beyond the composing check, the pass check only rejects a
/// composer carried into a later pass, which is exactly what D10 restarts do.
#[test]
fn a_memoized_content_lambda_may_use_the_composer_it_captured() {
    let flag = mutable_state_of(false);
    let handle = flag.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let c = current();
        // `Wrapper!({ if flag.get() { Leaf("on") } })`, expanded: a memoized content lambda
        // with a region group for the branch.
        let content: Rc<dyn Fn()> = remember(move || {
            Rc::new(move || {
                if handle.get() {
                    c.start_replace_group(key_of("review::content::then"));
                    Leaf("on");
                    c.end_group();
                }
            }) as Rc<dyn Fn()>
        });
        Wrapper(content);
    });
    assert_eq!(tree(&composition), "W\n");
    // The read in the content registered Wrapper's scope: Wrapper restarts alone, re-running
    // the content lambda memoized in the first pass.
    flag.set(true);
    composition.recompose();
    assert_eq!(tree(&composition), "W\n  on\n");
}

/// Recomposition matches a pure render of the data over many seeds, with reader vectors both
/// warm and cold (cold ones prune often: finding 1).
#[test]
fn fuzz_recomposition_matches_a_pure_render() {
    let seeds = std::env::var("FUZZ_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(50u64);
    let seeds = if cfg!(miri) { seeds.min(3) } else { seeds };
    for seed in 0..seeds {
        for warm in [true, false] {
            if let Err(error) = fuzz_seed(seed, if cfg!(miri) { 12 } else { 80 }, warm) {
                panic!("warm {warm}: {error}");
            }
        }
    }
}
