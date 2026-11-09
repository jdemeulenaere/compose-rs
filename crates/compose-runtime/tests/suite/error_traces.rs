//! Compose's `ErrorTraceTests`, adapted: a panic unwinding a pass leaves a trace of the
//! composables it went through, innermost first (D1507).
//!
//! Compose checks frames with names, files and lines from the compiler's source information, its
//! lambdas among them, and in its keys mode a number of frames. Here a trace names the
//! composables `#[composable]` made, those of this test file (and the mock's) kept, as Compose
//! keeps its test file's frames; closures aren't groups, and frames have no line. In keys mode the
//! trace has a key for each open group.

#![allow(non_snake_case)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::stack_trace::*;
use compose_runtime::*;

use crate::mock::*;

/// The panic the tests throw, as `TestComposeException`.
struct TestPanic;

fn throw_test_panic() -> ! {
    std::panic::panic_any(TestPanic)
}

/// Runs `scenario` in each mode, as the Kotlin file's `exceptionTest`: its panic must be the test's,
/// and its trace name `names` (this suite's composables, innermost first), with `keys` frames in
/// keys mode.
fn exception_test(names: &[&str], keys: usize, scenario: impl Fn()) {
    let trace = |mode| {
        set_diagnostic_stack_trace_mode(mode);
        let _ = take_compose_stack_trace();
        let panicked = catch_unwind(AssertUnwindSafe(&scenario)).expect_err("it panics");
        assert!(panicked.is::<TestPanic>(), "the test's own panic");
        set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
        take_compose_stack_trace().expect("a trace")
    };
    let source = trace(ComposeStackTraceMode::SourceInformation);
    let ours: Vec<&str> = source
        .frames
        .iter()
        .filter(|frame| frame.path.is_some_and(|path| path.starts_with("suite::")))
        .filter_map(|frame| frame.name)
        .collect();
    assert_eq!(ours, names, "{source}");
    let keyed = trace(ComposeStackTraceMode::GroupKeys);
    assert!(keyed.frames.iter().all(|frame| frame.name.is_none() && frame.path.is_none()));
    assert_eq!(keyed.frames.len(), keys, "{keyed}");
    // Auto is source information here.
    assert_eq!(trace(ComposeStackTraceMode::Auto).frames, source.frames);
}

/// Composes `content` in a test, recomposing after setting `state` when it's given.
fn compose_then(content: impl Fn(MutableState<bool>) + 'static) -> impl Fn() {
    let content = Rc::new(content);
    move || {
        let mut test = composition_test();
        let state = mutable_state_of(false);
        let s = state.handle();
        let content = content.clone();
        test.compose(move || content(s));
        state.set(true);
        test.advance();
    }
}

/// The Kotlin file's composables (`ErrorTraceComposables.kt`).
#[composable(non_restartable)]
fn InlineWrapper(content: impl FnOnce()) {
    content();
}

#[composable]
fn Wrapper(content: impl Fn() + 'static) {
    content();
}

#[composable(non_restartable)]
fn InlineLinear(content: impl FnOnce()) {
    ReusableComposeNodeWithContent::<ViewApplier>(view("linear"), |_| {}, content);
}

/// A composition of its own composing `content` where it's called: Compose's sets its content
/// in an effect, which runs after the pass.
#[composable]
fn Subcompose(content: Rc<dyn Fn()>) {
    let context = remember_composition_context();
    let child = remember(move || Rc::new(Composition::with_parent(ViewApplier::new(), context)));
    child.set_content(move || content());
}

/// `ComposableWithDefaults`: Kotlin's default argument is a `remember`, in the composable's own
/// group, before its body.
#[composable]
fn ComposableWithDefaults(block: Rc<dyn Fn(&str)>) {
    let value = remember(String::new);
    block(&value);
}

#[composable]
fn MovableWrapper(content: Rc<dyn Fn()>) {
    let movable = remember(move || movable_content_of(move || content()));
    movable();
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContent
#[test]
fn set_content() {
    exception_test(&[], 1, || composition_test().compose(|| throw_test_panic()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recompose
#[test]
fn recompose() {
    exception_test(
        &[],
        1,
        compose_then(|state| {
            if state.get() {
                throw_test_panic();
            }
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentLinear
#[test]
fn set_content_linear() {
    exception_test(&["Linear"], 4, || {
        composition_test().compose(|| Linear!(|| throw_test_panic()))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeLinear
#[test]
fn recompose_linear() {
    exception_test(
        &["Linear"],
        4,
        compose_then(|state| {
            Linear!(move || {
                if state.get() {
                    throw_test_panic();
                }
            })
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentInlineLinear
#[test]
fn set_content_inline_linear() {
    exception_test(&["InlineLinear"], 3, || {
        composition_test().compose(|| InlineLinear(|| throw_test_panic()))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeInlineLinear
#[test]
fn recompose_inline_linear() {
    exception_test(
        &["InlineLinear"],
        3,
        compose_then(|state| {
            InlineLinear(|| {
                if state.get() {
                    throw_test_panic();
                }
            })
        }),
    );
}

/// Five texts in a loop in `wrap`, the fifth panicking once `state` (if given) is set.
fn texts_in_a_loop(wrap: fn(Rc<dyn Fn()>), state: Option<MutableState<bool>>) {
    wrap(Rc::new(move || {
        for index in 0..5 {
            Text("test");
            if index > 3 && state.is_none_or(|state| state.get()) {
                throw_test_panic();
            }
        }
    }));
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentAfterTextInLoopInlineWrapper
#[test]
fn set_content_after_text_in_a_loop_inline_wrapper() {
    exception_test(&["InlineWrapper"], 2, || {
        composition_test()
            .compose(|| texts_in_a_loop(|content| InlineWrapper(move || content()), None))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeAfterTextInLoopInlineWrapper
#[test]
fn recompose_after_text_in_a_loop_inline_wrapper() {
    exception_test(
        &["InlineWrapper"],
        2,
        compose_then(|state| {
            texts_in_a_loop(|content| InlineWrapper(move || content()), Some(state))
        }),
    );
}

/// The Kotlin test's loop in `Linear`.
fn linear_wrap(content: Rc<dyn Fn()>) {
    Linear!(move || content());
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentAfterTextInLoop
#[test]
fn set_content_after_text_in_a_loop() {
    exception_test(&["Linear"], 4, || {
        composition_test().compose(|| texts_in_a_loop(linear_wrap, None))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeAfterTextInLoop
#[test]
fn recompose_after_text_in_a_loop() {
    exception_test(&["Linear"], 4, compose_then(|state| texts_in_a_loop(linear_wrap, Some(state))));
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentSubcomposition
#[test]
fn set_content_subcomposition() {
    exception_test(&["Subcompose"], 4, || {
        composition_test().compose(|| Subcompose(Rc::new(|| throw_test_panic())))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentNestedSubcomposition
#[test]
fn set_content_nested_subcomposition() {
    exception_test(&["Subcompose", "Subcompose"], 7, || {
        composition_test()
            .compose(|| Subcompose(Rc::new(|| Subcompose(Rc::new(|| throw_test_panic())))))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeSubcomposition
#[test]
fn recompose_subcomposition() {
    exception_test(
        &["Subcompose"],
        5,
        compose_then(|state| {
            Subcompose(Rc::new(move || {
                if state.get() {
                    throw_test_panic();
                }
            }))
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentDefaults
#[test]
fn set_content_defaults() {
    exception_test(&["ComposableWithDefaults"], 2, || {
        composition_test().compose(|| ComposableWithDefaults(Rc::new(|_| throw_test_panic())))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeDefaults
#[test]
fn recompose_defaults() {
    exception_test(
        &["ComposableWithDefaults"],
        2,
        compose_then(|state| {
            ComposableWithDefaults(Rc::new(move |_| {
                if state.get() {
                    throw_test_panic();
                }
            }))
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentRemember
#[test]
fn set_content_remember() {
    // Compose's frame is `remember`'s own group; here a slot is no group: the trace is empty.
    exception_test(&[], 1, || {
        composition_test().compose(|| {
            remember(|| -> i32 { throw_test_panic() });
        })
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentNodeUpdate
#[test]
fn set_content_node_update() {
    exception_test(&["LinearWithCallbacks", "Wrapper"], 4, || {
        composition_test().compose(|| {
            Wrapper!(|| {
                LinearWithCallbacks(
                    nothing(),
                    nothing(),
                    nothing(),
                    Rc::new(|| throw_test_panic()),
                    || {},
                )
            })
        })
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeUpdate
#[test]
fn recompose_update() {
    exception_test(
        &["LinearWithCallbacks", "Wrapper"],
        4,
        compose_then(|state| {
            Wrapper!(move || {
                if state.get() {
                    let on_set: Action = Rc::new(|| throw_test_panic());
                    LinearWithCallbacks(nothing(), nothing(), nothing(), on_set, || {});
                }
            })
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, emptySourceInformation
#[test]
fn keyed_groups_without_a_composable() {
    exception_test(&["InlineWrapper"], 3, || {
        let list = [1, 2, 3];
        composition_test().compose(move || {
            InlineWrapper(|| {
                for item in list {
                    key(item, || {
                        if item == 3 {
                            throw_test_panic();
                        }
                    });
                }
            })
        })
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentMovableContent
#[test]
fn set_content_movable_content() {
    exception_test(&["MovableWrapper"], 4, || {
        composition_test().compose(|| MovableWrapper(Rc::new(|| throw_test_panic())))
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeMovableContent
#[test]
fn recompose_movable_content() {
    exception_test(
        &["MovableWrapper"],
        4,
        compose_then(|state| {
            MovableWrapper(Rc::new(move || {
                if state.get() {
                    throw_test_panic();
                }
            }))
        }),
    );
}

/// The mode is the thread's: `None` makes no trace.
#[test]
fn no_trace_in_none_mode() {
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::None);
    let _ = take_compose_stack_trace();
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        composition_test().compose(|| Linear!(|| throw_test_panic()))
    }));
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
    assert!(panicked.is_err());
    assert!(take_compose_stack_trace().is_none());
}

/// A trace is the last panic's that unwound a pass: one left untaken isn't what a later panic,
/// with traces off, gives.
#[test]
fn a_trace_left_untaken_is_not_a_later_panics() {
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
    let _ = take_compose_stack_trace();
    let traced = catch_unwind(AssertUnwindSafe(|| {
        composition_test().compose(|| Linear!(|| throw_test_panic()))
    }));
    assert!(traced.is_err());
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::None);
    let untraced = catch_unwind(AssertUnwindSafe(|| {
        composition_test().compose(|| Wrapper!(|| throw_test_panic()))
    }));
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
    assert!(untraced.is_err());
    assert!(take_compose_stack_trace().is_none());
}

/// A panic that unwinds no pass makes no trace: the sequence number read before it tells the
/// caller the trace left there is an earlier panic's.
#[test]
fn a_trace_tells_which_panic_made_it() {
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
    let _ = take_compose_stack_trace();
    let before = compose_stack_trace_sequence();
    let traced = catch_unwind(AssertUnwindSafe(|| {
        composition_test().compose(|| Linear!(|| throw_test_panic()))
    }));
    assert!(traced.is_err());
    assert_eq!(compose_stack_trace_sequence(), before + 1, "a trace was made");
    // Not taken; a panic outside any pass.
    let mark = compose_stack_trace_sequence();
    let outside = catch_unwind(|| throw_test_panic());
    assert!(outside.is_err());
    assert_eq!(compose_stack_trace_sequence(), mark, "no trace for it");
    let trace = take_compose_stack_trace().expect("the earlier trace");
    assert_eq!(trace.sequence, mark, "made before the mark: not this panic's");
    // A pass's panic after the mark: its trace is after it.
    let traced = catch_unwind(AssertUnwindSafe(|| {
        composition_test().compose(|| Wrapper!(|| throw_test_panic()))
    }));
    assert!(traced.is_err());
    let trace = take_compose_stack_trace().expect("a trace");
    assert!(trace.sequence > mark);
    assert_eq!(trace.names(), ["Wrapper"]);
}

/// What `Leaky` composes keeps its composition in `kept`, beyond its place.
type Kept = Rc<std::cell::RefCell<Option<Rc<Composition>>>>;

#[composable]
fn Leaky(kept: Kept) {
    let context = remember_composition_context();
    let child = remember(move || Rc::new(Composition::with_parent(ViewApplier::new(), context)));
    *kept.borrow_mut() = Some(child.clone());
    child.set_content(|| {});
}

#[composable]
fn Holder(show: MutableState<bool>, reuse: bool, kept: Kept) {
    if show.get() {
        Leaky(kept.clone());
    } else if reuse {
        Wrapper!(|| Wrapper!(|| Wrapper!(|| Linear!(|| {}))));
    }
}

/// The trace of a panic in the pass alone of the composition `Leaky` made, kept, after its place
/// was removed if `remove`, its groups then given to other composables if `reuse`.
fn kept_composition_trace(remove: bool, reuse: bool) -> Vec<&'static str> {
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::SourceInformation);
    let _ = take_compose_stack_trace();
    let kept: Kept = Default::default();
    let mut test = composition_test();
    let show = mutable_state_of(true);
    let (s, k) = (show.handle(), kept.clone());
    test.compose(move || Holder(s, reuse, k.clone()));
    let child = kept.borrow_mut().take().expect("composed");
    if remove {
        show.set(false);
        test.advance();
    }
    let panicked = catch_unwind(AssertUnwindSafe(|| child.set_content(|| throw_test_panic())));
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
    assert!(panicked.expect_err("it panics").is::<TestPanic>());
    child.dispose();
    take_compose_stack_trace().expect("a trace").names()
}

/// A composition's pass alone goes on through its place in its parent while it's there.
#[test]
fn a_kept_composition_traces_through_its_place() {
    assert_eq!(
        kept_composition_trace(false, false),
        ["rememberCompositionContext", "Leaky", "Holder"]
    );
}

/// Its place removed, what was above it isn't on the trace: the place's group is freed, its
/// parent link stale.
#[test]
fn a_removed_place_is_not_traced() {
    assert_eq!(kept_composition_trace(true, false), Vec::<&str>::new());
}

/// Its place removed and its groups given to other composables: they aren't on the trace either.
#[test]
fn a_removed_place_reused_by_another_composable_is_not_traced() {
    assert_eq!(kept_composition_trace(true, true), Vec::<&str>::new());
}

#[composable]
fn NodeWithCallbacks(
    #[default(nothing())] on_attach: Action,
    #[default(nothing())] on_reuse: Action,
    #[default(nothing())] on_deactivate: Action,
) {
    ReusableComposeNode::<ViewApplier>(
        move || {
            let mut view = View::new("node_w_callbacks");
            view.callbacks = Some(Rc::new(ViewCallbacks {
                on_attach: Some(Box::new(move || on_attach())),
                on_reuse: Some(Box::new(move || on_reuse())),
                on_deactivate: Some(Box::new(move || on_deactivate())),
                on_release: None,
            }));
            view
        },
        |_| {},
    );
}

type MovableBool = Callback<dyn Fn(bool)>;

#[composable]
fn WrappedMovableContent(content: Rc<dyn Fn(bool)>, wrap: Rc<dyn Fn(MovableBool)>) {
    let movable = remember(move || movable_content_of_with(move |flag| content(flag)));
    wrap(movable);
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentRememberObserver
#[test]
fn set_content_remember_observer() {
    exception_test(&[], 1, || {
        composition_test().compose(|| {
            DisposableEffect((), |_| throw_test_panic());
        })
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeRememberObserver
#[test]
fn recompose_remember_observer() {
    exception_test(
        &[],
        1,
        compose_then(|state| {
            DisposableEffect(state.get(), move |scope| {
                if state.get() {
                    throw_test_panic();
                }
                scope.on_dispose(|| {})
            });
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, nodeReuse_gapBuffer, nodeReuse_linkBuffer
#[test]
fn node_reuse() {
    exception_test(
        &["NodeWithCallbacks"],
        4,
        compose_then(|state| {
            ReusableContent(state.get(), || {
                NodeWithCallbacks!(on_reuse = Rc::new(|| throw_test_panic()));
            })
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, nodeDeactivate
#[test]
fn node_deactivate() {
    exception_test(&["NodeWithCallbacks"], 4, || {
        let mut test = composition_test();
        let active = mutable_state_of(true);
        let a = active.handle();
        test.compose(move || {
            ReusableContentHost(a.get(), || {
                NodeWithCallbacks!(on_deactivate = Rc::new(|| throw_test_panic()));
            });
        });
        active.set(false);
        test.advance();
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, setContentNodeAttach
#[test]
fn set_content_node_attach() {
    exception_test(&["NodeWithCallbacks", "InlineWrapper"], 4, || {
        composition_test().compose(|| {
            InlineWrapper(|| {
                NodeWithCallbacks!(on_attach = Rc::new(|| throw_test_panic()));
            })
        })
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeNodeAttach
#[test]
fn recompose_node_attach() {
    exception_test(
        &["NodeWithCallbacks", "Wrapper"],
        4,
        compose_then(|state| {
            Wrapper!(move || {
                if state.get() {
                    NodeWithCallbacks!(on_attach = Rc::new(|| throw_test_panic()));
                }
            })
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, recomposeNodeAttachInlineWrapper
#[test]
fn recompose_node_attach_inline_wrapper() {
    exception_test(
        &["NodeWithCallbacks", "InlineWrapper"],
        4,
        compose_then(|state| {
            InlineWrapper(|| {
                if state.get() {
                    NodeWithCallbacks!(on_attach = Rc::new(|| throw_test_panic()));
                }
            })
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, moveMovableContentOf
#[test]
fn move_movable_content_of() {
    exception_test(
        &["Wrapper", "WrappedMovableContent"],
        5,
        compose_then(|state| {
            WrappedMovableContent(
                Rc::new(|flag| {
                    if flag {
                        throw_test_panic();
                    }
                }),
                Rc::new(move |movable| {
                    if !state.get() {
                        movable(false);
                    } else {
                        Wrapper!(move || movable(true));
                    }
                }),
            );
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, moveMovableContentOfStateRead
#[test]
fn move_movable_content_of_state_read() {
    exception_test(
        &["Wrapper", "WrappedMovableContent"],
        5,
        compose_then(|state| {
            WrappedMovableContent(
                Rc::new(move |_| {
                    if state.get() {
                        throw_test_panic();
                    }
                }),
                Rc::new(move |movable| {
                    let s = state.get();
                    if !s {
                        movable(s);
                    } else {
                        Wrapper!(move || movable(s));
                    }
                }),
            );
        }),
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/tooling/ErrorTraceTests.kt, moveMovableContentOfReverse
#[test]
fn move_movable_content_of_reverse() {
    exception_test(&["Wrapper", "WrappedMovableContent"], 5, || {
        let mut test = composition_test();
        let state = mutable_state_of(true);
        let s = state.handle();
        test.compose(move || {
            WrappedMovableContent(
                Rc::new(|flag| {
                    if flag {
                        throw_test_panic();
                    }
                }),
                Rc::new(move |movable| {
                    if !s.get() {
                        movable(false);
                    } else {
                        Wrapper!(move || movable(true));
                    }
                }),
            );
        });
        state.set(false);
        test.advance();
    });
}

#[test]
fn move_movable_content_out_of_wrapper() {
    exception_test(&["WrappedMovableContent"], 4, || {
        let mut test = composition_test();
        let state = mutable_state_of(true);
        let s = state.handle();
        test.compose(move || {
            WrappedMovableContent(
                Rc::new(|flag| {
                    if !flag {
                        throw_test_panic();
                    }
                }),
                Rc::new(move |movable| {
                    if !s.get() {
                        movable(false);
                    } else {
                        Wrapper!(move || movable(true));
                    }
                }),
            );
        });
        state.set(false);
        test.advance();
    });
}

#[test]
fn deferred_effects_and_subcomposition_through_disposable_effect_trace_ancestors() {
    // Remember observer inside nested composables.
    exception_test(&["Linear", "Wrapper"], 5, || {
        composition_test().compose(|| {
            Wrapper!(|| {
                Linear!(|| {
                    DisposableEffect((), |_| throw_test_panic());
                })
            })
        })
    });
    // SideEffect and KeyedSideEffect after the pass.
    exception_test(&["Wrapper"], 2, || {
        composition_test().compose(|| {
            Wrapper!(|| {
                SideEffect(|| throw_test_panic());
            })
        })
    });
    exception_test(&["Wrapper"], 2, || {
        composition_test().compose(|| {
            Wrapper!(|| {
                KeyedSideEffect(1, || throw_test_panic());
            })
        })
    });
    // Compose's real Subcompose sets its child's content inside DisposableEffect: an inner pass
    // panic keeps the child's frames plus its parent's place, not clobbered by the outer effect.
    #[composable]
    fn DeferredSubcompose(content: Rc<dyn Fn()>) {
        let context = remember_composition_context();
        let child =
            remember(move || Rc::new(Composition::with_parent(ViewApplier::new(), context)));
        let current = remember_updated_state(content);
        let c = child.clone();
        DisposableEffect(child, move |scope| {
            c.set_content(move || (current.get())());
            let c = c.clone();
            scope.on_dispose(move || c.dispose())
        });
    }
    exception_test(&["InlineWrapper", "DeferredSubcompose", "Wrapper"], 6, || {
        composition_test().compose(|| {
            Wrapper!(|| {
                DeferredSubcompose(Rc::new(|| InlineWrapper(|| throw_test_panic())));
            })
        })
    });
    // Deferred panic in a child composition set synchronously inside a parent pass extends onto
    // the parent's open frames (`extended_by`).
    exception_test(&["InlineWrapper", "Subcompose", "Wrapper"], 6, || {
        composition_test().compose(|| {
            Wrapper!(|| {
                Subcompose(Rc::new(|| {
                    InlineWrapper(|| {
                        DisposableEffect((), |_| throw_test_panic());
                    })
                }));
            })
        })
    });
}

#[test]
fn deferred_panic_in_none_mode_clears_earlier_trace() {
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
    let _ = take_compose_stack_trace();
    let traced = catch_unwind(AssertUnwindSafe(|| {
        composition_test().compose(|| Linear!(|| throw_test_panic()))
    }));
    assert!(traced.is_err());
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::None);
    let untraced = catch_unwind(AssertUnwindSafe(|| {
        composition_test().compose(|| {
            Wrapper!(|| {
                DisposableEffect((), |_| throw_test_panic());
            })
        })
    }));
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
    assert!(untraced.is_err());
    assert!(take_compose_stack_trace().is_none());
}

#[test]
fn paused_composition_apply_traces_deferred_callbacks() {
    exception_test(&["NodeWithCallbacks", "Wrapper"], 8, || {
        let composition =
            Composition::pausable(ViewApplier::new(), Recomposer::current_thread().as_context());
        let paused = composition.set_pausable_content(|| {
            Wrapper!(|| {
                NodeWithCallbacks!(on_attach = Rc::new(|| throw_test_panic()));
            })
        });
        while !paused.is_complete() {
            paused.resume(|| false);
        }
        paused.apply();
    });
    exception_test(&["Wrapper"], 6, || {
        let composition =
            Composition::pausable(ViewApplier::new(), Recomposer::current_thread().as_context());
        let paused = composition.set_pausable_content(|| {
            Wrapper!(|| {
                DisposableEffect((), |_| throw_test_panic());
            })
        });
        while !paused.is_complete() {
            paused.resume(|| false);
        }
        paused.apply();
    });
}
