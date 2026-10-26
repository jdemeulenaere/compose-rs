//! Compose's `EffectsTests`, adapted: memoization, state in composition, `DisposableEffect`
//! commit and dispose order, locals read in effects, updated compositions.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

/// A state composables read to be recomposed on demand, as Compose's test `Trigger`.
#[derive(Clone, Copy)]
struct Trigger(MutableState<i32>);

impl ComposeArg for Trigger {
    fn same(&self, other: &Self) -> bool {
        self.0.same(&other.0)
    }
}

impl Trigger {
    fn subscribe(&self) {
        self.0.get();
    }

    fn recompose(&self) {
        self.0.update(|count| count + 1);
    }
}

/// A trigger, and the owner keeping its state alive.
fn trigger() -> (Trigger, OwnedState<i32>) {
    let state = mutable_state_of(0);
    (Trigger(state.handle()), state)
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testMemoization1
#[test]
fn a_remembered_value_is_computed_once() {
    let mut test = composition_test();
    let inc = Counter::new();
    let i = inc.clone();
    test.compose(move || {
        let i = i.clone();
        remember(move || i.increment());
    });
    assert_eq!(inc.get(), 1);
    test.expect_no_changes();
    assert_eq!(inc.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testMemoization2
#[test]
fn a_keyed_remember_is_computed_again_when_its_key_changes() {
    let mut test = composition_test();
    let (calculations, compositions, calculation) =
        (Counter::new(), Counter::new(), Counter::new());
    let key = Rc::new(Cell::new(0));
    let (trigger, _owner) = trigger();
    let (ca, co, cn, k) =
        (calculations.clone(), compositions.clone(), calculation.clone(), key.clone());
    test.compose(move || {
        trigger.subscribe();
        co.increment();
        let ca = ca.clone();
        cn.set(remember_keyed(k.get(), move || 100 * ca.increment()));
    });
    let state = |test: &CompositionTest| {
        let _ = test;
        (calculations.get(), calculation.get(), compositions.get())
    };
    assert_eq!(state(&test), (1, 100, 1));
    trigger.recompose();
    test.expect_no_changes();
    assert_eq!(state(&test), (1, 100, 2));
    key.set(1);
    trigger.recompose();
    test.expect_changes();
    assert_eq!(state(&test), (2, 200, 3));
    trigger.recompose();
    test.expect_no_changes();
    assert_eq!(state(&test), (2, 200, 4));
    key.set(2);
    trigger.recompose();
    test.expect_changes();
    assert_eq!(state(&test), (3, 300, 5));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testState1
#[test]
fn remembered_state_is_kept_and_its_changes_shown() {
    thread_local! {
        static LOCAL: Cell<Option<MutableState<String>>> = const { Cell::new(None) };
    }
    let mut test = composition_test();
    let inc = Counter::new();
    let i = inc.clone();
    test.compose(move || {
        let i = i.clone();
        let local =
            remember(move || mutable_state_of(format!("Hello world! {}", i.increment() - 1)));
        LOCAL.set(Some(local));
        Text(&local.get());
    });
    let local = || LOCAL.get().expect("composed");
    test.validate(move |v| v.text(&local().get()));
    test.expect_no_changes();
    test.revalidate();
    local().set("New string".into());
    test.expect_changes();
    test.revalidate();
    test.validate(|v| v.text("New string"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testState2
#[test]
fn two_remembered_states_are_kept_apart() {
    thread_local! {
        static LOCALS: Cell<Option<(MutableState<String>, MutableState<String>)>> =
            const { Cell::new(None) };
    }
    let mut test = composition_test();
    test.compose(|| {
        let first = remember(|| mutable_state_of(String::from("First")));
        let second = remember(|| mutable_state_of(String::from("Second")));
        LOCALS.set(Some((first, second)));
        Text(&first.get());
        Text(&second.get());
    });
    let locals = || LOCALS.get().expect("composed");
    test.validate(move |v| {
        v.text(&locals().0.get());
        v.text(&locals().1.get());
    });
    test.expect_no_changes();
    test.revalidate();
    locals().0.set("New First".into());
    test.expect_changes();
    test.revalidate();
}

#[composable]
fn Unmountable(log: Log) {
    log.push("Unmountable:start");
    let effect_log = log.clone();
    DisposableEffect(NeverEqual, move |scope| {
        effect_log.push("onCommit");
        scope.on_dispose(move || effect_log.push("onDispose"))
    });
    log.push("Unmountable:end");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCommit1
#[test]
fn an_effect_commits_after_its_pass_and_disposes_after_it_leaves() {
    #[composable]
    fn Content(mount: MutableState<bool>, log: Log) {
        log.push("compose:start");
        if mount.get() {
            Unmountable(log.clone());
        }
        log.push("compose:end");
    }
    let mut test = composition_test();
    let mount = mutable_state_of(true);
    let log = Log::new();
    let (m, l) = (mount.handle(), log.clone());
    test.compose(move || Content(m, l.clone()));
    assert_eq!(
        log.entries(),
        ["compose:start", "Unmountable:start", "Unmountable:end", "compose:end", "onCommit"]
    );
    mount.set(false);
    test.expect_changes();
    assert_eq!(log.entries()[5..], ["compose:start", "compose:end", "onDispose"]);
}

/// A disposable effect logging `onCommit:{name}` and `onDispose:{name}`, run at each pass.
fn logged_effect(log: &Log, name: &'static str) {
    let log = log.clone();
    DisposableEffect(NeverEqual, move |scope| {
        log.push(format!("onCommit:{name}"));
        scope.on_dispose(move || log.push(format!("onDispose:{name}")))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCommit2
#[test]
fn effects_dispose_in_the_reverse_of_their_order() {
    #[composable]
    fn Unmountable2(log: Log) {
        logged_effect(&log, "a2");
        logged_effect(&log, "b2");
    }
    #[composable]
    fn Content(mount: MutableState<bool>, log: Log) {
        logged_effect(&log, "a1");
        if mount.get() {
            Unmountable2(log.clone());
        }
        logged_effect(&log, "b1");
    }
    let mut test = composition_test();
    let mount = mutable_state_of(true);
    let log = Log::new();
    let (m, l) = (mount.handle(), log.clone());
    test.compose(move || Content(m, l.clone()));
    assert_eq!(log.entries(), ["onCommit:a1", "onCommit:a2", "onCommit:b2", "onCommit:b1"]);
    mount.set(false);
    log.push("recompose");
    test.expect_changes();
    assert_eq!(
        log.entries()[4..],
        [
            "recompose",
            "onDispose:b1",
            "onDispose:b2",
            "onDispose:a2",
            "onDispose:a1",
            "onCommit:a1",
            "onCommit:b1",
        ]
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCommit3
#[test]
fn an_effect_keyed_on_a_never_equal_key_runs_again_at_each_pass() {
    let mut test = composition_test();
    let (trigger, _owner) = trigger();
    let log = Log::new();
    let x = Counter::new();
    let (l, xs) = (log.clone(), x.clone());
    test.compose(move || {
        trigger.subscribe();
        let (l, xs) = (l.clone(), xs.clone());
        DisposableEffect(NeverEqual, move |scope| {
            let y = xs.increment() - 1;
            l.push(format!("onCommit:{y}"));
            scope.on_dispose(move || l.push(format!("dispose:{y}")))
        });
    });
    log.push("recompose");
    trigger.recompose();
    test.expect_changes();
    assert_eq!(log.entries(), ["onCommit:0", "recompose", "dispose:0", "onCommit:1"]);
}

/// Two effects keyed on never-equal keys, counting with `a` and `b`.
fn two_effects(log: &Log, a: &Counter, b: &Counter) {
    for (name, counter) in [("a", a), ("b", b)] {
        let (log, counter) = (log.clone(), counter.clone());
        DisposableEffect(NeverEqual, move |scope| {
            let y = counter.increment() - 1;
            log.push(format!("onCommit {name}:{y}"));
            scope.on_dispose(move || log.push(format!("dispose {name}:{y}")))
        });
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCommit31
#[test]
fn effects_dispose_in_reverse_and_commit_in_order() {
    let mut test = composition_test();
    let (trigger, _owner) = trigger();
    let log = Log::new();
    let (a, b) = (Counter::new(), Counter::new());
    let (l, ac, bc) = (log.clone(), a.clone(), b.clone());
    test.compose(move || {
        trigger.subscribe();
        two_effects(&l, &ac, &bc);
    });
    log.push("recompose");
    trigger.recompose();
    test.expect_changes();
    assert_eq!(
        log.entries(),
        [
            "onCommit a:0",
            "onCommit b:0",
            "recompose",
            "dispose b:0",
            "dispose a:0",
            "onCommit a:1",
            "onCommit b:1",
        ]
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCommit4
#[test]
fn an_effect_runs_again_only_when_its_key_changes() {
    let mut test = composition_test();
    let (trigger, _owner) = trigger();
    let log = Log::new();
    let x = Counter::new();
    let key = Rc::new(Cell::new(123));
    let (l, xs, k) = (log.clone(), x.clone(), key.clone());
    test.compose(move || {
        trigger.subscribe();
        let (l, xs) = (l.clone(), xs.clone());
        DisposableEffect(k.get(), move |scope| {
            let y = xs.increment() - 1;
            l.push(format!("onCommit:{y}"));
            scope.on_dispose(move || l.push(format!("dispose:{y}")))
        });
    });
    log.push("recompose");
    trigger.recompose();
    test.expect_no_changes();
    assert_eq!(log.entries(), ["onCommit:0", "recompose"]);
    log.push("recompose (key -> 345)");
    key.set(345);
    trigger.recompose();
    test.expect_changes();
    assert_eq!(
        log.entries(),
        ["onCommit:0", "recompose", "recompose (key -> 345)", "dispose:0", "onCommit:1"]
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCommit5
#[test]
fn nested_effects_dispose_inner_first_and_commit_outer_first() {
    #[composable]
    fn Sub(trigger: Trigger, log: Log, c: Counter) {
        trigger.subscribe();
        DisposableEffect(NeverEqual, move |scope| {
            let y = c.increment() - 1;
            log.push(format!("onCommit c:{y}"));
            scope.on_dispose(move || log.push(format!("dispose c:{y}")))
        });
    }
    let mut test = composition_test();
    let (trigger, _owner) = trigger();
    let log = Log::new();
    let (a, b, c) = (Counter::new(), Counter::new(), Counter::new());
    let (l, ac, bc, cc) = (log.clone(), a.clone(), b.clone(), c.clone());
    test.compose(move || {
        trigger.subscribe();
        two_effects(&l, &ac, &bc);
        Sub(trigger, l.clone(), cc.clone());
    });
    log.push("recompose");
    trigger.recompose();
    test.expect_changes();
    assert_eq!(
        log.entries(),
        [
            "onCommit a:0",
            "onCommit b:0",
            "onCommit c:0",
            "recompose",
            "dispose c:0",
            "dispose b:0",
            "dispose a:0",
            "onCommit a:1",
            "onCommit b:1",
            "onCommit c:1",
        ]
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCommit6
#[test]
fn a_side_effect_writing_state_recomposes_its_reader() {
    #[composable]
    fn UpdateStateInCommit(read_value: Rc<Cell<i32>>) {
        let value = remember(|| mutable_state_of(1));
        read_value.set(value.get());
        SideEffect(move || value.set(2));
    }
    let mut test = composition_test();
    let read_value = Rc::new(Cell::new(0));
    let r = read_value.clone();
    test.compose(move || UpdateStateInCommit(r.clone()));
    test.expect_changes();
    assert_eq!(read_value.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testOnDispose1
#[test]
fn only_the_removed_effect_disposes() {
    #[composable]
    fn DisposeLogger(message: &'static str, log: Log) {
        DisposableEffect((), move |scope| scope.on_dispose(move || log.push(message)));
    }
    #[composable]
    fn Content(mount: MutableState<bool>, log: Log) {
        DisposeLogger("onDispose:1", log.clone());
        if mount.get() {
            DisposeLogger("onDispose:2", log.clone());
        }
    }
    let mut test = composition_test();
    let mount = mutable_state_of(true);
    let log = Log::new();
    let (m, l) = (mount.handle(), log.clone());
    test.compose(move || Content(m, l.clone()));
    assert!(log.entries().is_empty());
    mount.set(false);
    log.push("recompose");
    test.expect_changes();
    assert_eq!(log.entries(), ["recompose", "onDispose:2"]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCompositionLocal1
#[test]
fn a_provided_value_change_reaches_its_reader() {
    static FOO: CompositionLocal<String> =
        CompositionLocal::new("EffectsTests.Foo", || panic!("not provided"));
    #[composable]
    fn Bar() {
        Text(&FOO.current());
    }
    #[composable]
    fn Content(current: MutableState<String>) {
        CompositionLocalProvider(FOO.provides(current.get()), Bar);
    }
    let mut test = composition_test();
    let current = mutable_state_of(String::from("Hello World"));
    let c = current.handle();
    test.compose(move || Content(c));
    test.validate(move |v| v.text(&c.get()));
    current.set("abcd".into());
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCompositionLocal2
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testCompositionLocal_RecomposeScope
#[test]
fn a_provider_restarted_alone_provides_its_new_value() {
    static MY_LOCAL: CompositionLocal<i32> =
        CompositionLocal::new("EffectsTests.MyCompositionLocal", || panic!("not set"));
    #[composable]
    fn SimpleComposable2() {
        Text(&MY_LOCAL.current().to_string());
    }
    #[composable]
    fn SimpleComposable(scope: ScopeHolder, value: Counter) {
        scope.capture();
        CompositionLocalProvider(MY_LOCAL.provides(value.increment() as i32), || {
            SimpleComposable2();
            Text("Other");
        });
    }
    #[composable]
    fn Root(scope: ScopeHolder, value: Counter) {
        SimpleComposable(scope, value);
    }
    let mut test = composition_test();
    let scope = ScopeHolder::new();
    let value = Counter::new();
    let (s, v) = (scope.clone(), value.clone());
    test.compose(move || Root(s.clone(), v.clone()));
    let shown = value.clone();
    test.validate(move |v| {
        v.text(&shown.get().to_string());
        v.text("Other");
    });
    scope.invalidate();
    test.expect_changes();
    test.revalidate();
    assert_eq!(value.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, testUpdatedComposition
#[test]
fn a_remembered_state_stays_across_recompositions() {
    let mut test = composition_test();
    let inc = Counter::new();
    let i = inc.clone();
    test.compose(move || {
        let i = i.clone();
        let local =
            remember(move || mutable_state_of(format!("Hello world! {}", i.increment() - 1)));
        Text(&local.get());
    });
    test.validate(|v| v.text("Hello world! 0"));
    test.expect_no_changes();
    test.validate(|v| v.text("Hello world! 0"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/EffectsTests.kt, recomposeNewModifiedState
#[test]
fn state_an_effect_writes_in_new_content_recomposes_it() {
    thread_local! {
        static STATE: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
    }
    #[composable]
    fn Content(enabler: MutableState<bool>) {
        if enabler.get() {
            let state = remember(|| mutable_state_of(0));
            DisposableEffect((), move |scope| {
                state.set(1);
                scope.on_dispose(|| {})
            });
            Text(&state.get().to_string());
            Text("Some text");
            STATE.set(Some(state));
        }
    }
    let mut test = composition_test();
    let enabler = mutable_state_of(false);
    let e = enabler.handle();
    test.compose(move || Content(e));
    let expect = move |test: &mut CompositionTest, state: i32| {
        test.validate(move |v| {
            if e.get() {
                v.text(&state.to_string());
                v.text("Some text");
            }
        });
    };
    expect(&mut test, 0);
    enabler.set(true);
    // The first pass shows 0 and creates the effect, which sets 1: a second pass shows it.
    assert_eq!(test.advance_count(), 2);
    expect(&mut test, 1);
    let advance = || STATE.get().expect("composed").update(|state| state + 1);
    advance();
    assert_eq!(test.advance_count(), 1);
    expect(&mut test, 2);
    advance();
    assert_eq!(test.advance_count(), 1);
    expect(&mut test, 3);
}
