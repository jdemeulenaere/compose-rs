//! The resize screen (`resize`, not a measured scenario): its content's width animates between
//! half and all of the screen's, so each frame measures its texts at a new width.

use std::time::Duration;

use compose::runtime::platform;
use compose::test::*;
use compose::ui::platform::{FixedTextMeasurer, UiHost};
use compose::ui::{Density, IntSize};
use compose_rs_feed::resize::*;


/// Turns compose-rs's `REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH` on or off on `host` (D551).
fn reuse_at_same_width(host: &mut UiHost, on: bool) {
    let optimizations = host
        .optimizations()
        .with(compose::ui::platform::Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH, on);
    host.set_optimizations(optimizations);
}

/// The screen on a phone-sized host, its texts' layouts counted; 30 frames 16 ms apart.
fn frames(reuse: bool) -> (UiHost, TextLayoutCounter) {
    platform::set_manual_clock(platform::now_nanos());
    let (measurer, counter) = CountingTextMeasurer::new(FixedTextMeasurer::new());
    let mut host = UiHost::new(IntSize::new(1008, 2244), Density::new(2.25), Box::new(measurer));
    host.set_content(ResizeScreen);
    reuse_at_same_width(&mut host, reuse);
    host.frame();
    counter.clear();
    for _ in 0..30 {
        platform::advance_clock(Duration::from_millis(16));
        host.frame();
    }
    reuse_at_same_width(&mut host, false);
    (host, counter)
}

#[test]
fn the_contents_width_animates_and_its_texts_are_laid_out_again() {
    let (host, counter) = frames(false);
    let semantics = host.semantics_tree(true);
    let dump = format!("{semantics:?}");
    for text in ["Resizing", "Centered across the width", "R1 C1", "R10 C4", "Level 12 · design"] {
        assert!(dump.contains(text), "{text:?} shown");
    }
    // Each text is laid out again whenever its width bounds change: the levels' every frame
    // (about 5 px a frame, but where it turns), the cells' most frames (a quarter of that).
    let level = counter.of("Level 1 · kotlin").total();
    assert!(level >= 28, "{level}");
    let cell = counter.of("R1 C1").total();
    assert!(cell > 15 && cell <= 30, "{cell}");
    let texts = 4 + RESIZE_ROWS * RESIZE_COLUMNS + RESIZE_LEVELS;
    assert!(counter.total() > 15 * texts as usize, "{} layouts", counter.total());
}

#[test]
fn with_the_optimization_the_labels_keep_their_layouts() {
    let (_, parity) = frames(false);
    let (_, reusing) = frames(true);
    // The cells' and levels' labels fit at every width: never laid out again.
    assert_eq!(reusing.of("R1 C1").total(), 0);
    assert_eq!(reusing.of("Level 1 · kotlin").total(), 0);
    // The centered title spans the width, the paragraph wraps: as without it.
    assert_eq!(reusing.of("Centered across the width"), parity.of("Centered across the width"));
    assert!(reusing.total() * 5 < parity.total(), "{} of {}", reusing.total(), parity.total());
}
