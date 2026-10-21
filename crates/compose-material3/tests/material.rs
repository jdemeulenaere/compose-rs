//! Material components through the test rule: theme values reaching content, enabled and
//! disabled states, sizes from the Material defaults.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, IntSize, Modifier, Role, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

#[test]
fn a_button_follows_the_material_sizes_and_states() {
    let enabled = mutable_state_of(true);
    let clicks = mutable_state_of(0);
    let (enabled_handle, clicks_handle) = (enabled.handle(), clicks.handle());
    #[composable]
    fn Screen(enabled: MutableState<bool>, clicks: MutableState<u32>) {
        Button!(
            on_click = move || clicks.update(|count| count + 1),
            modifier = Modifier.test_tag("button"),
            enabled = enabled.get(),
            {
                Text!("Go");
            }
        );
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Screen(enabled_handle, clicks_handle)
    });
    // 24 dp padding each side around 2 × 7 px ("Go", 14 sp), at least 58 dp; 40 dp tall within a
    // 48 dp touch target.
    assert_eq!(bounds(&mut ui, "button").width(), 62.0);
    ui.on_node_with_tag("button").assert_is_enabled().perform_click();
    assert_eq!(clicks.get(), 1);
    enabled.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("button").assert_is_not_enabled().perform_click();
    assert_eq!(clicks.get(), 1);
    let roles = ui.on_node_with_tag("button").fetch_semantics_node().config.role;
    assert_eq!(roles, Some(Role::Button));
}

#[test]
fn the_theme_reaches_text_and_buttons() {
    #[composable]
    fn Screen() {
        MaterialTheme!(color_scheme = dark_color_scheme(), {
            Surface!({
                Text!("on surface", modifier = Modifier.test_tag("text"));
            });
            Button!(on_click = || {}, {
                Text!("label");
            });
        });
    }
    let ui = compose_test(Screen);
    let display = ui.dump_display_list();
    // The dark surface fills the box.
    let surface = format!("{:?}", dark_color_scheme().surface);
    assert!(display.contains(&surface), "{display}");
    let button = format!("{:?}", dark_color_scheme().primary);
    assert!(display.contains(&button), "{display}");
}

#[test]
fn text_takes_its_style_from_the_theme() {
    #[composable]
    fn Screen() {
        Text!("body", modifier = Modifier.test_tag("body"));
        Text!(
            "title",
            modifier = Modifier.test_tag("title"),
            style = MaterialTheme::typography().title_large
        );
        Text!("big", modifier = Modifier.test_tag("big"), font_size = 30.sp());
    }
    use compose_foundation::layout::Column;
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                Screen();
            });
        });
    });
    // Body large: 16 sp text, 24 sp lines (the fixed measurer: half the size per character).
    assert_eq!(bounds(&mut ui, "body").height(), 24.0);
    assert_eq!(bounds(&mut ui, "body").width(), 32.0);
    assert_eq!(bounds(&mut ui, "title").height(), 28.0);
    assert_eq!(bounds(&mut ui, "big").width(), 45.0);
}

#[test]
fn a_checkbox_toggles_in_a_touch_target() {
    #[composable]
    fn Screen(checked: MutableState<bool>) {
        Checkbox!(
            checked = checked.get(),
            on_checked_change = move |value| checked.set(value),
            modifier = Modifier.test_tag("box"),
        );
        Checkbox!(
            checked = true,
            on_checked_change = None,
            modifier = Modifier.test_tag("read only")
        );
    }
    let checked = mutable_state_of(false);
    let handle = checked.handle();
    let mut ui = compose_test(move || Screen(handle));
    assert_eq!(bounds(&mut ui, "box").width(), 48.0);
    ui.on_node_with_tag("box").assert_is_off().perform_click();
    ui.on_node_with_tag("box").assert_is_on();
    assert!(checked.get());
    // Checked: a filled box and a check mark in the theme's colors.
    let display = ui.dump_display_list();
    assert!(display.contains("line"), "{display}");
    assert!(display.contains(&format!("{:?}", light_color_scheme().primary)), "{display}");
    let _ = Color::BLACK;
}

#[test]
fn a_text_field_shows_its_label_and_placeholder() {
    #[composable]
    fn Screen(text: MutableState<String>) {
        TextField!(
            value = text.get(),
            on_value_change = move |value| text.set(value),
            modifier = Modifier.test_tag("field"),
            label = {
                Text!("Name");
            },
            placeholder = {
                Text!("Your name");
            },
        );
    }
    let text = mutable_state_of(String::new());
    let handle = text.handle();
    let mut ui = compose_test(move || Screen(handle));
    assert!(bounds(&mut ui, "field").height() >= 56.0);
    ui.on_node_with_tag("field").assert_text_equals(&["", "Name", "Your name"]);
    ui.on_node_with_tag("field").perform_text_input("Ada");
    assert_eq!(text.get(), "Ada");
    ui.on_node_with_tag("field").assert_text_equals(&["Ada", "Name"]);
}

#[test]
fn a_pressed_button_shows_its_state_layer() {
    #[composable]
    fn Screen() {
        Button!(on_click = || {}, modifier = Modifier.test_tag("button"), {
            Text!("Go");
        });
    }
    let mut ui = compose_test(Screen);
    let layer = format!("{:?}", light_color_scheme().on_primary.with_alpha(0.10));
    assert!(!ui.dump_display_list().contains(&layer));
    let center = ui.on_node_with_tag("button").fetch_semantics_node().center();
    ui.host().send_pointer(compose_ui::PointerEvent::press(center));
    ui.wait_for_idle();
    assert!(ui.dump_display_list().contains(&layer), "{}", ui.dump_display_list());
    ui.host().send_pointer(compose_ui::PointerEvent::release(center));
    ui.wait_for_idle();
    assert!(!ui.dump_display_list().contains(&layer));
}
