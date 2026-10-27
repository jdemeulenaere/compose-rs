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
            let node = ui.on_all_nodes(matcher.clone()).get(index).fetch_semantics_node();
            let text = node.config.all_text().collect::<Vec<_>>().join("");
            let bounds = node.bounds;
            (text, [bounds.left, bounds.top - shift, bounds.right, bounds.bottom - shift])
        })
        .collect();
    texts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    texts
}

/// The bounds of the top app bar holding the text `title`: the outermost of its ancestors
/// shorter than the window (the scaffold's top bar slot).
fn top_bar(ui: &mut ComposeTestRule, title: &str) -> [f32; 4] {
    let mut node = ui.on_node_with_text(title).node_id();
    ui.host().with_tree(|tree| {
        while let Some(parent) = tree.parent_of(node)
            && tree.bounds_in_root(parent).height() < HEIGHT as f32
        {
            node = parent;
        }
        let bounds = tree.bounds_in_root(node);
        [bounds.left, bounds.top, bounds.right, bounds.bottom]
    })
}

/// The bounds of the screen's vertically scrolling content (a lazy list, a grid, a scrolling
/// column), if it has one: inside its scaffold's padding.
fn scrolling_content(ui: &mut ComposeTestRule) -> Option<[f32; 4]> {
    let matcher = Matcher::new("scrolls vertically", |config| {
        config.get(&compose::ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE).copied().is_some()
    });
    if ui.count_nodes(&matcher) == 0 {
        return None;
    }
    let bounds = ui.on_all_nodes(matcher).get(0).fetch_semantics_node().bounds;
    Some([bounds.left, bounds.top, bounds.right, bounds.bottom])
}

/// The bounds of the items the screen's lazy list placed, in order, from the tree's dump
/// (`LazyItem [x,y w×h]`, from the list's origin, here the window's).
fn placed_items(ui: &mut ComposeTestRule) -> Vec<[f32; 4]> {
    ui.dump_tree()
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("LazyItem [") && !line.ends_with("(not placed)"))
        .map(|line| {
            let bounds = &line["LazyItem [".len()..line.find(']').expect("bounds")];
            let (origin, size) = bounds.split_once(' ').expect("an origin and a size");
            let (x, y) = origin.split_once(',').expect("x,y");
            let (width, height) = size.split_once('x').expect("wxh");
            let [x, y, width, height] =
                [x, y, width, height].map(|value| value.parse::<f32>().expect("a number"));
            [x, y, x + width, y + height]
        })
        .collect()
}

/// Each screen, edge to edge in the whole window with the bars' insets reported, shows what it
/// showed inside the bars (the view bench-rs had before D647, `fitsSystemWindows`), where it
/// showed it: its top app bar starts at the window's top, under the status bar, its content
/// clear of it, and the scaffold's content ends above the navigation bar. But for the feeds,
/// whose list takes the scaffold's padding as its content padding, as the Kotlin feed's
/// `contentPadding = padding` (D649): their list fills the window, its first post below the top
/// bar where it was, and its viewport goes on under the navigation bar, where it shows the
/// posts after those it showed.
#[test]
fn the_screens_keep_their_content_clear_of_the_system_bars() {
    let screens: [(&str, &str, fn()); 8] = [
        ("feed", "Feed", compose_rs_feed::Feed),
        ("feed_animated", "Feed", compose_rs_feed::FeedAnimated),
        ("big", "Big screen", compose_rs_feed::BigScreen),
        ("stress", "Stress", compose_rs_feed::stress::StressScroll),
        ("stress_animation", "Stress animation", compose_rs_feed::stress::StressAnimation),
        ("navigation", "Navigation", compose_rs_feed::navigation::NavigationApp),
        ("grid", "Grid", compose_rs_feed::grid::GridScreen),
        ("resize", "Resize", compose_rs_feed::resize::ResizeScreen),
    ];
    let insets = PlatformWindowInsets {
        status_bars: Insets::new(0, STATUS_BAR, 0, 0),
        navigation_bars: Insets::new(0, 0, 0, NAVIGATION_BAR),
        ..PlatformWindowInsets::default()
    };
    // A small top app bar: 64 dp (144 px) below the status bar.
    let bar = (STATUS_BAR + 144) as f32;
    let mut scrolling = 0;
    for (name, title, content) in screens {
        let inside = IntSize::new(WIDTH, HEIGHT - STATUS_BAR - NAVIGATION_BAR);
        let expected = texts(&mut screen(content, inside, PlatformWindowInsets::default()), 0.0);
        let mut ui = screen(content, IntSize::new(WIDTH, HEIGHT), insets);
        let mut shown = texts(&mut ui, STATUS_BAR as f32);
        assert!(!expected.is_empty(), "{name} shows no text");
        let feed = name.starts_with("feed");
        if feed {
            // Any post after those shown inside the bars is under the navigation bar: its
            // texts below where the view ended.
            let end = inside.height as f32;
            let (under, kept): (Vec<_>, Vec<_>) =
                shown.into_iter().partition(|text| !expected.contains(text));
            for (text, bounds) in &under {
                assert!(bounds[1] >= end, "{name}: {text:?} at {bounds:?}, above {end}");
            }
            shown = kept;
        }
        assert_eq!(shown, expected, "{name}: the texts moved");
        assert_eq!(top_bar(&mut ui, title), [0.0, 0.0, WIDTH as f32, bar], "{name}: its top bar");
        if let Some(content) = scrolling_content(&mut ui) {
            let (top, bottom) =
                if feed { (0.0, HEIGHT as f32) } else { (bar, (HEIGHT - NAVIGATION_BAR) as f32) };
            assert_eq!(content, [0.0, top, WIDTH as f32, bottom], "{name}: its content");
            scrolling += 1;
        }
        if feed {
            // The first post below the top bar; the last one laid out reaching under the
            // navigation bar, within the list.
            let items = placed_items(&mut ui);
            let (first, last) = (items[0], items[items.len() - 1]);
            assert_eq!(first[1], bar, "{name}: its first post");
            let under = (HEIGHT - NAVIGATION_BAR) as f32;
            assert!(last[1] < HEIGHT as f32 && last[3] > under, "{name}: its last post {last:?}");
        }
    }
    // The feeds, the big screen, the stress list and the grid.
    assert_eq!(scrolling, 5);
}
