//! A differential fuzz of composition trees (composition-context.md §6): compositions under places
//! of other compositions, made in bodies and in effects, coming and going, against plain arrays:
//! what each last read of its state and of what its parent provides, parents composing first, and
//! none composing once what holds it went in that frame. Seeded and bounded, as
//! composition_fuzz.rs. The wider one adds static locals, shared trees, lends, reuse, panics, a
//! parent's effect setting its child's content and `perform_frame`, against the arrays and, in each
//! frame, the order it's to do things in.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::snapshots::Snapshot;
use compose_runtime::*;

use crate::mock::*;

/// A root, 2 children, each with 2 children.
const NODES: usize = 7;
fn parent_of(node: usize) -> Option<usize> {
    (node > 0).then(|| (node - 1) / 2)
}
fn children_of(node: usize) -> [usize; 2] {
    [node * 2 + 1, node * 2 + 2]
}

static LOCAL: CompositionLocal<i32> = CompositionLocal::new("tree.local", || -1);

struct World {
    values: Vec<OwnedState<i32>>,
    shown: Vec<OwnedState<bool>>,
    /// What the root writes while composing, for the leaves to read in that frame.
    copy: OwnedState<i32>,
    made_in_body: Vec<bool>,
    seen: RefCell<Vec<(i32, i32, i32)>>,
    live: RefCell<Vec<Option<Rc<Composition>>>>,
    order: RefCell<Vec<usize>>,
}

/// Everything of a composition's content: what it reads, then a place for each child.
#[composable(non_restartable)]
fn Node(node: usize, world: Rc<World>) {
    let value = world.values[node].get();
    if node == 0 {
        world.copy.set(value);
    }
    let copy = if node >= 3 { world.copy.get() } else { -1 };
    let local = LOCAL.current();
    world.seen.borrow_mut()[node] = (value, local, copy);
    world.order.borrow_mut().push(node);
    // Once what holds it went in this frame, it's not composed (b/254645321).
    let mut above = Some(node);
    while let Some(at) = above {
        assert!(world.shown[at].get(), "{node} composed under a place that went");
        above = parent_of(at);
    }
    Text(&value.to_string());
    if node >= 3 {
        return;
    }
    CompositionLocalProvider(LOCAL.provides(value), || {
        for child in children_of(node) {
            if world.shown[child].get() {
                let world = world.clone();
                key(child, move || Hosted(child, world.clone()));
            }
        }
    });
}

/// A place and the composition under it.
#[composable(non_restartable)]
fn Hosted(node: usize, world: Rc<World>) {
    let parent = remember_composition_context();
    let child = remember(|| Rc::new(Composition::with_parent(ViewApplier::new(), parent)));
    let compose = {
        let (child, world) = (child.clone(), world.clone());
        move || {
            let world = world.clone();
            child.set_content(move || Node(node, world.clone()));
        }
    };
    if world.made_in_body[node] {
        let done = remember(|| Rc::new(Cell::new(false)));
        if !done.replace(true) {
            compose();
        }
    }
    let in_effect = !world.made_in_body[node];
    let (held, world) = (child.clone(), world.clone());
    DisposableEffect((), move |scope| {
        if in_effect {
            compose();
        }
        world.live.borrow_mut()[node] = Some(held.clone());
        scope.on_dispose(move || {
            world.live.borrow_mut()[node] = None;
            held.dispose();
        })
    });
}

fn run(seed: u64) {
    let mut random = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut next = move |bound: u64| {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        random % bound
    };
    let world = Rc::new(World {
        values: (0..NODES).map(|_| mutable_state_of(0)).collect(),
        shown: (0..NODES).map(|_| mutable_state_of(true)).collect(),
        copy: mutable_state_of(0),
        made_in_body: (0..NODES).map(|_| next(2) == 0).collect(),
        seen: RefCell::new(vec![(0, -1, -1); NODES]),
        live: RefCell::new(vec![None; NODES]),
        order: RefCell::new(Vec::new()),
    });
    let mut test = composition_test();
    let w = world.clone();
    test.compose(move || Node(0, w.clone()));
    for _ in 0..30 {
        for _ in 0..=next(3) {
            let node = next(NODES as u64) as usize;
            if next(4) == 0 && node > 0 {
                world.shown[node].update(|shown| !shown);
            } else {
                world.values[node].set(next(1000) as i32);
            }
        }
        // A frame at a time: parents before children, in each.
        world.order.borrow_mut().clear();
        test.composition().recompose();
        {
            let order = world.order.borrow();
            let first = |node: usize| order.iter().position(|other| *other == node);
            for node in 1..NODES {
                if let (Some(child), Some(parent)) = (first(node), parent_of(node).and_then(first))
                {
                    assert!(parent < child, "{order:?}");
                }
            }
        }
        test.advance();
        check(&world);
    }
    test.dispose();
    assert!(world.live.borrow().iter().all(Option::is_none), "seed {seed}");
}

fn check(world: &World) {
    let value = |node: usize| world.values[node].get();
    let seen = world.seen.borrow();
    let live = world.live.borrow();
    for node in 1..NODES {
        let mut above = Some(node);
        let mut shown = true;
        while let Some(at) = above {
            shown &= world.shown[at].get();
            above = parent_of(at);
        }
        assert_eq!(live[node].is_some(), shown, "{node} shown");
        let Some(composition) = &live[node] else { continue };
        composition.validate();
        let local = value(parent_of(node).expect("not the root"));
        let copy = if node >= 3 { value(0) } else { -1 };
        assert_eq!(seen[node], (value(node), local, copy), "{node}");
        let dump = composition.with_applier(|views: &mut ViewApplier| views.tree.dump());
        assert_eq!(dump.trim(), format!("<text text='{}'>", value(node)), "{node}");
    }
}

#[test]
fn composition_trees_agree_with_arrays() {
    for seed in 1..=150 {
        run(seed);
    }
}

// ---- Wider: kinds of composition, locals of both kinds, frames of both kinds -----------------

static STATIC: CompositionLocal<i32> = CompositionLocal::new_static("tree.static", || -2);

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Its own tree.
    Plain,
    /// Its own tree, deactivated and reused now and then.
    Reusable,
    /// Below a node of its parent's tree, sharing it, made in an effect as that node's there then.
    Shared,
}

enum Handle {
    Plain(Composition),
    Reusable(ReusableComposition),
}

impl Handle {
    fn composition(&self) -> &Composition {
        match self {
            Handle::Plain(composition) => composition,
            Handle::Reusable(composition) => composition,
        }
    }

    fn set_content(&self, content: impl Fn() + 'static) {
        match self {
            Handle::Plain(composition) => composition.set_content(content),
            Handle::Reusable(composition) => composition.set_content(content),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    value: i32,
    local: i32,
    static_local: i32,
    copy: i32,
    echoes: [i32; 2],
}

struct Wide {
    values: Vec<OwnedState<i32>>,
    statics: Vec<OwnedState<i32>>,
    echoes: Vec<OwnedState<i32>>,
    shown: Vec<OwnedState<bool>>,
    copy: OwnedState<i32>,
    kinds: Vec<Kind>,
    in_body: Vec<bool>,
    provides_static: Vec<bool>,
    set_in_parents_effect: Vec<bool>,
    /// A composition beside its children, composing in its provider's content each pass, with what
    /// it's told there for a composition out of the tree to hear.
    aside: Vec<bool>,
    told_outer: Vec<OwnedState<i32>>,
    told_inner: Vec<OwnedState<i32>>,
    /// What's read in the provider's content after that.
    after_aside: RefCell<Vec<i32>>,
    /// What the composition out of the tree last read of those told.
    heard: RefCell<Vec<(i32, i32)>>,
    seen: RefCell<Vec<Seen>>,
    /// Each node's composite key hash, once composed.
    hashes: RefCell<Vec<Option<u64>>>,
    live: RefCell<Vec<Option<Rc<Handle>>>>,
    roots: RefCell<Vec<Option<NodeRef>>>,
    /// Its first pass is over: what's left is a frame's.
    composed: Vec<Cell<bool>>,
    /// When each last composed since the frames of this round began, in passes; 0, it didn't.
    passes: Vec<Cell<u64>>,
    next_pass: Cell<u64>,
    fail: Vec<Cell<bool>>,
    poisoned: Vec<Cell<bool>>,
    /// "begin" and "end" of the root's tree, "c<node>" and "w<node>" of a frame's passes and effects.
    log: Rc<RefCell<Vec<String>>>,
    /// In a pass set by hand, not a frame's.
    quiet: Cell<bool>,
}

#[composable(non_restartable)]
fn WideNode(node: usize, world: Rc<Wide>) {
    if world.fail[node].replace(false) {
        world.poisoned[node].set(true);
        panic!("fuzz: {node} panics, as asked");
    }
    world.next_pass.set(world.next_pass.get() + 1);
    world.passes[node].set(world.next_pass.get());
    let value = world.values[node].get();
    if node == 0 {
        world.copy.set(value);
    }
    let copy = if node >= 3 { world.copy.get() } else { -1 };
    world.echoes[node].set(value);
    let echoes =
        if node < 3 { children_of(node).map(|child| world.echoes[child].get()) } else { [-1, -1] };
    let local = LOCAL.current();
    {
        let mut seen = world.seen.borrow_mut();
        seen[node] = Seen { value, local, static_local: seen[node].static_local, copy, echoes };
    }
    let hash = current_composite_key_hash_code();
    {
        let mut hashes = world.hashes.borrow_mut();
        assert!(hashes[node].is_none_or(|known| known == hash), "{node}'s hash moved");
        hashes[node] = Some(hash);
    }
    let in_a_frame = world.composed[node].get();
    if in_a_frame && !world.quiet.get() {
        world.log.borrow_mut().push(format!("c{node}"));
    }
    let w = world.clone();
    SideEffect(move || {
        if in_a_frame && !w.quiet.get() {
            w.log.borrow_mut().push(format!("w{node}"));
        }
    });
    Text(&value.to_string());
    WideReader!(node, world.clone());
    if node >= 3 {
        return;
    }
    // Above the provider: what the aside reads above it moves only with what's above this.
    let above_provider = remember_composition_context();
    let aside = remember(|| Rc::new(Composition::with_parent(ViewApplier::new(), above_provider)));
    CompositionLocalProvider(LOCAL.provides(value), || {
        let children = || {
            if world.aside[node] {
                let (outer, inner) =
                    (world.told_outer[node].handle(), world.told_inner[node].handle());
                Snapshot::global(|| outer.set(value));
                Snapshot::send_apply_notifications();
                aside.set_content(move || {
                    Snapshot::global(|| inner.set(value));
                    Snapshot::send_apply_notifications();
                });
            }
            for child in children_of(node) {
                if world.shown[child].get() {
                    let world = world.clone();
                    key(child, move || WideHosted(child, world.clone()));
                }
            }
            WideAfterAside!(node, world.clone());
        };
        if world.provides_static[node] {
            let provided = world.statics[node].get();
            CompositionLocalProvider(STATIC.provides(provided), children);
        } else {
            children();
        }
    });
}

/// With what it's given the same, it runs only if what's static above it changed.
#[composable]
fn WideReader(node: usize, world: Rc<Wide>) {
    world.seen.borrow_mut()[node].static_local = STATIC.current();
}

#[composable]
fn WideAfterAside(node: usize, world: Rc<Wide>) {
    world.after_aside.borrow_mut()[node] = STATIC.current();
}

fn set_wide_content(handle: &Handle, node: usize, world: &Rc<Wide>) {
    let w = world.clone();
    handle.set_content(move || WideNode(node, w.clone()));
    world.composed[node].set(true);
}

fn find_tagged(views: &ViewApplier, tag: &str) -> NodeRef {
    let mut stack = vec![views.tree.root()];
    while let Some(node) = stack.pop() {
        if views.tree.get(node).is_some_and(|view| view.attribute("tag") == Some(tag)) {
            return node;
        }
        stack.extend(views.tree.children(node));
    }
    panic!("no host {tag}")
}

/// A place and the composition under it.
#[composable(non_restartable)]
fn WideHosted(node: usize, world: Rc<Wide>) {
    let parent = remember_composition_context();
    let kind = world.kinds[node];
    if kind == Kind::Shared {
        let tag = node.to_string();
        ReusableComposeNode::<ViewApplier>(view("linear"), |updater| {
            updater.set(tag, set_attribute("tag"))
        });
    }
    let held: Rc<RefCell<Option<Rc<Handle>>>> = remember(Rc::default);
    let make = {
        let (held, world) = (held.clone(), world.clone());
        move || {
            let handle = Rc::new(match kind {
                Kind::Plain => Handle::Plain(Composition::with_parent(ViewApplier::new(), parent)),
                Kind::Reusable => {
                    Handle::Reusable(Composition::reusable(ViewApplier::new(), parent))
                }
                Kind::Shared => {
                    let above = world.live.borrow()[parent_of(node).expect("a child")].clone();
                    let host = above.expect("what's above").composition().with_applier(
                        |views: &mut ViewApplier| find_tagged(views, &node.to_string()),
                    );
                    world.roots.borrow_mut()[node] = Some(host);
                    Handle::Plain(subcomposition(parent, host))
                }
            });
            if kind != Kind::Shared {
                let root =
                    handle.composition().with_applier(|views: &mut ViewApplier| views.root());
                world.roots.borrow_mut()[node] = Some(root);
            }
            *held.borrow_mut() = Some(handle.clone());
            world.live.borrow_mut()[node] = Some(handle.clone());
            set_wide_content(&handle, node, &world);
        }
    };
    if kind != Kind::Shared && world.in_body[node] && held.borrow().is_none() {
        make();
    }
    {
        let (held, world) = (held.clone(), world.clone());
        DisposableEffect((), move |scope| {
            if held.borrow().is_none() {
                make();
            }
            let handle = held.borrow().clone().expect("made");
            scope.on_dispose(move || {
                world.live.borrow_mut()[node] = None;
                world.roots.borrow_mut()[node] = None;
                world.composed[node].set(false);
                world.poisoned[node].set(false);
                world.fail[node].set(false);
                handle.composition().dispose();
            })
        });
    }
    // A slot's content set from an effect in the frame that composed it, as `precompose` is.
    if world.set_in_parents_effect[node] {
        let (held, world) = (held.clone(), world.clone());
        SideEffect(move || {
            let handle = held.borrow().clone();
            if let Some(handle) = handle
                && world.composed[node].get()
                && !world.poisoned[node].get()
                && !world.fail[node].get()
            {
                let quiet = world.quiet.replace(true);
                set_wide_content(&handle, node, &world);
                world.quiet.set(quiet);
            }
        });
    }
}

struct WideRun {
    world: Rc<Wide>,
    root: Rc<Handle>,
    /// Out of the tree: hears what passes are told.
    listener: Composition,
    recomposer: Option<Owned<Recomposer>>,
    requests: Rc<Cell<u32>>,
}

impl WideRun {
    fn frame(&self) {
        let root = self.root.composition();
        let framed = catch_unwind(AssertUnwindSafe(|| match &self.recomposer {
            Some(recomposer) => {
                perform_frame(recomposer.handle(), 0);
            }
            None => {
                root.recompose();
            }
        }));
        let _ = framed;
    }

    /// Frames until there's nothing to compose, each checked as it goes.
    fn settle(&self, seed: u64) {
        let world = &self.world;
        for passes in &world.passes {
            passes.set(0);
        }
        let root = self.root.composition();
        for _ in 0..20 {
            Snapshot::send_apply_notifications();
            if !root.has_invalidations() {
                return;
            }
            world.log.borrow_mut().clear();
            self.frame();
            self.check_frame(seed);
        }
        panic!("seed {seed}: frames don't end");
    }

    /// In a frame: all compose, then the work of each, its changes over before its effects.
    fn check_frame(&self, seed: u64) {
        let log = self.world.log.borrow();
        let mut in_changes = false;
        for line in log.iter() {
            match line.as_str() {
                "begin" => {
                    assert!(!in_changes, "seed {seed}: {log:?}");
                    in_changes = true;
                }
                "end" => in_changes = false,
                effect if effect.starts_with('w') => {
                    let mut node: usize = effect[1..].parse().expect("a node");
                    while node > 0 && self.world.kinds[node] == Kind::Shared {
                        node = parent_of(node).expect("not the root");
                    }
                    // Those of another tree don't wait for this one's.
                    assert!(
                        node != 0 || !in_changes,
                        "seed {seed}: effects with the changes open: {log:?}"
                    );
                }
                _ => {}
            }
        }
        let last_compose = log.iter().rposition(|line| line.starts_with('c'));
        let first_effect = log.iter().position(|line| line.starts_with('w'));
        if let (Some(compose), Some(effect)) = (last_compose, first_effect) {
            assert!(compose < effect, "seed {seed}: work before all composed: {log:?}");
        }
        // As Compose's `toApply`, in the order they composed, which a child's writes can take the
        // parent past (its trailing recomposition, in the frame).
        let mut composed = log.iter().filter(|line| line.starts_with('c'));
        for effect in log.iter().filter(|line| line.starts_with('w')) {
            assert!(
                composed.any(|line| line[1..] == effect[1..]),
                "seed {seed}: work out of order: {log:?}"
            );
        }
    }

    fn check(&self, seed: u64) {
        let world = &self.world;
        let root = self.root.composition();
        assert!(!root.has_invalidations(), "seed {seed}");
        Snapshot::send_apply_notifications();
        self.listener.recompose();
        {
            let heard = world.heard.borrow();
            for node in 0..3 {
                let told = (world.told_outer[node].get(), world.told_inner[node].get());
                assert_eq!(heard[node], told, "seed {seed}: what {node}'s passes were told");
            }
        }
        let value = |node: usize| world.values[node].get();
        let live = world.live.borrow().clone();
        let seen = world.seen.borrow();
        let hashes = world.hashes.borrow();
        for node in 0..NODES {
            let mut above = Some(node);
            let mut shown = true;
            while let Some(at) = above {
                shown &= world.shown[at].get();
                above = parent_of(at);
            }
            assert_eq!(live[node].is_some(), shown, "seed {seed}: {node} shown");
            let Some(handle) = &live[node] else { continue };
            if world.poisoned[node].get() {
                continue;
            }
            let composition = handle.composition();
            composition.validate();
            let local = parent_of(node).map_or(-1, value);
            let copy = if node >= 3 { value(0) } else { -1 };
            let echoes = if node < 3 {
                children_of(node).map(|child| world.echoes[child].get())
            } else {
                [-1, -1]
            };
            // A static change reaches it through each composition up to the provider, in turn,
            // as each only tells those under it what it composed with last.
            let mut provider = parent_of(node);
            let mut pass = world.passes[node].get();
            let mut reached = pass != 0;
            while let Some(at) = provider {
                reached &= (1..pass).contains(&world.passes[at].get());
                pass = world.passes[at].get();
                if world.provides_static[at] {
                    break;
                }
                provider = parent_of(at);
            }
            let static_local = if reached {
                provider.map_or(-2, |at| world.statics[at].get())
            } else {
                seen[node].static_local
            };
            if node < 3
                && world.aside[node]
                && world.provides_static[node]
                && world.passes[node].get() != 0
            {
                let after = world.after_aside.borrow()[node];
                assert_eq!(after, world.statics[node].get(), "seed {seed}: after {node}'s aside");
            }
            let expected = Seen { value: value(node), local, static_local, copy, echoes };
            assert_eq!(seen[node], expected, "seed {seed}: {node}");
            for other in 0..node {
                if live[other].is_some() {
                    assert_ne!(hashes[other], hashes[node], "seed {seed}: {other}, {node}");
                }
            }
            let at = world.roots.borrow()[node];
            let text = composition.with_applier(|views: &mut ViewApplier| {
                let at = at.unwrap_or_else(|| views.root());
                let first = views.tree.children(at).next().expect("its text");
                views.tree.get(first).expect("a view").attribute("text").map(str::to_string)
            });
            assert_eq!(text, Some(value(node).to_string()), "seed {seed}: {node}'s text");
        }
    }

    /// Whichever of the composition's tree holds `node`'s, lending it for its next pass.
    fn lend_for(&self, node: usize) {
        let world = &self.world;
        let live = world.live.borrow().clone();
        let (Some(handle), Some(owner)) =
            (&live[node], parent_of(node).and_then(|above| live[above].as_ref()))
        else {
            return;
        };
        let owner = owner.composition();
        let unlent = |views: &mut ViewApplier| {
            with_lent_applier(owner, views, || {
                let quiet = world.quiet.replace(true);
                set_wide_content(handle, node, world);
                world.quiet.set(quiet);
            })
        };
        owner.with_applier(unlent);
    }

    fn reuse(&self, node: usize) {
        let world = &self.world;
        let live = world.live.borrow().clone();
        let Some(handle) = &live[node] else { return };
        let Handle::Reusable(composition) = &**handle else { return };
        let quiet = world.quiet.replace(true);
        composition.deactivate();
        let w = world.clone();
        composition.set_content_with_reuse(move || WideNode(node, w.clone()));
        world.quiet.set(quiet);
    }
}

fn run_wide(seed: u64) {
    let mut random = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut next = move |bound: u64| {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        random % bound
    };
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let world = Rc::new(Wide {
        values: (0..NODES).map(|_| mutable_state_of(0)).collect(),
        statics: (0..NODES).map(|_| mutable_state_of(0)).collect(),
        echoes: (0..NODES).map(|_| mutable_state_of(0)).collect(),
        shown: (0..NODES).map(|_| mutable_state_of(true)).collect(),
        copy: mutable_state_of(0),
        kinds: (0..NODES)
            .map(|_| match next(3) {
                0 => Kind::Plain,
                1 => Kind::Reusable,
                _ => Kind::Shared,
            })
            .collect(),
        in_body: (0..NODES).map(|_| next(2) == 0).collect(),
        provides_static: (0..NODES).map(|node| node == 0 || next(2) == 0).collect(),
        set_in_parents_effect: (0..NODES).map(|_| next(5) == 0).collect(),
        aside: (0..NODES).map(|_| next(2) == 0).collect(),
        told_outer: (0..NODES).map(|_| mutable_state_of(0)).collect(),
        told_inner: (0..NODES).map(|_| mutable_state_of(0)).collect(),
        after_aside: RefCell::new(vec![-2; NODES]),
        heard: RefCell::new(vec![(0, 0); NODES]),
        seen: RefCell::new(vec![
            Seen {
                value: 0,
                local: -1,
                static_local: -2,
                copy: -1,
                echoes: [-1, -1]
            };
            NODES
        ]),
        hashes: RefCell::new(vec![None; NODES]),
        live: RefCell::new(vec![None; NODES]),
        roots: RefCell::new(vec![None; NODES]),
        composed: (0..NODES).map(|_| Cell::new(false)).collect(),
        passes: (0..NODES).map(|_| Cell::new(0)).collect(),
        next_pass: Cell::new(0),
        fail: (0..NODES).map(|_| Cell::new(false)).collect(),
        poisoned: (0..NODES).map(|_| Cell::new(false)).collect(),
        log: log.clone(),
        quiet: Cell::new(false),
    });
    let mut views = ViewApplier::new();
    views.change_log = Some(log);
    let recomposer = seed.is_multiple_of(2).then(Recomposer::new);
    let root = Rc::new(Handle::Plain(match &recomposer {
        Some(recomposer) => Composition::with_parent(views, recomposer.as_context()),
        None => Composition::new(views),
    }));
    let requests = Rc::new(Cell::new(0));
    if recomposer.is_none() {
        let r = requests.clone();
        root.composition().set_frame_requester(move || r.set(r.get() + 1));
    }
    world.live.borrow_mut()[0] = Some(root.clone());
    set_wide_content(&root, 0, &world);
    let listener = Composition::new(ViewApplier::new());
    let w = world.clone();
    listener.set_content(move || {
        for node in 0..3 {
            let told = (w.told_outer[node].get(), w.told_inner[node].get());
            w.heard.borrow_mut()[node] = told;
        }
    });
    let run = WideRun { world: world.clone(), root: root.clone(), listener, recomposer, requests };
    run.settle(seed);
    run.check(seed);
    for _ in 0..30 {
        let mut plain = true;
        let mut changed = false;
        let asked = run.requests.get();
        for _ in 0..=next(3) {
            let node = next(NODES as u64) as usize;
            let live = world.live.borrow()[node].is_some();
            let healthy = live && !world.poisoned[node].get() && !world.fail[node].get();
            let in_use = world.composed[node].get();
            match next(10) {
                0 | 1 if node > 0 => {
                    world.shown[node].update(|shown| !shown);
                    plain = false;
                }
                2 => world.statics[node].set(next(1000) as i32),
                3 if node > 0 && healthy && in_use && world.kinds[node] == Kind::Shared => {
                    run.lend_for(node);
                    plain = false;
                }
                4 if healthy && in_use && world.kinds[node] == Kind::Reusable => {
                    run.reuse(node);
                    plain = false;
                }
                5 if node >= 3 && healthy && in_use => {
                    world.fail[node].set(true);
                    world.values[node].update(|value| value + 1);
                    plain = false;
                }
                _ => {
                    let value = next(1000) as i32;
                    changed |= healthy && in_use && world.values[node].get() != value;
                    world.values[node].set(value);
                }
            }
        }
        Snapshot::send_apply_notifications();
        if plain && changed {
            assert!(run.root.composition().has_invalidations(), "seed {seed}: says it's clean");
        }
        if plain && run.recomposer.is_none() && run.root.composition().has_invalidations() {
            assert!(run.requests.get() > asked, "seed {seed}: invalid and no frame asked");
        }
        run.settle(seed);
        run.check(seed);
    }
    root.composition().dispose();
    run.listener.dispose();
    assert!(world.live.borrow()[1..].iter().all(Option::is_none), "seed {seed}");
    world.live.borrow_mut()[0] = None;
    if let Some(recomposer) = &run.recomposer {
        recomposer.cancel();
    }
}

#[test]
fn trees_of_every_kind_agree_with_arrays() {
    for seed in 1..=150 {
        run_wide(seed);
    }
}
