//! `LookaheadScope` and its pass, as Compose's `LookaheadScopeTest`: what's in a scope is measured
//! and placed ahead first, by what its code does there, and again for now. Those of its cases with
//! an approach layout wait for `approach_layout`. Density 1.

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

type Log = Rc<RefCell<Vec<(&'static str, bool)>>>;

/// A modifier noting each measurement and placing, with the pass it's in.
fn noting(log: &Log) -> Modifier {
    let log = log.clone();
    Modifier.layout(move |scope, measurable, constraints| {
        log.borrow_mut().push(("measure", scope.is_looking_ahead()));
        let placeable = scope.measure(measurable, constraints);
        let log = log.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            log.borrow_mut().push(("place", placement.is_looking_ahead()));
            placement.place(&placeable, 0, 0);
        })
    })
}

fn scoped(content: impl Fn(LookaheadScope) + 'static) {
    LookaheadScope(callback(content));
}

fn sized(size: i32) -> Modifier {
    Modifier.size((size as f32).dp())
}

fn leaf(modifier: Modifier) {
    Layout(modifier, empty_measure_policy(), callback(|| {}));
}

/// A layout placing its first child at `x`.
fn placing_at(x: i32, modifier: Modifier, content: impl Fn() + 'static) {
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, x, 0);
        })
    });
    Layout(modifier, policy, callback(content));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testIsLookingAhead
#[test]
fn what_s_in_a_scope_is_measured_and_placed_ahead_first() {
    let log: Log = Rc::default();
    let l = log.clone();
    let _host = host(move || {
        let l = l.clone();
        scoped(move |_| leaf(noting(&l).then(sized(10))));
    });
    assert_eq!(
        *log.borrow(),
        [("measure", true), ("measure", false), ("place", true), ("place", false)]
    );
}

/// With no scope around it, once, not ahead.
#[test]
fn what_s_out_of_a_scope_isn_t_looked_ahead() {
    let (inside, outside): (Log, Log) = (Rc::default(), Rc::default());
    let (i, o) = (inside.clone(), outside.clone());
    let _host = host(move || {
        let i = i.clone();
        placing_at(0, noting(&o), move || {
            let i = i.clone();
            Stack!({
                let i = i.clone();
                scoped(move |_| leaf(noting(&i).then(sized(10))));
            });
        });
    });
    assert_eq!(*outside.borrow(), [("measure", false), ("place", false)]);
    assert_eq!(inside.borrow().iter().filter(|it| it.1).count(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testDifferentPlacementsInTwoPasses
#[test]
fn the_two_passes_have_their_own_results() {
    let scope_kept: Rc<Cell<Option<LookaheadScope>>> = Rc::default();
    let kept: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let top: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let sizes = Rc::new(RefCell::new(Vec::new()));
    let (sk, k, t, s) = (scope_kept.clone(), kept.clone(), top.clone(), sizes.clone());
    let _host = host(move || {
        let (sk, k, t, s) = (sk.clone(), k.clone(), t.clone(), s.clone());
        Stack!(modifier = Modifier.on_placed(move |it: LayoutCoordinates| t.set(Some(it))), {
            let (sk, k, s) = (sk.clone(), k.clone(), s.clone());
            scoped(move |scope| {
                sk.set(Some(scope));
                let (k, s) = (k.clone(), s.clone());
                let by_pass = measure_policy(move |scope, measurables, _| {
                    let ahead = scope.is_looking_ahead();
                    let inner = if ahead { 40 } else { 10 };
                    let placeable = scope.measure(measurables[0], Constraints::fixed(inner, inner));
                    s.borrow_mut().push((ahead, placeable.width));
                    let at = if ahead { 7 } else { 3 };
                    scope.layout_with(100, 100, move |placement| {
                        placement.place(&placeable, at, 0);
                    })
                });
                let k = k.clone();
                Layout(
                    Modifier,
                    by_pass,
                    callback(move || {
                        let k = k.clone();
                        leaf(Modifier.on_placed(move |it: LayoutCoordinates| k.set(Some(it))));
                    }),
                );
            });
        });
    });
    assert_eq!(*sizes.borrow(), [(true, 40), (false, 10)]);
    let (scope, child, top) = (
        scope_kept.get().expect("composed"),
        kept.get().expect("placed"),
        top.get().expect("placed"),
    );
    // What's now is the pass that isn't ahead.
    assert_eq!(child.size(), IntSize::new(10, 10));
    assert_eq!(top.local_position_of(&child, Offset::ZERO), Offset::new(3.0, 0.0));
    // Ahead, its own.
    let ahead = scope.to_lookahead_coordinates(child);
    assert_eq!(ahead.size(), IntSize::new(40, 40));
    assert_eq!(
        scope.to_lookahead_coordinates(top).local_position_of(ahead, Offset::new(1.0, 0.0)),
        Offset::new(8.0, 0.0)
    );
    assert_eq!(scope.local_lookahead_position_of(top, child, Offset::ZERO), Offset::new(7.0, 0.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, lookaheadSizeTrackedWhenModifierChanges
#[test]
fn a_state_read_measuring_measures_both_passes_once() {
    let width = mutable_state_of(10);
    let log: Log = Rc::default();
    let (w, l) = (width.handle(), log.clone());
    let mut host = host(move || {
        let l = l.clone();
        scoped(move |_| {
            let l = l.clone();
            Layout(
                noting(&l),
                measure_policy(move |scope, _, _| scope.layout(w.get(), 5)),
                callback(|| {}),
            );
        });
    });
    log.borrow_mut().clear();
    width.set(20);
    host.frame();
    let measures: Vec<_> = (log.borrow().iter()).filter(|it| it.0 == "measure").copied().collect();
    assert_eq!(measures, [("measure", true), ("measure", false)]);
}

/// A state only one pass's code reads, read while not looking ahead, leaves what's ahead alone.
#[test]
fn a_read_made_now_leaves_what_s_ahead() {
    let state = mutable_state_of(1);
    let (ahead, now) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (st, a, n) = (state.handle(), ahead.clone(), now.clone());
    let mut host = host(move || {
        let (a, n) = (a.clone(), n.clone());
        scoped(move |_| {
            let (a, n) = (a.clone(), n.clone());
            let measuring = Modifier.layout(move |scope, measurable, constraints| {
                if scope.is_looking_ahead() {
                    a.set(a.get() + 1);
                } else {
                    n.set(n.get() + 1);
                    st.get();
                }
                let placeable = scope.measure(measurable, constraints);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            });
            leaf(measuring);
        });
    });
    assert_eq!((ahead.get(), now.get()), (1, 1));
    state.set(2);
    host.frame();
    assert_eq!((ahead.get(), now.get()), (1, 2));
}

/// A state read in a lookahead block places both passes; one read now, only now.
#[test]
fn a_state_read_placing_places_by_where_it_was_read() {
    let (in_ahead, in_now) = (mutable_state_of(1), mutable_state_of(1));
    let log: Log = Rc::default();
    let (a, n, l) = (in_ahead.handle(), in_now.handle(), log.clone());
    let mut host = host(move || {
        let l = l.clone();
        scoped(move |_| {
            let l = l.clone();
            leaf(Modifier.layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let l = l.clone();
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    let ahead = placement.is_looking_ahead();
                    l.borrow_mut().push(("place", ahead));
                    if ahead {
                        a.get();
                    } else {
                        n.get();
                    }
                    placement.place(&placeable, 0, 0);
                })
            }));
        });
    });
    log.borrow_mut().clear();
    in_now.set(2);
    host.frame();
    assert_eq!(*log.borrow(), [("place", false)]);
    log.borrow_mut().clear();
    in_ahead.set(2);
    host.frame();
    assert_eq!(*log.borrow(), [("place", true), ("place", false)]);
}

/// What's told it's placed or sized hears of now, not of what's ahead.
#[test]
fn callbacks_hear_of_the_pass_that_isn_t_ahead() {
    let (placed, sizes) = (Rc::new(Cell::new(0)), Rc::new(RefCell::new(Vec::new())));
    let (p, s) = (placed.clone(), sizes.clone());
    let _host = host(move || {
        let (p, s) = (p.clone(), s.clone());
        scoped(move |_| {
            let (p, s) = (p.clone(), s.clone());
            let by_pass = measure_policy(|scope, _, _| {
                let at = if scope.is_looking_ahead() { 40 } else { 10 };
                scope.layout(at, at)
            });
            Layout(
                Modifier
                    .on_placed(move |_: LayoutCoordinates| p.set(p.get() + 1))
                    .on_size_changed(move |size: IntSize| s.borrow_mut().push(size)),
                by_pass,
                callback(|| {}),
            );
        });
    });
    assert_eq!(placed.get(), 1);
    assert_eq!(*sizes.borrow(), [IntSize::new(10, 10)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, nestedVirtualNodes
#[test]
fn a_scope_adds_no_node() {
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    let _host = host(move || {
        let c = c.clone();
        let policy = measure_policy(move |scope, measurables, _| {
            c.set(measurables.len());
            scope.layout(1, 1)
        });
        Layout(
            Modifier,
            policy,
            callback(|| {
                scoped(|_| {
                    scoped(|_| {
                        leaf(Modifier);
                        leaf(Modifier);
                    });
                    leaf(Modifier);
                });
                leaf(Modifier);
            }),
        );
    });
    assert_eq!(count.get(), 4);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, nestedLookaheadScope
#[test]
fn a_nested_scope_is_ahead_with_its_outer_one_once() {
    let log: Log = Rc::default();
    let l = log.clone();
    let _host = host(move || {
        let l = l.clone();
        scoped(move |_| {
            let l = l.clone();
            Stack!({
                let l = l.clone();
                scoped(move |_| leaf(noting(&l).then(sized(10))));
            });
        });
    });
    assert_eq!(
        *log.borrow(),
        [("measure", true), ("measure", false), ("place", true), ("place", false)]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testObtainLookaheadScopeCoordinates
#[test]
fn the_scope_s_coordinates_are_what_it_s_in() {
    let at = Rc::new(RefCell::new(Vec::new()));
    let a = at.clone();
    let _host = host(move || {
        let a = a.clone();
        placing_at(5, Modifier, move || {
            let a = a.clone();
            placing_at(100, Modifier, move || {
                let a = a.clone();
                scoped(move |scope| {
                    let a = a.clone();
                    leaf(Modifier.layout(move |measure, measurable, constraints| {
                        let placeable = measure.measure(measurable, constraints);
                        let a = a.clone();
                        measure.layout_with(placeable.width, placeable.height, move |placement| {
                            let coordinates =
                                scope.lookahead_scope_coordinates(placement).expect("in it");
                            a.borrow_mut().push(coordinates.position_in_root());
                            placement.place(&placeable, 0, 0);
                        })
                    }));
                });
            });
        });
    });
    assert_eq!(*at.borrow(), [Offset::new(5.0, 0.0), Offset::new(5.0, 0.0)]);
}

/// A node added inside a scope later is ahead too, its parent told in both.
#[test]
fn a_node_coming_later_is_looked_ahead() {
    let show = mutable_state_of(false);
    let log: Log = Rc::default();
    let (sh, l) = (show.handle(), log.clone());
    let mut host = host(move || {
        let l = l.clone();
        scoped(move |_| {
            let l = l.clone();
            Stack!({
                if sh.get() {
                    leaf(noting(&l).then(sized(10)));
                }
            });
        });
    });
    assert!(log.borrow().is_empty());
    show.set(true);
    host.frame();
    assert_eq!(
        *log.borrow(),
        [("measure", true), ("measure", false), ("place", true), ("place", false)]
    );
}

/// A panic in a lookahead pass leaves each node's fields as they were, for the next frame.
#[test]
fn a_panic_ahead_leaves_the_passes_apart() {
    let state = mutable_state_of(0);
    let panics = Rc::new(Cell::new(false));
    let sizes = Rc::new(RefCell::new(Vec::new()));
    let kept: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let (st, p, s, k) = (state.handle(), panics.clone(), sizes.clone(), kept.clone());
    let mut host = host(move || {
        let (p, s, k) = (p.clone(), s.clone(), k.clone());
        scoped(move |_| {
            let (p, s, k) = (p.clone(), s.clone(), k.clone());
            let policy = measure_policy(move |scope, _, _| {
                st.get();
                let ahead = scope.is_looking_ahead();
                assert!(!(ahead && p.get()), "the policy's panic");
                s.borrow_mut().push(ahead);
                let at = if ahead { 40 } else { 10 };
                scope.layout(at, at)
            });
            Layout(
                Modifier.on_placed(move |it: LayoutCoordinates| k.set(Some(it))),
                policy,
                callback(|| {}),
            );
        });
    });
    panics.set(true);
    state.set(1);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.frame();
    }));
    assert!(panicked.is_err());
    // What's now wasn't left with what's ahead's.
    assert_eq!(kept.get().expect("placed").size(), IntSize::new(10, 10));
    panics.set(false);
    sizes.borrow_mut().clear();
    host.frame();
    assert_eq!(*sizes.borrow(), [true, false]);
    assert_eq!(kept.get().expect("placed").size(), IntSize::new(10, 10));
}

/// A layout's line is its pass's.
#[test]
fn a_line_is_read_of_its_pass() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        scoped(move |_| {
            let s = s.clone();
            let reading = measure_policy(move |scope, measurables, constraints| {
                let placeable = scope.measure(measurables[0], constraints);
                s.borrow_mut()
                    .push((scope.is_looking_ahead(), scope.alignment_line(&placeable, line)));
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            });
            Layout(
                Modifier,
                reading,
                callback(move || {
                    let giving = measure_policy(move |scope, _, _| {
                        let at = if scope.is_looking_ahead() { 40 } else { 10 };
                        scope.provide_alignment_lines([(line, at)]);
                        scope.layout_with(1, 1, |_| {})
                    });
                    Layout(Modifier, giving, callback(|| {}));
                }),
            );
        });
    });
    assert_eq!(*seen.borrow(), [(true, 40), (false, 10)]);
}

/// A node giving rulers no state holds, asked to look again.
struct NonState {
    ruler: Ruler,
    value: Rc<Cell<f32>>,
    handle: Rc<Cell<Option<NodeHandle>>>,
}

impl ModifierNode for NonState {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.handle.set(Some(context.handle()));
    }
}

impl LayoutModifierNode for NonState {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let (ruler, value) = (self.ruler.clone(), self.value.clone());
        scope.layout_with_rulers(
            placeable.width,
            placeable.height,
            move |rulers| rulers.provides(&ruler, value.get()),
            move |placement| placement.place(&placeable, 0, 0),
        )
    }
}

struct NonStateElement(Ruler, Rc<Cell<f32>>, Rc<Cell<Option<NodeHandle>>>);

impl ModifierNodeElement for NonStateElement {
    type Node = NonState;
    fn create(&self) -> NonState {
        NonState { ruler: self.0.clone(), value: self.1.clone(), handle: self.2.clone() }
    }
    fn update(&self, _: &mut NonState) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, invalidateRulersInLookaheadScope
#[test]
fn invalidate_rulers_works_them_out_in_both_passes() {
    let ruler = VerticalRuler::new();
    let value = Rc::new(Cell::new(10.0));
    let handle: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let reads = Rc::new(RefCell::new(Vec::new()));
    let (r, v, h, re) = (ruler.clone(), value.clone(), handle.clone(), reads.clone());
    let mut host = host(move || {
        let (r, v, h, re) = (r.clone(), v.clone(), h.clone(), re.clone());
        scoped(move |_| {
            let (r, v, h, re) = (r.clone(), v.clone(), h.clone(), re.clone());
            let providing = Modifier.then_custom(NonStateElement(r.clone(), v.clone(), h.clone()));
            Stack!(modifier = providing, {
                let (r, re) = (r.clone(), re.clone());
                leaf(Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let (r, re) = (r.clone(), re.clone());
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        let at = placement.current(&r, f32::NAN);
                        re.borrow_mut().push((placement.is_looking_ahead(), at));
                        placement.place(&placeable, 0, 0);
                    })
                }));
            });
        });
    });
    let last = |ahead: bool| {
        reads.borrow().iter().rev().find(|it: &&(bool, f32)| it.0 == ahead).map(|it| it.1)
    };
    assert_eq!((last(true), last(false)), (Some(10.0), Some(10.0)));
    value.set(20.0);
    host.frame();
    assert_eq!((last(true), last(false)), (Some(10.0), Some(10.0)));
    handle.get().expect("attached").invalidate_rulers();
    host.frame();
    assert_eq!((last(true), last(false)), (Some(20.0), Some(20.0)));
}

/// Each pass remembers what it measured a child with: what's the same for it measures nothing.
#[test]
fn a_pass_keeps_its_own_constraints() {
    let again = mutable_state_of(0);
    let measures = Rc::new(Cell::new(0));
    let (a, m) = (again.handle(), measures.clone());
    let mut host = host(move || {
        let m = m.clone();
        scoped(move |_| {
            let m = m.clone();
            let by_pass = measure_policy(move |scope, measurables, _| {
                a.get();
                let at = if scope.is_looking_ahead() { 40 } else { 10 };
                let placeable = scope.measure(measurables[0], Constraints::fixed(at, at));
                scope.layout_with(at, at, move |placement| placement.place(&placeable, 0, 0))
            });
            let m = m.clone();
            Layout(
                Modifier,
                by_pass,
                callback(move || {
                    let m = m.clone();
                    Layout(
                        Modifier,
                        measure_policy(move |scope, _, constraints| {
                            m.set(m.get() + 1);
                            scope.layout(constraints.max_width, constraints.max_height)
                        }),
                        callback(|| {}),
                    );
                }),
            );
        });
    });
    assert_eq!(measures.get(), 2);
    again.set(1);
    host.frame();
    assert_eq!(measures.get(), 2);
}

/// A read deep in a scope: both passes get down to it.
#[test]
fn a_read_below_the_outermost_measures_both_passes() {
    let width = mutable_state_of(10);
    let log: Log = Rc::default();
    let (w, l) = (width.handle(), log.clone());
    let mut host = host(move || {
        let l = l.clone();
        scoped(move |_| {
            let l = l.clone();
            Stack!({
                let l = l.clone();
                Stack!({
                    Layout(
                        noting(&l),
                        measure_policy(move |scope, _, _| scope.layout(w.get(), 5)),
                        callback(|| {}),
                    );
                });
            });
        });
    });
    log.borrow_mut().clear();
    width.set(20);
    host.frame();
    let ahead = log.borrow().iter().filter(|it| it.0 == "measure" && it.1).count();
    assert_eq!(ahead, 1);
}

/// Blocks only ahead, the pass that isn't placing in its measuring: the frame still gets to them.
#[test]
fn blocks_only_ahead_run_and_run_again() {
    let state = mutable_state_of(0);
    let blocks = Rc::new(Cell::new(0));
    let (st, b) = (state.handle(), blocks.clone());
    let mut host = host(move || {
        let b = b.clone();
        scoped(move |_| {
            let b = b.clone();
            leaf(Modifier.layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                if scope.is_looking_ahead() {
                    let b = b.clone();
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        b.set(b.get() + 1);
                        st.get();
                        placement.place(&placeable, 0, 0);
                    })
                } else {
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                }
            }));
        });
    });
    assert_eq!(blocks.get(), 1);
    state.set(1);
    host.frame();
    assert_eq!(blocks.get(), 2);
}

/// An outermost moved with nothing to do ahead: its lookahead position's where it is now.
#[test]
fn an_outermost_s_lookahead_position_is_where_it_is() {
    let x = mutable_state_of(0);
    let kept: Rc<Cell<Option<(LookaheadScope, LayoutCoordinates)>>> = Rc::default();
    let top: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let (xs, k, t) = (x.handle(), kept.clone(), top.clone());
    let mut host = host(move || {
        let (k, t) = (k.clone(), t.clone());
        let moving = measure_policy(move |scope, measurables, constraints| {
            let placeable = scope.measure(measurables[0], constraints);
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                placement.place(&placeable, xs.get(), 0);
            })
        });
        Layout(
            Modifier.on_placed(move |it: LayoutCoordinates| t.set(Some(it))),
            moving,
            callback(move || {
                let k = k.clone();
                scoped(move |scope| {
                    let k = k.clone();
                    leaf(Modifier.on_placed(move |it: LayoutCoordinates| k.set(Some((scope, it)))));
                });
            }),
        );
    });
    x.set(30);
    host.frame();
    let (scope, child) = kept.get().expect("placed");
    let top = top.get().expect("placed");
    let at = scope
        .to_lookahead_coordinates(top)
        .local_position_of(scope.to_lookahead_coordinates(child), Offset::ZERO);
    assert_eq!(at, Offset::new(30.0, 0.0));
}
