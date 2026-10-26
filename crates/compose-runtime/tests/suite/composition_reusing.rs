//! Compose's `CompositionReusingTests`, adapted: `ReusableContent`, `ReusableContentHost` and
//! the node lifecycle (reuse, deactivation, release) under them, on the mock views. A reused
//! node is reset as the pass reaches it, not after the pass (`onReuseIsCalledInApplyStage`):
//! compose-rs applies nodes during the pass (D31).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReuseIsCalledBeforeSetter
#[test]
fn a_reused_node_is_reset_before_its_setters_run() {
    #[composable]
    fn Content(reuse_key: MutableState<i32>, on_reuse: Action, on_set: Action) {
        ReusableContent(reuse_key.get(), || {
            LinearWithCallbacks(on_reuse.clone(), nothing(), nothing(), on_set.clone(), || {});
        });
    }
    let mut test = composition_test();
    let reuse_key = mutable_state_of(0);
    let reuse_calls = Counter::new();
    let calls_when_set = Log::new();
    let on_reuse: Action = {
        let calls = reuse_calls.clone();
        Rc::new(move || {
            calls.increment();
        })
    };
    let on_set: Action = {
        let (calls, log) = (reuse_calls.clone(), calls_when_set.clone());
        Rc::new(move || log.push(calls.get().to_string()))
    };
    let k = reuse_key.handle();
    test.compose(move || Content(k, on_reuse.clone(), on_set.clone()));
    test.validate(|v| v.empty_linear());
    assert_eq!(calls_when_set.take(), ["0"]);
    reuse_key.set(1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(calls_when_set.take(), ["1"], "reset (onReuse) before the setter ran");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onDeactivateIsCalledWhenReusableContentDeactivated
#[test]
fn deactivated_content_deactivates_its_nodes() {
    #[composable]
    fn Content(active: MutableState<bool>, on_deactivate: Action) {
        ReusableContentHost(active.get(), || {
            ReusableContent(0, || {
                LinearWithCallbacks(nothing(), on_deactivate.clone(), nothing(), nothing(), || {});
            });
        });
    }
    let mut test = composition_test();
    let active = mutable_state_of(true);
    let deactivate_calls = Counter::new();
    let on_deactivate: Action = {
        let calls = deactivate_calls.clone();
        Rc::new(move || {
            calls.increment();
        })
    };
    let a = active.handle();
    test.compose(move || Content(a, on_deactivate.clone()));
    test.validate(|v| v.empty_linear());
    assert_eq!(deactivate_calls.get(), 0);
    active.set(false);
    test.expect_changes();
    test.revalidate();
    assert_eq!(deactivate_calls.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReuseIsCalledBeforeSetterAfterDeactivation
#[test]
fn a_node_reactivated_is_reset_before_its_setters_run() {
    #[composable]
    fn Content(active: MutableState<bool>, on_reuse: Action, on_set: Action) {
        ReusableContentHost(active.get(), || {
            ReusableContent(0, || {
                LinearWithCallbacks(on_reuse.clone(), nothing(), nothing(), on_set.clone(), || {});
            });
        });
    }
    let mut test = composition_test();
    let active = mutable_state_of(true);
    let reuse_calls = Counter::new();
    let calls_when_set = Log::new();
    let on_reuse: Action = {
        let calls = reuse_calls.clone();
        Rc::new(move || {
            calls.increment();
        })
    };
    let on_set: Action = {
        let (calls, log) = (reuse_calls.clone(), calls_when_set.clone());
        Rc::new(move || log.push(calls.get().to_string()))
    };
    let a = active.handle();
    test.compose(move || Content(a, on_reuse.clone(), on_set.clone()));
    test.validate(|v| v.empty_linear());
    active.set(false);
    test.expect_changes();
    test.revalidate();
    active.set(true);
    test.expect_changes();
    test.revalidate();
    assert_eq!(calls_when_set.entries(), ["0", "1"]);
}

/// A remember observer shared by the passes, counting its callbacks, as the tests'
/// `rememberedState` objects.
#[derive(Clone, Default)]
struct RememberedState(Rc<RememberedCounts>);

#[derive(Default)]
struct RememberedCounts {
    remembered: Cell<u32>,
    forgotten: Cell<u32>,
    abandoned: Cell<u32>,
    currently_remembered: Cell<bool>,
}

impl RememberedState {
    fn counts(&self) -> (u32, u32, u32) {
        (self.0.remembered.get(), self.0.forgotten.get(), self.0.abandoned.get())
    }

    fn currently_remembered(&self) -> bool {
        self.0.currently_remembered.get()
    }
}

impl ComposeArg for RememberedState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl RememberObserver for RememberedState {
    fn on_remembered(&self) {
        self.0.remembered.set(self.0.remembered.get() + 1);
        self.0.currently_remembered.set(true);
    }
    fn on_forgotten(&self) {
        self.0.forgotten.set(self.0.forgotten.get() + 1);
        self.0.currently_remembered.set(false);
    }
    fn on_abandoned(&self) {
        self.0.abandoned.set(self.0.abandoned.get() + 1);
        self.0.currently_remembered.set(false);
    }
}

/// An action counting its calls.
fn counting(calls: &Counter) -> Action {
    let calls = calls.clone();
    Rc::new(move || {
        calls.increment();
    })
}

/// An action logging whether `flag` was set when it was called.
fn logging(flag: &Rc<Cell<bool>>, log: &Log) -> Action {
    let (flag, log) = (flag.clone(), log.clone());
    Rc::new(move || log.push(flag.get().to_string()))
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, canReuse
#[test]
fn content_with_another_key_reuses_the_nodes_not_the_remembered_values() {
    #[composable]
    fn Content(key: MutableState<i32>, last_data: Rc<Cell<Option<MutableState<i32>>>>) {
        let current = key.get();
        ReusableContent(current, || {
            Linear!({
                let data = remember(|| mutable_state_of(1));
                last_data.set(Some(data));
                Text(&format!("Key = {current}"));
            });
        });
    }
    let mut test = composition_test();
    let key = mutable_state_of(0);
    let last_data = Rc::new(Cell::new(None));
    let (k, l) = (key.handle(), last_data.clone());
    test.compose(move || Content(k, l.clone()));
    let k = key.handle();
    test.validate(move |v| v.linear(|v| v.text(&format!("Key = {}", k.get()))));
    let first_data = last_data.get().expect("composed");
    let nodes = test.flatten();
    key.set(1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(test.flatten(), nodes, "the nodes are reused");
    assert!(!first_data.same(&last_data.get().unwrap()), "remembered values aren't reused");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, canRecycleAroundNonReusable
#[test]
fn reuse_replaces_the_nodes_that_are_not_reusable() {
    #[composable]
    fn Content(key: MutableState<i32>) {
        let current = key.get();
        ReusableContent(current, || {
            Linear!({
                Text(&format!("Key = {current}"));
                NonReusableText(format!("Non-recyclable key = {current}"));
            });
        });
    }
    let mut test = composition_test();
    let key = mutable_state_of(0);
    let k = key.handle();
    test.compose(move || Content(k));
    test.validate(move |v| {
        v.linear(|v| {
            v.text(&format!("Key = {}", k.get()));
            v.text(&format!("Non-recyclable key = {}", k.get()));
        })
    });
    let (recycled, not_recycled) =
        (test.find_text_with("Key"), test.find_text_with("Non-recyclable key"));
    key.set(1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(test.find_text_with("Key"), recycled, "the text is recycled");
    assert_ne!(
        test.find_text_with("Non-recyclable key"),
        not_recycled,
        "the other text is replaced"
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, recyclableNodesInNonReusableContainerNotRecycled
#[test]
fn reusable_nodes_in_a_container_that_is_not_reusable_are_replaced() {
    #[composable]
    fn Content(key: MutableState<i32>) {
        let current = key.get();
        ReusableContent(current, || {
            Linear!({
                Linear!({ Text(&format!("Key = {current}")) });
                NonReusableLinear!({ Text(&format!("Non-recyclable key = {current}")) });
                NonReusableLinear!({});
            });
        });
    }
    let mut test = composition_test();
    let key = mutable_state_of(0);
    let k = key.handle();
    test.compose(move || Content(k));
    test.validate(move |v| {
        v.linear(|v| {
            v.linear(|v| v.text(&format!("Key = {}", k.get())));
            v.linear(|v| v.text(&format!("Non-recyclable key = {}", k.get())));
            v.empty_linear();
        })
    });
    let (recycled, not_recycled) =
        (test.find_text_with("Key"), test.find_text_with("Non-recyclable key"));
    key.set(1);
    test.expect_changes();
    test.revalidate();
    test.verify_consistent();
    assert_eq!(test.find_text_with("Key"), recycled, "the text is recycled");
    assert_ne!(
        test.find_text_with("Non-recyclable key"),
        not_recycled,
        "the other text is replaced"
    );
}

#[composable]
fn HashedContent(key: MutableState<i32>, local_value: MutableState<i32>, last_hash: Rc<Cell<u64>>) {
    let current = key.get();
    ReusableContent(current, || {
        Linear!({
            Text(&format!("Key = {current}: {}", local_value.get()));
            last_hash.set(current_composite_key_hash());
        });
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, compositeHashCodeReflectsReusableChanges
#[test]
fn the_composite_hash_reflects_the_reuse_key() {
    let mut test = composition_test();
    let (key, local_value) = (mutable_state_of(0), mutable_state_of(0));
    let last_hash = Rc::new(Cell::new(0));
    let (k, v, h) = (key.handle(), local_value.handle(), last_hash.clone());
    test.compose(move || HashedContent(k, v, h.clone()));
    test.validate(move |e| e.linear(|e| e.text(&format!("Key = {}: {}", k.get(), v.get()))));
    let first = last_hash.get();
    key.set(1);
    test.expect_changes();
    test.revalidate();
    assert_ne!(last_hash.get(), first);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, compositeHashCodeIsConsistent
#[test]
fn the_composite_hash_is_the_same_when_reused_content_recomposes() {
    let mut test = composition_test();
    let (key, local_value) = (mutable_state_of(0), mutable_state_of(0));
    let last_hash = Rc::new(Cell::new(0));
    let (k, v, h) = (key.handle(), local_value.handle(), last_hash.clone());
    test.compose(move || HashedContent(k, v, h.clone()));
    test.validate(move |e| e.linear(|e| e.text(&format!("Key = {}: {}", k.get(), v.get()))));
    let for_key0 = last_hash.get();
    local_value.set(1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(last_hash.get(), for_key0);
    key.set(1);
    test.expect_changes();
    test.revalidate();
    let for_key1 = last_hash.get();
    assert_ne!(for_key0, for_key1);
    local_value.set(2);
    test.expect_changes();
    test.revalidate();
    assert_eq!(last_hash.get(), for_key1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, reusableContentHostCanDeactivate
#[test]
fn a_reusable_content_host_deactivates_and_reactivates_its_content() {
    #[composable]
    fn Content(
        active: MutableState<bool>,
        reuse_key: MutableState<i32>,
        remembered: RememberedState,
    ) {
        ReusableContentHost(active.get(), || {
            Linear!({
                ReusableContent(reuse_key.get(), || {
                    Linear!({
                        let r = remembered.clone();
                        remember_observer(move || r);
                        Text("Some text");
                    });
                });
            });
        });
    }
    let mut test = composition_test();
    let (active, reuse_key) = (mutable_state_of(true), mutable_state_of(0));
    let remembered = RememberedState::default();
    let (a, k, r) = (active.handle(), reuse_key.handle(), remembered.clone());
    test.compose(move || Content(a, k, r.clone()));
    test.validate(|v| v.linear(|v| v.linear(|v| v.text("Some text"))));
    assert!(remembered.currently_remembered());
    active.set(false);
    test.expect_changes();
    test.revalidate();
    assert!(!remembered.currently_remembered());
    active.set(true);
    test.expect_changes();
    test.revalidate();
    assert!(remembered.currently_remembered());
    reuse_key.set(1);
    test.expect_changes();
    test.revalidate();
    assert!(remembered.currently_remembered());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, reusableContentHost_deactivates_whenRecomposedWithOtherContent
#[test]
fn a_reusable_content_host_deactivates_when_content_is_inserted_before_it() {
    #[composable]
    fn Content(
        active: MutableState<bool>,
        reuse_key: MutableState<i32>,
        remembered: RememberedState,
    ) {
        if !active.get() {
            Text("Not active");
        }
        ReusableContentHost(active.get(), || {
            ReusableContent(reuse_key.get(), || {
                Linear!({
                    let r = remembered.clone();
                    remember_observer(move || r);
                    Text("Some text");
                });
            });
        });
    }
    let mut test = composition_test();
    let (active, reuse_key) = (mutable_state_of(true), mutable_state_of(0));
    let remembered = RememberedState::default();
    let (a, k, r) = (active.handle(), reuse_key.handle(), remembered.clone());
    test.compose(move || Content(a, k, r.clone()));
    test.validate(move |v| {
        if !a.get() {
            v.text("Not active");
        }
        v.linear(|v| v.text("Some text"));
    });
    assert!(remembered.currently_remembered());
    active.set(false);
    test.expect_changes();
    test.revalidate();
    assert!(!remembered.currently_remembered());
    active.set(true);
    test.expect_changes();
    test.revalidate();
    assert!(remembered.currently_remembered());
    reuse_key.set(1);
    test.expect_changes();
    test.revalidate();
    assert!(remembered.currently_remembered());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, reusableContentHostCanDisableRecompose
#[test]
fn deactivated_content_does_not_recompose() {
    #[composable]
    fn Content(
        active: MutableState<bool>,
        outer: MutableState<String>,
        name: MutableState<String>,
        remembered: RememberedState,
    ) {
        Text(&outer.get());
        ReusableContentHost(active.get(), || {
            Linear!({
                let r = remembered.clone();
                remember_observer(move || r);
                Text(&format!("Test {}", name.get()));
            });
        });
    }
    let mut test = composition_test();
    let active = mutable_state_of(true);
    let (outer, name) =
        (mutable_state_of("Outer".to_string()), mutable_state_of("Value".to_string()));
    let (a, o, n) = (active.handle(), outer.handle(), name.handle());
    test.compose(move || Content(a, o, n, RememberedState::default()));
    test.validate(move |v| {
        v.text(&o.get());
        v.linear(|v| v.text(&format!("Test {}", n.get())));
    });
    active.set(false);
    test.expect_changes();
    name.set("New value".into());
    // The name isn't observed. The views are out of date with it: no revalidate.
    test.expect_no_changes();
    outer.set("New outer".into());
    test.expect_changes();
    active.set(true);
    test.expect_changes();
    test.revalidate();
    name.set("New new value".into());
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReuseIsCalledWhenReusableContentKeyChanges
#[test]
fn a_node_hears_each_reuse() {
    #[composable]
    fn Content(reuse_key: MutableState<i32>, on_reuse: Action) {
        ReusableContent(reuse_key.get(), || {
            LinearWithCallbacks(on_reuse.clone(), nothing(), nothing(), nothing(), || {});
        });
    }
    let mut test = composition_test();
    let reuse_key = mutable_state_of(0);
    let reuse_calls = Counter::new();
    let (k, on_reuse) = (reuse_key.handle(), counting(&reuse_calls));
    test.compose(move || Content(k, on_reuse.clone()));
    test.validate(|v| v.empty_linear());
    assert_eq!(reuse_calls.get(), 0);
    reuse_key.set(1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(reuse_calls.get(), 1);
    reuse_key.set(2);
    test.expect_changes();
    test.revalidate();
    assert_eq!(reuse_calls.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReuseIsNotCalledWhenDisposed
#[test]
fn a_removed_node_does_not_hear_a_reuse() {
    #[composable]
    fn Content(emit: MutableState<bool>, on_reuse: Action) {
        if emit.get() {
            ReusableContent(0, || {
                LinearWithCallbacks(on_reuse.clone(), nothing(), nothing(), nothing(), || {});
            });
        }
    }
    let mut test = composition_test();
    let emit = mutable_state_of(true);
    let reuse_calls = Counter::new();
    let (e, on_reuse) = (emit.handle(), counting(&reuse_calls));
    test.compose(move || Content(e, on_reuse.clone()));
    emit.set(false);
    test.expect_changes();
    assert_eq!(reuse_calls.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onDeactivateIsCalledInApplyStage
#[test]
fn a_node_hears_its_deactivation_after_the_composition() {
    #[composable]
    fn Content(active: MutableState<bool>, on_deactivate: Action, finished: Rc<Cell<bool>>) {
        ReusableContentHost(active.get(), || {
            ReusableContent(0, || {
                LinearWithCallbacks(nothing(), on_deactivate.clone(), nothing(), nothing(), || {});
            });
        });
        if !active.get() {
            finished.set(true);
        }
    }
    let mut test = composition_test();
    let active = mutable_state_of(true);
    let (finished, log) = (Rc::new(Cell::new(false)), Log::new());
    let (a, on_deactivate, f) = (active.handle(), logging(&finished, &log), finished.clone());
    test.compose(move || Content(a, on_deactivate.clone(), f.clone()));
    active.set(false);
    test.expect_changes();
    assert_eq!(log.entries(), ["true"]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReleaseIsCalledWhenNodeIsRemoved
#[test]
fn a_removed_node_is_released() {
    #[composable]
    fn Content(emit: MutableState<bool>, on_release: Action) {
        if emit.get() {
            ReusableContent(0, || {
                LinearWithCallbacks(nothing(), nothing(), on_release.clone(), nothing(), || {});
            });
        }
    }
    let mut test = composition_test();
    let emit = mutable_state_of(true);
    let release_calls = Counter::new();
    let (e, on_release) = (emit.handle(), counting(&release_calls));
    test.compose(move || Content(e, on_release.clone()));
    emit.set(false);
    test.expect_changes();
    assert_eq!(release_calls.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReleaseIsNotCalledOnReuse
#[test]
fn a_reused_node_is_not_released() {
    #[composable]
    fn Content(key: MutableState<i32>, on_release: Action) {
        ReusableContent(key.get(), || {
            LinearWithCallbacks(nothing(), nothing(), on_release.clone(), nothing(), || {});
        });
    }
    let mut test = composition_test();
    let key = mutable_state_of(0);
    let release_calls = Counter::new();
    let (k, on_release) = (key.handle(), counting(&release_calls));
    test.compose(move || Content(k, on_release.clone()));
    key.set(1);
    test.expect_changes();
    assert_eq!(release_calls.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReleaseIsNotCalledWithReusableContentHost
#[test]
fn a_deactivated_node_is_released_only_when_removed() {
    #[composable]
    fn Content(active: MutableState<bool>, emit: MutableState<bool>, on_release: Action) {
        if emit.get() {
            ReusableContentHost(active.get(), || {
                LinearWithCallbacks(nothing(), nothing(), on_release.clone(), nothing(), || {});
            });
        }
    }
    let mut test = composition_test();
    let (active, emit) = (mutable_state_of(true), mutable_state_of(true));
    let release_calls = Counter::new();
    let (a, e, on_release) = (active.handle(), emit.handle(), counting(&release_calls));
    test.compose(move || Content(a, e, on_release.clone()));
    active.set(false);
    test.expect_changes();
    assert_eq!(release_calls.get(), 0);
    emit.set(false);
    test.expect_changes();
    assert_eq!(release_calls.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReleaseIsCalledInApplyStage
#[test]
fn a_node_hears_its_release_after_the_composition() {
    #[composable]
    fn Content(emit: MutableState<bool>, on_release: Action, finished: Rc<Cell<bool>>) {
        if emit.get() {
            ReusableContent(0, || {
                LinearWithCallbacks(nothing(), nothing(), on_release.clone(), nothing(), || {});
            });
        } else {
            finished.set(true);
        }
    }
    let mut test = composition_test();
    let emit = mutable_state_of(true);
    let (finished, log) = (Rc::new(Cell::new(false)), Log::new());
    let (e, on_release, f) = (emit.handle(), logging(&finished, &log), finished.clone());
    test.compose(move || Content(e, on_release.clone(), f.clone()));
    emit.set(false);
    test.expect_changes();
    assert_eq!(log.entries(), ["true"]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, reusableContentTriggersRememberObserver
#[test]
fn reused_content_forgets_and_remembers_its_observers() {
    #[composable]
    fn Content(reuse_key: MutableState<i32>, remembered: RememberedState) {
        ReusableContent(reuse_key.get(), || {
            let r = remembered.clone();
            remember_observer(move || r);
            Text("Some text");
        });
    }
    let mut test = composition_test();
    let reuse_key = mutable_state_of(0);
    let remembered = RememberedState::default();
    let (k, r) = (reuse_key.handle(), remembered.clone());
    test.compose(move || Content(k, r.clone()));
    test.validate(|v| v.text("Some text"));
    assert_eq!(remembered.counts(), (1, 0, 0));
    reuse_key.set(1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(remembered.counts(), (2, 1, 0));
}
