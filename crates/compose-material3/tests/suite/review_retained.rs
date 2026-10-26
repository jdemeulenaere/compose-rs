//! Adversarial review of retained drawing per repaint boundary (docs/06-ui.md, "Repaint
//! boundaries"): Material components in repaint boundaries, each frame of their interactions and
//! animations drawing what drawing everything again at that moment does.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, Offset, PointerEvent, UnitExt};

/// The display list the last frame drew, and the one drawing every boundary again draws now.
fn retained_and_redrawn(ui: &mut ComposeTestRule) -> (String, String) {
    let retained = ui.host().display_list().dump();
    let redrawn = ui.host().with_tree(|tree| {
        tree.invalidate_draw();
        tree.draw();
        tree.display_list().dump()
    });
    (retained, redrawn)
}

fn check_frames(ui: &mut ComposeTestRule, frames: usize, what: &str) {
    for frame in 0..frames {
        ui.advance_time_by_frame();
        let (retained, redrawn) = retained_and_redrawn(ui);
        assert_eq!(retained, redrawn, "{what}, frame {frame}");
    }
}

fn center(ui: &mut ComposeTestRule, tag: &str) -> Offset {
    let bounds = ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds;
    Offset::new((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0)
}

#[composable]
fn Screen() {
    let on = remember(|| mutable_state_of(false));
    let checked = remember(|| mutable_state_of(false));
    let value = remember(|| mutable_state_of(0.2f32));
    let tab = remember(|| mutable_state_of(0usize));
    let item = remember(|| mutable_state_of(0usize));
    let text = remember(|| mutable_state_of(String::new()));
    MaterialTheme!({
        Column!({
            Box!(modifier = Modifier.repaint_boundary(), {
                Switch!(
                    on.get(),
                    Some(move |now| on.set(now)),
                    modifier = Modifier.test_tag("switch")
                );
            });
            Box!(modifier = Modifier.repaint_boundary(), {
                Checkbox!(
                    checked = checked.get(),
                    on_checked_change = Some(move |now| checked.set(now)),
                    modifier = Modifier.test_tag("checkbox"),
                );
            });
            Box!(modifier = Modifier.repaint_boundary(), {
                Slider!(
                    value.get(),
                    move |new| value.set(new),
                    modifier = Modifier.width(200.dp()).test_tag("slider"),
                );
            });
            Box!(modifier = Modifier.repaint_boundary(), {
                IndeterminateCircularProgressIndicator!();
            });
            Box!(modifier = Modifier.repaint_boundary(), {
                OutlinedTextField!(
                    text.get(),
                    move |new| text.set(new),
                    modifier = Modifier.test_tag("field"),
                    label = Some(|| Text!("City")),
                    single_line = true,
                );
            });
            Box!(modifier = Modifier.repaint_boundary(), {
                SecondaryTabRow!(tab.get(), modifier = Modifier.test_tag("tabs"), {
                    for (index, name) in ["One", "Two"].into_iter().enumerate() {
                        Tab!(
                            tab.get() == index,
                            move || tab.set(index),
                            modifier = Modifier.test_tag(name),
                            text = Some(move || Text!("{}", name)),
                        );
                    }
                });
            });
            Box!(modifier = Modifier.repaint_boundary(), {
                NavigationBar!(|row: RowScope| {
                    for (index, name) in ["Home", "Search"].into_iter().enumerate() {
                        NavigationBarItem!(
                            row,
                            item.get() == index,
                            move || item.set(index),
                            { Icon!(icons::filled::star(), None) },
                            modifier = Modifier.test_tag(name),
                            label = Some(move || Text!("{}", name)),
                        );
                    }
                });
            });
        });
    });
}

#[test]
fn material_components_in_boundaries_draw_each_frame_what_drawing_everything_does() {
    let mut ui = ComposeTestRule::with_manual_clock(Screen);
    ui.host().resize(IntSize::new(400, 900));
    ui.wait_for_idle();
    check_frames(&mut ui, 3, "at rest");
    let switch = center(&mut ui, "switch");
    ui.host().send_pointer(PointerEvent::press(switch));
    check_frames(&mut ui, 2, "switch pressed");
    ui.host().send_pointer(PointerEvent::release(switch));
    check_frames(&mut ui, 30, "switch sliding");
    ui.on_node_with_tag("switch").assert_is_on();
    assert!(ui.host().display_list().groups.len() >= 7, "{}", ui.dump_display_list());
    let checkbox = center(&mut ui, "checkbox");
    ui.host().send_pointer(PointerEvent::press(checkbox));
    ui.host().send_pointer(PointerEvent::release(checkbox));
    check_frames(&mut ui, 20, "checkbox checked");
    ui.on_node_with_tag("checkbox").assert_is_on();
    let slider = center(&mut ui, "slider");
    ui.host().send_pointer(PointerEvent::press(slider));
    for step in 1..6 {
        let at = Offset::new(slider.x + step as f32 * 12.0, slider.y);
        ui.host().send_pointer(PointerEvent::moved(at, true));
        check_frames(&mut ui, 1, "slider dragged");
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(slider.x + 60.0, slider.y)));
    check_frames(&mut ui, 5, "slider released");
    let field = center(&mut ui, "field");
    ui.host().send_pointer(PointerEvent::press(field));
    ui.host().send_pointer(PointerEvent::release(field));
    check_frames(&mut ui, 20, "label floating");
    ui.host().send_text("Paris");
    check_frames(&mut ui, 3, "typed");
    let two = center(&mut ui, "Two");
    ui.host().send_pointer(PointerEvent::press(two));
    ui.host().send_pointer(PointerEvent::release(two));
    check_frames(&mut ui, 30, "tab indicator sliding");
    let search = center(&mut ui, "Search");
    ui.host().send_pointer(PointerEvent::moved(search, false));
    check_frames(&mut ui, 3, "navigation item hovered");
    ui.host().send_pointer(PointerEvent::press(search));
    ui.host().send_pointer(PointerEvent::release(search));
    check_frames(&mut ui, 20, "navigation item selected");
    let home = center(&mut ui, "Home");
    ui.host().send_pointer(PointerEvent::moved(home, false));
    check_frames(&mut ui, 3, "hover moved");
    ui.host().send_pointer_left();
    check_frames(&mut ui, 3, "mouse gone");
}
