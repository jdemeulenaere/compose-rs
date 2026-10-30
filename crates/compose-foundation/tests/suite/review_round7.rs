//! Seventh review round: `pointer_input`, its detectors, `draggable` and `TooltipArea`, against
//! Jetpack Compose and Compose Desktop. Each test states the input, what Compose does, and what
//! happens instead.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{
    DragGestures, IntSize, Offset, PointerEvent, PointerEventPass, PointerEventType, PointerId,
    PointerType, Popup, TapGestures, UnitExt,
};

type Log = Rc<RefCell<Vec<String>>>;

fn send(ui: &mut ComposeTestRule, event: PointerEvent) {
    ui.host().send_pointer(event);
    ui.wait_for_idle();
}

fn press(ui: &mut ComposeTestRule, x: f32, y: f32) {
    send(ui, PointerEvent::press(Offset::new(x, y)));
}

fn release(ui: &mut ComposeTestRule, x: f32, y: f32) {
    send(ui, PointerEvent::release(Offset::new(x, y)));
}

fn drag_to(ui: &mut ComposeTestRule, x: f32, y: f32) {
    send(ui, PointerEvent::moved(Offset::new(x, y), true));
}

fn finger(event: PointerEvent, id: u64) -> PointerEvent {
    event.with_pointer(PointerId(id), PointerType::Touch)
}

fn tap_detector(log: &Log, double: bool, long: bool) -> TapGestures {
    let (tap, double_tap, long_press) = (log.clone(), log.clone(), log.clone());
    let mut gestures =
        TapGestures::default().on_tap(move |at| tap.borrow_mut().push(format!("tap {}", at.x)));
    if double {
        gestures = gestures.on_double_tap(move |at| {
            double_tap.borrow_mut().push(format!("double {}", at.x));
        });
    }
    if long {
        gestures = gestures.on_long_press(move |at| {
            long_press.borrow_mut().push(format!("long {}", at.x));
        });
    }
    gestures
}

// ---- pointer_input -------------------------------------------------------------------------

/// A block that returns after the first press. Compose launches a `pointerInput` block once
/// (`pointerInputJob` stays set once it completes) and only a key change, a cancellation or a
/// detach starts it again: one press is logged. Here a finished block is launched again at the
/// next event, so each press is logged.
#[test]
fn a_completed_block_is_not_started_again() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let seen = seen.clone();
                async move {
                    let down = scope.await_first_down(true).await;
                    seen.borrow_mut().push(format!("down {}", down.position.x));
                }
            })
        );
    });
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    press(&mut ui, 20.0, 10.0);
    release(&mut ui, 20.0, 10.0);
    assert_eq!(*log.borrow(), vec!["down 10"]);
}

/// A mouse moving over a block that awaits the `Initial` pass. Compose sends a node's `Enter`
/// and `Exit` through all three passes, as any event; here they're sent in `Main` only, so a
/// block watching `Initial` (or `Final`) never sees them.
#[test]
fn enter_and_exit_reach_a_block_awaiting_the_initial_pass() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let seen = seen.clone();
                async move {
                    loop {
                        let change = scope.await_pointer_event(PointerEventPass::Initial).await;
                        if matches!(change.kind, PointerEventType::Enter | PointerEventType::Exit) {
                            seen.borrow_mut().push(format!("{:?}", change.kind));
                        }
                    }
                }
            })
        );
    });
    send(&mut ui, PointerEvent::moved(Offset::new(10.0, 10.0), false));
    send(&mut ui, PointerEvent::moved(Offset::new(150.0, 150.0), false));
    assert_eq!(*log.borrow(), vec!["Enter", "Exit"]);
}

// ---- detect_tap_gestures -------------------------------------------------------------------

/// A tap target inside a vertical `draggable`: pressed, dragged 40 px down (the parent drags,
/// consuming the moves in its `Main` pass, after the child's) and released over the target.
/// Compose's `waitForUpOrCancellation` checks the `Final` pass of each move and cancels once a
/// parent consumed it: no tap (a button in a scrolling list doesn't click when scrolled).
/// Here only the `Main` pass is looked at, before the parent consumes: it taps.
#[test]
fn a_tap_is_cancelled_when_a_parent_consumes_the_moves() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let seen = seen.clone();
        let state = remember_draggable_state!(|_| {});
        Box!(
            modifier = Modifier.size(200.dp()).draggable(
                state,
                Orientation::Vertical,
                true,
                false,
                |_| {},
                |_| {}
            ),
            {
                let seen = seen.clone();
                Box!(
                    modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                        let gestures = tap_detector(&seen, false, false);
                        async move { scope.detect_tap_gestures(gestures).await }
                    })
                );
            }
        );
    });
    press(&mut ui, 10.0, 10.0);
    drag_to(&mut ui, 10.0, 30.0);
    drag_to(&mut ui, 10.0, 50.0);
    release(&mut ui, 10.0, 50.0);
    assert_eq!(*log.borrow(), Vec::<String>::new());
}

/// Two fingers on a tap target: A down, B down, B up, then A up. Compose's
/// `waitForUpOrCancellation` returns once *all* pointers are up, so the tap comes at A's
/// release, not B's. Here the first release of any pointer ends the press: B's release taps
/// while A still touches.
#[test]
fn lifting_a_second_finger_does_not_tap() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let gestures = tap_detector(&seen, false, false);
                async move { scope.detect_tap_gestures(gestures).await }
            })
        );
    });
    send(&mut ui, finger(PointerEvent::press(Offset::new(10.0, 10.0)), 1));
    send(&mut ui, finger(PointerEvent::press(Offset::new(60.0, 10.0)), 2));
    send(&mut ui, finger(PointerEvent::release(Offset::new(60.0, 10.0)), 2));
    assert_eq!(*log.borrow(), Vec::<String>::new(), "finger 1 still touches");
    send(&mut ui, finger(PointerEvent::release(Offset::new(10.0, 10.0)), 1));
    assert_eq!(*log.borrow(), vec!["tap 10"]);
}

/// With `on_double_tap`: a tap, then a second press 100 ms later that moves out of the target
/// (cancelled) and is released there. Compose's `detectTapGestures` reports the first tap when
/// the second one is cancelled (`onTap(upOrCancel.position)`). Here nothing is reported: the
/// first tap is lost.
#[test]
fn a_cancelled_second_tap_still_reports_the_first() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let gestures = tap_detector(&seen, true, false);
                async move { scope.detect_tap_gestures(gestures).await }
            })
        );
    });
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(100));
    press(&mut ui, 10.0, 10.0);
    drag_to(&mut ui, 150.0, 10.0);
    release(&mut ui, 150.0, 10.0);
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(*log.borrow(), vec!["tap 10"]);
}

/// With `on_double_tap` and `on_long_press`: a tap, then a second press 100 ms later held for
/// 450 ms. Compose waits for the second release within the long-press timeout; past it, it
/// reports the first tap and then a long press (`onTap(upOrCancel.position)`,
/// `onLongPress(secondDown.position)`). Here the second press waits without a timeout: nothing
/// is reported while it's held (and its release later double-taps).
#[test]
fn a_long_second_press_taps_then_long_presses() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let gestures = tap_detector(&seen, true, true);
                async move { scope.detect_tap_gestures(gestures).await }
            })
        );
    });
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(100));
    press(&mut ui, 20.0, 10.0);
    ui.advance_time_by(Duration::from_millis(450));
    assert_eq!(*log.borrow(), vec!["tap 10", "long 20"]);
    release(&mut ui, 20.0, 10.0);
    assert_eq!(*log.borrow(), vec!["tap 10", "long 20"], "no double tap after a long press");
}

/// With `on_double_tap`: two presses 10 ms apart. Compose's `awaitSecondDown` ignores a second
/// press sooner than `doubleTapMinTimeMillis` (40 ms) after the first release, so this is one
/// tap, reported at the double-tap timeout. Here it's a double tap.
#[test]
fn a_second_press_sooner_than_40ms_is_not_a_double_tap() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let gestures = tap_detector(&seen, true, false);
                async move { scope.detect_tap_gestures(gestures).await }
            })
        );
    });
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(10));
    press(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(10));
    release(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(*log.borrow(), vec!["tap 10"]);
}

// ---- detect_drag_gestures and draggable ----------------------------------------------------

/// Pressed at x 10 and moved to x 30 (past the 8 px slop). Compose's `detectDragGestures` calls
/// `onDrag(change, overSlop)`: the movement less the slop along its direction, 12 px (as
/// `draggable` here already does). Here the first amount is the whole 20 px, so a drag moves
/// its content 8 px more than Compose's.
#[test]
fn the_first_drag_amount_excludes_the_touch_slop() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let drag = seen.clone();
                async move {
                    scope
                        .detect_drag_gestures(DragGestures::default().on_drag(move |_, amount| {
                            drag.borrow_mut().push(format!("drag {}", amount.x))
                        }))
                        .await
                }
            })
        );
    });
    send(&mut ui, finger(PointerEvent::press(Offset::new(10.0, 10.0)), 1));
    send(&mut ui, finger(PointerEvent::moved(Offset::new(30.0, 10.0), true), 1));
    send(&mut ui, finger(PointerEvent::moved(Offset::new(40.0, 10.0), true), 1));
    send(&mut ui, finger(PointerEvent::release(Offset::new(40.0, 10.0)), 1));
    assert_eq!(*log.borrow(), vec!["drag 12", "drag 10"]);
}

/// A horizontal `draggable` inside a vertical one. Pressed, moved 40 px down (the parent takes
/// the drag, consuming in its `Main` pass), then 40 px right. Compose's slop detector checks
/// the `Final` pass while waiting for the slop: the child sees the parent consumed the move and
/// gives up the gesture, so the child never drags. Here the child, still waiting, takes the
/// horizontal move (32 px past its slop), and the parent's drag stops.
#[test]
fn a_child_drag_gives_up_once_its_parent_took_the_drag() {
    let child = Rc::new(Cell::new(0.0f32));
    let parent = Rc::new(Cell::new(0.0f32));
    let (child_moved, parent_moved) = (child.clone(), parent.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (child_moved, parent_moved) = (child_moved.clone(), parent_moved.clone());
        let outer =
            remember_draggable_state!(move |delta| parent_moved.set(parent_moved.get() + delta));
        let inner =
            remember_draggable_state!(move |delta| child_moved.set(child_moved.get() + delta));
        Box!(
            modifier = Modifier.size(200.dp()).draggable(
                outer,
                Orientation::Vertical,
                true,
                false,
                |_| {},
                |_| {}
            ),
            {
                Box!(
                    modifier = Modifier.size(150.dp()).draggable(
                        inner.clone(),
                        Orientation::Horizontal,
                        true,
                        false,
                        |_| {},
                        |_| {}
                    )
                );
            }
        );
    });
    press(&mut ui, 20.0, 20.0);
    drag_to(&mut ui, 20.0, 60.0);
    assert_eq!(parent.get(), 40.0 - 8.0 / 144.0);
    drag_to(&mut ui, 60.0, 60.0);
    drag_to(&mut ui, 60.0, 80.0);
    release(&mut ui, 60.0, 80.0);
    assert_eq!(child.get(), 0.0, "the child gave up the gesture");
    assert_eq!(parent.get(), 60.0 - 8.0 / 144.0, "the parent's drag went on");
}

/// A horizontal `draggable` dragged by finger 1 (x 10 to 40: 22 px past the slop), then
/// finger 2 touches at x 120 and moves to x 130. Compose's draggable follows the pointer that
/// started the drag (its id): finger 2 changes nothing. Here every pointer's moves are the
/// drag's, measured from finger 1's last position: finger 2's move adds 90 px.
#[test]
fn a_second_finger_does_not_move_a_drag() {
    let offset = Rc::new(Cell::new(0.0f32));
    let moved = offset.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let moved = moved.clone();
        let state = remember_draggable_state!(move |delta| moved.set(moved.get() + delta));
        Box!(
            modifier = Modifier.size(200.dp()).draggable(
                state,
                Orientation::Horizontal,
                true,
                false,
                |_| {},
                |_| {}
            )
        );
    });
    send(&mut ui, finger(PointerEvent::press(Offset::new(10.0, 10.0)), 1));
    send(&mut ui, finger(PointerEvent::moved(Offset::new(40.0, 10.0), true), 1));
    assert_eq!(offset.get(), 22.0);
    send(&mut ui, finger(PointerEvent::press(Offset::new(120.0, 10.0)), 2));
    send(&mut ui, finger(PointerEvent::moved(Offset::new(130.0, 10.0), true), 2));
    assert_eq!(offset.get(), 22.0);
}

/// A `draggable` dragging when its gesture is cancelled (the window loses the pointer:
/// `cancel_pointers`). Compose's drag node turns the cancelled pointer input coroutine into
/// `DragCancelled`, which calls `onDragStopped`: whoever tracks "dragging" hears it ended. Here
/// the block is dropped silently: `on_drag_stopped` is never called.
#[test]
fn a_cancelled_drag_calls_on_drag_stopped() {
    let stopped = Rc::new(Cell::new(0));
    let stop = stopped.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let stop = stop.clone();
        let state = remember_draggable_state!(|_| {});
        Box!(
            modifier = Modifier.size(200.dp()).draggable(
                state,
                Orientation::Horizontal,
                true,
                false,
                |_| {},
                move |_| stop.set(stop.get() + 1)
            )
        );
    });
    press(&mut ui, 10.0, 10.0);
    drag_to(&mut ui, 40.0, 10.0);
    ui.host().with_tree(|tree| tree.cancel_pointers());
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
}

/// A `draggable` whose `on_drag_stopped` captures a value that changes (a recomposition) while
/// it's dragged. Compose's `DraggableNode.update` takes the new callbacks, so the release calls
/// the newest one (as `remember_draggable_state` here does for `on_delta`): "stopped b". Here
/// the running block keeps the callbacks it started with: "stopped a".
#[test]
fn draggable_calls_its_newest_callbacks() {
    let log: Log = Rc::default();
    let label = compose_runtime::mutable_state_of("a");
    let (seen, read) = (log.clone(), *label);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let current = read.get();
        let seen = seen.clone();
        let state = remember_draggable_state!(|_| {});
        Box!(
            modifier = Modifier.size(200.dp()).draggable(
                state,
                Orientation::Horizontal,
                true,
                false,
                |_| {},
                move |_| seen.borrow_mut().push(format!("stopped {current}"))
            )
        );
    });
    press(&mut ui, 10.0, 10.0);
    drag_to(&mut ui, 40.0, 10.0);
    label.set("b");
    ui.wait_for_idle();
    release(&mut ui, 40.0, 10.0);
    assert_eq!(*log.borrow(), vec!["stopped b"]);
}

// ---- TooltipArea ---------------------------------------------------------------------------

/// A 300 px area whose 100 x 50 tooltip shows at (50, 66) under a pointer resting at (50, 50);
/// the pointer then moves onto the tooltip at (60, 80), still inside the area. Compose
/// Desktop's area hides on `Exit` only if the pointer left the area's bounds
/// (`hideIfNotHovered`), and the tooltip's own moves check the same: the tooltip stays. Here
/// the area hears `Exit` (the popup takes the pointer) and hides it; it comes back 500 ms later
/// elsewhere (below the pointer's new place).
#[test]
fn the_tooltip_stays_while_the_pointer_is_on_it_inside_the_area() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        TooltipArea!(|| Box!(modifier = Modifier.size((100.dp(), 50.dp())).test_tag("tooltip")), {
            Box!(modifier = Modifier.size(300.dp()).test_tag("area"));
        });
    });
    send(&mut ui, PointerEvent::moved(Offset::new(50.0, 50.0), false));
    ui.advance_time_by(Duration::from_millis(600));
    let tooltip = ui.on_node_with_tag("tooltip").fetch_semantics_snapshot().bounds;
    assert_eq!((tooltip.left, tooltip.top), (50.0, 66.0));
    send(&mut ui, PointerEvent::moved(Offset::new(60.0, 80.0), false));
    ui.advance_time_by(Duration::from_millis(50));
    ui.on_node_with_tag("tooltip").assert_exists();
}

/// The pointer rests at (50, 780) in an 800 px high window, over an area at its bottom; the
/// tooltip is 50 px high, so it doesn't fit 16 px below the pointer. Compose Desktop's
/// `TooltipPlacement.CursorPoint` (`PopupPositionProviderAtPosition`) then puts it above the
/// pointer, 16 px away: top 780 - 16 - 50 = 714. Here it's pushed up into the window to
/// top 750, over the pointer itself: the pointer then hovers the tooltip, the area hears
/// `Exit` and hides it at once, so the tooltip is never seen (with the pointer at y 720 it
/// shows, below it).
#[test]
fn a_tooltip_without_room_below_the_pointer_shows_above_it() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        Column!({
            Box!(modifier = Modifier.size((400.dp(), 700.dp())));
            TooltipArea!(
                || Box!(modifier = Modifier.size((100.dp(), 50.dp())).test_tag("tooltip")),
                {
                    Box!(modifier = Modifier.size((400.dp(), 100.dp())));
                }
            );
        });
    });
    send(&mut ui, PointerEvent::moved(Offset::new(50.0, 780.0), false));
    ui.advance_time_by(Duration::from_millis(600));
    ui.on_node_with_tag("tooltip").assert_exists();
    let tooltip = ui.on_node_with_tag("tooltip").fetch_semantics_snapshot().bounds;
    assert_eq!(tooltip.top, 714.0);
}

// ---- Re-entrancy lock-ins ------------------------------------------------------------------

/// A tap callback that removes its own node, and a drag callback that changes its block's key
/// mid-gesture: nothing panics, and the restarted block waits for the next press.
#[test]
fn passes_callbacks_removing_the_node_or_changing_the_key() {
    let shown = Rc::new(Cell::new(true));
    let key = Rc::new(Cell::new(0));
    let drags = Rc::new(Cell::new(0));
    let (shown_state, key_state) =
        (compose_runtime::mutable_state_of(true), compose_runtime::mutable_state_of(0));
    let (s, k, d) = (shown.clone(), key.clone(), drags.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (s, k, d) = (s.clone(), k.clone(), d.clone());
        let (shown_state, key_state) = (*shown_state, *key_state);
        Column!({
            if shown_state.get() {
                let (s, shown_state) = (s.clone(), shown_state);
                Box!(
                    modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                        let (s, shown_state) = (s.clone(), shown_state);
                        async move {
                            scope
                                .detect_tap_gestures(TapGestures::default().on_tap(move |_| {
                                    s.set(false);
                                    shown_state.set(false);
                                }))
                                .await
                        }
                    })
                );
            }
            let (k, d, key_state) = (k.clone(), d.clone(), key_state);
            Box!(
                modifier = Modifier.size(100.dp()).pointer_input(key_state.get(), move |scope| {
                    let (k, d, key_state) = (k.clone(), d.clone(), key_state);
                    async move {
                        scope
                            .detect_drag_gestures(DragGestures::default().on_drag(move |_, _| {
                                d.set(d.get() + 1);
                                k.set(k.get() + 1);
                                key_state.set(key_state.get() + 1);
                            }))
                            .await
                    }
                })
            );
        });
    });
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    assert!(!shown.get());
    // The drag box is now at the top.
    press(&mut ui, 10.0, 10.0);
    drag_to(&mut ui, 40.0, 10.0);
    drag_to(&mut ui, 60.0, 10.0);
    drag_to(&mut ui, 80.0, 10.0);
    release(&mut ui, 80.0, 10.0);
    // The key changed at the first drag: the restarted block missed the press.
    assert_eq!(drags.get(), 1);
    press(&mut ui, 10.0, 10.0);
    drag_to(&mut ui, 40.0, 10.0);
    release(&mut ui, 40.0, 10.0);
    assert_eq!(drags.get(), 2);
}

/// A tap detector inside a `Popup`: its taps arrive, without a panic.
#[test]
fn passes_a_block_inside_a_popup() {
    let taps = Rc::new(Cell::new(0));
    let t = taps.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let t = t.clone();
        Box!(modifier = Modifier.size(50.dp()), {
            let t = t.clone();
            Popup!(offset = compose_ui::IntOffset::new(100, 100), {
                let t = t.clone();
                Box!(
                    modifier = Modifier.size(50.dp()).pointer_input((), move |scope| {
                        let t = t.clone();
                        async move {
                            scope
                                .detect_tap_gestures(
                                    TapGestures::default().on_tap(move |_| t.set(t.get() + 1)),
                                )
                                .await
                        }
                    })
                );
            });
        });
    });
    press(&mut ui, 110.0, 110.0);
    release(&mut ui, 110.0, 110.0);
    assert_eq!(taps.get(), 1);
}
