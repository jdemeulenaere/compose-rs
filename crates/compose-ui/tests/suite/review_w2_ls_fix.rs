//! Adversarial review tests for `w2-ls-fix` (D2450, revising D2383): content the pass ahead passed
//! over is placed ahead, as it's placed now, before it's placed now, as Compose's
//! `needsToBePlacedInApproach` (`LookaheadPassDelegate.onApproachPlacement`,
//! `MeasurePassDelegate.placeSelf`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::support::*;
use compose_runtime::*;
use compose_ui::subcompose::{SubcomposeLayout, subcompose_policy};
use compose_ui::*;

type Log = Rc<RefCell<Vec<(&'static str, bool)>>>;
type Kept<T> = Rc<Cell<Option<T>>>;

fn scoped(content: impl Fn(LookaheadScope) + 'static) {
    LookaheadScope(callback(content));
}

fn sized(size: i32) -> Modifier {
    Modifier.size((size as f32).dp())
}

fn leaf(modifier: Modifier) {
    Layout(modifier, empty_measure_policy(), callback(|| {}));
}

/// Notes each run of its block as `name` with its pass, reading `read` (if any) in both.
fn noting(name: &'static str, log: &Log, read: Option<MutableState<i32>>) -> Modifier {
    let log = log.clone();
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let log = log.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if let Some(read) = read {
                read.get();
            }
            log.borrow_mut().push((name, placement.is_looking_ahead()));
            placement.place(&placeable, 0, 0);
        })
    })
}

/// A layout measuring its first child in both passes and placing it at `now()` now, and ahead at
/// `ahead()` or not at all. Both read in its block.
fn placing(
    ahead: impl Fn() -> Option<i32> + 'static,
    now: impl Fn() -> i32 + 'static,
    modifier: Modifier,
    content: impl Fn() + 'static,
) {
    let ahead: Rc<dyn Fn() -> Option<i32>> = Rc::new(ahead);
    let now: Rc<dyn Fn() -> i32> = Rc::new(now);
    let policy = measure_policy(move |scope, measurables, constraints| {
        let Some(&first) = measurables.first() else {
            return scope.layout_with(0, 0, |_| {});
        };
        let placeable = scope.measure(first, constraints);
        let (ahead, now) = (ahead.clone(), now.clone());
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            let at = if placement.is_looking_ahead() { ahead() } else { Some(now()) };
            if let Some(at) = at {
                placement.place(&placeable, at, 0);
            }
        })
    });
    Layout(modifier, policy, callback(content));
}

#[derive(Clone, Default)]
struct Probe {
    placed: Rc<Cell<u32>>,
    unplaced: Rc<Cell<u32>>,
}

impl PartialEq for Probe {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.placed, &other.placed)
    }
}

struct ProbeNode(Probe);

impl ModifierNode for ProbeNode {
    fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
        Some(self)
    }

    fn as_unplaced_aware(&mut self) -> Option<&mut dyn UnplacedAwareModifierNode> {
        Some(self)
    }
}

impl LayoutAwareModifierNode for ProbeNode {
    fn on_placed(&mut self, _: LayoutCoordinates, _: &mut NodeContext<'_>) {
        self.0.placed.set(self.0.placed.get() + 1);
    }
}

impl UnplacedAwareModifierNode for ProbeNode {
    fn on_unplaced(&mut self, _: &mut NodeContext<'_>) {
        self.0.unplaced.set(self.0.unplaced.get() + 1);
    }
}

struct ProbeElement(Probe);

impl ModifierNodeElement for ProbeElement {
    type Node = ProbeNode;

    fn create(&self) -> ProbeNode {
        ProbeNode(self.0.clone())
    }

    fn update(&self, node: &mut ProbeNode) {
        node.0 = self.0.clone();
    }

    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

fn probe(probe: &Probe) -> Modifier {
    Modifier.then_custom(ProbeElement(probe.clone()))
}

/// What a test keeps to ask where a child is in either pass, after a frame.
#[derive(Clone, Default)]
struct Where {
    scope: Kept<LookaheadScope>,
    top: Kept<LayoutCoordinates>,
    child: Kept<LayoutCoordinates>,
}

impl Where {
    fn keep_top(&self) -> Modifier {
        let top = self.top.clone();
        Modifier.on_placed(move |it: LayoutCoordinates| top.set(Some(it)))
    }

    fn keep_child(&self) -> Modifier {
        let child = self.child.clone();
        Modifier.on_placed(move |it: LayoutCoordinates| child.set(Some(it)))
    }

    /// The child's x in what's around the scope: (ahead, now).
    fn x(&self) -> (f32, f32) {
        let (scope, top, child) = (
            self.scope.get().expect("composed"),
            self.top.get().expect("placed"),
            self.child.get().expect("placed"),
        );
        (
            scope.local_lookahead_position_of(top, child, Offset::ZERO).x,
            top.local_position_of(&child, Offset::ZERO).x,
        )
    }
}

/// A child its parent places now only, at `x` now: placed ahead where it is now, its lookahead
/// position following each move now, as Compose's `MeasurePassDelegate.placeSelf` places a node
/// `needsToBePlacedInApproach` ahead at the position it's given now, each time it's placed now.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/MeasurePassDelegate.kt, placeSelf
#[test]
#[ignore = "review: a child placed now only keeps its first lookahead position when it moves now \
            with no blocks waiting ahead (D2450 places ahead only with blocks waiting there)"]
fn a_child_placed_now_only_follows_where_it_is_now_ahead() {
    let x = mutable_state_of(1);
    let at = Where::default();
    let (xs, w) = (x.handle(), at.clone());
    let mut host = host(move || {
        let w = w.clone();
        Stack!(modifier = w.keep_top(), {
            let w = w.clone();
            scoped(move |scope| {
                w.scope.set(Some(scope));
                let w = w.clone();
                placing(
                    || None,
                    move || xs.get(),
                    Modifier,
                    move || {
                        leaf(w.keep_child().then(sized(10)));
                    },
                );
            });
        });
    });
    assert_eq!(at.x(), (1.0, 1.0));
    x.set(9);
    host.frame();
    assert_eq!(at.x(), (9.0, 9.0));
}

/// The first frame's half of the above: placed ahead where it is now, the lookahead position its
/// position now (D2450).
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LookaheadPassDelegate.kt, onApproachPlacement
#[test]
#[ignore = "review: a child placed now only with no blocks waiting ahead (only eager placing in \
            its chain) is never placed ahead: its lookahead position stays 0 (D2450 places ahead \
            only with blocks waiting there)"]
fn a_child_placed_now_only_is_ahead_where_it_is_now() {
    let at = Where::default();
    let w = at.clone();
    let _host = host(move || {
        let w = w.clone();
        Stack!(modifier = w.keep_top(), {
            let w = w.clone();
            scoped(move |scope| {
                w.scope.set(Some(scope));
                let w = w.clone();
                placing(
                    || None,
                    || 4,
                    Modifier,
                    move || {
                        leaf(w.keep_child().then(sized(10)));
                    },
                );
            });
        });
    });
    assert_eq!(at.x(), (4.0, 4.0));
}

/// A child its parent places ahead, then stops placing there (still placing it now), then places
/// there again: placed ahead where it is now while its parent doesn't, as Compose's
/// `onApproachPlacement` finds it `IsNotPlaced` and makes it `detachedFromParentLookaheadPlacement`;
/// where its parent puts it once it does again (`placeSelf` from a parent `LookaheadLayingOut`
/// clears the flag).
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LookaheadPassDelegate.kt, onApproachPlacement
#[test]
#[ignore = "review: a child its parent stops placing ahead keeps its old lookahead position until \
            it next has blocks waiting ahead (the not-modelled half of D2450)"]
fn a_child_its_parent_stops_placing_ahead_is_ahead_where_it_is_now() {
    let ahead = mutable_state_of(Some(7));
    let at = Where::default();
    let (a, w) = (ahead.handle(), at.clone());
    let mut host = host(move || {
        let w = w.clone();
        Stack!(modifier = w.keep_top(), {
            let w = w.clone();
            scoped(move |scope| {
                w.scope.set(Some(scope));
                let w = w.clone();
                placing(
                    move || a.get(),
                    || 3,
                    Modifier,
                    move || {
                        leaf(w.keep_child().then(sized(10)));
                    },
                );
            });
        });
    });
    assert_eq!(at.x(), (7.0, 3.0));
    ahead.set(None);
    host.frame();
    assert_eq!(at.x(), (3.0, 3.0));
    ahead.set(Some(7));
    host.frame();
    assert_eq!(at.x(), (7.0, 3.0));
}

/// The parts of the above that hold: placed ahead by its parent again, it's where its parent puts
/// it, not where it was placed ahead as it's placed now; and placed ahead now only when it has
/// blocks waiting ahead, it's where it is now.
#[test]
fn a_child_toggling_placed_ahead_with_blocks_waiting_is_where_each_pass_puts_it() {
    let ahead = mutable_state_of(Some(7));
    let tick = mutable_state_of(0);
    let at = Where::default();
    let log: Log = Rc::default();
    let (a, t, w, l) = (ahead.handle(), tick.handle(), at.clone(), log.clone());
    let mut host = host(move || {
        let (w, l) = (w.clone(), l.clone());
        Stack!(modifier = w.keep_top(), {
            let (w, l) = (w.clone(), l.clone());
            scoped(move |scope| {
                w.scope.set(Some(scope));
                let (w, l) = (w.clone(), l.clone());
                placing(
                    move || a.get(),
                    || 3,
                    Modifier,
                    move || {
                        leaf(w.keep_child().then(noting("child", &l, Some(t))).then(sized(10)));
                    },
                );
            });
        });
    });
    assert_eq!(at.x(), (7.0, 3.0));
    assert_eq!(*log.borrow(), [("child", true), ("child", false)]);
    for (step, placed_ahead) in [None, Some(7), None, Some(11), None].into_iter().enumerate() {
        log.borrow_mut().clear();
        ahead.set(placed_ahead);
        tick.set(step as i32 + 1);
        host.frame();
        let expected = placed_ahead.unwrap_or(3) as f32;
        assert_eq!(at.x(), (expected, 3.0), "step {step}");
        // Each block once a frame, ahead first.
        assert_eq!(*log.borrow(), [("child", true), ("child", false)], "step {step}");
    }
}

/// Toggled between placed and not placed ahead, a child placed now throughout is told it's placed
/// once and never told it's unplaced: what's ahead isn't shown (`markNodeAndSubtreeAsPlaced` of
/// the lookahead delegate calls no `onPlaced`). Its blocks run once a frame, ahead first.
#[test]
fn a_child_toggling_placed_ahead_hears_of_its_placing_now_only() {
    let ahead = mutable_state_of(Some(7));
    let tick = mutable_state_of(0);
    let told = Probe::default();
    let log: Log = Rc::default();
    let (a, t, p, l) = (ahead.handle(), tick.handle(), told.clone(), log.clone());
    let mut host = host(move || {
        let (p, l) = (p.clone(), l.clone());
        scoped(move |_| {
            let (p, l) = (p.clone(), l.clone());
            placing(
                move || a.get(),
                || 3,
                Modifier,
                move || {
                    leaf(probe(&p).then(noting("child", &l, Some(t))).then(sized(10)));
                },
            );
        });
    });
    assert_eq!((told.placed.get(), told.unplaced.get()), (1, 0));
    for step in 0..6 {
        log.borrow_mut().clear();
        ahead.set(if step % 2 == 0 { None } else { Some(7) });
        if step % 3 != 2 {
            tick.set(step + 1);
        }
        host.frame();
        assert_eq!(told.unplaced.get(), 0, "step {step}");
        let ahead_runs = log.borrow().iter().filter(|it| it.1).count();
        let now_runs = log.borrow().iter().filter(|it| !it.1).count();
        assert!(ahead_runs <= 1 && now_runs <= 1, "step {step}: {:?}", log.borrow());
        if step % 3 != 2 {
            assert_eq!(*log.borrow(), [("child", true), ("child", false)], "step {step}");
        }
    }
}

/// A child placed now only that its parent then stops placing now too: unplaced, told once; placed
/// in neither pass, it isn't placed ahead either (its blocks ahead don't run); placed now again,
/// it's placed ahead first again (D2451).
#[test]
fn a_child_placed_now_only_then_in_neither_pass_is_told_and_not_placed_ahead() {
    let shown = mutable_state_of(true);
    let tick = mutable_state_of(0);
    let told = Probe::default();
    let log: Log = Rc::default();
    let (s, t, p, l) = (shown.handle(), tick.handle(), told.clone(), log.clone());
    let mut host = host(move || {
        let (p, l) = (p.clone(), l.clone());
        scoped(move |_| {
            let (p, l) = (p.clone(), l.clone());
            let policy = measure_policy(move |scope, measurables, constraints| {
                let placeable = scope.measure(measurables[0], constraints);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    if !placement.is_looking_ahead() && s.get() {
                        placement.place(&placeable, 2, 0);
                    }
                })
            });
            Layout(
                Modifier,
                policy,
                callback(move || {
                    leaf(probe(&p).then(noting("child", &l, Some(t))).then(sized(10)));
                }),
            );
        });
    });
    assert_eq!(*log.borrow(), [("child", true), ("child", false)]);
    assert_eq!((told.placed.get(), told.unplaced.get()), (1, 0));
    log.borrow_mut().clear();
    shown.set(false);
    tick.set(1);
    host.frame();
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    assert_eq!(told.unplaced.get(), 1);
    log.borrow_mut().clear();
    shown.set(true);
    host.frame();
    assert_eq!(*log.borrow(), [("child", true), ("child", false)]);
    assert_eq!((told.placed.get(), told.unplaced.get()), (2, 1));
}

/// A nested scope in content its parent places now only: what's in it is placed ahead once and now
/// once, ahead first, its outermost being no root (its parent's in a scope).
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testSkipPlacementInLookahead
#[test]
fn a_nested_scope_in_content_placed_now_only_is_placed_ahead_once() {
    let tick = mutable_state_of(0);
    let log: Log = Rc::default();
    let (t, l) = (tick.handle(), log.clone());
    let mut host = host(move || {
        let l = l.clone();
        scoped(move |_| {
            let l = l.clone();
            placing(
                || None,
                || 0,
                noting("outer", &l, None),
                move || {
                    let l = l.clone();
                    scoped(move |_| {
                        let l = l.clone();
                        Stack!(modifier = noting("box", &l, None), {
                            leaf(noting("inner", &l, Some(t)).then(sized(10)));
                        });
                    });
                },
            );
        });
    });
    let ahead: Vec<_> = log.borrow().iter().filter(|it| it.1).map(|it| it.0).collect();
    let now: Vec<_> = log.borrow().iter().filter(|it| !it.1).map(|it| it.0).collect();
    assert_eq!(ahead, ["outer", "box", "inner"]);
    assert_eq!(now, ["outer", "box", "inner"]);
    // Each placed ahead before it's placed now.
    let first = |entry| log.borrow().iter().position(|it| *it == entry).expect("ran");
    for name in ["outer", "box", "inner"] {
        assert!(first((name, true)) < first((name, false)), "{name}: {:?}", log.borrow());
    }
    log.borrow_mut().clear();
    tick.set(1);
    host.frame();
    assert_eq!(*log.borrow(), [("inner", true), ("inner", false)]);
}

/// A grandchild with a read only ahead, under a child its parent places now only, when the
/// parent's block ahead runs again in the same frame (forgetting it placed the child, which it
/// didn't): the grandchild's blocks run once each, ahead first (a read ahead places both, as
/// `a_state_read_placing_places_by_where_it_was_read`), and the one ahead sees the read.
#[test]
fn a_grandchild_read_ahead_under_a_child_placed_now_only_runs_once_as_its_parent_reruns() {
    let parent_read = mutable_state_of(0);
    let read = mutable_state_of(0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log: Log = Rc::default();
    let (pr, r, s, l) = (parent_read.handle(), read.handle(), seen.clone(), log.clone());
    let mut host = host(move || {
        let (s, l) = (s.clone(), l.clone());
        scoped(move |_| {
            let (s, l) = (s.clone(), l.clone());
            placing(
                move || {
                    pr.get();
                    None
                },
                || 1,
                Modifier,
                move || {
                    let (s, l) = (s.clone(), l.clone());
                    Stack!(modifier = noting("child", &l, None), {
                        let (s, l) = (s.clone(), l.clone());
                        leaf(
                            Modifier
                                .layout(move |scope, measurable, constraints| {
                                    let placeable = scope.measure(measurable, constraints);
                                    let (s, l) = (s.clone(), l.clone());
                                    scope.layout_with(
                                        placeable.width,
                                        placeable.height,
                                        move |placement| {
                                            let ahead = placement.is_looking_ahead();
                                            if ahead {
                                                s.borrow_mut().push(r.get());
                                            }
                                            l.borrow_mut().push(("grandchild", ahead));
                                            placement.place(&placeable, 0, 0);
                                        },
                                    )
                                })
                                .then(sized(10)),
                        );
                    });
                },
            );
        });
    });
    assert_eq!(*seen.borrow(), [0]);
    for step in 1..4 {
        log.borrow_mut().clear();
        parent_read.set(step);
        read.set(step);
        host.frame();
        assert_eq!(*log.borrow(), [("grandchild", true), ("grandchild", false)], "step {step}");
        assert_eq!(seen.borrow().last(), Some(&step), "step {step}");
    }
    // The parent's block ahead alone: nothing under it runs again.
    log.borrow_mut().clear();
    parent_read.set(9);
    host.frame();
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}

/// A `SubcomposeLayout` measuring its slot ahead and placing it now only, as Compose's
/// `detachedFromParentLookaheadPlacement` was made for: the slot's content is placed ahead first,
/// once a frame, as its slot changes; nothing panics placing it twice.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LayoutNodeLayoutDelegate.kt, detachedFromParentLookaheadPlacement
#[test]
fn a_subcompose_slot_placed_now_only_is_placed_ahead_first_as_it_changes() {
    let version = mutable_state_of(0);
    let log: Log = Rc::default();
    let (v, l) = (version.handle(), log.clone());
    let mut host = host(move || {
        let l = l.clone();
        scoped(move |_| {
            let l = l.clone();
            let policy = subcompose_policy(move |scope, constraints| {
                let l = l.clone();
                let measurables = scope.subcompose(
                    "slot",
                    callback(move || {
                        let side = 10 + v.get();
                        leaf(noting("slot", &l, None).then(sized(side)));
                    }),
                );
                let placeable = scope.measure(measurables[0], constraints);
                let ahead = scope.is_looking_ahead();
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    if !ahead {
                        placement.place(&placeable, 0, 0);
                    }
                })
            });
            SubcomposeLayout(Modifier, policy);
        });
    });
    assert_eq!(*log.borrow(), [("slot", true), ("slot", false)]);
    for step in 1..4 {
        log.borrow_mut().clear();
        version.set(step);
        host.frame();
        assert_eq!(*log.borrow(), [("slot", true), ("slot", false)], "step {step}");
    }
}

/// Movable content moved from a parent placing it now only into one placing it in both passes,
/// and back: each pass's block runs once a frame, ahead first, and its lookahead position is where
/// its parent put it ahead, or where it is now.
#[test]
fn movable_content_moved_into_and_out_of_a_parent_placing_now_only() {
    let skipping = mutable_state_of(true);
    let at = Where::default();
    let log: Log = Rc::default();
    let (sk, w, l) = (skipping.handle(), at.clone(), log.clone());
    let mut host = host(move || {
        let (w, l) = (w.clone(), l.clone());
        Stack!(modifier = w.keep_top(), {
            let (w, l) = (w.clone(), l.clone());
            scoped(move |scope| {
                w.scope.set(Some(scope));
                let (w, l) = (w.clone(), l.clone());
                let content = remember(move || {
                    let (w, l) = (w.clone(), l.clone());
                    movable_content_of(move || {
                        leaf(w.keep_child().then(noting("moved", &l, None)).then(sized(10)));
                    })
                });
                let one = content.clone();
                Stack!({
                    let one = one.clone();
                    placing(
                        || None,
                        || 4,
                        Modifier,
                        move || {
                            if sk.get() {
                                one();
                            }
                        },
                    );
                    let two = content.clone();
                    placing(
                        || Some(6),
                        || 2,
                        Modifier,
                        move || {
                            if !sk.get() {
                                two();
                            }
                        },
                    );
                });
            });
        });
    });
    assert_eq!(*log.borrow(), [("moved", true), ("moved", false)]);
    assert_eq!(at.x(), (4.0, 4.0));
    for (step, now_only) in [false, true, false].into_iter().enumerate() {
        log.borrow_mut().clear();
        skipping.set(now_only);
        host.frame();
        assert_eq!(*log.borrow(), [("moved", true), ("moved", false)], "step {step}");
        let expected = if now_only { (4.0, 4.0) } else { (6.0, 2.0) };
        assert_eq!(at.x(), expected, "step {step}");
    }
}
