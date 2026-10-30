//! Compose's `TextFieldLabelPositionTest.kt`, its `Attached` deprecated and not ported, and the
//! label-position tests of `TextFieldTest.kt` and `OutlinedTextFieldTest.kt`, adapted.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Alignment, IntSize, LayoutDirection, Modifier, Rect, UnitExt};

const FIELD: &str = "field";
const LABEL: &str = "label";
const WIDTH: f32 = 280.0;
const HEIGHT: f32 = 56.0;
const LABEL_SIZE: f32 = 16.0;

const END_CENTER_INSIDE: TextFieldLabelPosition = TextFieldLabelPosition::Inside {
    is_always_minimized: false,
    minimized_alignment: Alignment::END,
    expanded_alignment: Alignment::CENTER_HORIZONTALLY,
};
const END_CENTER_CUTOUT: TextFieldLabelPosition = TextFieldLabelPosition::Cutout {
    is_always_minimized: false,
    minimized_alignment: Alignment::END,
    expanded_alignment: Alignment::CENTER_HORIZONTALLY,
};

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldLabelPositionTest.kt, inside_equality, cutout_equality, above_equality, defaultValues
#[test]
fn the_positions_compare_by_their_values_and_have_defaults() {
    assert_eq!(END_CENTER_INSIDE, END_CENTER_INSIDE);
    assert_ne!(END_CENTER_INSIDE, TextFieldLabelPosition::inside());
    assert_ne!(
        TextFieldLabelPosition::Inside {
            is_always_minimized: true,
            minimized_alignment: Alignment::START,
            expanded_alignment: Alignment::START,
        },
        TextFieldLabelPosition::inside()
    );
    assert_ne!(END_CENTER_CUTOUT, TextFieldLabelPosition::cutout());
    assert_ne!(TextFieldLabelPosition::inside(), TextFieldLabelPosition::cutout());
    assert_ne!(
        TextFieldLabelPosition::Above { alignment: Alignment::END },
        TextFieldLabelPosition::above()
    );
    assert_eq!(
        TextFieldLabelPosition::inside(),
        TextFieldLabelPosition::Inside {
            is_always_minimized: false,
            minimized_alignment: Alignment::START,
            expanded_alignment: Alignment::START,
        }
    );
    assert_eq!(
        TextFieldLabelPosition::cutout(),
        TextFieldLabelPosition::Cutout {
            is_always_minimized: false,
            minimized_alignment: Alignment::START,
            expanded_alignment: Alignment::START,
        }
    );
    assert_eq!(
        TextFieldLabelPosition::above(),
        TextFieldLabelPosition::Above { alignment: Alignment::START }
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldLabelPositionTest.kt, inside_toString, cutout_toString, above_toString
#[test]
fn the_positions_say_what_they_are() {
    let always = |position| match position {
        TextFieldLabelPosition::Inside { minimized_alignment, expanded_alignment, .. } => {
            TextFieldLabelPosition::Inside {
                is_always_minimized: true,
                minimized_alignment,
                expanded_alignment,
            }
        }
        TextFieldLabelPosition::Cutout { minimized_alignment, expanded_alignment, .. } => {
            TextFieldLabelPosition::Cutout {
                is_always_minimized: true,
                minimized_alignment,
                expanded_alignment,
            }
        }
        other => other,
    };
    assert_eq!(
        always(END_CENTER_INSIDE).to_string(),
        "Inside(isAlwaysMinimized=true, minimizedAlignment=Horizontal(bias=1.0), \
         expandedAlignment=Horizontal(bias=0.0))"
    );
    assert_eq!(
        always(END_CENTER_CUTOUT).to_string(),
        "Cutout(isAlwaysMinimized=true, minimizedAlignment=Horizontal(bias=1.0), \
         expandedAlignment=Horizontal(bias=0.0))"
    );
    assert_eq!(
        TextFieldLabelPosition::Above { alignment: Alignment::END }.to_string(),
        "Above(alignment=Horizontal(bias=1.0))"
    );
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag_unmerged(tag).fetch_semantics_snapshot().bounds
}

/// A field of a label of 16 by 16, at the top start of a window.
#[composable]
fn Field(outlined: bool, position: TextFieldLabelPosition, #[default(false)] single_line: bool) {
    MaterialTheme!({
        let label = Some(|| {
            Box!(modifier = Modifier.size(LABEL_SIZE.dp()).test_tag(LABEL));
        });
        if outlined {
            OutlinedTextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                label = label,
                label_position = position,
                single_line = single_line
            );
        } else {
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                label = label,
                label_position = position,
                single_line = single_line
            );
        }
    });
}

fn rule(outlined: bool, position: TextFieldLabelPosition) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
        Field!(outlined, position);
    });
    ui.wait_for_idle();
    ui
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_labelPosition_whenPositionedAbove
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/OutlinedTextFieldTest.kt, testOutlinedTextField_labelPosition_whenPositionedAbove
#[test]
fn a_label_above_is_above_the_container() {
    for outlined in [false, true] {
        let mut ui = rule(outlined, TextFieldLabelPosition::above());
        let label = bounds(&mut ui, LABEL);
        assert_eq!((label.left, label.top), (4.0, 0.0), "outlined {outlined}");
        ui.on_node_with_tag(FIELD).assert_exists();
        let field = bounds(&mut ui, FIELD);
        // The whole of it, with the label.
        assert_eq!(field.height(), LABEL_SIZE + 4.0 + HEIGHT, "outlined {outlined}: {field:?}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_labelPosition_customAlignment, testTextField_labelPosition_customAlignment_cutout
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/OutlinedTextFieldTest.kt, testOutlinedTextField_labelPosition_customAlignment, testOutlinedTextField_labelPosition_customAlignment_inside
#[test]
fn a_label_is_aligned_as_it_goes() {
    for (outlined, position) in [
        (false, END_CENTER_INSIDE),
        (false, END_CENTER_CUTOUT),
        (true, END_CENTER_CUTOUT),
        (true, END_CENTER_INSIDE),
    ] {
        let mut ui = rule(outlined, position);
        let left = bounds(&mut ui, LABEL).left;
        assert!((left - (WIDTH - LABEL_SIZE) / 2.0).abs() <= 1.0, "{position}: {left}");
        ui.on_node_with_tag(FIELD).request_focus();
        ui.wait_for_idle();
        let left = bounds(&mut ui, LABEL).left;
        assert_eq!(left, WIDTH - 16.0 - LABEL_SIZE, "{position}");
    }
}

/// Right to left, an end's on the left.
#[test]
fn right_to_left_the_alignments_mirror() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), || {
        CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
            || {
                MaterialTheme!({
                    TextField!(
                        String::new(),
                        |_: String| {},
                        modifier = Modifier.test_tag(FIELD).width(WIDTH.dp()),
                        label = Some(|| {
                            Box!(modifier = Modifier.size(LABEL_SIZE.dp()).test_tag(LABEL));
                        }),
                        label_position = END_CENTER_INSIDE
                    );
                });
            },
        );
    });
    ui.wait_for_idle();
    let field = bounds(&mut ui, FIELD);
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, LABEL).left - field.left, 16.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_labelPosition_initial_singleLine_cutout
#[test]
fn a_cutout_label_in_a_filled_field_is_centered_below_the_room_above() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), || {
        Field!(false, TextFieldLabelPosition::cutout(), single_line = true);
    });
    ui.wait_for_idle();
    let label = bounds(&mut ui, LABEL);
    assert_eq!(label.left, 16.0);
    assert_eq!(label.top, (HEIGHT - LABEL_SIZE) / 2.0 + 8.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/OutlinedTextFieldTest.kt, testOutlinedTextField_labelPosition_initial_withMultiLineLabel_inside
#[test]
fn an_inside_label_in_an_outlined_field_has_no_room_above() {
    let mut ui = rule(true, TextFieldLabelPosition::inside());
    let field = bounds(&mut ui, FIELD);
    assert_eq!(field.top, 0.0);
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    // At the top padding, inside: not on the edge.
    assert_eq!(bounds(&mut ui, LABEL).top, 8.0);
}

/// Always minimized, it's there from the start.
#[test]
fn an_always_minimized_label_rests_minimized() {
    let position = TextFieldLabelPosition::Inside {
        is_always_minimized: true,
        minimized_alignment: Alignment::START,
        expanded_alignment: Alignment::START,
    };
    let mut ui = rule(false, position);
    assert_eq!(bounds(&mut ui, LABEL).top, 8.0);
    let position = TextFieldLabelPosition::Cutout {
        is_always_minimized: true,
        minimized_alignment: Alignment::START,
        expanded_alignment: Alignment::START,
    };
    let mut ui = rule(true, position);
    assert_eq!(bounds(&mut ui, LABEL).top, 8.0 - LABEL_SIZE / 2.0);
}

#[composable]
fn Affixed(position: TextFieldLabelPosition) {
    MaterialTheme!({
        TextField!(
            String::new(),
            |_: String| {},
            label = Some(|| Text!("Label")),
            label_position = position,
            prefix = Some(|| Text!("Prefix")),
            suffix = Some(|| Text!("Suffix")),
            placeholder = Some(|| Text!("Placeholder"))
        );
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_prefixAndSuffixAndPlaceholder_areNotDisplayed_withLabel_ifLabelCanExpand, testTextField_prefixAndSuffixAndPlaceholder_areDisplayed_withLabel_ifLabelCannotExpand, testTextField_prefixAndSuffixAndPlaceholder_areDisplayed_withLabel_ifLabelIsAbove
#[test]
fn the_prefix_suffix_and_placeholder_show_unless_the_label_can_rest_there() {
    let shown = |position: TextFieldLabelPosition| {
        let mut ui = compose_test(move || Affixed!(position));
        ui.wait_for_idle();
        ui.on_node_with_text("Label").assert_is_displayed();
        ["Prefix", "Suffix", "Placeholder"]
            .map(|text| !ui.on_all_nodes_with_text(text).fetch_semantics_nodes().is_empty())
    };
    assert_eq!(shown(TextFieldLabelPosition::inside()), [false; 3]);
    assert_eq!(
        shown(TextFieldLabelPosition::Inside {
            is_always_minimized: true,
            minimized_alignment: Alignment::START,
            expanded_alignment: Alignment::START,
        }),
        [true; 3]
    );
    assert_eq!(shown(TextFieldLabelPosition::above()), [true; 3]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_labelScope_progressAndRecomposition
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/OutlinedTextFieldTest.kt, testOutlinedTextField_labelScope_progressAndRecomposition
#[test]
fn the_labels_scope_tells_its_progress_without_composing_it() {
    for outlined in [false, true] {
        let progress = Rc::new(Cell::new(-1.0));
        let count = Rc::new(Cell::new(0));
        let (seen, counted) = (progress.clone(), count.clone());
        let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), move || {
            let (seen, counted) = (seen.clone(), counted.clone());
            MaterialTheme!({
                let (seen, counted) = (seen.clone(), counted.clone());
                let label = Some(move || {
                    let counted = counted.clone();
                    SideEffect(move || counted.set(counted.get() + 1));
                    let seen = seen.clone();
                    let scope = LOCAL_TEXT_FIELD_LABEL_SCOPE.current();
                    Box!(
                        modifier = Modifier.size(10.dp()).graphics_layer(move |_| {
                            seen.set(scope.label_minimized_progress());
                        })
                    );
                });
                if outlined {
                    OutlinedTextField!(
                        String::new(),
                        |_: String| {},
                        modifier = Modifier.test_tag(FIELD),
                        label = label
                    );
                } else {
                    TextField!(
                        String::new(),
                        |_: String| {},
                        modifier = Modifier.test_tag(FIELD),
                        label = label
                    );
                }
            });
        });
        ui.wait_for_idle();
        assert_eq!(progress.get(), 0.0);
        assert_eq!(count.get(), 1);
        ui.on_node_with_tag(FIELD).request_focus();
        ui.wait_for_idle();
        assert_eq!(progress.get(), 1.0);
        assert_eq!(count.get(), 1, "outlined {outlined}");
    }
}

/// The cut follows the minimized alignment; none for an inside label, and none's drawn at rest.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/OutlinedTextField.kt, outlineCutout
#[test]
fn the_cut_is_at_the_minimized_label() {
    use compose_ui::platform::DrawOp;
    let first_x = |ui: &mut ComposeTestRule| {
        let ops = ui.host().display_list().ops.clone();
        ops.iter().find_map(|op| match op {
            DrawOp::DrawPath { path, .. } => {
                path.commands().iter().find_map(|command| match command {
                    compose_ui::PathCommand::MoveTo(point) => Some(point.x),
                    _ => None,
                })
            }
            _ => None,
        })
    };
    // The outline starts at the cut's end, the label's end and 4 dp.
    let mut ui = rule(true, END_CENTER_CUTOUT);
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    assert_eq!(first_x(&mut ui), Some(WIDTH - 16.0 + 4.0));
    let mut ui = rule(true, TextFieldLabelPosition::cutout());
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    assert_eq!(first_x(&mut ui), Some(16.0 + LABEL_SIZE + 4.0));
    // None: from the corner.
    let mut ui = rule(true, TextFieldLabelPosition::inside());
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    let x = first_x(&mut ui).unwrap();
    assert!(x < 16.0, "{x}");
}

/// A filled field's container is cut as an outline is.
#[test]
fn a_filled_fields_container_is_cut_for_a_label_on_its_edge() {
    use compose_ui::platform::DrawOp;
    let mut ui = rule(false, TextFieldLabelPosition::cutout());
    ui.on_node_with_tag(FIELD).request_focus();
    ui.wait_for_idle();
    let ops = ui.host().display_list().ops.clone();
    let cut = ops.iter().any(|op| {
        matches!(op, DrawOp::DrawPath { path, .. }
            if path.steps().iter().any(|step| matches!(step,
                compose_ui::PathStep::Combine(compose_ui::PathOperation::Difference)
                | compose_ui::PathStep::CombineOr(compose_ui::PathOperation::Difference))))
    });
    assert!(cut, "{ops:?}");
    // And not at rest, nothing to cut for.
    let mut ui = rule(false, TextFieldLabelPosition::cutout());
    let ops = ui.host().display_list().ops.clone();
    assert!(!ops.iter().any(|op| matches!(op, DrawOp::DrawPath { path, .. }
            if path.steps().iter().any(|step| matches!(step,
                compose_ui::PathStep::Combine(compose_ui::PathOperation::Difference)
                | compose_ui::PathStep::CombineOr(compose_ui::PathOperation::Difference))))));
}

/// A label above with a supporting text: above, the field, below.
#[test]
fn a_label_above_and_a_supporting_text_go_around_the_field() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), || {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_: String| {},
                modifier = Modifier.test_tag(FIELD),
                label = Some(|| {
                    Box!(modifier = Modifier.size(LABEL_SIZE.dp()).test_tag(LABEL));
                }),
                label_position = TextFieldLabelPosition::Above { alignment: Alignment::END },
                supporting_text = Some(|| {
                    Box!(modifier = Modifier.size(10.dp()).test_tag("supporting"));
                })
            );
        });
    });
    ui.wait_for_idle();
    let (label, field, supporting) =
        (bounds(&mut ui, LABEL), bounds(&mut ui, FIELD), bounds(&mut ui, "supporting"));
    assert_eq!(supporting.top, LABEL_SIZE + 4.0 + HEIGHT + 4.0);
    assert_eq!(field.bottom, supporting.bottom);
    assert_eq!(label.right, field.right - 4.0, "at the end");
}

/// A field with a label above asked for its height has it whole, in a row of its own height.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, calculateHeight
#[test]
fn the_intrinsic_height_has_the_label_above() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 800), || {
        MaterialTheme!({
            Row!(modifier = Modifier.test_tag("row").height(compose_ui::IntrinsicSize::Min), {
                Field!(false, TextFieldLabelPosition::above());
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "row").height(), LABEL_SIZE + 4.0 + HEIGHT);
}
