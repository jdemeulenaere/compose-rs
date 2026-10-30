//! A scroll under a motion frame of reference, as `PlacementScopeMotionFrameOfReferenceTest`: what
//! follows the scroll isn't a move of its own for what asks to leave it out. Density 1.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Dp, IntSize, LayoutCoordinates, Modifier, Offset, UnitExt};

use crate::scroll_support::run;

type Kept = Rc<RefCell<Vec<Option<LayoutCoordinates>>>>;

/// A box moved by `offset` keeping its coordinates in `root`, with `content` in it.
fn around(root: Rc<Cell<Option<LayoutCoordinates>>>, offset: MutableState<(i32, i32)>) -> Modifier {
    Modifier
        .layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let root = root.clone();
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                root.set(placement.coordinates());
                placement.place(&placeable, 0, 0);
            })
        })
        .layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let (x, y) = offset.get();
            scope.place(&placeable, x, y);
            scope.layout(placeable.width, placeable.height)
        })
}

fn kept(item: usize, into: &Kept) -> Modifier {
    let into = into.clone();
    Modifier.size(20.dp()).on_globally_positioned(move |coordinates: LayoutCoordinates| {
        into.borrow_mut()[item] = Some(coordinates);
    })
}

fn at(offset: (i32, i32), extra: i32) -> Offset {
    Offset::new(offset.0 as f32, (offset.1 + extra) as f32)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementScopeMotionFrameOfReferenceTest.kt, testLazyList
#[test]
fn a_list_s_items_are_under_a_motion_frame() {
    use_optimizations(Optimizations::NONE);
    let offset = mutable_state_of((0, 0));
    let coordinates: Kept = Rc::new(RefCell::new(vec![None; 30]));
    let root: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let state = LazyListState::new(0, 0);
    let (handle, o, c, r) = (*state, offset.handle(), coordinates.clone(), root.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let c = c.clone();
        Box!(modifier = around(r.clone(), o), {
            let c = c.clone();
            LazyColumn!(
                modifier = Modifier.required_height(Dp(100.0)),
                state = handle,
                move |scope: LazyListScope| {
                    let c = c.clone();
                    scope.items(30, move |index| {
                        Box!(modifier = kept(index, &c));
                    });
                }
            );
        });
    });
    for (step, wanted) in [(0, 0), (5, 20), (25, 0), (100, 10)].into_iter().enumerate() {
        let item = step * 5;
        offset.set(wanted);
        let _ = run(&mut ui, async move { handle.scroll_to_item(item, 0).await });
        ui.wait_for_idle();
        let root = root.get().expect("placed");
        for at_item in 0..5 {
            let child = coordinates.borrow()[item + at_item].expect("placed");
            assert_eq!(
                root.local_position_of_with(&child, Offset::ZERO, false),
                at(wanted, 0),
                "item {} left out",
                item + at_item,
            );
            assert_eq!(
                root.local_position_of_with(&child, Offset::ZERO, true),
                at(wanted, at_item as i32 * 20),
            );
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementScopeMotionFrameOfReferenceTest.kt, testLazyGrid
#[test]
fn a_grid_s_items_are_under_a_motion_frame() {
    use_optimizations(Optimizations::NONE);
    let offset = mutable_state_of((0, 0));
    let coordinates: Kept = Rc::new(RefCell::new(vec![None; 30]));
    let root: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let state = LazyGridState::new(0, 0);
    let (handle, o, c, r) = (*state, offset.handle(), coordinates.clone(), root.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let c = c.clone();
        Box!(modifier = around(r.clone(), o), {
            let c = c.clone();
            LazyVerticalGrid!(
                GridCells::Fixed(1),
                modifier = Modifier.required_height(Dp(100.0)).width(Dp(20.0)),
                state = handle,
                move |scope: LazyGridScope| {
                    let c = c.clone();
                    scope.items(30, move |index| {
                        Box!(modifier = kept(index, &c));
                    });
                }
            );
        });
    });
    for (step, wanted) in [(0, 0), (5, 20), (25, 0), (100, 10)].into_iter().enumerate() {
        let item = step * 5;
        offset.set(wanted);
        let _ = run(&mut ui, async move { handle.scroll_to_item(item, 0).await });
        ui.wait_for_idle();
        let root = root.get().expect("placed");
        for at_item in 0..5 {
            let child = coordinates.borrow()[item + at_item].expect("placed");
            assert_eq!(root.local_position_of_with(&child, Offset::ZERO, false), at(wanted, 0));
            assert_eq!(
                root.local_position_of_with(&child, Offset::ZERO, true),
                at(wanted, at_item as i32 * 20),
            );
        }
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Scroll.kt, ScrollNode's `withMotionFrameOfReferencePlacement`
#[test]
fn a_scroll_s_content_is_under_a_motion_frame() {
    use_optimizations(Optimizations::NONE);
    let scroll = ScrollState::new(0);
    let root: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let child: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let offset = mutable_state_of((0, 0));
    let (handle, o, r, c) = (*scroll, offset.handle(), root.clone(), child.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let c = c.clone();
        Box!(modifier = around(r.clone(), o).height(Dp(100.0)).vertical_scroll(handle), {
            let c = c.clone();
            Box!(
                modifier = Modifier.width(Dp(50.0)).height(Dp(500.0)).on_globally_positioned(
                    move |coordinates: LayoutCoordinates| c.set(Some(coordinates))
                )
            );
        });
    });
    let _ = run(&mut ui, async move { handle.scroll_to(30).await });
    ui.wait_for_idle();
    let (root, child) = (root.get().expect("placed"), child.get().expect("placed"));
    assert_eq!(root.local_position_of_with(&child, Offset::ZERO, true), Offset::new(0.0, -30.0));
    assert_eq!(root.local_position_of_with(&child, Offset::ZERO, false), Offset::ZERO);
}
