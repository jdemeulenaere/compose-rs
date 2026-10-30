//! The buttons' and cards' styling parameters, the surfaces' overloads, the floating action
//! buttons, the icon buttons, the scaffold's FAB positions and insets, and the icon set.

#![allow(non_snake_case)]

use std::cell::Cell;

use compose_foundation::BorderStroke;
use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, IntSize, Modifier, MutableInteractionSource, Offset, PointerEvent, Role, UnitExt,
    platform::DrawOp,
};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds
}

fn center_of(rect: compose_ui::Rect) -> Offset {
    Offset::new((rect.left + rect.right) / 2.0, (rect.top + rect.bottom) / 2.0)
}

/// The elevations of the shadows drawn.
fn shadows(ui: &mut ComposeTestRule) -> Vec<f32> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawShadow { elevation, .. } => Some(*elevation),
            _ => None,
        })
        .collect()
}

fn fills(ui: &mut ComposeTestRule) -> Vec<Color> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(_, paint) | DrawOp::DrawRoundRect(_, _, paint) => Some(paint.color),
            _ => None,
        })
        .collect()
}

#[test]
fn a_button_takes_its_shape_colors_border_and_padding() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Button!(
                || {},
                modifier = Modifier.test_tag("button"),
                shape = compose_foundation::RoundedCornerShape(4.dp()),
                colors =
                    ButtonColors { container_color: Color::RED, ..ButtonDefaults::button_colors() },
                border = Some(BorderStroke::new(2.dp(), Color::BLUE)),
                content_padding = PaddingValues::all(0.dp()),
                {
                    Spacer!(Modifier.size((100.dp(), 20.dp())));
                }
            );
        });
    });
    // No padding: the content, at least 40 dp high.
    let button = bounds(&mut ui, "button");
    assert_eq!((button.width(), button.height()), (100.0, 40.0));
    let ops = ui.host().display_list().ops.clone();
    assert!(ops.iter().any(|op| matches!(op,
        DrawOp::DrawRoundRect(_, radii, paint) if paint.color == Color::RED && radii[0] == 4.0)));
    assert!(fills(&mut ui).contains(&Color::BLUE) || ui.dump_display_list().contains("stroke 2"));
}

#[test]
fn the_default_paddings_and_minimum_sizes_are_composes() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                Button!(|| {}, modifier = Modifier.test_tag("filled"), {
                    Spacer!(Modifier.size((100.dp(), 20.dp())));
                });
                TextButton!(|| {}, modifier = Modifier.test_tag("text"), {
                    Spacer!(Modifier.size((100.dp(), 20.dp())));
                });
                OutlinedButton!(|| {}, modifier = Modifier.test_tag("small"), {});
            });
        });
    });
    // 24 dp at the sides of a filled button, 12 of a text button; 58 by 40 at least.
    assert_eq!(bounds(&mut ui, "filled").width(), 148.0);
    assert_eq!(bounds(&mut ui, "text").width(), 124.0);
    let small = bounds(&mut ui, "small");
    assert_eq!((small.width(), small.height()), (58.0, 40.0));
}

#[test]
fn an_elevated_buttons_shadow_rises_when_hovered_and_drops_disabled() {
    let enabled = std::rc::Rc::new(mutable_state_of(true));
    let given = enabled.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
        let enabled = given.get();
        MaterialTheme!({
            ElevatedButton!(|| {}, modifier = Modifier.test_tag("button"), enabled = enabled, {
                Text!("Raised");
            });
        });
    });
    assert_eq!(shadows(&mut ui), vec![1.0]);
    let center = center_of(bounds(&mut ui, "button"));
    ui.host().send_pointer(PointerEvent::moved(center, false));
    ui.wait_for_idle();
    assert_eq!(shadows(&mut ui), vec![3.0]);
    enabled.set(false);
    ui.wait_for_idle();
    assert!(shadows(&mut ui).iter().all(|&elevation| elevation == 0.0), "{:?}", shadows(&mut ui));
}

#[test]
fn a_button_reports_to_the_interaction_source_given() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = compose_test(move || {
        let source = given.clone();
        MaterialTheme!({
            FilledTonalButton!(
                || {},
                modifier = Modifier.test_tag("button"),
                interaction_source = Some(source.clone()),
                {
                    Text!("Tonal");
                }
            );
        });
    });
    let center = center_of(bounds(&mut ui, "button"));
    ui.host().send_pointer(PointerEvent::press(center));
    ui.wait_for_idle();
    assert!(source.is_pressed());
    ui.host().send_pointer(PointerEvent::release(center));
    ui.wait_for_idle();
    assert!(!source.is_pressed());
}

#[test]
fn surfaces_click_and_select() {
    thread_local! { static CLICKS: Cell<u32> = const { Cell::new(0) }; }
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                ClickableSurface!(
                    || CLICKS.with(|clicks| clicks.set(clicks.get() + 1)),
                    modifier = Modifier.test_tag("clickable"),
                    {
                        Spacer!(Modifier.size(20.dp()));
                    }
                );
                SelectableSurface!(true, || {}, modifier = Modifier.test_tag("selectable"), {
                    Spacer!(Modifier.size(60.dp()));
                });
            });
        });
    });
    // Its size; the 48 dp touch target is around it.
    assert_eq!(bounds(&mut ui, "clickable").width(), 20.0);
    ui.on_node_with_tag("clickable").perform_click();
    assert_eq!(CLICKS.with(Cell::get), 1);
    let selectable = ui.on_node_with_tag("selectable").fetch_semantics_snapshot();
    assert_eq!(
        selectable.config.get(&compose_ui::SemanticsProperties::SELECTED).copied(),
        Some(true)
    );
    assert_eq!(selectable.bounds.width(), 60.0);
}

#[test]
fn cards_take_their_elevations() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                ElevatedCard!(modifier = Modifier.test_tag("elevated"), {
                    Spacer!(Modifier.size(50.dp()));
                });
                Card!(
                    elevation = CardElevation {
                        default_elevation: 4.dp(),
                        ..CardDefaults::card_elevation()
                    },
                    {
                        Spacer!(Modifier.size(50.dp()));
                    }
                );
                OutlinedCard!(on_click = || {}, modifier = Modifier.test_tag("outlined"), {
                    Spacer!(Modifier.size(50.dp()));
                });
            });
        });
    });
    assert_eq!(shadows(&mut ui), vec![1.0, 4.0]);
    // A clickable elevated card rises when hovered (3 dp).
    let mut ui = compose_test(|| {
        MaterialTheme!({
            ElevatedCard!(on_click = || {}, modifier = Modifier.test_tag("card"), {
                Spacer!(Modifier.size(50.dp()));
            });
        });
    });
    let center = center_of(bounds(&mut ui, "card"));
    ui.host().send_pointer(PointerEvent::moved(center, false));
    ui.wait_for_idle();
    assert_eq!(shadows(&mut ui), vec![3.0]);
}

#[test]
fn floating_action_buttons_have_composes_sizes() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                SmallFloatingActionButton!(|| {}, modifier = Modifier.test_tag("small"), {
                    Icon!(icons::filled::add(), Some("Small"));
                });
                FloatingActionButton!(|| {}, modifier = Modifier.test_tag("regular"), {
                    Icon!(icons::filled::add(), Some("Regular"));
                });
                LargeFloatingActionButton!(|| {}, modifier = Modifier.test_tag("large"), {
                    Icon!(icons::filled::add(), Some("Large"));
                });
            });
        });
    });
    for (tag, size) in [("small", 40.0), ("regular", 56.0), ("large", 96.0)] {
        let fab = bounds(&mut ui, tag);
        assert_eq!((fab.width(), fab.height()), (size, size), "{tag}");
    }
    assert_eq!(
        ui.on_node_with_tag("regular")
            .fetch_semantics_snapshot()
            .config
            .get(&compose_ui::SemanticsProperties::ROLE)
            .copied(),
        Some(Role::Button)
    );
    assert_eq!(shadows(&mut ui), vec![6.0, 6.0, 6.0]);
}

#[test]
fn an_extended_fab_shows_its_text_while_expanded() {
    let expanded = std::rc::Rc::new(mutable_state_of(true));
    let given = expanded.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let expanded = given.get();
        MaterialTheme!({
            ExtendedFloatingActionButton!(
                { Text!("Compose") },
                { Icon!(icons::filled::edit(), Some("Edit")) },
                || {},
                modifier = Modifier.test_tag("fab"),
                expanded = expanded
            );
        });
    });
    let fab = bounds(&mut ui, "fab");
    assert_eq!(fab.height(), 56.0);
    // 16 + 24 + 12 + the text + 20, at least 80.
    assert!(fab.width() > 80.0, "{}", fab.width());
    // The text isn't read: the icon's description names the button.
    ui.on_node_with_text("Compose").assert_does_not_exist();
    ui.on_node_with_content_description("Edit").assert_exists();
    expanded.set(false);
    ui.advance_time_by(std::time::Duration::from_secs(1));
    let fab = bounds(&mut ui, "fab");
    assert_eq!((fab.width(), fab.height()), (56.0, 56.0));
}

#[test]
fn icon_buttons_fill_outline_and_toggle() {
    #[composable]
    fn Screen() {
        let checked = remember(|| mutable_state_of(false));
        MaterialTheme!({
            Row!({
                FilledIconButton!(|| {}, modifier = Modifier.test_tag("filled"), {
                    Icon!(icons::filled::add(), Some("Add"));
                });
                OutlinedIconButton!(|| {}, modifier = Modifier.test_tag("outlined"), {
                    Icon!(icons::filled::add(), Some("Add"));
                });
                IconToggleButton!(
                    checked.get(),
                    move |value| checked.set(value),
                    modifier = Modifier.test_tag("toggle"),
                    {
                        Icon!(icons::filled::favorite(), Some("Favorite"));
                    }
                );
            });
        });
    }
    let mut ui = compose_test(Screen);
    // 40 dp, in a 48 dp touch target.
    assert_eq!(bounds(&mut ui, "filled").width(), 40.0);
    assert_eq!(bounds(&mut ui, "outlined").width(), 40.0);
    assert_eq!(bounds(&mut ui, "toggle").width(), 40.0);
    let primary = light_color_scheme().primary;
    assert!(fills(&mut ui).contains(&primary));
    ui.on_node_with_tag("toggle").assert_is_off();
    ui.on_node_with_tag("toggle").perform_click();
    ui.on_node_with_tag("toggle").assert_is_on();
}

#[composable]
fn ScaffoldWith(position: FabPosition, bottom_bar: bool, insets: WindowInsets) {
    MaterialTheme!({
        Scaffold!(
            bottom_bar = {
                if bottom_bar {
                    BottomAppBar!(|_| {}, modifier = Modifier.test_tag("bar"));
                }
            },
            floating_action_button = {
                FloatingActionButton!(|| {}, modifier = Modifier.test_tag("fab"), {});
            },
            floating_action_button_position = position,
            content_window_insets = insets,
            |padding: PaddingValues| {
                Box!(modifier = Modifier.padding(padding).fill_max_size().test_tag("content"));
            }
        );
    });
}

#[test]
fn the_scaffold_places_its_fab() {
    let size = IntSize::new(400, 600);
    let fab_at = |position: FabPosition, bar: bool| {
        let mut ui = ComposeTestRule::sized(size, move || {
            ScaffoldWith(position, bar, WindowInsets::default())
        });
        bounds(&mut ui, "fab")
    };
    let end = fab_at(FabPosition::End, false);
    assert_eq!((end.left, end.top), (400.0 - 16.0 - 56.0, 600.0 - 16.0 - 56.0));
    assert_eq!(fab_at(FabPosition::Start, false).left, 16.0);
    assert_eq!(fab_at(FabPosition::Center, false).left, (400.0 - 56.0) / 2.0);
    // Above an 80 dp bottom bar, or over it.
    assert_eq!(fab_at(FabPosition::End, true).top, 600.0 - 80.0 - 16.0 - 56.0);
    assert_eq!(fab_at(FabPosition::EndOverlay, true).top, 600.0 - 16.0 - 56.0);
}

#[test]
fn the_scaffold_pads_its_content_by_the_insets_where_there_is_no_bar() {
    let insets = WindowInsets::new(10.dp(), 20.dp(), 30.dp(), 40.dp());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || {
        ScaffoldWith(FabPosition::End, false, insets.clone())
    });
    let content = bounds(&mut ui, "content");
    assert_eq!(
        (content.left, content.top, content.right, content.bottom),
        (10.0, 20.0, 370.0, 560.0)
    );
    let insets = WindowInsets::new(10.dp(), 20.dp(), 30.dp(), 40.dp());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || {
        ScaffoldWith(FabPosition::End, true, insets.clone())
    });
    // The bottom bar's height, not the inset.
    assert_eq!(bounds(&mut ui, "content").bottom, 600.0 - 80.0);
}

#[test]
fn the_core_icons_parse_within_their_viewport() {
    let all = icons::filled::all();
    assert_eq!(all.len(), 49);
    for icon in all {
        assert_eq!((icon.viewport_width, icon.viewport_height), (24.0, 24.0));
        let commands = icon.paths[0].0.commands();
        assert!(commands.len() > 2, "{}", icon.name);
        for command in commands {
            let points: &[Offset] = match command {
                compose_ui::PathCommand::MoveTo(point) | compose_ui::PathCommand::LineTo(point) => {
                    std::slice::from_ref(point)
                }
                compose_ui::PathCommand::QuadTo(a, b) => &[*a, *b],
                compose_ui::PathCommand::CubicTo(a, b, c) => &[*a, *b, *c],
                compose_ui::PathCommand::Close => &[],
            };
            for point in points {
                assert!(
                    (-0.5..=24.5).contains(&point.x) && (-0.5..=24.5).contains(&point.y),
                    "{} {point:?}",
                    icon.name
                );
            }
        }
    }
    // The same vector each time on a thread (parsed once).
    use compose_runtime::ComposeArg;
    assert!(icons::filled::settings().same(&icons::default::settings()));
}
