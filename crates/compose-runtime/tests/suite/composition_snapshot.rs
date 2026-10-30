//! Composition in a snapshot (docs/parity/design/snapshots.md §3.13, step S6): each pass composes
//! in a mutable snapshot it applies as composing ends, as `Recomposer.composing`.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::{self, NodeTree, state_records};
use compose_runtime::snapshots::{Snapshot, StateObjectId};
use compose_runtime::*;

fn composition() -> Composition {
    Composition::new(NodeTree::<()>::new())
}

fn panic_message(block: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(block)).expect_err("a panic");
    match (payload.downcast_ref::<String>(), payload.downcast_ref::<&str>()) {
        (Some(message), _) => message.clone(),
        (_, Some(message)) => message.to_string(),
        _ => String::new(),
    }
}

/// A pass composes in a snapshot of its own: code in it sees it, the global snapshot doesn't see
/// what it wrote until it applies, and what's left to the pass's end runs in the global snapshot.
#[test]
fn a_pass_composes_in_a_snapshot_of_its_own() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let composition = composition();
    composition.set_content(move || {
        assert!(Snapshot::is_in_snapshot());
        assert!(!Snapshot::current().read_only());
        assert_ne!(Snapshot::current(), Snapshot::global_snapshot());
        handle.set(1);
        assert_eq!(handle.get(), 1, "it sees what it wrote");
        assert_eq!(Snapshot::global(|| handle.get()), 0, "the global snapshot doesn't");
        let seen = seen.clone();
        SideEffect(move || {
            assert!(!Snapshot::is_in_snapshot());
            seen.borrow_mut().push(format!("effect read {}", handle.get()));
        });
    });
    assert_eq!(state.get(), 1);
    assert_eq!(*log.borrow(), ["effect read 1"]);
    assert_eq!(Snapshot::open_snapshot_count(), 1);
    assert_eq!(state_records(handle).0, 1);
}

/// A pass that panics discards what it wrote, and poisons its composition, as before.
#[test]
fn a_pass_that_panics_discards_what_it_wrote() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let composition = composition();
    let message = panic_message(|| {
        composition.set_content(move || {
            handle.set(1);
            panic!("in composition");
        })
    });
    assert_eq!(message, "in composition");
    assert_eq!(state.get(), 0);
    assert_eq!(Snapshot::open_snapshot_count(), 1);
    assert!(!Snapshot::is_in_snapshot());
    assert_eq!(state_records(handle).0, 1);
    state.set(2);
    assert_eq!(state.get(), 2);
}

/// Something writing what the pass wrote, outside it, fails the apply with Compose's message.
#[test]
fn a_write_outside_the_pass_to_what_it_wrote_fails_it() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let composition = composition();
    let message = panic_message(|| {
        composition.set_content(move || {
            handle.set(1);
            Snapshot::global(|| handle.set(2));
        })
    });
    assert!(message.starts_with("Unsupported concurrent change during composition."), "{message}");
    assert_eq!(state.get(), 2);
    assert_eq!(Snapshot::open_snapshot_count(), 1);
    assert!(catch_unwind(AssertUnwindSafe(|| composition.set_content(|| {}))).is_err());
}

/// Another composition's reader of what a pass wrote recomposes with the value applied.
#[test]
fn another_compositions_reader_recomposes_with_what_a_pass_wrote() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let trigger = mutable_state_of(0);
    let read_trigger = trigger.handle();
    let reader = composition();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    reader.set_content(move || log.set(handle.get()));
    assert_eq!(seen.get(), 0);
    let writer = composition();
    writer.set_content(move || {
        let value = read_trigger.get();
        handle.set(value);
    });
    trigger.set(5);
    assert!(writer.recompose());
    assert!(reader.has_invalidations());
    assert!(reader.recompose());
    assert_eq!(seen.get(), 5);
    assert!(!writer.has_invalidations());
}

/// Apply observers hear what a pass wrote as it ends, naming its snapshot, once; a state it made
/// and wrote isn't a change.
#[test]
fn apply_observers_hear_what_a_pass_wrote_as_it_ends() {
    let (written, unwritten) = (mutable_state_of(0), mutable_state_of(0));
    let (handle, unwritten_id) = (written.handle(), unwritten.state_object_id());
    let made: Rc<Cell<Option<StateObjectId>>> = Rc::default();
    let made_id = made.clone();
    type Heard = Vec<(Vec<StateObjectId>, bool)>;
    let heard: Rc<RefCell<Heard>> = Rc::default();
    let log = heard.clone();
    let _observer = Snapshot::register_apply_observer(move |changed, snapshot| {
        log.borrow_mut().push((changed.iter().collect(), *snapshot == Snapshot::global_snapshot()));
    });
    let composition = composition();
    composition.set_content(move || {
        handle.set(1);
        let own = remember(|| mutable_state_of(0));
        own.set(1);
        made_id.set(Some(own.state_object_id()));
    });
    let heard = heard.borrow();
    assert_eq!(heard.len(), 1);
    assert_eq!(heard[0], (vec![written.state_object_id()], false));
    assert!(!heard[0].0.contains(&unwritten_id));
    assert!(!heard[0].0.contains(&made.get().expect("made")));
}

/// A state made in a pass is readable outside it, and written with its value after it.
#[test]
fn a_state_made_in_a_pass_is_readable_everywhere() {
    let made: Rc<RefCell<Option<MutableState<i32>>>> = Rc::default();
    let log = made.clone();
    let composition = composition();
    composition.set_content(move || {
        let state = remember(|| mutable_state_of(1));
        state.set(2);
        *log.borrow_mut() = Some(state);
    });
    let state = made.borrow().expect("made");
    assert_eq!(state.get(), 2);
    assert_eq!(state_records(state).0, 1);
    let heard = Rc::new(Cell::new(0));
    let count = heard.clone();
    let id = state.state_object_id();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        if changed.contains(id) {
            count.set(count.get() + 1);
        }
    });
    state.set(3);
    Snapshot::send_apply_notifications();
    assert_eq!(heard.get(), 1, "its next write is a change");
}

/// A derived state in a pass reads what the pass wrote; its record is as good as the global
/// snapshot's, and what it reads reaches its reader.
#[test]
fn a_derived_state_in_a_pass_reads_what_the_pass_wrote() {
    let (input, trigger) = (mutable_state_of(1), mutable_state_of(0));
    let (handle, read_trigger) = (input.handle(), trigger.handle());
    let runs = Rc::new(Cell::new(0));
    let count = runs.clone();
    let derived = derived_state_of(move || {
        count.set(count.get() + 1);
        handle.get() * 10
    });
    let read = derived.as_state();
    let seen = Rc::new(Cell::new(0));
    let log = seen.clone();
    let composition = composition();
    composition.set_content(move || {
        let wanted = read_trigger.get();
        assert_eq!(read.get(), handle.get() * 10);
        if wanted != 0 {
            handle.set(wanted);
            assert_eq!(read.get(), wanted * 10);
            log.set(read.get());
        }
    });
    assert_eq!(runs.get(), 1);
    assert_eq!(read.get(), 10);
    assert_eq!(runs.get(), 1, "a pass's calculation stands for the global snapshot");
    trigger.set(2);
    composition.recompose();
    assert_eq!(seen.get(), 20);
    assert_eq!(input.get(), 2);
    assert_eq!(read.get(), 20);
    assert_eq!(Snapshot::open_snapshot_count(), 1);
    assert_eq!(state_records(input.handle()).0, 1);
}

/// A derived state computed in a pass keeps no more than one record and what it read registered
/// once, however many passes compute it.
#[test]
fn a_derived_state_computed_in_passes_doesnt_grow() {
    let input = mutable_state_of(0);
    let handle = input.handle();
    let derived = derived_state_of(move || handle.get() + 1);
    let read = derived.as_state();
    let composition = composition();
    composition.set_content(move || {
        read.get();
    });
    for value in 1..2000 {
        input.set(value);
        composition.recompose();
        assert_eq!(platform::state_records(input.handle()).0, 1);
    }
    assert!(platform::state_observed_read_count(handle) < 100);
    assert_eq!(read.get(), 2000);
}

/// What a calculation in a pass read reaches a layout observation of the derived state, as its
/// global record's would.
#[test]
fn a_pass_calculation_tells_an_observation_of_its_derived_state() {
    let (switch, left, right) = (mutable_state_of(true), mutable_state_of(1), mutable_state_of(2));
    let (read_switch, read_left, read_right) = (switch.handle(), left.handle(), right.handle());
    let derived =
        derived_state_of(
            move || {
                if read_switch.get() { read_left.get() } else { read_right.get() }
            },
        );
    let read = derived.as_state();
    let composition = composition();
    composition.set_content(move || {
        read.get();
    });
    let notified = Rc::new(Cell::new(0));
    let count = notified.clone();
    let observer = platform::ReadObserver::new(move || count.set(count.get() + 1));
    observer.observe(1, || read.get());
    switch.set(false);
    assert!(composition.recompose());
    assert_eq!(read.get(), 2);
    assert_eq!(notified.get(), 1);
    observer.drain_invalidated(|_| {});
    observer.observe(1, || read.get());
    right.set(3);
    Snapshot::send_apply_notifications();
    assert_eq!(notified.get(), 2, "what the pass's calculation read");
}

/// A mutable snapshot in a composable nests in the pass's: reads and writes count as composition's,
/// applied into the pass's, which applies them.
#[test]
fn a_mutable_snapshot_in_a_composable_nests_in_the_passs() {
    let (state, trigger) = (mutable_state_of(0), mutable_state_of(0));
    let (handle, read_trigger) = (state.handle(), trigger.handle());
    let heard = Rc::new(Cell::new(0));
    let count = heard.clone();
    let id = state.state_object_id();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        if changed.contains(id) {
            count.set(count.get() + 1);
        }
    });
    let compositions = Rc::new(Cell::new(0));
    let counted = compositions.clone();
    let composition = composition();
    composition.set_content(move || {
        counted.set(counted.get() + 1);
        let wanted = Snapshot::with_mutable_snapshot(|| read_trigger.get());
        if wanted != 0 {
            Snapshot::with_mutable_snapshot(|| handle.set(wanted));
            assert_eq!(handle.get(), wanted);
            assert_eq!(Snapshot::global(|| handle.get()), 0);
        }
    });
    trigger.set(3);
    assert!(composition.recompose(), "a read in the nested snapshot registered");
    assert_eq!(compositions.get(), 2);
    assert_eq!(state.get(), 3);
    assert_eq!(heard.get(), 1);
}

/// A snapshot taken outside the pass and entered in it hides what's read through it from
/// composition, as its observers aren't composition's (D771); what's written stays in it.
#[test]
fn a_snapshot_taken_outside_a_pass_is_its_own() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let outside = Rc::new(Snapshot::take_mutable_snapshot(None, None));
    let inside = outside.clone();
    let compositions = Rc::new(Cell::new(0));
    let counted = compositions.clone();
    let composition = composition();
    composition.set_content(move || {
        counted.set(counted.get() + 1);
        inside.enter(|| {
            handle.get();
            handle.set(1);
        });
        assert_eq!(Snapshot::without_read_observation(|| handle.get()), 0);
    });
    assert_eq!(state.get(), 0);
    state.set(5);
    assert!(!composition.recompose());
    assert_eq!(compositions.get(), 1);
    assert!(!outside.apply().succeeded(), "the global write conflicts with its write");
}

/// A state made in `Snapshot::global` during a pass isn't readable in it, as Compose's.
#[test]
fn a_state_made_in_the_global_snapshot_during_a_pass_isnt_readable_in_it() {
    let composition = composition();
    let message = panic_message(|| {
        composition.set_content(|| {
            let state = Snapshot::global(|| mutable_state_of(0));
            state.get();
        })
    });
    assert!(message.starts_with("Reading a state that was created after"), "{message}");
}

/// A composition recomposing in a mutable snapshot nests in it: what it wrote is that snapshot's;
/// in a read-only one it panics, as Compose's.
#[test]
fn a_pass_takes_its_snapshot_of_the_current_one() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let first = composition();
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.enter(|| {
        first.set_content(move || handle.set(1));
        assert_eq!(handle.get(), 1);
        assert_eq!(snapshot, Snapshot::current());
    });
    assert_eq!(state.get(), 0);
    snapshot.apply().check();
    assert_eq!(state.get(), 1);

    let read_only = Snapshot::take_snapshot(None);
    let other = composition();
    let message = panic_message(|| read_only.enter(|| other.set_content(|| {})));
    assert_eq!(message, "Cannot create a mutable snapshot of a read-only snapshot");
}

/// What observes around a pass goes on hearing its reads and writes: it has no frame of its own.
#[test]
fn what_observes_around_a_pass_hears_it() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let (reads, writes) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (counted_reads, counted_writes) = (reads.clone(), writes.clone());
    let composition = composition();
    Snapshot::observe(
        Some(Rc::new(move |_| counted_reads.set(counted_reads.get() + 1))),
        Some(Rc::new(move |_| counted_writes.set(counted_writes.get() + 1))),
        || {
            composition.set_content(move || {
                handle.get();
                handle.set(1);
            });
        },
    );
    assert_eq!(reads.get(), 1);
    assert_eq!(writes.get(), 1);
}

/// A panicking apply observer doesn't lose the pass's end-of-pass work, and doesn't poison.
#[test]
fn a_panicking_apply_observer_leaves_the_passs_work_to_run() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let ran = Rc::new(Cell::new(false));
    let log = ran.clone();
    let observer = Snapshot::register_apply_observer(|_, _| panic!("in the observer"));
    let composition = composition();
    let message = panic_message(|| {
        composition.set_content(move || {
            handle.set(1);
            let log = log.clone();
            SideEffect(move || log.set(true));
        })
    });
    assert_eq!(message, "in the observer");
    assert!(ran.get());
    drop(observer);
    assert_eq!(state.get(), 1);
    composition.set_content(|| {});
    assert_eq!(Snapshot::open_snapshot_count(), 1);
}

/// What a pass writes, by seeded rounds: in it, its writes are seen and a derived state goes by
/// them, the global snapshot and a snapshot held open go on seeing what they saw; after it, what it
/// wrote is everybody's, each state has one record, and the thread is back on its plain paths.
#[test]
fn passes_stay_isolated_whatever_they_write() {
    struct Random(u64);
    impl Random {
        fn next(&mut self, bound: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) % bound as u64) as usize
        }
    }
    for seed in 0..40 {
        let mut random = Random(seed);
        let states: Vec<_> = (0..4).map(|_| mutable_state_of(0i32)).collect();
        let handles: Vec<_> = states.iter().map(|state| state.handle()).collect();
        let summed = handles.clone();
        let derived = derived_state_of(move || summed.iter().map(|state| state.get()).sum::<i32>());
        let read = derived.as_state();
        let writes: Rc<RefCell<Vec<(usize, i32)>>> = Rc::default();
        let round = mutable_state_of(0);
        let read_round = round.handle();
        let script = writes.clone();
        let in_pass = handles.clone();
        let composition = composition();
        composition.set_content(move || {
            read_round.get();
            let before: Vec<i32> =
                in_pass.iter().map(|state| Snapshot::global(|| state.get())).collect();
            let mut expected = before.clone();
            for &(index, value) in script.borrow().iter() {
                in_pass[index].set(value);
                expected[index] = value;
                assert_eq!(in_pass[index].get(), value);
                assert_eq!(read.get(), expected.iter().sum::<i32>());
                assert_eq!(Snapshot::global(|| in_pass[index].get()), before[index]);
            }
        });
        let mut model = [0i32; 4];
        for number in 1..60 {
            let held = (random.next(3) == 0).then(|| Snapshot::take_snapshot(None));
            let held_saw = model;
            let mut pass_writes = Vec::new();
            for _ in 0..random.next(4) {
                let (index, value) = (random.next(4), random.next(5) as i32);
                pass_writes.push((index, value));
                model[index] = value;
            }
            *writes.borrow_mut() = pass_writes;
            round.set(number);
            composition.recompose();
            for (index, state) in handles.iter().enumerate() {
                assert_eq!(state.get(), model[index], "seed {seed}, round {number}");
                if let Some(held) = &held {
                    assert_eq!(held.enter(|| state.get()), held_saw[index]);
                }
            }
            assert_eq!(read.get(), model.iter().sum::<i32>());
            drop(held);
            assert_eq!(Snapshot::open_snapshot_count(), 1);
            Snapshot::notify_objects_initialized();
            assert!(handles.iter().all(|state| state_records(*state).0 == 1));
        }
    }
}
