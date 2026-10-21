//! Adversarial review of text overflow and custom drawing (`draw_with_content`), against
//! Compose's `BasicText` and `Modifier.drawWithContent`.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{DrawOp, IntSize, Paint, Rect, UnitExt};

fn fills(ui: &mut ComposeTestRule) -> Vec<Color> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(_, paint) => Some(paint.color),
            _ => None,
        })
        .collect()
}

/// Compose's text layout makes an ellipsized text without soft wrap one line
/// (`finalMaxLines`: `if (!softWrap && overflow == TextOverflow.Ellipsis) 1 else maxLines`), as
/// the Skia measurer does. The fixed-advance measurer (the test rule's) ignores the overflow: a
/// text with a newline stays two lines high, so layout tests disagree with the real text stack.
#[test]
fn an_ellipsized_text_without_soft_wrap_is_one_line() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), || {
        BasicText!(
            "first\nsecond",
            modifier = Modifier.test_tag("text"),
            overflow = TextOverflow::Ellipsis,
            soft_wrap = false,
        );
    });
    let bounds = ui.on_node_with_tag("text").fetch_semantics_node().bounds;
    // One line of 14 sp text: 17.5 px, rounded up.
    assert_eq!(bounds.height(), 18.0, "{bounds:?}");
}

/// Lock-in: state read after `draw_content` (around a child that draws under its own
/// observation) still draws again when written.
#[test]
fn passes_state_read_after_draw_content_draws_again() {
    let color = mutable_state_of(Color::RED);
    let inner = mutable_state_of(Color::BLUE);
    let (outer_handle, inner_handle) = (color.handle(), inner.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Box!(
            modifier = Modifier.size(50.dp()).draw_with_content(move |scope| {
                scope.draw_content();
                scope.draw_rect(Rect::new(0.0, 0.0, 5.0, 5.0), Paint::fill(outer_handle.get()));
            }),
            {
                Canvas(Modifier.size(10.dp()), move |scope| {
                    let bounds = scope.bounds();
                    scope.draw_rect(bounds, Paint::fill(inner_handle.get()));
                });
            }
        );
    });
    assert_eq!(fills(&mut ui), vec![Color::BLUE, Color::RED]);
    color.set(Color::GREEN);
    ui.wait_for_idle();
    assert_eq!(fills(&mut ui), vec![Color::BLUE, Color::GREEN]);
    inner.set(Color::BLACK);
    ui.wait_for_idle();
    assert_eq!(fills(&mut ui), vec![Color::BLACK, Color::GREEN]);
}

/// Lock-in: a clip and a layer around `draw_with_content`, and nested ones, save and restore
/// in pairs, the content drawn where asked (twice when asked twice).
#[test]
fn passes_draw_with_content_balances_saves_around_it() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        Box!(
            modifier = Modifier
                .size(50.dp())
                .clip(compose_ui::RECTANGLE_SHAPE)
                .draw_with_content(|scope| {
                    scope.draw_content();
                    scope.draw_content();
                })
                .alpha(0.5)
                .draw_with_content(|scope| {
                    scope.draw_rect(Rect::new(0.0, 0.0, 5.0, 5.0), Paint::fill(Color::RED));
                    scope.draw_content();
                })
                .background(Color::BLUE),
            {
                Box!(modifier = Modifier.size(10.dp()).background(Color::GREEN));
            }
        );
    });
    let list = ui.host().display_list();
    let (mut depth, mut max) = (0i32, 0i32);
    for op in &list.ops {
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha(_) => depth += 1,
            DrawOp::Restore => depth -= 1,
            _ => {}
        }
        assert!(depth >= 0, "{}", list.dump());
        max = max.max(depth);
    }
    assert_eq!(depth, 0, "{}", list.dump());
    assert_eq!(max, 2, "{}", list.dump());
    assert_eq!(
        fills(&mut ui),
        vec![Color::RED, Color::BLUE, Color::GREEN, Color::RED, Color::BLUE, Color::GREEN]
    );
}
