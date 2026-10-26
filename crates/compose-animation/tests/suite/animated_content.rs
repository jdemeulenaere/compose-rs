//! `AnimatedContent` against Compose's `AnimatedContentTest`: the size transform frame by
//! frame, an empty content, the alignment of both contents, an interrupted exit going back from
//! where it was, and a transition recreated under it.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntOffset, IntSize, LayoutDirection, Rect, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
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
        transition.AnimatedContent(
            Modifier.test_tag("container"),
            spec.clone(),
            alignment(),
            content.clone(),
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
            transition_spec = |_, target: i32| if target == 2 {
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
            |state: i32| {
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
                |_, _| fade_in().together_with(fade_out()),
                Alignment::TOP_START,
                |small: bool| {
                    let side = if small { 200 } else { 300 };
                    Box!(modifier = Modifier.size(side.dp()));
                },
            );
        });
    });
    // Grows, then shrinks, each time from a transition recreated.
    for (from, grows) in [(IntSize::new(200, 200), true), (IntSize::new(300, 300), false)] {
        toggle.update(|toggle| !toggle);
        ui.wait_for_idle();
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
