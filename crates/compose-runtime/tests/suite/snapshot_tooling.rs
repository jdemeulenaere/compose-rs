//! Observing snapshots, as Compose's `SnapshotObserverTests` (step S7, D1502).

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::NodeTree;
use compose_runtime::platform::{Applier, subcomposition};
use compose_runtime::snapshots::tooling::{
    SnapshotInstanceObservers, SnapshotObserver, observe_snapshots,
};
use compose_runtime::snapshots::{ChangeSet, Snapshot, StateObjectId, StateObserverFn};
use compose_runtime::*;

use crate::mock::ViewApplier;

/// Validates at its end, as the Kotlin file's `validateOpenSnapshots`.
struct OpenSnapshots(usize);

fn open_snapshots() -> OpenSnapshots {
    OpenSnapshots(Snapshot::open_snapshot_count())
}

impl Drop for OpenSnapshots {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert_eq!(self.0, Snapshot::open_snapshot_count(), "a snapshot wasn't disposed");
        }
    }
}

type Log<T> = Rc<RefCell<Vec<T>>>;

/// What an observer heard.
#[derive(PartialEq, Debug)]
enum Heard {
    Creating(Option<Snapshot>, bool),
    Created(Snapshot, Option<Snapshot>),
    Disposing(Snapshot),
    Applied(Snapshot, Vec<StateObjectId>),
    Reading(StateObjectId),
    Writing(StateObjectId),
}

/// Records everything, under `name`.
struct Recording {
    name: &'static str,
    log: Log<(&'static str, Heard)>,
}

impl Recording {
    fn record(&self, heard: Heard) {
        self.log.borrow_mut().push((self.name, heard));
    }
}

impl SnapshotObserver for Recording {
    fn on_pre_create(
        &self,
        parent: Option<&Snapshot>,
        read_only: bool,
    ) -> Option<Rc<SnapshotInstanceObservers>> {
        self.record(Heard::Creating(parent.map(Snapshot::handle), read_only));
        let (name, reads, writes) = (self.name, self.log.clone(), self.log.clone());
        Some(Rc::new(SnapshotInstanceObservers {
            read_observer: Some(Rc::new(move |state| {
                reads.borrow_mut().push((name, Heard::Reading(state)));
            })),
            write_observer: Some(Rc::new(move |state| {
                writes.borrow_mut().push((name, Heard::Writing(state)));
            })),
        }))
    }

    fn on_created(
        &self,
        snapshot: &Snapshot,
        parent: Option<&Snapshot>,
        _observers: Option<&Rc<SnapshotInstanceObservers>>,
    ) {
        self.record(Heard::Created(snapshot.handle(), parent.map(Snapshot::handle)));
    }

    fn on_pre_dispose(&self, snapshot: &Snapshot) {
        self.record(Heard::Disposing(snapshot.handle()));
    }

    fn on_applied(&self, snapshot: &Snapshot, changed: &ChangeSet) {
        self.record(Heard::Applied(snapshot.handle(), changed.iter().collect()));
    }
}

/// Keeps what each method named by the test gets.
#[derive(Default, Clone)]
struct Seen {
    created: Log<Snapshot>,
    disposed: Log<Snapshot>,
    applied: Log<(Snapshot, Vec<StateObjectId>)>,
}

impl SnapshotObserver for Seen {
    fn on_created(
        &self,
        snapshot: &Snapshot,
        _: Option<&Snapshot>,
        _: Option<&Rc<SnapshotInstanceObservers>>,
    ) {
        self.created.borrow_mut().push(snapshot.handle());
    }

    fn on_pre_dispose(&self, snapshot: &Snapshot) {
        self.disposed.borrow_mut().push(snapshot.handle());
    }

    fn on_applied(&self, snapshot: &Snapshot, changed: &ChangeSet) {
        self.applied.borrow_mut().push((snapshot.handle(), changed.iter().collect()));
    }
}

/// Gives each snapshot the observers `make` gives.
struct Giving(Box<dyn Fn() -> SnapshotInstanceObservers>);

impl SnapshotObserver for Giving {
    fn on_pre_create(
        &self,
        _: Option<&Snapshot>,
        _: bool,
    ) -> Option<Rc<SnapshotInstanceObservers>> {
        Some(Rc::new((self.0)()))
    }
}

fn tagged(log: &Log<(StateObjectId, bool)>, tag: bool) -> Option<StateObserverFn> {
    let log = log.clone();
    Some(Rc::new(move |state| log.borrow_mut().push((state, tag))))
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canAddAndRemoveObserver
#[test]
fn an_observer_can_be_added_and_removed() {
    struct Nothing;
    impl SnapshotObserver for Nothing {}
    let _open = open_snapshots();
    observe_snapshots(Nothing).dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserverReadonlySnapshotCreation
#[test]
fn a_read_only_snapshots_creation_is_observed() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let _observer = observe_snapshots(seen.clone());
    let created = Snapshot::take_snapshot(None);
    assert!(seen.created.borrow().contains(&created));
    created.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserverMutableSnapshotCreation
#[test]
fn a_mutable_snapshots_creation_is_observed() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let _observer = observe_snapshots(seen.clone());
    let created = Snapshot::take_mutable_snapshot(None, None);
    assert!(seen.created.borrow().iter().any(|snapshot| created == *snapshot));
    created.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserveApply
#[test]
fn an_apply_is_observed() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let _observer = observe_snapshots(seen.clone());
    let state = mutable_state_of(10);
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.enter(|| state.set(12));
    snapshot.apply().check();
    let applied = seen.applied.borrow();
    assert!(snapshot == applied[0].0);
    assert!(applied[0].1.contains(&state.state_object_id()));
    snapshot.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserverDisposeOfReadonlySnapshot
#[test]
fn a_read_only_snapshots_dispose_is_observed() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let _observer = observe_snapshots(seen.clone());
    let snapshot = Snapshot::take_snapshot(None);
    snapshot.dispose();
    assert!(seen.disposed.borrow().contains(&snapshot));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserverDisposeOfMutableSnapshot_NotApplied
#[test]
fn a_mutable_snapshots_dispose_is_observed_not_applied() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let _observer = observe_snapshots(seen.clone());
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.dispose();
    assert!(seen.disposed.borrow().iter().any(|disposed| snapshot == *disposed));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserverDisposeOfMutableSnapshot_Applied
#[test]
fn a_mutable_snapshots_dispose_is_observed_applied() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let _observer = observe_snapshots(seen.clone());
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.apply().check();
    snapshot.dispose();
    assert!(seen.disposed.borrow().iter().any(|disposed| snapshot == *disposed));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserveApplyOfNestedSnapshot
#[test]
fn a_nested_snapshots_apply_is_observed() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let _observer = observe_snapshots(seen.clone());
    let state = mutable_state_of(10);
    let id = state.state_object_id();
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    let nested = snapshot.take_nested_mutable_snapshot(None, None);
    nested.enter(|| state.set(12));
    nested.apply().check();
    snapshot.apply().check();
    let applied = seen.applied.borrow();
    assert!(nested == applied[0].0);
    assert!(applied[0].1.contains(&id));
    nested.dispose();
    assert!(snapshot == applied[applied.len() - 1].0);
    assert!(applied[applied.len() - 1].1.contains(&id));
    snapshot.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserveReadsInReadonlySnapshot
#[test]
fn reads_in_a_read_only_snapshot_are_observed() {
    let _open = open_snapshots();
    let state = mutable_state_of(10);
    let read: Log<(StateObjectId, bool)> = Rc::default();
    let log = read.clone();
    let _observer = observe_snapshots(Giving(Box::new(move || SnapshotInstanceObservers {
        read_observer: tagged(&log, true),
        write_observer: None,
    })));
    let snapshot = Snapshot::take_snapshot(tagged(&read, false));
    assert_eq!(snapshot.enter(|| state.get()), 10);
    let id = state.state_object_id();
    assert_eq!(*read.borrow(), [(id, true), (id, false)]);
    snapshot.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserveReadsInMutableSnapshot
#[test]
fn reads_in_a_mutable_snapshot_are_observed() {
    let _open = open_snapshots();
    let state = mutable_state_of(10);
    let read: Log<(StateObjectId, bool)> = Rc::default();
    let log = read.clone();
    let _observer = observe_snapshots(Giving(Box::new(move || SnapshotInstanceObservers {
        read_observer: tagged(&log, true),
        write_observer: None,
    })));
    let snapshot = Snapshot::take_mutable_snapshot(tagged(&read, false), None);
    assert_eq!(snapshot.enter(|| state.get()), 10);
    let id = state.state_object_id();
    assert_eq!(*read.borrow(), [(id, true), (id, false)]);
    snapshot.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canObserveWrites
#[test]
fn writes_are_observed() {
    let _open = open_snapshots();
    let state = mutable_state_of(10);
    let writes: Log<(StateObjectId, bool)> = Rc::default();
    let log = writes.clone();
    let _observer = observe_snapshots(Giving(Box::new(move || SnapshotInstanceObservers {
        read_observer: None,
        write_observer: tagged(&log, true),
    })));
    let snapshot = Snapshot::take_mutable_snapshot(None, tagged(&writes, false));
    let result = snapshot.enter(|| {
        state.set(20);
        state.get()
    });
    assert_eq!(result, 20);
    let id = state.state_object_id();
    assert_eq!(*writes.borrow(), [(id, true), (id, false)]);
    snapshot.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canHaveMultipleObservers
#[test]
fn several_observers_each_hear() {
    let _open = open_snapshots();
    let log: Log<(&'static str, Heard)> = Rc::default();
    let (state1, state2) = (mutable_state_of(1), mutable_state_of(2));
    let (id1, id2) = (state1.state_object_id(), state2.state_object_id());
    let _outer = observe_snapshots(Recording { name: "Outer", log: log.clone() });
    let _inner = observe_snapshots(Recording { name: "Inner", log: log.clone() });
    let read_only = Snapshot::take_snapshot(None);
    read_only.enter(|| {
        state1.get();
        state2.get();
    });
    read_only.dispose();
    let mutable = Snapshot::take_mutable_snapshot(None, None);
    mutable.enter(|| state1.set(11));
    mutable.apply().check();
    mutable.dispose();
    let ro = || read_only.handle();
    let ms = || mutable.handle();
    use Heard::*;
    assert_eq!(
        *log.borrow(),
        [
            ("Outer", Creating(None, true)),
            ("Inner", Creating(None, true)),
            ("Outer", Created(ro(), None)),
            ("Inner", Created(ro(), None)),
            ("Inner", Reading(id1)),
            ("Outer", Reading(id1)),
            ("Inner", Reading(id2)),
            ("Outer", Reading(id2)),
            ("Outer", Disposing(ro())),
            ("Inner", Disposing(ro())),
            ("Outer", Creating(None, false)),
            ("Inner", Creating(None, false)),
            ("Outer", Created(ms(), None)),
            ("Inner", Created(ms(), None)),
            ("Inner", Writing(id1)),
            ("Outer", Writing(id1)),
            ("Outer", Applied(ms(), vec![id1])),
            ("Inner", Applied(ms(), vec![id1])),
            ("Outer", Disposing(ms())),
            ("Inner", Disposing(ms())),
        ]
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, receivesTheCorrectParent
#[test]
fn each_hears_the_right_parent() {
    let _open = open_snapshots();
    let log: Log<(&'static str, Heard)> = Rc::default();
    let _observer = observe_snapshots(Recording { name: "", log: log.clone() });
    let ro1 = Snapshot::take_snapshot(None);
    let ro2 = ro1.take_nested_snapshot(None);
    ro2.dispose();
    ro1.dispose();
    let ms1 = Snapshot::take_mutable_snapshot(None, None);
    let ms2 = ms1.take_nested_mutable_snapshot(None, None);
    ms1.dispose();
    ms2.dispose();
    use Heard::*;
    let heard: Vec<_> = log.borrow_mut().drain(..).map(|(_, heard)| heard).collect();
    assert_eq!(
        heard,
        [
            Creating(None, true),
            Created(ro1.handle(), None),
            Creating(Some(ro1.handle()), true),
            Created(ro2.handle(), Some(ro1.handle())),
            Disposing(ro2.handle()),
            Disposing(ro1.handle()),
            Creating(None, false),
            Created(ms1.handle(), None),
            Creating(Some(ms1.handle()), false),
            Created(ms2.handle(), Some(ms1.handle())),
            Disposing(ms1.handle()),
            Disposing(ms2.handle()),
        ]
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/tooling/SnapshotObserverTests.kt, canCorrelateCreatingAndCreating
#[test]
fn what_on_pre_create_gives_reaches_on_created() {
    #[derive(Default)]
    struct Correlating {
        given: RefCell<Option<Rc<SnapshotInstanceObservers>>>,
        correlated: Rc<Cell<bool>>,
    }
    impl SnapshotObserver for Correlating {
        fn on_pre_create(
            &self,
            _: Option<&Snapshot>,
            _: bool,
        ) -> Option<Rc<SnapshotInstanceObservers>> {
            let given = Rc::new(SnapshotInstanceObservers::default());
            *self.given.borrow_mut() = Some(given.clone());
            Some(given)
        }
        fn on_created(
            &self,
            _: &Snapshot,
            _: Option<&Snapshot>,
            observers: Option<&Rc<SnapshotInstanceObservers>>,
        ) {
            let given = self.given.borrow();
            let same = Rc::ptr_eq(observers.expect("given"), given.as_ref().expect("given"));
            self.correlated.set(same);
        }
    }
    let _open = open_snapshots();
    let correlated: Rc<Cell<bool>> = Rc::default();
    let _observer = observe_snapshots(Correlating {
        given: RefCell::default(),
        correlated: correlated.clone(),
    });
    Snapshot::take_mutable_snapshot(None, None).dispose();
    assert!(correlated.get());
}

// ---- Beyond the ports: what compose-rs's own paths add -------------------------------------------

/// A disposed handle stops an observer hearing, and what it gave for a snapshot still open goes on.
#[test]
fn a_disposed_observer_hears_no_more() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let handle = observe_snapshots(seen.clone());
    let log: Log<(&'static str, Heard)> = Rc::default();
    let recording = observe_snapshots(Recording { name: "", log: log.clone() });
    let state = mutable_state_of(0);
    let first = Snapshot::take_snapshot(None);
    handle.dispose();
    recording.dispose();
    log.borrow_mut().clear();
    let second = Snapshot::take_snapshot(None);
    assert_eq!(seen.created.borrow().len(), 1);
    assert!(!seen.created.borrow().contains(&second));
    assert!(log.borrow().is_empty());
    first.enter(|| state.get());
    assert_eq!(*log.borrow(), [("", Heard::Reading(state.state_object_id()))]);
    second.enter(|| state.get());
    assert_eq!(log.borrow().len(), 1);
    drop((first, second));
    assert!(seen.disposed.borrow().is_empty());
}

/// A pass's snapshot is observed as any, as `Recomposer.composing` takes it with
/// `takeMutableSnapshot`: its creation, reads and writes, its apply, its dispose.
#[test]
fn a_pass_is_observed() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let (handle, id) = (state.handle(), state.state_object_id());
    let log: Log<(&'static str, Heard)> = Rc::default();
    let composition = Composition::new(NodeTree::<()>::new());
    let observer = observe_snapshots(Recording { name: "", log: log.clone() });
    composition.set_content(move || {
        handle.get();
        handle.set(1);
    });
    drop(observer);
    assert_eq!(state.get(), 1);
    use Heard::*;
    let heard: Vec<_> = log.borrow_mut().drain(..).map(|(_, heard)| heard).collect();
    let Created(pass, None) = &heard[1] else { panic!("{heard:?}") };
    assert_eq!(
        heard,
        [
            Creating(None, false),
            Created(pass.handle(), None),
            Reading(id),
            Writing(id),
            Applied(pass.handle(), vec![id]),
            Disposing(pass.handle()),
        ]
    );
    assert_eq!(Snapshot::current(), Snapshot::global_snapshot());
}

/// With no write in it, too, and no object reused while a tool may hold the last one.
#[test]
fn a_pass_writing_nothing_is_observed() {
    let _open = open_snapshots();
    let seen = Seen::default();
    let trigger = mutable_state_of(0);
    let read = trigger.handle();
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || {
        read.get();
    });
    let _observer = observe_snapshots(seen.clone());
    trigger.set(1);
    assert!(composition.recompose());
    trigger.set(2);
    assert!(composition.recompose());
    let created = seen.created.borrow();
    assert_eq!(created.len(), 2);
    assert!(created[0] != created[1]);
    assert_eq!(seen.applied.borrow().len(), 2);
    assert_eq!(seen.disposed.borrow().len(), 2);
}

/// A pass nested in a snapshot names it as its parent, and its apply goes to it.
#[test]
fn a_pass_in_a_snapshot_has_it_as_parent() {
    let _open = open_snapshots();
    let log: Log<(&'static str, Heard)> = Rc::default();
    let composition = Composition::new(NodeTree::<()>::new());
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    let _observer = observe_snapshots(Recording { name: "", log: log.clone() });
    snapshot.enter(|| composition.set_content(|| {}));
    let heard = log.borrow();
    assert!(heard.iter().any(|(_, heard)| {
        matches!(heard, Heard::Created(_, Some(parent)) if snapshot == *parent)
    }));
}

/// What a tool gives a pass ends with it, a panicking pass's too: later reads aren't heard.
#[test]
fn what_a_tool_gives_a_pass_ends_with_it() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let handle = state.handle();
    let reads = Rc::new(Cell::new(0));
    let counted = reads.clone();
    let count = reads.clone();
    let _observer = observe_snapshots(Giving(Box::new(move || {
        let count = count.clone();
        SnapshotInstanceObservers {
            read_observer: Some(Rc::new(move |_| count.set(count.get() + 1))),
            write_observer: None,
        }
    })));
    let composition = Composition::new(NodeTree::<()>::new());
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        composition.set_content(move || {
            handle.get();
            panic!("in composition");
        })
    }));
    assert!(panicked.is_err());
    assert_eq!(counted.get(), 1);
    state.get();
    assert_eq!(counted.get(), 1);
    assert!(!Snapshot::is_in_snapshot());
}

/// A tool that panics in a pass's snapshot ends the pass as an apply observer's panic does: what
/// the pass wrote applied, the snapshot left, none open.
#[test]
fn a_tool_panicking_for_a_pass_leaves_it_clean() {
    struct Panicking {
        on_create: bool,
    }
    impl SnapshotObserver for Panicking {
        fn on_pre_create(
            &self,
            _: Option<&Snapshot>,
            _: bool,
        ) -> Option<Rc<SnapshotInstanceObservers>> {
            assert!(!self.on_create, "on_pre_create");
            None
        }
        fn on_pre_dispose(&self, _: &Snapshot) {
            assert!(self.on_create, "on_pre_dispose");
        }
    }
    let _open = open_snapshots();
    for on_create in [true, false] {
        let state = mutable_state_of(0);
        let handle = state.handle();
        let composition = Composition::new(NodeTree::<()>::new());
        let observer = observe_snapshots(Panicking { on_create });
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            composition.set_content(move || handle.set(1));
        }));
        drop(observer);
        assert!(panicked.is_err());
        assert_eq!(state.get(), if on_create { 0 } else { 1 });
        assert!(!Snapshot::is_in_snapshot());
    }
}

/// A tool's panic for a nested pass takes nothing of the pass around it: the shared applier's count
/// is its own, begun before it's told.
#[test]
fn a_tool_panicking_for_a_nested_pass_leaves_the_outer_changes() {
    struct PanickingWhen(Rc<Cell<bool>>);
    impl SnapshotObserver for PanickingWhen {
        fn on_pre_create(
            &self,
            _: Option<&Snapshot>,
            _: bool,
        ) -> Option<Rc<SnapshotInstanceObservers>> {
            assert!(!self.0.get(), "on_pre_create");
            None
        }
    }
    let armed: Rc<Cell<bool>> = Rc::default();
    let _observer = observe_snapshots(PanickingWhen(armed.clone()));
    let composition = Rc::new(Composition::new(ViewApplier::new()));
    let root = composition.with_applier(|views: &mut ViewApplier| views.root());
    let counts = |composition: &Composition| {
        composition.with_applier(|views: &mut ViewApplier| (views.begin_changes, views.end_changes))
    };
    let in_body = Rc::new(Cell::new(None));
    let (seen, outer) = (in_body.clone(), composition.clone());
    composition.set_content(move || {
        let context = remember_composition_context();
        let first = remember(|| Rc::new(subcomposition(context, root)));
        let second = remember(|| Rc::new(subcomposition(context, root)));
        armed.set(true);
        let told = catch_unwind(AssertUnwindSafe(|| first.set_content(|| {})));
        armed.set(false);
        assert!(told.is_err());
        second.set_content(|| {});
        seen.set(Some(counts(&outer)));
    });
    assert_eq!(in_body.get(), Some((1, 0)));
    assert_eq!(counts(&composition), (1, 1));
}

/// A tool's frame is no barrier: a pass under one composes as any (it's the body's own that hides
/// a read), and what the tool gave hears what the pass observes.
#[test]
fn a_tools_frame_hides_nothing_from_a_pass() {
    let (read, hidden) = (mutable_state_of(0), mutable_state_of(0));
    let (r, h) = (read.handle(), hidden.handle());
    let heard: Log<(StateObjectId, bool)> = Rc::default();
    let log = heard.clone();
    let _observer = observe_snapshots(Giving(Box::new(move || SnapshotInstanceObservers {
        read_observer: tagged(&log, true),
        write_observer: None,
    })));
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let composition = Composition::new(NodeTree::<()>::new());
    Snapshot::without_read_observation(|| {
        composition.set_content(move || {
            counted.set(counted.get() + 1);
            r.get();
            Snapshot::without_read_observation(|| h.get());
        })
    });
    assert_eq!(runs.get(), 1);
    assert_eq!(*heard.borrow(), [(read.state_object_id(), true)]);
    hidden.set(1);
    composition.recompose();
    assert_eq!(runs.get(), 1, "under the body's own barrier");
    read.set(1);
    composition.recompose();
    assert_eq!(runs.get(), 2, "the pass's read");
}

/// Apply observers hear first, and tools after, as `MutableSnapshot.apply`, a pass's apply too.
#[test]
fn a_tool_hears_of_an_apply_after_apply_observers() {
    struct Applied(Log<&'static str>);
    impl SnapshotObserver for Applied {
        fn on_applied(&self, _: &Snapshot, _: &ChangeSet) {
            self.0.borrow_mut().push("tool");
        }
    }
    let _open = open_snapshots();
    let order: Log<&'static str> = Rc::default();
    let log = order.clone();
    let _apply = Snapshot::register_apply_observer(move |_, _| log.borrow_mut().push("apply"));
    let _observer = observe_snapshots(Applied(order.clone()));
    let state = mutable_state_of(0);
    let snapshot = Snapshot::take_mutable_snapshot(None, None);
    snapshot.enter(|| state.set(1));
    snapshot.apply().check();
    snapshot.dispose();
    assert_eq!(*order.borrow(), ["apply", "tool"]);
    order.borrow_mut().clear();
    let handle = state.handle();
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || handle.set(2));
    assert_eq!(*order.borrow(), ["apply", "tool"]);
}

/// Compose's dispatches read the list when they happen: a tool registered in a pass hears that
/// pass's end though not its creation, whether or not anything else wrote, and a nested pass begun
/// after it whole.
#[test]
fn a_tool_registered_in_a_pass_hears_what_follows() {
    let _open = open_snapshots();
    let log: Log<(&'static str, Heard)> = Rc::default();
    let (registering, handles) = (log.clone(), Rc::new(RefCell::new(Vec::new())));
    let kept = handles.clone();
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || {
        let tool = Recording { name: "", log: registering.clone() };
        kept.borrow_mut().push(observe_snapshots(tool));
        let context = remember_composition_context();
        let child = remember(|| Rc::new(Composition::with_parent(NodeTree::<()>::new(), context)));
        child.set_content(|| {});
    });
    handles.borrow_mut().clear();
    use Heard::*;
    let heard = log.borrow();
    let names: Vec<_> = heard
        .iter()
        .map(|(_, heard)| match heard {
            Creating(..) => "creating",
            Created(..) => "created",
            Applied(..) => "applied",
            Disposing(_) => "disposing",
            Reading(_) | Writing(_) => "state",
        })
        .collect();
    assert_eq!(
        names.iter().filter(|name| **name != "state").copied().collect::<Vec<_>>(),
        ["creating", "created", "applied", "disposing", "applied", "disposing"],
    );
    let Created(child, Some(pass)) = &heard[1].1 else { panic!("{heard:?}") };
    assert!(matches!(&heard[2].1, Applied(applied, _) if applied == child));
    assert!(matches!(&heard[4].1, Applied(applied, _) if applied == pass));
    assert!(matches!(&heard[5].1, Disposing(disposed) if disposed == pass));
}

/// A snapshot refused isn't heard of: Compose validates before `creatingSnapshot`.
#[test]
fn a_refused_snapshot_is_not_heard_of() {
    let _open = open_snapshots();
    let log: Log<(&'static str, Heard)> = Rc::default();
    let applied = Snapshot::take_mutable_snapshot(None, None);
    applied.apply().check();
    let disposed = Snapshot::take_snapshot(None);
    disposed.dispose();
    let _observer = observe_snapshots(Recording { name: "", log: log.clone() });
    let refused = |take: &dyn Fn()| catch_unwind(AssertUnwindSafe(take)).is_err();
    assert!(refused(&|| drop(applied.take_nested_snapshot(None))));
    assert!(refused(&|| drop(applied.take_nested_mutable_snapshot(None, None))));
    assert!(refused(&|| drop(disposed.take_nested_snapshot(None))));
    assert!(log.borrow().is_empty());
    log.borrow_mut().clear();
    drop(_observer);
    applied.dispose();
}

/// A tool's panic out of a destructor in an unwind would abort the process: a snapshot dropped by
/// a panic drops it, and the first panic goes on.
#[test]
fn a_tool_panicking_for_a_snapshot_dropped_in_an_unwind_aborts_nothing() {
    struct Panicking(Rc<Cell<u32>>);
    impl SnapshotObserver for Panicking {
        fn on_pre_dispose(&self, _: &Snapshot) {
            self.0.set(self.0.get() + 1);
            panic!("on_pre_dispose");
        }
    }
    let _open = open_snapshots();
    let told = Rc::new(Cell::new(0));
    let _first = observe_snapshots(Panicking(told.clone()));
    let _second = observe_snapshots(Panicking(told.clone()));
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _snapshot = Snapshot::take_snapshot(None);
        panic!("the first");
    }));
    let payload = unwound.expect_err("it panicked");
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"the first"));
    assert_eq!(told.get(), 2);
}
