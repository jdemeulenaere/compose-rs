//! Allocation checks for UI frames (docs/09-testing.md): an idle frame, and a frame that
//! recomposes, updates a modifier, measures and draws, allocate nothing once warm; clicks keep
//! live memory constant.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

// ---- The counting allocator ----------------------------------------------------------------

struct Counting;

thread_local! {
    // Const-initialized and without destructors, so the allocator can use them at any time.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static LIVE_BYTES: Cell<i64> = const { Cell::new(0) };
}

fn count(allocations: u64, bytes: i64) {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + allocations));
    let _ = LIVE_BYTES.try_with(|live| live.set(live.get() + bytes));
}

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(1, layout.size() as i64);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(1, layout.size() as i64);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(0, -(layout.size() as i64));
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(1, new_size as i64 - layout.size() as i64);
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

fn live_bytes() -> i64 {
    LIVE_BYTES.with(Cell::get)
}

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::*;

/// Reads the state itself, so only it recomposes. (A content lambda capturing a changed value
/// is a new callback, one allocation, as a new lambda is in Compose.)
#[composable]
fn Bar(color: MutableState<bool>, width: MutableState<u32>) {
    let color = if color.get() { Color::RED } else { Color::BLUE };
    Leaf(Modifier.size_wh((width.get() as f32).dp(), 10.dp()).background(color));
}

#[composable]
fn Screen(color: MutableState<bool>, width: MutableState<u32>, clicks: MutableState<u32>) {
    Column!({
        Bar(color, width);
        for index in 0..20 {
            Leaf(Modifier.size(5.dp()).clickable(move || clicks.update(|count| count + index)));
        }
    });
}

#[test]
fn warm_frames_allocate_nothing() {
    let color = mutable_state_of(false);
    let width = mutable_state_of(10);
    let clicks = mutable_state_of(0);
    let handles = (color.handle(), width.handle(), clicks.handle());
    let mut host = UiHost::headless(400, 400);
    host.set_content(move || Screen(handles.0, handles.1, handles.2));
    host.frame();
    // Warm up: grow the display list and the slot table to their steady sizes.
    for round in 0..3 {
        color.set(round % 2 == 0);
        width.set(10 + round);
        host.frame();
    }
    let before = allocations();
    host.frame();
    assert_eq!(allocations() - before, 0, "an idle frame");
    let before = allocations();
    for round in 0..100 {
        color.set(round % 2 == 0);
        width.set(20 + round % 7);
        let stats = host.frame();
        assert!(stats.recomposed && stats.measured && stats.drew);
    }
    assert_eq!(
        allocations() - before,
        0,
        "frames that recompose, update a modifier, measure and draw"
    );
}

#[test]
fn clicks_keep_live_memory_constant() {
    let color = mutable_state_of(false);
    let width = mutable_state_of(10);
    let clicks = mutable_state_of(0);
    let handles = (color.handle(), width.handle(), clicks.handle());
    let mut host = UiHost::headless(400, 400);
    host.set_content(move || Screen(handles.0, handles.1, handles.2));
    host.frame();
    for _ in 0..10 {
        click(&mut host, 2.0, 12.0);
    }
    let before = live_bytes();
    for _ in 0..1000 {
        click(&mut host, 2.0, 12.0);
    }
    assert_eq!(live_bytes() - before, 0);
    assert_eq!(clicks.get(), 0, "the first row adds 0");
    click(&mut host, 2.0, 17.0);
    assert_eq!(clicks.get(), 1);
}
