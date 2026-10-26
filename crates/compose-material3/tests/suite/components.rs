//! Icons, icon buttons, the floating action button, switches, radio buttons, sliders, cards,
//! dividers, the top app bar and the scaffold.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Key, Modifier, Offset, PointerEvent, Role, UnitExt, platform::DrawOp};

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
    assert_eq!(
        node.config
            .get(&compose_ui::SemanticsProperties::CONTENT_DESCRIPTION)
            .cloned()
            .unwrap_or_default()
            .first()
            .map(|text| text.as_str()),
        Some("Add")
    );
    assert_eq!(node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(), Some(Role::Image));
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
    // The frame that recomposes for it: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
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
            ui.on_node_with_tag(&format!("choice {index}"))
                .fetch_semantics_node()
                .config
                .get(&compose_ui::SemanticsProperties::SELECTED)
                .copied()
        })
        .collect();
    assert_eq!(selected, [Some(false), Some(false), Some(true)]);
    assert_eq!(
        ui.on_node_with_tag("choice 2")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::ROLE)
            .copied(),
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
                    Box!(modifier = Modifier.padding(padding).fill_max_size().test_tag("content"));
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

#[test]
fn an_alert_dialog_shows_its_parts_and_its_buttons_act() {
    thread_local! { static CONFIRMED: Cell<u32> = const { Cell::new(0) }; }
    #[composable]
    fn Screen(open: MutableState<bool>) {
        MaterialTheme!({
            if open.get() {
                AlertDialog!(
                    on_dismiss_request = move || open.set(false),
                    confirm_button = {
                        TextButton!(
                            on_click = move || {
                                CONFIRMED.with(|c| c.set(c.get() + 1));
                                open.set(false)
                            },
                            {
                                Text!("Delete");
                            }
                        );
                    },
                    dismiss_button = {
                        TextButton!(on_click = move || open.set(false), {
                            Text!("Cancel");
                        });
                    },
                    title = {
                        Text!("Delete the item?");
                    },
                    text = {
                        Text!("It can't be undone.");
                    },
                );
            }
        });
    }
    let open = mutable_state_of(true);
    let handle = open.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || Screen(handle));
    ui.on_node_with_text("Delete the item?").assert_exists();
    ui.on_node_with_text("It can't be undone.").assert_exists();
    let cancel = ui.on_node_with_text("Cancel").fetch_semantics_node().bounds;
    let delete = ui.on_node_with_text("Delete").fetch_semantics_node().bounds;
    assert!(cancel.right <= delete.left, "dismiss before confirm: {cancel:?} {delete:?}");
    ui.on_node_with_text("Delete").perform_click();
    assert_eq!(CONFIRMED.with(Cell::get), 1);
    ui.on_node_with_text("Delete the item?").assert_does_not_exist();
}

#[test]
fn a_dropdown_menu_opens_below_its_anchor_and_closes_on_selection() {
    thread_local! { static PICKED: Cell<u32> = const { Cell::new(0) }; }
    #[composable]
    fn Screen() {
        let expanded = remember(|| mutable_state_of(false));
        MaterialTheme!({
            Box!(modifier = Modifier.padding(20.dp()), {
                IconButton!(
                    on_click = move || expanded.set(true),
                    modifier = Modifier.test_tag("more"),
                    {
                        Icon!(icons::filled::more_vert(), Some("More"));
                    }
                );
                DropdownMenu!(expanded.get(), move || expanded.set(false), {
                    DropdownMenuItem!({ Text!("Edit") }, move || {
                        PICKED.with(|p| p.set(1));
                        expanded.set(false)
                    });
                    DropdownMenuItem!({ Text!("Share") }, move || {
                        PICKED.with(|p| p.set(2));
                        expanded.set(false)
                    });
                });
            });
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), Screen);
    ui.on_node_with_text("Edit").assert_does_not_exist();
    ui.on_node_with_tag("more").perform_click();
    let edit = ui.on_node(has_text("Edit")).fetch_semantics_node().bounds;
    let more = ui.on_node_with_tag("more").fetch_semantics_node().bounds;
    assert!(edit.top >= more.bottom, "below the anchor: {edit:?} under {more:?}");
    assert!(edit.width() >= 112.0, "{edit:?}");
    ui.on_node_with_text("Share").perform_click();
    assert_eq!(PICKED.with(Cell::get), 2);
    ui.on_node_with_text("Edit").assert_does_not_exist();
}

#[test]
fn tonal_elevation_tints_the_surface_color_and_adds_up() {
    use std::cell::Cell;
    thread_local! { static COLORS: Cell<Option<(ColorScheme, Color, Color)>> = const { Cell::new(None) }; }
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Surface!(
                tonal_elevation = 3.dp(),
                modifier = Modifier.size(50.dp()).test_tag("outer"),
                {
                    Surface!(tonal_elevation = 3.dp(), modifier = Modifier.size(20.dp()), {});
                }
            );
            let scheme = MaterialTheme::color_scheme();
            COLORS.with(|cell| {
                cell.set(Some((
                    scheme,
                    scheme.surface_color_at_elevation(3.dp()),
                    scheme.surface_color_at_elevation(6.dp()),
                )))
            });
        });
    });
    let (scheme, at_3, at_6) = COLORS.with(Cell::get).expect("composed");
    assert_ne!(at_3, scheme.surface);
    let fills: Vec<Color> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            compose_ui::platform::DrawOp::DrawRect(_, paint) => Some(paint.color),
            _ => None,
        })
        .collect();
    assert_eq!(fills, vec![at_3, at_6], "the inner surface at the sum of both");
}

#[test]
fn a_slider_takes_set_progress_snapped_to_its_steps() {
    let value = mutable_state_of(0.0f32);
    let handle = value.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Slider!(
                handle.get(),
                move |new| handle.set(new),
                modifier = Modifier.test_tag("slider"),
                steps = 3,
            );
        });
    });
    let node = ui.on_node_with_tag("slider").fetch_semantics_node();
    let info = node
        .config
        .get(&compose_ui::SemanticsProperties::PROGRESS_BAR_RANGE_INFO)
        .copied()
        .expect("a range");
    assert_eq!((info.current, info.range, info.steps), (0.0, (0.0, 1.0), 3));
    ui.on_node_with_tag("slider").perform_semantics_action_with(
        &compose_ui::SemanticsActions::SET_PROGRESS,
        |set| {
            set(0.3);
        },
    );
    assert_eq!(value.get(), 0.25, "snapped to the nearest of the 5 stops");
    let info = ui
        .on_node_with_tag("slider")
        .fetch_semantics_node()
        .config
        .get(&compose_ui::SemanticsProperties::PROGRESS_BAR_RANGE_INFO)
        .copied();
    assert_eq!(info.map(|info| info.current), Some(0.25));
}

/// What a slider's semantics say follows the application's value, its focus and its enabled
/// state, and `SetProgress` goes from the value now (the semantics kept per node, D1257).
#[test]
fn a_sliders_semantics_follow_its_value_focus_and_enabled_state() {
    let value = mutable_state_of(0.0f32);
    let enabled = mutable_state_of(true);
    let (handle, enabled_handle) = (value.handle(), enabled.handle());
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Slider!(
                handle.get(),
                move |new| handle.set(new),
                modifier = Modifier.test_tag("slider"),
                enabled = enabled_handle.get(),
            );
        });
    });
    let current = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag("slider")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::PROGRESS_BAR_RANGE_INFO)
            .map(|info| info.current)
    };
    assert_eq!(current(&mut ui), Some(0.0));
    value.set(0.5);
    assert_eq!(current(&mut ui), Some(0.5));
    ui.on_node_with_tag("slider").request_focus();
    ui.on_node_with_tag("slider").assert_is_focused();
    ui.on_node_with_tag("slider").perform_semantics_action_with(
        &compose_ui::SemanticsActions::SET_PROGRESS,
        |set| {
            // The value it was given by the application isn't asked for again.
            assert!(!set(0.5));
            assert!(set(0.75));
        },
    );
    assert_eq!(current(&mut ui), Some(0.75));
    enabled.set(false);
    ui.on_node_with_tag("slider").assert_is_not_enabled();
    ui.on_node_with_tag("slider")
        .assert(key_not_defined(&compose_ui::SemanticsProperties::FOCUSED));
}

/// `SetProgress` asks from the value last asked, as the slider's keys do: asked twice for the
/// same stop before the application answers, it asks once.
#[test]
fn a_sliders_set_progress_goes_on_from_the_value_asked() {
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = asked.clone();
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        MaterialTheme!({
            let seen = seen.clone();
            // A value the application never changes.
            Slider!(
                0.0,
                move |new| seen.borrow_mut().push(new),
                modifier = Modifier.test_tag("slider"),
                steps = 3,
            );
        });
    });
    let node = ui.on_node_with_tag("slider").node_id();
    let set = ui
        .host()
        .semantics_action(node, &compose_ui::SemanticsActions::SET_PROGRESS)
        .expect("a slider sets its progress");
    assert!(set(0.3));
    assert!(!set(0.3), "the stop asked already");
    assert!(set(0.5));
    assert_eq!(*asked.borrow(), [0.25, 0.5]);
}

#[test]
fn a_right_to_left_slider_runs_from_the_right() {
    use compose_ui::{LayoutDirection, Offset, PointerEvent};
    let value = mutable_state_of(0.0f32);
    let handle = value.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Slider!(
                handle.get(),
                move |new| handle.set(new),
                modifier = Modifier.test_tag("slider")
            );
        });
    });
    ui.host().set_layout_direction(LayoutDirection::Rtl);
    ui.wait_for_idle();
    let bounds = ui.on_node_with_tag("slider").fetch_semantics_node().bounds;
    // A tap near the left end: near the range's end.
    let at = Offset::new(bounds.left + 12.0, bounds.top + bounds.height() / 2.0);
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::release(at));
    ui.wait_for_idle();
    assert!(value.get() > 0.95, "{}", value.get());
    // The thumb is drawn on the left: the display list mirrors the slider.
    let list = ui.host().display_list();
    assert!(
        list.ops
            .iter()
            .any(|op| matches!(op, compose_ui::platform::DrawOp::Transform { scale, .. } if scale.0 == -1.0)),
        "{}",
        list.dump()
    );
}

#[test]
fn progress_indicators_draw_their_progress_and_animate_when_indeterminate() {
    use compose_ui::platform::DrawOp;
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            Column!({
                CircularProgressIndicator!(|| 0.25, modifier = Modifier.test_tag("circular"));
                LinearProgressIndicator!(|| 0.5, modifier = Modifier.test_tag("linear"));
                IndeterminateCircularProgressIndicator!();
            });
        });
    });
    // Sizes and semantics, as Compose's.
    let circular = ui.on_node_with_tag("circular").fetch_semantics_node();
    assert_eq!((circular.bounds.width(), circular.bounds.height()), (40.0, 40.0));
    let info = circular
        .config
        .get(&compose_ui::SemanticsProperties::PROGRESS_BAR_RANGE_INFO)
        .copied()
        .expect("a range");
    assert_eq!((info.current, info.range), (0.25, (0.0, 1.0)));
    let linear = ui.on_node_with_tag("linear").fetch_semantics_node().bounds;
    assert_eq!((linear.width(), linear.height()), (240.0, 4.0));
    // A quarter turn from the top, after its track past the 4 dp gap (and the 4 dp stroke, for
    // the caps: 8 dp of the 40 dp circle's circumference, 22.9 degrees); a line half way, then
    // its track past Material 3 1.4's 4 dp gap (and the stroke), their round caps (the default)
    // inside the indicator: the track from 128 to 238, the line from 2 to 120 (D572).
    let list = ui.host().display_list();
    let arcs: Vec<(f32, f32)> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            // The strokes (not the linear indicator's stop dot, a filled circle).
            DrawOp::DrawArc { start_angle, sweep_angle, paint, .. }
                if paint.style != compose_ui::PaintStyle::Fill =>
            {
                Some((*start_angle, *sweep_angle))
            }
            _ => None,
        })
        .collect();
    let gap = 8.0 / (std::f32::consts::PI * 40.0) * 360.0;
    assert!((arcs[0].0 - (270.0 + 90.0 + gap)).abs() < 1e-3, "the track: {arcs:?}");
    assert!((arcs[0].1 - (360.0 - 90.0 - 2.0 * gap)).abs() < 1e-3, "the track: {arcs:?}");
    assert_eq!(arcs[1], (270.0, 90.0));
    let lines: Vec<f32> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawLine(from, to, _) => Some(to.x - from.x),
            _ => None,
        })
        .collect();
    assert_eq!(lines, vec![110.0, 118.0]);
    // The indeterminate arc moves frame to frame, drawing again only.
    let first = arcs[2];
    ui.advance_time_by(std::time::Duration::from_millis(300));
    let list = ui.host().display_list();
    let moved = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawArc { start_angle, sweep_angle, paint, .. }
                if paint.style != compose_ui::PaintStyle::Fill =>
            {
                Some((*start_angle, *sweep_angle))
            }
            _ => None,
        })
        .nth(2);
    assert_ne!(moved, Some(first));
}

#[test]
fn list_items_are_as_high_as_their_lines_with_their_padding() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                ListItem!({ Text!("One") }, modifier = Modifier.test_tag("one"));
                ListItem!(
                    { Text!("Two") },
                    modifier = Modifier.test_tag("two"),
                    supporting_content = Some(|| Text!("Supporting")),
                    leading_content = Some(|| {
                        Icon!(icons::filled::star(), None, modifier = Modifier.test_tag("icon"))
                    }),
                );
                ListItem!(
                    { Text!("Three") },
                    modifier = Modifier.test_tag("three"),
                    overline_content = Some(|| Text!("Overline")),
                    supporting_content = Some(|| Text!("Supporting")),
                );
            });
        });
    });
    let height = |ui: &mut ComposeTestRule, tag: &str| {
        ui.on_node_with_tag(tag).fetch_semantics_node().bounds.height()
    };
    assert_eq!(height(&mut ui, "one"), 56.0);
    assert_eq!(height(&mut ui, "two"), 72.0);
    assert_eq!(height(&mut ui, "three"), 88.0);
    // The leading icon 16 dp in, centered down its 72 dp item.
    // Merged into its item (one node, as Compose's): found in the unmerged tree.
    let icon = ui.on_node_unmerged(has_test_tag("icon")).fetch_semantics_node().bounds;
    assert_eq!((icon.left, icon.top), (16.0, 56.0 + 24.0));
    ui.on_node_with_text("Two").assert_exists();
}

#[test]
fn a_navigation_bar_shares_its_width_and_selects_on_click() {
    let selected = mutable_state_of(0usize);
    let handle = selected.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            NavigationBar!(modifier = Modifier.test_tag("bar"), |row: RowScope| {
                for (index, name) in ["Home", "Search"].into_iter().enumerate() {
                    NavigationBarItem!(
                        row,
                        handle.get() == index,
                        move || handle.set(index),
                        { Icon!(icons::filled::star(), None) },
                        modifier = Modifier.test_tag(name),
                        label = Some(move || Text!("{}", name)),
                    );
                }
            });
        });
    });
    let bar = ui.on_node_with_tag("bar").fetch_semantics_node().bounds;
    assert_eq!(bar.height(), 80.0);
    let home = ui.on_node_with_tag("Home").fetch_semantics_node();
    assert_eq!(home.config.get(&compose_ui::SemanticsProperties::SELECTED).copied(), Some(true));
    // Two items sharing 400 px, 8 dp apart.
    assert_eq!(home.bounds.width(), 196.0);
    ui.on_node_with_tag("Search").perform_click();
    assert_eq!(selected.get(), 1);
    assert_eq!(
        ui.on_node_with_tag("Search")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied(),
        Some(true)
    );
}

#[test]
fn button_and_card_variants_have_their_colors_and_shadows() {
    use compose_ui::platform::DrawOp;
    thread_local! { static SCHEME: std::cell::Cell<Option<ColorScheme>> = const { std::cell::Cell::new(None) }; }
    let mut ui = compose_test(|| {
        MaterialTheme!({
            SCHEME.with(|cell| cell.set(Some(MaterialTheme::color_scheme())));
            Column!({
                FilledTonalButton!(|| {}, { Text!("Tonal") });
                ElevatedButton!(|| {}, { Text!("Elevated") });
                ElevatedCard!(modifier = Modifier.size(40.dp()), {});
                OutlinedCard!(modifier = Modifier.size(40.dp()), {});
            });
        });
    });
    let scheme = SCHEME.with(std::cell::Cell::get).expect("composed");
    let list = ui.host().display_list();
    let fills: Vec<Color> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRoundRect(_, _, paint) if paint.style == compose_ui::PaintStyle::Fill => {
                Some(paint.color)
            }
            _ => None,
        })
        .collect();
    assert!(fills.contains(&scheme.secondary_container), "the tonal button");
    assert_eq!(fills.iter().filter(|color| **color == scheme.surface_container_low).count(), 2);
    assert!(fills.contains(&scheme.surface), "the outlined card");
    let shadows = list.ops.iter().filter(|op| matches!(op, DrawOp::DrawShadow { .. })).count();
    assert_eq!(shadows, 2, "the elevated button and card");
}

#[test]
fn a_text_fields_label_rests_in_it_then_floats_when_focused() {
    use compose_ui::platform::DrawOp;
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        MaterialTheme!({
            Column!({
                TextField!(
                    String::new(),
                    |_| {},
                    modifier = Modifier.test_tag("filled"),
                    label = Some(|| Text!("Name", modifier = Modifier.test_tag("filled label"))),
                    single_line = true,
                );
                OutlinedTextField!(
                    String::new(),
                    |_| {},
                    modifier = Modifier.test_tag("outlined"),
                    label = Some(|| Text!("City", modifier = Modifier.test_tag("outlined label"))),
                    single_line = true,
                );
            });
        });
    });
    let label = |ui: &mut ComposeTestRule, tag: &str| {
        ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
    };
    let field = ui.on_node_with_tag("filled").fetch_semantics_node().bounds;
    let resting = label(&mut ui, "filled label");
    // Centered in the 56 dp field, in the large body style (24 px lines).
    assert_eq!(resting.height(), 24.0);
    assert_eq!(resting.top - field.top, 16.0);
    ui.on_node_with_tag("filled").request_focus();
    ui.advance_time_by(std::time::Duration::from_millis(300));
    let floated = label(&mut ui, "filled label");
    // 8 dp from the top, in the small body style (16 px lines).
    assert_eq!((floated.top - field.top, floated.height()), (8.0, 16.0));
    // The outlined field's floated label sits on its top edge, the outline cut around it.
    ui.on_node_with_tag("outlined").request_focus();
    ui.advance_time_by(std::time::Duration::from_millis(300));
    let outlined = ui.on_node_with_tag("outlined").fetch_semantics_node().bounds;
    let floated = label(&mut ui, "outlined label");
    // (The field's node is its outline, inside the 8 dp above it for the label.)
    assert_eq!(floated.top + floated.height() / 2.0, outlined.top, "on the edge");
    let list = ui.host().display_list();
    let open_path = list.ops.iter().any(|op| match op {
        DrawOp::DrawPath { path, .. } => {
            !matches!(path.commands().last(), Some(compose_ui::PathCommand::Close))
        }
        _ => false,
    });
    assert!(open_path, "the outline is cut: {}", list.dump());
}

#[test]
fn a_tab_row_shares_its_width_and_slides_its_indicator() {
    use compose_ui::platform::DrawOp;
    let selected = mutable_state_of(0usize);
    let handle = selected.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        MaterialTheme!({
            TabRow!(handle.get(), modifier = Modifier.test_tag("row"), {
                for (index, name) in ["One", "Two"].into_iter().enumerate() {
                    Tab!(
                        handle.get() == index,
                        move || handle.set(index),
                        modifier = Modifier.test_tag(name),
                        text = Some(move || Text!("{}", name)),
                    );
                }
            });
        });
    });
    ui.advance_time_by(std::time::Duration::from_millis(50));
    let row = ui.on_node_with_tag("row").fetch_semantics_node().bounds;
    let two = ui.on_node_with_tag("Two").fetch_semantics_node();
    assert_eq!((row.height(), two.bounds.left, two.bounds.width()), (48.0, 200.0, 200.0));
    assert_eq!(two.config.get(&compose_ui::SemanticsProperties::ROLE).copied(), Some(Role::Tab));
    let indicator = |ui: &mut ComposeTestRule| {
        ui.host().display_list().ops.iter().rev().find_map(|op| match op {
            DrawOp::DrawRect(rect, _) if rect.height() == 3.0 => Some(rect.left),
            _ => None,
        })
    };
    assert_eq!(indicator(&mut ui), Some(0.0));
    ui.on_node_with_tag("Two").perform_click();
    ui.advance_time_by(std::time::Duration::from_millis(100));
    let sliding = indicator(&mut ui).expect("the indicator");
    assert!(sliding > 0.0 && sliding < 200.0, "{sliding}");
    ui.advance_time_by(std::time::Duration::from_millis(300));
    assert_eq!(indicator(&mut ui), Some(200.0));
    assert_eq!(
        ui.on_node_with_tag("Two")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied(),
        Some(true)
    );
}

#[test]
fn chips_and_badges_are_sized_and_behave_as_compose() {
    let selected = mutable_state_of(false);
    let handle = selected.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Column!({
                FilterChip!(
                    handle.get(),
                    move || handle.set(!handle.get()),
                    { Text!("Filter") },
                    modifier = Modifier.test_tag("filter"),
                );
                AssistChip!(|| {}, { Text!("Assist") }, modifier = Modifier.test_tag("assist"));
                BadgedBox!(
                    { Badge!(modifier = Modifier.test_tag("dot")) },
                    modifier = Modifier.test_tag("badged"),
                    { Icon!(icons::filled::home(), Some("Home")) }
                );
            });
        });
    });
    // 32 dp high (in a 48 dp touch target).
    let filter = ui.on_node_with_tag("filter").fetch_semantics_node();
    assert_eq!(filter.bounds.height(), 32.0);
    assert_eq!(filter.config.get(&compose_ui::SemanticsProperties::SELECTED).copied(), Some(false));
    ui.on_node_with_tag("filter").perform_click();
    assert!(selected.get());
    assert_eq!(
        ui.on_node_with_tag("filter")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied(),
        Some(true)
    );
    ui.on_node_with_tag("assist").assert_exists();
    // The dot: 6 dp, its start 6 dp before the icon's end, centered on its top.
    let icon = ui.on_node_with_tag("badged").fetch_semantics_node().bounds;
    let dot = ui.on_node_unmerged(has_test_tag("dot")).fetch_semantics_node().bounds;
    assert_eq!((dot.width(), dot.height()), (6.0, 6.0));
    assert_eq!((dot.left, dot.top), (icon.right - 6.0, icon.top - 3.0));
}

/// Cards render as Compose's: a `Surface` is a layer casting its shadow (not clipping) when it
/// has an elevation, then its border and color, then a layer clipping its content to its shape
/// (material3's `Modifier.surface`). So a card is one clipping layer, and an elevated card adds
/// a 1 dp shadow's (D526).
#[test]
fn cards_are_a_shadow_layer_and_a_clip_layer_as_composes() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        MaterialTheme!({
            Column!({
                Card!(modifier = Modifier.size(40.dp()), { Text!("filled") });
                ElevatedCard!(modifier = Modifier.size(40.dp()), { Text!("elevated") });
                OutlinedCard!(modifier = Modifier.size(40.dp()), { Text!("outlined") });
            });
        });
    });
    let list = ui.host().display_list();
    let layers: Vec<compose_ui::platform::LayerProperties> =
        list.groups.iter().filter_map(|group| group.layer.clone()).collect();
    let rounded = |layer: &compose_ui::platform::LayerProperties| matches!(layer.outline, Some(compose_ui::Outline::Rounded(_, radii)) if radii == [12.0; 4]);
    let clips: Vec<_> = layers.iter().filter(|layer| layer.clip).collect();
    let shadows: Vec<_> = layers.iter().filter(|layer| layer.shadow_elevation > 0.0).collect();
    assert_eq!(clips.len(), 3, "a clip each: {layers:?}");
    assert!(clips.iter().all(|layer| rounded(layer) && layer.shadow_elevation == 0.0));
    assert_eq!(shadows.len(), 1, "the elevated card's: {layers:?}");
    assert!(rounded(shadows[0]) && !shadows[0].clip && shadows[0].shadow_elevation == 1.0);
    assert_eq!(layers.len(), 4);
}

/// A button's shadow follows its interaction state, composed as Compose's (its elevation's
/// state read in composition): no layer at rest without an elevation, one while hovered.
#[test]
fn a_buttons_shadow_layer_comes_with_its_elevation() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 200), || {
        MaterialTheme!({
            Button!(on_click = || {}, modifier = Modifier.test_tag("button"), { Text!("Go") });
        });
    });
    let elevations = |ui: &mut ComposeTestRule| -> Vec<f32> {
        let list = ui.host().display_list();
        list.groups
            .iter()
            .filter_map(|group| group.layer.clone())
            .map(|layer| layer.shadow_elevation)
            .filter(|elevation| *elevation > 0.0)
            .collect()
    };
    assert!(elevations(&mut ui).is_empty(), "no shadow at rest");
    let center = {
        let bounds = bounds(&mut ui, "button");
        Offset::new((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0)
    };
    ui.host().send_pointer(PointerEvent::moved(center, false));
    ui.wait_for_idle();
    assert_eq!(elevations(&mut ui), [1.0], "hovered: Material's 1 dp");
}
