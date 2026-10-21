//! Right-to-left layout review: failing tests for animations and Material layouts in a
//! right-to-left layout (docs/06-ui.md, "Layout direction"; D139). Each test states the input,
//! what Jetpack Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, LayoutDirection, Modifier, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
}

/// Input: right to left, a `Crossfade` given `Modifier.width(100.dp())` (at the window's right,
/// 100..200) showing a 20 px box.
///
/// Compose: `Crossfade` stacks its contents in a `Box` (top start): in right to left the box is
/// at the crossfade's right, 180..200.
///
/// Instead: Crossfade's `StackLayout` (crates/compose-animation/src/crossfade.rs) places each
/// content with `place(0, 0)`, not `place_relative`: the box is at the crossfade's left, 100.
#[test]
fn a_crossfade_puts_its_content_at_the_right() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Crossfade!(0u32, modifier = Modifier.width(100.dp()), |_| {
            Box!(modifier = Modifier.size(20.dp()).test_tag("content"));
        });
    });
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "content").left, 180.0);
}

#[composable]
fn Growing(wide: MutableState<bool>) {
    Box!(
        modifier = Modifier
            .test_tag("animated")
            .animate_content_size_with(tween(100).easing(LINEAR_EASING)),
        {
            Box!(
                modifier = Modifier
                    .size_wh(if wide.get() { 100.dp() } else { 50.dp() }, 20.dp())
                    .test_tag("content")
            );
        }
    );
}

/// Input: right to left, a box with `animate_content_size` at the window's right whose content
/// grows from 50 to 100 px wide; looked at midway.
///
/// Compose: `animateContentSize(alignment = Alignment.TopStart)` places its content at
/// `alignment.align(content, animated size, layoutDirection)`: in right to left the content's
/// right edge stays on the animated box's right edge (the window's, 400), its left part
/// clipped.
///
/// Instead: `ContentSizeNode::measure` (crates/compose-animation/src/content_size.rs) places
/// the content with `place(0, 0)`: its left edge on the animated box's left, its start (right
/// part) past the window's edge and clipped.
#[test]
fn a_content_size_animation_keeps_the_contents_start_at_the_right() {
    let wide = mutable_state_of(false);
    let handle = wide.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Growing(handle));
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "content").right, 400.0);
    wide.set(true);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(48));
    let animated = bounds(&mut ui, "animated");
    assert!(animated.width() > 50.0 && animated.width() < 100.0, "midway: {animated:?}");
    assert_eq!(animated.right, 400.0);
    assert_eq!(bounds(&mut ui, "content").right, 400.0, "the content's start on the right");
}

#[composable]
fn Screen() {
    MaterialTheme!({
        Scaffold!(
            top_bar = || {
                TopAppBar!(
                    { Text!("Title", modifier = Modifier.test_tag("title")) },
                    navigation_icon = Some(|| {
                        IconButton!(|| {}, modifier = Modifier.test_tag("nav"), { Text!("<") });
                    }),
                );
            },
            |padding: PaddingValues| {
                Column!(modifier = Modifier.padding_values(padding), {
                    TextField!(
                        String::from("abc"),
                        |_| {},
                        modifier = Modifier.test_tag("field"),
                        label = Some(|| Text!("Label", modifier = Modifier.test_tag("label"))),
                    );
                    Switch!(true, Some(|_| {}), modifier = Modifier.test_tag("switch"));
                    Slider!(0.5, |_| {}, modifier = Modifier.width(200.dp()).test_tag("slider"));
                    Box!(modifier = Modifier.size(40.dp()).test_tag("anchor"), {
                        DropdownMenu!(true, || {}, modifier = Modifier.test_tag("menu"), {
                            DropdownMenuItem!(
                                { Text!("One") },
                                || {},
                                modifier = Modifier.test_tag("item"),
                            );
                        });
                    });
                });
            },
        );
    });
}

/// Input: a Material screen (a top app bar, a text field with a label, a switch, a slider and
/// an open menu) laid out left to right, then right to left, then left to right again.
///
/// Compose: everything mirrors, then comes back where it was.
#[test]
fn passes_a_screen_turned_right_to_left_and_back_comes_back_where_it_was() {
    let tags = ["title", "nav", "field", "label", "switch", "slider", "anchor", "menu", "item"];
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), Screen);
    let before: Vec<_> = tags.iter().map(|tag| bounds(&mut ui, tag)).collect();
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.wait_for_idle();
    let mirrored: Vec<_> = tags.iter().map(|tag| bounds(&mut ui, tag)).collect();
    for ((tag, before), mirrored) in tags.iter().zip(&before).zip(&mirrored) {
        if *tag == "menu" || *tag == "item" {
            continue;
        }
        assert_eq!(
            (mirrored.left, mirrored.right),
            (400.0 - before.right, 400.0 - before.left),
            "{tag} mirrored"
        );
    }
    // The menu at its anchor's right edge, the window's.
    assert_eq!(mirrored[7].right, 400.0, "menu at the anchor's right");
    ui.host().set_layout_direction(LayoutDirection::Ltr);
    ui.wait_for_idle();
    let after: Vec<_> = tags.iter().map(|tag| bounds(&mut ui, tag)).collect();
    assert_eq!(before, after);
}
