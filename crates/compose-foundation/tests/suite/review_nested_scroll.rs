//! Review of nested scrolling (D188, D189): failing tests. Each states the input, what Jetpack
//! Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    IntSize, NestedScrollConnection, NestedScrollSource, Offset, PointerEvent, PointerId,
    PointerType, UnitExt,
};

/// Takes whatever is left after the scroll (as an exit-until-collapsed bar expanding, or a
/// sheet coming down), and counts the flings it hears.
#[derive(Default)]
struct TakesTheRest {
    post_scrolls: RefCell<Vec<Offset>>,
    post_flings: Cell<u32>,
}

impl NestedScrollConnection for TakesTheRest {
    fn on_post_scroll(&self, _: Offset, available: Offset, _: NestedScrollSource) -> Offset {
        self.post_scrolls.borrow_mut().push(available);
        available
    }
    fn on_post_fling(&self, _: Offset, _: Offset) -> Offset {
        self.post_flings.set(self.post_flings.get() + 1);
        Offset::ZERO
    }
}

thread_local! {
    static CONNECTION: RefCell<Option<Rc<TakesTheRest>>> = const { RefCell::new(None) };
    static OUTER: RefCell<Option<ScrollState>> = const { RefCell::new(None) };
    static INNER: RefCell<Option<ScrollState>> = const { RefCell::new(None) };
    static CLICKS: Cell<u32> = const { Cell::new(0) };
}

fn connection() -> Rc<TakesTheRest> {
    CONNECTION.with(|cell| cell.borrow_mut().get_or_insert_with(Default::default).clone())
}

fn outer() -> ScrollState {
    OUTER.with(|cell| (*cell.borrow()).expect("composed"))
}

fn inner() -> ScrollState {
    INNER.with(|cell| (*cell.borrow()).expect("composed"))
}

/// In a 100 by 300 window: an outer scrolling column (a 100 px header, an inner scrolling
/// column 100 px high with 500 px of content, then 1000 px), inside the connection when
/// `connected`.
#[composable]
fn Nested(connected: bool) {
    let outer = remember_scroll_state();
    let inner = remember_scroll_state();
    OUTER.with(|cell| *cell.borrow_mut() = Some(outer));
    INNER.with(|cell| *cell.borrow_mut() = Some(inner));
    let around = if connected {
        let connection: Rc<dyn NestedScrollConnection> = connection();
        Modifier.fill_max_size().nested_scroll(connection)
    } else {
        Modifier.fill_max_size()
    };
    Column!(modifier = around, {
        Column!(modifier = Modifier.fill_max_size().vertical_scroll(outer), {
            Spacer(Modifier.fill_max_width().height(100.dp()));
            Column!(
                modifier = Modifier.fill_max_width().height(100.dp()).vertical_scroll(inner),
                {
                    for _ in 0..10 {
                        Spacer(Modifier.fill_max_width().height(50.dp()));
                    }
                }
            );
            Spacer(Modifier.fill_max_width().height(1000.dp()));
        });
    });
}

fn nested(connected: bool) -> ComposeTestRule {
    CONNECTION.with(|cell| *cell.borrow_mut() = None);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 300), move || Nested(connected));
    // The outer column scrolled 50 down: the inner one (at its top) is at 50 to 150.
    outer().scroll_to(50);
    ui.wait_for_idle();
    assert_eq!((outer().value(), inner().value()), (50, 0));
    ui
}

/// Input: a connection (taking what's left after scrolls, as an exit-until-collapsed bar) around
/// an outer scrolling column scrolled 50 px down, holding an inner one at its top; a 30 px wheel
/// turn back up over the inner one.
///
/// Compose: the outer scrollable is a nested-scroll parent of the inner one: what the inner one
/// can't use is post-scrolled to the outer one first (it scrolls back to 20), and nothing is left
/// for the connection around it. (Without the connection, here, the leftover wheel delta reaches
/// the outer column, so the connection makes it worse.)
///
/// Instead: the outer scrollable isn't in the inner one's chain; the connection takes the 30 px
/// (a bar expands) and the outer column stays scrolled.
#[test]
fn an_outer_scrollable_takes_what_an_inner_one_leaves_before_the_connections_around() {
    let mut ui = nested(true);
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 100.0), Offset::new(0.0, -30.0)));
    ui.wait_for_idle();
    assert_eq!(outer().value(), 20, "the outer column didn't scroll back");
    assert!(
        connection().post_scrolls.borrow().iter().all(|offset| offset.y == 0.0),
        "the connection took {:?}",
        connection().post_scrolls.borrow()
    );
}

/// Input: no connection; the same columns; a finger dragging the inner column (at its top) down
/// 30 px past the slop.
///
/// Compose: the inner scrollable can't scroll; its post-scroll goes to the outer one (its
/// nested-scroll parent), which scrolls back by 30 to 20.
///
/// Instead: the inner column takes the drag past the slop and nothing scrolls.
#[test]
fn a_drag_an_inner_scrollable_cant_use_scrolls_the_outer_one() {
    let mut ui = nested(false);
    let touch = |event: PointerEvent| event.with_pointer(PointerId(1), PointerType::Touch);
    ui.host().send_pointer(touch(PointerEvent::press(Offset::new(50.0, 100.0))));
    for y in [104.0, 108.0, 118.0, 128.0, 138.0] {
        ui.host().send_pointer(touch(PointerEvent::moved(Offset::new(50.0, y), true)));
    }
    ui.wait_for_idle();
    assert_eq!(outer().value(), 20, "the outer column didn't scroll back");
    ui.host().send_pointer(touch(PointerEvent::release(Offset::new(50.0, 138.0))));
}

/// Records a fling's frames (their post-scrolls, `SideEffect`) and its post-fling.
#[derive(Default)]
struct Recorder {
    frames: RefCell<Vec<f32>>,
    post_fling: Cell<Option<Offset>>,
}

impl NestedScrollConnection for Recorder {
    fn on_post_scroll(&self, consumed: Offset, _: Offset, source: NestedScrollSource) -> Offset {
        if source == NestedScrollSource::SideEffect {
            self.frames.borrow_mut().push(consumed.y);
        }
        Offset::ZERO
    }
    fn on_post_fling(&self, _: Offset, available: Offset) -> Offset {
        self.post_fling.set(Some(available));
        Offset::ZERO
    }
}

thread_local! {
    static RECORDER: RefCell<Option<Rc<Recorder>>> = const { RefCell::new(None) };
}

#[composable]
fn Recorded() {
    let recorder: Rc<dyn NestedScrollConnection> =
        RECORDER.with(|cell| cell.borrow().clone().expect("set"));
    let state = remember_scroll_state();
    Column!(modifier = Modifier.fill_max_size().nested_scroll(recorder), {
        Column!(modifier = Modifier.fill_max_size().vertical_scroll(state), {
            // 3,250 px in 300: 2,950 to scroll.
            for _ in 0..65 {
                Spacer(Modifier.fill_max_width().height(50.dp()));
            }
        });
    });
}

/// Input: a connection around a scrolling column; a 5,000 px/s swipe up (it scrolls 242 px),
/// whose fling (about 3,180 px over 1.8 s) meets the end after 2,708 px, 85 % of the way, where
/// it's slowed down to about a quarter of its start.
///
/// Compose: the fling's animation is cancelled at the edge and `onPostFling` gets its velocity
/// then (`velocityLeft`): about what the last frames moved per second.
///
/// Instead: the velocity left is estimated as the start's times the fraction of the *time* left
/// (`velocity * (1 - fraction)`), linear where Android's spline is front-loaded: here 2,530
/// px/s where the last frames moved at 1,310. A sheet settling by it (`on_post_fling`) passes its velocity threshold when
/// the content was nearly stopped.
#[test]
fn the_post_fling_gets_the_flings_velocity_where_it_stopped() {
    let recorder = Rc::new(Recorder::default());
    RECORDER.with(|cell| *cell.borrow_mut() = Some(recorder.clone()));
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 300), Recorded);
    ui.set_auto_advance(false);
    let touch = |event: PointerEvent| event.with_pointer(PointerId(1), PointerType::Touch);
    let start = compose_runtime::executor::now_nanos();
    ui.host().send_pointer(touch(PointerEvent::press(Offset::new(50.0, 280.0))).at_time(start));
    for step in 1..=5u64 {
        let y = 280.0 - step as f32 * 50.0;
        ui.host().send_pointer(
            touch(PointerEvent::moved(Offset::new(50.0, y), true))
                .at_time(start + step * 10_000_000),
        );
    }
    ui.host().send_pointer(
        touch(PointerEvent::release(Offset::new(50.0, 30.0))).at_time(start + 50_000_000),
    );
    ui.advance_time_by(std::time::Duration::from_secs(3));
    let frames = recorder.frames.borrow().clone();
    let left = recorder.post_fling.get().expect("a post-fling").y.abs();
    assert!(frames.len() > 3, "{frames:?}");
    // The last whole frame before the edge, per second.
    let last = frames[frames.len() - 2].abs() / 0.016;
    assert!(
        left < last * 1.5,
        "the post-fling got {left} px/s; the fling moved {last} px/s when it stopped ({frames:?})"
    );
}

#[composable]
fn Overlapping() {
    Box!(modifier = Modifier.fill_max_size(), {
        Spacer(
            Modifier
                .fill_max_size()
                .clickable(|| CLICKS.with(|clicks| clicks.set(clicks.get() + 1))),
        );
        let connection: Rc<dyn NestedScrollConnection> = connection();
        // A sibling drawn over it, with no pointer input of its own.
        Spacer(Modifier.fill_max_size().nested_scroll(connection));
    });
}

/// Input: a clickable under a sibling drawn over it that has only `nested_scroll`; a tap.
///
/// Compose: `nestedScroll` isn't a pointer input modifier: hit testing finds nothing in the
/// sibling and goes on to the clickable under it, which clicks.
///
/// Instead: the nested-scroll node is a pointer input node (it collects its connection on the
/// way in), so the sibling is hit and the siblings under it aren't tested: the click is lost.
/// Adding `nested_scroll` to an overlay (a collapsing header, a pull indicator's box) blocks
/// the content under it.
#[test]
fn a_nested_scroll_node_doesnt_block_the_siblings_under_it() {
    CLICKS.with(|clicks| clicks.set(0));
    CONNECTION.with(|cell| *cell.borrow_mut() = None);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), Overlapping);
    ui.host().send_pointer(PointerEvent::press(Offset::new(50.0, 50.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(50.0, 50.0)));
    ui.wait_for_idle();
    assert_eq!(CLICKS.with(|clicks| clicks.get()), 1, "the click went to the nested-scroll node");
}

/// Input: a connection around a scrolling column; a finger drags it (past the slop), a second
/// finger goes down on it, the first lifts, then the second.
///
/// Compose: the drag goes on with the fingers still down and ends when the last one lifts: the
/// connection hears one post-fling (a bar settles, a sheet goes to an anchor).
///
/// Instead: the second press replaces the drag (not yet past the slop), and the first's release
/// ends it without a fling: no post-fling at all, so a bar or a sheet dragged through the
/// content is left between its anchors.
#[test]
fn a_second_finger_doesnt_lose_the_drags_end() {
    let mut ui = nested(true);
    let finger =
        |id: u64, event: PointerEvent| event.with_pointer(PointerId(id), PointerType::Touch);
    // Below the inner column, on the outer one.
    ui.host().send_pointer(finger(1, PointerEvent::press(Offset::new(50.0, 250.0))));
    for y in [240.0, 220.0, 200.0] {
        ui.host().send_pointer(finger(1, PointerEvent::moved(Offset::new(50.0, y), true)));
    }
    ui.wait_for_idle();
    assert!(outer().value() > 50, "the drag scrolled");
    ui.host().send_pointer(finger(2, PointerEvent::press(Offset::new(50.0, 280.0))));
    ui.host().send_pointer(finger(1, PointerEvent::release(Offset::new(50.0, 200.0))));
    ui.host().send_pointer(finger(2, PointerEvent::release(Offset::new(50.0, 280.0))));
    ui.wait_for_idle();
    assert_eq!(connection().post_flings.get(), 1, "the drag's end wasn't dispatched");
}
