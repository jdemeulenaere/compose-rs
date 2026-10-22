//! Right-to-left layout: rows, alignments, padding and offsets mirrored, as Compose's.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, UnitExt};

fn rtl(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), content);
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.wait_for_idle();
    ui
}

fn left(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds.left
}

#[test]
fn a_row_starts_at_the_right() {
    let mut ui = rtl(|| {
        Row!({
            Box!(modifier = Modifier.size(20.dp()).test_tag("first"));
            Box!(modifier = Modifier.size(30.dp()).test_tag("second"));
        });
    });
    // The row is as wide as its children (50), at the window's start: its right.
    assert_eq!(left(&mut ui, "first"), 180.0);
    assert_eq!(left(&mut ui, "second"), 150.0);
}

#[test]
fn start_alignments_and_padding_are_at_the_right() {
    let mut ui = rtl(|| {
        Column!(modifier = Modifier.fill_max_size(), {
            Box!(modifier = Modifier.size(20.dp()).test_tag("start"));
            Box!(
                modifier = Modifier
                    .fill_max_width()
                    .padding((10.dp(), 0.dp(), 30.dp(), 0.dp()))
                    .height(10.dp())
                    .test_tag("padded")
            );
            Box!(modifier = Modifier.fill_max_width().height(100.dp()), |scope: BoxScope| {
                Box!(
                    modifier = Modifier
                        .align(scope, Alignment::TOP_START)
                        .size(10.dp())
                        .test_tag("box start")
                );
            });
            Box!(modifier = Modifier.offset(5.dp(), 0.dp()).size(10.dp()).test_tag("offset"));
        });
    });
    assert_eq!(left(&mut ui, "start"), 180.0);
    let padded = ui.on_node_with_tag("padded").fetch_semantics_node().bounds;
    // Inside the padding: its start (10) on the right, its end (30) on the left.
    assert_eq!((padded.left, padded.right), (30.0, 190.0));
    assert_eq!(left(&mut ui, "box start"), 190.0);
    assert_eq!(left(&mut ui, "offset"), 185.0, "an offset moves toward the end: left");
}

#[test]
fn padding_insets_the_content_from_the_start_on_the_right() {
    let mut ui = rtl(|| {
        Box!(modifier = Modifier.width(100.dp()).padding((10.dp(), 0.dp(), 30.dp(), 0.dp())), {
            Box!(modifier = Modifier.fill_max_width().height(10.dp()).test_tag("content"));
        });
    });
    let content = ui.on_node_with_tag("content").fetch_semantics_node().bounds;
    assert_eq!((content.left, content.right), (130.0, 190.0));
}

#[test]
fn a_provided_direction_lays_out_its_content() {
    // The row, left-to-right, is placed by the right-to-left root at its start: the right.
    let mut ui = rtl(|| {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Ltr), || {
            Row!({
                Box!(modifier = Modifier.size(20.dp()).test_tag("first"));
                Box!(modifier = Modifier.size(30.dp()).test_tag("second"));
            });
        });
    });
    assert_eq!(left(&mut ui, "first"), 150.0);
    assert_eq!(left(&mut ui, "second"), 170.0);
}

#[test]
fn a_horizontal_scroll_starts_at_the_right_and_scrolls_the_other_way() {
    use compose_ui::{Offset, PointerEvent};
    let state = ScrollState::new(0);
    let given = *state;
    let mut ui = rtl(move || {
        Row!(modifier = Modifier.width(100.dp()).horizontal_scroll(given), {
            for index in 0..5 {
                Box!(modifier = Modifier.size(50.dp()).test_tag(format!("{index}")));
            }
        });
    });
    // The row at the window's right: 100..200; its first item at its right.
    assert_eq!(left(&mut ui, "0"), 150.0);
    assert_eq!(left(&mut ui, "1"), 100.0);
    // Dragging right scrolls forward, bringing later items in from the left.
    ui.host().send_pointer(PointerEvent::press(Offset::new(120.0, 10.0)));
    ui.host().send_pointer(PointerEvent::moved(Offset::new(190.0, 10.0), true));
    ui.host().send_pointer(PointerEvent::release(Offset::new(190.0, 10.0)));
    ui.wait_for_idle();
    assert!(state.value() > 0, "scrolled forward: {}", state.value());
    assert_eq!(left(&mut ui, "0"), 150.0 + state.value() as f32);
}

#[test]
fn a_lazy_row_starts_at_the_right() {
    let mut ui = rtl(|| {
        LazyRow!(modifier = Modifier.fill_max_width().height(20.dp()), |scope: LazyListScope| {
            scope.items(10, |index| {
                Box!(modifier = Modifier.size(30.dp()).test_tag(format!("{index}")));
            });
        });
    });
    assert_eq!(left(&mut ui, "0"), 170.0);
    assert_eq!(left(&mut ui, "1"), 140.0);
}

#[test]
fn a_popup_aligns_with_its_anchors_start_and_offsets_toward_its_end() {
    use compose_ui::{IntOffset, Popup};
    let mut ui = rtl(|| {
        Box!(modifier = Modifier.size(100.dp()), {
            Popup!(alignment = Alignment::TOP_START, offset = IntOffset::new(10, 0), {
                Box!(modifier = Modifier.size(20.dp()).test_tag("popup"));
            });
        });
    });
    // The anchor at 100..200: the popup's right on its right, then 10 toward the left.
    assert_eq!(left(&mut ui, "popup"), 170.0);
}

#[test]
fn a_column_still_runs_from_the_top() {
    let mut ui = rtl(|| {
        Column!(
            modifier = Modifier.fill_max_size(),
            vertical_arrangement = Arrangement::spaced_by(5.dp()),
            {
                Box!(modifier = Modifier.size(20.dp()).test_tag("first"));
                Box!(modifier = Modifier.size(20.dp()).test_tag("second"));
            }
        );
    });
    assert_eq!(ui.on_node_with_tag("first").fetch_semantics_node().bounds.top, 0.0);
    assert_eq!(ui.on_node_with_tag("second").fetch_semantics_node().bounds.top, 25.0);
}

#[test]
fn lazy_items_align_and_arrange_from_the_start() {
    let mut ui = rtl(|| {
        LazyColumn!(
            modifier = Modifier.fill_max_size(),
            horizontal_alignment = Alignment::START,
            |scope: LazyListScope| {
                scope.items(2, |index| {
                    Box!(modifier = Modifier.size(30.dp()).test_tag(format!("{index}")));
                });
            }
        );
    });
    assert_eq!(left(&mut ui, "0"), 170.0);
    assert_eq!(ui.on_node_with_tag("1").fetch_semantics_node().bounds.top, 30.0);
}
