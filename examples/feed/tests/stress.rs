//! The stress screens (docs/15) compose, scroll and animate as designed: the benchmark measures
//! what these check is there.

use compose::test::*;
use compose::ui::{Density, IntSize, Offset, PointerEvent};
use compose_rs_feed::stress::*;

/// The Pixel 8 Pro's screen, as the benchmark runs it: 1008 × 2244 px at 2.25.
fn phone(content: fn()) -> ComposeTestRule {
    let mut ui = ComposeTestRule::with_manual_clock(content);
    ui.host().resize(IntSize::new(1008, 2244));
    ui.host().set_density(Density::new(2.25));
    ui.advance_time_by_frame();
    ui
}

#[test]
fn bitmaps_are_opaque_gradients_with_rings() {
    for which in 0..BITMAPS {
        let bitmap = stress_bitmap(which);
        assert_eq!((bitmap.width(), bitmap.height()), (BITMAP_WIDTH, BITMAP_HEIGHT));
        assert!(bitmap.pixels().chunks(4).all(|pixel| pixel[3] == 0xFF));
        // Not flat: the corners differ.
        assert_ne!(
            bitmap_pixel(which, 0, 0),
            bitmap_pixel(which, BITMAP_WIDTH - 1, BITMAP_HEIGHT - 1)
        );
    }
    // The values the Kotlin app's `stressBitmapPixel` computes too (Stress.kt).
    assert_eq!(bitmap_pixel(0, 0, 0), 0xFF9780D4);
    assert_eq!(bitmap_pixel(3, 100, 50), 0xFFCF6850);
    assert_eq!(bitmap_pixel(7, 239, 179), 0xFF006A6A);
}

#[test]
fn the_kinds_alternate() {
    let kinds: Vec<StressKind> = (0..5).map(StressKind::of).collect();
    assert_eq!(
        kinds,
        [
            StressKind::Carousel,
            StressKind::RichPost,
            StressKind::ActionCard,
            StressKind::Banner,
            StressKind::Carousel
        ]
    );
}

#[test]
fn the_stress_list_shows_each_kind_under_a_header() {
    let mut ui = phone(StressScroll);
    ui.on_node_with_text("Stress").assert_exists();
    ui.on_node_with_text("Section 1").assert_exists();
    ui.on_node_with_text("Collection 0").assert_exists();
    assert!(ui.count_nodes(&has_text("Tile 1")) >= 1);
    ui.on_node_with_text("Alan Turing").assert_exists();
    ui.on_node_with_text("Task 2").assert_exists();
    assert!(ui.count_nodes(&has_text("Accept")) >= 1);
    ui.on_node_with_text("Featured 3").assert_exists();
    // A carousel composes the tiles in view and a few beyond, not all twelve.
    assert_eq!(ui.count_nodes(&has_text("Tile 12")), 0);
}

#[test]
fn a_header_sticks_while_its_items_scroll_under_it() {
    let mut ui = phone(StressScroll);
    let top = ui.on_node_with_text("Section 1").fetch_semantics_node().bounds.top;
    let host = ui.host();
    let mut y = 2000.0;
    host.send_pointer(PointerEvent::press(Offset::new(500.0, y)));
    while y > 600.0 {
        y -= 50.0;
        host.send_pointer(PointerEvent::moved(Offset::new(500.0, y), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(500.0, y)));
    ui.advance_time_by_frame();
    assert!(ui.on_node_with_text("Collection 0").fetch_semantics_node().bounds.bottom < 0.0);
    // Item 0 scrolled away; its section's header is where it was.
    assert_eq!(ui.on_node_with_text("Section 1").fetch_semantics_node().bounds.top, top);
}

#[test]
fn the_stress_animation_ticks_every_frame_and_its_card_grows() {
    let mut ui = phone(StressAnimation);
    assert_eq!(ui.count_nodes(&has_text_containing("Frame ")), 1);
    let first = ticker(&mut ui);
    ui.advance_time_by_frame();
    let second = ticker(&mut ui);
    ui.advance_time_by_frame();
    assert_ne!(first, second);
    assert_ne!(second, ticker(&mut ui));
    // Each bar's percentage.
    assert_eq!(ui.count_nodes(&has_text_containing(" %")), BARS as usize);
    // The card shows one sentence, then four after a second, then one again.
    let more = "Reading about incremental computation";
    assert_eq!(ui.count_nodes(&has_text_containing(more)), 0);
    ui.advance_time_by(std::time::Duration::from_millis(1_000));
    assert_eq!(ui.count_nodes(&has_text_containing(more)), 1);
    ui.advance_time_by(std::time::Duration::from_millis(1_000));
    assert_eq!(ui.count_nodes(&has_text_containing(more)), 0);
}

fn ticker(ui: &mut ComposeTestRule) -> String {
    let node = ui.on_node(has_text_containing("Frame ")).fetch_semantics_node();
    node.config.all_text().collect::<Vec<_>>().join("")
}
