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
use compose_ui::{
    Color, Constraints, IntSize, IntrinsicMeasurable, IntrinsicMeasureScope, IntrinsicSize,
    LayoutCoordinates, LayoutModifierNode, LookaheadScope, Measurable, MeasureResult, MeasureScope,
    ModifierNode, ModifierNodeElement, Offset, UnitExt,
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
