//! Review of the Material 3 additions (scrolling app bars and D182's nested scrolling, sheets,
//! swipe to dismiss, the bottom app bar): failing tests. Each states the input, what Jetpack
//! Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::{ScrollModifiers, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    IntSize, Key, LayoutDirection, Modifier, Offset, PointerEvent, PointerId, PointerType, UnitExt,
};

/// A finger's event (touches fling when let go moving; a mouse doesn't).
fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
}

/// A scaffold under a bar of `kind` (0 pinned, 1 enter-always, 2 exit-until-collapsed), medium
/// unless `small`, its content 40 items of 40 dp (the first tagged "first"): `content` 0, a
/// scrolling column below the bar (padded, then scrolling); 1, a scrolling column under the bar
/// whose content is padded (as Compose's usual `LazyColumn(contentPadding = innerPadding)`);
/// 2, a column that doesn't scroll.
#[composable]
fn Screen(kind: u8, small: bool, content: u8) {
    MaterialTheme!({
        let behavior = match kind {
            0 => TopAppBarDefaults::pinned_scroll_behavior(),
            1 => TopAppBarDefaults::enter_always_scroll_behavior(),
            _ => TopAppBarDefaults::exit_until_collapsed_scroll_behavior(),
        };
        let scroll = remember_scroll_state!();
        Scaffold!(
            modifier = Modifier.nested_scroll(behavior.nested_scroll_connection()),
            top_bar = {
                if small {
                    TopAppBar!(
                        { Text!("Title") },
                        modifier = Modifier.test_tag("bar"),
                        scroll_behavior = Some(behavior.clone())
                    );
                } else {
                    MediumTopAppBar!(
                        { Text!("Title") },
                        modifier = Modifier.test_tag("bar"),
                        scroll_behavior = Some(behavior.clone())
                    );
                }
            },
            |padding: PaddingValues| {
                let modifier = match content {
                    0 => Modifier.padding(padding).fill_max_size().vertical_scroll(scroll),
                    1 => Modifier.fill_max_size().vertical_scroll(scroll).padding(padding),
                    _ => Modifier.padding(padding).fill_max_size(),
                };
                Column!(modifier = modifier, {
                    for index in 0..40 {
                        let tag = if index == 0 { "first" } else { "other" };
                        Text!("Item {}", index, modifier = Modifier.height(40.dp()).test_tag(tag));
                    }
                });
            }
        );
    });
}

fn height(ui: &mut ComposeTestRule) -> f32 {
    bounds(ui, "bar").height()
}

/// How far the content has scrolled: the bar's height (the content's top padding) less where
/// its first item is.
fn scrolled(ui: &mut ComposeTestRule) -> f32 {
    height(ui) - bounds(ui, "first").top
}

/// A finger's drag from `from` through `to` (y), `step` ms apart (a frame after each).
fn drag(ui: &mut ComposeTestRule, from: f32, to: &[f32], step: u64) {
    ui.host().send_pointer(touch(PointerEvent::press(Offset::new(200.0, from))));
    for &y in to {
        ui.host().send_pointer(touch(PointerEvent::moved(Offset::new(200.0, y), true)));
        ui.advance_time_by(Duration::from_millis(step));
    }
}

/// A quick finger's flick through `to`, its events 10 ms apart at their times, released at the
/// last; no frame in between.
fn flick(ui: &mut ComposeTestRule, from: f32, to: &[f32]) {
    let start = compose_runtime::platform::now_nanos();
    ui.host().send_pointer(touch(PointerEvent::press(Offset::new(200.0, from))).at_time(start));
    let mut at = start;
    for &y in to {
        at += 10_000_000;
        ui.host().send_pointer(touch(PointerEvent::moved(Offset::new(200.0, y), true)).at_time(at));
    }
    let last = to[to.len() - 1];
    ui.host().send_pointer(touch(PointerEvent::release(Offset::new(200.0, last))).at_time(at));
}

fn release(ui: &mut ComposeTestRule, at: f32) {
    ui.host().send_pointer(touch(PointerEvent::release(Offset::new(200.0, at))));
}

/// Input: a medium bar exiting until collapsed over a column scrolling under it, its content
/// padded by the scaffold's padding (Compose's usual `LazyColumn(contentPadding =
/// innerPadding)`); a slow drag up 30 px past the touch slop, a frame after each move.
///
/// Compose: the content's scrollable offers each delta to the connection first; the bar takes
/// all of it (112 to 82 dp) and the content doesn't scroll: its first item, moved up by the
/// shrinking padding only, follows the finger (82 dp from the top).
///
/// Instead (D182): the connection takes each delta and the content's scrollable, which never
/// asks it, scrolls by it too: the items move at twice the finger's speed (52 dp from the top).
#[test]
fn a_drag_collapsing_the_bar_doesnt_scroll_the_content_too() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(2, false, 1));
    ui.set_auto_advance(false);
    drag(&mut ui, 400.0, &[392.0, 382.0, 372.0, 362.0], 100);
    assert_eq!(height(&mut ui), 82.0);
    assert_eq!(scrolled(&mut ui), 0.0, "the content scrolled while the bar collapsed");
    release(&mut ui, 362.0);
}

/// Input: the same bar over a column below it (padded, then scrolling: the column itself
/// moves up as the bar collapses); the same drag.
///
/// Compose: as above, the content doesn't scroll.
///
/// Instead: the column's own move up mostly hides the double scroll (its scrollable reads the
/// pointer in the column's coordinates, laid out again before each event), except for the
/// move crossing the slop: the content keeps 10 px of scroll it shouldn't have (and the node's
/// estimate of the content's position, which assumes none, is off by as much).
#[test]
fn the_move_crossing_the_slop_doesnt_scroll_the_content_too() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(2, false, 0));
    ui.set_auto_advance(false);
    drag(&mut ui, 400.0, &[392.0, 382.0, 372.0, 362.0], 100);
    assert_eq!(height(&mut ui), 82.0);
    assert_eq!(scrolled(&mut ui), 0.0, "the content scrolled while the bar collapsed");
    release(&mut ui, 362.0);
}

/// Input: the same bar over a column below it; a quick flick up (the content flings hundreds
/// of pixels on), then a slow drag down of 300 px.
///
/// Compose: the content scrolls back by 300 px; it's still far from its start, so the
/// exit-until-collapsed bar stays collapsed (64 dp).
///
/// Instead: the node estimates the content's position from the drags it saw (not the fling,
/// nor the deltas the bar took): it believes the content back at its start after a part of
/// the drag and offers the rest to the post-scroll, which expands the bar while the content is
/// still scrolled far down.
#[test]
fn after_a_fling_dragging_back_doesnt_expand_the_bar_before_the_content_is_at_its_start() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(2, false, 0));
    ui.set_auto_advance(false);
    flick(&mut ui, 500.0, &[490.0, 440.0, 390.0, 340.0, 290.0, 240.0]);
    ui.advance_time_by(Duration::from_secs(3));
    assert_eq!(height(&mut ui), 64.0);
    let flung = scrolled(&mut ui);
    assert!(flung > 500.0, "the content flung only to {flung}");
    drag(&mut ui, 100.0, &[108.0, 158.0, 208.0, 258.0, 308.0, 358.0, 408.0], 100);
    assert!(scrolled(&mut ui) > 100.0, "{}", scrolled(&mut ui));
    assert_eq!(height(&mut ui), 64.0, "the bar expanded with the content far from its start");
    release(&mut ui, 408.0);
}

/// Input: an exit-until-collapsed bar over content that doesn't scroll; a drag up of 30 px
/// past the slop.
///
/// Compose: nothing scrolls, so nothing reaches the connection: the bar stays 112 dp.
///
/// Instead: the node gives every vertical drag inside it to the connection, whatever is
/// under the finger (a button, a horizontal pager, a slider, content that can't scroll).
#[test]
fn a_drag_on_content_that_doesnt_scroll_leaves_the_bar() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(2, false, 2));
    ui.set_auto_advance(false);
    drag(&mut ui, 400.0, &[392.0, 382.0, 372.0, 362.0], 100);
    assert_eq!(height(&mut ui), 112.0);
    release(&mut ui, 362.0);
}

/// Input: an exit-until-collapsed bar over a scrolling column; a 30 px wheel turn over the bar
/// itself (y = 30).
///
/// Compose: the bar doesn't scroll; the wheel reaches no scrollable, so no connection: 112 dp.
///
/// Instead: the node pre-scrolls every wheel event inside it, wherever it is: 82 dp.
#[test]
fn a_wheel_over_the_bar_itself_doesnt_collapse_it() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(2, false, 0));
    ui.set_auto_advance(false);
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(200.0, 30.0), Offset::new(0.0, 30.0)));
    ui.advance_time_by(Duration::from_millis(32));
    assert_eq!(height(&mut ui), 112.0);
}

/// Input: a small enter-always bar over a scrolling column; a quick flick up 22 px past the
/// slop, released moving fast (the content flings on).
///
/// Compose: the content's fling dispatches its scrolls to the connection (source `SideEffect`)
/// and enter-always pre-scrolls them: the bar collapses fully (0 dp) as the content flings.
///
/// Instead: the fling isn't dispatched; the release settles the bar from 22/64 collapsed back
/// to expanded (64 dp) while the content flings away under it.
#[test]
fn a_fling_collapses_an_enter_always_bar() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(1, true, 0));
    ui.set_auto_advance(false);
    // 8 px every 10 ms: 800 px/s, as Compose's Lsq2 tracker reads it (D862).
    flick(&mut ui, 400.0, &[392.0, 384.0, 376.0, 368.0]);
    ui.advance_time_by(Duration::from_secs(3));
    // The drag went to the bar; the content scrolled by the fling only: 800 px/s flings about
    // 130 px (Android's spline), of which the bar takes the 40 px it had left.
    assert!(scrolled(&mut ui) > 50.0, "no fling: {}", scrolled(&mut ui));
    assert_eq!(height(&mut ui), 0.0);
}

// ---- ModalBottomSheet --------------------------------------------------------------------------

#[composable]
fn SheetApp(state: SheetState, shown: MutableState<bool>, content_height: f32) {
    MaterialTheme!({
        Box!(modifier = Modifier.fill_max_size(), {
            if shown.get() {
                ModalBottomSheet!(move || shown.set(false), sheet_state = state, {
                    Box!(
                        modifier = Modifier
                            .fill_max_width()
                            .height(content_height.dp())
                            .test_tag("content")
                    );
                });
            }
        });
    });
}

fn sheet(
    state: SheetState,
    content_height: f32,
) -> (ComposeTestRule, MutableState<bool>, OwnedState<bool>) {
    let owner = mutable_state_of(true);
    let shown = owner.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || {
        SheetApp(state, shown, content_height)
    });
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(600));
    (ui, shown, owner)
}

/// Input: a sheet whose `confirm_value_change` refuses `Hidden` (the usual way to keep a sheet
/// from being dismissed); a tap on the scrim.
///
/// Compose: the scrim's `animateToDismiss` asks `confirmValueChange(Hidden)` first: refused,
/// the sheet stays and `onDismissRequest` isn't called.
///
/// Instead: the scrim calls `hide()`, which doesn't ask: the sheet hides and is dismissed.
#[test]
fn the_scrim_asks_confirm_value_change_before_hiding_the_sheet() {
    let state = SheetState::new(
        &[SheetValue::Hidden, SheetValue::Expanded],
        || 56.0,
        || 125.0,
        SheetValue::Hidden,
        |value| value != SheetValue::Hidden,
    );
    let (mut ui, shown, _owner) = sheet(*state, 200.0);
    assert_eq!(state.current_value(), SheetValue::Expanded);
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 100.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, 100.0)));
    ui.advance_time_by(Duration::from_millis(600));
    assert!(shown.get(), "dismissed though confirm_value_change refused Hidden");
    assert_eq!(state.current_value(), SheetValue::Expanded);
}

/// Input: a tall sheet (it can rest half way), expanded; Escape (back).
///
/// Compose: back on an expanded sheet that has a partially expanded state partially expands
/// it (`ModalBottomSheetDialog`'s `onDismissRequest`); only from there does back hide it.
///
/// Instead: back hides it and dismisses it at once.
#[test]
fn back_on_an_expanded_sheet_takes_it_half_way_first() {
    let state = SheetState::new(&SheetValue::ALL, || 56.0, || 125.0, SheetValue::Hidden, |_| true);
    let (mut ui, shown, _owner) = sheet(*state, 500.0);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    let scope = CoroutineScope::new();
    let expanding = *state;
    scope.launch(async move { expanding.expand().await });
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(state.current_value(), SheetValue::Expanded);
    ui.press_key(Key::Escape);
    ui.advance_time_by(Duration::from_millis(600));
    assert!(shown.get(), "dismissed from expanded");
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
}

// ---- SwipeToDismissBox -------------------------------------------------------------------------

#[composable]
fn SwipeApp(state: SwipeToDismissBoxState) {
    MaterialTheme!({
        SwipeToDismissBox!(
            state,
            |_| {
                Box!(modifier = Modifier.fill_max_size());
            },
            modifier = Modifier.test_tag("box"),
            |_| {
                ListItem!(modifier = Modifier.test_tag("item"), {
                    Text!("Swipe me");
                });
            }
        );
    });
}

fn swipe(ui: &mut ComposeTestRule, y: f32, xs: &[f32]) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(xs[0], y)));
    for &x in &xs[1..] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
        ui.advance_time_by(Duration::from_millis(100));
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(xs[xs.len() - 1], y)));
    ui.advance_time_by(Duration::from_secs(1));
}

/// Input: an item swiped away towards the start, then dragged towards the end by 150 px.
///
/// Compose: `SwipeToDismissBox`'s `anchoredDraggable` is enabled only while `currentValue ==
/// Settled`: a dismissed item doesn't move (the app removes it).
///
/// Instead: the drag works on a dismissed item: it comes back into view.
#[test]
fn a_dismissed_item_cant_be_dragged_back() {
    let state = SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled, |_| 56.0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || SwipeApp(given));
    ui.set_auto_advance(false);
    let item = bounds(&mut ui, "item");
    let y = (item.top + item.bottom) / 2.0;
    swipe(&mut ui, y, &[300.0, 280.0, 180.0, 80.0]);
    assert_eq!(state.current_value(), SwipeToDismissBoxValue::EndToStart);
    assert_eq!(bounds(&mut ui, "item").left, -400.0);
    // The item is off screen; the box (its background) is still there to be dragged.
    ui.host().send_pointer(PointerEvent::press(Offset::new(100.0, y)));
    for x in [120.0, 180.0, 250.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
        ui.advance_time_by(Duration::from_millis(100));
    }
    assert_eq!(bounds(&mut ui, "item").left, -400.0, "a dismissed item moved");
    ui.host().send_pointer(PointerEvent::release(Offset::new(250.0, y)));
}

/// Input: right to left; an item swiped 40 px leftwards (towards the end there) and held.
///
/// Compose: the offset is logical (the drag is reversed in right to left), so
/// `dismissDirection` is `StartToEnd`, and a background choosing its icon by it shows the
/// right one.
///
/// Instead: the offset is physical (negative) and `dismiss_direction` reads its sign as
/// left-to-right would: `EndToStart`.
#[test]
fn the_dismiss_direction_is_logical_right_to_left() {
    let state = SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled, |_| 56.0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || SwipeApp(given));
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(100));
    let item = bounds(&mut ui, "item");
    let y = (item.top + item.bottom) / 2.0;
    ui.host().send_pointer(PointerEvent::press(Offset::new(300.0, y)));
    for x in [290.0, 280.0, 250.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
        ui.advance_time_by(Duration::from_millis(200));
    }
    assert!(bounds(&mut ui, "item").left < -20.0);
    assert_eq!(state.dismiss_direction(), SwipeToDismissBoxValue::StartToEnd);
    ui.host().send_pointer(PointerEvent::release(Offset::new(250.0, y)));
}

// ---- BottomAppBar ------------------------------------------------------------------------------

/// Input: a bottom app bar with a FAB.
///
/// Compose: the FAB's box is padded by `FABVerticalPadding` (12 - 4 = 8 dp) at the top and
/// `FABHorizontalPadding` (16 - 4 = 12 dp) at the end, inside the bar's 4 dp content padding:
/// the 56 dp FAB is 16 dp from the bar's end and 12 dp from its top, centered in the 80 dp bar.
///
/// Instead: 12 and 16 dp inside the 4 dp padding: 20 dp from the end, 16 dp from the top (8 dp
/// from the bottom).
#[test]
fn a_bottom_app_bars_fab_is_16_dp_from_the_end_and_centered() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || {
        MaterialTheme!({
            BottomAppBar!(
                |_| {},
                modifier = Modifier.test_tag("bar"),
                floating_action_button = {
                    FloatingActionButton!(|| {}, modifier = Modifier.test_tag("fab"), {
                        Icon!(icons::filled::add(), Some("Add"));
                    });
                }
            );
        });
    });
    let fab = bounds(&mut ui, "fab");
    assert_eq!((fab.right, fab.top), (400.0 - 16.0, 12.0));
}
