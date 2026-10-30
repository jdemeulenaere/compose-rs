//! Review of D230–D235: focus manager, IME actions, line bounds, `on_text_layout`.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{mutable_state_of, remember};
use compose_test::*;
use compose_ui::{FocusDirection, LOCAL_FOCUS_MANAGER, TextLayoutResult, UnitExt};

thread_local! {
    static OFFSET_ON_THE_TARGET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Focusable 20 dp boxes at the given dp offsets, tagged, in a Box.
fn boxes(
    spots: &'static [(&'static str, f32, f32)],
) -> (ComposeTestRule, compose_ui::FocusManager) {
    let manager = Rc::new(RefCell::new(None));
    let taken = manager.clone();
    let ui = compose_test(move || {
        *taken.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        Box!(modifier = Modifier.size(400.dp()), {
            for &(tag, x, y) in spots {
                if OFFSET_ON_THE_TARGET.with(|flag| flag.get()) {
                    Box!(
                        modifier =
                            Modifier.offset(x.dp(), y.dp()).size(20.dp()).focusable().test_tag(tag)
                    );
                } else {
                    // The offset on a parent: the focusable node's own bounds move.
                    Box!(modifier = Modifier.offset(x.dp(), y.dp()), {
                        Box!(modifier = Modifier.size(20.dp()).focusable().test_tag(tag));
                    });
                }
            }
        });
    });
    let manager = manager.borrow().clone().expect("composed");
    (ui, manager)
}

#[test]
fn horizontally_a_beam_candidate_beats_a_nearer_one_out_of_the_beam() {
    let (mut ui, manager) =
        boxes(&[("source", 0.0, 0.0), ("far_in_beam", 200.0, 0.0), ("near_diagonal", 30.0, 30.0)]);
    ui.on_node_with_tag("source").request_focus();
    manager.move_focus(FocusDirection::Right);
    ui.wait_for_idle();
    ui.on_node_with_tag("far_in_beam").assert_is_focused();
}

#[test]
fn vertically_a_near_candidate_out_of_the_beam_can_win() {
    // Compose's `beamBeats`, vertically: the beam candidate (180 away) isn't nearer than the
    // other's far edge (30), so the weighted distance decides.
    let (mut ui, manager) =
        boxes(&[("source", 0.0, 0.0), ("far_in_beam", 0.0, 200.0), ("near_diagonal", 30.0, 30.0)]);
    ui.on_node_with_tag("source").request_focus();
    manager.move_focus(FocusDirection::Down);
    ui.wait_for_idle();
    ui.on_node_with_tag("near_diagonal").assert_is_focused();
}

#[test]
fn equal_distances_go_to_the_first_in_order() {
    let (mut ui, manager) =
        boxes(&[("source", 50.0, 0.0), ("left", 20.0, 50.0), ("right", 80.0, 50.0)]);
    ui.on_node_with_tag("source").request_focus();
    manager.move_focus(FocusDirection::Down);
    ui.wait_for_idle();
    ui.on_node_with_tag("left").assert_is_focused();
}

#[test]
fn overlapping_candidates_count_when_they_reach_beyond() {
    let (mut ui, manager) = boxes(&[("source", 0.0, 0.0), ("overlapping", 10.0, 0.0)]);
    ui.on_node_with_tag("source").request_focus();
    manager.move_focus(FocusDirection::Right);
    ui.wait_for_idle();
    ui.on_node_with_tag("overlapping").assert_is_focused();
    manager.move_focus(FocusDirection::Left);
    ui.wait_for_idle();
    ui.on_node_with_tag("source").assert_is_focused();
}

#[test]
fn previous_from_no_focus_goes_to_the_last() {
    let (mut ui, manager) = boxes(&[("a", 0.0, 0.0), ("b", 30.0, 0.0), ("c", 60.0, 0.0)]);
    manager.move_focus(FocusDirection::Previous);
    ui.wait_for_idle();
    ui.on_node_with_tag("c").assert_is_focused();
    manager.clear_focus(true);
    ui.wait_for_idle();
    manager.move_focus(FocusDirection::Up);
    ui.wait_for_idle();
    // From the bottom right corner of the window.
    ui.on_node_with_tag("c").assert_is_focused();
}

#[test]
fn move_focus_in_rtl_uses_physical_directions() {
    let manager = Rc::new(RefCell::new(None));
    let taken = manager.clone();
    let mut ui = compose_test(move || {
        *taken.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl),
            || {
                Row!({
                    for tag in ["first", "second"] {
                        Box!(modifier = Modifier.size(20.dp()).focusable().test_tag(tag));
                    }
                });
            },
        );
    });
    let manager = manager.borrow().clone().unwrap();
    ui.on_node_with_tag("first").request_focus();
    // In RTL "second" is to the left of "first".
    manager.move_focus(FocusDirection::Left);
    ui.wait_for_idle();
    ui.on_node_with_tag("second").assert_is_focused();
}

fn height_of(ui: &mut ComposeTestRule, tag: &str) -> i32 {
    let bounds = ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds;
    (bounds.bottom - bounds.top).round() as i32
}

fn lines_high(lines: u32) -> i32 {
    (14.0f32 * 1.25 * lines as f32).ceil() as i32
}

#[test]
fn an_empty_text_with_min_lines_is_that_high() {
    let mut ui = compose_test(|| {
        Column!({
            BasicText!("", modifier = Modifier.test_tag("empty"));
            BasicText!("", modifier = Modifier.test_tag("empty3"), min_lines = 3);
        });
    });
    let empty = height_of(&mut ui, "empty");
    let three = height_of(&mut ui, "empty3");
    assert!((three - lines_high(3)).abs() <= 1, "{three}");
    assert!((empty - lines_high(1)).abs() <= 1, "an empty text is one line high: {empty}");
}

#[test]
fn min_lines_above_max_lines_is_rejected_as_compose_does() {
    // Compose: `require(minLines <= maxLines)` (validateMinMaxLines).
    let result = std::panic::catch_unwind(|| {
        let mut ui = compose_test(|| {
            BasicText!("x", modifier = Modifier.test_tag("t"), min_lines = 3, max_lines = 1);
        });
        height_of(&mut ui, "t")
    });
    assert!(result.is_err(), "accepted, {:?} px high", result.ok());
}

/// An unmemoized listener (a plain closure's lambda, or one capturing what can't be compared):
/// each recomposition re-lays out the paragraph (`constraints = None`) and re-reports the same
/// layout. Compose swaps the lambda and calls it only on a new layout.
#[test]
fn on_text_layout_is_not_heard_again_when_only_the_parent_recomposes() {
    let layouts = Rc::new(RefCell::new(0));
    let seen = layouts.clone();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let mut ui = compose_test(move || {
        let _ = handle.get();
        let seen = seen.clone();
        BasicText!("abc", on_text_layout = move |_: TextLayoutResult| *seen.borrow_mut() += 1);
    });
    assert_eq!(*layouts.borrow(), 1);
    tick.set(1);
    ui.wait_for_idle();
    tick.set(2);
    ui.wait_for_idle();
    assert_eq!(*layouts.borrow(), 1, "the same layout reported again");
}

#[test]
fn enter_runs_the_action_once_and_ime_action_on_a_read_only_field() {
    let log = Rc::new(RefCell::new(Vec::<&str>::new()));
    let done = log.clone();
    let mut ui = compose_test(move || {
        let done = done.clone();
        BasicTextField!(
            String::from("x"),
            |_: String| {},
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("f"),
            read_only = true,
            single_line = true,
            keyboard_actions =
                KeyboardActions::new().on_done(move || done.borrow_mut().push("done"))
        );
    });
    ui.on_node_with_tag("f").request_focus();
    assert!(ui.host().perform_ime_action(compose_ui::ImeAction::Default));
    assert_eq!(*log.borrow(), vec!["done"]);
}

#[test]
fn clearing_a_focused_fields_focus_reports_no_options() {
    let manager = Rc::new(RefCell::new(None));
    let taken = manager.clone();
    let mut ui = compose_test(move || {
        *taken.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        let text = remember(|| mutable_state_of(String::new()));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("f")
        );
    });
    let manager = manager.borrow().clone().unwrap();
    ui.on_node_with_tag("f").request_focus();
    assert!(ui.host().text_input_options().is_some());
    manager.clear_focus(false);
    ui.wait_for_idle();
    assert_eq!(ui.host().text_input_options(), None);
}

#[test]
fn two_dimensional_search_uses_the_focus_targets_bounds_not_the_layout_nodes() {
    // `Modifier.offset(…).focusable()`: Compose's `focusRect()` is the focus target's
    // coordinates, after the offset; the search here uses the layout node's outer bounds.
    OFFSET_ON_THE_TARGET.with(|flag| flag.set(true));
    let (mut ui, manager) = boxes(&[("source", 0.0, 0.0), ("right", 100.0, 0.0)]);
    ui.on_node_with_tag("source").request_focus();
    manager.move_focus(FocusDirection::Right);
    ui.wait_for_idle();
    ui.on_node_with_tag("right").assert_is_focused();
}

#[compose_runtime::composable]
fn Reporting(tick: u32, count: compose_runtime::MutableState<u32>) {
    let _ = tick;
    BasicText!("abc", on_text_layout = move |_: TextLayoutResult| count.update(|value| value + 1));
}

#[test]
fn on_text_layout_in_a_composable_is_heard_once_per_layout() {
    let count = mutable_state_of(0u32);
    let reads = count.handle();
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let mut ui = compose_test(move || {
        Reporting!(handle.get(), reads);
    });
    tick.set(1);
    ui.wait_for_idle();
    tick.set(2);
    ui.wait_for_idle();
    assert_eq!(count.get(), 1);
}

#[compose_runtime::composable]
fn ReportingText(text: String, count: compose_runtime::MutableState<u32>) {
    BasicText!(text, on_text_layout = move |_: TextLayoutResult| count.update(|value| value + 1));
}

#[test]
fn on_text_layout_hears_a_new_text_of_the_same_size() {
    let count = mutable_state_of(0u32);
    let reads = count.handle();
    let text = mutable_state_of(String::from("abc"));
    let handle = text.handle();
    let mut ui = compose_test(move || {
        ReportingText!(handle.get(), reads);
    });
    assert_eq!(count.get(), 1);
    text.set(String::from("abd"));
    ui.wait_for_idle();
    assert_eq!(count.get(), 2, "Compose calls onTextLayout for each new layout");
}

#[test]
fn a_lazy_item_reads_the_focus_manager_and_keyboard_controller() {
    let managers = Rc::new(RefCell::new(Vec::new()));
    let taken = managers.clone();
    let mut ui = compose_test(move || {
        let taken = taken.clone();
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), move |scope: LazyListScope| {
            let taken = taken.clone();
            scope.items(3, move |index| {
                taken.borrow_mut().push((
                    LOCAL_FOCUS_MANAGER.current(),
                    compose_ui::LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current().is_some(),
                ));
                Box!(
                    modifier = Modifier.size(20.dp()).focusable().test_tag(format!("item{index}"))
                );
            });
        });
    });
    let (manager, has_controller) = managers.borrow().first().cloned().expect("an item composed");
    assert!(has_controller);
    assert!(manager.move_focus(FocusDirection::Down));
    ui.wait_for_idle();
    ui.on_node_with_tag("item0").assert_is_focused();
}

/// A panicking background closure re-raises in the awaiting task: out of the host's frame (a
/// crash, as an uncaught exception in Compose's `LaunchedEffect`). Caught, the UI goes on.
#[test]
fn a_background_panic_surfaces_from_the_frame_and_the_ui_goes_on() {
    let text = mutable_state_of(String::from("a"));
    let handle = text.handle();
    let mut ui = compose_test(move || {
        compose_runtime::LaunchedEffect((), async move {
            compose_runtime::spawn_background(|| -> u32 { panic!("work failed") }).await;
        });
        BasicText!(handle.get());
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut surfaced = false;
    while std::time::Instant::now() < deadline {
        let step = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ui.wait_for_idle()));
        if step.is_err() {
            surfaced = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(surfaced);
    text.set(String::from("b"));
    ui.wait_for_idle();
    ui.on_node_with_text("b").assert_exists();
}
