//! Compose's `CompositionLocalTests`, adapted: dynamic and static locals, several values per
//! provider, `provides_default`, computed defaults and `provides_computed`,
//! `CompositionLocalContext`, `with_composition_local(s)`. The subcomposition tests are in
//! composition_context.rs; the host-default ones wait for host defaults.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

static LOCAL_SOME_TEXT: CompositionLocal<&'static str> =
    CompositionLocal::new("LocalSomeTextComposition", || "Default");
static LOCAL_SOME_INT: CompositionLocal<i32> =
    CompositionLocal::new("LocalSomeIntComposition", || 1);
static LOCAL_SOME_OTHER_INT: CompositionLocal<i32> =
    CompositionLocal::new("LocalSomeOtherIntProvider", || 1);
static LOCAL_SOME_STATIC_INT: CompositionLocal<i32> =
    CompositionLocal::new_static("LocalSomeStaticInt", || 40);

/// A local passed to a restartable composable: compared by address, as a Kotlin object.
#[derive(Clone, Copy)]
struct LocalRef<T: 'static>(&'static CompositionLocal<T>);

impl<T: 'static> ComposeArg for LocalRef<T> {
    fn same(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

#[composable]
fn ReadStringCompositionLocal(local: LocalRef<String>) {
    Text(&local.0.current());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testCompositionLocalApi
#[test]
fn locals_read_their_nearest_provider_or_default() {
    #[composable]
    fn Content() {
        assert_eq!(LOCAL_SOME_TEXT.current(), "Default");
        assert_eq!(LOCAL_SOME_INT.current(), 1);
        CompositionLocalProvider(
            (
                LOCAL_SOME_TEXT.provides("Test1"),
                LOCAL_SOME_INT.provides(12),
                LOCAL_SOME_OTHER_INT.provides(42),
                LOCAL_SOME_STATIC_INT.provides(50),
            ),
            || {
                assert_eq!(LOCAL_SOME_TEXT.current(), "Test1");
                assert_eq!(LOCAL_SOME_INT.current(), 12);
                assert_eq!(LOCAL_SOME_OTHER_INT.current(), 42);
                assert_eq!(LOCAL_SOME_STATIC_INT.current(), 50);
                CompositionLocalProvider(
                    (LOCAL_SOME_TEXT.provides("Test2"), LOCAL_SOME_STATIC_INT.provides(60)),
                    || {
                        assert_eq!(LOCAL_SOME_TEXT.current(), "Test2");
                        assert_eq!(LOCAL_SOME_INT.current(), 12);
                        assert_eq!(LOCAL_SOME_OTHER_INT.current(), 42);
                        assert_eq!(LOCAL_SOME_STATIC_INT.current(), 60);
                    },
                );
                assert_eq!(LOCAL_SOME_TEXT.current(), "Test1");
                assert_eq!(LOCAL_SOME_INT.current(), 12);
                assert_eq!(LOCAL_SOME_OTHER_INT.current(), 42);
                assert_eq!(LOCAL_SOME_STATIC_INT.current(), 50);
            },
        );
        assert_eq!(LOCAL_SOME_TEXT.current(), "Default");
        assert_eq!(LOCAL_SOME_INT.current(), 1);
    }
    let mut test = composition_test();
    test.compose(Content);
}

static LOCAL_DYNAMIC_STRING: CompositionLocal<String> =
    CompositionLocal::new("LocalDynamicString", || "Default".into());
static LOCAL_STATIC_STRING: CompositionLocal<String> =
    CompositionLocal::new_static("LocalStaticString", || "Default".into());

/// `recompose_Dynamic` and `recompose_Static`: the root provides `text` and is invalidated.
fn recompose_with(local: &'static CompositionLocal<String>) {
    let mut test = composition_test();
    let text = Rc::new(RefCell::new(String::from("Unmodified")));
    let scope = ScopeHolder::new();
    let (t, s) = (text.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        CompositionLocalProvider(local.provides(t.borrow().clone()), || {
            ReadStringCompositionLocal(LocalRef(local));
        });
    });
    test.validate(|v| v.text("Unmodified"));
    *text.borrow_mut() = "Modified".into();
    scope.invalidate();
    test.expect_changes();
    test.validate(|v| v.text("Modified"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, recompose_Dynamic
#[test]
fn a_dynamic_locals_new_value_recomposes_its_reader() {
    recompose_with(&LOCAL_DYNAMIC_STRING);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, recompose_Static
#[test]
fn a_static_locals_new_value_recomposes_the_content() {
    recompose_with(&LOCAL_STATIC_STRING);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, insertShouldSeePreviouslyProvidedValues
#[test]
fn inserted_content_sees_the_values_provided_before() {
    static LOCAL: CompositionLocal<String> =
        CompositionLocal::new_static("insertShouldSee.someStaticString", || "Default".into());
    #[composable]
    fn Content(should_read: Rc<Cell<bool>>, scope: ScopeHolder) {
        CompositionLocalProvider(LOCAL.provides("Provided A".into()), || {
            scope.capture();
            if should_read.get() {
                ReadStringCompositionLocal(LocalRef(&LOCAL));
            }
        });
    }
    let mut test = composition_test();
    let should_read = Rc::new(Cell::new(false));
    let scope = ScopeHolder::new();
    let (r, s) = (should_read.clone(), scope.clone());
    test.compose(move || Content(r.clone(), s.clone()));
    let shown = should_read.clone();
    test.validate(move |v| {
        if shown.get() {
            v.text("Provided A");
        }
    });
    should_read.set(true);
    scope.invalidate();
    test.expect_changes();
    test.revalidate();
}

#[derive(Clone, PartialEq, Debug)]
struct SomeData {
    value: String,
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, providingANewDataClassValueShouldNotRecompose
#[test]
fn an_equal_value_by_the_locals_policy_recomposes_nothing() {
    static LOCAL: CompositionLocal<SomeData> =
        CompositionLocal::with_policy("someDataCompositionLocal", StructuralEqualityPolicy, || {
            SomeData { value: "default".into() }
        });
    #[composable]
    fn ReadSomeData(composed: Rc<Cell<bool>>) {
        composed.set(true);
        Text(&LOCAL.current().value);
    }
    let mut test = composition_test();
    let composed = Rc::new(Cell::new(false));
    let scope = ScopeHolder::new();
    let (c, s) = (composed.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        let c = c.clone();
        CompositionLocalProvider(
            LOCAL.provides(SomeData { value: "provided".into() }),
            move || ReadSomeData(c.clone()),
        );
    });
    assert!(composed.get());
    composed.set(false);
    scope.invalidate();
    test.expect_no_changes();
    assert!(!composed.get(), "an equal data value: the reader didn't recompose");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, currentRetrievesCorrectValues
#[test]
fn current_reads_the_right_values_as_providers_come_and_go() {
    static LOCAL: CompositionLocal<i32> = CompositionLocal::new("currentRetrieves.local", || 0);
    static STATIC: CompositionLocal<i32> =
        CompositionLocal::new_static("currentRetrieves.static", || -1);
    #[composable]
    fn Validate(provided_value: i32, state: MutableState<i32>) {
        assert_eq!(LOCAL.current(), provided_value);
        state.get();
    }
    #[composable]
    fn Content(
        include_providers: MutableState<bool>,
        provided_value: MutableState<i32>,
        provided_static: MutableState<i32>,
        state: MutableState<i32>,
    ) {
        if include_providers.get() {
            CompositionLocalProvider(
                (LOCAL.provides(provided_value.get()), STATIC.provides(provided_static.get())),
                || Validate(provided_value.get(), state),
            );
        }
    }
    let mut test = composition_test();
    let state = mutable_state_of(0);
    let provided_value = mutable_state_of(100);
    let provided_static = mutable_state_of(2000);
    let include_providers = mutable_state_of(true);
    let (i, p, ps, s) = (
        include_providers.handle(),
        provided_value.handle(),
        provided_static.handle(),
        state.handle(),
    );
    test.compose(move || Content(i, p, ps, s));
    provided_static.update(|v| v + 1);
    test.advance();
    include_providers.set(false);
    test.advance();
    include_providers.set(true);
    test.advance();
    provided_value.update(|v| v + 1);
    test.advance();
    state.update(|v| v + 1);
    test.advance();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, canProvideManyProvidersSimultaneously
#[test]
fn a_provider_takes_many_values_at_once() {
    thread_local! {
        static LOCALS: Vec<&'static CompositionLocal<i32>> = (0..1000)
            .map(|index| {
                let name: &'static str = Box::leak(format!("canProvideMany.{index}").into_boxed_str());
                &*Box::leak(Box::new(CompositionLocal::new(name, || 2)))
            })
            .collect();
    }
    static LOCAL_TEST: CompositionLocal<i32> =
        CompositionLocal::new("canProvideMany.LocalTest", || panic!("not provided"));
    #[composable]
    fn Test(seen: Rc<Cell<i32>>) {
        CompositionLocalProvider(LOCAL_TEST.provides(1), || {
            let mut values: Vec<ProvidedValue<i32>> =
                LOCALS.with(|locals| locals.iter().map(|local| local.provides(3)).collect());
            values.push(LOCAL_TEST.provides(2));
            CompositionLocalProvider(values, || seen.set(LOCAL_TEST.current()));
        });
    }
    let mut test = composition_test();
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    test.compose(move || Test(s.clone()));
    assert_eq!(seen.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testSingleInvalidatedProvider
#[test]
fn nested_providers_each_provide_their_own() {
    static LOCAL1: CompositionLocal<i32> = CompositionLocal::new("testSingleInvalidated.1", || 10);
    static LOCAL2: CompositionLocal<i32> = CompositionLocal::new("testSingleInvalidated.2", || 20);
    static LOCAL3: CompositionLocal<i32> = CompositionLocal::new("testSingleInvalidated.3", || 30);
    #[composable]
    fn Content(state: MutableState<i32>) {
        state.get();
        CompositionLocalProvider(LOCAL1.provides(11), || {
            state.get();
            CompositionLocalProvider(LOCAL2.provides(22), || {
                state.get();
                CompositionLocalProvider(LOCAL3.provides(33), || {
                    state.get();
                    assert_eq!(
                        (LOCAL1.current(), LOCAL2.current(), LOCAL3.current()),
                        (11, 22, 33)
                    );
                });
                assert_eq!((LOCAL1.current(), LOCAL2.current(), LOCAL3.current()), (11, 22, 30));
            });
            assert_eq!((LOCAL1.current(), LOCAL2.current(), LOCAL3.current()), (11, 20, 30));
        });
        assert_eq!((LOCAL1.current(), LOCAL2.current(), LOCAL3.current()), (10, 20, 30));
    }
    let mut test = composition_test();
    let state = mutable_state_of(0);
    let s = state.handle();
    test.compose(move || Content(s));
    state.update(|v| v + 1);
    test.advance();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testProvideAllLocals
#[test]
fn a_context_provides_every_local_in_another_composition() {
    // Compose composes the second composition with a recomposer of its own; compositions are
    // independent here, and the second one is driven by hand.
    static LOCAL1: CompositionLocal<i32> = CompositionLocal::new("provideAll.local1", || 0);
    static LOCAL2: CompositionLocal<i32> = CompositionLocal::new("provideAll.local2", || 0);
    static STATIC_LOCAL: CompositionLocal<i32> =
        CompositionLocal::new_static("provideAll.staticLocal", || 0);
    type Seen = Rc<RefCell<(i32, i32, i32, u32)>>;
    thread_local! {
        static SECOND: RefCell<Option<Composition>> = const { RefCell::new(None) };
    }
    #[composable]
    fn LocalsConsumer(seen: Seen) {
        let locals = remember_updated_state(current_composition_local_context());
        DisposableEffect((), move |scope| {
            let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
            let seen = seen.clone();
            composition.set_content(move || {
                let seen = seen.clone();
                CompositionLocalProvider(locals.get(), move || {
                    let mut seen = seen.borrow_mut();
                    let count = seen.3 + 1;
                    *seen = (LOCAL1.current(), LOCAL2.current(), STATIC_LOCAL.current(), count);
                });
            });
            SECOND.with(|second| *second.borrow_mut() = Some(composition));
            scope.on_dispose(|| SECOND.with(|second| second.borrow_mut().take()).map_or((), drop))
        });
    }
    #[composable]
    fn Content(provided: MutableState<Rc<Vec<(u8, i32)>>>, seen: Seen) {
        let values: Vec<ProvidedValue<i32>> = provided
            .get()
            .iter()
            .map(|&(local, value)| match local {
                1 => LOCAL1.provides(value),
                2 => LOCAL2.provides(value),
                _ => STATIC_LOCAL.provides(value),
            })
            .collect();
        CompositionLocalProvider(values, || LocalsConsumer(seen.clone()));
    }
    let recompose_second = || {
        SECOND.with(|second| second.borrow().as_ref().map(Composition::recompose));
    };
    let mut test = composition_test();
    let provided = mutable_state_of_with(Rc::new(Vec::new()), NeverEqualPolicy);
    let seen: Seen = Rc::default();
    let (p, s) = (provided.handle(), seen.clone());
    test.compose(move || Content(p, s.clone()));
    test.advance();
    recompose_second();
    assert_eq!(*seen.borrow(), (0, 0, 0, 1));
    let mut step = |values: Vec<(u8, i32)>, expected: (i32, i32, i32)| {
        seen.borrow_mut().3 = 0;
        provided.set(Rc::new(values));
        test.advance();
        recompose_second();
        let seen = *seen.borrow();
        assert_eq!((seen.0, seen.1, seen.2), expected);
        assert_eq!(seen.3, 1, "the second composition's content ran once");
    };
    step(vec![(1, 1)], (1, 0, 0));
    step(vec![(1, 2)], (2, 0, 0));
    step(vec![(1, 2), (3, 1)], (2, 0, 1));
    step(vec![(1, 2), (3, 2)], (2, 0, 2));
    step(vec![(1, 1), (3, 1)], (1, 0, 1));
    step(vec![(1, 1), (2, 1), (3, 1)], (1, 1, 1));
    step(vec![(1, 1), (3, 1)], (1, 0, 1));
    step(vec![], (0, 0, 0));
    test.dispose();
}

static LOCAL_CACHE: CompositionLocal<&'static str> =
    CompositionLocal::new_static("LocalCache", || "Unset");

#[composable]
fn CacheInvalidateSet(content: impl Fn() + 'static) {
    CompositionLocalProvider(LOCAL_CACHE.provides("Set"), || content());
}

#[composable]
fn CacheInvalidate(state: MutableState<i32>) {
    Text(&state.get().to_string());
    Text(LOCAL_CACHE.current());
    CacheInvalidateSet!({
        Text(&state.get().to_string());
        Text(LOCAL_CACHE.current());
    });
    Text(LOCAL_CACHE.current());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testRecomposeCacheInvalidation
#[test]
fn a_scope_restarted_inside_a_provider_reads_its_values() {
    let mut test = composition_test();
    let state = mutable_state_of(0);
    let s = state.handle();
    test.compose(move || CacheInvalidate(s));
    test.validate(move |v| {
        v.text(&s.get().to_string());
        v.text("Unset");
        v.text(&s.get().to_string());
        v.text("Set");
        v.text("Unset");
    });
    state.update(|v| v + 1);
    test.advance();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testSingleProvideDefaultValue
#[test]
fn provides_default_yields_to_a_value_provided_above() {
    static LOCAL: CompositionLocal<i32> = CompositionLocal::new("singleProvideDefault", || 0);
    let mut test = composition_test();
    test.compose(|| {
        CompositionLocalProvider(LOCAL.provides(1), || {
            CompositionLocalProvider(LOCAL.provides_default(2), || assert_eq!(LOCAL.current(), 1));
        });
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testDefaultComputedLocal
#[test]
fn a_computed_default_reads_other_locals_where_its_read() {
    static LOCAL1: CompositionLocal<i32> = CompositionLocal::new("defaultComputed.1", || 0);
    static LOCAL2: CompositionLocal<i32> =
        CompositionLocal::with_computed_default("defaultComputed.2", |scope| {
            scope.current_value(&LOCAL1) + 10
        });
    let mut test = composition_test();
    test.compose(|| {
        assert_eq!(LOCAL2.current(), LOCAL1.current() + 10);
        CompositionLocalProvider(LOCAL1.provides(20), || {
            assert_eq!(LOCAL2.current(), LOCAL1.current() + 10);
            assert_eq!(LOCAL2.current(), 30);
        });
    });
}

fn provides_computed_with(
    local1: &'static CompositionLocal<i32>,
    local2: &'static CompositionLocal<i32>,
) {
    let mut test = composition_test();
    test.compose(move || {
        assert_eq!((local1.current(), local2.current()), (0, 0));
        CompositionLocalProvider(
            local2.provides_computed(move |scope| scope.current_value(local1) + 10),
            move || {
                assert_eq!(local2.current(), local1.current() + 10);
                CompositionLocalProvider(local1.provides(10), move || {
                    assert_eq!(local2.current(), local1.current() + 10);
                    assert_eq!(local2.current(), 20);
                });
            },
        );
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testProvidingDynamicLocalAsComputed
#[test]
fn a_dynamic_local_provided_as_computed_follows_its_inputs() {
    static LOCAL1: CompositionLocal<i32> = CompositionLocal::new("dynamicAsComputed.1", || 0);
    static LOCAL2: CompositionLocal<i32> = CompositionLocal::new("dynamicAsComputed.2", || 0);
    provides_computed_with(&LOCAL1, &LOCAL2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testProvidingAStaticLocalAsComputed
#[test]
fn a_static_local_provided_as_computed_follows_its_inputs() {
    static LOCAL1: CompositionLocal<i32> = CompositionLocal::new_static("staticAsComputed.1", || 0);
    static LOCAL2: CompositionLocal<i32> = CompositionLocal::new_static("staticAsComputed.2", || 0);
    provides_computed_with(&LOCAL1, &LOCAL2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testValueChangeWhileRemoving
#[test]
fn a_provided_value_changing_as_its_provider_leaves() {
    static LOCAL_TEXT: CompositionLocal<String> =
        CompositionLocal::new("valueChangeWhileRemoving", String::new);
    #[composable]
    fn Content(show_content: MutableState<bool>, text: MutableState<String>) {
        if show_content.get() {
            Linear!({
                CompositionLocalProvider(LOCAL_TEXT.provides(text.get()), || {
                    Text(&LOCAL_TEXT.current())
                });
            });
        }
    }
    let mut test = composition_test();
    let show_content = mutable_state_of(true);
    let text = mutable_state_of(String::from("Hello"));
    let (s, t) = (show_content.handle(), text.handle());
    test.compose(move || Content(s, t));
    test.validate(|v| v.linear(|v| v.text("Hello")));
    text.set("Goodbye".into());
    show_content.set(false);
    test.expect_changes();
    test.validate(|_| {});
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, validateSampledExample
#[test]
fn a_computed_default_follows_the_values_provided_around_it() {
    static LOCAL_VALUE: CompositionLocal<i32> = CompositionLocal::new("sample.LocalValue", || 10);
    static LOCAL_LARGER_VALUE: CompositionLocal<i32> =
        CompositionLocal::new("sample.LocalLargerValue", || 12);
    static LOCAL_COMPUTED_VALUE: CompositionLocal<i32> =
        CompositionLocal::with_computed_default("sample.LocalComputedValue", |scope| {
            scope.current_value(&LOCAL_VALUE) + 4
        });
    #[composable]
    fn App() {
        assert_eq!(LOCAL_VALUE.current(), 10);
        assert_eq!(LOCAL_LARGER_VALUE.current(), 12);
        assert_eq!(LOCAL_COMPUTED_VALUE.current(), 14);
        CompositionLocalProvider(LOCAL_VALUE.provides(20), || {
            assert_eq!(LOCAL_VALUE.current(), 20);
            assert_eq!(LOCAL_LARGER_VALUE.current(), 12, "not provided again");
            assert_eq!(LOCAL_COMPUTED_VALUE.current(), 24);
            CompositionLocalProvider(
                LOCAL_LARGER_VALUE.provides(LOCAL_VALUE.current() + 2),
                || {
                    assert_eq!(LOCAL_LARGER_VALUE.current(), 22);
                    CompositionLocalProvider(LOCAL_VALUE.provides(50), || {
                        assert_eq!(LOCAL_VALUE.current(), 50);
                        assert_eq!(LOCAL_LARGER_VALUE.current(), 22);
                        assert_eq!(LOCAL_COMPUTED_VALUE.current(), 54);
                    });
                },
            );
        });
    }
    let mut test = composition_test();
    test.compose(App);
}

static LOCAL_STATIC_VALUE: CompositionLocal<bool> =
    CompositionLocal::new_static("staticLocalUpdate.LocalValue", || panic!("not provided"));
static LOCAL_STATIC_OTHER_VALUE: CompositionLocal<i32> =
    CompositionLocal::new_static("staticLocalUpdate.LocalOtherValue", || panic!("not provided"));
static SOME_STATIC_VALUE: CompositionLocal<i32> =
    CompositionLocal::new_static("staticLocalUpdate.SomeValue", || 0);

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, staticLocalUpdateInvalidatesCorrectly_startProvide
#[test]
fn a_static_locals_change_reaches_content_under_default_providers() {
    #[composable]
    fn Content(value: MutableState<bool>, seen: Rc<Cell<bool>>) {
        CompositionLocalProvider(LOCAL_STATIC_VALUE.provides(value.get()), || {
            CompositionLocalProvider(LOCAL_STATIC_OTHER_VALUE.provides_default(1), || {
                CompositionLocalProvider(LOCAL_STATIC_OTHER_VALUE.provides_default(2), || {
                    seen.set(LOCAL_STATIC_VALUE.current());
                });
            });
        });
    }
    let mut test = composition_test();
    let value = mutable_state_of(false);
    let seen = Rc::new(Cell::new(false));
    let (v, s) = (value.handle(), seen.clone());
    test.compose(move || Content(v, s.clone()));
    assert!(!seen.get());
    value.set(true);
    test.advance();
    assert!(seen.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, staticLocalUpdateInvalidatesCorrectly_startProvides
#[test]
fn a_static_locals_change_reaches_content_under_several_providers() {
    #[composable]
    fn Content(value: MutableState<bool>, seen: Rc<Cell<bool>>) {
        CompositionLocalProvider(
            (SOME_STATIC_VALUE.provides(0), LOCAL_STATIC_VALUE.provides(value.get())),
            || {
                CompositionLocalProvider(
                    (SOME_STATIC_VALUE.provides(1), LOCAL_STATIC_OTHER_VALUE.provides_default(1)),
                    || {
                        CompositionLocalProvider(
                            (
                                SOME_STATIC_VALUE.provides(2),
                                LOCAL_STATIC_OTHER_VALUE.provides_default(2),
                            ),
                            || seen.set(LOCAL_STATIC_VALUE.current()),
                        );
                    },
                );
            },
        );
    }
    let mut test = composition_test();
    let value = mutable_state_of(false);
    let seen = Rc::new(Cell::new(false));
    let (v, s) = (value.handle(), seen.clone());
    test.compose(move || Content(v, s.clone()));
    assert!(!seen.get());
    value.set(true);
    test.advance();
    assert!(seen.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testProvidesCompositionLocalsAndReturnResult
#[test]
fn with_composition_locals_provides_and_returns_the_result() {
    let mut test = composition_test();
    test.compose(|| {
        assert_eq!(LOCAL_SOME_TEXT.current(), "Default");
        let result = with_composition_locals(
            (
                LOCAL_SOME_TEXT.provides("Test1"),
                LOCAL_SOME_INT.provides(12),
                LOCAL_SOME_OTHER_INT.provides(42),
                LOCAL_SOME_STATIC_INT.provides(50),
            ),
            || {
                assert_eq!(LOCAL_SOME_TEXT.current(), "Test1");
                assert_eq!(LOCAL_SOME_INT.current(), 12);
                assert_eq!(LOCAL_SOME_OTHER_INT.current(), 42);
                assert_eq!(LOCAL_SOME_STATIC_INT.current(), 50);
                CompositionLocalProvider(
                    (LOCAL_SOME_TEXT.provides("Test2"), LOCAL_SOME_STATIC_INT.provides(60)),
                    || {
                        assert_eq!(LOCAL_SOME_TEXT.current(), "Test2");
                        assert_eq!(LOCAL_SOME_STATIC_INT.current(), 60);
                    },
                );
                assert_eq!(LOCAL_SOME_TEXT.current(), "Test1");
                "expected"
            },
        );
        assert_eq!(LOCAL_SOME_TEXT.current(), "Default");
        assert_eq!(LOCAL_SOME_INT.current(), 1);
        assert_eq!(result, "expected");
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testProvidesSingleCompositionLocalAndReturnsResult
#[test]
fn with_composition_local_provides_one_and_returns_the_result() {
    let mut test = composition_test();
    test.compose(|| {
        let result = with_composition_local(LOCAL_SOME_TEXT.provides("Test1"), || {
            assert_eq!(LOCAL_SOME_TEXT.current(), "Test1");
            CompositionLocalProvider(LOCAL_SOME_TEXT.provides("Test2"), || {
                assert_eq!(LOCAL_SOME_TEXT.current(), "Test2");
            });
            assert_eq!(LOCAL_SOME_TEXT.current(), "Test1");
            "expected"
        });
        assert_eq!(LOCAL_SOME_TEXT.current(), "Default");
        assert_eq!(result, "expected");
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, testProvidesSingleCompositionLocalAndReturnsNestedResults
#[test]
fn nested_with_composition_local_returns_the_inner_result() {
    let mut test = composition_test();
    test.compose(|| {
        let result = with_composition_local(LOCAL_SOME_TEXT.provides("Test1"), || {
            with_composition_local(LOCAL_SOME_TEXT.provides("Test2"), || {
                assert_eq!(LOCAL_SOME_TEXT.current(), "Test2");
                "expected"
            })
        });
        assert_eq!(LOCAL_SOME_TEXT.current(), "Default");
        assert_eq!(result, "expected");
    });
}

static BASE_LOCAL: CompositionLocal<i32> = CompositionLocal::new("staticComputed.baseLocal", || 10);
static STATIC_COMPUTED_LOCAL: CompositionLocal<i32> =
    CompositionLocal::static_with_computed_default("staticComputed.local", |scope| {
        scope.current_value(&BASE_LOCAL) * 2
    });

#[composable]
fn ReadIntLocal(local: LocalRef<i32>, on_recompose: Counter) {
    on_recompose.increment();
    Text(&format!("Value: {}", local.0.current()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, staticComputedLocal_fallbackAndStaticOverride
#[test]
fn a_static_computed_local_falls_back_to_its_computation_or_takes_a_static_value() {
    #[composable]
    fn Content(override_value: MutableState<i32>, recomposes: Counter) {
        CompositionLocalProvider(BASE_LOCAL.provides(20), || {
            Text(&format!("Fallback: {}", STATIC_COMPUTED_LOCAL.current()));
        });
        CompositionLocalProvider(STATIC_COMPUTED_LOCAL.provides(override_value.get()), || {
            recomposes.increment();
            Text(&format!("Override: {}", STATIC_COMPUTED_LOCAL.current()));
        });
    }
    let mut test = composition_test();
    let override_value = mutable_state_of(100);
    let recomposes = Counter::new();
    let (o, r) = (override_value.handle(), recomposes.clone());
    test.compose(move || Content(o, r.clone()));
    test.validate(|v| {
        v.text("Fallback: 40");
        v.text("Override: 100");
    });
    assert_eq!(recomposes.get(), 1);
    override_value.set(200);
    test.expect_changes();
    test.validate(|v| {
        v.text("Fallback: 40");
        v.text("Override: 200");
    });
    assert_eq!(recomposes.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, staticComputedLocal_providesDefault_yieldsToExplicitOrOverridesFallback
#[test]
fn provides_default_overrides_a_computed_fallback_but_yields_to_a_provider() {
    static BASE: CompositionLocal<i32> = CompositionLocal::new("providesDefault.base", || 1);
    static LOCAL: CompositionLocal<i32> =
        CompositionLocal::static_with_computed_default("providesDefault.staticComputed", |scope| {
            scope.current_value(&BASE) * 10
        });
    let mut test = composition_test();
    test.compose(|| {
        CompositionLocalProvider(LOCAL.provides_default(50), || {
            Text(&format!("OmittedParent: {}", LOCAL.current()))
        });
        CompositionLocalProvider(LOCAL.provides(100), || {
            CompositionLocalProvider(LOCAL.provides_default(50), || {
                Text(&format!("ProvidedParent: {}", LOCAL.current()))
            });
        });
    });
    test.validate(|v| {
        v.text("OmittedParent: 50");
        v.text("ProvidedParent: 100");
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, staticComputedLocal_propagationOfUpdatedBaseLocal
#[test]
fn a_static_computed_local_follows_the_local_it_reads() {
    #[composable]
    fn Content(base_value: MutableState<i32>) {
        CompositionLocalProvider(BASE_LOCAL.provides(base_value.get()), || {
            Text(&format!("Value: {}", STATIC_COMPUTED_LOCAL.current()));
        });
    }
    let mut test = composition_test();
    let base_value = mutable_state_of(10);
    let b = base_value.handle();
    test.compose(move || Content(b));
    test.validate(|v| v.text("Value: 20"));
    base_value.set(20);
    test.expect_changes();
    test.validate(|v| v.text("Value: 40"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, staticComputedLocal_tracksDynamicDefaults_onlyWhenUnprovided
#[test]
fn a_static_computed_local_tracks_what_it_reads_only_when_not_provided() {
    #[composable]
    fn Content(base_value: MutableState<i32>, unprovided: Counter, provided: Counter) {
        let computed = remember(move || BASE_LOCAL.provides_computed(move |_| base_value.get()));
        CompositionLocalProvider(computed, || {
            ReadIntLocal(LocalRef(&STATIC_COMPUTED_LOCAL), unprovided.clone());
            CompositionLocalProvider(STATIC_COMPUTED_LOCAL.provides(100), || {
                ReadIntLocal(LocalRef(&STATIC_COMPUTED_LOCAL), provided.clone());
            });
        });
    }
    let mut test = composition_test();
    let base_value = mutable_state_of(10);
    let (unprovided, provided) = (Counter::new(), Counter::new());
    let (b, u, p) = (base_value.handle(), unprovided.clone(), provided.clone());
    test.compose(move || Content(b, u.clone(), p.clone()));
    test.validate(|v| {
        v.text("Value: 20");
        v.text("Value: 100");
    });
    assert_eq!((unprovided.get(), provided.get()), (1, 1));
    base_value.set(20);
    test.expect_changes();
    test.validate(|v| {
        v.text("Value: 40");
        v.text("Value: 100");
    });
    assert_eq!((unprovided.get(), provided.get()), (2, 1), "the provided reader didn't recompose");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, staticComputedLocal_changingProvidedValueRecomposesSubtree
#[test]
fn a_static_value_change_recomposes_the_whole_subtree() {
    static LOCAL: CompositionLocal<i32> =
        CompositionLocal::static_with_computed_default("changingProvided.local", |_| 10);
    #[composable]
    fn NonReadChild(on_recompose: Counter) {
        on_recompose.increment();
        Text("NoRead");
    }
    #[composable]
    fn ReadChild(on_recompose: Counter) {
        on_recompose.increment();
        Text(&format!("Read: {}", LOCAL.current()));
    }
    #[composable]
    fn Content(provided_value: MutableState<i32>, without_read: Counter, with_read: Counter) {
        CompositionLocalProvider(LOCAL.provides(provided_value.get()), || {
            NonReadChild(without_read.clone());
            ReadChild(with_read.clone());
        });
    }
    let mut test = composition_test();
    let provided_value = mutable_state_of(100);
    let (without_read, with_read) = (Counter::new(), Counter::new());
    let (p, wo, w) = (provided_value.handle(), without_read.clone(), with_read.clone());
    test.compose(move || Content(p, wo.clone(), w.clone()));
    test.validate(|v| {
        v.text("NoRead");
        v.text("Read: 100");
    });
    assert_eq!((without_read.get(), with_read.get()), (1, 1));
    provided_value.set(200);
    test.expect_changes();
    test.validate(|v| {
        v.text("NoRead");
        v.text("Read: 200");
    });
    assert_eq!((without_read.get(), with_read.get()), (2, 2), "the whole subtree recomposed");
}

/// Logs its lifecycle, as `createRememberObserver` in `withCompositionLocalRememberObserverOrdering`.
struct Observer(&'static str, Log);

impl RememberObserver for Observer {
    fn on_remembered(&self) {
        self.1.push(format!("Remember({})", self.0));
    }
    fn on_forgotten(&self) {
        self.1.push(format!("Forget({})", self.0));
    }
    fn on_abandoned(&self) {
        self.1.push(format!("Abandon({})", self.0));
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, withCompositionLocalRememberObserverOrdering
#[test]
fn with_composition_local_keeps_remember_order() {
    static LOCAL: CompositionLocal<i32> = CompositionLocal::new("rememberOrdering.local", || 0);
    #[composable]
    fn Content(show: MutableState<bool>, events: Log) {
        if show.get() {
            let e = events.clone();
            remember_observer(|| Observer("before", e));
            with_composition_local(LOCAL.provides(100), || {
                let e = events.clone();
                remember_observer(|| Observer("inner", e));
            });
            let e = events.clone();
            remember_observer(|| Observer("after", e));
        }
    }
    let mut test = composition_test();
    let show = mutable_state_of(true);
    let events = Log::new();
    let (s, e) = (show.handle(), events.clone());
    test.compose(move || Content(s, e.clone()));
    assert_eq!(events.take(), ["Remember(before)", "Remember(inner)", "Remember(after)"]);
    show.set(false);
    test.advance();
    assert_eq!(events.take(), ["Forget(after)", "Forget(inner)", "Forget(before)"]);
}

// Compose: compose/runtime/runtime/src/commonMain/kotlin/androidx/compose/runtime/CompositionLocalMap.kt, CompositionLocalMap.get
/// An anchor reads what `current()` reads where it was made, outside composition too, and doesn't
/// change while what it reads can only change through a dynamic local's state.
#[test]
fn an_anchor_reads_the_locals_where_it_was_made() {
    thread_local! {
        static INSIDE: Cell<Option<CompositionLocalAnchor>> = const { Cell::new(None) };
        static OUTSIDE: Cell<Option<CompositionLocalAnchor>> = const { Cell::new(None) };
    }
    let dynamic = mutable_state_of(12);
    let stat = mutable_state_of(50);
    let (d, s) = (dynamic.handle(), stat.handle());
    let mut test = composition_test();
    test.compose(move || {
        OUTSIDE.with(|cell| cell.set(Some(current_composition_local_anchor())));
        CompositionLocalProvider(
            (LOCAL_SOME_INT.provides(d.get()), LOCAL_SOME_STATIC_INT.provides(s.get())),
            || {
                CompositionLocalProvider(LOCAL_SOME_TEXT.provides("Test"), || {
                    INSIDE.with(|cell| cell.set(Some(current_composition_local_anchor())));
                });
            },
        );
    });
    let inside = INSIDE.with(Cell::get).unwrap();
    let outside = OUTSIDE.with(Cell::get).unwrap();
    assert_eq!(inside.get(&LOCAL_SOME_TEXT), "Test");
    assert_eq!(inside.get(&LOCAL_SOME_INT), 12);
    assert_eq!(inside.get(&LOCAL_SOME_STATIC_INT), 50);
    assert_eq!(inside.get(&LOCAL_SOME_OTHER_INT), 1, "the default");
    assert_eq!(outside.get(&LOCAL_SOME_INT), 1, "above the provider");
    assert_eq!(CompositionLocalAnchor::NONE.get(&LOCAL_SOME_INT), 1);

    // A dynamic local's new value: the same anchor, reading it.
    dynamic.set(13);
    test.advance();
    assert_eq!(INSIDE.with(Cell::get), Some(inside));
    assert_eq!(inside.get(&LOCAL_SOME_INT), 13);

    // A static local's: the content composes again, under another anchor.
    stat.set(51);
    test.advance();
    let again = INSIDE.with(Cell::get).unwrap();
    assert_ne!(again, inside);
    assert_eq!(again.get(&LOCAL_SOME_STATIC_INT), 51);
}
