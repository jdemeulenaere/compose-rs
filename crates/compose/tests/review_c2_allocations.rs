//! Phase C second review: steady-state passes through call macros with memoized lambdas,
//! defaulted callbacks and named callbacks allocate nothing, and the callees skip.
//!
//! Failing today: a function item given by name or as a default (`on_click = noop`,
//! `#[default(noop)]`). `probe_value` expands to
//! `lambda_probe(keys).resolve(to_callback(value))`, so `to_callback` (an `Rc::new`) runs on
//! every pass, before the probe hits and drops it: one allocation per pass, though the callee
//! skips. A closure is passed to `resolve` unconverted, so it doesn't allocate on a hit.

#![allow(non_snake_case)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use compose::runtime::platform::*;
use compose::runtime::*;

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static RUNS: Cell<u32> = const { Cell::new(0) };
}

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations_in(f: impl FnOnce()) -> u64 {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

fn noop() {}

#[composable]
fn Clickable(label: &'static str, #[default(noop)] on_click: impl Fn() + 'static) {
    RUNS.with(|runs| runs.set(runs.get() + 1));
    let _ = (label, on_click);
}

#[composable]
fn Holder(content: impl Fn() + 'static) {
    content();
}

/// One screen per case: it recomposes on `tick`; its callees must skip, allocating nothing.
macro_rules! case {
    ($test:ident, |$clicks:ident, $on_done:ident| $body:block) => {
        #[test]
        fn $test() {
            #[composable]
            fn Screen(
                tick: MutableState<u32>,
                $clicks: MutableState<u32>,
                $on_done: impl Fn() + 'static,
            ) {
                tick.get();
                $body
            }
            let composition = Composition::new(NodeTree::<()>::new());
            let tick = mutable_state_of(0u32);
            let clicks = mutable_state_of(0u32);
            let (tick_handle, clicks_handle) = (tick.handle(), clicks.handle());
            let on_done = callback(|| {});
            composition.set_content(move || Screen(tick_handle, clicks_handle, on_done.clone()));
            for round in 1..4 {
                tick.set(round);
                composition.recompose();
            }
            RUNS.with(|runs| runs.set(0));
            let mut allocated = 0;
            for round in 4..50 {
                allocated += allocations_in(|| {
                    tick.set(round);
                    assert!(composition.recompose());
                });
            }
            assert_eq!(allocated, 0, "passes with only memoized lambdas allocate nothing");
            assert_eq!(RUNS.with(Cell::get), 0, "the callee skips");
        }
    };
}

case!(a_memoized_closure_allocates_nothing, |clicks, on_done| {
    let _ = &on_done;
    let id = 3u32;
    Clickable!("closure", on_click = move || clicks.set(clicks.get() + id));
});

case!(a_defaulted_callback_allocates_nothing, |clicks, on_done| {
    let _ = (&clicks, &on_done);
    Clickable!("default");
});

case!(a_named_callback_parameter_allocates_nothing, |clicks, on_done| {
    let _ = &clicks;
    Clickable!("named parameter", on_click = on_done);
});

case!(a_named_function_allocates_nothing, |clicks, on_done| {
    let _ = (&clicks, &on_done);
    Clickable!("named function", on_click = noop);
});

case!(a_capture_less_closure_allocates_nothing, |clicks, on_done| {
    let _ = (&clicks, &on_done);
    Clickable!("capture-less", on_click = || {});
});

case!(an_explicit_callback_allocates_nothing, |clicks, on_done| {
    let _ = &on_done;
    let id = 3u32;
    Clickable!("callback", on_click = callback(move || clicks.set(id)));
});

case!(a_content_block_allocates_nothing, |clicks, on_done| {
    let _ = &on_done;
    let id = 3u32;
    Holder!({
        Clickable!("in content", on_click = move || clicks.set(id));
    });
});
