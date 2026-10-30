//! Compose's `MenuPositionTest.kt`, adapted: compose-rs's menu position provider is private,
//! so the scenarios go through `DropdownMenu` (the anchor and the menu's bounds).

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{
    Density, DpOffset, IntSize, LayoutDirection, Modifier, UnitExt, platform::FixedTextMeasurer,
};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds
}

/// At density 2, a 20 by 10 dp anchor 25 dp from the start and 50 dp from the top, its menu
/// open with `offset`.
fn anchored_menu(offset: DpOffset, direction: LayoutDirection) -> ComposeTestRule {
    let mut ui = compose_test_with(
        IntSize::new(400, 800),
        Density { density: 2.0, font_scale: 1.0 },
        Box::new(FixedTextMeasurer::new()),
        move || {
            MaterialTheme!({
                Box!(
                    modifier = Modifier.fill_max_size().padding((25.dp(), 50.dp(), 0.dp(), 0.dp())),
                    {
                        Box!(modifier = Modifier.size((20.dp(), 10.dp())).test_tag("anchor"), {
                            DropdownMenu!(
                                true,
                                || {},
                                modifier = Modifier.test_tag("menu"),
                                offset = offset,
                                |_| {
                                    DropdownMenuItem!({ Text!("Item") }, || {});
                                }
                            );
                        });
                    }
                );
            });
        },
    );
    ui.host().set_layout_direction(direction);
    ui.wait_for_idle();
    ui
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuPositionTest.kt, menuPosition_horizontal_anchorAlignment_withOffset
#[test]
fn a_menus_offset_is_in_dp_towards_the_end() {
    let offset = DpOffset::new(10.dp(), 0.dp());
    let mut ui = anchored_menu(offset, LayoutDirection::Ltr);
    let (anchor, menu) = (bounds(&mut ui, "anchor"), bounds(&mut ui, "menu"));
    // 10 dp is 20 px at density 2.
    assert_eq!(menu.left, anchor.left + 20.0);
    let mut ui = anchored_menu(offset, LayoutDirection::Rtl);
    let (anchor, menu) = (bounds(&mut ui, "anchor"), bounds(&mut ui, "menu"));
    assert_eq!(menu.right, anchor.right - 20.0);
}

/// The vertical offset moves the menu down from the anchor's bottom, in dp too.
#[test]
fn a_menus_vertical_offset_is_in_dp() {
    let mut ui = anchored_menu(DpOffset::new(0.dp(), 30.dp()), LayoutDirection::Ltr);
    let (anchor, menu) = (bounds(&mut ui, "anchor"), bounds(&mut ui, "menu"));
    // The menu's 8 dp top padding is inside its surface, around the tagged column's items.
    assert_eq!(menu.top, anchor.bottom + 60.0);
}

/// A plain menu has the same window: above the anchor when the keyboard is where it would go.
#[test]
fn a_menu_goes_above_its_anchor_when_the_keyboard_is_below() {
    let mut ui = compose_test_with(
        IntSize::new(600, 800),
        Density { density: 1.0, font_scale: 1.0 },
        Box::new(FixedTextMeasurer::new()),
        move || {
            MaterialTheme!({
                Box!(modifier = Modifier.padding((0.dp(), 420.dp(), 0.dp(), 0.dp())), {
                    Box!(modifier = Modifier.size((100.dp(), 56.dp())), {
                        DropdownMenu!(true, || {}, modifier = Modifier.test_tag("menu"), |_| {
                            for _ in 0..3 {
                                Box!(modifier = Modifier.size(50.dp()));
                            }
                        });
                    });
                });
            });
        },
    );
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "menu").top, 476.0);
    ui.host().set_window_insets(compose_ui::platform::PlatformWindowInsets {
        ime: compose_ui::platform::Insets { bottom: 300, ..Default::default() },
        ..Default::default()
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "menu").top, 254.0);
}

/// And it's no taller than that window, as a popup's window is.
#[test]
fn a_menu_is_no_taller_than_what_the_keyboard_leaves() {
    let mut ui = compose_test_with(
        IntSize::new(600, 800),
        Density { density: 1.0, font_scale: 1.0 },
        Box::new(FixedTextMeasurer::new()),
        move || {
            MaterialTheme!({
                DropdownMenu!(true, || {}, modifier = Modifier.test_tag("menu"), |_| {
                    for _ in 0..40 {
                        Box!(modifier = Modifier.size(50.dp()));
                    }
                });
            });
        },
    );
    ui.wait_for_idle();
    ui.host().set_window_insets(compose_ui::platform::PlatformWindowInsets {
        status_bars: compose_ui::platform::Insets { top: 24, ..Default::default() },
        ime: compose_ui::platform::Insets { bottom: 300, ..Default::default() },
        ..Default::default()
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "menu").height(), 476.0);
}
