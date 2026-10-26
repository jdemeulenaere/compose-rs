//! The benchmark screens draw edge to edge, as the Kotlin app's (`enableEdgeToEdge()`, which
//! Android 15+ enforces for apps targeting it): bench-rs's view fills the window, under the
//! system bars, and the screens keep their content clear of the bars as Compose's Material
//! defaults do, so the bars' strips show the screen, not the window's background (D647).

use compose::test::*;
use compose::ui::platform::{Insets, PlatformWindowInsets};
use compose::ui::{Density, IntSize};

/// The Pixel 8 Pro's window: 1008 × 2244 px at 2.25, its status bar 113 px tall and its
/// gesture navigation bar 54 px (from `screencap`).
const WIDTH: i32 = 1008;
const HEIGHT: i32 = 2244;
const STATUS_BAR: i32 = 113;
const NAVIGATION_BAR: i32 = 54;

/// The screen at `size`, with `insets` reported, after a few frames.
fn screen(content: fn(), size: IntSize, insets: PlatformWindowInsets) -> ComposeTestRule {
    let mut ui = ComposeTestRule::with_manual_clock(content);
    ui.host().resize(size);
    ui.host().set_density(Density::new(2.25));
    ui.host().set_window_insets(insets);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui
}

/// Each text shown and its bounds, in order, `shift` px higher.
fn texts(ui: &mut ComposeTestRule, shift: f32) -> Vec<(String, [f32; 4])> {
    let matcher = has_text_containing("");
    let count = ui.count_nodes(&matcher);
    let mut texts: Vec<(String, [f32; 4])> = (0..count)
        .map(|index| {
            let node = ui.on_all_nodes(matcher.clone(), index).fetch_semantics_node();
            let text = node.config.all_text().collect::<Vec<_>>().join("");
            let bounds = node.bounds;
            (text, [bounds.left, bounds.top - shift, bounds.right, bounds.bottom - shift])
        })
        .collect();
    texts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    texts
}

/// Each screen, edge to edge in the whole window with the bars' insets reported, shows what it
/// showed inside the bars (the view bench-rs had before D647, `fitsSystemWindows`), where it
/// showed it, below the status bar: its top app bar's content clears the status bar (its
/// container reaching under it) and the content's padding clears the navigation bar.
#[test]
fn the_screens_keep_their_content_clear_of_the_system_bars() {
    let screens: [(&str, fn()); 8] = [
        ("feed", compose_rs_feed::Feed),
        ("feed_animated", compose_rs_feed::FeedAnimated),
        ("big", compose_rs_feed::BigScreen),
        ("stress", compose_rs_feed::stress::StressScroll),
        ("stress_animation", compose_rs_feed::stress::StressAnimation),
        ("navigation", compose_rs_feed::navigation::NavigationApp),
        ("grid", compose_rs_feed::grid::GridScreen),
        ("resize", compose_rs_feed::resize::ResizeScreen),
    ];
    let insets = PlatformWindowInsets {
        status_bars: Insets::new(0, STATUS_BAR, 0, 0),
        navigation_bars: Insets::new(0, 0, 0, NAVIGATION_BAR),
        ..PlatformWindowInsets::default()
    };
    for (name, content) in screens {
        let inside = IntSize::new(WIDTH, HEIGHT - STATUS_BAR - NAVIGATION_BAR);
        let expected = texts(&mut screen(content, inside, PlatformWindowInsets::default()), 0.0);
        let mut ui = screen(content, IntSize::new(WIDTH, HEIGHT), insets);
        let shown = texts(&mut ui, STATUS_BAR as f32);
        assert!(!expected.is_empty(), "{name} shows no text");
        assert_eq!(shown, expected, "{name}: the texts moved");
    }
}
