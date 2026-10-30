//! The lifecycle effects against AndroidX's `LifecycleEffectTest`.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use LifecycleEvent::*;
use LifecycleState::*;
use compose_lifecycle::*;
use compose_runtime::*;
use compose_test::*;

fn counter() -> Rc<Cell<u32>> {
    Rc::default()
}

fn bump(count: &Rc<Cell<u32>>) {
    count.set(count.get() + 1);
}

/// Provides `owner` as the local lifecycle.
fn with_owner(owner: &TestLifecycleOwner, content: impl FnOnce()) {
    CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(owner.lifecycle()), content);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleEventEffectTest_noEvent
#[test]
fn an_event_effect_does_nothing_without_its_event() {
    let owner = TestLifecycleOwner::default();
    let stops = counter();
    let (seen, held) = (stops.clone(), owner.clone());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, || LifecycleEventEffect!(OnStop, on_event = move || bump(&seen)));
    });
    ui.wait_for_idle();
    assert_eq!(stops.get(), 0);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleEventEffectTest_localLifecycleOwner
#[test]
fn an_event_effect_hears_the_local_lifecycle() {
    let owner = TestLifecycleOwner::default();
    let stops = counter();
    let (seen, held) = (stops.clone(), owner.clone());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, || LifecycleEventEffect!(OnStop, on_event = move || bump(&seen)));
    });
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnStop);
    assert_eq!(stops.get(), 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleEventEffectTest_customLifecycleOwner
#[test]
fn an_event_effect_hears_the_lifecycle_it_is_given() {
    let owner = TestLifecycleOwner::default();
    let stops = counter();
    let (seen, held) = (stops.clone(), owner.lifecycle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        LifecycleEventEffect!(OnStop, held.clone(), on_event = move || bump(&seen));
    });
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnStop);
    assert_eq!(stops.get(), 1);
}

#[test]
#[should_panic(expected = "LifecycleEventEffect cannot be used to listen for")]
fn an_event_effect_refuses_on_destroy() {
    let _ui = compose_test(|| LifecycleEventEffect!(OnDestroy, on_event = || {}));
}

/// An event effect on pause shown while `visible`; the steps run; how many pauses it heard.
fn pause_effect_after(steps: impl FnOnce(&TestLifecycleOwner, &MutableState<bool>)) -> u32 {
    let owner = TestLifecycleOwner::new(Initialized);
    let visible = mutable_state_of(true);
    let stops = counter();
    let (seen, held, read) = (stops.clone(), owner.lifecycle(), visible.handle());
    let mut ui = compose_test(move || {
        if read.get() {
            let seen = seen.clone();
            LifecycleEventEffect!(OnPause, held.clone(), on_event = move || bump(&seen));
        }
    });
    ui.wait_for_idle();
    steps(&owner, &visible.handle());
    ui.wait_for_idle();
    stops.get()
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleEventEffectTest_onPause_beforeOnDispose_isIdempotent
#[test]
fn an_event_effect_hears_a_pause_once_then_disposal() {
    let stops = pause_effect_after(|owner, visible| {
        owner.handle_lifecycle_event(OnResume);
        owner.handle_lifecycle_event(OnPause);
        visible.set(false);
    });
    assert_eq!(stops, 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleEventEffectTest_onDestroy_beforeOnDispose_isIdempotent
#[test]
fn an_event_effect_hears_a_pause_once_on_destruction_then_disposal() {
    let stops = pause_effect_after(|owner, visible| {
        owner.handle_lifecycle_event(OnResume);
        owner.handle_lifecycle_event(OnDestroy);
        visible.set(false);
    });
    assert_eq!(stops, 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleEventEffectTest_onDispose_beforeOnDestroy_isIdempotent
#[test]
fn an_event_effect_hears_a_pause_once_with_disposal_then_destruction() {
    let stops = pause_effect_after(|owner, visible| {
        owner.handle_lifecycle_event(OnResume);
        visible.set(false);
        owner.handle_lifecycle_event(OnDestroy);
    });
    assert_eq!(stops, 1);
}

// ---- LifecycleStartEffect -------------------------------------------------------------------

/// LifecycleStartEffect counting its starts, and on its end its stops or, still at `Started`, disposals.
#[derive(Clone, Default)]
struct LifecycleStartEffectCounts {
    ups: Rc<Cell<u32>>,
    downs: Rc<Cell<u32>>,
    disposals: Rc<Cell<u32>>,
}

impl LifecycleStartEffectCounts {
    fn effect<K: ComposeArg + Clone + 'static>(&self, key: K) {
        let counts = self.clone();
        LifecycleStartEffect!(
            key,
            effects = move |scope: LifecycleStartStopEffectScope| {
                bump(&counts.ups);
                let counts = counts.clone();
                scope.on_stop_or_dispose(move |lifecycle| {
                    if lifecycle.current_state().is_at_least(Started) {
                        bump(&counts.disposals);
                    } else {
                        bump(&counts.downs);
                    }
                })
            }
        );
    }

    fn get(&self) -> (u32, u32, u32) {
        (self.ups.get(), self.downs.get(), self.disposals.get())
    }
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffectTest
#[test]
fn a_start_effect_runs_on_start_and_ends_on_stop() {
    let owner = TestLifecycleOwner::new(Initialized);
    let counts = LifecycleStartEffectCounts::default();
    let (seen, held) = (counts.clone(), owner.clone());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || seen.effect(0));
    });
    ui.wait_for_idle();
    assert_eq!(counts.get(), (0, 0, 0));
    owner.handle_lifecycle_event(OnStart);
    assert_eq!(counts.get(), (1, 0, 0));
    owner.handle_lifecycle_event(OnStop);
    assert_eq!(counts.get(), (1, 1, 0));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffectTest_disposal
#[test]
fn a_start_effect_ends_when_disposed() {
    let owner = TestLifecycleOwner::new(Initialized);
    let counts = LifecycleStartEffectCounts::default();
    let show = mutable_state_of(true);
    let (seen, held, read) = (counts.clone(), owner.clone(), show.handle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || {
            if read.get() {
                seen.effect(0);
            }
        });
    });
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnStart);
    assert_eq!(counts.get(), (1, 0, 0));
    show.set(false);
    ui.wait_for_idle();
    assert_eq!(counts.get(), (1, 0, 1));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffectTest_disposal_onKeyChange
#[test]
fn a_start_effect_ends_and_runs_again_on_a_new_key() {
    let owner = TestLifecycleOwner::new(Initialized);
    let counts = LifecycleStartEffectCounts::default();
    let key = mutable_state_of(true);
    let (seen, held, read) = (counts.clone(), owner.clone(), key.handle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || seen.effect(read.get()));
    });
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnStart);
    assert_eq!(counts.get(), (1, 0, 0));
    key.set(false);
    ui.wait_for_idle();
    assert_eq!(counts.get(), (2, 0, 1));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffectTest_disposal_onLifecycleOwner
#[test]
fn a_start_effect_moves_to_a_new_lifecycle() {
    let first = TestLifecycleOwner::new(Resumed);
    let second = TestLifecycleOwner::new(Resumed);
    let use_first = mutable_state_of(true);
    let started: Rc<RefCell<Vec<Lifecycle>>> = Rc::default();
    let ended: Rc<RefCell<Vec<Lifecycle>>> = Rc::default();
    let (ups, downs, read) = (started.clone(), ended.clone(), use_first.handle());
    let (one, two) = (first.lifecycle(), second.lifecycle());
    let mut ui = compose_test(move || {
        let (ups, downs) = (ups.clone(), downs.clone());
        let owner = if read.get() { one.clone() } else { two.clone() };
        CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(owner), move || {
            LifecycleStartEffect!(
                0,
                effects = move |scope: LifecycleStartStopEffectScope| {
                    ups.borrow_mut().push(scope.lifecycle().clone());
                    let downs = downs.clone();
                    scope.on_stop_or_dispose(move |lifecycle| {
                        downs.borrow_mut().push(lifecycle.clone())
                    })
                }
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(*started.borrow(), [first.lifecycle()]);
    use_first.set(false);
    ui.wait_for_idle();
    assert_eq!(*ended.borrow(), [first.lifecycle()]);
    assert_eq!(*started.borrow(), [first.lifecycle(), second.lifecycle()]);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffectTest_effectsLambdaUpdate
#[test]
fn a_start_effect_reads_what_is_current_when_it_runs() {
    let owner = TestLifecycleOwner::new(Initialized);
    let state = mutable_state_of(String::from("default"));
    let (held, written) = (owner.clone(), state.handle());
    let mut ui = compose_test(move || {
        with_owner(&held, move || {
            LifecycleStartEffect!(
                0,
                effects = move |scope: LifecycleStartStopEffectScope| {
                    written.set(written.get() + " started");
                    scope.on_stop_or_dispose(move |_| written.set(written.get() + " disposed"))
                }
            );
        });
    });
    ui.wait_for_idle();
    state.set(String::from("updated"));
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnStart);
    owner.handle_lifecycle_event(OnCreate);
    assert_eq!(state.get(), "updated started disposed");
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffectTest_effectsLambdaAndKeyChange
#[test]
fn a_start_effect_uses_its_latest_closure_after_a_new_key() {
    let owner = TestLifecycleOwner::new(Initialized);
    let state = mutable_state_of(String::from("default"));
    let other = mutable_state_of(String::from("changed"));
    let use_other = mutable_state_of(false);
    let (held, first, second, read) =
        (owner.clone(), state.handle(), other.handle(), use_other.handle());
    let mut ui = compose_test(move || {
        let using_other = read.get();
        let written = if using_other { second } else { first };
        with_owner(&held, move || {
            LifecycleStartEffect!(
                using_other,
                effects = move |scope: LifecycleStartStopEffectScope| {
                    written.set(written.get() + " started");
                    scope.on_stop_or_dispose(move |_| written.set(written.get() + " disposed"))
                }
            );
        });
    });
    ui.wait_for_idle();
    use_other.set(true);
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnStart);
    owner.handle_lifecycle_event(OnCreate);
    assert_eq!(other.get(), "changed started disposed");
    assert_eq!(state.get(), "default");
}

/// A start effect shown while `visible`; the steps run; how many ends it ran.
fn start_effect_ends_after(steps: impl FnOnce(&TestLifecycleOwner, &MutableState<bool>)) -> u32 {
    let owner = TestLifecycleOwner::new(Initialized);
    let visible = mutable_state_of(true);
    let ends = counter();
    let (seen, held, read) = (ends.clone(), owner.clone(), visible.handle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || {
            if read.get() {
                LifecycleStartEffect!(
                    "key1",
                    effects = move |scope: LifecycleStartStopEffectScope| {
                        let seen = seen.clone();
                        scope.on_stop_or_dispose(move |_| bump(&seen))
                    }
                );
            }
        });
    });
    ui.wait_for_idle();
    steps(&owner, &visible.handle());
    ui.wait_for_idle();
    ends.get()
}

// ---- LifecycleResumeEffect -------------------------------------------------------------------

/// LifecycleResumeEffect counting its resumes, and on its end its pauses or, still at `Resumed`, disposals.
#[derive(Clone, Default)]
struct LifecycleResumeEffectCounts {
    ups: Rc<Cell<u32>>,
    downs: Rc<Cell<u32>>,
    disposals: Rc<Cell<u32>>,
}

impl LifecycleResumeEffectCounts {
    fn effect<K: ComposeArg + Clone + 'static>(&self, key: K) {
        let counts = self.clone();
        LifecycleResumeEffect!(
            key,
            effects = move |scope: LifecycleResumePauseEffectScope| {
                bump(&counts.ups);
                let counts = counts.clone();
                scope.on_pause_or_dispose(move |lifecycle| {
                    if lifecycle.current_state().is_at_least(Resumed) {
                        bump(&counts.disposals);
                    } else {
                        bump(&counts.downs);
                    }
                })
            }
        );
    }

    fn get(&self) -> (u32, u32, u32) {
        (self.ups.get(), self.downs.get(), self.disposals.get())
    }
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffectTest
#[test]
fn a_resume_effect_runs_on_resume_and_ends_on_pause() {
    let owner = TestLifecycleOwner::new(Initialized);
    let counts = LifecycleResumeEffectCounts::default();
    let (seen, held) = (counts.clone(), owner.clone());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || seen.effect(0));
    });
    ui.wait_for_idle();
    assert_eq!(counts.get(), (0, 0, 0));
    owner.handle_lifecycle_event(OnResume);
    assert_eq!(counts.get(), (1, 0, 0));
    owner.handle_lifecycle_event(OnPause);
    assert_eq!(counts.get(), (1, 1, 0));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffectTest_disposal
#[test]
fn a_resume_effect_ends_when_disposed() {
    let owner = TestLifecycleOwner::new(Initialized);
    let counts = LifecycleResumeEffectCounts::default();
    let show = mutable_state_of(true);
    let (seen, held, read) = (counts.clone(), owner.clone(), show.handle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || {
            if read.get() {
                seen.effect(0);
            }
        });
    });
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnResume);
    assert_eq!(counts.get(), (1, 0, 0));
    show.set(false);
    ui.wait_for_idle();
    assert_eq!(counts.get(), (1, 0, 1));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffectTest_disposal_onKeyChange
#[test]
fn a_resume_effect_ends_and_runs_again_on_a_new_key() {
    let owner = TestLifecycleOwner::new(Initialized);
    let counts = LifecycleResumeEffectCounts::default();
    let key = mutable_state_of(true);
    let (seen, held, read) = (counts.clone(), owner.clone(), key.handle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || seen.effect(read.get()));
    });
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnResume);
    assert_eq!(counts.get(), (1, 0, 0));
    key.set(false);
    ui.wait_for_idle();
    assert_eq!(counts.get(), (2, 0, 1));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffectTest_disposal_onLifecycleOwnerChange
#[test]
fn a_resume_effect_moves_to_a_new_lifecycle() {
    let first = TestLifecycleOwner::new(Resumed);
    let second = TestLifecycleOwner::new(Resumed);
    let use_first = mutable_state_of(true);
    let started: Rc<RefCell<Vec<Lifecycle>>> = Rc::default();
    let ended: Rc<RefCell<Vec<Lifecycle>>> = Rc::default();
    let (ups, downs, read) = (started.clone(), ended.clone(), use_first.handle());
    let (one, two) = (first.lifecycle(), second.lifecycle());
    let mut ui = compose_test(move || {
        let (ups, downs) = (ups.clone(), downs.clone());
        let owner = if read.get() { one.clone() } else { two.clone() };
        CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(owner), move || {
            LifecycleResumeEffect!(
                0,
                effects = move |scope: LifecycleResumePauseEffectScope| {
                    ups.borrow_mut().push(scope.lifecycle().clone());
                    let downs = downs.clone();
                    scope.on_pause_or_dispose(move |lifecycle| {
                        downs.borrow_mut().push(lifecycle.clone())
                    })
                }
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(*started.borrow(), [first.lifecycle()]);
    use_first.set(false);
    ui.wait_for_idle();
    assert_eq!(*ended.borrow(), [first.lifecycle()]);
    assert_eq!(*started.borrow(), [first.lifecycle(), second.lifecycle()]);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffectTest_effectsLambdaUpdate
#[test]
fn a_resume_effect_reads_what_is_current_when_it_runs() {
    let owner = TestLifecycleOwner::new(Initialized);
    let state = mutable_state_of(String::from("default"));
    let (held, written) = (owner.clone(), state.handle());
    let mut ui = compose_test(move || {
        with_owner(&held, move || {
            LifecycleResumeEffect!(
                0,
                effects = move |scope: LifecycleResumePauseEffectScope| {
                    written.set(written.get() + " started");
                    scope.on_pause_or_dispose(move |_| written.set(written.get() + " disposed"))
                }
            );
        });
    });
    ui.wait_for_idle();
    state.set(String::from("updated"));
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnResume);
    owner.handle_lifecycle_event(OnStart);
    assert_eq!(state.get(), "updated started disposed");
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffectTest_effectsLambdaAndKeyChange
#[test]
fn a_resume_effect_uses_its_latest_closure_after_a_new_key() {
    let owner = TestLifecycleOwner::new(Initialized);
    let state = mutable_state_of(String::from("default"));
    let other = mutable_state_of(String::from("changed"));
    let use_other = mutable_state_of(false);
    let (held, first, second, read) =
        (owner.clone(), state.handle(), other.handle(), use_other.handle());
    let mut ui = compose_test(move || {
        let using_other = read.get();
        let written = if using_other { second } else { first };
        with_owner(&held, move || {
            LifecycleResumeEffect!(
                using_other,
                effects = move |scope: LifecycleResumePauseEffectScope| {
                    written.set(written.get() + " started");
                    scope.on_pause_or_dispose(move |_| written.set(written.get() + " disposed"))
                }
            );
        });
    });
    ui.wait_for_idle();
    use_other.set(true);
    ui.wait_for_idle();
    owner.handle_lifecycle_event(OnResume);
    owner.handle_lifecycle_event(OnStart);
    assert_eq!(other.get(), "changed started disposed");
    assert_eq!(state.get(), "default");
}

/// A resume effect shown while `visible`; the steps run; how many ends it ran.
fn resume_effect_ends_after(steps: impl FnOnce(&TestLifecycleOwner, &MutableState<bool>)) -> u32 {
    let owner = TestLifecycleOwner::new(Initialized);
    let visible = mutable_state_of(true);
    let ends = counter();
    let (seen, held, read) = (ends.clone(), owner.clone(), visible.handle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        with_owner(&held, move || {
            if read.get() {
                LifecycleResumeEffect!(
                    "key1",
                    effects = move |scope: LifecycleResumePauseEffectScope| {
                        let seen = seen.clone();
                        scope.on_pause_or_dispose(move |_| bump(&seen))
                    }
                );
            }
        });
    });
    ui.wait_for_idle();
    steps(&owner, &visible.handle());
    ui.wait_for_idle();
    ends.get()
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffect_onStop_beforeOnDispose_isIdempotent
#[test]
fn a_start_effect_ends_once_on_stop_then_disposal() {
    let ends = start_effect_ends_after(|owner, visible| {
        owner.handle_lifecycle_event(OnStart);
        owner.handle_lifecycle_event(OnStop);
        visible.set(false);
    });
    assert_eq!(ends, 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffect_onDestroy_beforeOnDispose_isIdempotent
#[test]
fn a_start_effect_ends_once_on_destruction_then_disposal() {
    let ends = start_effect_ends_after(|owner, visible| {
        owner.handle_lifecycle_event(OnStart);
        owner.handle_lifecycle_event(OnDestroy);
        visible.set(false);
    });
    assert_eq!(ends, 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleStartEffect_onDispose_beforeOnDestroy_isIdempotent
#[test]
fn a_start_effect_ends_once_on_disposal_then_destruction() {
    let ends = start_effect_ends_after(|owner, visible| {
        owner.handle_lifecycle_event(OnStart);
        visible.set(false);
        owner.handle_lifecycle_event(OnDestroy);
    });
    assert_eq!(ends, 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffect_onPause_beforeOnDispose_isIdempotent
#[test]
fn a_resume_effect_ends_once_on_pause_then_disposal() {
    let ends = resume_effect_ends_after(|owner, visible| {
        owner.handle_lifecycle_event(OnResume);
        owner.handle_lifecycle_event(OnPause);
        visible.set(false);
    });
    assert_eq!(ends, 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffect_onDispose_beforeOnPause_isIdempotent
#[test]
fn a_resume_effect_ends_once_on_disposal_then_pause() {
    let ends = resume_effect_ends_after(|owner, visible| {
        owner.handle_lifecycle_event(OnResume);
        visible.set(false);
        owner.handle_lifecycle_event(OnPause);
    });
    assert_eq!(ends, 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffect_onDestroy_beforeOnDispose_isIdempotent
#[test]
fn a_resume_effect_ends_once_on_destruction_then_disposal() {
    let ends = resume_effect_ends_after(|owner, visible| {
        owner.handle_lifecycle_event(OnResume);
        owner.handle_lifecycle_event(OnDestroy);
        visible.set(false);
    });
    assert_eq!(ends, 1);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/LifecycleEffectTest.kt, lifecycleResumeEffect_onDispose_beforeOnDestroy_isIdempotent
#[test]
fn a_resume_effect_ends_once_on_disposal_then_destruction() {
    let ends = resume_effect_ends_after(|owner, visible| {
        owner.handle_lifecycle_event(OnResume);
        visible.set(false);
        owner.handle_lifecycle_event(OnDestroy);
    });
    assert_eq!(ends, 1);
}
