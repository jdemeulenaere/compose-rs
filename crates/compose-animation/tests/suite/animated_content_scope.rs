//! `AnimatedContent`'s scopes against Compose's `AnimatedContentTest`: slides into and out of
//! the container, content keys, `KeepUntilTransitionsFinished`, and the enter and exit an
//! interruption picks.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntOffset, IntSize, Offset, UnitExt};

thread_local! {
    /// The content transitions of the states `false` and `true`.
    static CONTENT: RefCell<[Option<Transition<EnterExitState>>; 2]> =
        const { RefCell::new([None, None]) };
}

fn content_transition(state: bool) -> Transition<EnterExitState> {
    CONTENT.with(|content| content.borrow()[usize::from(state)].clone().expect("composed"))
}

/// The content's slide, its play time in milliseconds.
fn slide(state: bool) -> (IntOffset, i64) {
    let transition = content_transition(state);
    let slide = transition
        .animations()
        .into_iter()
        .find(|animation| animation.label().contains("slide"))
        .and_then(|animation| animation.value::<IntOffset>())
        .expect("a slide");
    (slide, transition.play_time_nanos() / 1_000_000)
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, AnimatedContentSlideInAndOutOfContainerTest
#[test]
fn contents_slide_into_and_out_of_their_container() {
    let state = MutableTransitionState::new(true);
    let given = state.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let root = remember_transition(&given, None);
        root.AnimatedContent(
            Modifier,
            |scope: AnimatedContentTransitionScope<bool>| {
                // Linear, so the slides don't reach their ends before their duration.
                let spec = tween(200).easing(LINEAR_EASING);
                let towards = if scope.is_transitioning_to(&true, &false) {
                    SlideDirection::Start
                } else {
                    SlideDirection::End
                };
                scope
                    .slide_into_container_with(towards, spec.clone(), |offset| offset)
                    .together_with(scope.slide_out_of_container_with(towards, spec, |o| o))
            },
            Alignment::TOP_START,
            ContentKey::state(),
            |scope: AnimatedContentScope, target: bool| {
                CONTENT.with(|content| {
                    content.borrow_mut()[usize::from(target)] = Some(scope.transition().clone())
                });
                Box!(modifier = Modifier.required_size(200.dp()).test_tag(format!("{target}")));
            },
        );
    });
    // The change composes both, the animations start a frame later.
    state.set_target_state(false);
    ui.advance_time_by_frame();
    ui.on_node_with_tag("true").assert_exists();
    ui.on_node_with_tag("false").assert_exists();
    ui.advance_time_by_frame();
    assert!(state.current_state());
    while state.current_state() {
        // True leaves towards the start, from 0 to -200; false comes from 200 to 0.
        let ((leaving, play), (coming, _)) = (slide(true), slide(false));
        assert_eq!(leaving, IntOffset::new(-play as i32, 0), "at {play} ms");
        assert_eq!(coming, IntOffset::new(200 - play as i32, 0), "at {play} ms");
        ui.advance_time_by_frame();
    }
    ui.on_node_with_tag("true").assert_does_not_exist();
    state.set_target_state(true);
    ui.advance_time_by_frame();
    ui.on_node_with_tag("true").assert_exists();
    ui.on_node_with_tag("false").assert_exists();
    ui.advance_time_by_frame();
    while !state.current_state() {
        // True comes from -200 to 0; false leaves from 0 to 200.
        let ((coming, play), (leaving, _)) = (slide(true), slide(false));
        assert_eq!(coming, IntOffset::new(play as i32 - 200, 0), "at {play} ms");
        assert_eq!(leaving, IntOffset::new(play as i32, 0), "at {play} ms");
        ui.advance_time_by_frame();
    }
    ui.on_node_with_tag("false").assert_does_not_exist();
}

fn offset_spring() -> AnimationSpec {
    spring_with(1.0, Spring::STIFFNESS_MEDIUM_LOW)
        .visibility_threshold(IntOffset::VISIBILITY_THRESHOLD)
        .into()
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, AnimatedContentWithContentKey
#[test]
fn only_a_change_of_content_key_animates() {
    let target = mutable_state_of(1);
    let handle = target.handle();
    let incoming = Rc::new(Cell::new(None::<Offset>));
    let outgoing = Rc::new(Cell::new(None::<Offset>));
    let container = Rc::new(Cell::new(None::<Offset>));
    let (seen_in, seen_out, seen_container) =
        (incoming.clone(), outgoing.clone(), container.clone());
    let mut ui = compose_test(move || {
        let (seen_in, seen_out, seen_container) =
            (seen_in.clone(), seen_out.clone(), seen_container.clone());
        AnimatedContent!(
            handle.get(),
            modifier = Modifier.on_globally_positioned(
                move |coordinates: compose_ui::LayoutCoordinates| {
                    seen_container.set(Some(coordinates.position_in_root()))
                }
            ),
            transition_spec = |_| slide_in_horizontally_with(offset_spring(), |_| -200)
                .together_with(
                    slide_out_horizontally_with(snap(0), |_| 200) + fade_out_with(tween(200), 0.0)
                ),
            content_key = ContentKey::of(|state: &i32| *state > 3),
            move |_, state: i32| {
                let (seen_in, seen_out) = (seen_in.clone(), seen_out.clone());
                Box!(
                    modifier = Modifier.required_size(200.dp()).on_globally_positioned(
                        move |coordinates: compose_ui::LayoutCoordinates| {
                            let at = Some(coordinates.position_in_root());
                            if state == handle.get() { seen_in.set(at) } else { seen_out.set(at) }
                        }
                    )
                );
            }
        );
    });
    ui.wait_for_idle();
    // No animation while the key stays.
    assert_eq!(container.get(), incoming.get());
    assert!(incoming.get().is_some());
    assert!(outgoing.get().is_none());
    // 1 to 4 (as Compose's three increments in one handler): the key changes.
    target.set(4);
    ui.wait_for_idle();
    assert_eq!(container.get(), incoming.get());
    let at = container.get().unwrap();
    assert_eq!(outgoing.get(), Some(Offset::new(at.x + 200.0, at.y)));
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, AnimatedContentWithKeysTest
#[test]
fn a_content_kept_by_its_key_keeps_its_saved_state() {
    let target = mutable_state_of(1);
    let handle = target.handle();
    let list = Rc::new(RefCell::new(Vec::new()));
    let seen = list.clone();
    let mut ui = compose_test(move || {
        let transition = update_transition(handle.get(), None);
        let holder = remember_saveable_state_holder();
        let seen = seen.clone();
        transition.AnimatedContent(
            Modifier,
            |_| default_content_transform(),
            Alignment::TOP_START,
            ContentKey::of(|state: &i32| *state > 2),
            move |_, state: i32| {
                if state <= 2 {
                    let seen = seen.clone();
                    holder.SaveableStateProvider(11, move || {
                        let count = remember_saveable(|| mutable_state_of(0));
                        LaunchedEffect((), async move {
                            count.update(|count| count + 1);
                            seen.borrow_mut().push(count.get());
                        });
                    });
                }
                Box!(modifier = Modifier.required_size(200.dp()));
            },
        );
        let watched = transition.clone();
        LaunchedEffect((), async move {
            assert!(!watched.is_running());
            handle.set(2);
            let watched_again = watched.clone();
            with_frame_millis(move |_| {
                // The same key: the change composes without animating.
                assert!(!watched_again.is_running());
                assert_eq!(watched_again.current_state(), 1);
                assert_eq!(watched_again.target_state(), 1);
                // A new key: this one animates.
                handle.set(3);
            })
            .await;
            // Compose resumes this task after the frame's composition, so at the next frame the
            // transition's frame comes first and it runs; compose-rs resumes it within the
            // frame, before the composition starting the transition, so this frame comes first
            // at the next frame and the transition runs from the one after.
            with_frame_millis(|_| ()).await;
            with_frame_millis(move |_| assert!(watched.is_running())).await;
        });
    });
    ui.wait_for_idle();
    assert_eq!(*list.borrow(), [1]);
    // Back to 1: the saved count comes back.
    target.set(1);
    ui.wait_for_idle();
    assert_eq!(*list.borrow(), [1, 2]);
}

/// The rule, the target, and whether `true`'s box left, `false`'s finished entering, and the
/// outer transition ended at `false`.
type Holding = (ComposeTestRule, OwnedState<bool>, Rc<Cell<bool>>, Rc<Cell<bool>>, Rc<Cell<bool>>);

/// A rule whose content is `true`'s or `false`'s box along `spec`, reporting whether `true`'s
/// box left and `false`'s finished entering.
fn holding(
    spec: impl Fn(AnimatedContentTransitionScope<bool>) -> ContentTransform + Clone + 'static,
    on_outer: bool,
) -> Holding {
    let target = mutable_state_of(true);
    let handle = target.handle();
    let disposed = Rc::new(Cell::new(false));
    let entered = Rc::new(Cell::new(false));
    let finished = Rc::new(Cell::new(false));
    let (seen_disposed, seen_entered, seen_finished) =
        (disposed.clone(), entered.clone(), finished.clone());
    let ui = compose_test(move || {
        let (seen_disposed, seen_entered) = (seen_disposed.clone(), seen_entered.clone());
        let outer = update_transition(handle.get(), None);
        if on_outer {
            seen_finished.set(!outer.target_state() && !outer.current_state());
        }
        outer.AnimatedContent(
            Modifier,
            spec.clone(),
            Alignment::TOP_START,
            ContentKey::state(),
            move |scope: AnimatedContentScope, state: bool| {
                if state {
                    let seen = seen_disposed.clone();
                    Box!(modifier = Modifier.size(200.dp()), {
                        let seen = seen.clone();
                        DisposableEffect((), move |effect| {
                            let seen = seen.clone();
                            effect.on_dispose(move || seen.set(true))
                        });
                    });
                } else {
                    let transition = scope.transition().clone();
                    seen_entered.set(
                        transition.target_state() == transition.current_state()
                            && transition.target_state() == EnterExitState::Visible,
                    );
                    Box!(modifier = Modifier.size(if on_outer { 400.dp() } else { 200.dp() }));
                }
            },
        );
    });
    (ui, target, disposed, entered, finished)
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, testExitHold
#[test]
fn a_held_exit_leaves_when_the_enter_ends() {
    let (mut ui, target, disposed, entered, _) = holding(
        |_| {
            fade_in_with(tween(200), 0.0).together_with(
                fade_out_with(tween(5), 0.0) + ExitTransition::KEEP_UNTIL_TRANSITIONS_FINISHED,
            )
        },
        false,
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    target.set(false);
    ui.wait_for_idle();
    for _ in 0..10 {
        ui.advance_time_by_frame();
        assert!(!disposed.get());
        assert!(!entered.get());
    }
    for _ in 0..10 {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        assert_eq!(disposed.get(), entered.get());
    }
    assert!(disposed.get());
    assert!(entered.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, testExitHoldDefersUntilAllFinished
#[test]
fn a_held_exit_leaves_when_every_animation_ends() {
    let (mut ui, target, disposed, entered, finished) = holding(
        |_| {
            fade_in_with(tween(160), 0.0)
                .together_with(
                    fade_out_with(tween(5), 0.0) + ExitTransition::KEEP_UNTIL_TRANSITIONS_FINISHED,
                )
                .using(Some(size_transform(true, |_, _| tween(300).into())))
        },
        true,
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    target.set(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    for _ in 0..10 {
        ui.advance_time_by_frame();
        assert!(!disposed.get());
        assert!(!entered.get());
    }
    // The enter has ended; the size still animates, so true's box stays.
    for _ in 0..3 {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        assert!(entered.get());
        assert!(!disposed.get());
        assert!(!finished.get());
    }
    for _ in 0..10 {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        assert!(entered.get());
        assert_eq!(disposed.get(), finished.get());
    }
    assert!(disposed.get());
    assert!(entered.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedContentTest.kt, testRightEnterExitTransitionIsChosenDuringInterruption
#[test]
fn an_interruption_picks_the_right_enter_and_exit() {
    let flag = mutable_state_of(false);
    let handle = flag.handle();
    let fixed = Rc::new(Cell::new(None::<Offset>));
    let sliding = Rc::new(Cell::new(None::<Offset>));
    let (seen_fixed, seen_sliding) = (fixed.clone(), sliding.clone());
    let unclipped = || {
        size_transform(false, |_, _| {
            spring_with(1.0, Spring::STIFFNESS_MEDIUM_LOW)
                .visibility_threshold(IntSize::VISIBILITY_THRESHOLD)
                .into()
        })
    };
    let mut ui = compose_test(move || {
        let (seen_fixed, seen_sliding) = (seen_fixed.clone(), seen_sliding.clone());
        AnimatedContent!(
            handle.get(),
            modifier = Modifier.fill_max_size(),
            transition_spec = move |scope: AnimatedContentTransitionScope<bool>| {
                if scope.is_transitioning_to(&false, &true) {
                    ContentTransform {
                        target_content_enter: EnterTransition::NONE,
                        initial_content_exit: scope.slide_out_of_container_with(
                            SlideDirection::Start,
                            tween(500),
                            |offset| offset,
                        ),
                        target_content_z_index: -1.0,
                        size_transform: Some(unclipped()),
                    }
                } else {
                    ContentTransform {
                        target_content_enter: scope.slide_into_container(SlideDirection::End),
                        initial_content_exit: ExitTransition::KEEP_UNTIL_TRANSITIONS_FINISHED,
                        target_content_z_index: 0.0,
                        size_transform: Some(unclipped()),
                    }
                }
            },
            label = "",
            move |_, state: bool| {
                let (seen_fixed, seen_sliding) = (seen_fixed.clone(), seen_sliding.clone());
                Spacer(
                    Modifier
                        .wrap_content_size_with(Alignment::CENTER, false)
                        .size(256.dp())
                        .on_globally_positioned(
                            move |coordinates: compose_ui::LayoutCoordinates| {
                                let at = Some(coordinates.position_in_root());
                                if state { seen_fixed.set(at) } else { seen_sliding.set(at) }
                            },
                        ),
                );
            }
        );
    });
    flag.set(true);
    ui.wait_until(|_| fixed.get().is_some());
    let initial = fixed.get();
    for _ in 0..10 {
        let last = sliding.get();
        ui.wait_until(|_| sliding.get() != last);
        assert_eq!(fixed.get(), initial);
    }
    // Interrupted: true's content stays where it is, false's slides in.
    flag.set(false);
    for _ in 0..10 {
        let last = sliding.get();
        ui.wait_until(|_| sliding.get() != last);
        assert_eq!(fixed.get(), initial);
    }
    ui.wait_for_idle();
}
