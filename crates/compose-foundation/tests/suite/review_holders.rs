//! Review of D240: state holders as Copy handles.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{
    MutableState, SavedState, executor, mutable_state_of, remember, remember_keyed,
};
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, PointerId, PointerType, UiHost, UnitExt};

fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

fn swipe_up(ui: &mut ComposeTestRule, x: f32) {
    let start = executor::now_nanos();
    ui.host().send_pointer(touch(PointerEvent::press(Offset::new(x, 400.0))).at_time(start));
    for step in 1..=6u64 {
        let at = start + step * 10_000_000;
        let y = 400.0 - step as f32 * 50.0;
        ui.host().send_pointer(touch(PointerEvent::moved(Offset::new(x, y), true)).at_time(at));
    }
    ui.host().send_pointer(
        touch(PointerEvent::release(Offset::new(x, 100.0))).at_time(start + 60_000_000),
    );
}

thread_local! {
    static TAB: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
    static TICK: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
}

/// Frames that recompose (as an app with an animation does), so retired objects get freed.
fn frames_with_passes(ui: &mut ComposeTestRule, count: u32) {
    for frame in 0..count {
        TICK.get().unwrap().set(frame + 1);
        ui.advance_time_by(Duration::from_millis(16));
    }
}

/// `remember(tab) { LazyListState() }` (Compose's common per-key state): a new state on the same
/// LazyColumn node. The fling in flight keeps the old handle (the node stays, so it isn't
/// stopped); once the old owner is freed, its next frame panics (Compose: the old state just
/// scrolls on, unseen).
#[test]
fn a_fling_outlives_a_keyed_lazy_list_state() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 500), || {
        let tab = remember(|| mutable_state_of(0u32));
        TAB.set(Some(tab));
        let tick = remember(|| mutable_state_of(0u32));
        TICK.set(Some(tick));
        let _ = tick.get();
        let state = remember_keyed(tab.get(), || LazyListState::new(0, 0));
        LazyColumn!(
            modifier = Modifier.fill_max_size(),
            state = state,
            move |scope: LazyListScope| {
                scope.items(1000, move |index| {
                    BasicText!(
                        "Row {}",
                        index,
                        modifier = Modifier.height(50.dp()).fill_max_width()
                    );
                });
            }
        );
    });
    ui.set_auto_advance(false);
    swipe_up(&mut ui, 100.0);
    ui.advance_time_by_frame();
    TAB.get().unwrap().set(1);
    frames_with_passes(&mut ui, 100);
}

/// The same with `Modifier.vertical_scroll(remember(tab) { ScrollState(0) })`.
#[test]
fn a_fling_outlives_a_keyed_scroll_state() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 500), || {
        let tab = remember(|| mutable_state_of(0u32));
        TAB.set(Some(tab));
        let tick = remember(|| mutable_state_of(0u32));
        TICK.set(Some(tick));
        let _ = tick.get();
        let state = remember_keyed(tab.get(), || ScrollState::new(0));
        Column!(modifier = Modifier.fill_max_size().vertical_scroll(state), {
            for _ in 0..200u32 {
                Spacer(Modifier.fill_max_width().height(50.dp()));
            }
        });
    });
    ui.set_auto_advance(false);
    swipe_up(&mut ui, 100.0);
    ui.advance_time_by_frame();
    TAB.get().unwrap().set(1);
    frames_with_passes(&mut ui, 100);
}

/// `*X::new()` compiles and gives a handle whose owner is already gone (freed at once with no
/// composition on the thread): a footgun. Since D240's revision it doesn't panic: the stale
/// handle reads the defaults and ignores writes (debug builds say so on stderr).
#[test]
fn a_handle_copied_out_of_a_temporary_owner_is_stale() {
    let state = *LazyListState::new(3, 0);
    assert_eq!(state.first_visible_item_index(), 0, "the default, not the freed state's 3");
    state.scroll_to_item(5, 0);
    assert!(!state.scroll_by(10.0));
    assert_eq!(state.first_visible_item_index(), 0);
}

thread_local! {
    static GRID: Cell<Option<LazyGridState>> = const { Cell::new(None) };
}

/// The grid's `Owned` saver: its position comes back in a new host.
#[test]
fn a_lazy_grid_keeps_its_position_in_a_new_host() {
    let content = || {
        let state = remember_lazy_grid_state!();
        GRID.set(Some(state));
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size_wh(150.dp(), 200.dp()),
            state = state,
            |scope: LazyGridScope| {
                scope.items(300, |index| {
                    Box!(modifier = Modifier.height(50.dp()), {
                        BasicText!("item {index}");
                    });
                });
            }
        );
    };
    let mut ui = compose_test(content);
    GRID.get().unwrap().scroll_to_item(60, 10);
    ui.wait_for_idle();
    assert_eq!(GRID.get().unwrap().first_visible_item_index(), 60);
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    let mut ui = ComposeTestRule::with_host(host, content);
    ui.wait_for_idle();
    assert_eq!(GRID.get().unwrap().first_visible_item_index(), 60);
    assert_eq!(GRID.get().unwrap().first_visible_item_scroll_offset(), 10);
}
