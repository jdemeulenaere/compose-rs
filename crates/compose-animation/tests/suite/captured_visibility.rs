//! `CapturedAnimatedVisibility`, against Compose's `CapturedAnimatedVisibilityTest`: the content
//! leaving the composition as soon as it's hidden, what's drawn being its last frame. The pixels
//! of its test are read from the display list; its lookahead tests wait for lookahead.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::*;

fn fade() -> (EnterTransition, ExitTransition) {
    (fade_in_with(tween(160), 0.0), fade_out_with(tween(160), 0.0))
}

#[composable]
fn Noted(present: Rc<Cell<bool>>, modifier: Modifier) {
    {
        let present = present.clone();
        DisposableEffect((), move |scope| {
            present.set(true);
            scope.on_dispose(move || present.set(false))
        });
    }
    Box!(modifier = modifier);
}

/// A visibility of a `MutableTransitionState`, its content noting it's there.
fn of_state(initial: bool) -> (ComposeTestRule, MutableTransitionState<bool>, Rc<Cell<bool>>) {
    let state = MutableTransitionState::new(initial);
    let present = Rc::new(Cell::new(false));
    let (shown, noted) = (state.clone(), present.clone());
    let ui = compose_test(move || {
        let noted = noted.clone();
        let (enter, exit) = fade();
        CapturedAnimatedVisibility!(shown.clone(), enter = enter, exit = exit, {
            Noted!(noted.clone(), Modifier.size(100.dp()));
        });
    });
    (ui, state, present)
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt, testCapturedAnimatedVisibility_disposesContentImmediatelyOnExit
#[test]
fn hidden_the_content_leaves_at_once() {
    let visible = mutable_state_of(true);
    let handle = visible.handle();
    let present = Rc::new(Cell::new(false));
    let noted = present.clone();
    let mut ui = compose_test(move || {
        let noted = noted.clone();
        let (enter, exit) = fade();
        CapturedAnimatedVisibility!(handle.get(), enter = enter, exit = exit, {
            Noted!(noted.clone(), Modifier.size(100.dp()));
        });
    });
    ui.wait_for_idle();
    assert!(present.get());
    ui.set_auto_advance(false);
    visible.set(false);
    ui.advance_time_by_frame();
    assert!(!present.get());
    ui.set_auto_advance(true);
    ui.wait_for_idle();
}

fn red_rects(ui: &mut ComposeTestRule) -> usize {
    let flat = ui.host().display_list();
    flat.ops
        .iter()
        .filter(|op| matches!(op, DrawOp::DrawRect(_, paint) if paint.color == Color::RED))
        .count()
}

fn alpha_of_layer(ui: &mut ComposeTestRule) -> Option<f32> {
    let flat = ui.host().display_list();
    flat.ops.iter().find_map(|op| match op {
        DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
        _ => None,
    })
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt, testCapturedAnimatedVisibility_fadingOutPixelVerification
#[test]
fn what_fades_out_is_the_last_frame_drawn() {
    let visible = mutable_state_of(true);
    let handle = visible.handle();
    let present = Rc::new(Cell::new(false));
    let noted = present.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let noted = noted.clone();
        Box!(modifier = Modifier.size(40.dp()).test_tag("container").background(Color::WHITE), {
            let noted = noted.clone();
            CapturedAnimatedVisibility!(
                handle.get(),
                enter = EnterTransition::NONE,
                exit = fade_out_with(tween(160).easing(LINEAR_EASING), 0.0),
                { Noted!(noted.clone(), Modifier.size(40.dp()).background(Color::RED)) }
            );
        });
    });
    ui.advance_time_by_frame();
    assert!(present.get());
    assert_eq!(red_rects(&mut ui), 1);
    assert_eq!(alpha_of_layer(&mut ui), None);
    visible.set(false);
    ui.advance_time_by_frame();
    assert!(!present.get());
    ui.advance_time_by(Duration::from_millis(96));
    assert_eq!(red_rects(&mut ui), 1, "the last frame is drawn, though its node is gone");
    let alpha = alpha_of_layer(&mut ui).expect("a fade");
    assert!((alpha - 0.5).abs() <= 0.1, "{alpha}");
    ui.advance_time_by(Duration::from_millis(100));
    ui.wait_for_idle();
    assert_eq!(red_rects(&mut ui), 0);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt, testCapturedAnimatedVisibility_mutableTransitionState
#[test]
fn a_state_enters_and_exits_and_says_so() {
    let (mut ui, state, present) = of_state(false);
    ui.wait_for_idle();
    assert!(!present.get());
    assert!(state.is_idle());
    assert!(!state.current_state());
    assert!(!state.target_state());
    state.set_target_state(true);
    ui.wait_for_idle();
    assert!(present.get());
    assert!(state.is_idle());
    assert!(state.current_state());
    ui.set_auto_advance(false);
    state.set_target_state(false);
    ui.advance_time_by_frame();
    assert!(!present.get());
    ui.advance_time_by(Duration::from_millis(200));
    ui.wait_for_idle();
    assert!(state.is_idle());
    assert!(!state.current_state());
    assert!(!state.target_state());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt, testCapturedAnimatedVisibility_mutableTransitionState_initialTrue
#[test]
fn a_state_visible_from_the_start_is_composed_and_idle() {
    let (mut ui, state, present) = of_state(true);
    ui.wait_for_idle();
    assert!(present.get());
    assert!(state.is_idle());
    assert!(state.current_state());
    assert!(state.target_state());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt, testCapturedAnimatedVisibility_interruptExitWithEnter
#[test]
fn an_enter_interrupting_an_exit_composes_the_content_anew() {
    let (mut ui, state, present) = of_state(true);
    ui.wait_for_idle();
    assert!(present.get());
    ui.set_auto_advance(false);
    state.set_target_state(false);
    ui.advance_time_by_frame();
    assert!(!present.get());
    assert!(!state.is_idle());
    ui.advance_time_by(Duration::from_millis(80));
    assert!(!present.get(), "it stays out during the exit");
    state.set_target_state(true);
    ui.advance_time_by_frame();
    assert!(present.get());
    assert!(!state.is_idle());
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert!(state.is_idle());
    assert!(state.current_state());
    assert!(state.target_state());
    assert!(present.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt, testCapturedAnimatedVisibility_interruptEnterWithExit
#[test]
fn an_exit_interrupting_an_enter_disposes_the_content() {
    let (mut ui, state, present) = of_state(false);
    ui.wait_for_idle();
    assert!(!present.get());
    ui.set_auto_advance(false);
    state.set_target_state(true);
    ui.advance_time_by_frame();
    assert!(present.get());
    ui.advance_time_by(Duration::from_millis(80));
    assert!(present.get());
    assert!(!state.is_idle());
    state.set_target_state(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert!(!present.get());
    ui.advance_time_by(Duration::from_millis(200));
    ui.wait_for_idle();
    assert!(state.is_idle());
    assert!(!state.current_state());
    assert!(!state.target_state());
    assert!(!present.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/CapturedAnimatedVisibilityTest.kt, testCapturedAnimatedVisibility_interruptExitWithNewSize
#[test]
fn an_exit_interrupted_by_a_new_size_grows_to_it() {
    let visible = mutable_state_of(true);
    let height = mutable_state_of(100.dp());
    let (shown, tall) = (visible.handle(), height.handle());
    let present = Rc::new(Cell::new(false));
    let noted = present.clone();
    let mut ui = compose_test(move || {
        let noted = noted.clone();
        Box!(modifier = Modifier.test_tag("box"), {
            let noted = noted.clone();
            CapturedAnimatedVisibility!(
                shown.get(),
                enter = expand_vertically_with(
                    tween(160).easing(LINEAR_EASING),
                    Alignment::BOTTOM,
                    true,
                    |_| 0
                ),
                exit = shrink_vertically_with(
                    tween(160).easing(LINEAR_EASING),
                    Alignment::BOTTOM,
                    true,
                    |_| 0
                ),
                { Noted!(noted.clone(), Modifier.size((100.dp(), tall.get()))) }
            );
        });
    });
    let height_of = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag("box").fetch_semantics_node().bounds.height()
    };
    ui.wait_for_idle();
    assert!(present.get());
    assert_eq!(height_of(&mut ui), 100.0);
    ui.set_auto_advance(false);
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(!present.get());
    ui.advance_time_by(Duration::from_millis(80));
    assert!((height_of(&mut ui) - 50.0).abs() <= 10.0, "{}", height_of(&mut ui));
    visible.set(true);
    height.set(200.dp());
    ui.advance_time_by_frame();
    assert!(present.get());
    ui.advance_time_by(Duration::from_millis(200));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert!(present.get());
    assert_eq!(height_of(&mut ui), 200.0);
}

/// Its text is drawn after the text's node has gone, and let go once the exit's over.
#[test]
fn a_captured_text_stays_for_as_long_as_it_s_drawn() {
    let visible = mutable_state_of(true);
    let handle = visible.handle();
    let mut ui = compose_test(move || {
        CapturedAnimatedVisibility!(handle.get(), exit = fade_out_with(tween(160), 0.0), {
            BasicText!("kept");
        });
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.on_node_with_text("kept").assert_does_not_exist();
    let paragraph = {
        let flat = ui.host().display_list();
        flat.ops.iter().find_map(|op| match op {
            DrawOp::DrawParagraph(paragraph, _) => Some(*paragraph),
            _ => None,
        })
    }
    .expect("the text, in its last frame");
    assert_eq!(
        ui.host().with_tree(|tree| tree.text_measurer_ref().text(paragraph).to_string()),
        "kept"
    );
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    // Gone: the next text gets its place.
    let next = ui.host().with_tree(|tree| {
        tree.text_measurer().layout(
            "next",
            &TextStyle::default(),
            compose_ui::platform::TextLayoutOptions::default(),
            Constraints::unbounded(),
            &Density::new(1.0),
        )
    });
    assert_eq!(next, paragraph);
}

/// What's in a repaint boundary inside it is in its last frame too, as that frame had it.
#[test]
fn a_captured_frame_has_the_groups_in_it() {
    let visible = mutable_state_of(true);
    let shade = mutable_state_of(Color::RED);
    let (handle, color) = (visible.handle(), shade.handle());
    let mut ui = compose_test(move || {
        CapturedAnimatedVisibility!(handle.get(), exit = fade_out_with(tween(160), 0.0), {
            Box!(
                modifier = Modifier.size(10.dp()).repaint_boundary().draw_behind(move |scope| {
                    let size = scope.size();
                    scope.draw_rect(
                        Rect::new(0.0, 0.0, size.width, size.height),
                        Paint::fill(color.get()),
                    );
                })
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(red_rects(&mut ui), 1);
    // Only the inner group changes, and the frame kept has it as it is now.
    shade.set(Color::BLUE);
    ui.wait_for_idle();
    assert_eq!(red_rects(&mut ui), 0);
    ui.set_auto_advance(false);
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let flat = ui.host().display_list();
    assert!(
        flat.ops
            .iter()
            .any(|op| matches!(op, DrawOp::DrawRect(_, paint) if paint.color == Color::BLUE)),
        "{}",
        flat.dump()
    );
}

/// Each exit draws the frame before it, not the first exit's.
#[test]
fn a_second_exit_draws_what_was_there_before_it() {
    let visible = mutable_state_of(true);
    let shade = mutable_state_of(Color::RED);
    let (handle, color) = (visible.handle(), shade.handle());
    let mut ui = compose_test(move || {
        CapturedAnimatedVisibility!(handle.get(), exit = fade_out_with(tween(160), 0.0), {
            Box!(modifier = Modifier.size(10.dp()).background(color.get()));
        });
    });
    ui.wait_for_idle();
    let exit = |ui: &mut ComposeTestRule, visible: &OwnedState<bool>| {
        ui.set_auto_advance(false);
        visible.set(false);
        ui.advance_time_by_frame();
        ui.advance_time_by_frame();
    };
    exit(&mut ui, &visible);
    assert_eq!(red_rects(&mut ui), 1);
    // Back, in another color, before that exit ends.
    visible.set(true);
    shade.set(Color::BLUE);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    exit(&mut ui, &visible);
    assert_eq!(red_rects(&mut ui), 0);
    let flat = ui.host().display_list();
    assert!(
        flat.ops
            .iter()
            .any(|op| matches!(op, DrawOp::DrawRect(_, paint) if paint.color == Color::BLUE)),
        "{}",
        flat.dump()
    );
}
