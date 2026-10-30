//! Composition in a snapshot (docs/parity/design/snapshots.md §3.13, step S6): each pass composes
//! in a mutable snapshot it applies as composing ends, as `Recomposer.composing`.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::{self, NodeTree, state_records};
use compose_runtime::snapshots::{self, Snapshot, StateObjectId};
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

// Compose: compose/runtime/runtime/src/commonMain/kotlin/androidx/compose/runtime/Recomposer.kt, composing: `finally { applyAndCheck(snapshot) }`
/// A composable that panics applies what it wrote all the same, and poisons its composition, as
/// before: its effects don't run.
#[test]
fn a_pass_that_panics_applies_what_it_wrote() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let effect = Rc::new(Cell::new(false));
    let log = effect.clone();
    let composition = composition();
    let message = panic_message(|| {
        composition.set_content(move || {
            handle.set(1);
            let log = log.clone();
            SideEffect(move || log.set(true));
            panic!("in composition");
        })
    });
    assert_eq!(message, "in composition");
    assert_eq!(state.get(), 1);
    assert!(!effect.get());
    assert_eq!(Snapshot::open_snapshot_count(), 1);
    assert!(!Snapshot::is_in_snapshot());
    assert_eq!(state_records(handle).0, 1);
    state.set(2);
    assert_eq!(state.get(), 2);
    assert!(platform::snapshot_paths_are_plain());
}

/// A panicking composable whose write conflicts: the first panic goes on, nothing applied.
#[test]
fn a_pass_that_panics_with_a_conflict_keeps_its_panic() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let composition = composition();
    let message = panic_message(|| {
        composition.set_content(move || {
            handle.set(1);
            Snapshot::global(|| handle.set(2));
            panic!("in composition");
        })
    });
    assert_eq!(message, "in composition");
    assert_eq!(state.get(), 2);
    assert_eq!(Snapshot::open_snapshot_count(), 1);
}

/// An apply observer panicking at the apply of a pass that panicked: no abort, the first panic goes
/// on, and an observer may recompose: the pass is over by then.
#[test]
fn what_runs_at_a_panicking_passs_apply_can_panic_or_recompose() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let other = Rc::new(composition());
    let recompose = other.clone();
    let observer = Snapshot::register_apply_observer(move |_, _| {
        recompose.set_content(|| {});
        panic!("in the observer");
    });
    let panicking = composition();
    let message = panic_message(|| {
        panicking.set_content(move || {
            handle.set(1);
            panic!("in composition");
        })
    });
    assert_eq!(message, "in composition");
    assert_eq!(state.get(), 1);
    drop(observer);
    assert_eq!(Snapshot::open_snapshot_count(), 1);
    assert!(!Snapshot::is_in_snapshot());
}

/// A `Drop` of a value a panicking pass replaced may recompose, the pass being over.
#[test]
fn a_value_dropped_at_a_panicking_passs_apply_may_recompose() {
    struct Recomposes(Rc<Composition>, Rc<Cell<bool>>);
    impl Drop for Recomposes {
        fn drop(&mut self) {
            if self.1.get() {
                self.0.set_content(|| {});
            }
        }
    }
    let (other, armed) = (Rc::new(composition()), Rc::new(Cell::new(false)));
    let state = mutable_state_of_with(
        Rc::new(Recomposes(other.clone(), armed.clone())),
        never_equal_policy(),
    );
    let handle = state.handle();
    let (other_again, armed_again) = (other.clone(), armed.clone());
    let panicking = composition();
    armed.set(true);
    let message = panic_message(|| {
        panicking.set_content(move || {
            handle.set(Rc::new(Recomposes(other_again.clone(), armed_again.clone())));
            panic!("in composition");
        })
    });
    assert_eq!(message, "in composition");
    armed.set(false);
    drop(state);
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
            let before = Snapshot::global(|| handle.get());
            Snapshot::with_mutable_snapshot(|| handle.set(wanted));
            assert_eq!(handle.get(), wanted);
            assert_eq!(Snapshot::global(|| handle.get()), before);
        }
    });
    trigger.set(3);
    assert!(composition.recompose(), "a read in the nested snapshot registered");
    assert_eq!(compositions.get(), 2);
    assert_eq!(state.get(), 3);
    assert_eq!(heard.get(), 1);
    // What it read after writing it hears the apply, as Compose's: a third, which writes nothing.
    assert!(composition.recompose());
    assert_eq!(compositions.get(), 3);
    assert!(!composition.has_invalidations());
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

// ---- After the review of S6: who hears what (D793) -----------------------------------------------

#[derive(Clone, Copy)]
struct Add;

impl MutationPolicy<i32> for Add {
    fn equivalent(a: &i32, b: &i32) -> bool {
        a == b
    }
    fn merge(previous: &i32, current: &i32, applied: &i32) -> Option<i32> {
        Some(current + applied - previous)
    }
}

/// A write outside the pass, during it: what read the state after it read the old value, and hears
/// at the send.
#[test]
fn a_reader_after_a_global_write_in_the_pass_recomposes() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let once = Rc::new(Cell::new(true));
    let composition = composition();
    composition.set_content(move || {
        if once.replace(false) {
            Snapshot::global(|| handle.set(10));
        }
        log.set(handle.get());
    });
    assert_eq!(state.get(), 10);
    assert_eq!(seen.get(), 0, "isolated from it");
    Snapshot::send_apply_notifications();
    assert!(composition.has_invalidations());
    composition.recompose();
    assert_eq!(seen.get(), 10);
}

/// The same with a send in the pass: readers later in it hear at its end.
#[test]
fn a_reader_after_a_send_in_the_pass_recomposes() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let once = Rc::new(Cell::new(true));
    let composition = composition();
    composition.set_content(move || {
        if once.replace(false) {
            Snapshot::global(|| handle.set(10));
            Snapshot::send_apply_notifications();
        }
        log.set(handle.get());
    });
    assert_eq!(seen.get(), 0);
    assert!(composition.has_invalidations());
    composition.recompose();
    assert_eq!(seen.get(), 10);
}

/// The same with a snapshot applied in the pass.
#[test]
fn a_reader_after_an_apply_in_the_pass_recomposes() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let outside = Rc::new(Snapshot::take_mutable_snapshot(None, None));
    outside.enter(|| handle.set(10));
    let inside = outside.clone();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let once = Rc::new(Cell::new(true));
    let composition = composition();
    composition.set_content(move || {
        if once.replace(false) {
            inside.apply().check();
        }
        log.set(handle.get());
    });
    assert_eq!(seen.get(), 0);
    assert!(composition.has_invalidations());
    composition.recompose();
    assert_eq!(seen.get(), 10);
}

/// A pass's write merged at its apply: its reader read the pass's value.
#[test]
fn a_merged_write_of_a_pass_tells_its_reader() {
    let state = mutable_state_of_with(0, Add);
    let handle = state.handle();
    let outside = Rc::new(Snapshot::take_mutable_snapshot(None, None));
    outside.enter(|| handle.set(10));
    let inside = outside.clone();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let once = Rc::new(Cell::new(true));
    let composition = composition();
    composition.set_content(move || {
        if once.replace(false) {
            inside.apply().check();
            handle.set(1);
        }
        log.set(handle.get());
    });
    assert_eq!(seen.get(), 1);
    assert_eq!(state.get(), 11, "merged");
    assert!(composition.has_invalidations());
    composition.recompose();
    assert_eq!(seen.get(), 11);
}

// Compose: compose/runtime/runtime/src/commonMain/kotlin/androidx/compose/runtime/Composition.kt, addPendingInvalidationsLocked: only `observationsProcessed` pairs are skipped
/// A reader that read the state after the write hears the apply, as Compose's: a second pass.
#[test]
fn a_new_reader_after_the_write_recomposes() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let composition = composition();
    composition.set_content(move || {
        counted.set(counted.get() + 1);
        handle.set(1);
        assert_eq!(handle.get(), 1);
    });
    assert_eq!(runs.get(), 1);
    assert!(composition.has_invalidations());
    assert!(composition.recompose());
    assert_eq!(runs.get(), 2);
    assert!(!composition.has_invalidations(), "an equal write writes nothing");
}

/// A nested snapshot taken before the write and read after it read the old value.
#[test]
fn a_reader_through_a_nested_snapshot_of_the_old_value_recomposes() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let once = Rc::new(Cell::new(true));
    let composition = composition();
    composition.set_content(move || {
        if once.replace(false) {
            let nested = Snapshot::take_snapshot(None);
            handle.set(1);
            log.set(nested.enter(|| handle.get()));
        } else {
            log.set(handle.get());
        }
    });
    assert_eq!(seen.get(), 0);
    assert_eq!(state.get(), 1);
    assert!(composition.has_invalidations());
}

/// A scope ahead of the write reads the new value in this pass: the apply skips it, once. A later
/// outside write is heard as any.
#[test]
fn a_scope_invalidated_imminently_is_skipped_at_the_apply_once() {
    #[composable]
    fn Reader(state: MutableState<i32>, runs: Rc<Cell<i32>>) {
        runs.set(runs.get() + 1);
        state.get();
    }
    #[composable]
    fn Content(write: MutableState<i32>, state: MutableState<i32>, runs: Rc<Cell<i32>>) {
        state.set(write.get());
        Reader(state, runs);
    }
    let (write, state) = (mutable_state_of(0), mutable_state_of(0));
    let (write_handle, state_handle) = (write.handle(), state.handle());
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let composition = composition();
    composition.set_content(move || Content(write_handle, state_handle, counted.clone()));
    assert_eq!(runs.get(), 1);
    write.set(5);
    assert!(composition.recompose());
    assert_eq!(runs.get(), 2, "the reader, ahead, ran in the pass");
    assert!(!composition.has_invalidations(), "and not again");
    state.set(6);
    assert!(composition.recompose());
    assert_eq!(runs.get(), 3);
}

/// So with a write in a snapshot nested in the pass: composition's as well, for a reader ahead.
#[test]
fn a_nested_snapshots_write_invalidates_a_scope_ahead_in_the_pass() {
    #[composable]
    fn Reader(state: MutableState<i32>, runs: Rc<Cell<i32>>) {
        runs.set(runs.get() + 1);
        state.get();
    }
    #[composable]
    fn Content(write: MutableState<i32>, state: MutableState<i32>, runs: Rc<Cell<i32>>) {
        let value = write.get();
        Snapshot::with_mutable_snapshot(|| state.set(value));
        Reader(state, runs);
    }
    let (write, state) = (mutable_state_of(0), mutable_state_of(0));
    let (write_handle, state_handle) = (write.handle(), state.handle());
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let composition = composition();
    composition.set_content(move || Content(write_handle, state_handle, counted.clone()));
    write.set(5);
    assert!(composition.recompose());
    assert_eq!(runs.get(), 2, "the reader, ahead, ran in the pass");
    assert!(!composition.has_invalidations(), "and not again");
}

/// A reader ahead that read a pass's write merged at its apply read what's no longer the value: it
/// hears, by the walk of what the write was merged with.
#[test]
fn a_merged_write_tells_a_reader_invalidated_for_the_pass() {
    #[composable]
    fn Reader(state: MutableState<i32>, seen: Rc<Cell<i32>>) {
        seen.set(state.get());
    }
    #[composable]
    fn Content(
        write: MutableState<i32>,
        state: MutableState<i32>,
        outside: Rc<RefCell<Option<Snapshot>>>,
        seen: Rc<Cell<i32>>,
    ) {
        let value = write.get();
        if let Some(outside) = outside.borrow_mut().take() {
            outside.as_mutable().expect("mutable").apply().check();
        }
        state.set(value);
        Reader(state, seen);
    }
    let (write, state) = (mutable_state_of(0), mutable_state_of_with(0, Add));
    let (write_handle, state_handle) = (write.handle(), state.handle());
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let outside: Rc<RefCell<Option<Snapshot>>> = Rc::default();
    let given = outside.clone();
    let composition = composition();
    composition
        .set_content(move || Content(write_handle, state_handle, given.clone(), log.clone()));
    assert_eq!(seen.get(), 0);
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.enter(|| state.set(10));
    *outside.borrow_mut() = Some(snapshot.handle());
    write.set(1);
    assert!(composition.recompose());
    assert_eq!(seen.get(), 1, "the pass's value");
    assert_eq!(state.get(), 11, "merged");
    assert!(composition.has_invalidations());
    composition.recompose();
    assert_eq!(seen.get(), 11);
    drop(snapshot);
}

/// A write found equivalent at its apply leaves no pair behind: a later apply is heard.
#[test]
fn an_equivalent_write_leaves_its_reader_to_hear_the_next_apply() {
    #[composable]
    fn Reader(state: MutableState<i32>, seen: Rc<Cell<i32>>) {
        seen.set(state.get());
    }
    #[composable]
    fn Content(write: MutableState<i32>, state: MutableState<i32>, seen: Rc<Cell<i32>>) {
        let value = write.get();
        if value != 0 {
            Snapshot::global(|| state.set(value));
        }
        state.set(value);
        Reader(state, seen);
    }
    let (write, state) = (mutable_state_of(0), mutable_state_of(0));
    let (write_handle, state_handle) = (write.handle(), state.handle());
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let composition = composition();
    composition.set_content(move || Content(write_handle, state_handle, log.clone()));
    write.set(1);
    assert!(composition.recompose());
    assert_eq!(state.get(), 1);
    composition.recompose();
    assert_eq!(seen.get(), 1);
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.enter(|| state.set(9));
    snapshot.apply().check();
    assert!(composition.has_invalidations(), "no pair of the pass's skips this");
    composition.recompose();
    assert_eq!(seen.get(), 9);
}

/// Another composition's reader, and an observation, hear at the apply, not at the write.
#[test]
fn what_isnt_the_compositions_hears_at_the_apply() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let reader = Rc::new(composition());
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    reader.set_content(move || log.set(handle.get()));
    let told = Rc::new(Cell::new(0));
    let count = told.clone();
    let observer = platform::ReadObserver::new(move || count.set(count.get() + 1));
    observer.observe(1, || state.get());
    let observer = Rc::new(observer);
    let during = Rc::new(Cell::new((true, -1, 0)));
    let (d, r, c, o) = (during.clone(), reader.clone(), told.clone(), observer.clone());
    let writer = composition();
    writer.set_content(move || {
        handle.set(1);
        let mut keys = 0;
        o.drain_invalidated(|_| keys += 1);
        d.set((r.has_invalidations(), c.get(), keys));
    });
    assert_eq!(during.get(), (false, 0, 0));
    assert!(reader.has_invalidations());
    assert_eq!(told.get(), 1);
    let mut keys = 0;
    observer.drain_invalidated(|_| keys += 1);
    assert_eq!(keys, 1);
}

// ---- After the review of S6: a pass in a snapshot, derived states ------------------------------

/// A pass nested in a user's mutable snapshot doesn't count as the global one: what a derived state
/// reads there goes with that snapshot.
#[test]
fn derived_in_a_pass_nested_in_a_discarded_snapshot() {
    let (switch, a, b) = (mutable_state_of(true), mutable_state_of(1), mutable_state_of(1));
    let (read_switch, read_a, read_b) = (switch.handle(), a.handle(), b.handle());
    let derived =
        derived_state_of(move || if read_switch.get() { read_a.get() } else { read_b.get() });
    let read = derived.as_state();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let reader = composition();
    reader.set_content(move || log.set(read.get()));
    assert_eq!(seen.get(), 1);
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    let other = composition();
    snapshot.enter(|| {
        switch.set(false);
        other.set_content(move || {
            read.get();
        });
    });
    snapshot.dispose();
    drop(snapshot);
    a.set(99);
    Snapshot::send_apply_notifications();
    assert!(reader.has_invalidations(), "the global snapshot's derived state reads `a`");
    reader.recompose();
    assert_eq!(seen.get(), 99);
}

/// A state made after a snapshot a pass nests in isn't readable in it, as Compose's.
#[test]
fn a_state_made_after_the_snapshot_a_pass_nests_in_isnt_readable() {
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    let late = mutable_state_of(1);
    let handle = late.handle();
    let composition = composition();
    let message = panic_message(|| {
        snapshot.enter(|| {
            composition.set_content(move || {
                handle.get();
            })
        })
    });
    assert!(message.starts_with("Reading a state that was created after"), "{message}");
}

/// When the value a derived state's readers saw isn't known any more, they're told.
#[test]
fn derived_global_saw_lost_after_a_discard() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get() * 10);
    let read = derived.as_state();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let reader = composition();
    reader.set_content(move || log.set(read.get()));
    assert_eq!(seen.get(), 10);
    let first = Snapshot::take_mutable_snapshot(None, None);
    first.enter(|| {
        handle.set(2);
        assert_eq!(read.get(), 20);
        assert_eq!(Snapshot::global(|| read.get()), 10);
        handle.set(3);
        assert_eq!(read.get(), 30);
    });
    first.dispose();
    drop(first);
    assert_eq!(read.get(), 10);
    reader.recompose();
    assert_eq!(seen.get(), 10);
    Snapshot::notify_objects_initialized();
    let writer = composition();
    writer.set_content(move || {
        let nested = Snapshot::take_mutable_snapshot(None, None);
        assert_eq!(
            nested.enter(|| {
                handle.set(7);
                read.get()
            }),
            70
        );
        drop(nested);
        handle.set(4);
        assert_eq!(read.get(), 40);
    });
    assert_eq!(read.get(), 40);
    reader.recompose();
    assert_eq!(seen.get(), 40, "the reader of the derived state is told");
}

/// A reader of a derived state, and a pass that's discarded by a conflict, after which the record of
/// what the global snapshot's readers last saw has gone: with that unknown, they're told.
type DiscardedPass = (OwnedState<i32>, DerivedState<i32>, State<i32>, Rc<Cell<i32>>, Composition);

fn derived_after_a_discarded_pass() -> DiscardedPass {
    let (state, conflicting) = (mutable_state_of(1), mutable_state_of(0));
    let (handle, conflict) = (state.handle(), conflicting.handle());
    let derived = derived_state_of(move || handle.get() * 10);
    let read = derived.as_state();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let reader = composition();
    reader.set_content(move || log.set(read.get()));
    assert_eq!(seen.get(), 10);
    let discarded = composition();
    let message = panic_message(|| {
        discarded.set_content(move || {
            handle.set(2);
            assert_eq!(read.get(), 20);
            assert_eq!(Snapshot::global(|| read.get()), 10);
            handle.set(3);
            assert_eq!(read.get(), 30);
            conflict.set(1);
            Snapshot::global(|| conflict.set(2));
        })
    });
    assert!(message.starts_with("Unsupported concurrent change"), "{message}");
    assert_eq!(state.get(), 1);
    assert_eq!(read.get(), 10);
    reader.recompose();
    assert_eq!(seen.get(), 10);
    Snapshot::notify_objects_initialized();
    (state, derived, read, seen, reader)
}

#[test]
fn derived_readers_are_told_after_a_discarded_pass_by_a_calculation() {
    let (state, _derived, read, seen, reader) = derived_after_a_discarded_pass();
    let handle = state.handle();
    let writer = composition();
    writer.set_content(move || {
        let nested = Snapshot::take_mutable_snapshot(None, None);
        assert_eq!(
            nested.enter(|| {
                handle.set(7);
                read.get()
            }),
            70
        );
        drop(nested);
        handle.set(4);
        assert_eq!(read.get(), 40);
    });
    assert_eq!(read.get(), 40);
    reader.recompose();
    assert_eq!(seen.get(), 40);
}

#[test]
fn derived_readers_are_told_after_a_discarded_pass_by_an_applied_record() {
    let (state, _derived, read, seen, reader) = derived_after_a_discarded_pass();
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.enter(|| {
        state.set(7);
        assert_eq!(read.get(), 70);
    });
    snapshot.apply().check();
    drop(snapshot);
    reader.recompose();
    assert_eq!(seen.get(), 70);
}

/// A recalculation in a pass stays readable to what validates it elsewhere: it keeps its record's
/// id, so `Snapshot::global` and a held snapshot read theirs, and each goes on.
#[test]
fn a_derived_state_read_in_and_around_a_pass() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get() * 10);
    let read = derived.as_state();
    let held = Rc::new(Snapshot::take_snapshot(None));
    let inside = held.clone();
    state.set(2);
    let composition = composition();
    composition.set_content(move || {
        assert_eq!(read.get(), 20);
        assert_eq!(inside.enter(|| read.get()), 10);
        assert_eq!(read.get(), 20);
        handle.set(3);
        assert_eq!(read.get(), 30);
        assert_eq!(inside.enter(|| read.get()), 10);
        assert_eq!(Snapshot::global(|| read.get()), 20);
        assert_eq!(read.get(), 30);
        let nested = Snapshot::take_snapshot(None);
        assert_eq!(nested.enter(|| read.get()), 30);
        handle.set(4);
        assert_eq!(nested.enter(|| read.get()), 30);
        assert_eq!(read.get(), 40);
        assert_eq!(
            Snapshot::with_mutable_snapshot(|| {
                handle.set(5);
                read.get()
            }),
            50
        );
        assert_eq!(read.get(), 50);
    });
    assert_eq!(read.get(), 50);
    assert_eq!(held.enter(|| read.get()), 10);
    held.dispose();
    drop(held);
    Snapshot::notify_objects_initialized();
    assert_eq!(read.get(), 50);
    assert_eq!(state_records(handle).0, 1);
}

/// A derived state with several records in a plain pass reads the one it sees, not the head.
#[test]
fn a_derived_state_with_several_records_in_a_pass_reads_its_own() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get() * 10);
    let read = derived.as_state();
    let held = Snapshot::take_mutable_snapshot(None, None);
    assert_eq!(held.enter(|| read.get()), 10);
    held.enter(|| handle.set(2));
    assert_eq!(held.enter(|| read.get()), 20, "a record of its own, beside the global one's");
    let composition = composition();
    composition.set_content(move || {
        assert_eq!(read.get(), 10);
    });
    assert_eq!(held.enter(|| read.get()), 20);
    assert_eq!(read.get(), 10);
}

// ---- After the review of S6: the object reused, and refusals -------------------------------------

/// A handle of a pass kept: the next pass doesn't reuse its object.
#[test]
fn a_kept_handle_of_a_pass_keeps_its_object() {
    let kept: Rc<RefCell<Vec<Snapshot>>> = Rc::default();
    let keep = kept.clone();
    let (state, trigger) = (mutable_state_of(0), mutable_state_of(0));
    let (handle, read_trigger) = (state.handle(), trigger.handle());
    let composition = composition();
    composition.set_content(move || {
        read_trigger.get();
        handle.set(1);
        keep.borrow_mut().push(Snapshot::current());
    });
    composition.recompose();
    let first = kept.borrow()[0].handle();
    let id = first.snapshot_id();
    trigger.set(1);
    composition.recompose();
    let second = kept.borrow().last().expect("kept").handle();
    assert!(first != second, "not the same object");
    assert_eq!(first.snapshot_id(), id);
    assert!(!first.has_pending_changes());
    kept.borrow_mut().clear();
    drop((first, second));
    trigger.set(2);
    composition.recompose();
    assert_eq!(Snapshot::open_snapshot_count(), 1);
}

/// The object a pass reuses starts clean: its previous ids, invalid set, write count and modified
/// list are the last pass's no more.
#[test]
fn a_reused_pass_snapshot_starts_clean() {
    let (state, trigger) = (mutable_state_of(0), mutable_state_of(0));
    let (handle, read_trigger) = (state.handle(), trigger.handle());
    let counts: Rc<RefCell<Vec<(u32, usize)>>> = Rc::default();
    let log = counts.clone();
    let composition = composition();
    composition.set_content(move || {
        let round = read_trigger.get();
        let current = Snapshot::current();
        log.borrow_mut().push((current.write_count(), current.modified().len()));
        if round % 2 == 0 {
            // Its id advances, a previous id to forget, and something to write.
            let nested = Snapshot::take_snapshot(None);
            handle.set(round + 1);
            drop(nested);
        }
    });
    for round in 1..6 {
        trigger.set(round);
        composition.recompose();
        assert_eq!(state.get(), if round % 2 == 0 { round + 1 } else { state.get() });
        // A held snapshot taken now sees what the pass applied: no id of the last pass's invalid.
        let held = Snapshot::take_snapshot(None);
        assert_eq!(held.enter(|| state.get()), state.get());
        drop(held);
    }
    assert!(counts.borrow().iter().all(|&count| count == (0, 0)), "{:?}", counts.borrow());
    // A snapshot open across passes: each pass's invalid set holds it, then doesn't.
    let held = Snapshot::take_mutable_snapshot(None, None);
    held.enter(|| handle.set(100));
    let seen: Rc<Cell<i32>> = Rc::default();
    let log = seen.clone();
    let reader = composition_reading(handle, log);
    assert_ne!(seen.get(), 100);
    held.apply().check();
    drop(held);
    state.get();
    trigger.set(10);
    composition.recompose();
    reader.recompose();
    assert_eq!(seen.get(), state.get());
}

fn composition_reading(state: MutableState<i32>, seen: Rc<Cell<i32>>) -> Composition {
    let reader = composition();
    reader.set_content(move || seen.set(state.get()));
    reader
}

/// A refused recompose keeps its invalidation.
#[test]
fn a_refused_recompose_keeps_the_invalidation() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let seen = Rc::new(Cell::new(-1));
    let log = seen.clone();
    let composition = composition();
    composition.set_content(move || log.set(handle.get()));
    state.set(1);
    Snapshot::send_apply_notifications();
    let read_only = Snapshot::take_snapshot(None);
    let message = panic_message(|| {
        read_only.enter(|| {
            composition.recompose();
        })
    });
    assert_eq!(message, "Cannot create a mutable snapshot of a read-only snapshot");
    drop(read_only);
    assert!(composition.has_invalidations());
    assert!(composition.recompose());
    assert_eq!(seen.get(), 1);
}

/// A policy's merge panicking at a pass's apply: nothing applied, and the pass's work doesn't run.
#[test]
fn a_merge_panicking_at_a_passs_apply_runs_no_work() {
    #[derive(Clone, Copy)]
    struct Panics;
    impl MutationPolicy<i32> for Panics {
        fn equivalent(a: &i32, b: &i32) -> bool {
            a == b
        }
        fn merge(_: &i32, _: &i32, _: &i32) -> Option<i32> {
            panic!("in the merge")
        }
    }
    let state = mutable_state_of_with(0, Panics);
    let handle = state.handle();
    let ran = Rc::new(Cell::new(false));
    let log = ran.clone();
    let composition = composition();
    let message = panic_message(|| {
        composition.set_content(move || {
            Snapshot::global(|| handle.set(10));
            handle.set(1);
            let log = log.clone();
            SideEffect(move || log.set(true));
        })
    });
    assert_eq!(message, "in the merge");
    assert_eq!(state.get(), 10);
    assert!(!ran.get());
}

/// A write through the global snapshot's handle, or a token, isn't the pass's: it conflicts.
#[test]
fn entering_the_global_snapshot_in_a_pass_counts_as_not_alone() {
    let state = mutable_state_of_with(0, Add);
    let handle = state.handle();
    let composition = composition();
    composition.set_content(move || {
        handle.set(1);
        let global = Snapshot::global_snapshot();
        let token = global.unsafe_enter();
        handle.set(10);
        global.unsafe_leave(token);
    });
    assert_eq!(state.get(), 11);
}

/// A snapshot applied in a pass, not entered: the pass's apply must look.
#[test]
fn a_snapshot_applied_in_a_pass_makes_its_apply_look() {
    let state = mutable_state_of_with(0, Add);
    let handle = state.handle();
    let outside = Rc::new(Snapshot::take_mutable_snapshot(None, None));
    outside.enter(|| handle.set(10));
    let inside = outside.clone();
    let composition = composition();
    composition.set_content(move || {
        handle.set(1);
        inside.apply().check();
    });
    assert_eq!(state.get(), 11);
}

/// A snapshot nested in the pass held past it: the pass applies by `apply`, with its object kept.
#[test]
fn a_nested_snapshot_held_past_the_pass() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let held: Rc<RefCell<Vec<Snapshot>>> = Rc::default();
    let keep = held.clone();
    let first = composition();
    first.set_content(move || {
        handle.set(1);
        keep.borrow_mut().push(Snapshot::take_snapshot(None));
        handle.set(2);
    });
    assert_eq!(state.get(), 2);
    assert_eq!(held.borrow()[0].enter(|| state.get()), 1, "as it was when taken");
    held.borrow_mut().clear();
    assert_eq!(Snapshot::open_snapshot_count(), 1);
    Snapshot::notify_objects_initialized();
    assert_eq!(state_records(handle).0, 1);

    let mutable: Rc<RefCell<Option<snapshots::MutableSnapshot>>> = Rc::default();
    let keep = mutable.clone();
    let second = composition();
    second.set_content(move || {
        let snapshot = Snapshot::take_mutable_snapshot(None, None);
        snapshot.enter(|| handle.set(5));
        *keep.borrow_mut() = Some(snapshot);
    });
    assert_eq!(state.get(), 2);
    let snapshot = mutable.borrow_mut().take().expect("kept");
    assert!(!snapshot.apply().succeeded(), "its parent is gone, as Compose's");
    drop(snapshot);
    assert_eq!(state.get(), 2);
    assert_eq!(Snapshot::open_snapshot_count(), 1);
    Snapshot::notify_objects_initialized();
    assert_eq!(state_records(handle).0, 1);
    assert!(!Snapshot::is_in_snapshot());
}

/// A calculation in a pass that made a state advances its snapshot, so the state's next write there
/// is a change; one that made none doesn't.
#[test]
fn a_calculation_in_a_pass_advances_it_only_if_it_made_a_state() {
    let made: Rc<RefCell<Option<MutableState<i32>>>> = Rc::default();
    let keep = made.clone();
    let owned: Rc<RefCell<Vec<OwnedState<i32>>>> = Rc::default();
    let hold = owned.clone();
    let derived = derived_state_of(move || {
        let state = mutable_state_of(1);
        *keep.borrow_mut() = Some(state.handle());
        hold.borrow_mut().push(state);
        1
    });
    let read = derived.as_state();
    let plain = derived_state_of(|| 2);
    let read_plain = plain.as_state();
    let ids: Rc<Cell<(u64, u64, u64)>> = Rc::default();
    let log = ids.clone();
    let wrote: Rc<Cell<bool>> = Rc::default();
    let was_modified = wrote.clone();
    let holder = made.clone();
    let composition = composition();
    composition.set_content(move || {
        let before = Snapshot::current().snapshot_id();
        read_plain.get();
        let after_plain = Snapshot::current().snapshot_id();
        read.get();
        let after_making = Snapshot::current().snapshot_id();
        log.set((before, after_plain, after_making));
        let state = holder.borrow().expect("made");
        state.set(2);
        was_modified.set(!Snapshot::current().modified().is_empty());
    });
    let (before, after_plain, after_making) = ids.get();
    assert_eq!(before, after_plain, "nothing made");
    assert_ne!(after_plain, after_making, "a state made");
    assert!(wrote.get(), "its write after the calculation is a change");
}

/// A panicking `Drop` in reclamation leaves the thread back on its plain paths in the end.
#[test]
fn a_panicking_drop_in_reclamation_leaves_nothing_behind() {
    #[derive(Clone)]
    struct Bomb(Rc<Cell<bool>>);
    impl Drop for Bomb {
        fn drop(&mut self) {
            if self.0.replace(false) {
                panic!("in a drop");
            }
        }
    }
    let armed = Rc::new(Cell::new(false));
    let state = mutable_state_of_with(Bomb(armed.clone()), never_equal_policy());
    let handle = state.handle();
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.enter(|| handle.set(Bomb(armed.clone())));
    armed.set(true);
    let message = panic_message(|| {
        let _ = snapshot.apply();
    });
    assert_eq!(message, "in a drop");
    drop(snapshot);
    Snapshot::notify_objects_initialized();
    assert_eq!(state_records(handle).0, 1);
    assert!(platform::snapshot_paths_are_plain());
}
