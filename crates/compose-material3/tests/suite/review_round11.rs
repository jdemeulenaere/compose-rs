//! Eleventh review round: Material components' keyboard focus indication, against Jetpack
//! Compose's material3 on desktop. Each test states the input, what Compose does, and what
//! happens instead.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{Key, Modifier, Rect, platform::DrawOp};

/// The rectangles drawn (filled rects and round rects), in the order drawn.
fn drawn_rects(ui: &mut ComposeTestRule) -> Vec<String> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(rect, paint) => Some(format!("rect {rect:?} {:?}", paint.color)),
            DrawOp::DrawRoundRect(rect, radii, paint) => {
                Some(format!("round rect {rect:?} {radii:?} {:?}", paint.color))
            }
            _ => None,
        })
        .collect()
}

fn rect_of(op: &DrawOp) -> Option<Rect> {
    match op {
        DrawOp::DrawRect(rect, _) | DrawOp::DrawRoundRect(rect, _, _) => Some(*rect),
        _ => None,
    }
}

/// Input: a `Checkbox(checked = false, onCheckedChange = {})` (a 20 dp box in its 48 dp touch
/// target), focused with Tab.
///
/// Compose: the checkbox is `triStateToggleable(…, indication = ripple(bounded = false, radius =
/// 20.dp))` (`CheckboxTokens.StateLayerSize / 2`): the focused state layer is a 40 dp circle
/// centered on the box, 4 dp inside the touch target on each side. `RadioButton` and the
/// `Switch`'s thumb do the same (unbounded, 20 dp radius).
///
/// Here: the toggleable's state layer is a plain rectangle over the whole touch target, so the
/// focused checkbox shows a 48 by 48 dp square (`rect 0,0 48x48`) instead of a circle around the
/// box (`RadioButton` and `Switch` put their state layer on the touch target the same way).
#[test]
fn a_focused_checkbox_shows_a_round_state_layer_around_its_box() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Checkbox!(false, Some(|_| {}), modifier = Modifier.test_tag("box"));
        });
    });
    let before = ui.host().display_list().ops;
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("box").assert_is_focused();
    let after = ui.host().display_list().ops;
    let added: Vec<Rect> = after
        .iter()
        .filter(|op| !before.iter().any(|old| format!("{old:?}") == format!("{op:?}")))
        .filter_map(rect_of)
        .collect();
    assert!(!added.is_empty(), "no focus indication drawn: {:?}", drawn_rects(&mut ui));
    assert!(
        added.iter().all(|rect| rect.left >= 3.5
            && rect.top >= 3.5
            && rect.right <= 44.5
            && rect.bottom <= 44.5),
        "the focus indication should be a 40 dp circle centered on the box (4..44); drawn: {:?}",
        drawn_rects(&mut ui)
    );
}

/// Input: a `NavigationBar` with one unselected `NavigationBarItem`, focused with Tab.
///
/// Compose: the item is `selectable(interactionSource, indication = null)` and its indicator
/// pill carries the ripple (`Modifier.indication(MappedInteractionSource(interactionSource),
/// ripple())`), which shows every interaction of that source but the press offset: a focused
/// item draws the 10 % focused state layer on its 64 by 32 dp pill, so keyboard users see
/// which destination has the focus.
///
/// Here: the item's interaction source only carries presses, and the pill draws a state layer
/// only while pressed: a focused navigation item draws exactly what an unfocused one does, so
/// Tab through a navigation bar shows no focus at all.
#[test]
fn a_focused_navigation_item_shows_a_state_layer_on_its_pill() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            NavigationBar!(|row: RowScope| {
                NavigationBarItem!(
                    row,
                    false,
                    || {},
                    { Icon!(icons::filled::star(), None) },
                    modifier = Modifier.test_tag("home"),
                    label = Some(|| Text!("Home")),
                );
            });
        });
    });
    let before = drawn_rects(&mut ui);
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("home").assert_is_focused();
    let after = drawn_rects(&mut ui);
    assert_ne!(before, after, "a focused navigation item draws nothing to show its focus");
}
