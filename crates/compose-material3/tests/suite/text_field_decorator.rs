//! Compose's `TextFieldDecoratorTest.kt`, on the value-based `DecorationBox` (`text-field-state`
//! isn't there, so neither is `decorator`); its `normalize_*` are `Attached`, deprecated and not
//! ported. And what else Compose's defaults have for a custom field.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::{
    BasicTextField, ScrollModifiers, VisualTransformation, remember_scroll_state,
};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, IntSize, IntrinsicSize, LayoutDirection, Modifier, MutableInteractionSource,
    PaddingValues, Shape, UnitExt,
};

const FIELD: &str = "field";
const INNER: &str = "inner";
const LABEL_HEIGHT: f32 = 40.0;
const INNER_HEIGHT: f32 = 50.0;
const INNER_WIDTH: f32 = 100.0;
const PADDING: f32 = 16.0;
const WITH_LABEL_PADDING: f32 = 8.0;

#[derive(Clone, Copy, PartialEq)]
struct Case {
    outlined: bool,
    padding: PaddingValues,
    single_line: bool,
    has_label: bool,
    position: TextFieldLabelPosition,
    rtl: bool,
}

impl ComposeArg for Case {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

fn case(outlined: bool, padding: PaddingValues) -> Case {
    Case {
        outlined,
        padding,
        single_line: false,
        has_label: false,
        position: TextFieldLabelPosition::inside(),
        rtl: false,
    }
}

fn filled(padding: PaddingValues) -> Case {
    case(false, padding)
}

fn outlined(padding: PaddingValues) -> Case {
    case(true, padding)
}

impl Case {
    fn single_line(self) -> Case {
        Case { single_line: true, ..self }
    }
    fn with_label(self) -> Case {
        Case { has_label: true, ..self }
    }
    fn at(self, position: TextFieldLabelPosition) -> Case {
        Case { position, has_label: true, ..self }
    }
    fn rtl(self) -> Case {
        Case { rtl: true, ..self }
    }
}

fn top(top: f32) -> PaddingValues {
    PaddingValues { top: top.dp(), ..TextFieldDefaults::content_padding_without_label() }
}

fn bottom(bottom: f32) -> PaddingValues {
    PaddingValues { bottom: bottom.dp(), ..TextFieldDefaults::content_padding_without_label() }
}

fn start(start: f32) -> PaddingValues {
    PaddingValues { start: start.dp(), ..TextFieldDefaults::content_padding_without_label() }
}

fn end(end: f32) -> PaddingValues {
    PaddingValues { end: end.dp(), ..TextFieldDefaults::content_padding_without_label() }
}

fn label_top(top: f32) -> PaddingValues {
    PaddingValues { top: top.dp(), ..TextFieldDefaults::content_padding_with_label() }
}

fn label_bottom(bottom: f32) -> PaddingValues {
    PaddingValues { bottom: bottom.dp(), ..TextFieldDefaults::content_padding_with_label() }
}

fn label_start(start: f32) -> PaddingValues {
    PaddingValues { start: start.dp(), ..TextFieldDefaults::content_padding_with_label() }
}

fn label_end(end: f32) -> PaddingValues {
    PaddingValues { end: end.dp(), ..TextFieldDefaults::content_padding_with_label() }
}

/// The field's whole size and the inner field's place, focused.
fn laid_out(case: Case) -> (compose_ui::Rect, compose_ui::Rect) {
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(if case.rtl {
                LayoutDirection::Rtl
            } else {
                LayoutDirection::Ltr
            }),
            move || {
                MaterialTheme!({
                    Decorated(case);
                });
            },
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    ui.wait_for_idle();
    (
        ui.on_node_with_tag_unmerged(FIELD).fetch_semantics_snapshot().bounds,
        ui.on_node_with_tag_unmerged(INNER).fetch_semantics_snapshot().bounds,
    )
}

#[composable]
fn Decorated(case: Case) {
    let source = remember(MutableInteractionSource::new);
    let decoration_source = source.clone();
    BasicTextField!(
        "Text".to_string(),
        |_: String| {},
        modifier = Modifier.test_tag(FIELD),
        single_line = case.single_line,
        interaction_source = Some(source.clone()),
        decoration_box = move |inner_text_field: Callback| {
            let inner = move || {
                let inner_text_field = inner_text_field.clone();
                Box!(
                    modifier =
                        Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp()).test_tag(INNER),
                    content = move |_| inner_text_field()
                );
            };
            let label = case.has_label.then_some(|| {
                Spacer(Modifier.width(20.dp()).height(LABEL_HEIGHT.dp()));
            });
            if case.outlined {
                OutlinedTextFieldDecorationBox!(
                    "Text".to_string(),
                    inner,
                    true,
                    case.single_line,
                    VisualTransformation::NONE,
                    decoration_source.clone(),
                    label_position = case.position,
                    label = label,
                    content_padding = Some(case.padding)
                );
            } else {
                TextFieldDecorationBox!(
                    "Text".to_string(),
                    inner,
                    true,
                    case.single_line,
                    VisualTransformation::NONE,
                    decoration_source.clone(),
                    label_position = case.position,
                    label = label,
                    content_padding = Some(case.padding)
                );
            }
        }
    );
}

fn assert_vertical(case: Case, height: f32, position: f32) {
    let (field, inner) = laid_out(case);
    assert_eq!(field.height(), height, "height");
    assert_eq!(inner.top - field.top, position, "position");
}

fn assert_horizontal(case: Case, width: f32, position: f32) {
    let (field, inner) = laid_out(case);
    assert_eq!(field.width(), width, "width");
    assert_eq!(inner.left - field.left, position, "position");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideTopPadding_multiLine
#[test]
fn outlined_top_padding_multi_line() {
    assert_vertical(outlined(top(10.0)), 10.0 + INNER_HEIGHT + PADDING, 10.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideTopPadding_singleLine
#[test]
fn outlined_top_padding_single_line() {
    assert_vertical(
        outlined(top(10.0)).single_line(),
        10.0 + INNER_HEIGHT + PADDING,
        (10.0 + PADDING) / 2.0,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideBottomPadding_multiLine
#[test]
fn outlined_bottom_padding_multi_line() {
    assert_vertical(outlined(bottom(10.0)), PADDING + INNER_HEIGHT + 10.0, PADDING);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideBottomPadding_singleLine
#[test]
fn outlined_bottom_padding_single_line() {
    assert_vertical(
        outlined(bottom(10.0)).single_line(),
        PADDING + INNER_HEIGHT + 10.0,
        (10.0 + PADDING) / 2.0,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideStartPadding
#[test]
fn outlined_start_padding() {
    assert_horizontal(outlined(start(10.0)), 10.0 + INNER_WIDTH + PADDING, 10.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideStartPadding_rtl
#[test]
fn outlined_start_padding_right_to_left() {
    assert_horizontal(outlined(start(10.0)).rtl(), 10.0 + INNER_WIDTH + PADDING, PADDING);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideEndPadding
#[test]
fn outlined_end_padding() {
    assert_horizontal(outlined(end(20.0)), PADDING + INNER_WIDTH + 20.0, PADDING);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideEndPadding_rtl
#[test]
fn outlined_end_padding_right_to_left() {
    assert_horizontal(outlined(end(20.0)).rtl(), PADDING + INNER_WIDTH + 20.0, 20.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideTopPadding_multiLine_withLabel
#[test]
fn outlined_top_padding_multi_line_with_label() {
    assert_vertical(
        outlined(label_top(40.0)).with_label(),
        40.0 + LABEL_HEIGHT + INNER_HEIGHT + WITH_LABEL_PADDING,
        40.0 + LABEL_HEIGHT,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideTopPadding_singleLine_withLabel
#[test]
fn outlined_top_padding_single_line_with_label() {
    assert_vertical(
        outlined(label_top(40.0)).with_label().single_line(),
        40.0 + LABEL_HEIGHT + INNER_HEIGHT + WITH_LABEL_PADDING,
        40.0 + LABEL_HEIGHT,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideBottomPadding_multiLine_withLabel
#[test]
fn outlined_bottom_padding_multi_line_with_label() {
    assert_vertical(
        outlined(label_bottom(40.0)).with_label(),
        WITH_LABEL_PADDING + LABEL_HEIGHT + INNER_HEIGHT + 40.0,
        WITH_LABEL_PADDING + LABEL_HEIGHT,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideBottomPadding_singleLine_withLabel
#[test]
fn outlined_bottom_padding_single_line_with_label() {
    assert_vertical(
        outlined(label_bottom(40.0)).with_label().single_line(),
        WITH_LABEL_PADDING + LABEL_HEIGHT + INNER_HEIGHT + 40.0,
        WITH_LABEL_PADDING + LABEL_HEIGHT,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideStartPadding_withLabel
#[test]
fn outlined_start_padding_with_label() {
    assert_horizontal(outlined(label_start(40.0)).with_label(), 40.0 + INNER_WIDTH + PADDING, 40.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_overrideEndPadding_withLabel
#[test]
fn outlined_end_padding_with_label() {
    assert_horizontal(
        outlined(label_end(40.0)).with_label(),
        PADDING + INNER_WIDTH + 40.0,
        PADDING,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideTopPadding_singleLine_withoutLabel
#[test]
fn filled_top_padding_single_line() {
    assert_vertical(
        filled(top(40.0)).single_line(),
        40.0 + INNER_HEIGHT + PADDING,
        (40.0 + PADDING) / 2.0,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideTopPadding_singleLine_withLabel
#[test]
fn filled_top_padding_single_line_with_label() {
    assert_vertical(
        filled(label_top(40.0)).single_line().with_label(),
        40.0 + LABEL_HEIGHT + INNER_HEIGHT + WITH_LABEL_PADDING,
        40.0 + LABEL_HEIGHT,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideBottomPadding_singleLine_withoutLabel
#[test]
fn filled_bottom_padding_single_line() {
    assert_vertical(
        filled(bottom(40.0)).single_line(),
        PADDING + INNER_HEIGHT + 40.0,
        (40.0 + PADDING) / 2.0,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideBottomPadding_singleLine_withLabel
#[test]
fn filled_bottom_padding_single_line_with_label() {
    assert_vertical(
        filled(label_bottom(40.0)).single_line().with_label(),
        WITH_LABEL_PADDING + LABEL_HEIGHT + INNER_HEIGHT + 40.0,
        WITH_LABEL_PADDING + LABEL_HEIGHT,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideTopPadding_multiLine_withoutLabel
#[test]
fn filled_top_padding_multi_line() {
    assert_vertical(filled(top(40.0)), 40.0 + INNER_HEIGHT + PADDING, 40.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideTopPadding_multiLine_withLabel
#[test]
fn filled_top_padding_multi_line_with_label() {
    assert_vertical(
        filled(label_top(40.0)).with_label(),
        40.0 + LABEL_HEIGHT + INNER_HEIGHT + WITH_LABEL_PADDING,
        40.0 + LABEL_HEIGHT,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideBottomPadding_multiLine_withoutLabel
#[test]
fn filled_bottom_padding_multi_line() {
    assert_vertical(filled(bottom(40.0)), PADDING + INNER_HEIGHT + 40.0, PADDING);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideBottomPadding_multiLine_withLabel
#[test]
fn filled_bottom_padding_multi_line_with_label() {
    assert_vertical(
        filled(label_bottom(40.0)).with_label(),
        WITH_LABEL_PADDING + LABEL_HEIGHT + INNER_HEIGHT + 40.0,
        WITH_LABEL_PADDING + LABEL_HEIGHT,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideStartPadding_withLabel
#[test]
fn filled_start_padding_with_label() {
    assert_horizontal(filled(label_start(40.0)).with_label(), 40.0 + INNER_WIDTH + PADDING, 40.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideStartPadding_withLabel_rtl
#[test]
fn filled_start_padding_with_label_right_to_left() {
    assert_horizontal(
        filled(label_start(40.0)).with_label().rtl(),
        40.0 + INNER_WIDTH + PADDING,
        PADDING,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideStartPadding_withoutLabel
#[test]
fn filled_start_padding() {
    assert_horizontal(filled(start(40.0)), 40.0 + INNER_WIDTH + PADDING, 40.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideStartPadding_withoutLabel_rtl
#[test]
fn filled_start_padding_right_to_left() {
    assert_horizontal(filled(start(40.0)).rtl(), 40.0 + INNER_WIDTH + PADDING, PADDING);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideEndPadding_withLabel
#[test]
fn filled_end_padding_with_label() {
    assert_horizontal(filled(label_end(40.0)).with_label(), PADDING + INNER_WIDTH + 40.0, PADDING);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideEndPadding_withLabel_rtl
#[test]
fn filled_end_padding_with_label_right_to_left() {
    assert_horizontal(
        filled(label_end(40.0)).with_label().rtl(),
        PADDING + INNER_WIDTH + 40.0,
        40.0,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideEndPadding_withoutLabel
#[test]
fn filled_end_padding() {
    assert_horizontal(filled(end(40.0)), PADDING + INNER_WIDTH + 40.0, PADDING);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overrideEndPadding_withoutLabel_rtl
#[test]
fn filled_end_padding_right_to_left() {
    assert_horizontal(filled(end(40.0)).rtl(), PADDING + INNER_WIDTH + 40.0, 40.0);
}

/// The field with a container of its own, as `Container` is: what it draws.
fn container_ops(content: impl Fn() + 'static) -> Vec<DrawOp> {
    let content = callback(content);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 150), move || {
        let content = content.clone();
        MaterialTheme!({
            let content = content.clone();
            Box!(
                modifier = Modifier.width(300.dp()).height(150.dp()),
                content = move |_| content()
            );
        });
    });
    ui.wait_for_idle();
    ui.host().display_list().ops.clone()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_defaultBorderColor_comesFromColors
#[test]
fn an_outlined_containers_border_comes_from_the_colors() {
    let ops = container_ops(|| {
        let source = remember(MutableInteractionSource::new);
        let colors = TextFieldColors {
            unfocused_indicator_color: Color::RED,
            ..OutlinedTextFieldDefaults::colors()
        };
        OutlinedTextFieldContainer!(
            true,
            false,
            source,
            modifier = Modifier.fill_max_size(),
            colors = colors,
            shape = Shape::Rectangle,
            unfocused_border_thickness = 40.dp()
        );
    });
    let drawn = format!("{ops:?}");
    assert!(drawn.contains(&format!("{:?}", Color::RED)), "{drawn}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_defaultIndicatorLineColor_comesFromColors
#[test]
fn a_filled_containers_line_comes_from_the_colors() {
    let ops = container_ops(|| {
        let source = remember(MutableInteractionSource::new);
        let colors = TextFieldColors {
            unfocused_indicator_color: Color::RED,
            ..TextFieldDefaults::colors()
        };
        TextFieldContainer!(
            true,
            false,
            source,
            modifier = Modifier.fill_max_size(),
            colors = colors,
            shape = Shape::Rectangle,
            unfocused_indicator_line_thickness = 40.dp()
        );
    });
    let line = ops.iter().rev().find_map(|op| match op {
        DrawOp::DrawPath { path, paint, .. } => Some((path.bounds(), paint.color)),
        _ => None,
    });
    let (bounds, color) = line.expect("the line");
    assert_eq!(color, Color::RED);
    assert_eq!((bounds.top, bounds.bottom, bounds.left, bounds.right), (110.0, 150.0, 0.0, 300.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_overridePadding_unfocusedState_withoutLabel_withPlaceholder
#[test]
fn a_placeholder_takes_the_paddings_height() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            let source = remember(MutableInteractionSource::new);
            let decoration_source = source.clone();
            BasicTextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                interaction_source = Some(source.clone()),
                decoration_box = move |inner: Callback| {
                    TextFieldDecorationBox!(
                        String::new(),
                        inner,
                        true,
                        false,
                        VisualTransformation::NONE,
                        decoration_source.clone(),
                        placeholder = Some(|| Spacer(Modifier.size(50.dp()))),
                        content_padding = Some(PaddingValues::symmetric(0.dp(), 10.dp()))
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(
        ui.on_node_with_tag_unmerged(FIELD).fetch_semantics_snapshot().bounds.height(),
        70.0
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_innerTextLocation_withMultilineLabel
#[test]
fn a_tall_label_on_the_edge_pushes_the_text_down() {
    assert_vertical(
        outlined(OutlinedTextFieldDefaults::content_padding_without_label())
            .at(TextFieldLabelPosition::cutout()),
        LABEL_HEIGHT / 2.0 + INNER_HEIGHT + PADDING,
        LABEL_HEIGHT / 2.0,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_singleLine_innerTextLocation_withMultilineLabel
#[test]
fn a_tall_label_on_the_edge_pushes_a_single_line_down() {
    assert_vertical(
        outlined(OutlinedTextFieldDefaults::content_padding_without_label())
            .at(TextFieldLabelPosition::cutout())
            .single_line(),
        LABEL_HEIGHT / 2.0 + INNER_HEIGHT + PADDING,
        LABEL_HEIGHT / 2.0,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, textFieldBox_aboveLabel_measurement
#[test]
fn a_filled_box_with_a_label_above() {
    assert_vertical(
        filled(TextFieldDefaults::content_padding_without_label())
            .at(TextFieldLabelPosition::above()),
        LABEL_HEIGHT + 4.0 + INNER_HEIGHT + PADDING * 2.0,
        LABEL_HEIGHT + 4.0 + PADDING,
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, outlinedTextFieldBox_aboveLabel_measurement
#[test]
fn an_outlined_box_with_a_label_above() {
    assert_vertical(
        outlined(OutlinedTextFieldDefaults::content_padding_without_label())
            .at(TextFieldLabelPosition::above()),
        LABEL_HEIGHT + 4.0 + INNER_HEIGHT + PADDING * 2.0,
        LABEL_HEIGHT + 4.0 + PADDING,
    );
}

/// Without paddings given: where the inner field goes, by whether there's a label and where.
fn default_padding_top(outlined: bool, label: Option<TextFieldLabelPosition>) -> f32 {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        MaterialTheme!({
            let inner = || {
                Spacer(Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp()).test_tag(INNER));
            };
            let source = remember(MutableInteractionSource::new);
            let position = label.unwrap_or(TextFieldLabelPosition::inside());
            let slot =
                label.is_some().then_some(|| Spacer(Modifier.width(20.dp()).height(10.dp())));
            Box!(
                modifier = Modifier.test_tag(FIELD),
                content = move |_| {
                    if outlined {
                        OutlinedTextFieldDecorationBox!(
                            "Text".to_string(),
                            inner,
                            true,
                            false,
                            VisualTransformation::NONE,
                            source.clone(),
                            label_position = position,
                            label = slot
                        );
                    } else {
                        TextFieldDecorationBox!(
                            "Text".to_string(),
                            inner,
                            true,
                            false,
                            VisualTransformation::NONE,
                            source.clone(),
                            label_position = position,
                            label = slot
                        );
                    }
                }
            );
        });
    });
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let field = ui.on_node_with_tag_unmerged(FIELD).fetch_semantics_snapshot().bounds;
    let inner = ui.on_node_with_tag_unmerged(INNER).fetch_semantics_snapshot().bounds;
    inner.top - field.top
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, defaultContentPadding_textFieldDefaults
#[test]
fn a_filled_boxs_default_padding_goes_by_the_label() {
    assert_eq!(default_padding_top(false, None), PADDING);
    assert_eq!(default_padding_top(false, Some(TextFieldLabelPosition::inside())), 8.0 + 10.0);
    assert_eq!(default_padding_top(false, Some(TextFieldLabelPosition::cutout())), PADDING);
    assert_eq!(
        default_padding_top(false, Some(TextFieldLabelPosition::above())),
        10.0 + 4.0 + PADDING
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, defaultContentPadding_outlinedTextFieldDefaults
#[test]
fn an_outlined_boxs_default_padding_goes_by_the_label() {
    assert_eq!(default_padding_top(true, None), PADDING);
    assert_eq!(default_padding_top(true, Some(TextFieldLabelPosition::inside())), 8.0 + 10.0);
    assert_eq!(default_padding_top(true, Some(TextFieldLabelPosition::cutout())), PADDING);
    assert_eq!(
        default_padding_top(true, Some(TextFieldLabelPosition::above())),
        10.0 + 4.0 + PADDING
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldDecoratorTest.kt, testTextFields_TextDecoration_noCrashConstraintsInfinity
#[test]
fn a_box_in_a_scroller_at_its_intrinsic_height_does_not_crash() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            let scroll = remember_scroll_state!();
            Column!(
                modifier = Modifier.height(IntrinsicSize::Min).horizontal_scroll(scroll),
                content = |_| {
                    let source = remember(MutableInteractionSource::new);
                    TextFieldDecorationBox!(
                        "Hats".to_string(),
                        || Text!("Cats"),
                        true,
                        true,
                        VisualTransformation::NONE,
                        source,
                        suffix = Some(|| Text!("Rats"))
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
}

#[composable]
fn Labelled(source: MutableInteractionSource, value: String) {
    BasicTextField!(
        value,
        |_: String| {},
        modifier = Modifier.test_tag(FIELD),
        interaction_source = Some(source)
    );
}

#[test]
fn a_basic_text_field_tells_its_source_of_the_focus() {
    let source = MutableInteractionSource::new();
    let shown_state = mutable_state_of(true);
    let shown = shown_state.handle();
    let content_source = source.clone();
    let focused_state = mutable_state_of(false);
    let focused = focused_state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let source = content_source.clone();
        let is_focused = source.collect_is_focused_as_state().get();
        SideEffect(move || focused.set(is_focused));
        if shown.get() {
            Labelled(source, "Text".to_string());
        }
    });
    ui.wait_for_idle();
    assert!(!source.is_focused());
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    assert!(source.is_focused());
    assert!(focused_state.get(), "as it's collected");
    // Gone, so its focus is.
    shown_state.set(false);
    ui.wait_for_idle();
    assert!(!source.is_focused());
}

#[test]
fn a_fields_interaction_source_paddings_and_shape_are_its_own() {
    let source = MutableInteractionSource::new();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let source = content_source.clone();
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                placeholder = Some(|| {
                    Spacer(Modifier.width(20.dp()).height(30.dp()).test_tag(INNER));
                }),
                content_padding =
                    Some(PaddingValues { start: 7.dp(), top: 3.dp(), end: 0.dp(), bottom: 5.dp() }),
                shape = Shape::Rectangle,
                interaction_source = Some(source.clone())
            );
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    assert!(source.is_focused());
    let field = ui.on_node_with_tag_unmerged(FIELD).fetch_semantics_snapshot().bounds;
    let inner = ui.on_node_with_tag_unmerged(INNER).fetch_semantics_snapshot().bounds;
    assert_eq!((inner.left - field.left, inner.top - field.top), (7.0, 3.0));
    // The rectangle, then the line's path: the shape's in both.
    let ops = &ui.host().display_list().ops;
    let scheme = light_color_scheme();
    assert!(
        ops.iter().any(|op| matches!(op, DrawOp::DrawRect(_, paint)
            if paint.color == scheme.surface_container_highest)),
        "the rectangle: {ops:?}"
    );
}

/// In a shape with rounded bottom corners, a field's line is cut by them, as `textFieldShape`.
#[test]
fn a_fields_line_is_within_its_shape() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                colors = TextFieldColors {
                    unfocused_indicator_color: Color::RED,
                    ..TextFieldDefaults::colors()
                },
                shape = TextFieldDefaults::rounded_shape()
            );
        });
    });
    ui.wait_for_idle();
    let (bounds, color) = last_path(&mut ui);
    assert_eq!((color, bounds.bottom - bounds.top), (Color::RED, 1.0));
    let ops = &ui.host().display_list().ops;
    assert!(ops.iter().any(|op| matches!(op, DrawOp::DrawPath { path, paint, .. }
        if paint.color == Color::RED && path.steps().iter().any(|step| matches!(step,
            compose_ui::PathStep::Combine(compose_ui::PathOperation::Intersect)
            | compose_ui::PathStep::CombineOr(compose_ui::PathOperation::Intersect))))));
}

/// Focused, an outlined container's border is thick and colored so, thin again when disabled.
#[test]
fn an_outlined_containers_border_follows_the_focus() {
    let source = MutableInteractionSource::new();
    let enabled_state = mutable_state_of(true);
    let enabled = enabled_state.handle();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), move || {
        let source = content_source.clone();
        let is_enabled = enabled.get();
        MaterialTheme!({
            OutlinedTextFieldContainer!(
                is_enabled,
                false,
                source.clone(),
                modifier = Modifier.width(300.dp()).height(100.dp()),
                colors = TextFieldColors {
                    focused_indicator_color: Color::RED,
                    unfocused_indicator_color: Color::BLUE,
                    disabled_indicator_color: Color::GREEN,
                    ..OutlinedTextFieldDefaults::colors()
                },
                shape = Shape::Rectangle,
                focused_border_thickness = 6.dp(),
                unfocused_border_thickness = 3.dp()
            );
        });
    });
    let drawn = |ui: &mut ComposeTestRule| format!("{:?}", ui.host().display_list().ops);
    ui.wait_for_idle();
    assert!(drawn(&mut ui).contains(&format!("{:?}", Color::BLUE)));
    source.try_emit(compose_ui::FocusInteraction::Focus(compose_ui::Focus::new()));
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let focused = drawn(&mut ui);
    assert!(focused.contains(&format!("{:?}", Color::RED)), "{focused}");
    assert!(focused.contains("Stroke(6.0)"), "{focused}");
    enabled_state.set(false);
    ui.wait_for_idle();
    let disabled = drawn(&mut ui);
    assert!(disabled.contains(&format!("{:?}", Color::GREEN)), "{disabled}");
    assert!(disabled.contains("Stroke(3.0)"), "{disabled}");
}

/// A new source starts unfocused, not animating to that, as `trackFocusState`.
#[test]
fn the_line_follows_a_new_source() {
    let (first, second) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let which_state = mutable_state_of(false);
    let which = which_state.handle();
    let sources = (first.clone(), second.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), move || {
        let source = if which.get() { sources.1.clone() } else { sources.0.clone() };
        MaterialTheme!({
            let colors = TextFieldColors {
                focused_indicator_color: Color::RED,
                unfocused_indicator_color: Color::BLUE,
                ..TextFieldDefaults::colors()
            };
            Box!(
                modifier = Modifier.width(300.dp()).height(100.dp()).indicator_line(
                    true,
                    false,
                    &source,
                    Some(colors),
                    Some(Shape::Rectangle),
                    4.dp(),
                    1.dp(),
                )
            );
        });
    });
    ui.wait_for_idle();
    which_state.set(true);
    ui.wait_for_idle();
    first.try_emit(compose_ui::FocusInteraction::Focus(compose_ui::Focus::new()));
    ui.advance_time_by(std::time::Duration::from_secs(2));
    assert_eq!(last_path(&mut ui).1, Color::BLUE, "the old one's gone");
    second.try_emit(compose_ui::FocusInteraction::Focus(compose_ui::Focus::new()));
    ui.advance_time_by(std::time::Duration::from_secs(2));
    assert_eq!(last_path(&mut ui).1, Color::RED);
}

#[test]
fn the_tonal_colors_and_the_rounded_shape_come_from_the_theme() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        MaterialTheme!({
            let scheme = MaterialTheme::color_scheme();
            let tonal = TextFieldDefaults::tonal_colors();
            assert_eq!(tonal.container_color, scheme.surface_container);
            assert_eq!(tonal.focused_container_color, scheme.surface_container);
            assert_eq!(tonal.error_container_color, scheme.error_container);
            assert_eq!(tonal.disabled_container_color, scheme.surface_container.with_alpha(0.38));
            assert_eq!(tonal.focused_indicator_color, Color::TRANSPARENT);
            assert_eq!(tonal.unfocused_indicator_color, Color::TRANSPARENT);
            assert_eq!(tonal.error_indicator_color, Color::TRANSPARENT);
            assert_eq!(tonal.focused_label_color, scheme.on_surface_variant);
            assert_eq!(tonal.supporting_text_color, scheme.on_background);
            let outlined = OutlinedTextFieldDefaults::tonal_colors();
            assert_eq!(outlined.container_color, Color::TRANSPARENT);
            assert_eq!(outlined.focused_indicator_color, scheme.outline_variant);
            assert_eq!(outlined.unfocused_indicator_color, scheme.outline_variant);
            assert_eq!(outlined.disabled_indicator_color, scheme.outline_variant);
            assert_eq!(outlined.error_indicator_color, scheme.error);
            assert_eq!(outlined.supporting_text_color, scheme.on_background);
            assert_eq!(TextFieldDefaults::rounded_shape(), MaterialTheme::shapes().medium.clone());
            assert_eq!(
                OutlinedTextFieldDefaults::rounded_shape(),
                MaterialTheme::shapes().medium.clone()
            );
            assert_eq!(
                OutlinedTextFieldDefaults::content_padding_with_label(),
                TextFieldDefaults::content_padding_with_label()
            );
            assert_eq!(
                OutlinedTextFieldDefaults::content_padding_without_label(),
                TextFieldDefaults::content_padding_without_label()
            );
        });
    });
    ui.wait_for_idle();
}

/// The last path drawn, its bounds and color.
fn last_path(ui: &mut ComposeTestRule) -> (compose_ui::Rect, Color) {
    ui.host()
        .display_list()
        .ops
        .iter()
        .rev()
        .find_map(|op| match op {
            DrawOp::DrawPath { path, paint, .. } => Some((path.bounds(), paint.color)),
            _ => None,
        })
        .expect("a path")
}

#[test]
fn the_indicator_line_follows_the_focus_animated() {
    let source = MutableInteractionSource::new();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), move || {
        let source = content_source.clone();
        MaterialTheme!({
            let colors = TextFieldColors {
                focused_indicator_color: Color::RED,
                unfocused_indicator_color: Color::BLUE,
                ..TextFieldDefaults::colors()
            };
            Box!(
                modifier = Modifier.width(300.dp()).height(100.dp()).indicator_line(
                    true,
                    false,
                    &source,
                    Some(colors),
                    Some(Shape::Rectangle),
                    4.dp(),
                    1.dp(),
                )
            );
        });
    });
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    let (bounds, color) = last_path(&mut ui);
    assert_eq!((bounds.top, bounds.bottom, color), (99.0, 100.0, Color::BLUE));
    let focus = compose_ui::Focus::new();
    source.try_emit(compose_ui::FocusInteraction::Focus(focus));
    ui.advance_time_by(std::time::Duration::from_millis(32));
    let (between, between_color) = last_path(&mut ui);
    assert!(between.top < 99.0 && between.top > 96.0, "{between:?}");
    assert!(between_color != Color::RED && between_color != Color::BLUE);
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let (bounds, color) = last_path(&mut ui);
    assert_eq!((bounds.top, bounds.bottom, color), (96.0, 100.0, Color::RED));
    source.try_emit(compose_ui::FocusInteraction::Unfocus(focus));
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let (bounds, color) = last_path(&mut ui);
    assert_eq!((bounds.top, bounds.bottom, color), (99.0, 100.0, Color::BLUE));
}

#[test]
fn a_disabled_indicator_line_goes_at_once_and_stays_thin() {
    let source = MutableInteractionSource::new();
    let enabled_state = mutable_state_of(true);
    let enabled = enabled_state.handle();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), move || {
        let source = content_source.clone();
        let is_enabled = enabled.get();
        MaterialTheme!({
            let colors = TextFieldColors {
                focused_indicator_color: Color::RED,
                unfocused_indicator_color: Color::BLUE,
                disabled_indicator_color: Color::GREEN,
                ..TextFieldDefaults::colors()
            };
            Box!(
                modifier = Modifier.width(300.dp()).height(100.dp()).indicator_line(
                    is_enabled,
                    false,
                    &source,
                    Some(colors),
                    Some(Shape::Rectangle),
                    4.dp(),
                    1.dp(),
                )
            );
        });
    });
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    source.try_emit(compose_ui::FocusInteraction::Focus(compose_ui::Focus::new()));
    ui.advance_time_by(std::time::Duration::from_secs(2));
    assert_eq!(last_path(&mut ui).1, Color::RED);
    enabled_state.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let (bounds, color) = last_path(&mut ui);
    assert_eq!((bounds.top, color), (99.0, Color::GREEN));
}

#[test]
fn the_indicator_line_is_clipped_to_the_shape() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), || {
        let source = remember(MutableInteractionSource::new);
        MaterialTheme!({
            Box!(
                modifier = Modifier.width(300.dp()).height(100.dp()).indicator_line(
                    true,
                    false,
                    &source,
                    None,
                    None,
                    2.dp(),
                    1.dp(),
                )
            );
        });
    });
    ui.wait_for_idle();
    let (_, color) = last_path(&mut ui);
    let path = ui
        .host()
        .display_list()
        .ops
        .iter()
        .rev()
        .find_map(|op| match op {
            DrawOp::DrawPath { path, .. } => Some(path.clone()),
            _ => None,
        })
        .expect("a path");
    assert!(
        path.steps().iter().any(|step| matches!(
            step,
            compose_ui::PathStep::Combine(compose_ui::PathOperation::Intersect)
                | compose_ui::PathStep::CombineOr(compose_ui::PathOperation::Intersect)
        )),
        "the line and the shape together"
    );
    assert_ne!(color, Color::TRANSPARENT);
}

#[test]
fn a_decoration_boxs_container_is_behind_as_big_as_the_field() {
    const CONTAINER: &str = "container";
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            let source = remember(MutableInteractionSource::new);
            Box!(
                modifier = Modifier.test_tag(FIELD),
                content = move |_| {
                    TextFieldDecorationBox!(
                        "Text".to_string(),
                        || Spacer(Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp())),
                        true,
                        false,
                        VisualTransformation::NONE,
                        source.clone(),
                        container = Some(|| {
                            Box!(modifier = Modifier.test_tag(CONTAINER));
                        })
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    let field = ui.on_node_with_tag_unmerged(FIELD).fetch_semantics_snapshot().bounds;
    let container = ui.on_node_with_tag_unmerged(CONTAINER).fetch_semantics_snapshot().bounds;
    assert_eq!(container, field);
    assert_eq!(field.height(), 16.0 + INNER_HEIGHT + 16.0);
    assert_eq!(field.width(), 16.0 + INNER_WIDTH + 16.0);
}

#[test]
fn a_cutout_box_cuts_its_own_container_at_the_label() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            let source = remember(MutableInteractionSource::new);
            OutlinedTextFieldDecorationBox!(
                "Text".to_string(),
                || Spacer(Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp())),
                true,
                false,
                VisualTransformation::NONE,
                source.clone(),
                label = Some(|| Spacer(Modifier.width(20.dp()).height(10.dp()))),
                container = Some(|| {
                    Box!(modifier = Modifier.size(100.dp()).background(Color::RED));
                })
            );
        });
    });
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let ops = &ui.host().display_list().ops;
    assert!(
        ops.iter().any(|op| matches!(op, DrawOp::ClipPath { .. })),
        "the label's room out of the container: {ops:?}"
    );
}

/// The room for the label a box's own container is cut by, left and right.
fn cut_of(ui: &mut ComposeTestRule) -> (f32, f32) {
    let cut = ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::ClipPath { path, .. } => path.steps().iter().rev().find_map(|step| match step {
            compose_ui::PathStep::Push(operand) => Some(operand.bounds()),
            _ => None,
        }),
        _ => None,
    });
    let cut = cut.expect("a cut");
    (cut.left, cut.right)
}

/// The cut's between the paddings given, 4 dp either side of the label.
#[test]
fn the_cut_is_between_the_paddings() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            let source = remember(MutableInteractionSource::new);
            OutlinedTextFieldDecorationBox!(
                "Text".to_string(),
                || Spacer(Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp())),
                true,
                false,
                VisualTransformation::NONE,
                source.clone(),
                label_position = TextFieldLabelPosition::Cutout {
                    is_always_minimized: false,
                    minimized_alignment: compose_ui::Alignment::CENTER_HORIZONTALLY,
                    expanded_alignment: compose_ui::Alignment::START,
                },
                label = Some(|| Spacer(Modifier.width(20.dp()).height(10.dp()))),
                content_padding = Some(PaddingValues {
                    start: 30.dp(),
                    top: 16.dp(),
                    end: 10.dp(),
                    bottom: 16.dp(),
                }),
                container = Some(|| {
                    Box!(modifier = Modifier.background(Color::RED));
                })
            );
        });
    });
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    // 140 wide: 100 between the paddings, the label in their middle.
    assert_eq!(cut_of(&mut ui), (30.0 + 40.0 - 4.0, 30.0 + 60.0 + 4.0));
}

/// A filled field with a label on its edge has its line too.
#[test]
fn a_cut_filled_field_has_its_line() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            TextField!(
                "Text".to_string(),
                |_: String| {},
                label = Some(|| Spacer(Modifier.width(20.dp()).height(10.dp()))),
                label_position = TextFieldLabelPosition::cutout(),
                colors = TextFieldColors {
                    unfocused_indicator_color: Color::RED,
                    ..TextFieldDefaults::colors()
                }
            );
        });
    });
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    assert_eq!(last_path(&mut ui).1, Color::RED);
}

/// A container of its own: nothing else drawn behind.
#[test]
fn a_boxs_own_container_replaces_the_default() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            let source = remember(MutableInteractionSource::new);
            TextFieldDecorationBox!(
                "Text".to_string(),
                || Spacer(Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp())),
                true,
                false,
                VisualTransformation::NONE,
                source.clone(),
                colors = TextFieldColors {
                    container_color: Color::RED,
                    unfocused_indicator_color: Color::GREEN,
                    ..TextFieldDefaults::colors()
                },
                container = Some(|| {
                    Box!(modifier = Modifier.fill_max_size());
                })
            );
        });
    });
    ui.wait_for_idle();
    let drawn = format!("{:?}", ui.host().display_list().ops);
    assert!(!drawn.contains(&format!("{:?}", Color::RED)), "{drawn}");
    assert!(!drawn.contains(&format!("{:?}", Color::GREEN)), "{drawn}");
}

/// Asked for its size, a box goes by the rest and not by its container.
#[test]
fn a_boxs_intrinsics_are_not_its_containers() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            let source = remember(MutableInteractionSource::new);
            Box!(
                modifier =
                    Modifier.width(IntrinsicSize::Max).height(IntrinsicSize::Max).test_tag(FIELD),
                content = move |_| {
                    TextFieldDecorationBox!(
                        "Text".to_string(),
                        || Spacer(Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp())),
                        true,
                        false,
                        VisualTransformation::NONE,
                        source.clone(),
                        container = Some(|| {
                            Box!(modifier = Modifier.width(500.dp()).height(500.dp()));
                        })
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    let field = ui.on_node_with_tag_unmerged(FIELD).fetch_semantics_snapshot().bounds;
    assert_eq!((field.width(), field.height()), (132.0, 82.0));
}

/// The height asked of a field with a label inside has the floated label's room.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, calculateHeight
#[test]
fn the_intrinsic_height_has_the_inside_labels_room() {
    let height = |label: bool| {
        let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
            MaterialTheme!({
                let source = remember(MutableInteractionSource::new);
                let slot = label.then_some(|| Spacer(Modifier.width(20.dp()).height(10.dp())));
                Box!(
                    modifier = Modifier.height(IntrinsicSize::Min).test_tag(FIELD),
                    content = move |_| {
                        TextFieldDecorationBox!(
                            "Text".to_string(),
                            || Spacer(Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp())),
                            true,
                            false,
                            VisualTransformation::NONE,
                            source.clone(),
                            label = slot,
                            content_padding = Some(PaddingValues::symmetric(0.dp(), 0.dp()))
                        );
                    }
                );
            });
        });
        ui.wait_for_idle();
        ui.on_node_with_tag_unmerged(FIELD).fetch_semantics_snapshot().bounds.height()
    };
    assert_eq!(height(false), INNER_HEIGHT);
    assert_eq!(height(true), INNER_HEIGHT + 16.0);
}

/// The room the text's given, in a field of a set height, is less the label's.
#[test]
fn the_text_is_measured_in_what_the_label_leaves() {
    let inner_height = |outlined: bool| {
        let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
            MaterialTheme!({
                let source = remember(MutableInteractionSource::new);
                Box!(
                    modifier = Modifier.height(100.dp()),
                    content = move |_| {
                        let inner = || {
                            Box!(
                                modifier =
                                    Modifier.width(20.dp()).fill_max_height().test_tag(INNER)
                            );
                        };
                        let label =
                            Some(|| Spacer(Modifier.width(20.dp()).height(LABEL_HEIGHT.dp())));
                        if outlined {
                            OutlinedTextFieldDecorationBox!(
                                "Text".to_string(),
                                inner,
                                true,
                                false,
                                VisualTransformation::NONE,
                                source.clone(),
                                label = label
                            );
                        } else {
                            TextFieldDecorationBox!(
                                "Text".to_string(),
                                inner,
                                true,
                                false,
                                VisualTransformation::NONE,
                                source.clone(),
                                label = label
                            );
                        }
                    }
                );
            });
        });
        ui.wait_for_idle();
        ui.advance_time_by(std::time::Duration::from_secs(2));
        ui.on_node_with_tag_unmerged(INNER).fetch_semantics_snapshot().bounds.height()
    };
    assert_eq!(inner_height(false), 100.0 - 8.0 - 8.0 - LABEL_HEIGHT);
    assert_eq!(inner_height(true), 100.0 - 16.0 - 16.0 - (LABEL_HEIGHT / 2.0 - 16.0));
}

/// New paddings are measured with.
#[test]
fn new_paddings_are_taken() {
    let top_state = mutable_state_of(10.0f32);
    let top_handle = top_state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        MaterialTheme!({
            let padding = top(top_handle.get());
            let source = remember(MutableInteractionSource::new);
            Box!(
                modifier = Modifier.test_tag(FIELD),
                content = move |_| {
                    TextFieldDecorationBox!(
                        "Text".to_string(),
                        || Spacer(Modifier.width(INNER_WIDTH.dp()).height(INNER_HEIGHT.dp())),
                        true,
                        false,
                        VisualTransformation::NONE,
                        source.clone(),
                        content_padding = Some(padding)
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    let height = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag_unmerged(FIELD).fetch_semantics_snapshot().bounds.height()
    };
    assert_eq!(height(&mut ui), 10.0 + INNER_HEIGHT + PADDING);
    top_state.set(30.0);
    ui.wait_for_idle();
    assert_eq!(height(&mut ui), 30.0 + INNER_HEIGHT + PADDING);
}

/// Two focuses at once, one ended: still focused, as `focusInteractions`.
#[test]
fn the_line_is_focused_until_every_focus_ended() {
    let source = MutableInteractionSource::new();
    let content_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), move || {
        let source = content_source.clone();
        MaterialTheme!({
            let colors = TextFieldColors {
                focused_indicator_color: Color::RED,
                unfocused_indicator_color: Color::BLUE,
                ..TextFieldDefaults::colors()
            };
            Box!(
                modifier = Modifier.width(300.dp()).height(100.dp()).indicator_line(
                    true,
                    false,
                    &source,
                    Some(colors),
                    Some(Shape::Rectangle),
                    4.dp(),
                    1.dp(),
                )
            );
        });
    });
    ui.wait_for_idle();
    let (first, second) = (compose_ui::Focus::new(), compose_ui::Focus::new());
    source.try_emit(compose_ui::FocusInteraction::Focus(first));
    source.try_emit(compose_ui::FocusInteraction::Focus(second));
    source.try_emit(compose_ui::FocusInteraction::Unfocus(first));
    ui.advance_time_by(std::time::Duration::from_secs(2));
    assert_eq!(last_path(&mut ui).1, Color::RED);
    source.try_emit(compose_ui::FocusInteraction::Unfocus(second));
    ui.advance_time_by(std::time::Duration::from_secs(2));
    assert_eq!(last_path(&mut ui).1, Color::BLUE);
}

/// A new source: the focus told to the old one ends there, as `focusable`'s `update`.
#[test]
fn a_basic_text_field_ends_its_focus_at_an_old_source() {
    let (first, second) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let which_state = mutable_state_of(false);
    let which = which_state.handle();
    let sources = (first.clone(), second.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let source = if which.get() { sources.1.clone() } else { sources.0.clone() };
        Labelled(source, "Text".to_string());
    });
    ui.wait_for_idle();
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    assert!(first.is_focused());
    which_state.set(true);
    ui.wait_for_idle();
    assert!(!first.is_focused());
}

/// The line's path is kept between draws, as `CacheDrawModifierNode`, for as long as it's the same:
/// a new size or shape makes a new one.
#[test]
fn the_lines_path_follows_the_size_and_the_shape() {
    let shape_state = mutable_state_of(Shape::Rectangle);
    let width_state = mutable_state_of(300.0f32);
    let rtl_state = mutable_state_of(false);
    let (shape, width, rtl) = (shape_state.handle(), width_state.handle(), rtl_state.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let (shape, width) = (shape.get(), width.get());
        let direction = if rtl.get() { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
        CompositionLocalProvider(compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction), || {
            let shape = shape.clone();
            MaterialTheme!({
                TextField!(
                    String::new(),
                    |_: String| {},
                    modifier = Modifier.width(width.dp()),
                    colors = TextFieldColors {
                        unfocused_indicator_color: Color::RED,
                        ..TextFieldDefaults::colors()
                    },
                    shape = shape.clone()
                );
            });
        });
    });
    ui.wait_for_idle();
    let line = |ui: &mut ComposeTestRule| {
        ui.host()
            .display_list()
            .ops
            .iter()
            .rev()
            .find_map(|op| match op {
                DrawOp::DrawPath { path, paint, .. } if paint.color == Color::RED => {
                    Some(path.clone())
                }
                _ => None,
            })
            .expect("the line")
    };
    assert_eq!(line(&mut ui).bounds().right, 300.0);
    width_state.set(400.0);
    ui.wait_for_idle();
    assert_eq!(line(&mut ui).bounds().right, 400.0);
    let rectangle = format!("{:?}", line(&mut ui).steps());
    let none = compose_ui::CornerSize::Dp(0.dp());
    shape_state.set(Shape::Rounded([none, none, compose_ui::CornerSize::Dp(8.dp()), none]));
    ui.wait_for_idle();
    let rounded = format!("{:?}", line(&mut ui).steps());
    assert_ne!(rounded, rectangle);
    rtl_state.set(true);
    ui.wait_for_idle();
    assert_ne!(format!("{:?}", line(&mut ui).steps()), rounded);
}
