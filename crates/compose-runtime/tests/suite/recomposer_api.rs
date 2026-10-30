//! The `Recomposer`'s lifecycle and its loop, as Compose's `RecomposerTests` with a clock of the
//! executor's frames for `TestMonotonicFrameClock`, and its frame-end callbacks, as
//! `RecomposerFrameEndSchedulingTests`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_runtime::platform::*;
use compose_runtime::snapshots::Snapshot;
use compose_runtime::*;

use crate::mock::ViewApplier;

/// The executor's tasks and frames, until `done` or it's clear nothing's coming.
fn drive(done: impl Fn() -> bool) {
    for _ in 0..200 {
        run_ready();
        if done() {
            return;
        }
        if has_frame_awaiters() {
            advance_clock(Duration::from_millis(16));
            dispatch_frame();
        } else if !has_ready_tasks() {
            if let Some(next) = next_timer() {
                advance_clock(next);
                fire_timers();
            } else {
                run_ready();
                return;
            }
        }
    }
}

/// A task's flag, set when `future` ends.
fn finishes(future: impl Future<Output = ()> + 'static) -> Rc<Cell<bool>> {
    let done = Rc::new(Cell::new(false));
    let seen = done.clone();
    launch(async move {
        future.await;
        seen.set(true);
    });
    done
}

fn running(recomposer: Recomposer) -> Rc<Cell<bool>> {
    let flag = finishes(async move { recomposer.run_recompose_and_apply_changes().await });
    run_ready();
    flag
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, recomposerRecomposesWhileOpen
#[test]
fn a_recomposer_recomposes_while_it_s_open() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let runner = running(recomposer);
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let last = Rc::new(Cell::new(-1));
    let seen = last.clone();
    composition.set_content(move || seen.set(read.get()));
    assert_eq!(last.get(), 0, "the initial composition");
    Snapshot::with_mutable_snapshot(|| state.set(1));
    let idle = finishes(async move { recomposer.await_idle().await });
    drive(|| idle.get());
    assert!(idle.get(), "idle after the recomposition");
    assert_eq!(last.get(), 1, "recomposed");
    recomposer.close();
    let joined = finishes(async move { recomposer.join().await });
    drive(|| joined.get());
    assert!(joined.get(), "joined");
    assert!(runner.get(), "its loop returned");
    Snapshot::with_mutable_snapshot(|| state.set(2));
    drive(|| false);
    assert!(recomposer.current_state().get() <= RecomposerState::PendingWork);
    assert_eq!(last.get(), 1, "no recomposition by a closed recomposer");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, recomposerRemainsOpenUntilEffectsJoin
#[test]
fn a_closed_recomposer_stays_until_its_effects_end() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let runner = running(recomposer);
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let completer = BroadcastFrameClock::new();
    let waiting = completer.clone();
    composition.set_content(move || {
        let waiting = waiting.clone();
        LaunchedEffect((), async move {
            waiting.with_frame_nanos(|_| {}).await;
        });
    });
    run_ready();
    let idle = finishes(async move { recomposer.await_idle().await });
    drive(|| idle.get());
    assert!(idle.get());
    recomposer.close();
    let idle = finishes(async move { recomposer.await_idle().await });
    drive(|| idle.get());
    assert!(idle.get(), "shutting down is past idle");
    assert!(!runner.get(), "the loop goes on while the effect does");
    completer.send_frame(1);
    let joined = finishes(async move { recomposer.join().await });
    drive(|| joined.get());
    assert!(joined.get(), "joined");
    assert_eq!(recomposer.current_state().get(), RecomposerState::ShutDown);
    assert!(runner.get(), "its loop returned");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, constructRecomposerWithCancelledJob
#[test]
fn a_recomposer_cancelled_first_is_shut_down() {
    let owner = Recomposer::new();
    owner.cancel();
    assert_eq!(owner.current_state().get(), RecomposerState::ShutDown);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, cancelWithPendingInvalidations
#[test]
fn cancelling_with_invalidations_pending_ends_its_loop() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let launched = Rc::new(Cell::new(false));
    let seen = launched.clone();
    let runner = finishes(async move {
        seen.set(true);
        recomposer.run_recompose_and_apply_changes().await;
    });
    run_ready();
    let (one, two) = (
        Composition::with_parent(ViewApplier::new(), recomposer.as_context()),
        Composition::with_parent(ViewApplier::new(), recomposer.as_context()),
    );
    let state = mutable_state_of(0);
    let read = state.handle();
    let (last_one, last_two) = (Rc::new(Cell::new(-1)), Rc::new(Cell::new(-1)));
    for (composition, last) in [(&one, last_one.clone()), (&two, last_two.clone())] {
        composition.set_content(move || {
            last.set(read.get());
            LaunchedEffect((), async {
                delay(Duration::from_secs(1)).await;
            });
        });
    }
    assert_eq!((last_one.get(), last_two.get()), (0, 0));
    run_ready();
    let idle = finishes(async move { recomposer.await_idle().await });
    run_ready();
    assert!(idle.get());
    assert!(launched.get(), "it's running");
    Snapshot::with_mutable_snapshot(|| state.set(1));
    recomposer.cancel();
    let idle = finishes(async move { recomposer.await_idle().await });
    drive(|| idle.get() && runner.get());
    assert!(idle.get());
    assert!(runner.get(), "its loop returned");
    assert_eq!((last_one.get(), last_two.get()), (0, 0), "cancelled before its frame");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, pausingTheFrameClockStopShouldBlockWithFrameNanos
#[test]
fn pausing_the_clock_holds_back_effects_frames_not_composition() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let runner = running(recomposer);
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let (last_state, last_nanos) = (Rc::new(Cell::new(-1)), Rc::new(Cell::new(0u64)));
    let (seen_state, seen_nanos) = (last_state.clone(), last_nanos.clone());
    composition.set_content(move || {
        seen_state.set(read.get());
        let seen_nanos = seen_nanos.clone();
        LaunchedEffect((), async move {
            loop {
                let seen = seen_nanos.clone();
                with_frame_nanos(move |nanos| seen.set(nanos)).await;
            }
        });
    });
    // A few frames, the effect asking for each.
    let frames = |count: usize| {
        for _ in 0..count {
            run_ready();
            advance_clock(Duration::from_millis(16));
            dispatch_frame();
            run_ready();
        }
    };
    frames(3);
    assert_eq!(last_state.get(), 0);
    assert!(last_nanos.get() > 0, "the effect's first frames");
    let after_first = last_nanos.get();
    Snapshot::with_mutable_snapshot(|| state.set(1));
    frames(3);
    assert_eq!(last_state.get(), 1, "recomposed");
    assert!(last_nanos.get() > after_first);

    recomposer.pause_composition_frame_clock();
    let after_pause = last_nanos.get();
    Snapshot::with_mutable_snapshot(|| state.set(2));
    frames(3);
    assert_eq!(last_state.get(), 2, "composition goes on");
    assert_eq!(last_nanos.get(), after_pause, "no frame for the effect");
    Snapshot::with_mutable_snapshot(|| state.set(3));
    frames(3);
    assert_eq!(last_state.get(), 3);
    assert_eq!(last_nanos.get(), after_pause);

    recomposer.resume_composition_frame_clock();
    frames(3);
    assert!(last_nanos.get() > after_pause, "its frames again");
    let after_resume = last_nanos.get();
    Snapshot::with_mutable_snapshot(|| state.set(4));
    frames(3);
    assert_eq!(last_state.get(), 4);
    assert!(last_nanos.get() > after_resume);

    composition.dispose();
    recomposer.cancel();
    let idle = finishes(async move { recomposer.await_idle().await });
    drive(|| idle.get() && runner.get());
    assert!(idle.get());
    assert!(runner.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/RecomposerTests.jvm.kt, recomposerCancelReportsShuttingDownImmediately
#[test]
fn cancelling_reports_shutting_down_at_once() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let _runner = running(recomposer);
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    composition.set_content(|| {
        LaunchedEffect((), std::future::pending::<()>());
    });
    recomposer.cancel();
    assert!(recomposer.current_state().get() <= RecomposerState::ShuttingDown);
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/RecomposerTests.jvm.kt, disposedInvalidatedCompositionDoesNotLeak
#[test]
fn a_disposed_invalidated_composition_isn_t_recomposed() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let _runner = running(recomposer);
    let state = mutable_state_of(0);
    let read = state.handle();
    let runs = Rc::new(Cell::new(0));
    let seen = runs.clone();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    composition.set_content(move || {
        read.get();
        seen.set(seen.get() + 1);
    });
    assert_eq!(recomposer.current_state().get(), RecomposerState::Idle);
    Snapshot::with_mutable_snapshot(|| state.set(1));
    run_ready();
    assert!(has_frame_awaiters(), "a frame was asked for");
    assert_eq!(recomposer.current_state().get(), RecomposerState::PendingWork);
    composition.dispose();
    assert_eq!(recomposer.current_state().get(), RecomposerState::Idle);
    drive(|| false);
    assert_eq!(runs.get(), 1);
    drop(composition);
    assert!(!recomposer.has_pending_work());
    recomposer.cancel();
    run_ready();
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, runningRecomposerFlow
#[test]
fn running_recomposers_are_listed_while_they_run() {
    let info = Rc::new(Cell::new(None));
    let seen = info.clone();
    let job = launch(async move {
        with_running_recomposer(async |recomposer: Recomposer| {
            seen.set(Some(recomposer.as_recomposer_info()));
            std::future::pending::<()>().await;
        })
        .await;
    });
    run_ready();
    let info = info.get().expect("its block ran");
    assert!(running_recomposers().get().contains(&info));
    job.cancel();
    run_ready();
    assert!(!running_recomposers().get().contains(&info));
}

/// Its block's recomposer is running, and shut down once it ends.
#[test]
fn a_running_recomposer_ends_with_its_block() {
    set_manual_clock(0);
    let result = Rc::new(Cell::new(0));
    let seen = result.clone();
    let done = finishes(async move {
        let value = with_running_recomposer(async |recomposer: Recomposer| {
            assert_eq!(recomposer.current_state().get(), RecomposerState::Idle);
            let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
            let state = mutable_state_of(1);
            let read = state.handle();
            let last = Rc::new(Cell::new(0));
            let shown = last.clone();
            composition.set_content(move || shown.set(read.get()));
            state.set(2);
            Snapshot::send_apply_notifications();
            recomposer.await_idle().await;
            last.get()
        })
        .await;
        seen.set(value);
    });
    drive(|| done.get());
    assert!(done.get());
    assert_eq!(result.get(), 2);
}

/// A loop started twice, or on one shut down, is refused.
#[test]
#[should_panic(expected = "already running")]
fn a_recomposer_runs_one_loop() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let _first = running(recomposer);
    let task = recomposer.run_recompose_and_apply_changes();
    let mut task = std::pin::pin!(task);
    let waker = std::task::Waker::noop();
    let _ = task.as_mut().poll(&mut std::task::Context::from_waker(waker));
}

#[test]
#[should_panic(expected = "can't be cancelled or closed")]
fn the_thread_s_recomposer_isn_t_shut_down() {
    Recomposer::current_thread().cancel();
}

/// What's composed in a frame sees what another wrote in it; each composes once, and all apply
/// after: an effect runs once every composition has composed.
#[test]
fn a_frame_composes_them_all_and_applies_them_after() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let (first, second) = (
        Composition::with_parent(ViewApplier::new(), recomposer.as_context()),
        Composition::with_parent(ViewApplier::new(), recomposer.as_context()),
    );
    let (source, written) = (mutable_state_of(0), mutable_state_of(0));
    let (read_source, read_written) = (source.handle(), written.handle());
    let log = crate::mock::Log::new();
    let (writing, reading) = (log.clone(), log.clone());
    first.set_content(move || {
        let value = read_source.get();
        writing.push(format!("first composes {value}"));
        read_written.set(value);
        let writing = writing.clone();
        SideEffect(move || writing.push(format!("first applies {value}")));
    });
    second.set_content(move || {
        let value = read_written.get();
        reading.push(format!("second composes {value}"));
        let reading = reading.clone();
        SideEffect(move || reading.push(format!("second applies {value}")));
    });
    log.take();
    source.set(1);
    assert!(perform_frame(recomposer, 0));
    assert_eq!(
        log.take(),
        ["first composes 1", "second composes 1", "first applies 1", "second applies 1"]
    );
    assert_eq!(recomposer.change_count(), 1);
    assert!(!perform_frame(recomposer, 0));
    assert_eq!(recomposer.change_count(), 1);
}

/// A composition under the thread's default is that recomposer's, one under another isn't.
#[test]
fn a_frame_is_of_its_own_compositions() {
    let owner = Recomposer::new();
    let other = owner.handle();
    let (default, under) = (
        Composition::new(ViewApplier::new()),
        Composition::with_parent(ViewApplier::new(), other.as_context()),
    );
    assert_eq!(default.parent(), Recomposer::current_thread().as_context());
    assert_eq!(under.parent(), other.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let (default_runs, other_runs) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (a, b) = (default_runs.clone(), other_runs.clone());
    default.set_content(move || {
        read.get();
        a.set(a.get() + 1);
    });
    under.set_content(move || {
        read.get();
        b.set(b.get() + 1);
    });
    state.set(1);
    assert!(perform_frame(other, 0));
    assert_eq!((default_runs.get(), other_runs.get()), (1, 2));
    assert!(perform_frame(Recomposer::current_thread(), 0));
    assert_eq!((default_runs.get(), other_runs.get()), (2, 2));
    let third_owner = Recomposer::new();
    state.set(2);
    assert!(!perform_frame(third_owner.handle(), 0), "nothing of its own");
    assert_eq!((default_runs.get(), other_runs.get()), (2, 2));
}

/// A panic composing one poisons it; what's composed is still applied, and the panic comes out.
#[test]
fn a_panic_in_a_frame_leaves_the_others_applied() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let (first, second) = (
        Composition::with_parent(ViewApplier::new(), recomposer.as_context()),
        Composition::with_parent(ViewApplier::new(), recomposer.as_context()),
    );
    let state = mutable_state_of(0);
    let read = state.handle();
    let applied = Rc::new(Cell::new(0));
    let seen = applied.clone();
    first.set_content(move || {
        let value = read.get();
        let seen = seen.clone();
        SideEffect(move || seen.set(value));
    });
    second.set_content(move || {
        assert_eq!(read.get(), 0, "boom");
    });
    state.set(1);
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| perform_frame(recomposer, 0)));
    assert!(result.is_err());
    assert_eq!(applied.get(), 1, "the one composed was applied");
    state.set(2);
    assert!(perform_frame(recomposer, 0));
    assert_eq!(applied.get(), 2, "the other goes on");
    assert!(!perform_frame(recomposer, 0), "the poisoned one isn't composed again");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerFrameEndSchedulingTests.kt, scheduleFrameEnd_whileIdle_requestsNewFrameCallback
#[test]
fn a_frame_end_callback_while_idle_asks_for_a_frame() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    composition.set_content(|| {});
    assert!(!recomposer.has_pending_work());
    let resumed = Rc::new(Cell::new(false));
    let seen = resumed.clone();
    composition.parent().schedule_frame_end_callback(move || seen.set(true));
    assert!(recomposer.has_pending_work());
    assert!(!perform_frame(recomposer, 0), "nothing to compose");
    assert!(resumed.get());
    assert!(!recomposer.has_pending_work());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerFrameEndSchedulingTests.kt, scheduleFrameEnd_whileIdle_cancelFirst_requestsNewFrameCallback
#[test]
fn a_frame_end_callback_cancelled_doesn_t_run() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let log = crate::mock::Log::new();
    let (one, two) = (log.clone(), log.clone());
    let first = recomposer.schedule_frame_end_callback(move || one.push("Job 1"));
    assert!(recomposer.has_pending_work());
    recomposer.schedule_frame_end_callback(move || two.push("Job 2"));
    first.cancel();
    assert!(recomposer.has_pending_work());
    assert!(!perform_frame(recomposer, 0));
    assert_eq!(log.take(), ["Job 2"]);
}

/// A running loop takes one on its next frame.
#[test]
fn a_running_loop_runs_a_frame_end_callback() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let _runner = running(recomposer);
    let resumed = Rc::new(Cell::new(false));
    let seen = resumed.clone();
    recomposer.schedule_frame_end_callback(move || seen.set(true));
    drive(|| resumed.get());
    assert!(resumed.get());
    recomposer.cancel();
    run_ready();
}

/// A task of a composition's scope counts as an effect of its recomposer.
#[test]
fn a_remembered_scope_s_tasks_are_its_recomposer_s_effects() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let dropped = Rc::new(Cell::new(false));
    struct Noted(Rc<Cell<bool>>);
    impl Drop for Noted {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let seen = dropped.clone();
    composition.set_content(move || {
        let scope = remember_coroutine_scope();
        let noted = Noted(seen.clone());
        remember(move || {
            scope.launch(async move {
                let _noted = noted;
                std::future::pending::<()>().await;
            })
        });
    });
    run_ready();
    assert!(!dropped.get());
    recomposer.close();
    recomposer.cancel();
    run_ready();
    assert!(dropped.get());
    assert_eq!(recomposer.current_state().get(), RecomposerState::ShutDown);
}

/// An effect's frame is its recomposer's: what it writes then is composed in that frame, and while
/// it waits for one the recomposer has work.
#[test]
fn what_an_effect_writes_in_its_frame_is_composed_in_it() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let _runner = running(recomposer);
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let shown = Rc::new(Cell::new(-1));
    let seen = shown.clone();
    composition.set_content(move || {
        seen.set(read.get());
        LaunchedEffect((), async move {
            with_frame_nanos(move |_| read.set(1)).await;
        });
    });
    assert_eq!(shown.get(), 0);
    run_ready();
    assert!(recomposer.has_pending_work(), "the effect waits for a frame");
    assert_eq!(recomposer.current_state().get(), RecomposerState::PendingWork);
    let idle = finishes(async move { recomposer.await_idle().await });
    run_ready();
    assert!(!idle.get(), "not idle while it waits");
    advance_clock(Duration::from_millis(16));
    dispatch_frame();
    run_ready();
    assert_eq!(shown.get(), 1, "composed in the frame that wrote it");
    run_ready();
    assert!(idle.get());
    recomposer.cancel();
    run_ready();
}

/// Closed, it starts no more effects.
#[test]
fn a_closed_recomposer_starts_no_effect() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    recomposer.close();
    let started = Rc::new(Cell::new(false));
    let seen = started.clone();
    composition.set_content(move || {
        let seen = seen.clone();
        LaunchedEffect((), async move { seen.set(true) });
    });
    run_ready();
    assert!(!started.get());
    assert_eq!(recomposer.current_state().get(), RecomposerState::ShutDown);
}

/// An effect beginning to wait for a frame, nothing being invalid, gets its loop going.
#[test]
fn an_effect_waiting_for_a_frame_later_wakes_the_loop() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let _runner = running(recomposer);
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let shown = Rc::new(Cell::new(-1));
    let seen = shown.clone();
    composition.set_content(move || {
        seen.set(read.get());
        LaunchedEffect((), async move {
            delay(Duration::from_secs(1)).await;
            with_frame_nanos(move |_| read.set(1)).await;
        });
    });
    run_ready();
    assert_eq!(recomposer.current_state().get(), RecomposerState::Idle);
    advance_clock(Duration::from_secs(1));
    fire_timers();
    run_ready();
    assert_eq!(recomposer.current_state().get(), RecomposerState::PendingWork);
    dispatch_frame();
    run_ready();
    assert_eq!(shown.get(), 1, "the loop's frame was the effect's");
    recomposer.cancel();
    run_ready();
}

/// A panic applying one comes out, once the others are applied.
#[test]
fn a_panic_in_an_effect_leaves_the_others_applied() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let (first, second) = (
        Composition::with_parent(ViewApplier::new(), recomposer.as_context()),
        Composition::with_parent(ViewApplier::new(), recomposer.as_context()),
    );
    let state = mutable_state_of(0);
    let read = state.handle();
    first.set_content(move || {
        let value = read.get();
        SideEffect(move || assert_eq!(value, 0, "boom"));
    });
    let applied = Rc::new(Cell::new(0));
    let seen = applied.clone();
    second.set_content(move || {
        let value = read.get();
        let seen = seen.clone();
        SideEffect(move || seen.set(value));
    });
    state.set(1);
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| perform_frame(recomposer, 0)));
    assert!(result.is_err(), "it comes out");
    assert_eq!(applied.get(), 1);
}

// ---- A loop's waits ---------------------------------------------------------------------------

/// A loop waiting for its frame is parked, and what it has to do stays what it has to do.
#[test]
fn a_loop_waiting_for_its_frame_doesnt_spin() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    running(recomposer);
    run_ready();
    recomposer.schedule_frame_end_callback(|| {});
    let mut ready = 0;
    for _ in 0..50 {
        run_ready();
        if has_ready_tasks() {
            ready += 1;
        }
    }
    assert_eq!(ready, 0);
    assert!(recomposer.has_pending_work());
    advance_clock(Duration::from_millis(16));
    dispatch_frame();
    run_ready();
    assert!(!recomposer.has_pending_work());
    assert_eq!(recomposer.current_state().get(), RecomposerState::Idle);
    recomposer.cancel();
    run_ready();
}

/// A loop stopped while it waits for its frame returns.
#[test]
fn a_loop_waiting_for_its_frame_returns_when_cancelled() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let runner = running(recomposer);
    run_ready();
    recomposer.schedule_frame_end_callback(|| {});
    run_ready();
    assert!(!runner.get());
    recomposer.cancel();
    run_ready();
    assert!(runner.get());
}

// ---- close and cancel ------------------------------------------------------------------------

/// `close` leaves what's there going: with an effect running it's as it was, and it recomposes.
#[test]
fn closing_with_effects_running_goes_on_recomposing() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let shown = Rc::new(Cell::new(-1));
    let seen = shown.clone();
    running(recomposer);
    composition.set_content(move || {
        seen.set(read.get());
        LaunchedEffect((), std::future::pending::<()>());
    });
    run_ready();
    drive(|| recomposer.current_state().get() == RecomposerState::Idle);
    recomposer.close();
    assert_eq!(recomposer.current_state().get(), RecomposerState::Idle);
    Snapshot::with_mutable_snapshot(|| state.set(1));
    drive(|| shown.get() == 1);
    assert_eq!(shown.get(), 1, "recomposed, closed");
    recomposer.cancel();
    run_ready();
}

/// A task of a closed recomposer's effect still starts, while one is running; with none, none does.
#[test]
fn a_closed_recomposer_starts_effects_while_it_has_effects() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let more = mutable_state_of(false);
    let read = more.handle();
    let started = Rc::new(Cell::new(false));
    let flag = started.clone();
    running(recomposer);
    composition.set_content(move || {
        LaunchedEffect((), std::future::pending::<()>());
        if read.get() {
            let flag = flag.clone();
            LaunchedEffect((), async move { flag.set(true) });
        }
    });
    run_ready();
    recomposer.close();
    more.set(true);
    drive(|| started.get());
    assert!(started.get());
    recomposer.cancel();
    run_ready();
}

/// `withRunningRecomposer` closes, and waits for what's running: an effect ends on its own.
#[test]
fn a_running_recomposers_block_ending_lets_its_effects_finish() {
    set_manual_clock(0);
    let seen = Rc::new(Cell::new(false));
    let (flag, after) = (seen.clone(), seen.clone());
    let at_return = Rc::new(Cell::new(false));
    let noted = at_return.clone();
    let done = finishes(async move {
        let composition = with_running_recomposer(async |recomposer: Recomposer| {
            let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
            composition.set_content(move || {
                let flag = flag.clone();
                LaunchedEffect((), async move {
                    delay(Duration::from_secs(1)).await;
                    flag.set(true);
                });
            });
            composition
        })
        .await;
        noted.set(after.get());
        drop(composition);
    });
    drive(|| done.get());
    assert!(done.get());
    assert!(at_return.get(), "the effect ended before it returned");
    assert!(seen.get());
}

// ---- Inactive, pending ------------------------------------------------------------------------

/// With no loop, only frame work is pending: what's invalid is not, as `deriveStateLocked` clears
/// it. An effect waiting for a frame is: and its frame is the loop's when it starts.
#[test]
fn inactive_pending_work_is_frame_work() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let shown = Rc::new(Cell::new(-1));
    let seen = shown.clone();
    composition.set_content(move || seen.set(read.get()));
    assert_eq!(recomposer.current_state().get(), RecomposerState::Inactive);
    state.set(1);
    Snapshot::send_apply_notifications();
    assert!(composition.has_invalidations());
    assert_eq!(recomposer.current_state().get(), RecomposerState::Inactive, "no one to do it");
    assert!(!recomposer.has_pending_work());

    let write = state.handle();
    let effects = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    effects.set_content(move || {
        LaunchedEffect((), async move {
            with_frame_nanos(|_| ()).await;
            write.set(2);
        });
    });
    run_ready();
    assert_eq!(recomposer.current_state().get(), RecomposerState::InactivePendingWork);
    assert!(recomposer.has_pending_work());

    running(recomposer);
    run_ready();
    assert_eq!(recomposer.current_state().get(), RecomposerState::PendingWork);
    advance_clock(Duration::from_millis(16));
    dispatch_frame();
    run_ready();
    assert_eq!(shown.get(), 2, "composed in the frame it wrote in");
    recomposer.cancel();
    run_ready();
}

/// A frame-end callback with no loop is pending too.
#[test]
fn a_frame_end_callback_with_no_loop_is_inactive_pending_work() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    assert_eq!(recomposer.current_state().get(), RecomposerState::Inactive);
    let handle = recomposer.schedule_frame_end_callback(|| {});
    assert_eq!(recomposer.current_state().get(), RecomposerState::InactivePendingWork);
    handle.cancel();
    assert_eq!(recomposer.current_state().get(), RecomposerState::Inactive);
}

/// What the turn wrote counts: asking sends it (D751).
#[test]
fn asking_for_pending_work_sends_the_turns_changes() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    composition.set_content(move || {
        read.get();
    });
    running(recomposer);
    drive(|| recomposer.current_state().get() == RecomposerState::Idle);
    state.set(1);
    assert!(recomposer.has_pending_work());
    recomposer.cancel();
    run_ready();
}

/// Starting the loop invalidates what's known, as it wasn't observed before, as
/// `recompositionRunner`.
#[test]
fn starting_the_loop_recomposes_its_compositions() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    composition.set_content(move || counted.set(counted.get() + 1));
    assert_eq!(runs.get(), 1);
    running(recomposer);
    drive(|| runs.get() == 2);
    assert_eq!(runs.get(), 2);
    recomposer.cancel();
    run_ready();
}

// ---- Pausing ---------------------------------------------------------------------------------

/// Pausing holds an awaiter already there, and asks no frame for it; a task outside the
/// recomposer's effects keeps its frames.
#[test]
fn pausing_holds_its_effects_only() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let (inner, outer) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (i, o) = (inner.clone(), outer.clone());
    running(recomposer);
    composition.set_content(move || {
        let i = i.clone();
        LaunchedEffect((), async move {
            loop {
                with_frame_nanos(|_| ()).await;
                i.set(i.get() + 1);
            }
        });
    });
    let outside = launch(async move {
        loop {
            with_frame_nanos(|_| ()).await;
            o.set(o.get() + 1);
        }
    });
    run_ready();
    assert_eq!(recomposer.current_state().get(), RecomposerState::PendingWork);
    recomposer.pause_composition_frame_clock();
    let frame = || {
        advance_clock(Duration::from_millis(16));
        dispatch_frame();
        run_ready();
    };
    frame();
    assert_eq!((inner.get(), outer.get()), (0, 1), "the first frame after the pause is held too");
    assert_eq!(recomposer.current_state().get(), RecomposerState::Idle, "what's held isn't work");
    assert!(!recomposer.has_pending_work());
    frame();
    assert_eq!((inner.get(), outer.get()), (0, 2));
    recomposer.resume_composition_frame_clock();
    run_ready();
    assert_eq!(recomposer.current_state().get(), RecomposerState::PendingWork);
    frame();
    assert_eq!((inner.get(), outer.get()), (1, 3));
    outside.cancel();
    recomposer.cancel();
    run_ready();
}

/// A frame-end callback while paused is held, as `hasNextFrameEndAwaitersLocked`.
#[test]
fn a_frame_end_callback_waits_for_the_pause_to_end() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    running(recomposer);
    run_ready();
    recomposer.pause_composition_frame_clock();
    recomposer.schedule_frame_end_callback(|| {});
    assert_eq!(recomposer.current_state().get(), RecomposerState::Idle);
    assert!(!recomposer.has_pending_work());
    assert!(!has_frame_awaiters(), "its loop doesn't ask for a frame");
    recomposer.resume_composition_frame_clock();
    run_ready();
    assert!(has_frame_awaiters(), "its loop heard the pause is over");
    assert_eq!(recomposer.current_state().get(), RecomposerState::PendingWork);
    recomposer.cancel();
    run_ready();
}

// ---- A task an effect launches ---------------------------------------------------------------

/// `launch` in an effect's task, as a child of its coroutine: what it waits for is its recomposer's.
fn effect_launching(
    recomposer: Recomposer,
    task: impl Future<Output = ()> + 'static,
) -> (Composition, Rc<Cell<bool>>) {
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let task = Rc::new(RefCell::new(Some(task)));
    let launched = Rc::new(Cell::new(false));
    let flag = launched.clone();
    composition.set_content(move || {
        let (task, flag) = (task.clone(), flag.clone());
        LaunchedEffect((), async move {
            launch(task.borrow_mut().take().expect("once"));
            flag.set(true);
        });
    });
    run_ready();
    assert!(launched.get());
    (composition, launched)
}

#[test]
fn pausing_holds_a_task_an_effect_launched() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    running(recomposer);
    run_ready();
    let frames = Rc::new(Cell::new(0));
    let counted = frames.clone();
    let _held = effect_launching(recomposer, async move {
        loop {
            with_frame_nanos(|_| ()).await;
            counted.set(counted.get() + 1);
        }
    });
    let frame = || {
        advance_clock(Duration::from_millis(16));
        dispatch_frame();
        run_ready();
    };
    frame();
    assert_eq!(frames.get(), 1);
    recomposer.pause_composition_frame_clock();
    frame();
    assert_eq!(frames.get(), 1);
    recomposer.resume_composition_frame_clock();
    run_ready();
    frame();
    assert_eq!(frames.get(), 2);
    recomposer.cancel();
    run_ready();
}

#[test]
fn what_a_task_an_effect_launched_writes_in_its_frame_is_composed_in_it() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let shown = Rc::new(Cell::new(-1));
    let seen = shown.clone();
    composition.set_content(move || seen.set(read.get()));
    running(recomposer);
    drive(|| recomposer.current_state().get() == RecomposerState::Idle);
    let write = state.handle();
    let _held = effect_launching(recomposer, async move {
        with_frame_nanos(|_| ()).await;
        write.set(1);
    });
    assert!(recomposer.has_pending_work(), "it waits for a frame");
    advance_clock(Duration::from_millis(16));
    dispatch_frame();
    run_ready();
    assert_eq!(shown.get(), 1);
    recomposer.cancel();
    run_ready();
}

#[test]
fn cancelling_cancels_a_task_an_effect_launched() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    struct Noted(Rc<Cell<bool>>);
    impl Drop for Noted {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let noted = Noted(dropped.clone());
    let _held = effect_launching(recomposer, async move {
        let _noted = noted;
        std::future::pending::<()>().await;
    });
    run_ready();
    assert!(!dropped.get());
    recomposer.cancel();
    run_ready();
    assert!(dropped.get());
    assert_eq!(recomposer.current_state().get(), RecomposerState::ShutDown);
}

// ---- Its owner dropped -----------------------------------------------------------------------

/// A recomposer dropped with its loop running, things waiting and its clock paused: all of it ends.
#[test]
fn dropping_a_recomposers_owner_ends_what_waits_on_it() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let runner = running(recomposer);
    run_ready();
    let joined = finishes(async move { recomposer.join().await });
    run_ready();
    let info = recomposer.as_recomposer_info();
    assert!(running_recomposers().get().contains(&info));
    drop(owner);
    assert!(!running_recomposers().get().contains(&info), "not listed once it's gone");
    run_ready();
    assert!(runner.get(), "its loop returned");
    assert!(joined.get());
    assert!(running_recomposers().get().is_empty());
}

/// Its effects go with it, once the arena lets it go, at the next pass of any composition.
#[test]
fn dropping_a_recomposers_owner_cancels_its_effects() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    struct Noted(Rc<Cell<bool>>);
    impl Drop for Noted {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let noted = Rc::new(RefCell::new(Some(Noted(dropped.clone()))));
    composition.set_content(move || {
        let noted = noted.borrow_mut().take();
        LaunchedEffect((), async move {
            let _noted = noted;
            std::future::pending::<()>().await;
        });
    });
    run_ready();
    recomposer.pause_composition_frame_clock();
    drop(owner);
    let other = Composition::new(ViewApplier::new());
    other.set_content(|| {});
    other.set_content(|| {});
    run_ready();
    assert!(dropped.get());
}

// ---- Shut down -------------------------------------------------------------------------------

/// Shut down, a frame composes nothing.
#[test]
fn a_frame_of_a_shut_down_recomposer_does_nothing() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    composition.set_content(move || {
        read.get();
        counted.set(counted.get() + 1);
    });
    recomposer.cancel();
    state.set(1);
    assert!(!perform_frame(recomposer, 0));
    assert_eq!(runs.get(), 1);
}

#[test]
#[should_panic(expected = "cancelled or closed")]
fn a_frame_end_callback_of_a_shut_down_recomposer_is_refused() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    recomposer.cancel();
    recomposer.schedule_frame_end_callback(|| {});
}

// ---- A frame's order --------------------------------------------------------------------------

/// A frame's callbacks run after its work.
#[test]
fn a_frame_end_callback_runs_after_the_frames_effects() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let state = mutable_state_of(0);
    let read = state.handle();
    let log = crate::mock::Log::new();
    let effects = log.clone();
    composition.set_content(move || {
        let value = read.get();
        let effects = effects.clone();
        SideEffect(move || effects.push(format!("effect {value}")));
    });
    log.clear();
    state.set(1);
    let ended = log.clone();
    recomposer.schedule_frame_end_callback(move || ended.push("end"));
    perform_frame(recomposer, 0);
    assert_eq!(log.take(), ["effect 1", "end"]);
}

/// What the next one wrote of what the first read is the next frame's: each composes once a frame.
#[test]
fn a_composition_composes_once_a_frame() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let first = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let second = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let (a, b) = (mutable_state_of(0), mutable_state_of(0));
    let log = crate::mock::Log::new();
    let (read_a, write_a, read_b, log_first, log_second) =
        (a.handle(), a.handle(), b.handle(), log.clone(), log.clone());
    first.set_content(move || log_first.push(format!("first {}", read_a.get())));
    second.set_content(move || {
        let value = read_b.get();
        log_second.push(format!("second {value}"));
        // Written in composing: what `first` read, once it's composed in this frame.
        write_a.set(value);
    });
    log.clear();
    b.set(5);
    assert!(perform_frame(recomposer, 0));
    assert_eq!(log.take(), ["second 5", "first 5"], "invalid by the write, this frame");
    b.set(6);
    a.set(1);
    assert!(perform_frame(recomposer, 0));
    assert_eq!(log.take(), ["first 1", "second 6"], "first's over for this frame");
    assert!(first.has_invalidations(), "it's for the next");
    assert!(perform_frame(recomposer, 0));
    assert_eq!(log.take(), ["first 6"]);
}

/// A frame counts as a change if something changed, before its work runs, as `changeCount`.
#[test]
fn a_frame_counts_as_a_change_before_its_work() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let (nothing, text) = (mutable_state_of(0), mutable_state_of(0));
    let (read_nothing, read_text) = (nothing.handle(), text.handle());
    let during = Rc::new(Cell::new(0));
    let seen = during.clone();
    composition.set_content(move || {
        read_nothing.get();
        let value = read_text.get();
        crate::mock::Text(&value.to_string());
        DisposableEffect(value, {
            let seen = seen.clone();
            move |scope| {
                seen.set(recomposer.change_count());
                scope.on_dispose(|| {})
            }
        });
    });
    let before = recomposer.change_count();
    nothing.set(1);
    assert!(perform_frame(recomposer, 0), "a pass ran");
    assert_eq!(recomposer.change_count(), before, "it changed nothing");
    text.set(1);
    assert!(perform_frame(recomposer, 0));
    assert_eq!(recomposer.change_count(), before + 1);
    assert_eq!(during.get(), before + 1, "its effects see it");
}

/// An effect polled again while its clock is paused gives up what it asked for: nothing's asked of
/// the host for it, and it asks again when the pause is over.
#[test]
fn an_effect_polled_while_paused_stops_asking_for_frames() {
    set_manual_clock(0);
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let composition = Composition::with_parent(ViewApplier::new(), recomposer.as_context());
    let waker: Rc<RefCell<Option<std::task::Waker>>> = Rc::default();
    let frames = Rc::new(Cell::new(0));
    let (kept, counted) = (waker.clone(), frames.clone());
    composition.set_content(move || {
        let (kept, counted) = (kept.clone(), counted.clone());
        LaunchedEffect((), async move {
            let mut frame = std::pin::pin!(with_frame_nanos(|_| ()));
            // What lets the test poll this task without a frame.
            std::future::poll_fn(|context| {
                *kept.borrow_mut() = Some(context.waker().clone());
                frame.as_mut().poll(context)
            })
            .await;
            counted.set(counted.get() + 1);
        });
    });
    run_ready();
    assert!(has_frame_awaiters());
    recomposer.pause_composition_frame_clock();
    waker.borrow().as_ref().expect("polled").wake_by_ref();
    run_ready();
    assert!(!has_frame_awaiters());
    recomposer.resume_composition_frame_clock();
    run_ready();
    assert!(has_frame_awaiters());
    assert!(recomposer.has_pending_work());
    advance_clock(Duration::from_millis(16));
    dispatch_frame();
    run_ready();
    assert_eq!(frames.get(), 1);
    assert!(!recomposer.has_pending_work());
    recomposer.cancel();
    run_ready();
}
