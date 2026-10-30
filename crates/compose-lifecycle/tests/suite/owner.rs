//! `remember_lifecycle_owner`, `LOCAL_LIFECYCLE_OWNER` and the host's lifecycle, against AndroidX's
//! `LifecycleOwnerTest` and `LifecycleExtTest`.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use LifecycleState::*;
use compose_lifecycle::*;
use compose_runtime::*;
use compose_test::*;

type Held = Rc<RefCell<Option<Lifecycle>>>;

fn held() -> Held {
    Rc::default()
}

fn state_of(child: &Held) -> LifecycleState {
    child.borrow().as_ref().unwrap().current_state()
}

/// A rule of a child of `parent`, held in the result.
fn child_of(parent: &TestLifecycleOwner) -> (ComposeTestRule, Held) {
    let (child, seen) = (held(), parent.lifecycle());
    let shared = child.clone();
    let ui = compose_test(move || {
        *shared.borrow_mut() = Some(remember_lifecycle_owner!(parent = Some(seen.clone())));
    });
    (ui, child)
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenComposed_thenReturnsNewLifecycleOwner
#[test]
fn a_remembered_owner_is_a_new_one_following_its_parent() {
    let parent = TestLifecycleOwner::default();
    let (mut ui, child) = child_of(&parent);
    ui.wait_for_idle();
    assert_ne!(child.borrow().clone().unwrap(), parent.lifecycle());
    assert_eq!(state_of(&child), Started);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenParentLifecycleChanges_thenChildLifecycleFollows
#[test]
fn a_remembered_owner_follows_its_parent() {
    let parent = TestLifecycleOwner::default();
    let (mut ui, child) = child_of(&parent);
    ui.wait_for_idle();
    parent.set_current_state(Created);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Created);
    parent.set_current_state(Resumed);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Resumed);
    parent.set_current_state(Destroyed);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Destroyed);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenParentStateExceedsMaxLifecycle_thenChildStateIsCapped
#[test]
fn a_remembered_owner_is_capped_by_its_max_lifecycle() {
    let parent = TestLifecycleOwner::new(Resumed);
    let (child, seen) = (held(), parent.lifecycle());
    let shared = child.clone();
    let mut ui = compose_test(move || {
        *shared.borrow_mut() =
            Some(remember_lifecycle_owner!(max_lifecycle = Started, parent = Some(seen.clone())));
    });
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Started);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenMaxLifecycleParameterChanges_thenChildStateUpdates
#[test]
fn a_new_max_lifecycle_moves_a_remembered_owner() {
    let parent = TestLifecycleOwner::new(Resumed);
    let (child, seen) = (held(), parent.lifecycle());
    let shared = child.clone();
    let max = mutable_state_of(Started);
    let read = max.handle();
    let mut ui = compose_test(move || {
        *shared.borrow_mut() = Some(remember_lifecycle_owner!(
            max_lifecycle = read.get(),
            parent = Some(seen.clone())
        ));
    });
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Started);
    max.set(Resumed);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Resumed);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenDisposed_thenObserverIsRemoved
#[test]
fn a_remembered_owner_disposed_stops_observing_its_parent() {
    let parent = TestLifecycleOwner::default();
    assert_eq!(parent.observer_count(), 0);
    let show = mutable_state_of(true);
    let (read, seen) = (show.handle(), parent.lifecycle());
    let mut ui = compose_test(move || {
        if read.get() {
            remember_lifecycle_owner!(parent = Some(seen.clone()));
        }
    });
    ui.wait_for_idle();
    assert_eq!(parent.observer_count(), 1);
    show.set(false);
    ui.wait_for_idle();
    assert_eq!(parent.observer_count(), 0);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenDisposed_thenChildIsDestroyed
#[test]
fn a_remembered_owner_disposed_is_destroyed_not_its_parent() {
    let parent = TestLifecycleOwner::new(Resumed);
    let show = mutable_state_of(true);
    let (child, read, seen) = (held(), show.handle(), parent.lifecycle());
    let shared = child.clone();
    let mut ui = compose_test(move || {
        if read.get() {
            *shared.borrow_mut() = Some(remember_lifecycle_owner!(parent = Some(seen.clone())));
        }
    });
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Resumed);
    show.set(false);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Destroyed);
    assert_eq!(parent.current_state(), Resumed);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenParentIsNull_thenStartsItself
#[test]
fn a_remembered_owner_without_a_parent_resumes_itself() {
    let child = held();
    let shared = child.clone();
    let mut ui = compose_test(move || {
        *shared.borrow_mut() = Some(remember_lifecycle_owner!(parent = None));
    });
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Resumed);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenParentIsNullAndMaxLifecycle_thenIsCapped
#[test]
fn a_remembered_owner_without_a_parent_is_capped_too() {
    let child = held();
    let shared = child.clone();
    let mut ui = compose_test(move || {
        *shared.borrow_mut() =
            Some(remember_lifecycle_owner!(max_lifecycle = Started, parent = None));
    });
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Started);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenParentIsNull_thenDestroyedOnDispose
#[test]
fn a_remembered_owner_without_a_parent_is_destroyed_on_disposal() {
    let show = mutable_state_of(true);
    let (child, read) = (held(), show.handle());
    let shared = child.clone();
    let mut ui = compose_test(move || {
        if read.get() {
            *shared.borrow_mut() = Some(remember_lifecycle_owner!(parent = None));
        }
    });
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Resumed);
    show.set(false);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Destroyed);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_withDefaultParent_followsLocalLifecycle
#[test]
fn a_remembered_owner_follows_the_local_lifecycle_by_default() {
    let parent = TestLifecycleOwner::new(Created);
    let (child, seen) = (held(), parent.lifecycle());
    let shared = child.clone();
    let mut ui = compose_test(move || {
        let shared = shared.clone();
        CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(seen.clone()), move || {
            *shared.borrow_mut() = Some(remember_lifecycle_owner!());
        });
    });
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Created);
    parent.set_current_state(Resumed);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Resumed);
    parent.set_current_state(Destroyed);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Destroyed);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleOwnerTest.kt, rememberLifecycleOwner_whenOnDestroyWhileInitialized_ignoresEvent
#[test]
fn a_remembered_owner_still_initialized_ignores_its_disposal() {
    let show = mutable_state_of(true);
    let (child, read) = (held(), show.handle());
    let shared = child.clone();
    let mut ui = compose_test(move || {
        if read.get() {
            *shared.borrow_mut() =
                Some(remember_lifecycle_owner!(max_lifecycle = Initialized, parent = None));
        }
    });
    show.set(false);
    ui.wait_for_idle();
    assert_eq!(state_of(&child), Initialized);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleExtTest.kt, lifecycleCollectAsState
#[test]
fn a_lifecycle_s_state_can_be_read_as_a_state() {
    let owner = TestLifecycleOwner::new(Initialized);
    let lifecycle = owner.lifecycle();
    assert_eq!(lifecycle.current_state(), Initialized);
    let seen = Rc::new(Cell::new(None));
    let shared = seen.clone();
    let mut ui = compose_test(move || shared.set(Some(lifecycle.as_state().get())));
    ui.wait_for_idle();
    assert_eq!(seen.get(), Some(Initialized));
    owner.set_current_state(Resumed);
    ui.wait_for_idle();
    assert_eq!(seen.get(), Some(Resumed));
}

// ---- The host's lifecycle --------------------------------------------------------------------

#[test]
fn a_host_provides_a_lifecycle_resumed_it_can_move() {
    let local = held();
    let shared = local.clone();
    let mut ui = compose_test(move || {
        *shared.borrow_mut() = Some(LOCAL_LIFECYCLE_OWNER.current());
    });
    ui.wait_for_idle();
    assert_eq!(local.borrow().clone().unwrap(), ui.host().lifecycle());
    assert_eq!(state_of(&local), Resumed);
    ui.host().set_lifecycle_state(Started);
    assert_eq!(state_of(&local), Started);
    // A host can't go back to Initialized, what a platform's owner may say at first.
    ui.host().set_lifecycle_state(Initialized);
    assert_eq!(state_of(&local), Created);
}

#[test]
fn a_host_s_lifecycle_is_destroyed_after_its_composition_is_disposed() {
    let order = Rc::new(RefCell::new(Vec::new()));
    let (disposed, destroyed) = (order.clone(), order.clone());
    let mut ui = compose_test(move || {
        let disposed = disposed.clone();
        let destroyed = destroyed.clone();
        DisposableEffect(0, move |scope| {
            scope.on_dispose(move || disposed.borrow_mut().push("composition disposed"))
        });
        let _ = remember(move || {
            LOCAL_LIFECYCLE_OWNER.current().add_observer_fn(move |_, event| {
                if event == LifecycleEvent::OnDestroy {
                    destroyed.borrow_mut().push("destroyed");
                }
            })
        });
    });
    ui.wait_for_idle();
    let lifecycle = ui.host().lifecycle();
    struct Model {
        base: ViewModelBase,
        order: Rc<RefCell<Vec<&'static str>>>,
    }
    impl ViewModel for Model {
        fn base(&self) -> &ViewModelBase {
            &self.base
        }
        fn on_cleared(&self) {
            self.order.borrow_mut().push("view models cleared");
        }
    }
    ui.host()
        .view_model_store()
        .put("model", Rc::new(Model { base: ViewModelBase::new(), order: order.clone() }));
    drop(ui);
    assert_eq!(*order.borrow(), ["composition disposed", "destroyed", "view models cleared"]);
    assert_eq!(lifecycle.current_state(), Destroyed);
    // Once destroyed, nothing moves it (a platform's late event).
}

#[test]
fn a_host_s_late_state_after_destruction_changes_nothing() {
    let mut ui = compose_test(|| {});
    ui.host().set_lifecycle_state(Destroyed);
    ui.host().set_lifecycle_state(Resumed);
    assert_eq!(ui.host().lifecycle().current_state(), Destroyed);
}
