//! Material text fields' icons, supporting text, error state and keyboard parameters.

#![allow(non_snake_case)]

use compose_material3::*;
use compose_test::*;
use compose_ui::Modifier;

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
}

#[test]
fn icons_sit_in_48_dp_boxes_at_the_ends_and_the_text_between() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                placeholder = Some(|| Text!("hint", modifier = Modifier.test_tag("hint"))),
                leading_icon = Some(|| Icon!(
                    icons::filled::search(),
                    None,
                    modifier = Modifier.test_tag("leading")
                )),
                trailing_icon = Some(|| Icon!(
                    icons::filled::close(),
                    None,
                    modifier = Modifier.test_tag("trailing")
                )),
                single_line = true,
            );
        });
    });
    let field = bounds(&mut ui, "field");
    let leading = bounds(&mut ui, "leading");
    let trailing = bounds(&mut ui, "trailing");
    // 24 dp icons centered in 48 dp boxes at each end, vertically centered.
    assert_eq!((leading.left - field.left, leading.top - field.top), (12.0, 16.0));
    assert_eq!(field.right - trailing.right, 12.0);
    // The text (and its placeholder) 4 dp after the leading box.
    let hint = bounds(&mut ui, "hint");
    assert_eq!(hint.left - field.left, 52.0);
}

#[test]
fn supporting_text_and_errors_show_below_in_the_error_color() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            OutlinedTextField!(
                "bad".to_string(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                supporting_text =
                    Some(|| Text!("Required", modifier = Modifier.test_tag("support"))),
                is_error = true,
            );
        });
    });
    let field = bounds(&mut ui, "field");
    let support = bounds(&mut ui, "support");
    // Below the 56 dp container, 4 dp under it, 16 dp in.
    assert_eq!((support.left - field.left, support.top - field.top), (16.0, 60.0));
    let node = ui.on_node_with_tag("field").fetch_semantics_snapshot();
    assert_eq!(
        node.config.get(&compose_ui::SemanticsProperties::ERROR).cloned().as_deref(),
        Some("Invalid input")
    );
    // The outline and the supporting text in the error color.
    let error = light_color_scheme().error;
    let list = ui.dump_display_list();
    assert!(list.contains(&format!("{:?}", error)), "{list}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_prefixAndSuffixPosition_whenNoLabel
#[test]
fn a_prefix_and_a_suffix_go_around_the_text() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(
                "12".to_string(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                prefix = Some(|| Text!("$", modifier = Modifier.test_tag("prefix"))),
                suffix = Some(|| Text!("kg", modifier = Modifier.test_tag("suffix"))),
                single_line = true,
            );
        });
    });
    let field = bounds(&mut ui, "field");
    let prefix = bounds(&mut ui, "prefix");
    let suffix = bounds(&mut ui, "suffix");
    // 16 dp in at the start and the end, on the text's line.
    assert_eq!(prefix.left - field.left, 16.0);
    assert_eq!(field.right - suffix.right, 16.0);
    assert_eq!(prefix.top, suffix.top);
    assert_eq!((prefix.top + prefix.bottom) / 2.0, (field.top + field.bottom) / 2.0);
}

/// Resting empty with a label, all three are out of the tree, not merely transparent: they'd
/// interfere with a screen reader, as Compose's "Transparent components interfere with Talkback".
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_prefixAndSuffixAndPlaceholder_areNotDisplayed_withLabel_ifLabelCanExpand
#[test]
fn a_hidden_prefix_suffix_and_placeholder_are_not_there() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_| {},
                label = Some(|| Text!("Label")),
                prefix = Some(|| Text!("Prefix")),
                suffix = Some(|| Text!("Suffix")),
                placeholder = Some(|| Text!("Placeholder")),
            );
        });
    });
    ui.on_node_with_text("Label").assert_is_displayed();
    ui.on_node_with_text("Prefix").assert_does_not_exist();
    ui.on_node_with_text("Suffix").assert_does_not_exist();
    ui.on_node_with_text("Placeholder").assert_does_not_exist();
    // Nor drawn, measured or composed: an outlined one too.
    let mut ui = compose_test(|| {
        MaterialTheme!({
            OutlinedTextField!(
                String::new(),
                |_| {},
                label = Some(|| Text!("Label")),
                prefix = Some(|| Text!("Prefix")),
                suffix = Some(|| Text!("Suffix")),
                placeholder = Some(|| Text!("Placeholder")),
            );
        });
    });
    ui.on_node_with_text("Prefix").assert_does_not_exist();
    ui.on_node_with_text("Suffix").assert_does_not_exist();
    ui.on_node_with_text("Placeholder").assert_does_not_exist();
}

/// With no label they're shown from the start, and a filled text shows the prefix and the suffix.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_prefixAndSuffixAndPlaceholder_areDisplayed_withLabel_ifLabelCannotExpand
#[test]
fn a_prefix_suffix_and_placeholder_show_without_a_label() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            TextField!(
                String::new(),
                |_| {},
                prefix = Some(|| Text!("Prefix")),
                suffix = Some(|| Text!("Suffix")),
                placeholder = Some(|| Text!("Placeholder")),
            );
        });
    });
    ui.on_node_with_text("Prefix").assert_is_displayed();
    ui.on_node_with_text("Suffix").assert_is_displayed();
    ui.on_node_with_text("Placeholder").assert_is_displayed();
}

/// Empty as the text shows, as `visualText`: a transformation showing something for no text
/// leaves no room for the placeholder.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, CommonDecorationBox
#[test]
fn a_placeholder_goes_by_the_text_as_it_shows() {
    use compose_foundation::{
        OffsetMapping, TransformedText, VisualTransform, VisualTransformation,
    };
    struct Same;
    impl OffsetMapping for Same {
        fn original_to_transformed(&self, _offset: usize) -> usize {
            0
        }
        fn transformed_to_original(&self, _offset: usize) -> usize {
            0
        }
    }
    struct Shows;
    impl VisualTransform for Shows {
        fn filter(&self, _text: &str) -> TransformedText {
            TransformedText { text: "shows".to_string(), offset_mapping: std::rc::Rc::new(Same) }
        }
    }
    let transformation = VisualTransformation::new(Shows);
    let mut ui = compose_test(move || {
        let transformation = transformation.clone();
        MaterialTheme!({
            TextField!(
                String::new(),
                |_| {},
                placeholder = Some(|| Text!("Placeholder")),
                visual_transformation = transformation.clone(),
            );
        });
    });
    ui.on_node_with_text("Placeholder").assert_does_not_exist();
}

/// A style with a color of its own on one of its two ends: the label takes it, focused the small
/// one's, else the large one's, a missing one the label's color.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/internal/TextFieldImpl.kt, overrideLabelTextStyleColor
#[test]
fn a_label_takes_the_color_of_the_typography_with_one() {
    use compose_ui::Color;
    use std::cell::Cell;
    thread_local! {
        static SEEN: Cell<Color> = const { Cell::new(Color::BLACK) };
    }
    let field = || {
        TextField!(
            String::new(),
            |_| {},
            modifier = Modifier.test_tag("field"),
            label = Some(|| SEEN.set(LOCAL_TEXT_STYLE.current().color)),
        );
    };
    let mut ui = compose_test(move || {
        let mut typography = (*MaterialTheme::typography()).clone();
        typography.body_large.color = Color::RED;
        MaterialTheme!(typography = typography, {
            field();
        });
    });
    assert_eq!(SEEN.get(), Color::RED);
    // Focused, the small one's, which has none: the label's color.
    ui.on_node_with_tag("field").request_focus();
    ui.wait_for_idle();
    assert_eq!(SEEN.get(), light_color_scheme().primary);
    // Without one on either, it has none: the label's content color applies.
    let _ui = compose_test(move || {
        MaterialTheme!({
            field();
        });
    });
    assert_eq!(SEEN.get(), Color::UNSPECIFIED);
}

/// A focused and an errored field have containers of their own, as `containerColor`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/TextFieldDefaults.kt, containerColor
#[test]
fn a_fields_container_follows_focus_and_error() {
    use compose_ui::Color;
    let field = |is_error: bool| {
        let colors = TextFieldColors {
            container_color: Color::RED,
            focused_container_color: Color::GREEN,
            error_container_color: Color::BLUE,
            ..TextFieldDefaults::colors()
        };
        TextField!(
            String::new(),
            |_| {},
            modifier = Modifier.test_tag("field"),
            is_error = is_error,
            colors = colors,
        );
    };
    let mut ui = compose_test(move || {
        MaterialTheme!({
            field(false);
        });
    });
    assert!(ui.dump_display_list().contains("#FFFF0000"), "{}", ui.dump_display_list());
    ui.on_node_with_tag("field").perform_click();
    ui.wait_for_idle();
    assert!(ui.dump_display_list().contains("#FF00FF00"), "{}", ui.dump_display_list());
    let ui = compose_test(move || {
        MaterialTheme!({
            field(true);
        });
    });
    assert!(ui.dump_display_list().contains("#FF0000FF"), "{}", ui.dump_display_list());
}

#[test]
fn a_multi_line_field_is_between_min_lines_and_max_lines_high() {
    let value = compose_runtime::mutable_state_of(String::new());
    let handle = value.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            OutlinedTextField!(
                handle.get(),
                |_| {},
                modifier = Modifier.test_tag("field"),
                max_lines = 4,
                min_lines = 3,
            );
        });
    });
    // 16 dp above and below the body-large lines (24 sp each).
    let height = |ui: &mut ComposeTestRule| {
        let field = bounds(ui, "field");
        field.bottom - field.top
    };
    assert_eq!(height(&mut ui), 32.0 + 3.0 * 24.0);
    value.set(String::from("1\n2\n3\n4\n5\n6"));
    ui.wait_for_idle();
    assert_eq!(height(&mut ui), 32.0 + 4.0 * 24.0);
}

/// A field with `ab`, an icon button at each end, and `requester` on it.
#[compose_runtime::composable]
fn FieldWithIconButtons(requester: compose_ui::FocusRequester) {
    MaterialTheme!({
        TextField!(
            String::from("ab"),
            |_| {},
            modifier = Modifier.test_tag("field").focus_requester(requester),
            leading_icon =
                Some(|| IconButton!(on_click = || {}, modifier = Modifier.test_tag("leading"), {
                    Icon!(icons::filled::search(), None);
                })),
            trailing_icon =
                Some(|| IconButton!(on_click = || {}, modifier = Modifier.test_tag("trailing"), {
                    Icon!(icons::filled::close(), None);
                })),
            single_line = true,
        );
    });
}

#[test]
fn a_requester_on_a_field_with_icon_buttons_focuses_the_field() {
    let requester = compose_ui::FocusRequester::new();
    let mut ui = compose_test(move || FieldWithIconButtons(requester));
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert!(ui.host().text_input_options().is_some(), "the field, not one of its icons");
    ui.on_node_with_tag("leading").assert_is_not_focused();
    ui.on_node_with_tag("trailing").assert_is_not_focused();
}

#[test]
fn a_tap_on_a_fields_padding_focuses_the_field_and_not_its_icon_button() {
    let mut ui = compose_test(|| FieldWithIconButtons(compose_ui::FocusRequester::new()));
    let field = bounds(&mut ui, "field");
    // Above the text, between the icons.
    let at = compose_ui::Offset::new(field.left + field.width() / 2.0, field.top + 2.0);
    ui.host().send_pointer(compose_ui::PointerEvent::press(at));
    ui.host().send_pointer(compose_ui::PointerEvent::release(at));
    ui.wait_for_idle();
    assert!(ui.host().text_input_options().is_some(), "the field, not one of its icons");
    ui.on_node_with_tag("leading").assert_is_not_focused();
    ui.on_node_with_tag("trailing").assert_is_not_focused();
}

#[test]
fn typing_into_a_field_with_an_icon_button_reaches_its_text() {
    let value = compose_runtime::mutable_state_of(String::new());
    let handle = value.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            TextField!(
                handle.get(),
                move |new| handle.set(new),
                modifier = Modifier.test_tag("field"),
                trailing_icon = Some(|| IconButton!(on_click = || {}, {
                    Icon!(icons::filled::close(), None);
                })),
            );
        });
    });
    ui.on_node_with_tag("field").perform_text_input("typed");
    assert_eq!(value.get(), "typed");
}

#[test]
fn tab_goes_from_the_field_to_its_icon_buttons() {
    let requester = compose_ui::FocusRequester::new();
    let mut ui = compose_test(move || FieldWithIconButtons(requester));
    assert!(requester.request_focus());
    ui.wait_for_idle();
    ui.host().send_key(compose_ui::KeyEvent::down(compose_ui::Key::Tab));
    ui.wait_for_idle();
    ui.on_node_with_tag("leading").assert_is_focused();
    ui.host().send_key(compose_ui::KeyEvent::down(compose_ui::Key::Tab));
    ui.wait_for_idle();
    ui.on_node_with_tag("trailing").assert_is_focused();
}
