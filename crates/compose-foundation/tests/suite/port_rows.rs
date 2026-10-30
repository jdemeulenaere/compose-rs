//! The small files of the layout and gesture ledger, rewritten from their scenarios: Spacer's sizes,
//! `Arrangement`'s table, the hover under a scrolled list, a drag moving content to a popup, and a
//! focused item scrolled up and down.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Density, Dp, DragGestures, FocusRequester, IntSize, LayoutDirection, Offset, PointerEventPass,
    PointerEventType, Popup, UnitExt,
};

/// A `Spacer` of `modifier` in a box of at most `room` and what it measured.
fn spacer_in(room: (Dp, Dp), modifier: Modifier) -> IntSize {
    let size = Rc::new(Cell::new(IntSize::ZERO));
    let noted = size.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let noted = noted.clone();
        let modifier = modifier.clone();
        Box!(modifier = Modifier.width_in(Dp(0.0), room.0).height_in(Dp(0.0), room.1), {
            let noted = noted.clone();
            Spacer(modifier.clone().on_size_changed(move |measured: IntSize| noted.set(measured)));
        });
    });
    ui.wait_for_idle();
    size.get()
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/SpacerTest.kt, fixedSpacer_Sizes, fixedSpacer_Sizes_WithSmallerContainer, widthSpacer_Sizes, widthSpacer_Sizes_WithSmallerContainer, heightSpacer_Sizes, heightSpacer_Sizes_WithSmallerContainer
#[test]
fn a_spacer_is_its_size_as_far_as_its_container_allows() {
    let big = (500.dp(), 500.dp());
    let small = (5.dp(), 7.dp());
    let cases = [
        (big, Modifier.size((40.dp(), 71.dp())), IntSize::new(40, 71)),
        (small, Modifier.size((40.dp(), 71.dp())), IntSize::new(5, 7)),
        (big, Modifier.width(71.dp()), IntSize::new(71, 0)),
        (small, Modifier.width(71.dp()), IntSize::new(5, 0)),
        (big, Modifier.height(71.dp()), IntSize::new(0, 71)),
        (small, Modifier.height(71.dp()), IntSize::new(0, 7)),
    ];
    for (room, modifier, expected) in cases {
        assert_eq!(spacer_in(room, modifier.clone()), expected, "{modifier:?} in {room:?}");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/ArrangementTest.kt, testArrangement
#[test]
fn space_between_puts_one_item_at_the_start_and_two_at_each_end_either_way() {
    let density = Density::new(1.0);
    let cases: [(&[i32], LayoutDirection, &[i32]); 4] = [
        (&[108, 24], LayoutDirection::Rtl, &[2268, 0]),
        (&[108], LayoutDirection::Rtl, &[2268]),
        (&[108, 24], LayoutDirection::Ltr, &[0, 2352]),
        (&[108], LayoutDirection::Ltr, &[0]),
    ];
    for (sizes, direction, expected) in cases {
        let mut positions = vec![0; sizes.len()];
        HorizontalArrangement::from(Arrangement::SPACE_BETWEEN).arrange(
            &density,
            2376,
            sizes,
            direction,
            &mut positions,
        );
        assert_eq!(positions, expected, "{sizes:?} {direction:?}");
    }
}

/// A long list of 40 px items, which tells who's hovered, the mouse resting on its first.
fn hovered_list() -> (ComposeTestRule, Rc<RefCell<Vec<usize>>>) {
    let hovered: Rc<RefCell<Vec<usize>>> = Rc::default();
    let noted = hovered.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let noted = noted.clone();
        LazyColumn!(
            modifier = Modifier.fill_max_size().test_tag("list"),
            move |scope: LazyListScope| {
                let noted = noted.clone();
                scope.items(500, move |index| {
                    let noted = noted.clone();
                    Box!(
                        modifier = Modifier
                            .fill_max_width()
                            .height(40.dp())
                            .test_tag(format!("item {index}"))
                            .pointer_input((), move |scope| {
                                let noted = noted.clone();
                                async move {
                                    loop {
                                        let change =
                                            scope.await_pointer_event(PointerEventPass::Main).await;
                                        match change.kind {
                                            PointerEventType::Enter => {
                                                noted.borrow_mut().push(index)
                                            }
                                            PointerEventType::Exit => {
                                                noted.borrow_mut().retain(|item| *item != index)
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            })
                    );
                });
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("list").perform_mouse_input(|mouse| mouse.enter(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    assert_eq!(*hovered.borrow(), [0]);
    (ui, hovered)
}

fn turn(ui: &mut ComposeTestRule, amount: f32) {
    ui.on_node_with_tag("list")
        .perform_mouse_input(|mouse| mouse.scroll(amount, ScrollWheel::Vertical));
    ui.wait_for_idle();
}

/// The item under the mouse, by what's laid out.
fn item_under_mouse(ui: &mut ComposeTestRule, state_first: usize) -> usize {
    (state_first..state_first + 10)
        .find(|index| {
            let tag = format!("item {index}");
            let bounds = ui.on_node_with_tag(&tag).get_unclipped_bounds_in_root();
            bounds.top.0 <= 10.0 && 10.0 < bounds.bottom.0
        })
        .expect("an item under the mouse")
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnMouseHoverTest.kt, hoverStateLocationAfterSingleVerySmallScrollDown_changeInHoverState, hoverStateLocationAfterSingleSmallScrollDown_changeInHoverState, hoverStateLocationAfterMultipleSmallScrollsDown_changeInHoverState
#[test]
fn the_hover_follows_a_list_scrolled_under_a_resting_mouse() {
    for (amount, turns) in [(10.0, 1), (100.0, 1), (100.0, 10)] {
        let (mut ui, hovered) = hovered_list();
        for _ in 0..turns {
            turn(&mut ui, amount);
        }
        let first = (0..500)
            .find(|index| {
                !ui.on_all_nodes_with_tag(&format!("item {index}"))
                    .fetch_semantics_nodes()
                    .is_empty()
            })
            .expect("an item laid out");
        let under = item_under_mouse(&mut ui, first);
        assert_ne!(under, 0, "{amount} {turns} times");
        assert_eq!(*hovered.borrow(), [under], "{amount} {turns} times");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnMouseHoverTest.kt, hoverStateLocationAfterSingleVerySmallScrollUp_noChangeInHoverState
#[test]
fn a_list_at_its_start_scrolled_further_back_keeps_the_hover() {
    let (mut ui, hovered) = hovered_list();
    turn(&mut ui, -10.0);
    assert_eq!(*hovered.borrow(), [0]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/DragGestureDetectorWhileMovingUIToPopupTest.kt, dragGesture_dragStartMovesComposeContentToPopup_shouldNotCrash (composed anew there: compose-rs has no `movableContentOf`)
#[test]
fn a_drag_start_moving_its_content_to_a_popup_goes_on() {
    #[derive(Default)]
    struct Counts {
        start: Cell<u32>,
        cancel: Cell<u32>,
        drag: Cell<u32>,
        end: Cell<u32>,
        in_popup: Cell<bool>,
    }
    let counts: Rc<Counts> = Rc::default();
    let given = counts.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let counts = given.clone();
        let shown = remember(|| mutable_state_of(false));
        let gestures = counts.clone();
        Box!(
            modifier = Modifier.fill_max_size().pointer_input((), move |scope| {
                let (start, cancel, drag, end) =
                    (gestures.clone(), gestures.clone(), gestures.clone(), gestures.clone());
                async move {
                    scope
                        .detect_drag_gestures(
                            DragGestures::default()
                                .on_drag_start(move |_| {
                                    start.start.set(start.start.get() + 1);
                                    shown.set(true);
                                })
                                .on_drag_cancel(move || {
                                    cancel.cancel.set(cancel.cancel.get() + 1);
                                    shown.set(false);
                                })
                                .on_drag(move |_, _| drag.drag.set(drag.drag.get() + 1))
                                .on_drag_end(move || {
                                    end.end.set(end.end.get() + 1);
                                    shown.set(false);
                                }),
                        )
                        .await;
                }
            }),
            {
                if shown.get() {
                    counts.in_popup.set(true);
                    Popup!(|| Box!(modifier = Modifier.size(200.dp())));
                } else {
                    Box!(modifier = Modifier.size(200.dp()));
                }
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(counts.start.get(), 0);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::ZERO);
        touch.move_by(Offset::new(0.0, 50.0));
    });
    ui.wait_for_idle();
    assert_eq!((counts.start.get(), counts.drag.get(), counts.cancel.get()), (1, 1, 0));
    assert!(counts.in_popup.get());
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert_eq!((counts.drag.get(), counts.cancel.get(), counts.end.get()), (1, 0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/focus/LazyListFocusableInteractionTest.kt, scrollingUpAndDownLazyColumnDoesntCrash (a focusable item here: the test's is an AndroidView)
#[test]
fn a_list_flung_back_and_forth_over_a_focused_item_goes_on() {
    let requester = FocusRequester::new();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        LazyColumn!(
            modifier = Modifier.fill_max_size().test_tag("list"),
            move |scope: LazyListScope| {
                scope.items(50, |_| Box!(modifier = Modifier.fill_max_width().height(30.dp())));
                scope.item(move || {
                    Box!(
                        modifier = Modifier
                            .fill_max_width()
                            .height(30.dp())
                            .focus_requester(requester)
                            .focusable()
                    );
                    LaunchedEffect((), async move {
                        requester.request_focus();
                    });
                });
                scope.items(10, |_| Box!(modifier = Modifier.fill_max_width().height(30.dp())));
            }
        );
    });
    ui.wait_for_idle();
    let (center, up) = (Offset::new(150.0, 150.0), Offset::new(150.0, -50.0));
    for (from, to) in [(center, up), (up, center)] {
        for _ in 0..20 {
            ui.on_node_with_tag("list").perform_touch_input(|touch| {
                touch.swipe_with_velocity(from, to, 600.0, Some(100));
            });
            ui.wait_for_idle();
        }
    }
}
