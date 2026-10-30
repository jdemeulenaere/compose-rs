//! A differential fuzz of composition trees (composition-context.md §6): compositions under places
//! of other compositions, made in bodies and in effects, coming and going, against plain arrays:
//! what each last read of its state and of what its parent provides, parents composing first, and
//! none composing once what holds it went in that frame. Seeded and bounded, as
//! composition_fuzz.rs.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

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
