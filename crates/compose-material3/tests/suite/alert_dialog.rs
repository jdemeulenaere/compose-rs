//! Compose's `AlertDialogTest.kt` and `AlertDialogScreenshotTest.kt`, adapted: the styling
//! parameters, the dialogs' widths, the parts' positions (the goldens as layout and display-list
//! checks), the buttons stacking when they don't fit, right to left. Positions are from the
//! dialog's bounds (Compose's dialog is a window of its own, its root at the dialog), at density
//! 1 in an 800 by 800 window.
//!
//! Not ported: `alertDialog_doesNotConsumeFullScreenWidth` and
//! `basicAlertDialog_customContentDoesNotConsumeFullScreenWidth` (the platform's dialog width,
//! `DialogProperties.usePlatformDefaultWidth`: compose-ui's dialogs take the window's), the two
//! `precisionPointer` tests (feature:precision-pointer).

use std::cell::Cell;

use compose_foundation::lazy::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    Color, Dp, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, Rect, UnitExt,
};

const WINDOW: IntSize = IntSize::new(800, 800);

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(WINDOW, content)
}

fn dialog(ui: &mut ComposeTestRule) -> Rect {
    ui.on_node(is_dialog()).fetch_semantics_snapshot().bounds
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag_unmerged(tag).fetch_semantics_snapshot().bounds
}

fn text_button(tag: &'static str, label: &'static str) {
    TextButton!(|| {}, modifier = Modifier.test_tag(tag).semantics_with(true, |_| {}), {
        Text!("{}", label)
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogTest.kt, customStyleProperties_shouldApply
#[test]
fn an_alert_dialog_takes_its_colors_and_modifier() {
    thread_local! {
        static COLORS: Cell<[Option<Color>; 4]> = const { Cell::new([None; 4]) };
    }
    let record = |index: usize| {
        let color = LOCAL_CONTENT_COLOR.current();
        COLORS.with(|colors| {
            let mut all = colors.get();
            all[index] = Some(color);
            colors.set(all);
        });
    };
    let ui = rule(move || {
        MaterialTheme!({
            AlertDialog!(
                || {},
                move || {
                    TextButton!(|| {}, {
                        Text!("Confirm");
                        record(0);
                    });
                },
                modifier = Modifier.border(10.dp(), Color::BLUE),
                icon = Some(move || {
                    Icon!(icons::filled::favorite(), None);
                    record(1);
                }),
                title = Some(move || {
                    Text!("Title");
                    record(2);
                }),
                text = Some(move || {
                    Text!("Text");
                    record(3);
                }),
                container_color = Color::YELLOW,
                tonal_elevation = 0.dp(),
                icon_content_color = Color::GREEN,
                title_content_color = Color::MAGENTA,
                text_content_color = Color::from_argb(0xFF444444),
            );
        });
    });
    let colors = COLORS.with(Cell::get);
    // A text button's content takes its own colors: the primary color.
    assert_eq!(
        colors,
        [
            Some(light_color_scheme().primary),
            Some(Color::GREEN),
            Some(Color::MAGENTA),
            Some(Color::from_argb(0xFF444444)),
        ]
    );
    let dump = ui.dump_display_list();
    assert!(dump.contains(&format!("{:?}", Color::YELLOW)), "the container: {dump}");
    assert!(dump.contains(&format!("{:?}", Color::BLUE)), "the border: {dump}");
}

thread_local! { static WIDTH: Cell<i32> = const { Cell::new(0) }; }

/// A modifier recording the width it's measured at, as the tests' `onSizeChanged`.
fn recording() -> Modifier {
    Modifier.layout(|scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        WIDTH.with(|width| width.set(placeable.width));
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    })
}

/// The width `content` records.
fn measured_width(content: fn()) -> i32 {
    let _ui = rule(move || {
        MaterialTheme!({
            content();
        });
    });
    WIDTH.with(Cell::get)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogTest.kt, alertDialog_minWidth
#[test]
fn a_short_alert_dialog_is_280_dp_wide() {
    let width = measured_width(|| {
        AlertDialog!(
            || {},
            || text_button("confirm", "Confirm"),
            modifier = recording(),
            title = Some(|| Text!("Title")),
            text = Some(|| Text!("Short")),
        );
    });
    assert_eq!(width, 280);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogTest.kt, basicAlertDialog_customContentMinWidth
#[test]
fn a_basic_alert_dialogs_content_is_at_least_280_dp_wide() {
    let width = measured_width(|| {
        BasicAlertDialog!(|| {}, {
            Surface!(modifier = recording(), {
                Text!("Short");
            });
        });
    });
    assert_eq!(width, 280);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogTest.kt, basicAlertDialog_customContentModifiedMinWidth
#[test]
fn a_basic_alert_dialog_can_be_narrower() {
    let width = measured_width(|| {
        BasicAlertDialog!(|| {}, modifier = Modifier.width(150.dp()), {
            Surface!(modifier = recording(), {
                Text!("Short");
            });
        });
    });
    assert_eq!(width, 150);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogTest.kt, alertDialog_withIcon_positioning
#[test]
fn an_alert_dialog_centers_its_icon_and_title_above_its_text_and_buttons() {
    let mut ui = rule(|| {
        MaterialTheme!({
            AlertDialog!(
                || {},
                || text_button("confirm", "Confirm"),
                dismiss_button = Some(|| text_button("dismiss", "Dismiss")),
                icon = Some(|| {
                    Icon!(
                        icons::filled::favorite(),
                        None,
                        modifier = Modifier.size(AlertDialogDefaults::ICON_SIZE).test_tag("icon"),
                    );
                }),
                title = Some(|| Text!("Title", modifier = Modifier.test_tag("title"))),
                text = Some(|| Text!("Text", modifier = Modifier.test_tag("text"))),
            );
        });
    });
    let dialog = dialog(&mut ui);
    let (icon, title, text) =
        (bounds(&mut ui, "icon"), bounds(&mut ui, "title"), bounds(&mut ui, "text"));
    let (confirm, dismiss) = (bounds(&mut ui, "confirm"), bounds(&mut ui, "dismiss"));
    assert_eq!(title.height(), 32.0, "the title's line height");
    assert_eq!(text.height(), 20.0, "the text's line height");
    // The icon centered, 24 dp from the top; the title centered, 16 dp below it.
    assert_eq!(icon.left - dialog.left, (dialog.width() - 24.0) / 2.0);
    assert_eq!(icon.top - dialog.top, 24.0);
    assert_eq!(title.left - dialog.left, (dialog.width() - title.width()) / 2.0);
    assert_eq!(title.top, icon.bottom + 16.0);
    // The text 24 dp from the start, 16 dp below the title.
    assert_eq!(text.left - dialog.left, 24.0);
    assert_eq!(text.top, title.bottom + 16.0);
    // The confirm button 24 dp from the end and the bottom (48 dp high), 24 dp below the text,
    // the dismiss button 8 dp before it.
    assert_eq!(confirm.left, dialog.right - 24.0 - confirm.width());
    assert_eq!(confirm.top, dialog.bottom - 24.0 - 48.0);
    assert_eq!(confirm.top - text.bottom, 24.0);
    assert_eq!((confirm.top, confirm.bottom), (dismiss.top, dismiss.bottom));
    assert_eq!(confirm.left - 8.0, dismiss.right);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogTest.kt, alertDialog_positioning
#[test]
fn an_alert_dialog_puts_its_title_at_the_start_without_an_icon() {
    let mut ui = rule(|| {
        MaterialTheme!({
            AlertDialog!(
                || {},
                || {},
                dismiss_button = Some(|| text_button("dismiss", "Dismiss")),
                title = Some(|| Text!("Title", modifier = Modifier.test_tag("title"))),
                text = Some(|| Text!("Text", modifier = Modifier.test_tag("text"))),
            );
        });
    });
    let dialog = dialog(&mut ui);
    let (title, text, dismiss) =
        (bounds(&mut ui, "title"), bounds(&mut ui, "text"), bounds(&mut ui, "dismiss"));
    assert_eq!((title.height(), text.height()), (32.0, 20.0));
    assert_eq!((title.left - dialog.left, title.top - dialog.top), (24.0, 24.0));
    assert_eq!((text.left - dialog.left, text.top), (24.0, title.bottom + 16.0));
    assert_eq!(dismiss.left, dialog.right - 24.0 - dismiss.width());
    assert_eq!(dismiss.top, dialog.bottom - 24.0 - 48.0);
    assert_eq!(dismiss.top - text.bottom, 24.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogTest.kt, alertDialog_positioningActionsWithLongText
#[test]
fn long_buttons_stack_the_dismiss_button_under_the_confirm_button() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), || {
        MaterialTheme!({
            AlertDialog!(
                || {},
                || text_button("confirm", "Confirm with a long text"),
                dismiss_button = Some(|| text_button("dismiss", "Dismiss with a long text")),
                title = Some(|| Text!("Title")),
                text = Some(|| Text!("Text")),
            );
        });
    });
    let (confirm, dismiss) = (bounds(&mut ui, "confirm"), bounds(&mut ui, "dismiss"));
    assert!(dismiss.top >= confirm.bottom, "{confirm:?} {dismiss:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogTest.kt, alertDialog_positioningWithLazyColumnText
#[test]
fn a_lazy_column_text_leaves_room_for_the_buttons() {
    let mut ui = rule(|| {
        MaterialTheme!({
            AlertDialog!(
                || {},
                || {},
                dismiss_button = Some(|| text_button("dismiss", "Dismiss")),
                title = Some(|| Text!("Title", modifier = Modifier.test_tag("title"))),
                text = Some(|| {
                    LazyColumn!(modifier = Modifier.test_tag("text"), |scope: LazyListScope| {
                        scope.items(100, |_| Text!("Message!"));
                    });
                }),
            );
        });
    });
    let dialog = dialog(&mut ui);
    let (title, text, dismiss) =
        (bounds(&mut ui, "title"), bounds(&mut ui, "text"), bounds(&mut ui, "dismiss"));
    assert_eq!((title.left - dialog.left, title.top - dialog.top), (24.0, 24.0));
    assert_eq!((text.left - dialog.left, text.top), (24.0, title.bottom + 16.0));
    assert_eq!(dismiss.left, dialog.right - 24.0 - dismiss.width());
    assert_eq!(dismiss.top, dialog.bottom - 24.0 - 48.0);
    assert_eq!(dismiss.top - text.bottom, 24.0);
}

// ---- The goldens ------------------------------------------------------------------------------

const SUPPORTING_TEXT: &str = "This area typically contains the supportive text which presents \
                               the details regarding the Dialog's purpose.";

/// The goldens' dialog: a title, the supporting text, a filled confirm button and an outlined
/// dismiss button (`width` wide if given), with an icon or not.
fn golden(
    scheme: ColorScheme,
    direction: LayoutDirection,
    icon: bool,
    width: Option<Dp>,
) -> ComposeTestRule {
    rule(move || {
        MaterialTheme!(color_scheme = scheme, {
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
                let modifier = move |tag: &'static str| -> Modifier {
                    match width {
                        Some(width) => Modifier.width(width).test_tag(tag),
                        None => Modifier.test_tag(tag),
                    }
                };
                AlertDialog!(
                    || {},
                    move || {
                        Button!(|| {}, modifier = modifier("confirm"), { Text!("Confirm") });
                    },
                    dismiss_button = Some(move || {
                        OutlinedButton!(|| {}, modifier = modifier("dismiss"), {
                            Text!("Dismiss")
                        });
                    }),
                    icon = icon.then_some(|| {
                        Icon!(icons::filled::favorite(), None, modifier = Modifier.test_tag("icon"))
                    }),
                    title = Some(|| Text!("Title", modifier = Modifier.test_tag("title"))),
                    text = Some(|| Text!("{}", SUPPORTING_TEXT)),
                );
            });
        });
    })
}

/// The container: a 28 dp round rectangle in the high surface container.
fn assert_container(ui: &mut ComposeTestRule, scheme: ColorScheme) {
    let found = ui.host().display_list().ops.iter().any(|op| {
        matches!(op, DrawOp::DrawRoundRect(_, radii, paint)
            if radii[0] == 28.0 && paint.color == scheme.surface_container_high)
    });
    assert!(found, "{}", ui.dump_display_list());
}

/// The buttons on one row at the end, the confirm button last (at the right left to right).
fn assert_buttons_at_the_end(ui: &mut ComposeTestRule, rtl: bool) {
    let dialog = dialog(ui);
    let (confirm, dismiss) = (bounds(ui, "confirm"), bounds(ui, "dismiss"));
    assert_eq!(confirm.top, dismiss.top);
    if rtl {
        assert_eq!(confirm.left, dialog.left + 24.0);
        assert_eq!(dismiss.left, confirm.right + 8.0);
    } else {
        assert_eq!(confirm.right, dialog.right - 24.0);
        assert_eq!(dismiss.right, confirm.left - 8.0);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogScreenshotTest.kt, alertDialog_lightTheme
#[test]
fn the_light_alert_dialog_golden() {
    let mut ui = golden(light_color_scheme(), LayoutDirection::Ltr, false, None);
    assert_container(&mut ui, light_color_scheme());
    assert_buttons_at_the_end(&mut ui, false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogScreenshotTest.kt, alertDialog_darkTheme
#[test]
fn the_dark_alert_dialog_golden() {
    let mut ui = golden(dark_color_scheme(), LayoutDirection::Ltr, false, None);
    assert_container(&mut ui, dark_color_scheme());
    assert_buttons_at_the_end(&mut ui, false);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogScreenshotTest.kt, alertDialog_withVerticalButtons_lightTheme
#[test]
fn the_vertical_buttons_golden() {
    // Each button 80 % of the dialog's widest: they stack, the confirm button on top, both at
    // the end, 0 dp apart (8 dp less the touch target's 8 dp).
    let mut ui = golden(light_color_scheme(), LayoutDirection::Ltr, false, Some(448.dp()));
    let dialog = dialog(&mut ui);
    let (confirm, dismiss) = (bounds(&mut ui, "confirm"), bounds(&mut ui, "dismiss"));
    assert_eq!(confirm.right, dialog.right - 24.0);
    assert_eq!(dismiss.right, dialog.right - 24.0);
    assert_eq!(dismiss.top, confirm.bottom + 8.0, "{confirm:?} {dismiss:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogScreenshotTest.kt, alertDialog_withIcon_lightTheme
#[test]
fn the_light_alert_dialog_with_an_icon_golden() {
    let mut ui = golden(light_color_scheme(), LayoutDirection::Ltr, true, None);
    assert_container(&mut ui, light_color_scheme());
    let dialog = dialog(&mut ui);
    let (icon, title) = (bounds(&mut ui, "icon"), bounds(&mut ui, "title"));
    assert_eq!(icon.left - dialog.left, (dialog.width() - icon.width()) / 2.0);
    assert_eq!(title.left - dialog.left, (dialog.width() - title.width()) / 2.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogScreenshotTest.kt, alertDialog_withIcon_darkTheme
#[test]
fn the_dark_alert_dialog_with_an_icon_golden() {
    let mut ui = golden(dark_color_scheme(), LayoutDirection::Ltr, true, None);
    assert_container(&mut ui, dark_color_scheme());
    let found = ui.host().display_list().ops.iter().any(|op| {
        matches!(op, DrawOp::DrawPath { paint, .. } if paint.color == dark_color_scheme().secondary)
    });
    assert!(found, "the icon in the secondary color");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AlertDialogScreenshotTest.kt, alertDialog_rtl
#[test]
fn the_right_to_left_alert_dialog_golden() {
    let mut ui = golden(light_color_scheme(), LayoutDirection::Rtl, false, None);
    assert_buttons_at_the_end(&mut ui, true);
    let dialog = dialog(&mut ui);
    let title = bounds(&mut ui, "title");
    assert_eq!(title.right, dialog.right - 24.0);
}

/// The dialog's pane, as `BasicAlertDialog`'s `paneTitle`.
#[test]
fn a_dialog_is_a_pane() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            BasicAlertDialog!(|| {}, modifier = Modifier.test_tag("dialog"), {});
        });
    });
    ui.on_node_with_tag("dialog")
        .assert(expect_value(&compose_ui::SemanticsProperties::PANE_TITLE, "Dialog".into()));
}
