//! Compose's `TextSelectionColorsScreenshotTest.kt` (foundation), adapted: the selection's
//! highlight is drawn in `LOCAL_TEXT_SELECTION_COLORS`' background color, read from the display
//! list instead of a golden. compose-rs draws no selection handles yet, so their color isn't
//! checked.

use std::time::Duration;

use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerEvent};

const TEXT: &str = "Selected text";

fn custom_colors() -> TextSelectionColors {
    TextSelectionColors {
        handle_color: Color::from_argb(0xFFFFB7B2),
        background_color: Color::from_argb(0xFFB5EAD7).with_alpha(0.4),
    }
}

/// The colors of the rectangles the display list fills.
fn rect_fills(ui: &mut ComposeTestRule) -> Vec<Color> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            compose_ui::platform::DrawOp::DrawRect(_, paint) => Some(paint.color),
            _ => None,
        })
        .collect()
}

/// Double-clicks the start of the node tagged "text", selecting its first word.
fn select_first_word(ui: &mut ComposeTestRule) {
    let bounds = ui.on_node_with_tag("text").fetch_semantics_node().bounds;
    let point = Offset::new(bounds.left + 2.0, bounds.top + 4.0);
    for _ in 0..2 {
        ui.host().send_pointer(PointerEvent::press(point));
        ui.host().send_pointer(PointerEvent::release(point));
        ui.advance_time_by(Duration::from_millis(50));
    }
    ui.wait_for_idle();
}

/// Clicks the field tagged "field" to focus it, then selects all of it with the shortcut.
fn focus_and_select_all(ui: &mut ComposeTestRule) {
    ui.on_node_with_tag("field").perform_click();
    let modifiers = if cfg!(target_os = "macos") {
        KeyModifiers { meta: true, ..Default::default() }
    } else {
        KeyModifiers { ctrl: true, ..Default::default() }
    };
    let key = Key::Character('a');
    ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyDown, modifiers });
    ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyUp, modifiers });
    ui.wait_for_idle();
}

fn text_content(colors: Option<TextSelectionColors>) -> impl Fn() {
    move || {
        let content = || {
            SelectionContainer!({
                BasicText!(TEXT, modifier = Modifier.test_tag("text"));
            });
        };
        match colors {
            Some(colors) => {
                CompositionLocalProvider(LOCAL_TEXT_SELECTION_COLORS.provides(colors), content)
            }
            None => content(),
        }
    }
}

fn field_content(colors: Option<TextSelectionColors>) -> impl Fn() {
    move || {
        let content = || {
            BasicTextField!(TEXT.to_string(), |_| {}, modifier = Modifier.test_tag("field"));
        };
        match colors {
            Some(colors) => {
                CompositionLocalProvider(LOCAL_TEXT_SELECTION_COLORS.provides(colors), content)
            }
            None => content(),
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/TextSelectionColorsScreenshotTest.kt, text_defaultSelectionColors
#[test]
fn a_selected_text_is_highlighted_in_the_default_selection_color() {
    let mut ui = compose_test(text_content(None));
    select_first_word(&mut ui);
    assert_eq!(rect_fills(&mut ui), vec![Color::from_argb(0x664286F4)]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/TextSelectionColorsScreenshotTest.kt, text_customSelectionColors
#[test]
fn a_selected_text_is_highlighted_in_the_provided_selection_color() {
    let mut ui = compose_test(text_content(Some(custom_colors())));
    select_first_word(&mut ui);
    assert_eq!(rect_fills(&mut ui), vec![custom_colors().background_color]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/TextSelectionColorsScreenshotTest.kt, textField_defaultSelectionColors
#[test]
fn a_selection_in_a_field_is_highlighted_in_the_default_selection_color() {
    let mut ui = compose_test(field_content(None));
    focus_and_select_all(&mut ui);
    assert!(rect_fills(&mut ui).contains(&Color::from_argb(0x664286F4)));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/selection/TextSelectionColorsScreenshotTest.kt, textField_customSelectionColors
#[test]
fn a_selection_in_a_field_is_highlighted_in_the_provided_selection_color() {
    let mut ui = compose_test(field_content(Some(custom_colors())));
    focus_and_select_all(&mut ui);
    let fills = rect_fills(&mut ui);
    assert!(fills.contains(&custom_colors().background_color), "{fills:?}");
    assert!(!fills.contains(&Color::from_argb(0x664286F4)));
}
