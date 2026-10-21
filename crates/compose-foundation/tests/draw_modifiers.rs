//! `draw_behind`, `draw_with_content` and `Canvas`: custom drawing, observed for redraws.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::Cell;
use std::rc::Rc;

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

#[test]
fn draw_behind_draws_under_the_content() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        Box!(
            modifier = Modifier
                .size(50.dp())
                .draw_behind(|scope| {
                    let bounds = scope.bounds();
                    scope.draw_rect(bounds, Paint::fill(Color::RED));
                })
                .background(Color::BLUE)
        );
    });
    assert_eq!(fills(&mut ui), vec![Color::RED, Color::BLUE]);
}

#[test]
fn draw_with_content_draws_around_the_content_or_skips_it() {
    let show = mutable_state_of(true);
    let handle = show.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Box!(
            modifier = Modifier.size(50.dp()).draw_with_content(move |scope| {
                scope.draw_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::fill(Color::RED));
                if handle.get() {
                    scope.draw_content();
                }
                scope.draw_rect(Rect::new(0.0, 0.0, 5.0, 5.0), Paint::fill(Color::GREEN));
            }),
            {
                Box!(modifier = Modifier.size(20.dp()).background(Color::BLUE));
            }
        );
    });
    assert_eq!(fills(&mut ui), vec![Color::RED, Color::BLUE, Color::GREEN]);
    // Read while drawing: a write draws again, without recomposing.
    show.set(false);
    ui.wait_for_idle();
    assert_eq!(fills(&mut ui), vec![Color::RED, Color::GREEN]);
}

#[test]
fn a_canvas_redraws_what_it_read_without_recomposing() {
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let compositions = Rc::new(Cell::new(0));
    let counted = compositions.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        counted.set(counted.get() + 1);
        Canvas(Modifier.size(30.dp()), move |scope| {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(handle.get()));
        });
    });
    assert_eq!(fills(&mut ui), vec![Color::RED]);
    color.set(Color::BLUE);
    ui.wait_for_idle();
    assert_eq!(fills(&mut ui), vec![Color::BLUE]);
    assert_eq!(compositions.get(), 1, "only drawn again");
}
