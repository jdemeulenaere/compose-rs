//! The top app bars (small, center-aligned, medium, large) and their scroll behaviors, nested
//! scrolling, and the bottom app bar.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::{ScrollModifiers, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, Offset, PointerEvent, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

#[test]
fn a_top_app_bar_is_64_dp_its_title_after_the_navigation_icon() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || {
        MaterialTheme!({
            Column!({
                TopAppBar!(
                    { Text!("Plain", modifier = Modifier.test_tag("plain")) },
                    modifier = Modifier.test_tag("bar")
                );
                TopAppBar!(
                    { Text!("With icon", modifier = Modifier.test_tag("after icon")) },
                    navigation_icon = {
                        IconButton!(|| {}, {
                            Icon!(icons::filled::menu(), Some("Menu"));
                        });
                    }
                );
                CenterAlignedTopAppBar!(
                    { Text!("Centered", modifier = Modifier.test_tag("centered")) },
                    navigation_icon = {
                        IconButton!(|| {}, {
                            Icon!(icons::filled::arrow_back(), Some("Back"));
                        });
                    }
                );
            });
        });
    });
    let bar = bounds(&mut ui, "bar");
    assert_eq!((bar.width(), bar.height()), (400.0, 64.0));
    // 16 dp in, or after the icon (4 dp, a 48 dp button, the title's own 4 dp).
    assert_eq!(bounds(&mut ui, "plain").left, 16.0);
    assert_eq!(bounds(&mut ui, "after icon").left, 56.0);
    let centered = bounds(&mut ui, "centered");
    assert!(((centered.left + centered.right) / 2.0 - 200.0).abs() <= 1.0, "{centered:?}");
}

/// A top app bar is a repaint boundary, as Compose's (its `Surface` clips with a graphics
/// layer): a frame drawing something else again (an animation beside it) keeps the bar's
/// drawing instead of drawing, sending and replaying it again.
#[test]
fn a_top_app_bar_keeps_its_drawing_while_what_is_beside_it_draws_again() {
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || {
        MaterialTheme!({
            Column!({
                TopAppBar!({ Text!("Bar") });
                Box!(
                    modifier = Modifier.size(10.dp()).draw_behind(move |scope| {
                        let alpha = (handle.get() % 2) as f32 * 0.5 + 0.5;
                        let color = compose_ui::Color::BLACK.with_alpha(alpha);
                        scope.draw_rect(scope.bounds(), compose_ui::Paint::fill(color));
                    })
                );
            });
        });
    });
    let before = ui.host().display_list();
    assert_eq!(before.groups.len(), 1, "the bar is a group: {}", ui.dump_display_list());
    tick.set(1);
    ui.wait_for_idle();
    let after = ui.host().display_list();
    assert_ne!(before.ops, after.ops, "the box drew again");
    assert_eq!(
        (after.groups[0].key, after.groups[0].version),
        (before.groups[0].key, before.groups[0].version),
        "the bar kept its drawing"
    );
}

/// A scaffold whose content scrolls under a bar of `kind` (0 pinned, 1 enter-always, 2
/// exit-until-collapsed), medium unless `small`.
#[composable]
fn Screen(kind: u8, small: bool) {
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
                Column!(
                    modifier = Modifier
                        .padding(padding)
                        .fill_max_size()
                        .vertical_scroll(scroll)
                        .test_tag("content"),
                    {
                        for index in 0..40 {
                            Text!("Item {}", index, modifier = Modifier.height(40.dp()));
                        }
                    }
                );
            }
        );
    });
}

fn wheel(ui: &mut ComposeTestRule, delta: f32) {
    ui.host()
        .send_pointer(PointerEvent::scroll(Offset::new(200.0, 400.0), Offset::new(0.0, delta)));
    ui.advance_time_by(Duration::from_millis(32));
}

fn height(ui: &mut ComposeTestRule) -> f32 {
    bounds(ui, "bar").height()
}

fn content_top(ui: &mut ComposeTestRule) -> f32 {
    bounds(ui, "content").top
}

#[test]
fn medium_and_large_bars_are_112_and_152_dp() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), || {
        MaterialTheme!({
            Column!({
                MediumTopAppBar!({ Text!("Medium") }, modifier = Modifier.test_tag("medium"));
                LargeTopAppBar!({ Text!("Large") }, modifier = Modifier.test_tag("large"));
            });
        });
    });
    assert_eq!(bounds(&mut ui, "medium").height(), 112.0);
    assert_eq!(bounds(&mut ui, "large").height(), 152.0);
    // Expanded, the big title is the one read.
    assert_eq!(ui.count_nodes(&has_text("Medium")), 1);
}

#[test]
fn exit_until_collapsed_collapses_then_expands_at_the_start() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(2, false));
    ui.set_auto_advance(false);
    assert_eq!(height(&mut ui), 112.0);
    // The bar takes the scroll first: the content doesn't move until it's collapsed.
    wheel(&mut ui, 30.0);
    assert_eq!(height(&mut ui), 82.0);
    assert_eq!(content_top(&mut ui), 82.0);
    // Collapsing takes the whole delta, as Compose's.
    wheel(&mut ui, 100.0);
    assert_eq!(height(&mut ui), 64.0);
    wheel(&mut ui, 100.0);
    // Then the content scrolls (the column moves up under the bar).
    assert_eq!((height(&mut ui), content_top(&mut ui)), (64.0, 64.0 - 100.0));
    // Scrolling back: the content first, then the bar once it's at its start.
    wheel(&mut ui, -20.0);
    assert_eq!(height(&mut ui), 64.0);
    wheel(&mut ui, -200.0);
    assert_eq!(height(&mut ui), 112.0);
    // Collapsed, the small title is the one read.
    wheel(&mut ui, 200.0);
    assert_eq!(height(&mut ui), 64.0);
    assert_eq!(ui.count_nodes(&has_text("Title")), 1);
}

#[test]
fn enter_always_comes_back_as_soon_as_the_content_scrolls_back() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(1, true));
    ui.set_auto_advance(false);
    assert_eq!(height(&mut ui), 64.0);
    wheel(&mut ui, 300.0);
    assert_eq!(height(&mut ui), 0.0);
    // Far from the content's start, scrolling back shows it at once.
    wheel(&mut ui, -24.0);
    assert_eq!(height(&mut ui), 24.0);
}

#[test]
fn a_pinned_bar_stays_and_takes_the_scrolled_color() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(0, true));
    ui.set_auto_advance(false);
    let scheme = light_color_scheme();
    let fills = |ui: &mut ComposeTestRule| {
        ui.host()
            .display_list()
            .ops
            .iter()
            .filter_map(|op| match op {
                compose_ui::platform::DrawOp::DrawRect(_, paint) => Some(paint.color),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(fills(&mut ui).contains(&scheme.surface));
    wheel(&mut ui, 200.0);
    ui.advance_time_by(Duration::from_secs(1));
    assert_eq!(height(&mut ui), 64.0);
    assert!(fills(&mut ui).contains(&scheme.surface_container));
}

#[test]
fn a_drag_collapses_the_bar_and_a_release_halfway_settles_it() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || Screen(2, false));
    ui.set_auto_advance(false);
    // Up 8 dp past the touch slop, then 30 more, slowly.
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 400.0)));
    for y in [392.0, 382.0, 372.0, 362.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(200.0, y), true));
        ui.advance_time_by(Duration::from_millis(100));
    }
    assert_eq!(height(&mut ui), 112.0 - 30.0);
    ui.advance_time_by(Duration::from_millis(200));
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, 362.0)));
    ui.advance_time_by(Duration::from_secs(1));
    // Past half way: collapsed.
    assert_eq!(height(&mut ui), 64.0);
}

#[test]
fn a_bottom_app_bar_is_80_dp_its_fab_at_the_end() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || {
        MaterialTheme!({
            BottomAppBar!(
                |_| {
                    IconButton!(|| {}, modifier = Modifier.test_tag("action"), {
                        Icon!(icons::filled::check(), Some("Check"));
                    });
                },
                modifier = Modifier.test_tag("bar"),
                floating_action_button = {
                    FloatingActionButton!(
                        || {},
                        modifier = Modifier.test_tag("fab"),
                        container_color = BottomAppBarDefaults::bottom_app_bar_fab_color(),
                        elevation = FloatingActionButtonDefaults::bottom_app_bar_fab_elevation(),
                        {
                            Icon!(icons::filled::add(), Some("Add"));
                        }
                    );
                }
            );
        });
    });
    let bar = bounds(&mut ui, "bar");
    assert_eq!((bar.width(), bar.height()), (400.0, 80.0));
    let fab = bounds(&mut ui, "fab");
    // 12 dp from the bar's top and 16 dp from its end, its 4 dp padding included.
    assert_eq!((fab.right, fab.top), (400.0 - 16.0, 12.0));
    // The action at the start, after the 4 dp padding (and its touch target's 4).
    assert_eq!(bounds(&mut ui, "action").left, 8.0);
}
