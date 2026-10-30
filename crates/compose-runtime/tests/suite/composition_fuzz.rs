//! A differential fuzz of composition in a snapshot (D791-D799): random passes, with what a pass does
//! in them, against plain arrays: what a pass, the global snapshot and held snapshots read, what
//! applies, and that every reader hears of what changed. Seeded and bounded, as snapshot_fuzz.rs.
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::{self, NodeTree, snapshot_paths_are_plain, state_records};
use compose_runtime::snapshots::{MutableSnapshot, Snapshot};
use compose_runtime::*;

const N: usize = 4;
type V = [i32; N];
struct Random(u64);
impl Random {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) % bound.max(1) as u64) as usize
    }
}
fn f0(v: &V) -> i32 {
    v.iter().sum()
}
fn f1(v: &V) -> i32 {
    if v[0] % 2 == 0 { v[1] } else { v[2] }
}
fn f2(v: &V) -> i32 {
    f1(v) * 100 + v[3]
}

enum Held {
    Ro(Snapshot, V),
    M(MutableSnapshot, V, [bool; N], [bool; N]),
}

#[derive(Clone, Copy, Debug)]
enum Op {
    W(usize, i32),
    R,
    GR,
    GW(usize, i32),
    NW(usize, i32),
    NA(usize, i32),
    Nro(usize, i32),
    HR,
    HA,
    Adv,
    Send,
    Panic,
}

struct World {
    states: Vec<MutableState<i32>>,
    ds: [State<i32>; 3],
}
impl World {
    fn check(&self, v: &V, what: &str) {
        for (i, state) in self.states.iter().enumerate() {
            assert_eq!(state.get(), v[i], "{what}: state {i}");
        }
        assert_eq!(self.ds[0].get(), f0(v), "{what}: f0 {v:?}");
        assert_eq!(self.ds[1].get(), f1(v), "{what}: f1 {v:?}");
        assert_eq!(self.ds[2].get(), f2(v), "{what}: f2 {v:?}");
    }
}

fn run(seed: u64, allow_panic: bool, global_writes: bool, held_apply: bool) {
    let mut random = Random(seed);
    let owned: Vec<_> = (0..N).map(|_| mutable_state_of(0)).collect();
    let states: Vec<MutableState<i32>> = owned.iter().map(|s| s.handle()).collect();
    let s0 = states.clone();
    let d0 = derived_state_of(move || s0.iter().map(|s| s.get()).sum::<i32>());
    let s1 = states.clone();
    let d1 = derived_state_of(move || if s1[0].get() % 2 == 0 { s1[1].get() } else { s1[2].get() });
    let r1 = d1.as_state();
    let s2 = states.clone();
    let d2 = derived_state_of(move || r1.get() * 100 + s2[3].get());
    let ds = [d0.as_state(), d1.as_state(), d2.as_state()];
    let world = Rc::new(World { states: states.clone(), ds });

    let global: Rc<RefCell<V>> = Rc::new(RefCell::new([0; N]));
    let helds: Rc<RefCell<Vec<Held>>> = Rc::default();
    let script: Rc<RefCell<Vec<Op>>> = Rc::default();
    let round = mutable_state_of(0);
    let read_round = round.handle();
    // What the last pass ended up writing.
    let written: Rc<RefCell<(V, [bool; N])>> = Rc::new(RefCell::new(([0; N], [false; N])));

    let writer_seen: Rc<RefCell<V>> = Rc::default();
    let make_writer = || {
        let (world, global, helds, script, written, writer_seen) = (
            world.clone(),
            global.clone(),
            helds.clone(),
            script.clone(),
            written.clone(),
            writer_seen.clone(),
        );
        let c = Composition::new(NodeTree::<()>::new());
        c.set_content(move || {
            read_round.get();
            let ops = std::mem::take(&mut *script.borrow_mut());
            let mut pass: V = *global.borrow();
            let mut wrote = [false; N];
            let mut gwrote = [false; N];
            let mut happlied = [false; N];
            let mut ros: Vec<(Snapshot, V)> = Vec::new();
            if seed.is_multiple_of(2) {
                world.check(&pass, "pass start");
            }
            for op in ops {
                match op {
                    Op::W(i, v) => {
                        if gwrote[i] || happlied[i] {
                            continue;
                        }
                        if pass[i] != v {
                            wrote[i] = true;
                        }
                        pass[i] = v;
                        world.states[i].set(v);
                        assert_eq!(world.states[i].get(), v);
                    }
                    Op::R => world.check(&pass, "pass read"),
                    Op::GR => {
                        let g = *global.borrow();
                        Snapshot::global(|| world.check(&g, "global in pass"));
                        world.check(&pass, "pass after global");
                    }
                    Op::GW(i, v) => {
                        if wrote[i] || happlied[i] {
                            continue;
                        }
                        let mut g = global.borrow_mut();
                        if g[i] != v {
                            gwrote[i] = true;
                            for h in helds.borrow_mut().iter_mut() {
                                if let Held::M(_, _, touched, _) = h {
                                    touched[i] = true;
                                }
                            }
                        }
                        g[i] = v;
                        drop(g);
                        Snapshot::global(|| world.states[i].set(v));
                    }
                    Op::NW(i, v) => {
                        if gwrote[i] || happlied[i] {
                            continue;
                        }
                        if pass[i] != v {
                            wrote[i] = true;
                        }
                        Snapshot::with_mutable_snapshot(|| {
                            world.states[i].set(v);
                            pass[i] = v;
                            world.check(&pass, "nested");
                        });
                        world.check(&pass, "after nested");
                    }
                    Op::NA(i, v) => {
                        let m = Snapshot::take_mutable_snapshot(None, None);
                        let mut copy = pass;
                        copy[i] = v;
                        m.enter(|| {
                            world.states[i].set(v);
                            world.check(&copy, "abandoned nested");
                        });
                        drop(m);
                        world.check(&pass, "after abandoned nested");
                    }
                    Op::Nro(i, v) => {
                        if gwrote[i] || happlied[i] {
                            continue;
                        }
                        let ro = Snapshot::take_snapshot(None);
                        ro.enter(|| world.check(&pass, "nested read-only"));
                        ros.push((ro, pass));
                        if pass[i] != v {
                            wrote[i] = true;
                        }
                        pass[i] = v;
                        world.states[i].set(v);
                        for (ro, saw) in &ros {
                            ro.enter(|| world.check(saw, "nested read-only later"));
                        }
                        ros.truncate(1);
                    }
                    Op::HR => {
                        for h in helds.borrow().iter() {
                            match h {
                                Held::Ro(s, v) => {
                                    s.enter(|| world.check(v, "held read-only in pass"))
                                }
                                Held::M(s, v, _, _) => {
                                    s.enter(|| world.check(v, "held mutable in pass"))
                                }
                            }
                        }
                        world.check(&pass, "after held in pass");
                    }
                    Op::HA => {
                        let mut hs = helds.borrow_mut();
                        let Some(at) = hs.iter().position(|h| matches!(h, Held::M(..))) else {
                            continue;
                        };
                        let Held::M(_, _, _, w) = &hs[at] else { continue };
                        if (0..N).any(|i| w[i] && (wrote[i] || gwrote[i])) {
                            continue;
                        }
                        let Held::M(m, v, touched, w) = hs.remove(at) else { unreachable!() };
                        let mut g = global.borrow_mut();
                        let conflict = (0..N).any(|i| w[i] && touched[i] && g[i] != v[i]);
                        let ok = m.apply().succeeded();
                        assert_eq!(ok, !conflict, "in-pass apply {w:?} {touched:?} {v:?} {g:?}");
                        if ok {
                            for i in 0..N {
                                if w[i] && !(touched[i] && g[i] == v[i]) {
                                    g[i] = v[i];
                                    happlied[i] = true;
                                    for h in hs.iter_mut() {
                                        if let Held::M(_, _, t, _) = h {
                                            t[i] = true;
                                        }
                                    }
                                }
                            }
                        }
                        drop(g);
                        drop(hs);
                        drop(m);
                        world.check(&pass, "after apply in pass");
                    }
                    Op::Adv => Snapshot::notify_objects_initialized(),
                    Op::Send => Snapshot::send_apply_notifications(),
                    Op::Panic => {
                        // What it wrote applies all the same, as Compose's `finally`.
                        *written.borrow_mut() = (pass, wrote);
                        panic!("fuzz panic");
                    }
                }
            }
            drop(ros);
            *written.borrow_mut() = (pass, wrote);
            let last: V = std::array::from_fn(|i| world.states[i].get());
            *writer_seen.borrow_mut() = last;
        });
        c
    };
    let mut writer = make_writer();

    // Readers.
    let seen_states: Rc<RefCell<V>> = Rc::default();
    let reader_states = {
        let (states, seen) = (states.clone(), seen_states.clone());
        let c = Composition::new(NodeTree::<()>::new());
        c.set_content(move || {
            for (i, state) in states.iter().enumerate() {
                let v = state.get();
                seen.borrow_mut()[i] = v;
            }
        });
        c
    };
    let seen_d: Vec<Rc<Cell<i32>>> = (0..3).map(|_| Rc::default()).collect();
    let reader_d: Vec<Composition> = (0..3)
        .map(|k| {
            let (d, seen) = (ds[k], seen_d[k].clone());
            let c = Composition::new(NodeTree::<()>::new());
            c.set_content(move || seen.set(d.get()));
            c
        })
        .collect();
    let told: Rc<Cell<u32>> = Rc::default();
    let t = told.clone();
    let observer = platform::ReadObserver::new(move || t.set(t.get() + 1));
    let last: Vec<Cell<i32>> =
        (0..3).map(|k| Cell::new(observer.observe(k as u64, || ds[k].get()))).collect();

    for number in 1..80 {
        // Between passes.
        for _ in 0..random.next(4) {
            let op = random.next(9);
            match op {
                0 | 1 => {
                    let (i, v) = (random.next(N), random.next(5) as i32);
                    let mut g = global.borrow_mut();
                    if g[i] != v {
                        for h in helds.borrow_mut().iter_mut() {
                            if let Held::M(_, _, t, _) = h {
                                t[i] = true;
                            }
                        }
                    }
                    g[i] = v;
                    drop(g);
                    states[i].set(v);
                }
                2 => Snapshot::send_apply_notifications(),
                3 if helds.borrow().len() < 3 => {
                    let s = Snapshot::take_snapshot(None);
                    helds.borrow_mut().push(Held::Ro(s, *global.borrow()));
                }
                4 if helds.borrow().len() < 3 => {
                    let m = Snapshot::take_mutable_snapshot(None, None);
                    let mut v = *global.borrow();
                    let mut w = [false; N];
                    for _ in 0..random.next(3) {
                        let (i, x) = (random.next(N), random.next(5) as i32);
                        if v[i] != x {
                            w[i] = true;
                        }
                        v[i] = x;
                        m.enter(|| states[i].set(x));
                    }
                    m.enter(|| world.check(&v, "held mutable, made"));
                    helds.borrow_mut().push(Held::M(m, v, [false; N], w));
                }
                5 => {
                    let mut hs = helds.borrow_mut();
                    if !hs.is_empty() {
                        let at = random.next(hs.len());
                        drop(hs.remove(at));
                    }
                }
                6 => {
                    let mut hs = helds.borrow_mut();
                    if let Some(at) = hs.iter().position(|h| matches!(h, Held::M(..))) {
                        let Held::M(m, v, touched, w) = hs.remove(at) else { unreachable!() };
                        let mut g = global.borrow_mut();
                        let conflict = (0..N).any(|i| w[i] && touched[i] && g[i] != v[i]);
                        let ok = m.apply().succeeded();
                        assert_eq!(ok, !conflict, "seed {seed} apply");
                        if ok {
                            for i in 0..N {
                                if w[i] && !(touched[i] && g[i] == v[i]) {
                                    g[i] = v[i];
                                    for h in hs.iter_mut() {
                                        if let Held::M(_, _, t, _) = h {
                                            t[i] = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    for h in helds.borrow().iter() {
                        match h {
                            Held::Ro(s, v) => s.enter(|| world.check(v, "held read-only")),
                            Held::M(s, v, _, _) => s.enter(|| world.check(v, "held mutable")),
                        }
                    }
                }
            }
        }
        world.check(&global.borrow().clone(), "between");
        // A pass.
        let mut ops = Vec::new();
        for _ in 0..random.next(7) {
            let (i, v) = (random.next(N), random.next(5) as i32);
            ops.push(match random.next(16) {
                0..=4 => Op::W(i, v),
                5 | 6 => Op::R,
                7 => Op::GR,
                8 if global_writes => Op::GW(i, v),
                9 => Op::NW(i, v),
                10 => Op::NA(i, v),
                11 => Op::Nro(i, v),
                12 => Op::HR,
                13 if held_apply => Op::HA,
                14 => Op::Adv,
                _ => Op::Send,
            });
        }
        let panicking = allow_panic && random.next(6) == 0;
        if panicking {
            ops.push(Op::Panic);
        }
        *script.borrow_mut() = ops.clone();
        round.set(number);
        *written.borrow_mut() = ([0; N], [false; N]);
        let result = catch_unwind(AssertUnwindSafe(|| {
            writer.recompose();
        }));
        match (&result, panicking) {
            (Ok(()), _) | (Err(_), true) => {
                let (pass, wrote) = *written.borrow();
                let mut g = global.borrow_mut();
                for i in 0..N {
                    if wrote[i] {
                        g[i] = pass[i];
                        for h in helds.borrow_mut().iter_mut() {
                            if let Held::M(_, _, t, _) = h {
                                t[i] = true;
                            }
                        }
                    }
                }
                // Its composition is poisoned: a new writer.
                if result.is_err() {
                    drop(g);
                    writer = make_writer();
                }
            }
            (Err(payload), false) => {
                let m = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                panic!("seed {seed} round {number}: {ops:?}: {m}");
            }
        }
        let g = *global.borrow();
        let ctx = format!("seed {seed} round {number} {ops:?} model {g:?}");
        world.check(&g, &ctx);
        for h in helds.borrow().iter() {
            match h {
                Held::Ro(s, v) => {
                    s.enter(|| world.check(v, &format!("after, held read-only: {ctx}")))
                }
                Held::M(s, v, _, _) => {
                    s.enter(|| world.check(v, &format!("after, held mutable: {ctx}")))
                }
            }
        }
        // Readers are told of whatever changed.
        Snapshot::send_apply_notifications();
        reader_states.recompose();
        assert_eq!(*seen_states.borrow(), g, "reader of states stale: {ctx}");
        writer.recompose();
        assert_eq!(*writer_seen.borrow(), g, "the writer's own read is stale: {ctx}");
        for k in 0..3 {
            reader_d[k].recompose();
            assert_eq!(
                seen_d[k].get(),
                [f0(&g), f1(&g), f2(&g)][k],
                "reader of derived {k} stale: {ctx}"
            );
        }
        let notified = told.replace(0);
        let mut invalid = Vec::new();
        observer.drain_invalidated(|k| invalid.push(k as usize));
        for k in 0..3 {
            let now = [f0(&g), f1(&g), f2(&g)][k];
            if now != last[k].get() {
                assert!(
                    invalid.contains(&k),
                    "observation of derived {k} stale ({} -> {now}), notified {notified}: {ctx}",
                    last[k].get()
                );
            }
        }
        for k in invalid {
            last[k].set(observer.observe(k as u64, || ds[k].get()));
        }
        let open = helds.borrow().len() + 1;
        if open == 1 {
            assert_eq!(Snapshot::open_snapshot_count(), 1, "{ctx}");
        }
        assert!(!Snapshot::is_in_snapshot());
        if open == 1 {
            Snapshot::notify_objects_initialized();
            assert!(states.iter().all(|s| state_records(*s).0 == 1), "records left: {ctx}");
            assert!(snapshot_paths_are_plain(), "{ctx}");
        }
    }
    helds.borrow_mut().clear();
    drop(owned);
}

const SEEDS: u64 = 40;

#[test]
fn passes_agree_with_arrays() {
    sweep(0, false, false, false);
}

#[test]
fn passes_that_panic_agree_with_arrays() {
    sweep(1000, true, false, false);
}

#[test]
fn passes_with_global_writes_agree_with_arrays() {
    sweep(2000, false, true, false);
}

#[test]
fn passes_applying_held_snapshots_agree_with_arrays() {
    sweep(3000, false, false, true);
}

#[test]
fn passes_with_all_of_it_agree_with_arrays() {
    sweep(4000, true, true, true);
}

fn sweep(from: u64, panics: bool, global_writes: bool, held_apply: bool) {
    for seed in from..from + SEEDS {
        // A thread each: a failing one leaves its runtime as it is, and each starts clean.
        let run = std::thread::spawn(move || run(seed, panics, global_writes, held_apply));
        if let Err(payload) = run.join() {
            std::panic::resume_unwind(payload);
        }
    }
}
