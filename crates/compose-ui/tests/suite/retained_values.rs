//! What a host's content retains across hosts on one view tree, as Compose's
//! `LifecycleRetainedValuesStoreOwner`: a store per view, retaining from its stop until a frame
//! after its resume (D1306).

use std::cell::Cell;
use std::rc::Rc;

use compose_lifecycle::LifecycleState;
use compose_runtime::retain::*;
use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::retained_values::*;
use compose_ui::*;

fn test_host() -> UiHost {
    UiHost::new(IntSize::new(100, 100), Density::new(2.0), Box::new(FixedTextMeasurer::new()))
}

struct Told(Rc<Cell<i32>>);

impl RetainObserver for Told {
    fn on_retired(&self) {
        self.0.set(self.0.get() + 1);
    }
}

/// A host as the bridge makes one: its content under the store of `view_id`.
fn host(
    owner: &LifecycleRetainedValuesStoreOwner,
    view_id: i32,
    content: impl Fn() + 'static,
) -> UiHost {
    let mut ui = test_host();
    let entry = owner.get_or_create_entry(view_id);
    let store = entry.store();
    ui.set_retained_values_entry(entry);
    ui.set_lifecycle_state(LifecycleState::Resumed);
    ui.set_content(move || LocalRetainedValuesStoreProvider(store.clone(), &content));
    ui.frame();
    ui
}

fn retaining(retired: &Rc<Cell<i32>>, made: &Rc<Cell<i32>>) -> impl Fn() + 'static + use<> {
    let (retired, made) = (retired.clone(), made.clone());
    move || {
        let (retired, made) = (retired.clone(), made.clone());
        retain_observer(move || {
            made.set(made.get() + 1);
            Told(retired)
        });
    }
}

/// `<include>` twice, a `RecyclerView`'s rows, a `ViewPager`'s pages: one id, a store each.
#[test]
fn two_views_with_one_id_each_have_their_store() {
    let owner = LifecycleRetainedValuesStoreOwner::new();
    let (first, second) = (host(&owner, 7, || {}), host(&owner, 7, || {}));
    let (mut first, mut second) = (first, second);
    first.frame();
    second.frame();
    let (entry, other) = (owner.get_or_create_entry(7), owner.get_or_create_entry(7));
    assert!(!Rc::ptr_eq(&entry, &other), "in use, each is its host's");
    entry.release();
    let again = owner.get_or_create_entry(7);
    assert!(Rc::ptr_eq(&entry, &again), "released, it's the next one's");
    drop(first);
    drop(second);
    owner.clear();
}

/// In `retain` or not, each keeps its own across a stop: the next hosts take them in order.
#[test]
fn two_views_with_one_id_keep_their_own_values() {
    let owner = LifecycleRetainedValuesStoreOwner::new();
    let (retired, made) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let first = host(&owner, 7, retaining(&retired, &made));
    let second = host(&owner, 7, retaining(&retired, &made));
    assert_eq!(made.get(), 2);
    for ui in [&first, &second] {
        ui.set_lifecycle_state(LifecycleState::Created);
    }
    drop(first);
    drop(second);
    assert_eq!(retired.get(), 0);
    let (third, fourth) =
        (host(&owner, 7, retaining(&retired, &made)), host(&owner, 7, retaining(&retired, &made)));
    assert_eq!(made.get(), 2, "each took its own");
    drop(third);
    drop(fourth);
    owner.clear();
}

/// A view with no id is one of the views with that id, as Compose's.
#[test]
fn views_with_no_id_each_have_their_store() {
    let owner = LifecycleRetainedValuesStoreOwner::new();
    let (mut first, mut second) = (host(&owner, -1, || {}), host(&owner, -1, || {}));
    first.frame();
    second.frame();
    owner.clear();
}

/// Removed while resumed: nothing keeps it, so it's retired at once, as Compose retains only from
/// its stop.
#[test]
fn a_view_removed_while_resumed_retires_what_it_retained() {
    let owner = LifecycleRetainedValuesStoreOwner::new();
    let (retired, made) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let ui = host(&owner, 7, retaining(&retired, &made));
    assert_eq!(retired.get(), 0);
    drop(ui);
    assert_eq!(retired.get(), 1);
    owner.clear();
}

/// A configuration change: stopped, destroyed, and a new host taking what's kept, the rest going a
/// frame after its resume.
#[test]
fn a_stopped_host_hands_what_it_retained_to_the_next() {
    let owner = LifecycleRetainedValuesStoreOwner::new();
    let (retired, made) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let ui = host(&owner, 7, retaining(&retired, &made));
    ui.set_lifecycle_state(LifecycleState::Started);
    ui.set_lifecycle_state(LifecycleState::Created);
    drop(ui);
    assert_eq!(retired.get(), 0, "kept");
    let mut next = host(&owner, 7, retaining(&retired, &made));
    assert_eq!(made.get(), 1, "taken back");
    next.frame();
    assert_eq!(retired.get(), 0);
    drop(next);
    assert_eq!(retired.get(), 1, "resumed by then, it's retired with its host");
    owner.clear();
}

/// What's kept and not asked for goes at the end of the new host's frame after its resume, not at
/// another host's.
#[test]
fn another_hosts_frame_doesnt_end_the_retaining() {
    let owner = LifecycleRetainedValuesStoreOwner::new();
    let (retired, made) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let ui = host(&owner, 7, retaining(&retired, &made));
    ui.set_lifecycle_state(LifecycleState::Created);
    drop(ui);
    let mut other = host(&owner, 8, || {});
    // Nothing asks for it here, and it's not resumed yet.
    let mut next = test_host();
    let entry = owner.get_or_create_entry(7);
    let store = entry.store();
    next.set_retained_values_entry(entry);
    next.set_lifecycle_state(LifecycleState::Created);
    next.set_content(move || LocalRetainedValuesStoreProvider(store.clone(), || {}));
    next.set_lifecycle_state(LifecycleState::Resumed);
    other.frame();
    assert_eq!(retired.get(), 0, "another host's frame");
    next.frame();
    assert_eq!(retired.get(), 1, "its own");
    owner.clear();
}

/// The holder cleared under a host still composing: what's kept is retired, what leaves too, and a
/// root recomposition isn't a panic.
#[test]
fn a_host_goes_on_after_its_holder_is_cleared() {
    let owner = LifecycleRetainedValuesStoreOwner::new();
    let (retired, made) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let tick = mutable_state_of(0);
    let t = tick.handle();
    let inner = retaining(&retired, &made);
    let mut ui = host(&owner, 7, move || {
        t.get();
        inner();
    });
    owner.clear();
    tick.set(1);
    ui.frame();
    assert_eq!(made.get(), 1);
    drop(ui);
    assert_eq!(retired.get(), 1);
}

/// The holder cleared: what's kept goes, and a host stopped under it keeps nothing more.
#[test]
fn clearing_the_holder_retires_what_it_keeps() {
    let owner = LifecycleRetainedValuesStoreOwner::new();
    let (retired, made) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let kept = host(&owner, 7, retaining(&retired, &made));
    kept.set_lifecycle_state(LifecycleState::Created);
    drop(kept);
    assert_eq!(retired.get(), 0);
    let still = host(&owner, 8, retaining(&retired, &made));
    still.set_lifecycle_state(LifecycleState::Created);
    owner.clear();
    assert_eq!(retired.get(), 1);
    drop(still);
    assert_eq!(retired.get(), 2);
}
