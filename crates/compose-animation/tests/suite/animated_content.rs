//! `AnimatedContent` against Compose's `AnimatedContentTest`: the size transform frame by
//! frame, an empty content, the alignment of both contents, an interrupted exit going back from
//! where it was, and a transition recreated under it.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntOffset, IntSize, LayoutDirection, Modifier, Rect, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
}

fn size_of(ui: &mut ComposeTestRule, tag: &str) -> (i32, i32) {
    let bounds = bounds(ui, tag);
    (bounds.width().round() as i32, bounds.height().round() as i32)
}

thread_local! {
    /// The transition under test, to read its play time between frames.
    static PLAYING: RefCell<Option<Transition<bool>>> = const { RefCell::new(None) };
}

/// The transition's play time in milliseconds, as the test's `playTimeMillis`.
fn play_time_millis() -> i64 {
    PLAYING.with(|playing| playing.borrow().as_ref().unwrap().play_time_nanos() / 1_000_000)
}

/// `content(state)` in an `AnimatedContent` of a transition of `state`, tagged "container".
fn on_transition(
    state: MutableTransitionState<bool>,
    spec: impl Fn(bool, bool) -> ContentTransform + Clone + 'static,
    alignment: impl Fn() -> Alignment + 'static,
    content: impl Fn(bool) + Clone + 'static,
) -> ComposeTestRule {
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&state, None);
        PLAYING.with(|playing| *playing.borrow_mut() = Some(transition.clone()));
        let (spec, content) = (spec.clone(), content.clone());
        transition.AnimatedContent(
            Modifier.test_tag("container"),
            move |scope| spec(*scope.initial_state(), *scope.target_state()),
            alignment(),
            ContentKey::state(),
            move |_, state| content(state),
        );
    });
    ui.wait_for_idle();
    ui
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, AnimatedContentSizeTransformTest
#[test]
fn the_size_transform_animates_the_containers_size() {
    let state = MutableTransitionState::new(true);
    let spec = |initial: bool, target: bool| {
        if initial && !target {
            // To full width in the first 160 ms, then full height in the next 160.
            fade_in().together_with(fade_out()).using(Some(size_transform(true, |from, to| {
                keyframes(|frames| {
                    frames.duration_millis = 320;
                    frames.at(160, IntSize::new(to.width, from.height)).using(LINEAR_EASING);
                    frames.at(320, to).using(LINEAR_EASING);
                })
                .into()
            })))
        } else {
            fade_in()
                .together_with(fade_out())
                .using(Some(size_transform(true, |_, _| tween(80).easing(LINEAR_EASING).into())))
        }
    };
    let content = |first: bool| {
        let size = if first { 40 } else { 200 };
        Box!(modifier = Modifier.size(size.dp()));
    };
    let mut ui = on_transition(state.clone(), spec, || Alignment::TOP_START, content);
    assert_eq!(size_of(&mut ui, "container"), (40, 40));
    state.set_target_state(false);
    ui.wait_for_idle();
    while state.current_state() != state.target_state() {
        let play_time = play_time_millis() as i32;
        let (width, height) = size_of(&mut ui, "container");
        if play_time <= 160 {
            assert_eq!((width, height), (play_time + 40, 40), "at {play_time} ms");
        } else {
            assert_eq!((width, height), (200, play_time - 120), "at {play_time} ms");
        }
        ui.advance_time_by_frame();
    }
    assert_eq!(size_of(&mut ui, "container"), (200, 200));
    state.set_target_state(true);
    ui.wait_for_idle();
    while state.current_state() != state.target_state() {
        let play_time = play_time_millis() as i32;
        if play_time <= 80 {
            let side = 200 - play_time * 2;
            assert_eq!(size_of(&mut ui, "container"), (side, side), "at {play_time} ms");
        }
        ui.advance_time_by_frame();
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, AnimatedContentSizeTransformEmptyComposableTest
#[test]
fn the_size_transform_animates_to_and_from_an_empty_content() {
    let state = MutableTransitionState::new(true);
    let spec = |_, _| {
        EnterTransition::NONE
            .together_with(ExitTransition::NONE)
            .using(Some(size_transform(true, |_, _| tween(160).easing(LINEAR_EASING).into())))
    };
    let content = |shown: bool| {
        if shown {
            Box!(modifier = Modifier.size(160.dp()));
        }
    };
    let mut ui = on_transition(state.clone(), spec, || Alignment::TOP_START, content);
    assert_eq!(size_of(&mut ui, "container"), (160, 160));
    state.set_target_state(false);
    ui.wait_for_idle();
    while state.current_state() != state.target_state() {
        let side = 160 - play_time_millis() as i32;
        assert_eq!(size_of(&mut ui, "container"), (side, side));
        ui.advance_time_by_frame();
    }
    // Only an empty content now.
    assert_eq!(size_of(&mut ui, "container"), (0, 0));
    state.set_target_state(true);
    ui.wait_for_idle();
    while state.current_state() != state.target_state() {
        let side = play_time_millis() as i32;
        assert_eq!(size_of(&mut ui, "container"), (side, side));
        ui.advance_time_by_frame();
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, AnimatedContentContentAlignmentTest
#[test]
fn both_contents_are_placed_by_the_content_alignment() {
    let (size1, size2) = (IntSize::new(80, 80), IntSize::new(160, 240));
    let state = MutableTransitionState::new(true);
    let alignment = mutable_state_of(Alignment::TOP_START);
    let handle = alignment.handle();
    let spec = |_, _| {
        fade_in_with(tween(80), 0.0)
            .together_with(fade_out_with(tween(80), 0.0))
            .using(Some(size_transform(true, |_, _| tween(80).easing(LINEAR_EASING).into())))
    };
    let content = move |first: bool| {
        let (size, tag) = if first { (size1, "one") } else { (size2, "two") };
        Box!(modifier = Modifier.size((size.width.dp(), size.height.dp())).test_tag(tag));
    };
    let mut ui = on_transition(state.clone(), spec, move || handle.get(), content);
    // Where `tag` is in the container.
    let offset = |ui: &mut ComposeTestRule, tag: &str| {
        let (container, at) = (bounds(ui, "container"), bounds(ui, tag));
        Some(IntOffset::new(
            (at.left - container.left).round() as i32,
            (at.top - container.top).round() as i32,
        ))
    };
    for each in [
        Alignment::TOP_START,
        Alignment::BOTTOM_START,
        Alignment::CENTER,
        Alignment::BOTTOM_END,
        Alignment::TOP_END,
    ] {
        assert_eq!(size_of(&mut ui, "container"), (80, 80));
        alignment.set(each);
        ui.advance_time_by_frame();
        state.set_target_state(false);
        ui.wait_for_idle();
        let check = |ui: &mut ComposeTestRule, leaving: &str, coming: &str| {
            let play_time = play_time_millis();
            let (width, height) = size_of(ui, "container");
            let space = IntSize::new(width, height);
            let (left, came) = if leaving == "one" { (size1, size2) } else { (size2, size1) };
            if play_time < 80 {
                let expected = each.align(left, space, LayoutDirection::Ltr);
                assert_eq!(offset(ui, leaving), Some(expected), "{each:?} at {play_time}");
            }
            if play_time > 0 {
                let expected = each.align(came, space, LayoutDirection::Ltr);
                assert_eq!(offset(ui, coming), Some(expected), "{each:?} at {play_time}");
            }
        };
        while state.current_state() != state.target_state() {
            check(&mut ui, "one", "two");
            ui.advance_time_by_frame();
        }
        assert_eq!(size_of(&mut ui, "container"), (160, 240));
        assert_eq!(offset(&mut ui, "two"), Some(IntOffset::ZERO));
        state.set_target_state(true);
        ui.wait_for_idle();
        while state.current_state() != state.target_state() {
            check(&mut ui, "two", "one");
            ui.advance_time_by_frame();
        }
        assert_eq!(size_of(&mut ui, "container"), (80, 80));
        assert_eq!(offset(&mut ui, "one"), Some(IntOffset::ZERO));
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, testInterruptionAfterExitFinishedRecoversExitState
#[test]
fn an_exit_finished_then_interrupted_goes_back_from_where_it_was() {
    let target = mutable_state_of(1);
    let handle = target.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        AnimatedContent!(
            handle.get(),
            modifier = Modifier.fill_max_size(),
            transition_spec =
                |scope: AnimatedContentTransitionScope<i32>| if *scope.target_state() == 2 {
                    // 1 exits fast (100 ms) to the left; 2 enters slowly (500 ms).
                    ContentTransform::new(
                        slide_in_horizontally_with(tween(500), |width| width),
                        slide_out_horizontally_with(tween(100), |width| -width),
                    )
                } else {
                    // Back to 1: 1 enters slowly, 2 exits fast.
                    ContentTransform::new(
                        slide_in_horizontally_with(tween(500), |width| -width),
                        slide_out_horizontally_with(tween(100), |width| width),
                    )
                },
            |_, state: i32| {
                Box!(modifier = Modifier.size(100.dp()).test_tag(format!("{state}")));
            }
        );
    });
    ui.advance_time_by_frame();
    assert_eq!(bounds(&mut ui, "1").left, 0.0);
    target.set(2);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // 1 has exited (100 ms), 2 still enters (500 ms).
    ui.advance_time_by(std::time::Duration::from_millis(150));
    target.set(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(std::time::Duration::from_millis(50));
    // 1 comes back from where its exit left it, -100, not at once at 0.
    let left = bounds(&mut ui, "1").left;
    assert!(left < -10.0, "1 at {left}, animating back from -100");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, testRecreatingTransitionInAnimatedContent
#[test]
fn a_recreated_transitions_content_animates_its_size_without_jumps() {
    let toggle = mutable_state_of(true);
    let target = mutable_state_of(true);
    let size = Rc::new(Cell::new(IntSize::new(200, 200)));
    let seen = size.clone();
    let (toggle_handle, target_handle) = (toggle.handle(), target.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        let transition = key(toggle_handle.get(), || update_transition(target_handle.get(), None));
        Column!({
            let seen = seen.clone();
            transition.AnimatedContent(
                Modifier.on_size_changed(move |size| {
                    assert_ne!(size, IntSize::ZERO);
                    seen.set(size);
                }),
                |_| fade_in().together_with(fade_out()),
                Alignment::TOP_START,
                ContentKey::state(),
                |_, small: bool| {
                    let side = if small { 200 } else { 300 };
                    Box!(modifier = Modifier.size(side.dp()));
                },
            );
        });
    });
    // Grows, then shrinks, each time from a transition recreated.
    for (from, grows) in [(IntSize::new(200, 200), true), (IntSize::new(300, 300), false)] {
        // Recreated, the clock advancing by itself; then the change, frame by frame.
        ui.set_auto_advance(true);
        toggle.update(|toggle| !toggle);
        ui.wait_for_idle();
        ui.set_auto_advance(false);
        target.update(|target| !target);
        let mut frames = 0;
        while size.get() == from {
            ui.advance_time_by_frame();
            frames += 1;
            assert!(frames < 100, "the size never changed");
        }
        let mut last = from;
        let mut frame_count = 0;
        loop {
            let current = size.get();
            let going = if grows {
                current.width < 290 && current.height < 290
            } else {
                current.width > 210 && current.height > 210
            };
            if !going && frame_count >= 10 {
                break;
            }
            // Monotonic, never jumping to 0.
            if grows {
                assert!(last.width < current.width && last.height < current.height, "{current:?}");
            } else {
                assert!(last.width > current.width && last.height > current.height, "{current:?}");
            }
            last = current;
            ui.advance_time_by_frame();
            frame_count += 1;
        }
        // Settled.
        for _ in 0..100 {
            ui.advance_time_by_frame();
        }
    }
}

/// The content entering keeps its composition when a recomposition finds the old content's exit
/// over (a short exit, a long enter): Compose's `key(contentKey(it))` keeps it (D828).
#[test]
fn the_entering_content_stays_composed_when_the_old_one_has_left_first() {
    let target = mutable_state_of(0);
    let unrelated = mutable_state_of(0);
    let created = Rc::new(Cell::new(0));
    let (read, read_unrelated, count) = (target.handle(), unrelated.handle(), created.clone());
    let mut ui = compose_test(move || {
        // Reads the state: every change recomposes the `AnimatedContent` call.
        let _ = read_unrelated.get();
        let count = count.clone();
        AnimatedContent!(
            read.get(),
            transition_spec = |_| fade_in_with(tween(10_000), 0.0)
                .together_with(fade_out_with(tween(32), 0.0))
                .using(None),
            content = move |_, state: i32| {
                let count = count.clone();
                let _ = remember(move || {
                    if state == 1 {
                        count.set(count.get() + 1);
                    }
                    Rc::new(())
                });
                Box!(modifier = Modifier.size(10.dp()));
            }
        );
    });
    ui.set_auto_advance(false);
    target.set(1);
    ui.advance_time_by(Duration::from_millis(200));
    assert_eq!(created.get(), 1);
    unrelated.set(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(created.get(), 1, "not composed again");
}

/// Where it's last been placed in the lookahead scope's coordinates: ahead, and now.
type Seen = Rc<Cell<Option<compose_ui::Offset>>>;

/// A layout noting where it's placed in `scope`'s coordinates, ahead in `ahead` and now in
/// `now`, as the tests' `localLookaheadPositionOf` and `localPositionOf` placement blocks.
fn noting_position(scope: compose_ui::LookaheadScope, ahead: Seen, now: Seen) -> Modifier {
    Modifier.layout(move |measure, measurable, constraints| {
        let placeable = measure.measure(measurable, constraints);
        let (ahead, now) = (ahead.clone(), now.clone());
        measure.layout_with(placeable.width, placeable.height, move |placement| {
            if let (Some(frame), Some(own)) =
                (scope.lookahead_scope_coordinates(placement), placement.coordinates())
            {
                if placement.is_looking_ahead() {
                    let at =
                        scope.local_lookahead_position_of(frame, own, compose_ui::Offset::ZERO);
                    ahead.set(Some(at));
                } else {
                    now.set(Some(frame.local_position_of(&own, compose_ui::Offset::ZERO)));
                }
            }
            placement.place(&placeable, 0, 0);
        })
    })
}

/// A layout noting the size its content's measured at ahead, as the tests' `TestModifier`.
fn noting_size_ahead(size: Rc<Cell<Option<IntSize>>>) -> Modifier {
    Modifier.layout(move |measure, measurable, constraints| {
        let placeable = measure.measure(measurable, constraints);
        if measure.is_looking_ahead() {
            size.set(Some(placeable.size()));
        }
        measure.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, 0, 0);
        })
    })
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, AnimatedContentLookaheadTest
/// In a `LookaheadScope`, the container's size ahead is its target content's, from the first
/// frame of the change to its end, not the size animating; and the target content is placed
/// ahead at the container's origin, centred in a container of its own size.
#[test]
fn ahead_the_container_is_its_target_contents_size_and_holds_it_at_its_origin() {
    let state = MutableTransitionState::new(true);
    let size_ahead = Rc::new(Cell::new(None::<IntSize>));
    let (ahead, now): (Seen, Seen) = Default::default();
    let (shown, s, a, n) = (state.clone(), size_ahead.clone(), ahead.clone(), now.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let (shown, s, a, n) = (shown.clone(), s.clone(), a.clone(), n.clone());
        compose_ui::LookaheadScope(callback(move |scope| {
            let (shown, a, n) = (shown.clone(), a.clone(), n.clone());
            Box!(modifier = noting_size_ahead(s.clone()), move |_| {
                let transition = remember_transition(&shown, None);
                PLAYING.with(|playing| *playing.borrow_mut() = Some(transition.clone()));
                let (a, n) = (a.clone(), n.clone());
                transition.AnimatedContent(
                    Modifier,
                    |_| {
                        fade_in()
                            .together_with(fade_out())
                            .using(Some(size_transform(true, |_, _| {
                                tween(80).easing(LINEAR_EASING).into()
                            })))
                    },
                    Alignment::CENTER,
                    ContentKey::state(),
                    move |_, first: bool| {
                        if first {
                            Box!(modifier = Modifier.size(400.dp()));
                        } else {
                            let noting = noting_position(scope, a.clone(), n.clone());
                            Box!(modifier = noting.size(20.dp()));
                        }
                    },
                );
            });
        }));
    });
    ui.wait_for_idle();
    assert_eq!(size_ahead.get(), Some(IntSize::new(400, 400)));
    state.set_target_state(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    let mut frames = 0;
    while state.current_state() != state.target_state() {
        ui.wait_for_idle();
        let play_time = play_time_millis();
        assert_eq!(size_ahead.get(), Some(IntSize::new(20, 20)), "at {play_time} ms");
        assert!(now.get().is_some(), "placed now at {play_time} ms");
        assert_eq!(ahead.get(), Some(compose_ui::Offset::ZERO), "at {play_time} ms");
        ui.advance_time_by_frame();
        frames += 1;
        assert!(frames < 100, "the change never ended");
    }
    ui.wait_for_idle();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, testSizeTransformAlwaysContinuous
/// Under `animate_bounds` growing it from 200 to 300 in a `LookaheadScope`, a container whose
/// content stays (the same key) grows frame by frame, never shrinking, to 300 × 300.
#[test]
fn under_animated_bounds_the_container_grows_without_going_back() {
    let large = mutable_state_of(false);
    let current = Rc::new(Cell::new(IntSize::ZERO));
    let (is_large, c) = (large.handle(), current.clone());
    let mut ui = compose_test(move || {
        let c = c.clone();
        compose_ui::LookaheadScope(callback(move |scope| {
            let c = c.clone();
            Box!(move |_| {
                let c = c.clone();
                let side = if is_large.get() { 300 } else { 200 };
                let noting = Modifier.layout(move |measure, measurable, constraints| {
                    let unbounded = compose_ui::Constraints {
                        max_width: compose_ui::Constraints::INFINITY,
                        max_height: compose_ui::Constraints::INFINITY,
                        ..constraints
                    };
                    let placeable = measure.measure(measurable, unbounded);
                    if !measure.is_looking_ahead() {
                        c.set(placeable.size());
                    }
                    measure.layout_with(placeable.width, placeable.height, move |placement| {
                        placement.place(&placeable, 0, 0);
                    })
                });
                AnimatedContent!(
                    is_large.get(),
                    modifier = Modifier
                        .animate_bounds_with(
                            scope,
                            Modifier.size(side.dp()),
                            SharedTransitionDefaults::bounds_transform(),
                            false,
                        )
                        .then(noting)
                        .background(compose_ui::Color::GRAY),
                    transition_spec = |_| fade_in().together_with(fade_out()),
                    label = "Test",
                    content_key = ContentKey::of(|_: &bool| true),
                    content = |_, _: bool| {
                        Box!(
                            modifier = Modifier
                                .background(compose_ui::Color::BLACK)
                                .size((200.dp(), 100.dp()))
                        );
                    }
                );
            });
        }));
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    large.set(true);
    assert_eq!(current.get(), IntSize::new(200, 200));
    let mut last = IntSize::new(200, 200);
    let mut frames = 0;
    loop {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        let now = current.get();
        assert!(now.width >= last.width && now.height >= last.height, "{last:?} to {now:?}");
        last = now;
        frames += 1;
        if frames >= 5 && now == IntSize::new(300, 300) {
            break;
        }
        assert!(frames < 200, "never reached 300 × 300: {now:?}");
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, testTargetChangeLookaheadPlacement
/// In a `LookaheadScope`, the content entering is placed ahead at the origin, while the one
/// leaving (filling the root) is placed ahead centred on the smaller target, away from it; when
/// the target changes back, the content that's the target again is placed ahead at the origin.
#[test]
fn ahead_the_target_content_is_at_the_origin_whichever_it_is() {
    let state = MutableTransitionState::new(true);
    let (first_ahead, second_ahead, ignored): (Seen, Seen, Seen) = Default::default();
    let (shown, a1, a2, i) =
        (state.clone(), first_ahead.clone(), second_ahead.clone(), ignored.clone());
    let mut ui = compose_test(move || {
        let (shown, a1, a2, i) = (shown.clone(), a1.clone(), a2.clone(), i.clone());
        compose_ui::LookaheadScope(callback(move |scope| {
            let transition = remember_transition(&shown, None);
            PLAYING.with(|playing| *playing.borrow_mut() = Some(transition.clone()));
            let (a1, a2, i) = (a1.clone(), a2.clone(), i.clone());
            transition.AnimatedContent(
                Modifier,
                |_| fade_in().together_with(fade_out()).using(None),
                Alignment::CENTER,
                ContentKey::state(),
                move |_, first: bool| {
                    if first {
                        let noting = noting_position(scope, a1.clone(), i.clone());
                        Box!(modifier = noting.fill_max_size().background(compose_ui::Color::BLUE));
                    } else {
                        let noting = noting_position(scope, a2.clone(), i.clone());
                        Box!(modifier = noting.size(100.dp()).background(compose_ui::Color::RED));
                    }
                },
            );
        }));
    });
    ui.wait_for_idle();
    assert!(state.target_state() && state.current_state());
    assert_eq!(first_ahead.get(), Some(compose_ui::Offset::ZERO));
    ui.set_auto_advance(false);
    state.set_target_state(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    for _ in 0..3 {
        assert_ne!(state.current_state(), state.target_state());
        ui.wait_for_idle();
        assert_eq!(second_ahead.get(), Some(compose_ui::Offset::ZERO));
        ui.advance_time_by_frame();
        ui.wait_for_idle();
    }
    // The content leaving, ahead, is centred on the 100 × 100 target, away from the origin.
    let leaving = first_ahead.get().unwrap();
    assert_ne!(leaving.x, 0.0, "{leaving:?}");
    assert_ne!(leaving.y, 0.0, "{leaving:?}");
    // Interrupted: the first content's the target again, at the origin ahead.
    state.set_target_state(true);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(first_ahead.get(), Some(compose_ui::Offset::ZERO));
}

// Compose: compose/animation/animation/src/commonMain/kotlin/androidx/compose/animation/AnimatedVisibility.kt, AnimatedEnterExitMeasurePolicy.measure (`targetSize` set ahead once measured ahead)
/// In a `LookaheadScope`, the size transform heads for the target content's size ahead: a
/// content 100 wide ahead and 50 now, entering after one of 20 along a linear 100 ms tween, makes
/// the container `20 + 0.8 t` wide at `t` ms, then its size now once the change is over.
#[test]
fn in_a_scope_the_size_transform_heads_for_the_targets_size_ahead() {
    let state = MutableTransitionState::new(true);
    let width = Rc::new(Cell::new(0));
    let (shown, w) = (state.clone(), width.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let (shown, w) = (shown.clone(), w.clone());
        compose_ui::LookaheadScope(callback(move |_| {
            let w = w.clone();
            let noting = Modifier.layout(move |measure, measurable, constraints| {
                let placeable = measure.measure(measurable, constraints);
                if !measure.is_looking_ahead() {
                    w.set(placeable.width);
                }
                measure.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0);
                })
            });
            let transition = remember_transition(&shown, None);
            PLAYING.with(|playing| *playing.borrow_mut() = Some(transition.clone()));
            transition.AnimatedContent(
                noting,
                |_| {
                    EnterTransition::NONE.together_with(ExitTransition::NONE).using(Some(
                        size_transform(true, |_, _| tween(100).easing(LINEAR_EASING).into()),
                    ))
                },
                Alignment::TOP_START,
                ContentKey::state(),
                |_, first: bool| {
                    if first {
                        Box!(modifier = Modifier.size(20.dp()));
                    } else {
                        let by_pass = Modifier.layout(|measure, measurable, _| {
                            let side = if measure.is_looking_ahead() { 100 } else { 50 };
                            let fixed = compose_ui::Constraints::fixed(side, side);
                            let placeable = measure.measure(measurable, fixed);
                            measure.layout_with(side, side, move |placement| {
                                placement.place(&placeable, 0, 0);
                            })
                        });
                        Box!(modifier = by_pass);
                    }
                },
            );
        }));
    });
    ui.wait_for_idle();
    assert_eq!(width.get(), 20);
    state.set_target_state(false);
    ui.wait_for_idle();
    let mut checked = 0;
    while state.current_state() != state.target_state() {
        let play_time = play_time_millis() as i32;
        if play_time > 0 && play_time < 100 {
            let expected = 20 + play_time * 4 / 5;
            assert!((width.get() - expected).abs() <= 1, "{} at {play_time} ms", width.get());
            checked += 1;
        }
        ui.advance_time_by_frame();
    }
    assert!(checked > 3, "{checked} frames checked");
    ui.wait_for_idle();
    assert_eq!(width.get(), 50);
}
