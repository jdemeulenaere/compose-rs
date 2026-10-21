//! Hover: a mouse over a clickable shows Material's 8 % state layer, gone when it leaves the
//! clickable or the window.

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{DrawOp, IntSize, Offset, PointerEvent, UnitExt};

fn layer_alpha(ui: &mut ComposeTestRule) -> Option<u8> {
    ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawRect(_, paint) if paint.color.red() == 255 && paint.color.alpha() < 255 => {
            Some(paint.color.alpha())
        }
        _ => None,
    })
}

#[test]
fn hovering_a_clickable_shows_its_state_layer() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Column!({
            Box!(
                modifier =
                    Modifier.size(50.dp()).clickable_indicated(true, None, Color::RED, || {})
            );
            Box!(modifier = Modifier.size(50.dp()));
        });
    });
    assert_eq!(layer_alpha(&mut ui), None);
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
    ui.wait_for_idle();
    assert_eq!(layer_alpha(&mut ui), Some(20), "8 %");
    // Pressed: 10 %.
    ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    assert_eq!(layer_alpha(&mut ui), Some(26));
    ui.host().send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    assert_eq!(layer_alpha(&mut ui), Some(20), "still hovered");
    // Moving off it, then leaving the window.
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 70.0), false));
    ui.wait_for_idle();
    assert_eq!(layer_alpha(&mut ui), None);
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
    ui.wait_for_idle();
    assert_eq!(layer_alpha(&mut ui), Some(20));
    ui.host().send_pointer_left();
    ui.wait_for_idle();
    assert_eq!(layer_alpha(&mut ui), None);
}

#[test]
fn a_text_field_shows_an_i_beam() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Column!({
            BasicTextField!(String::new(), |_| {}, modifier = Modifier.size_wh(100.dp(), 30.dp()));
            Box!(
                modifier = Modifier.size(50.dp()).pointer_hover_icon(compose_ui::PointerIcon::Hand)
            );
        });
    });
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
    assert_eq!(ui.host().pointer_icon(), compose_ui::PointerIcon::Text);
    ui.host().send_pointer(PointerEvent::moved(Offset::new(10.0, 50.0), false));
    assert_eq!(ui.host().pointer_icon(), compose_ui::PointerIcon::Hand);
    ui.host().send_pointer(PointerEvent::moved(Offset::new(150.0, 150.0), false));
    assert_eq!(ui.host().pointer_icon(), compose_ui::PointerIcon::Default);
}
