//! Adversarial review of `w2-lookahead` (`D2340`, `D2341`): nested and moved `OWN_ROOT`
//! approach layouts outside a `LookaheadScope`, transitions between explicit `LookaheadScope`s
//! and self-rooted approaches, stale `scope == 0` wrappers after moving out of a scope, and
//! repeated `movable_content_of` moves across identical-size containers.

use crate::support::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::UiHost;
use compose_ui::*;

fn scoped(content: impl Fn(LookaheadScope) + 'static) {
    LookaheadScope(callback(content));
}

fn sized(size: i32) -> Modifier {
    Modifier.size((size as f32).dp())
}

fn leaf(modifier: Modifier) {
    Layout(modifier, empty_measure_policy(), callback(|| {}));
}

fn large_host(content: impl Fn() + 'static) -> UiHost {
    let mut host = UiHost::headless(1000, 1000);
    host.set_content(content);
    host.frame();
    host
}

fn intermediate(
    measure: impl Fn(&mut ApproachMeasureScope<'_, '_>, Measurable, Constraints) -> MeasureResult
    + 'static,
) -> Modifier {
    Modifier.approach_layout(|_| true, |_, _| false, measure)
}

fn passing(
    scope: &mut ApproachMeasureScope<'_, '_>,
    content: Measurable,
    constraints: Constraints,
) -> MeasureResult {
    let placeable = scope.measure(content, constraints);
    scope.layout_with(placeable.width, placeable.height, move |placement| {
        placement.place(&placeable, 0, 0);
    })
}

/// Outside any `LookaheadScope`, an ancestor `approach_layout` (`OWN_ROOT`) is the lookahead
/// root for nested `approach_layout` descendants (`LayoutNode.kt`:
/// `lookaheadRoot = _foldedParent?.lookaheadRoot ?: lookaheadRoot`, `ApproachMeasureScope.kt`:
/// `lookaheadRoot.outerCoordinator`). So the inner approach's `lookahead_scope_coordinates`
/// resolves to layer 0 of the outermost approach ancestor, not to the inner node itself.
#[test]
fn nested_own_root_approaches_resolve_the_outermost_approach_as_scope_coordinates() {
    let inner_pos = Rc::new(Cell::new(IntOffset::new(i32::MAX, i32::MAX)));
    let scope_in_root = Rc::new(Cell::new(IntOffset::new(i32::MAX, i32::MAX)));
    let (ip, sr) = (inner_pos.clone(), scope_in_root.clone());
    let _host = large_host(move || {
        let (ip, sr) = (ip.clone(), sr.clone());
        Stack!(modifier = Modifier.offset(20.dp(), 30.dp()), {
            let (ip, sr) = (ip.clone(), sr.clone());
            // Outer approach node at `(20, 30)` in root; its layer 0 is BEFORE `.offset(50, 50)`.
            Stack!(
                modifier = Modifier
                    .offset(50.dp(), 50.dp())
                    .then(intermediate(passing))
                    .offset(60.dp(), 60.dp()),
                {
                    let (ip, sr) = (ip.clone(), sr.clone());
                    // Intermediate plain layout node (`scope == 0` under outer `OWN_ROOT`).
                    Stack!(modifier = Modifier.offset(3.dp(), 4.dp()), {
                        let (ip, sr) = (ip.clone(), sr.clone());
                        leaf(
                            Modifier
                                .offset(7.dp(), 9.dp())
                                .then(intermediate(move |scope, content, constraints| {
                                    let lookahead = scope.lookahead_scope();
                                    let placeable = scope.measure(content, constraints);
                                    let (ip, sr) = (ip.clone(), sr.clone());
                                    scope.layout_with(
                                        placeable.width,
                                        placeable.height,
                                        move |placement| {
                                            if let (Some(coords), Some(own)) = (
                                                lookahead.lookahead_scope_coordinates(placement),
                                                placement.coordinates(),
                                            ) {
                                                sr.set(coords.local_to_root(Offset::ZERO).round());
                                                let at = lookahead.local_lookahead_position_of(
                                                    coords,
                                                    own,
                                                    Offset::ZERO,
                                                );
                                                ip.set(at.round());
                                            }
                                            placement.place(&placeable, 0, 0);
                                        },
                                    )
                                }))
                                .then(sized(20)),
                        );
                    });
                }
            );
        });
    });
    // Outer approach's `outerCoordinator` (layer 0) sits at `(20, 30)` in root; inside it we have
    // `(50, 50) + (60, 60) + (3, 4) + (7, 9) = (120, 123)`.
    assert_eq!(scope_in_root.get(), IntOffset::new(20, 30));
    assert_eq!(inner_pos.get(), IntOffset::new(120, 123));
}

/// Without an enclosing top-level `LookaheadScope`, moving a `movable_content_of` subtree
/// (`Wrapper` plain `Stack` holding an `approach_layout` leaf):
/// 1. From inside `LookaheadScope` -> into an ancestor `approach_layout` (`OWN_ROOT`) outside any
///    `LookaheadScope`: must clear the stale `LookaheadScope` id and resolve the ancestor
///    `approach_layout`'s layer 0 as `lookahead_scope_coordinates`.
/// 2. From that ancestor `approach_layout` -> into a plain `Stack` outside any `LookaheadScope`
///    (where `LOCAL_LOOKAHEAD_SCOPE` stays `None`, so `movable_content_of` skips recomposition):
///    the `Wrapper` plain `Stack` must stop running a lookahead pass (`is_looking_ahead == false`),
///    and the inner `approach_layout` becomes its own root (layer 0 of the leaf).
/// 3. Back into `LookaheadScope`: resolves the `LookaheadScope` again, and `Wrapper` runs ahead.
#[test]
fn an_approach_moved_from_a_scope_to_an_own_root_to_a_plain_box_and_back() {
    let place = mutable_state_of(1);
    let pos_in_scope = Rc::new(Cell::new(IntOffset::new(i32::MAX, i32::MAX)));
    let scope_in_root = Rc::new(Cell::new(IntOffset::new(i32::MAX, i32::MAX)));
    let wrapper_passes: Rc<RefCell<Vec<bool>>> = Rc::default();
    let (pl, ps, sr, wp) =
        (place.handle(), pos_in_scope.clone(), scope_in_root.clone(), wrapper_passes.clone());

    let mut host = large_host(move || {
        let (ps, sr, wp) = (ps.clone(), sr.clone(), wp.clone());
        Stack!(modifier = Modifier.offset(10.dp(), 15.dp()), {
            let (ps, sr, wp) = (ps.clone(), sr.clone(), wp.clone());
            let movable = remember(move || {
                let (ps, sr, wp) = (ps.clone(), sr.clone(), wp.clone());
                movable_content_of(move || {
                    let (ps, sr, wp) = (ps.clone(), sr.clone(), wp.clone());
                    let note_wrapper = Modifier.layout(move |scope, measurable, constraints| {
                        wp.borrow_mut().push(scope.is_looking_ahead());
                        let placeable = scope.measure(measurable, constraints);
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            placement.place(&placeable, 0, 0);
                        })
                    });
                    Stack!(modifier = note_wrapper.offset(3.dp(), 4.dp()), {
                        let (ps, sr) = (ps.clone(), sr.clone());
                        leaf(
                            Modifier
                                .offset(7.dp(), 9.dp())
                                .then(intermediate(move |scope, content, constraints| {
                                    let lookahead = scope.lookahead_scope();
                                    let placeable = scope.measure(content, constraints);
                                    let (ps, sr) = (ps.clone(), sr.clone());
                                    scope.layout_with(
                                        placeable.width,
                                        placeable.height,
                                        move |placement| {
                                            if let (Some(coords), Some(own)) = (
                                                lookahead.lookahead_scope_coordinates(placement),
                                                placement.coordinates(),
                                            ) {
                                                sr.set(coords.local_to_root(Offset::ZERO).round());
                                                let at = lookahead.local_lookahead_position_of(
                                                    coords,
                                                    own,
                                                    Offset::ZERO,
                                                );
                                                ps.set(at.round());
                                            }
                                            placement.place(&placeable, 0, 0);
                                        },
                                    )
                                }))
                                .then(sized(20)),
                        );
                    });
                })
            });

            // 1: inside an explicit LookaheadScope, whose parent sits at `(10, 15)` in root.
            let m1 = movable.clone();
            Stack!({
                let m1 = m1.clone();
                scoped(move |_| {
                    let m1 = m1.clone();
                    Stack!(modifier = Modifier.offset(100.dp(), 5.dp()), {
                        if pl.get() == 1 {
                            m1();
                        }
                    });
                });
            });

            // 2: outside any LookaheadScope, inside an ancestor approach_layout at `(10, 15)` in
            // root (`layer 0` before `.offset(50, 50)`).
            let m2 = movable.clone();
            Stack!(
                modifier = Modifier
                    .offset(50.dp(), 50.dp())
                    .then(intermediate(passing))
                    .offset(60.dp(), 60.dp()),
                {
                    if pl.get() == 2 {
                        m2();
                    }
                }
            );

            // 3: outside any LookaheadScope, inside a plain Stack at `(10 + 40, 15 + 200) =
            // (50, 215)`; the Wrapper sits at `(53, 219)`, so the leaf's layer 0 sits at
            // `(53, 219)` in root, and inside the leaf `.offset(7, 9)` precedes the approach.
            let m3 = movable;
            Stack!(modifier = Modifier.offset(40.dp(), 200.dp()), {
                if pl.get() == 3 {
                    m3();
                }
            });
        });
    });

    // Step 1: inside `LookaheadScope` -> `(100 + 3 + 7, 5 + 4 + 9) = (110, 18)` in scope `(10, 15)`.
    assert_eq!(scope_in_root.get(), IntOffset::new(10, 15));
    assert_eq!(pos_in_scope.get(), IntOffset::new(110, 18));
    assert!(wrapper_passes.borrow().contains(&true), "{:?}", wrapper_passes.borrow());

    // Step 2: moved directly from `LookaheadScope` into ancestor `OWN_ROOT` -> `(50 + 60 + 3 + 7,
    // 50 + 60 + 4 + 9) = (120, 123)` in ancestor's layer 0 at `(10, 15)`.
    wrapper_passes.borrow_mut().clear();
    place.set(2);
    host.frame();
    assert_eq!(scope_in_root.get(), IntOffset::new(10, 15));
    assert_eq!(pos_in_scope.get(), IntOffset::new(120, 123));
    assert!(wrapper_passes.borrow().contains(&true), "{:?}", wrapper_passes.borrow());

    // Step 3: moved from ancestor `OWN_ROOT` into plain `Stack` (`None -> None`, no recomposition).
    // Wrapper is above the leaf's `OWN_ROOT`, so Wrapper itself must NOT run a lookahead pass!
    // Leaf's layer 0 is at `(10 + 40 + 3, 15 + 200 + 4) = (53, 219)` in root, and the approach
    // layer is at `(7, 9)` inside the leaf's layer 0.
    wrapper_passes.borrow_mut().clear();
    place.set(3);
    host.frame();
    assert_eq!(scope_in_root.get(), IntOffset::new(53, 219));
    assert_eq!(pos_in_scope.get(), IntOffset::new(7, 9));
    assert_eq!(*wrapper_passes.borrow(), [false], "wrapper above OWN_ROOT runs only once, now");

    // Step 4: moved back into `LookaheadScope`.
    wrapper_passes.borrow_mut().clear();
    place.set(1);
    host.frame();
    assert_eq!(scope_in_root.get(), IntOffset::new(10, 15));
    assert_eq!(pos_in_scope.get(), IntOffset::new(110, 18));
    assert!(wrapper_passes.borrow().contains(&true), "{:?}", wrapper_passes.borrow());
}

/// An approach starting outside any `LookaheadScope` (`OWN_ROOT`) and moving into an inner
/// `LookaheadScope` nested inside an outer `LookaheadScope` resolves the innermost scope, then
/// moving to the outer scope resolves the outer scope, then back outside resolves `OWN_ROOT`.
#[test]
fn an_own_root_approach_moved_into_nested_scopes_and_out_again() {
    let where_at = mutable_state_of(0);
    let scope_in_root = Rc::new(Cell::new(IntOffset::new(i32::MAX, i32::MAX)));
    let pos_in_scope = Rc::new(Cell::new(IntOffset::new(i32::MAX, i32::MAX)));
    let (w, sr, ps) = (where_at.handle(), scope_in_root.clone(), pos_in_scope.clone());

    let mut host = large_host(move || {
        let (sr, ps) = (sr.clone(), ps.clone());
        let movable = remember(move || {
            let (sr, ps) = (sr.clone(), ps.clone());
            movable_content_of(move || {
                let (sr, ps) = (sr.clone(), ps.clone());
                leaf(
                    Modifier
                        .offset(5.dp(), 6.dp())
                        .then(intermediate(move |scope, content, constraints| {
                            let lookahead = scope.lookahead_scope();
                            let placeable = scope.measure(content, constraints);
                            let (sr, ps) = (sr.clone(), ps.clone());
                            scope.layout_with(placeable.width, placeable.height, move |placement| {
                                if let (Some(coords), Some(own)) = (
                                    lookahead.lookahead_scope_coordinates(placement),
                                    placement.coordinates(),
                                ) {
                                    sr.set(coords.local_to_root(Offset::ZERO).round());
                                    ps.set(
                                        lookahead
                                            .local_lookahead_position_of(coords, own, Offset::ZERO)
                                            .round(),
                                    );
                                }
                                placement.place(&placeable, 0, 0);
                            })
                        }))
                        .then(sized(15)),
                );
            })
        });

        let m0 = movable.clone();
        Stack!(modifier = Modifier.offset(12.dp(), 18.dp()), {
            if w.get() == 0 {
                m0();
            }
        });

        let (m1, m2) = (movable.clone(), movable);
        Stack!(modifier = Modifier.offset(30.dp(), 40.dp()), {
            let (m1, m2) = (m1.clone(), m2.clone());
            scoped(move |_| {
                let (m1, m2) = (m1.clone(), m2.clone());
                Stack!(modifier = Modifier.offset(10.dp(), 20.dp()), {
                    if w.get() == 2 {
                        m2();
                    }
                    let m1 = m1.clone();
                    scoped(move |_| {
                        let m1 = m1.clone();
                        Stack!(modifier = Modifier.offset(7.dp(), 8.dp()), {
                            if w.get() == 1 {
                                m1();
                            }
                        });
                    });
                });
            });
        });
    });

    // 0: outside any scope -> own root at `(12, 18)`, internal offset `(5, 6)`.
    assert_eq!(scope_in_root.get(), IntOffset::new(12, 18));
    assert_eq!(pos_in_scope.get(), IntOffset::new(5, 6));

    // 1: inner LookaheadScope -> parent is at `(30 + 10, 40 + 20) = (40, 60)`, offset `(7 + 5, 8 + 6) = (12, 14)`.
    where_at.set(1);
    host.frame();
    assert_eq!(scope_in_root.get(), IntOffset::new(40, 60));
    assert_eq!(pos_in_scope.get(), IntOffset::new(12, 14));

    // 2: outer LookaheadScope -> parent is at `(30, 40)`, offset `(10 + 5, 20 + 6) = (15, 26)`.
    where_at.set(2);
    host.frame();
    assert_eq!(scope_in_root.get(), IntOffset::new(30, 40));
    assert_eq!(pos_in_scope.get(), IntOffset::new(15, 26));

    // Back to 0: outside any scope again.
    where_at.set(0);
    host.frame();
    assert_eq!(scope_in_root.get(), IntOffset::new(12, 18));
    assert_eq!(pos_in_scope.get(), IntOffset::new(5, 6));
}

/// Moving a `LookaheadScope` back and forth across fixed-size containers (`D2340`) re-runs both
/// lookahead and approach placement blocks on every move even when constraints and size stay
/// identical.
#[test]
fn a_moved_lookahead_scope_reruns_both_passes_across_repeated_round_trips() {
    let slot = mutable_state_of(0usize);
    let ahead_root = Rc::new(Cell::new(IntOffset::ZERO));
    let now_root = Rc::new(Cell::new(IntOffset::ZERO));
    let (sl, ar, nr) = (slot.handle(), ahead_root.clone(), now_root.clone());
    let offsets = [IntOffset::new(15, 25), IntOffset::new(90, 140), IntOffset::new(210, 55)];

    let mut host = large_host(move || {
        let (ar, nr) = (ar.clone(), nr.clone());
        let movable = remember(move || {
            let (ar, nr) = (ar.clone(), nr.clone());
            movable_content_of(move || {
                let (ar, nr) = (ar.clone(), nr.clone());
                scoped(move |scope| {
                    let (ar, nr) = (ar.clone(), nr.clone());
                    leaf(
                        Modifier
                            .layout(move |measure, measurable, constraints| {
                                let placeable = measure.measure(measurable, constraints);
                                let (ar, nr) = (ar.clone(), nr.clone());
                                measure.layout_with(
                                    placeable.width,
                                    placeable.height,
                                    move |placement| {
                                        if let Some(coords) =
                                            scope.lookahead_scope_coordinates(placement)
                                        {
                                            let in_root =
                                                coords.local_to_root(Offset::ZERO).round();
                                            if placement.is_looking_ahead() {
                                                ar.set(in_root);
                                            } else {
                                                nr.set(in_root);
                                            }
                                        }
                                        placement.place(&placeable, 0, 0);
                                    },
                                )
                            })
                            .then(sized(60)),
                    );
                });
            })
        });

        for (index, at) in offsets.into_iter().enumerate() {
            let m = movable.clone();
            Stack!(
                modifier = Modifier.offset((at.x as f32).dp(), (at.y as f32).dp()).then(sized(60)),
                {
                    if sl.get() == index {
                        m();
                    }
                }
            );
        }
    });

    for &target in &[0, 1, 2, 0, 2, 1, 0] {
        slot.set(target);
        host.frame();
        assert_eq!(ahead_root.get(), offsets[target], "ahead at slot {target}");
        assert_eq!(now_root.get(), offsets[target], "now at slot {target}");
    }
}
