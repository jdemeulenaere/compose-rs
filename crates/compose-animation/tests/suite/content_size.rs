//! `Modifier.animate_content_size` against Compose's `animateContentSize`: the size frame by
//! frame along its spec, the finished listener, and the content placed by the alignment in
//! either direction.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, LayoutDirection, Rect, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
}

const FRAME: u32 = 16;
const DURATION: u32 = 10 * FRAME;

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt, animateContentSizeTest
#[test]
fn a_content_size_animates_along_its_spec_and_reports_its_end() {
    let size = mutable_state_of((100.0f32, 400.0f32));
    let handle = size.handle();
    let ends = Rc::new(Cell::new(None::<(IntSize, IntSize)>));
    let heard = ends.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let heard = heard.clone();
        let (width, height) = handle.get();
        Box!(
            modifier = Modifier
                .test_tag("animated")
                .animate_content_size_with(
                    tween(DURATION).easing(LINEAR_OUT_SLOW_IN_EASING),
                    Alignment::TOP_START,
                    Some(move |start, end| heard.set(Some((start, end)))),
                )
                .required_size((width.dp(), height.dp()))
        );
    });
    size.set((150.0, 200.0));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    for play in (0..=DURATION).step_by(FRAME as usize) {
        let fraction = LINEAR_OUT_SLOW_IN_EASING.transform(play as f32 / DURATION as f32);
        let animated = bounds(&mut ui, "animated");
        let (width, height) = (
            100.0 * (1.0 - fraction) + 150.0 * fraction,
            400.0 * (1.0 - fraction) + 200.0 * fraction,
        );
        assert!((animated.width() - width).abs() <= 1.0, "at {play}: {animated:?}");
        assert!((animated.height() - height).abs() <= 1.0, "at {play}: {animated:?}");
        if play == DURATION {
            assert_eq!(ends.get(), Some((IntSize::new(100, 400), IntSize::new(150, 200))));
        } else {
            assert_eq!(ends.get(), None, "at {play}");
        }
        ui.advance_time_by(Duration::from_millis(u64::from(FRAME)));
    }
}

#[composable]
fn Aligned(size: MutableState<(f32, f32)>, alignment: Alignment, tag: &'static str) {
    let (width, height) = size.get();
    Box!(
        modifier = Modifier.test_tag(tag).animate_content_size_with(
            tween(DURATION).easing(LINEAR_OUT_SLOW_IN_EASING),
            alignment,
            None::<fn(_, _)>,
        ),
        {
            Box!(modifier = Modifier.required_size((width.dp(), height.dp())).test_tag("content"));
        }
    );
}

/// Each alignment's content, in a box of its own, frame by frame.
fn check_alignment(direction: LayoutDirection) {
    for alignment in [Alignment::TOP_START, Alignment::CENTER, Alignment::BOTTOM_END] {
        let size = mutable_state_of((100.0f32, 400.0f32));
        let handle = size.handle();
        let mut ui =
            ComposeTestRule::with_manual_clock(move || Aligned(handle, alignment, "animated"));
        ui.host().set_layout_direction(direction);
        ui.wait_for_idle();
        size.set((150.0, 200.0));
        ui.advance_time_by_frame();
        ui.advance_time_by_frame();
        for play in (0..=DURATION).step_by(FRAME as usize) {
            let fraction = LINEAR_OUT_SLOW_IN_EASING.transform(play as f32 / DURATION as f32);
            let space = IntSize::new(
                (100.0 * (1.0 - fraction) + 150.0 * fraction).round() as i32,
                (400.0 * (1.0 - fraction) + 200.0 * fraction).round() as i32,
            );
            let expected = alignment.align(IntSize::new(150, 200), space, direction);
            let (outer, inner) = (bounds(&mut ui, "animated"), bounds(&mut ui, "content"));
            let at = (inner.left - outer.left, inner.top - outer.top);
            assert!((at.0 - expected.x as f32).abs() <= 1.0, "{alignment:?} at {play}: {at:?}");
            assert!((at.1 - expected.y as f32).abs() <= 1.0, "{alignment:?} at {play}: {at:?}");
            ui.advance_time_by(Duration::from_millis(u64::from(FRAME)));
        }
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt, testAlignmentInAnimateContentSize_underLtr
#[test]
fn a_content_size_places_its_content_by_its_alignment_left_to_right() {
    check_alignment(LayoutDirection::Ltr);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt, testAlignmentInAnimateContentSize_underRtl
#[test]
fn a_content_size_places_its_content_by_its_alignment_right_to_left() {
    check_alignment(LayoutDirection::Rtl);
}

/// A box animating its content size, in reusable content keyed by `key` inside a host `active`
/// says, its listener counting the animations that finish.
#[composable]
fn ReusedAnimated(
    active: MutableState<bool>,
    key: MutableState<u32>,
    size: MutableState<f32>,
    finished: Rc<Cell<u32>>,
) {
    let finished = finished.clone();
    ReusableContentHost(active.get(), move || {
        let finished = finished.clone();
        ReusableContent(key.get(), move || {
            let finished = finished.clone();
            let side = size.get();
            Box!(
                modifier = Modifier
                    .test_tag("animated")
                    .animate_content_size_with(
                        tween(DURATION).easing(LINEAR_OUT_SLOW_IN_EASING),
                        Alignment::TOP_START,
                        Some(move |_, _| finished.set(finished.get() + 1)),
                    )
                    .required_size((side.dp(), side.dp()))
            );
        });
    });
}

/// An animation interrupted by reuse (`reuse`) or deactivation, then reactivation
/// (`deactivate`): the reused node takes the new content's size at once, as Compose's
/// `animData = null` on reset, and the cancelled animation tells its listener nothing (its scope
/// cancelled at the detach, D1153).
fn content_size_interrupted(deactivate: bool) {
    let (active, key, size) =
        (mutable_state_of(true), mutable_state_of(0), mutable_state_of(100.0));
    let finished = Rc::new(Cell::new(0));
    let (a, k, s, f) = (active.handle(), key.handle(), size.handle(), finished.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || ReusedAnimated(a, k, s, f.clone()));
    size.set(200.0);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(u64::from(3 * FRAME)));
    let midway = bounds(&mut ui, "animated");
    assert!(midway.width() > 100.0 && midway.width() < 200.0, "animating: {midway:?}");
    if deactivate {
        active.set(false);
        ui.advance_time_by_frame();
        active.set(true);
    }
    key.set(1);
    size.set(50.0);
    ui.advance_time_by_frame();
    let reused = bounds(&mut ui, "animated");
    assert_eq!((reused.width(), reused.height()), (50.0, 50.0), "the new size at once");
    ui.advance_time_by(Duration::from_millis(u64::from(2 * DURATION)));
    assert_eq!(finished.get(), 0, "the interrupted animation told nothing");
}

#[test]
fn a_content_size_animation_interrupted_by_reuse_takes_the_new_size_at_once() {
    content_size_interrupted(false);
}

#[test]
fn a_content_size_animation_interrupted_by_deactivation_takes_the_new_size_at_once() {
    content_size_interrupted(true);
}

/// A layout noting its content's size: into `ahead` when measured ahead (if given), into `now`
/// otherwise, as the tests' `TestModifier` and lookahead layout.
fn noting_size(ahead: Option<Rc<RefCell<Vec<IntSize>>>>, now: Rc<Cell<IntSize>>) -> Modifier {
    Modifier.layout(move |measure, measurable, constraints| {
        let placeable = measure.measure(measurable, constraints);
        match (&ahead, measure.is_looking_ahead()) {
            (Some(ahead), true) => ahead.borrow_mut().push(placeable.size()),
            (_, true) => {}
            (_, false) => now.set(placeable.size()),
        }
        measure.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, 0, 0);
        })
    })
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt, testAnimatedContentSizeInLookahead
/// In a `LookaheadScope`, its size ahead is its content's new size at once, change after
/// change, and never the size animating towards it.
#[test]
fn ahead_a_content_size_is_its_contents_new_size_at_once() {
    let size = mutable_state_of(IntSize::new(400, 600));
    let sizes_ahead: Rc<RefCell<Vec<IntSize>>> = Rc::default();
    let (wanted, seen) = (size.handle(), sizes_ahead.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let noting = noting_size(Some(seen.clone()), Rc::default());
        compose_ui::LookaheadScope(callback(move |_| {
            let size = wanted.get();
            Box!(
                modifier =
                    noting.clone().animate_content_size().size((size.width.dp(), size.height.dp())),
                {
                    Box!(modifier = Modifier.size(20.dp()));
                }
            );
        }));
    });
    ui.wait_for_idle();
    // In Compose's random ranges (200 to 600 by 100 to 800), fixed so that the test repeats.
    for (width, height) in
        [(523, 140), (211, 777), (480, 455), (300, 100), (599, 799), (250, 610), (404, 302)]
    {
        let each = IntSize::new(width, height);
        size.set(each);
        sizes_ahead.borrow_mut().clear();
        ui.wait_for_idle();
        let seen = sizes_ahead.borrow();
        assert!(!seen.is_empty(), "{each:?} measured ahead");
        assert!(seen.iter().all(|it| *it == each), "{each:?}: {seen:?}");
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt, properFinalStateAfterReAttach
/// Moved half-way through its animation (movable content into another container, which
/// attaches its node again), it still ends at its content's size.
#[test]
fn moved_half_way_through_it_still_ends_at_its_contents_size() {
    let (expanded, at_a) = (mutable_state_of(false), mutable_state_of(true));
    let size = Rc::new(Cell::new(IntSize::ZERO));
    let (e, a, s) = (expanded.handle(), at_a.handle(), size.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let s = s.clone();
        let animated = remember(move || {
            movable_content_of(move || {
                Box!(
                    modifier = Modifier
                        .wrap_content_size()
                        .then(noting_size(None, s.clone()))
                        .animate_content_size_with(
                            tween(DURATION).easing(LINEAR_EASING),
                            Alignment::TOP_START,
                            None::<fn(_, _)>,
                        ),
                    {
                        let side = if e.get() { 200 } else { 100 };
                        Box!(modifier = Modifier.required_size(side.dp()));
                    }
                );
            })
        });
        Box!(modifier = Modifier.size(300.dp()), move |_| {
            let (in_a, in_b) = (animated.clone(), animated.clone());
            Box!(modifier = Modifier.offset_with(|_| compose_ui::IntOffset::ZERO), move |_| {
                if a.get() {
                    in_a();
                }
            });
            let to_b = Modifier.offset_with(|_| compose_ui::IntOffset::new(100, 100));
            Box!(modifier = to_b, move |_| {
                if !a.get() {
                    in_b();
                }
            });
        });
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    expanded.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(u64::from(DURATION / 2)));
    ui.wait_for_idle();
    assert_eq!(size.get(), IntSize::new(150, 150));
    // Moved: its node is attached again; let the animation run out.
    at_a.set(false);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(size.get(), IntSize::new(200, 200));
}
