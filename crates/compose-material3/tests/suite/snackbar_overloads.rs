//! Compose's `SnackbarTest.kt` and `SnackbarScreenshotTest.kt`, adapted: the content overload
//! (`ContentSnackbar`) and the data one, their semantics, sizes, the action on its own line;
//! the goldens as layout and display-list checks. Compose's `SnackbarData` interface is
//! `SnackbarData::with_callbacks`.
//!
//! Not ported: the baseline assertions of `snackbar_shortTextOnly_*`, `snackbar_shortTextAndButton_*`
//! and `snackbar_longText*` (compose-rs has no alignment lines, ui's feature:alignment-lines);
//! their sizes are checked. The new line layout is Compose's with its
//! `isSnackbarStylingFixEnabled` flag on (its legacy one places the text by its baselines).

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{Modifier, SizeSpec, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag_unmerged(tag).fetch_semantics_snapshot().bounds
}

/// Visuals of `message`, `action` and a dismiss button or not.
fn visuals(message: &'static str, action: Option<&'static str>, dismiss: bool) -> SnackbarVisuals {
    SnackbarVisuals {
        message: message.into(),
        action_label: action.map(Into::into),
        with_dismiss_action: dismiss,
        duration: SnackbarDuration::Indefinite,
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarTest.kt, defaultSnackbar_semantics
#[test]
fn a_snackbars_action_clicks() {
    let clicked = Rc::new(Cell::new(false));
    let flag = clicked.clone();
    let mut ui = compose_test(move || {
        let flag = flag.clone();
        MaterialTheme!({
            let flag = flag.clone();
            Box!({
                let flag = flag.clone();
                ContentSnackbar!(
                    action = Some(move || {
                        let flag = flag.clone();
                        TextButton!(move || flag.set(true), { Text!("UNDO") });
                    }),
                    { Text!("Message") }
                );
            });
        });
    });
    ui.on_node_with_text("Message").assert_exists();
    assert!(!clicked.get());
    ui.on_node_with_text("UNDO").perform_click();
    assert!(clicked.get());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarTest.kt, snackbar_emptyContent
#[test]
fn an_empty_snackbar_lays_out() {
    let _ui = compose_test(|| {
        MaterialTheme!({
            ContentSnackbar!({});
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarTest.kt, snackbar_noTextInContent
#[test]
fn a_snackbar_centers_content_that_isnt_text() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            ContentSnackbar!({
                Box!(modifier = Modifier.test_tag("content").size(20.dp()));
            });
        });
    });
    ui.on_node_unmerged(has_test_tag("content"))
        .assert_top_position_in_root_is_equal_to(((48.0 - 20.0) / 2.0).dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarTest.kt, snackbar_withDismiss_semantics
#[test]
fn a_snackbars_dismiss_button_dismisses_it() {
    let dismissed = Rc::new(Cell::new(false));
    let flag = dismissed.clone();
    let mut ui = compose_test(move || {
        let flag = flag.clone();
        MaterialTheme!({
            let flag = flag.clone();
            Box!({
                let flag = flag.clone();
                let data = SnackbarData::with_callbacks(
                    visuals("Data message", None, true),
                    || {},
                    move || flag.set(true),
                );
                Snackbar!(data);
            });
        });
    });
    assert!(!dismissed.get());
    ui.on_node_with_content_description("Dismiss").perform_click();
    assert!(dismissed.get());
}

/// The size of a snackbar in a box at most 300 dp wide, as `setMaterialContentForSizeAssertions`.
fn snackbar_size(content: fn()) -> (f32, f32) {
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Box!(
                modifier = Modifier.size_in(SizeSpec::bounds(
                    0.dp(),
                    0.dp(),
                    300.dp(),
                    compose_ui::Dp::UNSPECIFIED,
                )),
                {
                    Box!(modifier = Modifier.test_tag("container"), { content() });
                }
            );
        });
    });
    let bounds = bounds(&mut ui, "container");
    (bounds.width(), bounds.height())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarTest.kt, snackbar_shortTextOnly_defaultSizes
#[test]
fn a_one_line_snackbar_is_300_by_48_dp() {
    let size = snackbar_size(|| ContentSnackbar!({ Text!("Message") }));
    assert_eq!(size, (300.0, 48.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarTest.kt, snackbar_longText_sizes
#[test]
fn a_two_line_snackbar_is_68_dp_high() {
    let size = snackbar_size(|| {
        ContentSnackbar!({
            Text!(
                "Message is very long and long and long and long and long and long and long",
                max_lines = 2,
            )
        })
    });
    assert_eq!(size, (300.0, 68.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarTest.kt, snackbar_textAndButtonOnSeparateLine_alignment
#[test]
fn an_action_on_a_new_line_sits_under_the_text_at_the_end() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!(
                modifier = Modifier.size_in(SizeSpec::bounds(
                    0.dp(),
                    0.dp(),
                    300.dp(),
                    compose_ui::Dp::UNSPECIFIED,
                )),
                {
                    Box!(modifier = Modifier.test_tag("container"), {
                        ContentSnackbar!(
                            action = Some(|| {
                                TextButton!(|| {}, modifier = Modifier.test_tag("button"), {
                                    Text!(
                                        "Undo",
                                        modifier = Modifier.size_in(SizeSpec::bounds(
                                            48.dp(),
                                            48.dp(),
                                            compose_ui::Dp::UNSPECIFIED,
                                            compose_ui::Dp::UNSPECIFIED,
                                        )),
                                    );
                                });
                            }),
                            action_on_new_line = true,
                            {
                                Text!(
                                    "Message",
                                    modifier =
                                        Modifier.test_tag("message").size_in(SizeSpec::bounds(
                                            48.dp(),
                                            48.dp(),
                                            compose_ui::Dp::UNSPECIFIED,
                                            compose_ui::Dp::UNSPECIFIED,
                                        )),
                                );
                            }
                        );
                    });
                }
            );
        });
    });
    let snackbar = bounds(&mut ui, "container");
    let (message, button) = (bounds(&mut ui, "message"), bounds(&mut ui, "button"));
    assert_eq!(message.top - snackbar.top, 14.0);
    assert_eq!(button.top - snackbar.top, 14.0 * 2.0 + message.height());
    assert_eq!(snackbar.height(), 4.0 + (button.bottom - snackbar.top));
    assert_eq!(snackbar.width(), 8.0 + (button.right - snackbar.left));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarTest.kt, defaultSnackbar_dataVersion_proxiesParameters
#[test]
fn the_data_snackbar_shows_its_message_and_performs_its_action() {
    let clicked = Rc::new(Cell::new(false));
    let flag = clicked.clone();
    let mut ui = compose_test(move || {
        let flag = flag.clone();
        MaterialTheme!({
            let flag = flag.clone();
            Box!({
                let flag = flag.clone();
                let data = SnackbarData::with_callbacks(
                    visuals("Data message", Some("UNDO"), false),
                    move || flag.set(true),
                    || {},
                );
                Snackbar!(data);
            });
        });
    });
    ui.on_node_with_text("Data message").assert_exists();
    assert!(!clicked.get());
    ui.on_node_with_text("UNDO").perform_click();
    assert!(clicked.get());
}

// ---- The goldens ------------------------------------------------------------------------------

const LONG_TEXT: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
                         tempor incididunt ut labore et dolore magna aliqua.";

/// The goldens' snackbar: `message`, an "Undo" action or not, a dismiss button when
/// indefinite, the action on its own line or not.
fn golden(
    scheme: ColorScheme,
    message: &'static str,
    action: bool,
    dismiss: bool,
    new_line: bool,
) -> ComposeTestRule {
    compose_test(move || {
        MaterialTheme!(color_scheme = scheme, {
            let data = SnackbarData::with_callbacks(
                visuals(message, action.then_some("Undo"), dismiss),
                || {},
                || {},
            );
            Snackbar!(
                data,
                modifier = Modifier.test_tag("snackbar"),
                action_on_new_line = new_line
            );
        });
    })
}

/// The container (6 dp shadow, extra-small corners) in the inverse surface, the message in
/// its content color.
fn assert_colors(ui: &mut ComposeTestRule, scheme: ColorScheme) {
    let dump = ui.dump_display_list();
    assert!(dump.contains(&format!("{:?}", scheme.inverse_surface)), "{dump}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_lightTheme
#[test]
fn the_light_snackbar_golden() {
    let mut ui = golden(light_color_scheme(), "Snackbar message", false, false, false);
    assert_colors(&mut ui, light_color_scheme());
    ui.on_node_with_text("Snackbar message").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_withAction_lightTheme
#[test]
fn the_light_snackbar_with_an_action_golden() {
    let mut ui = golden(light_color_scheme(), "Snackbar message", true, false, false);
    assert_colors(&mut ui, light_color_scheme());
    let (bar, undo) = (
        bounds(&mut ui, "snackbar"),
        ui.on_node_with_text("Undo").fetch_semantics_snapshot().bounds,
    );
    // The action at the end, 8 dp in.
    assert!((bar.right - 8.0 - undo.right).abs() <= 12.0, "{bar:?} {undo:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_withDismiss_lightTheme
#[test]
fn the_light_snackbar_with_a_dismiss_button_golden() {
    let mut ui = golden(light_color_scheme(), "Snackbar message", true, true, false);
    let dismiss = ui.on_node_with_content_description("Dismiss").fetch_semantics_snapshot().bounds;
    let undo = ui.on_node_with_text("Undo").fetch_semantics_snapshot().bounds;
    assert!(undo.right <= dismiss.left + 1.0, "the action before the dismiss button");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_darkTheme
#[test]
fn the_dark_snackbar_golden() {
    let mut ui = golden(dark_color_scheme(), "Snackbar message", false, false, false);
    assert_colors(&mut ui, dark_color_scheme());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_withAction_darkTheme
#[test]
fn the_dark_snackbar_with_an_action_golden() {
    let mut ui = golden(dark_color_scheme(), "Snackbar message", true, false, false);
    assert_colors(&mut ui, dark_color_scheme());
    ui.on_node_with_text("Undo").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_withDismiss_darkTheme
#[test]
fn the_dark_snackbar_with_a_dismiss_button_golden() {
    let mut ui = golden(dark_color_scheme(), "Snackbar message", true, true, false);
    assert_colors(&mut ui, dark_color_scheme());
    ui.on_node_with_content_description("Dismiss").assert_exists();
}

/// A long message's snackbar is taller than one line's, its action beside the text.
fn check_long(scheme: ColorScheme) {
    let mut ui = golden(scheme, LONG_TEXT, true, true, false);
    let bar = bounds(&mut ui, "snackbar");
    assert!(bar.height() >= 68.0 + 24.0, "{bar:?}");
    let message = ui.on_node_with_text(LONG_TEXT).fetch_semantics_snapshot().bounds;
    let undo = ui.on_node_with_text("Undo").fetch_semantics_snapshot().bounds;
    assert!(undo.left >= message.right, "beside it: {message:?} {undo:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_withLongMessage_darkTheme
#[test]
fn the_dark_snackbar_with_a_long_message_golden() {
    check_long(dark_color_scheme());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_withLongMessage_lightTheme
#[test]
fn the_light_snackbar_with_a_long_message_golden() {
    check_long(light_color_scheme());
}

/// The action under a long message, at the end.
fn check_new_line(scheme: ColorScheme) {
    let mut ui = golden(scheme, LONG_TEXT, true, true, true);
    let message = ui.on_node_with_text(LONG_TEXT).fetch_semantics_snapshot().bounds;
    let undo = ui.on_node_with_text("Undo").fetch_semantics_snapshot().bounds;
    assert!(undo.top >= message.bottom, "under it: {message:?} {undo:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_withLongMessageAndActionOnNewLine_darkTheme
#[test]
fn the_dark_snackbar_with_its_action_on_a_new_line_golden() {
    check_new_line(dark_color_scheme());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SnackbarScreenshotTest.kt, snackbar_withLongMessageAndActionOnNewLine_lightTheme
#[test]
fn the_light_snackbar_with_its_action_on_a_new_line_golden() {
    check_new_line(light_color_scheme());
}
