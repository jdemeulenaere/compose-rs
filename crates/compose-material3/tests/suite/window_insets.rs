//! Material's window insets, as Compose's defaults (D1359): the system bars and a cutout by default,
//! what's consumed around a scaffold left out, the snackbar and the floating action button clear
//! of the insets. Sizes in pixels at density 1.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{Insets, PlatformWindowInsets};
use compose_ui::{IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

/// The system bars and a cutout, as `systemBarsForVisualComponents`.
fn visual() -> WindowInsets {
    WindowInsets::system_bars().union(WindowInsets::display_cutout())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MaterialComponentsInsetSupportTest.kt, topAppBar_respectsInsetsDefault,
// bottomAppBar_respectsInsetsDefault, drawerSheets_respectsInsetsDefault, navigationBar_respectsInsetsDefault and NavRail_respectsInsetsDefault
#[test]
fn the_components_default_insets_are_the_system_bars_and_a_cutouts() {
    let checked = Rc::new(Cell::new(false));
    let done = checked.clone();
    let _ui = compose_test(move || {
        let horizontal_top = WindowInsetsSides::HORIZONTAL + WindowInsetsSides::TOP;
        let horizontal_bottom = WindowInsetsSides::HORIZONTAL + WindowInsetsSides::BOTTOM;
        let start_vertical = WindowInsetsSides::START + WindowInsetsSides::VERTICAL;
        assert_eq!(TopAppBarDefaults::window_insets(), visual().only(horizontal_top));
        assert_eq!(BottomAppBarDefaults::window_insets(), visual().only(horizontal_bottom));
        assert_eq!(DrawerDefaults::window_insets(), visual().only(start_vertical));
        assert_eq!(NavigationBarDefaults::window_insets(), visual().only(horizontal_bottom));
        assert_eq!(NavigationRailDefaults::window_insets(), visual().only(start_vertical));
        assert_eq!(ScaffoldDefaults::content_window_insets(), visual());
        done.set(true);
    });
    assert!(checked.get());
}

/// The host's insets at density 1.
fn sent() -> PlatformWindowInsets {
    PlatformWindowInsets {
        status_bars: Insets::new(0, 24, 0, 0),
        navigation_bars: Insets::new(0, 0, 0, 48),
        system_bars: Insets::new(0, 24, 0, 48),
        display_cutout: Insets::new(30, 10, 0, 0),
        ..PlatformWindowInsets::default()
    }
}

fn with_insets(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), content);
    ui.host().set_window_insets(sent());
    ui.wait_for_idle();
    ui
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MaterialComponentsInsetSupportTest.kt, scaffold_providesInsets
#[test]
fn a_scaffold_gives_its_content_the_default_insets() {
    let padding = Rc::new(Cell::new(PaddingValues::ZERO));
    let into = padding.clone();
    let mut ui = with_insets(move || {
        let into = into.clone();
        Scaffold!(|padding: PaddingValues| into.set(padding));
    });
    ui.wait_for_idle();
    assert_eq!(padding.get(), PaddingValues::new(30.dp(), 24.dp(), 0.dp(), 48.dp()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScaffoldTest.kt, scaffold_insetsTests_FabRespectsInsets (the default insets
// end to end: its bars and its content, nested in a scaffold as Compose's screens are)
#[test]
fn a_top_app_bar_and_its_scaffold_pad_each_part_once() {
    let mut ui = with_insets(|| {
        Scaffold!(
            top_bar = {
                TopAppBar!(
                    {
                        Text!("Title", modifier = Modifier.test_tag("title"));
                    },
                    modifier = Modifier.test_tag("bar")
                );
            },
            |padding: PaddingValues| {
                Box!(modifier = Modifier.padding(padding).fill_max_size().test_tag("content"));
            }
        );
    });
    let bar = bounds(&mut ui, "bar");
    // Under the status bar, its row below: 24 + 64 dp, from the window's top.
    assert_eq!((bar.top, bar.bottom), (0.0, 88.0));
    assert!(bounds(&mut ui, "title").top >= 24.0);
    assert!(bounds(&mut ui, "title").left >= 30.0);
    let content = bounds(&mut ui, "content");
    assert_eq!(
        (content.left, content.top, content.right, content.bottom),
        (30.0, 88.0, 400.0, 600.0 - 48.0)
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScaffoldTest.kt, scaffold_respectsConsumedWindowInsets
#[test]
fn a_scaffold_leaves_out_what_is_consumed_around_it() {
    let padding = Rc::new(Cell::new(PaddingValues::ZERO));
    let into = padding.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 400), move || {
        let into = into.clone();
        Box!(
            modifier = Modifier.fill_max_size().window_insets_padding(WindowInsets::new(
                0.dp(),
                10.dp(),
                0.dp(),
                10.dp()
            )),
            {
                let into = into.clone();
                Scaffold!(
                    content_window_insets = WindowInsets::new(0.dp(), 15.dp(), 0.dp(), 15.dp()),
                    |padding: PaddingValues| into.set(padding)
                );
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(padding.get().top, 5.dp());
    assert_eq!(padding.get().bottom, 5.dp());
}

/// A scaffold's padding in pixels with `top_bar` and `bottom_bar` heights, its insets 5 at the top
/// and 3 at the bottom.
fn padding_with_bars(top_bar: Option<i32>, bottom_bar: Option<i32>) -> PaddingValues {
    let padding = Rc::new(Cell::new(PaddingValues::ZERO));
    let into = padding.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 400), move || {
        let into = into.clone();
        Scaffold!(
            top_bar = {
                if let Some(height) = top_bar {
                    Box!(
                        modifier = Modifier.fill_max_width().required_height((height as f32).dp())
                    );
                }
            },
            bottom_bar = {
                if let Some(height) = bottom_bar {
                    Box!(
                        modifier = Modifier.fill_max_width().required_height((height as f32).dp())
                    );
                }
            },
            content_window_insets = WindowInsets::new(0.dp(), 5.dp(), 0.dp(), 3.dp()),
            |padding: PaddingValues| into.set(padding)
        );
    });
    ui.wait_for_idle();
    padding.get()
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScaffoldTest.kt, scaffold_providesInsets_respectCollapsedTopAppBar
#[test]
fn a_bar_collapsed_to_nothing_still_takes_the_place_of_the_inset() {
    let top = padding_with_bars(Some(0), None);
    assert_eq!((top.top, top.bottom), (0.dp(), 3.dp()));
    let bottom = padding_with_bars(None, Some(0));
    assert_eq!((bottom.top, bottom.bottom), (5.dp(), 0.dp()));
}

/// A bar emptied at the same size (0 tall) gives its side back to the insets: the slot's state
/// measures the scaffold again though its size didn't change.
#[test]
fn a_collapsed_bar_removed_gives_its_side_back_to_the_inset() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let padding = Rc::new(Cell::new(PaddingValues::ZERO));
    let into = padding.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 400), move || {
        let into = into.clone();
        Scaffold!(
            top_bar = {
                if handle.get() {
                    Box!(modifier = Modifier.fill_max_width().required_height(0.dp()));
                }
            },
            content_window_insets = WindowInsets::new(0.dp(), 5.dp(), 0.dp(), 3.dp()),
            |padding: PaddingValues| into.set(padding)
        );
    });
    ui.wait_for_idle();
    assert_eq!(padding.get().top, 0.dp());
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(padding.get().top, 5.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScaffoldTest.kt, scaffold_providesInsets_respectTopAppBar,
// scaffold_respectsProvidedInsets and scaffold_providesInsets_respectsBottomAppBar
#[test]
fn a_bar_takes_the_place_of_the_inset_at_its_side() {
    let top = padding_with_bars(Some(10), None);
    assert_eq!((top.top, top.bottom), (10.dp(), 3.dp()));
    let none = padding_with_bars(None, None);
    assert_eq!((none.top, none.bottom), (5.dp(), 3.dp()));
    let bottom = padding_with_bars(None, Some(10));
    assert_eq!((bottom.top, bottom.bottom), (5.dp(), 10.dp()));
}

/// A scaffold with a 20 by 10 snackbar and a 56 dp button, and `insets`, laid out in `direction`.
#[composable]
fn ScaffoldOf(position: FabPosition, insets: WindowInsets, direction: LayoutDirection) {
    CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
        Scaffold!(
            snackbar_host = {
                Box!(modifier = Modifier.size((20.dp(), 10.dp())).test_tag("snackbar"));
            },
            floating_action_button = {
                FloatingActionButton!(|| {}, modifier = Modifier.test_tag("fab"), {});
            },
            floating_action_button_position = position,
            content_window_insets = insets.clone(),
            |_: PaddingValues| {}
        );
    });
}

fn fab_and_snackbar(
    position: FabPosition,
    direction: LayoutDirection,
) -> (compose_ui::Rect, compose_ui::Rect) {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || {
        ScaffoldOf(position, WindowInsets::new(10.dp(), 0.dp(), 15.dp(), 3.dp()), direction)
    });
    (bounds(&mut ui, "fab"), bounds(&mut ui, "snackbar"))
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScaffoldTest.kt, scaffold_insetsTests_FabRespectsInsets,
// scaffold_insetsTests_FabRespectsHorizontalInsets_fabPosition_end, scaffold_insetsTests_FabRespectsHorizontalInsets_fabPosition_strat_landscape_rtl,
// scaffold_insetsTests_FabRespectsHorizontalInsets_fabPosition_center, scaffold_insetsTests_snackbarRespectsInsets and scaffold_insetsTests_snackbarRespectsHorizontalInsets
#[test]
fn the_button_and_the_snackbar_keep_clear_of_the_insets() {
    use LayoutDirection::{Ltr, Rtl};
    let (end, snackbar) = fab_and_snackbar(FabPosition::End, Ltr);
    assert_eq!(end.right, 400.0 - 16.0 - 15.0);
    assert_eq!(end.bottom, 600.0 - 16.0 - 3.0);
    // Above the button, centered between the insets.
    assert_eq!(snackbar.bottom, end.top);
    assert_eq!(snackbar.left, ((400 - 20 + 10 - 15) / 2) as f32);
    assert_eq!(fab_and_snackbar(FabPosition::Start, Ltr).0.left, 16.0 + 10.0);
    // Its start is on the right, whose inset is 15.
    assert_eq!(fab_and_snackbar(FabPosition::Start, Rtl).0.right, 400.0 - 16.0 - 15.0);
    assert_eq!(fab_and_snackbar(FabPosition::End, Rtl).0.left, 16.0 + 10.0);
    let center = fab_and_snackbar(FabPosition::Center, Ltr).0;
    assert_eq!(center.left, ((400 - 56 + 10 - 15) / 2) as f32);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, bottomAppBarWithFAB_respectsWindowInsets
#[test]
fn a_bottom_app_bar_is_taller_by_its_insets() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), || {
        BottomAppBar!(
            |_| {},
            modifier = Modifier.test_tag("bar"),
            floating_action_button = Some(|| {
                FloatingActionButton!(|| {}, {});
            }),
            window_insets = WindowInsets::new(10.dp(), 10.dp(), 10.dp(), 10.dp())
        );
    });
    let bar = bounds(&mut ui, "bar");
    assert_eq!((bar.width(), bar.height()), (400.0, 100.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_correctlyPadsWhenParentHandlesInsetsAndContentPaddingIsUsed
#[test]
fn a_top_app_bar_doesnt_pad_by_what_its_parent_consumed_nor_does_its_sibling() {
    let mut ui = with_insets(|| {
        Column!(
            modifier = Modifier.fill_max_size().window_insets_padding(WindowInsets::status_bars()),
            {
                TopAppBar!(
                    {
                        Text!("Title");
                    },
                    modifier = Modifier.test_tag("bar"),
                    window_insets = WindowInsets::status_bars()
                );
                Box!(
                    modifier = Modifier
                        .test_tag("sibling")
                        .fill_max_width()
                        .height(40.dp())
                        .window_insets_padding(WindowInsets::status_bars())
                );
            }
        );
    });
    let bar = bounds(&mut ui, "bar");
    assert_eq!((bar.top, bar.bottom), (24.0, 24.0 + 64.0));
    let sibling = bounds(&mut ui, "sibling");
    assert_eq!((sibling.top, sibling.height()), (24.0 + 64.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_respectsContentWindowInsets_whenImeIsPresent
#[test]
fn a_sheet_without_insets_isnt_pushed_up_by_the_keyboard() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), || {
        ModalBottomSheet!(
            || {},
            sheet_state = remember_bottom_sheet_state!(
                SheetValue::Hidden,
                enabled_values = &[SheetValue::Hidden, SheetValue::Expanded]
            ),
            content_window_insets = WindowInsets::default(),
            |_| {
                Box!(modifier = Modifier.test_tag("sheet").height(200.dp()).fill_max_size());
            }
        );
    });
    ui.host().set_window_insets(PlatformWindowInsets {
        ime: Insets::new(0, 0, 0, 200),
        ..PlatformWindowInsets::default()
    });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "sheet").bottom, 800.0);
}

/// A sheet's content, with its default insets: clear of the navigation bar below, and of the
/// status bar only as far as the sheet doesn't already keep clear of it (the sheet's offset is
/// consumed, as Compose's `SheetWindowInsets`).
#[test]
fn a_sheets_content_pads_what_its_offset_doesnt_cover() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), || {
        ModalBottomSheet!(
            || {},
            sheet_state = remember_bottom_sheet_state!(
                SheetValue::Hidden,
                enabled_values = &[SheetValue::Hidden, SheetValue::Expanded]
            ),
            drag_handle = None::<fn()>,
            |_| {
                Box!(modifier = Modifier.test_tag("sheet").height(200.dp()).fill_max_width());
            }
        );
    });
    ui.host().set_window_insets(PlatformWindowInsets {
        status_bars: Insets::new(0, 24, 0, 0),
        navigation_bars: Insets::new(0, 0, 0, 48),
        system_bars: Insets::new(0, 24, 0, 48),
        ..PlatformWindowInsets::default()
    });
    ui.wait_for_idle();
    let sheet = bounds(&mut ui, "sheet");
    assert_eq!(sheet.height(), 200.0);
    // 48 up from the window's bottom; no top padding: it's far below the status bar.
    assert_eq!(sheet.bottom, 800.0 - 48.0);
}
