//! `Modifier.shadow`: an elevation's shadows under a shape, behind the content.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{Color, DrawOp, IntSize, Outline, Rect, UnitExt};

#[test]
fn a_shadow_is_drawn_behind_the_clipped_content() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Box!(
            modifier = Modifier
                .padding(10.dp())
                .shadow(4.dp(), RECTANGLE_SHAPE, true)
                .background(Color::WHITE)
                .size(50.dp())
        );
    });
    let list = ui.host().display_list();
    let shadow = list.ops.iter().position(|op| matches!(op, DrawOp::DrawShadow { .. }));
    let clip = list.ops.iter().position(|op| matches!(op, DrawOp::ClipRect(..)));
    let fill = list.ops.iter().position(|op| matches!(op, DrawOp::DrawRect(..)));
    assert!(shadow < clip && clip < fill, "{}", list.dump());
    let Some(DrawOp::DrawShadow { outline, elevation, ambient, spot, light, .. }) =
        shadow.map(|index| &list.ops[index])
    else {
        unreachable!()
    };
    assert_eq!(*outline, Outline::Rectangle(Rect::new(10.0, 10.0, 60.0, 60.0)));
    assert_eq!(*elevation, 4.0);
    assert_eq!((ambient.alpha(), spot.alpha()), (10, 48), "Compose's 3.9 % and 19 %");
    assert_eq!(*light, (0.0, -300.0, 600.0));
}

#[test]
fn no_elevation_draws_no_shadow() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Box!(modifier = Modifier.shadow(0.dp(), RECTANGLE_SHAPE, false).size(50.dp()));
    });
    let list = ui.host().display_list();
    assert!(!list.ops.iter().any(|op| matches!(op, DrawOp::DrawShadow { .. })));
}
