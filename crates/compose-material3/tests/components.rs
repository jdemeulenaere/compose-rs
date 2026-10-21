//! Icons, icon buttons, the floating action button, switches, radio buttons, sliders, cards,
//! dividers, the top app bar and the scaffold.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::Cell;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{DrawOp, IntSize, Key, Modifier, Offset, PointerEvent, Role, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

#[test]
fn an_icon_is_24_dp_and_described() {
    let mut ui = compose_test(|| {
        Icon!(icons::filled::add(), Some("Add"), modifier = Modifier.test_tag("icon"));
    });
    assert_eq!(bounds(&mut ui, "icon").width(), 24.0);
    let node = ui.on_node_with_tag("icon").fetch_semantics_node();
    assert_eq!(node.config.content_description.first().map(|text| text.as_str()), Some("Add"));
    assert_eq!(node.config.role, Some(Role::Image));
    let paths = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::DrawPath { .. }))
        .count();
    assert_eq!(paths, 1);
}

#[test]
fn every_icon_parses() {
    use icons::filled::*;
    for icon in [
        add(),
        arrow_back(),
        check(),
        close(),
        delete(),
        edit(),
        favorite(),
        home(),
        info(),
        menu(),
        more_vert(),
        person(),
        search(),
        star(),
    ] {
        assert!(!icon.paths[0].0.commands().is_empty(), "{}", icon.name);
    }
}

#[test]
fn an_icon_button_is_40_dp_in_a_48_dp_target_and_clicks() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let mut ui = compose_test(|| {
        IconButton!(
            on_click = || CLICKS.with(|clicks| clicks.set(clicks.get() + 1)),
            modifier = Modifier.test_tag("button"),
            {
                Icon!(icons::filled::menu(), Some("Menu"));
            }
        );
    });
    // As Compose's: 40 dp, in a 48 dp touch target.
    assert_eq!(bounds(&mut ui, "button").width(), 40.0);
    ui.on_node_with_tag("button").perform_click();
    assert_eq!(CLICKS.with(Cell::get), 1);
}

#[test]
fn a_switch_toggles_and_its_thumb_slides() {
    #[composable]
    fn Screen() {
        let on = remember(|| mutable_state_of(false));
        MaterialTheme!({
            Switch!(
                on.get(),
                Some(move |checked| on.set(checked)),
                modifier = Modifier.test_tag("switch")
            );
        });
    }
    let mut ui = ComposeTestRule::with_manual_clock(Screen);
    assert_eq!(bounds(&mut ui, "switch").width(), 52.0);
    ui.on_node_with_tag("switch").assert_is_off();
    ui.on_node_with_tag("switch").perform_click();
    ui.on_node_with_tag("switch").assert_is_on();
    // The thumb (the last shape drawn) moves right over a few frames.
    let thumb = |ui: &mut ComposeTestRule| {
        ui.host()
            .display_list()
            .ops
            .iter()
            .rev()
            .find_map(|op| match op {
                DrawOp::DrawRoundRect(rect, _, _) => Some(rect.left),
                _ => None,
            })
            .unwrap()
    };
    let first = thumb(&mut ui);
    ui.advance_time_by(Duration::from_millis(48));
    let midway = thumb(&mut ui);
    ui.advance_time_by(Duration::from_secs(1));
    let last = thumb(&mut ui);
    assert!(first < midway && midway < last, "{first} {midway} {last}");
}

#[test]
fn radio_buttons_select_one() {
    #[composable]
    fn Choices() {
        let selected = remember(|| mutable_state_of(0));
        Column!({
            for index in 0..3 {
                RadioButton!(
                    selected.get() == index,
                    Some(move || selected.set(index)),
                    modifier = Modifier.test_tag(format!("choice {index}"))
                );
            }
        });
    }
    let mut ui = compose_test(Choices);
    ui.on_node_with_tag("choice 2").perform_click();
    let selected: Vec<Option<bool>> = (0..3)
        .map(|index| {
            ui.on_node_with_tag(&format!("choice {index}")).fetch_semantics_node().config.selected
        })
        .collect();
    assert_eq!(selected, [Some(false), Some(false), Some(true)]);
    assert_eq!(
        ui.on_node_with_tag("choice 2").fetch_semantics_node().config.role,
        Some(Role::RadioButton)
    );
}

#[test]
fn a_slider_follows_presses_drags_and_keys() {
    thread_local! { static FINISHED: Cell<u32> = const { Cell::new(0) }; }
    #[composable]
    fn Screen(value: MutableState<f32>) {
        Slider!(
            value.get(),
            move |new| value.set(new),
            modifier = Modifier.width(220.dp()).test_tag("slider"),
            value_range = (0.0, 100.0),
            steps = 9,
            on_value_change_finished =
                || FINISHED.with(|finished| finished.set(finished.get() + 1)),
        );
    }
    let value = mutable_state_of(0.0f32);
    let handle = value.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), move || Screen(handle));
    let slider = bounds(&mut ui, "slider");
    assert_eq!((slider.width(), slider.height()), (220.0, 48.0));
    // The track runs from 10 to 210: the middle is 50, a step.
    ui.host().send_pointer(PointerEvent::press(Offset::new(110.0, 24.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(110.0, 24.0)));
    ui.wait_for_idle();
    assert!((value.get() - 50.0).abs() < 0.001, "{}", value.get());
    // Dragged to 68 px: 29 %, snapped to 30.
    ui.host().send_pointer(PointerEvent::press(Offset::new(110.0, 24.0)));
    ui.host().send_pointer(PointerEvent::moved(Offset::new(68.0, 24.0), true));
    ui.host().send_pointer(PointerEvent::release(Offset::new(68.0, 24.0)));
    ui.wait_for_idle();
    assert!((value.get() - 30.0).abs() < 0.001, "{}", value.get());
    ui.on_node_with_tag("slider").request_focus();
    ui.press_key(Key::DirectionRight);
    assert!((value.get() - 40.0).abs() < 0.001, "{}", value.get());
    assert_eq!(FINISHED.with(Cell::get), 3);
}

#[test]
fn a_scaffold_pads_its_content_and_places_its_bars() {
    #[composable]
    fn Screen() {
        MaterialTheme!({
            Scaffold!(
                top_bar = {
                    TopAppBar!({ Text!("Title") }, modifier = Modifier.test_tag("top"));
                },
                bottom_bar = {
                    Box!(modifier = Modifier.fill_max_width().height(80.dp()).test_tag("bottom"));
                },
                floating_action_button = {
                    FloatingActionButton!(on_click = || {}, modifier = Modifier.test_tag("fab"), {
                        Icon!(icons::filled::add(), Some("Add"));
                    });
                },
                |padding: compose_ui::PaddingValues| {
                    Box!(
                        modifier =
                            Modifier.padding_values(padding).fill_max_size().test_tag("content")
                    );
                }
            );
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), Screen);
    assert_eq!(bounds(&mut ui, "top").height(), 64.0);
    let content = bounds(&mut ui, "content");
    assert_eq!((content.top, content.bottom), (64.0, 720.0));
    let fab = bounds(&mut ui, "fab");
    assert_eq!((fab.right, fab.bottom), (384.0, 704.0));
    ui.on_node_with_text("Title").assert_exists();
}

#[test]
fn a_clickable_card_clicks() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let mut ui = compose_test(|| {
        Card!(
            on_click = || CLICKS.with(|clicks| clicks.set(clicks.get() + 1)),
            modifier = Modifier.test_tag("card"),
            {
                Text!("A card");
                HorizontalDivider!();
            }
        );
    });
    ui.on_node_with_tag("card").perform_click();
    assert_eq!(CLICKS.with(Cell::get), 1);
}
