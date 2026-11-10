//! The rest of Compose's `LookaheadScopeTest` scenes that need foundation's layouts: a flow row
//! with an animated size, a lazy list with animated items, an approach skipping its content's
//! placement ahead, intrinsics asked in either pass. Density 1.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use compose_animation::{AnimationModifiers, tween};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::subcompose::*;
use compose_ui::{
    Color, Constraints, IntOffset, IntSize, IntrinsicMeasurable, IntrinsicMeasureScope,
    IntrinsicSize, LayoutCoordinates, LayoutModifierNode, LookaheadScope, Measurable,
    MeasureResult, MeasureScope, ModifierNode, ModifierNodeElement, Offset, UnitExt,
};

fn dp(pixels: i32) -> compose_ui::Dp {
    pixels.dp()
}

fn in_a_scope(content: impl Fn(LookaheadScope) + 'static) {
    LookaheadScope(callback(content));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testNestedLookaheadPlacement
/// A flow row centring a box whose size animates and a flow column holding a box: once the size
/// has settled after each change, the column's box is where it is ahead, both read from the root.
#[test]
fn a_box_in_a_flow_beside_an_animated_size_ends_where_it_is_ahead() {
    let expanded = mutable_state_of(true);
    type Seen = Rc<Cell<Option<Offset>>>;
    let (now, ahead): (Seen, Seen) = (Rc::default(), Rc::default());
    let (e, n, a) = (expanded.handle(), now.clone(), ahead.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let (n, a) = (n.clone(), a.clone());
        in_a_scope(move |scope| {
            let (n, a) = (n.clone(), a.clone());
            FlowRow!(
                modifier = Modifier.fill_max_size(),
                horizontal_arrangement = Arrangement::CENTER,
                vertical_arrangement = Arrangement::CENTER,
                max_items_in_each_row = 3,
                move |_| {
                    let height = if e.get() { dp(500) } else { dp(300) };
                    Box!(
                        modifier = Modifier
                            .animate_content_size()
                            .width_in(compose_ui::Dp::UNSPECIFIED, dp(600))
                            .background(Color::RED),
                        { Box!(modifier = Modifier.fill_max_width().height(height)) }
                    );
                    let (n, a) = (n.clone(), a.clone());
                    FlowColumn!(move |_| {
                        let (n, a) = (n.clone(), a.clone());
                        let reading = Modifier.layout(move |measure, measurable, constraints| {
                            let placeable = measure.measure(measurable, constraints);
                            let (n, a) = (n.clone(), a.clone());
                            measure.layout_with(
                                placeable.width,
                                placeable.height,
                                move |placement| {
                                    if let Some(own) = placement.coordinates() {
                                        let root = own.find_root_coordinates();
                                        if placement.is_looking_ahead() {
                                            a.set(Some(scope.local_lookahead_position_of(
                                                root,
                                                own,
                                                Offset::ZERO,
                                            )));
                                        } else {
                                            n.set(Some(root.local_position_of(&own, Offset::ZERO)));
                                        }
                                    }
                                    placement.place(&placeable, 0, 0);
                                },
                            )
                        });
                        Box!(
                            modifier = Modifier
                                .size(dp(200))
                                .then(reading)
                                .wrap_content_width()
                                .height_in(dp(156), compose_ui::Dp::UNSPECIFIED)
                                .background(Color::BLUE),
                            { Box!(modifier = Modifier.size(dp(200))) }
                        );
                    });
                }
            );
        });
    });
    ui.wait_for_idle();
    for _ in 0..2 {
        expanded.set(!expanded.get());
        ui.wait_for_idle();
        assert!(now.get().is_some());
        assert_eq!(now.get(), ahead.get());
        now.set(None);
        ahead.set(None);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testApproachUsingContentNotComposedInLookahead
/// A lazy column of 500 keyed items animating their placement and fading out, in a scope,
/// shuffled 30 times: what the pass that's now composes, which the pass ahead didn't, is laid out
/// without a panic.
#[test]
fn a_lazy_list_of_animated_items_shuffled_in_a_scope_lays_out() {
    let items = mutable_state_of(Vec::from_iter(0..500));
    let it = items.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        in_a_scope(move |_| {
            let list = it.get();
            LazyColumn!(
                modifier = Modifier.required_height(dp(400)),
                move |scope: LazyListScope| {
                    let item_scope = scope.item_scope();
                    scope.items_list_keyed(
                        list.clone(),
                        |it| *it,
                        move |_, &item| {
                            let modifier = Modifier.animate_item_with(
                                &item_scope,
                                None,
                                Some(tween(15).into()),
                                Some(tween(15).into()),
                            );
                            Box!(modifier = modifier, {
                                Box!(move |_| BasicText!(format!("Item {item}")));
                            });
                        },
                    );
                }
            );
        });
    });
    let mut seed: u64 = 11;
    for _ in 0..30 {
        ui.wait_for_idle();
        let mut list = items.get();
        for at in (1..list.len()).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            list.swap(at, (seed >> 33) as usize % (at + 1));
        }
        items.set(list);
    }
    ui.wait_for_idle();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testSkipPlacementInLookahead
/// An approach (its size ahead its constraints now) holding a box whose layout ahead measures its
/// content but places nothing: the content is still measured and placed ahead, as Compose's
/// `ConditionallyMeasureAndPlace`.
#[test]
fn content_its_parent_doesn_t_place_ahead_is_still_placed_ahead() {
    let (measured, placed) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let (m, p) = (measured.clone(), placed.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let (m, p) = (m.clone(), p.clone());
        in_a_scope(move |_| {
            let (m, p) = (m.clone(), p.clone());
            Box!(modifier = Modifier.fill_max_size().safe_drawing_padding(), move |outer| {
                let (m, p) = (m.clone(), p.clone());
                let approach = Modifier.approach_layout(
                    |_| false,
                    |_, _| false,
                    |scope, measurable, constraints| {
                        let size = scope.lookahead_size();
                        let c = if scope.is_looking_ahead() {
                            constraints
                        } else {
                            Constraints::fixed(size.width, size.height)
                        };
                        let placeable = scope.measure(measurable, c);
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            placement.place_relative(&placeable, 0, 0);
                        })
                    },
                );
                Box!(
                    modifier = Modifier
                        .align(outer, compose_ui::Alignment::CENTER)
                        .border(dp(3), Color::BLUE)
                        .then(approach),
                    move |middle| {
                        let (m, p) = (m.clone(), p.clone());
                        let skipping = Modifier.layout(|scope, measurable, constraints| {
                            if scope.is_looking_ahead() {
                                scope.measure(measurable, constraints);
                                scope.layout_with(0, 0, |_| {})
                            } else {
                                let placeable = scope.measure(measurable, constraints);
                                scope.layout_with(placeable.width, placeable.height, move |pl| {
                                    pl.place_relative(&placeable, 0, 0);
                                })
                            }
                        });
                        Box!(
                            modifier = Modifier
                                .align(middle, compose_ui::Alignment::CENTER)
                                .width(dp(100))
                                .height(dp(0))
                                .then(skipping)
                                .background(Color::RED),
                            move |_| {
                                let (m, p) = (m.clone(), p.clone());
                                let noting = Modifier.layout(move |scope, measurable, c| {
                                    if scope.is_looking_ahead() {
                                        m.set(true);
                                    }
                                    let placeable = scope.measure(measurable, c);
                                    let p = p.clone();
                                    scope.layout_with(
                                        placeable.width,
                                        placeable.height,
                                        move |pl| {
                                            if pl.is_looking_ahead() {
                                                p.set(true);
                                            }
                                            pl.place(&placeable, 0, 0);
                                        },
                                    )
                                });
                                Box!(modifier = noting);
                            }
                        );
                    }
                );
            });
        });
    });
    ui.wait_for_idle();
    assert!(measured.get());
    assert!(placed.get());
}

type Results = Rc<RefCell<HashMap<&'static str, i32>>>;

/// The test's `TestElement`: a layout passing its content through, noting each intrinsic asked of
/// it in the map of the pass asking.
#[derive(Clone)]
struct IntrinsicsNoted {
    ahead: Results,
    now: Results,
}

impl IntrinsicsNoted {
    fn note(&self, scope: &IntrinsicMeasureScope<'_>, name: &'static str, value: i32) -> i32 {
        let map = if scope.is_looking_ahead() { &self.ahead } else { &self.now };
        map.borrow_mut().insert(name, value);
        value
    }
}

impl ModifierNodeElement for IntrinsicsNoted {
    type Node = IntrinsicsNoted;
    fn create(&self) -> IntrinsicsNoted {
        self.clone()
    }
    fn update(&self, node: &mut IntrinsicsNoted) {
        *node = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.ahead, &other.ahead) && Rc::ptr_eq(&self.now, &other.now)
    }
}

impl ModifierNode for IntrinsicsNoted {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for IntrinsicsNoted {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, 0, 0);
        })
    }

    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        height: i32,
    ) -> i32 {
        let value = scope.min_intrinsic_width(content, height);
        self.note(scope, "minWidth", value)
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        height: i32,
    ) -> i32 {
        let value = scope.max_intrinsic_width(content, height);
        self.note(scope, "maxWidth", value)
    }

    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        width: i32,
    ) -> i32 {
        let value = scope.min_intrinsic_height(content, width);
        self.note(scope, "minHeight", value)
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        width: i32,
    ) -> i32 {
        let value = scope.max_intrinsic_height(content, width);
        self.note(scope, "maxHeight", value)
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testIsLookingAheadWithIntrinsics
/// A box in a row of its intrinsic width, its own width or height intrinsic in turn, over a layout
/// 200 by 250 ahead and 100 by 150 now: each intrinsic asked ahead hears the size ahead, and each
/// asked now the size now.
#[test]
fn intrinsics_asked_in_each_pass_hear_that_pass_s_size() {
    let (ahead, now): (Results, Results) = (Rc::default(), Rc::default());
    let iteration = mutable_state_of(0);
    let (a, n, i) = (ahead.clone(), now.clone(), iteration.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let noted = IntrinsicsNoted { ahead: a.clone(), now: n.clone() };
        in_a_scope(move |_| {
            let noted = noted.clone();
            Row!(modifier = Modifier.width(IntrinsicSize::Max), move |_| {
                let modifier = match i.get() {
                    0 => Modifier.width(IntrinsicSize::Max),
                    1 => Modifier.width(IntrinsicSize::Min),
                    2 => Modifier.height(IntrinsicSize::Max),
                    _ => Modifier.height(IntrinsicSize::Min),
                };
                let by_pass = Modifier.layout(|scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let (width, height) =
                        if scope.is_looking_ahead() { (200, 250) } else { (100, 150) };
                    scope.layout_with(width, height, move |placement| {
                        placement.place(&placeable, 0, 0);
                    })
                });
                Box!(modifier = modifier.then_custom(noted.clone()).then(by_pass), {
                    Box!(modifier = Modifier.size(dp(10)));
                });
            });
        });
    });
    for _ in 0..3 {
        ui.wait_for_idle();
        iteration.set(iteration.get() + 1);
    }
    ui.wait_for_idle();
    let (ahead, now) = (ahead.borrow(), now.borrow());
    assert_eq!(ahead.get("maxHeight"), Some(&250));
    assert_eq!(ahead.get("minHeight"), Some(&250));
    assert_eq!(ahead.get("maxWidth"), Some(&200));
    assert_eq!(ahead.get("minWidth"), Some(&200));
    assert_eq!(now.get("maxHeight"), Some(&150));
    assert_eq!(now.get("minHeight"), Some(&150));
    assert_eq!(now.get("maxWidth"), Some(&100));
    assert_eq!(now.get("minWidth"), Some(&100));
}

#[allow(dead_code)]
fn unused(_: LayoutCoordinates) {}

// ---- SubcomposeLayout cases from LookaheadScopeTest --------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, defaultIntermediateMeasurePolicyInSubcomposeLayout
/// A `SubcomposeLayout` whose own modifier is an approach (so it looks ahead for itself): ahead,
/// its slot sees its required size; now, the fixed sizes the approach measures it with across
/// iterations.
#[test]
fn subcompose_layout_with_approach_sees_its_required_size_ahead_and_approaches_now() {
    let expected = [
        IntSize::new(200, 100),
        IntSize::new(400, 300),
        IntSize::new(100, 500),
        IntSize::new(20, 5),
        IntSize::new(90, 120),
    ];
    let target = IntSize::new(260, 350);
    let actual = Rc::new(Cell::new(IntSize::ZERO));
    let actual_target = Rc::new(Cell::new(IntSize::ZERO));
    let iteration = mutable_state_of(0usize);
    let (a, at, it) = (actual.clone(), actual_target.clone(), iteration.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let (a, at) = (a.clone(), at.clone());
        let approach = Modifier.approach_layout(
            move |_| it.get() <= 6,
            |_, _| false,
            move |scope, measurable, _| {
                let size = expected[it.get()];
                let placeable =
                    scope.measure(measurable, Constraints::fixed(size.width, size.height));
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            },
        );
        let policy = subcompose_policy(move |scope, constraints| {
            let placeables =
                scope.subcompose(0, callback(|| Box!(modifier = Modifier.fill_max_size())));
            let placeable = scope.measure(placeables[0], constraints);
            let size = placeable.size();
            if scope.is_looking_ahead() {
                at.set(size);
            } else {
                a.set(size);
            }
            scope.layout_with(size.width, size.height, move |placement| {
                placement.place(&placeable, 0, 0);
            })
        });
        SubcomposeLayout!(
            modifier = Modifier.required_size((dp(target.width), dp(target.height))).then(approach),
            measure_policy = policy
        );
    });
    for step in 0..5 {
        ui.wait_for_idle();
        assert_eq!(actual_target.get(), target);
        assert_eq!(actual.get(), expected[iteration.get()]);
        if step < 4 {
            iteration.set(iteration.get() + 1);
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, defaultMeasurePolicyInSubcomposeLayout
/// An approach on a `SubcomposeLayout` in a `LookaheadScope`, measuring unconstrained now: its
/// `measure` ahead runs the `SubcomposeLayout`'s own policy and reports `(500, 300)` to the
/// approach.
#[test]
fn approach_on_subcompose_layout_sees_the_policy_s_size_ahead() {
    let seen = Rc::new(Cell::new(IntSize::ZERO));
    let s = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let s = s.clone();
        in_a_scope(move |_| {
            let s = s.clone();
            let approach = Modifier.approach_layout(
                |_| true,
                |_, _| false,
                move |scope, measurable, _| {
                    let placeable = scope.measure(measurable, Constraints::new(0, 2000, 0, 2000));
                    s.set(placeable.size());
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        placement.place(&placeable, 0, 0);
                    })
                },
            );
            let policy = subcompose_policy(|scope, constraints| {
                let placeables = scope.subcompose(
                    (),
                    callback(|| Box!(modifier = Modifier.required_size((dp(400), dp(600))))),
                );
                let placeable = scope.measure(placeables[0], constraints);
                scope.layout_with(500, 300, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            });
            SubcomposeLayout!(
                modifier = Modifier.size(dp(150)).then(approach),
                measure_policy = policy
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(seen.get(), IntSize::new(500, 300));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, subcomposeLayoutInLookahead
/// A `SubcomposeLayout` in a `LookaheadScope` subcomposing three slots (each with an unplaced
/// second child, as a guard against crashing) with distinct constraints and placements: each
/// slot's approach sees its constraints and its lookahead position in the scope.
#[test]
fn subcompose_layout_slots_in_lookahead_see_their_constraints_and_placements() {
    let expected_constraints =
        [Constraints::fixed(100, 800), Constraints::fixed(300, 400), Constraints::fixed(1000, 200)];
    let expected_placements =
        [IntOffset::new(-50, 1200), IntOffset::ZERO, IntOffset::new(800, -200)];
    let actual_constraints: Rc<RefCell<[Option<Constraints>; 3]>> = Default::default();
    let actual_placements: Rc<RefCell<[Option<IntOffset>; 3]>> = Default::default();
    let (ac, ap) = (actual_constraints.clone(), actual_placements.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let (ac, ap) = (ac.clone(), ap.clone());
        in_a_scope(move |scope| {
            let (ac, ap) = (ac.clone(), ap.clone());
            let policy = subcompose_policy(move |subcompose_scope, _| {
                let mut placeables = Vec::new();
                for (id, &expected) in expected_constraints.iter().enumerate() {
                    let (ac, ap) = (ac.clone(), ap.clone());
                    let measurables = subcompose_scope.subcompose(
                        id,
                        callback(move || {
                            let (ac, ap) = (ac.clone(), ap.clone());
                            let approach = Modifier.approach_layout(
                                |_| true,
                                |_, _| false,
                                move |approach_scope, measurable, constraints| {
                                    ac.borrow_mut()[id] = Some(constraints);
                                    let placeable = approach_scope.measure(measurable, constraints);
                                    let ap = ap.clone();
                                    approach_scope.layout_with(
                                        placeable.width,
                                        placeable.height,
                                        move |placement| {
                                            if let (Some(frame), Some(own)) = (
                                                scope.lookahead_scope_coordinates(placement),
                                                placement.coordinates(),
                                            ) {
                                                ap.borrow_mut()[id] = Some(
                                                    scope
                                                        .local_lookahead_position_of(
                                                            frame,
                                                            own,
                                                            Offset::ZERO,
                                                        )
                                                        .round(),
                                                );
                                            }
                                            placement.place(&placeable, 0, 0);
                                        },
                                    )
                                },
                            );
                            Box!(modifier = approach.fill_max_size());
                            // Intentionally left unplaced, as Kotlin's second child.
                            Box!(modifier = Modifier.size(dp(200)));
                        }),
                    );
                    placeables.push(subcompose_scope.measure(measurables[0], expected));
                }
                subcompose_scope.layout_with(100, 100, move |placement| {
                    for (id, placeable) in placeables.iter().enumerate() {
                        placement.place(
                            placeable,
                            expected_placements[id].x,
                            expected_placements[id].y,
                        );
                    }
                })
            });
            SubcomposeLayout!(measure_policy = policy);
        });
    });
    ui.wait_for_idle();
    let (ac, ap) = (actual_constraints.borrow(), actual_placements.borrow());
    for (id, (&actual_c, &actual_p)) in ac.iter().zip(ap.iter()).enumerate() {
        assert_eq!(actual_c, Some(expected_constraints[id]), "slot {id} constraints");
        assert_eq!(actual_p, Some(expected_placements[id]), "slot {id} placement");
    }
}

type SeenOffset = Rc<Cell<Option<Offset>>>;

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, forceMeasureLookaheadRootInParentsMeasurePass
/// A `LazyColumn` whose own layout modifier changes its fixed constraints while measuring,
/// wrapping a `SubcomposeLayout` whose slot holds a `LookaheadScope`: changing the outer size
/// updates both lookahead and actual positions in root of the layout inside the `LookaheadScope`.
#[test]
fn parent_measure_changing_constraints_updates_nested_lookahead_root_in_subcompose() {
    let size = mutable_state_of(200);
    let (lookahead_offset, actual_offset): (SeenOffset, SeenOffset) = Default::default();
    let (s, lo, ao) = (size.handle(), lookahead_offset.clone(), actual_offset.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let (lo, ao) = (lo.clone(), ao.clone());
        Box!(modifier = Modifier.fill_max_size(), move |_| {
            let (lo, ao) = (lo.clone(), ao.clone());
            let resize = Modifier.layout(move |measure, measurable, _| {
                let side = s.get();
                let placeable = measure.measure(measurable, Constraints::fixed(side, side));
                measure.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            });
            LazyColumn!(modifier = resize, move |list_scope: LazyListScope| {
                let (lo, ao) = (lo.clone(), ao.clone());
                list_scope.item(move || {
                    let (lo, ao) = (lo.clone(), ao.clone());
                    let policy = subcompose_policy(move |subcompose_scope, _| {
                        let (lo, ao) = (lo.clone(), ao.clone());
                        let measurables = subcompose_scope.subcompose(
                            (),
                            callback(move || {
                                let (lo, ao) = (lo.clone(), ao.clone());
                                Box!(modifier = Modifier.required_size(dp(222)), move |_| {
                                    let (lo, ao) = (lo.clone(), ao.clone());
                                    in_a_scope(move |_| {
                                        let (lo, ao) = (lo.clone(), ao.clone());
                                        Box!(
                                            modifier = Modifier.required_size(dp(200)),
                                            content_alignment = compose_ui::Alignment::TOP_START,
                                            move |_| {
                                                let (lo, ao) = (lo.clone(), ao.clone());
                                                let read = Modifier.layout(
                                                    move |measure, measurable, constraints| {
                                                        let placeable = measure
                                                            .measure(measurable, constraints);
                                                        let (lo, ao) = (lo.clone(), ao.clone());
                                                        measure.layout_with(
                                                            placeable.width,
                                                            placeable.height,
                                                            move |placement| {
                                                                if let Some(coords) =
                                                                    placement.coordinates()
                                                                {
                                                                    let pos =
                                                                        coords.position_in_root();
                                                                    if placement.is_looking_ahead()
                                                                    {
                                                                        lo.set(Some(pos));
                                                                    } else {
                                                                        ao.set(Some(pos));
                                                                    }
                                                                }
                                                                placement.place(&placeable, 0, 0);
                                                            },
                                                        )
                                                    },
                                                );
                                                Row!(
                                                    modifier = Modifier.fill_max_size().then(read),
                                                    |_| {}
                                                );
                                            }
                                        );
                                    });
                                });
                            }),
                        );
                        let placeable = subcompose_scope
                            .measure(measurables[0], Constraints::new(0, 2000, 0, 2000));
                        subcompose_scope.layout_with(700, 800, move |placement| {
                            placement.place(&placeable, 0, 0);
                        })
                    });
                    SubcomposeLayout!(modifier = Modifier.fill_max_size(), measure_policy = policy);
                });
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(lookahead_offset.get(), Some(Offset::new(-250.0, 0.0)));
    assert_eq!(actual_offset.get(), Some(Offset::new(-250.0, 0.0)));
    size.set(400);
    ui.wait_for_idle();
    assert_eq!(lookahead_offset.get(), Some(Offset::new(-150.0, 0.0)));
    assert_eq!(actual_offset.get(), Some(Offset::new(-150.0, 0.0)));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, lookaheadRecomposesLayoutOnStaticLocalChange
/// A `SubcomposeLayout` in a `LookaheadScope` whose slot reads a static composition local:
/// changing the provider's value recomposes the subcomposition in lookahead.
#[test]
fn static_local_change_recomposes_subcompose_slot_in_lookahead() {
    static LOCAL: CompositionLocal<i32> =
        CompositionLocal::new_static("LookaheadStaticLocal", || -1);
    let current = Rc::new(Cell::new(-1));
    let value = mutable_state_of(0);
    let (c, v) = (current.clone(), value.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let c = c.clone();
        in_a_scope(move |_| {
            let c = c.clone();
            CompositionLocalProvider(LOCAL.provides(v.get()), move || {
                let c = c.clone();
                let policy = subcompose_policy(move |scope, constraints| {
                    let c = c.clone();
                    let measurables = scope.subcompose(
                        (),
                        callback(move || {
                            c.set(LOCAL.current());
                            Box!(modifier = Modifier.size(dp(100)));
                        }),
                    );
                    scope.measure(measurables[0], constraints);
                    scope.layout_with(0, 0, |_| {})
                });
                SubcomposeLayout!(measure_policy = policy);
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(current.get(), 0);
    value.set(1);
    ui.wait_for_idle();
    assert_eq!(current.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, onlyApproachRecomposesLayoutOnStaticLocalChange
/// A `SubcomposeLayout` in a `LookaheadScope` that subcomposes only when not looking ahead:
/// changing a static composition local above it recomposes that approach-only slot.
#[test]
fn static_local_change_recomposes_approach_only_subcompose_slot() {
    static LOCAL: CompositionLocal<i32> =
        CompositionLocal::new_static("ApproachOnlyStaticLocal", || -1);
    let current = Rc::new(Cell::new(-1));
    let value = mutable_state_of(0);
    let (c, v) = (current.clone(), value.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 1920), move || {
        let c = c.clone();
        in_a_scope(move |_| {
            let c = c.clone();
            CompositionLocalProvider(LOCAL.provides(v.get()), move || {
                let c = c.clone();
                let policy = subcompose_policy(move |scope, constraints| {
                    let c = c.clone();
                    if !scope.is_looking_ahead() {
                        let measurables = scope.subcompose(
                            (),
                            callback(move || {
                                c.set(LOCAL.current());
                                Box!(modifier = Modifier.size(dp(100)));
                            }),
                        );
                        scope.measure(measurables[0], constraints);
                    }
                    scope.layout_with(0, 0, |_| {})
                });
                SubcomposeLayout!(measure_policy = policy);
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(current.get(), 0);
    value.set(1);
    ui.wait_for_idle();
    assert_eq!(current.get(), 1);
}
