//! Material 3's intrinsic sizes, as Compose's layouts answer them: top app bars, navigation bar
//! items, list items, text fields and snackbars (D1207). Adapted from Compose's `AppBarTest`,
//! `NavigationBarTest`, `ListItemTest`, `TextFieldTest` and `OutlinedTextFieldTest`.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, IntrinsicSize, LayoutCoordinates, Modifier, UnitExt};

fn cell() -> Rc<Cell<IntSize>> {
    Rc::new(Cell::new(IntSize::new(-1, -1)))
}

fn size_into(size: &Rc<Cell<IntSize>>) -> Modifier {
    let size = size.clone();
    Modifier
        .on_globally_positioned(move |coordinates: LayoutCoordinates| size.set(coordinates.size()))
}

/// A rule over `content` in a theme, in a root of 800 by 800 px at density 1.
fn themed(content: impl Fn() + 'static) -> ComposeTestRule {
    let content = Rc::new(content);
    ComposeTestRule::sized(IntSize::new(800, 800), move || {
        let content = content.clone();
        MaterialTheme!({ content() });
    })
}

/// As `AppBarTest`'s `FakeIcon`: a 24 dp box.
#[composable]
fn FakeIcon(modifier: Modifier) {
    Box!(modifier = modifier.size(24.dp()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_intrinsicHeight
#[test]
fn a_top_app_bars_intrinsic_height_is_its_expanded_height() {
    let (min, max) = (cell(), cell());
    let state = Rc::new(Cell::new(None));
    let (min_size, max_size, kept) = (min.clone(), max.clone(), state.clone());
    let _ui = themed(move || {
        let behavior = TopAppBarDefaults::enter_always_scroll_behavior();
        kept.set(Some(behavior.state()));
        let (min_size, max_size) = (min_size.clone(), max_size.clone());
        Column!({
            let behavior = behavior.clone();
            for (size, into) in [(IntrinsicSize::Min, &min_size), (IntrinsicSize::Max, &max_size)] {
                let behavior = behavior.clone();
                Row!(modifier = Modifier.height(size).then(size_into(into)), {
                    TopAppBar!(
                        { Text!("Title") },
                        expanded_height = 50.dp(),
                        scroll_behavior = Some(behavior.clone())
                    );
                });
            }
        });
    });
    assert_eq!(state.get().expect("composed").height_offset_limit(), -50.0);
    assert_eq!(min.get().height, 50);
    assert_eq!(max.get().height, 50);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/AppBarTest.kt, topAppBar_intrinsicWidth
#[test]
fn a_top_app_bars_intrinsic_width_is_its_slots_with_their_padding() {
    let (min, max) = (cell(), cell());
    let (navigation, title, actions) = (cell(), cell(), cell());
    let (min_size, max_size) = (min.clone(), max.clone());
    let (navigation_size, title_size, actions_size) =
        (navigation.clone(), title.clone(), actions.clone());
    let _ui = themed(move || {
        let (min_size, max_size) = (min_size.clone(), max_size.clone());
        let (navigation_size, title_size, actions_size) =
            (navigation_size.clone(), title_size.clone(), actions_size.clone());
        Column!({
            for (size, into) in [(IntrinsicSize::Min, &min_size), (IntrinsicSize::Max, &max_size)] {
                let (navigation_size, title_size, actions_size) =
                    (navigation_size.clone(), title_size.clone(), actions_size.clone());
                Row!(modifier = Modifier.width(size).then(size_into(into)), {
                    let (navigation_size, title_size, actions_size) =
                        (navigation_size.clone(), title_size.clone(), actions_size.clone());
                    TopAppBar!(
                        { Text!("Title", modifier = size_into(&title_size)) },
                        modifier = Modifier.fill_max_width(),
                        navigation_icon = Some(move || FakeIcon(size_into(&navigation_size))),
                        actions = move |_| FakeIcon(size_into(&actions_size)),
                        expanded_height = 50.dp()
                    );
                });
            }
        });
    });
    // 4 dp before the navigation icon, before and after the title, and after the actions.
    let expected = 16 + navigation.get().width + title.get().width + actions.get().width;
    assert!((min.get().width - expected).abs() <= 1, "{} against {expected}", min.get().width);
    assert!((max.get().width - expected).abs() <= 1, "{} against {expected}", max.get().width);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/NavigationBarTest.kt, navigationBarItem_respectsMinIntrinsicSize
#[test]
fn a_navigation_bar_items_minimum_intrinsic_width_is_its_icon_and_its_minimum_padding() {
    let row = cell();
    let row_size = row.clone();
    let _ui = themed(move || {
        let row_size = row_size.clone();
        Row!(modifier = Modifier.width(IntrinsicSize::Min).then(size_into(&row_size)), |row| {
            NavigationBarItem!(row, true, || {}, {
                Icon!(icons::filled::star(), None, modifier = Modifier.size(24.dp()))
            });
        });
    });
    // The icon and 44 dp each side (`NavigationBarItemToIconMinimumPadding`).
    assert_eq!(row.get().width, 24 + 44 * 2);
}

/// `ListItemTest`'s scene: a list item filling an intrinsically sized column's height, its
/// leading and trailing boxes filling its height; their heights.
fn list_item_heights(overline: bool, supporting: Option<&'static str>) -> (i32, i32, i32) {
    let (item, leading, trailing) = (cell(), cell(), cell());
    let (item_size, leading_size, trailing_size) =
        (item.clone(), leading.clone(), trailing.clone());
    let _ui = themed(move || {
        let (item_size, leading_size, trailing_size) =
            (item_size.clone(), leading_size.clone(), trailing_size.clone());
        Column!(modifier = Modifier.height(IntrinsicSize::Min), {
            let (leading_size, trailing_size) = (leading_size.clone(), trailing_size.clone());
            ListItem!(
                { Text!("Primary text") },
                modifier = Modifier.fill_max_height().then(size_into(&item_size)),
                overline_content = overline.then_some(|| Text!("OVERLINE")),
                supporting_content = supporting.map(|text| move || Text!("{}", text)),
                leading_content = Some(move || Box!(
                    modifier = Modifier.fill_max_height().then(size_into(&leading_size))
                )),
                trailing_content = Some(move || Box!(
                    modifier = Modifier.fill_max_height().then(size_into(&trailing_size))
                ))
            );
        });
    });
    (item.get().height, leading.get().height, trailing.get().height)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ListItemTest.kt, listItem_oneLine_intrinsicSize,
// listItem_twoLine_intrinsicSize and listItem_threeLine_overline_intrinsicSize
// (`listItem_threeLine_noOverline_intrinsicSize` waits on Compose's `ListItem` layout: compose-rs's
// counts its slots for its lines, where Compose's takes a multi-line supporting text as three)
#[test]
fn a_list_items_intrinsic_height_is_its_container_height_and_its_slots_fill_it() {
    // (item, padding above and below): 56 dp and 8 dp for one line, 72 and 8 for two, 88 and
    // 12 for three.
    assert_eq!(list_item_heights(false, None), (56, 56 - 16, 56 - 16));
    assert_eq!(list_item_heights(false, Some("Secondary text")), (72, 72 - 16, 72 - 16));
    assert_eq!(list_item_heights(true, Some("Secondary text")), (88, 88 - 24, 88 - 24));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ListItemTest.kt, listItem_multipleItems_intrinsicSize
#[test]
fn list_items_in_an_intrinsically_sized_column_wrap_alike() {
    let (first, second) = (cell(), cell());
    let (first_size, second_size) = (first.clone(), second.clone());
    let _ui = themed(move || {
        let (first_size, second_size) = (first_size.clone(), second_size.clone());
        Column!(modifier = Modifier.width(300.dp()).height(IntrinsicSize::Min), {
            for into in [&first_size, &second_size] {
                ListItem!(
                    { Text!("A B C D E F G H") },
                    modifier = size_into(into),
                    leading_content = Some(|| Box!(modifier = Modifier.width(240.dp())))
                );
            }
        });
    });
    assert_eq!(first.get().height, second.get().height);
}

/// `TextFieldTest`'s and `OutlinedTextFieldTest`'s fields: a filled or outlined field of `text`
/// with `modifier`, a label if asked, and the icons, prefix and suffix if `decorated`.
#[composable]
fn Field(outlined: bool, text: &'static str, modifier: Modifier, label: bool, decorated: bool) {
    let label = label.then_some(|| Text!("Label"));
    let icon = decorated.then_some(|| Icon!(icons::filled::star(), None));
    let prefix = decorated.then_some(|| Text!("P"));
    let suffix = decorated.then_some(|| Text!("S"));
    if outlined {
        OutlinedTextField!(
            text.to_string(),
            |_| {},
            modifier = modifier,
            label = label,
            leading_icon = icon,
            trailing_icon = icon,
            prefix = prefix,
            suffix = suffix
        );
    } else {
        TextField!(
            text.to_string(),
            |_| {},
            modifier = modifier,
            label = label,
            leading_icon = icon,
            trailing_icon = icon,
            prefix = prefix,
            suffix = suffix
        );
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_withLabel_doesNotCrash_rowHeightWithMinIntrinsics
// (and OutlinedTextFieldTest.kt's testOutlinedTextField_withLabel_doesNotCrash_rowHeightWithMinIntrinsics)
#[test]
fn a_text_field_in_a_row_of_its_minimum_intrinsic_height_fills_it() {
    for outlined in [false, true] {
        let (row, divider) = (cell(), cell());
        let (row_size, divider_size) = (row.clone(), divider.clone());
        let _ui = themed(move || {
            let (row_size, divider_size) = (row_size.clone(), divider_size.clone());
            Box!(modifier = size_into(&row_size), {
                let divider_size = divider_size.clone();
                Row!(modifier = Modifier.height(IntrinsicSize::Min), {
                    VerticalDivider!(modifier = size_into(&divider_size), thickness = 10.dp());
                    Field(outlined, "", Modifier, true, false);
                });
            });
        });
        assert_eq!(divider.get().height, row.get().height);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_withLabel_doesNotCrash_columnWidthWithMinIntrinsics
// (and OutlinedTextFieldTest.kt's testOutlinedTextField_withLabel_doesNotCrash_columnWidthWithMinIntrinsics)
#[test]
fn a_text_field_in_a_column_of_its_minimum_intrinsic_width_fills_it() {
    for outlined in [false, true] {
        let (field, divider) = (cell(), cell());
        let (field_size, divider_size) = (field.clone(), divider.clone());
        let _ui = themed(move || {
            let (field_size, divider_size) = (field_size.clone(), divider_size.clone());
            Box!({
                let (field_size, divider_size) = (field_size.clone(), divider_size.clone());
                Column!(modifier = Modifier.width(IntrinsicSize::Min), {
                    HorizontalDivider!(modifier = size_into(&divider_size), thickness = 10.dp());
                    Field(outlined, "", size_into(&field_size), true, false);
                });
            });
        });
        assert_eq!(divider.get().width, field.get().width);
    }
}

/// The height of a row of its minimum intrinsic height holding a field and a divider.
fn field_row_height(outlined: bool, decorated: bool) -> i32 {
    let row = cell();
    let row_size = row.clone();
    let _ui = themed(move || {
        let row_size = row_size.clone();
        Box!(modifier = size_into(&row_size), {
            Row!(modifier = Modifier.height(IntrinsicSize::Min), {
                Field(outlined, "", Modifier, false, decorated);
                VerticalDivider!();
            });
        });
    });
    row.get().height
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_intrinsicHeight_withOnlyEmptyInput
// and testTextField_intrinsicHeight_withEmptyInput_andDecorations (and OutlinedTextFieldTest.kt's
// testOutlinedTextField_intrinsicHeight_withOnlyEmptyInput and …_withEmptyInput_andDecorations)
#[test]
fn an_empty_text_fields_intrinsic_height_is_its_minimum_height() {
    for outlined in [false, true] {
        assert_eq!(field_row_height(outlined, false), 56, "outlined: {outlined}");
        assert_eq!(field_row_height(outlined, true), 56, "decorated, outlined: {outlined}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, testTextField_intrinsicHeight_withLongInput_andDecorations
// (and OutlinedTextFieldTest.kt's testOutlinedTextField_intrinsicHeight_withLongInput_andDecorations)
#[test]
fn a_long_text_fields_intrinsic_height_is_its_measured_height() {
    for outlined in [false, true] {
        let (intrinsic, measured) = (cell(), cell());
        let (intrinsic_size, measured_size) = (intrinsic.clone(), measured.clone());
        let text: &'static str =
            "Long text input. Long text input. Long text input. Long text input.";
        let _ui = themed(move || {
            let (intrinsic_size, measured_size) = (intrinsic_size.clone(), measured_size.clone());
            Row!({
                let (intrinsic_size, measured_size) =
                    (intrinsic_size.clone(), measured_size.clone());
                Box!(modifier = Modifier.width(150.dp()).height(IntrinsicSize::Min), {
                    Field(outlined, text, size_into(&intrinsic_size), false, true);
                });
                Box!(modifier = Modifier.width(150.dp()), {
                    Field(outlined, text, size_into(&measured_size), false, true);
                });
            });
        });
        assert!(intrinsic.get().height > 0);
        assert_eq!(intrinsic.get().height, measured.get().height, "outlined: {outlined}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextFieldTest.kt, textField_withTrailingIcon_inIntrinsicHeight_andTooShortWidth_doesNotCrash
// (and OutlinedTextFieldTest.kt's outlinedTextField_withTrailingIcon_inIntrinsicHeight_andTooShortWidth_doesNotCrash)
#[test]
fn a_text_field_too_narrow_for_its_icon_answers_its_intrinsic_height() {
    for outlined in [false, true] {
        let _ui = themed(move || {
            Box!(modifier = Modifier.width(200.dp()), {
                Box!(modifier = Modifier.height(IntrinsicSize::Min), {
                    let icon = Some(|| Box!(modifier = Modifier.size(20.dp())));
                    if outlined {
                        OutlinedTextField!(
                            String::new(),
                            |_| {},
                            modifier = Modifier.padding(100.dp()),
                            trailing_icon = icon
                        );
                    } else {
                        TextField!(
                            String::new(),
                            |_| {},
                            modifier = Modifier.padding(100.dp()),
                            trailing_icon = icon
                        );
                    }
                });
            });
        });
    }
}

// Review of D1207: a snackbar in a row of its maximum intrinsic width is at most 600 dp wide, as
// Compose's `OneRowSnackbar` (`ContainerMaxWidth`), and doesn't panic.
#[test]
fn a_snackbars_intrinsic_width_is_bounded() {
    let row = cell();
    let row_size = row.clone();
    let host = SnackbarHostState::new();
    let state = *host;
    let scope = Rc::new(std::cell::RefCell::new(None));
    let kept = scope.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(2000, 800), move || {
        *kept.borrow_mut() = Some(remember_coroutine_scope());
        let row_size = row_size.clone();
        MaterialTheme!({
            let row_size = row_size.clone();
            Row!(modifier = Modifier.width(IntrinsicSize::Max).then(size_into(&row_size)), {
                SnackbarHost!(
                    state,
                    snackbar = Some(|data: SnackbarData| {
                        Snackbar!(data);
                    })
                );
            });
        });
    });
    scope.borrow().as_ref().unwrap().launch(async move {
        state.show_snackbar("Message").await;
    });
    ui.wait_for_idle();
    host.current_snackbar_data().expect("shown");
    // 600 dp, and the host's 12 dp each side.
    assert_eq!(row.get().width, 600 + 2 * 12);
}
