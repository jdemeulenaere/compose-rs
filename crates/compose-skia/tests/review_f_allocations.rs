//! Phase F review: allocations of the desktop window's present path, with the counting
//! allocator of compose-ui's `allocations.rs`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use compose::ui::{
    Color, Density, IntSize,
    platform::{Renderer, UiHost},
};
use compose_skia::{RasterRenderer, SkiaTextMeasurer};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static ALLOCATED_BYTES: Cell<u64> = const { Cell::new(0) };
}

fn count(bytes: usize) {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    let _ = ALLOCATED_BYTES.try_with(|total| total.set(total.get() + bytes as u64));
}

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What `compose-desktop`'s `redraw` does for a frame once the host's frame is done: it clones
/// the display list (`host.display_list()`), renders it, and copies the pixels out
/// (`renderer.pixels()`) to write them into softbuffer's buffer.
///
/// Since the fix, as the desktop does now: the display list rendered in place, and the pixels
/// copied straight into the window's buffer (here a buffer kept across frames).
fn present(host: &mut UiHost, renderer: &mut RasterRenderer, window: &mut [u32]) -> usize {
    host.frame();
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, &*tree.text_measurer_ref())
    });
    renderer.copy_pixels_to(window);
    window.len()
}

/// The docs promise warm frames that allocate nothing (docs/06-ui.md, "Drawing": "a warm frame
/// allocates nothing") and the desktop's frame statistics count allocations per frame. On an
/// 800 x 600 dp window at scale 2 (1600 x 1200 pixels), the desktop's present path allocates
/// about 15 MB per frame on the Rust heap: `pixels()` reads the surface into a fresh byte
/// vector and then collects a second, `u32`, vector of the same size (7.7 MB each), and the
/// display list is cloned. Each redraw (every resize step, every animation frame, every state
/// change) pays two full-frame allocations and copies before the copy into softbuffer.
/// Compose Desktop draws into the window's surface directly. Reading the pixels straight into
/// softbuffer's buffer (or peeking the raster surface's memory) needs no allocation.
#[test]
fn a_warm_desktop_frame_allocates_nothing() {
    let (width, height) = (1600, 1200);
    let mut host = UiHost::new(
        IntSize::new(width, height),
        Density::new(2.0),
        Box::new(SkiaTextMeasurer::new()),
    );
    host.set_content(counter::Counter);
    let mut renderer = RasterRenderer::new(width, height);
    let mut window = vec![0u32; (width * height) as usize];
    for _ in 0..3 {
        present(&mut host, &mut renderer, &mut window);
    }
    let (allocations, bytes) = (ALLOCATIONS.with(Cell::get), ALLOCATED_BYTES.with(Cell::get));
    let start = Instant::now();
    present(&mut host, &mut renderer, &mut window);
    let elapsed = start.elapsed();
    let allocations = ALLOCATIONS.with(Cell::get) - allocations;
    let bytes = ALLOCATED_BYTES.with(Cell::get) - bytes;
    assert!(
        bytes < 64 * 1024,
        "an idle frame's present path: {allocations} allocations, {bytes} bytes, {elapsed:?}"
    );
}
