//! A text field's layout as Compose's `TextFieldMeasurePolicy` and
//! `OutlinedTextFieldMeasurePolicy`: what the ported tests of `TextFieldTest.kt` and
//! `TextFieldDecoratorTest.kt` can't tell, at other densities, font scales and constraints.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, Density, IntSize, IntrinsicSize, LOCAL_DENSITY, Modifier, MutableInteractionSource,
    PaddingValues, Shape, TextStyle, UnitExt,
};

const FIELD: &str = "field";
const HINT: &str = "hint";

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag_unmerged(tag).fetch_semantics_snapshot().bounds
}

fn settle(ui: &mut ComposeTestRule) {
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_secs(2));
    ui.wait_for_idle();
}

/// The height of a field with a label inside before and after focus, in `content`'s surroundings.
fn heights_across_focus(ui: &mut ComposeTestRule) -> (f32, f32) {
    settle(ui);
    let before = bounds(ui, FIELD).height();
    ui.on_node_with_tag(FIELD).request_focus();
    settle(ui);
    (before, bounds(ui, FIELD).height())
}

#[composable]
fn LabelledField() {
    TextField!(
        String::new(),
        |_: String| {},
        modifier = Modifier.test_tag(FIELD),
        label = Some(|| Text!("Label"))
    );
}

/// As big as body small's line, whatever the font scale: the height holds through focus.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, minimizedLabelHalfHeight
#[test]
fn at_a_font_scale_the_height_holds_through_focus() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        CompositionLocalProvider(
            LOCAL_DENSITY.provides(Density { density: 1.0, font_scale: 2.0 }),
            || {
                MaterialTheme!({
                    LabelledField();
                });
            },
        );
    });
    let (before, after) = heights_across_focus(&mut ui);
    assert_eq!(before, after);
}

/// With a body small of another line height.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, minimizedLabelHalfHeight
#[test]
fn with_a_taller_body_small_the_height_holds_through_focus() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        let mut typography = Typography::default();
        typography.body_small = TextStyle { line_height: 24.sp(), ..typography.body_small };
        MaterialTheme!(typography = typography, {
            LabelledField();
        });
    });
    let (before, after) = heights_across_focus(&mut ui);
    assert_eq!(before, after);
}

/// A field with a label on its edge has half the minimized label's line above, as
/// `topPaddingForLabelCutout`.
#[test]
fn the_room_above_a_cut_label_goes_by_body_small() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        let mut typography = Typography::default();
        typography.body_small = TextStyle { line_height: 24.sp(), ..typography.body_small };
        MaterialTheme!(typography = typography, {
            Box!(
                modifier = Modifier.test_tag("around"),
                content = |_| {
                    OutlinedTextField!(
                        String::new(),
                        |_: String| {},
                        modifier = Modifier.test_tag(FIELD),
                        label = Some(|| Text!("Label"))
                    );
                }
            );
        });
    });
    settle(&mut ui);
    let around = bounds(&mut ui, "around");
    assert_eq!(around.height(), 12.0 + 56.0);
}

/// At the density of a phone: 4 dp beside an icon are rounded, not 16 dp less 12 dp.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, InsideTextFieldLayout
#[test]
fn the_padding_beside_an_icon_is_rounded_in_dp() {
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 800), || {
        CompositionLocalProvider(LOCAL_DENSITY.provides(Density::new(2.625)), || {
            MaterialTheme!({
                TextField!(
                    String::new(),
                    |_: String| {},
                    modifier = Modifier.test_tag(FIELD),
                    placeholder = Some(|| Spacer(Modifier.size(10.dp()).test_tag(HINT))),
                    leading_icon = Some(|| Spacer(Modifier.size(24.dp())))
                );
            });
        });
    });
    settle(&mut ui);
    let field = bounds(&mut ui, FIELD);
    // Box of 48 dp: 126 px, then 4 dp: 10.5, 11 px.
    assert_eq!(bounds(&mut ui, HINT).left - field.left, 137.0);
}

/// Without a minimum touch target, an icon isn't padded to one, so no padding is taken back.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, textFieldHorizontalIconPadding
#[test]
fn the_icons_padding_goes_by_the_minimum_touch_target() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        CompositionLocalProvider(
            LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(36.dp()),
            || {
                MaterialTheme!({
                    TextField!(
                        String::new(),
                        |_: String| {},
                        modifier = Modifier.test_tag(FIELD),
                        placeholder = Some(|| Spacer(Modifier.size(10.dp()).test_tag(HINT))),
                        leading_icon = Some(|| Spacer(Modifier.size(24.dp())))
                    );
                });
            },
        );
    });
    settle(&mut ui);
    let field = bounds(&mut ui, FIELD);
    // 36 dp, then 16 less (36 - 24) / 2.
    assert_eq!(bounds(&mut ui, HINT).left - field.left, 36.0 + 10.0);
}

/// A label above doesn't take the minimum width away.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/TextField.kt, TextField
#[test]
fn a_label_above_keeps_the_minimum_size() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            Box!(
                modifier = Modifier.test_tag("around"),
                content = |_| {
                    TextField!(
                        String::new(),
                        |_: String| {},
                        label = Some(|| Spacer(Modifier.size(20.dp()))),
                        label_position = TextFieldLabelPosition::above(),
                        content_padding = Some(PaddingValues::ZERO)
                    );
                }
            );
        });
    });
    settle(&mut ui);
    let around = bounds(&mut ui, "around");
    assert_eq!(around.width(), 280.0);
    // The minimum is of the whole: the label's 20 and its 4 are in the 56.
    assert_eq!(around.height(), 56.0);
}

/// The container's what's left of a given height by the supporting text.
#[test]
fn a_set_height_is_shared_with_the_supporting_text() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.height(100.dp()).test_tag(FIELD),
                colors = TextFieldColors {
                    unfocused_indicator_color: Color::RED,
                    ..TextFieldDefaults::colors()
                },
                supporting_text = Some(|| Spacer(Modifier.height(16.dp()).fill_max_width()))
            );
        });
    });
    settle(&mut ui);
    let line = ui
        .host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawPath { path, paint, .. } if paint.color == Color::RED => {
                Some(path.bounds())
            }
            _ => None,
        })
        .expect("the line");
    assert_eq!(bounds(&mut ui, FIELD).height(), 100.0);
    // 16, with its 4 above.
    assert_eq!(line.bottom, 80.0);
}

/// A supporting text coming, as validation does while typing, keeps the field and its focus.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, InsideTextFieldLayout
#[test]
fn a_supporting_text_coming_keeps_the_focus() {
    let state = mutable_state_of(false);
    let handle = state.handle();
    let source = MutableInteractionSource::new();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let source = content_source.clone();
        let shows = handle.get();
        MaterialTheme!({
            TextField!(
                "Text".to_string(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                supporting_text = shows.then_some(|| Text!("Wrong")),
                interaction_source = Some(source.clone())
            );
        });
    });
    settle(&mut ui);
    ui.on_node_with_tag(FIELD).request_focus();
    settle(&mut ui);
    assert!(source.is_focused());
    state.set(true);
    settle(&mut ui);
    assert!(source.is_focused());
    state.set(false);
    settle(&mut ui);
    assert!(source.is_focused());
}

/// The label going elsewhere keeps it too.
#[test]
fn a_new_label_position_keeps_the_focus() {
    let state = mutable_state_of(TextFieldLabelPosition::inside());
    let handle = state.handle();
    let source = MutableInteractionSource::new();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let source = content_source.clone();
        let position = handle.get();
        MaterialTheme!({
            TextField!(
                "Text".to_string(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                label = Some(|| Text!("Label")),
                label_position = position,
                interaction_source = Some(source.clone())
            );
        });
    });
    settle(&mut ui);
    ui.on_node_with_tag(FIELD).request_focus();
    settle(&mut ui);
    for position in [
        TextFieldLabelPosition::above(),
        TextFieldLabelPosition::cutout(),
        TextFieldLabelPosition::inside(),
    ] {
        state.set(position);
        settle(&mut ui);
        assert!(source.is_focused(), "at {position}");
    }
}

/// What the source says is what the field looks like, as `collectIsFocusedAsState`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, CommonDecorationBox
#[test]
fn a_field_looks_as_its_source_says() {
    let source = MutableInteractionSource::new();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let source = content_source.clone();
        MaterialTheme!({
            TextField!(
                "Text".to_string(),
                |_: String| {},
                colors = TextFieldColors {
                    focused_indicator_color: Color::RED,
                    unfocused_indicator_color: Color::BLUE,
                    ..TextFieldDefaults::colors()
                },
                interaction_source = Some(source.clone())
            );
        });
    });
    settle(&mut ui);
    let line = |ui: &mut ComposeTestRule| {
        ui.host()
            .display_list()
            .ops
            .iter()
            .find_map(|op| match op {
                DrawOp::DrawPath { path, paint, .. }
                    if paint.color == Color::RED || paint.color == Color::BLUE =>
                {
                    Some((paint.color, path.bounds().height()))
                }
                _ => None,
            })
            .expect("the line")
    };
    assert_eq!(line(&mut ui), (Color::BLUE, 1.0));
    let focus = compose_ui::Focus::new();
    source.try_emit(compose_ui::FocusInteraction::Focus(focus));
    settle(&mut ui);
    assert_eq!(line(&mut ui), (Color::RED, 2.0));
    source.try_emit(compose_ui::FocusInteraction::Unfocus(focus));
    settle(&mut ui);
    assert_eq!(line(&mut ui), (Color::BLUE, 1.0));
}

/// Focus on an icon button in the field isn't the text's: the line stays as it is.
#[test]
fn focus_on_an_icon_button_is_not_the_fields() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                "Text".to_string(),
                |_: String| {},
                colors = TextFieldColors {
                    focused_indicator_color: Color::RED,
                    unfocused_indicator_color: Color::BLUE,
                    ..TextFieldDefaults::colors()
                },
                trailing_icon = Some(|| {
                    IconButton!(|| {}, modifier = Modifier.test_tag("button"), {
                        Spacer(Modifier.size(24.dp()));
                    });
                })
            );
        });
    });
    settle(&mut ui);
    ui.on_node_with_tag("button").request_focus();
    settle(&mut ui);
    let drawn = format!("{:?}", ui.host().display_list().ops);
    assert!(!drawn.contains(&format!("{:?}", Color::RED)), "{drawn}");
}

thread_local! {
    static SEEN: std::cell::RefCell<Vec<(&'static str, Color, Color, f32)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[composable]
fn Probe(name: &'static str) {
    let (color, style) = (LOCAL_CONTENT_COLOR.current(), LOCAL_TEXT_STYLE.current());
    let size = style.font_size.value();
    SEEN.with(|seen| seen.borrow_mut().push((name, color, style.color, size)));
}

/// In a placeholder, a prefix and a suffix: the slot's color as the content color, body large as
/// it is, so a `Text` and an icon in them both take the slot's color.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, CommonDecorationBox
#[test]
fn the_slots_color_is_the_content_color() {
    SEEN.with(|seen| seen.borrow_mut().clear());
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                text_style = TextStyle { font_size: 40.sp(), ..TextStyle::DEFAULT },
                placeholder = Some(|| Probe("placeholder")),
                prefix = Some(|| Probe("prefix")),
                suffix = Some(|| Probe("suffix")),
                colors = TextFieldColors {
                    unfocused_placeholder_color: Color::RED,
                    unfocused_prefix_color: Color::GREEN,
                    unfocused_suffix_color: Color::BLUE,
                    ..TextFieldDefaults::colors()
                }
            );
        });
    });
    settle(&mut ui);
    let seen = SEEN.with(|seen| seen.borrow().clone());
    for (name, color) in
        [("placeholder", Color::RED), ("prefix", Color::GREEN), ("suffix", Color::BLUE)]
    {
        let probe = seen.iter().rfind(|probe| probe.0 == name).expect(name);
        assert_eq!(probe.1, color, "{name}'s content color");
        assert_eq!(probe.2, Color::UNSPECIFIED, "{name}'s style's color");
        assert_eq!(probe.3, 16.0, "{name}'s body large");
    }
}

/// A disabled filled container is as an enabled one.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/TextFieldDefaults.kt, defaultTextFieldColors
#[test]
fn the_filled_containers_are_all_of_the_highest_surface() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        MaterialTheme!({
            let scheme = MaterialTheme::color_scheme();
            let colors = TextFieldDefaults::colors();
            for color in [
                colors.unfocused_container_color,
                colors.focused_container_color,
                colors.error_container_color,
                colors.disabled_container_color,
            ] {
                assert_eq!(color, scheme.surface_container_highest);
            }
        });
    });
    ui.wait_for_idle();
}

/// An outline in a shape not all of one corner is the shape's, not four of the first corner's.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/TextFieldDefaults.kt, Container
#[test]
fn an_outline_follows_a_shape_of_several_corners() {
    let shape = Shape::Rounded([
        compose_ui::CornerSize::Dp(16.dp()),
        compose_ui::CornerSize::Dp(16.dp()),
        compose_ui::CornerSize::Dp(0.dp()),
        compose_ui::CornerSize::Dp(0.dp()),
    ]);
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let shape = shape.clone();
        MaterialTheme!({
            OutlinedTextField!(
                "Text".to_string(),
                |_: String| {},
                colors = TextFieldColors {
                    unfocused_indicator_color: Color::RED,
                    ..OutlinedTextFieldDefaults::colors()
                },
                shape = shape.clone()
            );
        });
    });
    settle(&mut ui);
    let ops = &ui.host().display_list().ops;
    let outline = ops.iter().find_map(|op| match op {
        DrawOp::DrawPath { path, paint, .. } if paint.color == Color::RED => Some(path.clone()),
        _ => None,
    });
    let outline = outline.expect("a path");
    // Two curves in all, of the top, for two rounded corners, not four: a border, not a rounded
    // rectangle.
    let bottom = outline.bounds().bottom;
    let curves_below = outline
        .commands()
        .iter()
        .filter(|command| {
            matches!(command, compose_ui::PathCommand::CubicTo(_, _, end)
                | compose_ui::PathCommand::QuadTo(_, end) if end.y > bottom - 16.0)
        })
        .count();
    assert_eq!(curves_below, 0, "{:?}", outline.commands());
}

/// A cut shape's outline is cut too.
#[test]
fn an_outline_follows_a_cut_shape() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            OutlinedTextField!(
                "Text".to_string(),
                |_: String| {},
                colors = TextFieldColors {
                    unfocused_indicator_color: Color::RED,
                    ..OutlinedTextFieldDefaults::colors()
                },
                shape = Shape::Cut([compose_ui::CornerSize::Dp(8.dp()); 4])
            );
        });
    });
    settle(&mut ui);
    let drawn = ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::DrawPath { path, paint, .. } if paint.color == Color::RED => Some(path.clone()),
        _ => None,
    });
    assert!(drawn.is_some_and(|path| path.commands().len() > 4));
}

/// The room a clip takes out: the second contour's bounds.
fn cut_of(ui: &mut ComposeTestRule) -> compose_ui::Rect {
    let clip = ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::ClipPath { path, .. } => Some(path.clone()),
        _ => None,
    });
    let clip = clip.expect("a clip");
    assert_eq!(clip.fill_type(), compose_ui::PathFillType::EvenOdd);
    let closes = clip
        .commands()
        .iter()
        .position(|command| matches!(command, compose_ui::PathCommand::Close))
        .expect("a contour");
    let points: Vec<compose_ui::Offset> = clip.commands()[closes + 1..]
        .iter()
        .filter_map(|command| match command {
            compose_ui::PathCommand::MoveTo(at) | compose_ui::PathCommand::LineTo(at) => Some(*at),
            _ => None,
        })
        .collect();
    let (xs, ys) = (points.iter().map(|at| at.x), points.iter().map(|at| at.y));
    compose_ui::Rect::new(
        xs.clone().fold(f32::MAX, f32::min),
        ys.clone().fold(f32::MAX, f32::min),
        xs.fold(f32::MIN, f32::max),
        ys.fold(f32::MIN, f32::max),
    )
}

/// A cut as high as the label on the edge, all of what's behind it: a fill too.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/OutlinedTextField.kt, outlineCutout
#[test]
fn a_cut_is_as_high_as_the_label_and_clips_the_fill() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            OutlinedTextField!(
                "Text".to_string(),
                |_: String| {},
                modifier = Modifier.width(300.dp()),
                label = Some(|| Spacer(Modifier.width(20.dp()).height(40.dp()))),
                colors = TextFieldColors {
                    unfocused_container_color: Color::RED,
                    ..OutlinedTextFieldDefaults::colors()
                }
            );
        });
    });
    settle(&mut ui);
    let cut = cut_of(&mut ui);
    assert_eq!((cut.left, cut.right), (16.0 - 4.0, 16.0 + 20.0 + 4.0));
    assert_eq!((cut.top, cut.bottom), (-20.0, 20.0));
}

/// Right to left, the cut's at the right, by each side's padding.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/OutlinedTextField.kt, outlineCutout
#[test]
fn right_to_left_the_cut_is_on_the_right() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl),
            || {
                MaterialTheme!({
                    OutlinedTextField!(
                        "Text".to_string(),
                        |_: String| {},
                        modifier = Modifier.width(300.dp()),
                        label = Some(|| Spacer(Modifier.width(20.dp()).height(10.dp()))),
                        content_padding = Some(PaddingValues {
                            start: 30.dp(),
                            top: 16.dp(),
                            end: 10.dp(),
                            bottom: 16.dp(),
                        }),
                        colors = TextFieldColors {
                            unfocused_container_color: Color::RED,
                            ..OutlinedTextFieldDefaults::colors()
                        }
                    );
                });
            },
        );
    });
    settle(&mut ui);
    let cut = cut_of(&mut ui);
    // 30 from the right, the label's 20 and 4 either side.
    assert_eq!((cut.left, cut.right), (300.0 - 30.0 - 20.0 - 4.0, 300.0 - 30.0 + 4.0));
}

/// A default outline with nothing to fill clips nothing: it's cut in its own path.
#[test]
fn a_default_outline_has_no_clip() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            OutlinedTextField!(
                "Text".to_string(),
                |_: String| {},
                label = Some(|| Spacer(Modifier.width(20.dp()).height(10.dp())))
            );
        });
    });
    settle(&mut ui);
    let ops = &ui.host().display_list().ops;
    assert!(!ops.iter().any(|op| matches!(op, DrawOp::ClipPath { .. })));
}

/// The clip's path is kept between draws while it's the same, a keystroke not building it.
#[test]
fn the_cut_is_kept_between_draws() {
    let state = mutable_state_of("a".to_string());
    let handle = state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let value = handle.get();
        MaterialTheme!({
            TextField!(
                value.clone(),
                |_: String| {},
                label = Some(|| Spacer(Modifier.width(20.dp()).height(10.dp()))),
                label_position = TextFieldLabelPosition::cutout()
            );
        });
    });
    settle(&mut ui);
    let id = |ui: &mut ComposeTestRule| {
        ui.host().display_list().ops.iter().find_map(|op| match op {
            DrawOp::ClipPath { path, .. } => Some((path.id(), path.generation())),
            _ => None,
        })
    };
    let before = id(&mut ui);
    assert!(before.is_some());
    state.set("ab".to_string());
    settle(&mut ui);
    assert_eq!(id(&mut ui), before);
}

/// The intrinsic height has the label's, as `calculateHeight` has.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, intrinsicHeight
#[test]
fn the_intrinsic_height_has_a_label_of_its_height() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            Column!(
                modifier = Modifier.height(IntrinsicSize::Min).test_tag(FIELD),
                content = |_| {
                    TextField!(
                        "Text".to_string(),
                        |_: String| {},
                        label = Some(|| Spacer(Modifier.width(20.dp()).height(30.dp()))),
                        placeholder = Some(|| Spacer(Modifier.height(30.dp())))
                    );
                }
            );
        });
    });
    settle(&mut ui);
    // 8 + the label's 30 + the text's line, 24 + 8.
    assert_eq!(bounds(&mut ui, FIELD).height(), 8.0 + 30.0 + 24.0 + 8.0);
}

/// Asked for the size with supporting text and a label above: they're all in it.
#[test]
fn the_intrinsic_height_has_the_supporting_text_and_a_label_above() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            Column!(
                modifier = Modifier.height(IntrinsicSize::Min).test_tag(FIELD),
                content = |_| {
                    TextField!(
                        "Text".to_string(),
                        |_: String| {},
                        label = Some(|| Spacer(Modifier.width(20.dp()).height(30.dp()))),
                        label_position = TextFieldLabelPosition::above(),
                        supporting_text = Some(|| Spacer(Modifier.height(30.dp())))
                    );
                }
            );
        });
    });
    settle(&mut ui);
    // 30 + 4 above; 56 for the field; 4 + 30 below.
    assert_eq!(bounds(&mut ui, FIELD).height(), 34.0 + 56.0 + 34.0);
}

/// A floated label on the edge has the width between the paddings, over the icons.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, OutlinedTextFieldMeasurePolicy
#[test]
fn a_floated_cut_label_has_the_width_over_the_icons() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            OutlinedTextField!(
                "Text".to_string(),
                |_: String| {},
                modifier = Modifier.width(280.dp()),
                label = Some(|| Spacer(Modifier.fill_max_width().height(16.dp()).test_tag(HINT))),
                leading_icon = Some(|| Spacer(Modifier.size(24.dp()))),
                trailing_icon = Some(|| Spacer(Modifier.size(24.dp())))
            );
        });
    });
    settle(&mut ui);
    assert_eq!(bounds(&mut ui, HINT).width(), 280.0 - 32.0);
}

/// A suffix is measured in what the prefix left.
#[test]
fn a_suffix_has_what_the_prefix_left() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                "Text".to_string(),
                |_: String| {},
                modifier = Modifier.width(280.dp()),
                prefix = Some(|| Spacer(Modifier.width(200.dp()))),
                suffix = Some(|| Spacer(Modifier.fill_max_width().test_tag(HINT)))
            );
        });
    });
    settle(&mut ui);
    // 280, less the prefix's 200 + its 16 + 2, less the suffix's own 2 + 16.
    assert_eq!(bounds(&mut ui, HINT).width(), 280.0 - 218.0 - 18.0);
}

/// Above in a set height, a single line is centered in what's below the label.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, placeWithLabel
#[test]
fn a_placeholder_below_a_label_above_starts_at_the_padding() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.height(200.dp()).test_tag(FIELD),
                single_line = true,
                label = Some(|| Spacer(Modifier.width(20.dp()).height(30.dp()))),
                label_position = TextFieldLabelPosition::above(),
                placeholder =
                    Some(|| Spacer(Modifier.width(10.dp()).height(24.dp()).test_tag(HINT)))
            );
        });
    });
    settle(&mut ui);
    let field = bounds(&mut ui, FIELD);
    // Below the label and its 4, at the top padding, as `textPosition`.
    assert_eq!(bounds(&mut ui, HINT).top - field.top, 34.0 + 16.0);
}

/// A prefix or suffix keeps the field's text in what's read out.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, visualTextSemantics
#[test]
fn with_an_affix_the_text_is_read_between() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                "Value".to_string(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                prefix = Some(|| Text!("Pre")),
                suffix = Some(|| Text!("Suf"))
            );
        });
    });
    settle(&mut ui);
    let node = ui.on_node_with_tag(FIELD).fetch_semantics_snapshot();
    let text = format!("{:?}", node.config.get(&compose_ui::SemanticsProperties::TEXT));
    let (pre, value, suf) =
        (text.find("Pre").unwrap(), text.find("Value").unwrap(), text.find("Suf").unwrap());
    assert!(pre < value && value < suf, "{text}");
}

/// A prefix coming keeps the field and its focus too, though it's composed before the text.
#[test]
fn a_prefix_coming_keeps_the_focus() {
    let state = mutable_state_of(false);
    let handle = state.handle();
    let source = MutableInteractionSource::new();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let source = content_source.clone();
        let shows = handle.get();
        MaterialTheme!({
            TextField!(
                "Text".to_string(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                prefix = shows.then_some(|| Text!("Pre")),
                interaction_source = Some(source.clone())
            );
        });
    });
    settle(&mut ui);
    ui.on_node_with_tag(FIELD).request_focus();
    settle(&mut ui);
    state.set(true);
    settle(&mut ui);
    assert!(source.is_focused());
    state.set(false);
    settle(&mut ui);
    assert!(source.is_focused());
}

/// Resting in a field of several lines, the label's a half minimized line below the padding.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, TextFieldMeasurePolicy
#[test]
fn a_resting_label_is_half_a_minimized_line_lower() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        let mut typography = Typography::default();
        typography.body_small = TextStyle { line_height: 24.sp(), ..typography.body_small };
        MaterialTheme!(typography = typography, {
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                label = Some(|| Spacer(Modifier.width(20.dp()).height(24.dp()).test_tag(HINT)))
            );
        });
    });
    settle(&mut ui);
    let field = bounds(&mut ui, FIELD);
    assert_eq!(bounds(&mut ui, HINT).top - field.top, 8.0 + 12.0);
}

/// Resting, the label's at least a line of text high, minimized a small line, the lerp of them.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, textFieldLabelMinHeight
#[test]
fn a_labels_least_height_goes_with_it() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                label = Some(|| Spacer(Modifier.width(20.dp()).height(10.dp()).test_tag(HINT)))
            );
        });
    });
    settle(&mut ui);
    let field = bounds(&mut ui, FIELD);
    // At 8 + 8, in 24 high: centered, 7 in.
    assert_eq!(bounds(&mut ui, HINT).top - field.top, 16.0 + 7.0);
    ui.on_node_with_tag(FIELD).request_focus();
    settle(&mut ui);
    // At 8, in 16 high: 3 in.
    assert_eq!(bounds(&mut ui, HINT).top - field.top, 8.0 + 3.0);
}

/// The prefix is at least a line of text high.
#[test]
fn a_prefix_is_at_least_a_line_high() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                prefix = Some(|| Spacer(Modifier.width(20.dp()).height(10.dp()).test_tag(HINT)))
            );
        });
    });
    settle(&mut ui);
    let field = bounds(&mut ui, FIELD);
    assert_eq!(bounds(&mut ui, HINT).top - field.top, 16.0 + 7.0);
}

/// The text's what a set height leaves, the supporting text's intrinsic height kept.
#[test]
fn the_text_is_measured_in_what_the_supporting_text_leaves() {
    for outlined in [false, true] {
        let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
            MaterialTheme!({
                let source = remember(MutableInteractionSource::new);
                Box!(
                    modifier = Modifier.height(200.dp()),
                    content = move |_| {
                        let inner = || {
                            Spacer(Modifier.width(20.dp()).fill_max_height().test_tag(HINT));
                        };
                        let supporting = Some(|| Spacer(Modifier.height(40.dp())));
                        if outlined {
                            OutlinedTextFieldDecorationBox!(
                                "Text".to_string(),
                                inner,
                                true,
                                false,
                                compose_foundation::VisualTransformation::NONE,
                                source.clone(),
                                supporting_text = supporting
                            );
                        } else {
                            TextFieldDecorationBox!(
                                "Text".to_string(),
                                inner,
                                true,
                                false,
                                compose_foundation::VisualTransformation::NONE,
                                source.clone(),
                                supporting_text = supporting
                            );
                        }
                    }
                );
            });
        });
        settle(&mut ui);
        // 200, less 16 and 16, less the supporting text's 40 and its 4 above.
        assert_eq!(bounds(&mut ui, HINT).height(), 200.0 - 32.0 - 44.0, "outlined {outlined}");
    }
}

/// A label on the edge wider than the rest makes the field wider by the paddings, floated.
#[test]
fn a_floated_cut_label_widens_the_field() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            OutlinedTextField!(
                "Text".to_string(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                label = Some(|| Spacer(Modifier.width(400.dp()).height(10.dp())))
            );
        });
    });
    settle(&mut ui);
    assert_eq!(bounds(&mut ui, FIELD).width(), 400.0 + 32.0);
}

/// Resting, a tall label on the edge asks for no more than the top padding above it.
#[test]
fn a_resting_cut_label_takes_the_padding_above() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            Box!(
                modifier = Modifier.test_tag(FIELD),
                content = |_| {
                    let source = remember(MutableInteractionSource::new);
                    OutlinedTextFieldDecorationBox!(
                        String::new(),
                        || Spacer(Modifier.width(10.dp()).height(10.dp())),
                        true,
                        false,
                        compose_foundation::VisualTransformation::NONE,
                        source,
                        label = Some(|| Spacer(Modifier.width(20.dp()).height(60.dp())))
                    );
                }
            );
        });
    });
    settle(&mut ui);
    assert_eq!(bounds(&mut ui, FIELD).height(), 16.0 + 60.0 + 16.0);
}

/// Asked its height, floated on the edge, a label has the width over the icons.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, OutlinedTextFieldMeasurePolicy
#[test]
fn the_intrinsic_height_has_a_floated_cut_label_over_the_icons() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            Column!(
                modifier = Modifier.width(280.dp()).height(IntrinsicSize::Min).test_tag(FIELD),
                content = |_| {
                    OutlinedTextField!(
                        "Text".to_string(),
                        |_: String| {},
                        modifier = Modifier.fill_max_width(),
                        label = Some(|| Text!("x ".repeat(30))),
                        leading_icon = Some(|| Spacer(Modifier.size(24.dp()))),
                        trailing_icon = Some(|| Spacer(Modifier.size(24.dp())))
                    );
                }
            );
        });
    });
    settle(&mut ui);
    // 420 px of text: two lines in 280, of 16: half of it's the padding. In 184 it'd be three.
    assert_eq!(bounds(&mut ui, FIELD).height(), 8.0 + 16.0 + 24.0 + 16.0);
}

/// An empty text isn't read between them.
#[test]
fn with_an_affix_an_empty_text_is_not_read() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                prefix = Some(|| Text!("Pre"))
            );
        });
    });
    settle(&mut ui);
    let node = ui.on_node_with_tag(FIELD).fetch_semantics_snapshot();
    let text = node.config.get(&compose_ui::SemanticsProperties::TEXT).cloned().unwrap();
    assert_eq!(text.len(), 1, "{text:?}");
}

/// A new label's width is cut: its state tells the container, which draws apart.
#[test]
fn a_cut_follows_the_labels_width() {
    let state = mutable_state_of(20.0f32);
    let handle = state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let width = handle.get();
        MaterialTheme!({
            TextField!(
                "Text".to_string(),
                |_: String| {},
                modifier = Modifier.width(300.dp()),
                label = Some(move || Spacer(Modifier.width(width.dp()).height(10.dp()))),
                label_position = TextFieldLabelPosition::cutout()
            );
        });
    });
    settle(&mut ui);
    assert_eq!(cut_of(&mut ui).right, 16.0 + 20.0 + 4.0);
    state.set(50.0);
    settle(&mut ui);
    assert_eq!(cut_of(&mut ui).right, 16.0 + 50.0 + 4.0);
}

/// The text's at least a line high, as `MinTextLineHeight`.
#[test]
fn the_text_is_at_least_a_line_high() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), || {
        MaterialTheme!({
            Box!(
                modifier = Modifier.test_tag(FIELD),
                content = |_| {
                    let source = remember(MutableInteractionSource::new);
                    TextFieldDecorationBox!(
                        String::new(),
                        || Spacer(Modifier.width(10.dp()).height(10.dp())),
                        true,
                        false,
                        compose_foundation::VisualTransformation::NONE,
                        source,
                        content_padding = Some(PaddingValues::ZERO)
                    );
                }
            );
        });
    });
    settle(&mut ui);
    assert_eq!(bounds(&mut ui, FIELD).height(), 24.0);
}

/// Each part's color by the state: disabled first, then in error, then focused.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/TextFieldDefaults.kt, TextFieldColors
#[test]
fn the_colors_go_by_the_state() {
    let (red, green, blue, black) = (Color::RED, Color::GREEN, Color::BLUE, Color::BLACK);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        MaterialTheme!({
            let given = TextFieldColors {
                disabled_text_color: red,
                error_text_color: green,
                focused_text_color: blue,
                unfocused_text_color: black,
                disabled_container_color: red,
                error_container_color: green,
                focused_container_color: blue,
                unfocused_container_color: black,
                disabled_indicator_color: red,
                error_indicator_color: green,
                focused_indicator_color: blue,
                unfocused_indicator_color: black,
                disabled_leading_icon_color: red,
                error_leading_icon_color: green,
                focused_leading_icon_color: blue,
                unfocused_leading_icon_color: black,
                disabled_trailing_icon_color: red,
                error_trailing_icon_color: green,
                focused_trailing_icon_color: blue,
                unfocused_trailing_icon_color: black,
                disabled_label_color: red,
                error_label_color: green,
                focused_label_color: blue,
                unfocused_label_color: black,
                disabled_placeholder_color: red,
                error_placeholder_color: green,
                focused_placeholder_color: blue,
                unfocused_placeholder_color: black,
                disabled_supporting_text_color: red,
                error_supporting_text_color: green,
                focused_supporting_text_color: blue,
                unfocused_supporting_text_color: black,
                disabled_prefix_color: red,
                error_prefix_color: green,
                focused_prefix_color: blue,
                unfocused_prefix_color: black,
                disabled_suffix_color: red,
                error_suffix_color: green,
                focused_suffix_color: blue,
                unfocused_suffix_color: black,
                cursor_color: blue,
                error_cursor_color: green,
                ..TextFieldDefaults::colors()
            };
            type Pick = fn(&TextFieldColors, bool, bool, bool) -> Color;
            let parts: [(&str, Pick); 10] = [
                ("text", TextFieldColors::text_color),
                ("container", TextFieldColors::container_color),
                ("indicator", TextFieldColors::indicator_color),
                ("leading", TextFieldColors::leading_icon_color),
                ("trailing", TextFieldColors::trailing_icon_color),
                ("label", TextFieldColors::label_color),
                ("placeholder", TextFieldColors::placeholder_color),
                ("supporting", TextFieldColors::supporting_text_color),
                ("prefix", TextFieldColors::prefix_color),
                ("suffix", TextFieldColors::suffix_color),
            ];
            for (name, pick) in parts {
                assert_eq!(pick(&given, false, true, true), red, "{name} disabled");
                assert_eq!(pick(&given, true, true, true), green, "{name} in error");
                assert_eq!(pick(&given, true, false, true), blue, "{name} focused");
                assert_eq!(pick(&given, true, false, false), black, "{name} unfocused");
            }
            assert_eq!(given.cursor_color(false), blue);
            assert_eq!(given.cursor_color(true), green);
        });
    });
    ui.wait_for_idle();
}

/// A field goes by them: an icon in its focused color, its text in its error color.
#[test]
fn a_field_takes_the_focused_and_error_colors() {
    SEEN.with(|seen| seen.borrow_mut().clear());
    let is_error_state = mutable_state_of(false);
    let is_error = is_error_state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let error = is_error.get();
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                is_error = error,
                placeholder = Some(|| Probe("placeholder")),
                leading_icon = Some(|| Probe("leading")),
                trailing_icon = Some(|| Probe("trailing")),
                supporting_text = Some(|| Probe("supporting")),
                prefix = Some(|| Probe("prefix")),
                suffix = Some(|| Probe("suffix")),
                colors = TextFieldColors {
                    focused_placeholder_color: Color::RED,
                    focused_leading_icon_color: Color::RED,
                    focused_trailing_icon_color: Color::RED,
                    focused_supporting_text_color: Color::RED,
                    focused_prefix_color: Color::RED,
                    focused_suffix_color: Color::RED,
                    error_placeholder_color: Color::GREEN,
                    error_prefix_color: Color::GREEN,
                    error_suffix_color: Color::GREEN,
                    ..TextFieldDefaults::colors()
                }
            );
        });
    });
    settle(&mut ui);
    ui.on_node_with_tag(FIELD).request_focus();
    settle(&mut ui);
    let last = |name: &str| {
        SEEN.with(|seen| seen.borrow().iter().rfind(|probe| probe.0 == name).unwrap().1)
    };
    for name in ["placeholder", "leading", "trailing", "supporting", "prefix", "suffix"] {
        assert_eq!(last(name), Color::RED, "{name} focused");
    }
    is_error_state.set(true);
    settle(&mut ui);
    for name in ["placeholder", "prefix", "suffix"] {
        assert_eq!(last(name), Color::GREEN, "{name} in error");
    }
}

/// A text measurer recording the color of each text laid out.
struct Recording {
    inner: compose_ui::platform::FixedTextMeasurer,
    log: std::rc::Rc<std::cell::RefCell<Vec<Color>>>,
}

impl compose_ui::platform::TextMeasurer for Recording {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: compose_ui::platform::TextLayoutOptions,
        constraints: compose_ui::Constraints,
        density: &Density,
    ) -> compose_ui::platform::ParagraphHandle {
        self.log.borrow_mut().push(style.color);
        self.inner.layout(text, style, options, constraints, density)
    }
    fn metrics(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
    ) -> compose_ui::platform::ParagraphMetrics {
        self.inner.metrics(paragraph)
    }
    fn hit_test(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
        position: compose_ui::Offset,
    ) -> compose_ui::platform::TextPosition {
        self.inner.hit_test(paragraph, position)
    }
    fn cursor_rect(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
        offset: usize,
    ) -> compose_ui::Rect {
        self.inner.cursor_rect(paragraph, offset)
    }
    fn text(&self, paragraph: compose_ui::platform::ParagraphHandle) -> &str {
        self.inner.text(paragraph)
    }
    fn release(&mut self, paragraph: compose_ui::platform::ParagraphHandle) {
        self.inner.release(paragraph)
    }
}

/// The text in the state's color: focused, in error.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/TextField.kt, TextField
#[test]
fn the_text_takes_the_focused_and_error_colors() {
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let state = mutable_state_of(false);
    let handle = state.handle();
    let mut ui = compose_test_with(
        IntSize::new(800, 800),
        Density::default(),
        Box::new(Recording {
            inner: compose_ui::platform::FixedTextMeasurer::new(),
            log: log.clone(),
        }),
        move || {
            let is_error = handle.get();
            MaterialTheme!({
                TextField!(
                    "Text".to_string(),
                    |_: String| {},
                    modifier = Modifier.test_tag(FIELD),
                    is_error = is_error,
                    colors = TextFieldColors {
                        unfocused_text_color: Color::BLUE,
                        focused_text_color: Color::RED,
                        error_text_color: Color::GREEN,
                        ..TextFieldDefaults::colors()
                    }
                );
            });
        },
    );
    settle(&mut ui);
    assert_eq!(log.borrow().last(), Some(&Color::BLUE));
    ui.on_node_with_tag(FIELD).request_focus();
    settle(&mut ui);
    assert_eq!(log.borrow().last(), Some(&Color::RED));
    state.set(true);
    settle(&mut ui);
    assert_eq!(log.borrow().last(), Some(&Color::GREEN));
}

thread_local! { static CONTAINERS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }

#[composable]
fn CountedContainer() {
    CONTAINERS.with(|count| count.set(count.get() + 1));
}

/// What's typed composes the decoration, not its container, as Compose's `Container`, which skips:
/// its animations and drawing don't run again for a keystroke (D1564).
#[test]
fn a_keystroke_doesnt_compose_the_container() {
    let value = mutable_state_of(String::new());
    let handle = value.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            let source = remember(MutableInteractionSource::new);
            TextFieldDecorationBox!(
                handle.get(),
                || {},
                true,
                false,
                compose_foundation::VisualTransformation::NONE,
                source,
                container = Some(CountedContainer)
            );
        });
    });
    ui.wait_for_idle();
    let before = CONTAINERS.with(std::cell::Cell::get);
    assert!(before > 0);
    value.set("a".to_string());
    ui.wait_for_idle();
    value.set("ab".to_string());
    ui.wait_for_idle();
    assert_eq!(CONTAINERS.with(std::cell::Cell::get), before);
}
