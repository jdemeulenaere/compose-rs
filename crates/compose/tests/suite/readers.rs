//! State readers (docs/05-state.md): a write recomposes exactly the scopes whose latest run read
//! the state, once each, however many times they read it and however many other scopes read it
//! in between; and what a state keeps of its readers stays bounded by the scopes reading it.

#![allow(non_snake_case)]

use std::any::Any;
use std::cell::Cell;

use compose::runtime::platform::*;
use compose::runtime::*;

thread_local! {
    static OUTER_RUNS: Cell<u32> = const { Cell::new(0) };
    static INNER_RUNS: Cell<u32> = const { Cell::new(0) };
}

fn take_runs() -> (u32, u32) {
    (OUTER_RUNS.with(|runs| runs.replace(0)), INNER_RUNS.with(|runs| runs.replace(0)))
}

/// A reader of `shared`; it also reads `tick` when `ticking`.
#[composable]
fn Inner(shared: MutableState<u32>, tick: MutableState<u32>, ticking: bool) {
    INNER_RUNS.with(|runs| runs.set(runs.get() + 1));
    let _ = shared.get();
    if ticking {
        let _ = tick.get();
    }
}

/// Ten readers of `shared`: more than the latest readers a read searches.
#[composable]
fn Block(shared: MutableState<u32>, tick: MutableState<u32>, ticking: bool) {
    for index in 0..10 {
        key(index, move || Inner(shared, tick, ticking));
    }
}

/// Reads `shared` before, between and after `blocks` blocks of other readers, so its later reads
/// find its earlier one far behind the latest readers; and `tick`, to run alone.
#[composable]
fn Outer(shared: MutableState<u32>, tick: MutableState<u32>, blocks: MutableState<u32>) {
    OUTER_RUNS.with(|runs| runs.set(runs.get() + 1));
    let _ = tick.get();
    let _ = shared.get();
    for block in 0..blocks.get() {
        key(block, move || Block(shared, tick, false));
        let _ = shared.get();
    }
}

struct Fixture {
    composition: Composition,
    shared: OwnedState<u32>,
    tick: OwnedState<u32>,
    blocks: OwnedState<u32>,
}

fn outer(blocks: u32) -> Fixture {
    let fixture = Fixture {
        composition: Composition::new(NodeTree::<()>::new()),
        shared: mutable_state_of(0),
        tick: mutable_state_of(0),
        blocks: mutable_state_of(blocks),
    };
    let (shared, tick, blocks) =
        (fixture.shared.handle(), fixture.tick.handle(), fixture.blocks.handle());
    fixture.composition.set_content(move || Outer(shared, tick, blocks));
    take_runs();
    fixture
}

#[test]
fn a_scope_whose_read_is_far_behind_the_latest_readers_recomposes_once() {
    let fixture = outer(5);
    fixture.shared.update(|value| value + 1);
    assert!(fixture.composition.recompose());
    assert_eq!(take_runs(), (1, 50));
    // The outer scope runs alone: its reads are recorded again behind the blocks' 50.
    fixture.tick.update(|value| value + 1);
    assert!(fixture.composition.recompose());
    assert_eq!(take_runs(), (1, 0));
    fixture.shared.update(|value| value + 1);
    assert!(fixture.composition.recompose());
    assert_eq!(take_runs(), (1, 50));
    // Nothing is left invalid: one run each.
    assert!(!fixture.composition.recompose());
}

#[test]
fn readers_that_stop_reading_or_leave_are_not_recomposed_and_are_dropped() {
    let fixture = outer(5);
    assert!(state_reader_count(fixture.shared.handle()) >= 51);
    // The blocks leave: the next write recomposes the outer scope only, and forgets them.
    fixture.blocks.set(0);
    assert!(fixture.composition.recompose());
    take_runs();
    fixture.shared.update(|value| value + 1);
    assert!(fixture.composition.recompose());
    assert_eq!(take_runs(), (1, 0));
    assert_eq!(state_reader_count(fixture.shared.handle()), 1);

    // A scope that stops reading: not recomposed by the next write, which forgets it.
    let reading = mutable_state_of(true);
    let shared = mutable_state_of(0);
    let (reading_handle, shared_handle) = (reading.handle(), shared.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || {
        let reading = reading_handle;
        let shared = shared_handle;
        Conditional(reading, shared);
    });
    assert_eq!(state_reader_count(shared.handle()), 1);
    reading.set(false);
    assert!(composition.recompose());
    take_runs();
    shared.set(1);
    assert!(!composition.recompose());
    assert_eq!(take_runs(), (0, 0));
    assert_eq!(state_reader_count(shared.handle()), 0);
}

#[composable]
fn Conditional(reading: MutableState<bool>, shared: MutableState<u32>) {
    OUTER_RUNS.with(|runs| runs.set(runs.get() + 1));
    if reading.get() {
        let _ = shared.get();
    }
}

/// Room for at most four records per scope reading the state (and a few to start with).
fn assert_bounded(state: MutableState<u32>, readers: usize) {
    let (count, capacity) = (state_reader_count(state), state_reader_capacity(state));
    assert!(count <= capacity);
    assert!(
        capacity <= 4 * readers.max(2),
        "{count} records in room for {capacity}, {readers} readers"
    );
}

#[test]
fn a_state_read_in_a_loop_between_other_readers_keeps_bounded_records() {
    // 1,001 scopes read `shared`: the outer one 101 times a run, behind ten others each time.
    let fixture = outer(100);
    let shared = fixture.shared.handle();
    for round in 0..100 {
        fixture.tick.update(|value| value + 1);
        assert!(fixture.composition.recompose());
        if round % 10 == 0 {
            fixture.shared.update(|value| value + 1);
            assert!(fixture.composition.recompose());
        }
        assert_bounded(shared, 1001);
    }
    assert_eq!(take_runs(), (110, 10 * 1000));
}

#[test]
fn a_state_read_by_many_scopes_every_frame_keeps_bounded_records() {
    let shared = mutable_state_of(0);
    let tick = mutable_state_of(0);
    let (shared_handle, tick_handle) = (shared.handle(), tick.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    // 1,000 readers, all running again each frame, in the order they first read.
    composition.set_content(move || {
        for block in 0..100 {
            key(block, move || Block(shared_handle, tick_handle, true));
        }
    });
    take_runs();
    let compactions = reader_compaction_count();
    for _ in 0..100 {
        tick.update(|value| value + 1);
        assert!(composition.recompose());
        assert_bounded(shared.handle(), 1000);
    }
    assert_eq!(take_runs(), (0, 100 * 1000));
    // Each frame records 1,000 reads in each of the two states; a compaction, linear in the
    // records, comes after at least half the room's worth of them (at least 512): at most about
    // 4 compactions a frame, not one every few reads.
    let compactions = reader_compaction_count() - compactions;
    assert!(compactions <= 2 * 100 * 1000 / 512 + 10, "{compactions} compactions");
}

#[test]
fn readers_coming_and_going_every_frame_keep_bounded_records() {
    let shared = mutable_state_of(0);
    let first = mutable_state_of(0u32);
    let (shared_handle, first_handle) = (shared.handle(), first.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    // 100 blocks under keys that move on by ten each frame: 100 readers come, 100 go.
    composition.set_content(move || {
        let tick = first_handle;
        let first = tick.get();
        for block in first..first + 100 {
            key(block, move || Block(shared_handle, tick, false));
        }
    });
    for _ in 0..200 {
        first.update(|value| value + 10);
        assert!(composition.recompose());
        // The new readers read before the old ones leave, at the end of the pass.
        assert_bounded(shared.handle(), 1100);
    }
    take_runs();
    shared.update(|value| value + 1);
    assert!(composition.recompose());
    assert_eq!(take_runs(), (0, 1000));
}

/// A restart group written by hand whose body runs without `skipping()` (a slip of the
/// protocol): its scope's epoch never moves, so its reads of one run never go stale. Ten
/// readers it composes, which run again with it, come between its two reads of `shared`.
fn Sloppy(shared: MutableState<u32>, tick: MutableState<u32>) {
    let composer = current();
    composer.start_restart_group(key_of("readers::Sloppy"));
    composer.update_kept(|_: &(MutableState<u32>, MutableState<u32>)| false, || (shared, tick));
    OUTER_RUNS.with(|runs| runs.set(runs.get() + 1));
    let _ = tick.get();
    let _ = shared.get();
    Block(shared, tick, true);
    let _ = shared.get();
    composer.end_restart_group(|kept: &dyn Any| {
        let (shared, tick) =
            *kept.downcast_ref::<(MutableState<u32>, MutableState<u32>)>().unwrap();
        Sloppy(shared, tick)
    });
}

#[test]
fn a_scope_whose_epoch_never_moves_keeps_bounded_records() {
    let shared = mutable_state_of(0);
    let tick = mutable_state_of(0);
    let (shared_handle, tick_handle) = (shared.handle(), tick.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || Sloppy(shared_handle, tick_handle));
    take_runs();
    for _ in 0..1000 {
        tick.update(|value| value + 1);
        assert!(composition.recompose());
        assert_bounded(shared.handle(), 11);
    }
    assert_eq!(take_runs(), (1000, 10 * 1000));
    // Still exact: one run each.
    shared.update(|value| value + 1);
    assert!(composition.recompose());
    assert_eq!(take_runs(), (1, 10));
}

// ---- Random read patterns (from the review of D310) ----------------------------------------

const RANDOM_SCOPES: usize = 40;

thread_local! {
    static RANDOM_RUNS: std::cell::RefCell<Vec<u32>> =
        std::cell::RefCell::new(vec![0; RANDOM_SCOPES]);
    static RANDOM_PATTERNS: std::cell::RefCell<Vec<MutableState<u32>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[composable]
fn RandomRoot(order: MutableState<Vec<usize>>, shared: MutableState<u32>) {
    let patterns = RANDOM_PATTERNS.with(|patterns| patterns.borrow().clone());
    for index in order.get() {
        let pattern = patterns[index];
        key(index, move || Patterned(index, shared, pattern));
    }
}

#[composable]
fn RandomChild(shared: MutableState<u32>) {
    INNER_RUNS.with(|runs| runs.set(runs.get() + 1));
    let _ = shared.get();
}

/// Reads `shared` `pattern % 4` times; when `pattern & 4`, nine children reading it come after
/// each read, so the scope's next read finds its earlier one beyond the latest readers.
#[composable]
fn Patterned(index: usize, shared: MutableState<u32>, pattern: MutableState<u32>) {
    RANDOM_RUNS.with(|runs| runs.borrow_mut()[index] += 1);
    let pattern = pattern.get();
    for read in 0..pattern % 4 {
        let _ = shared.get();
        if pattern & 4 != 0 {
            for child in 0..9u32 {
                key((read, child), move || RandomChild(shared));
            }
        }
    }
}

/// xorshift64: a fixed sequence, so a failure reproduces.
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
fn random_read_patterns_recompose_exactly_the_current_readers() {
    let shared = mutable_state_of(0u32);
    let patterns: Vec<OwnedState<u32>> =
        (0..RANDOM_SCOPES).map(|_| mutable_state_of(0u32)).collect();
    let order = mutable_state_of((0..RANDOM_SCOPES).collect::<Vec<usize>>());
    let handles: Vec<MutableState<u32>> = patterns.iter().map(|state| state.handle()).collect();
    RANDOM_PATTERNS.with(|stored| *stored.borrow_mut() = handles);
    let (shared_handle, order_handle) = (shared.handle(), order.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || RandomRoot(order_handle, shared_handle));
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for round in 0..2000 {
        // A few new patterns, and sometimes two scopes swapped.
        for _ in 0..next(&mut seed) % 6 {
            let index = (next(&mut seed) % RANDOM_SCOPES as u64) as usize;
            patterns[index].set((next(&mut seed) % 16) as u32);
        }
        if next(&mut seed).is_multiple_of(7) {
            let mut current = order.get();
            let first = (next(&mut seed) % RANDOM_SCOPES as u64) as usize;
            let second = (next(&mut seed) % RANDOM_SCOPES as u64) as usize;
            current.swap(first, second);
            order.set(current);
        }
        composition.recompose();
        RANDOM_RUNS.with(|runs| runs.borrow_mut().fill(0));
        take_runs();

        shared.update(|value| value + 1);
        composition.recompose();
        let expected: Vec<u32> =
            patterns.iter().map(|pattern| u32::from(pattern.get() % 4 != 0)).collect();
        let expected_children: u32 = patterns
            .iter()
            .map(|pattern| {
                let pattern = pattern.get();
                if pattern & 4 != 0 { (pattern % 4) * 9 } else { 0 }
            })
            .sum();
        assert_eq!(RANDOM_RUNS.with(|runs| runs.borrow().clone()), expected, "round {round}");
        assert_eq!(take_runs().1, expected_children, "round {round}");
        assert!(!composition.recompose(), "round {round}: something left invalid");
        assert_bounded(shared.handle(), RANDOM_SCOPES * 28);
    }
}
