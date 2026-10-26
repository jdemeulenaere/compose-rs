//! Phase E review: allocation and leak checks for frames of Material components, with the
//! counting allocator of compose-ui's `allocations.rs`.
//!
//! Checked and sound (no test kept): an idle frame of a Material screen allocates nothing;
//! typing into a `TextField` and deleting it again 200 times keeps live bytes and live
//! paragraphs constant; removing a screen releases every paragraph.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
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

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_ui::platform::UiHost;

#[composable]
fn Screen(checked: MutableState<bool>) {
    MaterialTheme!({
        Surface!({
            Column!({
                Text!("Groceries");
                Checkbox!(
                    checked = checked.get(),
                    on_checked_change = move |value| checked.set(value)
                );
                Button!(on_click = move || checked.set(!checked.get()), { Text!("Toggle") });
            });
        });
    });
}

fn screen() -> (UiHost, OwnedState<bool>) {
    let checked = mutable_state_of(false);
    let handle = checked.handle();
    let mut host = UiHost::headless(400, 400);
    host.set_content(move || Screen(handle));
    host.settle(10);
    (host, checked)
}

/// Frames 16 ms apart on a manual clock until nothing animates (at most 2 s).
fn settle_animations(host: &mut UiHost) {
    for _ in 0..125 {
        if !host.needs_frame() {
            return;
        }
        compose_runtime::platform::advance_clock(std::time::Duration::from_millis(16));
        host.frame();
    }
    panic!("the animations never ended");
}

/// A checkbox toggled back and forth: once warm, each toggle's frames allocate a bounded number
/// of times and keep nothing, and an idle frame allocates nothing, as compose-ui's frames that
/// recompose and update modifiers (allocations.rs). The checkbox animates as Compose's (D1009):
/// a toggle starts five animations (its check mark's two, its three colors'), whose launches
/// and frames allocate in compose-animation (a few allocations a frame for each running
/// animation, as a `Switch`'s thumb or an `animate_color_as_state`): about 110 a toggle, all
/// freed.
#[test]
fn toggling_a_checkbox_allocates_a_bounded_amount_once_warm() {
    compose_runtime::platform::set_manual_clock(0);
    let (mut host, checked) = screen();
    for round in 0..4 {
        checked.set(round % 2 == 0);
        settle_animations(&mut host);
    }
    let live = LIVE_BYTES.with(Cell::get);
    let before = allocations();
    for round in 0..10 {
        checked.set(round % 2 == 0);
        settle_animations(&mut host);
    }
    let toggles = allocations() - before;
    assert!(toggles <= 1300, "allocations over 10 toggles: {toggles}");
    // Back where it was: what the toggles allocated is freed (a little slack for buffers that
    // grew once).
    let grown = LIVE_BYTES.with(Cell::get) - live;
    assert!(grown <= 4096, "live bytes grew by {grown}");
    // At rest, a frame allocates nothing.
    let before = allocations();
    host.frame();
    assert_eq!(allocations() - before, 0, "an idle frame allocated");
}
