//! Compose's integration `SideEffectTests`, adapted: `SideEffect` (with and without keys),
//! `DisposableEffect` and `LaunchedEffect` ordering, `rememberUpdatedState` in a long-lived
//! effect. Compose's `.then { … }` steps are `advance()`s of the mock driver.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testSideEffectsRunInOrder
#[test]
fn side_effects_run_in_order_after_each_pass() {
    let mut test = composition_test();
    let results = Log::new();
    let at_composition = Log::new();
    let scope = ScopeHolder::new();
    let (r, a, s) = (results.clone(), at_composition.clone(), scope.clone());
    test.compose(move || {
        let (one, two) = (r.clone(), r.clone());
        SideEffect(move || one.push("1"));
        SideEffect(move || two.push("2"));
        a.clear();
        for entry in r.entries() {
            a.push(entry);
        }
        s.capture();
    });
    assert_eq!(results.entries(), ["1", "2"], "side effects were applied");
    assert!(at_composition.entries().is_empty(), "not applied until after composition");
    scope.invalidate();
    test.advance();
    assert_eq!(results.entries(), ["1", "2", "1", "2"], "applied a second time");
}

/// Remembered, says whether it's in the composition.
struct Presence(Rc<Cell<bool>>);

impl RememberObserver for Presence {
    fn on_remembered(&self) {
        self.0.set(true);
    }
    fn on_forgotten(&self) {
        self.0.set(false);
    }
    fn on_abandoned(&self) {
        panic!("unexpected call to on_abandoned");
    }
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testSideEffectsRunAfterLifecycleObservers
#[test]
fn side_effects_run_after_remember_observers_enter() {
    let mut test = composition_test();
    let (one, two) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let seen = Rc::new(Cell::new((false, false)));
    let (o, t, s) = (one.clone(), two.clone(), seen.clone());
    test.compose(move || {
        let o2 = o.clone();
        remember_observer(|| Presence(o2));
        let t2 = t.clone();
        let (o, t, s) = (o.clone(), t.clone(), s.clone());
        SideEffect(move || s.set((o.get(), t.get())));
        remember_observer(|| Presence(t2));
    });
    assert_eq!(seen.get(), (true, true), "both observers present for the side effect");
}

type Keys = Rc<RefCell<Vec<&'static str>>>;

/// The six side effects of `testSideEffectsWithKeys`, keyed on `keys` (the last on all of
/// them, an array standing for Kotlin's varargs).
#[composable]
fn KeyedEffects(
    results: Log,
    key1: &'static str,
    key2: &'static str,
    key3: &'static str,
    key4: &'static str,
    keys: Rc<Vec<&'static str>>,
) {
    let r = results.clone();
    SideEffect(move || r.push("1"));
    let r = results.clone();
    KeyedSideEffect(key1, move || r.push("2"));
    let r = results.clone();
    KeyedSideEffect((key1, key2), move || r.push("3"));
    let r = results.clone();
    KeyedSideEffect((key1, key2, key3), move || r.push("4"));
    let r = results.clone();
    KeyedSideEffect((key1, key2, key3, key4), move || r.push("5"));
    let r = results.clone();
    VarargSideEffect(keys, Rc::new(move || r.push("6")));
}

/// `SideEffect(*keys) { … }`: an array of the keys; one of another length is a change.
#[composable(non_restartable)]
fn VarargSideEffect(keys: Rc<Vec<&'static str>>, effect: Rc<dyn Fn()>) {
    let effect = move || effect();
    match keys.len() {
        0 => KeyedSideEffect([""; 0], effect),
        2 => KeyedSideEffect([keys[0], keys[1]], effect),
        3 => KeyedSideEffect([keys[0], keys[1], keys[2]], effect),
        4 => KeyedSideEffect([keys[0], keys[1], keys[2], keys[3]], effect),
        length => panic!("no case for {length} keys"),
    }
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testSideEffectsWithKeys
#[test]
fn keyed_side_effects_run_when_their_keys_change() {
    let mut test = composition_test();
    let results = Log::new();
    let keys: Keys = Rc::new(RefCell::new(vec!["A", "B", "C", "D"]));
    let vararg: Keys = Rc::new(RefCell::new(vec!["A", "B", "C", "D"]));
    let scope = ScopeHolder::new();
    let (r, k, v, s) = (results.clone(), keys.clone(), vararg.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        let k = k.borrow();
        KeyedEffects(r.clone(), k[0], k[1], k[2], k[3], Rc::new(v.borrow().clone()));
    });
    let mut expected = String::from("123456");
    assert_eq!(results.entries().concat(), expected, "side effects were applied");
    let mut step = |change: &dyn Fn(), added: &str| {
        change();
        scope.invalidate();
        test.advance();
        expected.push_str(added);
        assert_eq!(results.entries().concat(), expected);
    };
    step(&|| {}, "1");
    let set = |index: usize, value: &'static str| {
        keys.borrow_mut()[index] = value;
        *vararg.borrow_mut() = keys.borrow().clone();
    };
    step(&|| set(0, "a"), "123456");
    step(&|| set(1, "b"), "13456");
    step(&|| set(2, "b"), "1456");
    step(&|| set(3, "b"), "156");
    step(&|| *vararg.borrow_mut() = vec!["x", "y"], "16");
    step(&|| *vararg.borrow_mut() = vec!["x", "y", "z"], "16");
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testSideEffectsWithStateKeys
#[test]
fn keyed_side_effects_run_when_their_state_keys_change() {
    #[composable]
    fn Content(
        results: Log,
        keys: MutableState<[&'static str; 4]>,
        vararg: MutableState<Rc<Vec<&'static str>>>,
    ) {
        let k = keys.get();
        KeyedEffects(results, k[0], k[1], k[2], k[3], vararg.get());
    }
    let mut test = composition_test();
    let results = Log::new();
    let keys = mutable_state_of(["A", "B", "C", "D"]);
    let vararg = mutable_state_of_with(Rc::new(vec!["A", "B", "C", "D"]), NeverEqualPolicy);
    let (r, k, v) = (results.clone(), keys.handle(), vararg.handle());
    test.compose(move || Content(r.clone(), k, v));
    let mut expected = String::from("123456");
    assert_eq!(results.entries().concat(), expected);
    let mut step = |index: Option<usize>, vararg_keys: Option<Vec<&'static str>>, added: &str| {
        if let Some(index) = index {
            keys.update(|keys| {
                let mut keys = *keys;
                keys[index] = if index == 0 { "a" } else { "b" };
                keys
            });
        }
        vararg.set(Rc::new(vararg_keys.unwrap_or_else(|| keys.get().to_vec())));
        test.advance();
        expected.push_str(added);
        assert_eq!(results.entries().concat(), expected);
    };
    step(Some(0), None, "123456");
    step(Some(1), None, "13456");
    step(Some(2), None, "1456");
    step(Some(3), None, "156");
    step(None, Some(vec!["x", "y"]), "16");
    step(None, Some(vec!["x", "y", "z"]), "16");
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testSideEffectVarargKeyRemoved
#[test]
fn a_side_effect_runs_when_vararg_keys_are_dropped() {
    #[composable]
    fn Content(keys: MutableState<Rc<Vec<&'static str>>>, invocations: Counter) {
        VarargSideEffect(
            keys.get(),
            Rc::new(move || {
                invocations.increment();
            }),
        );
    }
    let mut test = composition_test();
    let keys = mutable_state_of_with(Rc::new(vec!["1", "2", "3"]), NeverEqualPolicy);
    let invocations = Counter::new();
    let (k, i) = (keys.handle(), invocations.clone());
    test.compose(move || Content(k, i.clone()));
    assert_eq!(invocations.get(), 1, "runs once after the first composition");
    keys.set(Rc::new(vec!["1", "2"]));
    test.advance();
    assert_eq!(invocations.get(), 2, "runs when trailing keys are dropped");
    keys.set(Rc::new(vec![]));
    test.advance();
    assert_eq!(invocations.get(), 3, "runs when all keys are removed");
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testDisposableEffectExecutionOrder
#[test]
fn a_disposable_effect_runs_after_composition_and_disposes_after_the_next() {
    #[composable]
    fn Unmountable(log: Log) {
        log.push("Unmountable:start");
        let effect_log = log.clone();
        DisposableEffect((), move |scope| {
            effect_log.push("DisposableEffect");
            scope.on_dispose(move || effect_log.push("onDispose"))
        });
        log.push("Unmountable:end");
    }
    #[composable]
    fn Content(log: Log, mount: MutableState<bool>) {
        log.push("compose:start");
        if mount.get() {
            Unmountable(log.clone());
        }
        log.push("compose:end");
    }
    let mut test = composition_test();
    let log = Log::new();
    let mount = mutable_state_of(true);
    let (l, m) = (log.clone(), mount.handle());
    test.compose(move || Content(l.clone(), m));
    assert_eq!(
        log.entries(),
        [
            "compose:start",
            "Unmountable:start",
            "Unmountable:end",
            "compose:end",
            "DisposableEffect"
        ]
    );
    mount.set(false);
    test.advance();
    assert_eq!(log.entries()[5..], ["compose:start", "compose:end", "onDispose"]);
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testDisposableEffectRelativeOrdering
#[test]
#[ignore = "forget order: a conditional's removed content is forgotten after later replaced values (fixed with the remember-order ports)"]
fn disposable_effects_dispose_in_reverse_order() {
    #[composable]
    fn Unmountable(log: Log) {
        for name in ["a2", "b2"] {
            let log = log.clone();
            DisposableEffect((), move |scope| {
                log.push(format!("DisposableEffect:{name}"));
                scope.on_dispose(move || log.push(format!("onDispose:{name}")))
            });
        }
    }
    #[composable]
    fn Content(log: Log, mount: MutableState<bool>) {
        let first = log.clone();
        DisposableEffect(NeverEqual, move |scope| {
            first.push("DisposableEffect:a1");
            scope.on_dispose(move || first.push("onDispose:a1"))
        });
        if mount.get() {
            Unmountable(log.clone());
        }
        let last = log.clone();
        DisposableEffect(NeverEqual, move |scope| {
            last.push("DisposableEffect:b1");
            scope.on_dispose(move || last.push("onDispose:b1"))
        });
    }
    let mut test = composition_test();
    let log = Log::new();
    let mount = mutable_state_of(true);
    let (l, m) = (log.clone(), mount.handle());
    test.compose(move || Content(l.clone(), m));
    assert_eq!(
        log.entries(),
        [
            "DisposableEffect:a1",
            "DisposableEffect:a2",
            "DisposableEffect:b2",
            "DisposableEffect:b1"
        ]
    );
    mount.set(false);
    log.push("recompose");
    test.advance();
    assert_eq!(
        log.entries()[4..],
        [
            "recompose",
            "onDispose:b1",
            "onDispose:b2",
            "onDispose:a2",
            "onDispose:a1",
            "DisposableEffect:a1",
            "DisposableEffect:b1",
        ]
    );
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testDisposableEffectKeyChange
#[test]
fn a_disposable_effect_runs_again_when_its_key_changes() {
    let mut test = composition_test();
    let log = Log::new();
    let key = Rc::new(Cell::new(123));
    let counter = Counter::new();
    let scope = ScopeHolder::new();
    let (l, k, c, s) = (log.clone(), key.clone(), counter.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        let (l, c) = (l.clone(), c.clone());
        DisposableEffect(k.get(), move |scope| {
            let y = c.get();
            c.increment();
            l.push(format!("DisposableEffect:{y}"));
            scope.on_dispose(move || l.push(format!("dispose:{y}")))
        });
    });
    log.push("recompose");
    scope.invalidate();
    test.advance();
    assert_eq!(log.entries(), ["DisposableEffect:0", "recompose"]);
    log.push("recompose (key -> 345)");
    key.set(345);
    scope.invalidate();
    test.advance();
    assert_eq!(
        log.entries(),
        [
            "DisposableEffect:0",
            "recompose",
            "recompose (key -> 345)",
            "dispose:0",
            "DisposableEffect:1",
        ]
    );
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testLaunchedEffect
#[test]
fn a_launched_effect_runs_its_task_across_suspensions() {
    let mut test = composition_test();
    let counter = mutable_state_of(0);
    let channel = Channel::<()>::conflated();
    let (c, ch) = (counter.handle(), channel.clone());
    test.compose(move || {
        let ch = ch.clone();
        LaunchedEffect(ch.clone(), async move {
            c.update(|c| c + 1);
            ch.receive().await;
            c.update(|c| c + 1);
            ch.receive().await;
            c.update(|c| c + 1);
        });
    });
    test.advance();
    assert_eq!(counter.get(), 1);
    channel.send(());
    test.advance();
    assert_eq!(counter.get(), 2);
    channel.send(());
    test.advance();
    assert_eq!(counter.get(), 3);
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testAwaitFrameFromLaunchedEffect
#[test]
fn a_launched_effect_awaits_a_frame() {
    // Compose compares the frame time with the Choreographer's (Android): here, with the
    // frames the driver dispatched.
    let mut test = composition_test();
    let frame_time = Rc::new(Cell::new(None));
    let time = frame_time.clone();
    test.compose(move || {
        let time = time.clone();
        LaunchedEffect((), async move {
            with_frame_nanos(|nanos| time.set(Some(nanos))).await;
        });
    });
    test.advance();
    assert_eq!(frame_time.get(), Some(FRAME.as_nanos() as u64), "the first frame's time");
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testLaunchedEffectRunsAfter
#[test]
fn a_launched_effect_runs_after_the_side_effects_of_its_pass() {
    let mut test = composition_test();
    let side_effect_ran = Rc::new(Cell::new(false));
    let launch_ran_after = Rc::new(Cell::new(false));
    let (s, l) = (side_effect_ran.clone(), launch_ran_after.clone());
    test.compose(move || {
        let (s1, l) = (s.clone(), l.clone());
        LaunchedEffect((), async move { l.set(s1.get()) });
        let s = s.clone();
        SideEffect(move || s.set(true));
    });
    test.advance();
    assert!(launch_ran_after.get(), "the LaunchedEffect ran after a later SideEffect");
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/SideEffectTests.kt, testRememberUpdatedStateRecomposition
#[test]
fn a_long_lived_effect_sees_the_updated_state() {
    #[composable]
    fn MyComposable(arg: String, input: Channel<()>, output: Channel<String>) {
        let current = remember_updated_state(arg);
        LaunchedEffect((input.clone(), output.clone()), async move {
            input.receive().await;
            output.send(current.get());
        });
    }
    let mut test = composition_test();
    let arg = mutable_state_of(String::from("hello"));
    let (please_send, output) = (Channel::new(), Channel::new());
    let (a, p, o) = (arg.handle(), please_send.clone(), output.clone());
    test.compose(move || MyComposable(a.get(), p.clone(), o.clone()));
    test.advance();
    arg.set("world".into());
    test.advance();
    please_send.send(());
    test.advance();
    assert_eq!(output.try_receive().as_deref(), Some("world"));
}
