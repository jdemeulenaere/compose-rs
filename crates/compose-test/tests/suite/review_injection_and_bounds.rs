//! Adversarial review of `ui-text-ports` (D2075, D2076), `foundation-text-ports` (D2077),
//! `ui-text-partials` (D2078, D2079) and `ui-text-partials-2` (D2080, `move_by_after`):
//! multi-pointer touch `up` and `cancel` with buffered pointer moves, lock keys across all
//! pointer dispatch paths (window hover exits included), bounds and touch-bounds assertions under
//! nested `LOCAL_DENSITY`, panic safety in `with_keys_down` and `with_keys_toggled`, mouse button
//! error ids and `move_by_after`, `has_text_exactly_including`, and coerced composition ranges on
//! multi-byte UTF-8 text.

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{CompositionLocalProvider, mutable_state_of};
use compose_test::*;
use compose_ui::platform::FixedTextMeasurer;
use compose_ui::{
    Density, DpSize, EditProcessor, EventContext, HorizontalAlignmentLine, IntSize, Key,
    LOCAL_DENSITY, LOCAL_VIEW_CONFIGURATION, Layout, Modifier, ModifierNode, ModifierNodeElement,
    Offset, PaddingValues, PointerEventPass, PointerEventType, PointerEvents,
    PointerInputModifierNode, PointerKeyboardModifiers, SemanticsConfiguration, TextFieldValue,
    TextRange, UnitExt, ViewConfiguration, measure_policy,
};

use super::dispatcher::{
    Change, Heard, ROOT, assert_has_valid_event_times, finger, heard, recording_root,
    recording_root_with_keys, touch,
};
use super::input::expect_error;

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

// ---- D2075: Touch up / cancel with buffered pointer moves and error messages -------------------

#[test]
fn an_up_flushes_updated_staying_and_lifted_pointers_in_one_release_event() {
    let (mut ui, log) = recording_root();
    let (p0, p1, p2) = (Offset::new(10.0, 10.0), Offset::new(20.0, 20.0), Offset::new(30.0, 30.0));
    let p0_moved = Offset::new(15.0, 18.0);
    let p1_delta = Offset::new(4.0, -2.0);
    ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
        scope.down_pointer(0, p0);
        scope.down_pointer(1, p1);
        scope.down_pointer(2, p2);
        scope.update_pointer_to(0, p0_moved);
        scope.update_pointer_by(1, p1_delta);
        scope.advance_event_time(16);
        scope.up_pointer(1);
        assert_eq!(scope.current_position(0), Some(p0_moved));
        assert_eq!(scope.current_position(1), None);
        assert_eq!(scope.current_position(2), Some(p2));
        scope.advance_event_time(16);
        scope.up_pointer(0);
        scope.up_pointer(2);
    });
    assert_has_valid_event_times(&log);
    let recorded = heard(&log);
    let down = |id: u64, at: Offset| -> Change { (finger(id), at, true) };
    let up = |id: u64, at: Offset| -> Change { (finger(id), at, false) };
    assert_eq!(
        recorded,
        [
            touch(PointerEventType::Press, 0, &[down(0, p0)]),
            touch(PointerEventType::Press, 0, &[down(0, p0), down(1, p1)]),
            touch(PointerEventType::Press, 0, &[down(0, p0), down(1, p1), down(2, p2)]),
            touch(
                PointerEventType::Release,
                16,
                &[down(0, p0_moved), up(1, p1 + p1_delta), down(2, p2)],
            ),
            touch(PointerEventType::Release, 32, &[up(0, p0_moved), down(2, p2)]),
            touch(PointerEventType::Release, 32, &[up(2, p2)]),
        ]
    );
}

/// A finger dragging a `draggable_2d` box whose final move is buffered with `update_pointer_to`
/// and flushed by another finger's `up_pointer` (D2075, D2130): single-change handlers still hear
/// the staying finger's move in the release event.
#[test]
fn an_up_carrying_another_fingers_move_drags_a_single_pointer_handler() {
    let dragged = mutable_state_of(Offset::ZERO);
    let handle = dragged.handle();
    let mut ui = compose_test(move || {
        let state = remember_draggable_2d_state!(move |delta: Offset| {
            handle.set(handle.get() + delta);
        });
        Box!(
            modifier = Modifier.test_tag("drag").size(200.dp()).draggable_2d(
                state,
                Draggable2DOptions {
                    start_drag_immediately: true,
                    ..Draggable2DOptions::default()
                }
            )
        );
    });
    ui.on_node_with_tag("drag").perform_touch_input(|scope| {
        scope.down_pointer(0, Offset::new(20.0, 20.0));
        scope.move_pointer_to(0, Offset::new(50.0, 30.0), 16);
        scope.down_pointer(1, Offset::new(100.0, 100.0));
        // Finger 0 moves another (25, 15) while finger 1 lifts: no `move_pointers` in between.
        scope.update_pointer_to(0, Offset::new(75.0, 45.0));
        scope.advance_event_time(16);
        scope.up_pointer(1);
        scope.up_pointer(0);
    });
    assert_eq!(dragged.get(), Offset::new(55.0, 25.0));
}

#[test]
fn a_cancel_discards_buffered_pointer_updates_without_sending_a_move() {
    let (mut ui, log) = recording_root();
    let (p0, p1) = (Offset::new(10.0, 10.0), Offset::new(20.0, 20.0));
    ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
        scope.down_pointer(0, p0);
        scope.down_pointer(1, p1);
        scope.update_pointer_to(0, Offset::new(80.0, 80.0));
        scope.update_pointer_by(1, Offset::new(30.0, 30.0));
        scope.cancel();
        assert_eq!(scope.current_position(0), None);
        assert_eq!(scope.current_position(1), None);
    });
    let recorded = heard(&log);
    assert_eq!(
        recorded,
        [
            touch(PointerEventType::Press, 0, &[(finger(0), p0, true)]),
            touch(PointerEventType::Press, 0, &[(finger(0), p0, true), (finger(1), p1, true)]),
            Heard::Cancel { millis: 16 },
        ]
    );
}

#[test]
fn a_cursor_interrupting_a_touch_with_buffered_updates_clears_them() {
    let (mut ui, log) = recording_root();
    let (p0, p1, p2) = (Offset::new(10.0, 10.0), Offset::new(50.0, 50.0), Offset::new(30.0, 30.0));
    ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
        scope.down_pointer(0, p0);
        scope.update_pointer_to(0, p1);
    });
    ui.on_node_with_tag(ROOT).perform_mouse_input(|mouse| {
        mouse.move_to(Offset::new(5.0, 5.0));
    });
    log.borrow_mut().clear();
    // Starting a new touch must not flush an empty move from the cancelled gesture.
    ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
        scope.down_pointer(0, p2);
    });
    let kinds: Vec<Option<PointerEventType>> = heard(&log).iter().map(Heard::kind).collect();
    assert_eq!(kinds, [Some(PointerEventType::Exit), Some(PointerEventType::Press)]);
}

#[test]
fn touch_error_messages_distinguish_no_gesture_from_inactive_pointer() {
    let (mut ui, _) = recording_root();
    let no_gesture_move = expect_error(|| {
        ui.on_node_with_tag(ROOT)
            .perform_touch_input(|scope| scope.update_pointer_to(7, Offset::ZERO));
    });
    assert!(
        no_gesture_move.contains("Cannot move pointers, no gesture is in progress"),
        "{no_gesture_move}"
    );
    let no_gesture_up = expect_error(|| {
        ui.on_node_with_tag(ROOT).perform_touch_input(|scope| scope.up_pointer(7));
    });
    assert!(
        no_gesture_up.contains("Cannot send UP event, no gesture is in progress"),
        "{no_gesture_up}"
    );
    ui.on_node_with_tag(ROOT).perform_touch_input(|scope| scope.down_pointer(0, Offset::ZERO));
    let inactive_move = expect_error(|| {
        ui.on_node_with_tag(ROOT)
            .perform_touch_input(|scope| scope.update_pointer_to(7, Offset::ZERO));
    });
    assert!(
        inactive_move.contains("Cannot move pointer 7, it is not active in the current gesture"),
        "{inactive_move}"
    );
    let inactive_up = expect_error(|| {
        ui.on_node_with_tag(ROOT).perform_touch_input(|scope| scope.up_pointer(7));
    });
    assert!(
        inactive_up.contains(
            "Cannot send UP event for pointer 7, it is not active in the current gesture"
        ),
        "{inactive_up}"
    );
}

// ---- D2076: Lock keys across mouse, touch and trackpad dispatch paths --------------------------

fn locks_and_shift(modifiers: PointerKeyboardModifiers) -> (bool, bool, bool, bool) {
    (
        modifiers.is_caps_lock_on(),
        modifiers.is_num_lock_on(),
        modifiers.is_scroll_lock_on(),
        modifiers.is_shift_pressed(),
    )
}

/// Records every pass-Main pointer event's kind and keyboard modifiers, including `Exit`.
#[derive(Clone)]
struct ModifiersRecorder(Rc<RefCell<Vec<(PointerEventType, PointerKeyboardModifiers)>>>);

struct ModifiersRecorderNode(Rc<RefCell<Vec<(PointerEventType, PointerKeyboardModifiers)>>>);

impl ModifierNodeElement for ModifiersRecorder {
    type Node = ModifiersRecorderNode;
    fn create(&self) -> ModifiersRecorderNode {
        ModifiersRecorderNode(self.0.clone())
    }
    fn update(&self, node: &mut ModifiersRecorderNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNode for ModifiersRecorderNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for ModifiersRecorderNode {
    fn takes_whole_events(&self) -> bool {
        true
    }

    fn on_pointer_events(
        &mut self,
        event: &PointerEvents,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main {
            self.0.borrow_mut().push((event.kind, event.keyboard_modifiers));
        }
    }
}

#[test]
fn mouse_enter_move_press_scroll_release_and_exit_all_carry_lock_keys() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorder = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        Box!(
            modifier = Modifier
                .size(200.dp())
                .then_custom(ModifiersRecorder(recorder.clone()))
                .focusable()
                .test_tag("target")
        );
    });
    ui.on_node_with_tag("target").request_focus();
    ui.on_node_with_tag("target").perform_key_input(|keys| {
        keys.press_key(Key::CapsLock);
        keys.press_key(Key::ScrollLock);
        keys.key_down(Key::ShiftLeft);
    });
    ui.on_node_with_tag("target").perform_mouse_input(|mouse| {
        mouse.enter(Offset::new(10.0, 10.0));
        mouse.move_to(Offset::new(20.0, 20.0));
        mouse.press(MouseButton::Primary);
        mouse.scroll(1.0, ScrollWheel::Vertical);
        mouse.release(MouseButton::Primary);
        mouse.exit(Offset::new(250.0, 250.0));
        // Also test leaving the window via `move_to` outside root bounds.
        mouse.move_to(Offset::new(30.0, 30.0));
        mouse.move_to(Offset::new(-5.0, -5.0));
    });
    let events = seen.borrow().clone();
    let kinds: Vec<PointerEventType> = events.iter().map(|(kind, _)| *kind).collect();
    assert!(kinds.contains(&PointerEventType::Enter), "{kinds:?}");
    assert!(kinds.contains(&PointerEventType::Move), "{kinds:?}");
    assert!(kinds.contains(&PointerEventType::Press), "{kinds:?}");
    assert!(kinds.contains(&PointerEventType::Scroll), "{kinds:?}");
    assert!(kinds.contains(&PointerEventType::Release), "{kinds:?}");
    assert_eq!(kinds.iter().filter(|&&k| k == PointerEventType::Exit).count(), 2, "{kinds:?}");
    for (kind, modifiers) in &events {
        assert_eq!(
            locks_and_shift(*modifiers),
            (true, false, true, true),
            "event {kind:?} lost active lock or modifier keys: {modifiers:?}"
        );
    }
}

#[test]
fn touch_down_move_and_up_carry_lock_keys_and_hover_exit_does_too() {
    let (mut ui, log) = recording_root_with_keys();
    ui.on_node_with_tag(ROOT).perform_key_input(|keys| {
        keys.press_key(Key::NumLock);
    });
    ui.on_node_with_tag(ROOT).perform_mouse_input(|mouse| {
        mouse.enter(Offset::new(10.0, 10.0));
    });
    log.borrow_mut().clear();
    ui.on_node_with_tag(ROOT).perform_touch_input(|touch| {
        touch.down(Offset::new(25.0, 25.0));
        touch.move_to(Offset::new(35.0, 35.0));
        touch.up();
    });
    let events = heard(&log);
    let kinds: Vec<Option<PointerEventType>> = events.iter().map(Heard::kind).collect();
    assert_eq!(
        kinds,
        [
            Some(PointerEventType::Exit),
            Some(PointerEventType::Press),
            Some(PointerEventType::Move),
            Some(PointerEventType::Release),
        ]
    );
    for event in &events {
        let modifiers = event.modifiers().expect("pointer event");
        assert_eq!(locks_and_shift(modifiers), (false, true, false, false), "{event:?}");
    }
}

#[test]
fn trackpad_enter_press_release_pan_scale_and_exit_carry_lock_keys() {
    let (mut ui, log) = recording_root_with_keys();
    ui.on_node_with_tag(ROOT).perform_key_input(|keys| {
        keys.press_key(Key::CapsLock);
        keys.press_key(Key::NumLock);
        keys.press_key(Key::ScrollLock);
    });
    log.borrow_mut().clear();
    ui.on_node_with_tag(ROOT).perform_trackpad_input(|trackpad| {
        trackpad.enter(Offset::new(40.0, 40.0));
        trackpad.press(TrackpadButton::Primary);
        trackpad.release(TrackpadButton::Primary);
        trackpad.pan(Offset::new(10.0, 5.0));
        trackpad.scale(1.5);
        trackpad.exit(Offset::new(1200.0, 1200.0));
    });
    let events = heard(&log);
    let kinds: Vec<Option<PointerEventType>> = events.iter().map(Heard::kind).collect();
    assert!(kinds.contains(&Some(PointerEventType::PanStart)), "{kinds:?}");
    assert!(kinds.contains(&Some(PointerEventType::PanMove)), "{kinds:?}");
    assert!(kinds.contains(&Some(PointerEventType::PanEnd)), "{kinds:?}");
    assert!(kinds.contains(&Some(PointerEventType::ScaleStart)), "{kinds:?}");
    assert!(kinds.contains(&Some(PointerEventType::ScaleChange)), "{kinds:?}");
    assert!(kinds.contains(&Some(PointerEventType::ScaleEnd)), "{kinds:?}");
    assert!(kinds.contains(&Some(PointerEventType::Exit)), "{kinds:?}");
    for event in &events {
        let modifiers = event.modifiers().expect("pointer event");
        assert_eq!(locks_and_shift(modifiers), (true, true, true, false), "{event:?}");
    }
}

// ---- D2078: Bounds, position, alignment line and touch bounds under nested LOCAL_DENSITY -------

#[test]
fn nested_local_density_converts_root_pixels_and_touch_bounds_with_the_nodes_own_density() {
    let line = HorizontalAlignmentLine::new(i32::max);
    let mut ui = compose_test_with(
        IntSize::new(600, 600),
        Density::new(2.0),
        Box::new(FixedTextMeasurer::new()),
        move || {
            // Outer padding at root density 2.0: (20 dp, 30 dp) = (40 px, 60 px).
            Box!(
                modifier = Modifier
                    .padding(PaddingValues { start: 20.dp(), top: 30.dp(), ..PaddingValues::ZERO })
                    .size((30.dp(), 40.dp()))
                    .clip_to_bounds()
                    .wrap_content_size_with(compose_ui::Alignment::TOP_START, true),
                {
                    // Child subtree at density 4.0: (40 px, 60 px) in root is (10 dp, 15 dp) in
                    // the child's own density, as Compose's `with(node.layoutInfo.density)`.
                    CompositionLocalProvider(
                        LOCAL_DENSITY.provides(Density::with_font_scale(4.0, 1.0)),
                        move || {
                            let configuration = ViewConfiguration {
                                minimum_touch_target_size: DpSize::new(24.dp(), 28.dp()),
                                ..LOCAL_VIEW_CONFIGURATION.current()
                            };
                            CompositionLocalProvider(
                                LOCAL_VIEW_CONFIGURATION.provides(configuration),
                                move || {
                                    let policy = measure_policy(move |scope, _, _| {
                                        // 8 dp at density 4.0 = 32 px; 20 dp × 12 dp = 80 px × 48 px.
                                        scope.provide_alignment_lines([(line, 32)]);
                                        scope.layout(80, 48)
                                    });
                                    Layout(
                                        Modifier.test_tag("nested").clickable(|| {}),
                                        policy,
                                        compose_runtime::callback(|| {}),
                                    );
                                },
                            );
                        },
                    );
                }
            );
        },
    );
    // Unclipped: top-left (40 px, 60 px) / 4.0 = (10 dp, 15 dp); size (80 px, 48 px) / 4.0 =
    // (20 dp, 12 dp). Outer clip is 30 dp × 40 dp at density 2.0 = 60 px × 80 px, so width clips
    // from 80 px to 60 px (= 15 dp in the child's density 4.0).
    ui.on_node_with_tag("nested")
        .assert_position_in_root_is_equal_to(10.dp(), 15.dp())
        .assert_left_position_in_root_is_equal_to(10.dp())
        .assert_top_position_in_root_is_equal_to(15.dp())
        .assert_width_is_equal_to(20.dp())
        .assert_height_is_equal_to(12.dp())
        .assert_touch_width_is_equal_to(24.dp())
        .assert_touch_height_is_equal_to(28.dp())
        .assert_touch_width_is_equal_to_with_tolerance(24.2.dp(), 0.25.dp())
        .assert_touch_height_is_equal_to_with_tolerance(27.8.dp(), 0.25.dp());
    let unclipped = ui.on_node_with_tag("nested").get_unclipped_bounds_in_root();
    assert_eq!(
        (unclipped.left, unclipped.top, unclipped.width(), unclipped.height()),
        (10.dp(), 15.dp(), 20.dp(), 12.dp())
    );
    let clipped = ui.on_node_with_tag("nested").get_bounds_in_root();
    assert_eq!(
        (clipped.left, clipped.top, clipped.width(), clipped.height()),
        (10.dp(), 15.dp(), 15.dp(), 12.dp())
    );
    let aligned = ui.on_node_with_tag("nested").get_alignment_line_position(line);
    aligned.assert_is_equal_to(8.dp(), "alignment line", 0.01.dp());
}

// ---- D2079: Panic safety in with_keys_down and with_keys_toggled -------------------------------

#[test]
fn with_keys_down_duplicate_key_panics_and_releases_the_first_key() {
    let (mut ui, _) = recording_root_with_keys();
    ui.on_node_with_tag(ROOT).perform_key_input(|keys| {
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            keys.with_keys_down(&[Key::ShiftLeft, Key::ShiftLeft], |_| {
                panic!("unreachable");
            });
        }))
        .expect_err("duplicate key must panic");
        let message = panic_message(err);
        assert!(message.contains("Key(ShiftLeft) is already pressed down."), "{message}");
        assert!(!keys.is_shift_down(), "ShiftLeft leaked down after mid-slice panic");
    });
}

#[test]
fn with_keys_down_panicking_on_already_held_second_key_releases_only_what_it_pressed() {
    let (mut ui, _) = recording_root_with_keys();
    ui.on_node_with_tag(ROOT).perform_key_input(|keys| {
        keys.key_down(Key::CtrlLeft);
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            keys.with_keys_down(&[Key::ShiftLeft, Key::CtrlLeft, Key::AltLeft], |_| {
                panic!("unreachable");
            });
        }))
        .expect_err("already held key must panic");
        let message = panic_message(err);
        assert!(message.contains("Key(CtrlLeft) is already pressed down."), "{message}");
        assert!(!keys.is_shift_down(), "ShiftLeft pressed before the panic must be released");
        assert!(keys.is_ctrl_down(), "CtrlLeft held before with_keys_down must stay held");
        assert!(!keys.is_alt_down());
        keys.key_up(Key::CtrlLeft);
    });
}

#[test]
fn with_keys_down_panicking_after_block_released_a_key_releases_the_rest_and_keeps_panic() {
    let (mut ui, _) = recording_root_with_keys();
    ui.on_node_with_tag(ROOT).perform_key_input(|keys| {
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            keys.with_keys_down(&[Key::ShiftLeft, Key::AltLeft], |keys| {
                keys.key_up(Key::ShiftLeft);
                panic!("block failure");
            });
        }))
        .expect_err("block panicked");
        assert_eq!(panic_message(err), "block failure");
        assert!(!keys.is_shift_down());
        assert!(!keys.is_alt_down(), "AltLeft must still be released when ShiftLeft was lifted");

        // Without a panic in the block, releasing one of `keys` inside the block still panics.
        let misuse = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            keys.with_key_down(Key::ShiftLeft, |keys| {
                keys.key_up(Key::ShiftLeft);
            });
        }))
        .expect_err("releasing held key without panicking must fail");
        assert!(panic_message(misuse).contains("Key(ShiftLeft) is not pressed down."));
    });
}

#[test]
fn with_keys_toggled_panicking_mid_setup_toggles_back_earlier_keys_only() {
    let (mut ui, _) = recording_root_with_keys();
    ui.on_node_with_tag(ROOT).perform_key_input(|keys| {
        keys.key_down(Key::NumLock);
        assert!(keys.is_num_lock_on());
        assert!(!keys.is_caps_lock_on());
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            keys.with_keys_toggled(&[Key::CapsLock, Key::NumLock, Key::ScrollLock], |_| {
                panic!("unreachable");
            });
        }))
        .expect_err("already held NumLock must panic");
        let message = panic_message(err);
        assert!(message.contains("Key(NumLock) is already pressed down."), "{message}");
        assert!(!keys.is_caps_lock_on(), "CapsLock toggled before panic must be toggled back off");
        assert!(keys.is_key_down(Key::NumLock), "pre-held NumLock must remain down");
        assert!(!keys.is_scroll_lock_on());
        keys.key_up(Key::NumLock);
    });
}

#[test]
fn with_keys_toggled_panicking_with_key_left_down_finishes_toggle_and_keeps_panic() {
    let (mut ui, _) = recording_root_with_keys();
    ui.on_node_with_tag(ROOT).perform_key_input(|keys| {
        // Starting with CapsLock off (0).
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            keys.with_keys_toggled(&[Key::CapsLock, Key::ScrollLock], |keys| {
                keys.key_down(Key::CapsLock);
                panic!("toggled block failure");
            });
        }))
        .expect_err("block panicked");
        assert_eq!(panic_message(err), "toggled block failure");
        assert!(!keys.is_key_down(Key::CapsLock));
        assert!(!keys.is_caps_lock_on());
        assert!(!keys.is_scroll_lock_on());

        // Starting with CapsLock already on (2).
        keys.press_key(Key::CapsLock);
        assert!(keys.is_caps_lock_on());
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            keys.with_key_toggled(Key::CapsLock, |keys| {
                assert!(!keys.is_caps_lock_on());
                keys.key_down(Key::CapsLock);
                panic!("toggled from on failure");
            });
        }))
        .expect_err("block panicked");
        assert_eq!(panic_message(err), "toggled from on failure");
        assert!(!keys.is_key_down(Key::CapsLock));
        assert!(keys.is_caps_lock_on(), "CapsLock should return to on");
    });
}

// ---- D2080, move_by_after and has_text_exactly_including ---------------------------------------

#[test]
fn mouse_button_ids_and_error_messages_match_compose_for_all_three_buttons() {
    assert_eq!(MouseButton::Primary.button_id(), 1);
    assert_eq!(MouseButton::Secondary.button_id(), 2);
    assert_eq!(MouseButton::Tertiary.button_id(), 4);
    let (mut ui, _) = recording_root();
    for (button, id) in
        [(MouseButton::Primary, 1), (MouseButton::Secondary, 2), (MouseButton::Tertiary, 4)]
    {
        let up_err = expect_error(|| {
            ui.on_node_with_tag(ROOT).perform_mouse_input(|mouse| mouse.release(button));
        });
        assert!(
            up_err.contains(&format!(
                "Cannot send mouse button up event, button {id} is not pressed"
            )),
            "{up_err}"
        );
        let down_err = expect_error(|| {
            ui.on_node_with_tag(ROOT).perform_mouse_input(|mouse| {
                mouse.press(button);
                mouse.press(button);
            });
        });
        assert!(
            down_err.contains(&format!(
                "Cannot send mouse button down event, button {id} is already pressed"
            )),
            "{down_err}"
        );
        ui.on_node_with_tag(ROOT).perform_mouse_input(|mouse| mouse.release(button));
    }
}

#[test]
fn mouse_move_by_after_accumulates_deltas_and_timestamps_in_local_coordinates() {
    let seen: Rc<RefCell<Vec<(PointerEventType, Offset, u64)>>> = Rc::default();
    let log = seen.clone();
    struct OffsetRecorder(Rc<RefCell<Vec<(PointerEventType, Offset, u64)>>>);
    struct OffsetRecorderNode(Rc<RefCell<Vec<(PointerEventType, Offset, u64)>>>);
    impl ModifierNodeElement for OffsetRecorder {
        type Node = OffsetRecorderNode;
        fn create(&self) -> OffsetRecorderNode {
            OffsetRecorderNode(self.0.clone())
        }
        fn update(&self, node: &mut OffsetRecorderNode) {
            node.0 = self.0.clone();
        }
        fn same(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }
    }
    impl ModifierNode for OffsetRecorderNode {
        fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
            Some(self)
        }
    }
    impl PointerInputModifierNode for OffsetRecorderNode {
        fn takes_whole_events(&self) -> bool {
            true
        }
        fn on_pointer_events(
            &mut self,
            event: &PointerEvents,
            pass: PointerEventPass,
            _bounds: IntSize,
            _context: &mut EventContext<'_>,
        ) {
            if pass == PointerEventPass::Main {
                let change = &event.changes[0];
                self.0.borrow_mut().push((event.kind, change.position, change.uptime_millis));
            }
        }
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let log = log.clone();
        Box!(
            modifier = Modifier.padding(PaddingValues {
                start: 30.dp(),
                top: 40.dp(),
                ..PaddingValues::ZERO
            }),
            {
                let log = log.clone();
                Box!(
                    modifier =
                        Modifier.size(100.dp()).then_custom(OffsetRecorder(log)).test_tag("box")
                );
            }
        );
    });
    let t0 = ui.current_time_millis();
    ui.on_node_with_tag("box").perform_mouse_input(|mouse| {
        // Starts at root (0, 0) = local (-30, -40).
        assert_eq!(mouse.current_position(), Offset::new(-30.0, -40.0));
        mouse.move_by_after(Offset::new(40.0, 55.0), 0);
        assert_eq!(mouse.current_position(), Offset::new(10.0, 15.0));
        mouse.move_by_after(Offset::new(5.0, -5.0), 25);
        assert_eq!(mouse.current_position(), Offset::new(15.0, 10.0));
        mouse.move_by_after(Offset::new(-3.0, 8.0), 40);
        assert_eq!(mouse.current_position(), Offset::new(12.0, 18.0));
    });
    let events: Vec<(PointerEventType, Offset, u64)> =
        seen.borrow().iter().map(|&(k, p, t)| (k, p, t - t0)).collect();
    assert_eq!(
        events,
        [
            (PointerEventType::Enter, Offset::new(10.0, 15.0), 0),
            (PointerEventType::Move, Offset::new(15.0, 10.0), 25),
            (PointerEventType::Move, Offset::new(12.0, 18.0), 65),
        ]
    );
}

#[test]
fn has_text_exactly_including_distinguishes_text_editable_text_and_input_text() {
    let mut ui = compose_test(|| {
        Column!(
            modifier =
                Modifier.test_tag("field").semantics(|config: &mut SemanticsConfiguration| {
                    config.set_text("label");
                    config.set_editable_text("•••");
                    config.set_input_text("secret");
                }),
            {}
        );
    });
    ui.on_node_with_tag("field")
        .assert_text_equals(&["label", "•••"])
        .assert_text_equals_with(&["label"], false)
        .assert_text_equals_including(&["label", "•••", "secret"], true, true)
        .assert_text_equals_including(&["label", "secret"], false, true);
    expect_error(|| {
        ui.on_node_with_tag("field").assert_text_equals(&["label", "•••", "secret"]);
    });
}

#[test]
fn merged_content_descriptions_do_not_match_joined_content_description() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.semantics_with(true, |_| {}), {
            Box!(modifier = Modifier.semantics(|config| config.set_content_description("Hello")));
            Box!(modifier = Modifier.semantics(|config| config.set_content_description("World")));
        });
    });
    expect_error(|| {
        ui.on_node_with_content_description("Hello, World").assert_exists();
    });
    expect_error(|| {
        ui.on_node_with_content_description("Hello World").assert_exists();
    });
}

// ---- D2077: Coerced collapsed composition ranges and UTF-8 character boundary safety -----------

#[test]
fn text_field_value_keeps_collapsed_composition_and_clamps_to_utf8_char_boundaries() {
    // 'a' (0..1), 'é' (1..3), '🙂' (3..7), 'b' (7..8): 8 UTF-8 bytes.
    let text = "aé🙂b";
    assert_eq!(text.len(), 8);

    // Out-of-bounds collapsed and non-collapsed compositions coerce to collapsed `8..8` and stay.
    for range in [TextRange::at(99), TextRange::new(12, 20), TextRange::at(0)] {
        let value = TextFieldValue::new(text, TextRange::at(99), Some(range));
        let composition = value.composition().expect("collapsed composition stays");
        assert!(composition.collapsed(), "{composition:?}");
        assert!(text.is_char_boundary(composition.start));
        assert!(text.is_char_boundary(composition.end));
    }

    // Every arbitrary byte pair `(start, end)` lands on valid UTF-8 boundaries and never panics
    // when slicing or rendering inside `BasicTextField`.
    for start in 0..=10 {
        for end in 0..=10 {
            let value = TextFieldValue::new(
                text,
                TextRange::new(start, end),
                Some(TextRange::new(end, start)),
            );
            let sel = value.selection();
            let comp = value.composition().expect("composition present");
            assert!(text.is_char_boundary(sel.start), "{start}..{end} -> {sel:?}");
            assert!(text.is_char_boundary(sel.end), "{start}..{end} -> {sel:?}");
            assert!(text.is_char_boundary(comp.start), "{end}..{start} -> {comp:?}");
            assert!(text.is_char_boundary(comp.end), "{end}..{start} -> {comp:?}");
            let _ = value.selected_text();
            let _ = value.text_before_selection(3);
            let _ = value.text_after_selection(3);
            let _ = comp.substring(value.text());
        }
    }

    // Passing a coerced collapsed composition on multi-byte UTF-8 text into `EditProcessor` and
    // `BasicTextFieldOfState` never panics and keeps valid boundaries.
    let initial = TextFieldValue::new(text, TextRange::at(5), Some(TextRange::at(50)));
    assert_eq!(initial.composition(), Some(TextRange::at(8)));
    let mut processor = EditProcessor::from_value(initial.clone());
    processor.reset(initial);
    let mut ui = compose_test(move || {
        let state = remember_text_field_state(text, Some(TextRange::new(2, 5)));
        BasicTextFieldOfState!(state, modifier = Modifier.test_tag("utf8-field"));
    });
    ui.on_node_with_tag("utf8-field").assert_text_equals(&[text]);
}

#[test]
fn multi_touch_swipe_with_velocity_validates_arguments_and_lifts_all_fingers() {
    let (mut ui, log) = recording_root();
    let empty_err = expect_error(|| {
        ui.on_node_with_tag(ROOT)
            .perform_touch_input(|scope| scope.multi_touch_swipe_with_velocity(&[], None, &[]));
    });
    assert!(empty_err.contains("Swipes list cannot be empty"), "{empty_err}");

    let neg_err = expect_error(|| {
        ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
            scope.multi_touch_swipe_with_velocity(
                &[SwipeInfo {
                    start: Offset::ZERO,
                    end: Offset::new(100.0, 0.0),
                    end_velocity: -1.0,
                }],
                None,
                &[],
            );
        });
    });
    assert!(neg_err.contains("Velocity cannot be -1, it must be positive"), "{neg_err}");

    let short_err = expect_error(|| {
        ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
            scope.multi_touch_swipe_with_velocity(
                &[SwipeInfo {
                    start: Offset::ZERO,
                    end: Offset::new(100.0, 0.0),
                    end_velocity: 100.0,
                }],
                Some(39),
                &[],
            );
        });
    });
    assert!(short_err.contains("Duration must be at least 40ms"), "{short_err}");

    let swipes = [
        SwipeInfo {
            start: Offset::new(10.0, 10.0),
            end: Offset::new(110.0, 10.0),
            end_velocity: 500.0,
        },
        SwipeInfo {
            start: Offset::new(10.0, 80.0),
            end: Offset::new(10.0, 280.0),
            end_velocity: 0.0,
        },
    ];
    ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
        scope.multi_touch_swipe_with_velocity(&swipes, Some(100), &[50]);
        assert_eq!(scope.current_position(0), None);
        assert_eq!(scope.current_position(1), None);
    });
    assert_has_valid_event_times(&log);
    let events = heard(&log);
    assert!(events.iter().any(|e| e.millis() == 50), "key_time 50ms hit: {events:?}");
    assert_eq!(events.last().unwrap().millis(), 100);
}

#[test]
fn paragraph_cut_by_max_height_distinguishes_ellipsis_and_clip_in_bounding_boxes() {
    use compose_ui::platform::{FixedFontMetrics, TextLayoutOptions, TextMeasurer};
    let mut measurer = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
    let style = compose_ui::TextStyle {
        font_size: 50.0.sp(),
        text_direction: Some(compose_ui::TextDirection::Content),
        ..compose_ui::TextStyle::DEFAULT
    };
    let lay_out =
        |measurer: &mut FixedTextMeasurer, max_lines: u32, overflow: compose_ui::TextOverflow| {
            let handle = measurer.layout_annotated(
                &compose_ui::AnnotatedString::new("abc\ndef"),
                &style,
                TextLayoutOptions { max_lines, overflow, ..TextLayoutOptions::default() },
                compose_ui::Constraints::new(200, 200, 0, 50),
                &Density::new(1.0),
            );
            measurer.paragraph(handle)
        };
    // Ellipsized at height 50 px: `get_line_end(0, false)` reaches the end of the text, so
    // `fill_bounding_boxes` writes empty boxes at the line's end `[150, 0, 150, 50]` for "def".
    let ellipsized = lay_out(&mut measurer, u32::MAX, compose_ui::TextOverflow::Ellipsis);
    assert_eq!(ellipsized.line_count(), 1);
    assert!(ellipsized.is_line_ellipsized(0));
    assert_eq!(ellipsized.get_bounding_boxes_for_range(4, 7), Vec::new());
    let mut boxes = [-1.0f32; 28];
    ellipsized.fill_bounding_boxes(TextRange::new(0, 7), &mut boxes, 0);
    assert_eq!(&boxes[0..4], &[0.0, 0.0, 50.0, 50.0]);
    assert_eq!(
        &boxes[16..28],
        &[150.0, 0.0, 150.0, 50.0, 150.0, 0.0, 150.0, 50.0, 150.0, 0.0, 150.0, 50.0]
    );

    // Clipped at max_lines 1: `get_line_end(0, false)` stops at line 0 ("abc\n"), so characters
    // past the cut ("def") leave `array` untouched; at max_height 50 with `Clip` and unbounded
    // `max_lines`, both lines are laid out.
    let height_clipped = lay_out(&mut measurer, u32::MAX, compose_ui::TextOverflow::Clip);
    assert_eq!(height_clipped.line_count(), 2);
    let line_clipped = lay_out(&mut measurer, 1, compose_ui::TextOverflow::Clip);
    assert_eq!(line_clipped.line_count(), 1);
    assert!(!line_clipped.is_line_ellipsized(0));
    let mut clipped_boxes = [-1.0f32; 28];
    line_clipped.fill_bounding_boxes(TextRange::new(0, 7), &mut clipped_boxes, 0);
    assert_eq!(&clipped_boxes[0..4], &[0.0, 0.0, 50.0, 50.0]);
    assert_eq!(&clipped_boxes[16..28], &[-1.0; 12]);
}
