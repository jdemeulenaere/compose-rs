//! Alignment lines, as Compose's `AlignmentLinesTest` and `AlignmentLineTest`: given with a size,
//! read of what's measured, inherited through what's placed, a change telling who read. Density 1.

use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::UiHost;
use compose_ui::*;
use support::*;

type Count = Rc<Cell<u32>>;

fn count() -> Count {
    Rc::default()
}

fn bump(count: &Count) {
    count.set(count.get() + 1);
}

fn dp(pixels: i32) -> Dp {
    (pixels as f32).dp()
}

/// A layout with no content giving `lines`, at zero size, in a block.
fn lines_at<L: Into<AlignmentLine> + Copy + 'static>(lines: Vec<(L, i32)>) {
    let policy = measure_policy(move |scope, _, _| {
        scope.provide_alignment_lines(lines.iter().copied());
        scope.layout_with(0, 0, |_| {})
    });
    Layout(Modifier, policy, callback(|| {}));
}

/// A layout sized by its largest child, placing all at the origin in a block, as Compose's `Wrap`.
fn wrap_with(modifier: Modifier, content: impl Fn() + 'static) {
    let policy = measure_policy(|scope, measurables, constraints| {
        let placeables: Vec<_> =
            measurables.iter().map(|it| scope.measure(*it, constraints)).collect();
        let width = placeables.iter().map(|it| it.width).max().unwrap_or(0);
        let height = placeables.iter().map(|it| it.height).max().unwrap_or(0);
        scope.layout_with(width, height, move |placement| {
            for placeable in &placeables {
                placement.place(placeable, 0, 0);
            }
        })
    });
    Layout(modifier, policy, callback(content));
}

fn wrap(content: impl Fn() + 'static) {
    wrap_with(Modifier, content);
}

/// A layout measuring its only child and doing with it what `measure` says.
fn one_child(
    measure: impl Fn(&mut MeasureScope<'_>, Placeable, Constraints) -> MeasureResult + 'static,
    content: impl Fn() + 'static,
) {
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        measure(scope, placeable, constraints)
    });
    Layout(Modifier, policy, callback(content));
}

// ---- AlignmentLinesTest ----------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines
#[test]
fn lines_are_read_of_a_child_and_merged_in_its_parent() {
    let vertical = VerticalAlignmentLine::new(i32::min);
    let horizontal = HorizontalAlignmentLine::new(i32::max);
    let asked = count();
    let a = asked.clone();
    let _host = host(move || {
        let (a, a2) = (a.clone(), a.clone());
        one_child(
            move |scope, placeable, _| {
                bump(&a2);
                assert_eq!(scope.alignment_line(&placeable, vertical), 10);
                assert_eq!(scope.alignment_line(&placeable, horizontal), 20);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            },
            move || {
                let a = a.clone();
                let policy = measure_policy(move |scope, measurables, constraints| {
                    let first = scope.measure(measurables[0], constraints);
                    let second = scope.measure(measurables[1], constraints);
                    assert_eq!(scope.alignment_line(&first, vertical), 10);
                    assert_eq!(scope.alignment_line(&first, horizontal), 20);
                    assert_eq!(scope.alignment_line(&second, vertical), 20);
                    assert_eq!(scope.alignment_line(&second, horizontal), 10);
                    bump(&a);
                    scope.layout_with(0, 0, move |placement| {
                        placement.place(&first, 0, 0);
                        placement.place(&second, 0, 0);
                    })
                });
                Layout(
                    Modifier,
                    policy,
                    callback(move || {
                        wrap(move || {
                            let policy = measure_policy(move |scope, _, _| {
                                scope.provide_alignment_lines([
                                    (AlignmentLine::from(vertical), 10),
                                    (AlignmentLine::from(horizontal), 20),
                                ]);
                                scope.layout_with(0, 0, |_| {})
                            });
                            Layout(Modifier, policy, callback(|| {}));
                        });
                        wrap(move || {
                            let policy = measure_policy(move |scope, _, _| {
                                scope.provide_alignment_lines([
                                    (AlignmentLine::from(vertical), 20),
                                    (AlignmentLine::from(horizontal), 10),
                                ]);
                                scope.layout_with(0, 0, |_| {})
                            });
                            Layout(Modifier, policy, callback(|| {}));
                        });
                    }),
                );
            },
        );
    });
    assert_eq!(asked.get(), 2);
}

/// A layout placing the first of two children giving a line each, in a block.
fn two_lines(
    first: VerticalAlignmentLine,
    second: VerticalAlignmentLine,
    place_second: bool,
    counts: [Count; 4],
) -> impl Fn() + 'static {
    move || {
        let [m1, l1, m2, l2] = counts.clone();
        let policy = measure_policy(move |scope, measurables, constraints| {
            let one = scope.measure(measurables[0], constraints);
            let two = scope.measure(measurables[1], constraints);
            scope.layout_with(0, 0, move |placement| {
                placement.place(&one, 0, 0);
                if place_second {
                    placement.place(&two, 0, 0);
                }
            })
        });
        Layout(
            Modifier,
            policy,
            callback(move || {
                let (m1, l1, m2, l2) = (m1.clone(), l1.clone(), m2.clone(), l2.clone());
                Layout(
                    Modifier,
                    measure_policy(move |scope, _, _| {
                        bump(&m1);
                        scope.provide_alignment_lines([(first, 10)]);
                        let l1 = l1.clone();
                        scope.layout_with(0, 0, move |_| bump(&l1))
                    }),
                    callback(|| {}),
                );
                Layout(
                    Modifier,
                    measure_policy(move |scope, _, _| {
                        bump(&m2);
                        scope.provide_alignment_lines([(second, 20)]);
                        let l2 = l2.clone();
                        scope.layout_with(0, 0, move |_| bump(&l2))
                    }),
                    callback(|| {}),
                );
            }),
        );
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_areNotInheritedFromInvisibleChildren
#[test]
fn lines_of_what_s_not_placed_aren_t_inherited() {
    let (first, second) =
        (VerticalAlignmentLine::new(i32::min), VerticalAlignmentLine::new(i32::min));
    let asked = count();
    let a = asked.clone();
    let _host = host(move || {
        let a = a.clone();
        one_child(
            move |scope, placeable, _| {
                bump(&a);
                assert_eq!(scope.alignment_line(&placeable, first), 10);
                assert_eq!(scope.alignment_line(&placeable, second), AlignmentLine::UNSPECIFIED);
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            two_lines(first, second, false, [count(), count(), count(), count()]),
        );
    });
    assert_eq!(asked.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_doNotCauseMultipleMeasuresOrLayouts
#[test]
fn reading_lines_measures_and_places_once() {
    let (first, second) =
        (VerticalAlignmentLine::new(i32::min), VerticalAlignmentLine::new(i32::min));
    let counts = [count(), count(), count(), count()];
    let c = counts.clone();
    let _host = host(move || {
        one_child(
            move |scope, placeable, _| {
                assert_eq!(scope.alignment_line(&placeable, first), 10);
                assert_eq!(scope.alignment_line(&placeable, second), 20);
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            two_lines(first, second, true, c.clone()),
        );
    });
    assert_eq!(counts.iter().map(|it| it.get()).collect::<Vec<_>>(), [1, 1, 1, 1]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_onlyLayoutEarlyWhenNeeded
#[test]
fn only_what_a_line_needs_is_placed_early() {
    let (first, second) =
        (VerticalAlignmentLine::new(i32::min), VerticalAlignmentLine::new(i32::min));
    let (m1, l1, m2, l2) = (count(), count(), count(), count());
    let (a, b, c, d) = (m1.clone(), l1.clone(), m2.clone(), l2.clone());
    let _host = host(move || {
        let (a, b, c, d) = (a.clone(), b.clone(), c.clone(), d.clone());
        // The outer layout never places the inner one.
        one_child(
            |scope, placeable, _| scope.layout_with(placeable.width, placeable.height, |_| {}),
            move || {
                let (a, b, c, d) = (a.clone(), b.clone(), c.clone(), d.clone());
                let policy = measure_policy(move |scope, measurables, constraints| {
                    let one = scope.measure(measurables[0], constraints);
                    assert_eq!(scope.alignment_line(&one, first), 10);
                    let two = scope.measure(measurables[1], constraints);
                    scope.layout_with(0, 0, move |placement| {
                        placement.place(&one, 0, 0);
                        placement.place(&two, 0, 0);
                    })
                });
                Layout(
                    Modifier,
                    policy,
                    callback(move || {
                        let (a, b, c, d) = (a.clone(), b.clone(), c.clone(), d.clone());
                        Layout(
                            Modifier,
                            measure_policy(move |scope, _, _| {
                                bump(&a);
                                scope.provide_alignment_lines([(first, 10)]);
                                let b = b.clone();
                                scope.layout_with(0, 0, move |_| bump(&b))
                            }),
                            callback(|| {}),
                        );
                        Layout(
                            Modifier,
                            measure_policy(move |scope, _, _| {
                                bump(&c);
                                scope.provide_alignment_lines([(second, 20)]);
                                let d = d.clone();
                                scope.layout_with(0, 0, move |_| bump(&d))
                            }),
                            callback(|| {}),
                        );
                    }),
                );
            },
        );
    });
    assert_eq!((m1.get(), l1.get(), m2.get(), l2.get()), (1, 1, 1, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_canBeQueriedInThePositioningBlock
#[test]
fn lines_are_read_while_placing_of_what_s_measured_there_too() {
    let line = VerticalAlignmentLine::new(i32::min);
    let asked = count();
    let a = asked.clone();
    let _host = host(move || {
        let a = a.clone();
        one_child(
            |scope, placeable, _| scope.layout_with(placeable.width, placeable.height, |_| {}),
            move || {
                let a = a.clone();
                let policy = measure_policy(move |scope, measurables, constraints| {
                    let one = scope.measure(measurables[0], constraints);
                    let second = measurables[1];
                    let a = a.clone();
                    scope.layout_with(0, 0, move |placement| {
                        assert_eq!(placement.alignment_line(&one, line), 10);
                        let two = placement.measure(second, constraints);
                        assert_eq!(placement.alignment_line(&two, line), 20);
                        bump(&a);
                    })
                });
                Layout(
                    Modifier,
                    policy,
                    callback(move || {
                        lines_at(vec![(line, 10)]);
                        lines_at(vec![(line, 20)]);
                    }),
                );
            },
        );
    });
    // The outer one doesn't place it: its block's run for no line, not at all.
    assert_eq!(asked.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_doNotCauseExtraLayout_whenQueriedAfterPositioning
#[test]
fn reading_before_and_after_placing_places_once() {
    let line = VerticalAlignmentLine::new(i32::min);
    let layouts = count();
    let l = layouts.clone();
    let _host = host(move || {
        let l = l.clone();
        wrap(move || {
            let l = l.clone();
            one_child(
                move |scope, placeable, _| {
                    scope.layout_with(0, 0, move |placement| {
                        assert_eq!(placement.alignment_line(&placeable, line), 10);
                        placement.place(&placeable, 0, 0);
                        assert_eq!(placement.alignment_line(&placeable, line), 10);
                    })
                },
                move || {
                    let l = l.clone();
                    Layout(
                        Modifier,
                        measure_policy(move |scope, _, _| {
                            scope.provide_alignment_lines([(line, 10)]);
                            let l = l.clone();
                            scope.layout_with(0, 0, move |_| bump(&l))
                        }),
                        callback(|| {}),
                    );
                },
            );
        });
    });
    assert_eq!(layouts.get(), 1);
}

struct Recompose {
    state: OwnedState<i32>,
    measures: Count,
    layouts: Count,
    read: Rc<Cell<i32>>,
    host: UiHost,
}

/// A layout reading a child's line as `read_in` says, what it gives in `offset`.
fn recompose(
    reads: impl Fn(
        &mut MeasureScope<'_>,
        Measurable,
        Constraints,
        VerticalAlignmentLine,
        Count,
        Rc<Cell<i32>>,
    ) -> MeasureResult
    + 'static,
) -> Recompose {
    let line = VerticalAlignmentLine::new(i32::min);
    let state = mutable_state_of(10);
    let (measures, layouts, read) = (count(), count(), Rc::new(Cell::new(0)));
    let (st, m, l, r) = (state.handle(), measures.clone(), layouts.clone(), read.clone());
    let reads = Rc::new(reads);
    let host = host(move || {
        let (m, l, r, reads) = (m.clone(), l.clone(), r.clone(), reads.clone());
        let policy = measure_policy(move |scope, measurables, constraints| {
            bump(&m);
            reads(scope, measurables[0], constraints, line, l.clone(), r.clone())
        });
        Layout(
            Modifier,
            policy,
            callback(move || {
                Layout(
                    Modifier,
                    measure_policy(move |scope, _, _| {
                        scope.provide_alignment_lines([(line, st.get())]);
                        scope.layout_with(0, 0, |_| {})
                    }),
                    callback(|| {}),
                );
            }),
        );
    });
    Recompose { state, measures, layouts, read, host }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_recomposeCorrectly
#[test]
fn a_line_read_measuring_measures_again() {
    let mut test = recompose(|scope, child, constraints, line, layouts, read| {
        let placeable = scope.measure(child, constraints);
        read.set(scope.alignment_line(&placeable, line));
        scope.layout_with(placeable.width, placeable.height, move |_| bump(&layouts))
    });
    assert_eq!((test.measures.get(), test.layouts.get(), test.read.get()), (1, 1, 10));
    test.state.set(20);
    test.host.frame();
    assert_eq!((test.measures.get(), test.layouts.get(), test.read.get()), (2, 2, 20));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_recomposeCorrectly_whenQueriedInLayout
#[test]
fn a_line_read_placing_places_again() {
    let mut test = recompose(|scope, child, constraints, line, layouts, read| {
        let placeable = scope.measure(child, constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            read.set(placement.alignment_line(&placeable, line));
            bump(&layouts);
        })
    });
    assert_eq!((test.measures.get(), test.layouts.get(), test.read.get()), (1, 1, 10));
    test.state.set(20);
    test.host.frame();
    assert_eq!((test.measures.get(), test.layouts.get(), test.read.get()), (1, 2, 20));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_recomposeCorrectly_whenMeasuredAndQueriedInLayout
#[test]
fn a_line_of_what_s_measured_placing_places_again() {
    let mut test = recompose(|scope, child, constraints, line, layouts, read| {
        scope.layout_with(1, 1, move |placement| {
            let placeable = placement.measure(child, constraints);
            read.set(placement.alignment_line(&placeable, line));
            bump(&layouts);
        })
    });
    assert_eq!((test.measures.get(), test.layouts.get(), test.read.get()), (1, 1, 10));
    test.state.set(20);
    test.host.frame();
    assert_eq!((test.measures.get(), test.layouts.get(), test.read.get()), (1, 2, 20));
}

thread_local! {
    static MERGES: Cell<u32> = const { Cell::new(0) };
}

fn merges() -> u32 {
    MERGES.with(Cell::get)
}

fn count_merge_first(first: i32, _: i32) -> i32 {
    MERGES.with(|it| it.set(it.get() + 1));
    first
}

fn count_merge_zero(_: i32, _: i32) -> i32 {
    MERGES.with(|it| it.set(it.get() + 1));
    0
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_onlyComputesAlignmentLinesWhenNeeded
#[test]
fn lines_are_worked_out_only_when_asked() {
    MERGES.with(|it| it.set(0));
    let line = VerticalAlignmentLine::new(count_merge_zero);
    let (offset, position) = (mutable_state_of(10), mutable_state_of(10));
    let (o, p) = (offset.handle(), position.handle());
    let mut host = host(move || {
        one_child(
            move |scope, placeable, _| {
                if o.get() < 15 {
                    scope.alignment_line(&placeable, line);
                }
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            move || {
                let policy = measure_policy(move |scope, measurables, constraints| {
                    o.get();
                    let one = scope.measure(measurables[0], constraints);
                    let two = scope.measure(measurables[1], constraints);
                    scope.layout_with(0, 0, move |placement| {
                        placement.place(&one, 0, 0);
                        placement.place(&two, 0, 0);
                    })
                });
                Layout(
                    Modifier,
                    policy,
                    callback(move || {
                        for _ in 0..2 {
                            Layout(
                                Modifier,
                                measure_policy(move |scope, _, _| {
                                    o.get();
                                    scope.provide_alignment_lines([(line, p.get())]);
                                    scope.layout_with(0, 0, |_| {})
                                }),
                                callback(|| {}),
                            );
                        }
                    }),
                );
            },
        );
    });
    assert_eq!(merges(), 1);
    offset.set(20);
    position.set(20);
    host.frame();
    assert_eq!(merges(), 1);
    offset.set(10);
    position.set(30);
    host.frame();
    assert_eq!(merges(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_providedLinesOverrideInherited
#[test]
fn a_layout_s_own_line_overrides_what_it_inherits() {
    let line = VerticalAlignmentLine::new(i32::min);
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        one_child(
            move |scope, placeable, _| {
                s.set(scope.alignment_line(&placeable, line));
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            move || {
                one_child(
                    move |scope, placeable, _| {
                        scope.provide_alignment_lines([(line, 20)]);
                        scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
                    },
                    move || lines_at(vec![(line, 10)]),
                );
            },
        );
    });
    assert_eq!(seen.get(), 20);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_areRecalculatedCorrectlyOnRelayout_withNoRemeasure
#[test]
fn a_child_moved_gives_its_new_line_measuring_nothing() {
    let line = VerticalAlignmentLine::new(i32::min);
    let offset = mutable_state_of(0);
    let (inner_measures, inner_layouts, outer_measures, outer_layouts) =
        (count(), count(), count(), count());
    let (im, il, om, ol) = (
        inner_measures.clone(),
        inner_layouts.clone(),
        outer_measures.clone(),
        outer_layouts.clone(),
    );
    let o = offset.handle();
    let mut host = host(move || {
        let (im, il, om, ol) = (im.clone(), il.clone(), om.clone(), ol.clone());
        one_child(
            move |scope, placeable, _| {
                let (width, height) = (placeable.width.max(10), placeable.height.max(10));
                scope.layout_with(width, height, move |placement| {
                    assert_eq!(placement.alignment_line(&placeable, line), o.get() + 10);
                    placement.place(&placeable, 0, 0);
                })
            },
            move || {
                let (im, il, om, ol) = (im.clone(), il.clone(), om.clone(), ol.clone());
                one_child(
                    move |scope, placeable, _| {
                        bump(&om);
                        let ol = ol.clone();
                        scope.layout_with(0, 0, move |placement| {
                            bump(&ol);
                            placement.place(&placeable, o.get(), 0);
                        })
                    },
                    move || {
                        let (im, il) = (im.clone(), il.clone());
                        wrap(move || {
                            let (im, il) = (im.clone(), il.clone());
                            wrap(move || {
                                let (im, il) = (im.clone(), il.clone());
                                Layout(
                                    Modifier,
                                    measure_policy(move |scope, _, _| {
                                        bump(&im);
                                        scope.provide_alignment_lines([(line, 10)]);
                                        let il = il.clone();
                                        scope.layout_with(0, 0, move |_| bump(&il))
                                    }),
                                    callback(|| {}),
                                );
                            });
                        });
                    },
                );
            },
        );
    });
    let counts = |what: [&Count; 4]| what.map(|it| it.get());
    let all = [&inner_measures, &inner_layouts, &outer_measures, &outer_layouts];
    assert_eq!(counts(all), [1, 1, 1, 1]);
    offset.set(10);
    host.frame();
    assert_eq!(counts(all), [1, 1, 1, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_whenQueriedAfterPlacing
#[test]
fn a_line_read_after_placing_places_nothing_again() {
    let line = VerticalAlignmentLine::new(i32::min);
    let layouts = count();
    let l = layouts.clone();
    let _host = host(move || {
        let l = l.clone();
        wrap(move || {
            let l = l.clone();
            one_child(
                move |scope, placeable, _| {
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        placement.place(&placeable, 0, 0);
                        assert_eq!(placement.alignment_line(&placeable, line), 10);
                    })
                },
                move || {
                    let l = l.clone();
                    wrap(move || {
                        let l = l.clone();
                        wrap(move || {
                            let l = l.clone();
                            Layout(
                                Modifier,
                                measure_policy(move |scope, _, constraints| {
                                    scope.provide_alignment_lines([(line, 10)]);
                                    let l = l.clone();
                                    scope.layout_with(
                                        constraints.min_width,
                                        constraints.min_height,
                                        move |_| bump(&l),
                                    )
                                }),
                                callback(|| {}),
                            );
                        });
                    });
                },
            );
        });
    });
    assert_eq!(layouts.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_whenQueriedAfterPlacing_haveCorrectNumberOfLayouts
#[test]
fn reads_placing_work_a_line_out_once_each_time_it_changes() {
    MERGES.with(|it| it.set(0));
    let line = VerticalAlignmentLine::new(count_merge_first);
    let (offset, position_state) = (mutable_state_of(10), mutable_state_of(10));
    let position = Rc::new(Cell::new(10));
    let child_layouts = count();
    let (o, ps, p, cl) =
        (offset.handle(), position_state.handle(), position.clone(), child_layouts.clone());
    let mut host = host(move || {
        let (p, cl) = (p.clone(), cl.clone());
        one_child(
            move |scope, placeable, _| {
                let (width, height) = (placeable.width.max(10), placeable.height.max(10));
                scope.layout_with(width, height, move |placement| {
                    o.get();
                    placement.place(&placeable, 0, 0);
                })
            },
            move || {
                let (p, cl) = (p.clone(), cl.clone());
                let p2 = p.clone();
                one_child(
                    move |scope, placeable, _| {
                        let p = p2.clone();
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            if o.get() > 15 {
                                assert_eq!(placement.alignment_line(&placeable, line), p.get());
                            }
                            placement.place(&placeable, 0, 0);
                            if o.get() > 5 {
                                assert_eq!(placement.alignment_line(&placeable, line), p.get());
                            }
                        })
                    },
                    move || {
                        let cl = cl.clone();
                        // `WrapForceRelayout`.
                        let force = measure_policy(move |scope, measurables, constraints| {
                            let placeable = scope.measure(measurables[0], constraints);
                            scope.layout_with(placeable.width, placeable.height, move |placement| {
                                o.get();
                                placement.place(&placeable, 0, 0);
                            })
                        });
                        Layout(
                            Modifier,
                            force,
                            callback(move || {
                                let cl = cl.clone();
                                let policy =
                                    measure_policy(move |scope, measurables, constraints| {
                                        let placeables: Vec<_> = measurables
                                            .iter()
                                            .map(|it| scope.measure(*it, constraints))
                                            .collect();
                                        let cl = cl.clone();
                                        scope.layout_with(
                                            constraints.min_width,
                                            constraints.min_height,
                                            move |placement| {
                                                o.get();
                                                for placeable in &placeables {
                                                    placement.place(placeable, 0, 0);
                                                }
                                                bump(&cl);
                                            },
                                        )
                                    });
                                Layout(
                                    Modifier,
                                    policy,
                                    callback(move || {
                                        for _ in 0..2 {
                                            Layout(
                                                Modifier,
                                                measure_policy(move |scope, _, constraints| {
                                                    scope.provide_alignment_lines([(
                                                        line,
                                                        ps.get(),
                                                    )]);
                                                    scope.layout_with(
                                                        constraints.min_width,
                                                        constraints.min_height,
                                                        move |_| {
                                                            o.get();
                                                        },
                                                    )
                                                }),
                                                callback(|| {}),
                                            );
                                        }
                                    }),
                                );
                            }),
                        );
                    },
                );
            },
        );
    });
    let total = || child_layouts.get() + merges();
    assert_eq!(total(), 2);
    offset.set(1);
    host.frame();
    assert_eq!(total(), 3);
    let change = |to: i32, expected: u32, host: &mut UiHost, what: &dyn Fn() -> u32| {
        offset.set(to);
        position.set(30 - position.get());
        position_state.set(30 - position_state.get());
        host.frame();
        assert_eq!(what(), expected, "at {to}");
    };
    change(10, 5, &mut host, &total);
    change(12, 7, &mut host, &total);
    change(17, 9, &mut host, &total);
    change(12, 11, &mut host, &total);
    change(1, 13, &mut host, &total);
    change(10, 15, &mut host, &total);
}

#[derive(Clone, Copy)]
enum ReadAt {
    Measuring,
    BeforePlacing,
    AfterPlacing,
}

/// What Compose's `assertLines` does, a modifier asserting what's inside it gives.
fn assert_lines(
    at: ReadAt,
    vertical: VerticalAlignmentLine,
    horizontal: HorizontalAlignmentLine,
    expected: (i32, i32),
    asserted: &Count,
) -> Modifier {
    let asserted = asserted.clone();
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let asserted = asserted.clone();
        if let ReadAt::Measuring = at {
            assert_eq!(
                (
                    scope.alignment_line(&placeable, vertical),
                    scope.alignment_line(&placeable, horizontal)
                ),
                expected
            );
            bump(&asserted);
        }
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if let ReadAt::BeforePlacing = at {
                assert_eq!(
                    (
                        placement.alignment_line(&placeable, vertical),
                        placement.alignment_line(&placeable, horizontal)
                    ),
                    expected
                );
                bump(&asserted);
            }
            placement.place(&placeable, 0, 0);
            if let ReadAt::AfterPlacing = at {
                assert_eq!(
                    (
                        placement.alignment_line(&placeable, vertical),
                        placement.alignment_line(&placeable, horizontal)
                    ),
                    expected
                );
                bump(&asserted);
            }
        })
    })
}

/// As `testAlignmentLinesReads`: read from a modifier through what's between it and them.
fn modifier_reads(at: ReadAt) {
    let vertical = VerticalAlignmentLine::new(i32::min);
    let horizontal = HorizontalAlignmentLine::new(i32::max);
    let asserted = count();
    let a = asserted.clone();
    let _host = host(move || {
        let provider = move |modifier: Modifier| {
            Layout(
                modifier,
                measure_policy(move |scope, _, _| {
                    scope.provide_alignment_lines([
                        (AlignmentLine::from(vertical), 10),
                        (AlignmentLine::from(horizontal), 20),
                    ]);
                    scope.layout_with(0, 0, |_| {})
                }),
                callback(|| {}),
            );
        };
        let lines = |expected| assert_lines(at, vertical, horizontal, expected, &a);
        let blue = || Modifier.background(Color::BLUE);
        let by = || Modifier.offset(dp(20), dp(10));
        let layer = || Modifier.graphics_layer(|_| {});
        provider(lines((10, 20)));
        provider(lines((30, 30)).then(by()));
        provider(lines((30, 30)).then(layer()).then(by()));
        provider(lines((30, 30)).then(blue()).then(layer()).then(by()).then(layer()).then(blue()));
        provider(
            blue()
                .then(lines((30, 30)))
                .then(blue())
                .then(layer())
                .then(by())
                .then(layer())
                .then(blue()),
        );
        wrap_with(
            blue()
                .then(lines((30, 30)))
                .then(blue())
                .then(layer())
                .then(by())
                .then(layer())
                .then(blue()),
            move || provider(Modifier),
        );
        wrap_with(
            blue()
                .then(lines((40, 50)))
                .then(blue())
                .then(layer())
                .then(by())
                .then(layer())
                .then(blue()),
            move || provider(Modifier.offset(dp(10), dp(20))),
        );
    });
    assert_eq!(asserted.get(), 7);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_readFromModifier_duringMeasurement
#[test]
fn a_modifier_reads_lines_measuring() {
    modifier_reads(ReadAt::Measuring);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_readFromModifier_duringPositioning_before
#[test]
fn a_modifier_reads_lines_before_placing() {
    modifier_reads(ReadAt::BeforePlacing);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, testAlignmentLines_readFromModifier_duringPositioning_after
#[test]
fn a_modifier_reads_lines_after_placing() {
    modifier_reads(ReadAt::AfterPlacing);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/AlignmentLinesTest.kt, alignmentLinesInheritedCorrectlyByParents_withModifiedPosition
#[test]
fn a_padding_moves_an_inherited_line() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        one_child(
            move |scope, placeable, _| {
                s.set(scope.alignment_line(&placeable, line));
                scope.layout_with(0, 0, |_| {})
            },
            move || {
                wrap(move || {
                    Layout(
                        Modifier.padding(dp(20)),
                        measure_policy(move |scope, _, _| {
                            scope.provide_alignment_lines([(line, 10)]);
                            scope.layout_with(0, 0, |_| {})
                        }),
                        callback(|| {}),
                    );
                });
            },
        );
    });
    assert_eq!(seen.get(), 30);
}

// ---- AlignmentLineTest -----------------------------------------------------------------------

/// What Compose's scenarios share: a line, where it is, and the layouts reading and giving it.
#[derive(Clone)]
struct World {
    line: HorizontalAlignmentLine,
    position: Rc<Cell<i32>>,
    state: MutableState<i32>,
    _owner: Rc<OwnedState<i32>>,
}

impl World {
    fn new() -> World {
        let owner = mutable_state_of(10);
        World {
            line: HorizontalAlignmentLine::new(i32::min),
            position: Rc::new(Cell::new(10)),
            state: owner.handle(),
            _owner: Rc::new(owner),
        }
    }

    fn change(&self) {
        self.position.set(self.position.get() + 10);
        self.state.set(self.state.get() + 10);
    }

    fn parent(&self, spec: Parent, content: impl Fn() + 'static) {
        let (world, modifier) = (self.clone(), spec.modifier.clone());
        let policy = measure_policy(move |scope, measurables, constraints| {
            bump(&spec.measures);
            let mut placeables = Vec::new();
            for measurable in measurables {
                let placeable = scope.measure(*measurable, constraints);
                if (spec.measuring)() {
                    assert_eq!(world.position.get(), scope.alignment_line(&placeable, world.line));
                }
                placeables.push(placeable);
            }
            let (world, spec) = (world.clone(), spec.clone());
            scope.layout_with(constraints.max_width, constraints.max_height, move |placement| {
                bump(&spec.layouts);
                for placeable in &placeables {
                    if (spec.before)() {
                        assert_eq!(
                            world.position.get(),
                            placement.alignment_line(placeable, world.line)
                        );
                    }
                    placement.place(placeable, 0, 0);
                    if (spec.after)() {
                        assert_eq!(
                            world.position.get(),
                            placement.alignment_line(placeable, world.line)
                        );
                    }
                }
            })
        });
        Layout(modifier, policy, callback(content));
    }

    fn provider(&self, modifier: Modifier, measures: &Count, layouts: &Count) {
        let (line, state, measures, layouts) =
            (self.line, self.state, measures.clone(), layouts.clone());
        let policy = measure_policy(move |scope, _, constraints| {
            bump(&measures);
            scope.provide_alignment_lines([(line, state.get())]);
            let layouts = layouts.clone();
            scope
                .layout_with(constraints.max_width, constraints.max_height, move |_| bump(&layouts))
        });
        Layout(modifier, policy, callback(|| {}));
    }

    /// `Parent { Provider() }`.
    fn parent_of_provider(&self) {
        let world = self.clone();
        self.parent(Parent::default(), move || world.provider(Modifier, &count(), &count()));
    }

    /// Compose's `Modifier.provider()`.
    fn provider_modifier(&self) -> Modifier {
        let (line, state) = (self.line, self.state);
        Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            scope.provide_alignment_lines([(line, state.get())]);
            scope.layout_with(constraints.max_width, constraints.max_height, move |placement| {
                placement.place(&placeable, 0, 0);
            })
        })
    }

    /// Compose's `Modifier.reader(...)`.
    fn reader(
        &self,
        measures: &Count,
        layouts: &Count,
        measuring: bool,
        before: bool,
        after: bool,
    ) -> Modifier {
        let (world, measures, layouts) = (self.clone(), measures.clone(), layouts.clone());
        Modifier.layout(move |scope, measurable, constraints| {
            bump(&measures);
            let placeable = scope.measure(measurable, constraints);
            if measuring {
                assert_eq!(world.position.get(), scope.alignment_line(&placeable, world.line));
            }
            let (world, layouts) = (world.clone(), layouts.clone());
            scope.layout_with(constraints.max_width, constraints.max_height, move |placement| {
                bump(&layouts);
                if before {
                    assert_eq!(
                        world.position.get(),
                        placement.alignment_line(&placeable, world.line)
                    );
                }
                placement.place(&placeable, 0, 0);
                if after {
                    assert_eq!(
                        world.position.get(),
                        placement.alignment_line(&placeable, world.line)
                    );
                }
            })
        })
    }
}

type Reads = Rc<dyn Fn() -> bool>;

#[derive(Clone)]
struct Parent {
    modifier: Modifier,
    measures: Count,
    layouts: Count,
    measuring: Reads,
    before: Reads,
    after: Reads,
}

impl Default for Parent {
    fn default() -> Parent {
        let no: Reads = Rc::new(|| false);
        Parent {
            modifier: Modifier,
            measures: count(),
            layouts: count(),
            measuring: no.clone(),
            before: no.clone(),
            after: no,
        }
    }
}

fn yes() -> Reads {
    Rc::new(|| true)
}

impl Parent {
    fn counting(measures: &Count, layouts: &Count) -> Parent {
        Parent { measures: measures.clone(), layouts: layouts.clone(), ..Parent::default() }
    }

    fn measuring(self) -> Parent {
        Parent { measuring: yes(), ..self }
    }

    fn before(self) -> Parent {
        Parent { before: yes(), ..self }
    }

    fn after(self) -> Parent {
        Parent { after: yes(), ..self }
    }
}

/// Four counters' values.
fn four(counts: [&Count; 4]) -> [u32; 4] {
    counts.map(|it| it.get())
}

/// Scenarios 1 to 6: a grandparent and a parent over `Parent { Provider() }`, each reading as said.
fn over_provider(
    outer: fn(Parent) -> Parent,
    middle: fn(Parent) -> Parent,
    before: [u32; 4],
    after: [u32; 4],
) {
    let world = World::new();
    let (pm, pl, m, l) = (count(), count(), count(), count());
    let (w, a, b, c, d) = (world.clone(), pm.clone(), pl.clone(), m.clone(), l.clone());
    let mut host = host(move || {
        let (w2, c, d) = (w.clone(), c.clone(), d.clone());
        w.parent(outer(Parent::counting(&a, &b)), move || {
            let w3 = w2.clone();
            w2.parent(middle(Parent::counting(&c, &d)), move || w3.parent_of_provider());
        });
    });
    let all = [&pm, &pl, &m, &l];
    assert_eq!(four(all), before);
    world.change();
    host.frame();
    assert_eq!(four(all), after);
}

fn plain(spec: Parent) -> Parent {
    spec
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario1
#[test]
fn a_line_read_measuring_measures_only_who_read() {
    over_provider(plain, Parent::measuring, [1, 1, 1, 1], [1, 1, 2, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario2
#[test]
fn a_line_read_before_placing_places_only_who_read() {
    over_provider(plain, Parent::before, [1, 1, 1, 1], [1, 1, 1, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario3
#[test]
fn a_line_read_after_placing_places_only_who_read() {
    over_provider(plain, Parent::after, [1, 1, 1, 1], [1, 1, 1, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario4
#[test]
fn read_measuring_above_read_before_placing() {
    over_provider(Parent::measuring, Parent::before, [1, 1, 1, 1], [2, 2, 1, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario5
#[test]
fn read_measuring_above_read_after_placing() {
    over_provider(Parent::measuring, Parent::after, [1, 1, 1, 1], [2, 2, 1, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario6
#[test]
fn read_after_placing_above_read_measuring() {
    over_provider(Parent::after, Parent::measuring, [1, 1, 1, 1], [1, 2, 2, 2]);
}

/// Scenarios 7 to 10: as above, the line given by a modifier on the child, which measures again.
fn over_provider_modifier(
    outer: fn(Parent) -> Parent,
    middle: fn(Parent) -> Parent,
    after: [u32; 4],
) {
    let world = World::new();
    let (pm, m, l, cm) = (count(), count(), count(), count());
    let (w, a, b, c, d) = (world.clone(), pm.clone(), m.clone(), l.clone(), cm.clone());
    let mut host = host(move || {
        let (w2, b, c, d) = (w.clone(), b.clone(), c.clone(), d.clone());
        w.parent(outer(Parent::counting(&a, &count())), move || {
            let (w3, d) = (w2.clone(), d.clone());
            w2.parent(middle(Parent::counting(&b, &c)), move || {
                let w4 = w3.clone();
                let spec = Parent {
                    modifier: w3.provider_modifier(),
                    measures: d.clone(),
                    ..Parent::default()
                };
                w3.parent(spec, move || w4.parent(Parent::default(), || {}));
            });
        });
    });
    let all = [&pm, &m, &l, &cm];
    assert_eq!(four(all), [1, 1, 1, 1]);
    world.change();
    host.frame();
    assert_eq!(four(all), after);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario7
#[test]
fn a_modifier_s_line_read_measuring() {
    over_provider_modifier(plain, Parent::measuring, [1, 2, 2, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario8
#[test]
fn a_modifier_s_line_read_before_placing() {
    over_provider_modifier(plain, Parent::before, [1, 1, 2, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario9
#[test]
fn a_modifier_s_line_read_after_placing() {
    over_provider_modifier(plain, Parent::after, [1, 1, 2, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario10
#[test]
fn a_modifier_s_line_read_measuring_above_read_after_placing() {
    over_provider_modifier(Parent::measuring, Parent::after, [2, 1, 2, 2]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario11
#[test]
fn read_by_a_modifier_measuring_below_read_after_placing() {
    let world = World::new();
    let (m, l, cm) = (count(), count(), count());
    let (w, a, b, c) = (world.clone(), m.clone(), l.clone(), cm.clone());
    let mut host = host(move || {
        let (w2, a, b, c) = (w.clone(), a.clone(), b.clone(), c.clone());
        w.parent(Parent::default(), move || {
            let (w3, c) = (w2.clone(), c.clone());
            w2.parent(Parent::counting(&a, &b).after(), move || {
                let w4 = w3.clone();
                let spec = Parent {
                    modifier: w3.reader(&count(), &count(), true, false, false),
                    measures: c.clone(),
                    ..Parent::default()
                };
                w3.parent(spec, move || w4.provider(Modifier, &count(), &count()));
            });
        });
    });
    assert_eq!((m.get(), l.get(), cm.get()), (1, 1, 1));
    world.change();
    host.frame();
    assert_eq!((m.get(), l.get(), cm.get()), (1, 2, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario12
#[test]
fn read_by_a_modifier_of_what_gives_it_measuring() {
    let world = World::new();
    let measures = count();
    let (w, m) = (world.clone(), measures.clone());
    let mut host = host(move || {
        let (w2, m) = (w.clone(), m.clone());
        w.parent(Parent::default(), move || {
            w2.provider(w2.reader(&count(), &count(), true, false, false), &m, &count());
        });
    });
    assert_eq!(measures.get(), 1);
    world.change();
    host.frame();
    assert_eq!(measures.get(), 2);
}

fn read_by_a_modifier_of_what_gives_it(before: bool, after: bool) {
    let world = World::new();
    let (parent, measures, layouts) = (count(), count(), count());
    let (w, p, m, l) = (world.clone(), parent.clone(), measures.clone(), layouts.clone());
    let mut host = host(move || {
        let (w2, m, l) = (w.clone(), m.clone(), l.clone());
        w.parent(Parent::counting(&p, &count()), move || {
            w2.provider(w2.reader(&count(), &count(), false, before, after), &m, &l);
        });
    });
    assert_eq!((parent.get(), measures.get(), layouts.get()), (1, 1, 1));
    world.change();
    host.frame();
    assert_eq!((parent.get(), measures.get(), layouts.get()), (1, 2, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario13
#[test]
fn read_by_a_modifier_of_what_gives_it_before_placing() {
    read_by_a_modifier_of_what_gives_it(true, false);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario14
#[test]
fn read_by_a_modifier_of_what_gives_it_after_placing() {
    read_by_a_modifier_of_what_gives_it(false, true);
}

fn a_reader_outside_a_provider_modifier(outer: fn(Parent) -> Parent, after: [u32; 3]) {
    let world = World::new();
    let (pm, m, cm) = (count(), count(), count());
    let (w, a, b, c) = (world.clone(), pm.clone(), m.clone(), cm.clone());
    let mut host = host(move || {
        let (w2, b, c) = (w.clone(), b.clone(), c.clone());
        w.parent(outer(Parent::counting(&a, &count())), move || {
            let (w3, c) = (w2.clone(), c.clone());
            let spec = Parent {
                modifier: w2
                    .reader(&count(), &count(), true, false, false)
                    .then(w2.provider_modifier()),
                measures: b.clone(),
                ..Parent::default()
            };
            w2.parent(spec, move || {
                w3.parent(Parent { measures: c.clone(), ..Parent::default() }, || {})
            });
        });
    });
    assert_eq!((pm.get(), m.get(), cm.get()), (1, 1, 1));
    world.change();
    host.frame();
    assert_eq!([pm.get(), m.get(), cm.get()], after);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario15
#[test]
fn a_reader_modifier_outside_a_provider_modifier() {
    a_reader_outside_a_provider_modifier(plain, [1, 2, 1]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario16
#[test]
fn a_reader_modifier_outside_a_provider_modifier_read_above() {
    a_reader_outside_a_provider_modifier(Parent::measuring, [2, 2, 1]);
}

/// Scenarios 17 and 18: a reader stopping reading, above it one that still does.
fn stops_reading(
    outer: fn(Parent) -> Parent,
    first: [u32; 3],
    second: [u32; 3],
    which: fn(&Count, &Count) -> Count,
) {
    let world = World::new();
    let read = mutable_state_of(true);
    let (pm, pl, m, cm) = (count(), count(), count(), count());
    let (w, a, b, c, d, r) =
        (world.clone(), pm.clone(), pl.clone(), m.clone(), cm.clone(), read.handle());
    let mut host = host(move || {
        let (w2, c, d) = (w.clone(), c.clone(), d.clone());
        w.parent(outer(Parent::counting(&a, &b)), move || {
            let (w3, d) = (w2.clone(), d.clone());
            let spec = Parent {
                measures: c.clone(),
                measuring: Rc::new(move || r.get()),
                ..Parent::default()
            };
            w2.parent(spec, move || {
                let w4 = w3.clone();
                let spec = Parent { measures: d.clone(), ..Parent::default() };
                w3.parent(spec, move || w4.provider(Modifier, &count(), &count()));
            });
        });
    });
    let shown = which(&pm, &pl);
    assert_eq!((shown.get(), m.get(), cm.get()), (1, 1, 1));
    world.change();
    read.set(false);
    host.frame();
    assert_eq!([shown.get(), m.get(), cm.get()], first);
    world.change();
    host.frame();
    assert_eq!([shown.get(), m.get(), cm.get()], second);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario17
#[test]
fn a_reader_stopping_leaves_who_reads_above_it_measuring() {
    stops_reading(Parent::measuring, [2, 2, 1], [3, 2, 1], |measures, _| measures.clone());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario18
#[test]
fn a_reader_stopping_leaves_who_reads_above_it_placing() {
    stops_reading(Parent::after, [2, 2, 1], [3, 2, 1], |_, layouts| layouts.clone());
}

fn moved_by_a_block(offset: MutableState<i32>) -> Modifier {
    Modifier.absolute_offset_with(move |_| IntOffset::new(offset.get(), offset.get()))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario19
#[test]
fn a_provider_moved_by_its_modifier_is_read_there() {
    let world = World::new();
    let offset = mutable_state_of(0);
    let (w, o) = (world.clone(), offset.handle());
    let mut host = host(move || {
        let w2 = w.clone();
        w.parent(Parent::default().before(), move || {
            w2.provider(moved_by_a_block(o), &count(), &count());
        });
    });
    offset.set(10);
    world.position.set(world.position.get() + 10);
    host.frame();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario20
#[test]
fn a_provider_moved_below_who_reads_places_it_again() {
    let world = World::new();
    let offset = mutable_state_of(0);
    let layouts = count();
    let (w, o, l) = (world.clone(), offset.handle(), layouts.clone());
    let mut host = host(move || {
        let w2 = w.clone();
        w.parent(Parent::counting(&count(), &l).before(), move || {
            let w3 = w2.clone();
            w2.parent(Parent::default(), move || {
                w3.provider(moved_by_a_block(o), &count(), &count());
            });
        });
    });
    offset.set(10);
    world.position.set(world.position.get() + 10);
    host.frame();
    assert_eq!(layouts.get(), 2);
}

/// Scenarios 21 and 22: a state making it read for the first time.
fn starts_reading(spec: impl Fn(&Count, Reads) -> Parent + 'static, shown: fn(&Count) -> u32) {
    let world = World::new();
    let read = mutable_state_of(false);
    let counted = count();
    let (w, r, c) = (world.clone(), read.handle(), counted.clone());
    let mut host = host(move || {
        let w2 = w.clone();
        w.parent(spec(&c, Rc::new(move || r.get())), move || w2.parent_of_provider());
    });
    read.set(true);
    host.frame();
    assert_eq!(shown(&counted), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario21
#[test]
fn starting_to_read_measuring() {
    starts_reading(
        |counted, read| Parent { measures: counted.clone(), measuring: read, ..Parent::default() },
        |counted| counted.get(),
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario22
#[test]
fn starting_to_read_placing() {
    starts_reading(
        |counted, read| Parent { layouts: counted.clone(), before: read, ..Parent::default() },
        |counted| counted.get(),
    );
}

/// Scenarios 23 and 24: a line changing as what reads it is placed again anyway.
fn read_placing_as_it_changes(before: bool) {
    let world = World::new();
    let changing = mutable_state_of(false);
    let seen = Rc::new(Cell::new(0));
    let (w, s, c) = (world.clone(), seen.clone(), changing.handle());
    let mut host = host(move || {
        let (line, s, w2) = (w.line, s.clone(), w.clone());
        one_child(
            move |scope, placeable, constraints| {
                let s = s.clone();
                scope.layout_with(constraints.max_width, constraints.max_height, move |placement| {
                    c.get();
                    if before {
                        s.set(placement.alignment_line(&placeable, line));
                    }
                    placement.place(&placeable, 0, 0);
                    if !before {
                        s.set(placement.alignment_line(&placeable, line));
                    }
                })
            },
            move || w2.parent_of_provider(),
        );
    });
    assert_eq!(seen.get(), world.position.get());
    world.change();
    changing.set(true);
    host.frame();
    assert_eq!(seen.get(), world.position.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario23
#[test]
fn read_before_placing_as_it_changes() {
    read_placing_as_it_changes(true);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario24
#[test]
fn read_after_placing_as_it_changes() {
    read_placing_as_it_changes(false);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario25
#[test]
fn coordinates_hear_of_a_line_changing() {
    let world = World::new();
    let seen = Rc::new(Cell::new(0));
    let (w, s) = (world.clone(), seen.clone());
    let mut host = host(move || {
        let (w2, s, line) = (w.clone(), s.clone(), w.line);
        let modifier = Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
            s.set(coordinates.alignment_line(line));
        });
        w.parent(Parent { modifier, ..Parent::default() }, move || w2.parent_of_provider());
    });
    assert_eq!(seen.get(), world.position.get());
    world.change();
    host.frame();
    assert_eq!(seen.get(), world.position.get());
}

/// Scenarios 26 to 28: a modifier reading what its own node inherits.
fn read_by_a_modifier_above(measuring: bool, before: bool, after: bool, then: (u32, u32)) {
    let world = World::new();
    let (measures, layouts) = (count(), count());
    let (w, m, l) = (world.clone(), measures.clone(), layouts.clone());
    let mut host = host(move || {
        let w2 = w.clone();
        let modifier = w.reader(&m, &l, measuring, before, after);
        w.parent(Parent { modifier, ..Parent::default() }, move || w2.parent_of_provider());
    });
    assert_eq!((measures.get(), layouts.get()), (1, 1));
    world.change();
    host.frame();
    assert_eq!((measures.get(), layouts.get()), then);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario26
#[test]
fn a_modifier_reading_inherited_measuring_and_placing() {
    read_by_a_modifier_above(true, true, false, (2, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario27
#[test]
fn a_modifier_reading_inherited_before_placing() {
    read_by_a_modifier_above(false, true, false, (1, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, scenario28
#[test]
fn a_modifier_reading_inherited_after_placing() {
    read_by_a_modifier_above(false, false, true, (1, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, queryingLinesOfUnmeasuredChild
#[test]
fn a_line_of_what_its_child_didn_t_measure_is_nothing() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        one_child(
            move |scope, placeable, _| {
                s.set(scope.alignment_line(&placeable, line));
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            || {
                let policy = measure_policy(|scope, _, _| scope.layout_with(5, 5, |_| {}));
                Layout(Modifier, policy, callback(|| wrap(|| {})));
            },
        );
    });
    assert_eq!(seen.get(), AlignmentLine::UNSPECIFIED);
}

/// Two lines from what's measured `(200, 200)` in a `(100, 100)`, read through `child`.
fn disobeying(child: fn(HorizontalAlignmentLine, VerticalAlignmentLine)) {
    let (horizontal, vertical) =
        (HorizontalAlignmentLine::new(i32::min), VerticalAlignmentLine::new(i32::min));
    let seen = Rc::new(Cell::new((0, 0)));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        let policy = measure_policy(move |scope, measurables, _| {
            let constraints = Constraints::new(0, 100, 0, 100);
            let placeable = scope.measure(measurables[0], constraints);
            s.set((
                scope.alignment_line(&placeable, horizontal),
                scope.alignment_line(&placeable, vertical),
            ));
            scope.layout_with(0, 0, |_| {})
        });
        Layout(Modifier, policy, callback(move || child(horizontal, vertical)));
    });
    assert_eq!(seen.get(), (0, 100));
}

fn giving_lines_at(
    size: i32,
    horizontal: HorizontalAlignmentLine,
    vertical: VerticalAlignmentLine,
) {
    Layout(
        Modifier,
        measure_policy(move |scope, _, _| {
            scope.provide_alignment_lines([
                (AlignmentLine::from(horizontal), 50),
                (AlignmentLine::from(vertical), 150),
            ]);
            scope.layout_with(size, size, |_| {})
        }),
        callback(|| {}),
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, alignmentLinesPositionInCooperation_whenModifierDisobeys
#[test]
fn a_line_is_where_it_ends_up_when_a_modifier_disobeys() {
    disobeying(|horizontal, vertical| {
        Stack!(modifier = Modifier.required_size(dp(200)), {
            giving_lines_at(0, horizontal, vertical);
        });
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, alignmentLinesPositionInCooperation_whenLayoutDisobeys
#[test]
fn a_line_is_where_it_ends_up_when_a_layout_disobeys() {
    disobeying(|horizontal, vertical| giving_lines_at(200, horizontal, vertical));
}

fn supply(lines: impl Fn() -> Vec<(AlignmentLine, i32)> + 'static) -> Modifier {
    let lines = Rc::new(lines);
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let lines: Vec<(AlignmentLine, i32)> = lines();
        scope.provide_alignment_lines(lines);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, 0, 0);
        })
    })
}

/// A layout reading two lines of its only child, keeping what it read.
fn reading_two(
    horizontal: HorizontalAlignmentLine,
    vertical: VerticalAlignmentLine,
    seen: &Rc<Cell<(i32, i32)>>,
    content: impl Fn() + 'static,
) {
    let seen = seen.clone();
    one_child(
        move |scope, placeable, _| {
            seen.set((
                scope.alignment_line(&placeable, horizontal),
                scope.alignment_line(&placeable, vertical),
            ));
            scope.layout_with(0, 0, |_| {})
        },
        content,
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, alignmentLinesArePropagated_whenSuppliedViaModifier
#[test]
fn a_modifier_s_line_is_inherited() {
    let (h, v) = (HorizontalAlignmentLine::new(i32::min), VerticalAlignmentLine::new(i32::min));
    let seen = Rc::new(Cell::new((0, 0)));
    let s = seen.clone();
    let _host = host(move || {
        reading_two(h, v, &s, move || {
            wrap_with(Modifier.size(dp(50)), move || {
                let lines = supply(move || vec![(h.into(), 25), (v.into(), 25)]);
                Layout(lines.then(Modifier.size(dp(50))), empty_measure_policy(), callback(|| {}));
            });
        });
    });
    assert_eq!(seen.get(), (25, 25));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, alignmentLinesArePropagated_whenSuppliedViaModifier_withCorrectPosition
#[test]
fn a_modifier_s_line_is_inherited_where_it_is() {
    let (h, v) = (HorizontalAlignmentLine::new(i32::min), VerticalAlignmentLine::new(i32::min));
    let seen = Rc::new(Cell::new((0, 0)));
    let s = seen.clone();
    let _host = host(move || {
        reading_two(h, v, &s, move || {
            wrap_with(Modifier.size(dp(50)), move || {
                let modifier = Modifier
                    .offset(dp(10), dp(10))
                    .then(supply(move || vec![(h.into(), 25), (v.into(), 25)]))
                    .then(Modifier.size(dp(50)))
                    .then(Modifier.offset(dp(10), dp(10)));
                Layout(modifier, empty_measure_policy(), callback(|| {}));
            });
        });
    });
    assert_eq!(seen.get(), (35, 35));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, alignmentLinesChangeCausesRemeasure_whenSuppliedViaModifier
#[test]
fn a_modifier_s_line_changing_measures_who_read_it() {
    let (h, v) = (HorizontalAlignmentLine::new(i32::min), VerticalAlignmentLine::new(i32::min));
    let position = mutable_state_of(25);
    let seen = Rc::new(Cell::new((0, 0)));
    let (s, p) = (seen.clone(), position.handle());
    let mut host = host(move || {
        reading_two(h, v, &s, move || {
            wrap_with(Modifier.size(dp(50)), move || {
                let lines = supply(move || vec![(h.into(), p.get()), (v.into(), p.get())]);
                Layout(lines.then(Modifier.size(dp(50))), empty_measure_policy(), callback(|| {}));
            });
        });
    });
    assert_eq!(seen.get(), (25, 25));
    position.set(35);
    host.frame();
    assert_eq!(seen.get(), (35, 35));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, alignmentLinesChangeCausesRemeasure_whenSuppliedViaLayout
#[test]
fn a_layout_s_line_changing_measures_who_read_it() {
    let (h, v) = (HorizontalAlignmentLine::new(i32::min), VerticalAlignmentLine::new(i32::min));
    let position = mutable_state_of(25);
    let seen = Rc::new(Cell::new((0, 0)));
    let (s, p) = (seen.clone(), position.handle());
    let mut host = host(move || {
        reading_two(h, v, &s, move || {
            let inner = move || {
                Layout(
                    Modifier,
                    measure_policy(move |scope, _, _| {
                        scope.provide_alignment_lines([
                            (AlignmentLine::from(h), p.get()),
                            (AlignmentLine::from(v), p.get()),
                        ]);
                        scope.layout_with(50, 50, |_| {})
                    }),
                    callback(|| {}),
                );
            };
            let policy = measure_policy(|scope, measurables, constraints| {
                let placeable = scope.measure(measurables[0], constraints);
                scope.layout_with(constraints.max_width, constraints.max_height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            });
            Layout(Modifier.size(dp(50)), policy, callback(inner));
        });
    });
    assert_eq!(seen.get(), (25, 25));
    position.set(35);
    host.frame();
    assert_eq!(seen.get(), (35, 35));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/AlignmentLineTest.kt, notMeasuredChildIsNotCrashingWhenGrandParentQueriesAlignments
#[test]
fn a_child_not_measured_doesn_t_stop_a_grandparent_asking() {
    let emit = mutable_state_of(false);
    let e = emit.handle();
    let mut host = host(move || {
        one_child(
            |scope, placeable, _| {
                scope.alignment_line(&placeable, FIRST_BASELINE);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            },
            move || {
                let policy = measure_policy(|scope, _, constraints| {
                    scope.layout_with(constraints.max_width, constraints.max_height, |_| {})
                });
                Layout(
                    Modifier,
                    policy,
                    callback(move || {
                        if e.get() {
                            Leaf(Modifier.size(dp(10)));
                        }
                    }),
                );
            },
        );
    });
    emit.set(true);
    host.frame();
}

// ---- PlacedChildTest, PlacementLayoutCoordinatesTest -----------------------------------------

/// A leaf at zero or 50 giving the first baseline at 0, with `modifier`.
fn baseline_leaf(modifier: Modifier, block: impl Fn(&mut PlacementScope<'_>) + Clone + 'static) {
    let policy = measure_policy(move |scope, _, _| {
        scope.provide_alignment_lines([(FIRST_BASELINE, 0)]);
        scope.layout_with(50, 50, block.clone())
    });
    Layout(modifier, policy, callback(|| {}));
}

/// Asks its only child its first baseline, measuring, and places it at `(x, 0)` if `place`.
fn asking(place: bool, x: i32, content: impl Fn() + 'static) {
    one_child(
        move |scope, placeable, _| {
            scope.alignment_line(&placeable, FIRST_BASELINE);
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                if place {
                    placement.place(&placeable, x, x);
                }
            })
        },
        content,
    );
}

type Positions = Rc<std::cell::RefCell<Vec<Option<Offset>>>>;

fn keeping_on_placed(positions: &Positions) -> Modifier {
    let positions = positions.clone();
    Modifier.on_placed(move |coordinates: LayoutCoordinates| {
        positions.borrow_mut().push(Some(coordinates.position_in_root()));
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, onPlacedIsNotCalledDuringAlignmentLinesCalculation
#[test]
fn on_placed_hears_only_of_the_real_placing() {
    let positions: Positions = Rc::default();
    let p = positions.clone();
    let _host = host(move || {
        let p = p.clone();
        asking(true, 10, move || {
            let p = p.clone();
            wrap(move || baseline_leaf(keeping_on_placed(&p), |_| {}));
        });
    });
    assert_eq!(*positions.borrow(), [Some(Offset::new(10.0, 10.0))]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, onPlacedIsNotCalledOnNotPlacedChildUsedByAlignmentLinesCalculation
#[test]
fn on_placed_hears_nothing_of_what_s_only_read() {
    let positions: Positions = Rc::default();
    let p = positions.clone();
    let _host = host(move || {
        let p = p.clone();
        asking(false, 0, move || {
            let p = p.clone();
            wrap(move || baseline_leaf(keeping_on_placed(&p), |_| {}));
        });
    });
    assert!(positions.borrow().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, onPlacedIsNotCalledOnNotPlacedChildUsedByAlignmentLinesCalculation_nested
#[test]
fn on_placed_hears_nothing_of_what_s_only_read_nested() {
    let positions: Positions = Rc::default();
    let p = positions.clone();
    let _host = host(move || {
        let p = p.clone();
        asking(false, 0, move || {
            let p = p.clone();
            wrap(move || {
                let p = p.clone();
                wrap_with(Modifier.offset(dp(0), dp(0)), move || {
                    baseline_leaf(keeping_on_placed(&p), |_| {});
                });
            });
        });
    });
    assert!(positions.borrow().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, addingChildWithBaselineLater_isDisplayedDrawnAndOnPlacedIsCalled
#[test]
fn a_child_with_a_baseline_coming_later_is_placed_and_drawn() {
    let need = mutable_state_of(false);
    let positions: Positions = Rc::default();
    let draws = count();
    let (n, p, d) = (need.handle(), positions.clone(), draws.clone());
    let mut host = host(move || {
        let (p, d) = (p.clone(), d.clone());
        asking(true, 10, move || {
            let (p, d) = (p.clone(), d.clone());
            wrap(move || {
                if n.get() {
                    let d = d.clone();
                    let modifier = keeping_on_placed(&p).draw_behind(move |_| bump(&d));
                    baseline_leaf(modifier, |_| {});
                }
            });
        });
    });
    need.set(true);
    host.frame();
    assert_eq!(*positions.borrow(), [Some(Offset::new(10.0, 10.0))]);
    assert_eq!(draws.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, addingChildWithBaselineLater_onPlacedIsCalledOnOuterCoordinator
#[test]
fn a_child_with_a_baseline_coming_later_tells_its_outermost() {
    let need = mutable_state_of(false);
    let positions: Positions = Rc::default();
    let (n, p) = (need.handle(), positions.clone());
    let mut host = host(move || {
        let p = p.clone();
        asking(true, 10, move || {
            let p = p.clone();
            wrap(move || {
                if n.get() {
                    baseline_leaf(
                        keeping_on_placed(&p).then(Modifier.offset(dp(0), dp(0))),
                        |_| {},
                    );
                }
            });
        });
    });
    need.set(true);
    host.frame();
    assert_eq!(positions.borrow().last().copied().flatten(), Some(Offset::new(10.0, 10.0)));
}

/// Lines given by a layout modifier the state's read in, what's told of it counted.
fn told_of_a_line(ask: fn(&LayoutCoordinates)) {
    let line = mutable_state_of(10);
    let calls = count();
    let (l, c) = (line.handle(), calls.clone());
    let mut host = host(move || {
        let c = c.clone();
        wrap(move || {
            let c = c.clone();
            let told = Modifier.on_placed(move |coordinates: LayoutCoordinates| {
                ask(&coordinates);
                bump(&c);
            });
            let giving = Modifier.layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                scope.provide_alignment_lines([(FIRST_BASELINE, l.get())]);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            });
            Layout(told.then(giving), empty_measure_policy(), callback(|| {}));
        });
    });
    calls.set(0);
    line.set(15);
    host.frame();
    assert_eq!(calls.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, alignmentLineReadTriggersCallbackOnLineChange
#[test]
fn on_placed_reading_a_line_hears_of_it_changing() {
    told_of_a_line(|coordinates| {
        coordinates.alignment_line(FIRST_BASELINE);
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, providedAlignmentLinesReadTriggersCallbackOnLineChange
#[test]
fn on_placed_reading_what_s_provided_hears_of_it_changing() {
    told_of_a_line(|coordinates| {
        assert_eq!(coordinates.provided_alignment_lines(), [AlignmentLine::from(FIRST_BASELINE)]);
    });
}

fn keeping_coordinates(
    positions: &Positions,
) -> impl Fn(&mut PlacementScope<'_>) + Clone + 'static {
    let positions = positions.clone();
    move |placement| {
        positions
            .borrow_mut()
            .push(placement.coordinates().map(|coordinates| coordinates.position_in_root()));
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, grandChildIsOnlyCalledWithNullCoordinatesWhenUsedByAlignmentLinesCalculationButNotPlaced
#[test]
fn coordinates_are_none_for_what_s_only_read() {
    let positions: Positions = Rc::default();
    let p = positions.clone();
    let _host = host(move || {
        let p = p.clone();
        asking(false, 0, move || {
            let p = p.clone();
            wrap(move || baseline_leaf(Modifier, keeping_coordinates(&p)));
        });
    });
    assert_eq!(*positions.borrow(), [None]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, addingChildWithBaselineLater_layoutBlockUsingCoordinatesIsReexecuted
#[test]
fn a_block_that_asked_coordinates_while_read_runs_again() {
    let need = mutable_state_of(false);
    let positions: Positions = Rc::default();
    let (n, p) = (need.handle(), positions.clone());
    let mut host = host(move || {
        let p = p.clone();
        one_child(
            |scope, placeable, _| {
                scope.alignment_line(&placeable, FIRST_BASELINE);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 10, 0);
                })
            },
            move || {
                let p = p.clone();
                wrap(move || {
                    if n.get() {
                        baseline_leaf(Modifier, keeping_coordinates(&p));
                    }
                });
            },
        );
    });
    need.set(true);
    host.frame();
    assert_eq!(*positions.borrow(), [None, Some(Offset::new(10.0, 0.0))]);
}

/// A grandchild asked through two boxes, placed at `x`.
fn grandchild(x: i32, in_modifier: bool) {
    let positions: Positions = Rc::default();
    let p = positions.clone();
    let _host = host(move || {
        let p = p.clone();
        one_child(
            move |scope, placeable, _| {
                scope.alignment_line(&placeable, FIRST_BASELINE);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, x, 0);
                })
            },
            move || {
                let p = p.clone();
                wrap(move || {
                    let p = p.clone();
                    wrap(move || {
                        if in_modifier {
                            let keep = keeping_coordinates(&p);
                            let modifier = Modifier.layout(move |scope, measurable, _| {
                                let placeable =
                                    scope.measure(measurable, Constraints::fixed(50, 50));
                                scope.provide_alignment_lines([(FIRST_BASELINE, 0)]);
                                let keep = keep.clone();
                                scope.layout_with(
                                    placeable.width,
                                    placeable.height,
                                    move |placement| {
                                        keep(placement);
                                        placement.place(&placeable, 0, 0);
                                    },
                                )
                            });
                            Layout(modifier, empty_measure_policy(), callback(|| {}));
                        } else {
                            baseline_leaf(Modifier, keeping_coordinates(&p));
                        }
                    });
                });
            },
        );
    });
    assert_eq!(*positions.borrow(), [None, Some(Offset::new(x as f32, 0.0))]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, grandChildIsPlacedWithNullCoordinatesFirstDuringAlignmentLinesCalculation
#[test]
fn a_grandchild_read_is_told_none_then_where_it_is() {
    grandchild(0, false);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, grandChildIsPlacedWithNullCoordinatesFirstDuringAlignmentLinesCalculation_modifier
#[test]
fn a_grandchild_s_modifier_read_is_told_none_then_where_it_is() {
    grandchild(0, true);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, grandChildIsPlacedWithNullCoordinatesFirstDuringAlignmentLinesCalculation_nonZeroOffset
#[test]
fn a_grandchild_read_is_told_none_then_where_it_is_moved() {
    grandchild(10, false);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, grandChildIsPlacedWithNullCoordinatesFirstDuringAlignmentLinesCalculation_modifier_nonZeroOffset
#[test]
fn a_grandchild_s_modifier_read_is_told_none_then_where_it_is_moved() {
    grandchild(10, true);
}

/// Asking which lines there are, a block isn't run again by moving.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, onlyRealPositionReadsTriggerRelayout
#[test]
fn moving_doesn_t_run_a_block_that_asked_which_lines_there_are() {
    let offset = mutable_state_of(0);
    let blocks = count();
    let (o, b) = (offset.handle(), blocks.clone());
    let mut host = host(move || {
        let b = b.clone();
        one_child(
            move |scope, placeable, _| {
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, o.get(), 0);
                })
            },
            move || {
                let b = b.clone();
                let policy = measure_policy(move |scope, _, _| {
                    let b = b.clone();
                    scope.layout_with(10, 10, move |placement| {
                        placement.coordinates().expect("placing").provided_alignment_lines();
                        bump(&b);
                    })
                });
                Layout(Modifier, policy, callback(|| {}));
            },
        );
    });
    blocks.set(0);
    offset.set(10);
    host.frame();
    assert_eq!(blocks.get(), 0);
}

/// A block run early for a modifier's read, what it reads still places it again, once.
#[test]
fn what_a_block_run_early_reads_places_it_again() {
    let line = VerticalAlignmentLine::new(i32::min);
    let state = mutable_state_of(0);
    let (blocks, reads) = (count(), Rc::new(Cell::new(0)));
    let (st, b, r) = (state.handle(), blocks.clone(), reads.clone());
    let mut host = host(move || {
        let (b, r) = (b.clone(), r.clone());
        let reader = Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            scope.alignment_line(&placeable, line);
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                placement.place(&placeable, 0, 0);
            })
        });
        let policy = measure_policy(move |scope, measurables, constraints| {
            let placeable = scope.measure(measurables[0], constraints);
            let (b, r) = (b.clone(), r.clone());
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                bump(&b);
                r.set(st.get());
                placement.place(&placeable, 0, 0);
            })
        });
        Layout(reader, policy, callback(move || lines_at(vec![(line, 10)])));
    });
    assert_eq!((blocks.get(), reads.get()), (1, 0));
    state.set(1);
    host.frame();
    assert_eq!((blocks.get(), reads.get()), (2, 1));
}

/// A policy measured in an intrinsic asks nothing of its child's lines.
#[test]
fn an_intrinsic_measurement_sees_no_line() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        let policy = measure_policy(move |scope, measurables, constraints| {
            let placeable = scope.measure(measurables[0], constraints);
            s.borrow_mut().push(scope.alignment_line(&placeable, line));
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                placement.place(&placeable, 0, 0);
            })
        });
        let asks = Modifier.layout(|scope, measurable, constraints| {
            scope.min_intrinsic_height(measurable, 10);
            let placeable = scope.measure(measurable, constraints);
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                placement.place(&placeable, 0, 0);
            })
        });
        Layout(asks, policy, callback(move || lines_at(vec![(line, 10)])));
    });
    assert_eq!(*seen.borrow(), [AlignmentLine::UNSPECIFIED, 10]);
}

/// A block that stopped reading isn't told of what it read last time but one.
#[test]
fn a_block_that_stopped_reading_isn_t_told() {
    let line = VerticalAlignmentLine::new(i32::min);
    let (reads, position) = (mutable_state_of(true), mutable_state_of(10));
    let blocks = count();
    let (r, p, b) = (reads.handle(), position.handle(), blocks.clone());
    let mut host = host(move || {
        let b = b.clone();
        one_child(
            move |scope, placeable, _| {
                let b = b.clone();
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    bump(&b);
                    if r.get() {
                        placement.alignment_line(&placeable, line);
                    }
                    placement.place(&placeable, 0, 0);
                })
            },
            move || {
                Layout(
                    Modifier,
                    measure_policy(move |scope, _, _| {
                        scope.provide_alignment_lines([(line, p.get())]);
                        scope.layout_with(0, 0, |_| {})
                    }),
                    callback(|| {}),
                );
            },
        );
    });
    assert_eq!(blocks.get(), 1);
    reads.set(false);
    host.frame();
    assert_eq!(blocks.get(), 2);
    position.set(20);
    host.frame();
    assert_eq!(blocks.get(), 2);
}

/// A modifier's line over the same line of what's inside it, read from above the node.
#[test]
fn a_modifier_s_line_overrides_what_the_node_inherits() {
    let line = VerticalAlignmentLine::new(i32::max);
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        one_child(
            move |scope, placeable, _| {
                s.set(scope.alignment_line(&placeable, line));
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            move || {
                wrap(move || {
                    let giving = supply(move || vec![(line.into(), 5)]);
                    let policy = measure_policy(move |scope, _, _| {
                        scope.provide_alignment_lines([(line, 10)]);
                        scope.layout_with(0, 0, |_| {})
                    });
                    Layout(giving, policy, callback(|| {}));
                });
            },
        );
    });
    assert_eq!(seen.get(), 5);
}

/// Two blocks of a node run early, what both read still places them again.
#[test]
fn what_two_blocks_run_early_read_places_them_again() {
    let line = VerticalAlignmentLine::new(i32::min);
    let (outer_state, inner_state) = (mutable_state_of(0), mutable_state_of(0));
    let (outer_blocks, inner_blocks) = (count(), count());
    let (o, i, ob, ib) =
        (outer_state.handle(), inner_state.handle(), outer_blocks.clone(), inner_blocks.clone());
    let mut host = host(move || {
        let (ob, ib) = (ob.clone(), ib.clone());
        let reader = Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            scope.alignment_line(&placeable, line);
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                placement.place(&placeable, 0, 0);
            })
        });
        let reading = Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let ob = ob.clone();
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                bump(&ob);
                o.get();
                placement.place(&placeable, 0, 0);
            })
        });
        let policy = measure_policy(move |scope, measurables, constraints| {
            let placeable = scope.measure(measurables[0], constraints);
            let ib = ib.clone();
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                bump(&ib);
                i.get();
                placement.place(&placeable, 0, 0);
            })
        });
        Layout(reader.then(reading), policy, callback(move || lines_at(vec![(line, 10)])));
    });
    assert_eq!((outer_blocks.get(), inner_blocks.get()), (1, 1));
    inner_state.set(1);
    host.frame();
    assert_eq!(inner_blocks.get(), 2);
    // A node's blocks run together.
    let before = outer_blocks.get();
    outer_state.set(1);
    host.frame();
    assert_eq!(outer_blocks.get(), before + 1);
}

/// As `told_of_a_line`, its node placing inside its measurement.
#[test]
fn on_placed_asking_lines_of_what_places_in_its_measurement_hears() {
    for which in [false, true] {
        let line = mutable_state_of(10);
        let calls = count();
        let (l, c) = (line.handle(), calls.clone());
        let mut host = host(move || {
            let c = c.clone();
            wrap(move || {
                let c = c.clone();
                let told = Modifier.on_placed(move |coordinates: LayoutCoordinates| {
                    if which {
                        coordinates.provided_alignment_lines();
                    } else {
                        coordinates.alignment_line(FIRST_BASELINE);
                    }
                    bump(&c);
                });
                let giving = Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    scope.place(&placeable, 0, 0);
                    scope.provide_alignment_lines([(FIRST_BASELINE, l.get())]);
                    scope.layout(placeable.width, placeable.height)
                });
                Layout(told.then(giving), empty_measure_policy(), callback(|| {}));
            });
        });
        calls.set(0);
        line.set(15);
        host.frame();
        assert_eq!(calls.get(), 1, "{which}");
    }
}

/// What it asked of its coordinates is what it inherits, from a child that gives it.
#[test]
fn on_placed_asking_an_inherited_line_hears_of_it_changing() {
    let line = mutable_state_of(10);
    let (calls, seen) = (count(), Rc::new(Cell::new(0)));
    let (l, c, s) = (line.handle(), calls.clone(), seen.clone());
    let mut host = host(move || {
        let (c, s) = (c.clone(), s.clone());
        let told = Modifier.on_placed(move |coordinates: LayoutCoordinates| {
            s.set(coordinates.alignment_line(FIRST_BASELINE));
            bump(&c);
        });
        wrap_with(told, move || {
            Layout(
                Modifier,
                measure_policy(move |scope, _, _| {
                    scope.provide_alignment_lines([(FIRST_BASELINE, l.get())]);
                    scope.layout_with(0, 0, |_| {})
                }),
                callback(|| {}),
            );
        });
    });
    assert_eq!(seen.get(), 10);
    calls.set(0);
    line.set(15);
    host.frame();
    assert_eq!((calls.get(), seen.get()), (1, 15));
}

// ---- Through a graphics layer ------------------------------------------------------------------

fn translated(y: f32) -> Modifier {
    Modifier.graphics_layer(move |layer| layer.translation_y = y)
}

/// A node with `modifier` giving a line at 7, read directly by what's around it through a wrap.
fn read_through_a_parent(modifier: impl Fn(HorizontalAlignmentLine) -> Modifier + 'static) -> i32 {
    let line = HorizontalAlignmentLine::new(i32::min);
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        let modifier = modifier(line);
        one_child(
            move |scope, placeable, _| {
                s.set(scope.alignment_line(&placeable, line));
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            move || {
                let modifier = modifier.clone();
                wrap(move || {
                    Layout(
                        modifier.clone(),
                        measure_policy(move |scope, _, _| {
                            scope.provide_alignment_lines([(line, 7)]);
                            scope.layout_with(0, 0, |_| {})
                        }),
                        callback(|| {}),
                    );
                });
            },
        );
    });
    seen.get()
}

fn placing_layout() -> Modifier {
    Modifier.layout(|scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, 0, 0);
        })
    })
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LayoutNodeAlignmentLines.kt, addAlignmentLine (`toParentPosition`, a layer's `mapOffset`)
#[test]
fn an_inherited_line_goes_through_a_graphics_layer() {
    assert_eq!(read_through_a_parent(|_| translated(20.0)), 27);
}

#[test]
fn an_inherited_line_goes_through_a_graphics_layer_inside_a_layout_modifier() {
    assert_eq!(read_through_a_parent(|_| placing_layout().then(translated(20.0))), 27);
}

#[test]
fn an_inherited_line_goes_through_a_graphics_layer_around_a_layout_modifier() {
    assert_eq!(read_through_a_parent(|_| translated(20.0).then(placing_layout())), 27);
}

#[test]
fn a_modifier_s_line_goes_through_a_graphics_layer_around_it_not_inside_it() {
    let around =
        read_through_a_parent(|line| translated(20.0).then(supply(move || vec![(line.into(), 3)])));
    assert_eq!(around, 23);
    let inside =
        read_through_a_parent(|line| supply(move || vec![(line.into(), 3)]).then(translated(20.0)));
    assert_eq!(inside, 3);
}

/// As `get` is: a parent reading its child sees no transform of the child's own, as
/// `calculateAlignmentAndPlaceChildAsNeeded` adds positions only.
#[test]
fn a_line_read_of_the_node_itself_doesn_t_go_through_its_layer() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        one_child(
            move |scope, placeable, _| {
                s.set(scope.alignment_line(&placeable, line));
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            move || {
                Layout(
                    translated(20.0),
                    measure_policy(move |scope, _, _| {
                        scope.provide_alignment_lines([(line, 7)]);
                        scope.layout_with(0, 0, |_| {})
                    }),
                    callback(|| {}),
                );
            },
        );
    });
    assert_eq!(seen.get(), 7);
}

#[test]
fn merged_baselines_go_through_each_child_s_layer() {
    let seen = Rc::new(Cell::new((0, 0)));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        one_child(
            move |scope, placeable, _| {
                s.set((
                    scope.alignment_line(&placeable, FIRST_BASELINE),
                    scope.alignment_line(&placeable, LAST_BASELINE),
                ));
                scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
            },
            || {
                wrap(|| {
                    let giving = |at: i32, modifier: Modifier| {
                        Layout(
                            modifier,
                            measure_policy(move |scope, _, _| {
                                scope.provide_alignment_lines([
                                    (FIRST_BASELINE, at),
                                    (LAST_BASELINE, at),
                                ]);
                                scope.layout_with(0, 0, |_| {})
                            }),
                            callback(|| {}),
                        );
                    };
                    giving(15, Modifier);
                    giving(7, translated(20.0));
                });
            },
        );
    });
    assert_eq!(seen.get(), (15, 27));
}

#[test]
fn coordinates_read_an_inherited_line_through_a_graphics_layer() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let seen = Rc::new(Cell::new((0, 0)));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        let told = Modifier.on_placed(move |coordinates: LayoutCoordinates| {
            s.set((coordinates.alignment_line(line), s.get().1));
        });
        wrap_with(told, move || {
            Layout(
                translated(20.0).then(placing_layout()),
                measure_policy(move |scope, _, _| {
                    scope.provide_alignment_lines([(line, 5)]);
                    scope.layout_with(0, 0, |_| {})
                }),
                callback(|| {}),
            );
        });
    });
    assert_eq!(seen.get().0, 25);
}

// ---- A panic in a block run for a line ---------------------------------------------------------

/// A block panics while a parent's measure reads a line; the panic's caught; what's placed after
/// knows where it is, and rulers.
#[test]
fn a_panic_in_a_block_run_for_a_line_leaves_no_alignment_pass() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let ruler = VerticalRuler::new();
    let trigger = mutable_state_of(0);
    let again = mutable_state_of(0);
    let panics = Rc::new(Cell::new(false));
    let seen = Rc::new(Cell::new((false, 0.0)));
    let (t, a, p, s, r) =
        (trigger.handle(), again.handle(), panics.clone(), seen.clone(), ruler.clone());
    let mut host = host(move || {
        let (p, s, r) = (p.clone(), s.clone(), r.clone());
        let r2 = r.clone();
        let providing = Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let r = r2.clone();
            scope.layout_with_rulers(
                placeable.width,
                placeable.height,
                move |rulers| rulers.provides(&r, 3.0),
                move |placement| placement.place(&placeable, 0, 0),
            )
        });
        wrap_with(providing, move || {
            let p = p.clone();
            one_child(
                move |scope, placeable, _| {
                    t.get();
                    scope.alignment_line(&placeable, line);
                    scope.layout_with(0, 0, move |placement| placement.place(&placeable, 0, 0))
                },
                move || {
                    let p = p.clone();
                    Layout(
                        Modifier,
                        measure_policy(move |scope, _, _| {
                            t.get();
                            scope.provide_alignment_lines([(line, 1)]);
                            let p = p.clone();
                            scope.layout_with(0, 0, move |_| {
                                assert!(!p.get(), "the block's panic");
                            })
                        }),
                        callback(|| {}),
                    );
                },
            );
            let (s, r) = (s.clone(), r.clone());
            Leaf(Modifier.layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let (s, r) = (s.clone(), r.clone());
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    a.get();
                    s.set((placement.coordinates().is_some(), placement.current(&r, -1.0)));
                    placement.place(&placeable, 0, 0);
                })
            }));
        });
    });
    assert_eq!(seen.get(), (true, 3.0));
    panics.set(true);
    trigger.set(1);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.frame();
    }));
    assert!(panicked.is_err());
    panics.set(false);
    seen.set((false, 0.0));
    again.set(1);
    host.frame();
    assert_eq!(seen.get(), (true, 3.0));
}

/// A provider's rulers aren't worked out while it's placed for a line: where it is isn't known.
#[test]
fn rulers_aren_t_captured_for_a_line() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let ruler = VerticalRuler::new();
    let captured = Rc::new(std::cell::RefCell::new(Vec::new()));
    let (c, r) = (captured.clone(), ruler.clone());
    let _host = host(move || {
        let (c, r) = (c.clone(), r.clone());
        one_child(
            move |scope, placeable, _| {
                scope.alignment_line(&placeable, line);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 30, 0);
                })
            },
            move || {
                let (c, r) = (c.clone(), r.clone());
                let providing = Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    scope.provide_alignment_lines([(line, 1)]);
                    let (c, r) = (c.clone(), r.clone());
                    scope.layout_with_rulers(
                        placeable.width,
                        placeable.height,
                        move |rulers| {
                            let at = -rulers.coordinates().position_in_root().x;
                            c.borrow_mut().push(at);
                            rulers.provides(&r, at);
                        },
                        move |placement| placement.place(&placeable, 0, 0),
                    )
                });
                wrap(move || Layout(providing.clone(), empty_measure_policy(), callback(|| {})));
            },
        );
    });
    assert_eq!(*captured.borrow(), [-30.0]);
}
