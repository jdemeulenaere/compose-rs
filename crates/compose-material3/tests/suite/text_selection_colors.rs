//! Compose's `TextSelectionColorsScreenshotTest.kt` (material3) and the text fields'
//! selection-color tests, adapted: `MaterialTheme` provides its primary color's selection colors,
//! and the text fields provide their colors' own. The screenshots read the display list's
//! highlight instead (compose-rs draws no selection handles yet).

use std::cell::Cell;
use std::time::Duration;

use compose_foundation::{LOCAL_TEXT_SELECTION_COLORS, SelectionContainer, TextSelectionColors};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, Key, KeyEvent, KeyEventType, KeyModifiers, Modifier, Offset, PointerEvent,
};

const TEXT: &str = "Selected text";

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

fn highlight(scheme: &ColorScheme) -> Color {
    scheme.primary.with_alpha(0.4)
}

fn select_first_word(ui: &mut ComposeTestRule) {
    let bounds = ui.on_node_with_tag("text").fetch_semantics_snapshot().bounds;
    let point = Offset::new(bounds.left + 2.0, bounds.top + 4.0);
    for _ in 0..2 {
        ui.host().send_pointer(PointerEvent::press(point));
        ui.host().send_pointer(PointerEvent::release(point));
        ui.advance_time_by(Duration::from_millis(50));
    }
    ui.wait_for_idle();
}

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

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextSelectionColorsScreenshotTest.kt, rememberTextSelectionColors
#[test]
fn the_theme_provides_its_primary_colors_selection_colors() {
    thread_local! {
        static COLORS: Cell<Option<(TextSelectionColors, TextSelectionColors)>> =
            const { Cell::new(None) };
    }
    compose_test(|| {
        MaterialTheme!(color_scheme = light_color_scheme(), {
            let light = LOCAL_TEXT_SELECTION_COLORS.current();
            MaterialTheme!(color_scheme = dark_color_scheme(), {
                let dark = LOCAL_TEXT_SELECTION_COLORS.current();
                COLORS.with(|cell| cell.set(Some((light, dark))));
            });
        });
    });
    let (light, dark) = COLORS.with(Cell::get).expect("composed");
    for (colors, scheme) in [(light, light_color_scheme()), (dark, dark_color_scheme())] {
        assert_eq!(colors.handle_color, scheme.primary);
        assert_eq!(colors.background_color, highlight(&scheme));
    }
}

fn selected_text_fill(scheme: ColorScheme) -> Vec<Color> {
    let mut ui = compose_test(move || {
        MaterialTheme!(color_scheme = scheme, {
            Surface!({
                SelectionContainer!({
                    Text!(TEXT, modifier = Modifier.test_tag("text"));
                });
            });
        });
    });
    select_first_word(&mut ui);
    rect_fills(&mut ui)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextSelectionColorsScreenshotTest.kt, text_lightThemeSelectionColors
#[test]
fn a_selected_text_is_highlighted_in_the_light_themes_primary_color() {
    let scheme = light_color_scheme();
    assert_eq!(selected_text_fill(scheme), vec![scheme.surface, highlight(&scheme)]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextSelectionColorsScreenshotTest.kt, text_darkThemeSelectionColors
#[test]
fn a_selected_text_is_highlighted_in_the_dark_themes_primary_color() {
    let scheme = dark_color_scheme();
    assert_eq!(selected_text_fill(scheme), vec![scheme.surface, highlight(&scheme)]);
}

fn selected_field_fills(scheme: ColorScheme, outlined: bool) -> Vec<Color> {
    let mut ui = compose_test(move || {
        MaterialTheme!(color_scheme = scheme, {
            Surface!({
                if outlined {
                    OutlinedTextField!(
                        TEXT.to_string(),
                        |_| {},
                        modifier = Modifier.test_tag("field")
                    );
                } else {
                    TextField!(TEXT.to_string(), |_| {}, modifier = Modifier.test_tag("field"));
                }
            });
        });
    });
    focus_and_select_all(&mut ui);
    rect_fills(&mut ui)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextSelectionColorsScreenshotTest.kt, filledTextField_lightThemeSelectionColors
#[test]
fn a_selection_in_a_filled_field_is_highlighted_in_the_light_themes_primary_color() {
    let scheme = light_color_scheme();
    assert!(selected_field_fills(scheme, false).contains(&highlight(&scheme)));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextSelectionColorsScreenshotTest.kt, filledTextField_darkThemeSelectionColors
#[test]
fn a_selection_in_a_filled_field_is_highlighted_in_the_dark_themes_primary_color() {
    let scheme = dark_color_scheme();
    assert!(selected_field_fills(scheme, false).contains(&highlight(&scheme)));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextSelectionColorsScreenshotTest.kt, outlinedTextField_lightThemeSelectionColors
#[test]
fn a_selection_in_an_outlined_field_is_highlighted_in_the_light_themes_primary_color() {
    let scheme = light_color_scheme();
    assert!(selected_field_fills(scheme, true).contains(&highlight(&scheme)));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextSelectionColorsScreenshotTest.kt, outlinedTextField_darkThemeSelectionColors
#[test]
fn a_selection_in_an_outlined_field_is_highlighted_in_the_dark_themes_primary_color() {
    let scheme = dark_color_scheme();
    assert!(selected_field_fills(scheme, true).contains(&highlight(&scheme)));
}

/// What a field's label reads as the selection colors: the field provides its colors' own
/// (`custom`, else the defaults') around its decoration.
fn label_selection_colors(
    custom: Option<TextSelectionColors>,
    outlined: bool,
) -> TextSelectionColors {
    thread_local! { static SEEN: Cell<Option<TextSelectionColors>> = const { Cell::new(None) }; }
    compose_test(move || {
        MaterialTheme!({
            let defaults = if outlined {
                OutlinedTextFieldDefaults::colors()
            } else {
                TextFieldDefaults::colors()
            };
            let colors = TextFieldColors {
                text_selection_colors: custom.unwrap_or(defaults.text_selection_colors),
                ..defaults
            };
            let label = || SEEN.with(|cell| cell.set(Some(LOCAL_TEXT_SELECTION_COLORS.current())));
            if outlined {
                OutlinedTextField!(String::new(), |_| {}, label = Some(label), colors = colors);
            } else {
                TextField!(String::new(), |_| {}, label = Some(label), colors = colors);
            }
        });
    });
    SEEN.with(Cell::get).expect("the label composed")
}

fn theme_selection_colors() -> TextSelectionColors {
    let scheme = light_color_scheme();
    TextSelectionColors { handle_color: scheme.primary, background_color: highlight(&scheme) }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_selectionColors_areCustomizable
#[test]
fn a_filled_fields_selection_colors_are_the_themes_or_its_colors() {
    assert_eq!(label_selection_colors(None, false), theme_selection_colors());
    let custom = TextSelectionColors { handle_color: Color::RED, background_color: Color::GREEN };
    assert_eq!(label_selection_colors(Some(custom), false), custom);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/OutlinedTextFieldTest.kt, testOutlinedTextField_selectionColors_areCustomizable
#[test]
fn an_outlined_fields_selection_colors_are_the_themes_or_its_colors() {
    assert_eq!(label_selection_colors(None, true), theme_selection_colors());
    let custom = TextSelectionColors { handle_color: Color::RED, background_color: Color::GREEN };
    assert_eq!(label_selection_colors(Some(custom), true), custom);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_selectionColors_areCustomizable
// (the third field: selection colors provided around it)
#[test]
fn a_field_takes_the_selection_colors_provided_around_it() {
    thread_local! { static SEEN: Cell<Option<TextSelectionColors>> = const { Cell::new(None) }; }
    let provided =
        TextSelectionColors { handle_color: Color::MAGENTA, background_color: Color::YELLOW };
    compose_test(move || {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_TEXT_SELECTION_COLORS.provides(provided), || {
                let label =
                    || SEEN.with(|cell| cell.set(Some(LOCAL_TEXT_SELECTION_COLORS.current())));
                TextField!(String::new(), |_| {}, label = Some(label));
            });
        });
    });
    assert_eq!(SEEN.with(Cell::get), Some(provided));
}
