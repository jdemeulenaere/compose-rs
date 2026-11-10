//! Compose's `LookaheadScopeTest` cases on lookahead coordinates with foundation's layouts in
//! them: what a motion frame of reference placed (a scroll) left out of a lookahead position,
//! `transform_from` the scope's coordinates, the scope's root placed by its parent's alignment,
//! a lazy list's animated items and a pager in a scope. Density 1.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::gestures::SnapPosition;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, Color, IntSize, LayoutCoordinates, LookaheadScope, Matrix, Offset, UnitExt,
};

use crate::scroll_support::run;

fn dp(pixels: i32) -> compose_ui::Dp {
    pixels.dp()
}

fn in_a_scope(content: impl Fn(LookaheadScope) + 'static) {
    LookaheadScope(callback(content));
}

/// A position's whole pixels, so a `-0.0` reads as 0, as the tests' `fastRoundToInt`.
fn rounded(offset: Offset) -> (i32, i32) {
    (offset.x.round() as i32, offset.y.round() as i32)
}

/// Each of three items' lookahead position, then with a motion frame of reference left out.
type Positions = Rc<RefCell<[(Offset, Offset); 3]>>;

fn unspecified() -> Positions {
    let unspecified = Offset::new(f32::NAN, f32::NAN);
    Rc::new(RefCell::new([(unspecified, unspecified); 3]))
}

/// Scrolls `state` to `value` and lets what that changes lay out.
fn scroll_to(ui: &mut ComposeTestRule, state: ScrollState, value: i32) {
    run(ui, async move {
        let _ = state.scroll_to(value).await;
    });
    ui.wait_for_idle();
}

/// The three items' positions, before and after scrolling to the last one, as both
/// `testDirectManipulationCoordinates_inScroll` tests check them.
fn assert_a_scroll_is_left_out(ui: &mut ComposeTestRule, state: ScrollState, seen: &Positions) {
    ui.wait_for_idle();
    for (index, (position, excluded)) in seen.borrow().iter().enumerate() {
        let expected = index as i32 * 150;
        assert_eq!(rounded(*position).1, expected, "at index {index}");
        assert_eq!(rounded(*excluded).1, expected, "at index {index}");
    }
    scroll_to(ui, state, 2 * 150);
    for (index, (position, excluded)) in seen.borrow().iter().enumerate() {
        assert_eq!(rounded(*position).1, (index as i32 - 2) * 150, "scrolled, at index {index}");
        assert_eq!(
            rounded(*excluded).1,
            index as i32 * 150,
            "scrolled, excluded, at index {index}"
        );
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testDirectManipulationCoordinates_inScroll
#[test]
fn a_scroll_in_a_scope_is_left_out_of_lookahead_positions_if_asked() {
    let owner = ScrollState::new(0);
    let state = *owner;
    let seen = unspecified();
    let s = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let s = s.clone();
        in_a_scope(move |lookahead| {
            let s = s.clone();
            // One item shows; the scroll goes up to the last.
            Column!(modifier = Modifier.size(dp(150)).vertical_scroll(state), move |_| {
                for index in 0..3 {
                    let s = s.clone();
                    let noting = Modifier.layout(move |scope, measurable, constraints| {
                        let placeable = scope.measure(measurable, constraints);
                        let s = s.clone();
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            if placement.is_looking_ahead()
                                && let Some(own) = placement.coordinates()
                            {
                                let parent = own
                                    .parent_layout_coordinates()
                                    .and_then(|it| it.parent_coordinates())
                                    .expect("the column's scroll");
                                let at = |include| {
                                    lookahead.local_lookahead_position_of_with(
                                        parent,
                                        own,
                                        Offset::ZERO,
                                        include,
                                    )
                                };
                                s.borrow_mut()[index] = (at(true), at(false));
                            }
                            placement.place(&placeable, 0, 0);
                        })
                    });
                    Box!(modifier = Modifier.size(dp(150)).then(noting), {});
                }
            });
        });
    });
    assert_a_scroll_is_left_out(&mut ui, state, &seen);
    drop(owner);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testDirectManipulationCoordinates_inScroll_LookaheadChild
#[test]
fn a_scroll_around_a_scope_is_left_out_of_lookahead_positions_if_asked() {
    let owner = ScrollState::new(0);
    let state = *owner;
    let seen = unspecified();
    let s = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let s = s.clone();
        Column!(modifier = Modifier.size(dp(150)).vertical_scroll(state), move |_| {
            let s = s.clone();
            in_a_scope(move |lookahead| {
                for index in 0..3 {
                    let s = s.clone();
                    let noting = Modifier.layout(move |scope, measurable, constraints| {
                        let placeable = scope.measure(measurable, constraints);
                        let s = s.clone();
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            placement.place(&placeable, 0, 0);
                            if !placement.is_looking_ahead()
                                && let Some(own) = placement.coordinates()
                            {
                                let root = own.find_root_coordinates();
                                let at = |include| {
                                    lookahead.local_lookahead_position_of_with(
                                        root,
                                        own,
                                        Offset::ZERO,
                                        include,
                                    )
                                };
                                s.borrow_mut()[index] = (at(true), at(false));
                            }
                        })
                    });
                    Box!(modifier = Modifier.size(dp(150)).then(noting), {});
                }
            });
        });
    });
    assert_a_scroll_is_left_out(&mut ui, state, &seen);
    drop(owner);
}

/// Places its content `offset` down, in a motion frame of reference if `motion`, as the test's
/// `verticalOffset`.
fn vertical_offset(offset: i32, motion: bool) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if motion {
                placement.with_motion_frame_of_reference_placement(|placement| {
                    placement.place(&placeable, 0, offset);
                });
            } else {
                placement.place(&placeable, 0, offset);
            }
        })
    })
}

/// Notes, when placed, its lookahead position in its parent's content, then with a motion frame
/// of reference left out.
fn noting_position_in_parent(
    lookahead: LookaheadScope,
    seen: impl Fn((Offset, Offset)) + 'static,
) -> Modifier {
    Modifier.on_placed(move |it: LayoutCoordinates| {
        let parent = it.parent_layout_coordinates().expect("a parent");
        let at =
            |include| lookahead.local_lookahead_position_of_with(parent, it, Offset::ZERO, include);
        seen((at(true), at(false)));
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testDirectManipulationCoordinates_usingModifierLayout
#[test]
fn a_layout_modifier_s_motion_frame_of_reference_is_left_out_if_asked() {
    let motion = mutable_state_of(true);
    let seen = Rc::new(Cell::new((Offset::ZERO, Offset::ZERO)));
    let (m, s) = (motion.handle(), seen.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let s = s.clone();
        in_a_scope(move |lookahead| {
            let s = s.clone();
            Box!({
                let s = s.clone();
                let modifier = Modifier
                    .width(dp(100))
                    .height(dp(100))
                    .then(vertical_offset(300, m.get()))
                    .then(noting_position_in_parent(lookahead, move |it| s.set(it)));
                Box!(modifier = modifier, {});
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!((seen.get().0.y, seen.get().1.y), (300.0, 0.0));
    motion.set(false);
    ui.wait_for_idle();
    assert_eq!((seen.get().0.y, seen.get().1.y), (300.0, 300.0));
}

/// What the test's layout places a child by: down by `offset`, in a motion frame of reference if
/// `motion`.
#[derive(Clone, Copy, PartialEq, Debug)]
struct OffsetData {
    offset: i32,
    motion: bool,
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testDirectManipulationCoordinates_usingMeasurePolicy
#[test]
fn a_measure_policy_s_motion_frame_of_reference_is_left_out_if_asked() {
    let motion = mutable_state_of(false);
    let seen = Rc::new(RefCell::new([(Offset::ZERO, Offset::ZERO); 2]));
    let (m, s) = (motion.handle(), seen.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let s = s.clone();
        in_a_scope(move |lookahead| {
            let s = s.clone();
            Column!(move |_| {
                let s = s.clone();
                let by_data = compose_ui::measure_policy(|scope, measurables, constraints| {
                    let placed: Vec<_> = measurables
                        .iter()
                        .map(|&measurable| {
                            let data = scope
                                .layout_id(measurable)
                                .and_then(|it| it.downcast_ref::<OffsetData>().copied())
                                .expect("each child has its data");
                            (scope.measure(measurable, constraints), data)
                        })
                        .collect();
                    scope.layout_with(300, 300, move |placement| {
                        for (placeable, data) in &placed {
                            if data.motion {
                                placement.with_motion_frame_of_reference_placement(|placement| {
                                    placement.place(placeable, 0, data.offset);
                                });
                            } else {
                                placement.place(placeable, 0, data.offset);
                            }
                        }
                    })
                });
                compose_ui::Layout(
                    Modifier.fill_max_size(),
                    by_data,
                    callback(move || {
                        let motion = m.get();
                        for (index, (offset, motion)) in
                            [(100, motion), (200, !motion)].into_iter().enumerate()
                        {
                            let s = s.clone();
                            let modifier = Modifier.layout_id(OffsetData { offset, motion }).then(
                                noting_position_in_parent(lookahead, move |it| {
                                    s.borrow_mut()[index] = it;
                                }),
                            );
                            Box!(modifier = modifier, {});
                        }
                    }),
                );
            });
        });
    });
    ui.wait_for_idle();
    let ys = |seen: &Rc<RefCell<[(Offset, Offset); 2]>>| {
        let seen = seen.borrow();
        [(seen[0].0.y, seen[0].1.y), (seen[1].0.y, seen[1].1.y)]
    };
    // The first isn't placed in a motion frame of reference, the second is.
    assert_eq!(ys(&seen), [(100.0, 100.0), (200.0, 0.0)]);
    motion.set(true);
    ui.wait_for_idle();
    assert_eq!(ys(&seen), [(100.0, 0.0), (200.0, 200.0)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testDirectManipulationCoordinates_duringPlacement
#[test]
fn a_motion_frame_of_reference_is_left_out_of_a_position_asked_while_placing_ahead() {
    let motion = mutable_state_of(false);
    let seen = Rc::new(Cell::new((Offset::ZERO, Offset::ZERO)));
    let (m, s) = (motion.handle(), seen.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let s = s.clone();
        in_a_scope(move |lookahead| {
            let s = s.clone();
            Box!({
                let s = s.clone();
                let asking = Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let s = s.clone();
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        if placement.is_looking_ahead()
                            && let (Some(frame), Some(own)) = (
                                lookahead.lookahead_scope_coordinates(placement),
                                placement.coordinates(),
                            )
                        {
                            let own = lookahead.to_lookahead_coordinates(own).coordinates();
                            let at = |include| {
                                lookahead.local_lookahead_position_of_with(
                                    frame,
                                    own,
                                    Offset::ZERO,
                                    include,
                                )
                            };
                            s.set((at(true), at(false)));
                        }
                        placement.place(&placeable, 0, 0);
                    })
                });
                let modifier =
                    Modifier.size(dp(100)).then(vertical_offset(200, m.get())).then(asking);
                Box!(modifier = modifier, {});
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!((seen.get().0.y, seen.get().1.y), (200.0, 200.0));
    motion.set(true);
    ui.wait_for_idle();
    assert_eq!((seen.get().0.y, rounded(seen.get().1).1), (200.0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, lookaheadLayoutTransformFrom
#[test]
fn an_approach_s_transform_from_the_scope_s_coordinates_takes_out_where_it_is() {
    let matrix = Rc::new(RefCell::new(Matrix::new()));
    let mx = matrix.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let mx = mx.clone();
        in_a_scope(move |lookahead| {
            let mx = mx.clone();
            let at_ten = compose_ui::measure_policy(|scope, measurables, constraints| {
                let placeable = scope.measure(measurables[0], constraints);
                scope.layout_with(placeable.width + 10, placeable.height + 10, move |placement| {
                    placement.place(&placeable, 10, 10);
                })
            });
            compose_ui::Layout(
                Modifier,
                at_ten,
                callback(move || {
                    let mx = mx.clone();
                    // The test's `intermediateLayout`: an approach never over, not placing what's
                    // in it.
                    let intermediate = Modifier.approach_layout(
                        |_| true,
                        |_, _| false,
                        move |scope, content, constraints| {
                            let placeable = scope.measure(content, constraints);
                            let mx = mx.clone();
                            scope.layout_with(placeable.width, placeable.height, move |placement| {
                                let frame = lookahead.lookahead_scope_coordinates(placement);
                                if let (Some(own), Some(frame)) = (placement.coordinates(), frame) {
                                    own.transform_from(&frame, &mut mx.borrow_mut());
                                }
                            })
                        },
                    );
                    Box!(modifier = intermediate.size(dp(10)), {});
                }),
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(matrix.borrow().map(Offset::new(10.0, 10.0)), Offset::ZERO);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testLookaheadAndApproachCoordinatesAreConsistentOnFirstPass_usingAlign
/// The scope's root is placed by its parent's alignment. Compose's aligns with a lambda giving
/// (0, 100); compose-rs's alignments are biases, so the bias that puts it there.
#[test]
fn both_passes_see_the_same_lookahead_position_of_an_aligned_scope_at_once() {
    type Seen = Rc<Cell<Option<Offset>>>;
    let (ahead, now): (Seen, Seen) = (Rc::default(), Rc::default());
    let (a, n) = (ahead.clone(), now.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (a, n) = (a.clone(), n.clone());
        Box!(modifier = Modifier.size(dp(300)), move |scope| {
            let (a, n) = (a.clone(), n.clone());
            in_a_scope(move |lookahead| {
                let (a, n) = (a.clone(), n.clone());
                // (300 - 0) / 2 * (1 - 1/3) is 100 down.
                let aligned =
                    Modifier.align(scope, Alignment::new(-1.0, -1.0 / 3.0)).fill_max_width();
                Box!(modifier = aligned, {
                    let (a, n) = (a.clone(), n.clone());
                    let on_lookahead = Modifier.layout(move |scope, measurable, constraints| {
                        let placeable = scope.measure(measurable, constraints);
                        let a = a.clone();
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            if placement.is_looking_ahead()
                                && let (Some(frame), Some(own)) = (
                                    lookahead.lookahead_scope_coordinates(placement),
                                    placement.coordinates(),
                                )
                            {
                                a.set(Some(frame.local_position_of(&own, Offset::ZERO)));
                            }
                            placement.place(&placeable, 0, 0);
                        })
                    });
                    let on_approach = Modifier.approach_layout(
                        |_| false,
                        move |placement, coordinates| {
                            if let Some(frame) = lookahead.lookahead_scope_coordinates(placement) {
                                n.set(Some(
                                    lookahead
                                        .to_lookahead_coordinates(frame)
                                        .local_position_of(coordinates, Offset::ZERO),
                                ));
                            }
                            false
                        },
                        |scope, content, constraints| {
                            let placeable = scope.measure(content, constraints);
                            scope.layout_with(placeable.width, placeable.height, move |placement| {
                                placement.place(&placeable, 0, 0);
                            })
                        },
                    );
                    Box!(modifier = on_lookahead.then(on_approach), {});
                });
            });
        });
    });
    ui.wait_for_idle();
    let expected = Some((0, 100));
    assert_eq!((ahead.get().map(rounded), now.get().map(rounded)), (expected, expected));
}

/// What the test's list shows: headers, each followed, when expanded, by its items, then a
/// divider.
#[derive(Clone, PartialEq, Debug)]
enum Element {
    Header(String),
    Divider(String),
    Item { header: String, text: String, checked: bool },
}

impl Element {
    fn id(&self) -> String {
        match self {
            Element::Header(title) => title.clone(),
            Element::Divider(id) => id.clone(),
            Element::Item { header, text, .. } => format!("{header}-{text}"),
        }
    }

    fn content_type(&self) -> u8 {
        match self {
            Element::Header(_) => 0,
            Element::Divider(_) => 1,
            Element::Item { .. } => 2,
        }
    }
}

/// The test's `generateItems`. Its checkmarks are random; here every third item has one.
fn elements(expanded: &[String]) -> Vec<Element> {
    let mut elements = Vec::new();
    for header in 0..5 {
        let header = header.to_string();
        elements.push(Element::Header(header.clone()));
        if expanded.contains(&header) {
            for item in 0..100 {
                let (header, text) = (header.clone(), item.to_string());
                elements.push(Element::Item { header, text, checked: item % 3 == 0 });
            }
        }
        elements.push(Element::Divider(format!("{header}-divider")));
    }
    elements
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testPlacementInLazyWithAnimatedItem
/// Compose's `Scaffold` is a top bar over the list, padded by its height; `Text` is
/// `BasicText`, the checkmark's `Icon` a box of its size.
#[test]
fn a_lazy_list_s_animated_items_expanding_and_collapsing_in_a_scope_are_placed() {
    let expanded = mutable_state_of(Vec::<String>::new());
    let items = mutable_state_of(elements(&[]));
    let (ex, it) = (expanded.handle(), items.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), move || {
        in_a_scope(move |_| {
            Box!(modifier = Modifier.fill_max_size(), {
                LazyColumn!(
                    modifier = Modifier.padding((dp(0), dp(100), dp(0), dp(0))),
                    content = move |scope: LazyListScope| {
                        let item_scope = scope.item_scope();
                        let list = it.get();
                        let (keys, types) = (list.clone(), list.clone());
                        scope.items_keyed_with(
                            list.len(),
                            move |index| keys[index].id(),
                            move |index| types[index].content_type(),
                            move |index| match list[index].clone() {
                                Element::Header(title) => {
                                    let toggle = title.clone();
                                    let toggled = Modifier.clickable(move || {
                                        let mut headers = ex.get();
                                        if headers.contains(&toggle) {
                                            headers.retain(|it| *it != toggle);
                                        } else {
                                            headers.push(toggle.clone());
                                        }
                                        it.set(elements(&headers));
                                        ex.set(headers);
                                    });
                                    let modifier = Modifier
                                        .animate_item(&item_scope)
                                        .test_tag(title.clone())
                                        .then(toggled);
                                    Box!(modifier = modifier, {
                                        BasicText!(format!("Header {title}"));
                                    });
                                }
                                Element::Item { text, checked, .. } => {
                                    let row = Modifier.animate_item(&item_scope).padding(dp(16));
                                    Row!(modifier = row, move |_| {
                                        let circle = Modifier
                                            .padding(dp(16))
                                            .height(dp(48))
                                            .aspect_ratio_with(1.0, true)
                                            .background_with(Color::RED, CIRCLE_SHAPE);
                                        Box!(modifier = circle, move |scope| {
                                            let centered = Modifier.align(scope, Alignment::CENTER);
                                            BasicText!("A", modifier = centered);
                                            if checked {
                                                let corner = Modifier
                                                    .align(scope, Alignment::BOTTOM_END)
                                                    .clip(CIRCLE_SHAPE);
                                                Box!(modifier = corner, {
                                                    Box!(modifier = Modifier.size(dp(16)), {});
                                                });
                                            }
                                        });
                                        BasicText!(text.clone());
                                    });
                                }
                                Element::Divider(_) => {
                                    let divider = Modifier
                                        .animate_item(&item_scope)
                                        .fill_max_width()
                                        .height(dp(5));
                                    Box!(modifier = divider, {});
                                }
                            },
                        );
                    }
                );
                Box!(modifier = Modifier.fill_max_width().height(dp(100)), {});
            });
        });
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    // A header clicked, how many are expanded after, and the time let pass.
    type Step = (&'static str, fn(usize) -> bool, u64);
    let steps: [Step; 4] = [
        ("1", |count| count > 0, 200),
        ("0", |count| count > 1, 300),
        ("0", |count| count < 2, 400),
        ("1", |count| count < 1, 400),
    ];
    for (tag, until, millis) in steps {
        ui.on_node_with_tag(tag).perform_click();
        let ex = expanded.handle();
        ui.wait_until(move |_| until(ex.get().len()));
        ui.advance_time_by(std::time::Duration::from_millis(millis));
    }
    assert!(expanded.get().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LookaheadScopeTest.kt, testDifferentPlacementsInTwoPasses
/// Compose's reads `positionInParent()`, its page being a direct child of the pager; here `LazyItem`
/// and `PageLayout` wrap each page (D290, D1467), so its position in the root (where the pager sits)
/// is its position in the pager.
#[test]
fn a_pager_s_pages_are_where_they_were_ahead_after_a_scroll_in_a_scope() {
    const PAGE: i32 = 50;
    let owner = PagerState::new(0, 0.0, || 4);
    let state = *owner;
    type Seen = Rc<RefCell<[Option<Offset>; 4]>>;
    let (ahead, now): (Seen, Seen) = (Rc::default(), Rc::default());
    let (a, n) = (ahead.clone(), now.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (a, n) = (a.clone(), n.clone());
        in_a_scope(move |_| {
            let (a, n) = (a.clone(), n.clone());
            Box!(modifier = Modifier.size(dp(PAGE * 3 / 2)), {
                let (a, n) = (a.clone(), n.clone());
                HorizontalPager!(
                    state,
                    content_padding = (dp(PAGE * 2), dp(0)).into(),
                    page_size = PageSize::Fixed(dp(PAGE)),
                    snap_position = SnapPosition::Start,
                    page_content = move |_: PagerScope, page: usize| {
                        let (a, n) = (a.clone(), n.clone());
                        // Notes where it is in each pass and places nothing, as the test's.
                        let noting = Modifier.layout(move |scope, measurable, constraints| {
                            let placeable = scope.measure(measurable, constraints);
                            let (a, n) = (a.clone(), n.clone());
                            scope.layout_with(placeable.width, placeable.height, move |placement| {
                                let at = placement.coordinates().map(|it| it.position_in_root());
                                if placement.is_looking_ahead() {
                                    let mut ahead = a.borrow_mut();
                                    ahead[page] = at.or(ahead[page]);
                                } else {
                                    n.borrow_mut()[page] = at.or(a.borrow()[page]);
                                }
                            })
                        });
                        let modifier = noting.background(Color::RED).size(dp(PAGE));
                        Box!(modifier = modifier, {});
                    }
                );
            });
            LaunchedEffect((), async move {
                // The container, the padding before the pages, and the pages.
                let all = PAGE * 3 / 2 + PAGE * 2 + PAGE * 3;
                let _ = state.animate_scroll_by(all as f32, compose_animation::snap(0)).await;
            });
        });
    });
    ui.wait_for_idle();
    // Not in Compose's: that the first and last pages were placed where expected, so the comparison
    // isn't of zeros or nothing.
    assert_eq!(ahead.borrow()[0], Some(Offset::new(100.0, 0.0)));
    assert_eq!(ahead.borrow()[3], Some(Offset::new(-75.0, 0.0)));
    assert_eq!(*now.borrow(), *ahead.borrow());
    drop(owner);
}
